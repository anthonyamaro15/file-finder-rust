//! Copy operations with progress tracking.

use log::warn;
use rayon::prelude::*;
use std::fs;
use std::io::{self, ErrorKind};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::thread;
use walkdir::{DirEntry, WalkDir};

/// Progress message for copy operations.
#[derive(Debug, Clone)]
pub enum CopyMessage {
    Progress {
        files_processed: usize,
        total_files: usize,
        bytes_copied: u64,
        total_bytes: u64,
        current_file: String,
    },
    Completed {
        success: bool,
        message: String,
    },
    Error(String),
}

/// Recreate a link without following its target.
fn copy_symlink(src: &Path, dest: &Path) -> anyhow::Result<()> {
    let target = std::fs::read_link(src)
        .map_err(|e| anyhow::anyhow!("Failed to readlink '{}': {}", src.display(), e))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        symlink(&target, dest).map_err(|e| {
            anyhow::anyhow!(
                "Failed to create symlink '{}' -> '{}' : {}",
                dest.display(),
                target.display(),
                e
            )
        })?;
    }
    #[cfg(not(unix))]
    {
        // On non-unix, best-effort: fall back to copying the target contents
        if target.is_file() {
            fast_copy_file(&target, dest).map_err(|e| {
                anyhow::anyhow!(
                    "Failed to copy symlink target '{}' -> '{}': {}",
                    target.display(),
                    dest.display(),
                    e
                )
            })?;
        }
    }
    Ok(())
}

/// Split walked entries into copyable ones and special files (sockets, FIFOs,
/// devices), which are skipped like `cp -R` does.
fn split_special_files(entries: Vec<DirEntry>) -> (Vec<DirEntry>, Vec<DirEntry>) {
    entries.into_iter().partition(|entry| {
        let ft = entry.file_type();
        ft.is_dir() || ft.is_file() || ft.is_symlink()
    })
}

/// Remove a destination that did not exist before this failed copy.
/// `remove_dir_all` doesn't follow symlinks, so links we created inside are
/// removed without touching their targets.
fn remove_partial_copy(dest: &Path, error: anyhow::Error) -> anyhow::Error {
    let removed = match fs::symlink_metadata(dest) {
        Ok(metadata) if metadata.is_dir() => fs::remove_dir_all(dest),
        Ok(_) => fs::remove_file(dest),
        Err(e) if e.kind() == ErrorKind::NotFound => return error,
        Err(e) => Err(e),
    };
    match removed {
        Ok(()) => anyhow::anyhow!("{} (partial copy removed)", error),
        Err(e) => anyhow::anyhow!(
            "{} (failed to remove partial copy '{}': {})",
            error,
            dest.display(),
            e
        ),
    }
}

/// Synchronous copy helper for files and directories.
pub fn copy_dir_file_helper(src: &Path, new_src: &Path) -> anyhow::Result<()> {
    let metadata = fs::symlink_metadata(src)
        .map_err(|e| anyhow::anyhow!("Failed to read source '{}': {}", src.display(), e))?;
    match fs::symlink_metadata(new_src) {
        Ok(_) => {
            return Err(anyhow::anyhow!(
                "Destination already exists: {}",
                new_src.display()
            ))
        }
        Err(e) if e.kind() == ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }

    copy_to_new_destination(src, new_src, &metadata).map_err(|e| remove_partial_copy(new_src, e))
}

