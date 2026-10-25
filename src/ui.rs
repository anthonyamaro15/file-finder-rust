use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    symbols::border,
    text::Span,
    widgets::{Block, Borders, List, ListItem, ListState},
    Frame,
};
use std::{path::Path, rc::Rc};

use crate::app::{App, InputMode, SearchResult, ViewMode};
use crate::config::Settings;
use crate::icons::IconProvider;
use crate::render::create_size_text;
pub mod theme;
use self::theme::OneDarkTheme;
use crate::highlight::highlight_search_term;

/// Create a preview block with consistent styling based on settings
pub fn create_preview_block<'a>(title: &'a str, settings: &Settings) -> Block<'a> {
    if settings.show_borders {
        Block::default()
            .borders(Borders::ALL)
            .border_set(border::ROUNDED)
            .title(title)
    } else {
        // Modern mode: subtle left border separator, muted title
        Block::default()
            .borders(Borders::LEFT)
            .border_style(Style::default().fg(Color::Rgb(62, 68, 81)))
            .title(title)
            .title_style(Style::default().fg(Color::Rgb(92, 99, 112)))
    }
}

#[derive(Debug, Clone)]
pub struct ListFileItem {
    pub label: String,
    pub path: String,
}
#[derive(Debug, Clone)]
pub struct FileListContent {
    pub items: Vec<ListFileItem>,
}

#[derive(Debug, Clone)]
pub struct Ui {
    pub files_list: FileListContent,
    pub icon_provider: IconProvider,
}

impl Ui {
    pub fn new(files: Vec<String>, settings: &Settings) -> Self {
        let mut current_item_list: Vec<ListFileItem> = Vec::new();
        for path in files.iter() {
            let new_path = Path::new(path);
            // Safely extract file name, skip if path is invalid
            let get_file_name = new_path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_else(|| path.rsplit('/').next().unwrap_or(path))
                .to_string();
            let create_item_list = ListFileItem {
                label: get_file_name,
                path: String::from(path),
            };

            current_item_list.push(create_item_list);
        }

        //const item = ListFileItem
        let file_list = FileListContent {
            items: current_item_list,
        };

        let icon_provider = IconProvider::new(&settings.use_nerd_fonts);

        Self {
            files_list: file_list,
            icon_provider,
        }
    }

    pub fn render_list_preview(
        &mut self,
        f: &mut Frame<'_>,
        chunks: &Rc<[Rect]>,
        state: &mut ListState,
        app: &App,
        settings: &Settings,
    ) {
        // Create enhanced title with search feedback
        let list_title = self.generate_list_title(app, settings);

        // Generate list items based on search mode
        let mut filtered_read_only_items = self.generate_list_items(app, settings);
        if !(app.view_mode == ViewMode::DualPane && app.is_right_pane_active()) {
            filtered_read_only_items = style_visual_rows(filtered_read_only_items, app);
        }

        // Dynamic layout based on view mode
        let inner_constraints = match app.view_mode {
            ViewMode::FullList => vec![Constraint::Percentage(100)],
            _ => vec![Constraint::Percentage(50), Constraint::Percentage(50)],
        };
        let inner_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(inner_constraints)
            .split(chunks[2]);

        // Create block with or without borders based on settings
        let block = if settings.show_borders {
            Block::default()
                .borders(Borders::ALL)
                .border_set(border::ROUNDED)
                .title(list_title.as_str())
                .style(match app.input_mode {
                    _ if app.searching_right_pane() => OneDarkTheme::inactive_border(),
                    InputMode::Normal | InputMode::Visual => {
                        if app.view_mode == ViewMode::DualPane && app.is_right_pane_active() {
                            OneDarkTheme::inactive_border()
                        } else {
                            OneDarkTheme::active_border()
                        }
                    }
                    InputMode::Editing => {
                        if app.global_search_mode {
                            OneDarkTheme::global_search()
                        } else {
                            OneDarkTheme::local_search()
                        }
                    }
                    _ => OneDarkTheme::inactive_border(),
                })
        } else {
            // Modern mode: no borders, clean look
            // Use active/inactive title style in dual pane mode
            let title_style = if app.view_mode == ViewMode::DualPane {
                if app.is_right_pane_active() {
                    OneDarkTheme::inactive_title()
                } else {
                    OneDarkTheme::active_title()
                }
            } else {
                Style::default().fg(Color::Rgb(92, 99, 112)) // Default muted title
            };
            Block::default()
                .borders(Borders::NONE)
                .title(list_title.as_str())
                .title_style(title_style)
                .padding(ratatui::widgets::Padding::horizontal(1))
        };

        let list_block = List::new(filtered_read_only_items.clone())
            .block(block)
            .highlight_style(OneDarkTheme::selected())
            .highlight_symbol(if settings.show_borders { "❯" } else { " " })
            .style(match app.input_mode {
                InputMode::Normal | InputMode::Visual => OneDarkTheme::normal(),
                InputMode::Editing => OneDarkTheme::normal(),
                InputMode::WatchDelete => OneDarkTheme::disabled(),
                InputMode::WatchCreate => OneDarkTheme::disabled(),
                InputMode::WatchRename => OneDarkTheme::disabled(),
                InputMode::WatchSort => OneDarkTheme::disabled(),
                _ => OneDarkTheme::disabled(),
            });

        f.render_stateful_widget(list_block.clone(), inner_layout[0], state);
    }

