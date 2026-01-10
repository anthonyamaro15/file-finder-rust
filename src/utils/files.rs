//! File utilities for path processing, sorting, and metadata.

use std::fs::{self, Metadata};
use std::io;
use std::path::{Path, PathBuf};

use rayon::prelude::*;

use super::format::{format_file_size, format_system_time};

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
            SortBy::Size => entries.sort_by(|a, b| {
                a.metadata()
                    .ok()
                    .map(|meta| meta.len())
                    .unwrap_or(0)
                    .cmp(&b.metadata().ok().map(|meta| meta.len()).unwrap_or(0))
            }),
            SortBy::DateAdded => entries.sort_by(|a, b| {
                a.metadata()
                    .ok()
                    .and_then(|meta| meta.created().ok())
                    .unwrap_or(std::time::SystemTime::now())
                    .cmp(
                        &b.metadata()
                            .ok()
                            .and_then(|meta| meta.created().ok())
                            .unwrap_or(std::time::SystemTime::now()),
                    )
            }),
            SortBy::Default => {}
        },
        SortType::DESC => match sort_by {
            SortBy::Name => entries.sort_by(|a, b| {
                b.file_name()
                    .unwrap()
                    .to_ascii_lowercase()
                    .cmp(&a.file_name().unwrap().to_ascii_lowercase())
            }),
            SortBy::Size => entries.sort_by(|a, b| {
                b.metadata()
                    .ok()
                    .map(|meta| meta.len())
                    .unwrap_or(0)
                    .cmp(&a.metadata().ok().map(|meta| meta.len()).unwrap_or(0))
            }),
            SortBy::DateAdded => entries.sort_by(|a, b| {
                b.metadata()
                    .ok()
                    .and_then(|meta| meta.created().ok())
                    .unwrap_or(std::time::SystemTime::now())
                    .cmp(
                        &a.metadata()
                            .ok()
                            .and_then(|meta| meta.created().ok())
                            .unwrap_or(std::time::SystemTime::now()),
                    )
            }),
            SortBy::Default => {}
        },
    }

    entries
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
) -> anyhow::Result<Option<Vec<String>>> {
    let entries = match fs::read_dir(file) {
        Ok(en) => {
            let val = en.map(|res| res.map(|e| e.path())).collect();
            match val {
                Ok(v) => v,
                Err(e) => {
                    println!("Error: {}", e);
                    return Ok(None);
                }
            }
        }
        Err(e) => {
            println!("Error: {}", e);
            return Ok(None);
        }
    };

    let file_strings =
        convert_file_path_to_string(entries, show_hidden_files, sort_by, sort_type.clone());
    Ok(Some(file_strings))
}

/// Get file names from a directory path (without full paths).
pub fn get_content_from_path(path: String) -> Option<Vec<String>> {
    let mut file_name_list: Vec<String> = Vec::new();
    match fs::read_dir(path) {
        Ok(val) => {
            for name in val.into_iter() {
                match name {
                    Ok(result) => {
                        let file_name = result.file_name().to_str().unwrap().to_string();
                        file_name_list.push(file_name);
                    }
                    Err(e) => {
                        println!("error getting content from path: {:?}", e);
                        return None;
                    }
                }
            }
        }
        Err(e) => {
            println!("her: {:?}", e);
            return None;
        }
    };
    Some(file_name_list)
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

/// Get metadata for a file path.
pub fn get_metadata_info(path: String) -> anyhow::Result<Option<Metadata>> {
    let metadata = match fs::metadata(path) {
        Ok(info) => Some(info),
        Err(_) => None,
    };

    Ok(metadata)
}

/// Generate a formatted string with file metadata info.
pub fn generate_metadata_str_info(metadata: anyhow::Result<Option<Metadata>>) -> String {
    let metadata_info = match metadata {
        Ok(res) => match res {
            Some(info) => {
                let size = format_file_size(info.len());
                let permissions = info.permissions();
                let readonly = if permissions.readonly() { "RO" } else { "RW" };

                // Try to get modification time
                let modified = info
                    .modified()
                    .map(format_system_time)
                    .unwrap_or_else(|_| "unknown".to_string());

                format!("{} | {} | modified {}", size, readonly, modified)
            }
            None => String::from("Info not available"),
        },
        Err(_) => String::from("Error reading metadata"),
    };

    metadata_info
}

/// Generate a unique copy name by prefixing "copy_" until unique.
pub fn generate_copy_file_dir_name(curr_path: String, new_path: String) -> String {
    let get_info = Path::new(&curr_path);
    let file_name = get_info.file_name().unwrap().to_str().unwrap().to_string();

    // Generate a unique name by prefixing copy_ repeatedly until it does not exist
    let mut copies = 1usize;
    loop {
        let prefix = "copy_".repeat(copies);
        let candidate = format!("{}/{}{}", new_path, prefix, file_name);
        if !Path::new(&candidate).exists() {
            return candidate;
        }
        copies += 1;
    }
}

/// Check if a path exists.
pub fn check_if_exists(new_path: String) -> bool {
    match Path::new(&new_path).try_exists() {
        Ok(value) => value,
        Err(_) => {
            // If we can't determine existence, assume it doesn't exist
            false
        }
    }
}

/// Get the parent directory path from a file path.
pub fn get_curr_path(path: String) -> String {
    let mut split_path = path.split('/').collect::<Vec<&str>>();
    split_path.pop();
    split_path.join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(get_curr_path("/foo".to_string()), "");
    }

    #[test]
    fn test_generate_metadata_str_info_none() {
        let result = generate_metadata_str_info(Ok(None));
        assert_eq!(result, "Info not available");
    }

    #[test]
    fn test_generate_metadata_str_info_error() {
        let result = generate_metadata_str_info(Err(anyhow::anyhow!("test error")));
        assert_eq!(result, "Error reading metadata");
    }
}
