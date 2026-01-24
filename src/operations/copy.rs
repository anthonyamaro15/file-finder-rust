//! Copy operations with progress tracking.

use log::warn;
use rayon::prelude::*;
use std::fs;
use std::io::{self, ErrorKind};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::thread;
use walkdir::WalkDir;

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

/// Synchronous copy helper for files and directories.
pub fn copy_dir_file_helper(src: &Path, new_src: &Path) -> anyhow::Result<()> {
    // Check if source exists
    if !src.exists() {
        return Err(anyhow::anyhow!(
            "Source path does not exist: {}",
            src.display()
        ));
    }

    // Check if destination already exists
    if new_src.exists() {
        return Err(anyhow::anyhow!(
            "Destination already exists: {}",
            new_src.display()
        ));
    }

    if src.is_file() {
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
    } else if src.is_dir() {
        // Create the destination directory
        fs::create_dir_all(new_src)
            .map_err(|e| anyhow::anyhow!("Failed to create destination directory: {}", e))?;

        let entries: Vec<_> = WalkDir::new(src)
            .into_iter()
            .filter_map(|entry| match entry {
                Ok(e) => Some(e),
                Err(err) => {
                    warn!("Skipping entry due to error: {}", err);
                    None
                }
            })
            .collect();

        entries.par_iter().try_for_each(|entry| {
            let entry_path = entry.path();
            let relative_path = entry_path.strip_prefix(src).map_err(|e| {
                io::Error::new(ErrorKind::Other, format!("Failed to strip prefix: {}", e))
            })?;
            let dst_path = new_src.join(relative_path);

            if entry_path.is_dir() {
                fs::create_dir_all(&dst_path).map_err(|e| {
                    io::Error::new(
                        ErrorKind::Other,
                        format!("Failed to create directory '{}': {}", dst_path.display(), e),
                    )
                })?;
            } else if entry_path.is_file() {
                if let Some(parent) = dst_path.parent() {
                    fs::create_dir_all(parent).map_err(|e| {
                        io::Error::new(
                            ErrorKind::Other,
                            format!("Failed to create parent directory: {}", e),
                        )
                    })?;
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
            } else {
                // Skip special files (symlinks, devices, etc.)
                warn!("Skipping unsupported file type: {}", entry_path.display());
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
        // Check if source exists
        if !src.exists() {
            let _ = tx.send(CopyMessage::Error(format!(
                "Source path does not exist: {}",
                src.display()
            )));
            return;
        }

        // Check if destination already exists
        if new_src.exists() {
            let _ = tx.send(CopyMessage::Error(format!(
                "Destination already exists: {}",
                new_src.display()
            )));
            return;
        }

        let result = if src.is_file() {
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

            let file_size = src.metadata().map(|m| m.len()).unwrap_or(0);

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
                    Ok(())
                }
                Err(e) => Err(anyhow::anyhow!(
                    "Failed to copy file '{}' to '{}': {}",
                    src.display(),
                    new_src.display(),
                    e
                )),
            }
        } else if src.is_dir() {
            // Directory copy - parallelized with batched progress updates
            if let Err(e) = fs::create_dir_all(&new_src) {
                let _ = tx.send(CopyMessage::Error(format!(
                    "Failed to create destination directory: {}",
                    e
                )));
                return;
            }

            // Single-pass partition: split entries into directories and files
            let (dirs, files): (Vec<_>, Vec<_>) = WalkDir::new(&src)
                .into_iter()
                .filter_map(|entry| match entry {
                    Ok(e) => Some(e),
                    Err(err) => {
                        warn!("Skipping entry due to error: {}", err);
                        None
                    }
                })
                .partition(|e| e.path().is_dir());

            // Create all directories first (sorted to ensure parents before children)
            let mut dir_paths: Vec<_> = dirs.iter().map(|e| e.path().to_path_buf()).collect();
            dir_paths.sort();
            for dir_path in &dir_paths {
                let rel = match dir_path.strip_prefix(&src) {
                    Ok(p) => p,
                    Err(e) => {
                        let _ =
                            tx.send(CopyMessage::Error(format!("Failed to strip prefix: {}", e)));
                        return;
                    }
                };
                if let Err(e) = fs::create_dir_all(new_src.join(rel)) {
                    let _ = tx.send(CopyMessage::Error(format!(
                        "Failed to create directory '{}': {}",
                        new_src.join(rel).display(),
                        e
                    )));
                    return;
                }
            }

            // Prepare items to copy: regular files and symlinks, and calculate total bytes
            let files: Vec<_> = files
                .iter()
                .filter(|e| {
                    let ft = e.file_type();
                    ft.is_file() || ft.is_symlink()
                })
                .map(|e| e.path().to_path_buf())
                .collect();

            // Calculate total bytes for progress tracking
            let total_bytes: u64 = files
                .iter()
                .filter_map(|p| p.metadata().ok())
                .map(|m| m.len())
                .sum();

            let total_files = files.len();
            let processed = AtomicUsize::new(0);
            let bytes_copied = std::sync::atomic::AtomicU64::new(0);
            let progress_tx = tx.clone();
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
            let par_result: Result<(), anyhow::Error> = pool.install(|| {
                files
                    .par_iter()
                    .try_for_each(|entry_path| -> anyhow::Result<()> {
                        let rel = entry_path
                            .strip_prefix(&src)
                            .map_err(|e| anyhow::anyhow!("Failed to strip prefix: {}", e))?;
                        let dst_path = new_src.join(rel);

                        // Note: Parent directories are guaranteed to exist since we created
                        // all directories in the first pass (sorted to ensure proper order)

                        // Preserve symlinks instead of skipping them
                        let ft = std::fs::symlink_metadata(entry_path)
                            .map_err(|e| {
                                anyhow::anyhow!("Failed to lstat '{}': {}", entry_path.display(), e)
                            })?
                            .file_type();

                        if ft.is_symlink() {
                            let target = std::fs::read_link(entry_path).map_err(|e| {
                                anyhow::anyhow!(
                                    "Failed to readlink '{}': {}",
                                    entry_path.display(),
                                    e
                                )
                            })?;
                            #[cfg(unix)]
                            {
                                use std::os::unix::fs::symlink;
                                // If destination exists, remove it first to avoid EEXIST
                                if dst_path.exists() {
                                    let _ = fs::remove_file(&dst_path);
                                }
                                symlink(&target, &dst_path).map_err(|e| {
                                    anyhow::anyhow!(
                                        "Failed to create symlink '{}' -> '{}' : {}",
                                        dst_path.display(),
                                        target.display(),
                                        e
                                    )
                                })?;
                            }
                            #[cfg(not(unix))]
                            {
                                // On non-unix, best-effort: fall back to copying the target contents
                                if target.is_file() {
                                    fast_copy_file(&target, &dst_path).map_err(|e| {
                                        anyhow::anyhow!(
                                            "Failed to copy symlink target '{}' -> '{}': {}",
                                            target.display(),
                                            dst_path.display(),
                                            e
                                        )
                                    })?;
                                }
                            }
                        } else if ft.is_file() {
                            fast_copy_file(entry_path, &dst_path).map_err(|e| {
                                anyhow::anyhow!(
                                    "Failed to copy '{}' -> '{}': {}",
                                    entry_path.display(),
                                    dst_path.display(),
                                    e
                                )
                            })?;
                        } else {
                            // Skip special files (sockets, devices, etc.)
                            warn!("Skipping unsupported file type: {}", entry_path.display());
                        }

                        // Track bytes copied (get file size)
                        let file_size = entry_path.metadata().map(|m| m.len()).unwrap_or(0);
                        let current_bytes = bytes_copied.fetch_add(file_size, Ordering::Relaxed) + file_size;

                        let count = processed.fetch_add(1, Ordering::Relaxed) + 1;
                        if count % UPDATE_INTERVAL == 0 || count == total_files {
                            let _ = progress_tx.send(CopyMessage::Progress {
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
            });

            par_result
        } else {
            Err(anyhow::anyhow!(
                "Source is neither a file nor a directory: {}",
                src.display()
            ))
        };

        // Send completion message
        match result {
            Ok(_) => {
                let _ = tx.send(CopyMessage::Completed {
                    success: true,
                    message: format!("Successfully copied to {}", new_src.display()),
                });
            }
            Err(e) => {
                let _ = tx.send(CopyMessage::Error(e.to_string()));
            }
        }
    });

    rx
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn drain_until_complete(rx: mpsc::Receiver<CopyMessage>) -> Result<(), String> {
        // Wait up to 5 seconds for completion
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            match rx.recv_timeout(Duration::from_millis(200)) {
                Ok(CopyMessage::Completed { success, message }) => {
                    if success {
                        return Ok(());
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
}
