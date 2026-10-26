//! Get Info: an item's details, and its folder size counted on a background thread.

use std::{
    fs, io,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    time::{Duration, Instant, UNIX_EPOCH},
};

use walkdir::WalkDir;

use crate::utils::format::{format_local_time, format_thousands, replace_home_with_tilde};
use crate::utils::format_file_size;

/// How often a running folder count reports its progress.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

/// Running or final totals of a folder count.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FolderCount {
    /// Logical size of the regular files.
    pub bytes: u64,
    /// Everything that isn't a folder, links included.
    pub files: u64,
    pub folders: u64,
    /// Folders that couldn't be listed and entries that couldn't be read.
    pub unreadable: u64,
    /// Set on the last count, once the whole folder was walked.
    pub finished: bool,
}

/// The open Get Info popup. Dropping it stops its folder count.
#[derive(Debug)]
pub struct ItemInfo {
    /// Label and value of each row, in display order.
    pub rows: Vec<(&'static str, String)>,
    /// Updates from the folder count until it finishes.
    pub folder_count: Option<mpsc::Receiver<FolderCount>>,
    cancel: Arc<AtomicBool>,
}

impl Drop for ItemInfo {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl ItemInfo {
    /// Read `path` without following a final symlink, and start counting a folder.
    pub fn read(path: &Path) -> io::Result<Self> {
        let metadata = fs::symlink_metadata(path)?;
        let file_type = metadata.file_type();
        let mut rows = vec![(
            "Name",
            path.file_name()
                .unwrap_or(path.as_os_str())
                .to_string_lossy()
                .into_owned(),
        )];
        if file_type.is_symlink() {
            let kind = match fs::metadata(path) {
                Ok(target) if target.is_dir() => "Symlink to folder",
                Ok(_) => "Symlink to file",
                Err(_) => "Symlink (target missing)",
            };
            rows.push(("Kind", kind.into()));
            if let Ok(target) = fs::read_link(path) {
                rows.push(("Points to", target.to_string_lossy().into_owned()));
            }
        } else {
            let kind = if file_type.is_dir() { "Folder" } else { "File" };
            rows.push(("Kind", kind.into()));
        }
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            rows.push((
                "Location",
                replace_home_with_tilde(&parent.to_string_lossy()),
            ));
        }
        if file_type.is_dir() {
            rows.push(("Size", "Calculating…".into()));
            rows.push(("Contents", "Calculating…".into()));
        } else {
            rows.push(("Size", size_text(metadata.len())));
        }
        // Some file systems have no creation time, or report the epoch for it.
        if let Some(created) = metadata
            .created()
            .ok()
            .filter(|created| *created != UNIX_EPOCH)
            .and_then(format_local_time)
        {
            rows.push(("Created", created));
        }
        if let Some(modified) = metadata.modified().ok().and_then(format_local_time) {
            rows.push(("Modified", modified));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            rows.push(("Permissions", permissions(metadata.mode())));
            rows.push(("Owner", owner(metadata.uid(), metadata.gid())));
        }

        let cancel = Arc::new(AtomicBool::new(false));
        let folder_count = file_type.is_dir().then(|| {
            let (tx, rx) = mpsc::channel();
            let root = path.to_path_buf();
            let worker_cancel = cancel.clone();
            std::thread::spawn(move || {
                let total = count_folder(&root, &worker_cancel, |count| {
                    let _ = tx.send(count.clone());
                });
                if let Some(total) = total {
                    let _ = tx.send(total);
                }
            });
            rx
        });
        Ok(Self {
            rows,
            folder_count,
            cancel,
        })
    }

    /// Show a folder count in the Size and Contents rows.
    pub fn show_folder_count(&mut self, count: &FolderCount) {
        if !count.finished {
            self.set_row(
                "Size",
                format!(
                    "Calculating… {} so far ({})",
                    format_file_size(count.bytes),
                    plural(count.files + count.folders, "item")
                ),
            );
            return;
        }
        let mut contents = format!(
            "{}, {}",
            plural(count.files, "file"),
            plural(count.folders, "folder")
        );
        if count.unreadable > 0 {
            contents.push_str(&format!(
                "; {} couldn't be read",
                plural(count.unreadable, "item")
            ));
        }
        self.set_row("Size", size_text(count.bytes));
        self.set_row("Contents", contents);
    }

