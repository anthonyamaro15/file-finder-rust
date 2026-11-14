//! The index manager's "biggest" view: the index as a tree whose heaviest branches are
//! opened down to the folders that fill it, with everything small folded into
//! "… N smaller" lines.

use std::collections::{BTreeMap, HashMap};

use crate::directory_store::{below, DirectoryIndex};
use crate::index_manager::IndexRow;

/// A branch is "big" when it holds at least this share of the index (and at least
/// `MIN_BIG` folders).
const BIG_PERCENT: usize = 2;
const MIN_BIG: usize = 50;
/// A branch opens by itself when its big subfolders hold at least this share of it, so
/// the tree never swaps a folder for a fragment of it.
const OPEN_SHARE: (usize, usize) = (2, 3);
/// "… N smaller" lines holding less than this share of the index are left out.
const SMALLER_PERCENT: usize = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TreeKind {
    /// A folder (or a chain of folders that each hold just one, joined).
    Folder {
        /// Whether it has big subfolders to open onto.
        has_big: bool,
        /// Whether it opens without being asked: its big subfolders hold most of it.
        opens: bool,
    },
    /// The folders of a branch left out as small: how many there are.
    Smaller { folders: usize },
}

/// One line of the tree.
#[derive(Debug, Clone)]
pub struct TreeNode {
    /// The folder `␣` switches (the deepest of a joined chain). For a "… smaller" line,
    /// `path` is the branch it belongs to and `count` what the small folders hold.
    pub row: IndexRow,
    /// The joined chain's folders above `row.name`, as "work / ", shown dimmed.
    pub prefix: String,
    /// The chain's first folder (`row.path` itself when nothing is joined).
    pub head: String,
    pub depth: usize,
    pub kind: TreeKind,
    /// The line it hangs from, if any.
    pub parent: Option<usize>,
}

/// The tree view's state: its lines in order, the branches opened or folded by hand,
/// and the highlighted line.
#[derive(Debug, Clone, Default)]
pub struct IndexTree {
    pub nodes: Vec<TreeNode>,
    /// Branches opened (true) or folded (false) by hand with `l`/`h`, by path.
    pub chosen: BTreeMap<String, bool>,
    /// The highlighted line (an index into `nodes`).
    pub cursor: usize,
    /// First visible line shown, kept between frames (set when drawn).
    pub scroll: usize,
    /// The line the cursor was on when arriving from the by-folder view (see `key`):
    /// Tab back only moves that view if the cursor moved since.
    pub arrived_at: Option<String>,
}

/// A line's identity across rebuilds: its path, and whether it's a "… smaller" line.
pub fn key(node: &TreeNode) -> String {
    match node.kind {
        TreeKind::Smaller { .. } => format!("{}\0smaller", node.row.path),
        TreeKind::Folder { .. } => node.row.path.clone(),
    }
}

impl IndexTree {
    /// The tree of `index`, keeping what `previous` had opened or folded and its
    /// highlight, by path.
    pub fn build(index: &DirectoryIndex, previous: Option<&IndexTree>) -> Self {
        let mut tree = Self {
            nodes: nodes(index),
            ..Self::default()
        };
        if let Some(previous) = previous {
            tree.chosen = previous.chosen.clone();
            tree.scroll = previous.scroll;
            tree.arrived_at = previous.arrived_at.clone();
            if let Some(node) = previous.nodes.get(previous.cursor) {
                let wanted = key(node);
                tree.cursor = tree
                    .nodes
                    .iter()
                    .position(|other| key(other) == wanted)
                    .or_else(|| tree.nearest(&node.row.path))
                    .unwrap_or(0);
            }
        }
        tree
    }

    /// Whether line `i` shows its big subfolders.
    pub fn is_open(&self, i: usize) -> bool {
        match self.nodes[i].kind {
            TreeKind::Folder { has_big, opens } => {
                has_big
                    && self
                        .chosen
                        .get(&self.nodes[i].row.path)
                        .copied()
                        .unwrap_or(opens)
            }
            TreeKind::Smaller { .. } => false,
        }
    }

    /// The lines shown, in order: those whose branches above are all open and on.
    pub fn visible(&self, is_off: impl Fn(&IndexRow) -> bool) -> Vec<usize> {
        let mut shown = Vec::with_capacity(self.nodes.len());
        let mut hidden: Vec<bool> = vec![false; self.nodes.len()];
        for (i, node) in self.nodes.iter().enumerate() {
            let hide = node.parent.is_some_and(|parent| {
                hidden[parent] || !self.is_open(parent) || is_off(&self.nodes[parent].row)
            });
            hidden[i] = hide;
            if !hide {
                shown.push(i);
            }
        }
        shown
    }

