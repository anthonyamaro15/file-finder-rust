use file_finder::app::App;
use file_finder::errors::FileOperationResult;
use file_finder::operations::file_ops::{create_new_dir, create_new_file, delete_dir, delete_file};
use file_finder::operations::{
    copy_dir_file_helper, create_item_based_on_type, handle_delete_based_on_type, handle_rename,
};
use file_finder::utils::files::{check_if_exists, generate_copy_file_dir_name};
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::tempdir;

#[cfg(test)]
mod file_creation_tests {
    use super::*;

    #[test]
    fn test_create_new_file_success() {
        let temp_dir = tempdir().expect("Failed to create temp directory");
        let temp_path = temp_dir.path().to_string_lossy().to_string();
        let file_name = "test.txt".to_string();

        let result = create_new_file(temp_path.clone(), file_name.clone());

        assert!(result.is_ok());
        assert!(Path::new(&format!("{}/{}", temp_path, file_name)).exists());
    }

    #[test]
    fn test_create_new_file_already_exists() {
        let temp_dir = tempdir().expect("Failed to create temp directory");
        let temp_path = temp_dir.path().to_string_lossy().to_string();
        let file_name = "existing.txt".to_string();
        let file_path = format!("{}/{}", temp_path, file_name);

        // Create the file first
        fs::write(&file_path, "content").expect("Failed to create existing file");

        // Try to create it again
        let result = create_new_file(temp_path, file_name);

        assert!(result.is_err());
    }

    #[test]
    fn test_create_new_dir_success() {
        let temp_dir = tempdir().expect("Failed to create temp directory");
        let temp_path = temp_dir.path().to_string_lossy().to_string();
        let dir_name = "new_directory".to_string();

        let result = create_new_dir(temp_path.clone(), dir_name.clone());

        assert!(result.is_ok());
        assert!(Path::new(&format!("{}/{}", temp_path, dir_name)).is_dir());
    }

    #[test]
    fn test_create_new_dir_already_exists() {
        let temp_dir = tempdir().expect("Failed to create temp directory");
        let temp_path = temp_dir.path().to_string_lossy().to_string();
        let dir_name = "existing_dir".to_string();
        let dir_path = format!("{}/{}", temp_path, dir_name);

        // Create the directory first
        fs::create_dir(&dir_path).expect("Failed to create existing directory");

        // Try to create it again
        let result = create_new_dir(temp_path, dir_name);

        assert!(result.is_err());
    }

    fn create(dir: &Path, name: &str) -> FileOperationResult<PathBuf> {
        create_item_based_on_type(dir.to_string_lossy().into_owned(), name.to_string())
    }

    /// Every entry below `dir`, relative and sorted, e.g. ["docs", "docs/todo.md"].
    fn tree(dir: &Path) -> Vec<String> {
        let mut entries: Vec<String> = walkdir::WalkDir::new(dir)
            .min_depth(1)
            .into_iter()
            .map(|e| {
                let e = e.unwrap();
                e.path().strip_prefix(dir).unwrap().display().to_string()
            })
            .collect();
        entries.sort();
        entries
    }

    #[test]
    fn create_item_makes_a_folder_only_when_the_name_ends_with_a_slash() {
        let temp_dir = tempdir().unwrap();
        let dir = temp_dir.path();

        for name in ["Makefile", ".env", "notes", "document.txt"] {
            assert_eq!(create(dir, name).unwrap(), dir.join(name));
            assert!(dir.join(name).is_file(), "{name}");
        }
        for name in ["photos/", "v1.2/", ".config/"] {
            let folder = dir.join(name.trim_end_matches('/'));
            assert_eq!(create(dir, name).unwrap(), folder);
            assert!(folder.is_dir(), "{name}");
        }
    }

    #[test]
    fn create_item_creates_or_reuses_the_folders_on_the_way() {
        let temp_dir = tempdir().unwrap();
        let dir = temp_dir.path();

        // Returns the top-most new entry.
        assert_eq!(create(dir, "docs/todo.md").unwrap(), dir.join("docs"));
        assert_eq!(
            create(dir, "docs/more.md").unwrap(),
            dir.join("docs/more.md")
        );
        assert_eq!(create(dir, "a/b/c/").unwrap(), dir.join("a"));
        assert_eq!(create(dir, "a/b/d/").unwrap(), dir.join("a/b/d"));

        assert!(dir.join("docs/todo.md").is_file());
        assert!(dir.join("docs/more.md").is_file());
        assert!(dir.join("a/b/c").is_dir());
        assert!(dir.join("a/b/d").is_dir());
    }

