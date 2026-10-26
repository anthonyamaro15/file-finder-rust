//! Folder sizes for listings sorted by size: walked on a few dedicated threads, and
//! remembered while ff runs.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    time::{SystemTime, UNIX_EPOCH},
};

use crate::info::{count_folder, FolderCount};
use crate::utils::files::{EntryInfo, Listing, SortType};
use crate::utils::format_file_size;

/// Most walks that run at once, however many cores there are.
const MAX_WALKERS: usize = 4;

/// A folder's total, counted as Get Info counts it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FolderTotal {
    pub bytes: u64,
    /// Some entries couldn't be read, so the real total may be larger.
    pub partial: bool,
}

/// "3.2 GB", or "≥ 3.2 GB" when some of it couldn't be read.
pub fn total_text(total: FolderTotal) -> String {
    let marker = if total.partial { "≥ " } else { "" };
    format!("{marker}{}", format_file_size(total.bytes))
}

/// What a listing sorted by size shows for an item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowSize {
    /// A file's size, or a sized folder's total.
    Known(FolderTotal),
    /// A folder still being sized.
    Sizing,
    /// A link to a folder, which is never followed.
    Unsized,
}

/// A walk's folder and cancel flag, with its count unless it was cancelled.
type WalkResult = (PathBuf, Arc<AtomicBool>, Option<FolderCount>);

/// The totals of the folders listed in size order, shared by both panes, and the
/// walks still counting them.
#[derive(Debug)]
pub struct FolderSizes {
    /// ponytail: kept until ff exits; dropped only for ff's own changes and what the
    /// watchers see in a listed folder, so other apps' changes deeper down show after a
    /// restart. Watching every listed folder recursively would catch those.
    totals: HashMap<PathBuf, FolderTotal>,
    /// The folders being walked, each with its walk's cancel flag.
    walks: HashMap<PathBuf, Arc<AtomicBool>>,
    sender: mpsc::Sender<WalkResult>,
    receiver: mpsc::Receiver<WalkResult>,
    /// Started with the first walk, so nothing runs until a listing is sorted by size.
    /// Never rayon's global pool, which reads the folders being browsed.
    pool: Option<rayon::ThreadPool>,
}

impl Default for FolderSizes {
    fn default() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            totals: HashMap::new(),
            walks: HashMap::new(),
            sender,
            receiver,
            pool: None,
        }
    }
}

impl Drop for FolderSizes {
    fn drop(&mut self) {
        for cancel in self.walks.values() {
            cancel.store(true, Ordering::Relaxed);
        }
    }
}

impl FolderSizes {
    pub fn total(&self, folder: &Path) -> Option<FolderTotal> {
        self.totals.get(folder).copied()
    }

    /// What size order shows for the listed item `file`.
    pub fn row_size(&self, file: &str, info: &EntryInfo) -> RowSize {
        if !info.is_dir() {
            RowSize::Known(FolderTotal {
                bytes: info.size,
                partial: false,
            })
        } else if info.is_symlink {
            RowSize::Unsized
        } else {
            self.total(Path::new(file))
                .map_or(RowSize::Sizing, RowSize::Known)
        }
    }