    pub fn set_row(&mut self, label: &str, value: String) {
        if let Some(row) = self.rows.iter_mut().find(|(row, _)| *row == label) {
            row.1 = value;
        }
    }
}

/// Total the entries beneath `root` without following links, reporting the running
/// count every `PROGRESS_INTERVAL`. Returns `None` once `cancel` is set.
pub fn count_folder(
    root: &Path,
    cancel: &AtomicBool,
    mut progress: impl FnMut(&FolderCount),
) -> Option<FolderCount> {
    let mut count = FolderCount::default();
    let mut reported = Instant::now();
    // Not even the root is followed, in case it became a link after it was read. Disks
    // mounted inside aren't counted.
    // ponytail: macOS firmlinks share the root's device, so counting `/` counts
    // /System/Volumes/Data twice; skipping firmlinked paths would fix it.
    let walk = WalkDir::new(root)
        .follow_root_links(false)
        .same_file_system(true)
        .min_depth(1);
    for entry in walk {
        if cancel.load(Ordering::Relaxed) {
            return None;
        }
        match entry {
            Ok(entry) if entry.file_type().is_dir() => count.folders += 1,
            Ok(entry) => {
                count.files += 1;
                if entry.file_type().is_file() {
                    match entry.metadata() {
                        Ok(metadata) => count.bytes += metadata.len(),
                        Err(_) => count.unreadable += 1,
                    }
                }
            }
            Err(_) => count.unreadable += 1,
        }
        if reported.elapsed() >= PROGRESS_INTERVAL {
            progress(&count);
            reported = Instant::now();
        }
    }
    count.finished = true;
    Some(count)
}

/// "1.2 MB (1,234,567 bytes)".
fn size_text(bytes: u64) -> String {
    format!(
        "{} ({} bytes)",
        format_file_size(bytes),
        format_thousands(bytes)
    )
}

/// "1 file", "1,234 files".
pub(crate) fn plural(n: u64, noun: &str) -> String {
    let s = if n == 1 { "" } else { "s" };
    format!("{} {noun}{s}", format_thousands(n))
}

/// "rwxr-xr-x (755)".
#[cfg(unix)]
fn permissions(mode: u32) -> String {
    format!(
        "{} ({:03o})",
        crate::utils::format::permission_bits(mode),
        mode & 0o777
    )
}

/// "user:group", with the numeric id for a name the system doesn't know.
#[cfg(unix)]
fn owner(uid: u32, gid: u32) -> String {
    let user = user_name(uid).unwrap_or_else(|| uid.to_string());
    let group = group_name(gid).unwrap_or_else(|| gid.to_string());
    format!("{user}:{group}")
}

/// Largest buffer offered to the `get*_r` lookups, which ask for more with ERANGE.
#[cfg(unix)]
const MAX_LOOKUP_BUFFER: usize = 1 << 20;

#[cfg(unix)]
fn user_name(uid: libc::uid_t) -> Option<String> {
    let mut buf = vec![0; 1024];
    loop {
        // SAFETY: passwd is a plain C struct, for which all zeros is valid.
        let mut entry: libc::passwd = unsafe { std::mem::zeroed() };
        let mut found = std::ptr::null_mut();
        // SAFETY: every pointer refers to a live local, and buf.len() is buf's size.
        let code =
            unsafe { libc::getpwuid_r(uid, &mut entry, buf.as_mut_ptr(), buf.len(), &mut found) };
        if code == libc::ERANGE && buf.len() < MAX_LOOKUP_BUFFER {
            buf.resize(buf.len() * 4, 0);
            continue;
        }
        if code != 0 || found.is_null() || entry.pw_name.is_null() {
            return None;
        }
        // SAFETY: on success pw_name is a NUL-terminated string inside buf.
        let name = unsafe { std::ffi::CStr::from_ptr(entry.pw_name) };
        return Some(name.to_string_lossy().into_owned());
    }
}

#[cfg(unix)]
fn group_name(gid: libc::gid_t) -> Option<String> {
    let mut buf = vec![0; 1024];
    loop {
        // SAFETY: group is a plain C struct, for which all zeros is valid.
        let mut entry: libc::group = unsafe { std::mem::zeroed() };
        let mut found = std::ptr::null_mut();
        // SAFETY: every pointer refers to a live local, and buf.len() is buf's size.
        let code =
            unsafe { libc::getgrgid_r(gid, &mut entry, buf.as_mut_ptr(), buf.len(), &mut found) };
        if code == libc::ERANGE && buf.len() < MAX_LOOKUP_BUFFER {
            buf.resize(buf.len() * 4, 0);
            continue;
        }
        if code != 0 || found.is_null() || entry.gr_name.is_null() {
            return None;
        }
        // SAFETY: on success gr_name is a NUL-terminated string inside buf.
        let name = unsafe { std::ffi::CStr::from_ptr(entry.gr_name) };
        return Some(name.to_string_lossy().into_owned());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn count(root: &Path) -> FolderCount {
        count_folder(root, &AtomicBool::new(false), |_| {}).unwrap()
    }

    #[cfg(unix)]
    #[test]
    fn permissions_read_like_ls_with_the_octal_mode() {
        for (mode, expected) in [
            (0o755, "rwxr-xr-x (755)"),
            (0o644, "rw-r--r-- (644)"),
            (0o600, "rw------- (600)"),
            (0o777, "rwxrwxrwx (777)"),
            // File type bits from st_mode never leak into the string.
            (0o100_644, "rw-r--r-- (644)"),
        ] {
            assert_eq!(permissions(mode), expected);
        }
    }

    #[test]
    fn folder_count_totals_nested_files_and_counts_empty_folders() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("a/b")).unwrap();
        fs::create_dir(root.join("empty")).unwrap();
        fs::write(root.join("top.txt"), "abc").unwrap();
        fs::write(root.join("a/b/deep.txt"), "hello").unwrap();

        assert_eq!(
            count(root),
            FolderCount {
                bytes: 8,
                files: 2,
                folders: 3,
                unreadable: 0,
                finished: true,
            }
        );
    }

