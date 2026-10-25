//! Popup rendering utilities and widget builders.

use std::path::Path;

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    symbols::border,
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Padding, Paragraph, Wrap},
    Frame,
};

use crate::app::FolderPicker;
use crate::config::{Action, KeymapSettings};
use crate::theme::OneDarkTheme;
use crate::utils::format::replace_home_with_tilde;
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

/// Keep the confirmation and warning visible on a typical 24-row terminal.
pub fn draw_delete_popup(rect: Rect) -> Rect {
    draw_fixed_height_popup(rect, 60, 50, 9)
}

/// Create the Trash or permanent-delete confirmation popup with file info.
pub fn create_delete_confirmation_popup<'a>(
    name: &str,
    is_dir: bool,
    _file_count: Option<usize>,
    total_size: Option<u64>,
    to_trash: bool,
) -> Paragraph<'a> {
    let info_line = if is_dir {
        // For directories, stats are calculated async, so show placeholder
        // Note: We don't block on stat calculation anymore to keep UI responsive
        "(directory)".to_string()
    } else {
        // For files, we have the size
        let size_str = total_size
            .map(format_file_size)
            .unwrap_or_else(|| "unknown".to_string());
        format!("({})", size_str)
    };

    let content = vec![
        Line::from(""),
        Line::from(if to_trash {
            format!("Move '{name}' to Trash?")
        } else {
            format!("Permanently delete '{name}'?")
        }),
        Line::from(info_line),
        Line::from(if to_trash {
            ""
        } else {
            "This can't be undone."
        }),
        Line::from(""),
        Line::from("Press 'y' to confirm, 'n' to cancel"),
    ];

    Paragraph::new(content)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_set(border::ROUNDED)
                .title(if to_trash {
                    "Move to Trash"
                } else {
                    "Permanent Delete"
                })
                .style(OneDarkTheme::error()),
        )
        .wrap(Wrap { trim: true })
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

/// Draw the "Copy to…"/"Move to…" picker over the list: the query (with the cursor at
/// `cursor`) above the matching folders, each with its full path.
pub fn render_folder_picker(f: &mut Frame, picker: &FolderPicker, query: &str, cursor: usize) {
    let area = draw_popup(f.size(), 70, 60);
    let chunks = Layout::vertical([Constraint::Length(3), Constraint::Min(1)]).split(area);
    let name = Path::new(&picker.source)
        .file_name()
        .map_or(picker.source.clone(), |name| {
            name.to_string_lossy().into_owned()
        });

    // Scroll a long typed path so the cursor stays inside the box.
    let scroll = cursor.saturating_sub(chunks[0].width.saturating_sub(3) as usize);
    let input = Paragraph::new(query).scroll((0, scroll as u16)).block(
        Block::default()
            .borders(Borders::ALL)
            .border_set(border::ROUNDED)
            .title(format!("{} '{name}' to…", picker.kind.title())),
    );

    let hint = if query.trim().is_empty() {
        "Type a folder name, or a path starting with / or ~"
    } else if picker.results.is_empty() {
        "No matching folders"
    } else {
        "↑/↓ choose · Enter confirm · Esc cancel"
    };
    let items: Vec<ListItem> = picker
        .results
        .iter()
        .map(|result| {
            ListItem::new(Line::from(vec![
                Span::raw(result.display_name.clone()),
                Span::styled(
                    format!("  {}", replace_home_with_tilde(&result.file_path)),
                    OneDarkTheme::dotfile(),
                ),
            ]))
        })
        .collect();
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_set(border::ROUNDED)
                .title(hint),
        )
        .highlight_style(OneDarkTheme::selected())
        .highlight_symbol("❯ ");
    let mut list_state = ListState::default();
    list_state.select(picker.selected);

    f.render_widget(Clear, area);
    f.render_widget(input.style(OneDarkTheme::info()), chunks[0]);
    f.render_stateful_widget(
        list.style(OneDarkTheme::normal()),
        chunks[1],
        &mut list_state,
    );
    let (x, y) = input_popup_cursor_position(chunks[0], cursor - scroll);
    f.set_cursor(x, y);
}

/// Draw Get Info centred and sized to its rows: labels in a column, and each value
/// wrapped beside its label, so long names and paths are never cut off.
pub fn render_info_popup(f: &mut Frame, rows: &[(&str, String)]) {
    // "Permissions: ", then the value; borders and padding take 4 columns.
    let label_width = rows
        .iter()
        .map(|(label, _)| label.len() + 2)
        .max()
        .unwrap_or(0);
    let widest_value = rows
        .iter()
        .map(|(_, value)| Span::raw(value.as_str()).width())
        .max()
        .unwrap_or(0);
    let screen = f.size();
    let max_value_width = (usize::from(screen.width) * 4 / 5).saturating_sub(label_width + 4);
    let value_width = widest_value.min(max_value_width).max(10);

    let mut lines = Vec::new();
    for (label, value) in rows {
        for (i, part) in wrap_to_width(value, value_width).into_iter().enumerate() {
            let label = if i == 0 {
                format!("{label}:")
            } else {
                String::new()
            };
            lines.push(Line::from(vec![
                Span::styled(format!("{label:<label_width$}"), OneDarkTheme::info()),
                Span::raw(part),
            ]));
        }
    }
    let width = u16::try_from(label_width + value_width + 4).unwrap_or(u16::MAX);
    let height = u16::try_from(lines.len() + 2).unwrap_or(u16::MAX);
    let area = draw_fixed_height_popup(screen, 0, width, height);
    let popup = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_set(border::ROUNDED)
                .padding(Padding::horizontal(1))
                .title("Get Info (Esc to close)"),
        )
        .style(OneDarkTheme::normal());
    f.render_widget(Clear, area);
    f.render_widget(popup, area);
}

