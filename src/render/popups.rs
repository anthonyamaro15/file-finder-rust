//! Popup rendering utilities and widget builders.

use std::path::Path;

use ratatui::{
    layout::{Constraint, Direction, Layout, Margin, Rect},
    symbols::border,
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Padding, Paragraph, Wrap},
    Frame,
};

use crate::app::FolderPicker;
use crate::config::{
    keymap::{key_label, Bindings},
    Action, KeySequence, KeymapSettings,
};
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

/// The help's sections: each action and what it does. Copy to/Move to appear twice
/// because Dual Pane mode sends the item to the other pane instead.
const HELP_SECTIONS: [(&str, &[(Action, &str)]); 6] = [
    (
        "Navigation",
        &[
            (Action::NavDown, "Move down"),
            (Action::NavUp, "Move up"),
            (Action::NavParent, "Go to parent directory"),
            (Action::NavEnter, "Enter directory"),
            (Action::NavChoose, "Open with IDE / copy path"),
        ],
    ),
    (
        "File Operations",
        &[
            (Action::FileCreate, "Create new file/directory"),
            (Action::FileTrash, "Move to Trash (with confirmation)"),
            (Action::FileDelete, "Permanently delete (with confirmation)"),
            (Action::FileRename, "Rename file/directory"),
            (Action::FileDuplicate, "Copy file/directory beside it"),
            (Action::FileCopyTo, "Copy to… (pick a folder)"),
            (Action::FileMoveTo, "Move to… (pick a folder)"),
            (Action::FileExtract, "Extract ZIP archive"),
            (Action::FileCompress, "Compress to ZIP archive"),
            (Action::FileOpen, "Open with system default"),
            (Action::FileInfo, "Get Info (size, dates, permissions)"),
        ],
    ),
    (
        "Search",
        &[
            (Action::SearchCurrent, "Search current directory"),
            (Action::SearchRoot, "Search from root cache"),
        ],
    ),
    (
        "View & Sort",
        &[
            (Action::ViewCycle, "Cycle view mode (Normal/Full/Dual)"),
            (Action::ViewSort, "Sort options menu"),
            (Action::ViewHidden, "Toggle hidden files"),
        ],
    ),
    (
        "Dual Pane Mode",
        &[
            (Action::PaneSwitch, "Switch active pane"),
            (Action::FileCopyTo, "Copy to other pane"),
            (Action::FileMoveTo, "Move to other pane"),
        ],
    ),
    (
        "General",
        &[(Action::Help, "Show this help"), (Action::Quit, "Quit")],
    ),
];

/// Keys inside popups, fixed so a keymap can never lock you out of one.
const FIXED_KEYS: [(&str, &str); 10] = [
    ("y / n or Esc", "Trash/Delete prompt: confirm / cancel"),
    ("n s t / a d", "Sort menu: name, size, date / ASC, DESC"),
    ("q or Esc", "Close the sort menu"),
    ("Enter / Esc", "Text field: confirm / leave"),
    ("← → Backspace", "Text field: move the cursor / delete"),
    ("↑ / ↓", "Copy to… / Move to…: choose a folder"),
    ("Esc, q or I", "Close Get Info"),
    ("j k ↑ ↓", "Scroll this help (and PageUp/PageDown)"),
    ("Esc, q or ?", "Close this help"),
    ("Ctrl-C", "Quit (always)"),
];

/// Draw the help from the live keymap, sized to its lines and scrolled to `scroll`
/// (kept in range). Returns the scroll used and how many lines fit.
pub fn render_keybindings_popup(
    f: &mut Frame,
    keymap: &KeymapSettings,
    scroll: usize,
) -> (usize, usize) {
    let lines = keybinding_help_lines(keymap);
    // Borders and padding take 4 columns and 2 rows.
    let width = lines.iter().map(Line::width).max().unwrap_or(0) + 4;
    let area = draw_fixed_height_popup(
        f.size().inner(&Margin::new(1, 1)),
        0,
        u16::try_from(width).unwrap_or(u16::MAX),
        u16::try_from(lines.len() + 2).unwrap_or(u16::MAX),
    );
    let visible = usize::from(area.height.saturating_sub(2));
    let total = lines.len();
    let scroll = scroll.min(total.saturating_sub(visible));
    let title = if total > visible {
        format!(
            "Keybindings — j/k scroll, Esc close ({}/{total})",
            scroll + visible
        )
    } else {
        "Keybindings (Esc to close)".to_string()
    };
    let popup = Paragraph::new(lines)
        .scroll((u16::try_from(scroll).unwrap_or(u16::MAX), 0))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_set(border::ROUNDED)
                .padding(Padding::horizontal(1))
                .title(title),
        )
        .style(OneDarkTheme::normal());
    f.render_widget(Clear, area);
    f.render_widget(popup, area);
    (scroll, visible)
}

