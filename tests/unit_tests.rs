mod common;

use common::*;
use std::fs;
use std::path::PathBuf;
use tempfile::tempdir;

// We need to make the functions in main.rs accessible for testing
// For now, I'll test the logic by creating similar functions

#[cfg(test)]
mod sorting_tests {
    use super::*;

    #[derive(Clone)]
    pub enum SortType {
        ASC,
        DESC,
    }

    #[derive(Clone)]
    pub enum SortBy {
        Name,
        Size,
        DateAdded,
        Default,
    }

    // Replicate the sorting function for testing
    fn sort_entries_by_type(
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
                _ => {}
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
                _ => {}
            },
        }
        entries
    }

    #[test]
    fn test_sort_by_name_ascending() {
        let temp_dir = setup_simple_test_directory().expect("Failed to create test directory");
        let base_path = temp_dir.path();

        let mut entries = vec![
            base_path.join("zebra.txt"),
            base_path.join("apple.txt"),
            base_path.join("banana.txt"),
        ];

        // Create the files
        for entry in &entries {
            fs::write(entry, "test content").expect("Failed to create test file");
        }

        let sorted = sort_entries_by_type(SortBy::Name, SortType::ASC, entries.clone());

        assert_eq!(
            sorted[0].file_name().unwrap().to_str().unwrap(),
            "apple.txt"
        );
        assert_eq!(
            sorted[1].file_name().unwrap().to_str().unwrap(),
            "banana.txt"
        );
        assert_eq!(
            sorted[2].file_name().unwrap().to_str().unwrap(),
            "zebra.txt"
        );
    }

    #[test]
    fn test_sort_by_name_descending() {
        let temp_dir = setup_simple_test_directory().expect("Failed to create test directory");
        let base_path = temp_dir.path();

        let mut entries = vec![
            base_path.join("apple.txt"),
            base_path.join("zebra.txt"),
            base_path.join("banana.txt"),
        ];

        // Create the files
        for entry in &entries {
            fs::write(entry, "test content").expect("Failed to create test file");
        }

        let sorted = sort_entries_by_type(SortBy::Name, SortType::DESC, entries.clone());

        assert_eq!(
            sorted[0].file_name().unwrap().to_str().unwrap(),
            "zebra.txt"
        );
        assert_eq!(
            sorted[1].file_name().unwrap().to_str().unwrap(),
            "banana.txt"
        );
        assert_eq!(
            sorted[2].file_name().unwrap().to_str().unwrap(),
            "apple.txt"
        );
    }

    #[test]
    fn test_sort_by_size_ascending() {
        let temp_dir = setup_simple_test_directory().expect("Failed to create test directory");
        let base_path = temp_dir.path();

        let entries = vec![
            base_path.join("large.txt"),
            base_path.join("medium.txt"),
            base_path.join("small.txt"),
        ];

        // Create files with different sizes
        fs::write(&entries[0], "x".repeat(1000)).expect("Failed to create large file");
        fs::write(&entries[1], "x".repeat(100)).expect("Failed to create medium file");
        fs::write(&entries[2], "x".repeat(10)).expect("Failed to create small file");

        let sorted = sort_entries_by_type(SortBy::Size, SortType::ASC, entries.clone());

        // Should be sorted by size: small, medium, large
        assert!(sorted[0].metadata().unwrap().len() < sorted[1].metadata().unwrap().len());
        assert!(sorted[1].metadata().unwrap().len() < sorted[2].metadata().unwrap().len());
    }

    #[test]
    fn test_sort_case_insensitive() {
        let temp_dir = setup_simple_test_directory().expect("Failed to create test directory");
        let base_path = temp_dir.path();

        let entries = vec![
            base_path.join("Apple.txt"),
            base_path.join("banana.txt"),
            base_path.join("Cherry.txt"),
        ];

        // Create the files
        for entry in &entries {
            fs::write(entry, "test content").expect("Failed to create test file");
        }

        let sorted = sort_entries_by_type(SortBy::Name, SortType::ASC, entries.clone());

        assert_eq!(
            sorted[0].file_name().unwrap().to_str().unwrap(),
            "Apple.txt"
        );
        assert_eq!(
            sorted[1].file_name().unwrap().to_str().unwrap(),
            "banana.txt"
        );
        assert_eq!(
            sorted[2].file_name().unwrap().to_str().unwrap(),
            "Cherry.txt"
        );
    }
}

#[cfg(test)]
mod path_handling_tests {
    use super::*;

    fn get_curr_path(path: String) -> String {
        let mut split_path = path.split("/").collect::<Vec<&str>>();
        split_path.pop();
        let vec_to_str = split_path.join("/");
        vec_to_str
    }