fn copy_to_new_destination(
    src: &Path,
    new_src: &Path,
    metadata: &fs::Metadata,
) -> anyhow::Result<()> {
    if metadata.file_type().is_symlink() {
        if let Some(parent) = new_src.parent() {
            fs::create_dir_all(parent)?;
        }
        copy_symlink(src, new_src)?;
    } else if metadata.is_file() {
        // Ensure parent directory exists
        if let Some(parent) = new_src.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| anyhow::anyhow!("Failed to create parent directory: {}", e))?;
        }

        fs::copy(src, new_src).map_err(|e| {
            anyhow::anyhow!(
                "Failed to copy file '{}' to '{}': {}",
                src.display(),
                new_src.display(),
                e
            )
        })?;
    } else if metadata.is_dir() {
        // Create the destination directory
        fs::create_dir_all(new_src)
            .map_err(|e| anyhow::anyhow!("Failed to create destination directory: {}", e))?;

        let entries = WalkDir::new(src)
            .follow_links(false)
            .follow_root_links(false)
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?;
        let (entries, special) = split_special_files(entries);
        for entry in &special {
            warn!("Skipping special file: {}", entry.path().display());
        }

        entries.par_iter().try_for_each(|entry| {
            let entry_path = entry.path();
            let relative_path = entry_path.strip_prefix(src).map_err(|e| {
                io::Error::new(ErrorKind::Other, format!("Failed to strip prefix: {}", e))
            })?;
            let dst_path = new_src.join(relative_path);

            if entry.file_type().is_dir() {
                fs::create_dir_all(&dst_path).map_err(|e| {
                    io::Error::new(
                        ErrorKind::Other,
                        format!("Failed to create directory '{}': {}", dst_path.display(), e),
                    )
                })?;
            } else {
                if let Some(parent) = dst_path.parent() {
                    fs::create_dir_all(parent).map_err(|e| {
                        io::Error::new(
                            ErrorKind::Other,
                            format!("Failed to create parent directory: {}", e),
                        )
                    })?;
                }
                if entry.file_type().is_symlink() {
                    copy_symlink(entry_path, &dst_path)
                        .map_err(|e| io::Error::other(e.to_string()))?;
                    return Ok(());
                }
                fs::copy(entry_path, &dst_path).map_err(|e| {
                    io::Error::new(
                        ErrorKind::Other,
                        format!(
                            "Failed to copy file '{}' to '{}': {}",
                            entry_path.display(),
                            dst_path.display(),
                            e
                        ),
                    )
                })?;
            }

            Ok::<(), io::Error>(())
        })?;
    } else {
        return Err(anyhow::anyhow!(
            "Source is neither a file nor a directory: {}",
            src.display()
        ));
    }

    Ok(())
}

/// Fast file copy optimized for macOS/APFS using clonefile/copyfile.
#[cfg(target_os = "macos")]
pub fn fast_copy_file(src: &Path, dst: &Path) -> io::Result<()> {
    // Allow disabling clonefile/copyfile via env for baseline comparisons
    let disable_clone = std::env::var("FF_DISABLE_CLONEFILE")
        .ok()
        .filter(|v| v == "1")
        .is_some();
    if !disable_clone {
        use std::{ffi::CString, os::unix::ffi::OsStrExt};
        unsafe {
            let src_c = CString::new(src.as_os_str().as_bytes())
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
            let dst_c = CString::new(dst.as_os_str().as_bytes())
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;

            // 1) Try APFS clone-on-write for same-volume instant copy
            if libc::clonefile(src_c.as_ptr(), dst_c.as_ptr(), 0) == 0 {
                return Ok(());
            }

            // 2) Fallback to kernel-optimized copy with metadata preservation
            // Compose flags equivalent to COPYFILE_ALL (ACL | STAT | XATTR | DATA | SECURITY)
            let flags = libc::COPYFILE_ACL
                | libc::COPYFILE_STAT
                | libc::COPYFILE_XATTR
                | libc::COPYFILE_DATA
                | libc::COPYFILE_SECURITY;
            if libc::copyfile(src_c.as_ptr(), dst_c.as_ptr(), std::ptr::null_mut(), flags) == 0 {
                return Ok(());
            }
        }
    }

    // 3) Last resort (or when disabled)
    std::fs::copy(src, dst).map(|_| ())
}

/// Fast file copy for non-macOS platforms.
#[cfg(not(target_os = "macos"))]
pub fn fast_copy_file(src: &Path, dst: &Path) -> io::Result<()> {
    std::fs::copy(src, dst).map(|_| ())
}

