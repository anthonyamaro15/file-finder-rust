// ============================================================
// File Finder (ff) - Terminal File Browser
// ============================================================

// Standard library imports
use std::{
    io,
    path::Path,
    process::{Command, Stdio},
    sync::mpsc,
};

// External crate imports
use crossterm::{
    event::{
        self as crossterm_event, DisableMouseCapture, EnableMouseCapture, Event, KeyCode,
        KeyEventKind,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use log::{debug, warn};
use ratatui::{backend::CrosstermBackend, widgets::ListState, Terminal};
use syntect::{highlighting::ThemeSet, parsing::SyntaxSet};

// Internal imports
use file_finder::exit::{handle_file_selection, may_exit, system_opener, write_result_file};
use file_finder::{app, cli, event::background, operations, render};
use file_finder::{
    app::{App, InputMode, PanePosition, SearchScope, ViewMode},
    cli::{compute_effective_config, CliArgs},
    config::Action,
    directory_store::{build_directory_from_store_async, load_directory_from_file, DirectoryStore},
    event::{
        file_actions::{
            containing_directory_or_current, create_target_directory, created_item_path,
            current_refresh_directory, refresh_file_list, rename_target,
        },
        keymap::KeymapRuntime,
        navigation::{next_index_wrapping, parent_directory, previous_index_wrapping},
        search::handle_search_key,
        sort::apply_sort,
    },
    file_reader_content::{FileContent, FileType},
    operations::{
        copy_dir_file_with_progress_cancellable, create_item_based_on_type, delete_with_progress,
        handle_delete_based_on_type, handle_rename, CopyMessage, DeleteMessage,
    },
    render::{configured_action_key_labels, image_renderer::ImageRenderer},
    status_bar::StatusBar,
    ui::Ui,
    utils::{
        check_if_exists, generate_copy_file_dir_name, get_curr_path, get_file_path_data,
        get_inner_files_info, init, is_file, SortBy, SortType,
    },
};

#[derive(Debug, Clone, PartialEq, Eq)]
struct StartupPaths {
    file_list_path: String,
    watcher_path: String,
    cache_path: String,
    refresh_fallback_path: String,
}

fn startup_paths(effective_config: &file_finder::cli::EffectiveConfig) -> StartupPaths {
    let start_path = effective_config.start_path.to_string_lossy().to_string();

    StartupPaths {
        file_list_path: start_path.clone(),
        watcher_path: start_path.clone(),
        cache_path: start_path.clone(),
        refresh_fallback_path: start_path,
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse command line arguments using clap
    let cli_args = CliArgs::parse_args();

    // Log the effective CLI values
    debug!("CLI Arguments: Start path: {:?}, Theme: {:?}, Editor: {:?}, Path: {:?}, Reset Config: {}, Rebuild Cache: {}",
        cli_args.start, cli_args.theme, cli_args.editor, cli_args.path, cli_args.reset_config, cli_args.rebuild_cache);

    let ps = SyntaxSet::load_defaults_newlines();
    let ts = ThemeSet::load_defaults();

    // Handle subcommands first
    if let Some(command) = &cli_args.command {
        match command {
            cli::Commands::Init { shell } => {
                println!("{}", shell.generate_script());
                return Ok(());
            }
        }
    }

    init();

    // Check for --reset-config flag (from CLI args)
    if cli_args.reset_config {
        println!("Resetting configuration to defaults...");
        if let Err(e) = file_finder::config::reset_configuration() {
            eprintln!("Failed to reset configuration: {}", e);
            return Err(e.into());
        }
        println!("Configuration reset successfully!");
        return Ok(());
    }

    // Load configuration using new TOML system
    let settings = match file_finder::config::Settings::load() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Configuration error: {}", e);
            return Err(e.into());
        }
    };

    // Compute effective configuration using precedence rules
    let effective_config = match compute_effective_config(&cli_args, &settings) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("ff: {}", e);
            std::process::exit(1);
        }
    };

    debug!(
        "Using effective configuration: start_path={}, theme={:?}, editor={:?}",
        effective_config.start_path.display(),
        effective_config.theme,
        effective_config.editor
    );
    let startup_paths = startup_paths(&effective_config);

    let mut file_reader_content = FileContent::new(ps, ts);
    // Apply syntax theme from settings
    file_reader_content.set_syntax_theme(&settings.syntax_theme);
    let mut image_renderer = ImageRenderer::new();
    // Setup terminal

    // Initial file list uses default sort (will be stored in app.sort_by/sort_type)
    let file_strings = get_file_path_data(
        startup_paths.file_list_path.clone(),
        false,
        SortBy::Default,
        &SortType::ASC,
    )?;
    let mut app = App::new(file_strings.clone());
    app.current_directory = startup_paths.file_list_path.clone();
    app.max_search_results = settings.finder.max_results;

    // Initialize config and theme after app creation
    if let Err(e) = app.initialize_config_and_theme() {
        eprintln!("Configuration error: {}", e);
        return Err(e.into());
    }

    // Start file system watching for the initial directory
    if let Err(e) = app.start_watching_directory(&startup_paths.watcher_path) {
        debug!(
            "Failed to start watching directory '{}': {}",
            startup_paths.watcher_path, e
        );
    } else {
        debug!("Started watching directory: {}", startup_paths.watcher_path);
    }

    // Set editor from effective configuration (CLI args have precedence)
    if let Some(editor) = effective_config.editor {
        let ide_selection = match editor {
            file_finder::cli::Editor::Nvim => app::IDE::NVIM,
            file_finder::cli::Editor::Vscode => app::IDE::VSCODE,
            file_finder::cli::Editor::Zed => app::IDE::ZED,
        };
        app.selected_id = Some(ide_selection);
        debug!("Set editor from CLI: {:?}", editor);
    }

    // Set print_path mode from CLI
    app.print_path = effective_config.print_path;

    let mut keymap_runtime = KeymapRuntime::from_settings(&settings.keymap);
    let root_search_keys = configured_action_key_labels(&settings.keymap, Action::SearchRoot);
    let find_hint = match root_search_keys.first() {
        Some(root_key) => format!(" find (i)  global ({})", root_key),
        None => " find (i)".to_string(),
    };

    let mut store = if !cli_args.rebuild_cache && Path::new(&settings.cache_directory).exists() {
        match load_directory_from_file(&settings.cache_directory.to_owned()) {
            Ok(res) => {
                debug!("Loading directory cache from file");
                res
            }
            Err(e) => {
                eprintln!("Warning: Could not load cache file ({}), rebuilding...", e);
                // Fall through to rebuild cache
                DirectoryStore::new()
            }
        }
    } else {
        debug!("Starting asynchronous directory cache build...");

        // Start async cache building
        let start_time = std::time::Instant::now();
        let rx = build_directory_from_store_async(
            startup_paths.cache_path.clone(),
            settings.ignore_directories.clone(),
        );

        // Set up the app to display loading progress and record start time
        app.start_cache_loading(rx, start_time);

        // Return empty store for now - it will be populated when cache building completes
        file_finder::directory_store::DirectoryStore {
            directories: Vec::new(),
        }
    };

    let widgets_ui = Ui::new(app.files.clone(), &settings);
    let mut status_bar = StatusBar::new();
    if let Some(notice) = effective_config.start_notice {
        status_bar.show_warning(notice, None);
    }

    debug!("{:?}", widgets_ui.files_list);

    let ui_thread_id = std::thread::current().id();
    let original_panic_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        if std::thread::current().id() == ui_thread_id {
            let _ = disable_raw_mode();
            let _ = execute!(
                io::stdout(),
                LeaveAlternateScreen,
                DisableMouseCapture,
                crossterm::cursor::Show
            );
        }
        original_panic_hook(panic_info);
    }));
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    terminal.clear()?;
    // Initial selected state
    let mut state = ListState::default();
    state.select(Some(0)); // Select the first item by default
    debug!("{:?}", state);
    let mut read_only_state = ListState::default();
    read_only_state.select(Some(0));

    // Copy progress receiver (for async copy operations)
    let mut copy_receiver: Option<mpsc::Receiver<CopyMessage>> = None;

    // Delete progress receiver (for async delete operations)
    let mut delete_receiver: Option<mpsc::Receiver<DeleteMessage>> = None;

    // File chosen with Enter, handled after the terminal is restored
    let mut selected_file: Option<String> = None;
    let mut quit_error = None;

    // Main loop
    loop {
        // Update status bar with current app state
        status_bar.update(&app);

        background::start_debounced_preview(
            &mut app,
            &mut file_reader_content,
            &mut image_renderer,
        );
        background::poll_preview_result(&mut app, &mut file_reader_content, &mut image_renderer);
        background::poll_image_result(&mut app, &file_reader_content, &mut image_renderer);

        render::frame::draw(
            &mut terminal,
            &mut app,
            &mut state,
            &file_reader_content,
            &mut image_renderer,
            &settings,
            &status_bar,
            &widgets_ui,
            &find_hint,
        )?;

        background::refresh_after_copy(
            &mut app,
            &mut state,
            &mut status_bar,
            &mut copy_receiver,
            &startup_paths.refresh_fallback_path,
        );

        background::poll_job(
            &mut app,
            &mut state,
            &mut status_bar,
            &startup_paths.refresh_fallback_path,
        );
        background::poll_delete(
            &mut app,
            &mut state,
            &mut status_bar,
            &mut delete_receiver,
            &startup_paths.refresh_fallback_path,
        );
        background::poll_file_system_events(
            &mut app,
            &mut state,
            &startup_paths.refresh_fallback_path,
        );
        background::poll_cache_loading(
            &mut app,
            &mut store,
            &settings,
            &startup_paths.refresh_fallback_path,
        );
        background::sync_selection(
            &mut app,
            &mut state,
            &mut file_reader_content,
            &mut image_renderer,
        );

        // Handle input with timeout for dynamic progress updates
        let timeout = std::time::Duration::from_millis(100); // 100ms timeout
        if let Ok(available) = crossterm_event::poll(timeout) {
            if available {
                if let Event::Key(key) = crossterm_event::read()? {
                    match app.input_mode {
                        InputMode::Normal => {
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
                                continue;
                            }

                            match key.code {
                                KeyCode::Char('q') => {
                                    if may_exit(
                                        &mut app,
                                        &mut status_bar,
                                        "q",
                                        &mut copy_receiver,
                                        &mut terminal,
                                        &mut quit_error,
                                    ) {
                                        break;
                                    }
                                }
                                KeyCode::Down | KeyCode::Char('j') => {
                                    // In DualPane mode with right pane active, navigate right pane
                                    if app.view_mode.is_dual_pane() && app.is_right_pane_active() {
                                        app.right_pane.navigate_down();
                                    } else {
                                        let list_len = app.get_list_length();

                                        if let Some(i) =
                                            next_index_wrapping(state.selected(), list_len)
                                        {
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

                                        if let Some(i) =
                                            previous_index_wrapping(state.selected(), list_len)
                                        {
                                            state.select(Some(i));
                                            app.curr_index = Some(i);
                                        }
                                    }
                                }
                                KeyCode::Char('h') => {
                                    // In DualPane mode with right pane active, navigate right pane to parent
                                    if app.view_mode.is_dual_pane() && app.is_right_pane_active() {
                                        let current_dir = app.right_pane.current_directory.clone();
                                        if let Some(parent) =
                                            parent_directory(Path::new(&current_dir))
                                        {
                                            if let Some(parent_str) = parent.to_str() {
                                                app.right_pane.navigate_to_directory(parent_str);
                                            }
                                        }
                                    } else if let Some(selected) =
                                        app.get_selected_path(state.selected())
                                    {
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
                                                    if let Err(e) =
                                                        app.start_watching_directory(&new_path)
                                                    {
                                                        debug!(
                                                    "Failed to start watching directory '{}': {}",
                                                    new_path, e
                                                );
                                                    } else {
                                                        debug!(
                                                            "Started watching directory: {}",
                                                            new_path
                                                        );
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

                                                            app.read_only_files =
                                                                files_strs.clone();
                                                            app.files = files_strs;
                                                            app.update_file_references();
                                                            state.select(Some(0));

                                                            // Start watching the new directory
                                                            if let Err(e) = app
                                                                .start_watching_directory(&selected)
                                                            {
                                                                debug!("Failed to start watching directory '{}': {}", selected, e);
                                                            } else {
                                                                debug!(
                                                                "Started watching directory: {}",
                                                                selected
                                                            );
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
                                    if let Some(selected) = app.get_selected_path(state.selected())
                                    {
                                        // Opening leaves ff, so it waits for confirmation during an operation.
                                        if !may_exit(
                                            &mut app,
                                            &mut status_bar,
                                            "o",
                                            &mut copy_receiver,
                                            &mut terminal,
                                            &mut quit_error,
                                        ) {
                                            continue;
                                        }
                                        // Spawn without waiting; null stdio keeps opener output off the TUI.
                                        match Command::new(system_opener())
                                            .arg(&selected)
                                            .stdin(Stdio::null())
                                            .stdout(Stdio::null())
                                            .stderr(Stdio::null())
                                            .spawn()
                                        {
                                            Ok(_) => break,
                                            Err(e) => status_bar.show_error(
                                                format!(
                                                    "Failed to open with {}: {}",
                                                    system_opener(),
                                                    e
                                                ),
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
                                    if app.view_mode == ViewMode::DualPane
                                        && new_mode != ViewMode::DualPane
                                    {
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
                                        } else if let Some((src_path, dest_dir)) =
                                            app.other_pane_transfer(state.selected())
                                        {
                                            let src = Path::new(&src_path);
                                            if let Some(src_name) = src.file_name() {
                                                let dest = Path::new(&dest_dir).join(src_name);
                                                app.preserved_selection_index = state.selected();
                                                app.start_copy(
                                                    dest.clone(),
                                                    Some(src_name.to_string_lossy().to_string()),
                                                );
                                                copy_receiver =
                                                    Some(copy_dir_file_with_progress_cancellable(
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
                                        if let Some((src_path, dest_dir)) =
                                            app.other_pane_transfer(state.selected())
                                        {
                                            let name = Path::new(&src_path)
                                                .file_name()
                                                .unwrap_or_default()
                                                .to_string_lossy()
                                                .to_string();
                                            let status = format!("Moving '{name}'…");
                                            let started =
                                                app.start_job("move", status, move |cancel| {
                                                    operations::move_file_or_dir_cancellable(
                                                        &src_path, &dest_dir, &cancel,
                                                    )
                                                    .map(|()| {
                                                        format!("Moved '{name}' to other pane")
                                                    })
                                                    .map_err(|e| format!("Move failed: {e}"))
                                                });
                                            if let Err(warning) = started {
                                                status_bar.show_warning(warning, None);
                                            }
                                        }
                                    }
                                }
                                KeyCode::Char('y') => {
                                    if let Some(selected) = app.get_selected_path(state.selected())
                                    {
                                        let file_type = file_reader_content
                                            .get_file_extension(selected.clone());
                                        if let FileType::ZIP = file_type {
                                            let name = Path::new(&selected)
                                                .file_name()
                                                .unwrap_or_default()
                                                .to_string_lossy()
                                                .to_string();
                                            let status = format!("Extracting '{name}'…");
                                            let started = app.start_job(
                                                "extraction",
                                                status,
                                                move |cancel| {
                                                    FileContent::extract_zip_content_cancellable(
                                                        &selected, &cancel,
                                                    )
                                                },
                                            );
                                            if let Err(warning) = started {
                                                status_bar.show_warning(warning, None);
                                            }
                                        }
                                    }
                                }
                                KeyCode::Char('r') => {
                                    if let Some(selected) = app.get_selected_path(state.selected())
                                    {
                                        if let Some((parent, placeholder_name)) =
                                            rename_target(Path::new(&selected))
                                        {
                                            app.current_path_to_edit =
                                                parent.to_string_lossy().to_string();
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
                                    let refresh_dir = current_refresh_directory(
                                        &app,
                                        &startup_paths.refresh_fallback_path,
                                    );
                                    match get_file_path_data(
                                        refresh_dir,
                                        app.show_hidden_files,
                                        app.sort_by.clone(),
                                        &app.sort_type,
                                    ) {
                                        Ok(files) => {
                                            app.refresh_files_from_watcher(files, &mut state);
                                            status_bar.invalidate_cache();
                                        }
                                        Err(e) => status_bar.show_error(
                                            format!("Could not refresh the listing: {e}"),
                                            None,
                                        ),
                                    }
                                }
                                KeyCode::Char('c') => {
                                    // Copy the highlighted item beside its source.
                                    if let Some(warning) = app.busy_warning() {
                                        status_bar.show_warning(warning, None);
                                    } else if let Some(selected) =
                                        app.get_selected_path(state.selected())
                                    {
                                        let src_path = Path::new(&selected);
                                        let current_dir = containing_directory_or_current(src_path);
                                        let new_src =
                                            generate_copy_file_dir_name(src_path, &current_dir);
                                        app.preserved_selection_index = state.selected();
                                        app.start_copy(new_src.clone(), None);
                                        copy_receiver =
                                            Some(copy_dir_file_with_progress_cancellable(
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
                                            &mut app,
                                            &mut status_bar,
                                            "Enter",
                                            &mut copy_receiver,
                                            &mut terminal,
                                            &mut quit_error,
                                        ) {
                                            continue;
                                        }
                                        app.input = selected.clone();

                                        // Check if print_path mode - hand the path to the shell integration
                                        if app.print_path {
                                            // Write to $FF_RESULT_FILE (set by `ff init`) before restoring terminal
                                            let wrote_result_file = std::env::var_os(
                                                "FF_RESULT_FILE",
                                            )
                                            .is_some_and(|result_file| {
                                                write_result_file(
                                                    Path::new(&result_file),
                                                    &selected,
                                                )
                                                .is_ok()
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
                                            return Ok(());
                                        }

                                        // Open in the IDE or copy to the clipboard once the terminal is restored
                                        selected_file = Some(selected);
                                        break;
                                    }
                                }
                                _ => {}
                            }
                        }

                        InputMode::WatchRename if key.kind == KeyEventKind::Press => match key.code
                        {
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
                                    let new_path = format!(
                                        "{}/{}",
                                        app.current_path_to_edit, app.create_edit_file_name
                                    );
                                    if !check_if_exists(new_path.clone()) {
                                        match handle_rename(&app) {
                                            Ok(_) => {
                                                let current_dir = current_refresh_directory(
                                                    &app,
                                                    &startup_paths.refresh_fallback_path,
                                                );
                                                app.reset_create_edit_values();
                                                let file_path_list = get_file_path_data(
                                                    current_dir,
                                                    app.show_hidden_files,
                                                    app.sort_by.clone(),
                                                    &app.sort_type,
                                                )?;
                                                app.refresh_files_after_operation(
                                                    file_path_list,
                                                    &mut state,
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
                        },
                        InputMode::WatchCreate if key.kind == KeyEventKind::Press => match key.code
                        {
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
                                match create_item_based_on_type(
                                    new_path.clone(),
                                    app.create_edit_file_name.clone(),
                                ) {
                                    Ok(_) => {
                                        app.input_mode = InputMode::Normal;

                                        let created_path = created_item_path(
                                            Path::new(&new_path),
                                            &app.create_edit_file_name,
                                        );
                                        app.reset_create_edit_values();
                                        refresh_file_list(
                                            &mut app,
                                            Path::new(&new_path),
                                            &mut state,
                                            created_path,
                                        )?;
                                        status_bar.invalidate_cache();
                                    }
                                    Err(e) => {
                                        app.is_create_edit_error = true;
                                        app.error_message = e.user_message();
                                    }
                                }
                            }
                            _ => {}
                        },

                        InputMode::Editing if key.kind == KeyEventKind::Press => {
                            handle_search_key(&mut app, key.code, &store);
                        }
                        InputMode::WatchDelete => match key.code {
                            KeyCode::Char('q') => {
                                app.close_delete_popup();
                                if may_exit(
                                    &mut app,
                                    &mut status_bar,
                                    "q",
                                    &mut copy_receiver,
                                    &mut terminal,
                                    &mut quit_error,
                                ) {
                                    break;
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
                                            app.delete_progress_message =
                                                "Counting files...".to_string();
                                            app.preserved_selection_index = state.selected();
                                            delete_receiver = Some(delete_with_progress(&selected));
                                        }
                                    } else {
                                        // A file removed externally already satisfies the request.
                                        let delete_result = handle_delete_based_on_type(&selected);
                                        let is_success = match &delete_result {
                                            Ok(_) => true,
                                            Err(e) => e.is_not_found(),
                                        };
                                        if is_success {
                                            let current_dir = current_refresh_directory(
                                                &app,
                                                &startup_paths.refresh_fallback_path,
                                            );
                                            let files = get_file_path_data(
                                                current_dir,
                                                app.show_hidden_files,
                                                app.sort_by.clone(),
                                                &app.sort_type,
                                            )?;
                                            app.refresh_files_after_operation(
                                                files, &mut state, None,
                                            );
                                            status_bar.invalidate_cache();
                                        } else if let Err(e) = delete_result {
                                            status_bar.show_error(e.user_message(), None);
                                            warn!("Delete operation failed: {}", e.user_message());
                                        }
                                    }
                                }
                            }
                            _ => {}
                        },
                        InputMode::WatchSort => match key.code {
                            KeyCode::Char('q') => {
                                app.input_mode = InputMode::Normal;
                            }
                            KeyCode::Char('n') => {
                                apply_sort(&mut app, SortBy::Name)?;
                            }
                            KeyCode::Char('s') => {
                                apply_sort(&mut app, SortBy::Size)?;
                            }
                            KeyCode::Char('t') => {
                                apply_sort(&mut app, SortBy::DateAdded)?;
                            }
                            KeyCode::Char('a') => {
                                app.sort_type = SortType::ASC;
                            }
                            KeyCode::Char('d') => {
                                app.sort_type = SortType::DESC;
                            }
                            _ => {}
                        },

                        InputMode::WatchKeyBinding => {
                            if let KeyCode::Char('q') = key.code {
                                app.input_mode = InputMode::Normal;
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;

    terminal.show_cursor()?;
    terminal.clear()?;

    if let Some(error) = quit_error {
        eprintln!("ff: {error}");
    }
    if let Some(file) = selected_file {
        if let Err(message) = handle_file_selection(&file, app.get_selected_ide()) {
            eprintln!("ff: {}", message);
            std::process::exit(1);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

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
    fn startup_paths_use_effective_start_path_for_display_watch_and_cache() {
        let effective_config = file_finder::cli::EffectiveConfig {
            start_path: PathBuf::from("/tmp/ff-cli-start"),
            theme: None,
            editor: None,
            layout_style: file_finder::config::settings::LayoutStyle::Modern,
            print_path: false,
            start_notice: None,
        };

        let paths = startup_paths(&effective_config);

        assert_eq!(paths.file_list_path, "/tmp/ff-cli-start");
        assert_eq!(paths.watcher_path, "/tmp/ff-cli-start");
        assert_eq!(paths.cache_path, "/tmp/ff-cli-start");
        assert_eq!(paths.refresh_fallback_path, "/tmp/ff-cli-start");
    }
}
