use crate::app::{App, InputMode, ViewMode};
use crate::config::theme::ThemeColors;
use crate::info::plural;
use crate::sizes::{total_text, FolderTotal};
use crate::ui::truncate;
use crate::utils::files::{EntryInfo, EntryKind};
use crate::utils::format::{format_file_size, modified_text, replace_home_with_tilde, LocalStamp};
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
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

const DEFAULT_NOTIFICATION_SECS: u64 = 3;
/// Most rows a message takes, growing up from the hint row over the lists.
const MESSAGE_ROWS: usize = 4;
/// Fewest columns the highlighted item's name keeps before its details are cut.
const MIN_NAME_WIDTH: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationKind {
    Error,
    Warning,
    Info,
}

pub struct StatusBar {
    pub total_size: u64,
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
            total_size: 0,
            system_info: Self::get_system_info(),
            error_message: None,
            error_display_time: None,
            notification_kind: NotificationKind::Error,
            notification_duration: Duration::from_secs(DEFAULT_NOTIFICATION_SECS),
        }
    }

    /// Add up the active pane's listing from its records, never the disk.
    pub fn update(&mut self, app: &App) {
        let file_info = if is_right_pane(app) {
            &app.right_pane.file_info
        } else {
            &app.file_info
        };
        self.total_size = file_info.iter().map(EntryInfo::file_size).sum();
        self.update_error_display();
    }

    fn get_system_info() -> String {
        let os = env::consts::OS;
        let arch = env::consts::ARCH;
        format!("{} ({})", os, arch)
    }

    /// The badge's name and color: the mode's, for the modes that have one.
    fn badge(app: &App, theme: &ThemeColors) -> (&'static str, Color) {
        let name = match app.input_mode {
            InputMode::Normal => "NORMAL",
            InputMode::Editing if app.search_box().1 => "GLOBAL",
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
        let color = match app.input_mode {
            InputMode::WatchKeyBinding => theme.fg,
            InputMode::CacheLoading => theme.yellow,
            InputMode::PickFolder | InputMode::Palette => theme.blue,
            InputMode::WatchInfo => theme.cyan,
            _ => app.mode_color(),
        };
        (name, color)
    }

    fn get_view_mode_indicator(view_mode: &ViewMode, theme: &ThemeColors) -> (&'static str, Color) {
        match view_mode {
            ViewMode::Normal => ("Normal", theme.fg),
            ViewMode::FullList => ("Full", theme.cyan),
            ViewMode::DualPane => ("Dual", theme.purple),
        }
    }

    /// The highlighted item's name in bright, then its details muted, in `room` columns.
    /// The name gives way first, since the list shows it too.
    fn item_line(
        app: &App,
        theme: &ThemeColors,
        today: LocalStamp,
        room: usize,
    ) -> Vec<Span<'static>> {
        let (name, details) = item_text(app, today);
        let name_room = room
            .saturating_sub(details.width() + 2)
            .max(name.width().min(MIN_NAME_WIDTH));
        let name = truncate(&name, name_room);
        vec![
            Span::styled(name, Style::default().fg(theme.bright)),
            Span::raw("  "),
            Span::styled(details, Style::default().fg(theme.light_gray)),
        ]
    }

    pub fn render(
        &self,
        frame: &mut Frame,
        area: Rect,
        app: &App,
        theme: &ThemeColors,
        today: LocalStamp,
    ) {
        let (mode_text, mode_color) = Self::badge(app, theme);
        let position = position(app);
        // Wide enough for "right pane · 12/36" inside its borders.
        let position_width = u16::try_from(position.chars().count() + 4).map_or(12, |w| w.max(12));
        // In size order: "7.1 GB here · 212 GB free".
        let size_text = app
            .active_sizing()
            .summary()
            .unwrap_or_else(|| format_file_size(self.total_size));
        let size_width = u16::try_from(size_text.chars().count() + 4).map_or(15, |w| w.max(15));
        // Split the status bar into multiple sections
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(15),             // Mode indicator
                Constraint::Length(14),             // View mode indicator
                Constraint::Min(20),                // Highlighted item
                Constraint::Length(position_width), // Position
                Constraint::Length(size_width),     // Total size
                Constraint::Length(20),             // System info
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

        // The highlighted item, or the visual selection
        let room = usize::from(chunks[2].width.saturating_sub(2));
        let item_widget = Paragraph::new(Line::from(Self::item_line(app, theme, today, room)))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_set(border::ROUNDED)
                    .title("Item"),
            )
            .style(Style::default().fg(theme.statusbar_fg));
        frame.render_widget(item_widget, chunks[2]);

        // Where the highlight is in the list
        let position_widget = Paragraph::new(position)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_set(border::ROUNDED)
                    .title("Position"),
            )
            .style(Style::default().fg(theme.cyan))
            .alignment(Alignment::Center);
        frame.render_widget(position_widget, chunks[3]);

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

    /// Draw the minimal status line: the mode's badge and the highlighted item at the
    /// left; in size order the folder's total and free space, then the highlight's
    /// position, at the right.
    pub fn render_minimal(
        &self,
        frame: &mut Frame,
        area: Rect,
        app: &App,
        theme: &ThemeColors,
        today: LocalStamp,
    ) {
        let (mode_text, mode_color) = Self::badge(app, theme);
        let muted = Style::default().fg(theme.light_gray);
        let summary = app.active_sizing().summary();
        let right: Vec<Span> = summary
            .into_iter()
            .flat_map(|summary| [Span::styled(summary, muted), Span::raw("  ")])
            .chain([Span::styled(position(app), muted), Span::raw(" ")])
            .collect();
        let right = Line::from(right);
        // A bar along the line it's drawn on; the item gives way to the position.
        let bar = Style::default().bg(theme.statusbar_bg);
        let area = Rect {
            height: area.height.min(1),
            ..area
        };
        let left_area = Rect {
            width: area
                .width
                .saturating_sub(u16::try_from(right.width() + 2).unwrap_or(u16::MAX)),
            ..area
        };
        let badge = Span::styled(
            format!(" {mode_text} "),
            Style::default()
                .fg(theme.black)
                .bg(mode_color)
                .add_modifier(Modifier::BOLD),
        );
        let room = usize::from(left_area.width).saturating_sub(badge.width() + 2);
        let left = Line::from(
            [
                vec![badge, Span::raw("  ")],
                Self::item_line(app, theme, today, room),
            ]
            .concat(),
        );
        frame.render_widget(
            Paragraph::new(right).alignment(Alignment::Right).style(bar),
            area,
        );
        frame.render_widget(Paragraph::new(left).style(bar), left_area);
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

    /// How many rows the message takes at `width`: as many as it wraps to, up to
    /// [`MESSAGE_ROWS`]; 0 with no message.
    pub fn message_rows(&self, width: u16) -> u16 {
        u16::try_from(self.message_text(width).len()).unwrap_or(u16::MAX)
    }

    /// Draw the message on `area`, in its kind's tint: its marker, then its text,
    /// wrapped under itself.
    pub fn render_message(&self, frame: &mut Frame, area: Rect, theme: &ThemeColors) {
        let (marker, color, bg) = match self.notification_kind {
            NotificationKind::Error => ("✗", theme.red, theme.error_bg),
            NotificationKind::Warning => ("!", theme.yellow, theme.warning_bg),
            NotificationKind::Info => ("✓", theme.green, theme.success_bg),
        };
        let marker_style = Style::default().fg(color).add_modifier(Modifier::BOLD);
        let text = Style::default().fg(theme.bright);
        let lines: Vec<Line> = self
            .message_text(area.width)
            .into_iter()
            .enumerate()
            .map(|(i, line)| {
                let lead = if i == 0 {
                    Span::styled(format!(" {marker} "), marker_style)
                } else {
                    Span::raw("   ")
                };
                Line::from(vec![lead, Span::styled(line, text)])
            })
            .collect();
        frame.render_widget(Paragraph::new(lines).style(Style::default().bg(bg)), area);
    }

    /// The message's lines at `width`, beside its 3-column marker. One too long for
    /// [`MESSAGE_ROWS`] ends in `…`.
    fn message_text(&self, width: u16) -> Vec<String> {
        let Some(message) = &self.error_message else {
            return Vec::new();
        };
        let room = usize::from(width).saturating_sub(4);
        let mut lines = wrap(message, room);
        if lines.len() > MESSAGE_ROWS {
            lines.truncate(MESSAGE_ROWS);
            let last = lines.pop().unwrap_or_default();
            lines.push(truncate(&format!("{last} …"), room));
        }
        lines
    }
}