/// Async copy function with progress tracking.
/// Returns a channel receiver for progress updates.
pub fn copy_dir_file_with_progress(src: &Path, new_src: &Path) -> mpsc::Receiver<CopyMessage> {
    let (tx, rx) = mpsc::channel();
    let src = src.to_path_buf();
    let new_src = new_src.to_path_buf();

    thread::spawn(move || {
        let metadata = match fs::symlink_metadata(&src) {
            Ok(metadata) => metadata,
            Err(e) => {
                let _ = tx.send(CopyMessage::Error(format!(
                    "Failed to read source '{}': {}",
                    src.display(),
                    e
                )));
                return;
            }
        };
        match fs::symlink_metadata(&new_src) {
            Ok(_) => {
                let _ = tx.send(CopyMessage::Error(format!(
                    "Destination already exists: {}",
                    new_src.display()
                )));
                return;
            }
            Err(e) if e.kind() == ErrorKind::NotFound => {}
            Err(e) => {
                let _ = tx.send(CopyMessage::Error(e.to_string()));
                return;
            }
        }

        let result = if metadata.file_type().is_symlink() {
            copy_dir_file_helper(&src, &new_src).map(|()| Vec::new())
        } else if metadata.is_file() {
            // Single file copy
            if let Some(parent) = new_src.parent() {
                if let Err(e) = fs::create_dir_all(parent) {
                    let _ = tx.send(CopyMessage::Error(format!(
                        "Failed to create parent directory: {}",
                        e
                    )));
                    return;
                }
            }

            let file_size = metadata.len();

            let _ = tx.send(CopyMessage::Progress {
                files_processed: 0,
                total_files: 1,
                bytes_copied: 0,
                total_bytes: file_size,
                current_file: src
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string(),
            });

            match fast_copy_file(&src, &new_src) {
                Ok(()) => {
                    let _ = tx.send(CopyMessage::Progress {
                        files_processed: 1,
                        total_files: 1,
                        bytes_copied: file_size,
                        total_bytes: file_size,
                        current_file: src
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string(),
                    });
                    Ok(Vec::new())
                }
                Err(e) => Err(anyhow::anyhow!(
                    "Failed to copy file '{}' to '{}': {}",
                    src.display(),
                    new_src.display(),
                    e
                )),
            }
        } else if metadata.is_dir() {
            copy_dir_with_progress(&src, &new_src, &tx)
        } else {
            Err(anyhow::anyhow!(
                "Source is neither a file nor a directory: {}",
                src.display()
            ))
        };

        // Send completion message
        match result {
            Ok(skipped) => {
                let mut message = format!("Successfully copied to {}", new_src.display());
                match skipped.as_slice() {
                    [] => {}
                    [only] => {
                        message += &format!(" (skipped 1 special file: {})", only.path().display())
                    }
                    [first, ..] => {
                        message += &format!(
                            " (skipped {} special files, first: {})",
                            skipped.len(),
                            first.path().display()
                        )
                    }
                }
                let _ = tx.send(CopyMessage::Completed {
                    success: true,
                    message,
                });
            }
            Err(e) => {
                // new_src didn't exist before this copy, so anything there now is ours.
                let error = remove_partial_copy(&new_src, e);
                let _ = tx.send(CopyMessage::Error(error.to_string()));
            }
        }
    });

    rx
}

