use ignore::{DirEntry, WalkBuilder};
use log::{debug, warn};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf, MAIN_SEPARATOR};
use std::sync::{mpsc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// How long a loaded cache is trusted before ff re-reads the folders in the background.
/// ff's own folder changes update the index at once, so this only bounds how long
/// folders created or removed outside ff stay unseen.
pub const REFRESH_AFTER: Duration = Duration::from_secs(60 * 60);

/// Walker threads for a background build or refresh; few, so browsing stays responsive.
pub const REFRESH_THREADS: usize = 2;

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct DirectoryStore {
    pub directories: Vec<String>,
    /// When the folders were read from disk, in seconds since the Unix epoch. Saving
    /// ff's own changes keeps it, so they don't postpone the next refresh.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub built_at: Option<u64>,
    /// The folder walked, spelled as the entries are; `None` until a walk or a cache
    /// fills the store. Older caches don't record it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root: Option<String>,
    /// The rules the last full walk used; `None` in older caches. When settings ask for
    /// other rules, ff prunes the index at once and walks again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rules: Option<IndexRules>,
    /// Each `ignore_paths` folder's count (it and the folders below it) when it was
    /// last indexed, so what's excluded can still be shown.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub excluded: BTreeMap<String, usize>,
}

/// What the index leaves out besides hidden and gitignored folders and `~/Library`:
/// folders with these names at any depth, and these folders with everything below.
#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct IndexRules {
    /// Lowercased, sorted names (`ignore_directories`).
    pub ignore_directories: Vec<String>,
    /// Sorted folders strictly below the root, spelled as the index spells them
    /// (`ignore_paths`).
    pub ignore_paths: Vec<String>,
    /// Each `ignore_paths` folder as settings.toml spells it, so it can be found
    /// there again after its folder is gone.
    #[serde(skip)]
    pub sources: BTreeMap<String, String>,
}

/// Rules are the same when they leave out the same folders, however they're spelled.
impl PartialEq for IndexRules {
    fn eq(&self, other: &Self) -> bool {
        self.ignore_directories == other.ignore_directories
            && self.ignore_paths == other.ignore_paths
    }
}

impl Eq for IndexRules {}

impl IndexRules {
    /// The rules for an index of `root`. `ignore_paths` entries may start with `~`;
    /// relative ones are taken under `root`. They're spelled as the walk of `root`
    /// spells folders, so aliases such as /tmp and /private/tmp match.
    pub fn new(ignore_directories: &[String], ignore_paths: &[String], root: &Path) -> Self {
        let mut names: Vec<String> = ignore_directories
            .iter()
            .map(|name| name.to_lowercase())
            .collect();
        names.sort();
        names.dedup();
        let mut sources = BTreeMap::new();
        for entry in ignore_paths {
            if let Some(path) = index_spelling(Path::new(entry.trim()), root) {
                sources.entry(path).or_insert_with(|| entry.clone());
            }
        }
        Self {
            ignore_directories: names,
            ignore_paths: sources.keys().cloned().collect(),
            sources,
        }
    }

    /// Rules with only folder names, as builds without `ignore_paths` use.
    pub fn names(ignore_directories: &[String]) -> Self {
        Self::new(ignore_directories, &[], Path::new("/"))
    }

    /// Whether `path` is an `ignore_paths` folder or inside one.
    pub fn excludes(&self, path: &str) -> bool {
        self.ignore_paths
            .iter()
            .any(|excluded| below(path, excluded).is_some())
    }

    /// Whether a folder named `name` is skipped wherever it is.
    fn ignores_name(&self, name: &OsStr) -> bool {
        is_ignored(name, &self.ignore_directories)
    }
}

/// `path` (`~`-relative, absolute, or relative to `root`) spelled as a walk of `root`
/// spells it, or `None` unless it's strictly below `root`: the walk never meets the
/// root or a folder outside it, so excluding one would only confuse the pruning.
fn index_spelling(path: &Path, root: &Path) -> Option<String> {
    if path.as_os_str().is_empty() {
        return None;
    }
    let expanded = crate::cli::path_utils::expand_home(path).ok()?;
    let absolute = if expanded.is_relative() {
        root.join(expanded)
    } else {
        expanded
    };
    let resolved = fs::canonicalize(&absolute).unwrap_or_else(|_| absolute.clone());
    let canonical_root = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let relative: PathBuf = resolved
        .strip_prefix(&canonical_root)
        .or_else(|_| absolute.strip_prefix(root))
        .ok()?
        .components()
        .collect();
    if relative.as_os_str().is_empty() {
        return None;
    }
    root.join(relative).to_str().map(String::from)
}

impl DirectoryStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, path: &str) {
        self.directories.push(path.to_string());
    }

    pub fn search(&self, prefix: &str) -> Vec<String> {
        let mut new_files: Vec<String> = Vec::new();
        for file in self.directories.iter() {
            if file.contains(prefix) {
                new_files.push(file.clone());
            }
        }
        new_files
    }
}

fn is_ignored(name: &OsStr, ignore_lower: &[String]) -> bool {
    ignore_lower.contains(&name.to_string_lossy().to_lowercase())
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |age| age.as_secs())
}

/// `~/Library`: about 100k folders of macOS app data. Only the home folder's is
/// skipped; other folders named Library are indexed. Resolved, as the root the index
/// walks is.
fn home_library() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    Some(fs::canonicalize(&home).unwrap_or(home).join("Library"))
}

/// Walker with the rules every index build uses: hidden and gitignored entries,
/// `~/Library` and what `rules` leave out are pruned. `keep` can prune more entries
/// below `root`.
fn index_walker(
    root: &Path,
    rules: &IndexRules,
    keep: impl Fn(&DirEntry) -> bool + Send + Sync + 'static,
) -> WalkBuilder {
    let ignore_lower = rules.ignore_directories.clone();
    // ponytail: a linear scan per folder; a set if people list hundreds of paths.
    let skipped: Vec<PathBuf> = home_library()
        .into_iter()
        .chain(rules.ignore_paths.iter().map(PathBuf::from))
        .collect();

    let mut builder = WalkBuilder::new(root);
    builder
        .hidden(true) // ignore hidden files/dirs like .git
        .follow_links(false)
        .git_ignore(true)
        .git_exclude(true)
        .parents(true)
        .ignore(true)
        .max_depth(None);

    // Prune exact folder names while always walking the root itself.
    builder.filter_entry(move |e| {
        e.depth() == 0
            || (!is_ignored(e.file_name(), &ignore_lower)
                && !skipped.iter().any(|skip| skip.as_path() == e.path())
                && keep(e))
    });
    builder
}