    pub fn render_preview_window(
        &self,
        f: &mut Frame<'_>,
        chunks: &Rc<[Rect]>,
        state: &mut ListState,
        app: &App,
        settings: &Settings,
    ) {
        let inner_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(vec![Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(chunks[2]);

        let filtered_curr_read_only_items: Vec<ListItem> = app
            .preview_files
            .iter()
            .map(|file| ListItem::from(file.clone()))
            .collect();

        // Create block with or without borders based on settings
        let block = if settings.show_borders {
            Block::default()
                .borders(Borders::ALL)
                .border_set(border::ROUNDED)
                .title("Preview")
                .style(match app.input_mode {
                    InputMode::Normal => OneDarkTheme::inactive_border(),
                    InputMode::Editing => OneDarkTheme::inactive_border(),
                    _ => OneDarkTheme::disabled(),
                })
        } else {
            // Modern mode: no borders, muted title
            Block::default()
                .borders(Borders::LEFT) // Only left border as separator
                .border_style(Style::default().fg(Color::Rgb(62, 68, 81))) // Subtle separator
                .title("Preview")
                .title_style(Style::default().fg(Color::Rgb(92, 99, 112)))
        };

        let list_preview_block = List::new(filtered_curr_read_only_items)
            .block(block)
            .style(Style::default().fg(Color::Rgb(92, 99, 112))); // Muted preview text

        f.render_stateful_widget(list_preview_block, inner_layout[1], state);
    }

    /// Generate an enhanced title that shows search feedback
    fn generate_list_title(&self, app: &App, settings: &Settings) -> String {
        if app.copy_in_progress {
            if !app.copy_progress_message.is_empty() {
                return app.copy_progress_message.clone();
            } else {
                return "Preparing copy...".to_string();
            }
        }

        if app.delete_in_progress {
            if !app.delete_progress_message.is_empty() {
                return app.delete_progress_message.clone();
            } else {
                return "Preparing delete...".to_string();
            }
        }

        if let Some(job) = &app.job {
            return job.status.clone();
        }

        if app.loading {
            return "Loading...".to_string();
        }

        if app.input_mode == InputMode::Editing
            && !app.input.is_empty()
            && !app.searching_right_pane()
        {
            let count = if app.global_search_mode {
                app.search_results.len()
            } else {
                app.filtered_indexes.len()
            };

            if settings.show_borders {
                if app.global_search_mode {
                    format!("Global Search: {} results", count)
                } else {
                    format!("Search: {} results", count)
                }
            } else {
                // Modern mode - simpler title, position in status bar
                format!("{} matches", count)
            }
        } else {
            let total = app.files.len();
            let is_left_active = !app.is_right_pane_active();
            if settings.show_borders {
                // Classic mode with borders
                if app.view_mode == ViewMode::DualPane {
                    if is_left_active {
                        format!("▸ Left: Files ({})", total)
                    } else {
                        format!("  Left: Files ({})", total)
                    }
                } else {
                    format!("Files ({})", total)
                }
            } else {
                // Modern mode - minimal title, position in status bar
                if app.view_mode == ViewMode::DualPane {
                    if is_left_active {
                        format!("▸ {} items", total)
                    } else {
                        format!("{} items", total)
                    }
                } else {
                    format!("{} items", total)
                }
            }
        }
    }

    /// Generate list items based on current search state (optimized with pagination)
    fn generate_list_items(&self, app: &App, settings: &Settings) -> Vec<ListItem<'_>> {
        let show_file_sizes = settings.show_size_bars;
        let search_term = highlight_term(app, false);

        if app.global_search_mode && !app.search_results.is_empty() {
            // Show global search results with highlighted search terms
            app.search_results
                .iter()
                .take(50) // Limit to top 50 results for performance
                .map(|result| self.search_result_item(result, search_term, app, show_file_sizes))
                .collect()
        } else {
            // Use all filtered indexes (or all files if no filter)
            let indexes: Vec<usize> = if !app.filtered_indexes.is_empty() {
                app.filtered_indexes.clone()
            } else {
                (0..app.files.len()).collect()
            };

            indexes
                .iter()
                .map(|&file_index| {
                    // Use pre-computed file name from app
                    let file_name = &app.file_read_only_label_list[file_index];
                    let file_path = &app.files[file_index];
                    let info = app.file_info[file_index];
                    let path = Path::new(file_path);
                    let is_dir = info.is_dir();
                    let colored_icon =
                        self.icon_provider
                            .get_colored_for_path(path, is_dir, &app.theme_colors);

                    if app.input_mode == InputMode::Editing && !search_term.is_empty() {
                        // Local search with highlighting
                        let score = app
                            .search_results
                            .iter()
                            .find(|r| r.original_index == file_index)
                            .map(|r| r.score)
                            .unwrap_or(0);

                        // Create highlighted filename with search term
                        let highlighted_line = highlight_search_term(
                            file_name,
                            search_term,
                            OneDarkTheme::normal(),
                            OneDarkTheme::search_highlight(),
                        );

                        // Add colored icon and score to the highlighted line
                        let mut spans = vec![ratatui::text::Span::styled(
                            format!("{} ", colored_icon.icon),
                            Style::default().fg(colored_icon.color),
                        )];
                        spans.extend(highlighted_line.spans);
                        spans.push(ratatui::text::Span::styled(
                            format!(" ({})", score),
                            OneDarkTheme::info(),
                        ));

                        // Add file size for files (not directories)
                        if show_file_sizes && !is_dir {
                            spans.push(Span::raw(" "));
                            spans.push(create_size_text(info.file_size()));
                        }

                        ListItem::from(ratatui::text::Line::from(spans))
                    } else {
                        // Check if this is a dotfile (hidden file)
                        let is_dotfile = file_name.starts_with('.');

                        // Normal display with colored icon (dimmed for dotfiles)
                        let mut spans = if is_dotfile {
                            vec![
                                Span::styled(
                                    format!("{} ", colored_icon.icon),
                                    OneDarkTheme::dotfile(),
                                ),
                                Span::styled(file_name.to_string(), OneDarkTheme::dotfile()),
                            ]
                        } else {
                            vec![
                                Span::styled(
                                    format!("{} ", colored_icon.icon),
                                    Style::default().fg(colored_icon.color),
                                ),
                                Span::raw(file_name.to_string()),
                            ]
                        };

                        // Add file size for files (not directories)
                        if show_file_sizes && !is_dir {
                            spans.push(Span::raw(" "));
                            spans.push(create_size_text(info.file_size()));
                        }

                        ListItem::from(ratatui::text::Line::from(spans))
                    }
                })
                .collect()
        }
    }

    /// The right pane's rows: its listing, or its search's results, highlighted like the
    /// left listing's while the query is typed.
    pub fn right_pane_items(&self, app: &App, settings: &Settings) -> Vec<ListItem<'static>> {
        let right_pane = &app.right_pane;
        // Generate list items for right pane using icon provider (with dotfile dimming)
        let listing_item = |path: &str, label: &str, is_dir: bool| {
            let icon = self.icon_provider.get_for_path(Path::new(path), is_dir);

            // Check if this is a dotfile (hidden file)
            let is_dotfile = label.starts_with('.');

            if is_dotfile {
                // Dim dotfiles, with `crate::theme`'s dimmer colour rather than this module's
                ListItem::new(ratatui::text::Line::from(vec![Span::styled(
                    format!("{} {}", icon, label),
                    crate::theme::OneDarkTheme::dotfile(),
                )]))
            } else {
                ListItem::new(format!("{} {}", icon, label))
            }
        };
        // A search shows its results, highlighted like the left listing's
        let search_term = highlight_term(app, true);
        if right_pane.is_searching() && (right_pane.global_search_mode || !search_term.is_empty()) {
            right_pane
                .search_results
                .iter()
                .map(|result| {
                    self.search_result_item(result, search_term, app, settings.show_size_bars)
                })
                .collect()
        } else if right_pane.is_searching() {
            right_pane
                .search_results
                .iter()
                .map(|result| {
                    listing_item(
                        &result.file_path,
                        &result.display_name,
                        result.info.is_dir(),
                    )
                })
                .collect()
        } else {
            right_pane
                .files
                .iter()
                .zip(&right_pane.file_labels)
                .zip(&right_pane.file_info)
                .map(|((path, label), info)| listing_item(path, label, info.is_dir()))
                .collect()
        }
    }

