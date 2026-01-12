//! File operations: create, delete, and rename files and directories.

use std::fs::{self, File};
use std::io;
use std::path::Path;

use crate::app::App;
use crate::errors::{validation, FileOperationError, FileOperationResult};

/// Delete a file at the specified path.
pub fn delete_file(file_path: &str) -> FileOperationResult<()> {
    let path = Path::new(file_path);

    // Validate file exists
    validation::validate_path_exists(path)?;

    // Validate permissions
    validation::validate_permissions(path, "delete")?;

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

    // Validate file exists
    validation::validate_path_exists(path)?;

    let metadata = fs::metadata(path).map_err(|e| match e.kind() {
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

    // Validate source exists
    validation::validate_path_exists(src)?;

    // Validate destination doesn't exist
    validation::validate_path_not_exists(&dest)?;

    // Try fs::rename first (fast path for same filesystem)
    match fs::rename(src, &dest) {
        Ok(()) => Ok(()),
        Err(e) => {
            // If cross-device link error, fall back to copy+delete
            if e.raw_os_error() == Some(18) {
                // EXDEV (cross-device link) on Unix
                // Copy first
                if src.is_dir() {
                    copy_dir_recursive(src, &dest)?;
                } else {
                    fs::copy(src, &dest).map_err(|e| FileOperationError::from(e))?;
                }
                // Then delete source
                if src.is_dir() {
                    fs::remove_dir_all(src).map_err(|e| FileOperationError::from(e))?;
                } else {
                    fs::remove_file(src).map_err(|e| FileOperationError::from(e))?;
                }
                Ok(())
            } else {
                Err(match e.kind() {
                    io::ErrorKind::PermissionDenied => {
                        FileOperationError::permission_denied(src, "Cannot move - check permissions")
                    }
                    io::ErrorKind::NotFound => FileOperationError::file_not_found(src),
                    io::ErrorKind::AlreadyExists => FileOperationError::already_exists(&dest),
                    _ => FileOperationError::from(e),
                })
            }
        }
    }
}

/// Helper function to copy a directory recursively.
fn copy_dir_recursive(src: &Path, dest: &Path) -> FileOperationResult<()> {
    fs::create_dir_all(dest).map_err(|e| FileOperationError::from(e))?;

    for entry in fs::read_dir(src).map_err(|e| FileOperationError::from(e))? {
        let entry = entry.map_err(|e| FileOperationError::from(e))?;
        let entry_path = entry.path();
        let dest_path = dest.join(entry.file_name());

        if entry_path.is_dir() {
            copy_dir_recursive(&entry_path, &dest_path)?;
        } else {
            fs::copy(&entry_path, &dest_path).map_err(|e| FileOperationError::from(e))?;
        }
    }

    Ok(())
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
