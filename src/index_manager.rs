//! The index manager: one folder's subfolders at a time, biggest first, each switched
//! in or out of the folder index (`ignore_paths`). Changes wait until they're saved.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use crate::directory_store::{below, DirectoryIndex};

/// A subfolder of the folder shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexRow {
    pub name: String,
    /// Spelled as the index spells folders.
    pub path: String,
    /// Indexed folders it holds (itself included); for an excluded folder, what it held
    /// when last indexed, if known.
    pub count: Option<usize>,
    /// Excluded in the saved settings.
    pub excluded: bool,
    /// `ignore_paths` folders deeper inside it.
    pub off_inside: usize,
}

/// A change waiting to be saved, with the folder's count for the totals: what it holds
/// now when switched off, what it held when last indexed (if known) when switched on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Pending {
    off: bool,
    count: Option<usize>,
}

/// The open index manager.
#[derive(Debug, Default)]
pub struct IndexManager {
    /// The index's root: the manager never goes above it.
    pub root: PathBuf,
    /// The folder whose subfolders are listed, spelled as the index spells it.
    pub folder: PathBuf,
    pub rows: Vec<IndexRow>,
    pub selected: usize,
    /// First row shown, kept between frames (set when drawn).
    pub scroll: usize,
    /// Folders in the index now.
    pub total: usize,
    /// Hidden folders in `folder`, never indexed.
    pub hidden_here: usize,
    /// Whether `folder` is the home folder, whose Library is never indexed.
    pub at_home: bool,
    /// The `ignore_paths` folders in the saved settings.
    saved_off: Vec<String>,
    /// Whether the first index build is still running, so there's nothing to show.
    pub filling: bool,
    /// Esc was pressed once with unsaved changes; a second Esc discards them.
    pub confirm_discard: bool,
    /// Why the last save failed.
    pub error: Option<String>,
    pending: BTreeMap<String, Pending>,
}

impl IndexManager {
    /// The manager at the index's root.
    pub fn open(index: &DirectoryIndex, filling: bool) -> Self {
        let mut manager = Self {
            root: index.root.clone(),
            folder: index.root.clone(),
            filling,
            ..Self::default()
        };
        manager.reload(index);
        manager
    }

    /// Re-read the rows of the folder shown, e.g. after the index was refreshed,
    /// keeping the highlight on the same folder.
    pub fn reload(&mut self, index: &DirectoryIndex) {
        let highlighted = self.highlighted().map(|row| row.path.clone());
        self.total = index.store.directories.len();
        self.rows = rows(index, &self.folder);
        self.hidden_here = hidden_folders(&self.folder);
        self.at_home = dirs::home_dir().is_some_and(|home| same_folder(&home, &self.folder));
        self.saved_off = index.rules().ignore_paths.clone();
        // The index may have changed under the waiting changes: count them again.
        for (path, pending) in &mut self.pending {
            pending.count = if pending.off {
                Some(folders_in(index, path))
            } else {
                index.store.excluded.get(path).copied()
            };
        }
        self.selected = highlighted
            .and_then(|path| self.rows.iter().position(|row| row.path == path))
            .unwrap_or(0)
            .min(self.rows.len().saturating_sub(1));
    }

    pub fn highlighted(&self) -> Option<&IndexRow> {
        self.rows.get(self.selected)
    }

    /// Whether `row` will be excluded once saved.
    pub fn is_off(&self, row: &IndexRow) -> bool {
        self.pending
            .get(&row.path)
            .map_or(row.excluded, |pending| pending.off)
    }

    /// How many changes wait to be saved.
    pub fn unsaved(&self) -> usize {
        self.pending.len()
    }

    /// Switch the highlighted folder in or out; switching it back undoes the change.
    pub fn toggle(&mut self) {
        let Some(row) = self.highlighted().cloned() else {
            return;
        };
        let off = !self.is_off(&row);
        if off == row.excluded {
            self.pending.remove(&row.path);
        } else {
            self.pending.insert(
                row.path,
                Pending {
                    off,
                    count: row.count,
                },
            );
        }
        self.confirm_discard = false;
    }