    /// Walk `folder`, unless its total is known or it's being walked.
    pub fn request(&mut self, folder: &Path) {
        if self.totals.contains_key(folder) || self.walks.contains_key(folder) {
            return;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        self.walks.insert(folder.to_path_buf(), cancel.clone());
        let sender = self.sender.clone();
        let folder = folder.to_path_buf();
        let pool = self.pool.get_or_insert_with(|| {
            let threads = std::thread::available_parallelism()
                .map_or(1, |n| n.get())
                .min(MAX_WALKERS);
            rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .thread_name(|i| format!("ff-sizes-{i}"))
                .build()
                .expect("start the folder size threads")
        });
        pool.spawn(move || {
            let count = count_folder(&folder, &cancel, |_| {});
            let _ = sender.send((folder, cancel, count));
        });
    }

    /// Stop walking the items of `dir`.
    pub fn cancel_in(&mut self, dir: &Path) {
        self.walks.retain(|folder, cancel| {
            let keep = folder.parent() != Some(dir);
            if !keep {
                cancel.store(true, Ordering::Relaxed);
            }
            keep
        });
    }

    /// Drop the totals of `path` and every folder above it, stopping their walks:
    /// something inside them changed. Returns whether there was anything to drop.
    pub fn forget_with_ancestors(&mut self, path: &Path) -> bool {
        let mut forgot = false;
        for folder in path.ancestors() {
            if let Some(cancel) = self.walks.remove(folder) {
                cancel.store(true, Ordering::Relaxed);
                forgot = true;
            }
            forgot |= self.totals.remove(folder).is_some();
        }
        forgot
    }

    /// Keep the totals of finished walks. Returns whether any arrived.
    pub fn poll(&mut self) -> bool {
        let mut arrived = false;
        while let Ok((folder, cancel, count)) = self.receiver.try_recv() {
            // A walk cancelled or started again since: its result no longer applies.
            if !self
                .walks
                .get(&folder)
                .is_some_and(|current| Arc::ptr_eq(current, &cancel))
            {
                continue;
            }
            self.walks.remove(&folder);
            if let Some(count) = count {
                let total = FolderTotal {
                    bytes: count.bytes,
                    partial: count.unreadable > 0,
                };
                self.totals.insert(folder, total);
                arrived = true;
            }
        }
        arrived
    }
}

/// One listing's part in folder sizing, while it's sorted by size.
#[derive(Debug, Default)]
pub struct ListingSizes {
    /// The folder whose items are sized; `None` outside size order.
    pub folder: Option<PathBuf>,
    /// The folder totals the listing's order was sorted with.
    order: HashMap<String, u64>,
    /// The listing was read again, or totals changed, since its folders were last
    /// counted up.
    pub changed: bool,
    /// Totals may have changed since the order was sorted; it is again once the
    /// listing may reorder.
    resort: bool,
    /// Folders sized, of those listed, while some are still being sized.
    pub progress: Option<(usize, usize)>,
    /// What the listed items add up to, once every folder is sized.
    pub here: Option<FolderTotal>,
    /// Free space on the folder's disk.
    pub free: Option<u64>,
}

impl ListingSizes {
    /// Sort `listing` by size, each folder by the total the current order used, so
    /// reading the listing again never moves an item for a newer total.
    pub fn sort(&self, listing: &mut Listing, sort_type: &SortType) {
        listing.sort_by_size(sort_type, |file| self.order.get(file).copied());
    }

    /// Size the items of `folder` from now on (`None` stops), leaving the walks of
    /// the folder before to `other`, the other listing's sized folder, if it's the same.
    pub fn follow(
        &mut self,
        folder: Option<PathBuf>,
        other: Option<&Path>,
        sizes: &mut FolderSizes,
    ) {
        if self.folder == folder {
            return;
        }
        if let Some(old) = self.folder.take() {
            if other != Some(old.as_path()) {
                sizes.cancel_in(&old);
            }
        }
        *self = Self {
            free: folder.as_deref().and_then(free_space),
            folder,
            changed: true,
            ..Self::default()
        };
    }

    /// Ask for the totals the listing still lacks and count up what it shows, after
    /// it changed or totals arrived.
    pub fn update(
        &mut self,
        files: &[String],
        info: &[EntryInfo],
        sizes: &mut FolderSizes,
        arrived: bool,
    ) {
        let Some(folder) = &self.folder else {
            return;
        };
        if !(self.changed || arrived) {
            return;
        }
        let (mut folders, mut sized) = (0, 0);
        let mut here = FolderTotal::default();
        for (file, info) in files.iter().zip(info) {
            match sizes.row_size(file, info) {
                RowSize::Known(total) => {
                    if info.is_dir() {
                        folders += 1;
                        sized += 1;
                    }
                    here.bytes += total.bytes;
                    here.partial |= total.partial;
                }
                RowSize::Sizing => {
                    folders += 1;
                    sizes.request(Path::new(file));
                }
                RowSize::Unsized => {}
            }
        }
        let was_finished = self.here.is_some();
        self.progress = (sized < folders).then_some((sized, folders));
        self.here = (sized == folders).then_some(here);
        if self.here.is_some() && !was_finished {
            self.free = free_space(folder);
        }
        self.changed = false;
        self.resort = true;
    }

    /// Take the totals `sizes` has for the listed folders as the order's, if totals
    /// changed since. Returns whether the order needs sorting again.
    pub fn take_resort(
        &mut self,
        files: &[String],
        info: &[EntryInfo],
        sizes: &FolderSizes,
    ) -> bool {
        if !std::mem::take(&mut self.resort) {
            return false;
        }
        let order: HashMap<String, u64> = files
            .iter()
            .zip(info)
            .filter_map(|(file, info)| match sizes.row_size(file, info) {
                RowSize::Known(total) if info.is_dir() => Some((file.clone(), total.bytes)),
                _ => None,
            })
            .collect();
        let changed = order != self.order;
        self.order = order;
        changed
    }

    /// "7.1 GB here · 212 GB free", the total only once every folder is sized.
    pub fn summary(&self) -> Option<String> {
        self.folder.as_ref()?;
        let here = self.here.map(|here| format!("{} here", total_text(here)));
        let free = self
            .free
            .map(|free| format!("{} free", format_file_size(free)));
        let parts: Vec<String> = here.into_iter().chain(free).collect();
        (!parts.is_empty()).then(|| parts.join(" · "))
    }