/// Directories below the walk's root, in walk order.
fn walk_directories(builder: &WalkBuilder) -> Vec<String> {
    builder
        .build()
        .filter_map(|result| match result {
            Ok(entry) if entry.depth() > 0 && entry.file_type().is_some_and(|ft| ft.is_dir()) => {
                entry.path().to_str().map(String::from)
            }
            Ok(_) => None,
            Err(err) => {
                debug!("Error processing directory entry: {}", err);
                None
            }
        })
        .collect()
}

/// New implementation using ignore::WalkBuilder (gitignore-aware, early pruning)
pub fn build_directory_from_store_ignore(
    root_dir: &str,
    ignore_directories: Vec<String>,
) -> DirectoryStore {
    let built_at = Some(now_secs());
    let rules = IndexRules::names(&ignore_directories);
    let directories = walk_directories(&index_walker(Path::new(root_dir), &rules, |_| true));
    DirectoryStore {
        directories,
        built_at,
        root: Some(root_dir.to_string()),
        rules: Some(rules),
        excluded: BTreeMap::new(),
    }
}

/// Default function now delegates to the ignore-based implementation
pub fn build_directory_from_store(
    root_dir: &str,
    ignore_directories: Vec<String>,
) -> DirectoryStore {
    build_directory_from_store_ignore(root_dir, ignore_directories)
}

/// Save without ever leaving a half-written cache: write a temporary file beside
/// `path`, then rename it over `path`.
pub fn save_directory_to_file(store: &DirectoryStore, path: &str) -> io::Result<()> {
    // The process id keeps two ff instances from writing the same temporary file.
    let temp = PathBuf::from(format!("{path}.{}.tmp", std::process::id()));
    let result = File::create(&temp).and_then(|file| {
        let mut writer = BufWriter::new(file);
        serde_json::to_writer(&mut writer, store)?;
        writer.flush()?;
        fs::rename(&temp, path)
    });
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

pub fn load_directory_from_file(path: &str) -> io::Result<DirectoryStore> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);

    let trie = serde_json::from_reader(reader)?;
    Ok(trie)
}

/// Whether folders read from disk at `built_at` are due for a refresh at `now`. A time
/// in the future (the clock moved back) counts as due, so the cache can't stay frozen.
pub fn needs_refresh(built_at: SystemTime, now: SystemTime) -> bool {
    now.duration_since(built_at)
        .map_or(true, |age| age >= REFRESH_AFTER)
}

/// The part of `path` below `dir` ("" for `dir` itself), or `None` outside it.
pub(crate) fn below<'a>(path: &'a str, dir: &str) -> Option<&'a str> {
    let rest = path.strip_prefix(dir)?;
    (rest.is_empty() || rest.starts_with(MAIN_SEPARATOR)).then_some(rest)
}

/// The folder index ff searches. ff's own folder changes update it at once and save
/// it; a background refresh picks up changes made elsewhere.
pub struct DirectoryIndex {
    pub store: DirectoryStore,
    /// Folder the index covers, spelled as its entries are.
    pub root: PathBuf,
    rules: IndexRules,
    /// Where changes are saved; `None` keeps them in memory.
    cache_path: Option<PathBuf>,
    refresh: Option<mpsc::Receiver<DirectoryStore>>,
    /// Paths ff changed while a refresh was walking, re-read once it finishes.
    changed_during_refresh: Vec<PathBuf>,
    /// The folder the running walk reads; it differs from `root` while a new
    /// `root_dir` is walked.
    walking: Option<PathBuf>,
}

impl DirectoryIndex {
    pub fn new(
        store: DirectoryStore,
        root: PathBuf,
        rules: IndexRules,
        cache_path: Option<PathBuf>,
    ) -> Self {
        Self {
            store,
            root,
            rules,
            cache_path,
            refresh: None,
            changed_during_refresh: Vec::new(),
            walking: None,
        }
    }

    /// The index of `root`, saved at `cache_path`: the cache there, refreshed in the
    /// background when stale. When `rebuild` asks for it, or the cache is missing,
    /// unreadable, empty or covers another folder, `root` is walked in the background;
    /// a cache of another folder stays searchable until that walk swaps in. A cache
    /// built with other `rules` is pruned to them at once, then walked again.
    pub fn open(cache_path: &Path, root: &Path, rules: IndexRules, rebuild: bool) -> Self {
        let loaded = Self::load(cache_path, root, rules.clone())
            .inspect_err(|e| {
                if e.kind() != io::ErrorKind::NotFound {
                    warn!(
                        "Could not read the folder index {}: {e}",
                        cache_path.display()
                    );
                }
            })
            .ok();
        // A walk can't count the folders it skips, so keep the counts a rebuild or an
        // empty cache would lose.
        let excluded = loaded
            .as_ref()
            .map(|index| index.store.excluded.clone())
            .unwrap_or_default();
        let mut index = loaded
            .filter(|index| !rebuild && !index.store.directories.is_empty())
            .unwrap_or_else(|| {
                Self::new(
                    DirectoryStore::new(),
                    root.to_path_buf(),
                    rules,
                    Some(cache_path.to_path_buf()),
                )
            });
        if index.store.excluded.is_empty() {
            index.store.excluded = excluded;
        }
        if index.root != root {
            debug!(
                "Folder index covers {}, walking {}",
                index.root.display(),
                root.display()
            );
            index.walk(root, REFRESH_THREADS);
        } else if index.store.root.is_some() && index.store.rules.as_ref() != Some(&index.rules) {
            debug!("Folder index rules changed; pruning and walking again");
            index.prune_to_rules();
            index.save();
            index.walk(root, REFRESH_THREADS);
        } else {
            // A store never walked counts as stale.
            index.refresh_if_stale(SystemTime::now(), REFRESH_THREADS);
        }
        index
    }

