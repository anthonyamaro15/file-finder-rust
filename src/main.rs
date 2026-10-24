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
use copypasta::{ClipboardContext, ClipboardProvider};
use crossterm::{
    event::{
        self as crossterm_event, DisableMouseCapture, EnableMouseCapture, Event, KeyCode,
        KeyEventKind,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use log::{debug, warn};
use ratatui::{
    backend::CrosstermBackend,
    prelude::*,
    style::{Color, Style},
    symbols::border,
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
    Terminal,
};
use ratatui_image::{picker::Picker, protocol::StatefulProtocol, StatefulImage};
use syntect::{highlighting::ThemeSet, parsing::SyntaxSet};

// Internal imports
use file_finder::{app, cli, file_reader_content, operations};
use file_finder::{
    app::{App, InputMode, PanePosition, SearchScope, ViewMode},
    cli::{compute_effective_config, CliArgs},
    config::Action,
    directory_store::{build_directory_from_store_async, load_directory_from_file, DirectoryStore},
    event::{
        file_actions::{
            containing_directory_or_current, create_target_directory, created_item_path,
            refresh_file_list, rename_target,
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
    render::{
        configured_action_key_labels, create_cache_loading_screen, create_create_input_popup,
        create_delete_confirmation_popup, create_keybindings_popup, create_rename_input_popup,
        create_sort_options_popup, draw_input_popup, draw_popup, input_popup_cursor_position,
        split_popup_area_vertical,
    },
    status_bar::StatusBar,
    theme::OneDarkTheme,
    ui::{create_preview_block, Ui},
    utils::{
        check_if_exists, format_file_size, generate_copy_file_dir_name, get_curr_path,
        get_file_path_data, get_inner_files_info, init, is_file, SortBy, SortType,
    },
};

/// Default font size for terminal graphics protocol picker (width, height in pixels)
const DEFAULT_FONT_SIZE: (u16, u16) = (8, 16);

/// Preview debounce delay in milliseconds
/// Prevents rapid preview updates when scrolling quickly through files
const PREVIEW_DEBOUNCE_MS: u64 = 50;

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

/// Directory to reload the left listing from; the right pane refreshes itself.
fn current_refresh_directory(app: &App, fallback_start_path: &str) -> String {
    let current_directory = app.left_directory_path();

    if current_directory == Path::new(".") {
        fallback_start_path.to_string()
    } else {
        current_directory.to_string_lossy().to_string()
    }
}

/// Drain copy updates, report terminal results, and return whether a copy succeeded.
fn poll_copy_messages(
    app: &mut App,
    status_bar: &mut StatusBar,
    receiver: &mut Option<mpsc::Receiver<CopyMessage>>,
) -> bool {
    let Some(rx) = receiver.as_ref() else {
        return false;
    };
    // Coalesce status/progress updates, but never discard a terminal result.
    let mut last_message = None;
    loop {
        match rx.try_recv() {
            Ok(message) => {
                let finished = matches!(
                    message,
                    CopyMessage::Completed { .. } | CopyMessage::Error(_)
                );
                last_message = Some(message);
                if finished {
                    break;
                }
            }
            Err(mpsc::TryRecvError::Empty) => break,
            Err(mpsc::TryRecvError::Disconnected) => {
                last_message = Some(CopyMessage::Error(
                    "Copy stopped unexpectedly before reporting a result".into(),
                ));
                break;
            }
        }
    }

    let success = match last_message {
        None => return false,
        Some(CopyMessage::Status(message)) => {
            app.copy_progress_message = message;
            return false;
        }
        Some(CopyMessage::Progress {
            files_processed,
            total_files,
            bytes_copied,
            total_bytes,
            current_file,
        }) => {
            app.copy_files_processed = files_processed;
            app.copy_total_files = total_files;
            let percent = if total_bytes > 0 {
                (bytes_copied as f64 / total_bytes as f64 * 100.0) as u8
            } else {
                0
            };
            app.copy_progress_message = format!(
                "Copying: {} ({}% - {} / {})",
                current_file,
                percent,
                format_file_size(bytes_copied),
                format_file_size(total_bytes),
            );
            return false;
        }
        Some(CopyMessage::Completed {
            success,
            message,
            skipped_special_files,
        }) => {
            // The engine names the destination, which a cross-pane copy shares with its source.
            let message = app
                .copy_to_other_pane
                .as_deref()
                .and_then(|name| {
                    let details = message.strip_prefix(&format!("Copied '{name}' to '{name}'"))?;
                    Some(format!("Copied '{name}' to other pane{details}"))
                })
                .unwrap_or(message);
            if success {
                debug!("Copy completed: {}", message);
                if skipped_special_files > 0 {
                    status_bar.show_warning(message, None);
                } else {
                    status_bar.show_info(message, None);
                }
            } else {
                warn!("Copy failed: {}", message);
                status_bar.show_error(message, None);
            }
            success
        }
        Some(CopyMessage::Error(message)) => {
            warn!("Copy operation failed: {}", message);
            status_bar.show_error(message, None);
            false
        }
    };

    app.copy_in_progress = false;
    app.copy_progress_message.clear();
    app.copy_files_processed = 0;
    app.copy_total_files = 0;
    app.copy_to_other_pane = None;
    app.copy_destination = None;
    *receiver = None;
    success
}

/// Refresh completed copies and select an in-place duplicate when visible.
fn refresh_after_copy(
    app: &mut App,
    state: &mut ListState,
    status_bar: &mut StatusBar,
    receiver: &mut Option<mpsc::Receiver<CopyMessage>>,
    fallback_start_path: &str,
) {
    let destination = app
        .copy_destination
        .as_ref()
        .filter(|_| app.copy_to_other_pane.is_none())
        .map(|path| path.to_string_lossy().into_owned());
    if poll_copy_messages(app, status_bar, receiver) {
        let refresh_dir = current_refresh_directory(app, fallback_start_path);
        if let Ok(files) = get_file_path_data(
            refresh_dir,
            app.show_hidden_files,
            app.sort_by.clone(),
            &app.sort_type,
        ) {
            app.refresh_files_after_operation(files, state, destination);
            status_bar.invalidate_cache();
        }
        // Shift+C may have copied into the right pane's folder.
        if app.view_mode.is_dual_pane() {
            app.right_pane.refresh_files();
        }
    }
}

/// Confirm once, then cancel and wait for cleanup before any exit route proceeds.
fn may_exit<B: Backend>(
    app: &mut App,
    status_bar: &mut StatusBar,
    key: &'static str,
    receiver: &mut Option<mpsc::Receiver<CopyMessage>>,
    terminal: &mut Terminal<B>,
    quit_error: &mut Option<String>,
) -> bool {
    use std::sync::atomic::Ordering;

    if let Err(warning) = app.confirm_quit(key, std::time::Instant::now()) {
        status_bar.show_warning(warning, None);
        return false;
    }
    let Some(operation) = app.busy_operation().filter(|kind| *kind != "delete") else {
        return true;
    };

    app.copy_cancel.store(true, Ordering::Relaxed);
    if let Some(job) = &app.job {
        job.cancel.store(true, Ordering::Relaxed);
    }
    let message = format!("Stopping the {operation} and removing partial files…");
    status_bar.show_info(message.clone(), None);
    // A draw failure must not bypass the wait and leave a worker writing.
    if let Err(error) = terminal.draw(|frame| {
        frame.render_widget(
            Paragraph::new(message)
                .block(Block::default().borders(Borders::ALL).title("Quitting"))
                .wrap(ratatui::widgets::Wrap { trim: true }),
            frame.size(),
        );
    }) {
        warn!("Could not redraw while stopping {operation}: {error}");
    }

    let result = if let Some(rx) = receiver.take() {
        let result = loop {
            match rx.recv() {
                Ok(CopyMessage::Completed { success: true, .. }) => break Ok(String::new()),
                Ok(CopyMessage::Completed { message, .. } | CopyMessage::Error(message)) => {
                    break Err(message)
                }
                Ok(_) => {} // Progress is not an acknowledgement of cancellation.
                Err(_) => break Err("Copy stopped unexpectedly before reporting a result".into()),
            }
        };
        app.copy_in_progress = false;
        app.copy_progress_message.clear();
        app.copy_files_processed = 0;
        app.copy_total_files = 0;
        app.copy_to_other_pane = None;
        app.copy_destination = None;
        result
    } else if let Some(job) = app.job.take() {
        // A move already deleting its source deliberately ignores cancellation.
        job.wait()
    } else {
        Ok(String::new())
    };
    if let Err(error) = result {
        if error.contains("failed to remove partial copy") || !error.contains("cancelled") {
            *quit_error = Some(error);
        }
    }
    true
}

/// Image renderer that supports terminal graphics protocols (Sixel, Kitty, iTerm2)
/// with fallback to Unicode halfblocks for unsupported terminals.
struct ImageRenderer {
    /// Terminal graphics protocol picker
    picker: Picker,
    /// Current image protocol state for rendering
    image_state: Option<Box<dyn StatefulProtocol>>,
    /// Whether an image is currently loaded
    has_image: bool,
    /// Image metadata info (dimensions, format)
    image_info: String,
    /// Error message if image loading failed
    error_message: Option<String>,
}

impl ImageRenderer {
    /// Create a new ImageRenderer with terminal protocol detection
    pub fn new() -> ImageRenderer {
        let mut picker = Picker::new(DEFAULT_FONT_SIZE);
        with_stdin_state_restored(|| picker.guess_protocol());

        ImageRenderer {
            picker,
            image_state: None,
            has_image: false,
            image_info: String::new(),
            error_message: None,
        }
    }

    /// Load an image from a pre-decoded DynamicImage (for async loading)
    pub fn load_from_decoded(&mut self, decoded_image: image::DynamicImage, format: String) {
        self.clear();

        let width = decoded_image.width();
        let height = decoded_image.height();

        // Store metadata
        self.image_info = format!("{}x{} {}", width, height, format);

        // Create protocol state for rendering
        let protocol = self.picker.new_resize_protocol(decoded_image);
        self.image_state = Some(protocol);
        self.has_image = true;
        self.error_message = None;
    }

    /// Set an error message for image loading
    pub fn set_error(&mut self, message: String) {
        self.clear();
        self.error_message = Some(message);
    }

    /// Clear the current image
    pub fn clear(&mut self) {
        self.image_state = None;
        self.has_image = false;
        self.image_info.clear();
        self.error_message = None;
    }

    /// Check if an image is loaded and ready to render
    pub fn has_image(&self) -> bool {
        self.has_image && self.image_state.is_some()
    }

    /// Get the image info string
    pub fn get_info(&self) -> &str {
        &self.image_info
    }

    /// Get error message if any
    pub fn get_error(&self) -> Option<&str> {
        self.error_message.as_deref()
    }
}

/// Run `f`, then restore stdin's fd flags and termios to what they were before.
/// ratatui-image's graphics query sets O_NONBLOCK and clears ICANON/ECHO on stdin,
/// and returns early without restoring them when the terminal doesn't reply
/// (e.g. detached tmux), which breaks crossterm reads with WouldBlock.
#[cfg(unix)]
fn with_stdin_state_restored<T>(f: impl FnOnce() -> T) -> T {
    // SAFETY: isatty only inspects fd 0.
    if unsafe { libc::isatty(libc::STDIN_FILENO) } == 0 {
        return f();
    }
    // SAFETY: fd 0 is open; termios is a valid out-pointer for tcgetattr.
    let (flags, termios) = unsafe {
        let mut termios: libc::termios = std::mem::zeroed();
        let flags = libc::fcntl(libc::STDIN_FILENO, libc::F_GETFL);
        let saved = libc::tcgetattr(libc::STDIN_FILENO, &mut termios) == 0;
        (flags, saved.then_some(termios))
    };

    let result = f();

    // SAFETY: restores the flags and termios read from fd 0 above.
    unsafe {
        if flags != -1 {
            libc::fcntl(libc::STDIN_FILENO, libc::F_SETFL, flags);
        }
        if let Some(termios) = &termios {
            libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, termios);
        }
    }
    result
}

#[cfg(not(unix))]
fn with_stdin_state_restored<T>(f: impl FnOnce() -> T) -> T {
    f()
}

/// Program that opens a file with the system default application.
fn system_opener() -> &'static str {
    if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    }
}

/// Write the selected path to the shell integration's result file.
/// The file must already exist (the shell function creates it with mktemp).
fn write_result_file(result_file: &Path, selected: &str) -> io::Result<()> {
    use std::io::Write;
    std::fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(result_file)?
        .write_all(selected.as_bytes())
}

/// Open `file` with `editor` and wait for it to exit.
fn launch_editor(editor: &str, file: &str) -> Result<(), String> {
    if !Path::new(file).exists() {
        return Err(format!("'{}' no longer exists", file));
    }
    let status = Command::new(editor)
        .arg(file)
        .status()
        .map_err(|e| format!("failed to launch '{}': {}", editor, e))?;

    if !status.success() {
        return Err(format!("'{}' failed with {}", editor, status));
    }
    debug!("Opened {} with {}", file, editor);
    Ok(())
}

/// Open the selection in the configured editor, or copy its path to the clipboard.
/// Call after the terminal is restored; prints the path if the clipboard is unavailable.
fn handle_file_selection(file: &str, ide: Option<String>) -> Result<(), String> {
    if let Some(ide) = ide {
        return launch_editor(&ide, file);
    }

    match ClipboardContext::new().and_then(|mut ctx| ctx.set_contents(file.to_owned())) {
        Ok(()) => debug!("Copied path to clipboard: {}", file),
        Err(e) => {
            warn!("Clipboard unavailable: {}", e);
            eprintln!("ff: clipboard unavailable; path printed below");
            println!("{}", file);
        }
    }
    Ok(())
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

        // Check for debounced preview updates - use cache or spawn async loader
        if let Some(path) = app.take_ready_preview(PREVIEW_DEBOUNCE_MS) {
            // Results still loading for the previous selection must not overwrite this one
            app.cancel_preview_loads();

            // Check if this file is cached (for source code files with syntax highlighting)
            let is_file_path = std::path::Path::new(&path).is_file();
            let file_type = file_reader_content::detect_file_type(&path);
            let is_cacheable = matches!(
                file_type,
                FileType::SourceCode
                    | FileType::ConfigFile
                    | FileType::JSON
                    | FileType::Markdown
                    | FileType::TextFile
            );

            if is_file_path && is_cacheable && file_reader_content.is_cached(&path) {
                // Cache hit - use cached content directly (instant)
                file_reader_content.file_type = file_type;
                if file_reader_content.try_use_cached_preview(&path) {
                    app.preview_file_content.clear();
                    app.preview_files = Vec::new();
                    image_renderer.clear();
                }
            } else {
                // Cache miss - use async loading
                // Update file type and clear all previous state to avoid showing stale preview
                file_reader_content.file_type = file_type;
                file_reader_content.hightlighted_content = None;
                file_reader_content.curr_selected_path = path.clone();
                app.preview_loading = true;
                app.preview_loading_path = Some(path.clone());
                app.preview_file_content = "Loading...".to_string();
                app.preview_files = Vec::new();
                image_renderer.clear();

                // Create channel for async preview result
                let (tx, rx) = mpsc::channel();
                app.preview_receiver = Some(rx);

                // Spawn thread to load preview
                let path_for_thread = path.clone();
                std::thread::spawn(move || {
                    file_reader_content::load_preview_async(path_for_thread, tx);
                });
            }
        }

        // Check for async preview results
        if let Some(ref rx) = app.preview_receiver {
            match rx.try_recv() {
                Ok(msg) => {
                    use file_finder::app::PreviewMessage;
                    match msg {
                        PreviewMessage::Loaded {
                            path,
                            content,
                            file_type,
                            ..
                        } => {
                            // Only update if this is still the file we're waiting for
                            if app.preview_loading_path.as_ref() == Some(&path) {
                                app.preview_loading = false;
                                file_reader_content.file_type = file_type.clone();

                                // For text-based files, apply syntax highlighting or markdown rendering
                                match file_type {
                                    FileType::SourceCode
                                    | FileType::ConfigFile
                                    | FileType::JSON => {
                                        // Set current path BEFORE calling get_highlighted_content (used for cache lookup)
                                        file_reader_content.curr_selected_path = path.clone();
                                        let ext_type = file_reader_content
                                            .get_file_extension_type(path.clone());
                                        let highlighted = file_reader_content
                                            .get_highlighted_content(content.clone(), ext_type);
                                        app.preview_file_content = highlighted;
                                    }
                                    FileType::Markdown => {
                                        // Render markdown with formatting
                                        file_reader_content
                                            .render_markdown_content(&content, &path);
                                        app.preview_file_content.clear();
                                    }
                                    FileType::Image => {
                                        // Spawn async image loading to avoid blocking UI
                                        file_reader_content.hightlighted_content = None;
                                        file_reader_content.curr_asset_path = path.clone();
                                        app.preview_file_content = "Loading image...".to_string();

                                        // Create channel for async image result
                                        let (img_tx, img_rx) = mpsc::channel();
                                        app.image_receiver = Some(img_rx);

                                        // Spawn thread to decode image
                                        let path_for_thread = path;
                                        std::thread::spawn(move || {
                                            file_reader_content::load_image_async(
                                                path_for_thread,
                                                img_tx,
                                            );
                                        });
                                    }
                                    FileType::Binary => {
                                        file_reader_content.hightlighted_content = None;
                                        app.preview_file_content =
                                            file_reader_content.read_binary_hex_view(&path);
                                    }
                                    FileType::ZIP => {
                                        file_reader_content.hightlighted_content = None;
                                        file_reader_content.read_zip_content(path);
                                    }
                                    FileType::Archive => {
                                        file_reader_content.hightlighted_content = None;
                                        // Determine archive type and read
                                        let ext = std::path::Path::new(&path)
                                            .extension()
                                            .and_then(|e| e.to_str())
                                            .unwrap_or("")
                                            .to_lowercase();
                                        let entries = match ext.as_str() {
                                            "tar" => file_reader_content.read_tar_content(&path),
                                            "tgz" => file_reader_content.read_tar_gz_content(&path),
                                            "gz" if path.to_lowercase().ends_with(".tar.gz") => {
                                                file_reader_content.read_tar_gz_content(&path)
                                            }
                                            "bz2" | "tbz2"
                                                if path.to_lowercase().ends_with(".tar.bz2") =>
                                            {
                                                file_reader_content.read_tar_bz2_content(&path)
                                            }
                                            "xz" | "txz"
                                                if path.to_lowercase().ends_with(".tar.xz") =>
                                            {
                                                file_reader_content.read_tar_xz_content(&path)
                                            }
                                            _ => vec!["Archive preview not supported".to_string()],
                                        };
                                        app.preview_file_content = entries.join("\n");
                                    }
                                    FileType::PDF => {
                                        // PDF content is now loaded asynchronously
                                        file_reader_content.hightlighted_content = None;
                                        app.preview_file_content = content;
                                    }
                                    FileType::TextFile => {
                                        // Plain text - clear any previous highlighted content
                                        file_reader_content.hightlighted_content = None;
                                        app.preview_file_content = content;
                                    }
                                    _ => {
                                        // Other file types - clear highlighted content
                                        file_reader_content.hightlighted_content = None;
                                        app.preview_file_content = content;
                                    }
                                }
                                app.preview_files = Vec::new();
                            }
                        }
                        PreviewMessage::DirectoryListing { path, entries } => {
                            if app.preview_loading_path.as_ref() == Some(&path) {
                                app.preview_loading = false;
                                app.preview_files = entries;
                                app.preview_file_content.clear();
                                file_reader_content.file_type = FileType::NotAvailable;
                                image_renderer.clear();
                            }
                        }
                        PreviewMessage::Error { path, message } => {
                            if app.preview_loading_path.as_ref() == Some(&path) {
                                app.preview_loading = false;
                                app.preview_file_content = format!("Error: {}", message);
                                file_reader_content.file_type = FileType::NotAvailable;
                            }
                        }
                    }
                    app.preview_receiver = None;
                }
                Err(mpsc::TryRecvError::Empty) => {
                    // Still loading, keep waiting
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    // Thread finished unexpectedly
                    app.preview_loading = false;
                    app.preview_receiver = None;
                }
            }
        }

        // Check for async image loading results
        if let Some(ref rx) = app.image_receiver {
            match rx.try_recv() {
                Ok(result) => {
                    // Only update if this is still the file we're waiting for
                    if file_reader_content.curr_asset_path == result.path {
                        match result.result {
                            Ok((decoded_image, format)) => {
                                image_renderer.load_from_decoded(decoded_image, format);
                                app.preview_file_content.clear();
                            }
                            Err(error_msg) => {
                                image_renderer.set_error(error_msg);
                                app.preview_file_content.clear();
                            }
                        }
                    }
                    app.image_receiver = None;
                }
                Err(mpsc::TryRecvError::Empty) => {
                    // Still loading, keep waiting
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    // Thread finished unexpectedly
                    app.image_receiver = None;
                }
            }
        }

        // Extract image state before drawing (needed for mutable access in render_stateful_widget)
        let has_image = image_renderer.has_image();
        let image_info = image_renderer.get_info().to_string();
        let image_error = image_renderer.get_error().map(|s| s.to_string());
        let mut image_state_for_render = image_renderer.image_state.take();

        // Extract right pane list state for rendering (needed for scroll position persistence)
        let mut right_pane_list_state = app.right_pane.list_state.clone();

        // Draw UI
        terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(1)
                .constraints(
                    [
                        Constraint::Length(2),
                        Constraint::Length(3),
                        Constraint::Min(1),
                        Constraint::Length(3),
                    ]
                    .as_ref(),
                )
                .split(f.size());

            let (msg, style) = match app.input_mode {
                InputMode::Normal => {
                    let search_indicator = if app.global_search_mode {
                        " [Global Search]".bold()
                    } else if !app.input.is_empty() {
                        " [Local Search]".bold()
                    } else {
                        find_hint.as_str().bold()
                    };
                    (
                        vec![
                            "Exit (q)".bold(),
                            search_indicator,
                            app.input.clone().bold(),
                            " Enter to select file (enter)".bold(),
                        ],
                        OneDarkTheme::normal(),
                    )
                },
                InputMode::Editing => {
                    let mode_text = if app.global_search_mode {
                        "Global Search Mode (Esc to exit)".bold()
                    } else {
                        "Local Search Mode (Esc to exit)".bold()
                    };
                    (vec![mode_text], OneDarkTheme::search_active())
                },
                InputMode::WatchDelete => (vec!["Delete Mode".bold()], OneDarkTheme::error()),
                InputMode::WatchCreate => (vec!["Create Mode".bold()], OneDarkTheme::success()),
                InputMode::WatchRename => (vec!["Rename Mode".bold()], OneDarkTheme::warning()),
                InputMode::WatchSort => (vec!["Sort Mode".bold()], OneDarkTheme::info()),
                InputMode::CacheLoading => {
                    let (directories_processed, current_directory) = app.get_cache_loading_info().unwrap_or((0, String::new()));
                    (
                        vec![
                            "⚡ Building directory cache...".bold(),
                            format!(" {} directories processed", directories_processed).bold(),
                            if !current_directory.is_empty() {
                                format!(" Processing: {}", current_directory).bold()
                            } else {
                                " Starting cache build...".bold()
                            },
                        ],
                        OneDarkTheme::loading(),
                    )
                },
                _ => (vec!["Default".bold()], OneDarkTheme::normal()),
            };

            // Dynamic layout based on view mode
            let inner_constraints = match app.view_mode {
                ViewMode::Normal => vec![
                    Constraint::Percentage(50),  // File list
                    Constraint::Percentage(50),  // Preview
                ],
                ViewMode::FullList => vec![
                    Constraint::Percentage(100), // Full width file list
                ],
                ViewMode::DualPane => vec![
                    Constraint::Percentage(50),  // Left pane
                    Constraint::Percentage(50),  // Right pane
                ],
            };
            let inner_layout = Layout::default()
                .direction(Direction::Horizontal)
                .constraints(inner_constraints)
                .split(chunks[2]);

            // Input field with OneDark theming
            let search_title = if app.global_search_mode {
                "🔍 Global Search"
            } else if app.input_mode == InputMode::Editing {
                "🔍 Local Search"
            } else {
                "🔍 Search"
            };
            let input_block = Paragraph::new(app.input.clone()).block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(search_title)
                        .style(match app.input_mode {
                            InputMode::Normal => OneDarkTheme::inactive_border(),
                            InputMode::Editing => {
                                if app.global_search_mode {
                                    OneDarkTheme::global_search()
                                } else {
                                    OneDarkTheme::local_search()
                                }
                            },
                            _ => OneDarkTheme::disabled(),
                        }),
                )
                .style(match app.input_mode {
                    InputMode::Editing => OneDarkTheme::normal(),
                    InputMode::Normal => OneDarkTheme::normal(),
                    _ => OneDarkTheme::disabled(),
                });

            let mut list_title = String::new();
            if app.loading {
                let title_with_loader = "Copying Files...".to_string();
                list_title.push_str(&title_with_loader);
            } else {
                list_title.push_str("List");
            }
                        // Render the status bar based on style setting
            match settings.status_bar_style {
                file_finder::config::settings::StatusBarStyle::Classic => {
                    status_bar.render(f, chunks[3], &app, &app.theme_colors);
                }
                file_finder::config::settings::StatusBarStyle::Minimal => {
                    status_bar.render_minimal(f, chunks[3], &app, &app.theme_colors);
                }
            }
            let text = Text::from(Line::from(msg)).patch_style(style);
            let help_message = Paragraph::new(text);

            let input_area = chunks[1];
            match app.input_mode {
                InputMode::Normal => {}
                InputMode::WatchDelete => {}
                InputMode::WatchCreate => {
                    let area = draw_input_popup(f.size());
                    let (x, y) = input_popup_cursor_position(area, app.char_index);
                    f.set_cursor(x, y);
                }
                InputMode::WatchRename => {
                    let area = draw_input_popup(f.size());
                    let (x, y) = input_popup_cursor_position(area, app.char_index);
                    f.set_cursor(x, y);
                }
                InputMode::WatchSort => {}
                InputMode::Editing => f.set_cursor(
                    input_area.x + app.character_index as u16 + 1,
                    input_area.y + 1,
                ),
                _ => {}
            }

            f.render_widget(help_message, chunks[0]);
            f.render_widget(input_block, chunks[1]);

            // Handle cache loading screen or normal list rendering
            match app.input_mode {
                InputMode::CacheLoading => {
                    // Render cache loading screen using helper from render module
                    let (directories_processed, current_directory) = app.get_cache_loading_info().unwrap_or((0, String::new()));
                    let loading_block = create_cache_loading_screen(directories_processed, &current_directory);
                    f.render_widget(loading_block, chunks[2]);
                }
                _ => {
                    // Normal file list rendering
                    widgets_ui.clone().render_list_preview(f, &chunks, &mut state, &app, &settings);
                }
            }

            // Only render preview in Normal view mode
            // In FullList mode, there's no preview pane
            // In DualPane mode, we render two file lists (handled in Phase 4)
            if app.view_mode.shows_preview() {
                let t = file_reader_content.file_type.clone();
                match t {
                    FileType::SourceCode | FileType::Markdown | FileType::TextFile |
                    FileType::ConfigFile | FileType::JSON => {
                        image_renderer.clear();
                        if let Some(highlighted_content) = file_reader_content.hightlighted_content.as_ref() {
                            let file_preview_text = highlighted_content.clone()
                                .block(create_preview_block("Preview", &settings))
                                .style(Style::default());
                            f.render_widget(file_preview_text, inner_layout[1]);
                        } else {
                            // Fallback for when highlighted content is not available
                            let preview_text = Paragraph::new(app.preview_file_content.clone())
                                .block(create_preview_block("Preview", &settings))
                                .style(Style::default());
                            f.render_widget(preview_text, inner_layout[1]);
                        }
                    }
                    FileType::Image => {
                        if has_image {
                            if let Some(ref mut img_state) = image_state_for_render {
                                // Create a block for the image with title showing dimensions
                                let title = format!("Image ({})", image_info);
                                let block = create_preview_block(&title, &settings);

                                // Calculate inner area for the image
                                let image_area = block.inner(inner_layout[1]);

                                // Render the block first
                                f.render_widget(block, inner_layout[1]);

                                // Render the actual image using StatefulImage
                                let image_widget = StatefulImage::new(None);
                                f.render_stateful_widget(image_widget, image_area, img_state);
                            }
                        } else {
                            // Show error message if image failed to load
                            let error_msg = image_error.unwrap_or_else(|| {
                                "Unable to load image preview\n\nPossible reasons:\n• Unsupported image format\n• Corrupted image file\n• Insufficient permissions\n\nSupported formats: PNG, JPEG, GIF, BMP, WebP".to_string()
                            });
                            let error_widget = Paragraph::new(error_msg)
                                .block(create_preview_block("Image - Error", &settings))
                                .style(Style::default().fg(Color::Yellow));
                            f.render_widget(error_widget, inner_layout[1]);
                        }
                    }
                    FileType::ZIP => {
                        let block = create_preview_block("ZIP Archive", &settings);
                        let zip_list_content = List::new(file_reader_content.curr_zip_content.clone())
                            .block(block)
                            .style(Style::default().fg(Color::Rgb(92, 99, 112)));
                        f.render_widget(zip_list_content, inner_layout[1]);
                    }
                    FileType::Archive => {
                        let archive_info = Paragraph::new(app.preview_file_content.clone())
                            .block(create_preview_block("Archive", &settings))
                            .style(Style::default());
                        f.render_widget(archive_info, inner_layout[1]);
                    }
                    FileType::CSV => {
                        let csv_content = Paragraph::new(app.preview_file_content.clone())
                            .block(create_preview_block("CSV Data", &settings))
                            .style(Style::default().fg(Color::Rgb(92, 99, 112)));
                        f.render_widget(csv_content, inner_layout[1]);
                    }
                    FileType::PDF => {
                        let pdf_content = Paragraph::new(app.preview_file_content.clone())
                            .block(create_preview_block("PDF", &settings))
                            .style(Style::default().fg(Color::White))
                            .wrap(ratatui::widgets::Wrap { trim: false });
                        f.render_widget(pdf_content, inner_layout[1]);
                    }
                    FileType::Binary => {
                        let binary_info = Paragraph::new(app.preview_file_content.clone())
                            .block(create_preview_block("Hex View", &settings))
                            .style(Style::default().fg(Color::White));
                        f.render_widget(binary_info, inner_layout[1]);
                    }
                    _ => {
                        image_renderer.clear();
                        widgets_ui.clone().render_preview_window(f, &chunks, &mut state, &app, &settings);
                    }
                }
            } else if app.view_mode.is_dual_pane() {
                // DualPane mode: render right pane file list
                let right_pane = &app.right_pane;
                let is_right_active = app.is_right_pane_active();

                // Generate list items for right pane using icon provider (with dotfile dimming)
                let right_items: Vec<ListItem> = right_pane.file_labels.iter().enumerate().map(|(idx, label)| {
                    let is_dir = right_pane.files.get(idx)
                        .map(|p| std::path::Path::new(p).is_dir())
                        .unwrap_or(false);
                    let path = right_pane.files.get(idx)
                        .map(std::path::Path::new)
                        .unwrap_or(std::path::Path::new(""));
                    let icon = widgets_ui.icon_provider.get_for_path(path, is_dir);

                    // Check if this is a dotfile (hidden file)
                    let is_dotfile = label.starts_with('.');

                    if is_dotfile {
                        // Dim dotfiles
                        ListItem::new(ratatui::text::Line::from(vec![
                            ratatui::text::Span::styled(
                                format!("{} {}", icon, label),
                                OneDarkTheme::dotfile(),
                            ),
                        ]))
                    } else {
                        ListItem::new(format!("{} {}", icon, label))
                    }
                }).collect();

                // Create title with directory info and active indicator
                let dir_name = std::path::Path::new(&right_pane.current_directory)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("...");

                let right_title = if settings.show_borders {
                    if is_right_active {
                        format!("▸ Right: {}", dir_name)
                    } else {
                        format!("  Right: {}", dir_name)
                    }
                } else {
                    if is_right_active {
                        format!("▸ {}", dir_name)
                    } else {
                        dir_name.to_string()
                    }
                };

                // Create block based on settings
                let block = if settings.show_borders {
                    let right_border_style = if is_right_active {
                        OneDarkTheme::active_border()
                    } else {
                        OneDarkTheme::inactive_border()
                    };
                    Block::default()
                        .borders(Borders::ALL)
                        .border_set(border::ROUNDED)
                        .title(right_title)
                        .style(right_border_style)
                } else {
                    // Borderless mode - use title style for active indicator
                    let title_style = if is_right_active {
                        OneDarkTheme::active_title()
                    } else {
                        OneDarkTheme::inactive_title()
                    };
                    Block::default()
                        .borders(Borders::LEFT)
                        .border_style(Style::default().fg(Color::Rgb(62, 68, 81)))
                        .title(right_title)
                        .title_style(title_style)
                };

                let right_list = List::new(right_items)
                    .block(block)
                    .highlight_style(OneDarkTheme::selected())
                    .highlight_symbol(if settings.show_borders { "❯" } else { " " });

                f.render_stateful_widget(right_list, inner_layout[1], &mut right_pane_list_state);
            }
            // FullList mode: no second pane to render (100% width file list)

            if app.render_popup {
                let popup = create_delete_confirmation_popup(
                    &app.delete_target_name,
                    app.delete_target_is_dir,
                    app.delete_target_file_count,
                    app.delete_target_total_size,
                );
                let area = draw_popup(f.size(), 50, 12);
                f.render_widget(Clear, area);
                f.render_widget(popup, area);
            }

            // Popup areas using render module helpers
            let input_popup_area = draw_input_popup(f.size());
            let sort_option_area = draw_popup(f.size(), 90, 20);
            let sort_options_chunks = split_popup_area_vertical(sort_option_area);
            let keybinding_area = draw_popup(f.size(), 55, 75);
            let keybinding_chunks = split_popup_area_vertical(keybinding_area);

            match app.input_mode {
                InputMode::WatchCreate => {
                    let create_input_block = create_create_input_popup(
                        &app.create_edit_file_name,
                        app.is_create_edit_error,
                        &app.error_message,
                    );
                    f.render_widget(Clear, input_popup_area);
                    f.render_widget(create_input_block, input_popup_area);
                }
                InputMode::WatchRename => {
                    let rename_input_block = create_rename_input_popup(&app.create_edit_file_name);
                    f.render_widget(Clear, input_popup_area);
                    f.render_widget(rename_input_block, input_popup_area);
                }
                InputMode::WatchSort => {
                    let sort_popup = create_sort_options_popup(&app.sort_by, &app.sort_type);
                    f.render_widget(Clear, sort_options_chunks[0]);
                    f.render_widget(sort_popup, sort_options_chunks[0]);
                }
                InputMode::WatchKeyBinding => {
                    let keybindings_popup = create_keybindings_popup(&settings.keymap);
                    f.render_widget(Clear, keybinding_chunks[0]);
                    f.render_widget(keybindings_popup, keybinding_chunks[0]);
                }
                _ => {}
            }
            // Render error notification overlay if there's an active error
            if status_bar.has_error() {
                status_bar.render_error_notification(f, f.size());
            }
        })?;

        // Restore image state after drawing.
        // The state was taken before the draw closure because render_stateful_widget
        // requires &mut access, but the closure captures image_renderer immutably.
        // Taking ownership before and restoring after avoids borrow checker conflicts.
        image_renderer.image_state = image_state_for_render;

        // Restore right pane list state after drawing (preserves scroll position)
        app.right_pane.list_state = right_pane_list_state;

        refresh_after_copy(
            &mut app,
            &mut state,
            &mut status_bar,
            &mut copy_receiver,
            &startup_paths.refresh_fallback_path,
        );

        // A finished move or extraction may have changed both panes, even if it failed.
        if let Some(result) = app.poll_job() {
            let refresh_dir = current_refresh_directory(&app, &startup_paths.refresh_fallback_path);
            if let Ok(files) = get_file_path_data(
                refresh_dir,
                app.show_hidden_files,
                app.sort_by.clone(),
                &app.sort_type,
            ) {
                if result.is_ok() {
                    app.refresh_files_after_operation(files, &mut state, None);
                } else {
                    app.refresh_files_from_watcher(files, &mut state);
                }
                status_bar.invalidate_cache();
            }
            if app.view_mode.is_dual_pane() {
                app.right_pane.refresh_files();
            }
            match result {
                Ok(message) => status_bar.show_info(message, None),
                Err(message) => {
                    warn!("Background operation failed: {}", message);
                    status_bar.show_error(message, None);
                }
            }
        }

        // Handle delete progress messages
        if let Some(ref rx) = delete_receiver {
            let mut last_msg: Option<DeleteMessage> = None;
            loop {
                match rx.try_recv() {
                    Ok(msg) => last_msg = Some(msg),
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        // A worker that dies without a result must not block later operations.
                        if !matches!(
                            last_msg,
                            Some(DeleteMessage::Completed { .. } | DeleteMessage::Error(_))
                        ) {
                            last_msg = Some(DeleteMessage::Error(
                                "Delete stopped unexpectedly before reporting a result".into(),
                            ));
                        }
                        delete_receiver = None;
                        break;
                    }
                }
            }

            if let Some(msg) = last_msg {
                match msg {
                    DeleteMessage::Counting { files_counted } => {
                        app.delete_progress_message =
                            format!("Counting files... {}", files_counted);
                    }
                    DeleteMessage::Progress {
                        files_deleted,
                        total_files,
                        current_file,
                    } => {
                        let percent = if total_files > 0 {
                            (files_deleted as f64 / total_files as f64 * 100.0) as u8
                        } else {
                            0
                        };
                        app.delete_progress_message = format!(
                            "Deleting: {} ({}% - {}/{})",
                            current_file, percent, files_deleted, total_files
                        );
                    }
                    DeleteMessage::Completed { success, message } => {
                        if success {
                            debug!("Delete completed: {}", message);
                        } else {
                            warn!("Delete failed: {}", message);
                            status_bar.show_error(message.clone(), None);
                        }

                        // Refresh the directory being browsed, even for a global search target.
                        let refresh_dir =
                            current_refresh_directory(&app, &startup_paths.refresh_fallback_path);
                        if let Ok(files) = get_file_path_data(
                            refresh_dir,
                            app.show_hidden_files,
                            app.sort_by.clone(),
                            &app.sort_type,
                        ) {
                            if success {
                                app.refresh_files_after_operation(files, &mut state, None);
                            } else {
                                app.refresh_files_from_watcher(files, &mut state);
                            }
                            status_bar.invalidate_cache();
                        }

                        // Reset delete state
                        app.delete_in_progress = false;
                        app.delete_progress_message.clear();
                        app.preserved_selection_index = None;
                        delete_receiver = None;
                    }
                    DeleteMessage::Error(error_msg) => {
                        warn!("Delete operation failed: {}", error_msg);
                        status_bar.show_error(error_msg.clone(), None);

                        // Reset delete state
                        app.delete_in_progress = false;
                        app.delete_progress_message = format!("Delete failed: {}", error_msg);
                        app.preserved_selection_index = None;
                        delete_receiver = None;
                    }
                }
            }
        }

        // Process file system events
        let fs_events = app.process_file_system_events();
        if !fs_events.is_empty() {
            // File system changes detected, refresh the CURRENT directory (not the start path)
            let refresh_dir = current_refresh_directory(&app, &startup_paths.refresh_fallback_path);

            if let Ok(file_path_list) = get_file_path_data(
                refresh_dir,
                app.show_hidden_files,
                app.sort_by.clone(),
                &app.sort_type,
            ) {
                app.refresh_files_from_watcher(file_path_list, &mut state);

                // Log the file system events for debugging
                for event_msg in fs_events {
                    debug!("File system event: {}", event_msg);
                }
            }
        }

        // Process cache loading progress
        if app.input_mode == InputMode::CacheLoading && app.process_cache_loading_progress() {
            // Cache loading is complete, finish setup
            debug!("Cache loading completed, finalizing...");

            // Use the completed cache if available, otherwise fallback to current directory
            // Capture the completed cache and persist it; also update the in-memory store used for global search
            let completed_directories = if let Some(ref cache_store) = app.completed_cache_store {
                debug!(
                    "Using completed cache with {} directories",
                    cache_store.directories.len()
                );

                // Update the in-memory store so global search starts working immediately
                store = cache_store.clone();

                // Save the cache to file for future use
                match file_finder::directory_store::save_directory_to_file(
                    cache_store,
                    &settings.cache_directory,
                ) {
                    Ok(()) => {
                        debug!(
                            "Successfully saved cache to file: {}",
                            settings.cache_directory
                        );
                        if let Some(ms) = app.cache_build_elapsed_ms {
                            debug!("Directory cache built and saved in {} ms", ms);
                        }
                    }
                    Err(e) => {
                        debug!("Failed to save cache to file: {}", e);
                    }
                }

                // We no longer replace the current file list with the entire cache; keep current directory view.
                // Still return directories for compatibility (ignored by finish_cache_loading).
                cache_store.directories.clone()
            } else {
                debug!("No completed cache available, falling back to current directory");
                // Fallback to current directory files
                get_file_path_data(
                    startup_paths.refresh_fallback_path.clone(),
                    false,
                    app.sort_by.clone(),
                    &app.sort_type,
                )
                .unwrap_or_else(|_| Vec::new())
            };

            // Finalize cache loading without changing the current directory view
            app.finish_cache_loading(completed_directories);

            debug!("Cache loading finished, app is now in normal mode");
        }

        // Apply preserved selection and clamp after all refreshes, before handling a key.
        let selected_index = app.preserved_selection_index.take().or(state.selected());
        let valid_index = app
            .get_list_length()
            .checked_sub(1)
            .map(|last_index| selected_index.unwrap_or(0).min(last_index));
        state.select(valid_index);
        app.curr_index = valid_index;
        // Keep the preview on the selected item, however the selection changed.
        if app.sync_preview_with_selection(valid_index) {
            file_reader_content.file_type = FileType::NotAvailable;
            file_reader_content.hightlighted_content = None;
            file_reader_content.curr_selected_path.clear();
            image_renderer.clear();
        }

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
    use file_finder::status_bar::NotificationKind;
    use std::path::PathBuf;

    fn running_copy_app() -> App {
        let mut app = App::new(Vec::new());
        app.start_copy(PathBuf::from("copy_target"), None);
        app.copy_progress_message = "Copying...".into();
        app.copy_files_processed = 2;
        app.copy_total_files = 10;
        app
    }

    fn assert_copy_finished(app: &App, receiver: &Option<mpsc::Receiver<CopyMessage>>) {
        assert!(!app.copy_in_progress);
        assert!(app.copy_progress_message.is_empty());
        assert_eq!(app.copy_files_processed, 0);
        assert_eq!(app.copy_total_files, 0);
        assert!(app.copy_destination.is_none());
        assert!(receiver.is_none());
    }

    fn copy_progress() -> CopyMessage {
        CopyMessage::Progress {
            files_processed: 3,
            total_files: 10,
            bytes_copied: 30,
            total_bytes: 100,
            current_file: "data.txt".into(),
        }
    }

    #[test]
    fn copy_refresh_selects_successful_destination_and_updates_preview_and_metadata() {
        for fail in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let source = dir.path().join("zulu.txt");
            std::fs::write(&source, "copy contents").unwrap();
            std::fs::write(dir.path().join("alpha.txt"), "existing").unwrap();
            std::fs::write(dir.path().join("middle.txt"), "existing").unwrap();
            let destination = generate_copy_file_dir_name(&source, dir.path());
            if fail {
                std::fs::write(&destination, "existing destination").unwrap();
            }
            let directory = dir.path().to_string_lossy().into_owned();
            let files =
                get_file_path_data(directory.clone(), false, SortBy::Name, &SortType::ASC).unwrap();
            let mut app = App::new(files);
            app.current_directory = directory.clone();
            app.sort_by = SortBy::Name;
            app.sort_type = SortType::ASC;
            let mut state = ListState::default();
            state.select(Some(app.files.len() - 1));
            app.curr_index = state.selected();
            app.preserved_selection_index = state.selected();
            app.sync_preview_with_selection(state.selected());
            app.start_copy(destination.clone(), None);
            let mut receiver = Some(copy_dir_file_with_progress_cancellable(
                &source,
                &destination,
                app.copy_cancel.clone(),
            ));
            let mut status = StatusBar::new();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while receiver.is_some() {
                refresh_after_copy(&mut app, &mut state, &mut status, &mut receiver, &directory);
                assert!(std::time::Instant::now() < deadline, "copy did not finish");
                std::thread::yield_now();
            }

            let expected = if fail { &source } else { &destination }
                .to_string_lossy()
                .into_owned();
            assert_eq!(
                app.get_selected_path(state.selected()),
                Some(expected.clone())
            );
            assert_eq!(app.curr_index, state.selected());
            assert_copy_finished(&app, &receiver);
            assert_eq!(
                std::fs::read_to_string(&destination).unwrap(),
                if fail {
                    "existing destination"
                } else {
                    "copy contents"
                },
            );
            // The main loop synchronizes previews after refreshing all operation results.
            app.sync_preview_with_selection(state.selected());
            assert_eq!(app.take_ready_preview(0), Some(expected));
            status.update(&app);
            assert!(status.selected_item_info.starts_with(if fail {
                "zulu.txt |"
            } else {
                "copy_zulu.txt |"
            }));
        }
    }

    #[test]
    fn copy_refresh_preserves_selection_after_navigation_or_cross_pane_copy() {
        for cross_pane in [false, true] {
            let browsed = tempfile::tempdir().unwrap();
            let other = tempfile::tempdir().unwrap();
            for directory in [browsed.path(), other.path()] {
                for name in ["alpha.txt", "copy_target.txt", "zulu.txt"] {
                    std::fs::write(directory.join(name), "contents").unwrap();
                }
            }
            let directory = browsed.path().to_string_lossy().into_owned();
            let files =
                get_file_path_data(directory.clone(), false, SortBy::Name, &SortType::ASC).unwrap();
            let mut app = App::new(files);
            app.current_directory = directory.clone();
            app.sort_by = SortBy::Name;
            app.sort_type = SortType::ASC;
            app.view_mode = ViewMode::DualPane;
            app.right_pane = file_finder::pane::Pane::new(
                other.path().to_string_lossy().into_owned(),
                Vec::new(),
            );
            app.right_pane.list_state.select(Some(2));
            let mut state = ListState::default();
            state.select(Some(0));
            app.preserved_selection_index = state.selected();
            // A right-to-left Shift+C keeps selection even when its destination is visible.
            // A local copy finishing after navigation must not select an unrelated namesake.
            let destination_dir = if cross_pane {
                browsed.path()
            } else {
                other.path()
            };
            app.start_copy(
                destination_dir.join("copy_target.txt"),
                cross_pane.then(|| "copy_target.txt".into()),
            );
            let (tx, rx) = mpsc::channel();
            tx.send(CopyMessage::Completed {
                success: true,
                message: "Copied".into(),
                skipped_special_files: 0,
            })
            .unwrap();
            let mut receiver = Some(rx);

            refresh_after_copy(
                &mut app,
                &mut state,
                &mut StatusBar::new(),
                &mut receiver,
                &directory,
            );

            assert_eq!(state.selected(), Some(0));
            assert_eq!(
                app.get_selected_path(state.selected()),
                Some(
                    browsed
                        .path()
                        .join("alpha.txt")
                        .to_string_lossy()
                        .into_owned()
                )
            );
            assert_eq!(app.right_pane.list_state.selected(), Some(2));
            assert_copy_finished(&app, &receiver);
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
    fn confirmed_quit_cancels_copy_and_waits_for_the_final_message() {
        use std::sync::atomic::Ordering;
        use std::time::Duration;

        for key in ["q", "o", "Enter"] {
            let mut app = running_copy_app();
            let cancel = app.copy_cancel.clone();
            let mut status = StatusBar::new();
            let mut terminal = Terminal::new(ratatui::backend::TestBackend::new(80, 6)).unwrap();
            let mut quit_error = None;
            let (tx, rx) = mpsc::channel();
            let mut receiver = Some(rx);
            assert!(!may_exit(
                &mut app,
                &mut status,
                key,
                &mut receiver,
                &mut terminal,
                &mut quit_error
            ));
            assert!(!cancel.load(Ordering::Relaxed));

            let (returned_tx, returned_rx) = mpsc::channel();
            let waiter = std::thread::spawn(move || {
                let allowed = may_exit(
                    &mut app,
                    &mut status,
                    key,
                    &mut receiver,
                    &mut terminal,
                    &mut quit_error,
                );
                assert_copy_finished(&app, &receiver);
                returned_tx
                    .send((
                        allowed,
                        app.copy_in_progress,
                        receiver.is_none(),
                        quit_error,
                    ))
                    .unwrap();
            });
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            while !cancel.load(Ordering::Relaxed) && std::time::Instant::now() < deadline {
                std::thread::yield_now();
            }
            assert!(cancel.load(Ordering::Relaxed));
            tx.send(copy_progress()).unwrap();
            assert!(matches!(
                returned_rx.recv_timeout(Duration::from_millis(25)),
                Err(mpsc::RecvTimeoutError::Timeout)
            ));
            let error =
                "Copy cancelled (failed to remove partial copy 'leftover': permission denied)";
            tx.send(CopyMessage::Error(error.into())).unwrap();
            let result = returned_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            assert_eq!(result, (true, false, true, Some(error.into())));
            waiter.join().unwrap();
        }
    }

    #[test]
    fn copy_completion_notifies_with_severity_from_skipped_count() {
        for (skipped_special_files, kind) in
            [(0, NotificationKind::Info), (2, NotificationKind::Warning)]
        {
            let mut app = running_copy_app();
            let mut status = StatusBar::new();
            let (tx, rx) = mpsc::channel();
            tx.send(copy_progress()).unwrap();
            tx.send(CopyMessage::Completed {
                success: true,
                message: "Copied 'data' to 'copy_data'".into(),
                skipped_special_files,
            })
            .unwrap();
            // Even a late progress update must not hide the final result.
            tx.send(copy_progress()).unwrap();
            drop(tx);
            let mut receiver = Some(rx);

            assert!(poll_copy_messages(&mut app, &mut status, &mut receiver));
            assert!(status.has_error());
            assert_eq!(status.notification_kind, kind);
            assert!(status
                .error_message
                .as_deref()
                .unwrap()
                .contains("copy_data"));
            assert_copy_finished(&app, &receiver);
        }
    }

    #[test]
    fn copy_to_other_pane_names_the_pane_and_keeps_details() {
        for (skipped_special_files, details) in [
            (0, ""),
            (1, " (skipped 1 special file: sockets/pipes/devices)"),
        ] {
            let mut app = running_copy_app();
            app.copy_to_other_pane = Some("data".into());
            let mut status = StatusBar::new();
            let (tx, rx) = mpsc::channel();
            tx.send(CopyMessage::Completed {
                success: true,
                message: format!("Copied 'data' to 'data'{details}"),
                skipped_special_files,
            })
            .unwrap();
            let mut receiver = Some(rx);

            assert!(poll_copy_messages(&mut app, &mut status, &mut receiver));
            assert_eq!(
                status.error_message,
                Some(format!("Copied 'data' to other pane{details}"))
            );
            assert_eq!(app.copy_to_other_pane, None);
            assert_copy_finished(&app, &receiver);
        }
    }

    #[test]
    fn copy_failures_notify_with_reason_and_clear_progress() {
        for message in [
            CopyMessage::Error("permission denied".into()),
            CopyMessage::Completed {
                success: false,
                message: "permission denied".into(),
                skipped_special_files: 0,
            },
        ] {
            let mut app = running_copy_app();
            let mut status = StatusBar::new();
            let (tx, rx) = mpsc::channel();
            tx.send(message).unwrap();
            let mut receiver = Some(rx);

            assert!(!poll_copy_messages(&mut app, &mut status, &mut receiver));
            assert_eq!(status.notification_kind, NotificationKind::Error);
            assert_eq!(status.error_message.as_deref(), Some("permission denied"));
            assert_copy_finished(&app, &receiver);
        }
    }

    #[test]
    fn copy_disconnect_without_result_reports_error_even_after_progress() {
        for send_progress in [false, true] {
            let mut app = running_copy_app();
            let mut status = StatusBar::new();
            let (tx, rx) = mpsc::channel();
            if send_progress {
                tx.send(copy_progress()).unwrap();
            }
            drop(tx);
            let mut receiver = Some(rx);

            assert!(!poll_copy_messages(&mut app, &mut status, &mut receiver));
            assert_eq!(status.notification_kind, NotificationKind::Error);
            assert!(status
                .error_message
                .as_deref()
                .unwrap()
                .contains("Copy stopped unexpectedly"));
            assert_copy_finished(&app, &receiver);
        }
    }

    #[test]
    fn copy_progress_and_empty_channel_keep_copy_running() {
        let mut app = running_copy_app();
        let mut status = StatusBar::new();
        let (tx, rx) = mpsc::channel();
        let mut receiver = Some(rx);
        assert!(!poll_copy_messages(&mut app, &mut status, &mut receiver));
        assert!(app.copy_in_progress);

        tx.send(copy_progress()).unwrap();
        assert!(!poll_copy_messages(&mut app, &mut status, &mut receiver));
        assert_eq!(app.copy_files_processed, 3);
        assert_eq!(app.copy_total_files, 10);
        assert!(app.copy_progress_message.contains("30%"));
        assert!(app.copy_in_progress);
        assert!(receiver.is_some());
        assert!(!status.has_error());
    }

    #[test]
    fn copy_status_keeps_receiver_alive_and_yields_to_progress() {
        let mut app = running_copy_app();
        let mut status = StatusBar::new();
        let (tx, rx) = mpsc::channel();
        let mut receiver = Some(rx);
        tx.send(CopyMessage::Status("Cloning 'source'…".into()))
            .unwrap();
        assert!(!poll_copy_messages(&mut app, &mut status, &mut receiver));
        assert_eq!(app.copy_progress_message, "Cloning 'source'…");
        assert!(app.copy_in_progress);
        assert!(receiver.is_some());
        assert!(!status.has_error());

        tx.send(copy_progress()).unwrap();
        assert!(!poll_copy_messages(&mut app, &mut status, &mut receiver));
        assert!(app.copy_progress_message.contains("30%"));
        assert!(app.copy_in_progress);
    }

    #[test]
    fn copy_status_does_not_hide_completion_or_disconnect() {
        for completed in [false, true] {
            let mut app = running_copy_app();
            let mut status = StatusBar::new();
            let (tx, rx) = mpsc::channel();
            let mut receiver = Some(rx);
            tx.send(CopyMessage::Status("Cloning 'source'…".into()))
                .unwrap();
            if completed {
                tx.send(CopyMessage::Completed {
                    success: true,
                    message: "Copied 'source' to 'copy_source'".into(),
                    skipped_special_files: 0,
                })
                .unwrap();
            }
            drop(tx);
            assert_eq!(
                poll_copy_messages(&mut app, &mut status, &mut receiver),
                completed
            );
            assert_copy_finished(&app, &receiver);
            assert_eq!(
                status.notification_kind,
                if completed {
                    NotificationKind::Info
                } else {
                    NotificationKind::Error
                }
            );
        }
    }

    #[test]
    fn system_opener_matches_platform() {
        #[cfg(target_os = "macos")]
        assert_eq!(system_opener(), "open");
        #[cfg(not(target_os = "macos"))]
        assert_eq!(system_opener(), "xdg-open");
    }

    #[test]
    fn write_result_file_truncates_existing_file() {
        let result_file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(result_file.path(), "/a/much/longer/previous/path").unwrap();

        write_result_file(result_file.path(), "/tmp/selected").unwrap();

        assert_eq!(
            std::fs::read_to_string(result_file.path()).unwrap(),
            "/tmp/selected"
        );
    }

    #[test]
    fn write_result_file_does_not_create_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("ff_result");

        assert!(write_result_file(&missing, "/tmp/selected").is_err());
        assert!(!missing.exists());
    }

    #[test]
    fn launch_editor_reports_missing_editor() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let file = file.path().to_str().unwrap();

        let err = launch_editor("ff-definitely-not-installed-xyz", file).unwrap_err();

        assert!(err.starts_with("failed to launch 'ff-definitely-not-installed-xyz': "));
    }

    #[test]
    fn launch_editor_reports_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("gone.txt");
        let missing = missing.to_str().unwrap();

        let err = launch_editor("ff-definitely-not-installed-xyz", missing).unwrap_err();

        assert_eq!(err, format!("'{}' no longer exists", missing));
    }

    #[cfg(unix)]
    #[test]
    fn launch_editor_reports_non_zero_exit() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let file = file.path().to_str().unwrap();

        // `false` ignores its arguments and exits 1; `true` exits 0. Neither opens the file.
        let err = launch_editor("false", file).unwrap_err();

        assert_eq!(err, "'false' failed with exit status: 1");
        assert_eq!(launch_editor("true", file), Ok(()));
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
        app.right_pane = file_finder::pane::Pane::new("/tmp/ff-right".to_string(), Vec::new());

        assert_eq!(
            current_refresh_directory(&app, "/tmp/ff-settings-start"),
            "/tmp/ff-left"
        );
    }
}