/// `text` in lines at most `width` columns wide, broken between words, or inside a
/// word wider than a line.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && line.width() + 1 + word.width() <= width {
            line.push(' ');
            line.push_str(word);
            continue;
        }
        if !line.is_empty() {
            lines.push(std::mem::take(&mut line));
        }
        for c in word.chars() {
            if !line.is_empty() && line.width() + c.width().unwrap_or(0) > width {
                lines.push(std::mem::take(&mut line));
            }
            line.push(c);
        }
    }
    lines.push(line);
    lines
}

/// Whether the status bar describes the right pane: it's active in dual pane.
fn is_right_pane(app: &App) -> bool {
    app.view_mode.is_dual_pane() && app.is_right_pane_active()
}

/// What the status line says about the active pane's highlighted item, from its
/// records: its name, then its kind and item count or type and size, when it was
/// modified and its permissions; a global result says where it lives instead. In
/// visual mode, and while its prompt is open, it's the selection's size.
pub fn item_text(app: &App, today: LocalStamp) -> (String, String) {
    if let Some(visual) = &app.visual {
        return (
            format!("{} selected", visual.paths.len()),
            selection_kinds(app),
        );
    }
    let (Some(path), Some(info)) = (
        app.active_selected_path(app.curr_index),
        app.active_selected_info(app.curr_index),
    ) else {
        return (String::new(), String::new());
    };
    let item = Path::new(&path);
    let mut name = item
        .file_name()
        .map_or_else(|| path.clone(), |name| name.to_string_lossy().into_owned());
    let global = showing_global_results(app);
    let mut details = Vec::new();
    if global {
        details
            .extend(item.parent().map(|parent| {
                format!("in {}", replace_home_with_tilde(&parent.to_string_lossy()))
            }));
    }
    match info.kind {
        Some(EntryKind::Folder) => {
            if !name.ends_with('/') {
                name.push('/');
            }
            if !global {
                details.push("folder".to_string());
            }
            // The preview's listing, read in the background, once it's this folder's.
            details.extend(
                app.preview_count(&path)
                    .map(|count| plural(count as u64, "item")),
            );
            details.extend(folder_total(app, item, &info).map(total_text));
        }
        Some(EntryKind::File) => {
            if !global {
                details.push(type_label(item));
            }
            details.push(format_file_size(info.size));
        }
        Some(EntryKind::Other) => details.push("other".to_string()),
        None => {}
    }
    details.extend(
        info.modified_local
            .map(|stamp| format!("modified {}", modified_text(stamp, today))),
    );
    if info.kind.is_some() && !global {
        details.push(format_permissions(&info));
    }
    (name, details.join(" · "))
}