    /// Load the cache at `cache_path`. Its root is the one it records or, for older
    /// caches, the parent of its shallowest folder: every indexed folder's parent is
    /// the root or indexed itself. An empty cache uses `fallback_root`.
    pub fn load(cache_path: &Path, fallback_root: &Path, rules: IndexRules) -> io::Result<Self> {
        let mut store = load_directory_from_file(&cache_path.to_string_lossy())?;
        // Older caches don't record their walk; the file's time stands in for it
        // (none counts as stale).
        if store.built_at.is_none() {
            store.built_at = fs::metadata(cache_path)
                .and_then(|metadata| metadata.modified())
                .ok()
                .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
                .map(|age| age.as_secs());
        }
        let root = match &store.root {
            Some(root) => PathBuf::from(root),
            None => store
                .directories
                .iter()
                .min_by_key(|dir| Path::new(dir).components().count())
                .and_then(|dir| Path::new(dir).parent())
                .unwrap_or(fallback_root)
                .to_path_buf(),
        };
        store.root = Some(root.to_string_lossy().into_owned());
        Ok(Self::new(
            store,
            root,
            rules,
            Some(cache_path.to_path_buf()),
        ))
    }

    /// Start a background refresh with `threads` walkers if the folders were read
    /// `REFRESH_AFTER` ago or more. Returns whether one started.
    pub fn refresh_if_stale(&mut self, now: SystemTime, threads: usize) -> bool {
        let built_at = self
            .store
            .built_at
            .and_then(|secs| UNIX_EPOCH.checked_add(Duration::from_secs(secs)))
            .unwrap_or(UNIX_EPOCH);
        if !needs_refresh(built_at, now) || !self.root.is_dir() {
            return false;
        }
        self.walk(&self.root.clone(), threads);
        true
    }

    /// Walk `root` in the background with `threads` walkers; the index becomes it
    /// when the walk finishes.
    fn walk(&mut self, root: &Path, threads: usize) {
        self.refresh = Some(build_directory_from_store_async(
            root.to_string_lossy().into_owned(),
            self.rules.clone(),
            threads,
        ));
        self.walking = Some(root.to_path_buf());
        self.changed_during_refresh.clear();
    }

    pub fn is_refreshing(&self) -> bool {
        self.refresh.is_some()
    }

    /// Whether searches find nothing yet because the first walk hasn't finished.
    pub fn is_filling(&self) -> bool {
        self.is_refreshing() && self.store.root.is_none()
    }

    /// Swap in a finished refresh, re-reading what ff changed meanwhile, and save it.
    /// Returns true when it was swapped in.
    pub fn poll_refresh(&mut self) -> bool {
        let Some(receiver) = &self.refresh else {
            return false;
        };
        let store = match receiver.try_recv() {
            Err(mpsc::TryRecvError::Empty) => return false,
            result => result.ok(),
        };
        self.refresh = None;
        self.walking = None;
        let changed = std::mem::take(&mut self.changed_during_refresh);
        let Some(store) = store else {
            return false;
        };
        let root = store
            .root
            .as_ref()
            .map_or_else(|| self.root.clone(), PathBuf::from);
        // A root that vanished walks as empty; keep the old index rather than save that.
        if !root.is_dir() {
            return false;
        }
        debug!(
            "Refreshed directory cache of {}: {} folders",
            root.display(),
            store.directories.len()
        );
        self.root = root;
        let excluded = std::mem::take(&mut self.store.excluded);
        self.store = store;
        // The walk can't count what it skipped, so keep the counts from before.
        self.store.excluded = excluded;
        // A walk started before the rules changed may hold folders they now leave out.
        if self.store.rules.as_ref() != Some(&self.rules) {
            self.prune_to_rules();
        }
        for path in changed {
            self.resync(&path);
        }
        self.save();
        true
    }

    /// The rules the index follows.
    pub fn rules(&self) -> &IndexRules {
        &self.rules
    }

    /// Follow `rules` from now on: drop what they leave out at once, and walk again in
    /// the background for what they let back in.
    pub fn set_rules(&mut self, rules: IndexRules) {
        if rules == self.rules {
            return;
        }
        self.rules = rules;
        self.prune_to_rules();
        self.save();
        // Keep walking a new root_dir rather than the old one.
        let root = self.walking.clone().unwrap_or_else(|| self.root.clone());
        if root.is_dir() {
            self.walk(&root, REFRESH_THREADS);
        }
    }

    /// Drop the folders the rules leave out, counting each `ignore_paths` folder's as
    /// it goes, and forget the counts of folders no longer excluded.
    fn prune_to_rules(&mut self) {
        let root = &self.root;
        let rules = &self.rules;
        let excluded = &mut self.store.excluded;
        excluded.retain(|path, _| rules.ignore_paths.contains(path));
        for path in &rules.ignore_paths {
            let count = self
                .store
                .directories
                .iter()
                .filter(|dir| below(dir, path).is_some())
                .count();
            if count > 0 {
                excluded.insert(path.clone(), count);
            }
        }
        self.store.directories.retain(|dir| {
            !rules.excludes(dir)
                && !Path::new(dir).strip_prefix(root).is_ok_and(|rest| {
                    rest.components()
                        .any(|part| rules.ignores_name(part.as_os_str()))
                })
        });
    }

    /// ff created, copied, moved, extracted or deleted `path`: re-read it and save.
    pub fn changed(&mut self, path: &Path) {
        self.changed_all([path]);
    }