fn keybinding_help_lines(keymap: &KeymapSettings) -> Vec<Line<'static>> {
    let bindings = keymap.resolve();
    // Columns as wide as the widest keys (custom ones can be long) and description.
    let key_width = Action::ALL
        .iter()
        .map(|(action, _)| {
            key_text(&action_key_labels(&bindings, *action))
                .chars()
                .count()
        })
        .max()
        .unwrap_or(0)
        .max(15);
    let description_width = HELP_SECTIONS
        .iter()
        .flat_map(|(_, rows)| rows.iter())
        .map(|(_, description)| description.chars().count())
        .max()
        .unwrap_or(0);
    let widths = (key_width, description_width);
    let mut lines = Vec::new();
    for (title, rows) in HELP_SECTIONS {
        if !lines.is_empty() {
            lines.push(Line::from(""));
        }
        lines.push(Line::styled(format!("── {title} ──"), OneDarkTheme::info()));
        for (action, description) in rows {
            let keys = key_text(&action_key_labels(&bindings, *action));
            lines.push(keybinding_help_row(&keys, description, widths, *action));
        }
    }
    lines.push(Line::from(""));
    lines.push(Line::styled(
        "── Popup keys (can't be rebound) ──",
        OneDarkTheme::info(),
    ));
    for (keys, description) in FIXED_KEYS {
        lines.push(Line::from(format!("  {keys:<key_width$} {description}")));
    }
    lines
}

pub fn configured_action_key_labels(keymap: &KeymapSettings, action: Action) -> Vec<String> {
    action_key_labels(&keymap.resolve(), action)
}

/// Plain characters first, then named keys, Ctrl chords and leader sequences.
fn action_key_labels(bindings: &Bindings, action: Action) -> Vec<String> {
    let mut key_labels: Vec<(u8, String)> = bindings
        .normal
        .iter()
        .filter(|(_, bound)| *bound == action)
        .map(|(sequence, _)| {
            let rank = match sequence {
                KeySequence::Leader(_) => 3,
                KeySequence::Single(key) if key.starts_with("<C-") => 2,
                KeySequence::Single(key) if key.chars().count() > 1 => 1,
                KeySequence::Single(_) => 0,
            };
            (rank, format_key_sequence(sequence, &bindings.leader))
        })
        .collect();

    key_labels.sort();
    key_labels.into_iter().map(|(_, label)| label).collect()
}

fn format_key_sequence(sequence: &KeySequence, leader: &str) -> String {
    match sequence {
        KeySequence::Leader(key) => format!("{} + {}", key_label(leader), key_label(key)),
        KeySequence::Single(key) => key_label(key),
    }
}

/// One help row: the keys, what the action does, and its settings.toml name, dimmed.
fn keybinding_help_row(
    keys: &str,
    description: &str,
    (key_width, width): (usize, usize),
    action: Action,
) -> Line<'static> {
    Line::from(vec![
        Span::raw(format!("  {keys:<key_width$} {description:<width$}  ")),
        Span::styled(action.name(), OneDarkTheme::dotfile()),
    ])
}

