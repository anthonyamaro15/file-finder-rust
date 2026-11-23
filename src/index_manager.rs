//! The index manager: one folder's subfolders at a time, biggest first, each switched
//! in or out of the folder index (`ignore_paths`). Changes wait until they're saved.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use crate::directory_store::{below, DirectoryIndex};
use crate::index_tree::{key, IndexTree, TreeKind};

/// Which way the index manager shows the folders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IndexView {
    /// One folder's subfolders at a time.
    #[default]
    ByFolder,
    /// The heaviest branches as a tree (see `index_tree`).
    Biggest,
}

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
    /// The view shown; Tab switches.
    pub view: IndexView,
    /// The "biggest" view, built the first time it's shown.
    pub tree: Option<IndexTree>,
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
        let highlighted = self.rows.get(self.selected).map(|row| row.path.clone());
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

    /// The folder `␣` would switch: the by-folder row, or the tree line's folder.
    pub fn highlighted(&self) -> Option<&IndexRow> {
        match self.view {
            IndexView::ByFolder => self.rows.get(self.selected),
            IndexView::Biggest => {
                let tree = self.tree.as_ref()?;
                let node = tree.nodes.get(tree.cursor)?;
                matches!(node.kind, TreeKind::Folder { .. }).then_some(&node.row)
            }
        }
    }

    /// Whether `row` will be excluded once saved.
    pub fn is_off(&self, row: &IndexRow) -> bool {
        self.pending
            .get(&row.path)
            .map_or(row.excluded, |pending| pending.off)
    }

    /// How many changes saving would write.
    pub fn unsaved(&self) -> usize {
        let (add, remove) = self.changes();
        add.len() + remove.len()
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

    /// What tree line `i`'s "… N smaller" folders will hold once the changes inside
    /// them (not inside one of the big lines beside it) are saved.
    pub fn smaller_after(&self, tree: &IndexTree, i: usize) -> usize {
        let node = &tree.nodes[i];
        let mut count = node.row.count.unwrap_or(0) as isize;
        for (path, pending) in self.effective_changes() {
            let inside = below(path, &node.row.path).is_some_and(|rest| !rest.is_empty());
            let in_big = tree.nodes.iter().any(|other| {
                other.parent == node.parent
                    && matches!(other.kind, TreeKind::Folder { .. })
                    && below(path, &other.head).is_some()
            });
            if inside && !in_big {
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

    /// Move the highlight `delta` lines in the view shown.
    pub fn move_highlight(&mut self, delta: isize) {
        match self.view {
            IndexView::ByFolder => self.move_by(delta),
            IndexView::Biggest => self.tree_move(delta),
        }
    }

    pub fn move_by(&mut self, delta: isize) {
        let last = self.rows.len().saturating_sub(1) as isize;
        self.selected = (self.selected as isize + delta).clamp(0, last) as usize;
        self.confirm_discard = false;
    }

    /// The saved settings' changes: folders to add to `ignore_paths`, and the folders
    /// to take out of it (spelled as the index spells them).
    /// A folder switched off inside another one that ends up off isn't written: it
    /// would stay excluded unseen if the outer one were switched back on later.
    pub fn changes(&self) -> (Vec<PathBuf>, Vec<String>) {
        let mut add = Vec::new();
        let mut remove = Vec::new();
        for (path, pending) in &self.pending {
            if pending.off {
                if !self.inside_off(path) {
                    add.push(PathBuf::from(path));
                }
            } else {
                remove.push(path.clone());
            }
        }
        (add, remove)
    }

    /// The index was re-read: reload the folder shown and rebuild the tree, keeping
    /// the highlight on a line that's still shown.
    pub fn refresh(&mut self, index: &DirectoryIndex) {
        self.reload(index);
        if let Some(tree) = &self.tree {
            self.tree = Some(IndexTree::build(index, Some(tree)));
            self.settle_tree_cursor();
        }
    }

    /// Move the tree's highlight up to the nearest line shown, if it's on a hidden one.
    fn settle_tree_cursor(&mut self) {
        let shown = self.tree_visible();
        let Some(tree) = self.tree.as_mut() else {
            return;
        };
        let mut at = tree.cursor;
        while !shown.contains(&at) {
            match tree.nodes.get(at).and_then(|node| node.parent) {
                Some(parent) => at = parent,
                None => {
                    at = shown.first().copied().unwrap_or(0);
                    break;
                }
            }
        }
        tree.cursor = at;
    }

    /// Whether a tree line ends up off: switched off itself, or inside a folder that
    /// is (a joined chain's line is its deepest folder, so its head counts too).
    pub fn tree_off(&self, row: &IndexRow) -> bool {
        self.is_off(row) || self.inside_off(&row.path)
    }

    /// The folder that switched off a line inside it, for "… is off" messages.
    fn off_holder(&self, path: &str) -> Option<String> {
        self.final_off()
            .filter(|off| *off != path && below(path, off).is_some())
            .min_by_key(|off| off.len())
            .map(|off| off.rsplit('/').next().unwrap_or(off).to_string())
    }

    /// `␣`: switch the highlighted folder, unless it's inside a folder that's off.
    pub fn toggle_highlighted(&mut self) -> Result<(), String> {
        if let Some(row) = self.highlighted() {
            if let Some(holder) = self.off_holder(&row.path) {
                return Err(format!(
                    "{} is inside {holder}, which is off: turn {holder} on first",
                    row.name
                ));
            }
        }
        self.toggle();
        Ok(())
    }

    /// The tree's lines shown now: those not inside a folded or switched-off branch.
    pub fn tree_visible(&self) -> Vec<usize> {
        self.tree
            .as_ref()
            .map_or_else(Vec::new, |tree| tree.visible(|row| self.tree_off(row)))
    }

    /// Move the tree's highlight `delta` shown lines.
    pub fn tree_move(&mut self, delta: isize) {
        let shown = self.tree_visible();
        let Some(tree) = self.tree.as_mut() else {
            return;
        };
        let at = shown.iter().position(|&i| i == tree.cursor).unwrap_or(0) as isize;
        let last = shown.len().saturating_sub(1) as isize;
        if let Some(&i) = shown.get((at + delta).clamp(0, last) as usize) {
            tree.cursor = i;
        }
        self.confirm_discard = false;
    }

    /// `l` in the tree: open a folded branch, step into an open one, or, for a folder
    /// with nothing big inside or a "… smaller" line, show it in the by-folder view.
    /// Returns why not, for a folder that's off.
    pub fn tree_open(&mut self, index: &DirectoryIndex) -> Result<(), String> {
        let shown = self.tree_visible();
        let Some(node) = self
            .tree
            .as_ref()
            .and_then(|tree| tree.nodes.get(tree.cursor).cloned())
        else {
            return Ok(());
        };
        let off = self.tree_off(&node.row);
        let Some(tree) = self.tree.as_mut() else {
            return Ok(());
        };
        let i = tree.cursor;
        match node.kind {
            TreeKind::Folder { .. } if off => Err(format!(
                "{} is off: turn it on (␣) to choose folders inside it",
                node.row.name
            )),
            TreeKind::Folder { has_big: true, .. } if !tree.is_open(i) => {
                tree.flip(i);
                Ok(())
            }
            TreeKind::Folder { has_big: true, .. } => {
                // Already open: step onto its first line.
                if let Some(&next) = shown.iter().find(|&&j| tree.nodes[j].parent == Some(i)) {
                    tree.cursor = next;
                }
                Ok(())
            }
            TreeKind::Folder { .. } => {
                self.show_in_by_folder(index, PathBuf::from(&node.row.path), None);
                Ok(())
            }
            TreeKind::Smaller { .. } => {
                let big: Vec<String> = tree
                    .nodes
                    .iter()
                    .filter(|other| {
                        other.parent == node.parent && matches!(other.kind, TreeKind::Folder { .. })
                    })
                    .map(|other| other.row.path.clone())
                    .collect();
                self.show_in_by_folder(index, PathBuf::from(&node.row.path), Some(&big));
                Ok(())
            }
        }
    }

    /// `h` in the tree: fold an open branch, otherwise go to the line it hangs from.
    pub fn tree_fold(&mut self) {
        let Some(node) = self
            .tree
            .as_ref()
            .and_then(|tree| tree.nodes.get(tree.cursor).cloned())
        else {
            return;
        };
        // An off line shows nothing inside, so there's nothing to fold: go up.
        let off = self.tree_off(&node.row);
        let Some(tree) = self.tree.as_mut() else {
            return;
        };
        let i = tree.cursor;
        if tree.is_open(i) && !off {
            tree.flip(i);
        } else if let Some(parent) = node.parent {
            tree.cursor = parent;
        }
        self.confirm_discard = false;
    }

    /// Switch to the by-folder view showing `folder`, highlighting the first folder not
    /// in `skip` (the ones the tree already listed), or the first.
    fn show_in_by_folder(
        &mut self,
        index: &DirectoryIndex,
        folder: PathBuf,
        skip: Option<&[String]>,
    ) {
        self.view = IndexView::ByFolder;
        self.folder = folder;
        self.selected = 0;
        self.scroll = 0;
        self.reload(index);
        // A row holding a line the tree showed (e.g. media for "media / app") counts too.
        if let Some(skip) = skip {
            let shown = |row: &IndexRow| skip.iter().any(|path| below(path, &row.path).is_some());
            if let Some(i) = self.rows.iter().position(|row| !shown(row)) {
                self.selected = i;
            }
        }
        if let Some(tree) = self.tree.as_mut() {
            tree.arrived_at = None;
        }
    }

    /// Tab: switch views, keeping your place. To the tree: the line of the highlighted
    /// folder, or of the nearest folder holding it. Back: the tree line's folder in its
    /// parent, unless the tree's highlight hasn't moved, which leaves that view as it was.
    pub fn switch_view(&mut self, index: &DirectoryIndex) {
        self.confirm_discard = false;
        match self.view {
            IndexView::ByFolder => {
                let tree = self.tree.take();
                let mut tree = tree.unwrap_or_else(|| IndexTree::build(index, None));
                let here = self.rows.get(self.selected).map_or_else(
                    || self.folder.to_string_lossy().into_owned(),
                    |row| row.path.clone(),
                );
                self.tree = Some(tree.clone());
                let shown = self.tree_visible();
                let mut at = tree
                    .nearest(&here)
                    .or_else(|| shown.first().copied())
                    .unwrap_or(0);
                // Inside a folded branch: its nearest shown line.
                while !shown.contains(&at) {
                    match tree.nodes.get(at).and_then(|node| node.parent) {
                        Some(parent) => at = parent,
                        None => {
                            at = shown.first().copied().unwrap_or(0);
                            break;
                        }
                    }
                }
                tree.cursor = at;
                tree.arrived_at = tree.nodes.get(at).map(key);
                self.tree = Some(tree);
                self.view = IndexView::Biggest;
            }
            IndexView::Biggest => {
                let Some(tree) = self.tree.as_ref() else {
                    self.view = IndexView::ByFolder;
                    return;
                };
                if tree.arrived_at.is_some()
                    && tree.arrived_at == tree.nodes.get(tree.cursor).map(key)
                {
                    self.view = IndexView::ByFolder;
                    return;
                }
                let Some(node) = tree.nodes.get(tree.cursor).cloned() else {
                    self.view = IndexView::ByFolder;
                    return;
                };
                match node.kind {
                    TreeKind::Smaller { .. } => {
                        let big: Vec<String> = tree
                            .nodes
                            .iter()
                            .filter(|other| {
                                other.parent == node.parent
                                    && matches!(other.kind, TreeKind::Folder { .. })
                            })
                            .map(|other| other.row.path.clone())
                            .collect();
                        self.show_in_by_folder(index, PathBuf::from(&node.row.path), Some(&big));
                    }
                    TreeKind::Folder { .. } => {
                        let path = PathBuf::from(&node.row.path);
                        let parent = path
                            .parent()
                            .filter(|parent| parent.starts_with(&self.root))
                            .map_or_else(|| self.root.clone(), Path::to_path_buf);
                        self.show_in_by_folder(index, parent, None);
                        if let Some(i) = self.rows.iter().position(|row| row.path == node.row.path)
                        {
                            self.selected = i;
                        }
                    }
                }
            }
        }
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

    /// An index of /h with media/app/{cache,x} (cache holding 60), work/a (30) and
    /// filler, so big means 50 or more.
    fn tree_index() -> DirectoryIndex {
        let mut dirs: Vec<String> = [
            "media",
            "media/app",
            "media/app/cache",
            "media/app/x",
            "work",
            "work/a",
            "music",
        ]
        .iter()
        .map(|dir| dir.to_string())
        .collect();
        for (parent, n) in [
            ("media/app/cache", 60),
            ("work/a", 30),
            ("work", 10),
            ("music", 5),
        ] {
            dirs.extend((0..n).map(|i| format!("{parent}/f{i}")));
        }
        let dirs: Vec<&str> = dirs.iter().map(String::as_str).collect();
        index(Path::new("/h"), &dirs, &[])
    }

    fn tree_names(manager: &IndexManager) -> Vec<String> {
        let tree = manager.tree.as_ref().unwrap();
        manager
            .tree_visible()
            .into_iter()
            .map(|i| format!("{}{}", tree.nodes[i].prefix, tree.nodes[i].row.name))
            .collect()
    }

    #[test]
    fn tab_keeps_your_place_and_tab_tab_changes_nothing() {
        let index = tree_index();
        let mut manager = IndexManager::open(&index, false);
        select(&mut manager, "media");
        manager.switch_view(&index);
        assert_eq!(manager.view, IndexView::Biggest);
        assert_eq!(
            tree_names(&manager),
            ["media / app", "cache", "1 smaller", "2 smaller"]
        );
        // media itself isn't a line (it's joined into media / app): its nearest one.
        assert_eq!(manager.highlighted().unwrap().name, "app");

        // Back without moving: the by-folder view is as it was.
        manager.switch_view(&index);
        assert_eq!(
            (manager.view, manager.folder.as_path()),
            (IndexView::ByFolder, Path::new("/h"))
        );
        assert_eq!(manager.highlighted().unwrap().name, "media");

        // Move to cache, then back: its folder, with it highlighted.
        manager.switch_view(&index);
        manager.move_highlight(1);
        assert_eq!(manager.highlighted().unwrap().name, "cache");
        manager.switch_view(&index);
        assert_eq!(manager.folder, PathBuf::from("/h/media/app"));
        assert_eq!(manager.highlighted().unwrap().name, "cache");
    }

    #[test]
    fn space_in_the_tree_switches_the_tree_line_and_hides_what_is_inside() {
        let index = tree_index();
        let mut manager = IndexManager::open(&index, false);
        manager.switch_view(&index);
        manager.toggle(); // media / app: switches app, the deepest of the chain
        assert_eq!(manager.changes().0, [PathBuf::from("/h/media/app")]);
        assert_eq!(tree_names(&manager), ["media / app", "2 smaller"]);
        let app = manager.highlighted().unwrap().clone();
        assert!(manager.is_off(&app));
        let refused = manager.tree_open(&index).unwrap_err();
        assert!(refused.starts_with("app is off"), "{refused}");
    }

    #[test]
    fn a_folder_switched_off_inside_another_one_off_is_not_saved_twice() {
        let index = tree_index();
        let mut manager = IndexManager::open(&index, false);
        manager.switch_view(&index);
        manager.move_highlight(1);
        manager.toggle(); // cache
        manager.move_highlight(-1);
        manager.toggle(); // app, holding cache
        assert_eq!(
            manager.changes(),
            (vec![PathBuf::from("/h/media/app")], vec![])
        );
        assert_eq!(manager.unsaved(), 1);
    }

    #[test]
    fn l_shows_a_folder_with_nothing_big_inside_or_the_smaller_ones_in_by_folder() {
        let index = tree_index();
        let mut manager = IndexManager::open(&index, false);
        manager.switch_view(&index);
        manager.move_highlight(1); // cache: 60 small folders inside
        manager.tree_open(&index).unwrap();
        assert_eq!(
            (manager.view, manager.folder.as_path()),
            (IndexView::ByFolder, Path::new("/h/media/app/cache"))
        );

        manager.switch_view(&index);
        manager.move_highlight(10); // "2 smaller" at the top level: work and music
        manager.tree_open(&index).unwrap();
        assert_eq!(manager.folder, PathBuf::from("/h"));
        assert_eq!(manager.highlighted().unwrap().name, "work");
    }

    #[test]
    fn h_folds_an_open_branch_then_goes_to_its_parent() {
        let index = tree_index();
        let mut manager = IndexManager::open(&index, false);
        manager.switch_view(&index);
        manager.tree_fold();
        assert_eq!(tree_names(&manager), ["media / app", "2 smaller"]);
        manager.tree_open(&index).unwrap();
        manager.move_highlight(1);
        manager.tree_fold(); // cache has nothing to fold: up to its line
        assert_eq!(manager.highlighted().unwrap().name, "app");
    }

    #[test]
    fn tree_counts_match_what_by_folder_counts() {
        let index = tree_index();
        let tree = IndexTree::build(&index, None);
        for node in &tree.nodes {
            if let TreeKind::Folder { .. } = node.kind {
                assert_eq!(
                    node.row.count,
                    Some(folders_in(&index, &node.row.path)),
                    "{}",
                    node.row.path
                );
            }
        }
    }

    /// An index of /h from `dirs` plus `n` filler folders under each `filler` parent.
    fn filled(dirs: &[&str], filler: &[(&str, usize)]) -> DirectoryIndex {
        let mut all: Vec<String> = dirs.iter().map(|dir| dir.to_string()).collect();
        for (parent, n) in filler {
            all.extend((0..*n).map(|i| format!("{parent}/f{i}")));
        }
        let all: Vec<&str> = all.iter().map(String::as_str).collect();
        index(Path::new("/h"), &all, &[])
    }

    #[test]
    fn a_root_of_slash_has_a_tree() {
        let mut dirs: Vec<String> = vec!["/usr".into(), "/usr/lib".into(), "/opt".into()];
        dirs.extend((0..60).map(|i| format!("/usr/lib/f{i}")));
        let store = DirectoryStore {
            directories: dirs,
            root: Some("/".into()),
            ..Default::default()
        };
        let index = DirectoryIndex::new(store, PathBuf::from("/"), IndexRules::default(), None);
        let mut manager = IndexManager::open(&index, false);
        manager.switch_view(&index);
        assert_eq!(tree_names(&manager), ["usr / lib", "1 smaller"]);
    }

    #[test]
    fn keys_on_an_empty_tree_do_nothing() {
        let index = index(Path::new("/h"), &[], &[]);
        let mut manager = IndexManager::open(&index, false);
        manager.switch_view(&index);
        assert!(manager.tree_visible().is_empty());
        manager.tree_open(&index).unwrap();
        manager.tree_fold();
        manager.move_highlight(1);
        assert!(manager.toggle_highlighted().is_ok());
        assert_eq!(manager.unsaved(), 0);
    }

    #[test]
    fn a_chain_whose_upper_folder_is_off_shows_off_in_the_tree() {
        // big first, then media → app (a chain) holding cache and logs.
        let index = filled(
            &[
                "big",
                "media",
                "media/app",
                "media/app/cache",
                "media/app/logs",
            ],
            &[
                ("big", 230),
                ("media/app/cache", 70),
                ("media/app/logs", 60),
            ],
        );
        let mut manager = IndexManager::open(&index, false);
        select(&mut manager, "media");
        manager.toggle();
        manager.switch_view(&index);
        // Tab lands on the chain's line (not on the first line, big).
        assert_eq!(manager.highlighted().unwrap().name, "app");
        let app = manager.highlighted().unwrap().clone();
        assert!(manager.tree_off(&app));
        assert_eq!(tree_names(&manager), ["big", "media / app"]);
        // Nothing inside it opens, and ␣ explains what holds it.
        assert!(manager.tree_open(&index).unwrap_err().contains("off"));
        let refused = manager.toggle_highlighted().unwrap_err();
        assert!(refused.contains("inside media"), "{refused}");
        assert_eq!(manager.unsaved(), 1);
    }

    #[test]
    fn tab_from_a_small_top_level_folder_lands_on_its_smaller_line() {
        let index = tree_index();
        let mut manager = IndexManager::open(&index, false);
        select(&mut manager, "music");
        manager.switch_view(&index);
        let tree = manager.tree.as_ref().unwrap();
        let node = &tree.nodes[tree.cursor];
        assert!(matches!(node.kind, TreeKind::Smaller { .. }) && node.parent.is_none());
    }

    #[test]
    fn a_refresh_keeps_the_cursor_on_a_shown_line_and_tab_back_still_stays() {
        let mut index = tree_index();
        let mut manager = IndexManager::open(&index, false);
        manager.switch_view(&index);
        manager.move_highlight(1);
        assert_eq!(manager.highlighted().unwrap().name, "cache");
        // cache stops being a line: the index grows elsewhere until it's small.
        for i in 0..3000 {
            index.store.directories.push(format!("/h/music/g{i}"));
        }
        manager.refresh(&index);
        let shown = manager.tree_visible();
        let tree = manager.tree.as_ref().unwrap();
        assert!(shown.contains(&tree.cursor), "cursor on a hidden line");
    }

    #[test]
    fn h_on_a_switched_off_branch_goes_up_without_folding_it() {
        // top opens onto media / app (a chain), which opens onto x.
        let index = filled(
            &[
                "top",
                "top/media",
                "top/media/app",
                "top/media/app/x",
                "top/other",
            ],
            &[
                ("top/media/app/x", 100),
                ("top/media/app", 10),
                ("top/other", 40),
            ],
        );
        let mut manager = IndexManager::open(&index, false);
        manager.switch_view(&index);
        assert_eq!(tree_names(&manager)[..3], ["top", "media / app", "x"]);
        manager.move_highlight(1);
        manager.toggle_highlighted().unwrap(); // app off: x hidden
        assert!(!tree_names(&manager).contains(&"x".to_string()));
        manager.tree_fold();
        assert_eq!(manager.highlighted().unwrap().name, "top");
        manager.move_highlight(1);
        manager.toggle_highlighted().unwrap(); // back on: still open
        assert!(tree_names(&manager).contains(&"x".to_string()));
    }

    #[test]
    fn a_smaller_line_counts_the_changes_inside_it() {
        let index = tree_index();
        let mut manager = IndexManager::open(&index, false);
        select(&mut manager, "work");
        manager.toggle(); // work: one of the top-level "2 smaller"
        manager.switch_view(&index);
        let tree = manager.tree.clone().unwrap();
        let smaller = tree
            .nodes
            .iter()
            .position(|node| node.parent.is_none() && matches!(node.kind, TreeKind::Smaller { .. }))
            .unwrap();
        assert_eq!(tree.nodes[smaller].row.count, Some(48));
        assert_eq!(manager.smaller_after(&tree, smaller), 6);
    }
}
