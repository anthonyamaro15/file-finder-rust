//! File operations: create, delete, and rename files and directories.

use std::fs::{self, File};
use std::io;
use std::path::Path;
use std::sync::mpsc;
use std::thread;

use log::warn;
use walkdir::WalkDir;

use crate::app::App;
use crate::errors::{validation, FileOperationError, FileOperationResult};
use crate::operations::copy::copy_dir_file_helper;

/// Progress message for delete operations.
#[derive(Debug, Clone)]
pub enum DeleteMessage {
    /// Counting files before deletion
    Counting { files_counted: usize },
    /// Progress during deletion
    Progress {
        files_deleted: usize,
        total_files: usize,
        current_file: String,
    },
    /// Deletion completed
    Completed { success: bool, message: String },
    /// Error occurred
    Error(String),
}

/// Delete a file at the specified path.
pub fn delete_file(file_path: &str) -> FileOperationResult<()> {
    let path = Path::new(file_path);

    // A link can be deleted even if its target is missing or read-only.
    if !path.is_symlink() {
        validation::validate_path_exists(path)?;
        validation::validate_permissions(path, "delete")?;
    }

    // Attempt to delete the file
    fs::remove_file(path).map_err(|e| match e.kind() {
        io::ErrorKind::PermissionDenied => {
            FileOperationError::permission_denied(path, "Cannot delete file - check permissions")
        }
        io::ErrorKind::NotFound => FileOperationError::file_not_found(path),
        _ => FileOperationError::from(e),
    })?;

    Ok(())
}

/// Delete a directory and all its contents at the specified path.
pub fn delete_dir(dir_path: &str) -> FileOperationResult<()> {
    let path = Path::new(dir_path);

    if path.is_symlink() {
        return delete_file(dir_path);
    }

    // Validate directory exists
    validation::validate_path_exists(path)?;

    // Validate permissions
    validation::validate_permissions(path, "delete")?;

    // Attempt to delete the directory
    fs::remove_dir_all(path).map_err(|e| match e.kind() {
        io::ErrorKind::PermissionDenied => FileOperationError::permission_denied(
            path,
            "Cannot delete directory - check permissions",
        ),
        io::ErrorKind::NotFound => FileOperationError::file_not_found(path),
        _ => FileOperationError::from(e),
    })?;

    Ok(())
}

/// Delete a file or directory based on its type.
pub fn handle_delete_based_on_type(file_path: &str) -> FileOperationResult<()> {
    let path = Path::new(file_path);

    let metadata = fs::symlink_metadata(path).map_err(|e| match e.kind() {
        io::ErrorKind::PermissionDenied => {
            FileOperationError::permission_denied(path, "Cannot read file information")
        }
        io::ErrorKind::NotFound => FileOperationError::file_not_found(path),
        _ => FileOperationError::from(e),
    })?;

    if metadata.is_dir() {
        delete_dir(file_path)
    } else {
        delete_file(file_path)
    }
}

