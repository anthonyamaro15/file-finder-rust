use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    symbols::border,
    text::Span,
    widgets::{Block, Borders, List, ListItem, ListState},
    Frame,
};
use std::{path::Path, rc::Rc};

use crate::app::{App, InputMode, ViewMode};
use crate::config::Settings;
use crate::icons::IconProvider;
use crate::render::{create_size_text, get_file_size};
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
        let filtered_read_only_items = self.generate_list_items(app, settings);

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
                    InputMode::Normal => {
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
            Block::default()
                .borders(Borders::NONE)
                .title(list_title.as_str())
                .title_style(Style::default().fg(Color::Rgb(92, 99, 112))) // Muted title
                .padding(ratatui::widgets::Padding::horizontal(1))
        };

        let list_block = List::new(filtered_read_only_items.clone())
            .block(block)
            .highlight_style(OneDarkTheme::selected())
            .highlight_symbol(if settings.show_borders { "❯" } else { " " })
            .style(match app.input_mode {
                InputMode::Normal => OneDarkTheme::normal(),
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
            if app.copy_total_files > 0 {
                let percentage = (app.copy_files_processed * 100) / app.copy_total_files;
                return format!(
                    "Copying... {}% ({}/{})",
                    percentage, app.copy_files_processed, app.copy_total_files
                );
            } else {
                return "Preparing copy...".to_string();
            }
        }

        if app.loading {
            return "Loading...".to_string();
        }

        if app.input_mode == InputMode::Editing && !app.input.is_empty() {
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
            if settings.show_borders {
                // Classic mode with borders
                if app.view_mode == ViewMode::DualPane {
                    format!("Left: Files ({})", total)
                } else {
                    format!("Files ({})", total)
                }
            } else {
                // Modern mode - minimal title, position in status bar
                format!("{} items", total)
            }
        }
    }

    /// Generate list items based on current search state (optimized with pagination)
    fn generate_list_items(&self, app: &App, settings: &Settings) -> Vec<ListItem> {
        let show_file_sizes = settings.show_size_bars;
        let search_term = if app.input_mode == InputMode::Editing && !app.input.is_empty() {
            let term = app.input.trim();
            // Remove leading search indicators for highlighting
            if term.starts_with(' ') || term.starts_with('/') {
                term.trim_start_matches(' ').trim_start_matches('/')
            } else {
                term
            }
        } else {
            ""
        };

        if app.global_search_mode && !app.search_results.is_empty() {
            // Show global search results with highlighted search terms
            app.search_results
                .iter()
                .take(50) // Limit to top 50 results for performance
                .map(|result| {
                    let icon = self.icon_provider.get_for_path(
                        Path::new(&result.file_path),
                        result.is_directory,
                    );

                    if !search_term.is_empty() {
                        // Create highlighted filename with search term
                        let highlighted_line = highlight_search_term(
                            &result.display_name,
                            search_term,
                            OneDarkTheme::normal(),
                            OneDarkTheme::search_highlight(),
                        );

                        // Add icon and score to the highlighted line
                        let mut spans = vec![ratatui::text::Span::styled(
                            format!("{} ", icon),
                            OneDarkTheme::normal(),
                        )];
                        spans.extend(highlighted_line.spans);
                        spans.push(ratatui::text::Span::styled(
                            format!(" ({})", result.score),
                            OneDarkTheme::info(),
                        ));

                        // Add file size for files (not directories)
                        if show_file_sizes && !result.is_directory {
                            let file_size = get_file_size(&result.file_path);
                            spans.push(Span::raw(" "));
                            spans.push(create_size_text(file_size));
                        }

                        ListItem::from(ratatui::text::Line::from(spans))
                    } else {
                        // Fallback to simple text with file size
                        let mut spans = vec![
                            Span::raw(format!("{} {} ({})", icon, result.display_name, result.score)),
                        ];

                        // Add file size for files (not directories)
                        if show_file_sizes && !result.is_directory {
                            let file_size = get_file_size(&result.file_path);
                            spans.push(Span::raw(" "));
                            spans.push(create_size_text(file_size));
                        }

                        ListItem::from(ratatui::text::Line::from(spans))
                    }
                })
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
                    let path = Path::new(file_path);
                    let is_dir = path.is_dir();
                    let icon = self.icon_provider.get_for_path(path, is_dir);

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

                        // Add icon and score to the highlighted line
                        let mut spans = vec![ratatui::text::Span::styled(
                            format!("{} ", icon),
                            OneDarkTheme::normal(),
                        )];
                        spans.extend(highlighted_line.spans);
                        spans.push(ratatui::text::Span::styled(
                            format!(" ({})", score),
                            OneDarkTheme::info(),
                        ));

                        // Add file size for files (not directories)
                        if show_file_sizes && !is_dir {
                            let file_size = get_file_size(file_path);
                            spans.push(Span::raw(" "));
                            spans.push(create_size_text(file_size));
                        }

                        ListItem::from(ratatui::text::Line::from(spans))
                    } else {
                        // Normal display without highlighting
                        if show_file_sizes && !is_dir {
                            // With file size
                            let file_size = get_file_size(file_path);
                            let spans = vec![
                                Span::raw(format!("{} {} ", icon, file_name)),
                                create_size_text(file_size),
                            ];
                            ListItem::from(ratatui::text::Line::from(spans))
                        } else {
                            // Without file size (directories or disabled)
                            let display_text = format!("{} {}", icon, file_name);
                            ListItem::from(display_text)
                        }
                    }
                })
                .collect()
        }
    }
}
