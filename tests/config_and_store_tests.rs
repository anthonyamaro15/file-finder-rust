mod common;

use common::*;
use std::fs;
use tempfile::{tempdir, NamedTempFile};

use file_finder::directory_store::{
    build_directory_from_store, load_directory_from_file, save_directory_to_file, DirectoryStore,
};

#[cfg(test)]
mod directory_store_tests {
    use super::*;

    #[test]
    fn test_directory_store_new() {
        let store = DirectoryStore::new();
        assert!(store.directories.is_empty());
    }

    #[test]
    fn test_directory_store_insert() {
        let mut store = DirectoryStore::new();
        let path = "/home/user/documents";

        store.insert(path);

        assert_eq!(store.directories.len(), 1);
        assert_eq!(store.directories[0], path);
    }

    #[test]
    fn test_directory_store_insert_multiple() {
        let mut store = DirectoryStore::new();
        let paths = vec![
            "/home/user/documents",
            "/home/user/pictures",
            "/home/user/downloads",
        ];

        for path in &paths {
            store.insert(path);
        }

        assert_eq!(store.directories.len(), 3);
        for (i, path) in paths.iter().enumerate() {
            assert_eq!(store.directories[i], *path);
        }
    }

    #[test]
    fn test_directory_store_search_exact_match() {
        let mut store = DirectoryStore::new();
        store.insert("/home/user/documents");
        store.insert("/home/user/pictures");
        store.insert("/home/user/downloads");

        let results = store.search("documents");

        assert_eq!(results.len(), 1);
        assert_eq!(results[0], "/home/user/documents");
    }

    #[test]
    fn test_directory_store_search_partial_match() {
        let mut store = DirectoryStore::new();
        store.insert("/home/user/documents/work");
        store.insert("/home/user/documents/personal");
        store.insert("/home/user/pictures");

        let results = store.search("documents");

        assert_eq!(results.len(), 2);
        assert!(results.contains(&"/home/user/documents/work".to_string()));
        assert!(results.contains(&"/home/user/documents/personal".to_string()));
    }

    #[test]
    fn test_directory_store_search_no_match() {
        let mut store = DirectoryStore::new();
        store.insert("/home/user/documents");
        store.insert("/home/user/pictures");

        let results = store.search("nonexistent");

        assert!(results.is_empty());
    }

    #[test]
    fn test_directory_store_search_empty_query() {
        let mut store = DirectoryStore::new();
        store.insert("/home/user/documents");
        store.insert("/home/user/pictures");

        let results = store.search("");

        // Empty search should match all directories
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_directory_store_search_case_sensitive() {
        let mut store = DirectoryStore::new();
        store.insert("/home/user/Documents");
        store.insert("/home/user/documents");

        let results = store.search("Documents");

        assert_eq!(results.len(), 1);
        assert_eq!(results[0], "/home/user/Documents");
    }
}

#[cfg(test)]
mod directory_store_file_operations_tests {
    use super::*;

    #[test]
    fn test_save_and_load_directory_store() {
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let file_path = temp_file.path().to_str().unwrap();

        // Create and populate a directory store
        let mut store = DirectoryStore::new();
        store.insert("/home/user/documents");
        store.insert("/home/user/pictures");
        store.insert("/home/user/downloads");

        // Save to file
        let save_result = save_directory_to_file(&store, file_path);
        assert!(save_result.is_ok());

        // Load from file
        let loaded_store = load_directory_from_file(file_path);
        assert!(loaded_store.is_ok());

        let loaded_store = loaded_store.unwrap();
        assert_eq!(loaded_store.directories.len(), 3);
        assert!(loaded_store
            .directories
            .contains(&"/home/user/documents".to_string()));
        assert!(loaded_store
            .directories
            .contains(&"/home/user/pictures".to_string()));
        assert!(loaded_store
            .directories
            .contains(&"/home/user/downloads".to_string()));
    }

    #[test]
    fn test_save_empty_directory_store() {
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let file_path = temp_file.path().to_str().unwrap();

        let store = DirectoryStore::new();

        let save_result = save_directory_to_file(&store, file_path);
        assert!(save_result.is_ok());

        let loaded_store = load_directory_from_file(file_path).unwrap();
        assert!(loaded_store.directories.is_empty());
    }

    #[test]
    fn test_load_nonexistent_file() {
        let nonexistent_path = "/path/that/does/not/exist.json";
        let result = load_directory_from_file(nonexistent_path);
        assert!(result.is_err());
    }

    #[test]
    fn test_load_malformed_json() {
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let file_path = temp_file.path().to_str().unwrap();

        // Write invalid JSON
        fs::write(file_path, "invalid json content").expect("Failed to write invalid JSON");

        let result = load_directory_from_file(file_path);
        assert!(result.is_err());
    }
}

#[cfg(test)]
mod build_directory_store_tests {
    use super::*;

    #[test]
    fn test_build_directory_from_store_basic() {
        let temp_dir = setup_test_directory().expect("Failed to create test directory");
        let temp_path = temp_dir.path().to_str().unwrap();

        let ignore_directories = vec![];
        let store = build_directory_from_store(temp_path, ignore_directories);

        // Should find the subdirectories we created
        assert!(!store.directories.is_empty());
        assert!(store.directories.iter().any(|dir| dir.contains("subdir1")));
        assert!(store.directories.iter().any(|dir| dir.contains("subdir2")));
    }

    #[test]
    fn test_build_directory_from_store_with_ignore() {
        let temp_dir = setup_test_directory().expect("Failed to create test directory");
        let temp_path = temp_dir.path().to_str().unwrap();

        // Create some directories that should be ignored
        let node_modules_path = temp_dir.path().join("node_modules");
        let git_path = temp_dir.path().join(".git");
        fs::create_dir_all(&node_modules_path).expect("Failed to create node_modules");
        fs::create_dir_all(&git_path).expect("Failed to create .git");

        let ignore_directories = vec!["node_modules".to_string(), ".git".to_string()];
        let store = build_directory_from_store(temp_path, ignore_directories);

        // Should not contain ignored directories
        assert!(!store
            .directories
            .iter()
            .any(|dir| dir.contains("node_modules")));
        assert!(!store.directories.iter().any(|dir| dir.contains(".git")));

        // But should contain other directories
        assert!(store.directories.iter().any(|dir| dir.contains("subdir1")));
    }

    #[test]
    fn test_build_directory_from_store_empty_directory() {
        let temp_dir = tempdir().expect("Failed to create temp directory");
        let temp_path = temp_dir.path().to_str().unwrap();

        let ignore_directories = vec![];
        let store = build_directory_from_store(temp_path, ignore_directories);

        // Should be empty since there are no subdirectories
        assert!(store.directories.is_empty());
    }
}
