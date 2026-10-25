use crate::app::{App, InputMode, ViewMode};
use crate::config::theme::ThemeColors;
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
use std::time::{Duration, Instant};

const DEFAULT_NOTIFICATION_SECS: u64 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationKind {
    Error,
    Warning,
    Info,
}

pub struct StatusBar {
    pub file_count: usize,
    pub directory_count: usize,
    pub current_directory: String,
    pub total_size: u64,
    pub selected_item_info: String,
    pub system_info: String,
    pub error_message: Option<String>,
    pub error_display_time: Option<Instant>,
    pub notification_kind: NotificationKind,
    pub notification_duration: Duration,
    // Cache tracking - only recalculate when the active pane or its directory changes:
    // (right pane active, directory); None when invalidated
    cached_listing: Option<(bool, String)>,
}

impl Default for StatusBar {
    fn default() -> Self {
        Self::new()
    }
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
            notification_kind: NotificationKind::Error,
            notification_duration: Duration::from_secs(DEFAULT_NOTIFICATION_SECS),
            cached_listing: None,
        }
    }

    pub fn update(&mut self, app: &App) {
        // Describe the active pane: its directory (even when empty) and listing
        let right_pane = is_right_pane(app);
        let current_dir = app.current_directory_path().to_string_lossy().to_string();
        let files = if right_pane {
            &app.right_pane.files
        } else {
            &app.files
        };

        // Check if we need to recalculate expensive metadata operations
        let listing = (right_pane, current_dir.clone());
        let needs_recalc = self.cached_listing.as_ref() != Some(&listing)
            || self.file_count + self.directory_count != files.len();

        if needs_recalc {
            self.update_file_counts(files);
            self.calculate_total_size(files);
            self.cached_listing = Some(listing);
        }

        // These are cheap operations - always update
        self.current_directory = current_dir;
        self.update_selected_item_info(app);
        self.update_error_display();
    }

    /// Invalidate cache - call this after file operations (create, delete, rename)
    pub fn invalidate_cache(&mut self) {
        self.cached_listing = None;
    }

    fn update_file_counts(&mut self, files: &[String]) {
        let mut file_count = 0;
        let mut directory_count = 0;

        for path in files {
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
        if let Some(selected_path) = app.active_selected_path(app.curr_index) {
            let path = Path::new(&selected_path);

            if let Some(file_name) = path.file_name() {
                let name = file_name.to_string_lossy();

                if let Ok(metadata) = fs::metadata(&selected_path) {
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
            self.selected_item_info = "No selection".to_string();
        }
    }

    fn calculate_total_size(&mut self, files: &[String]) {
        let mut total = 0u64;

        for path in files {
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

    fn get_mode_indicator(input_mode: &InputMode, theme: &ThemeColors) -> (&'static str, Color) {
        match input_mode {
            InputMode::Normal => ("NORMAL", theme.green),
            InputMode::Editing => ("SEARCH", theme.blue),
            InputMode::WatchDelete => ("DELETE", theme.red),
            InputMode::WatchTrash => ("TRASH", theme.red),
            InputMode::WatchCreate => ("CREATE", theme.yellow),
            InputMode::WatchRename => ("RENAME", theme.cyan),
            InputMode::WatchSort => ("SORT", theme.purple),
            InputMode::WatchKeyBinding => ("HELP", theme.fg),
            InputMode::CacheLoading => ("LOADING", theme.yellow),
            InputMode::PickFolder => ("PICK", theme.blue),
            InputMode::WatchInfo => ("INFO", theme.cyan),
            InputMode::Visual => ("VISUAL", theme.statusbar_mode_visual),
        }
    }

    /// The mode's label, with the selection's size in visual mode: "VISUAL · 5 selected".
    fn mode_label(app: &App, label: &str) -> String {
        match &app.visual {
            Some(visual) if app.input_mode == InputMode::Visual => {
                format!("{label} · {} selected", visual.paths.len())
            }
            _ => label.to_string(),
        }
    }

    fn get_view_mode_indicator(view_mode: &ViewMode, theme: &ThemeColors) -> (&'static str, Color) {
        match view_mode {
            ViewMode::Normal => ("Normal", theme.fg),
            ViewMode::FullList => ("Full", theme.cyan),
            ViewMode::DualPane => ("Dual", theme.purple),
        }
    }

    pub fn render(&self, frame: &mut Frame, area: Rect, app: &App, theme: &ThemeColors) {
        let (mode_text, mode_color) = Self::get_mode_indicator(&app.input_mode, theme);
        let mode_text = Self::mode_label(app, mode_text);
        // Wide enough for "VISUAL · 5 selected" inside its borders.
        let mode_width = u16::try_from(mode_text.chars().count() + 4).map_or(15, |w| w.max(15));
        // Split the status bar into multiple sections
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(mode_width), // Mode indicator
                Constraint::Length(14),         // View mode indicator
                Constraint::Min(20),            // Directory path
                Constraint::Length(25),         // File counts
                Constraint::Length(15),         // Total size
                Constraint::Length(20),         // System info
            ])
            .split(area);

        // Mode indicator with rounded borders
        let mode_widget = Paragraph::new(mode_text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_set(border::ROUNDED),
            )
            .style(Style::default().fg(mode_color).add_modifier(Modifier::BOLD))
            .alignment(Alignment::Center);
        frame.render_widget(mode_widget, chunks[0]);

        // View mode indicator
        let (view_text, view_color) = Self::get_view_mode_indicator(&app.view_mode, theme);
        let view_widget = Paragraph::new(view_text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_set(border::ROUNDED)
                    .title("View"),
            )
            .style(Style::default().fg(view_color))
            .alignment(Alignment::Center);
        frame.render_widget(view_widget, chunks[1]);

        // Current directory with smart path formatting
        // Account for border (2 chars) and padding (2 chars) = 4 chars overhead
        let available_width = (chunks[2].width as usize).saturating_sub(4);
        let dir_text = format_path_for_display(&self.current_directory, available_width);

        let dir_widget = Paragraph::new(dir_text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_set(border::ROUNDED)
                    .title("Directory"),
            )
            .style(Style::default().fg(theme.fg));
        frame.render_widget(dir_widget, chunks[2]);

        // File and directory counts
        let counts_text = format!("{}F {}D", self.file_count, self.directory_count);
        let counts_widget = Paragraph::new(counts_text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_set(border::ROUNDED)
                    .title("Items"),
            )
            .style(Style::default().fg(theme.cyan))
            .alignment(Alignment::Center);
        frame.render_widget(counts_widget, chunks[3]);

        // Total size
        let size_text = Self::format_file_size(self.total_size);
        let size_widget = Paragraph::new(size_text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_set(border::ROUNDED)
                    .title("Size"),
            )
            .style(Style::default().fg(theme.yellow))
            .alignment(Alignment::Center);
        frame.render_widget(size_widget, chunks[4]);

        // System info
        let system_widget = Paragraph::new(self.system_info.clone())
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_set(border::ROUNDED)
                    .title("System"),
            )
            .style(Style::default().fg(theme.gray))
            .alignment(Alignment::Center);
        frame.render_widget(system_widget, chunks[5]);
    }

    /// Render minimal status bar - single clean line with essential info
    /// Format:  NOR  ~/path  position  size  permissions
    pub fn render_minimal(&self, frame: &mut Frame, area: Rect, app: &App, theme: &ThemeColors) {
        let (mode_text, mode_color) = Self::get_mode_indicator_short(&app.input_mode, theme);
        let mode_text = Self::mode_label(app, mode_text);

        // Calculate position indicator
        let position = Self::get_position_indicator(app);

        // Get selected file size and permissions
        let (size_text, perms_text) = self.get_selected_file_info(app);

        // Format path with home directory abbreviation
        let path = Self::abbreviate_home_path(&self.current_directory);

        // Build the status line with spans
        let spans = vec![
            // Mode indicator with background
            Span::styled(
                format!(" {} ", mode_text),
                Style::default()
                    .fg(theme.black)
                    .bg(mode_color)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            // Path
            Span::styled(path, Style::default().fg(theme.fg)),
            Span::raw("  "),
            // Position
            Span::styled(position, Style::default().fg(theme.gray)), // Muted
            Span::raw("  "),
            // Size
            Span::styled(size_text, Style::default().fg(theme.gray)), // Muted
            Span::raw("  "),
            // Permissions
            Span::styled(perms_text, Style::default().fg(theme.gray)), // Muted
        ];

        let paragraph = Paragraph::new(Line::from(spans));
        frame.render_widget(paragraph, area);
    }

    /// Get short mode indicator for minimal status bar
    fn get_mode_indicator_short(
        input_mode: &InputMode,
        theme: &ThemeColors,
    ) -> (&'static str, Color) {
        match input_mode {
            InputMode::Normal => ("NOR", theme.green),
            InputMode::Editing => ("SRC", theme.blue),
            InputMode::WatchDelete => ("DEL", theme.red),
            InputMode::WatchTrash => ("TRASH", theme.red),
            InputMode::WatchCreate => ("NEW", theme.yellow),
            InputMode::WatchRename => ("REN", theme.cyan),
            InputMode::WatchSort => ("SRT", theme.purple),
            InputMode::WatchKeyBinding => ("HLP", theme.fg),
            InputMode::CacheLoading => ("...", theme.yellow),
            InputMode::PickFolder => ("PCK", theme.blue),
            InputMode::WatchInfo => ("INFO", theme.cyan),
            InputMode::Visual => ("VISUAL", theme.statusbar_mode_visual),
        }
    }

    /// Get position indicator: "Top", "Bot", or "N/M"
    fn get_position_indicator(app: &App) -> String {
        let (index, total) = if is_right_pane(app) {
            (
                app.right_pane.selected_index,
                app.right_pane.get_list_length(),
            )
        } else {
            (app.curr_index, app.get_list_length())
        };

        if total == 0 {
            return "Empty".to_string();
        }

        let current = index.unwrap_or(0) + 1; // 1-indexed for display

        if current == 1 {
            "Top".to_string()
        } else if current == total {
            "Bot".to_string()
        } else {
            format!("{}/{}", current, total)
        }
    }

    /// Abbreviate home directory path with ~
    fn abbreviate_home_path(path: &str) -> String {
        if let Some(home) = dirs::home_dir() {
            let home_str = home.to_string_lossy();
            if path.starts_with(home_str.as_ref()) {
                return path.replacen(home_str.as_ref(), "~", 1);
            }
        }
        path.to_string()
    }

    /// Get size and permissions for selected file
    fn get_selected_file_info(&self, app: &App) -> (String, String) {
        if let Some(path) = app.active_selected_path(app.curr_index) {
            if let Ok(metadata) = fs::metadata(path) {
                let size = Self::format_file_size(metadata.len());
                let perms = Self::format_permissions(&metadata);
                return (size, perms);
            }
        }
        ("--".to_string(), "----------".to_string())
    }

    /// Format Unix-style permissions string
    #[cfg(unix)]
    fn format_permissions(metadata: &fs::Metadata) -> String {
        use std::os::unix::fs::PermissionsExt;
        let mode = metadata.permissions().mode();

        let file_type = if metadata.is_dir() {
            'd'
        } else if metadata.file_type().is_symlink() {
            'l'
        } else {
            '-'
        };

        format!(
            "{}{}",
            file_type,
            crate::utils::format::permission_bits(mode)
        )
    }

    #[cfg(not(unix))]
    fn format_permissions(metadata: &fs::Metadata) -> String {
        if metadata.is_dir() {
            "d---------".to_string()
        } else if metadata.permissions().readonly() {
            "-r--r--r--".to_string()
        } else {
            "-rw-rw-rw-".to_string()
        }
    }

    /// Show an error message for a specified duration (default: 3 seconds)
    pub fn show_error(&mut self, message: String, duration_secs: Option<u64>) {
        self.show_notification(NotificationKind::Error, message, duration_secs);
    }

    /// Show a warning message for a specified duration (default: 3 seconds)
    pub fn show_warning(&mut self, message: String, duration_secs: Option<u64>) {
        self.show_notification(NotificationKind::Warning, message, duration_secs);
    }

    /// Show an informational/success message for a specified duration (default: 3 seconds)
    pub fn show_info(&mut self, message: String, duration_secs: Option<u64>) {
        self.show_notification(NotificationKind::Info, message, duration_secs);
    }

    fn show_notification(
        &mut self,
        kind: NotificationKind,
        message: String,
        duration_secs: Option<u64>,
    ) {
        self.error_message = Some(message);
        self.error_display_time = Some(Instant::now());
        self.notification_kind = kind;
        self.notification_duration =
            Duration::from_secs(duration_secs.unwrap_or(DEFAULT_NOTIFICATION_SECS));
    }

    /// Clear the current error message
    pub fn clear_error(&mut self) {
        self.error_message = None;
        self.error_display_time = None;
    }

    /// Update error display (auto-hide after timeout)
    fn update_error_display(&mut self) {
        if let (Some(_), Some(display_time)) = (&self.error_message, self.error_display_time) {
            if display_time.elapsed() > self.notification_duration {
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

            let (title, color) = match self.notification_kind {
                NotificationKind::Error => ("❌ Error", Color::Red),
                NotificationKind::Warning => ("⚠ Warning", Color::Yellow),
                NotificationKind::Info => ("ℹ Info", Color::Green),
            };
            let error_block = Paragraph::new(error_msg.clone())
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(title)
                        .style(Style::default().fg(color)),
                )
                .style(Style::default().fg(Color::White).bg(Color::DarkGray))
                .wrap(ratatui::widgets::Wrap { trim: true });

            frame.render_widget(error_block, popup_area);
        }
    }
}

/// Whether the status bar describes the right pane: it's active in dual pane.
fn is_right_pane(app: &App) -> bool {
    app.view_mode.is_dual_pane() && app.is_right_pane_active()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_styles_show_visual_mode_with_the_selection_size() {
        use ratatui::{backend::TestBackend, Terminal};
        let mut app = App::new(vec!["/w/a".into(), "/w/b".into(), "/w/c".into()]);
        app.start_visual(Some(0)).unwrap();
        app.move_visual_cursor(Some(1));
        let status_bar = StatusBar::new();
        for minimal in [false, true] {
            let mut terminal = Terminal::new(TestBackend::new(120, 3)).unwrap();
            terminal
                .draw(|f| {
                    if minimal {
                        status_bar.render_minimal(f, f.size(), &app, &app.theme_colors);
                    } else {
                        status_bar.render(f, f.size(), &app, &app.theme_colors);
                    }
                })
                .unwrap();
            let screen: String = terminal
                .backend()
                .buffer()
                .content()
                .iter()
                .map(|cell| cell.symbol())
                .collect();
            assert!(screen.contains("VISUAL · 2 selected"), "{screen}");
        }
        app.end_visual();
        assert_eq!(StatusBar::mode_label(&app, "NORMAL"), "NORMAL");
    }

    fn shown_for(status_bar: &mut StatusBar, elapsed: Duration) -> bool {
        status_bar.error_display_time = Some(Instant::now() - elapsed);
        status_bar.update_error_display();
        status_bar.has_error()
    }

    #[test]
    fn notification_honors_duration() {
        let mut status_bar = StatusBar::new();

        status_bar.show_info("copied".to_string(), Some(1));
        assert_eq!(status_bar.notification_duration, Duration::from_secs(1));
        assert!(shown_for(&mut status_bar, Duration::from_millis(500)));
        assert!(!shown_for(&mut status_bar, Duration::from_millis(1100)));

        status_bar.show_error("failed".to_string(), None);
        assert_eq!(status_bar.notification_duration, Duration::from_secs(3));
        assert!(shown_for(&mut status_bar, Duration::from_millis(2900)));
        assert!(!shown_for(&mut status_bar, Duration::from_millis(3100)));
    }

    #[test]
    fn notification_kind_follows_show_method() {
        let mut status_bar = StatusBar::new();

        status_bar.show_info("copied".to_string(), None);
        assert_eq!(status_bar.notification_kind, NotificationKind::Info);

        status_bar.show_warning("fallback".to_string(), None);
        assert_eq!(status_bar.notification_kind, NotificationKind::Warning);

        status_bar.show_error("failed".to_string(), None);
        assert_eq!(status_bar.notification_kind, NotificationKind::Error);
        assert_eq!(status_bar.error_message.as_deref(), Some("failed"));
    }

    #[test]
    fn update_describes_the_active_right_pane() {
        use crate::app::PanePosition;
        use crate::utils::{get_file_path_data, SortBy, SortType};

        let dir = tempfile::tempdir().unwrap();
        let folder = |name: &str| dir.path().join(name).to_string_lossy().into_owned();
        let (left, right) = (folder("left"), folder("right"));
        std::fs::create_dir_all(Path::new(&right).join("sub")).unwrap();
        std::fs::create_dir(&left).unwrap();
        std::fs::write(Path::new(&left).join("left.txt"), "1").unwrap();
        std::fs::write(Path::new(&right).join("a.txt"), "12345").unwrap();
        std::fs::write(Path::new(&right).join("b.txt"), "1234567").unwrap();
        let mut app = App::new(
            get_file_path_data(left.clone(), false, SortBy::Name, &SortType::ASC).unwrap(),
        );
        app.current_directory = left.clone();
        app.view_mode = ViewMode::DualPane;
        app.right_pane = crate::pane::Pane::new(right.clone(), Vec::new());
        app.right_pane.sort_by = SortBy::Name;
        app.right_pane.refresh_files();
        app.right_pane
            .select_path(&Path::new(&right).join("b.txt").to_string_lossy());
        let mut status_bar = StatusBar::new();

        status_bar.update(&app);
        assert_eq!(status_bar.current_directory, left);
        assert_eq!((status_bar.file_count, status_bar.directory_count), (1, 0));
        assert!(status_bar.selected_item_info.starts_with("left.txt | File"));

        app.active_pane = PanePosition::Right;
        status_bar.update(&app);
        assert_eq!(status_bar.current_directory, right);
        assert_eq!((status_bar.file_count, status_bar.directory_count), (2, 1));
        assert_eq!(status_bar.total_size, 12);
        assert_eq!(status_bar.selected_item_info, "b.txt | File | 7 B");
        assert_eq!(StatusBar::get_position_indicator(&app), "2/3");
        assert_eq!(status_bar.get_selected_file_info(&app).0, "7 B");
    }

    #[test]
    fn update_shows_an_empty_folder_being_browsed() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = App::new(Vec::new());
        app.current_directory = dir.path().to_string_lossy().into_owned();
        let mut status_bar = StatusBar::new();

        status_bar.update(&app);

        assert_eq!(status_bar.current_directory, app.current_directory);
        assert_eq!(StatusBar::get_position_indicator(&app), "Empty");
    }
}