    /// The line closest to the folder `path`: the line of a chain it belongs to, the
    /// deepest line holding it, or the "… smaller" line it's folded into.
    pub fn nearest(&self, path: &str) -> Option<usize> {
        self.nodes
            .iter()
            .enumerate()
            .filter_map(|(i, node)| {
                let score = match node.kind {
                    // Anywhere from the chain's head down holds `path`.
                    TreeKind::Folder { .. } => {
                        below(path, &node.head)?;
                        node.row.path.len().min(path.len()).max(node.head.len())
                    }
                    // Strictly inside the branch, and not in one of its big lines.
                    TreeKind::Smaller { .. } => {
                        let rest = below(path, &node.row.path)?;
                        let in_big = self.nodes.iter().any(|other| {
                            other.parent == node.parent
                                && matches!(other.kind, TreeKind::Folder { .. })
                                && below(path, &other.head).is_some()
                        });
                        if rest.is_empty() || in_big {
                            return None;
                        }
                        node.row.path.len() + 1
                    }
                };
                Some((i, score))
            })
            .max_by_key(|(_, score)| *score)
            .map(|(i, _)| i)
    }

    /// Switch line `i` between open and folded.
    pub fn flip(&mut self, i: usize) {
        let open = self.is_open(i);
        self.chosen.insert(self.nodes[i].row.path.clone(), !open);
    }
}

/// The tree's lines in order, from the index.
fn nodes(index: &DirectoryIndex) -> Vec<TreeNode> {
    let root = index.root.to_string_lossy().into_owned();
    let total = index.store.directories.len();
    let big = (total * BIG_PERCENT / 100).max(MIN_BIG);
    let smaller_floor = total * SMALLER_PERCENT / 100;

    // Each folder's subfolders, and how many folders each holds, counted bottom-up.
    let mut children: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut counts: HashMap<&str, usize> = HashMap::new();
    let mut by_depth: Vec<(usize, &str)> = Vec::with_capacity(total);
    for dir in &index.store.directories {
        // Below the root ("/" holds everything but itself).
        let inside = if root == "/" {
            dir != "/"
        } else {
            below(dir, &root).is_some_and(|rest| !rest.is_empty())
        };
        if !inside {
            continue;
        }
        let parent = dir
            .rsplit_once('/')
            .map_or(root.as_str(), |(parent, _)| parent);
        let parent = if parent.is_empty() { "/" } else { parent };
        children.entry(parent).or_default().push(dir.as_str());
        counts.insert(dir.as_str(), 1);
        by_depth.push((
            dir.bytes().filter(|&byte| byte == b'/').count(),
            dir.as_str(),
        ));
    }
    by_depth.sort_unstable_by_key(|(depth, _)| std::cmp::Reverse(*depth));
    for (_, dir) in &by_depth {
        let count = counts[dir];
        let parent = dir
            .rsplit_once('/')
            .map_or(root.as_str(), |(parent, _)| parent);
        let parent = if parent.is_empty() { "/" } else { parent };
        if let Some(parent_count) = counts.get_mut(parent) {
            *parent_count += count;
        }
    }

    let mut nodes = Vec::new();
    let root_key = if root.is_empty() { "/" } else { root.as_str() };
    add_children(
        &mut nodes,
        root_key,
        None,
        0,
        &children,
        &counts,
        big,
        smaller_floor,
    );
    nodes
}

/// Add the big subfolders of `folder` below line `parent`, each with its own branch,
/// then one "… N smaller" line for the rest.
#[allow(clippy::too_many_arguments)]
fn add_children(
    nodes: &mut Vec<TreeNode>,
    folder: &str,
    parent: Option<usize>,
    depth: usize,
    children: &HashMap<&str, Vec<&str>>,
    counts: &HashMap<&str, usize>,
    big: usize,
    smaller_floor: usize,
) {
    let mut kids: Vec<(&str, usize)> = children
        .get(folder)
        .map(|kids| kids.iter().map(|kid| (*kid, counts[kid])).collect())
        .unwrap_or_default();
    kids.sort_unstable_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    let (large, small): (Vec<_>, Vec<_>) = kids.into_iter().partition(|(_, kid)| *kid >= big);
    for (kid, _) in large {
        // Join a chain of folders that each hold just one.
        let mut path = kid;
        let mut names = vec![name_of(kid)];
        while let Some([only]) = children.get(path).map(Vec::as_slice) {
            path = *only;
            names.push(name_of(path));
        }
        let deepest_count = counts[path];
        let name = names.pop().unwrap_or_default().to_string();
        let prefix: String = names.iter().map(|name| format!("{name} / ")).collect();
        let grandkids: Vec<usize> = children
            .get(path)
            .map(|kids| kids.iter().map(|kid| counts[kid]).collect())
            .unwrap_or_default();
        let big_held: usize = grandkids.iter().filter(|held| **held >= big).sum();
        let has_big = big_held > 0;
        let opens = has_big && big_held * OPEN_SHARE.1 >= deepest_count * OPEN_SHARE.0;
        let index = nodes.len();
        nodes.push(TreeNode {
            row: IndexRow {
                name,
                path: path.to_string(),
                count: Some(deepest_count),
                excluded: false,
                off_inside: 0,
            },
            prefix,
            head: kid.to_string(),
            depth,
            kind: TreeKind::Folder { has_big, opens },
            parent,
        });
        if has_big {
            add_children(
                nodes,
                path,
                Some(index),
                depth + 1,
                children,
                counts,
                big,
                smaller_floor,
            );
        }
    }
    // What the small subfolders hold: the folder minus itself and its big subfolders.
    let small_held: usize = small.iter().map(|(_, held)| held).sum();
    if !small.is_empty() && small_held >= smaller_floor.max(1) {
        nodes.push(TreeNode {
            row: IndexRow {
                name: format!("{} smaller", small.len()),
                path: folder.to_string(),
                count: Some(small_held),
                excluded: false,
                off_inside: 0,
            },
            prefix: String::new(),
            head: folder.to_string(),
            depth,
            kind: TreeKind::Smaller {
                folders: small.len(),
            },
            parent,
        });
    }
}