fn key_text(keys: &[String]) -> String {
    if keys.is_empty() {
        "Unbound".to_string()
    } else {
        keys.join(" or ")
    }
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

    fn help_text(keymap: &KeymapSettings) -> Vec<String> {
        keybinding_help_lines(keymap)
            .iter()
            .map(|line| {
                let text: String = line
                    .spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect();
                text.trim_end().to_string()
            })
            .collect()
    }

    fn has_row(lines: &[String], keys: &str, description: &str, action: &str) -> bool {
        lines.iter().any(|line| {
            line.strip_prefix(&format!("  {keys} "))
                .is_some_and(|rest| {
                    rest.trim_start().starts_with(description)
                        && line.ends_with(&format!("  {action}"))
                })
        })
    }

    /// The column `text` starts at in the help.
    fn column(lines: &[String], text: &str) -> usize {
        lines
            .iter()
            .find_map(|line| line.find(text).map(|i| line[..i].chars().count()))
            .unwrap()
    }

    #[test]
    fn keybinding_help_lists_every_action_with_its_default_keys() {
        let lines = help_text(&KeymapSettings::default());

        assert!(has_row(
            &lines,
            "/ or i",
            "Search current directory",
            "search.current"
        ));
        assert!(has_row(
            &lines,
            "Space + /",
            "Search from root cache",
            "search.root"
        ));
        assert!(has_row(&lines, "j or ↓", "Move down", "nav.down"));
        assert!(has_row(
            &lines,
            "Enter",
            "Open with IDE / copy path",
            "nav.choose"
        ));
        assert!(has_row(
            &lines,
            "Shift+C",
            "Copy to… (pick a folder)",
            "file.copy_to"
        ));
        assert!(has_row(
            &lines,
            "Shift+C",
            "Copy to other pane",
            "file.copy_to"
        ));
        assert!(has_row(&lines, "Tab", "Switch active pane", "pane.switch"));
        assert!(lines.contains(&"  Ctrl-C          Quit (always)".to_string()));
        assert!(!lines.iter().any(|line| line.contains("Unbound")));
        for (_, name) in Action::ALL {
            assert!(lines.iter().any(|line| line.ends_with(name)), "{name}");
        }
    }

    #[test]
    fn keybinding_help_reflects_custom_keys_and_marks_unbound_actions() {
        let mut settings = KeymapSettings {
            leader: ",".to_string(),
            ..KeymapSettings::default()
        };
        for (key, action) in [
            ("f", "search.current"),
            ("/", "search.root"),
            ("<C-d>", "nav.down"),
            ("q", "none"),
        ] {
            settings.normal.insert(key.to_string(), action.to_string());
        }

        let lines = help_text(&settings);

        assert!(has_row(
            &lines,
            "f or i",
            "Search current directory",
            "search.current"
        ));
        assert!(has_row(
            &lines,
            "/ or , + /",
            "Search from root cache",
            "search.root"
        ));
        assert!(has_row(&lines, "j or ↓ or Ctrl-d", "Move down", "nav.down"));
        assert!(has_row(&lines, "Unbound", "Quit", "quit"));
        // Long custom keys widen the key column instead of shifting their row.
        assert_eq!(column(&lines, "Move down"), column(&lines, "Move up"));
        assert_eq!(column(&lines, "Move down"), column(&lines, "Quit (always)"));
        assert!(!has_row(
            &lines,
            "/ or i",
            "Search current directory",
            "search.current"
        ));
    }

    #[test]
    fn keybinding_help_scrolls_when_it_does_not_fit() {
        use ratatui::{backend::TestBackend, Terminal};
        let screen = |width, height, scroll| {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            let mut view = (0, 0);
            terminal
                .draw(|f| view = render_keybindings_popup(f, &KeymapSettings::default(), scroll))
                .unwrap();
            let text: String = terminal
                .backend()
                .buffer()
                .content()
                .iter()
                .map(|cell| cell.symbol())
                .collect();
            (view, text)
        };
        let total = keybinding_help_lines(&KeymapSettings::default()).len();

        let ((scroll, visible), top) = screen(80, 24, 0);
        assert_eq!((scroll, visible), (0, 20));
        assert!(top.contains(&format!("Keybindings — j/k scroll, Esc close (20/{total})")));
        assert!(top.contains("── Navigation ──") && top.contains("nav.down"));
        assert!(!top.contains("Quit (always)"));

        let ((scroll, _), bottom) = screen(80, 24, usize::MAX);
        assert_eq!(scroll, total - visible);
        assert!(bottom.contains(&format!("({total}/{total})")));
        assert!(bottom.contains("Quit (always)"));
        assert!(!bottom.contains("── Navigation ──"));

        let ((scroll, _), all) = screen(120, 80, 5);
        assert_eq!(scroll, 0);
        assert!(all.contains("Keybindings (Esc to close)"));
        assert!(all.contains("── Navigation ──") && all.contains("Quit (always)"));
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