    #[cfg(unix)]
    #[test]
    fn folder_count_counts_links_as_items_without_their_targets() {
        let outside = tempfile::tempdir().unwrap();
        let big = outside.path().join("big.bin");
        fs::File::create(&big)
            .unwrap()
            .set_len(64 * 1024 * 1024)
            .unwrap();
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("small.txt"), "hello").unwrap();
        std::os::unix::fs::symlink(&big, dir.path().join("file-link")).unwrap();
        std::os::unix::fs::symlink(outside.path(), dir.path().join("dir-link")).unwrap();

        let total = count(dir.path());
        assert_eq!((total.bytes, total.files, total.folders), (5, 3, 0));
    }

    #[test]
    fn cancelled_folder_count_stops_and_says_so() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..10 {
            fs::write(dir.path().join(format!("{i}.txt")), "x").unwrap();
        }
        let mut reports = 0;
        let result = count_folder(dir.path(), &AtomicBool::new(true), |_| reports += 1);
        assert_eq!(result, None);
        assert_eq!(reports, 0);
    }

    #[cfg(unix)]
    #[test]
    fn folder_count_skips_unreadable_folders_and_reports_them() {
        use std::os::unix::fs::PermissionsExt;

        if unsafe { libc::geteuid() } == 0 {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let locked = dir.path().join("locked");
        fs::create_dir(&locked).unwrap();
        fs::write(locked.join("hidden.txt"), "secret").unwrap();
        fs::write(dir.path().join("open.txt"), "abc").unwrap();
        let permissions = fs::metadata(&locked).unwrap().permissions();
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();

        let total = count(dir.path());
        fs::set_permissions(&locked, permissions).unwrap();

        assert_eq!(
            (total.bytes, total.files, total.folders, total.unreadable),
            (3, 1, 1, 1)
        );
        let mut info = ItemInfo::read(dir.path()).unwrap();
        info.show_folder_count(&total);
        let contents = info.rows.iter().find(|(label, _)| *label == "Contents");
        assert_eq!(
            contents.map(|(_, value)| value.as_str()),
            Some("1 file, 1 folder; 1 item couldn't be read")
        );
    }

    #[test]
    fn closing_the_popup_stops_its_folder_count() {
        let dir = tempfile::tempdir().unwrap();
        let info = ItemInfo::read(dir.path()).unwrap();
        assert!(info.folder_count.is_some());
        let cancel = info.cancel.clone();
        drop(info);
        assert!(cancel.load(Ordering::Relaxed));
    }

    #[cfg(unix)]
    #[test]
    fn symlink_info_describes_the_link_itself() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("target")).unwrap();
        let link = dir.path().join("link");
        std::os::unix::fs::symlink("target", &link).unwrap();

        let info = ItemInfo::read(&link).unwrap();
        let row = |name: &str| {
            info.rows
                .iter()
                .find(|(label, _)| *label == name)
                .map(|(_, value)| value.as_str())
        };
        assert_eq!(row("Kind"), Some("Symlink to folder"));
        assert_eq!(row("Points to"), Some("target"));
        assert_eq!(row("Size"), Some("6 B (6 bytes)"));
        assert_eq!(row("Contents"), None);
        assert!(info.folder_count.is_none());
    }
}
