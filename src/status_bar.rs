use crate::app::{App, InputMode, ViewMode};
use crate::config::theme::ThemeColors;
use crate::sizes::total_text;
use crate::utils::files::{EntryInfo, EntryKind};
use crate::utils::format::format_path_for_display;
use ratatui::{
    prelude::*,
    style::{Color, Modifier, Style},
    symbols::border,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};
use std::env;
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
        }
    }

    /// Describe the active pane from its listing's records, never the disk.
    pub fn update(&mut self, app: &App) {
        // Describe the active pane: its directory (even when empty) and listing
        let file_info = if is_right_pane(app) {
            &app.right_pane.file_info
        } else {
            &app.file_info
        };
        self.file_count = file_info.iter().filter(|info| info.is_file()).count();
        self.directory_count = file_info.iter().filter(|info| info.is_dir()).count();
        self.total_size = file_info.iter().map(EntryInfo::file_size).sum();

        self.current_directory = app.current_directory_path().to_string_lossy().to_string();
        self.update_selected_item_info(app);
        self.update_error_display();
    }

    fn update_selected_item_info(&mut self, app: &App) {
        if let Some(selected_path) = app.active_selected_path(app.curr_index) {
            let path = Path::new(&selected_path);

            if let Some(file_name) = path.file_name() {
                let name = file_name.to_string_lossy();
                let info = app.active_selected_info(app.curr_index).unwrap_or_default();

                if let Some(kind) = info.kind {
                    let file_type = match kind {
                        EntryKind::File => "File",
                        EntryKind::Folder => "Directory",
                        EntryKind::Other => "Other",
                    };

                    let size = Self::selected_size(app, path, &info);
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

    /// The selected item's size; in size order a folder shows its total, as its row does.
    fn selected_size(app: &App, path: &Path, info: &EntryInfo) -> String {
        match app.folder_sizes.total(path) {
            Some(total) if info.is_dir() && app.active_sizing().folder.is_some() => {
                total_text(total)
            }
            _ => Self::format_file_size(info.size),
        }
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

    fn get_mode_indicator(app: &App, theme: &ThemeColors) -> (&'static str, Color) {
        let label = match app.input_mode {
            InputMode::Normal => "NORMAL",
            InputMode::Editing => "SEARCH",
            InputMode::WatchDelete => "DELETE",
            InputMode::WatchTrash => "TRASH",
            InputMode::WatchCreate => "CREATE",
            InputMode::WatchRename => "RENAME",
            InputMode::WatchSort => "SORT",
            InputMode::WatchKeyBinding => "HELP",
            InputMode::CacheLoading => "LOADING",
            InputMode::PickFolder => "PICK",
            InputMode::WatchInfo => "INFO",
            InputMode::Visual => "VISUAL",
            InputMode::Palette => "PALETTE",
        };
        (label, Self::mode_color(app, theme))
    }

    /// The badge's color: the mode's, for the modes that have one.
    fn mode_color(app: &App, theme: &ThemeColors) -> Color {
        match app.input_mode {
            InputMode::WatchKeyBinding => theme.fg,
            InputMode::CacheLoading => theme.yellow,
            InputMode::PickFolder | InputMode::Palette => theme.blue,
            InputMode::WatchInfo => theme.cyan,
            _ => app.mode_color(),
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
        let (mode_text, mode_color) = Self::get_mode_indicator(app, theme);
        let mode_text = Self::mode_label(app, mode_text);
        // Wide enough for "VISUAL · 5 selected" inside its borders.
        let mode_width = u16::try_from(mode_text.chars().count() + 4).map_or(15, |w| w.max(15));
        // In size order: "7.1 GB here · 212 GB free".
        let size_text = app
            .active_sizing()
            .summary()
            .unwrap_or_else(|| Self::format_file_size(self.total_size));
        let size_width = u16::try_from(size_text.chars().count() + 4).map_or(15, |w| w.max(15));
        // Split the status bar into multiple sections
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(mode_width), // Mode indicator
                Constraint::Length(14),         // View mode indicator
                Constraint::Min(20),            // Directory path
                Constraint::Length(25),         // File counts
                Constraint::Length(size_width), // Total size
                Constraint::Length(20),         // System info
            ])
            .split(area);
        frame.render_widget(
            Block::default().style(Style::default().bg(theme.statusbar_bg)),
            area,
        );

        // Mode indicator with rounded borders, its name black on the mode's color
        let badge = Style::default()
            .fg(theme.black)
            .bg(mode_color)
            .add_modifier(Modifier::BOLD);
        let mode_widget = Paragraph::new(Line::styled(format!(" {mode_text} "), badge))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_set(border::ROUNDED)
                    .border_style(Style::default().fg(mode_color)),
            )
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
            .style(Style::default().fg(theme.statusbar_fg));
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
        let (mode_text, mode_color) = Self::get_mode_indicator_short(app, theme);
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
            Span::styled(path, Style::default().fg(theme.statusbar_fg)),
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
        // In size order: the folder's total and its disk's free space.
        let spans = match app.active_sizing().summary() {
            Some(summary) => [
                spans,
                vec![
                    Span::raw("  "),
                    Span::styled(summary, Style::default().fg(theme.gray)),
                ],
            ]
            .concat(),
            None => spans,
        };

        // A bar along the line it's drawn on.
        let paragraph =
            Paragraph::new(Line::from(spans)).style(Style::default().bg(theme.statusbar_bg));
        frame.render_widget(
            paragraph,
            Rect {
                height: area.height.min(1),
                ..area
            },
        );
    }

    /// Get short mode indicator for minimal status bar
    fn get_mode_indicator_short(app: &App, theme: &ThemeColors) -> (&'static str, Color) {
        let label = match app.input_mode {
            InputMode::Normal => "NOR",
            InputMode::Editing => "SRC",
            InputMode::WatchDelete => "DEL",
            InputMode::WatchTrash => "TRASH",
            InputMode::WatchCreate => "NEW",
            InputMode::WatchRename => "REN",
            InputMode::WatchSort => "SRT",
            InputMode::WatchKeyBinding => "HLP",
            InputMode::CacheLoading => "...",
            InputMode::PickFolder => "PCK",
            InputMode::WatchInfo => "INFO",
            InputMode::Visual => "VISUAL",
            InputMode::Palette => "PAL",
        };
        (label, Self::mode_color(app, theme))
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
        let path = app.active_selected_path(app.curr_index).unwrap_or_default();
        match app.active_selected_info(app.curr_index) {
            Some(info) if info.kind.is_some() => (
                Self::selected_size(app, Path::new(&path), &info),
                Self::format_permissions(&info),
            ),
            _ => ("--".to_string(), "----------".to_string()),
        }
    }

    /// Format Unix-style permissions string of what the item is (a link's target)
    #[cfg(unix)]
    fn format_permissions(info: &EntryInfo) -> String {
        let file_type = if info.is_dir() { 'd' } else { '-' };

        format!(
            "{}{}",
            file_type,
            crate::utils::format::permission_bits(info.mode)
        )
    }

    #[cfg(not(unix))]
    fn format_permissions(info: &EntryInfo) -> String {
        if info.is_dir() {
            "d---------".to_string()
        } else if info.mode & 0o222 == 0 {
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
    pub fn render_error_notification(&self, frame: &mut Frame, area: Rect, theme: &ThemeColors) {
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
                NotificationKind::Error => ("❌ Error", theme.red),
                NotificationKind::Warning => ("⚠ Warning", theme.yellow),
                NotificationKind::Info => ("ℹ Info", theme.green),
            };
            let error_block = Paragraph::new(error_msg.clone())
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(title)
                        .style(Style::default().fg(color)),
                )
                .style(Style::default().fg(theme.bright).bg(theme.gray))
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

    #[test]
    fn both_styles_show_the_badge_black_on_the_mode_color() {
        use ratatui::{backend::TestBackend, Terminal};
        let mut app = App::new(vec!["/w/a".into()]);
        let status_bar = StatusBar::new();
        let theme = app.theme_colors.clone();
        for (mode, global, label, color) in [
            (InputMode::Normal, false, "NOR", theme.mode_normal),
            (InputMode::Editing, false, "SRC", theme.mode_search),
            (InputMode::Editing, true, "SRC", theme.mode_global),
            (InputMode::WatchTrash, false, "TRASH", theme.mode_delete),
            (InputMode::WatchRename, false, "REN", theme.mode_rename),
        ] {
            app.input_mode = mode;
            app.global_search_mode = global;
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
                // The badge's first letter, inside the classic style's border: "NORMAL"
                // there, "NOR" in the minimal one.
                let row = usize::from(!minimal);
                let cell = terminal.backend().buffer().content()[row * 120..]
                    .iter()
                    .find(|cell| cell.symbol() == &label[..1])
                    .unwrap();
                assert_eq!(
                    (cell.fg, cell.bg, cell.modifier.contains(Modifier::BOLD)),
                    (theme.black, color, true),
                    "{label} minimal: {minimal}"
                );
            }
        }
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
    fn selected_item_info_comes_from_the_records_until_the_listing_is_read_again() {
        use crate::app::PanePosition;
        use crate::utils::{read_listing, SortBy, SortType};

        let dir = tempfile::tempdir().unwrap();
        let notes = dir.path().join("notes.txt");
        std::fs::write(&notes, "notes").unwrap();
        let start = dir.path().to_string_lossy().into_owned();
        let listing = || read_listing(start.clone(), false, SortBy::Name, &SortType::ASC).unwrap();
        let mut app = App::with_listing(listing());
        app.right_pane = crate::pane::Pane::with_listing(start.clone(), listing());
        let mut status_bar = StatusBar::new();
        let minimal = |status_bar: &StatusBar, app: &App| {
            let mut terminal =
                ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 1)).unwrap();
            terminal
                .draw(|f| status_bar.render_minimal(f, f.size(), app, &app.theme_colors))
                .unwrap();
            let screen: String = terminal
                .backend()
                .buffer()
                .content()
                .iter()
                .map(|cell| cell.symbol())
                .collect();
            screen
        };

        for right in [false, true] {
            if right {
                app.view_mode = ViewMode::DualPane;
                app.active_pane = PanePosition::Right;
            }
            std::fs::write(&notes, "notes").unwrap();
            app.refresh_files_from_watcher(listing(), &mut ratatui::widgets::ListState::default());
            app.right_pane.refresh_files();
            status_bar.update(&app);
            assert_eq!(status_bar.selected_item_info, "notes.txt | File | 5 B");
            assert!(minimal(&status_bar, &app).contains("5 B  -rw"));

            // Grown, then gone: still what the records say.
            std::fs::write(&notes, "x".repeat(2048)).unwrap();
            status_bar.update(&app);
            assert_eq!(status_bar.selected_item_info, "notes.txt | File | 5 B");
            assert_eq!(status_bar.total_size, 5);
            std::fs::remove_file(&notes).unwrap();
            status_bar.update(&app);
            assert_eq!(status_bar.selected_item_info, "notes.txt | File | 5 B");
            assert_eq!((status_bar.file_count, status_bar.directory_count), (1, 0));
            assert!(
                minimal(&status_bar, &app).contains("5 B  -rw"),
                "right: {right}"
            );

            std::fs::write(&notes, "x".repeat(2048)).unwrap();
            if right {
                app.right_pane.refresh_files();
            } else {
                app.refresh_files_from_watcher(
                    listing(),
                    &mut ratatui::widgets::ListState::default(),
                );
            }
            status_bar.update(&app);
            assert_eq!(status_bar.selected_item_info, "notes.txt | File | 2.0 KB");
            assert_eq!(status_bar.total_size, 2048);
            assert!(minimal(&status_bar, &app).contains("2.0 KB  -rw"));
        }
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
