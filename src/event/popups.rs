//! Key handling for the popup modes: rename, create, delete confirmation, sort and
//! the keybinding help.

use std::{io, path::Path};

use crossterm::event::{KeyCode, KeyEvent};
use log::warn;
use ratatui::{backend::CrosstermBackend, widgets::ListState, Terminal};

use crate::{
    app::{App, InputMode},
    directory_store::DirectoryIndex,
    event::{
        file_actions::{
            create_target_directory, created_item_path, current_refresh_directory,
            refresh_file_list,
        },
        sort::apply_sort,
        LoopState,
    },
    exit::may_exit,
    operations::{
        create_item_based_on_type, delete_with_progress, handle_delete_based_on_type, handle_rename,
    },
    status_bar::StatusBar,
    utils::{check_if_exists, get_file_path_data, SortBy, SortType},
};

/// Rename mode: edit the new name, then rename on Enter.
pub fn handle_rename_key(
    key: KeyEvent,
    app: &mut App,
    state: &mut ListState,
    status_bar: &mut StatusBar,
    index: &mut DirectoryIndex,
    fallback_start_path: &str,
) -> anyhow::Result<()> {
    match key.code {
        KeyCode::Char(c) => {
            app.add_char(c);
        }
        KeyCode::Backspace => {
            app.delete_c();
        }
        KeyCode::Left => {
            app.move_create_edit_cursor_left();
        }
        KeyCode::Right => {
            app.move_create_edit_cursor_right();
        }
        KeyCode::Esc => {
            app.input_mode = InputMode::Normal;
            app.reset_create_edit_values();
        }
        KeyCode::Enter => {
            // rename file to new name
            // validate tha the new name and the previous name are not the same,
            // if names are equal then exit the current mode
            if app.create_edit_file_name == app.current_name_to_edit {
                app.input_mode = InputMode::Normal;
                app.reset_create_edit_values();
            } else {
                // proceed with operation
                let new_path =
                    format!("{}/{}", app.current_path_to_edit, app.create_edit_file_name);
                if !check_if_exists(new_path.clone()) {
                    match handle_rename(app) {
                        Ok(_) => {
                            let old_path = Path::new(&app.current_path_to_edit)
                                .join(&app.current_name_to_edit);
                            index.renamed(&old_path, Path::new(&new_path));
                            // Only the pane it was renamed in moves its highlight to it.
                            let from_right_pane =
                                app.view_mode.is_dual_pane() && app.is_right_pane_active();
                            if app.view_mode.is_dual_pane() {
                                app.right_pane.refresh_files();
                                if from_right_pane {
                                    app.right_pane.select_path(&new_path);
                                }
                            }
                            let current_dir = current_refresh_directory(app, fallback_start_path);
                            app.reset_create_edit_values();
                            let file_path_list = get_file_path_data(
                                current_dir,
                                app.show_hidden_files,
                                app.sort_by.clone(),
                                &app.sort_type,
                            )?;
                            app.refresh_files_after_operation(
                                file_path_list,
                                state,
                                (!from_right_pane).then_some(new_path),
                            );
                            status_bar.invalidate_cache();
                            app.input_mode = InputMode::Normal;
                        }
                        Err(e) => {
                            app.is_create_edit_error = true;
                            app.error_message = e.user_message();
                        }
                    }
                } else {
                    app.is_create_edit_error = true;
                    app.error_message = "Already exist".to_string();
                }
            }
        }
        _ => {}
    }
    Ok(())
}

/// Create mode: edit the name, then create the file or directory on Enter.
pub fn handle_create_key(
    key: KeyEvent,
    app: &mut App,
    state: &mut ListState,
    status_bar: &mut StatusBar,
    index: &mut DirectoryIndex,
    fallback_start_path: &str,
) -> anyhow::Result<()> {
    match key.code {
        KeyCode::Char(c) => {
            app.add_char(c);
        }
        KeyCode::Backspace => {
            app.delete_c();
        }
        KeyCode::Left => {
            app.move_create_edit_cursor_left();
        }
        KeyCode::Right => {
            app.move_create_edit_cursor_right();
        }
        KeyCode::Esc => {
            app.input_mode = InputMode::Normal;
            app.reset_create_edit_values();
            // create methods to reset the create_edit_file_name and state of it
        }
        KeyCode::Enter if !app.create_edit_file_name.is_empty() => {
            // create file/dir
            // Create inside a highlighted directory, beside a file,
            // or in the browsed directory when no item is selected.
            let new_path = app
                .active_selected_path(state.selected())
                .map(|selected| create_target_directory(Path::new(&selected)))
                .unwrap_or_else(|| app.current_directory_path())
                .to_string_lossy()
                .to_string();
            match create_item_based_on_type(new_path.clone(), app.create_edit_file_name.clone()) {
                Ok(_) => {
                    app.input_mode = InputMode::Normal;
                    index.changed(&Path::new(&new_path).join(&app.create_edit_file_name));

                    let created_path =
                        created_item_path(Path::new(&new_path), &app.create_edit_file_name);
                    app.reset_create_edit_values();
                    let from_right_pane =
                        app.view_mode.is_dual_pane() && app.is_right_pane_active();
                    if app.view_mode.is_dual_pane() {
                        app.right_pane.refresh_files();
                    }
                    if from_right_pane {
                        if let Some(created_path) = created_path {
                            app.right_pane.select_path(&created_path);
                        }
                        // The left listing stays in its own folder.
                        let left_dir = current_refresh_directory(app, fallback_start_path);
                        refresh_file_list(app, Path::new(&left_dir), state, None)?;
                    } else {
                        refresh_file_list(app, Path::new(&new_path), state, created_path)?;
                    }
                    status_bar.invalidate_cache();
                }
                Err(e) => {
                    app.is_create_edit_error = true;
                    app.error_message = e.user_message();
                }
            }
        }
        _ => {}
    }
    Ok(())
}