    /// Whether `path` will be excluded once saved.
    fn ends_off(&self, path: &str) -> bool {
        self.pending
            .get(path)
            .map_or_else(|| self.saved_off.iter().any(|off| off == path), |p| p.off)
    }

    /// The folders that will be excluded once saved.
    fn final_off(&self) -> impl Iterator<Item = &String> {
        self.saved_off
            .iter()
            .chain(self.pending.keys())
            .filter(|path| self.ends_off(path))
    }

    /// Whether `path` lies inside another folder that will be excluded once saved, so
    /// its own change makes no difference to the totals.
    fn inside_off(&self, path: &str) -> bool {
        self.final_off()
            .any(|off| off != path && below(path, off).is_some())
    }

    /// The waiting changes that count: (folder, its change), leaving out those inside a
    /// folder that ends up excluded.
    fn effective_changes(&self) -> impl Iterator<Item = (&String, &Pending)> {
        self.pending
            .iter()
            .filter(|(path, _)| !self.inside_off(path))
    }

    /// Folders in the index once the changes are saved and re-read, and whether that
    /// misses folders switched back on whose count isn't known.
    pub fn total_after(&self) -> (usize, bool) {
        let mut total = self.total as isize;
        let mut unknown = false;
        for (_, pending) in self.effective_changes() {
            unknown |= pending.count.is_none();
            let count = pending.count.unwrap_or(0) as isize;
            total += if pending.off { -count } else { count };
        }
        (total.max(0) as usize, unknown)
    }

    /// What `row` will hold once the changes inside it are saved.
    pub fn count_after(&self, row: &IndexRow) -> usize {
        let mut count = row.count.unwrap_or(0) as isize;
        for (path, pending) in self.effective_changes() {
            if *path != row.path && below(path, &row.path).is_some() {
                let changed = pending.count.unwrap_or(0) as isize;
                count += if pending.off { -changed } else { changed };
            }
        }
        count.max(0) as usize
    }

    /// How many folders inside `row` will be excluded once saved.
    pub fn off_inside(&self, row: &IndexRow) -> usize {
        let mut inside: Vec<&String> = self
            .final_off()
            .filter(|path| **path != row.path && below(path, &row.path).is_some())
            .collect();
        inside.sort();
        inside.dedup();
        inside.len()
    }

    /// Show the highlighted folder's subfolders. Returns why not, for an excluded one.
    pub fn open_highlighted(&mut self, index: &DirectoryIndex) -> Result<(), String> {
        let Some(row) = self.highlighted().cloned() else {
            return Ok(());
        };
        if self.is_off(&row) {
            return Err(format!(
                "{} is off: turn it on (␣) to choose folders inside it",
                row.name
            ));
        }
        self.folder = PathBuf::from(&row.path);
        self.selected = 0;
        self.scroll = 0;
        self.confirm_discard = false;
        self.reload(index);
        Ok(())
    }

    /// Show the parent folder, with the folder just left highlighted. Not above the root.
    pub fn back(&mut self, index: &DirectoryIndex) {
        if self.folder == self.root {
            return;
        }
        let left = self.folder.to_string_lossy().into_owned();
        if let Some(parent) = self.folder.parent() {
            self.folder = parent.to_path_buf();
        }
        self.scroll = 0;
        self.confirm_discard = false;
        self.reload(index);
        if let Some(i) = self.rows.iter().position(|row| row.path == left) {
            self.selected = i;
        }
    }

    pub fn move_by(&mut self, delta: isize) {
        let last = self.rows.len().saturating_sub(1) as isize;
        self.selected = (self.selected as isize + delta).clamp(0, last) as usize;
        self.confirm_discard = false;
    }

