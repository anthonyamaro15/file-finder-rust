mod common;

use std::fs;
use std::path::Path;
use tempfile::{tempdir, NamedTempFile};
use serde_json;
use common::*;

use file_finder::configuration::Configuration;
use file_finder::directory_store::{DirectoryStore, build_directory_from_store, save_directory_to_file, load_directory_from_file};

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
            "/home/user/downloads"
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
        assert!(loaded_store.directories.contains(&"/home/user/documents".to_string()));
        assert!(loaded_store.directories.contains(&"/home/user/pictures".to_string()));
        assert!(loaded_store.directories.contains(&"/home/user/downloads".to_string()));
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
        assert!(!store.directories.iter().any(|dir| dir.contains("node_modules")));
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

#[cfg(test)]
mod configuration_tests {
    use super::*;

    #[test]
    fn test_configuration_new() {
        let config = Configuration::new();
        
        // Should have default values
        assert!(!config.start_path.is_empty());
        assert!(!config.ignore_directories.is_empty());
        assert!(!config.root_dir.is_empty());
        assert!(!config.cache_directory.is_empty());
        assert!(!config.settings_path.is_empty());
        
        // Should contain common ignore patterns
        assert!(config.ignore_directories.contains(&"node_modules".to_string()));
        assert!(config.ignore_directories.contains(&".git".to_string()));
        assert!(config.ignore_directories.contains(&"target".to_string()));
    }

    #[test] 
    fn test_configuration_default_ignore_directories() {
        let config = Configuration::new();
        
        let expected_ignores = vec![
            "node_modules", "bower_components", "obj", "maui", "target", 
            ".venv", "src", "cdn", ".git", ".sauce", ".husky", ".vscode", 
            ".zed", "cypress", "dist-prod"
        ];
        
        for ignore in expected_ignores {
            assert!(
                config.ignore_directories.contains(&ignore.to_string()),
                "Missing ignore pattern: {}", ignore
            );
        }
    }

    #[test]
    fn test_configuration_paths_format() {
        let config = Configuration::new();
        
        // Paths should contain expected patterns
        assert!(config.cache_directory.contains(".config/ff/cache_directory.json"));
        assert!(config.settings_path.contains(".config/ff/settings.json"));
        assert!(config.start_path.contains("Desktop"));
    }
}

#[cfg(test)]
mod configuration_file_operations_tests {
    use super::*;

    #[test]
    fn test_write_and_load_settings() {
        let temp_dir = tempdir().expect("Failed to create temp directory");
        let settings_path = temp_dir.path().join("settings.json").to_string_lossy().to_string();
        
        // Create a configuration and modify the settings path
        let mut config = Configuration::new();
        config.settings_path = settings_path.clone();
        config.start_path = "/custom/start/path".to_string();
        
        // Write settings to file
        let write_result = config.write_settings_to_file();
        assert!(write_result.is_ok());
        assert!(Path::new(&settings_path).exists());
        
        // Load settings from file
        let loaded_config_result = config.load_settings_from_file(&settings_path);
        assert!(loaded_config_result.is_ok());
        
        let loaded_config = loaded_config_result.unwrap();
        assert_eq!(loaded_config.start_path, "/custom/start/path");
        assert_eq!(loaded_config.settings_path, settings_path);
    }

    #[test]
    fn test_load_settings_nonexistent_file() {
        let config = Configuration::new();
        let nonexistent_path = "/path/that/does/not/exist.json";
        
        let result = config.load_settings_from_file(nonexistent_path);
        assert!(result.is_err());
    }

    #[test] 
    fn test_load_settings_malformed_json() {
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let file_path = temp_file.path().to_str().unwrap();
        
        // Write invalid JSON
        fs::write(file_path, "{ invalid json }").expect("Failed to write invalid JSON");
        
        let config = Configuration::new();
        let result = config.load_settings_from_file(file_path);
        assert!(result.is_err());
    }

    #[test]
    fn test_configuration_serialization_deserialization() {
        let config = Configuration::new();
        
        // Serialize to JSON
        let json_result = serde_json::to_string(&config);
        assert!(json_result.is_ok());
        
        let json_str = json_result.unwrap();
        
        // Deserialize from JSON
        let deserialized_result: Result<Configuration, _> = serde_json::from_str(&json_str);
        assert!(deserialized_result.is_ok());
        
        let deserialized_config = deserialized_result.unwrap();
        
        // Should be equal to original
        assert_eq!(config.start_path, deserialized_config.start_path);
        assert_eq!(config.ignore_directories, deserialized_config.ignore_directories);
        assert_eq!(config.root_dir, deserialized_config.root_dir);
        assert_eq!(config.cache_directory, deserialized_config.cache_directory);
        assert_eq!(config.settings_path, deserialized_config.settings_path);
    }
}

#[cfg(test)]
mod configuration_directory_creation_tests {
    use super::*;

    #[test] 
    fn test_handle_settings_configuration_creates_files() {
        let temp_dir = tempdir().expect("Failed to create temp directory");
        let temp_path = temp_dir.path().to_string_lossy().to_string();
        
        // Create a configuration with custom paths pointing to our temp directory
        let mut config = Configuration::new();
        config.root_dir = temp_path.clone();
        config.cache_directory = format!("{}/.config/ff/cache_directory.json", temp_path);
        config.settings_path = format!("{}/.config/ff/settings.json", temp_path);
        
        // This should create the directory and files
        config.handle_settings_configuration().expect("Failed to handle settings configuration");
        
        // Check that the directory was created
        let config_dir = format!("{}/.config/ff", temp_path);
        assert!(Path::new(&config_dir).exists());
        assert!(Path::new(&config_dir).is_dir());
        
        // Check that the settings file was created
        assert!(Path::new(&config.settings_path).exists());
    }

    #[test]
    fn test_handle_settings_configuration_loads_existing() {
        let temp_dir = tempdir().expect("Failed to create temp directory");
        let temp_path = temp_dir.path().to_string_lossy().to_string();
        
        // Create config directory and a settings file manually
        let config_dir = format!("{}/.config/ff", temp_path);
        fs::create_dir_all(&config_dir).expect("Failed to create config directory");
        
        let settings_path = format!("{}/settings.json", config_dir);
        let custom_config = Configuration {
            start_path: "/custom/path".to_string(),
            ignore_directories: vec!["custom_ignore".to_string()],
            root_dir: temp_path.clone(),
            cache_directory: format!("{}/cache_directory.json", config_dir),
            settings_path: settings_path.clone(),
        };
        
        // Write custom settings
        let json_content = serde_json::to_string(&custom_config).expect("Failed to serialize config");
        fs::write(&settings_path, json_content).expect("Failed to write settings file");
        
        // Create a new config and handle settings configuration
        let mut config = Configuration::new();
        config.root_dir = temp_path;
        config.settings_path = settings_path;
        
        config.handle_settings_configuration().expect("Failed to handle settings configuration");
        
        // Should have loaded the custom values
        assert_eq!(config.start_path, "/custom/path");
        assert_eq!(config.ignore_directories, vec!["custom_ignore".to_string()]);
    }
}
