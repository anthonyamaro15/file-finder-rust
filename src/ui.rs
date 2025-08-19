use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, ListState},
    Frame,
};
use std::{path::Path, rc::Rc};

use crate::app::{App, InputMode};
pub mod theme;
use self::theme::OneDarkTheme;

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
}

impl Ui {
    pub fn new(files: Vec<String>) -> Self {
        let mut current_item_list: Vec<ListFileItem> = Vec::new();
        for path in files.iter() {
            let new_path = Path::new(path);
            let get_file_name = new_path.file_name().unwrap().to_str().unwrap().to_string();
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

        Self {
            files_list: file_list,
        }
    }

    pub fn render_list_preview(
        &mut self,
        f: &mut Frame<'_>,
        chunks: &Rc<[Rect]>,
        state: &mut ListState,
        app: &App,
    ) {
        // Create enhanced title with search feedback
        let list_title = self.generate_list_title(app);
        
        // Generate list items based on search mode
        let filtered_read_only_items = self.generate_list_items(app);

        let inner_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(vec![Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(chunks[2]);

        let list_block = List::new(filtered_read_only_items.clone())
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(list_title.as_str())
                    .style(match app.input_mode {
                        InputMode::Normal => OneDarkTheme::active_border(),
                        InputMode::Editing => {
                            if app.global_search_mode {
                                OneDarkTheme::global_search()
                            } else {
                                OneDarkTheme::local_search()
                            }
                        },
                        _ => OneDarkTheme::inactive_border(),
                    }),
            )
            .highlight_style(OneDarkTheme::selected())
            .highlight_symbol("❯")
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

        let list_preview_block = List::new(filtered_curr_read_only_items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("📂 Preview")
                    .style(match app.input_mode {
                        InputMode::Normal => OneDarkTheme::inactive_border(),
                        InputMode::Editing => OneDarkTheme::inactive_border(),
                        _ => OneDarkTheme::disabled(),
                    }),
            )
            .style(OneDarkTheme::disabled());

        f.render_stateful_widget(list_preview_block, inner_layout[1], state);
    }
    
    /// Generate an enhanced title that shows search feedback
    fn generate_list_title(&self, app: &App) -> String {
        if app.copy_in_progress {
            if app.copy_total_files > 0 {
                let percentage = (app.copy_files_processed * 100) / app.copy_total_files;
                return format!("📦 Copying... {}% ({}/{})", 
                    percentage, app.copy_files_processed, app.copy_total_files);
            } else {
                return "📦 Preparing copy...".to_string();
            }
        }
        
        if app.loading {
            return "⏳ Loading...".to_string();
        }
        
        if app.input_mode == InputMode::Editing && !app.input.is_empty() {
            if app.global_search_mode {
                let count = app.search_results.len();
                format!("Global Search: {} results", count)
            } else {
                let count = app.filtered_indexes.len();
                format!("Search: {} results", count)
            }
        } else {
            let total = app.files.len();
            format!("Files ({})", total)
        }
    }
    
    /// Generate list items based on current search state (optimized with pagination)
    fn generate_list_items(&self, app: &App) -> Vec<ListItem> {
        if app.global_search_mode && !app.search_results.is_empty() {
            // Show global search results with enhanced formatting
            app.search_results
                .iter()
                .take(50) // Limit to top 50 results for performance
                .map(|result| {
                    let display_text = if result.is_directory {
                        format!("📁 {} ({})", result.display_name, result.score)
                    } else {
                        format!("📄 {} ({})", result.display_name, result.score)
                    };
                    ListItem::from(display_text)
                })
                .collect()
        } else {
            // Use pagination for large directories to improve performance
            let page_items = app.get_current_page_items();
            
            page_items
                .iter()
                .map(|&file_index| {
                    // Use pre-computed file name from app
                    let file_name = &app.file_read_only_label_list[file_index];
                    let file_path = &app.files[file_index];
                    let is_dir = Path::new(file_path).is_dir();
                    
                    // Add icons and search score if available
                    let display_text = if app.input_mode == InputMode::Editing && !app.input.is_empty() {
                        // Find the search result for this file to show score
                        let score = app.search_results
                            .iter()
                            .find(|r| r.original_index == file_index)
                            .map(|r| r.score)
                            .unwrap_or(0);
                            
                        if is_dir {
                            format!("📁 {} ({})", file_name, score)
                        } else {
                            format!("📄 {} ({})", file_name, score)
                        }
                    } else {
                        // Normal display without scores
                        if is_dir {
                            format!("📁 {}", file_name)
                        } else {
                            format!("📄 {}", file_name)
                        }
                    };
                    
                    ListItem::from(display_text)
                })
                .collect()
        }
    }
}
