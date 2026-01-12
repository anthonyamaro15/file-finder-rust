use crate::app::{App, InputMode, ViewMode};
use crate::utils::format::format_path_for_display;
use ratatui::{
    prelude::*,
    style::{Color, Modifier, Style},
    symbols::border,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};
use std::env;
use std::fs;
use std::path::Path;

pub struct StatusBar {
    pub file_count: usize,
    pub directory_count: usize,
    pub current_directory: String,
    pub total_size: u64,
    pub selected_item_info: String,
    pub system_info: String,
    pub error_message: Option<String>,
    pub error_display_time: Option<std::time::Instant>,
    // Cache tracking - only recalculate when directory changes
    cached_directory: String,
    cache_valid: bool,
}

impl StatusBar {
    pub fn new() -> Self {
        StatusBar {
            file_count: 0,
            directory_count: 0,
            current_directory: String::new(),
            total_size: 0,
            selected_item_info: String::new(),
            system_info: Self::get_system_info(),
            error_message: None,
            error_display_time: None,
            cached_directory: String::new(),
            cache_valid: false,
        }
    }

    pub fn update(&mut self, app: &App) {
        // Determine current directory first
        let current_dir = if !app.files.is_empty() {
            Path::new(&app.files[0])
                .parent()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default()
        } else {
            env::current_dir()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string()
        };

        // Check if we need to recalculate expensive metadata operations
        let needs_recalc = !self.cache_valid
            || self.cached_directory != current_dir
            || self.file_count + self.directory_count != app.files.len();

        if needs_recalc {
            self.update_file_counts(app);
            self.calculate_total_size(app);
            self.cached_directory = current_dir.clone();
            self.cache_valid = true;
        }

        // These are cheap operations - always update
        self.current_directory = current_dir;
        self.update_selected_item_info(app);
        self.update_error_display();
    }

    /// Invalidate cache - call this after file operations (create, delete, rename)
    pub fn invalidate_cache(&mut self) {
        self.cache_valid = false;
    }

    fn update_file_counts(&mut self, app: &App) {
        let mut file_count = 0;
        let mut directory_count = 0;

        for path in &app.files {
            if let Ok(metadata) = fs::metadata(path) {
                if metadata.is_file() {
                    file_count += 1;
                } else if metadata.is_dir() {
                    directory_count += 1;
                }
            }
        }

        self.file_count = file_count;
        self.directory_count = directory_count;
    }

    fn update_selected_item_info(&mut self, app: &App) {
        if let Some(index) = app.curr_index {
            if index < app.files.len() {
                let selected_path = &app.files[index];
                let path = Path::new(selected_path);

                if let Some(file_name) = path.file_name() {
                    let name = file_name.to_string_lossy();

                    if let Ok(metadata) = fs::metadata(selected_path) {
                        let file_type = if metadata.is_file() {
                            "File"
                        } else if metadata.is_dir() {
                            "Directory"
                        } else {
                            "Other"
                        };

                        let size = Self::format_file_size(metadata.len());
                        self.selected_item_info = format!("{} | {} | {}", name, file_type, size);
                    } else {
                        self.selected_item_info = format!("{} | Unknown", name);
                    }
                } else {
                    self.selected_item_info = "No selection".to_string();
                }
            } else {
                self.selected_item_info = "Invalid selection".to_string();
            }
        } else {
            self.selected_item_info = "No selection".to_string();
        }
    }

    fn calculate_total_size(&mut self, app: &App) {
        let mut total = 0u64;

        for path in &app.files {
            if let Ok(metadata) = fs::metadata(path) {
                if metadata.is_file() {
                    total += metadata.len();
                }
            }
        }

        self.total_size = total;
    }

    fn get_system_info() -> String {
        let os = env::consts::OS;
        let arch = env::consts::ARCH;
        format!("{} ({})", os, arch)
    }

