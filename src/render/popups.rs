//! Popup rendering utilities and widget builders.

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Text},
    widgets::{Block, Borders, Paragraph},
};

use crate::theme::OneDarkTheme;
use crate::utils::SortType;

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

/// Generate the sort type display string.
pub fn generate_sort_by_string(sort_type: &SortType) -> String {
    let str_sort_type = match sort_type {
        SortType::ASC => "ASC",
        SortType::DESC => "DESC",
    };
    format!("Sort By: '{}'", str_sort_type)
}

/// Create the delete confirmation popup block.
pub fn create_delete_confirmation_block<'a>() -> Block<'a> {
    Block::bordered()
        .title("⚠️  Confirm to delete y/n")
        .style(OneDarkTheme::error())
}

/// Create the create file/dir input popup widget.
pub fn create_create_input_popup<'a>(
    input_text: &'a str,
    is_error: bool,
    error_message: &str,
) -> Paragraph<'a> {
    let title = if is_error {
        format!("❌ {}", error_message)
    } else {
        "✨ Create File/Dir".to_string()
    };

    let style = if is_error {
        OneDarkTheme::error()
    } else {
        OneDarkTheme::success()
    };

    Paragraph::new(input_text)
        .block(Block::default().borders(Borders::ALL).title(title))
        .style(style)
}

/// Create the rename input popup widget.
pub fn create_rename_input_popup<'a>(input_text: &'a str) -> Paragraph<'a> {
    Paragraph::new(input_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("✏️  Enter file/dir name"),
        )
        .style(OneDarkTheme::warning())
}

/// Create the sort options popup widget.
pub fn create_sort_options_popup<'a>(sort_type: &SortType) -> Paragraph<'a> {
    let lines = vec![
        Line::from("Press (a) to sort ASC or (d) to sort DESC, (q) to exit"),
        Line::from("Name: (n)"),
        Line::from("Date Created: (t)"),
        Line::from("Size: (s)"),
    ];

    let sort_by_text = generate_sort_by_string(sort_type);
    let list_items = Text::from(lines);

    Paragraph::new(list_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!("🔄 {}", sort_by_text)),
        )
        .style(OneDarkTheme::info())
}

/// Create the keybindings help popup widget.
pub fn create_keybindings_popup<'a>() -> Paragraph<'a> {
    let lines = vec![
        Line::from("<Enter>: Open directory with selected IDE. copy path if not IDE option provided."),
        Line::from("<s>: Sort"),
        Line::from("<a>: Create new"),
        Line::from("<d>: Delete"),
        Line::from("<i>: Search mode"),
        Line::from("<c>: Copy dir/file"),
        Line::from("<.>: Show hidden files"),
        Line::from(""),
        Line::from("-- Search Features --"),
        Line::from("Type in search mode to use fuzzy search with scoring and ranking"),
        Line::from("Start search with space or / to search across entire directory tree"),
        Line::from("Results sorted by relevance with highlighting of matched text"),
        Line::from("Search history is available using up/down arrow keys"),
    ];

    let list_items = Text::from(lines);

    Paragraph::new(list_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("⌨️  Keybindings"),
        )
        .style(OneDarkTheme::info())
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
    fn test_generate_sort_by_string_asc() {
        let result = generate_sort_by_string(&SortType::ASC);
        assert_eq!(result, "Sort By: 'ASC'");
    }

    #[test]
    fn test_generate_sort_by_string_desc() {
        let result = generate_sort_by_string(&SortType::DESC);
        assert_eq!(result, "Sort By: 'DESC'");
    }
}