    /// "⠼ 3 of 5 folders sized", while sizing runs.
    pub fn progress_text(&self, now: SystemTime) -> Option<String> {
        const SPINNER: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
        let (sized, folders) = self.progress?;
        let tick = now
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            / 100;
        let spinner = SPINNER[tick as usize % SPINNER.len()];
        let noun = if folders == 1 { "folder" } else { "folders" };
        Some(format!("{spinner} {sized} of {folders} {noun} sized"))
    }
}

/// Space free to ff on `path`'s disk: statvfs's available blocks times their size.
#[cfg(unix)]
pub fn free_space(path: &Path) -> Option<u64> {
    use std::os::unix::ffi::OsStrExt;

    let path = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    // SAFETY: statvfs is a plain C struct, for which all zeros is valid.
    let mut stats: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: path is NUL-terminated and stats is a live local.
    if unsafe { libc::statvfs(path.as_ptr(), &mut stats) } != 0 {
        return None;
    }
    Some(stats.f_bavail as u64 * stats.f_frsize as u64)
}

#[cfg(not(unix))]
pub fn free_space(_path: &Path) -> Option<u64> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn known(bytes: u64) -> FolderTotal {
        FolderTotal {
            bytes,
            partial: false,
        }
    }

    /// Poll until `folder` has a total, or fail after a few seconds.
    fn wait_for(sizes: &mut FolderSizes, folder: &Path) -> FolderTotal {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            sizes.poll();
            if let Some(total) = sizes.total(folder) {
                return total;
            }
            assert!(
                Instant::now() < deadline,
                "{} wasn't sized",
                folder.display()
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn a_change_inside_a_folder_drops_the_totals_of_every_folder_above_it() {
        let mut sizes = FolderSizes::default();
        for folder in ["/a", "/a/b", "/a/b/c", "/a/x", "/z"] {
            sizes.totals.insert(folder.into(), known(1));
        }

        assert!(sizes.forget_with_ancestors(Path::new("/a/b/c/photo.jpg")));

        for gone in ["/a", "/a/b", "/a/b/c"] {
            assert_eq!(sizes.total(Path::new(gone)), None, "{gone}");
        }
        for kept in ["/a/x", "/z"] {
            assert_eq!(sizes.total(Path::new(kept)), Some(known(1)), "{kept}");
        }
        assert!(!sizes.forget_with_ancestors(Path::new("/elsewhere/file")));
    }

    #[test]
    fn a_walk_cancelled_before_its_result_arrives_leaves_no_total() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("photos");
        fs_folder(&folder, 5);
        let mut sizes = FolderSizes::default();
        sizes.request(&folder);
        let cancel = sizes.walks[&folder].clone();

        sizes.cancel_in(dir.path());
        // Whatever the walk got to, a count it reports now is late.
        sizes
            .sender
            .send((folder.clone(), cancel.clone(), Some(FolderCount::default())))
            .unwrap();
        std::thread::sleep(Duration::from_millis(100));

        assert!(cancel.load(Ordering::Relaxed));
        assert!(!sizes.poll());
        assert_eq!(sizes.total(&folder), None);
        // Asked again, it's walked again.
        sizes.request(&folder);
        assert_eq!(wait_for(&mut sizes, &folder), known(5));
    }

    fn fs_folder(folder: &Path, bytes: usize) {
        std::fs::create_dir_all(folder).unwrap();
        std::fs::write(folder.join("file"), "x".repeat(bytes)).unwrap();
    }

    #[test]
    fn a_total_missing_unreadable_entries_is_marked_as_at_least() {
        assert_eq!(
            total_text(FolderTotal {
                bytes: 3 * 1024 * 1024 * 1024 + 200 * 1024 * 1024,
                partial: true,
            }),
            "≥ 3.2 GB"
        );
        assert_eq!(total_text(known(2048)), "2.0 KB");
    }

    #[cfg(unix)]
    #[test]
    fn a_folder_with_an_unreadable_folder_inside_is_partial() {
        use std::os::unix::fs::PermissionsExt;

        if unsafe { libc::geteuid() } == 0 {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("library");
        fs_folder(&folder.join("locked"), 6);
        std::fs::write(folder.join("open.txt"), "abc").unwrap();
        let locked = folder.join("locked");
        let permissions = std::fs::metadata(&locked).unwrap().permissions();
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();

        let mut sizes = FolderSizes::default();
        sizes.request(&folder);
        let total = wait_for(&mut sizes, &folder);
        std::fs::set_permissions(&locked, permissions).unwrap();

        assert_eq!(
            total,
            FolderTotal {
                bytes: 3,
                partial: true
            }
        );
        assert_eq!(total_text(total), "≥ 3 B");
    }
}
