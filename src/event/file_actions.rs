use std::path::{Path, PathBuf};

use log::debug;
use ratatui::widgets::ListState;

use crate::{
    app::{App, InputMode},
    event::navigation::parent_directory,
    status_bar::StatusBar,
    utils::{get_curr_path, get_file_path_data},
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

/// Select the visible entry for a possibly nested created name.
pub fn created_item_path(directory: &Path, name: &str) -> Option<String> {
    Path::new(name)
        .components()
        .next()
        .map(|component| directory.join(component).to_string_lossy().into_owned())
}

/// Directory to reload the left listing from; the right pane refreshes itself.
pub(crate) fn current_refresh_directory(app: &App, fallback_start_path: &str) -> String {
    let current_directory = app.left_directory_path();

    if current_directory == Path::new(".") {
        fallback_start_path.to_string()
    } else {
        current_directory.to_string_lossy().to_string()
    }
}

/// Reload the left listing after an operation. A failed read shows an error and keeps
/// the old listing, so it never ends ff.
pub fn refresh_file_list(
    app: &mut App,
    directory: &Path,
    state: &mut ListState,
    select_path: Option<String>,
    status_bar: &mut StatusBar,
) {
    match get_file_path_data(
        directory.to_string_lossy().to_string(),
        app.show_hidden_files,
        app.sort_by.clone(),
        &app.sort_type,
    ) {
        Ok(files) => app.refresh_files_after_operation(files, state, select_path),
        Err(e) => status_bar.show_error(
            format!("Could not refresh '{}': {e:#}", directory.display()),
            None,
        ),
    }
}

/// Browse `directory` in the left listing, as `l` does: leave any search and move
/// `current_directory`, the watcher and `prev_dir` there. Highlights `select_path`,
/// else the first row. A failed read changes nothing.
pub fn enter_directory(
    app: &mut App,
    directory: &str,
    state: &mut ListState,
    select_path: Option<String>,
) -> anyhow::Result<()> {
    let files = get_file_path_data(
        directory.to_string(),
        app.show_hidden_files,
        app.sort_by.clone(),
        &app.sort_type,
    )?;
    app.input_mode = InputMode::Normal;
    app.prev_dir = get_curr_path(directory.to_string());
    app.preserved_selection_index = Some(0);
    app.refresh_files_after_operation(files, state, select_path);
    if let Err(e) = app.start_watching_directory(directory) {
        debug!("Failed to start watching directory '{}': {}", directory, e);
    } else {
        debug!("Started watching directory: {}", directory);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    use crate::app::{PanePosition, ViewMode};
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
        // A nested name highlights its first folder, the entry visible here.
        for (name, visible) in [
            ("zulu.txt", "zulu.txt"),
            ("zulu", "zulu"),
            ("zulu/", "zulu"),
            ("zulu/todo.md", "zulu"),
        ] {
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
                &mut StatusBar::new(),
            );

            assert_eq!(state.selected(), Some(1));
            assert_eq!(
                app.get_selected_path(state.selected()),
                Some(dir.path().join(visible).to_string_lossy().into_owned())
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
            &mut StatusBar::new(),
        );
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
            &mut StatusBar::new(),
        );
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

        refresh_file_list(
            &mut app,
            temp_dir.path(),
            &mut ListState::default(),
            None,
            &mut StatusBar::new(),
        );

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

    #[test]
    fn refresh_directory_prefers_watched_current_directory_when_list_empty() {
        let mut app = App::new(Vec::new());
        app.current_directory = "/tmp/ff-empty-current".to_string();

        assert_eq!(
            current_refresh_directory(&app, "/tmp/ff-settings-start"),
            "/tmp/ff-empty-current"
        );
    }

    #[test]
    fn refresh_directory_prefers_tracked_current_directory_when_available() {
        let mut app = App::new(vec!["/tmp/ff-visible/file.txt".to_string()]);
        app.current_directory = "/tmp/ff-current".to_string();

        assert_eq!(
            current_refresh_directory(&app, "/tmp/ff-settings-start"),
            "/tmp/ff-current"
        );
    }

    #[test]
    fn refresh_directory_stays_on_left_listing_while_right_pane_is_active() {
        let mut app = App::new(vec!["/tmp/ff-left/file.txt".to_string()]);
        app.current_directory = "/tmp/ff-left".to_string();
        app.view_mode = ViewMode::DualPane;
        app.active_pane = PanePosition::Right;
        app.right_pane = crate::pane::Pane::new("/tmp/ff-right".to_string(), Vec::new());

        assert_eq!(
            current_refresh_directory(&app, "/tmp/ff-settings-start"),
            "/tmp/ff-left"
        );
    }
}