/// Delete a file or directory asynchronously with progress updates.
/// Returns a receiver for progress messages.
pub fn delete_with_progress(file_path: &str) -> mpsc::Receiver<DeleteMessage> {
    let (tx, rx) = mpsc::channel();
    let path = file_path.to_string();

    thread::spawn(move || {
        let src = Path::new(&path);

        let metadata = match fs::symlink_metadata(src) {
            Ok(metadata) => metadata,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                let _ = tx.send(DeleteMessage::Completed {
                    success: true,
                    message: "File already deleted".to_string(),
                });
                return;
            }
            Err(e) => {
                let _ = tx.send(DeleteMessage::Error(format!(
                    "Failed to read '{}': {}",
                    src.display(),
                    e
                )));
                return;
            }
        };

        if metadata.file_type().is_symlink() || metadata.is_file() {
            // Single file or symlink delete - quick operation
            let file_name = src
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();

            let _ = tx.send(DeleteMessage::Progress {
                files_deleted: 0,
                total_files: 1,
                current_file: file_name.clone(),
            });

            match fs::remove_file(src) {
                Ok(()) => {
                    let _ = tx.send(DeleteMessage::Completed {
                        success: true,
                        message: format!("Deleted: {}", file_name),
                    });
                }
                Err(e) => {
                    let _ = tx.send(DeleteMessage::Error(format!(
                        "Failed to delete '{}': {}",
                        file_name, e
                    )));
                }
            }
        } else if metadata.is_dir() {
            // Directory delete - collect files first, then delete in reverse order
            // (files first, then directories from deepest to shallowest)

            // Count files first
            let _ = tx.send(DeleteMessage::Counting { files_counted: 0 });

            let mut failed = 0;
            let mut first_error = None;
            let mut entries: Vec<_> = WalkDir::new(src)
                .follow_links(false)
                .follow_root_links(false)
                .into_iter()
                .filter_map(|entry| match entry {
                    Ok(entry) => Some(entry),
                    Err(e) => {
                        failed += 1;
                        first_error.get_or_insert_with(|| e.to_string());
                        None
                    }
                })
                .collect();

            let file_count = entries.len() + failed;
            let _ = tx.send(DeleteMessage::Counting {
                files_counted: file_count,
            });

            // Sort by depth (deepest first) so we delete children before parents
            entries.sort_by_key(|entry| std::cmp::Reverse(entry.depth()));

            let mut deleted = 0;
            const UPDATE_INTERVAL: usize = 100;

            for (index, entry) in entries.iter().enumerate() {
                let entry_path = entry.path();
                let file_name = entry_path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();

                let result = if entry.file_type().is_dir() {
                    fs::remove_dir(entry_path)
                } else {
                    fs::remove_file(entry_path)
                };

                match result {
                    Ok(()) => deleted += 1,
                    Err(e) => {
                        warn!("Failed to delete '{}': {}", entry_path.display(), e);
                        failed += 1;
                        first_error
                            .get_or_insert_with(|| format!("{}: {}", entry_path.display(), e));
                    }
                }

                if (index + 1) % UPDATE_INTERVAL == 0 || index + 1 == entries.len() {
                    let _ = tx.send(DeleteMessage::Progress {
                        files_deleted: deleted,
                        total_files: file_count,
                        current_file: file_name,
                    });
                }
            }

            let _ = tx.send(DeleteMessage::Completed {
                success: failed == 0,
                message: match first_error {
                    Some(error) => format!(
                        "Deleted {} of {} items; {} failed (first error: {})",
                        deleted, file_count, failed, error
                    ),
                    None => format!("Deleted {} items", deleted),
                },
            });
        }
    });

    rx
}

/// Create a new directory.
pub fn create_new_dir(current_file_path: String, new_item: String) -> FileOperationResult<()> {
    // Validate the directory name
    validation::validate_filename(&new_item)?;

    let append_path = format!("{}/{}", current_file_path, new_item);
    let path = Path::new(&append_path);

    // Validate that the directory doesn't already exist
    validation::validate_path_not_exists(path)?;

    // Validate parent directory permissions
    validation::validate_permissions(path, "create")?;

    // Create the directory
    fs::create_dir(path).map_err(|e| match e.kind() {
        io::ErrorKind::PermissionDenied => FileOperationError::permission_denied(
            path,
            "Cannot create directory - check permissions",
        ),
        io::ErrorKind::AlreadyExists => FileOperationError::already_exists(path),
        io::ErrorKind::NotFound => {
            FileOperationError::file_not_found(Path::new(&current_file_path))
        }
        _ => FileOperationError::from(e),
    })?;

    Ok(())
}