/// Delete confirmation. Returns true when `q` confirmed quitting, so the loop should end.
pub fn handle_delete_key(
    key: KeyEvent,
    loop_state: &mut LoopState,
    terminal: &mut Terminal<CrosstermBackend<impl io::Write>>,
    fallback_start_path: &str,
) -> anyhow::Result<bool> {
    let LoopState {
        app,
        state,
        status_bar,
        copy_receiver,
        delete_receiver,
        quit_error,
        ..
    } = loop_state;
    match key.code {
        KeyCode::Char('q') => {
            app.close_delete_popup();
            if may_exit(app, status_bar, "q", copy_receiver, terminal, quit_error) {
                return Ok(true);
            }
        }
        KeyCode::Char('n') | KeyCode::Esc => {
            app.close_delete_popup();
        }
        KeyCode::Char('y') => {
            let to_trash = app.input_mode == InputMode::WatchTrash;
            if let Some(selected) = app.close_delete_popup() {
                if to_trash {
                    if let Err(warning) = start_trash(
                        app,
                        state.selected(),
                        selected,
                        crate::operations::trash::move_to_trash,
                    ) {
                        status_bar.show_warning(warning, None);
                    }
                    return Ok(false);
                }
                let path = Path::new(&selected);
                if path
                    .symlink_metadata()
                    .map(|metadata| metadata.is_dir())
                    .unwrap_or(false)
                {
                    // Folders delete in the background, one operation at a time.
                    if let Some(warning) = app.busy_warning() {
                        status_bar.show_warning(warning, None);
                    } else {
                        app.delete_in_progress = true;
                        app.delete_progress_message = "Counting files...".to_string();
                        app.preserved_selection_index = state.selected();
                        *delete_receiver = Some(delete_with_progress(&selected));
                        app.delete_path = Some(selected);
                    }
                } else {
                    // A file removed externally already satisfies the request.
                    let delete_result = handle_delete_based_on_type(&selected);
                    let is_success = match &delete_result {
                        Ok(_) => true,
                        Err(e) => e.is_not_found(),
                    };
                    if is_success {
                        if app.view_mode.is_dual_pane() {
                            app.right_pane.refresh_files();
                        }
                        let current_dir = current_refresh_directory(app, fallback_start_path);
                        let files = get_file_path_data(
                            current_dir,
                            app.show_hidden_files,
                            app.sort_by.clone(),
                            &app.sort_type,
                        )?;
                        app.refresh_files_after_operation(files, state, None);
                        status_bar.invalidate_cache();
                    } else if let Err(e) = delete_result {
                        status_bar.show_error(e.user_message(), None);
                        warn!("Delete operation failed: {}", e.user_message());
                    }
                }
            }
        }
        _ => {}
    }
    Ok(false)
}

/// The backend is injected so tests never touch the user's system Trash.
pub(crate) fn start_trash(
    app: &mut App,
    selected_index: Option<usize>,
    source: String,
    trash: impl FnOnce(&Path) -> Result<(), String> + Send + 'static,
) -> Result<(), String> {
    let name = Path::new(&source)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let status = format!("Moving '{name}' to Trash…");
    let changes = vec![std::path::PathBuf::from(&source)];
    app.start_job("trash", status, changes, move |_| {
        // Ignore cancellation: once handed to the OS, trashing must finish before exit.
        // Pass the original path, without resolving the final symlink component.
        trash(Path::new(&source))
            .map(|()| format!("Moved '{name}' to Trash"))
            .map_err(|error| {
                let error = error.trim_end_matches('.');
                format!("Couldn't move '{name}' to Trash: {error}. Press D to delete permanently.")
            })
    })?;
    app.preserved_selection_index = selected_index;
    Ok(())
}

