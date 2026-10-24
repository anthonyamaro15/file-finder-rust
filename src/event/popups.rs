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
                                Some(new_path),
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
                .get_selected_path(state.selected())
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
                    refresh_file_list(app, Path::new(&new_path), state, created_path)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{
        background::{poll_delete, poll_job, tests::memory_index},
        keymap::KeymapRuntime,
        normal::handle_normal_key,
    };
    use crate::status_bar::NotificationKind;
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
        handle_create_key(enter, &mut app, &mut state, &mut status, &mut index).unwrap();
        assert_eq!(index.store.directories, [spelled("alpha")]);
        std::fs::create_dir(root.join("alpha/inner")).unwrap();
        index.changed(&root.join("alpha/inner"));

        app.input_mode = InputMode::WatchRename;
        app.current_path_to_edit = root.to_string_lossy().into_owned();
        app.current_name_to_edit = "alpha".into();
        app.create_edit_file_name = "beta".into();
        let fallback = root.to_string_lossy().into_owned();
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
}