    /// ff changed each of `paths`: re-read them, then save once.
    pub fn changed_all<'a>(&mut self, paths: impl IntoIterator<Item = &'a Path>) {
        let mut changed = false;
        for path in paths {
            if self.is_refreshing() {
                self.changed_during_refresh.push(path.to_path_buf());
            }
            changed |= self.resync(path);
        }
        if changed {
            self.save();
        }
    }

    /// ff renamed the folder `from` to `to`: rewrite the paths under it and save.
    /// `ignore_paths` folders at or below `from` move with it; returns, for each, its
    /// settings.toml entry and its new path, so settings.toml can follow.
    pub fn renamed(&mut self, from: &Path, to: &Path) -> Vec<(String, String)> {
        if self.is_refreshing() {
            self.changed_during_refresh
                .extend([from.to_path_buf(), to.to_path_buf()]);
        }
        let moved_rules = self.rename_rules(from, to);
        let changed = match (self.index_path(from), self.index_path(to)) {
            (Some(old), Some(new)) if self.contains(&old) && self.keeps_name(&new) => {
                self.store
                    .directories
                    .retain(|dir| below(dir, &new).is_none());
                for dir in &mut self.store.directories {
                    if let Some(rest) = below(dir, &old) {
                        *dir = format!("{new}{rest}");
                    }
                }
                // An `ignore_paths` folder may lie below the new name.
                let rules = &self.rules;
                self.store.directories.retain(|dir| !rules.excludes(dir));
                true
            }
            // The rules may treat the new name differently, so read it from disk.
            _ => self.resync(from) | self.resync(to),
        };
        if changed || !moved_rules.is_empty() {
            self.save();
        }
        moved_rules
    }

    /// Move the `ignore_paths` folders at or below `from` (and their counts) to `to`.
    fn rename_rules(&mut self, from: &Path, to: &Path) -> Vec<(String, String)> {
        let (Some(old), Some(new)) = (self.index_path(from), self.index_path(to)) else {
            return Vec::new();
        };
        let in_step = self.store.rules.as_ref() == Some(&self.rules);
        let mut moved = Vec::new();
        for path in &mut self.rules.ignore_paths {
            if let Some(rest) = below(path, &old) {
                let renamed = format!("{new}{rest}");
                moved.push((std::mem::replace(path, renamed.clone()), renamed));
            }
        }
        if moved.is_empty() {
            return moved;
        }
        self.rules.ignore_paths.sort();
        self.rules.ignore_paths.dedup();
        let mut entries = Vec::new();
        for (old_path, new_path) in moved {
            if let Some(count) = self.store.excluded.remove(&old_path) {
                self.store.excluded.insert(new_path.clone(), count);
            }
            let entry = self.rules.sources.remove(&old_path).unwrap_or(old_path);
            self.rules
                .sources
                .insert(new_path.clone(), new_path.clone());
            entries.push((entry, new_path));
        }
        // A cache that followed the rules still does: the folders moved with them.
        if in_step {
            self.store.rules = Some(self.rules.clone());
        }
        entries
    }

    /// Make the next start refresh the index, e.g. when quitting stopped an operation
    /// whose folders it can't see yet.
    pub fn mark_stale(&mut self) {
        self.store.built_at = Some(0);
        self.save();
    }

    /// Save the index, unless no walk has filled it yet: the next start walks again.
    /// ponytail: saves the whole index, so with several ff instances the last save wins
    /// until the next refresh re-reads the folders.
    pub fn save(&self) {
        if self.store.root.is_none() {
            return;
        }
        if let Some(path) = &self.cache_path {
            match save_directory_to_file(&self.store, &path.to_string_lossy()) {
                Ok(()) => debug!("Saved directory cache to {}", path.display()),
                Err(e) => debug!("Failed to save directory cache: {}", e),
            }
        }
    }

    /// Make the index match disk for `path`: drop it and everything under it, then add
    /// it back with its subfolders if a full walk would index it. Returns whether
    /// anything changed.
    /// ponytail: walks on the UI thread (about 0.1 ms a folder); move it to a
    /// worker if copying or moving huge trees makes the pause noticeable.
    fn resync(&mut self, path: &Path) -> bool {
        let Some(indexed) = self.index_path(path) else {
            return false;
        };
        let before = self.store.directories.len();
        self.store
            .directories
            .retain(|dir| below(dir, &indexed).is_none());
        let removed = before - self.store.directories.len();

        let indexed = Path::new(&indexed);
        // Files, links and deleted paths have no folders to add back.
        if !fs::symlink_metadata(indexed).is_ok_and(|m| m.is_dir()) {
            return removed > 0;
        }
        let (Some(parent), Some(name)) = (indexed.parent(), indexed.file_name()) else {
            return removed > 0;
        };
        // Below an unindexed (pruned) folder, a full walk would never get here.
        if parent != self.root && !self.contains(&parent.to_string_lossy()) {
            return removed > 0;
        }
        // Walk from the parent so its ignore files and the name rules apply to `indexed`.
        let name = name.to_os_string();
        let walker = index_walker(parent, &self.rules, move |e| {
            e.depth() > 1 || e.file_name() == name
        });
        let added = walk_directories(&walker);
        let added_any = !added.is_empty();
        self.store.directories.extend(added);
        removed > 0 || added_any
    }

    /// `path` spelled as the index spells it, or `None` if it isn't inside the root.
    /// Resolving the parent matches aliases such as /tmp and /private/tmp without
    /// following a link at `path` itself, which may no longer exist.
    fn index_path(&self, path: &Path) -> Option<String> {
        let parent = fs::canonicalize(path.parent()?).ok()?;
        let root = fs::canonicalize(&self.root).ok()?;
        let relative = parent.join(path.file_name()?);
        let relative = relative.strip_prefix(&root).ok()?;
        if relative.as_os_str().is_empty() {
            return None;
        }
        self.root.join(relative).to_str().map(String::from)
    }

    fn contains(&self, path: &str) -> bool {
        self.store.directories.iter().any(|dir| dir == path)
    }

    /// Whether a full walk could index a folder at `path`: not hidden, ignored by name
    /// or inside an `ignore_paths` folder.
    /// ponytail: gitignore patterns aren't checked here; the next refresh applies them.
    fn keeps_name(&self, path: &str) -> bool {
        !self.rules.excludes(path)
            && Path::new(path).file_name().is_some_and(|name| {
                !name.to_string_lossy().starts_with('.') && !self.rules.ignores_name(name)
            })
    }
}