/// Create a new file.
pub fn create_new_file(current_file_path: String, file_name: String) -> FileOperationResult<()> {
    // Validate the filename
    validation::validate_filename(&file_name)?;

    let append_path = format!("{}/{}", current_file_path, file_name);
    let path = Path::new(&append_path);

    // Validate that the file doesn't already exist
    validation::validate_path_not_exists(path)?;

    // Validate parent directory permissions
    validation::validate_permissions(path, "create")?;

    // Create the file
    File::create_new(path).map_err(|e| match e.kind() {
        io::ErrorKind::PermissionDenied => {
            FileOperationError::permission_denied(path, "Cannot create file - check permissions")
        }
        io::ErrorKind::AlreadyExists => FileOperationError::already_exists(path),
        io::ErrorKind::NotFound => {
            FileOperationError::file_not_found(Path::new(&current_file_path))
        }
        _ => FileOperationError::from(e),
    })?;

    Ok(())
}

/// Create a file or directory based on whether the name contains a dot.
pub fn create_item_based_on_type(
    current_file_path: String,
    new_item: String,
) -> FileOperationResult<()> {
    if new_item.contains('.') {
        create_new_file(current_file_path, new_item)
    } else {
        create_new_dir(current_file_path, new_item)
    }
}

/// Move a file or directory to a new location.
/// Falls back to copy+delete for cross-filesystem moves.
pub fn move_file_or_dir(src_path: &str, dest_dir: &str) -> FileOperationResult<()> {
    let src = Path::new(src_path);
    let src_name = src
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| FileOperationError::file_not_found(src))?;

    let dest = Path::new(dest_dir).join(src_name);

    // Validate the links themselves, including dangling sources and destinations.
    fs::symlink_metadata(src).map_err(FileOperationError::from)?;
    match fs::symlink_metadata(&dest) {
        Ok(_) => return Err(FileOperationError::already_exists(&dest)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(FileOperationError::from(e)),
    }

    // Try fs::rename first (fast path for same filesystem)
    match fs::rename(src, &dest) {
        Ok(()) => Ok(()),
        Err(e) => {
            // If cross-device link error, fall back to copy+delete
            if e.raw_os_error() == Some(18) {
                // EXDEV (cross-device link) on Unix
                copy_then_delete(src, &dest)
            } else {
                Err(match e.kind() {
                    io::ErrorKind::PermissionDenied => FileOperationError::permission_denied(
                        src,
                        "Cannot move - check permissions",
                    ),
                    io::ErrorKind::NotFound => FileOperationError::file_not_found(src),
                    io::ErrorKind::AlreadyExists => FileOperationError::already_exists(&dest),
                    _ => FileOperationError::from(e),
                })
            }
        }
    }
}

/// Cross-device fallback: remove the source only after a successful copy.
fn copy_then_delete(src: &Path, dest: &Path) -> FileOperationResult<()> {
    copy_dir_file_helper(src, dest).map_err(|e| FileOperationError::Io {
        message: e.to_string(),
    })?;
    handle_delete_based_on_type(
        src.to_str()
            .ok_or_else(|| FileOperationError::InvalidPath {
                path: src.display().to_string(),
            })?,
    )
}

