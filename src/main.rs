// ============================================================
// File Finder (ff) - Terminal File Browser
// ============================================================

// Standard library imports
use std::{io, path::Path, sync::mpsc};

// External crate imports
use crossterm::{
    event::{
        self as crossterm_event, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use log::debug;
use ratatui::{backend::CrosstermBackend, widgets::ListState, Terminal};
use syntect::{highlighting::ThemeSet, parsing::SyntaxSet};

// Internal imports
use file_finder::exit::handle_file_selection;
use file_finder::{app, cli, event::background, event::normal, event::popups, render};
use file_finder::{
    app::{App, InputMode},
    cli::{compute_effective_config, CliArgs},
    config::Action,
    directory_store::{build_directory_from_store_async, load_directory_from_file, DirectoryStore},
    event::{keymap::KeymapRuntime, search::handle_search_key, LoopControl},
    file_reader_content::FileContent,
    operations::{CopyMessage, DeleteMessage},
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
                        InputMode::Normal => match normal::handle_normal_key(
                            key,
                            &mut app,
                            &mut state,
                            &mut file_reader_content,
                            &mut image_renderer,
                            &mut keymap_runtime,
                            &mut status_bar,
                            &mut copy_receiver,
                            &mut terminal,
                            &mut quit_error,
                            &mut selected_file,
                            &startup_paths.refresh_fallback_path,
                        )? {
                            LoopControl::Continue => continue,
                            LoopControl::Break => break,
                            LoopControl::Return => return Ok(()),
                        },

                        InputMode::WatchRename if key.kind == KeyEventKind::Press => {
                            popups::handle_rename_key(
                                key,
                                &mut app,
                                &mut state,
                                &mut status_bar,
                                &startup_paths.refresh_fallback_path,
                            )?
                        }
                        InputMode::WatchCreate if key.kind == KeyEventKind::Press => {
                            popups::handle_create_key(key, &mut app, &mut state, &mut status_bar)?
                        }

                        InputMode::Editing if key.kind == KeyEventKind::Press => {
                            handle_search_key(&mut app, key.code, &store);
                        }
                        InputMode::WatchDelete => {
                            if popups::handle_delete_key(
                                key,
                                &mut app,
                                &mut state,
                                &mut status_bar,
                                &mut copy_receiver,
                                &mut delete_receiver,
                                &mut terminal,
                                &mut quit_error,
                                &startup_paths.refresh_fallback_path,
                            )? {
                                break;
                            }
                        }
                        InputMode::WatchSort => popups::handle_sort_key(key, &mut app)?,

                        InputMode::WatchKeyBinding => popups::handle_keybinding_key(key, &mut app),
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
