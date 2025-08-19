use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, ListState},
    Frame,
};
use std::{path::Path, rc::Rc};

use crate::app::{App, InputMode};

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
        let mut list_title = String::new();
        if app.loading {
            let title_with_loader = format!("Copying Files...");
            list_title.push_str(&title_with_loader);
        } else {
            list_title.push_str(&"List");
        }

        let filtered_read_only_items: Vec<ListItem> = app
            .filtered_indexes
            .iter()
            .map(|file| ListItem::from(app.file_read_only_label_list[*file].clone()))
            //.map(|file| ListItem::from(self.files_list.items[*file].label.clone()))
            .collect();

        let inner_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(vec![Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(chunks[2]);

        let list_block = List::new(filtered_read_only_items.clone())
            //let list_block = List::new(filtered_items.clone())
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(list_title.as_str())
                    .style(match app.input_mode {
                        InputMode::Normal => Style::default().fg(Color::Green),
                        InputMode::Editing => Style::default().fg(Color::White),
                        _ => Style::default().fg(Color::White),
                    }), //.title("Filtered List"),
            )
            .highlight_style(
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol(">")
            .style(match app.input_mode {
                InputMode::Normal => Style::default().fg(Color::White),
                InputMode::Editing => Style::default().fg(Color::White),
                InputMode::WatchDelete => Style::default().fg(Color::Gray),
                InputMode::WatchCreate => Style::default().fg(Color::Gray),
                InputMode::WatchRename => Style::default().fg(Color::Gray),
                InputMode::WatchSort => Style::default().fg(Color::Gray),
                _ => Style::default().fg(Color::Gray),
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
                //let list_preview_block = List::new(app.preview_files.clone()).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Preview")
                    .style(match app.input_mode {
                        InputMode::Normal => Style::default().fg(Color::Green),
                        InputMode::Editing => Style::default().fg(Color::White),
                        _ => Style::default().fg(Color::White),
                    }), //.title("Filtered List"),
            )
            //.highlight_symbol(">")
            .style(Style::default().fg(Color::DarkGray));

        f.render_stateful_widget(list_preview_block, inner_layout[1], state);
    }
}