    /// The saved settings' changes: folders to add to `ignore_paths`, and the folders
    /// to take out of it (spelled as the index spells them).
    pub fn changes(&self) -> (Vec<PathBuf>, Vec<String>) {
        let mut add = Vec::new();
        let mut remove = Vec::new();
        for (path, pending) in &self.pending {
            if pending.off {
                add.push(PathBuf::from(path));
            } else {
                remove.push(path.clone());
            }
        }
        (add, remove)
    }
}

/// The subfolders of `folder`, indexed or excluded, biggest first.
fn rows(index: &DirectoryIndex, folder: &Path) -> Vec<IndexRow> {
    // By path components, so a root of "/" works too.
    let child_of = |path: &str| -> Option<String> {
        let rest = Path::new(path).strip_prefix(folder).ok()?;
        let first = rest.components().next()?;
        Some(first.as_os_str().to_string_lossy().into_owned())
    };

    let mut counts: HashMap<String, usize> = HashMap::new();
    for dir in &index.store.directories {
        if let Some(child) = child_of(dir) {
            *counts.entry(child).or_insert(0) += 1;
        }
    }
    let mut rows: BTreeMap<String, IndexRow> = counts
        .into_iter()
        .map(|(name, count)| {
            let path = folder.join(&name).to_string_lossy().into_owned();
            let row = IndexRow {
                name: name.clone(),
                path,
                count: Some(count),
                excluded: false,
                off_inside: 0,
            };
            (name, row)
        })
        .collect();
    for excluded in &index.rules().ignore_paths {
        let Some(child) = child_of(excluded) else {
            continue;
        };
        let path = folder.join(&child).to_string_lossy().into_owned();
        let row = rows.entry(child.clone()).or_insert_with(|| IndexRow {
            name: child,
            path: path.clone(),
            count: None,
            excluded: false,
            off_inside: 0,
        });
        if *excluded == path {
            row.excluded = true;
            row.count = index.store.excluded.get(excluded).copied().or(row.count);
        } else {
            row.off_inside += 1;
        }
    }
    // A folder holding exclusions but not indexed itself (hidden, ~/Library, a name in
    // ignore_directories) is never indexed: nothing to switch.
    let mut rows: Vec<IndexRow> = rows
        .into_values()
        .filter(|row| row.count.is_some() || row.excluded)
        .collect();
    rows.sort_by(|a, b| {
        b.count
            .unwrap_or(0)
            .cmp(&a.count.unwrap_or(0))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    rows
}

/// Indexed folders at or below `path`.
fn folders_in(index: &DirectoryIndex, path: &str) -> usize {
    index
        .store
        .directories
        .iter()
        .filter(|dir| below(dir, path).is_some())
        .count()
}

/// Whether `a` and `b` are the same folder, however they're spelled.
fn same_folder(a: &Path, b: &Path) -> bool {
    let resolve = |path: &Path| fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    resolve(a) == resolve(b)
}

/// Hidden folders directly in `folder`.
fn hidden_folders(folder: &Path) -> usize {
    fs::read_dir(folder).map_or(0, |entries| {
        entries
            .flatten()
            .filter(|entry| {
                entry.file_name().to_string_lossy().starts_with('.')
                    && entry.file_type().is_ok_and(|kind| kind.is_dir())
            })
            .count()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::directory_store::{DirectoryStore, IndexRules};

    /// An index of `root` holding `dirs` (below it), excluding `excluded` with counts.
    fn index(root: &Path, dirs: &[&str], excluded: &[(&str, usize)]) -> DirectoryIndex {
        let spell = |dir: &str| root.join(dir).to_string_lossy().into_owned();
        let paths: Vec<String> = excluded.iter().map(|(path, _)| spell(path)).collect();
        let mut store = DirectoryStore {
            directories: dirs.iter().map(|dir| spell(dir)).collect(),
            root: Some(root.to_string_lossy().into_owned()),
            ..Default::default()
        };
        for (path, count) in excluded {
            store.excluded.insert(spell(path), *count);
        }
        DirectoryIndex::new(
            store,
            root.to_path_buf(),
            IndexRules::new(&[], &paths, root),
            None,
        )
    }

    type Shown = (String, Option<usize>, bool, usize);

    /// Each row: name, count, off once saved, and how many are off inside it.
    fn shown(manager: &IndexManager) -> Vec<Shown> {
        manager
            .rows
            .iter()
            .map(|row| {
                (
                    row.name.clone(),
                    row.count,
                    manager.is_off(row),
                    row.off_inside,
                )
            })
            .collect()
    }

    fn row(name: &str, count: usize, off: bool, off_inside: usize) -> Shown {
        (name.to_string(), Some(count), off, off_inside)
    }

    fn select(manager: &mut IndexManager, name: &str) {
        manager.selected = manager
            .rows
            .iter()
            .position(|row| row.name == name)
            .unwrap();
    }

    #[test]
    fn rows_are_the_subfolders_biggest_first_with_excluded_ones_and_what_is_off_inside() {
        let index = index(
            Path::new("/h"),
            &[
                "work",
                "work/a",
                "work/b",
                "Downloads",
                "Downloads/x",
                "music",
            ],
            &[("Movies", 5), ("Downloads/big.app", 3), ("gone", 0)],
        );
        let manager = IndexManager::open(&index, false);
        assert_eq!(manager.total, 6);
        assert_eq!(
            shown(&manager),
            [
                row("Movies", 5, true, 0),
                row("work", 3, false, 0),
                row("Downloads", 2, false, 1),
                row("music", 1, false, 0),
                row("gone", 0, true, 0),
            ]
        );
    }

    #[test]
    fn toggles_wait_to_be_saved_and_change_the_total_they_will_leave() {
        let index = index(
            Path::new("/h"),
            &["work", "work/a", "work/b", "music"],
            &[("Movies", 5)],
        );
        let mut manager = IndexManager::open(&index, false);
        assert_eq!((manager.unsaved(), manager.total_after().0), (0, 4));

        select(&mut manager, "work");
        manager.toggle();
        select(&mut manager, "Movies");
        manager.toggle();
        assert_eq!(manager.unsaved(), 2);
        // 4 indexed − work's 3 + Movies' 5 when last indexed.
        assert_eq!(manager.total_after().0, 6);
        assert_eq!(
            manager.changes(),
            (
                vec![PathBuf::from("/h/work")],
                vec!["/h/Movies".to_string()]
            )
        );

        // Switching back undoes the change.
        manager.toggle();
        assert_eq!((manager.unsaved(), manager.total_after().0), (1, 1));
    }

    #[test]
    fn a_folder_off_inside_another_one_switched_off_counts_once() {
        let index = index(
            Path::new("/h"),
            &["work", "work/a", "work/a/b", "music"],
            &[],
        );
        let mut manager = IndexManager::open(&index, false);
        select(&mut manager, "work");
        manager.open_highlighted(&index).unwrap();
        manager.toggle(); // work/a: 2 folders
        manager.back(&index);
        assert_eq!(manager.highlighted().unwrap().name, "work");
        manager.toggle(); // work: 3 folders, a included
        assert_eq!(manager.total_after().0, 1);
    }

    #[test]
    fn opening_goes_down_one_folder_and_back_stops_at_the_root() {
        let index = index(
            Path::new("/h"),
            &["work", "work/a", "music"],
            &[("Movies", 2)],
        );
        let mut manager = IndexManager::open(&index, false);

        select(&mut manager, "Movies");
        let refused = manager.open_highlighted(&index).unwrap_err();
        assert!(refused.starts_with("Movies is off"), "{refused}");
        assert_eq!(manager.folder, PathBuf::from("/h"));

        select(&mut manager, "work");
        manager.open_highlighted(&index).unwrap();
        assert_eq!(manager.folder, PathBuf::from("/h/work"));
        assert_eq!(shown(&manager), [row("a", 1, false, 0)]);
        manager.back(&index);
        manager.back(&index);
        assert_eq!(manager.folder, PathBuf::from("/h"));
        assert_eq!(manager.highlighted().unwrap().name, "work");
    }

    #[test]
    fn reload_keeps_the_highlight_and_the_unsaved_changes() {
        let mut index = index(Path::new("/h"), &["work", "music", "music/x"], &[]);
        let mut manager = IndexManager::open(&index, false);
        select(&mut manager, "work");
        manager.toggle();
        index.store.directories.push("/h/art".into());
        index.store.directories.push("/h/art/a".into());
        index.store.directories.push("/h/art/b".into());

        manager.reload(&index);
        assert_eq!(manager.highlighted().unwrap().name, "work");
        assert_eq!(manager.unsaved(), 1);
        assert_eq!(manager.total, 6);
    }

    #[test]
    fn a_folders_share_counts_the_changes_inside_it() {
        // work holds 4: itself, big, big/x and small.
        let index = index(
            Path::new("/h"),
            &["work", "work/big", "work/big/x", "work/small", "music"],
            &[],
        );
        let mut manager = IndexManager::open(&index, false);
        select(&mut manager, "work");
        manager.open_highlighted(&index).unwrap();
        select(&mut manager, "big");
        manager.toggle();
        manager.back(&index);
        let work = manager.highlighted().unwrap().clone();
        assert_eq!(manager.total_after().0, 3);
        assert_eq!(manager.count_after(&work), 2);
        assert_eq!(manager.off_inside(&work), 1);
    }

    #[test]
    fn a_folder_switched_on_inside_one_that_stays_off_changes_nothing() {
        let index = index(Path::new("/h"), &["music"], &[("work", 3), ("work/a", 2)]);
        let mut manager = IndexManager::open(&index, false);
        select(&mut manager, "work");
        manager.toggle(); // on
        manager.open_highlighted(&index).unwrap();
        select(&mut manager, "a");
        manager.toggle(); // on
        manager.back(&index);
        manager.toggle(); // work off again
        assert_eq!(manager.total_after(), (1, false));
    }

    #[test]
    fn a_walk_landing_recounts_the_waiting_changes() {
        let mut index = index(Path::new("/h"), &["work", "work/a", "music"], &[]);
        let mut manager = IndexManager::open(&index, false);
        select(&mut manager, "work");
        manager.toggle();
        assert_eq!(manager.total_after().0, 1);
        for dir in ["/h/work/b", "/h/work/c"] {
            index.store.directories.push(dir.into());
        }
        manager.reload(&index);
        assert_eq!(manager.total_after().0, 1);
        assert_eq!(manager.total, 5);
    }

    #[test]
    fn folders_never_indexed_get_no_row_and_unknown_counts_say_so() {
        // ~/Library holds an exclusion but isn't indexed itself: no row for it.
        let mut index = index(Path::new("/h"), &["music"], &[("Library/Caches", 9)]);
        let manager = IndexManager::open(&index, false);
        assert_eq!(shown(&manager), [row("music", 1, false, 0)]);

        // Switching on a folder whose count was never recorded.
        index.store.excluded.clear();
        index.set_rules(IndexRules::new(
            &[],
            &["/h/old".to_string()],
            Path::new("/h"),
        ));
        let mut manager = IndexManager::open(&index, false);
        select(&mut manager, "old");
        manager.toggle();
        assert_eq!(manager.total_after(), (1, true));
    }

    #[test]
    fn a_root_of_slash_lists_its_folders() {
        let store = DirectoryStore {
            directories: vec!["/usr".into(), "/usr/bin".into(), "/opt".into()],
            root: Some("/".into()),
            ..Default::default()
        };
        let index = DirectoryIndex::new(store, PathBuf::from("/"), IndexRules::default(), None);
        let manager = IndexManager::open(&index, false);
        assert_eq!(
            shown(&manager),
            [row("usr", 2, false, 0), row("opt", 1, false, 0)]
        );
    }
}