/// Sort mode: pick the sort field or direction.
pub fn handle_sort_key(key: KeyEvent, app: &mut App) -> anyhow::Result<()> {
    match key.code {
        KeyCode::Char('q') => {
            app.input_mode = InputMode::Normal;
        }
        KeyCode::Char('n') => {
            apply_sort(app, SortBy::Name)?;
        }
        KeyCode::Char('s') => {
            apply_sort(app, SortBy::Size)?;
        }
        KeyCode::Char('t') => {
            apply_sort(app, SortBy::DateAdded)?;
        }
        KeyCode::Char('a') => {
            app.sort_type = SortType::ASC;
        }
        KeyCode::Char('d') => {
            app.sort_type = SortType::DESC;
        }
        _ => {}
    }
    Ok(())
}

/// Keybinding help: `q` closes it.
pub fn handle_keybinding_key(key: KeyEvent, app: &mut App) {
    if let KeyCode::Char('q') = key.code {
        app.input_mode = InputMode::Normal;
    }
}

/// Get Info: Esc, `q` or `I` closes it, which stops its folder count.
pub fn handle_info_key(key: KeyEvent, app: &mut App) {
    if let KeyCode::Esc | KeyCode::Char('q' | 'I') = key.code {
        app.info = None;
        app.input_mode = InputMode::Normal;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{PanePosition, ViewMode};
    use crate::event::{
        background::{poll_delete, poll_job, refresh_after_copy, tests::memory_index},
        keymap::KeymapRuntime,
        normal::handle_normal_key,
        LoopControl,
    };
    use crate::status_bar::NotificationKind;
    use crate::utils::generate_copy_file_dir_name;
    use crossterm::event::KeyModifiers;
    use ratatui::{layout::Rect, TerminalOptions, Viewport};
    use std::time::{Duration, Instant};

    fn removal_state(root: &Path, files: Vec<String>) -> LoopState {
        let mut app = App::new(files);
        app.current_directory = root.to_string_lossy().into_owned();
        app.sort_by = SortBy::Name;
        app.sort_type = SortType::ASC;
        LoopState {
            app,
            state: ListState::default().with_selected(Some(0)),
            status_bar: StatusBar::new(),
            index: memory_index(root),
            file_reader_content: crate::file_reader_content::FileContent::new(
                syntect::parsing::SyntaxSet::new(),
                syntect::highlighting::ThemeSet::new(),
            ),
            image_renderer: crate::render::image_renderer::ImageRenderer::new(),
            copy_receiver: None,
            delete_receiver: None,
            selected_file: None,
            quit_error: None,
        }
    }

    fn test_terminal() -> Terminal<CrosstermBackend<Vec<u8>>> {
        Terminal::with_options(
            CrosstermBackend::new(Vec::new()),
            TerminalOptions {
                viewport: Viewport::Fixed(Rect::new(0, 0, 80, 24)),
            },
        )
        .unwrap()
    }

    fn open_removal(key: char, state: &mut LoopState) {
        let mut keymap = KeymapRuntime::from_settings(&crate::config::KeymapSettings::default());
        let modifiers = if key == 'D' {
            KeyModifiers::SHIFT
        } else {
            KeyModifiers::NONE
        };
        handle_normal_key(
            KeyEvent::new(KeyCode::Char(key), modifiers),
            state,
            &mut keymap,
            &mut test_terminal(),
            "unused",
        )
        .unwrap();
    }

    fn confirm_removal(state: &mut LoopState) {
        assert!(!handle_delete_key(
            KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE),
            state,
            &mut test_terminal(),
            "unused"
        )
        .unwrap());
    }

    fn finish_removal(state: &mut LoopState) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while state.app.job.is_some() || state.delete_receiver.is_some() {
            poll_job(
                &mut state.app,
                &mut state.state,
                &mut state.status_bar,
                &mut state.index,
                "unused",
            );
            poll_delete(
                &mut state.app,
                &mut state.state,
                &mut state.status_bar,
                &mut state.delete_receiver,
                &mut state.index,
                "unused",
            );
            assert!(Instant::now() < deadline, "removal did not finish");
            std::thread::yield_now();
        }
    }

    #[test]
    fn removal_keys_choose_distinct_popups_without_removing_the_target() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("notes.txt");
        std::fs::write(&path, "keep me").unwrap();
        for (key, mode) in [('d', InputMode::WatchTrash), ('D', InputMode::WatchDelete)] {
            let mut state = removal_state(dir.path(), vec![path.to_string_lossy().into_owned()]);
            open_removal(key, &mut state);
            assert_eq!(state.app.input_mode, mode);
            assert!(state.app.render_popup);
            assert_eq!(state.app.delete_target_path.as_deref(), path.to_str());
            assert!(state.app.job.is_none());
            assert!(state.delete_receiver.is_none());
            assert_eq!(std::fs::read_to_string(&path).unwrap(), "keep me");
        }
    }

    #[test]
    fn removal_popups_cancel_or_quit_without_removing_anything() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("notes.txt");
        std::fs::write(&path, "keep me").unwrap();
        for key in ['d', 'D'] {
            for cancel in [KeyCode::Char('n'), KeyCode::Esc, KeyCode::Char('q')] {
                let mut state =
                    removal_state(dir.path(), vec![path.to_string_lossy().into_owned()]);
                open_removal(key, &mut state);
                let quit = handle_delete_key(
                    KeyEvent::new(cancel, KeyModifiers::NONE),
                    &mut state,
                    &mut test_terminal(),
                    "unused",
                )
                .unwrap();
                assert_eq!(quit, cancel == KeyCode::Char('q'));
                assert_eq!(state.app.input_mode, InputMode::Normal);
                assert!(!state.app.render_popup);
                assert!(state.app.delete_target_path.is_none());
                assert!(state.app.job.is_none());
                assert!(state.delete_receiver.is_none());
                assert_eq!(std::fs::read_to_string(&path).unwrap(), "keep me");
            }
        }
    }

    #[test]
    fn trash_failure_reports_a_hint_and_never_permanently_deletes() {
        for directory in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("notes");
            let contents = if directory {
                std::fs::create_dir(&path).unwrap();
                path.join("keep.txt")
            } else {
                path.clone()
            };
            std::fs::write(&contents, "keep me").unwrap();
            let mut state = removal_state(dir.path(), vec![path.to_string_lossy().into_owned()]);
            state.index.changed(&path);
            let before = state.index.store.directories.clone();
            open_removal('d', &mut state);
            confirm_removal(&mut state);
            assert!(state.app.job.is_some());
            assert!(state.delete_receiver.is_none());
            finish_removal(&mut state);
            assert_eq!(std::fs::read_to_string(&contents).unwrap(), "keep me");
            assert_eq!(state.index.store.directories, before);
            assert_eq!(state.status_bar.notification_kind, NotificationKind::Error);
            assert_eq!(state.status_bar.error_message.as_deref(), Some(
                "Couldn't move 'notes' to Trash: system Trash is disabled in unit tests. Press D to delete permanently."));
        }
    }

    #[test]
    fn trash_success_refreshes_selection_and_removes_deleted_folders_from_index() {
        for (count, selected, expected) in [(3, 1, Some(1)), (3, 2, Some(1)), (1, 0, None)] {
            let dir = tempfile::tempdir().unwrap();
            let listing = dir.path().join("listing");
            let fake_trash = dir.path().join("fake-trash");
            std::fs::create_dir(&listing).unwrap();
            let files: Vec<String> = (0..count)
                .map(|i| {
                    let path = listing.join(format!("folder{i}"));
                    std::fs::create_dir_all(path.join("inner")).unwrap();
                    path.to_string_lossy().into_owned()
                })
                .collect();
            let source = files[selected].clone();
            let mut state = removal_state(&listing, files.clone());
            for path in &files {
                state.index.changed(Path::new(path));
            }
            state.state.select(Some(selected));
            // An earlier operation's index must not override this selection.
            state.app.preserved_selection_index = Some(0);
            let original_thread = std::thread::current().id();
            start_trash(
                &mut state.app,
                Some(selected),
                source.clone(),
                move |path| {
                    assert_ne!(std::thread::current().id(), original_thread);
                    std::fs::rename(path, fake_trash).map_err(|e| e.to_string())
                },
            )
            .unwrap();
            assert_eq!(
                state.app.job.as_ref().unwrap().status,
                format!("Moving 'folder{selected}' to Trash…")
            );
            state.state.select(Some(0)); // Navigation while the job is running.
            finish_removal(&mut state);
            assert_eq!(state.state.selected(), expected);
            assert_eq!(state.app.files.len(), count - 1);
            assert!(!state.app.files.contains(&source));
            assert!(!state
                .index
                .store
                .directories
                .iter()
                .any(|p| Path::new(p).starts_with(&source)));
            assert_eq!(state.index.store.directories.len(), (count - 1) * 2);
            assert_eq!(state.status_bar.notification_kind, NotificationKind::Info);
            assert_eq!(
                state.status_bar.error_message,
                Some(format!("Moved 'folder{selected}' to Trash"))
            );
        }
    }

    #[test]
    fn trash_busy_guard_keeps_the_current_job_and_selection() {
        let mut app = App::new(Vec::new());
        app.preserved_selection_index = Some(2);
        app.start_job("move", "Moving…".into(), vec![], |_| Ok("Moved".into()))
            .unwrap();
        let result = start_trash(&mut app, Some(0), "unused".into(), |_| {
            panic!("a busy app must not call the trash backend");
        });
        assert_eq!(result, Err("Wait for the current move to finish".into()));
        assert_eq!(app.preserved_selection_index, Some(2));
        assert_eq!(app.job.take().unwrap().wait(), Ok("Moved".into()));
    }

    #[cfg(unix)]
    #[test]
    fn trash_worker_passes_symlinks_unchanged_and_keeps_their_targets() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let target_dir = root.join("target-dir");
        std::fs::create_dir(&target_dir).unwrap();
        let target_file = target_dir.join("keep.txt");
        std::fs::write(&target_file, "keep me").unwrap();
        for (i, target) in [
            target_file.clone(),
            target_dir.clone(),
            root.join("missing"),
        ]
        .into_iter()
        .enumerate()
        {
            let link = root.join(format!("link{i}"));
            std::os::unix::fs::symlink(&target, &link).unwrap();
            let moved = root.join(format!("fake-trash{i}"));
            let expected_link = link.clone();
            let destination = moved.clone();
            let mut app = App::new(vec![link.to_string_lossy().into_owned()]);
            app.prepare_trash(Some(0));
            assert!(!app.delete_target_is_dir);
            let source = app.close_delete_popup().unwrap();
            start_trash(&mut app, Some(0), source, move |path| {
                assert_eq!(path, expected_link);
                assert!(path.symlink_metadata().unwrap().file_type().is_symlink());
                std::fs::rename(path, destination).map_err(|e| e.to_string())
            })
            .unwrap();
            app.job.take().unwrap().wait().unwrap();
            assert_eq!(std::fs::read_link(moved).unwrap(), target);
            assert!(link.symlink_metadata().is_err());
            assert_eq!(std::fs::read_to_string(&target_file).unwrap(), "keep me");
            assert!(target_dir.is_dir());
        }
    }

    #[test]
    fn uppercase_d_keeps_permanent_file_and_background_folder_deletion() {
        for directory in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("notes");
            if directory {
                std::fs::create_dir(&path).unwrap();
                std::fs::write(path.join("file.txt"), "delete me").unwrap();
            } else {
                std::fs::write(&path, "delete me").unwrap();
            }
            let mut state = removal_state(dir.path(), vec![path.to_string_lossy().into_owned()]);
            open_removal('D', &mut state);
            confirm_removal(&mut state);
            assert!(state.app.job.is_none());
            assert_eq!(state.delete_receiver.is_some(), directory);
            finish_removal(&mut state);
            assert!(!path.exists());
            assert!(state.app.files.is_empty());
            assert_eq!(state.state.selected(), None);
        }
    }

    fn open_info(state: &mut LoopState) {
        let mut keymap = KeymapRuntime::from_settings(&crate::config::KeymapSettings::default());
        handle_normal_key(
            KeyEvent::new(KeyCode::Char('I'), KeyModifiers::SHIFT),
            state,
            &mut keymap,
            &mut test_terminal(),
            "unused",
        )
        .unwrap();
    }

    fn info_row<'a>(app: &'a App, label: &str) -> Option<&'a str> {
        let rows = &app.info.as_ref()?.rows;
        rows.iter()
            .find(|(row, _)| *row == label)
            .map(|(_, value)| value.as_str())
    }

    #[test]
    fn shift_i_opens_get_info_for_the_selection_while_a_copy_runs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("notes.txt");
        std::fs::write(&path, "hello").unwrap();
        let mut state = removal_state(dir.path(), vec![path.to_string_lossy().into_owned()]);
        state.app.start_copy(dir.path().join("elsewhere"), None);
        open_info(&mut state);
        assert_eq!(state.app.input_mode, InputMode::WatchInfo);
        assert_eq!(info_row(&state.app, "Name"), Some("notes.txt"));
        assert_eq!(info_row(&state.app, "Kind"), Some("File"));
        assert_eq!(info_row(&state.app, "Size"), Some("5 B (5 bytes)"));
        assert!(state.app.copy_in_progress);
    }

    #[test]
    fn get_info_closes_on_esc_q_or_shift_i_and_ignores_other_keys() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("photos");
        std::fs::create_dir(&folder).unwrap();
        for close in [KeyCode::Esc, KeyCode::Char('q'), KeyCode::Char('I')] {
            let mut state = removal_state(dir.path(), vec![folder.to_string_lossy().into_owned()]);
            open_info(&mut state);
            for ignored in ['j', 'd', 'D', 'i', 'r']
                .map(KeyCode::Char)
                .into_iter()
                .chain([KeyCode::Enter, KeyCode::Down])
            {
                handle_info_key(KeyEvent::new(ignored, KeyModifiers::NONE), &mut state.app);
                assert_eq!(state.app.input_mode, InputMode::WatchInfo);
                assert!(state.app.info.is_some());
            }
            handle_info_key(KeyEvent::new(close, KeyModifiers::NONE), &mut state.app);
            assert_eq!(state.app.input_mode, InputMode::Normal);
            assert!(state.app.info.is_none());
            assert!(!state.app.render_popup);
            assert!(state.app.job.is_none());
            assert!(folder.is_dir());
        }
    }

    #[test]
    fn get_info_needs_a_selection_and_reports_a_vanished_item() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = removal_state(dir.path(), Vec::new());
        state.state.select(None);
        open_info(&mut state);
        assert_eq!(state.app.input_mode, InputMode::Normal);
        assert!(state.app.info.is_none());
        assert!(!state.status_bar.has_error());

        let gone = dir.path().join("gone.txt");
        let mut state = removal_state(dir.path(), vec![gone.to_string_lossy().into_owned()]);
        open_info(&mut state);
        assert_eq!(state.app.input_mode, InputMode::Normal);
        assert!(state.app.info.is_none());
        assert_eq!(state.status_bar.notification_kind, NotificationKind::Error);
        assert!(state
            .status_bar
            .error_message
            .as_deref()
            .unwrap()
            .starts_with("Couldn't get info for"));
    }

    #[test]
    fn get_info_shows_the_folder_count_once_it_finishes() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("photos");
        std::fs::create_dir_all(folder.join("2026")).unwrap();
        std::fs::write(folder.join("a.jpg"), "abc").unwrap();
        std::fs::write(folder.join("2026/b.jpg"), "hello").unwrap();
        let mut state = removal_state(dir.path(), vec![folder.to_string_lossy().into_owned()]);
        open_info(&mut state);
        assert_eq!(info_row(&state.app, "Kind"), Some("Folder"));
        let deadline = Instant::now() + Duration::from_secs(5);
        while state.app.info.as_ref().unwrap().folder_count.is_some() {
            crate::event::background::poll_info(&mut state.app);
            assert!(Instant::now() < deadline, "folder count did not finish");
            std::thread::yield_now();
        }
        assert_eq!(info_row(&state.app, "Size"), Some("8 B (8 bytes)"));
        assert_eq!(info_row(&state.app, "Contents"), Some("2 files, 1 folder"));
    }

    #[test]
    fn rename_refresh_selects_the_destination_after_input_is_reset() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("alpha.txt"), "rename me").unwrap();
        std::fs::write(dir.path().join("middle.txt"), "existing").unwrap();
        let directory = dir.path().to_string_lossy().into_owned();
        let mut app = App::new(Vec::new());
        app.current_path_to_edit = directory.clone();
        app.current_name_to_edit = "alpha.txt".into();
        app.create_edit_file_name = "zulu.txt".into();
        let mut state = ListState::default();
        state.select(Some(0));
        let destination = format!("{}/{}", app.current_path_to_edit, app.create_edit_file_name);

        handle_rename(&app).unwrap();
        app.reset_create_edit_values();
        let files = get_file_path_data(directory, false, SortBy::Name, &SortType::ASC).unwrap();
        app.refresh_files_after_operation(files, &mut state, Some(destination.clone()));

        assert_eq!(state.selected(), Some(1));
        assert_eq!(app.get_selected_path(state.selected()), Some(destination));
        assert!(app.create_edit_file_name.is_empty());
    }

    #[test]
    fn create_and_rename_update_the_folder_index() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let mut index = crate::event::background::tests::memory_index(root);
        let mut app = App::new(Vec::new());
        app.current_directory = root.to_string_lossy().into_owned();
        let mut state = ListState::default();
        let mut status = StatusBar::new();
        let enter = KeyEvent::new(KeyCode::Enter, crossterm::event::KeyModifiers::NONE);
        let spelled = |name: &str| root.join(name).to_string_lossy().into_owned();

        app.input_mode = InputMode::WatchCreate;
        app.create_edit_file_name = "alpha".into();
        let fallback = root.to_string_lossy().into_owned();
        handle_create_key(
            enter,
            &mut app,
            &mut state,
            &mut status,
            &mut index,
            &fallback,
        )
        .unwrap();
        assert_eq!(index.store.directories, [spelled("alpha")]);
        std::fs::create_dir(root.join("alpha/inner")).unwrap();
        index.changed(&root.join("alpha/inner"));

        app.input_mode = InputMode::WatchRename;
        app.current_path_to_edit = root.to_string_lossy().into_owned();
        app.current_name_to_edit = "alpha".into();
        app.create_edit_file_name = "beta".into();
        handle_rename_key(
            enter,
            &mut app,
            &mut state,
            &mut status,
            &mut index,
            &fallback,
        )
        .unwrap();
        assert_eq!(
            index.store.directories,
            [spelled("beta"), spelled("beta/inner")]
        );
    }

    fn press(state: &mut LoopState, code: KeyCode) -> LoopControl {
        let mut keymap = KeymapRuntime::from_settings(&crate::config::KeymapSettings::default());
        let modifiers = match code {
            KeyCode::Char(c) if c.is_ascii_uppercase() => KeyModifiers::SHIFT,
            _ => KeyModifiers::NONE,
        };
        handle_normal_key(
            KeyEvent::new(code, modifiers),
            state,
            &mut keymap,
            &mut test_terminal(),
            "unused",
        )
        .unwrap()
    }

    /// Dual pane over `root/left` with alpha.txt highlighted and `root/right` with
    /// delta.txt highlighted, the right pane active. Returns both highlighted paths.
    fn dual_pane_state(root: &Path) -> (LoopState, String, String) {
        let folder = |name: &str, files: &[&str]| {
            let dir = root.join(name);
            std::fs::create_dir(&dir).unwrap();
            for file in files {
                std::fs::write(dir.join(file), file).unwrap();
            }
            dir
        };
        let left = folder("left", &["alpha.txt", "bravo.txt"]);
        let right = folder("right", &["charlie.txt", "delta.txt", "echo.txt"]);
        let spelled = |path: std::path::PathBuf| path.to_string_lossy().into_owned();
        let left_files =
            get_file_path_data(spelled(left.clone()), false, SortBy::Name, &SortType::ASC).unwrap();
        let mut state = removal_state(&left, left_files);
        state.app.view_mode = ViewMode::DualPane;
        state.app.right_pane = crate::pane::Pane::new(spelled(right.clone()), Vec::new());
        state.app.right_pane.refresh_files();
        let right_item = spelled(right.join("delta.txt"));
        state.app.right_pane.select_path(&right_item);
        state.app.active_pane = PanePosition::Right;
        let left_item = spelled(left.join("alpha.txt"));
        assert_eq!(
            state.app.get_selected_path(state.state.selected()),
            Some(left_item.clone())
        );
        assert_eq!(
            state.app.right_pane.get_selected_path(),
            Some(right_item.clone())
        );
        (state, left_item, right_item)
    }

    fn name_of(path: &str) -> String {
        Path::new(path)
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned()
    }

    #[test]
    fn item_keys_act_on_the_active_panes_highlighted_item() {
        for (view, active, right_is_target) in [
            (ViewMode::DualPane, PanePosition::Right, true),
            (ViewMode::DualPane, PanePosition::Left, false),
            (ViewMode::Normal, PanePosition::Left, false),
            // Unreachable by keys (leaving dual pane resets it), but must stay harmless.
            (ViewMode::Normal, PanePosition::Right, false),
        ] {
            let setup = || {
                let dir = tempfile::tempdir().unwrap();
                let (mut state, left_item, right_item) = dual_pane_state(dir.path());
                state.app.view_mode = view.clone();
                state.app.active_pane = active.clone();
                let target = if right_is_target {
                    right_item
                } else {
                    left_item
                };
                (dir, state, target)
            };

            for (key, mode) in [('d', InputMode::WatchTrash), ('D', InputMode::WatchDelete)] {
                let (_dir, mut state, target) = setup();
                press(&mut state, KeyCode::Char(key));
                assert_eq!(state.app.input_mode, mode);
                assert_eq!(state.app.delete_target_path, Some(target.clone()));
                assert_eq!(state.app.delete_target_name, name_of(&target));
                assert!(Path::new(&target).exists());
            }

            let (_dir, mut state, target) = setup();
            press(&mut state, KeyCode::Char('r'));
            assert_eq!(state.app.input_mode, InputMode::WatchRename);
            assert_eq!(state.app.current_name_to_edit, name_of(&target));
            assert_eq!(state.app.create_edit_file_name, name_of(&target));
            assert_eq!(
                Path::new(&state.app.current_path_to_edit),
                Path::new(&target).parent().unwrap()
            );

            let (_dir, mut state, target) = setup();
            press(&mut state, KeyCode::Char('I'));
            assert_eq!(state.app.input_mode, InputMode::WatchInfo);
            assert_eq!(
                info_row(&state.app, "Name"),
                Some(name_of(&target).as_str())
            );

            // Enter hands the item over once the terminal is restored; --print-path
            // resolves the same `selected_path` before printing it.
            let (_dir, mut state, target) = setup();
            assert!(matches!(
                press(&mut state, KeyCode::Enter),
                LoopControl::Break
            ));
            assert_eq!(state.selected_file, Some(target));
        }
    }

    #[test]
    fn leaving_dual_pane_returns_item_keys_to_the_left_listing() {
        let dir = tempfile::tempdir().unwrap();
        let (mut state, left_item, _) = dual_pane_state(dir.path());
        state.app.view_mode = ViewMode::Normal;
        state.app.active_pane = PanePosition::Left;

        press(&mut state, KeyCode::Char('p'));
        press(&mut state, KeyCode::Char('p'));
        assert_eq!(state.app.view_mode, ViewMode::DualPane);
        press(&mut state, KeyCode::Tab);
        press(&mut state, KeyCode::Char('j'));
        let right_item = state.app.right_pane.get_selected_path().unwrap();
        assert_ne!(right_item, left_item);
        assert_eq!(
            state.app.active_selected_path(state.state.selected()),
            Some(right_item)
        );

        press(&mut state, KeyCode::Char('p'));
        assert_eq!(state.app.view_mode, ViewMode::Normal);
        assert!(!state.app.is_right_pane_active());
        press(&mut state, KeyCode::Char('D'));
        assert_eq!(state.app.delete_target_path, Some(left_item));
    }

    #[test]
    fn duplicate_copies_the_right_panes_item_beside_it() {
        let dir = tempfile::tempdir().unwrap();
        let (mut state, left_item, right_item) = dual_pane_state(dir.path());
        let right_dir = Path::new(&right_item).parent().unwrap().to_path_buf();
        let copy = generate_copy_file_dir_name(&right_item, &right_dir);

        press(&mut state, KeyCode::Char('c'));
        let deadline = Instant::now() + Duration::from_secs(5);
        while state.copy_receiver.is_some() {
            refresh_after_copy(
                &mut state.app,
                &mut state.state,
                &mut state.status_bar,
                &mut state.copy_receiver,
                &mut state.index,
                "unused",
            );
            assert!(Instant::now() < deadline, "copy did not finish");
            std::thread::yield_now();
        }

        assert_eq!(std::fs::read_to_string(&copy).unwrap(), "delta.txt");
        let left_dir = Path::new(&left_item).parent().unwrap();
        assert!(!generate_copy_file_dir_name(&left_item, left_dir).exists());
        assert!(state
            .app
            .right_pane
            .files
            .contains(&copy.to_string_lossy().into_owned()));
        assert_eq!(state.app.right_pane.get_selected_path(), Some(right_item));
        assert_eq!(
            state.app.get_selected_path(state.state.selected()),
            Some(left_item)
        );
    }

    #[test]
    fn removal_from_the_right_pane_refreshes_it_and_keeps_the_left_selection() {
        for removal in ["trash", "delete file", "delete folder"] {
            let dir = tempfile::tempdir().unwrap();
            let (mut state, left_item, right_item) = dual_pane_state(dir.path());
            if removal == "delete folder" {
                std::fs::remove_file(&right_item).unwrap();
                std::fs::create_dir(&right_item).unwrap();
                std::fs::write(Path::new(&right_item).join("inner.txt"), "x").unwrap();
            }
            if removal == "trash" {
                open_removal('d', &mut state);
                let source = state.app.close_delete_popup().unwrap();
                assert_eq!(source, right_item);
                let fake_trash = dir.path().join("fake-trash");
                start_trash(
                    &mut state.app,
                    state.state.selected(),
                    source,
                    move |path| std::fs::rename(path, fake_trash).map_err(|e| e.to_string()),
                )
                .unwrap();
            } else {
                open_removal('D', &mut state);
                assert_eq!(state.app.delete_target_path, Some(right_item.clone()));
                confirm_removal(&mut state);
            }
            finish_removal(&mut state);

            assert!(!Path::new(&right_item).exists(), "{removal}");
            let pane = &state.app.right_pane;
            assert_eq!(pane.files.len(), 2, "{removal}");
            assert!(!pane.files.contains(&right_item), "{removal}");
            let highlighted = pane.get_selected_path().unwrap();
            assert!(Path::new(&highlighted).exists(), "{removal}");
            assert_eq!(pane.list_state.selected(), pane.selected_index);
            assert_eq!(
                state.app.get_selected_path(state.state.selected()),
                Some(left_item.clone()),
                "{removal}"
            );
            assert_eq!(state.app.files.len(), 2, "{removal}");
        }
    }

    #[test]
    fn rename_from_the_right_pane_refreshes_it_and_highlights_the_new_name() {
        let dir = tempfile::tempdir().unwrap();
        let (mut state, left_item, right_item) = dual_pane_state(dir.path());
        let renamed = Path::new(&right_item)
            .with_file_name("aardvark.txt")
            .to_string_lossy()
            .into_owned();

        press(&mut state, KeyCode::Char('r'));
        state.app.create_edit_file_name = "aardvark.txt".into();
        handle_rename_key(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            &mut state.app,
            &mut state.state,
            &mut state.status_bar,
            &mut state.index,
            "unused",
        )
        .unwrap();

        assert_eq!(state.app.input_mode, InputMode::Normal);
        assert_eq!(std::fs::read_to_string(&renamed).unwrap(), "delta.txt");
        let pane = &state.app.right_pane;
        assert!(!pane.files.contains(&right_item));
        assert_eq!(pane.get_selected_path(), Some(renamed));
        assert_eq!(pane.list_state.selected(), pane.selected_index);
        assert_eq!(
            state.app.get_selected_path(state.state.selected()),
            Some(left_item)
        );
        assert_eq!(state.app.files.len(), 2);
    }

    #[test]
    fn create_from_the_right_pane_lands_there_and_keeps_the_left_listing() {
        let dir = tempfile::tempdir().unwrap();
        let (mut state, left_item, right_item) = dual_pane_state(dir.path());
        let created = Path::new(&right_item)
            .with_file_name("new.txt")
            .to_string_lossy()
            .into_owned();

        press(&mut state, KeyCode::Char('a'));
        state.app.create_edit_file_name = "new.txt".into();
        handle_create_key(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            &mut state.app,
            &mut state.state,
            &mut state.status_bar,
            &mut state.index,
            "unused",
        )
        .unwrap();

        assert!(Path::new(&created).is_file());
        assert_eq!(state.app.right_pane.get_selected_path(), Some(created));
        let left_dir = Path::new(&left_item).parent().unwrap();
        assert!(!left_dir.join("new.txt").exists());
        assert_eq!(state.app.files.len(), 2);
        assert!(state
            .app
            .files
            .iter()
            .all(|file| Path::new(file).parent() == Some(left_dir)));
        assert_eq!(
            state.app.get_selected_path(state.state.selected()),
            Some(left_item)
        );
    }
}