fn name_of(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::directory_store::{DirectoryStore, IndexRules};
    use std::path::Path;

    /// An index of /h holding `dirs` plus `n` filler folders under `filler`.
    fn index(dirs: &[&str], filler: &[(&str, usize)]) -> DirectoryIndex {
        let mut directories: Vec<String> = dirs.iter().map(|dir| format!("/h/{dir}")).collect();
        for (parent, n) in filler {
            for i in 0..*n {
                directories.push(format!("/h/{parent}/f{i}"));
            }
        }
        let store = DirectoryStore {
            directories,
            root: Some("/h".into()),
            ..Default::default()
        };
        DirectoryIndex::new(
            store,
            Path::new("/h").to_path_buf(),
            IndexRules::default(),
            None,
        )
    }

    /// Each visible line: indent, ▾/▸/space or …, the label and its count.
    fn lines(tree: &IndexTree) -> Vec<String> {
        tree.visible(|_| false)
            .into_iter()
            .map(|i| {
                let node = &tree.nodes[i];
                let glyph = match node.kind {
                    TreeKind::Smaller { .. } => "…",
                    _ if tree.is_open(i) => "▾",
                    TreeKind::Folder { has_big: true, .. } => "▸",
                    _ => " ",
                };
                format!(
                    "{}{glyph} {}{} {}",
                    "  ".repeat(node.depth),
                    node.prefix,
                    node.row.name,
                    node.row.count.unwrap()
                )
            })
            .collect()
    }

    #[test]
    fn the_heaviest_branches_open_down_to_what_fills_them() {
        // 100 + 10 folders: big means 50 or more.
        // 112 folders: big means 50 or more, "… smaller" lines need 1.
        let index = index(
            &[
                "media",
                "media/app",
                "media/app/cache",
                "media/app/x",
                "work",
                "work/a",
                "music",
            ],
            &[
                ("media/app/cache", 60),
                ("work/a", 30),
                ("work", 10),
                ("music", 5),
            ],
        );
        let tree = IndexTree::build(&index, None);
        assert_eq!(
            lines(&tree),
            [
                // app is media's only subfolder: the chain is one line.
                "▾ media / app 63",
                "    cache 61",
                "  … 1 smaller 1",
                "… 2 smaller 48",
            ]
        );
    }

    #[test]
    fn a_branch_whose_big_part_is_a_fragment_stays_closed_and_can_be_opened() {
        // venv: 60 in big/, 40 spread over small folders: 60 < ⅔ of 101, so closed.
        let mut dirs = vec!["venv", "venv/big"];
        let small: Vec<String> = (0..40).map(|i| format!("venv/s{i}")).collect();
        dirs.extend(small.iter().map(String::as_str));
        let index = index(&dirs, &[("venv/big", 59)]);
        let mut tree = IndexTree::build(&index, None);
        assert_eq!(lines(&tree), ["▸ venv 101"]);
        tree.flip(0);
        assert_eq!(
            lines(&tree),
            ["▾ venv 101", "    big 60", "  … 40 smaller 40"]
        );
    }

    #[test]
    fn counts_match_what_the_folder_holds() {
        let index = index(&["a", "a/b", "a/b-c", "a/b.d", "a/b/e"], &[("a/b/e", 70)]);
        let tree = IndexTree::build(&index, None);
        // "b-c" and "b.d" sort before "b/" but aren't inside it; b holds only e.
        let e = tree.nodes.iter().find(|node| node.row.name == "e").unwrap();
        assert_eq!((e.prefix.as_str(), e.row.count), ("b / ", Some(71)));
        assert_eq!(e.row.path, "/h/a/b/e");
        let a = &tree.nodes[0];
        assert_eq!(
            (a.prefix.as_str(), a.row.name.as_str(), a.row.count),
            ("", "a", Some(75))
        );
    }

    #[test]
    fn rebuilding_keeps_what_was_flipped_and_the_highlight() {
        let index = index(
            &["media", "media/app", "work"],
            &[("media/app", 60), ("work", 55)],
        );
        let mut tree = IndexTree::build(&index, None);
        let work = tree
            .nodes
            .iter()
            .position(|node| node.row.name == "work")
            .unwrap();
        tree.cursor = work;
        tree.flip(0);
        let rebuilt = IndexTree::build(&index, Some(&tree));
        assert_eq!(rebuilt.nodes[rebuilt.cursor].row.name, "work");
        assert!(rebuilt.chosen.contains_key(&tree.nodes[0].row.path));
    }
}
