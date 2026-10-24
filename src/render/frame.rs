//! Drawing one frame of the main screen.

use std::io;

use ratatui::{
    backend::CrosstermBackend,
    prelude::*,
    style::{Color, Style},
    symbols::border,
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
    Terminal,
};
use ratatui_image::StatefulImage;

use crate::{
    app::{InputMode, ViewMode},
    config::Settings,
    event::LoopState,
    file_reader_content::FileType,
    render::{
        create_cache_loading_screen, create_create_input_popup, create_delete_confirmation_popup,
        create_keybindings_popup, create_rename_input_popup, create_sort_options_popup,
        draw_input_popup, draw_popup, input_popup_cursor_position, popups::draw_delete_popup,
        render_folder_picker, render_info_popup, split_popup_area_vertical,
    },
    theme::OneDarkTheme,
    ui::{create_preview_block, Ui},
};

/// Draw the frame, lending the image and right-pane list states to the widgets.
pub fn draw(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    loop_state: &mut LoopState,
    settings: &Settings,
    widgets_ui: &Ui,
    find_hint: &str,
) -> io::Result<()> {
    let LoopState {
        app,
        state,
        file_reader_content,
        image_renderer,
        status_bar,
        ..
    } = loop_state;
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
                    find_hint.bold()
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
            InputMode::WatchDelete => (vec!["Permanent Delete".bold()], OneDarkTheme::error()),
            InputMode::WatchTrash => (vec!["Move to Trash".bold()], OneDarkTheme::error()),
            InputMode::WatchCreate => (vec!["Create Mode".bold()], OneDarkTheme::success()),
            InputMode::WatchRename => (vec!["Rename Mode".bold()], OneDarkTheme::warning()),
            InputMode::WatchSort => (vec!["Sort Mode".bold()], OneDarkTheme::info()),
            InputMode::WatchInfo => (vec!["Get Info".bold()], OneDarkTheme::info()),
            InputMode::PickFolder => (
                vec!["Pick a folder (Enter to confirm, Esc to cancel)".bold()],
                OneDarkTheme::info(),
            ),
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
            crate::config::settings::StatusBarStyle::Classic => {
                status_bar.render(f, chunks[3], app, &app.theme_colors);
            }
            crate::config::settings::StatusBarStyle::Minimal => {
                status_bar.render_minimal(f, chunks[3], app, &app.theme_colors);
            }
        }
        let text = Text::from(Line::from(msg)).patch_style(style);
        let help_message = Paragraph::new(text);

        let input_area = chunks[1];
        match app.input_mode {
            InputMode::Normal => {}
            InputMode::WatchDelete | InputMode::WatchTrash => {}
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
                widgets_ui.clone().render_list_preview(f, &chunks, state, app, settings);
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
                            .block(create_preview_block("Preview", settings))
                            .style(Style::default());
                        f.render_widget(file_preview_text, inner_layout[1]);
                    } else {
                        // Fallback for when highlighted content is not available
                        let preview_text = Paragraph::new(app.preview_file_content.clone())
                            .block(create_preview_block("Preview", settings))
                            .style(Style::default());
                        f.render_widget(preview_text, inner_layout[1]);
                    }
                }
                FileType::Image => {
                    if has_image {
                        if let Some(ref mut img_state) = image_state_for_render {
                            // Create a block for the image with title showing dimensions
                            let title = format!("Image ({})", image_info);
                            let block = create_preview_block(&title, settings);

                            // Calculate inner area for the image
                            let image_area = block.inner(inner_layout[1]);

                            // Render the block first
                            f.render_widget(block, inner_layout[1]);

                            // Render the actual image using StatefulImage
                            let image_widget = StatefulImage::new(None);
                            f.render_stateful_widget(image_widget, image_area, img_state);
                        }
                    } else if image_error.is_none()
                        && (app.preview_receiver.is_some() || app.image_receiver.is_some())
                    {
                        // Still reading or decoding, which can take seconds for large images
                        let loading = Paragraph::new("Loading image...")
                            .block(create_preview_block("Image", settings));
                        f.render_widget(loading, inner_layout[1]);
                    } else {
                        // Show error message if image failed to load
                        let error_msg = image_error.unwrap_or_else(|| {
                            "Unable to load image preview\n\nPossible reasons:\n• Unsupported image format\n• Corrupted image file\n• Insufficient permissions\n\nSupported formats: PNG, JPEG, GIF, BMP, WebP".to_string()
                        });
                        let error_widget = Paragraph::new(error_msg)
                            .block(create_preview_block("Image - Error", settings))
                            .style(Style::default().fg(Color::Yellow));
                        f.render_widget(error_widget, inner_layout[1]);
                    }
                }
                FileType::ZIP => {
                    let block = create_preview_block("ZIP Archive", settings);
                    let zip_list_content = List::new(file_reader_content.curr_zip_content.clone())
                        .block(block)
                        .style(Style::default().fg(Color::Rgb(92, 99, 112)));
                    f.render_widget(zip_list_content, inner_layout[1]);
                }
                FileType::Archive => {
                    let archive_info = Paragraph::new(app.preview_file_content.clone())
                        .block(create_preview_block("Archive", settings))
                        .style(Style::default());
                    f.render_widget(archive_info, inner_layout[1]);
                }
                FileType::CSV => {
                    let csv_content = Paragraph::new(app.preview_file_content.clone())
                        .block(create_preview_block("CSV Data", settings))
                        .style(Style::default().fg(Color::Rgb(92, 99, 112)));
                    f.render_widget(csv_content, inner_layout[1]);
                }
                FileType::PDF => {
                    let pdf_content = Paragraph::new(app.preview_file_content.clone())
                        .block(create_preview_block("PDF", settings))
                        .style(Style::default().fg(Color::White))
                        .wrap(ratatui::widgets::Wrap { trim: false });
                    f.render_widget(pdf_content, inner_layout[1]);
                }
                FileType::Binary => {
                    let binary_info = Paragraph::new(app.preview_file_content.clone())
                        .block(create_preview_block("Hex View", settings))
                        .style(Style::default().fg(Color::White));
                    f.render_widget(binary_info, inner_layout[1]);
                }
                _ => {
                    image_renderer.clear();
                    widgets_ui.clone().render_preview_window(f, &chunks, state, app, settings);
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
                app.input_mode == InputMode::WatchTrash,
            );
            let area = draw_delete_popup(f.size());
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
            InputMode::PickFolder => {
                if let Some(picker) = &app.folder_picker {
                    render_folder_picker(f, picker, &app.create_edit_file_name, app.char_index);
                }
            }
            InputMode::WatchInfo => {
                if let Some(info) = &app.info {
                    render_info_popup(f, &info.rows);
                }
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
    Ok(())
}