/// Directory branch of `copy_dir_file_with_progress`: parallelized with batched
/// progress updates. Returns the special files that were skipped.
fn copy_dir_with_progress(
    src: &Path,
    new_src: &Path,
    tx: &mpsc::Sender<CopyMessage>,
) -> anyhow::Result<Vec<DirEntry>> {
    fs::create_dir_all(new_src)
        .map_err(|e| anyhow::anyhow!("Failed to create destination directory: {}", e))?;

    // Single-pass partition: split entries into directories and files
    let entries = WalkDir::new(src)
        .follow_links(false)
        .follow_root_links(false)
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?;
    let (entries, special) = split_special_files(entries);
    let (dirs, files): (Vec<_>, Vec<_>) = entries.into_iter().partition(|e| e.file_type().is_dir());

    // Create all directories first (sorted to ensure parents before children)
    let mut dir_paths: Vec<_> = dirs.iter().map(|e| e.path().to_path_buf()).collect();
    dir_paths.sort();
    for dir_path in &dir_paths {
        let rel = dir_path
            .strip_prefix(src)
            .map_err(|e| anyhow::anyhow!("Failed to strip prefix: {}", e))?;
        fs::create_dir_all(new_src.join(rel)).map_err(|e| {
            anyhow::anyhow!(
                "Failed to create directory '{}': {}",
                new_src.join(rel).display(),
                e
            )
        })?;
    }

    // Calculate total bytes for progress tracking
    let total_bytes: u64 = files
        .iter()
        .filter_map(|entry| fs::symlink_metadata(entry.path()).ok())
        .map(|m| m.len())
        .sum();

    let total_files = files.len();
    let processed = AtomicUsize::new(0);
    let bytes_copied = std::sync::atomic::AtomicU64::new(0);
    const UPDATE_INTERVAL: usize = 10; // Update progress every 10 files

    // Limit concurrency: FF_COPY_THREADS overrides default (4)
    let threads = std::env::var("FF_COPY_THREADS")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .filter(|&n| n >= 1 && n <= 64)
        .unwrap_or(4);
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build()
        .unwrap();
    pool.install(|| {
        files
            .par_iter()
            .try_for_each(|entry| -> anyhow::Result<()> {
                let entry_path = entry.path();
                let rel = entry_path
                    .strip_prefix(src)
                    .map_err(|e| anyhow::anyhow!("Failed to strip prefix: {}", e))?;
                let dst_path = new_src.join(rel);

                // Note: Parent directories are guaranteed to exist since we created
                // all directories in the first pass (sorted to ensure proper order)

                // Preserve symlinks instead of skipping them
                if entry.file_type().is_symlink() {
                    copy_symlink(entry_path, &dst_path)?;
                } else {
                    fast_copy_file(entry_path, &dst_path).map_err(|e| {
                        anyhow::anyhow!(
                            "Failed to copy '{}' -> '{}': {}",
                            entry_path.display(),
                            dst_path.display(),
                            e
                        )
                    })?;
                }

                // Track bytes copied (get file size)
                let file_size = fs::symlink_metadata(entry_path)
                    .map(|m| m.len())
                    .unwrap_or(0);
                let current_bytes =
                    bytes_copied.fetch_add(file_size, Ordering::Relaxed) + file_size;

                let count = processed.fetch_add(1, Ordering::Relaxed) + 1;
                if count % UPDATE_INTERVAL == 0 || count == total_files {
                    let _ = tx.send(CopyMessage::Progress {
                        files_processed: count,
                        total_files,
                        bytes_copied: current_bytes,
                        total_bytes,
                        current_file: entry_path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string(),
                    });
                }

                Ok(())
            })
    })?;

    Ok(special)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn drain_until_complete(rx: mpsc::Receiver<CopyMessage>) -> Result<String, String> {
        // Wait up to 5 seconds for completion
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            match rx.recv_timeout(Duration::from_millis(200)) {
                Ok(CopyMessage::Completed { success, message }) => {
                    if success {
                        return Ok(message);
                    }
                    return Err(format!("Copy failed: {}", message));
                }
                Ok(CopyMessage::Error(e)) => return Err(e),
                Ok(_) => {
                    if std::time::Instant::now() > deadline {
                        return Err("Timeout waiting for copy".into());
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if std::time::Instant::now() > deadline {
                        return Err("Timeout waiting for copy".into());
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err("Channel disconnected".into())
                }
            }
        }
    }

    #[test]
    fn copies_single_file() {
        let tmp = tempfile::tempdir().unwrap();
        let src_file = tmp.path().join("a.txt");
        std::fs::write(&src_file, b"hello").unwrap();
        let dst_file = tmp.path().join("out.txt");

        let rx = copy_dir_file_with_progress(&src_file, &dst_file);
        drain_until_complete(rx).expect("copy should complete");

        let out = std::fs::read(&dst_file).unwrap();
        assert_eq!(out, b"hello");
    }

    #[test]
    fn copies_directory_with_symlink_and_file() {
        let tmp = tempfile::tempdir().unwrap();
        let src_dir = tmp.path().join("src");
        let dst_dir = tmp.path().join("dst");
        std::fs::create_dir_all(&src_dir).unwrap();
        let real = src_dir.join("real.txt");
        std::fs::write(&real, b"data").unwrap();
        let sub = src_dir.join("sub");
        std::fs::create_dir_all(&sub).unwrap();

        // Create a symlink inside sub -> ../real.txt
        let link_path = sub.join("ln.txt");
        #[cfg(unix)]
        std::os::unix::fs::symlink(Path::new("../real.txt"), &link_path).unwrap();
        #[cfg(not(unix))]
        {
            // On non-unix, simulate by creating another real file
            std::fs::write(&link_path, b"data").unwrap();
        }

        let rx = copy_dir_file_with_progress(&src_dir, &dst_dir);
        drain_until_complete(rx).expect("dir copy should complete");

        // Verify real file exists
        assert!(dst_dir.join("real.txt").is_file());

        // Verify link handling
        let copied_link = dst_dir.join("sub/ln.txt");
        #[cfg(unix)]
        {
            let meta = std::fs::symlink_metadata(&copied_link).unwrap();
            assert!(
                meta.file_type().is_symlink(),
                "expected symlink to be preserved"
            );
            let target = std::fs::read_link(&copied_link).unwrap();
            assert_eq!(target, Path::new("../real.txt"));
        }
        #[cfg(not(unix))]
        {
            // best-effort: we copied the target contents
            assert!(copied_link.is_file());
            let content = std::fs::read(&copied_link).unwrap();
            assert_eq!(content, b"data");
        }
    }

    #[cfg(unix)]
    fn check_copy_preserves_symlinks(asynchronous: bool) {
        use std::os::unix::fs::symlink;
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("project");
        let dst = tmp.path().join("copy");
        let outside = tmp.path().join("outside_photos");
        fs::create_dir(&src).unwrap();
        fs::create_dir(&outside).unwrap();
        for name in ["one", "two", "three"] {
            fs::write(outside.join(name), name).unwrap();
        }
        fs::write(src.join("real.txt"), "data").unwrap();
        for (name, target) in [
            ("link_to_file", "real.txt"),
            ("link_to_dir", "../outside_photos"),
            ("dangling_link", "missing"),
        ] {
            symlink(target, src.join(name)).unwrap();
        }
        if asynchronous {
            let rx = copy_dir_file_with_progress(&src, &dst);
            let mut final_progress = None;
            loop {
                match rx.recv_timeout(Duration::from_secs(5)).unwrap() {
                    CopyMessage::Progress {
                        files_processed,
                        bytes_copied,
                        total_bytes,
                        ..
                    } => {
                        if files_processed == 4 {
                            final_progress = Some((bytes_copied, total_bytes));
                        }
                    }
                    CopyMessage::Completed { success, message } => {
                        assert!(success, "{message}");
                        break;
                    }
                    CopyMessage::Error(message) => panic!("{message}"),
                }
            }
            // Four file bytes plus the lengths of the three link targets, not their contents.
            assert_eq!(final_progress, Some((36, 36)));
        } else {
            copy_dir_file_helper(&src, &dst).unwrap();
        }
        for name in ["link_to_file", "link_to_dir", "dangling_link"] {
            assert!(
                fs::symlink_metadata(dst.join(name))
                    .unwrap()
                    .file_type()
                    .is_symlink(),
                "{name}"
            );
            assert_eq!(
                fs::read_link(dst.join(name)).unwrap(),
                fs::read_link(src.join(name)).unwrap()
            );
        }
        assert!(fs::symlink_metadata(dst.join("real.txt"))
            .unwrap()
            .is_file());
        assert_eq!(fs::read_to_string(dst.join("real.txt")).unwrap(), "data");
        assert_eq!(fs::read_dir(&outside).unwrap().count(), 3);
        for name in ["one", "two", "three"] {
            assert_eq!(fs::read_to_string(outside.join(name)).unwrap(), name);
        }
    }

    #[cfg(unix)]
    #[test]
    fn sync_copy_preserves_all_symlinks() {
        check_copy_preserves_symlinks(false);
    }

    #[cfg(unix)]
    #[test]
    fn async_copy_preserves_all_symlinks() {
        check_copy_preserves_symlinks(true);
    }

    #[cfg(unix)]
    #[test]
    fn both_copy_paths_preserve_root_symlinks() {
        use std::os::unix::fs::symlink;
        let tmp = tempfile::tempdir().unwrap();
        let outside = tmp.path().join("outside");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("keep"), "keep").unwrap();
        for asynchronous in [false, true] {
            for target in ["outside", "missing"] {
                let src = tmp.path().join("source");
                let dst = tmp.path().join("copy");
                symlink(target, &src).unwrap();
                if asynchronous {
                    drain_until_complete(copy_dir_file_with_progress(&src, &dst)).unwrap();
                } else {
                    copy_dir_file_helper(&src, &dst).unwrap();
                }
                assert!(fs::symlink_metadata(&dst).unwrap().file_type().is_symlink());
                assert_eq!(fs::read_link(&dst).unwrap(), Path::new(target));
                fs::remove_file(&src).unwrap();
                fs::remove_file(&dst).unwrap();
            }
        }
        assert_eq!(fs::read_to_string(outside.join("keep")).unwrap(), "keep");
    }

    #[cfg(unix)]
    #[test]
    fn both_copy_paths_reject_dangling_destination() {
        use std::os::unix::fs::symlink;
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("source");
        let dst = tmp.path().join("copy");
        fs::write(&src, "data").unwrap();
        symlink("missing", &dst).unwrap();
        assert!(copy_dir_file_helper(&src, &dst).is_err());
        assert!(drain_until_complete(copy_dir_file_with_progress(&src, &dst)).is_err());
        assert_eq!(fs::read_link(&dst).unwrap(), Path::new("missing"));
        assert!(!tmp.path().join("missing").exists());
    }

    #[cfg(unix)]
    #[test]
    fn both_copy_paths_skip_special_files() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("repo");
        fs::create_dir_all(src.join(".git")).unwrap();
        fs::write(src.join("main.rs"), "fn main() {}").unwrap();
        let socket = Path::new(".git/fsmonitor--daemon.ipc");
        let _listener = std::os::unix::net::UnixListener::bind(src.join(socket)).unwrap();

        let sync_dst = tmp.path().join("sync");
        copy_dir_file_helper(&src, &sync_dst).unwrap();
        let async_dst = tmp.path().join("async");
        let message = drain_until_complete(copy_dir_file_with_progress(&src, &async_dst)).unwrap();
        assert!(
            message.contains("skipped 1 special file: ")
                && message.contains("fsmonitor--daemon.ipc"),
            "{message}"
        );

        for dst in [&sync_dst, &async_dst] {
            assert_eq!(
                fs::read_to_string(dst.join("main.rs")).unwrap(),
                "fn main() {}"
            );
            assert!(fs::symlink_metadata(dst.join(socket)).is_err());
        }
    }

    #[cfg(unix)]
    #[test]
    fn both_copy_paths_remove_partial_copy_on_failure() {
        use std::os::unix::fs::PermissionsExt;
        if unsafe { libc::geteuid() } == 0 {
            return; // root can read the locked directory
        }
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("source");
        let locked = src.join("locked");
        fs::create_dir_all(&locked).unwrap();
        fs::write(src.join("keep.txt"), "keep").unwrap();
        fs::write(locked.join("inner.txt"), "inner").unwrap();
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();

        let dst = tmp.path().join("copy");
        let sync_error = copy_dir_file_helper(&src, &dst).unwrap_err().to_string();
        let sync_dst = fs::symlink_metadata(&dst).map_err(|e| e.kind());
        let async_error =
            drain_until_complete(copy_dir_file_with_progress(&src, &dst)).unwrap_err();
        let async_dst = fs::symlink_metadata(&dst).map_err(|e| e.kind());
        // Restore before asserting so the tempdir can always be removed.
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();

        for (error, dst) in [(sync_error, sync_dst), (async_error, async_dst)] {
            assert!(error.contains("partial copy removed"), "{error}");
            assert_eq!(dst.unwrap_err(), ErrorKind::NotFound);
        }
        assert_eq!(fs::read_to_string(src.join("keep.txt")).unwrap(), "keep");
        assert_eq!(
            fs::read_to_string(locked.join("inner.txt")).unwrap(),
            "inner"
        );
    }
}
