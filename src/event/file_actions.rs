use std::path::{Path, PathBuf};

use crate::{
    app::App,
    event::navigation::parent_directory,
    utils::get_file_path_data,
};

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

pub fn refresh_file_list(app: &mut App, directory: &Path) -> anyhow::Result<()> {
    let file_path_list = get_file_path_data(
        directory.to_string_lossy().to_string(),
        app.show_hidden_files,
        app.sort_by.clone(),
        &app.sort_type,
    )?;

    app.files = file_path_list.clone();
    app.read_only_files = file_path_list;
    app.update_file_references();

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    use crate::utils::{SortBy, SortType};

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
    fn containing_directory_or_current_preserves_root_directory() {
        assert_eq!(
            containing_directory_or_current(Path::new("/")),
            PathBuf::from("/")
        );
    }

    #[test]
    fn refresh_file_list_preserves_current_sort_preferences() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        fs::write(temp_dir.path().join("apple.txt"), "small").expect("write apple");
        fs::write(temp_dir.path().join("zebra.txt"), "larger content").expect("write zebra");

        let mut app = App::new(Vec::new());
        app.sort_by = SortBy::Name;
        app.sort_type = SortType::DESC;

        refresh_file_list(&mut app, temp_dir.path()).expect("refresh");

        let names = app
            .files
            .iter()
            .map(|path| {
                Path::new(path)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap()
                    .to_string()
            })
            .collect::<Vec<_>>();

        assert_eq!(names, vec!["zebra.txt", "apple.txt"]);
        assert_eq!(app.read_only_files, app.files);
    }
}