/// Whether the active pane lists a search from the root's results.
fn showing_global_results(app: &App) -> bool {
    if is_right_pane(app) {
        app.right_pane.is_searching() && app.right_pane.global_search_mode
    } else {
        app.global_search_mode && !app.search_results.is_empty()
    }
}

/// In size order, a folder's total, as its row shows it.
fn folder_total(app: &App, path: &Path, info: &EntryInfo) -> Option<FolderTotal> {
    app.active_sizing().folder.as_ref()?;
    app.folder_sizes.total(path).filter(|_| info.is_dir())
}

/// "6 folders, 1 file": what the visual selection holds, while both its ends are listed.
fn selection_kinds(app: &App) -> String {
    let info = if is_right_pane(app) {
        &app.right_pane.file_info
    } else {
        &app.file_info
    };
    let Some(selected) = app.visual_rows().and_then(|rows| info.get(rows)) else {
        return String::new();
    };
    let folders = selected.iter().filter(|info| info.is_dir()).count();
    [(folders, "folder"), (selected.len() - folders, "file")]
        .into_iter()
        .filter(|(count, _)| *count > 0)
        .map(|(count, noun)| plural(count as u64, noun))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Where the highlight is in the active pane's list, "12/36", naming the pane in dual
/// pane: "right pane · 3/9".
fn position(app: &App) -> String {
    let (index, total) = if is_right_pane(app) {
        (
            app.right_pane.selected_index,
            app.right_pane.get_list_length(),
        )
    } else {
        (app.curr_index, app.get_list_length())
    };
    let position = if total == 0 {
        "Empty".to_string()
    } else {
        format!("{}/{total}", index.unwrap_or(0) + 1)
    };
    if app.view_mode.is_dual_pane() {
        let pane = if app.is_right_pane_active() {
            "right"
        } else {
            "left"
        };
        format!("{pane} pane · {position}")
    } else {
        position
    }
}

/// A short name for a file's type from its extension, like "TypeScript" or "PNG
/// image", else "file".
fn type_label(path: &Path) -> String {
    let ext = path
        .extension()
        .map(|ext| ext.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let kind = match ext.as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "svg" | "ico" | "tiff" | "heic" => {
            "image"
        }
        "mp4" | "mov" | "mkv" | "avi" | "webm" => "video",
        "mp3" | "wav" | "flac" | "aac" | "m4a" | "ogg" => "audio",
        "zip" | "tar" | "gz" | "tgz" | "bz2" | "xz" | "7z" | "rar" => "archive",
        _ => "",
    };
    if !kind.is_empty() {
        return format!("{} {kind}", ext.to_uppercase());
    }
    match ext.as_str() {
        "rs" => "Rust",
        "ts" | "mts" | "cts" | "tsx" => "TypeScript",
        "js" | "mjs" | "cjs" | "jsx" => "JavaScript",
        "py" => "Python",
        "go" => "Go",
        "java" => "Java",
        "kt" | "kts" => "Kotlin",
        "swift" => "Swift",
        "rb" => "Ruby",
        "php" => "PHP",
        "c" | "h" => "C",
        "cpp" | "cc" | "cxx" | "hpp" => "C++",
        "lua" => "Lua",
        "sh" | "bash" | "zsh" | "fish" => "shell script",
        "html" | "htm" => "HTML",
        "css" | "scss" | "sass" => "CSS",
        "md" | "markdown" => "Markdown",
        "json" => "JSON",
        "yaml" | "yml" => "YAML",
        "toml" => "TOML",
        "xml" => "XML",
        "csv" => "CSV",
        "sql" => "SQL",
        "txt" => "text",
        "pdf" => "PDF document",
        _ => "file",
    }
    .to_string()
}

/// Unix-style permissions of what the item is (a link's target): "drwxr-xr-x".
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{PanePosition, SearchResult};
    use crate::pane::Pane;
    use crate::utils::files::Listing;
    use ratatui::{backend::TestBackend, buffer::Buffer, Terminal};

    /// What the fixture's dates are judged against.
    const TODAY: LocalStamp = LocalStamp {
        year: 2026,
        month: 9,
        day: 25,
        hour: 14,
        minute: 30,
    };

    fn at(year: i32, month: u8, day: u8, hour: u8, minute: u8) -> Option<LocalStamp> {
        Some(LocalStamp {
            year,
            month,
            day,
            hour,
            minute,
        })
    }

    fn folder(modified_local: Option<LocalStamp>) -> EntryInfo {
        EntryInfo {
            kind: Some(EntryKind::Folder),
            size: 288,
            modified_local,
            mode: 0o755,
            ..EntryInfo::default()
        }
    }

    /// `/w`'s 36 items: a geo-pipeline folder from this morning, a 2.2 MB data.ts from
    /// Oct 12 and a 4 KB logo.png from 2024, then 33 folders.
    fn listing() -> Listing {
        let file = |size, modified_local| EntryInfo {
            kind: Some(EntryKind::File),
            size,
            modified_local,
            mode: 0o644,
            ..EntryInfo::default()
        };
        let mut files = vec![
            "/w/geo-pipeline".to_string(),
            "/w/data.ts".into(),
            "/w/logo.png".into(),
        ];
        let mut info = vec![
            folder(at(2026, 9, 25, 9, 15)),
            file(2_306_867, at(2026, 9, 12, 8, 0)),
            file(4096, at(2024, 9, 23, 10, 0)),
        ];
        for i in 3..36 {
            files.push(format!("/w/folder-{i:02}"));
            info.push(folder(None));
        }
        Listing { files, info }
    }

    /// Show `count` items in the folder preview, loaded for `path`.
    fn preview(app: &mut App, path: &str, count: usize) {
        app.preview_loading = false;
        app.preview_loading_path = Some(path.into());
        app.preview_files = Listing::from(
            (0..count)
                .map(|i| format!("{path}/{i}"))
                .collect::<Vec<_>>(),
        );
        app.preview_file_content.clear();
    }

    /// The minimal status line, or the classic boxes, on a 120-column screen.
    fn draw(app: &App, minimal: bool) -> Buffer {
        let status_bar = StatusBar::new();
        let mut terminal = Terminal::new(TestBackend::new(120, 3)).unwrap();
        terminal
            .draw(|f| {
                if minimal {
                    status_bar.render_minimal(f, f.size(), app, &app.theme_colors, TODAY);
                } else {
                    status_bar.render(f, f.size(), app, &app.theme_colors, TODAY);
                }
            })
            .unwrap();
        terminal.backend().buffer().clone()
    }

    fn text(buffer: &Buffer) -> String {
        buffer.content().iter().map(|cell| cell.symbol()).collect()
    }

    fn item(app: &App) -> (String, String) {
        item_text(app, TODAY)
    }

    fn pair(name: &str, details: &str) -> (String, String) {
        (name.to_string(), details.to_string())
    }

    #[test]
    fn a_folder_shows_its_item_count_once_its_preview_has_loaded_it() {
        let mut app = App::with_listing(listing());
        let without_count = pair(
            "geo-pipeline/",
            "folder · modified today 09:15 · drwxr-xr-x",
        );
        assert_eq!(item(&app), without_count);

        // Still loading, then loaded.
        app.preview_loading = true;
        app.preview_loading_path = Some("/w/geo-pipeline".into());
        assert_eq!(item(&app), without_count);
        preview(&mut app, "/w/geo-pipeline", 7);
        let with_count = pair(
            "geo-pipeline/",
            "folder · 7 items · modified today 09:15 · drwxr-xr-x",
        );
        assert_eq!(item(&app), with_count);

        // Another folder's preview gives no count.
        app.curr_index = Some(3);
        assert_eq!(item(&app), pair("folder-03/", "folder · drwxr-xr-x"));

        // The line: the name bright, its details muted.
        app.curr_index = Some(0);
        let buffer = draw(&app, true);
        let line = text(&buffer);
        assert!(
            line.starts_with(" NORMAL   geo-pipeline/  folder · 7 items · modified today"),
            "{line}"
        );
        let theme = &app.theme_colors;
        assert_eq!(buffer.content()[10].fg, theme.bright);
        assert_eq!(buffer.content()[25].fg, theme.light_gray);
        assert_eq!(buffer.content()[119].bg, theme.statusbar_bg);
    }

    #[test]
    fn a_long_name_gives_way_to_its_details() {
        let mut listing = listing();
        listing.files[1] = "/w/a-screenshot-name-far-too-long-for-the-line.ts".into();
        let mut app = App::with_listing(listing);
        app.curr_index = Some(1);
        let status_bar = StatusBar::new();
        let mut terminal = Terminal::new(TestBackend::new(80, 1)).unwrap();
        terminal
            .draw(|f| status_bar.render_minimal(f, f.size(), &app, &app.theme_colors, TODAY))
            .unwrap();

        let line = text(terminal.backend().buffer());
        assert!(line.contains(" a-screenshot-na… "), "{line}");
        assert!(
            line.contains("TypeScript · 2.2 MB · modified Oct 12"),
            "{line}"
        );
    }

    #[test]
    fn a_file_shows_its_type_size_and_date() {
        let mut app = App::with_listing(listing());
        app.curr_index = Some(1);
        assert_eq!(
            item(&app),
            pair(
                "data.ts",
                "TypeScript · 2.2 MB · modified Oct 12 · -rw-r--r--"
            )
        );
        app.curr_index = Some(2);
        assert_eq!(
            item(&app),
            pair(
                "logo.png",
                "PNG image · 4.0 KB · modified Oct 23 2024 · -rw-r--r--"
            )
        );
        assert_eq!(type_label(Path::new("/w/notes")), "file");
    }

    #[test]
    fn a_global_result_says_where_it_lives() {
        let home = dirs::home_dir().unwrap();
        let path = home.join("Desktop/work/projects/geo-pipeline");
        let path = path.to_string_lossy().into_owned();
        let mut app = App::with_listing(listing());
        app.global_search_mode = true;
        app.search_results = vec![SearchResult {
            file_path: path.clone(),
            display_name: "geo-pipeline".into(),
            score: 1,
            info: folder(at(2026, 9, 25, 9, 15)),
            original_index: 0,
        }];
        app.curr_index = Some(0);
        preview(&mut app, &path, 7);
        assert_eq!(
            item(&app),
            pair(
                "geo-pipeline/",
                "in ~/Desktop/work/projects · 7 items · modified today 09:15"
            )
        );
    }

    #[test]
    fn visual_mode_counts_the_selection_by_kind_in_both_styles() {
        let mut app = App::with_listing(listing());
        app.start_visual(Some(0)).unwrap();
        app.move_visual_cursor(Some(6));
        assert_eq!(item(&app), pair("7 selected", "5 folders, 2 files"));
        for minimal in [false, true] {
            let screen = text(&draw(&app, minimal));
            assert!(screen.contains("VISUAL"), "{screen}");
            assert!(
                screen.contains("7 selected  5 folders, 2 files"),
                "{screen}"
            );
        }

        // The same while its Trash prompt is open.
        app.input_mode = InputMode::WatchTrash;
        let line = text(&draw(&app, true));
        assert!(
            line.starts_with(" TRASH   7 selected  5 folders, 2 files "),
            "{line}"
        );

        app.end_visual();
        app.input_mode = InputMode::Normal;
        assert_eq!(item(&app).0, "geo-pipeline/");
    }

    #[test]
    fn the_position_counts_rows_and_names_the_pane_in_dual_pane() {
        let mut app = App::with_listing(listing());
        for (index, shown) in [(0, "1/36 "), (11, "12/36 "), (35, "36/36 ")] {
            app.curr_index = Some(index);
            let line = text(&draw(&app, true));
            assert!(line.contains(shown) && !line.contains("Top") && !line.contains("Bot"));
            assert_eq!(position(&app), shown.trim());
        }

        app.curr_index = Some(11);
        app.view_mode = ViewMode::DualPane;
        let files = (0..9).map(|i| format!("/v/f{i}")).collect();
        app.right_pane = Pane::with_listing(
            "/v".into(),
            Listing {
                files,
                info: vec![folder(None); 9],
            },
        );
        app.right_pane.selected_index = Some(2);
        assert_eq!(position(&app), "left pane · 12/36");
        app.active_pane = PanePosition::Right;
        assert_eq!(position(&app), "right pane · 3/9");
        assert_eq!(item(&app), pair("f2/", "folder · drwxr-xr-x"));
        let buffer = draw(&app, true);
        assert!(text(&buffer).contains("right pane · 3/9 "));
        // The classic style's box says the same.
        let screen = text(&draw(&app, false));
        assert!(screen.contains("right pane · 3/9"), "{screen}");
        assert!(screen.contains("f2/  folder · drwxr-xr-x"), "{screen}");
    }

    #[test]
    fn in_size_order_the_line_shows_the_folders_total_and_free_space() {
        let mut app = App::with_listing(listing());
        app.sizing.folder = Some("/w".into());
        app.sizing.free = Some(212 << 30);
        let line = text(&draw(&app, true));
        assert!(line.contains("212.0 GB free  1/36 "), "{line}");
    }

    #[test]
    fn both_styles_show_the_badge_black_on_the_mode_color() {
        let mut app = App::new(vec!["/w/a".into()]);
        let theme = app.theme_colors.clone();
        for (mode, global, label, color) in [
            (InputMode::Normal, false, "NORMAL", theme.mode_normal),
            (InputMode::Editing, false, "SEARCH", theme.mode_search),
            (InputMode::Editing, true, "GLOBAL", theme.mode_global),
            (InputMode::WatchTrash, false, "TRASH", theme.mode_delete),
            (InputMode::WatchDelete, false, "DELETE", theme.mode_delete),
            (InputMode::WatchRename, false, "RENAME", theme.mode_rename),
            (InputMode::WatchSort, false, "SORT", theme.mode_sort),
        ] {
            app.input_mode = mode;
            app.global_search_mode = global;
            for minimal in [false, true] {
                let buffer = draw(&app, minimal);
                // The badge's first letter, inside the classic style's border.
                let row = usize::from(!minimal);
                let line: String = buffer.content()[row * 120..(row + 1) * 120]
                    .iter()
                    .map(|cell| cell.symbol())
                    .collect();
                assert!(line.contains(&format!(" {label} ")), "{line}");
                let cell = buffer.content()[row * 120..]
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
        assert_eq!(status_bar.total_size, 1);
        let (name, details) = item(&app);
        assert_eq!(name, "left.txt");
        assert!(details.starts_with("text · 1 B · modified "), "{details}");
        assert_eq!(position(&app), "left pane · 1/1");

        app.active_pane = PanePosition::Right;
        status_bar.update(&app);
        assert_eq!(status_bar.total_size, 12);
        let (name, details) = item(&app);
        assert_eq!(name, "b.txt");
        assert!(details.starts_with("text · 7 B · modified "), "{details}");
        assert_eq!(position(&app), "right pane · 2/3");
    }

    #[test]
    fn the_item_comes_from_the_records_until_the_listing_is_read_again() {
        use crate::utils::{read_listing, SortBy, SortType};

        let dir = tempfile::tempdir().unwrap();
        let notes = dir.path().join("notes.txt");
        std::fs::write(&notes, "notes").unwrap();
        let start = dir.path().to_string_lossy().into_owned();
        let listing = || read_listing(start.clone(), false, SortBy::Name, &SortType::ASC).unwrap();
        let mut app = App::with_listing(listing());
        app.right_pane = crate::pane::Pane::with_listing(start.clone(), listing());
        let mut status_bar = StatusBar::new();
        let minimal = |app: &App| text(&draw(app, true));
        let details = |app: &App| item(app).1;

        for right in [false, true] {
            if right {
                app.view_mode = ViewMode::DualPane;
                app.active_pane = PanePosition::Right;
            }
            std::fs::write(&notes, "notes").unwrap();
            app.refresh_files_from_watcher(listing(), &mut ratatui::widgets::ListState::default());
            app.right_pane.refresh_files();
            status_bar.update(&app);
            assert!(details(&app).starts_with("text · 5 B · "));
            assert!(minimal(&app).contains("notes.txt  text · 5 B · "));

            // Grown, then gone: still what the records say.
            std::fs::write(&notes, "x".repeat(2048)).unwrap();
            status_bar.update(&app);
            assert!(details(&app).starts_with("text · 5 B · "));
            assert_eq!(status_bar.total_size, 5);
            std::fs::remove_file(&notes).unwrap();
            status_bar.update(&app);
            assert!(details(&app).starts_with("text · 5 B · "));
            assert!(
                minimal(&app).contains("notes.txt  text · 5 B · "),
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
            assert!(details(&app).starts_with("text · 2.0 KB · "));
            assert_eq!(status_bar.total_size, 2048);
            assert!(minimal(&app).contains("notes.txt  text · 2.0 KB · "));
        }
    }

    #[test]
    fn an_empty_folder_has_no_item_to_describe() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = App::new(Vec::new());
        app.current_directory = dir.path().to_string_lossy().into_owned();

        assert_eq!(item(&app), pair("", ""));
        assert_eq!(position(&app), "Empty");
    }
}
