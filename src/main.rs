// ============================================================
// File Finder (ff) - Terminal File Browser
// ============================================================

// Standard library imports
use std::{
    io::{self, Stdout},
    path::Path,
    process::Command,
    sync::mpsc,
};

// External crate imports
use copypasta::{ClipboardContext, ClipboardProvider};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use image::ImageReader;
use log::{debug, warn};
use ratatui::{
    backend::CrosstermBackend,
    prelude::*,
    style::{Color, Style},
    symbols::border,
    widgets::{Block, Borders, Clear, List, ListState, Paragraph},
    Terminal,
};
use syntect::{highlighting::ThemeSet, parsing::SyntaxSet};

// Internal module declarations
mod app;
mod cli;
mod config;
mod configuration;
mod directory_store;
mod errors;
mod file_reader_content;
mod highlight;
mod operations;
mod render;
mod status_bar;
mod theme;
mod ui;
mod utils;
mod watcher;

// Internal imports
use crate::{
    app::{App, InputMode},
    cli::{compute_effective_config, CliArgs},
    directory_store::{build_directory_from_store_async, load_directory_from_file, DirectoryStore},
    file_reader_content::{FileContent, FileType},
    operations::{
        copy_dir_file_with_progress, create_item_based_on_type, handle_delete_based_on_type,
        handle_rename, CopyMessage,
    },
    render::{
        create_cache_loading_screen, create_create_input_popup, create_delete_confirmation_block,
        create_keybindings_popup, create_rename_input_popup, create_sort_options_popup,
        draw_popup, split_popup_area, split_popup_area_vertical,
    },
    status_bar::StatusBar,
    theme::OneDarkTheme,
    ui::Ui,
    utils::{
        check_if_exists, generate_copy_file_dir_name, generate_metadata_str_info,
        get_content_from_path, get_curr_path, get_file_path_data, get_inner_files_info,
        get_metadata_info, init, is_file, SortBy, SortType,
    },
};

#[derive(Clone)]
struct ImageGenerator {
    has_image: bool,
    image_info: String,
}

impl ImageGenerator {
    pub fn new() -> ImageGenerator {
        ImageGenerator {
            has_image: false,
            image_info: String::new(),
        }
    }

    pub fn load_img(&mut self, path: String) {
        match ImageReader::open(&path) {
            Ok(reader) => {
                // Save format before calling decode since decode consumes reader
                let format = reader
                    .format()
                    .map(|f| format!("{:?}", f))
                    .unwrap_or_else(|| "Unknown".to_string());
                match reader.decode() {
                    Ok(dyn_img) => {
                        let width = dyn_img.width();
                        let height = dyn_img.height();

                        self.image_info = format!(
                            "Image Preview\n\nDimensions: {}x{}\nFormat: {}\n\nNote: Image rendering in terminal is limited.\nUse external viewer for full image experience.",
                            width, height, format
                        );
                        self.has_image = true;
                    }
                    Err(e) => {
                        debug!("Failed to decode image {}: {}", path, e);
                        self.has_image = false;
                        self.image_info = format!("Failed to decode image: {}", e);
                    }
                }
            }
            Err(e) => {
                debug!("Failed to open image {}: {}", path, e);
                self.has_image = false;
                self.image_info = format!("Failed to open image: {}", e);
            }
        }
    }

    pub fn clear(&mut self) {
        self.has_image = false;
        self.image_info.clear();
    }
}

