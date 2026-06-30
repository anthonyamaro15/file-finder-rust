//! Popup rendering utilities and widget builders.

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    symbols::border,
    text::{Line, Text},
    widgets::{Block, Borders, Paragraph},
};

use crate::config::{Action, KeymapSettings};
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

/// Calculate a centered input popup area with a minimum size.
pub fn draw_input_popup(rect: Rect) -> Rect {
    draw_fixed_height_popup(rect, 40, 30, 3)
}

fn draw_fixed_height_popup(rect: Rect, percent_x: u16, min_width: u16, height: u16) -> Rect {
    let width = rect
        .width
        .saturating_mul(percent_x)
        .checked_div(100)
        .unwrap_or(0)
        .max(min_width)
        .min(rect.width);
    let height = height.min(rect.height);

    Rect {
        x: rect.x + rect.width.saturating_sub(width) / 2,
        y: rect.y + rect.height.saturating_sub(height) / 2,
        width,
        height,
    }
}

/// Calculate the terminal cursor position for a one-line bordered input popup.
pub fn input_popup_cursor_position(area: Rect, char_index: usize) -> (u16, u16) {
    let inner_width = area.width.saturating_sub(2);
    let cursor_offset = (char_index as u16).min(inner_width.saturating_sub(1));

    (area.x + 1 + cursor_offset, area.y + 1)
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
    _file_count: Option<usize>,
    total_size: Option<u64>,
) -> Paragraph<'a> {
    let info_line = if is_dir {
        // For directories, stats are calculated async, so show placeholder
        // Note: We don't block on stat calculation anymore to keep UI responsive
        "(directory)".to_string()
    } else {
        // For files, we have the size
        let size_str = total_size
            .map(|s| format_file_size(s))
            .unwrap_or_else(|| "unknown".to_string());
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
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_set(border::ROUNDED)
                .title(title),
        )
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
pub fn create_keybindings_popup<'a>(keymap: &KeymapSettings) -> Paragraph<'a> {
    let lines = keybinding_help_lines(keymap);

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

fn keybinding_help_lines(keymap: &KeymapSettings) -> Vec<Line<'static>> {
    keybinding_help_text_lines(keymap)
        .into_iter()
        .map(|line| {
            if line.starts_with("──") {
                Line::styled(line, OneDarkTheme::info())
            } else {
                Line::from(line)
            }
        })
        .collect()
}

fn keybinding_help_text_lines(keymap: &KeymapSettings) -> Vec<String> {
    vec![
        "".to_string(),
        "── Navigation ──".to_string(),
        "  j / ↓           Move down".to_string(),
        "  k / ↑           Move up".to_string(),
        "  h               Go to parent directory".to_string(),
        "  l               Enter directory".to_string(),
        "  Enter           Open with IDE / copy path".to_string(),
        "".to_string(),
        "── File Operations ──".to_string(),
        "  a               Create new file/directory".to_string(),
        "  d               Delete (with confirmation)".to_string(),
        "  r               Rename file/directory".to_string(),
        "  c               Copy file/directory".to_string(),
        "  y               Extract ZIP archive".to_string(),
        "  o               Open with system default".to_string(),
        "".to_string(),
        "── Search ──".to_string(),
        keybinding_help_row(
            &configured_action_key_labels(keymap, Action::SearchCurrent),
            "Search current directory",
        ),
        keybinding_help_row(
            &configured_action_key_labels(keymap, Action::SearchRoot),
            "Search from root cache",
        ),
        "  Esc             Exit search mode".to_string(),
        "  ↑/↓             Search history".to_string(),
        "".to_string(),
        "── View & Sort ──".to_string(),
        "  p               Cycle view mode (Normal/Full/Dual)".to_string(),
        "  s               Sort options menu".to_string(),
        "  .               Toggle hidden files".to_string(),
        "  ?               Show this help".to_string(),
        "".to_string(),
        "── Dual Pane Mode ──".to_string(),
        "  Tab             Switch active pane".to_string(),
        "  j/k/h/l         Navigate in active pane".to_string(),
        "  Shift+C         Copy to other pane".to_string(),
        "  Shift+M         Move to other pane".to_string(),
        "".to_string(),
        "── General ──".to_string(),
        "  q               Quit / Close popup".to_string(),
    ]
}

fn configured_action_key_labels(keymap: &KeymapSettings, action: Action) -> Vec<String> {
    let mut key_labels: Vec<(bool, String)> = keymap
        .normal
        .iter()
        .filter_map(|(key, configured_action)| {
            if Action::parse(configured_action) != Some(action.clone()) {
                return None;
            }

            Some((
                key.starts_with("<leader>"),
                format_key_sequence(key, keymap),
            ))
        })
        .collect();

    key_labels.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    key_labels.into_iter().map(|(_, label)| label).collect()
}

fn format_key_sequence(key: &str, keymap: &KeymapSettings) -> String {
    if let Some(rest) = key.strip_prefix("<leader>") {
        format!("{} + {}", format_key(&keymap.leader), format_key(rest))
    } else {
        format_key(key)
    }
}

fn format_key(key: &str) -> String {
    match key {
        " " => "Space".to_string(),
        "" => "Unbound".to_string(),
        value => value.to_string(),
    }
}

fn keybinding_help_row(keys: &[String], description: &str) -> String {
    let key_text = if keys.is_empty() {
        "Unbound".to_string()
    } else {
        keys.join(" or ")
    };

    format!("  {:<15} {}", key_text, description)
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
    use crate::config::KeymapSettings;

    use super::*;

    #[test]
    fn keybinding_help_text_uses_default_configured_search_keys() {
        let lines = keybinding_help_text_lines(&KeymapSettings::default());

        assert!(lines.contains(&"  / or i          Search current directory".to_string()));
        assert!(lines.contains(&"  Space + /       Search from root cache".to_string()));
        assert!(lines.contains(&"  j / ↓           Move down".to_string()));
    }

    #[test]
    fn keybinding_help_text_reflects_custom_search_keys() {
        let mut settings = KeymapSettings::default();
        settings.leader = ",".to_string();
        settings
            .normal
            .insert("f".to_string(), "search.current".to_string());
        settings
            .normal
            .insert("/".to_string(), "search.root".to_string());

        let lines = keybinding_help_text_lines(&settings);

        assert!(lines.contains(&"  f or i          Search current directory".to_string()));
        assert!(lines.contains(&"  / or , + /      Search from root cache".to_string()));
        assert!(!lines.contains(&"  / or i          Search current directory".to_string()));
        assert!(lines.contains(&"  q               Quit / Close popup".to_string()));
    }

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
    fn test_draw_input_popup_is_compact_bordered_input() {
        let outer = Rect::new(0, 0, 160, 24);
        let popup = draw_input_popup(outer);

        assert_eq!(popup.height, 3);
    }

    #[test]
    fn test_input_popup_cursor_position_uses_inner_input_row() {
        let popup = Rect::new(20, 10, 40, 3);

        assert_eq!(input_popup_cursor_position(popup, 9), (30, 11));
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