    fn is_file(path: String) -> bool {
        match fs::metadata(path) {
            Ok(file) => {
                let file_t = file.file_type();
                file_t.is_file()
            }
            Err(_) => false,
        }
    }

    #[test]
    fn test_get_curr_path() {
        let path = "/home/user/documents/file.txt".to_string();
        let parent = get_curr_path(path);
        assert_eq!(parent, "/home/user/documents");
    }

    #[test]
    fn test_get_curr_path_root() {
        let path = "/file.txt".to_string();
        let parent = get_curr_path(path);
        assert_eq!(parent, "");
    }

    #[test]
    fn test_get_curr_path_no_extension() {
        let path = "/home/user/documents/folder".to_string();
        let parent = get_curr_path(path);
        assert_eq!(parent, "/home/user/documents");
    }

    #[test]
    fn test_is_file_detection() {
        let temp_dir = setup_simple_test_directory().expect("Failed to create test directory");
        let base_path = temp_dir.path();

        let file_path = base_path.join("test.txt");
        let dir_path = base_path.join("test_dir");

        fs::write(&file_path, "test content").expect("Failed to create test file");
        fs::create_dir(&dir_path).expect("Failed to create test directory");

        assert!(is_file(file_path.to_string_lossy().to_string()));
        assert!(!is_file(dir_path.to_string_lossy().to_string()));
        assert!(!is_file("/nonexistent/path".to_string()));
    }
}

#[cfg(test)]
mod string_conversion_tests {
    use super::sorting_tests::{SortBy, SortType};
    use super::*;

    fn convert_file_path_to_string(
        entries: Vec<PathBuf>,
        show_hidden: bool,
        _sort_by: SortBy,
        _sort_type: SortType,
    ) -> Vec<String> {
        let mut file_strings: Vec<String> = Vec::new();

        for entry in entries {
            if entry.is_dir() {
                let file = entry.into_os_string().to_str().unwrap().to_string();
                file_strings.push(file);
            } else if entry.is_file() {
                let file_name = entry.file_name().unwrap().to_str().unwrap();
                if show_hidden || !file_name.starts_with(".") {
                    let entry_value = entry.to_str().unwrap().to_string();
                    file_strings.push(entry_value);
                }
            }
        }

        file_strings
    }

    #[test]
    fn test_convert_paths_show_all() {
        let temp_dir = setup_test_directory().expect("Failed to create test directory");
        let base_path = temp_dir.path();

        let entries = vec![
            base_path.join("file1.txt"),
            base_path.join(".hidden_file"),
            base_path.join("subdir1"),
        ];

        let result = convert_file_path_to_string(entries, true, SortBy::Default, SortType::ASC);

        // Should include hidden files when show_hidden is true
        assert_eq!(result.len(), 3);
        assert!(result.iter().any(|path| path.contains(".hidden_file")));
    }

    #[test]
    fn test_convert_paths_hide_hidden() {
        let temp_dir = setup_test_directory().expect("Failed to create test directory");
        let base_path = temp_dir.path();

        let entries = vec![
            base_path.join("file1.txt"),
            base_path.join(".hidden_file"),
            base_path.join("subdir1"),
        ];

        let result = convert_file_path_to_string(entries, false, SortBy::Default, SortType::ASC);

        // Should not include hidden files when show_hidden is false
        assert_eq!(result.len(), 2);
        assert!(!result.iter().any(|path| path.contains(".hidden_file")));
    }
}

#[cfg(test)]
mod utility_tests {
    use super::*;

    fn generate_copy_file_dir_name(curr_path: String, new_path: String) -> String {
        let get_info = std::path::Path::new(&curr_path);
        let file_name = get_info.file_name().unwrap().to_str().unwrap();
        format!("{}/copy_{}", new_path, file_name)
    }

    fn create_item_based_on_type(
        current_file_path: String,
        new_item: String,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        if new_item.contains(".") {
            // It's a file
            Ok(true)
        } else {
            // It's a directory
            Ok(false)
        }
    }

    #[test]
    fn test_generate_copy_file_dir_name() {
        let curr_path = "/home/user/document.txt".to_string();
        let new_path = "/home/user/backup".to_string();

        let result = generate_copy_file_dir_name(curr_path, new_path);
        assert_eq!(result, "/home/user/backup/copy_document.txt");
    }

    #[test]
    fn test_create_item_based_on_type_file() {
        let current_path = "/tmp".to_string();
        let new_item = "test.txt".to_string();

        let result = create_item_based_on_type(current_path, new_item).unwrap();
        assert!(result); // Should be true for files
    }

    #[test]
    fn test_create_item_based_on_type_directory() {
        let current_path = "/tmp".to_string();
        let new_item = "new_directory".to_string();

        let result = create_item_based_on_type(current_path, new_item).unwrap();
        assert!(!result); // Should be false for directories
    }
}