    fn format_file_size(size: u64) -> String {
        const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
        let mut size = size as f64;
        let mut unit_index = 0;

        while size >= 1024.0 && unit_index < UNITS.len() - 1 {
            size /= 1024.0;
            unit_index += 1;
        }

        if unit_index == 0 {
            format!("{} {}", size as u64, UNITS[unit_index])
        } else {
            format!("{:.1} {}", size, UNITS[unit_index])
        }
    }

    fn get_mode_indicator(input_mode: &InputMode) -> (&'static str, Color) {
        match input_mode {
            InputMode::Normal => ("NORMAL", Color::Green),
            InputMode::Editing => ("SEARCH", Color::Blue),
            InputMode::WatchDelete => ("DELETE", Color::Red),
            InputMode::WatchCreate => ("CREATE", Color::Yellow),
            InputMode::WatchRename => ("RENAME", Color::Cyan),
            InputMode::WatchSort => ("SORT", Color::Magenta),
            InputMode::WatchKeyBinding => ("HELP", Color::White),
            InputMode::CacheLoading => ("LOADING", Color::Yellow),
        }
    }

    fn get_view_mode_indicator(view_mode: &ViewMode) -> (&'static str, Color) {
        match view_mode {
            ViewMode::Normal => ("Normal", Color::White),
            ViewMode::FullList => ("Full", Color::Cyan),
            ViewMode::DualPane => ("Dual", Color::Magenta),
        }
    }