/// Update the preview panel for a given file/directory path.
/// This consolidates the duplicated preview update logic from navigation handlers.
fn update_preview_for_path(
    path: &str,
    app: &mut App,
    file_reader_content: &mut FileContent,
    image_generator: &mut ImageGenerator,
) {
    // Update metadata stats
    let metadata = get_metadata_info(path.to_owned());
    app.curr_stats = generate_metadata_str_info(metadata);

    // Update the current selected path in file reader
    file_reader_content.curr_selected_path = path.to_string();

    if !is_file(path.to_string()) {
        // Directory: show contents
        if let Some(file_names) = get_content_from_path(path.to_string()) {
            image_generator.clear();
            file_reader_content.file_type = FileType::NotAvailable;
            app.preview_files = file_names;
        }
    } else {
        // File: detect type and show appropriate preview
        let file_extension = file_reader_content.get_file_extension(path.to_string());

        match file_extension {
            FileType::SourceCode
            | FileType::Markdown
            | FileType::TextFile
            | FileType::ConfigFile
            | FileType::JSON => {
                image_generator.clear();
                file_reader_content.file_type = file_extension.clone();

                let file_content = file_reader_content.read_file_content(path.to_string());
                let curr_file_type = file_reader_content.get_file_extension_type(path.to_string());
                let highlighted_content =
                    file_reader_content.get_highlighted_content(file_content, curr_file_type);

                if !file_reader_content.is_error {
                    app.preview_file_content = highlighted_content;
                }
            }
            FileType::Image => {
                image_generator.clear();
                file_reader_content.curr_asset_path = path.to_string();
                image_generator.load_img(path.to_string());
                file_reader_content.file_type = FileType::Image;
                app.preview_file_content.clear();
                file_reader_content.hightlighted_content = None;
            }
            FileType::ZIP => {
                image_generator.clear();
                file_reader_content.read_zip_content(path.to_string());
                file_reader_content.file_type = FileType::ZIP;
            }
            FileType::Archive => {
                image_generator.clear();
                file_reader_content.file_type = FileType::Archive;
                app.preview_file_content = format!("Archive file: {}", path);
            }
            FileType::CSV => {
                image_generator.clear();
                file_reader_content.read_csv_content();
                file_reader_content.file_type = FileType::CSV;
            }
            FileType::PDF => {
                image_generator.clear();
                file_reader_content.file_type = FileType::PDF;
                app.preview_file_content = file_reader_content.read_pdf_content(path);
            }
            FileType::Binary => {
                image_generator.clear();
                file_reader_content.file_type = FileType::Binary;
                app.preview_file_content =
                    format!("Binary file: {} (use external viewer)", path);
            }
            _ => {
                image_generator.clear();
                file_reader_content.file_type = FileType::NotAvailable;
            }
        }

        app.preview_files = Vec::new();
    }
}

/// Apply a sort operation to the current file list.
/// This consolidates the duplicated sort logic from the WatchSort handlers.
fn apply_sort(
    app: &mut App,
    sort_by: SortBy,
    sort_type: &SortType,
) -> anyhow::Result<()> {
    if app.files.is_empty() {
        return Ok(());
    }

    // Get current directory path from first file in list
    let cur_path = get_curr_path(app.files[0].clone());

    // Get sorted file list
    let file_path_list = get_file_path_data(
        cur_path,
        app.show_hidden_files,
        sort_by,
        sort_type,
    )?;

    // Update app state
    app.files = file_path_list.clone();
    app.read_only_files = file_path_list;
    app.update_file_references();
    app.input_mode = InputMode::Normal;

    Ok(())
}

