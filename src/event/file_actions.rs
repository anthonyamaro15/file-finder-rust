use std::path::{Path, PathBuf};

use ratatui::widgets::ListState;

use crate::{app::App, event::navigation::parent_directory, utils::get_file_path_data};

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

/// Select the visible entry for a possibly nested created name.
pub fn created_item_path(directory: &Path, name: &str) -> Option<String> {
    Path::new(name)
        .components()
        .next()
        .map(|component| directory.join(component).to_string_lossy().into_owned())
}

pub fn refresh_file_list(
    app: &mut App,
    directory: &Path,
    state: &mut ListState,
    select_path: Option<String>,
) -> anyhow::Result<()> {
    let file_path_list = get_file_path_data(
        directory.to_string_lossy().to_string(),
        app.show_hidden_files,
        app.sort_by.clone(),
        &app.sort_type,
    )?;

    app.refresh_files_after_operation(file_path_list, state, select_path);

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
    fn create_refresh_selects_the_new_file_or_folder() {
        for name in ["zulu.txt", "zulu"] {
            let dir = tempfile::tempdir().unwrap();
            fs::write(dir.path().join("alpha.txt"), "existing").unwrap();
            let mut app = App::new(Vec::new());
            app.sort_by = SortBy::Name;
            app.sort_type = SortType::ASC;
            let mut state = ratatui::widgets::ListState::default();
            state.select(Some(0));
            crate::operations::create_item_based_on_type(
                dir.path().to_string_lossy().into_owned(),
                name.into(),
            )
            .unwrap();

            refresh_file_list(
                &mut app,
                dir.path(),
                &mut state,
                created_item_path(dir.path(), name),
            )
            .unwrap();

            assert_eq!(state.selected(), Some(1));
            assert_eq!(
                app.get_selected_path(state.selected()),
                Some(dir.path().join(name).to_string_lossy().into_owned())
            );
        }
    }

    #[test]
    fn create_refresh_selects_first_component_and_preserves_selection_for_hidden_files() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("alpha.txt"), "existing").unwrap();
        fs::create_dir_all(dir.path().join("zulu/child")).unwrap();
        let mut app = App::new(Vec::new());
        app.sort_by = SortBy::Name;
        app.sort_type = SortType::ASC;
        let mut state = ratatui::widgets::ListState::default();
        state.select(Some(0));

        refresh_file_list(
            &mut app,
            dir.path(),
            &mut state,
            created_item_path(dir.path(), "zulu/child"),
        )
        .unwrap();
        assert_eq!(state.selected(), Some(1));
        assert_eq!(
            app.get_selected_path(state.selected()),
            Some(dir.path().join("zulu").to_string_lossy().into_owned())
        );

        crate::operations::create_item_based_on_type(
            dir.path().to_string_lossy().into_owned(),
            ".hidden.txt".into(),
        )
        .unwrap();
        refresh_file_list(
            &mut app,
            dir.path(),
            &mut state,
            created_item_path(dir.path(), ".hidden.txt"),
        )
        .unwrap();
        assert_eq!(state.selected(), Some(1));
        assert_eq!(app.files.len(), 2);
    }

    #[test]
    fn refresh_file_list_preserves_current_sort_preferences() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        fs::write(temp_dir.path().join("apple.txt"), "small").expect("write apple");
        fs::write(temp_dir.path().join("zebra.txt"), "larger content").expect("write zebra");

        let mut app = App::new(Vec::new());
        app.sort_by = SortBy::Name;
        app.sort_type = SortType::DESC;

        refresh_file_list(&mut app, temp_dir.path(), &mut ListState::default(), None)
            .expect("refresh");

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
