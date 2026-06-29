use std::path::{Path, PathBuf};

use crate::event::navigation::parent_directory;

pub fn containing_directory(path: &Path) -> PathBuf {
    parent_directory(path).unwrap_or_default()
}

pub fn containing_directory_or_current(path: &Path) -> PathBuf {
    parent_directory(path).unwrap_or_else(|| PathBuf::from("."))
}

pub fn create_target_directory(selected: &Path) -> PathBuf {
    if selected.is_dir() {
        selected.to_path_buf()
    } else {
        containing_directory(selected)
    }
}

pub fn rename_target(path: &Path) -> Option<(PathBuf, String)> {
    let parent = containing_directory(path);
    let name = path.file_name()?.to_str()?.to_string();

    Some((parent, name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn containing_directory_returns_parent_path() {
        assert_eq!(
            containing_directory(Path::new("/tmp/example/file.txt")),
            PathBuf::from("/tmp/example")
        );
    }

    #[test]
    fn rename_target_returns_parent_and_file_name() {
        assert_eq!(
            rename_target(Path::new("/tmp/example/file.txt")),
            Some((PathBuf::from("/tmp/example"), "file.txt".to_string()))
        );
    }

    #[test]
    fn containing_directory_or_current_falls_back_to_current_directory() {
        assert_eq!(containing_directory_or_current(Path::new("/")), PathBuf::from("."));
    }
}