fn handle_file_selection(
    file: &str,
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &App,
) -> anyhow::Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    terminal.clear()?;

    let ide = app.get_selected_ide();
    if ide.is_some() {
        let selected_ide = ide.unwrap();

        if Path::new(file).exists() {
            let output = Command::new(selected_ide.to_owned())
                .arg(file.to_owned())
                .status()
                .expect("Failed to open file");

            if output.success() {
                println!("Successfully opened file with {}", selected_ide);
            } else {
                println!("Failed to open file with {}", selected_ide);
            }
        }
    } else {
        let mut ctx = ClipboardContext::new().unwrap();
        ctx.set_contents(file.to_owned()).unwrap();
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

    init();

    // Check for --reset-config flag (from CLI args)
    if cli_args.reset_config {
        println!("Resetting configuration to defaults...");
        if let Err(e) = crate::config::reset_configuration() {
            eprintln!("Failed to reset configuration: {}", e);
            return Err(e.into());
        }
        println!("Configuration reset successfully!");
        return Ok(());
    }

    // Load configuration using new TOML system
    let settings = match crate::config::Settings::load() {
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
            eprintln!("Failed to compute effective configuration: {}", e);
            return Err(e.into());
        }
    };

    debug!(
        "Using effective configuration: start_path={}, theme={:?}, editor={:?}",
        effective_config.start_path.display(),
        effective_config.theme,
        effective_config.editor
    );

    let mut sort_type = SortType::ASC;
    let mut file_reader_content = FileContent::new(ps, ts);
    // Apply syntax theme from settings
    file_reader_content.set_syntax_theme(&settings.syntax_theme);
    let mut image_generator = ImageGenerator::new();
    // Setup terminal

    let file_strings = get_file_path_data(
        effective_config.start_path.to_string_lossy().to_string(),
        false,
        SortBy::Default,
        &sort_type,
    )?;
    let mut app = App::new(file_strings.clone());

    // Initialize config and theme after app creation
    if let Err(e) = app.initialize_config_and_theme() {
        eprintln!("Configuration error: {}", e);
        return Err(e.into());
    }

    // Start file system watching for the initial directory
    if let Err(e) = app.start_watching_directory(&settings.start_path) {
        debug!(
            "Failed to start watching directory '{}': {}",
            settings.start_path, e
        );
    } else {
        debug!("Started watching directory: {}", settings.start_path);
    }

    // Set editor from effective configuration (CLI args have precedence)
    if let Some(editor) = effective_config.editor {
        let ide_selection = match editor {
            crate::cli::Editor::Nvim => app::IDE::NVIM,
            crate::cli::Editor::Vscode => app::IDE::VSCODE,
            crate::cli::Editor::Zed => app::IDE::ZED,
        };
        app.selected_id = Some(ide_selection);
        debug!("Set editor from CLI: {:?}", editor);
    }

    let mut store = if !cli_args.rebuild_cache && Path::new(&settings.cache_directory).exists() {
        match load_directory_from_file(&settings.cache_directory.to_owned()) {
            Ok(res) => {
                println!("Loading directory cache from file");
                res
            }
            Err(e) => {
                eprintln!("Warning: Could not load cache file ({}), rebuilding...", e);
                // Fall through to rebuild cache
                DirectoryStore::new()
            }
        }
    } else {
        println!("Starting asynchronous directory cache build...");

        // Start async cache building
        let start_time = std::time::Instant::now();
        let rx = build_directory_from_store_async(
            settings.start_path.clone(),
            settings.ignore_directories.clone(),
        );

        // Set up the app to display loading progress and record start time
        app.start_cache_loading(rx, start_time);

        // Return empty store for now - it will be populated when cache building completes
        crate::directory_store::DirectoryStore {
            directories: Vec::new(),
        }
    };

    let widgets_ui = Ui::new(app.files.clone());
    let mut status_bar = StatusBar::new();

    debug!("{:?}", widgets_ui.files_list);

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

    // Main loop
    loop {
        // Check if we need to restore a preserved selection
        if let Some(preserved_index) = app.preserved_selection_index.take() {
            // Ensure the index is valid for the current file list
            let valid_index = if app.files.is_empty() {
                0
            } else {
                preserved_index.min(app.files.len() - 1)
            };
            state.select(Some(valid_index));
            app.curr_index = Some(valid_index);
        }

        // Update status bar with current app state
        status_bar.update(&app);

        // Extract image info before drawing if needed
        let has_image = image_generator.has_image;

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
                        " find (i)".bold()
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

            let inner_layout = Layout::default()
                .direction(Direction::Horizontal)
                .constraints(vec![
                    Constraint::Percentage(50),
                    Constraint::Percentage(50),
                ])
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
                let title_with_loader = format!("Copying Files...");
                list_title.push_str(&title_with_loader);
            } else {
                list_title.push_str(&"List");
            }
                        // Render the enhanced status bar
            status_bar.render(f, chunks[3], &app);
            let text = Text::from(Line::from(msg)).patch_style(style);
            let help_message = Paragraph::new(text);

            let input_area = chunks[1];
            match app.input_mode {
                InputMode::Normal => {}
                InputMode::WatchDelete => {}
                InputMode::WatchCreate => {}
                InputMode::WatchRename => {
                    // Place cursor inside the rename popup input, aligned to app.char_index
                    let area = draw_popup(f.size(), 40, 7);
                    let popup_chunks = Layout::default()
                        .direction(Direction::Horizontal)
                        .margin(1)
                        .constraints([Constraint::Percentage(100)])
                        .split(area);
                    f.set_cursor(
                        popup_chunks[0].x + app.char_index as u16 + 1,
                        popup_chunks[0].y + 1,
                    );
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

            let t = file_reader_content.file_type.clone();
            match t {
                FileType::SourceCode | FileType::Markdown | FileType::TextFile |
                FileType::ConfigFile | FileType::JSON => {
                    image_generator.clear();
                    if let Some(highlighted_content) = file_reader_content.hightlighted_content.as_ref() {
                        let file_preview_text = highlighted_content.clone()
                            .block(Block::default().borders(Borders::ALL).border_set(border::ROUNDED).title("Preview"))
                            .style(Style::default());
                        f.render_widget(file_preview_text, inner_layout[1]);
                    } else {
                        // Fallback for when highlighted content is not available
                        let preview_text = Paragraph::new(app.preview_file_content.clone())
                            .block(Block::default().borders(Borders::ALL).border_set(border::ROUNDED).title("Preview"))
                            .style(Style::default());
                        f.render_widget(preview_text, inner_layout[1]);
                    }
                }
                FileType::Image => {
                    if has_image {
                        let image_info = Paragraph::new(image_generator.image_info.clone())
                            .block(Block::default().borders(Borders::ALL).border_set(border::ROUNDED).title("Image"))
                            .style(Style::default().fg(Color::Green));
                        f.render_widget(image_info, inner_layout[1]);
                    } else {
                        let image_info = Paragraph::new(if image_generator.image_info.is_empty() {
                            "Unable to load image preview\n\nPossible reasons:\n• Unsupported image format\n• Corrupted image file\n• Insufficient permissions\n\nSupported formats: PNG, JPEG, GIF, BMP".to_string()
                        } else {
                            image_generator.image_info.clone()
                        })
                            .block(Block::default().borders(Borders::ALL).border_set(border::ROUNDED).title("Image - Error"))
                            .style(Style::default().fg(Color::Yellow));
                        f.render_widget(image_info, inner_layout[1]);
                    }
                }
                FileType::ZIP => {
                    let zip_list_content = List::new(file_reader_content.curr_zip_content.clone()).block(
                        Block::default()
                            .borders(Borders::ALL)
                            .border_set(border::ROUNDED)
                            .title("ZIP Archive")
                            .style(match app.input_mode {
                                InputMode::Normal => Style::default().fg(Color::Green),
                                InputMode::Editing => Style::default().fg(Color::Gray),
                                _ => Style::default().fg(Color::Gray),
                            })
                    )
                    .style(Style::default().fg(Color::DarkGray));
                    f.render_widget(zip_list_content, inner_layout[1]);
                }
                FileType::Archive => {
                    let archive_info = Paragraph::new(app.preview_file_content.clone())
                        .block(Block::default().borders(Borders::ALL).border_set(border::ROUNDED).title("Archive"))
                        .style(Style::default());
                    f.render_widget(archive_info, inner_layout[1]);
                }
                FileType::CSV => {
                    let csv_list_content = List::new(file_reader_content.curr_csv_content.clone()).block(
                        Block::default()
                            .borders(Borders::ALL)
                            .border_set(border::ROUNDED)
                            .title("CSV Data")
                            .style(match app.input_mode {
                                InputMode::Normal => Style::default().fg(Color::Green),
                                InputMode::Editing => Style::default().fg(Color::Gray),
                                _ => Style::default().fg(Color::Gray),
                            })
                    )
                    .style(Style::default().fg(Color::DarkGray));
                    f.render_widget(csv_list_content, inner_layout[1]);
                }
                FileType::PDF => {
                    let pdf_content = Paragraph::new(app.preview_file_content.clone())
                        .block(Block::default().borders(Borders::ALL).border_set(border::ROUNDED).title("PDF"))
                        .style(Style::default().fg(Color::White))
                        .wrap(ratatui::widgets::Wrap { trim: false });
                    f.render_widget(pdf_content, inner_layout[1]);
                }
                FileType::Binary => {
                    let binary_info = Paragraph::new(app.preview_file_content.clone())
                        .block(Block::default().borders(Borders::ALL).border_set(border::ROUNDED).title("Binary"))
                        .style(Style::default().fg(Color::Yellow));
                    f.render_widget(binary_info, inner_layout[1]);
                }
                _ => {
                    image_generator.clear();
                    widgets_ui.clone().render_preview_window(f, &chunks, &mut state, &app);
                }
            }

            if app.render_popup {
                let block = create_delete_confirmation_block();
                let area = draw_popup(f.size(), 40, 7);
                let popup_chunks = split_popup_area(area);
                f.render_widget(Clear, area);
                f.render_widget(block, popup_chunks[0]);
            }

            // Popup areas using render module helpers
            let popup_area = draw_popup(f.size(), 40, 7);
            let popup_chunks = split_popup_area(popup_area);
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
                    f.render_widget(Clear, popup_chunks[0]);
                    f.render_widget(create_input_block, popup_chunks[0]);
                }
                InputMode::WatchRename => {
                    let rename_input_block = create_rename_input_popup(&app.create_edit_file_name);
                    f.render_widget(rename_input_block, popup_chunks[0]);
                }
                InputMode::WatchSort => {
                    let sort_popup = create_sort_options_popup(&sort_type);
                    f.render_widget(Clear, sort_options_chunks[0]);
                    f.render_widget(sort_popup, sort_options_chunks[0]);
                }
                InputMode::WatchKeyBinding => {
                    let keybindings_popup = create_keybindings_popup();
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

        // Handle copy progress messages
        if let Some(ref rx) = copy_receiver {
            // Drain all pending messages so we only render the most recent state.
            // This avoids a huge backlog of progress updates that would otherwise
            // make the UI appear to take minutes for large trees.
            let mut last_msg: Option<CopyMessage> = None;
            loop {
                match rx.try_recv() {
                    Ok(msg) => last_msg = Some(msg),
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        // Copy thread finished, reset state
                        app.copy_in_progress = false;
                        app.copy_progress_message.clear();
                        app.copy_files_processed = 0;
                        app.copy_total_files = 0;
                        copy_receiver = None;
                        last_msg = None;
                        break;
                    }
                }
            }

            if let Some(msg) = last_msg {
                match msg {
                    CopyMessage::Progress {
                        files_processed,
                        total_files,
                        current_file,
                    } => {
                        app.copy_files_processed = files_processed;
                        app.copy_total_files = total_files;
                        app.copy_progress_message = format!("Copying: {}", current_file);
                    }
                    CopyMessage::Completed { success, message } => {
                        if success {
                            debug!("Copy completed: {}", message);

                            // Refresh file lists to show the copied item in the CURRENT directory
                            let refresh_dir: String = if !app.files.is_empty() {
                                // Derive current directory from the first listed item
                                get_curr_path(app.files[0].clone())
                            } else {
                                // Fallback to configured start path
                                settings.start_path.to_owned()
                            };

                            if let Ok(file_path_list) = get_file_path_data(
                                refresh_dir,
                                app.show_hidden_files,
                                SortBy::Default,
                                &sort_type,
                            ) {
                                app.files = file_path_list.clone();
                                app.read_only_files = file_path_list;
                                app.update_file_references();
                                status_bar.invalidate_cache();
                            }
                        } else {
                            warn!("Copy failed: {}", message);
                        }

                        // Reset copy state
                        app.copy_in_progress = false;
                        app.copy_progress_message.clear();
                        app.copy_files_processed = 0;
                        app.copy_total_files = 0;
                        copy_receiver = None;
                    }
                    CopyMessage::Error(error_msg) => {
                        warn!("Copy operation failed: {}", error_msg);

                        // Reset copy state
                        app.copy_in_progress = false;
                        app.copy_progress_message = format!("Copy failed: {}", error_msg);
                        app.copy_files_processed = 0;
                        app.copy_total_files = 0;
                        copy_receiver = None;
                    }
                }
            }
        }

        // Process file system events
        let fs_events = app.process_file_system_events();
        if !fs_events.is_empty() {
            // File system changes detected, refresh the CURRENT directory (not the start path)
            let refresh_dir: String = if !app.files.is_empty() {
                get_curr_path(app.files[0].clone())
            } else {
                settings.start_path.to_owned()
            };

            if let Ok(file_path_list) = get_file_path_data(
                refresh_dir,
                app.show_hidden_files,
                SortBy::Default,
                &sort_type,
            ) {
                app.files = file_path_list.clone();
                app.read_only_files = file_path_list;
                app.update_file_references();

                // Log the file system events for debugging
                for event_msg in fs_events {
                    debug!("File system event: {}", event_msg);
                }
            }
        }

        // Process cache loading progress
        if app.input_mode == InputMode::CacheLoading {
            if app.process_cache_loading_progress() {
                // Cache loading is complete, finish setup
                debug!("Cache loading completed, finalizing...");

                // Use the completed cache if available, otherwise fallback to current directory
                // Capture the completed cache and persist it; also update the in-memory store used for global search
                let completed_directories = if let Some(ref cache_store) = app.completed_cache_store
                {
                    debug!(
                        "Using completed cache with {} directories",
                        cache_store.directories.len()
                    );

                    // Update the in-memory store so global search starts working immediately
                    store = cache_store.clone();

                    // Save the cache to file for future use
                    match crate::directory_store::save_directory_to_file(
                        cache_store,
                        &settings.cache_directory,
                    ) {
                        Ok(()) => {
                            debug!(
                                "Successfully saved cache to file: {}",
                                settings.cache_directory
                            );
                            if let Some(ms) = app.cache_build_elapsed_ms {
                                println!("Directory cache built and saved in {} ms", ms);
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
                        settings.start_path.clone(),
                        false,
                        SortBy::Default,
                        &sort_type,
                    )
                    .unwrap_or_else(|_| Vec::new())
                };

                // Finalize cache loading without changing the current directory view
                app.finish_cache_loading(completed_directories);

                debug!("Cache loading finished, app is now in normal mode");
            }
        }

        // Handle input with timeout for dynamic progress updates
        let timeout = std::time::Duration::from_millis(100); // 100ms timeout
        if let Ok(available) = event::poll(timeout) {
            if available {
                if let Event::Key(key) = event::read()? {
                    match app.input_mode {
                        InputMode::Normal => match key.code {
                            KeyCode::Char('i') => {
                                app.input_mode = InputMode::Editing;
                                file_reader_content.file_type = FileType::NotAvailable;
                                image_generator.clear();
                            }
                            KeyCode::Char('q') => {
                                break;
                            }
                            KeyCode::Down | KeyCode::Char('j') => {
                                let list_len = app.get_list_length();

                                if list_len > 0 {
                                    let i = match state.selected() {
                                        Some(i) => {
                                            if i >= list_len - 1 { 0 } else { i + 1 }
                                        }
                                        None => 0,
                                    };
                                    state.select(Some(i));
                                    app.curr_index = Some(i);

                                    // Update preview using helper function
                                    if let Some(path) = app.get_selected_path(Some(i)) {
                                        update_preview_for_path(
                                            &path,
                                            &mut app,
                                            &mut file_reader_content,
                                            &mut image_generator,
                                        );
                                    }
                                }
                            }
                            KeyCode::Up | KeyCode::Char('k') => {
                                let list_len = app.get_list_length();

                                if list_len > 0 {
                                    let i = match state.selected() {
                                        Some(i) => {
                                            if i == 0 { list_len - 1 } else { i - 1 }
                                        }
                                        None => 0,
                                    };
                                    state.select(Some(i));
                                    app.curr_index = Some(i);

                                    // Update preview using helper function
                                    if let Some(path) = app.get_selected_path(Some(i)) {
                                        update_preview_for_path(
                                            &path,
                                            &mut app,
                                            &mut file_reader_content,
                                            &mut image_generator,
                                        );
                                    }
                                }
                            }
                            KeyCode::Char('h') => {
                                if app.files.len() > 0 {
                                    let selected = &app.files[state.selected().unwrap()];
                                    let mut split_path = selected.split("/").collect::<Vec<&str>>();

                                    let sort_type_copy = sort_type.clone();
                                    // TODO: refactor this to be more idiomatic
                                    if split_path.len() > 4 {
                                        split_path.pop();
                                        split_path.pop();

                                        let new_path = split_path.join("/");
                                        let files_strings = get_inner_files_info(
                                            new_path.clone(),
                                            app.show_hidden_files,
                                            SortBy::Default,
                                            &sort_type_copy,
                                        )
                                        .unwrap();

                                        if let Some(f_s) = files_strings {
                                            app.read_only_files = f_s.clone();
                                            app.files = f_s;
                                            app.update_file_references();
                                            state.select(Some(0));

                                            // Start watching the new directory
                                            if let Err(e) = app.start_watching_directory(&new_path)
                                            {
                                                debug!(
                                                    "Failed to start watching directory '{}': {}",
                                                    new_path, e
                                                );
                                            } else {
                                                debug!("Started watching directory: {}", new_path);
                                            }
                                        }
                                    }
                                } else {
                                    let copy = sort_type.clone();
                                    let files_strings = get_inner_files_info(
                                        app.prev_dir.clone(),
                                        app.show_hidden_files,
                                        SortBy::Default,
                                        &copy,
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
                                // Use helper method to get selected path based on current mode
                                let selected_path = app.get_selected_path(state.selected());

                                if let Some(selected) = selected_path {
                                    app.prev_dir = get_curr_path(selected.to_string());
                                    if !is_file(selected.to_string()) {
                                        match get_inner_files_info(
                                            selected.to_string(),
                                            app.show_hidden_files,
                                            SortBy::Default,
                                            &sort_type,
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
                                                    if let Err(e) =
                                                        app.start_watching_directory(&selected)
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

                            KeyCode::Char('o') => {
                                if let Some(index) = state.selected() {
                                    Command::new("open")
                                        .arg(&app.files[index])
                                        .spawn()
                                        .expect("failed to open file");

                                    break;
                                }
                            }
                            KeyCode::Char('d') => {
                                app.render_popup = true;
                                app.input_mode = InputMode::WatchDelete;
                            }
                            KeyCode::Char('a') => {
                                app.input_mode = InputMode::WatchCreate;
                            }
                            KeyCode::Char('y') => {
                                let curr_file_path = file_reader_content.curr_selected_path.clone();
                                let file_type =
                                    file_reader_content.get_file_extension(curr_file_path.clone());
                                match file_type {
                                    FileType::ZIP => {
                                        let t = file_reader_content.extract_zip_content();
                                        app.input = t;
                                    }
                                    _ => {}
                                }
                            }
                            KeyCode::Char('r') => {
                                let selected_index = state.selected();
                                if let Some(index) = selected_index {
                                    let selected = &app.files[index];
                                    let mut split_path = selected.split("/").collect::<Vec<&str>>();
                                    let placeholder_name = split_path.pop().unwrap();
                                    let new_path = split_path.join("/");
                                    let placeholder_name_copy = placeholder_name;
                                    app.current_path_to_edit = new_path;
                                    app.current_name_to_edit = placeholder_name_copy.to_string();
                                    app.create_edit_file_name = placeholder_name.to_string();
                                    app.char_index = placeholder_name.len();
                                }
                                app.input_mode = InputMode::WatchRename;
                            }
                            KeyCode::Char('.') => {
                                let is_hidden = !app.show_hidden_files;
                                app.show_hidden_files = is_hidden;
                                let selected_index = state.selected();
                                if let Some(indx) = selected_index {
                                    let selected = &app.files[indx];

                                    let mut split_path = selected.split("/").collect::<Vec<&str>>();
                                    split_path.pop();
                                    let new_path = split_path.join("/");
                                    match get_inner_files_info(
                                        new_path,
                                        is_hidden,
                                        SortBy::Default,
                                        &sort_type,
                                    ) {
                                        Ok(files) => {
                                            if let Some(file_strs) = files {
                                                app.read_only_files = file_strs.clone();
                                                app.files = file_strs;
                                                app.update_file_references();
                                            }
                                        }
                                        Err(e) => {
                                            println!("error  {}", e);
                                        }
                                    }
                                }
                            }
                            KeyCode::Char('c') => {
                                // Copy selected file/directory in current location using async operation
                                if let Some(index) = state.selected() {
                                    if index < app.files.len() {
                                        let selected_path = &app.files[index];
                                        let src_path = Path::new(selected_path);

                                        // Get current directory (parent of selected item)
                                        let current_dir = if src_path.is_file() {
                                            src_path.parent().unwrap_or(Path::new("."))
                                        } else {
                                            // For directories, get the parent directory
                                            src_path.parent().unwrap_or(Path::new("."))
                                        };

                                        // Generate copy name
                                        let new_path_with_new_name = generate_copy_file_dir_name(
                                            selected_path.clone(),
                                            current_dir.to_string_lossy().to_string(),
                                        );

                                        let new_src = Path::new(&new_path_with_new_name);

                                        // Start async copy operation
                                        let rx = copy_dir_file_with_progress(src_path, new_src);
                                        copy_receiver = Some(rx);

                                        // Initialize copy progress state
                                        app.copy_in_progress = true;
                                        app.copy_progress_message =
                                            String::from("Starting copy...");
                                        app.copy_files_processed = 0;
                                        app.copy_total_files = 0;
                                    }
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
                                    app.input = selected.clone();

                                    // Check if IDE is configured - if so, open file, otherwise copy to clipboard
                                    if app.get_selected_ide().is_some() {
                                        let _ =
                                            handle_file_selection(&selected, &mut terminal, &app);
                                        break;
                                    } else {
                                        // Copy path to clipboard instead of opening
                                        use copypasta::{ClipboardContext, ClipboardProvider};
                                        if let Ok(mut ctx) = ClipboardContext::new() {
                                            if let Ok(_) = ctx.set_contents(selected.clone()) {
                                                debug!("Copied path to clipboard: {}", selected);
                                                // Optionally show a brief message to user
                                                // For now just break to exit
                                                break;
                                            }
                                        }
                                        // Fallback to normal file selection if clipboard fails
                                        let _ =
                                            handle_file_selection(&selected, &mut terminal, &app);
                                        break;
                                    }
                                }
                            }
                            _ => {}
                        },

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
                                    if !check_if_exists(new_path) {
                                        match handle_rename(&app) {
                                            Ok(_) => {
                                                app.reset_create_edit_values();
                                                let file_path_list = get_file_path_data(
                                                    settings.start_path.to_owned(),
                                                    app.show_hidden_files,
                                                    SortBy::Default,
                                                    &sort_type,
                                                )?;
                                                app.files = file_path_list.clone();
                                                app.read_only_files = file_path_list.clone();
                                                app.update_file_references();
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
                            KeyCode::Enter => {
                                // create file/dir
                                if !app.create_edit_file_name.is_empty() {
                                    let selected_index = state.selected();
                                    let selected = &app.files[selected_index.unwrap()];
                                    let mut split_path = selected.split("/").collect::<Vec<&str>>();
                                    split_path.pop();
                                    let new_path = split_path.join("/");
                                    match create_item_based_on_type(
                                        new_path,
                                        app.create_edit_file_name.clone(),
                                    ) {
                                        Ok(_) => {
                                            app.input_mode = InputMode::Normal;

                                            app.reset_create_edit_values();
                                            let file_path_list = get_file_path_data(
                                                settings.start_path.to_owned(),
                                                app.show_hidden_files,
                                                SortBy::Default,
                                                &sort_type,
                                            )?;
                                            app.files = file_path_list.clone();
                                            app.read_only_files = file_path_list.clone();
                                            app.update_file_references();
                                            status_bar.invalidate_cache();
                                        }
                                        Err(e) => {
                                            app.is_create_edit_error = true;
                                            app.error_message = e.user_message();
                                        }
                                    }
                                }
                            }
                            _ => {}
                        },

                        InputMode::Editing if key.kind == KeyEventKind::Press => match key.code {
                            KeyCode::Enter => app.submit_message(),
                            KeyCode::Char(to_insert) => {
                                app.enter_char(to_insert, store.clone());
                            }
                            KeyCode::Backspace => {
                                app.delete_char(store.clone());
                            }
                            KeyCode::Left => {
                                app.move_cursor_left();
                            }

                            KeyCode::Right => {
                                app.move_cursor_right();
                            }
                            KeyCode::Esc => {
                                app.input_mode = InputMode::Normal;
                            }

                            _ => {}
                        },
                        InputMode::WatchDelete => match key.code {
                            KeyCode::Char('q') => {
                                app.render_popup = false;
                                app.input_mode = InputMode::Normal;
                                break;
                            }
                            KeyCode::Char('n') => {
                                app.render_popup = false;
                                app.input_mode = InputMode::Normal;
                            }

                            KeyCode::Char('y') => {
                                let selected_index = state.selected();

                                if let Some(selected_indx) = selected_index {
                                    let selected = &app.files[selected_indx];

                                    match handle_delete_based_on_type(selected) {
                                        Ok(_) => {
                                            // Preserve selection by finding a nearby item after deletion
                                            let next_selection_index = if selected_indx > 0
                                                && selected_indx >= app.files.len() - 1
                                            {
                                                // If we deleted the last item, select the new last item
                                                Some(app.files.len().saturating_sub(2))
                                            } else if selected_indx < app.files.len() - 1 {
                                                // Select the item that will take the deleted item's position
                                                Some(selected_indx)
                                            } else {
                                                // Fallback to previous item or first item
                                                Some(
                                                    selected_indx
                                                        .saturating_sub(1)
                                                        .min(app.files.len().saturating_sub(2)),
                                                )
                                            };

                                            let file_path_list = get_file_path_data(
                                                settings.start_path.to_owned(),
                                                app.show_hidden_files,
                                                SortBy::Default,
                                                &sort_type,
                                            )?;
                                            app.render_popup = false;
                                            app.files = file_path_list.clone();
                                            app.read_only_files = file_path_list.clone();
                                            app.update_file_references();
                                            status_bar.invalidate_cache();

                                            // Restore selection to a nearby item
                                            app.preserved_selection_index = next_selection_index;
                                            app.input_mode = InputMode::Normal;
                                        }
                                        Err(e) => {
                                            // Show error in status bar
                                            status_bar.show_error(e.user_message(), None);
                                            warn!("Delete operation failed: {}", e.user_message());
                                            app.render_popup = false;
                                            app.input_mode = InputMode::Normal;
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
                                apply_sort(&mut app, SortBy::Name, &sort_type)?;
                            }
                            KeyCode::Char('s') => {
                                apply_sort(&mut app, SortBy::Size, &sort_type)?;
                            }
                            KeyCode::Char('t') => {
                                apply_sort(&mut app, SortBy::DateAdded, &sort_type)?;
                            }
                            KeyCode::Char('a') => {
                                sort_type = SortType::ASC;
                            }
                            KeyCode::Char('d') => {
                                sort_type = SortType::DESC;
                            }
                            _ => {}
                        },

                        InputMode::WatchKeyBinding => match key.code {
                            KeyCode::Char('q') => {
                                app.input_mode = InputMode::Normal;
                            }
                            _ => {}
                        },
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
    Ok(())
}