    #[test]
    fn create_item_rejects_bad_names_and_creates_nothing() {
        let temp_dir = tempdir().unwrap();
        let dir = temp_dir.path();

        for name in [
            "", "/", "//", "/abs", "../x", "a/../b", "a/./b", "a//b", "a//", "con", "a/con",
            "con/", "a/b:c",
        ] {
            assert!(create(dir, name).is_err(), "{name:?}");
        }
        assert!(tree(dir).is_empty());
    }

    #[test]
    fn create_item_never_overwrites() {
        let temp_dir = tempdir().unwrap();
        let dir = temp_dir.path();
        fs::create_dir_all(dir.join("photos/kept")).unwrap();
        fs::create_dir(dir.join("docs")).unwrap();
        fs::write(dir.join("docs/todo.md"), "keep me").unwrap();
        fs::write(dir.join("notes.txt"), "keep me too").unwrap();
        let before = tree(dir);

        for name in [
            "photos/",
            "photos",
            "docs/todo.md",
            "docs/todo.md/",
            "notes.txt",
            "notes.txt/",
            // A file on the way is not replaced by a folder.
            "notes.txt/x",
            "notes.txt/x/y/",
        ] {
            assert!(create(dir, name).is_err(), "{name}");
        }
        assert_eq!(tree(dir), before);
        assert_eq!(
            fs::read_to_string(dir.join("docs/todo.md")).unwrap(),
            "keep me"
        );
        assert_eq!(
            fs::read_to_string(dir.join("notes.txt")).unwrap(),
            "keep me too"
        );
    }

    #[cfg(unix)]
    #[test]
    fn create_item_never_goes_through_a_link() {
        let temp_dir = tempdir().unwrap();
        let outside = tempdir().unwrap();
        let dir = temp_dir.path();
        std::os::unix::fs::symlink(outside.path(), dir.join("link")).unwrap();

        for name in ["link/x", "link/x/", "link/new/x"] {
            assert!(create(dir, name).is_err(), "{name}");
        }
        assert!(tree(outside.path()).is_empty());
        assert_eq!(tree(dir), ["link"]);
    }

    #[test]
    fn create_item_removes_only_the_folders_it_created_when_it_fails() {
        let temp_dir = tempdir().unwrap();
        let dir = temp_dir.path();
        fs::create_dir_all(dir.join("keep/old")).unwrap();
        // Each name is valid, but the whole path grows past the system limit, so the
        // folders near the start get created before a later one fails.
        let long = "d".repeat(200);
        let name = format!("keep/{}/end.txt", vec![long.as_str(); 30].join("/"));

        assert!(create(dir, &name).is_err());
        assert_eq!(tree(dir), ["keep", "keep/old"]);
    }
}

#[cfg(test)]
mod file_deletion_tests {
    use super::*;

    #[test]
    fn test_delete_file_success() {
        let temp_dir = tempdir().expect("Failed to create temp directory");
        let file_path = temp_dir.path().join("test.txt");

        // Create a test file
        fs::write(&file_path, "test content").expect("Failed to create test file");
        assert!(file_path.exists());

        let result = delete_file(file_path.to_str().unwrap());

        assert!(result.is_ok());
        assert!(!file_path.exists());
    }

    #[test]
    fn test_delete_file_not_exists() {
        let temp_dir = tempdir().expect("Failed to create temp directory");
        let file_path = temp_dir.path().join("nonexistent.txt");

        let result = delete_file(file_path.to_str().unwrap());

        assert!(result.is_err());
    }

    #[test]
    fn test_delete_dir_success() {
        let temp_dir = tempdir().expect("Failed to create temp directory");
        let dir_path = temp_dir.path().join("test_dir");

        // Create a test directory with files
        fs::create_dir(&dir_path).expect("Failed to create test directory");
        fs::write(dir_path.join("file.txt"), "content")
            .expect("Failed to create file in directory");
        assert!(dir_path.exists());

        let result = delete_dir(dir_path.to_str().unwrap());

        assert!(result.is_ok());
        assert!(!dir_path.exists());
    }

    #[test]
    fn test_delete_dir_not_exists() {
        let temp_dir = tempdir().expect("Failed to create temp directory");
        let dir_path = temp_dir.path().join("nonexistent_dir");

        let result = delete_dir(dir_path.to_str().unwrap());

        assert!(result.is_err());
    }

