use ignore::{DirEntry, WalkBuilder};
use log::debug;
use serde::{Deserialize, Serialize};
use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf, MAIN_SEPARATOR};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    mpsc, Arc, Mutex,
};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// How long a loaded cache is trusted before ff re-reads the folders in the background.
/// ff's own folder changes update the index at once, so this only bounds how long
/// folders created or removed outside ff stay unseen.
pub const REFRESH_AFTER: Duration = Duration::from_secs(60 * 60);

/// Walker threads for a background refresh; few, so browsing stays responsive.
pub const REFRESH_THREADS: usize = 2;

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct DirectoryStore {
    pub directories: Vec<String>,
    /// When the folders were read from disk, in seconds since the Unix epoch. Saving
    /// ff's own changes keeps it, so they don't postpone the next refresh.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub built_at: Option<u64>,
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

/// Walker with the rules every index build uses: hidden and gitignored entries and the
/// configured folder names are pruned. `keep` can prune more entries below `root`.
fn index_walker(
    root: &Path,
    ignore_directories: &[String],
    keep: impl Fn(&DirEntry) -> bool + Send + Sync + 'static,
) -> WalkBuilder {
    let ignore_lower: Vec<String> = ignore_directories
        .iter()
        .map(|s| s.to_lowercase())
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
        e.depth() == 0 || (!is_ignored(e.file_name(), &ignore_lower) && keep(e))
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
    let directories = walk_directories(&index_walker(
        Path::new(root_dir),
        &ignore_directories,
        |_| true,
    ));
    DirectoryStore {
        directories,
        built_at,
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
fn below<'a>(path: &'a str, dir: &str) -> Option<&'a str> {
    let rest = path.strip_prefix(dir)?;
    (rest.is_empty() || rest.starts_with(MAIN_SEPARATOR)).then_some(rest)
}

/// The folder index ff searches. ff's own folder changes update it at once and save
/// it; a background refresh picks up changes made elsewhere.
pub struct DirectoryIndex {
    pub store: DirectoryStore,
    /// Folder the index covers, spelled as its entries are.
    pub root: PathBuf,
    ignore_directories: Vec<String>,
    /// Where changes are saved; `None` keeps them in memory.
    cache_path: Option<PathBuf>,
    refresh: Option<mpsc::Receiver<CacheBuildProgress>>,
    /// Paths ff changed while a refresh was walking, re-read once it finishes.
    changed_during_refresh: Vec<PathBuf>,
}

impl DirectoryIndex {
    pub fn new(
        store: DirectoryStore,
        root: PathBuf,
        ignore_directories: Vec<String>,
        cache_path: Option<PathBuf>,
    ) -> Self {
        Self {
            store,
            root,
            ignore_directories,
            cache_path,
            refresh: None,
            changed_during_refresh: Vec::new(),
        }
    }

    /// Load the cache at `cache_path`. Its root is the parent of its shallowest folder:
    /// every indexed folder's parent is the root or indexed itself. An empty cache
    /// uses `fallback_root`, the folder a rebuild would walk.
    pub fn load(
        cache_path: &Path,
        fallback_root: &Path,
        ignore_directories: Vec<String>,
    ) -> io::Result<Self> {
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
        let root = store
            .directories
            .iter()
            .min_by_key(|dir| Path::new(dir).components().count())
            .and_then(|dir| Path::new(dir).parent())
            .unwrap_or(fallback_root)
            .to_path_buf();
        Ok(Self::new(
            store,
            root,
            ignore_directories,
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
        self.refresh = Some(build_directory_from_store_async(
            self.root.to_string_lossy().into_owned(),
            self.ignore_directories.clone(),
            threads,
        ));
        self.changed_during_refresh.clear();
        true
    }

    pub fn is_refreshing(&self) -> bool {
        self.refresh.is_some()
    }

    /// Swap in a finished refresh, re-reading what ff changed meanwhile, and save it.
    /// Returns true when it was swapped in.
    pub fn poll_refresh(&mut self) -> bool {
        let Some(receiver) = &self.refresh else {
            return false;
        };
        let store = loop {
            match receiver.try_recv() {
                Ok(CacheBuildProgress::Completed { store }) => break Some(store),
                Ok(CacheBuildProgress::Progress { .. }) => {}
                Err(mpsc::TryRecvError::Empty) => return false,
                Ok(CacheBuildProgress::Error(_)) | Err(mpsc::TryRecvError::Disconnected) => {
                    break None
                }
            }
        };
        self.refresh = None;
        let changed = std::mem::take(&mut self.changed_during_refresh);
        // A root that vanished walks as empty; keep the old index rather than save that.
        let Some(store) = store.filter(|_| self.root.is_dir()) else {
            return false;
        };
        debug!(
            "Refreshed directory cache: {} folders",
            store.directories.len()
        );
        self.store = store;
        for path in changed {
            self.resync(&path);
        }
        self.save();
        true
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
    pub fn renamed(&mut self, from: &Path, to: &Path) {
        if self.is_refreshing() {
            self.changed_during_refresh
                .extend([from.to_path_buf(), to.to_path_buf()]);
        }
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
                true
            }
            // The rules may treat the new name differently, so read it from disk.
            _ => self.resync(from) | self.resync(to),
        };
        if changed {
            self.save();
        }
    }

    /// Make the next start refresh the index, e.g. when quitting stopped an operation
    /// whose folders it can't see yet.
    pub fn mark_stale(&mut self) {
        self.store.built_at = Some(0);
        self.save();
    }

    /// ponytail: saves the whole index, so with several ff instances the last save wins
    /// until the next refresh re-reads the folders.
    pub fn save(&self) {
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
        let added = walk_directories(&index_walker(parent, &self.ignore_directories, move |e| {
            e.depth() > 1 || e.file_name() == name
        }));
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

    /// Whether a full walk could index a folder with this name: not hidden or ignored.
    /// ponytail: gitignore patterns aren't checked here; the next refresh applies them.
    fn keeps_name(&self, path: &str) -> bool {
        let ignore_lower: Vec<String> = self
            .ignore_directories
            .iter()
            .map(|s| s.to_lowercase())
            .collect();
        Path::new(path).file_name().is_some_and(|name| {
            !name.to_string_lossy().starts_with('.') && !is_ignored(name, &ignore_lower)
        })
    }
}

// Progress message for directory cache building
#[derive(Debug, Clone)]
pub enum CacheBuildProgress {
    Progress {
        directories_found: usize,
        current_path: String,
    },
    Completed {
        store: DirectoryStore,
    },
    Error(String),
}

// Async directory building with progress tracking (ignore::WalkBuilder, parallel)
pub fn build_directory_from_store_async(
    root_dir: String,
    ignore_directories: Vec<String>,
    threads: usize,
) -> mpsc::Receiver<CacheBuildProgress> {
    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        let store = Arc::new(Mutex::new(DirectoryStore {
            directories: Vec::new(),
            built_at: Some(now_secs()),
        }));
        const UPDATE_INTERVAL: usize = 100; // Update progress every 100 directories
        let counter = Arc::new(AtomicUsize::new(0));

        let mut builder = index_walker(Path::new(&root_dir), &ignore_directories, |_| true);
        builder.threads(threads);

        let walker = builder.build_parallel();
        let tx_progress = tx.clone();
        let store_arc = store.clone();
        let counter_arc = counter.clone();

        walker.run(|| {
            let tx_progress = tx_progress.clone();
            let store_arc = store_arc.clone();
            let counter_arc = counter_arc.clone();
            Box::new(move |res| {
                match res {
                    Ok(entry) => {
                        if entry.depth() == 0 {
                            return ignore::WalkState::Continue;
                        }
                        if entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
                            if let Some(s) = entry.path().to_str() {
                                {
                                    let mut guard = store_arc.lock().unwrap();
                                    guard.insert(s);
                                }
                                let new_count = counter_arc.fetch_add(1, Ordering::Relaxed) + 1;
                                if new_count.is_multiple_of(UPDATE_INTERVAL) {
                                    let _ = tx_progress.send(CacheBuildProgress::Progress {
                                        directories_found: new_count,
                                        current_path: s.to_string(),
                                    });
                                }
                            }
                        }
                    }
                    Err(e) => {
                        debug!("Error processing directory entry: {}", e);
                    }
                }
                ignore::WalkState::Continue
            })
        });

        // Final progress update
        let final_count = counter.load(Ordering::Relaxed);
        let _ = tx.send(CacheBuildProgress::Progress {
            directories_found: final_count,
            current_path: "Finalizing...".to_string(),
        });

        // Send completion message without risking a panic if Arc still has clones
        let store_cloned = {
            let guard = store.lock().unwrap();
            guard.clone()
        };
        let _ = tx.send(CacheBuildProgress::Completed {
            store: store_cloned,
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
        DirectoryIndex::new(store, root.to_path_buf(), ignore, None)
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

        let index = DirectoryIndex::load(&path, Path::new("/fallback"), Vec::new()).unwrap();
        assert_eq!(index.root, PathBuf::from("/r"));
        assert_eq!(index.store.built_at, Some(file_time));

        fs::write(&path, r#"{"directories":[],"built_at":7}"#).unwrap();
        let index = DirectoryIndex::load(&path, Path::new("/fallback"), Vec::new()).unwrap();
        assert_eq!(index.root, PathBuf::from("/fallback"));
        assert_eq!(index.store.built_at, Some(7));
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
        };
        let mut index = DirectoryIndex::new(fresh, root.clone(), Vec::new(), Some(cache.clone()));
        assert!(!index.refresh_if_stale(now, REFRESH_THREADS));

        index.store.built_at = Some(0);
        let mut missing = DirectoryIndex::new(
            index.store.clone(),
            dir.path().join("gone"),
            Vec::new(),
            None,
        );
        assert!(!missing.refresh_if_stale(now, REFRESH_THREADS));

        assert!(index.refresh_if_stale(now, REFRESH_THREADS));
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while !index.poll_refresh() {
            assert!(
                std::time::Instant::now() < deadline,
                "refresh never finished"
            );
            thread::yield_now();
        }

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
            .send(CacheBuildProgress::Completed {
                store: DirectoryStore {
                    directories: walked.iter().map(|p| p.to_str().unwrap().into()).collect(),
                    built_at: Some(1),
                },
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
        walker
            .send(CacheBuildProgress::Completed {
                store: DirectoryStore::new(),
            })
            .unwrap();

        assert!(!index.poll_refresh());
        assert!(!index.is_refreshing());
        assert_eq!(listed(&index), ["/alpha"]);
    }
}