    pub fn render(&self, frame: &mut Frame, area: Rect, app: &App) {
        // Split the status bar into multiple sections
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(15), // Mode indicator
                Constraint::Length(14), // View mode indicator
                Constraint::Min(20),    // Directory path
                Constraint::Length(25), // File counts
                Constraint::Length(15), // Total size
                Constraint::Length(20), // System info
            ])
            .split(area);

        // Mode indicator with rounded borders
        let (mode_text, mode_color) = Self::get_mode_indicator(&app.input_mode);
        let mode_widget = Paragraph::new(mode_text)
            .block(Block::default().borders(Borders::ALL).border_set(border::ROUNDED))
            .style(Style::default().fg(mode_color).add_modifier(Modifier::BOLD))
            .alignment(Alignment::Center);
        frame.render_widget(mode_widget, chunks[0]);

        // View mode indicator
        let (view_text, view_color) = Self::get_view_mode_indicator(&app.view_mode);
        let view_widget = Paragraph::new(view_text)
            .block(Block::default().borders(Borders::ALL).border_set(border::ROUNDED).title("View"))
            .style(Style::default().fg(view_color))
            .alignment(Alignment::Center);
        frame.render_widget(view_widget, chunks[1]);

        // Current directory with smart path formatting
        // Account for border (2 chars) and padding (2 chars) = 4 chars overhead
        let available_width = (chunks[2].width as usize).saturating_sub(4);
        let dir_text = format_path_for_display(&self.current_directory, available_width);

        let dir_widget = Paragraph::new(dir_text)
            .block(Block::default().borders(Borders::ALL).border_set(border::ROUNDED).title("Directory"))
            .style(Style::default().fg(Color::White));
        frame.render_widget(dir_widget, chunks[2]);

        // File and directory counts
        let counts_text = format!("{}F {}D", self.file_count, self.directory_count);
        let counts_widget = Paragraph::new(counts_text)
            .block(Block::default().borders(Borders::ALL).border_set(border::ROUNDED).title("Items"))
            .style(Style::default().fg(Color::Cyan))
            .alignment(Alignment::Center);
        frame.render_widget(counts_widget, chunks[3]);

        // Total size
        let size_text = Self::format_file_size(self.total_size);
        let size_widget = Paragraph::new(size_text)
            .block(Block::default().borders(Borders::ALL).border_set(border::ROUNDED).title("Size"))
            .style(Style::default().fg(Color::Yellow))
            .alignment(Alignment::Center);
        frame.render_widget(size_widget, chunks[4]);

        // System info
        let system_widget = Paragraph::new(self.system_info.clone())
            .block(Block::default().borders(Borders::ALL).border_set(border::ROUNDED).title("System"))
            .style(Style::default().fg(Color::Gray))
            .alignment(Alignment::Center);
        frame.render_widget(system_widget, chunks[5]);
    }

    pub fn render_detailed(&self, frame: &mut Frame, area: Rect, app: &App) {
        // More detailed status bar for when there's more vertical space
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Main status line
                Constraint::Length(3), // Selected item info
            ])
            .split(area);

        // Render main status bar
        self.render(frame, chunks[0], app);

        // Selected item details
        let selected_widget = Paragraph::new(self.selected_item_info.clone())
            .block(Block::default().borders(Borders::ALL).title("Selection"))
            .style(Style::default().fg(Color::LightGreen));
        frame.render_widget(selected_widget, chunks[1]);
    }

    /// Show an error message for a specified duration (default: 3 seconds)
    pub fn show_error(&mut self, message: String, duration_secs: Option<u64>) {
        self.error_message = Some(message);
        self.error_display_time = Some(std::time::Instant::now());
    }

    /// Clear the current error message
    pub fn clear_error(&mut self) {
        self.error_message = None;
        self.error_display_time = None;
    }

    /// Update error display (auto-hide after timeout)
    fn update_error_display(&mut self) {
        if let (Some(_), Some(display_time)) = (&self.error_message, self.error_display_time) {
            // Auto-hide error after 3 seconds
            if display_time.elapsed() > std::time::Duration::from_secs(3) {
                self.clear_error();
            }
        }
    }

    /// Check if there's an active error message
    pub fn has_error(&self) -> bool {
        self.error_message.is_some()
    }

    /// Render error notification as an overlay
    pub fn render_error_notification(&self, frame: &mut Frame, area: Rect) {
        if let Some(ref error_msg) = self.error_message {
            // Calculate popup size (60% width, minimum height for message)
            let popup_width = (area.width * 6) / 10;
            let popup_height = 5;

            let popup_x = (area.width - popup_width) / 2;
            let popup_y = 2; // Show near top of screen

            let popup_area = Rect {
                x: popup_x,
                y: popup_y,
                width: popup_width,
                height: popup_height,
            };

            // Clear the area
            let clear_widget = ratatui::widgets::Clear;
            frame.render_widget(clear_widget, popup_area);

            // Render error message
            let error_block = Paragraph::new(error_msg.clone())
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title("❌ Error")
                        .style(Style::default().fg(Color::Red)),
                )
                .style(Style::default().fg(Color::White).bg(Color::DarkGray))
                .wrap(ratatui::widgets::Wrap { trim: true });

            frame.render_widget(error_block, popup_area);
        }
    }

    pub fn get_status_text(&self, app: &App) -> Vec<Line> {
        let (mode_text, mode_color) = Self::get_mode_indicator(&app.input_mode);

        // If there's an error, show it in the status line
        if let Some(ref error_msg) = self.error_message {
            return vec![Line::from(vec![
                Span::styled(
                    "❌ ERROR: ",
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                ),
                Span::styled(error_msg.clone(), Style::default().fg(Color::White)),
            ])];
        }

        vec![Line::from(vec![
            Span::styled(
                format!("[{}]", mode_text),
                Style::default().fg(mode_color).add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::styled(&self.current_directory, Style::default().fg(Color::White)),
            Span::raw(" | "),
            Span::styled(
                format!("{}F/{}D", self.file_count, self.directory_count),
                Style::default().fg(Color::Cyan),
            ),
            Span::raw(" | "),
            Span::styled(
                Self::format_file_size(self.total_size),
                Style::default().fg(Color::Yellow),
            ),
            Span::raw(" | "),
            Span::styled(&self.system_info, Style::default().fg(Color::Gray)),
        ])]
    }
}
