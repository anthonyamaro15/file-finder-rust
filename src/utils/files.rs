//! File utilities for path processing, sorting, and metadata.

use std::cmp::Reverse;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Context;
use rayon::prelude::*;

/// Sort order direction.
#[derive(Clone, Debug, PartialEq)]
pub enum SortType {
    ASC,
    DESC,
}

/// Sort criteria for file listings.
#[derive(Clone, Debug, PartialEq)]
pub enum SortBy {
    Name,
    Size,
    DateAdded,
    Default,
}

/// Sort file entries by the specified criteria and direction.
pub fn sort_entries_by_type(
    sort_by: SortBy,
    sort_type: SortType,
    mut entries: Vec<PathBuf>,
) -> Vec<PathBuf> {
    match sort_type {
        SortType::ASC => match sort_by {
            SortBy::Name => entries.sort_by(|a, b| {
                a.file_name()
                    .unwrap()
                    .to_ascii_lowercase()
                    .cmp(&b.file_name().unwrap().to_ascii_lowercase())
            }),
            SortBy::Size => entries.sort_by_cached_key(|path| size_key(path)),
            SortBy::DateAdded => entries.sort_by_cached_key(|path| date_added_key(path)),
            SortBy::Default => {}
        },
        SortType::DESC => match sort_by {
            SortBy::Name => entries.sort_by(|a, b| {
                b.file_name()
                    .unwrap()
                    .to_ascii_lowercase()
                    .cmp(&a.file_name().unwrap().to_ascii_lowercase())
            }),
            SortBy::Size => entries.sort_by_cached_key(|path| Reverse(size_key(path))),
            SortBy::DateAdded => entries.sort_by_cached_key(|path| Reverse(date_added_key(path))),
            SortBy::Default => {}
        },
    }

    entries
}

/// Size sort key; unreadable entries sort as empty.
fn size_key(path: &Path) -> u64 {
    path.metadata().map(|meta| meta.len()).unwrap_or(0)
}

/// Date sort key: creation time, then modification time, then a fixed epoch so
/// the key never changes between lookups.
fn date_added_key(path: &Path) -> SystemTime {
    path.metadata()
        .ok()
        .and_then(|meta| meta.created().or_else(|_| meta.modified()).ok())
        .unwrap_or(UNIX_EPOCH)
}

/// Convert PathBuf entries to strings with filtering and sorting.
/// Uses parallel processing via rayon for efficiency.
pub fn convert_file_path_to_string(
    entries: Vec<PathBuf>,
    show_hidden: bool,
    sort_by: SortBy,
    sort_type: SortType,
) -> Vec<String> {
    let sort_entries = sort_entries_by_type(sort_by, sort_type, entries);

    // Filter and process files in parallel
    let filtered_entries: Vec<PathBuf> = sort_entries
        .into_par_iter()
        .filter(|value| {
            if value.is_dir() {
                true
            } else if value.is_file() {
                if !show_hidden {
                    value
                        .file_name()
                        .and_then(|name| name.to_str())
                        .map(|name| !name.starts_with('.'))
                        .unwrap_or(false)
                } else {
                    true
                }
            } else {
                false
            }
        })
        .collect();

    // Convert to strings in parallel
    filtered_entries
        .into_par_iter()
        .filter_map(|entry| entry.to_str().map(|s| s.to_string()))
        .collect()
}

/// Get file list from a directory with filtering and sorting.
pub fn get_inner_files_info(
    file: String,
    show_hidden_files: bool,
    sort_by: SortBy,
    sort_type: &SortType,
) -> anyhow::Result<Vec<String>> {
    get_file_path_data(file.clone(), show_hidden_files, sort_by, sort_type)
        .with_context(|| format!("Could not read directory '{file}'"))
}

/// Get sorted file paths from a starting directory.
pub fn get_file_path_data(
    start_path: String,
    show_hidden: bool,
    sort_by: SortBy,
    sort_type: &SortType,
) -> anyhow::Result<Vec<String>> {
    let entries = fs::read_dir(start_path)?
        .map(|res| res.map(|e| e.path()))
        .collect::<Result<Vec<_>, io::Error>>()?;

    let file_strings =
        convert_file_path_to_string(entries, show_hidden, sort_by, sort_type.clone());

    Ok(file_strings)
}

/// Check if a path is a file (not a directory).
pub fn is_file(path: String) -> bool {
    match fs::metadata(path) {
        Ok(file) => file.file_type().is_file(),
        Err(_) => false,
    }
}

/// Generate a unique copy path in `new_path` by prefixing the file name with
/// "copy_" (then "copy_2_", "copy_3_", ...) until no entry with that name exists.
pub fn generate_copy_file_dir_name(
    curr_path: impl AsRef<Path>,
    new_path: impl AsRef<Path>,
) -> PathBuf {
    let file_name = curr_path.as_ref().file_name().unwrap_or_default();
    let candidate = |prefix: String| {
        let mut name = OsString::from(prefix);
        name.push(file_name);
        new_path.as_ref().join(name)
    };
    // symlink_metadata also treats dangling symlinks as taken.
    let is_free = |path: &Path| fs::symlink_metadata(path).is_err();

    // First try without a number suffix
    let first = candidate("copy_".to_string());
    if is_free(&first) {
        return first;
    }

    // Generate a unique name using numbered format: copy_2_file.txt, copy_3_file.txt, etc.
    let mut n = 2usize;
    loop {
        let numbered = candidate(format!("copy_{}_", n));
        if is_free(&numbered) {
            return numbered;
        }
        n += 1;
    }
}

