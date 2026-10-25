//! Key handling for Normal mode: navigation, file operations, view changes and
//! leaving ff.

use std::{
    io,
    path::Path,
    process::{Command, Stdio},
};

use crossterm::{
    event::{DisableMouseCapture, KeyCode, KeyEvent},
    execute,
    terminal::{disable_raw_mode, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use crate::{
    app::{InputMode, PanePosition, SearchScope, TransferKind, ViewMode},
    config::Action,
    event::{
        file_actions::{
            containing_directory_or_current, current_refresh_directory, enter_directory,
            rename_target,
        },
        keymap::KeymapRuntime,
        navigation::{next_index_wrapping, parent_listing_directory, previous_index_wrapping},
        transfer::{open_folder_picker, start_transfer},
        LoopControl, LoopState,
    },
    exit::{may_exit, system_opener, write_result_file},
    file_reader_content::{FileContent, FileType},
    info::ItemInfo,
    operations::copy_dir_file_with_progress_cancellable,
    utils::{generate_copy_file_dir_name, get_curr_path, get_file_path_data, is_file},
};

/// Handle a key in Normal mode and tell the main loop how to go on.
pub fn handle_normal_key(
    key: KeyEvent,
    loop_state: &mut LoopState,
    keymap_runtime: &mut KeymapRuntime,
    terminal: &mut Terminal<CrosstermBackend<impl io::Write>>,
    fallback_start_path: &str,
) -> io::Result<LoopControl> {
    let LoopState {
        app,
        state,
        file_reader_content,
        image_renderer,
        status_bar,
        copy_receiver,
        selected_file,
        quit_error,
        ..
    } = loop_state;
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
                if let Some(parent) = Path::new(&current_dir).parent() {
                    if let Some(parent_str) = parent.to_str() {
                        if let Err(e) = app.right_pane.navigate_to_directory(parent_str) {
                            status_bar
                                .show_error(format!("Could not open '{parent_str}': {e:#}"), None);
                        }
                    }
                }
            } else if let Some(selected) = app.get_selected_path(state.selected()) {
                let selected_path = Path::new(&selected);

                let new_path = parent_listing_directory(selected_path)
                    .map(|path| path.to_string_lossy().to_string());

                if let Some(new_path) = new_path {
                    // Highlight the directory just left.
                    let previous_dir = get_curr_path(selected.clone());
                    if let Err(e) = enter_directory(app, &new_path, state, Some(previous_dir)) {
                        status_bar.show_error(format!("Could not open '{new_path}': {e:#}"), None);
                    }
                }
            } else {
                // An empty listing has nothing to step out of, so go to the folder above.
                let previous_dir = current_refresh_directory(app, fallback_start_path);
                let prev_dir = app.prev_dir.clone();
                if let Err(e) = enter_directory(app, &prev_dir, state, Some(previous_dir)) {
                    status_bar.show_error(format!("Could not open '{prev_dir}': {e:#}"), None);
                }
            }
        }
        KeyCode::Char('l') => {
            // In DualPane mode with right pane active, enter directory in right pane
            if app.view_mode.is_dual_pane() && app.is_right_pane_active() {
                if let Some(selected) = app.right_pane.get_selected_path() {
                    if !is_file(selected.clone()) {
                        if let Err(e) = app.right_pane.navigate_to_directory(&selected) {
                            status_bar
                                .show_error(format!("Could not open '{selected}': {e:#}"), None);
                        }
                    }
                }
            } else {
                // Use helper method to get selected path based on current mode
                let selected_path = app.get_selected_path(state.selected());

                if let Some(selected) = selected_path {
                    if !is_file(selected.to_string()) {
                        if let Err(e) = enter_directory(app, &selected, state, None) {
                            status_bar
                                .show_error(format!("Could not open '{selected}': {e:#}"), None);
                        }
                    }
                }
            }
        }

        KeyCode::Char('o') => {
            if let Some(selected) = app.active_selected_path(state.selected()) {
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
            app.prepare_trash(state.selected());
        }
        KeyCode::Char('D') => {
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
                app.right_pane.watcher = None;
            }

            app.view_mode = new_mode;
        }
        KeyCode::Tab => {
            // Switch active pane in dual-pane mode
            if app.view_mode.is_dual_pane() {
                app.toggle_active_pane();
            }
        }
        KeyCode::Char(c @ ('C' | 'M')) => {
            let kind = if c == 'C' {
                TransferKind::Copy
            } else {
                TransferKind::Move
            };
            if app.view_mode.is_dual_pane() {
                // Shift+C/Shift+M: send the highlighted item to the other pane's folder.
                if let Some((source, dest_dir)) = app.other_pane_transfer(state.selected()) {
                    if let Err(warning) = start_transfer(
                        app,
                        copy_receiver,
                        state.selected(),
                        kind,
                        source,
                        dest_dir,
                        "other pane".to_string(),
                    ) {
                        status_bar.show_warning(warning, None);
                    }
                }
            } else {
                // Elsewhere they pick the folder by name: "Copy to…"/"Move to…".
                open_folder_picker(app, status_bar, state.selected(), kind);
            }
        }
        KeyCode::Char('y') => {
            if let Some(selected) = app.active_selected_path(state.selected()) {
                let file_type = file_reader_content.get_file_extension(selected.clone());
                if let FileType::ZIP = file_type {
                    let name = Path::new(&selected)
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();
                    let status = format!("Extracting '{name}'…");
                    let changes = FileContent::zip_extraction_dir(Path::new(&selected))
                        .into_iter()
                        .collect();
                    let started = app.start_job("extraction", status, changes, move |cancel| {
                        FileContent::extract_zip_content_cancellable(&selected, &cancel)
                    });
                    if let Err(warning) = started {
                        status_bar.show_warning(warning, None);
                    }
                }
            }
        }
        KeyCode::Char('r') => {
            if let Some(selected) = app.active_selected_path(state.selected()) {
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
            app.right_pane.show_hidden_files = app.show_hidden_files;
            if app.view_mode.is_dual_pane() {
                app.right_pane.refresh_files();
            }
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
            } else if let Some(selected) = app.active_selected_path(state.selected()) {
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

        KeyCode::Char('I') => {
            if let Some(selected) = app.active_selected_path(state.selected()) {
                // Read-only, so it never waits for (or blocks) a running operation.
                match ItemInfo::read(Path::new(&selected)) {
                    Ok(info) => {
                        // Replacing an earlier popup stops its folder count.
                        app.info = Some(info);
                        app.input_mode = InputMode::WatchInfo;
                    }
                    Err(e) => status_bar
                        .show_error(format!("Couldn't get info for '{selected}': {e}"), None),
                }
            }
        }

        KeyCode::Enter => {
            // The active pane's item, from search results or the listing
            let selected_path = app.active_selected_path(state.selected());

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