/// Rename a file or directory using app state for paths.
pub fn handle_rename(app: &App) -> FileOperationResult<()> {
    // Validate the new filename
    validation::validate_filename(&app.create_edit_file_name)?;

    let curr_path = format!("{}/{}", app.current_path_to_edit, app.current_name_to_edit);
    let new_path = format!("{}/{}", app.current_path_to_edit, app.create_edit_file_name);

    let old_path = Path::new(&curr_path);
    let new_path_obj = Path::new(&new_path);

    // Validate the old file exists
    validation::validate_path_exists(old_path)?;

    // Validate the new path doesn't already exist
    validation::validate_path_not_exists(new_path_obj)?;

    // Validate permissions for rename operation
    validation::validate_permissions(new_path_obj, "rename")?;

    // Attempt the rename
    fs::rename(old_path, new_path_obj).map_err(|e| match e.kind() {
        io::ErrorKind::PermissionDenied => {
            FileOperationError::permission_denied(old_path, "Cannot rename - check permissions")
        }
        io::ErrorKind::NotFound => FileOperationError::file_not_found(old_path),
        io::ErrorKind::AlreadyExists => FileOperationError::already_exists(new_path_obj),
        _ => FileOperationError::from(e),
    })?;

    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;
    use tempfile::tempdir;

    fn wait_for_delete(path: &Path) {
        let receiver = delete_with_progress(path.to_str().unwrap());
        loop {
            match receiver
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap()
            {
                DeleteMessage::Completed { success, message } => {
                    assert!(success, "{}", message);
                    break;
                }
                DeleteMessage::Error(message) => panic!("{}", message),
                _ => {}
            }
        }
    }

    #[test]
    fn delete_with_progress_removes_only_symlinks() {
        let target = tempdir().unwrap();
        let links = tempdir().unwrap();
        let file = target.path().join("keep.txt");
        fs::write(&file, "keep").unwrap();

        for (name, destination) in [
            ("directory", target.path().to_path_buf()),
            ("file", file.clone()),
            ("dangling", target.path().join("missing")),
        ] {
            let link = links.path().join(name);
            symlink(destination, &link).unwrap();
            wait_for_delete(&link);
            assert!(fs::symlink_metadata(&link).is_err());
            assert!(target.path().is_dir());
            assert_eq!(fs::read_to_string(&file).unwrap(), "keep");
        }
    }

    #[test]
    fn delete_directory_preserves_nested_symlink_targets() {
        let outside = tempdir().unwrap();
        let parent = tempdir().unwrap();
        let file = outside.path().join("keep.txt");
        fs::write(&file, "keep").unwrap();

        for name in ["async", "sync"] {
            let directory = parent.path().join(name);
            fs::create_dir(&directory).unwrap();
            fs::write(directory.join("delete.txt"), "delete").unwrap();
            symlink(outside.path(), directory.join("link")).unwrap();
            symlink(outside.path().join("missing"), directory.join("dangling")).unwrap();
            if name == "async" {
                wait_for_delete(&directory);
            } else {
                handle_delete_based_on_type(directory.to_str().unwrap()).unwrap();
            }
            assert!(!directory.exists());
            assert!(outside.path().is_dir());
            assert_eq!(fs::read_to_string(&file).unwrap(), "keep");
        }
    }

    #[test]
    fn synchronous_delete_helpers_remove_only_symlinks() {
        let target = tempdir().unwrap();
        let links = tempdir().unwrap();
        let file = target.path().join("keep.txt");
        fs::write(&file, "keep").unwrap();

        for delete in [handle_delete_based_on_type, delete_file, delete_dir] {
            for (name, destination) in [
                ("directory", target.path().to_path_buf()),
                ("file", file.clone()),
                ("dangling", target.path().join("missing")),
            ] {
                let link = links.path().join(name);
                symlink(destination, &link).unwrap();
                delete(link.to_str().unwrap()).unwrap();
                assert!(fs::symlink_metadata(&link).is_err());
                assert!(target.path().is_dir());
                assert_eq!(fs::read_to_string(&file).unwrap(), "keep");
            }
        }
    }

    #[test]
    fn delete_with_progress_reports_failed_removals() {
        use std::os::unix::fs::PermissionsExt;
        // Root bypasses directory write permissions.
        if unsafe { libc::geteuid() } == 0 {
            return;
        }
        let tmp = tempdir().unwrap();
        let src = tmp.path().join("source");
        let locked = src.join("locked");
        fs::create_dir_all(&locked).unwrap();
        fs::write(locked.join("keep"), "keep").unwrap();
        fs::write(src.join("removable"), "remove").unwrap();
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o555)).unwrap();
        let receiver = delete_with_progress(src.to_str().unwrap());
        let completion = loop {
            match receiver.recv_timeout(std::time::Duration::from_secs(5)) {
                Ok(DeleteMessage::Completed { success, message }) => break Ok((success, message)),
                Ok(DeleteMessage::Error(message)) => break Err(message),
                Ok(_) => {}
                Err(error) => break Err(error.to_string()),
            }
        };
        // Restore permissions before assertions, including on failure.
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
        let (success, message) = completion.unwrap();
        assert!(!success, "{message}");
        assert!(
            message.contains("Deleted 1 of 4 items; 3 failed"),
            "{message}"
        );
        assert!(
            message.contains(locked.join("keep").to_str().unwrap()),
            "{message}"
        );
        assert_eq!(fs::read_to_string(locked.join("keep")).unwrap(), "keep");
        assert!(!src.join("removable").exists());
    }

    #[test]
    fn copy_then_delete_preserves_root_symlink() {
        let tmp = tempdir().unwrap();
        let outside = tmp.path().join("outside");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("keep"), "keep").unwrap();
        let src = tmp.path().join("source");
        let dest = tmp.path().join("destination");
        symlink("outside", &src).unwrap();
        copy_then_delete(&src, &dest).unwrap();
        assert!(fs::symlink_metadata(&dest)
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(fs::read_link(&dest).unwrap(), Path::new("outside"));
        assert!(fs::symlink_metadata(&src).is_err());
        assert_eq!(fs::read_to_string(outside.join("keep")).unwrap(), "keep");
    }

    #[test]
    fn copy_then_delete_preserves_directory_links_and_loop() {
        let tmp = tempdir().unwrap();
        let src = tmp.path().join("source");
        let dest = tmp.path().join("destination");
        let outside = tmp.path().join("outside");
        fs::create_dir(&src).unwrap();
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("keep"), "keep").unwrap();
        fs::write(src.join("real.txt"), "data").unwrap();
        symlink("../outside", src.join("link")).unwrap();
        symlink(".", src.join("loop")).unwrap();
        copy_then_delete(&src, &dest).unwrap();
        for (name, target) in [("link", "../outside"), ("loop", ".")] {
            assert!(fs::symlink_metadata(dest.join(name))
                .unwrap()
                .file_type()
                .is_symlink());
            assert_eq!(fs::read_link(dest.join(name)).unwrap(), Path::new(target));
        }
        assert!(!src.exists());
        assert_eq!(fs::read_to_string(dest.join("real.txt")).unwrap(), "data");
        assert_eq!(fs::read_to_string(outside.join("keep")).unwrap(), "keep");
        assert_eq!(fs::read_dir(&outside).unwrap().count(), 1);
    }

    #[test]
    fn copy_then_delete_keeps_source_on_copy_failure() {
        use std::os::unix::fs::PermissionsExt;
        if unsafe { libc::geteuid() } == 0 {
            return; // root can read the locked file
        }
        let tmp = tempdir().unwrap();
        let src = tmp.path().join("source");
        let dest = tmp.path().join("destination");
        fs::create_dir(&src).unwrap();
        fs::write(src.join("keep"), "keep").unwrap();
        fs::write(src.join("locked"), "locked").unwrap();
        fs::set_permissions(src.join("locked"), fs::Permissions::from_mode(0o000)).unwrap();
        assert!(copy_then_delete(&src, &dest).is_err());
        assert_eq!(fs::read_to_string(src.join("keep")).unwrap(), "keep");
        assert!(src.join("locked").exists());
        assert!(fs::symlink_metadata(&dest).is_err());
    }

    #[test]
    fn move_preserves_dangling_root_and_rejects_existing_link() {
        let tmp = tempdir().unwrap();
        let src = tmp.path().join("source");
        let dest_dir = tmp.path().join("destination");
        fs::create_dir(&dest_dir).unwrap();
        symlink("missing", &src).unwrap();
        move_file_or_dir(src.to_str().unwrap(), dest_dir.to_str().unwrap()).unwrap();
        let dest = dest_dir.join("source");
        assert_eq!(fs::read_link(&dest).unwrap(), Path::new("missing"));
        assert!(fs::symlink_metadata(&src).is_err());
        fs::write(&src, "keep").unwrap();
        assert!(move_file_or_dir(src.to_str().unwrap(), dest_dir.to_str().unwrap()).is_err());
        assert_eq!(fs::read_link(&dest).unwrap(), Path::new("missing"));
        assert_eq!(fs::read_to_string(&src).unwrap(), "keep");
    }
}