    /// A search result row: colored icon, name with `search_term` highlighted, score,
    /// and the size of a file.
    pub fn search_result_item(
        &self,
        result: &SearchResult,
        search_term: &str,
        app: &App,
        show_file_sizes: bool,
    ) -> ListItem<'static> {
        let colored_icon = self.icon_provider.get_colored_for_path(
            Path::new(&result.file_path),
            result.info.is_dir(),
            &app.theme_colors,
        );

        if !search_term.is_empty() {
            // Create highlighted filename with search term
            let highlighted_line = highlight_search_term(
                &result.display_name,
                search_term,
                OneDarkTheme::normal(),
                OneDarkTheme::search_highlight(),
            );

            // Add colored icon and score to the highlighted line
            let mut spans = vec![ratatui::text::Span::styled(
                format!("{} ", colored_icon.icon),
                Style::default().fg(colored_icon.color),
            )];
            spans.extend(highlighted_line.spans);
            spans.push(ratatui::text::Span::styled(
                format!(" ({})", result.score),
                OneDarkTheme::info(),
            ));

            // Add file size for files (not directories)
            if show_file_sizes && !result.info.is_dir() {
                spans.push(Span::raw(" "));
                spans.push(create_size_text(result.info.file_size()));
            }

            ListItem::from(ratatui::text::Line::from(spans))
        } else {
            // Fallback with colored icon
            let mut spans = vec![
                Span::styled(
                    format!("{} ", colored_icon.icon),
                    Style::default().fg(colored_icon.color),
                ),
                Span::raw(format!("{} ({})", result.display_name, result.score)),
            ];

            // Add file size for files (not directories)
            if show_file_sizes && !result.info.is_dir() {
                spans.push(Span::raw(" "));
                spans.push(create_size_text(result.info.file_size()));
            }

            ListItem::from(ratatui::text::Line::from(spans))
        }
    }
}

