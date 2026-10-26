//! Drawing one frame of the main screen.

use std::{
    io,
    time::{Instant, SystemTime},
};

use ratatui::{
    prelude::*,
    style::{Modifier, Style},
    symbols::border,
    widgets::{Block, Borders, Clear, List, Paragraph},
    Terminal,
};
use ratatui_image::StatefulImage;

use crate::{
    app::{InputMode, ViewMode},
    config::{Settings, StatusBarStyle},
    event::{keymap::KeymapRuntime, LoopState},
    file_reader_content::FileType,
    render::{
        create_cache_loading_screen, create_create_input_popup, create_delete_confirmation_popup,
        create_rename_input_popup, create_sort_options_popup, draw_input_popup, draw_popup,
        hints::hint_line,
        input_popup_cursor_position,
        popups::{
            draw_delete_popup, render_batch_confirmation_popup, render_palette, render_which_key,
        },
        render_folder_picker, render_info_popup, render_keybindings_popup,
        split_popup_area_vertical, top_bar,
    },
    ui::{create_preview_block, draw_scrollbar, Ui},
    utils::format::local_stamp,
};

/// Draw the frame, lending the image and right-pane list states to the widgets. The
/// hint row takes its keys from `keymap_runtime`, which also says when a sequence is
/// waiting for its next key.
pub fn draw(
    terminal: &mut Terminal<impl Backend>,
    loop_state: &mut LoopState,
    settings: &Settings,
    widgets_ui: &Ui,
    keymap_runtime: &KeymapRuntime,
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
    // What the lists' dates are judged against, worked out once a frame.
    let today = local_stamp(SystemTime::now()).unwrap_or_default();

    // Draw UI
    terminal.draw(|f| {
        let theme = &app.theme_colors;
        // The mode's accent, for a searched pane's border.
        let mode = Style::default().fg(app.mode_color());
        // The top bar, the lists, the status line (the classic style's boxes are three
        // rows) and the hint row, or a message there, which grows up over the lists.
        let status_height = match settings.status_bar_style {
            StatusBarStyle::Classic => 3,
            StatusBarStyle::Minimal => 1,
        };
        let message_rows = status_bar.message_rows(f.size().width);
        let bottom_height = if message_rows > 0 {
            message_rows
        } else {
            u16::from(settings.show_hints)
        };
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Min(1),
                Constraint::Length(status_height),
                Constraint::Length(bottom_height),
            ])
            .split(f.size());
        let lists = chunks[1];
        let selected = state.selected();

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
            .split(lists);

        top_bar::render(f, chunks[0], app, SystemTime::now());
        match settings.status_bar_style {
            StatusBarStyle::Classic => status_bar.render(f, chunks[2], app, theme, today),
            StatusBarStyle::Minimal => status_bar.render_minimal(f, chunks[2], app, theme, today),
        }
        match app.input_mode {
            InputMode::WatchCreate | InputMode::WatchRename => {
                let area = draw_input_popup(f.size());
                let (x, y) = input_popup_cursor_position(area, app.char_index);
                f.set_cursor(x, y);
            }
            _ => {}
        }

        // Handle cache loading screen or normal list rendering
        let left_rows = match app.input_mode {
            InputMode::CacheLoading => {
                // Render cache loading screen using helper from render module
                let (directories_processed, current_directory) = app.get_cache_loading_info().unwrap_or((0, String::new()));
                let loading_block = create_cache_loading_screen(directories_processed, &current_directory, theme);
                f.render_widget(loading_block, lists);
                None
            }
            _ => {
                // Normal file list rendering
                let (rows, total) =
                    widgets_ui.clone().render_list_preview(f, lists, state, app, settings, today);
                Some((rows, total, state.offset()))
            }
        };

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
                            .block(create_preview_block("Preview", settings, theme))
                            .style(Style::default());
                        f.render_widget(file_preview_text, inner_layout[1]);
                    } else {
                        // Fallback for when highlighted content is not available
                        let preview_text = Paragraph::new(app.preview_file_content.clone())
                            .block(create_preview_block("Preview", settings, theme))
                            .style(Style::default());
                        f.render_widget(preview_text, inner_layout[1]);
                    }
                }
                FileType::Image => {
                    if has_image {
                        if let Some(ref mut img_state) = image_state_for_render {
                            // Create a block for the image with title showing dimensions
                            let title = format!("Image ({})", image_info);
                            let block = create_preview_block(&title, settings, theme);

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
                            .block(create_preview_block("Image", settings, theme));
                        f.render_widget(loading, inner_layout[1]);
                    } else {
                        // Show error message if image failed to load
                        let error_msg = image_error.unwrap_or_else(|| {
                            "Unable to load image preview\n\nPossible reasons:\n• Unsupported image format\n• Corrupted image file\n• Insufficient permissions\n\nSupported formats: PNG, JPEG, GIF, BMP, WebP".to_string()
                        });
                        let error_widget = Paragraph::new(error_msg)
                            .block(create_preview_block("Image - Error", settings, theme))
                            .style(theme.warning);
                        f.render_widget(error_widget, inner_layout[1]);
                    }
                }
                FileType::ZIP => {
                    let block = create_preview_block("ZIP Archive", settings, theme);
                    let zip_list_content = List::new(file_reader_content.curr_zip_content.clone())
                        .block(block)
                        .style(Style::default().fg(theme.muted));
                    f.render_widget(zip_list_content, inner_layout[1]);
                }
                FileType::Archive => {
                    let archive_info = Paragraph::new(app.preview_file_content.clone())
                        .block(create_preview_block("Archive", settings, theme))
                        .style(Style::default());
                    f.render_widget(archive_info, inner_layout[1]);
                }
                FileType::CSV => {
                    let csv_content = Paragraph::new(app.preview_file_content.clone())
                        .block(create_preview_block("CSV Data", settings, theme))
                        .style(Style::default().fg(theme.muted));
                    f.render_widget(csv_content, inner_layout[1]);
                }
                FileType::PDF => {
                    let pdf_content = Paragraph::new(app.preview_file_content.clone())
                        .block(create_preview_block("PDF", settings, theme))
                        .style(Style::default().fg(theme.bright))
                        .wrap(ratatui::widgets::Wrap { trim: false });
                    f.render_widget(pdf_content, inner_layout[1]);
                }
                FileType::Binary => {
                    let binary_info = Paragraph::new(app.preview_file_content.clone())
                        .block(create_preview_block("Hex View", settings, theme))
                        .style(Style::default().fg(theme.bright));
                    f.render_widget(binary_info, inner_layout[1]);
                }
                _ => {
                    image_renderer.clear();
                    widgets_ui.clone().render_preview_window(f, lists, app, settings, today);
                }
            }
        } else if app.view_mode.is_dual_pane() {
            // DualPane mode: render right pane file list
            let right_pane = &app.right_pane;
            let is_right_active = app.is_right_pane_active();

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
                let right_border_style = if app.searching_right_pane() {
                    mode.add_modifier(Modifier::BOLD)
                } else if is_right_active {
                    theme.active_border
                } else {
                    theme.inactive_border
                };
                Block::default()
                    .borders(Borders::ALL)
                    .border_set(border::ROUNDED)
                    .title(right_title)
                    .style(right_border_style)
            } else {
                // Borderless mode - use title style for active indicator
                let title_style = if is_right_active {
                    theme.active_border
                } else {
                    theme.inactive_border
                };
                Block::default()
                    .borders(Borders::LEFT)
                    .border_style(Style::default().fg(theme.dim))
                    .title(right_title)
                    .title_style(title_style)
            };

            let width = block.inner(inner_layout[1]).width;
            let right_items = widgets_ui.right_pane_items(app, settings, width.into(), today);

            let right_list = List::new(right_items).block(block).style(theme.normal);

            f.render_stateful_widget(right_list, inner_layout[1], &mut right_pane_list_state);
        }
        // FullList mode: no second pane to render (100% width file list)

        // The divider beside the left list doubles as its scrollbar: its own right border,
        // or the left edge of the pane beside it.
        if let Some((rows, total, offset)) = left_rows {
            let divider = if settings.show_borders {
                Some(rows.right())
            } else {
                (app.view_mode != ViewMode::FullList).then(|| inner_layout[0].right())
            };
            if let Some(x) = divider {
                draw_scrollbar(f.buffer_mut(), x, rows, total, offset, theme.light_gray);
            }
        }

        if app.render_popup && !app.batch_targets.is_empty() {
            render_batch_confirmation_popup(
                f,
                &app.batch_targets,
                |path| app.listed_info(path),
                app.input_mode == InputMode::WatchTrash,
                theme,
            );
        } else if app.render_popup {
            let popup = create_delete_confirmation_popup(
                &app.delete_target_name,
                app.delete_target_is_dir,
                app.delete_target_file_count,
                app.delete_target_total_size,
                app.input_mode == InputMode::WatchTrash,
                theme,
            );
            let area = draw_delete_popup(f.size());
            f.render_widget(Clear, area);
            f.render_widget(popup, area);
        }

        // Popup areas using render module helpers
        let input_popup_area = draw_input_popup(f.size());
        let sort_option_area = draw_popup(f.size(), 90, 20);
        let sort_options_chunks = split_popup_area_vertical(sort_option_area);

        match app.input_mode {
            InputMode::WatchCreate => {
                let create_input_block = create_create_input_popup(
                    &app.create_edit_file_name,
                    app.is_create_edit_error,
                    &app.error_message,
                    theme,
                );
                f.render_widget(Clear, input_popup_area);
                f.render_widget(create_input_block, input_popup_area);
            }
            InputMode::WatchRename => {
                let rename_input_block = create_rename_input_popup(&app.create_edit_file_name, theme);
                f.render_widget(Clear, input_popup_area);
                f.render_widget(rename_input_block, input_popup_area);
            }
            InputMode::WatchSort => {
                let sort_popup = create_sort_options_popup(&app.sort_by, &app.sort_type, theme);
                f.render_widget(Clear, sort_options_chunks[0]);
                f.render_widget(sort_popup, sort_options_chunks[0]);
            }
            InputMode::WatchKeyBinding => {
                (app.help_scroll, app.help_page) =
                    render_keybindings_popup(f, &settings.keymap, app.help_scroll, theme);
            }
            InputMode::PickFolder => {
                if let Some(picker) = &app.folder_picker {
                    render_folder_picker(f, picker, &app.create_edit_file_name, app.char_index, theme);
                }
            }
            InputMode::WatchInfo => {
                if let Some(info) = &app.info {
                    render_info_popup(f, &info.rows, theme);
                }
            }
            InputMode::Palette => {
                app.palette.scroll =
                    render_palette(f, &app.palette, &settings.keymap, theme);
            }
            InputMode::Normal | InputMode::Visual => {
                if let Some((title, keys)) = keymap_runtime.which_key(Instant::now()) {
                    render_which_key(f, &title, &keys, theme);
                }
            }
            _ => {}
        }
        // Last, so no popup covers a message.
        if message_rows > 0 {
            status_bar.render_message(f, chunks[3], theme);
        } else if settings.show_hints {
            let hints = hint_line(app, keymap_runtime, selected, chunks[3].width);
            f.render_widget(Paragraph::new(hints), chunks[3]);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        app::App,
        config::settings::NerdFontSetting,
        event::background::tests::memory_index,
        file_reader_content::FileContent,
        render::image_renderer::ImageRenderer,
        status_bar::{NotificationKind, StatusBar},
        utils::files::{EntryInfo, EntryKind, Listing},
    };
    use ratatui::{backend::TestBackend, buffer::Buffer, widgets::ListState};
    use std::time::Duration;

    /// `/w` listing `count` folders, the first highlighted.
    fn loop_state(count: usize) -> LoopState {
        let listing = Listing {
            files: (0..count).map(|i| format!("/w/item-{i:02}")).collect(),
            info: vec![
                EntryInfo {
                    kind: Some(EntryKind::Folder),
                    ..EntryInfo::default()
                };
                count
            ],
        };
        let mut app = App::with_listing(listing);
        app.current_directory = "/w".into();
        LoopState {
            app,
            state: ListState::default().with_selected(Some(0)),
            file_reader_content: FileContent::new(
                syntect::parsing::SyntaxSet::new(),
                syntect::highlighting::ThemeSet::new(),
            ),
            image_renderer: ImageRenderer::new(),
            status_bar: StatusBar::new(),
            index: memory_index(std::path::Path::new("/w")),
            copy_receiver: None,
            delete_receiver: None,
            selected_file: None,
            quit_error: None,
            clipboard: Box::new(|_| Err("tests never use the real clipboard".into())),
        }
    }

    fn settings(classic: bool, show_borders: bool) -> Settings {
        Settings {
            use_nerd_fonts: NerdFontSetting::Always,
            status_bar_style: if classic {
                StatusBarStyle::Classic
            } else {
                StatusBarStyle::Minimal
            },
            show_borders,
            ..Settings::default()
        }
    }

    /// The whole screen, `width` by `height`, after one frame.
    fn frame(loop_state: &mut LoopState, settings: &Settings, width: u16, height: u16) -> Buffer {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        let keymap = KeymapRuntime::from_settings(&settings.keymap);
        let ui = Ui::new(Vec::new(), settings);
        draw(&mut terminal, loop_state, settings, &ui, &keymap).unwrap();
        terminal.backend().buffer().clone()
    }

    fn rows(buffer: &Buffer) -> Vec<String> {
        buffer
            .content()
            .chunks(usize::from(buffer.area.width))
            .map(|row| row.iter().map(|cell| cell.symbol()).collect())
            .collect()
    }

    /// The rows whose cell in column `x` is a scrollbar's thumb.
    fn thumb(buffer: &Buffer, x: u16) -> Vec<u16> {
        (0..buffer.area.height)
            .filter(|&y| buffer.get(x, y).symbol() == "┃")
            .collect()
    }

    #[test]
    fn a_24_row_screen_gives_the_lists_21_rows() {
        let mut state = loop_state(30);
        let screen = rows(&frame(&mut state, &settings(false, false), 100, 24));
        assert!(screen[0].starts_with(" /w "), "{}", screen[0]);
        assert!(screen[0].ends_with("30 items "), "{}", screen[0]);
        assert!(screen[1].contains("item-00"));
        assert!(screen[21].contains("item-20"), "{}", screen[21]);
        assert!(
            screen[22].starts_with(" NORMAL   item-00/  folder "),
            "{}",
            screen[22]
        );
        assert!(screen[22].trim_end().ends_with("1/30"), "{}", screen[22]);
        assert!(
            screen[23].starts_with(" l open   h up   / find   ␣/ all folders"),
            "{}",
            screen[23]
        );
        assert!(screen[23].ends_with("? all keys "), "{}", screen[23]);

        // A search is typed in the top bar; there's no search box.
        state.app.input_mode = InputMode::Editing;
        let screen = rows(&frame(&mut state, &settings(false, false), 100, 24));
        assert!(screen[0].starts_with(" /   "), "{}", screen[0]);
        assert!(screen[1].contains("item-00"));
        assert!(
            screen[23].starts_with(" Esc cancel   ←/→ move the cursor"),
            "{}",
            screen[23]
        );
        assert!(!screen.iter().any(|row| row.contains('🔍')));
    }

    #[test]
    fn the_classic_status_bar_gets_its_boxes_height() {
        let mut state = loop_state(30);
        let screen = rows(&frame(&mut state, &settings(true, false), 120, 24));
        assert!(screen[19].contains("item-18"), "{}", screen[19]);
        assert!(!screen.iter().any(|row| row.contains("item-19")));
        assert!(screen[20].starts_with('╭'), "{}", screen[20]);
        assert!(screen[21].contains(" NORMAL ") && screen[21].contains("1/30"));
        assert!(screen[21].contains("item-00/  folder"), "{}", screen[21]);
        assert!(screen[22].starts_with('╰'));
        assert!(screen[23].starts_with(" l open"), "{}", screen[23]);
    }

    #[test]
    fn the_divider_is_a_scrollbar_once_the_list_overflows() {
        let light_gray = loop_state(0).app.theme_colors.light_gray;
        // The divider beside the list's 21 rows: 30 rows give a 15-row thumb.
        let mut state = loop_state(30);
        let buffer = frame(&mut state, &settings(false, false), 100, 24);
        assert_eq!(thumb(&buffer, 50), (1..=15).collect::<Vec<_>>());
        assert_eq!(buffer.get(50, 1).fg, light_gray);
        assert_eq!(buffer.get(50, 16).symbol(), "│");

        // Scrolled to the end, it reaches the bottom.
        state.state.select(Some(29));
        state.app.curr_index = Some(29);
        let buffer = frame(&mut state, &settings(false, false), 100, 24);
        assert_eq!(thumb(&buffer, 50), (7..=21).collect::<Vec<_>>());

        // With borders, on the list's own right border, beside its 19 rows.
        let mut state = loop_state(30);
        let buffer = frame(&mut state, &settings(false, true), 100, 24);
        assert_eq!(thumb(&buffer, 49), (2..=13).collect::<Vec<_>>());

        // Everything fits: no thumb.
        for borders in [false, true] {
            let buffer = frame(&mut loop_state(10), &settings(false, borders), 100, 24);
            assert!(thumb(&buffer, 49).is_empty() && thumb(&buffer, 50).is_empty());
        }
    }

    #[test]
    fn without_hints_the_lists_get_the_row() {
        let mut state = loop_state(30);
        let settings = Settings {
            show_hints: false,
            ..settings(false, false)
        };
        let screen = rows(&frame(&mut state, &settings, 100, 24));
        assert!(screen[22].contains("item-21"), "{}", screen[22]);
        assert!(screen[23].starts_with(" NORMAL "), "{}", screen[23]);

        // A message still shows, on a row taken from the lists.
        state.status_bar.show_info("Copied".into(), None);
        let screen = rows(&frame(&mut state, &settings, 100, 24));
        assert!(screen[21].contains("item-20"), "{}", screen[21]);
        assert!(screen[22].starts_with(" NORMAL "), "{}", screen[22]);
        assert!(screen[23].starts_with(" ✓ Copied"), "{}", screen[23]);
    }

    #[test]
    fn a_message_takes_the_hint_row_with_its_marker_and_tint() {
        let theme = loop_state(0).app.theme_colors;
        let kinds = [
            (NotificationKind::Error, "✗", theme.red, theme.error_bg),
            (
                NotificationKind::Warning,
                "!",
                theme.yellow,
                theme.warning_bg,
            ),
            (NotificationKind::Info, "✓", theme.green, theme.success_bg),
        ];
        for (kind, marker, color, tint) in kinds {
            let mut state = loop_state(3);
            state.status_bar.show_error("Couldn't rename".into(), None);
            state.status_bar.notification_kind = kind;
            let buffer = frame(&mut state, &settings(false, false), 100, 24);
            let screen = rows(&buffer);
            // The top bar and the list's top stay in view.
            assert!(screen[0].starts_with(" /w "), "{}", screen[0]);
            assert!(screen[1].contains("item-00"), "{}", screen[1]);
            assert_eq!(screen[23].trim_end(), format!(" {marker} Couldn't rename"));
            let marker_cell = buffer.get(1, 23);
            assert_eq!(marker_cell.fg, color, "{marker}");
            assert!(marker_cell.modifier.contains(Modifier::BOLD));
            assert_eq!(buffer.get(3, 23).fg, theme.bright);
            for x in [0, 50, 99] {
                assert_eq!(buffer.get(x, 23).bg, tint, "{marker} at {x}");
            }
        }
    }

    #[test]
    fn a_long_message_wraps_upward_whole_then_the_hints_come_back() {
        let mut state = loop_state(30);
        let message = format!(
            "Couldn't copy '/w/{}/report.pdf' to '/w/archive': Permission denied (os error 13)",
            "nested-folder/".repeat(6)
        );
        state.status_bar.show_error(message.clone(), None);
        let screen = rows(&frame(&mut state, &settings(false, false), 80, 24));
        // Three rows: the lists give up two, and the status line sits above.
        assert!(screen[19].contains("item-18"), "{}", screen[19]);
        assert!(screen[20].starts_with(" NORMAL "), "{}", screen[20]);
        assert!(screen[21].starts_with(" ✗ Couldn't copy"), "{}", screen[21]);
        assert!(screen[22].starts_with("   "), "{}", screen[22]);
        // Nothing is cut: past the marker, the rows hold every character.
        let shown: String = screen[21..].concat().chars().skip(3).collect();
        let without_spaces = |text: &str| text.replace(' ', "");
        assert_eq!(without_spaces(&shown), without_spaces(&message));

        // Once it expires the hint row is back.
        state.status_bar.error_display_time = Some(Instant::now() - Duration::from_secs(4));
        state.status_bar.update(&state.app);
        let screen = rows(&frame(&mut state, &settings(false, false), 80, 24));
        assert!(screen[21].contains("item-20"), "{}", screen[21]);
        assert!(screen[22].starts_with(" NORMAL "), "{}", screen[22]);
        assert!(screen[23].starts_with(" l open"), "{}", screen[23]);
    }

    #[test]
    fn moving_up_keeps_the_list_scrolled_where_it_was() {
        let mut state = loop_state(30);
        state.app.preview_files = Listing::from(vec!["/w/item-25/a".to_string()]);
        state.state.select(Some(25));
        state.app.curr_index = Some(25);
        frame(&mut state, &settings(false, false), 100, 24);
        let offset = state.state.offset();
        assert_eq!(offset, 5);

        // One row up stays on screen, so nothing scrolls; the folder preview doesn't
        // pull the list back to its cursor.
        state.state.select(Some(24));
        state.app.curr_index = Some(24);
        let screen = rows(&frame(&mut state, &settings(false, false), 100, 24));
        assert_eq!(state.state.offset(), offset);
        assert!(screen[1].contains("item-05"), "{}", screen[1]);
    }
}
