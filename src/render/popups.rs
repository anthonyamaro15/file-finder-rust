//! Popup rendering utilities and widget builders.

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    symbols::border,
    text::{Line, Text},
    widgets::{Block, Borders, Paragraph},
};

use crate::theme::OneDarkTheme;
use crate::utils::{format_file_size, SortBy, SortType};

/// Calculate a centered popup area within the given rect.
///
/// # Arguments
/// * `rect` - The outer rectangle to center within
/// * `percent_x` - Width of popup as percentage of outer rect
/// * `percent_y` - Height of popup as percentage of outer rect
pub fn draw_popup(rect: Rect, percent_x: u16, percent_y: u16) -> Rect {
    let popup_layout = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .split(rect);

    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(popup_layout[1])[1]
}

/// Generate the sort indicator display string showing field and direction.
pub fn generate_sort_indicator(sort_by: &SortBy, sort_type: &SortType) -> String {
    let field = match sort_by {
        SortBy::Name => "Name",
        SortBy::Size => "Size",
        SortBy::DateAdded => "Date",
        SortBy::Default => "Default",
    };
    let direction = match sort_type {
        SortType::ASC => "ASC",
        SortType::DESC => "DESC",
    };
    format!("Current: {} {}", field, direction)
}

/// Create the delete confirmation popup with file info.
pub fn create_delete_confirmation_popup<'a>(
    name: &str,
    is_dir: bool,
    file_count: Option<usize>,
    total_size: Option<u64>,
) -> Paragraph<'a> {
    let size_str = total_size
        .map(|s| format_file_size(s))
        .unwrap_or_else(|| "unknown".to_string());

    let info_line = if is_dir {
        let count = file_count.unwrap_or(0);
        let files_word = if count == 1 { "file" } else { "files" };
        format!("({} {}, {})", count, files_word, size_str)
    } else {
        format!("({})", size_str)
    };

    let content = vec![
        Line::from(""),
        Line::from(format!("Delete \"{}\"?", name)),
        Line::from(info_line),
        Line::from(""),
        Line::from("Press 'y' to confirm, 'n' to cancel"),
    ];

    Paragraph::new(content)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_set(border::ROUNDED)
                .title("Confirm Delete")
                .style(OneDarkTheme::error()),
        )
        .centered()
}

/// Create the create file/dir input popup widget.
/// Shows "Create File" if input contains a dot (has extension),
/// or "Create Directory" if no dot is present.
pub fn create_create_input_popup<'a>(
    input_text: &'a str,
    is_error: bool,
    error_message: &str,
) -> Paragraph<'a> {
    let title = if is_error {
        format!("Error: {}", error_message)
    } else if input_text.is_empty() {
        "Create File/Dir".to_string()
    } else if input_text.contains('.') {
        "Create File".to_string()
    } else {
        "Create Directory".to_string()
    };

    let style = if is_error {
        OneDarkTheme::error()
    } else {
        OneDarkTheme::success()
    };

    Paragraph::new(input_text)
        .block(Block::default().borders(Borders::ALL).border_set(border::ROUNDED).title(title))
        .style(style)
}

/// Create the rename input popup widget.
pub fn create_rename_input_popup<'a>(input_text: &'a str) -> Paragraph<'a> {
    Paragraph::new(input_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_set(border::ROUNDED)
                .title("Rename"),
        )
        .style(OneDarkTheme::warning())
}

/// Create the sort options popup widget.
pub fn create_sort_options_popup<'a>(sort_by: &SortBy, sort_type: &SortType) -> Paragraph<'a> {
    let current_indicator = generate_sort_indicator(sort_by, sort_type);

    let lines = vec![
        Line::from(current_indicator),
        Line::from(""),
        Line::from("Sort by:  (n) Name  (s) Size  (t) Date"),
        Line::from("Order:    (a) ASC   (d) DESC"),
        Line::from("          (q) Close"),
    ];

    let list_items = Text::from(lines);

    Paragraph::new(list_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_set(border::ROUNDED)
                .title("Sort Options"),
        )
        .style(OneDarkTheme::info())
}

/// Create the keybindings help popup widget.
pub fn create_keybindings_popup<'a>() -> Paragraph<'a> {
    let lines = vec![
        Line::from(""),
        Line::styled("── Navigation ──", OneDarkTheme::info()),
        Line::from("  j / ↓           Move down"),
        Line::from("  k / ↑           Move up"),
        Line::from("  h               Go to parent directory"),
        Line::from("  l               Enter directory"),
        Line::from("  Enter           Open with IDE / copy path"),
        Line::from(""),
        Line::styled("── File Operations ──", OneDarkTheme::info()),
        Line::from("  a               Create new file/directory"),
        Line::from("  d               Delete (with confirmation)"),
        Line::from("  r               Rename file/directory"),
        Line::from("  c               Copy file/directory"),
        Line::from("  y               Extract ZIP archive"),
        Line::from("  o               Open with system default"),
        Line::from(""),
        Line::styled("── Search ──", OneDarkTheme::info()),
        Line::from("  i               Enter search mode"),
        Line::from("  Space or /      Global search (at start)"),
        Line::from("  Esc             Exit search mode"),
        Line::from("  ↑/↓             Search history"),
        Line::from(""),
        Line::styled("── View & Sort ──", OneDarkTheme::info()),
        Line::from("  p               Cycle view mode (Normal/Full/Dual)"),
        Line::from("  s               Sort options menu"),
        Line::from("  .               Toggle hidden files"),
        Line::from("  ?               Show this help"),
        Line::from(""),
        Line::styled("── Dual Pane Mode ──", OneDarkTheme::info()),
        Line::from("  Tab             Switch active pane"),
        Line::from("  j/k/h/l         Navigate in active pane"),
        Line::from("  Shift+C         Copy to other pane"),
        Line::from("  Shift+M         Move to other pane"),
        Line::from(""),
        Line::styled("── General ──", OneDarkTheme::info()),
        Line::from("  q               Quit / Close popup"),
    ];

    let list_items = Text::from(lines);

    Paragraph::new(list_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_set(border::ROUNDED)
                .title("Keybindings (q to close)"),
        )
        .style(OneDarkTheme::normal())
}

/// Split an area for popup content with margins.
pub fn split_popup_area(area: Rect) -> Vec<Rect> {
    Layout::default()
        .direction(Direction::Horizontal)
        .margin(1)
        .constraints([Constraint::Percentage(100)])
        .split(area)
        .to_vec()
}

/// Split an area for vertical popup content with margins.
pub fn split_popup_area_vertical(area: Rect) -> Vec<Rect> {
    Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([Constraint::Percentage(100)])
        .split(area)
        .to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_draw_popup_centered() {
        let outer = Rect::new(0, 0, 100, 50);
        let popup = draw_popup(outer, 50, 50);

        // Popup should be roughly centered
        assert!(popup.x > 0);
        assert!(popup.y > 0);
        assert!(popup.width < outer.width);
        assert!(popup.height < outer.height);
    }

    #[test]
    fn test_generate_sort_indicator_name_asc() {
        let result = generate_sort_indicator(&SortBy::Name, &SortType::ASC);
        assert_eq!(result, "Current: Name ASC");
    }

    #[test]
    fn test_generate_sort_indicator_size_desc() {
        let result = generate_sort_indicator(&SortBy::Size, &SortType::DESC);
        assert_eq!(result, "Current: Size DESC");
    }
}