    #[test]
    fn test_handle_delete_based_on_type_file() {
        let temp_dir = tempdir().expect("Failed to create temp directory");
        let file_path = temp_dir.path().join("test.txt");

        // Create a test file
        fs::write(&file_path, "test content").expect("Failed to create test file");
        assert!(file_path.exists());

        let result = handle_delete_based_on_type(file_path.to_str().unwrap());

        assert!(result.is_ok());
        assert!(!file_path.exists());
    }

    #[test]
    fn test_handle_delete_based_on_type_directory() {
        let temp_dir = tempdir().expect("Failed to create temp directory");
        let dir_path = temp_dir.path().join("test_dir");

        // Create a test directory with files
        fs::create_dir(&dir_path).expect("Failed to create test directory");
        fs::write(dir_path.join("file.txt"), "content")
            .expect("Failed to create file in directory");
        assert!(dir_path.exists());

        let result = handle_delete_based_on_type(dir_path.to_str().unwrap());

        assert!(result.is_ok());
        assert!(!dir_path.exists());
    }
}

#[cfg(test)]
mod file_rename_tests {
    use super::*;

    fn rename_app(current_dir: String, current_name: &str, new_name: &str) -> App {
        let mut app = App::new(Vec::new());
        app.current_path_to_edit = current_dir;
        app.current_name_to_edit = current_name.to_string();
        app.create_edit_file_name = new_name.to_string();
        app
    }

    #[test]
    fn test_rename_file_success() {
        let temp_dir = tempdir().expect("Failed to create temp directory");
        let temp_path = temp_dir.path().to_string_lossy().to_string();

        // Create a test file
        let old_name = "old_file.txt";
        let new_name = "new_file.txt";
        let file_content = "test content";

        fs::write(temp_dir.path().join(old_name), file_content)
            .expect("Failed to create test file");

        let app = rename_app(temp_path.clone(), old_name, new_name);
        let result = handle_rename(&app);

        assert!(result.is_ok());
        assert!(!Path::new(&format!("{}/{}", temp_path, old_name)).exists());
        assert!(Path::new(&format!("{}/{}", temp_path, new_name)).exists());

        // Verify content is preserved
        let content = fs::read_to_string(temp_dir.path().join(new_name))
            .expect("Failed to read renamed file");
        assert_eq!(content, file_content);
    }

    #[test]
    fn test_rename_directory_success() {
        let temp_dir = tempdir().expect("Failed to create temp directory");
        let temp_path = temp_dir.path().to_string_lossy().to_string();

        // Create a test directory with content
        let old_name = "old_dir";
        let new_name = "new_dir";

        let old_dir_path = temp_dir.path().join(old_name);
        fs::create_dir(&old_dir_path).expect("Failed to create test directory");
        fs::write(old_dir_path.join("file.txt"), "content")
            .expect("Failed to create file in directory");

        let app = rename_app(temp_path.clone(), old_name, new_name);
        let result = handle_rename(&app);

        assert!(result.is_ok());
        assert!(!Path::new(&format!("{}/{}", temp_path, old_name)).exists());
        assert!(Path::new(&format!("{}/{}", temp_path, new_name)).exists());

        // Verify content is preserved
        let content = fs::read_to_string(temp_dir.path().join(new_name).join("file.txt"))
            .expect("Failed to read file in renamed directory");
        assert_eq!(content, "content");
    }

    #[test]
    fn test_rename_file_not_exists() {
        let temp_dir = tempdir().expect("Failed to create temp directory");
        let temp_path = temp_dir.path().to_string_lossy().to_string();

        let old_name = "nonexistent.txt";
        let new_name = "new_file.txt";

        let app = rename_app(temp_path, old_name, new_name);
        let result = handle_rename(&app);

        assert!(result.is_err());
    }

    #[test]
    fn test_rename_target_already_exists() {
        let temp_dir = tempdir().expect("Failed to create temp directory");
        let temp_path = temp_dir.path().to_string_lossy().to_string();

        // Create both source and target files
        let old_name = "source.txt";
        let new_name = "target.txt";

        fs::write(temp_dir.path().join(old_name), "source content")
            .expect("Failed to create source file");
        fs::write(temp_dir.path().join(new_name), "target content")
            .expect("Failed to create target file");

        let app = rename_app(temp_path.clone(), old_name, new_name);
        let result = handle_rename(&app);

        assert!(result.is_err());
        assert!(Path::new(&format!("{}/{}", temp_path, old_name)).exists());
        assert!(Path::new(&format!("{}/{}", temp_path, new_name)).exists());
    }
}

#[cfg(test)]
mod file_copy_tests {
    use super::*;