/// Split `text` into lines at most `width` terminal columns wide.
fn wrap_to_width(text: &str, width: usize) -> Vec<String> {
    let mut lines = vec![String::new()];
    let mut used = 0;
    for ch in text.chars() {
        let ch_width = Span::raw(ch.to_string()).width();
        if used > 0 && used + ch_width > width {
            lines.push(String::new());
            used = 0;
        }
        lines.last_mut().expect("starts with one line").push(ch);
        used += ch_width;
    }
    lines
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
        "  h / ←           Go to parent directory".to_string(),
        "  l / →           Enter directory".to_string(),
        "  Enter           Open with IDE / copy path".to_string(),
        "".to_string(),
        "── File Operations ──".to_string(),
        "  a               Create new file/directory".to_string(),
        "  d               Move to Trash (with confirmation)".to_string(),
        "  D (Shift+D)     Permanently delete (with confirmation)".to_string(),
        "  r               Rename file/directory".to_string(),
        "  c               Copy file/directory".to_string(),
        "  Shift+C         Copy to… (pick a folder)".to_string(),
        "  Shift+M         Move to… (pick a folder)".to_string(),
        "  y               Extract ZIP archive".to_string(),
        "  z               Compress to ZIP archive".to_string(),
        "  o               Open with system default".to_string(),
        "  I (Shift+I)     Get Info (size, dates, permissions)".to_string(),
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
        "  q / Ctrl-C      Quit / Close popup".to_string(),
    ]
}

pub fn configured_action_key_labels(keymap: &KeymapSettings, action: Action) -> Vec<String> {
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
        assert!(lines.contains(&"  Shift+C         Copy to… (pick a folder)".to_string()));
        assert!(lines.contains(&"  Shift+C         Copy to other pane".to_string()));
    }

    #[test]
    fn keybinding_help_text_reflects_custom_search_keys() {
        let mut settings = KeymapSettings {
            leader: ",".to_string(),
            ..KeymapSettings::default()
        };
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
        assert!(lines.contains(&"  q / Ctrl-C      Quit / Close popup".to_string()));
    }

    #[test]
    fn folder_picker_shows_its_title_rows_and_the_end_of_a_long_query() {
        use crate::app::{SearchResult, TransferKind};
        use ratatui::{backend::TestBackend, Terminal};

        let picker = FolderPicker {
            kind: TransferKind::Move,
            source: "/w/notes.txt".into(),
            results: vec![SearchResult {
                file_path: "/w/Projects".into(),
                display_name: "Projects".into(),
                score: 1,
                is_directory: true,
                original_index: 0,
            }],
            selected: Some(0),
        };
        let query = format!("/{}END", "x".repeat(200));
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal
            .draw(|f| render_folder_picker(f, &picker, &query, query.chars().count()))
            .unwrap();

        let screen: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(screen.contains("Move 'notes.txt' to…"));
        assert!(screen.contains("Projects  /w/Projects"));
        assert!(screen.contains("xEND"));
    }

    #[test]
    fn info_popup_wraps_long_values_instead_of_cutting_them_off() {
        use ratatui::{backend::TestBackend, Terminal};
        let name = format!("start-{}-end", "x".repeat(150));
        let rows = [("Name", name), ("Kind", "File".to_string())];
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal.draw(|f| render_info_popup(f, &rows)).unwrap();

        let screen: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(screen.contains("Get Info"));
        assert!(screen.contains("Name: start-x"));
        assert!(screen.contains("x-end"));
        assert_eq!(screen.matches('x').count(), 150);
        assert!(screen.contains("Kind: File"));
    }

    #[test]
    fn permanent_delete_popup_warns_that_it_cannot_be_undone() {
        use ratatui::{backend::TestBackend, Terminal};
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal
            .draw(|frame| {
                frame.render_widget(
                    create_delete_confirmation_popup("notes.txt", false, None, Some(5), false),
                    draw_delete_popup(frame.size()),
                );
            })
            .unwrap();
        let screen: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(screen.contains("Permanently delete 'notes.txt'?"));
        assert!(screen.contains("This can't be undone."));
        assert!(screen.contains("'y' to confirm, 'n' to cancel"));
    }

    #[test]
    fn trash_popup_offers_trash_without_the_permanent_delete_warning() {
        use ratatui::{backend::TestBackend, Terminal};
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal
            .draw(|frame| {
                frame.render_widget(
                    create_delete_confirmation_popup("notes", true, None, None, true),
                    draw_delete_popup(frame.size()),
                );
            })
            .unwrap();
        let screen: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(screen.contains("Move 'notes' to Trash?"));
        assert!(screen.contains("(directory)"));
        assert!(screen.contains("'y' to confirm, 'n' to cancel"));
        assert!(!screen.contains("can't be undone"));
        assert!(!screen.contains("Permanently delete"));
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