/// Mark the active pane's selected rows (visual mode) in the theme's selection colour,
/// names in the visual mode colour; the highlighted row keeps its own style on top.
pub fn style_visual_rows<'a>(items: Vec<ListItem<'a>>, app: &App) -> Vec<ListItem<'a>> {
    let Some(rows) = app.visual_rows() else {
        return items;
    };
    let style = Style::default()
        .bg(app.theme_colors.selection)
        .fg(app.theme_colors.statusbar_mode_visual);
    items
        .into_iter()
        .enumerate()
        .map(|(row, item)| {
            if rows.contains(&row) {
                item.style(style)
            } else {
                item
            }
        })
        .collect()
}

/// The search box's query to highlight in a pane's rows while it's typed there:
/// the left listing's, or the right pane's when `right_pane`.
pub fn highlight_term(app: &App, right_pane: bool) -> &str {
    if app.input_mode != InputMode::Editing
        || app.input.is_empty()
        || app.searching_right_pane() != right_pane
    {
        return "";
    }
    let term = app.input.trim();
    // Remove leading search indicators for highlighting
    if term.starts_with(' ') || term.starts_with('/') {
        term.trim_start_matches(' ').trim_start_matches('/')
    } else {
        term
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::{read_listing, SortBy, SortType};
    use ratatui::{backend::TestBackend, Terminal};
    use std::fs;

    /// A folder listing a 5-byte notes.txt and a photos/ folder.
    fn notes_and_photos() -> (tempfile::TempDir, String) {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("notes.txt"), "notes").unwrap();
        fs::create_dir(dir.path().join("photos")).unwrap();
        let start = dir.path().to_string_lossy().into_owned();
        (dir, start)
    }

    fn listing(start: &str) -> crate::utils::Listing {
        read_listing(start.to_string(), false, SortBy::Name, &SortType::ASC).unwrap()
    }

    /// Grow notes.txt to 2 KB and make photos a 3000-byte file, behind ff's back.
    fn change_on_disk(dir: &Path) {
        fs::write(dir.join("notes.txt"), "x".repeat(2048)).unwrap();
        fs::remove_dir(dir.join("photos")).unwrap();
        fs::write(dir.join("photos"), "x".repeat(3000)).unwrap();
    }

    /// The screen's rows.
    fn draw(width: u16, render: impl FnOnce(&mut Frame)) -> Vec<String> {
        let mut terminal = Terminal::new(TestBackend::new(width, 6)).unwrap();
        terminal.draw(render).unwrap();
        let buffer = terminal.backend().buffer();
        buffer
            .content()
            .chunks(buffer.area.width as usize)
            .map(|row| row.iter().map(|cell| cell.symbol()).collect())
            .collect()
    }

    fn row<'a>(rows: &'a [String], name: &str) -> &'a str {
        rows.iter().find(|row| row.contains(name)).unwrap()
    }

    #[test]
    fn left_rows_draw_from_the_records_until_the_listing_is_read_again() {
        let (dir, start) = notes_and_photos();
        let settings = Settings::default();
        let mut ui = Ui::new(Vec::new(), &settings);
        let mut app = App::with_listing(listing(&start));
        let mut state = ListState::default();
        let mut draw_left = |app: &App| {
            draw(100, |f| {
                let area = f.size();
                let chunks: Rc<[Rect]> = vec![area, area, area].into();
                ui.render_list_preview(f, &chunks, &mut ListState::default(), app, &settings);
            })
        };

        let before = draw_left(&app);
        assert!(row(&before, "notes.txt").contains("notes.txt 5 B"));
        assert!(!row(&before, "photos").contains(" B"));

        change_on_disk(dir.path());
        assert_eq!(draw_left(&app), before);

        app.refresh_files_from_watcher(listing(&start), &mut state);
        let after = draw_left(&app);
        assert!(row(&after, "notes.txt").contains("notes.txt 2.0 KB"));
        assert!(row(&after, "photos").contains("photos 2.9 KB"));
    }

    #[test]
    fn right_pane_rows_draw_from_the_records_until_the_listing_is_read_again() {
        let (dir, start) = notes_and_photos();
        let settings = Settings::default();
        let ui = Ui::new(Vec::new(), &settings);
        let mut app = App::new(Vec::new());
        app.view_mode = ViewMode::DualPane;
        app.right_pane = crate::pane::Pane::with_listing(start.clone(), listing(&start));
        let photos = dir.path().join("photos");
        let folder_row = format!("{} photos", ui.icon_provider.get_for_path(&photos, true));
        let file_row = format!("{} photos", ui.icon_provider.get_for_path(&photos, false));
        assert_ne!(folder_row, file_row);
        let draw_right = |app: &App| {
            draw(40, |f| {
                f.render_widget(List::new(ui.right_pane_items(app, &settings)), f.size())
            })
        };

        assert!(row(&draw_right(&app), "photos").contains(&folder_row));
        change_on_disk(dir.path());
        assert!(row(&draw_right(&app), "photos").contains(&folder_row));

        app.right_pane.refresh_files();
        assert!(row(&draw_right(&app), "photos").contains(&file_row));
    }

    #[test]
    fn right_pane_dims_dotfiles_with_the_dimmer_theme_colour() {
        let settings = Settings::default();
        let ui = Ui::new(Vec::new(), &settings);
        let mut app = App::new(Vec::new());
        app.right_pane = crate::pane::Pane::new("/w".into(), vec!["/w/.hidden".into()]);
        let mut terminal = Terminal::new(TestBackend::new(20, 1)).unwrap();
        terminal
            .draw(|f| f.render_widget(List::new(ui.right_pane_items(&app, &settings)), f.size()))
            .unwrap();
        let buffer = terminal.backend().buffer();
        let dot = buffer
            .content()
            .iter()
            .find(|cell| cell.symbol() == ".")
            .unwrap();
        assert_eq!(Some(dot.fg), crate::theme::OneDarkTheme::dotfile().fg);
    }
}