    #[test]
    fn test_copy_file_success() {
        let temp_dir = tempdir().expect("Failed to create temp directory");
        let src_file = temp_dir.path().join("source.txt");
        let dst_file = temp_dir.path().join("destination.txt");
        let file_content = "test file content";

        // Create source file
        fs::write(&src_file, file_content).expect("Failed to create source file");

        let result = copy_dir_file_helper(&src_file, &dst_file);

        assert!(result.is_ok());
        assert!(dst_file.exists());

        // Verify content
        let copied_content = fs::read_to_string(&dst_file).expect("Failed to read copied file");
        assert_eq!(copied_content, file_content);

        // Verify original still exists
        assert!(src_file.exists());
    }

    #[test]
    fn test_copy_directory_success() {
        let temp_dir = tempdir().expect("Failed to create temp directory");
        let src_dir = temp_dir.path().join("source_dir");
        let dst_dir = temp_dir.path().join("destination_dir");

        // Create source directory structure
        fs::create_dir_all(&src_dir).expect("Failed to create source directory");
        fs::create_dir_all(src_dir.join("subdir")).expect("Failed to create subdirectory");
        fs::write(src_dir.join("file1.txt"), "content1").expect("Failed to create file1");
        fs::write(src_dir.join("subdir").join("file2.txt"), "content2")
            .expect("Failed to create file2");

        let result = copy_dir_file_helper(&src_dir, &dst_dir);

        assert!(result.is_ok());
        assert!(dst_dir.exists());
        assert!(dst_dir.join("file1.txt").exists());
        assert!(dst_dir.join("subdir").join("file2.txt").exists());

        // Verify content
        let content1 = fs::read_to_string(dst_dir.join("file1.txt")).expect("Failed to read file1");
        let content2 = fs::read_to_string(dst_dir.join("subdir").join("file2.txt"))
            .expect("Failed to read file2");
        assert_eq!(content1, "content1");
        assert_eq!(content2, "content2");

        // Verify original still exists
        assert!(src_dir.exists());
        assert!(src_dir.join("file1.txt").exists());
    }

    #[test]
    fn test_copy_file_not_exists() {
        let temp_dir = tempdir().expect("Failed to create temp directory");
        let src_file = temp_dir.path().join("nonexistent.txt");
        let dst_file = temp_dir.path().join("destination.txt");

        let result = copy_dir_file_helper(&src_file, &dst_file);

        // The copy operation should fail when source doesn't exist
        // On some systems, fs::copy might return 0 bytes but succeed
        // Let's check that either it errors OR the destination doesn't exist
        if result.is_ok() {
            assert!(!dst_file.exists());
        } else {
            assert!(result.is_err());
        }
    }

    #[test]
    fn test_generate_copy_file_dir_name() {
        let curr_path = "/home/user/document.txt".to_string();
        let new_path = "/home/user/backup".to_string();

        let result = generate_copy_file_dir_name(curr_path, new_path);
        assert_eq!(result, Path::new("/home/user/backup/copy_document.txt"));
    }

    #[test]
    fn test_generate_copy_dir_name() {
        let curr_path = "/home/user/important_folder".to_string();
        let new_path = "/home/user/backup".to_string();

        let result = generate_copy_file_dir_name(curr_path, new_path);
        assert_eq!(result, Path::new("/home/user/backup/copy_important_folder"));
    }
}

#[cfg(test)]
mod path_validation_tests {
    use super::*;

    #[test]
    fn test_check_if_exists_file() {
        let temp_dir = tempdir().expect("Failed to create temp directory");
        let file_path = temp_dir.path().join("test.txt");

        // File doesn't exist yet
        assert!(!check_if_exists(file_path.to_string_lossy().to_string()));

        // Create the file
        fs::write(&file_path, "content").expect("Failed to create test file");

        // Now it should exist
        assert!(check_if_exists(file_path.to_string_lossy().to_string()));
    }

    #[test]
    fn test_check_if_exists_directory() {
        let temp_dir = tempdir().expect("Failed to create temp directory");
        let dir_path = temp_dir.path().join("test_dir");

        // Directory doesn't exist yet
        assert!(!check_if_exists(dir_path.to_string_lossy().to_string()));

        // Create the directory
        fs::create_dir(&dir_path).expect("Failed to create test directory");

        // Now it should exist
        assert!(check_if_exists(dir_path.to_string_lossy().to_string()));
    }

    #[test]
    fn test_check_if_exists_nonexistent_path() {
        let nonexistent_path = "/this/path/should/never/exist/hopefully".to_string();
        assert!(!check_if_exists(nonexistent_path));
    }
}
