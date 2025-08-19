use std::fs;
use std::path::{Path, PathBuf};
use tempfile::{tempdir, TempDir};

/// Creates a temporary directory structure for testing
/// 
/// Creates the following structure:
/// ```
/// temp_dir/
/// ├── file1.txt
/// ├── file2.rs  
/// ├── .hidden_file
/// ├── subdir1/
/// │   ├── nested_file.md
/// │   └── another_file.py
/// ├── subdir2/
/// │   └── empty/
/// ├── large_file.log
/// └── image.png (empty file)
/// ```
pub fn setup_test_directory() -> Result<TempDir, Box<dyn std::error::Error>> {
    let temp_dir = tempdir()?;
    let base_path = temp_dir.path();

    // Create files
    fs::write(base_path.join("file1.txt"), "Hello, world!")?;
    fs::write(base_path.join("file2.rs"), "fn main() { println!(\"test\"); }")?;
    fs::write(base_path.join(".hidden_file"), "hidden content")?;
    fs::write(base_path.join("large_file.log"), "x".repeat(1024))?;
    fs::write(base_path.join("image.png"), "")?; // Empty file representing an image

    // Create subdirectories
    fs::create_dir_all(base_path.join("subdir1"))?;
    fs::create_dir_all(base_path.join("subdir2").join("empty"))?;

    // Create nested files
    fs::write(base_path.join("subdir1").join("nested_file.md"), "# Test\nThis is a test markdown file.")?;
    fs::write(base_path.join("subdir1").join("another_file.py"), "print('hello from python')")?;

    Ok(temp_dir)
}

/// Creates a minimal temporary directory with just a few files
pub fn setup_simple_test_directory() -> Result<TempDir, Box<dyn std::error::Error>> {
    let temp_dir = tempdir()?;
    let base_path = temp_dir.path();

    fs::write(base_path.join("test.txt"), "test content")?;
    fs::write(base_path.join("another.rs"), "// Rust code")?;
    fs::create_dir(base_path.join("directory"))?;

    Ok(temp_dir)
}

/// Helper function to create a test file with specific content
pub fn create_test_file<P: AsRef<Path>>(path: P, content: &str) -> Result<(), Box<dyn std::error::Error>> {
    fs::write(path, content)?;
    Ok(())
}

/// Helper function to get all file paths in a directory recursively
pub fn get_all_paths_in_dir<P: AsRef<Path>>(dir: P) -> Result<Vec<PathBuf>, Box<dyn std::error::Error>> {
    let mut paths = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            let mut sub_paths = get_all_paths_in_dir(&path)?;
            paths.append(&mut sub_paths);
        }
        paths.push(path);
    }
    paths.sort();
    Ok(paths)
}

/// Create a test configuration for testing configuration functionality
pub fn create_test_config_content() -> serde_json::Value {
    serde_json::json!({
        "start_path": "/tmp/test",
        "ignore_directories": ["node_modules", ".git", "target"],
        "root_dir": "/tmp",
        "cache_directory": "/tmp/.config/ff/cache.json",
        "settings_path": "/tmp/.config/ff/settings.json"
    })
}

/// Helper to assert that two path lists are equivalent (ignoring order)
pub fn assert_paths_equal(actual: &[PathBuf], expected: &[PathBuf]) {
    let mut actual_sorted = actual.to_vec();
    let mut expected_sorted = expected.to_vec();
    actual_sorted.sort();
    expected_sorted.sort();
    
    assert_eq!(actual_sorted, expected_sorted, "Path lists don't match");
}

/// Mock directory store with predefined content for testing
pub fn create_mock_directory_store() -> file_finder::directory_store::DirectoryStore {
    let mut store = file_finder::directory_store::DirectoryStore::new();
    store.insert("/home/user/Documents");
    store.insert("/home/user/Pictures");  
    store.insert("/home/user/Downloads");
    store.insert("/home/user/Desktop/projects");
    store.insert("/home/user/Desktop/notes");
    store
}