/// Walk `root_dir` with `threads` walkers on a background thread; the receiver gets
/// the store when the walk finishes.
pub fn build_directory_from_store_async(
    root_dir: String,
    rules: IndexRules,
    threads: usize,
) -> mpsc::Receiver<DirectoryStore> {
    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        let built_at = Some(now_secs());
        let directories = Mutex::new(Vec::new());
        let mut builder = index_walker(Path::new(&root_dir), &rules, |_| true);
        builder.threads(threads);
        builder.build_parallel().run(|| {
            let directories = &directories;
            Box::new(move |result| {
                match result {
                    Ok(entry)
                        if entry.depth() > 0 && entry.file_type().is_some_and(|ft| ft.is_dir()) =>
                    {
                        if let Some(path) = entry.path().to_str() {
                            directories.lock().unwrap().push(path.to_string());
                        }
                    }
                    Ok(_) => {}
                    Err(err) => debug!("Error processing directory entry: {}", err),
                }
                ignore::WalkState::Continue
            })
        });
        // A walker that panicked leaves no store; the old index stays.
        let Ok(directories) = directories.into_inner() else {
            return;
        };
        let _ = tx.send(DirectoryStore {
            directories,
            built_at,
            root: Some(root_dir),
            rules: Some(rules),
            excluded: BTreeMap::new(),
        });
    });

    rx
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    /// An index of `root` as a full build makes it, never saved.
    fn index_over(root: &Path, ignore: &[&str]) -> DirectoryIndex {
        let ignore: Vec<String> = ignore.iter().map(|name| name.to_string()).collect();
        let store = build_directory_from_store(root.to_str().unwrap(), ignore.clone());
        DirectoryIndex::new(store, root.to_path_buf(), IndexRules::names(&ignore), None)
    }

    /// The index's folders below its root, sorted, e.g. ["/alpha", "/alpha/beta"].
    fn listed(index: &DirectoryIndex) -> Vec<String> {
        let root = index.root.to_str().unwrap();
        let mut dirs: Vec<String> = index
            .store
            .directories
            .iter()
            .map(|dir| below(dir, root).unwrap().to_string())
            .collect();
        dirs.sort();
        dirs
    }

    #[test]
    fn changed_adds_a_new_folder_and_its_subfolders_once() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        fs::create_dir(root.join("existing")).unwrap();
        let mut index = index_over(root, &["node_modules"]);

        fs::create_dir_all(root.join("alpha/beta/gamma")).unwrap();
        fs::create_dir_all(root.join("alpha/.hidden/inner")).unwrap();
        fs::create_dir_all(root.join("alpha/node_modules/pkg")).unwrap();
        fs::write(root.join("alpha/notes.txt"), "").unwrap();
        index.changed(&root.join("alpha"));
        index.changed(&root.join("alpha"));

        assert_eq!(
            listed(&index),
            ["/alpha", "/alpha/beta", "/alpha/beta/gamma", "/existing"]
        );
        // Exactly what a full rebuild finds.
        assert_eq!(listed(&index), listed(&index_over(root, &["node_modules"])));
    }

    #[test]
    fn changed_with_what_a_nested_create_returns_indexes_every_new_folder() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        fs::create_dir(root.join("existing")).unwrap();
        let mut index = index_over(root, &[]);

        for name in ["a/b/c/", "docs/todo.md", "existing/new/deeper/"] {
            let created = crate::operations::create_item_based_on_type(
                root.to_string_lossy().into_owned(),
                name.to_string(),
            )
            .unwrap();
            index.changed(&created);
        }

        assert_eq!(
            listed(&index),
            [
                "/a",
                "/a/b",
                "/a/b/c",
                "/docs",
                "/existing",
                "/existing/new",
                "/existing/new/deeper"
            ]
        );
    }

    #[test]
    fn changed_skips_what_a_full_walk_would_skip() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("node_modules")).unwrap();
        let mut index = index_over(root, &["node_modules"]);
        assert!(index.store.directories.is_empty());

        fs::create_dir(root.join(".secret")).unwrap();
        fs::create_dir(root.join("node_modules/deep")).unwrap();
        fs::write(root.join("notes.txt"), "").unwrap();
        let outside = tempdir().unwrap();
        fs::create_dir(outside.path().join("elsewhere")).unwrap();
        for path in [
            root.join(".secret"),
            root.join("node_modules"),
            root.join("node_modules/deep"),
            root.join("notes.txt"),
            root.to_path_buf(),
            outside.path().join("elsewhere"),
        ] {
            index.changed(&path);
        }

        assert!(index.store.directories.is_empty());
    }

    #[test]
    fn changed_after_a_delete_removes_the_folder_and_everything_under_it() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("alpha/beta")).unwrap();
        fs::create_dir(root.join("alphabet")).unwrap();
        let mut index = index_over(root, &[]);

        fs::remove_dir_all(root.join("alpha")).unwrap();
        index.changed(&root.join("alpha"));

        assert_eq!(listed(&index), ["/alphabet"]);
    }

    #[test]
    fn renamed_rewrites_the_prefix_without_duplicates() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("alpha/beta")).unwrap();
        fs::create_dir(root.join("alpha2")).unwrap();
        let mut index = index_over(root, &[]);
        // Stale entries for the new name, e.g. a folder deleted outside ff.
        index.store.insert(root.join("omega").to_str().unwrap());
        index.store.insert(root.join("omega/old").to_str().unwrap());

        fs::rename(root.join("alpha"), root.join("omega")).unwrap();
        index.renamed(&root.join("alpha"), &root.join("omega"));

        assert_eq!(listed(&index), ["/alpha2", "/omega", "/omega/beta"]);
    }

    #[test]
    fn renamed_reads_the_new_name_when_the_rules_treat_it_differently() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("alpha/beta")).unwrap();
        fs::create_dir_all(root.join("node_modules/pkg")).unwrap();
        let mut index = index_over(root, &["node_modules"]);

        fs::rename(root.join("alpha"), root.join(".alpha")).unwrap();
        index.renamed(&root.join("alpha"), &root.join(".alpha"));
        assert!(index.store.directories.is_empty());

        fs::rename(root.join("node_modules"), root.join("deps")).unwrap();
        index.renamed(&root.join("node_modules"), &root.join("deps"));
        assert_eq!(listed(&index), ["/deps", "/deps/pkg"]);
    }

    #[cfg(unix)]
    #[test]
    fn aliases_update_the_entry_as_the_index_spells_it() {
        let dir = tempdir().unwrap();
        let real = dir.path().join("real");
        let alias = dir.path().join("alias");
        fs::create_dir(&real).unwrap();
        std::os::unix::fs::symlink(&real, &alias).unwrap();
        let mut index = index_over(&real, &[]);

        fs::create_dir(real.join("alpha")).unwrap();
        index.changed(&alias.join("alpha"));
        index.changed(&fs::canonicalize(&real).unwrap().join("alpha"));

        assert_eq!(
            index.store.directories,
            [real.join("alpha").to_str().unwrap()]
        );
    }

    #[test]
    fn save_replaces_the_cache_through_a_temporary_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("cache.json");
        fs::write(&path, "old cache").unwrap();
        let store = DirectoryStore {
            directories: vec!["/a".into(), "/a/b".into()],
            built_at: Some(42),
            root: Some("/".into()),
            ..Default::default()
        };

        save_directory_to_file(&store, path.to_str().unwrap()).unwrap();

        let saved = load_directory_from_file(path.to_str().unwrap()).unwrap();
        assert_eq!(saved.directories, store.directories);
        assert_eq!(saved.built_at, Some(42));
        let names: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(names, ["cache.json"]);
    }

    #[test]
    fn failed_save_leaves_no_temporary_file() {
        let dir = tempdir().unwrap();
        // Renaming a file over a folder fails after the temporary file is written.
        let path = dir.path().join("cache.json");
        fs::create_dir(&path).unwrap();

        assert!(save_directory_to_file(&DirectoryStore::new(), path.to_str().unwrap()).is_err());

        assert!(path.is_dir());
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn needs_refresh_after_an_hour_or_when_the_clock_went_back() {
        let built = UNIX_EPOCH + Duration::from_secs(1_000_000);
        for (now, due) in [
            (built, false),
            (built + Duration::from_secs(59 * 60), false),
            (built + REFRESH_AFTER, true),
            (built + Duration::from_secs(3 * 24 * 60 * 60), true),
            (built - Duration::from_secs(60), true),
        ] {
            assert_eq!(needs_refresh(built, now), due, "{now:?}");
        }
    }

    #[test]
    fn load_infers_the_root_and_dates_old_caches_by_file_time() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("cache.json");
        fs::write(&path, r#"{"directories":["/r/a/b","/r/a","/r/c"]}"#).unwrap();
        let file_time = fs::metadata(&path)
            .unwrap()
            .modified()
            .unwrap()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let index =
            DirectoryIndex::load(&path, Path::new("/fallback"), IndexRules::default()).unwrap();
        assert_eq!(index.root, PathBuf::from("/r"));
        assert_eq!(index.store.root.as_deref(), Some("/r"));
        assert_eq!(index.store.built_at, Some(file_time));

        // A root the cache records wins over the inferred one.
        fs::write(&path, r#"{"directories":["/r/a"],"root":"/elsewhere"}"#).unwrap();
        let index =
            DirectoryIndex::load(&path, Path::new("/fallback"), IndexRules::default()).unwrap();
        assert_eq!(index.root, PathBuf::from("/elsewhere"));

        fs::write(&path, r#"{"directories":[],"built_at":7}"#).unwrap();
        let index =
            DirectoryIndex::load(&path, Path::new("/fallback"), IndexRules::default()).unwrap();
        assert_eq!(index.root, PathBuf::from("/fallback"));
        assert_eq!(index.store.built_at, Some(7));
    }

    /// Swap in the index's walk, failing after 5 seconds.
    fn finish_walk(index: &mut DirectoryIndex) {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while !index.poll_refresh() {
            assert!(std::time::Instant::now() < deadline, "walk never finished");
            thread::yield_now();
        }
    }

    #[test]
    fn walks_skip_the_home_library_but_no_other_library() {
        let dir = tempdir().unwrap();
        let home = dir.path().join("home");
        for path in [
            "Library/Caches",
            "Library2",
            "Library.old",
            "Projects/Library/notes",
        ] {
            fs::create_dir_all(home.join(path)).unwrap();
        }
        // What a walk from `root` finds, spelled below home.
        let walk = |root: &Path, keep: fn(&DirEntry) -> bool| {
            let rules = IndexRules {
                ignore_paths: vec![home.join("Library").to_string_lossy().into_owned()],
                ..Default::default()
            };
            let walker = index_walker(root, &rules, keep);
            let mut dirs: Vec<String> = walk_directories(&walker)
                .iter()
                .map(|dir| below(dir, home.to_str().unwrap()).unwrap().to_string())
                .collect();
            dirs.sort();
            dirs
        };

        let expected = [
            "/Library.old",
            "/Library2",
            "/Projects",
            "/Projects/Library",
            "/Projects/Library/notes",
        ];
        assert_eq!(walk(&home, |_| true), expected);
        // Re-reading home from its parent, as ff does after changing it, skips it too.
        let from_parent = walk(dir.path(), |e| {
            e.depth() > 1 || e.file_name() == OsStr::new("home")
        });
        assert_eq!(from_parent[0], "");
        assert_eq!(from_parent[1..], expected);
    }

    #[test]
    fn a_cache_of_another_folder_stays_searchable_while_root_dir_is_walked() {
        for records_root in [true, false] {
            let dir = tempdir().unwrap();
            let (old_root, root) = (dir.path().join("projects"), dir.path().join("home"));
            fs::create_dir_all(old_root.join("app")).unwrap();
            fs::create_dir_all(root.join("alpha")).unwrap();
            let cache = dir.path().join("cache.json");
            let mut old = build_directory_from_store(old_root.to_str().unwrap(), Vec::new());
            if !records_root {
                old.root = None;
            }
            save_directory_to_file(&old, cache.to_str().unwrap()).unwrap();

            let mut index = DirectoryIndex::open(&cache, &root, IndexRules::default(), false);
            assert!(index.is_refreshing() && !index.is_filling());
            assert_eq!(
                (&index.root, listed(&index)),
                (&old_root, vec!["/app".into()])
            );
            // ff's own change while it walks lands too.
            fs::create_dir(root.join("mine")).unwrap();
            index.changed(&root.join("mine"));
            finish_walk(&mut index);

            assert_eq!(index.root, root);
            assert_eq!(listed(&index), ["/alpha", "/mine"]);
            let saved = load_directory_from_file(cache.to_str().unwrap()).unwrap();
            assert_eq!(saved.root.as_deref(), root.to_str());
            assert_eq!(saved.directories, index.store.directories);
            // The next start keeps it.
            let index = DirectoryIndex::open(&cache, &root, IndexRules::default(), false);
            assert!(!index.is_refreshing());
        }
    }

    #[test]
    fn without_a_usable_cache_root_dir_is_walked_in_the_background() {
        for case in ["missing", "unreadable", "empty", "rebuild"] {
            let dir = tempdir().unwrap();
            let root = dir.path().join("home");
            fs::create_dir_all(root.join("alpha")).unwrap();
            let cache = dir.path().join("cache.json");
            match case {
                "unreadable" => fs::write(&cache, "not json").unwrap(),
                "empty" => fs::write(&cache, r#"{"directories":[],"built_at":1}"#).unwrap(),
                "rebuild" => {
                    let store = build_directory_from_store(root.to_str().unwrap(), Vec::new());
                    save_directory_to_file(&store, cache.to_str().unwrap()).unwrap();
                }
                _ => {}
            }
            let before = fs::read_to_string(&cache).ok();

            let mut index =
                DirectoryIndex::open(&cache, &root, IndexRules::default(), case == "rebuild");
            assert!(index.is_filling(), "{case}");
            // Changes before the walk ends aren't saved: the next start walks again.
            fs::create_dir(root.join("mine")).unwrap();
            index.changed(&root.join("mine"));
            index.mark_stale();
            assert_eq!(fs::read_to_string(&cache).ok(), before, "{case}");
            finish_walk(&mut index);

            assert!(!index.is_filling(), "{case}");
            assert_eq!(listed(&index), ["/alpha", "/mine"], "{case}");
            let saved = load_directory_from_file(cache.to_str().unwrap()).unwrap();
            assert_eq!(saved.root.as_deref(), root.to_str(), "{case}");
        }
    }

    #[test]
    fn stale_index_refreshes_in_the_background_and_saves() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("root");
        fs::create_dir_all(root.join("alpha")).unwrap();
        let cache = dir.path().join("cache.json");
        let now = SystemTime::now();
        let fresh = DirectoryStore {
            directories: Vec::new(),
            built_at: Some(now_secs()),
            root: Some(root.to_string_lossy().into_owned()),
            ..Default::default()
        };
        let mut index = DirectoryIndex::new(
            fresh,
            root.clone(),
            IndexRules::default(),
            Some(cache.clone()),
        );
        assert!(!index.refresh_if_stale(now, REFRESH_THREADS));

        index.store.built_at = Some(0);
        let mut missing = DirectoryIndex::new(
            index.store.clone(),
            dir.path().join("gone"),
            IndexRules::default(),
            None,
        );
        assert!(!missing.refresh_if_stale(now, REFRESH_THREADS));

        assert!(index.refresh_if_stale(now, REFRESH_THREADS));
        finish_walk(&mut index);

        assert!(!index.is_refreshing());
        assert_eq!(listed(&index), ["/alpha"]);
        let saved = load_directory_from_file(cache.to_str().unwrap()).unwrap();
        assert_eq!(saved.directories, index.store.directories);
        assert!(saved.built_at.unwrap() + 60 > now_secs());
    }

    #[test]
    fn refresh_keeps_the_changes_ff_made_while_it_walked() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        fs::create_dir(root.join("alpha")).unwrap();
        let mut index = index_over(root, &[]);
        let (walker, receiver) = mpsc::channel();
        index.refresh = Some(receiver);

        fs::create_dir(root.join("mine")).unwrap();
        index.changed(&root.join("mine"));
        fs::remove_dir(root.join("alpha")).unwrap();
        index.changed(&root.join("alpha"));
        fs::create_dir(root.join("outside")).unwrap();
        // The walk read the folders before ff's changes, but saw one made elsewhere.
        let walked = [root.join("alpha"), root.join("outside")];
        walker
            .send(DirectoryStore {
                directories: walked.iter().map(|p| p.to_str().unwrap().into()).collect(),
                built_at: Some(1),
                root: None,
                ..Default::default()
            })
            .unwrap();

        assert!(index.poll_refresh());
        assert_eq!(listed(&index), ["/mine", "/outside"]);
        assert_eq!(index.store.built_at, Some(1));
    }

    #[test]
    fn refresh_of_a_vanished_root_keeps_the_old_index() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("root");
        fs::create_dir_all(root.join("alpha")).unwrap();
        let mut index = index_over(&root, &[]);
        let (walker, receiver) = mpsc::channel();
        index.refresh = Some(receiver);
        fs::remove_dir_all(&root).unwrap();
        walker.send(DirectoryStore::new()).unwrap();

        assert!(!index.poll_refresh());
        assert!(!index.is_refreshing());
        assert_eq!(listed(&index), ["/alpha"]);
    }

    /// Rules excluding `paths`, given as settings.toml would (relative to `root` here).
    fn excluding(root: &Path, paths: &[&str]) -> IndexRules {
        let paths: Vec<String> = paths.iter().map(|path| path.to_string()).collect();
        IndexRules::new(&[], &paths, root)
    }

    /// Make a folder tree: Movies/CapCut/Cache, Movies2, go/pkg, gopher, work/app.
    fn media_tree(root: &Path) {
        for path in [
            "Movies/CapCut/Cache",
            "Movies2",
            "go/pkg",
            "gopher",
            "work/app",
        ] {
            fs::create_dir_all(root.join(path)).unwrap();
        }
    }

    #[test]
    fn ignore_paths_skip_exactly_those_folders_and_what_is_below_them() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        media_tree(root);
        let rules = excluding(root, &["Movies", "go"]);
        let walked = walk_directories(&index_walker(root, &rules, |_| true));
        let index = DirectoryIndex::new(
            DirectoryStore {
                directories: walked,
                ..Default::default()
            },
            root.to_path_buf(),
            rules,
            None,
        );
        // Movies2 and gopher only share a prefix with an excluded folder.
        assert_eq!(
            listed(&index),
            ["/Movies2", "/gopher", "/work", "/work/app"]
        );
    }

    #[test]
    fn rules_spell_paths_as_the_index_does() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        fs::create_dir(root.join("Movies")).unwrap();
        let spelled = root.join("Movies").to_string_lossy().into_owned();
        // Relative, through the resolved alias (/var vs /private/var on macOS), absolute.
        let canonical = fs::canonicalize(root).unwrap().join("Movies");
        for given in ["Movies", canonical.to_str().unwrap(), spelled.as_str()] {
            let rules = IndexRules::new(&[], &[given.to_string()], root);
            assert_eq!(
                rules.ignore_paths,
                std::slice::from_ref(&spelled),
                "{given}"
            );
        }
        let home = dirs::home_dir().unwrap();
        let rules = IndexRules::new(&[], &["~/ff-no-such-folder".to_string()], &home);
        assert_eq!(
            rules.ignore_paths,
            [home
                .join("ff-no-such-folder")
                .to_string_lossy()
                .into_owned()]
        );
        // Names are compared lowercased, in a stable order.
        let rules = IndexRules::names(&["Target".to_string(), "node_modules".to_string()]);
        assert_eq!(rules.ignore_directories, ["node_modules", "target"]);
    }

    #[test]
    fn a_cache_built_with_other_rules_is_pruned_at_once_then_walked_again() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("home");
        media_tree(&root);
        let cache = dir.path().join("cache.json");
        let walked = build_directory_from_store(root.to_str().unwrap(), Vec::new());
        save_directory_to_file(&walked, cache.to_str().unwrap()).unwrap();

        let rules = excluding(&root, &["Movies", "work/app"]);
        let mut index = DirectoryIndex::open(&cache, &root, rules.clone(), false);
        // Searchable without Movies before the walk ends, with what it held counted.
        assert_eq!(
            listed(&index),
            ["/Movies2", "/go", "/go/pkg", "/gopher", "/work"]
        );
        let movies = root.join("Movies").to_string_lossy().into_owned();
        let app = root.join("work/app").to_string_lossy().into_owned();
        assert_eq!(index.store.excluded.get(&movies), Some(&3));
        assert_eq!(index.store.excluded.get(&app), Some(&1));
        assert!(index.is_refreshing());

        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while !index.poll_refresh() {
            assert!(std::time::Instant::now() < deadline, "walk never finished");
            thread::yield_now();
        }
        assert_eq!(
            listed(&index),
            ["/Movies2", "/go", "/go/pkg", "/gopher", "/work"]
        );
        assert_eq!(index.store.rules.as_ref(), Some(&rules));
        // The walk can't count what it skipped; the counts from before stay.
        assert_eq!(index.store.excluded.get(&movies), Some(&3));

        // Same rules next time: nothing to redo.
        let index = DirectoryIndex::open(&cache, &root, rules, false);
        assert!(!index.is_refreshing());
        assert_eq!(index.store.excluded.get(&movies), Some(&3));

        // Letting Movies back in forgets its count and walks to find it again.
        let rules = excluding(&root, &["work/app"]);
        let mut index = DirectoryIndex::open(&cache, &root, rules, false);
        assert!(index.is_refreshing());
        assert!(!index.store.excluded.contains_key(&movies));
        while !index.poll_refresh() {
            thread::yield_now();
        }
        assert!(listed(&index).contains(&"/Movies/CapCut/Cache".to_string()));
    }

    #[test]
    fn a_walk_that_started_under_old_rules_is_pruned_when_it_lands() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        media_tree(root);
        let mut index = index_over(root, &[]);
        index.rules = excluding(root, &["Movies"]);
        let (walker, receiver) = mpsc::channel();
        index.refresh = Some(receiver);
        walker
            .send(build_directory_from_store(
                root.to_str().unwrap(),
                Vec::new(),
            ))
            .unwrap();

        assert!(index.poll_refresh());
        assert!(!listed(&index).iter().any(|dir| dir.starts_with("/Movies/")));
        assert!(listed(&index).contains(&"/Movies2".to_string()));
    }

    #[test]
    fn set_rules_prunes_at_once_and_walks_for_what_comes_back() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        media_tree(root);
        let mut index = index_over(root, &[]);

        index.set_rules(excluding(root, &["Movies"]));
        assert!(!listed(&index).iter().any(|dir| dir.starts_with("/Movies/")));
        assert!(index.is_refreshing());

        index.set_rules(IndexRules::default());
        while !index.poll_refresh() {
            thread::yield_now();
        }
        assert!(listed(&index).contains(&"/Movies/CapCut".to_string()));
    }

    #[test]
    fn renaming_an_excluded_folder_keeps_it_excluded() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        media_tree(root);
        let mut index = index_over(root, &[]);
        index.set_rules(excluding(root, &["Movies", "Movies/CapCut"]));
        while !index.poll_refresh() {
            thread::yield_now();
        }

        fs::rename(root.join("Movies"), root.join("Films")).unwrap();
        let moved = index.renamed(&root.join("Movies"), &root.join("Films"));

        let spell = |path: &str| root.join(path).to_string_lossy().into_owned();
        // Each settings.toml entry as written, and where its folder is now.
        assert_eq!(
            moved,
            [
                ("Movies".to_string(), spell("Films")),
                ("Movies/CapCut".to_string(), spell("Films/CapCut"))
            ]
        );
        assert_eq!(
            index.rules().ignore_paths,
            [spell("Films"), spell("Films/CapCut")]
        );
        assert!(!listed(&index).iter().any(|dir| dir.starts_with("/Films")));
        assert_eq!(index.store.excluded.get(&spell("Films")), Some(&3));
    }

    #[test]
    fn renaming_a_folder_onto_an_excluded_path_drops_it() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        media_tree(root);
        let mut index = index_over(root, &[]);
        index.rules = excluding(root, &["archive"]);

        fs::rename(root.join("work"), root.join("archive")).unwrap();
        let moved = index.renamed(&root.join("work"), &root.join("archive"));

        assert!(moved.is_empty());
        assert!(!listed(&index)
            .iter()
            .any(|dir| dir.contains("work") || dir.contains("archive")));
    }

    #[test]
    fn rules_drop_the_root_its_ancestors_and_folders_outside_it() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("home");
        fs::create_dir_all(root.join("a")).unwrap();
        let given: Vec<String> = [
            dir.path().to_str().unwrap(),
            root.to_str().unwrap(),
            "/elsewhere",
            "",
            "a/",
        ]
        .iter()
        .map(|path| path.to_string())
        .collect();
        let rules = IndexRules::new(&[], &given, &root);
        assert_eq!(
            rules.ignore_paths,
            [root.join("a").to_string_lossy().into_owned()]
        );

        // An ancestor of the root doesn't empty the index.
        let mut index = index_over(&root, &[]);
        index.set_rules(rules);
        while !index.poll_refresh() {
            thread::yield_now();
        }
        assert!(listed(&index).is_empty());
        index.set_rules(IndexRules::new(&[], &given[..1], &root));
        while !index.poll_refresh() {
            thread::yield_now();
        }
        assert_eq!(listed(&index), ["/a"]);
    }

    #[test]
    fn renaming_onto_a_parent_of_an_excluded_folder_keeps_that_folder_out() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("a/raw/deep")).unwrap();
        let mut index = index_over(root, &[]);
        index.rules = excluding(root, &["b/raw"]);

        fs::rename(root.join("a"), root.join("b")).unwrap();
        index.renamed(&root.join("a"), &root.join("b"));
        assert_eq!(listed(&index), ["/b"]);
    }

    #[test]
    fn a_rebuild_keeps_the_counts_of_excluded_folders() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("home");
        media_tree(&root);
        let cache = dir.path().join("cache.json");
        let walked = build_directory_from_store(root.to_str().unwrap(), Vec::new());
        save_directory_to_file(&walked, cache.to_str().unwrap()).unwrap();
        let rules = excluding(&root, &["Movies"]);
        let movies = root.join("Movies").to_string_lossy().into_owned();
        let index = DirectoryIndex::open(&cache, &root, rules.clone(), false);
        assert_eq!(index.store.excluded.get(&movies), Some(&3));
        drop(index);

        let mut index = DirectoryIndex::open(&cache, &root, rules, true);
        assert!(index.is_filling());
        while !index.poll_refresh() {
            thread::yield_now();
        }
        assert_eq!(index.store.excluded.get(&movies), Some(&3));
    }
}
