//! Key handling for Normal mode: navigation, file operations, view changes and
//! leaving ff.

use std::{
    io,
    path::Path,
    process::{Command, Stdio},
    sync::mpsc,
};

use crossterm::{
    event::{DisableMouseCapture, KeyCode, KeyEvent},
    execute,
    terminal::{disable_raw_mode, LeaveAlternateScreen},
};
use log::debug;
use ratatui::{backend::CrosstermBackend, widgets::ListState, Terminal};

use crate::{
    app::{App, InputMode, PanePosition, SearchScope, ViewMode},
    config::Action,
    event::{
        file_actions::{containing_directory_or_current, current_refresh_directory, rename_target},
        keymap::KeymapRuntime,
        navigation::{next_index_wrapping, parent_directory, previous_index_wrapping},
        LoopControl,
    },
    exit::{may_exit, system_opener, write_result_file},
    file_reader_content::{FileContent, FileType},
    operations::{self, copy_dir_file_with_progress_cancellable, CopyMessage},
    render::image_renderer::ImageRenderer,
    status_bar::StatusBar,
    utils::{
        generate_copy_file_dir_name, get_curr_path, get_file_path_data, get_inner_files_info,
        is_file,
    },
};

/// Handle a key in Normal mode and tell the main loop how to go on.
#[allow(clippy::too_many_arguments)]
pub fn handle_normal_key(
    key: KeyEvent,
    app: &mut App,
    state: &mut ListState,
    file_reader_content: &mut FileContent,
    image_renderer: &mut ImageRenderer,
    keymap_runtime: &mut KeymapRuntime,
    status_bar: &mut StatusBar,
    copy_receiver: &mut Option<mpsc::Receiver<CopyMessage>>,
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    quit_error: &mut Option<String>,
    selected_file: &mut Option<String>,
    fallback_start_path: &str,
) -> io::Result<LoopControl> {
    if let Some(action) = keymap_runtime.resolve_normal(key) {
        match action {
            Action::SearchCurrent => {
                app.enter_search(SearchScope::CurrentDirectory);
                file_reader_content.file_type = FileType::NotAvailable;
                image_renderer.clear();
            }
            Action::SearchRoot => {
                app.enter_search(SearchScope::Root);
                file_reader_content.file_type = FileType::NotAvailable;
                image_renderer.clear();
            }
        }
        return Ok(LoopControl::Continue);
    }

    match key.code {
        KeyCode::Char('q') => {
            if may_exit(app, status_bar, "q", copy_receiver, terminal, quit_error) {
                return Ok(LoopControl::Break);
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            // In DualPane mode with right pane active, navigate right pane
            if app.view_mode.is_dual_pane() && app.is_right_pane_active() {
                app.right_pane.navigate_down();
            } else {
                let list_len = app.get_list_length();

                if let Some(i) = next_index_wrapping(state.selected(), list_len) {
                    state.select(Some(i));
                    app.curr_index = Some(i);
                }
            }
        }
        KeyCode::Up | KeyCode::Char('k') => {
            // In DualPane mode with right pane active, navigate right pane
            if app.view_mode.is_dual_pane() && app.is_right_pane_active() {
                app.right_pane.navigate_up();
            } else {
                let list_len = app.get_list_length();

                if let Some(i) = previous_index_wrapping(state.selected(), list_len) {
                    state.select(Some(i));
                    app.curr_index = Some(i);
                }
            }
        }
        KeyCode::Char('h') => {
            // In DualPane mode with right pane active, navigate right pane to parent
            if app.view_mode.is_dual_pane() && app.is_right_pane_active() {
                let current_dir = app.right_pane.current_directory.clone();
                if let Some(parent) = parent_directory(Path::new(&current_dir)) {
                    if let Some(parent_str) = parent.to_str() {
                        app.right_pane.navigate_to_directory(parent_str);
                    }
                }
            } else if let Some(selected) = app.get_selected_path(state.selected()) {
                let selected_path = Path::new(&selected);

                // TODO: refactor this to be more idiomatic
                if selected_path.components().count() > 4 {
                    let new_path = parent_directory(selected_path)
                        .and_then(|path| parent_directory(&path))
                        .map(|path| path.to_string_lossy().to_string());

                    if let Some(new_path) = new_path {
                        let files_strings = get_inner_files_info(
                            new_path.clone(),
                            app.show_hidden_files,
                            app.sort_by.clone(),
                            &app.sort_type,
                        )
                        .unwrap();

                        if let Some(f_s) = files_strings {
                            app.clear_search();
                            app.read_only_files = f_s.clone();
                            app.files = f_s;
                            app.update_file_references();
                            state.select(Some(0));

                            // Start watching the new directory
                            if let Err(e) = app.start_watching_directory(&new_path) {
                                debug!("Failed to start watching directory '{}': {}", new_path, e);
                            } else {
                                debug!("Started watching directory: {}", new_path);
                            }
                        }
                    }
                }
            } else {
                let files_strings = get_inner_files_info(
                    app.prev_dir.clone(),
                    app.show_hidden_files,
                    app.sort_by.clone(),
                    &app.sort_type,
                )
                .unwrap();

                if let Some(f_s) = files_strings {
                    app.read_only_files = f_s.clone();
                    app.files = f_s;
                    app.update_file_references();
                    state.select(Some(0));
                }
            }
        }
        KeyCode::Char('l') => {
            // In DualPane mode with right pane active, enter directory in right pane
            if app.view_mode.is_dual_pane() && app.is_right_pane_active() {
                if let Some(selected) = app.right_pane.get_selected_path() {
                    if !is_file(selected.clone()) {
                        app.right_pane.navigate_to_directory(&selected);
                    }
                }
            } else {
                // Use helper method to get selected path based on current mode
                let selected_path = app.get_selected_path(state.selected());

                if let Some(selected) = selected_path {
                    app.prev_dir = get_curr_path(selected.to_string());
                    if !is_file(selected.to_string()) {
                        match get_inner_files_info(
                            selected.to_string(),
                            app.show_hidden_files,
                            app.sort_by.clone(),
                            &app.sort_type,
                        ) {
                            Ok(files_strings) => {
                                if let Some(files_strs) = files_strings {
                                    // Exit search mode when navigating into a directory
                                    app.clear_search();
                                    app.input_mode = InputMode::Normal;

                                    app.read_only_files = files_strs.clone();
                                    app.files = files_strs;
                                    app.update_file_references();
                                    state.select(Some(0));

                                    // Start watching the new directory
                                    if let Err(e) = app.start_watching_directory(&selected) {
                                        debug!(
                                            "Failed to start watching directory '{}': {}",
                                            selected, e
                                        );
                                    } else {
                                        debug!("Started watching directory: {}", selected);
                                    }
                                }
                            }
                            Err(e) => {
                                println!("Error: {}", e);
                            }
                        }
                    }
                }
            }
        }

        KeyCode::Char('o') => {
            if let Some(selected) = app.get_selected_path(state.selected()) {
                // Opening leaves ff, so it waits for confirmation during an operation.
                if !may_exit(app, status_bar, "o", copy_receiver, terminal, quit_error) {
                    return Ok(LoopControl::Continue);
                }
                // Spawn without waiting; null stdio keeps opener output off the TUI.
                match Command::new(system_opener())
                    .arg(&selected)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                {
                    Ok(_) => return Ok(LoopControl::Break),
                    Err(e) => status_bar.show_error(
                        format!("Failed to open with {}: {}", system_opener(), e),
                        Some(3),
                    ),
                }
            }
        }
        KeyCode::Char('d') => {
            app.prepare_delete(state.selected());
        }
        KeyCode::Char('a') => {
            app.input_mode = InputMode::WatchCreate;
        }
        KeyCode::Char('p') => {
            // Cycle through view modes: Normal -> FullList -> DualPane -> Normal
            let new_mode = app.view_mode.next();

            // Initialize right pane when entering DualPane mode
            if new_mode == ViewMode::DualPane {
                app.init_right_pane();
            }

            // Reset active pane to left when leaving DualPane mode
            if app.view_mode == ViewMode::DualPane && new_mode != ViewMode::DualPane {
                app.active_pane = PanePosition::Left;
            }

            app.view_mode = new_mode;
        }
        KeyCode::Tab => {
            // Switch active pane in dual-pane mode
            if app.view_mode.is_dual_pane() {
                app.toggle_active_pane();
            }
        }
        KeyCode::Char('C') => {
            // Shift+C: copy the highlighted item into the other pane's folder.
            if app.view_mode.is_dual_pane() {
                if let Some(warning) = app.busy_warning() {
                    status_bar.show_warning(warning, None);
                } else if let Some((src_path, dest_dir)) = app.other_pane_transfer(state.selected())
                {
                    let src = Path::new(&src_path);
                    if let Some(src_name) = src.file_name() {
                        let dest = Path::new(&dest_dir).join(src_name);
                        app.preserved_selection_index = state.selected();
                        app.start_copy(dest.clone(), Some(src_name.to_string_lossy().to_string()));
                        *copy_receiver = Some(copy_dir_file_with_progress_cancellable(
                            src,
                            &dest,
                            app.copy_cancel.clone(),
                        ));
                    }
                }
            }
        }
        KeyCode::Char('M') => {
            // Shift+M: move the highlighted item into the other pane's folder.
            if app.view_mode.is_dual_pane() {
                if let Some((src_path, dest_dir)) = app.other_pane_transfer(state.selected()) {
                    let name = Path::new(&src_path)
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();
                    let status = format!("Moving '{name}'…");
                    let started = app.start_job("move", status, move |cancel| {
                        operations::move_file_or_dir_cancellable(&src_path, &dest_dir, &cancel)
                            .map(|()| format!("Moved '{name}' to other pane"))
                            .map_err(|e| format!("Move failed: {e}"))
                    });
                    if let Err(warning) = started {
                        status_bar.show_warning(warning, None);
                    }
                }
            }
        }
        KeyCode::Char('y') => {
            if let Some(selected) = app.get_selected_path(state.selected()) {
                let file_type = file_reader_content.get_file_extension(selected.clone());
                if let FileType::ZIP = file_type {
                    let name = Path::new(&selected)
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();
                    let status = format!("Extracting '{name}'…");
                    let started = app.start_job("extraction", status, move |cancel| {
                        FileContent::extract_zip_content_cancellable(&selected, &cancel)
                    });
                    if let Err(warning) = started {
                        status_bar.show_warning(warning, None);
                    }
                }
            }
        }
        KeyCode::Char('r') => {
            if let Some(selected) = app.get_selected_path(state.selected()) {
                if let Some((parent, placeholder_name)) = rename_target(Path::new(&selected)) {
                    app.current_path_to_edit = parent.to_string_lossy().to_string();
                    app.current_name_to_edit = placeholder_name.clone();
                    app.create_edit_file_name = placeholder_name.clone();
                    app.char_index = placeholder_name.len();
                    app.input_mode = InputMode::WatchRename;
                }
            }
        }
        KeyCode::Char('.') => {
            app.show_hidden_files = !app.show_hidden_files;
            // Refresh the browsed directory (even when it's empty or a
            // global result is highlighted), keeping any search filter.
            let refresh_dir = current_refresh_directory(app, fallback_start_path);
            match get_file_path_data(
                refresh_dir,
                app.show_hidden_files,
                app.sort_by.clone(),
                &app.sort_type,
            ) {
                Ok(files) => {
                    app.refresh_files_from_watcher(files, state);
                    status_bar.invalidate_cache();
                }
                Err(e) => {
                    status_bar.show_error(format!("Could not refresh the listing: {e}"), None)
                }
            }
        }
        KeyCode::Char('c') => {
            // Copy the highlighted item beside its source.
            if let Some(warning) = app.busy_warning() {
                status_bar.show_warning(warning, None);
            } else if let Some(selected) = app.get_selected_path(state.selected()) {
                let src_path = Path::new(&selected);
                let current_dir = containing_directory_or_current(src_path);
                let new_src = generate_copy_file_dir_name(src_path, &current_dir);
                app.preserved_selection_index = state.selected();
                app.start_copy(new_src.clone(), None);
                *copy_receiver = Some(copy_dir_file_with_progress_cancellable(
                    src_path,
                    &new_src,
                    app.copy_cancel.clone(),
                ));
            }
        }

        KeyCode::Char('s') => {
            app.input_mode = InputMode::WatchSort;
        }

        KeyCode::Char('?') => {
            app.input_mode = InputMode::WatchKeyBinding;
        }

        KeyCode::Enter => {
            // Use helper method to get selected path based on current mode
            let selected_path = app.get_selected_path(state.selected());

            if let Some(selected) = selected_path {
                // Leaving for the shell or editor waits for confirmation during an operation.
                if !may_exit(
                    app,
                    status_bar,
                    "Enter",
                    copy_receiver,
                    terminal,
                    quit_error,
                ) {
                    return Ok(LoopControl::Continue);
                }
                app.input = selected.clone();

                // Check if print_path mode - hand the path to the shell integration
                if app.print_path {
                    // Write to $FF_RESULT_FILE (set by `ff init`) before restoring terminal
                    let wrote_result_file =
                        std::env::var_os("FF_RESULT_FILE").is_some_and(|result_file| {
                            write_result_file(Path::new(&result_file), &selected).is_ok()
                        });

                    // Restore terminal
                    disable_raw_mode()?;
                    execute!(
                        terminal.backend_mut(),
                        LeaveAlternateScreen,
                        DisableMouseCapture
                    )?;
                    terminal.show_cursor()?;
                    if let Some(error) = &quit_error {
                        eprintln!("ff: {error}");
                    }

                    // No result file (old shell function or direct use): print to stdout
                    if !wrote_result_file {
                        println!("{}", selected);
                    }
                    return Ok(LoopControl::Return);
                }

                // Open in the IDE or copy to the clipboard once the terminal is restored
                *selected_file = Some(selected);
                return Ok(LoopControl::Break);
            }
        }
        _ => {}
    }
    Ok(LoopControl::Continue)
}
