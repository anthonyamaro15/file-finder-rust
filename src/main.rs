// ============================================================
// File Finder (ff) - Terminal File Browser
// ============================================================

// Standard library imports
use std::{
    io,
    path::{Path, PathBuf},
    time::SystemTime,
};

// External crate imports
use crossterm::{
    event::{
        self as crossterm_event, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use log::{debug, error};
use ratatui::{backend::CrosstermBackend, widgets::ListState, Terminal};
use syntect::{highlighting::ThemeSet, parsing::SyntaxSet};

// Internal imports
use file_finder::exit::handle_file_selection;
use file_finder::{app, cli, event::background, event::normal, event::popups, render};
use file_finder::{
    app::{App, InputMode},
    cli::{compute_effective_config, CliArgs},
    config::Action,
    directory_store::{
        build_directory_from_store_async, DirectoryIndex, DirectoryStore, REFRESH_THREADS,
    },
    event::{
        keymap::KeymapRuntime, search::handle_search_key, transfer::handle_folder_picker_key,
        LoopControl, LoopState,
    },
    file_reader_content::FileContent,
    render::{configured_action_key_labels, image_renderer::ImageRenderer},
    status_bar::StatusBar,
    ui::Ui,
    utils::{get_file_path_data, init, SortBy, SortType},
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

/// Quitting during an operation (confirmed after its warning) can stop it before ff
/// sees its folder changes, so the next start refreshes the folder index.
fn mark_index_stale_after_interrupted_quit(loop_state: &mut LoopState) {
    if loop_state.app.quit_warning.is_some() {
        loop_state.index.mark_stale();
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
    let image_renderer = ImageRenderer::new();
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

    let cache_path = PathBuf::from(&settings.cache_directory);
    let loaded = if !cli_args.rebuild_cache && cache_path.exists() {
        match DirectoryIndex::load(
            &cache_path,
            Path::new(&startup_paths.cache_path),
            settings.ignore_directories.clone(),
        ) {
            Ok(index) => {
                debug!("Loading directory cache from file");
                Some(index)
            }
            Err(e) => {
                eprintln!("Warning: Could not load cache file ({}), rebuilding...", e);
                None
            }
        }
    } else {
        None
    };
    let index = match loaded {
        Some(mut index) => {
            // A cache read a while ago misses folders changed outside ff; re-read
            // them quietly while ff runs.
            if index.refresh_if_stale(SystemTime::now(), REFRESH_THREADS) {
                debug!("Refreshing directory cache in the background");
            }
            index
        }
        None => {
            debug!("Starting asynchronous directory cache build...");

            // Start async cache building
            let start_time = std::time::Instant::now();
            let rx = build_directory_from_store_async(
                startup_paths.cache_path.clone(),
                settings.ignore_directories.clone(),
                num_cpus::get(),
            );

            // Set up the app to display loading progress and record start time
            app.start_cache_loading(rx, start_time);

            // Empty for now - it will be populated when cache building completes
            DirectoryIndex::new(
                DirectoryStore::new(),
                PathBuf::from(&startup_paths.cache_path),
                settings.ignore_directories.clone(),
                Some(cache_path),
            )
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
            original_panic_hook(panic_info);
        } else {
            error!("Background thread panicked: {panic_info}");
        }
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

    let mut loop_state = LoopState {
        app,
        state,
        file_reader_content,
        image_renderer,
        status_bar,
        index,
        copy_receiver: None,
        delete_receiver: None,
        selected_file: None,
        quit_error: None,
    };

    // Main loop
    loop {
        // Update status bar with current app state
        loop_state.status_bar.update(&loop_state.app);

        background::start_debounced_preview(
            &mut loop_state.app,
            &mut loop_state.file_reader_content,
            &mut loop_state.image_renderer,
        );
        background::poll_preview_result(
            &mut loop_state.app,
            &mut loop_state.file_reader_content,
            &mut loop_state.image_renderer,
        );
        background::poll_image_result(
            &mut loop_state.app,
            &loop_state.file_reader_content,
            &mut loop_state.image_renderer,
        );

        render::frame::draw(
            &mut terminal,
            &mut loop_state,
            &settings,
            &widgets_ui,
            &find_hint,
        )?;

        background::refresh_after_copy(
            &mut loop_state.app,
            &mut loop_state.state,
            &mut loop_state.status_bar,
            &mut loop_state.copy_receiver,
            &mut loop_state.index,
            &startup_paths.refresh_fallback_path,
        );

        background::poll_job(
            &mut loop_state.app,
            &mut loop_state.state,
            &mut loop_state.status_bar,
            &mut loop_state.index,
            &startup_paths.refresh_fallback_path,
        );
        background::poll_delete(
            &mut loop_state.app,
            &mut loop_state.state,
            &mut loop_state.status_bar,
            &mut loop_state.delete_receiver,
            &mut loop_state.index,
            &startup_paths.refresh_fallback_path,
        );
        background::poll_file_system_events(
            &mut loop_state.app,
            &mut loop_state.state,
            &startup_paths.refresh_fallback_path,
        );
        background::poll_cache_loading(
            &mut loop_state.app,
            &mut loop_state.index,
            &startup_paths.refresh_fallback_path,
        );
        loop_state.index.poll_refresh();
        background::sync_selection(
            &mut loop_state.app,
            &mut loop_state.state,
            &mut loop_state.file_reader_content,
            &mut loop_state.image_renderer,
        );

        // Handle input with timeout for dynamic progress updates
        let timeout = std::time::Duration::from_millis(100); // 100ms timeout
        if let Ok(available) = crossterm_event::poll(timeout) {
            if available {
                if let Event::Key(key) = crossterm_event::read()? {
                    match loop_state.app.input_mode {
                        InputMode::Normal => match normal::handle_normal_key(
                            key,
                            &mut loop_state,
                            &mut keymap_runtime,
                            &mut terminal,
                            &startup_paths.refresh_fallback_path,
                        )? {
                            LoopControl::Continue => continue,
                            LoopControl::Break => break,
                            LoopControl::Return => {
                                mark_index_stale_after_interrupted_quit(&mut loop_state);
                                return Ok(());
                            }
                        },

                        InputMode::WatchRename if key.kind == KeyEventKind::Press => {
                            popups::handle_rename_key(
                                key,
                                &mut loop_state.app,
                                &mut loop_state.state,
                                &mut loop_state.status_bar,
                                &mut loop_state.index,
                                &startup_paths.refresh_fallback_path,
                            )?
                        }
                        InputMode::WatchCreate if key.kind == KeyEventKind::Press => {
                            popups::handle_create_key(
                                key,
                                &mut loop_state.app,
                                &mut loop_state.state,
                                &mut loop_state.status_bar,
                                &mut loop_state.index,
                            )?
                        }

                        InputMode::Editing if key.kind == KeyEventKind::Press => {
                            handle_search_key(
                                &mut loop_state.app,
                                key.code,
                                &loop_state.index.store,
                            );
                        }
                        InputMode::WatchDelete => {
                            if popups::handle_delete_key(
                                key,
                                &mut loop_state,
                                &mut terminal,
                                &startup_paths.refresh_fallback_path,
                            )? {
                                break;
                            }
                        }
                        InputMode::WatchSort => popups::handle_sort_key(key, &mut loop_state.app)?,

                        InputMode::WatchKeyBinding => {
                            popups::handle_keybinding_key(key, &mut loop_state.app)
                        }
                        InputMode::PickFolder if key.kind == KeyEventKind::Press => {
                            handle_folder_picker_key(
                                key,
                                &mut loop_state.app,
                                loop_state.state.selected(),
                                &mut loop_state.status_bar,
                                &loop_state.index.store,
                                &mut loop_state.copy_receiver,
                            )
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    mark_index_stale_after_interrupted_quit(&mut loop_state);

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;

    terminal.show_cursor()?;
    terminal.clear()?;

    if let Some(error) = loop_state.quit_error {
        eprintln!("ff: {error}");
    }
    if let Some(file) = loop_state.selected_file {
        if let Err(message) = handle_file_selection(&file, loop_state.app.get_selected_ide()) {
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
