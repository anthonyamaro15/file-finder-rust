//! Key handling for the popup modes: rename, create, delete confirmation, sort and
//! the keybinding help.

use std::{io, path::Path, sync::mpsc};

use crossterm::event::{KeyCode, KeyEvent};
use log::warn;
use ratatui::{backend::CrosstermBackend, widgets::ListState, Terminal};

use crate::{
    app::{App, InputMode},
    event::{
        file_actions::{
            create_target_directory, created_item_path, current_refresh_directory,
            refresh_file_list,
        },
        sort::apply_sort,
    },
    exit::may_exit,
    operations::{
        create_item_based_on_type, delete_with_progress, handle_delete_based_on_type,
        handle_rename, CopyMessage, DeleteMessage,
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
#[allow(clippy::too_many_arguments)]
pub fn handle_delete_key(
    key: KeyEvent,
    app: &mut App,
    state: &mut ListState,
    status_bar: &mut StatusBar,
    copy_receiver: &mut Option<mpsc::Receiver<CopyMessage>>,
    delete_receiver: &mut Option<mpsc::Receiver<DeleteMessage>>,
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    quit_error: &mut Option<String>,
    fallback_start_path: &str,
) -> anyhow::Result<bool> {
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
            if let Some(selected) = app.close_delete_popup() {
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
}