/// Check if a path exists.
pub fn check_if_exists(new_path: String) -> bool {
    Path::new(&new_path).try_exists().unwrap_or_default()
}

/// Get the parent directory path from a file path.
pub fn get_curr_path(path: String) -> String {
    Path::new(&path)
        .parent()
        .map(|parent| parent.to_string_lossy().to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inner_files_reports_missing_directory() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("missing").to_string_lossy().into_owned();

        let error = get_inner_files_info(missing.clone(), false, SortBy::Default, &SortType::ASC)
            .unwrap_err();

        assert!(error.to_string().contains(&missing));
        assert_eq!(
            error.downcast_ref::<io::Error>().unwrap().kind(),
            io::ErrorKind::NotFound
        );
    }

    #[cfg(unix)]
    #[test]
    fn inner_files_reports_unreadable_directory() {
        use std::os::unix::fs::PermissionsExt;

        if unsafe { libc::geteuid() } == 0 {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let permissions = fs::metadata(dir.path()).unwrap().permissions();
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o000)).unwrap();

        let path = dir.path().to_string_lossy().into_owned();
        let result = get_inner_files_info(path.clone(), false, SortBy::Default, &SortType::ASC);
        fs::set_permissions(dir.path(), permissions).unwrap();

        let error = result.unwrap_err();
        assert!(error.to_string().contains(&path));
        assert_eq!(
            error.downcast_ref::<io::Error>().unwrap().kind(),
            io::ErrorKind::PermissionDenied
        );
    }

    #[test]
    fn test_is_file_with_directory() {
        // Current directory should not be a file
        assert!(!is_file(".".to_string()));
    }

    #[test]
    fn test_check_if_exists() {
        // Current directory should exist
        assert!(check_if_exists(".".to_string()));
        // Random path should not exist
        assert!(!check_if_exists("/nonexistent/path/12345".to_string()));
    }

    #[test]
    fn test_get_curr_path() {
        assert_eq!(get_curr_path("/foo/bar/baz.txt".to_string()), "/foo/bar");
        assert_eq!(get_curr_path("/foo/bar".to_string()), "/foo");
        assert_eq!(get_curr_path("/foo".to_string()), "/");
    }

    #[test]
    fn date_sort_is_deterministic_with_missing_entries() {
        let dir = tempfile::tempdir().unwrap();
        let mut entries = vec![dir.path().join("missing")];
        for name in ["b.txt", "a.txt", "c.txt"] {
            fs::write(dir.path().join(name), name).unwrap();
            entries.push(dir.path().join(name));
        }

        for (sort_type, missing_index) in [(SortType::ASC, 0), (SortType::DESC, 3)] {
            let first = sort_entries_by_type(SortBy::DateAdded, sort_type.clone(), entries.clone());
            let second = sort_entries_by_type(SortBy::DateAdded, sort_type, first.clone());
            assert_eq!(first, second);
            // The missing entry falls back to UNIX_EPOCH, the oldest possible date.
            assert_eq!(first[missing_index], dir.path().join("missing"));
        }
    }

    #[test]
    fn size_sort_orders_by_length() {
        let dir = tempfile::tempdir().unwrap();
        let entries: Vec<PathBuf> = [("big", 3), ("small", 1), ("mid", 2)]
            .into_iter()
            .map(|(name, len)| {
                let path = dir.path().join(name);
                fs::write(&path, "x".repeat(len)).unwrap();
                path
            })
            .collect();
        let names = |sorted: Vec<PathBuf>| -> Vec<String> {
            sorted
                .iter()
                .map(|path| path.file_name().unwrap().to_string_lossy().to_string())
                .collect()
        };

        let asc = sort_entries_by_type(SortBy::Size, SortType::ASC, entries.clone());
        assert_eq!(names(asc), ["small", "mid", "big"]);
        let desc = sort_entries_by_type(SortBy::Size, SortType::DESC, entries);
        assert_eq!(names(desc), ["big", "mid", "small"]);
    }

    #[test]
    fn copy_name_adds_numbered_suffix_when_taken() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("file.txt");
        for name in ["file.txt", "copy_file.txt", "copy_2_file.txt"] {
            fs::write(dir.path().join(name), "").unwrap();
        }

        assert_eq!(
            generate_copy_file_dir_name(&src, dir.path()),
            dir.path().join("copy_3_file.txt")
        );
    }

    #[test]
    fn copy_name_without_file_name_does_not_panic() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            generate_copy_file_dir_name(Path::new("/"), dir.path()),
            dir.path().join("copy_")
        );
    }

    #[cfg(unix)]
    #[test]
    fn copy_name_keeps_non_utf8_file_name() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;

        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join(OsStr::from_bytes(b"bad\xff.txt"));

        let first = generate_copy_file_dir_name(&src, dir.path());
        assert_eq!(
            first,
            dir.path().join(OsStr::from_bytes(b"copy_bad\xff.txt"))
        );

        // Some filesystems (e.g. APFS) reject non-UTF-8 names; check collisions where allowed.
        if fs::write(&first, "").is_ok() {
            let second = generate_copy_file_dir_name(&src, dir.path());
            assert_eq!(
                second,
                dir.path().join(OsStr::from_bytes(b"copy_2_bad\xff.txt"))
            );
        }
    }
}
