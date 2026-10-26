//! Popup rendering utilities and widget builders.

use std::{collections::HashSet, path::Path};

use ratatui::{
    layout::{Constraint, Direction, Layout, Margin, Rect},
    style::{Modifier, Style},
    symbols::border,
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Padding, Paragraph, Wrap},
    Frame,
};

use crate::app::{FolderPicker, Palette};
use crate::config::{
    keymap::{key_label, Bindings},
    Action, KeySequence, KeymapSettings, ThemeColors,
};
use crate::event::palette::palette_matches;
use crate::utils::format::replace_home_with_tilde;
use crate::utils::{format_file_size, EntryInfo, SortBy, SortType};

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
    theme: &ThemeColors,
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
                .style(Style::default().fg(theme.mode_delete)),
        )
        .wrap(Wrap { trim: true })
        .centered()
}

/// Draw a selection's Trash or permanent-delete prompt: how many items, the first few
/// names (folders end in `/`, by their records from `listed_info`) and how many more,
/// sized to fit.
pub fn render_batch_confirmation_popup(
    f: &mut Frame,
    targets: &[String],
    listed_info: impl Fn(&str) -> Option<EntryInfo>,
    to_trash: bool,
    theme: &ThemeColors,
) {
    const SHOWN: usize = 5;
    let count = match targets.len() {
        1 => "1 item".to_string(),
        n => format!("{n} items"),
    };
    let mut content = vec![
        Line::from(""),
        Line::from(if to_trash {
            format!("Move {count} to Trash?")
        } else {
            format!("Permanently delete {count}?")
        }),
    ];
    for target in targets.iter().take(SHOWN) {
        let path = Path::new(target);
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        // A folder itself, not a link to one.
        let slash = if listed_info(target).is_some_and(|info| info.is_dir() && !info.is_symlink) {
            "/"
        } else {
            ""
        };
        content.push(Line::from(format!("{name}{slash}")));
    }
    if targets.len() > SHOWN {
        content.push(Line::from(format!("and {} more", targets.len() - SHOWN)));
    }
    if !to_trash {
        content.push(Line::from("This can't be undone."));
    }
    content.push(Line::from(""));
    content.push(Line::from("Press 'y' to confirm, 'n' to cancel"));

    let height = u16::try_from(content.len() + 2).unwrap_or(u16::MAX);
    let area = draw_fixed_height_popup(f.size(), 60, 50, height);
    let popup = Paragraph::new(content)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_set(border::ROUNDED)
                .title(if to_trash {
                    "Move to Trash"
                } else {
                    "Permanent Delete"
                })
                .style(Style::default().fg(theme.mode_delete)),
        )
        .centered();
    f.render_widget(Clear, area);
    f.render_widget(popup, area);
}

/// Create the create file/dir input popup widget.
/// Shows "Create Folder" if input ends with `/`, or "Create File" otherwise.
pub fn create_create_input_popup<'a>(
    input_text: &'a str,
    is_error: bool,
    error_message: &str,
    theme: &ThemeColors,
) -> Paragraph<'a> {
    let title = if is_error {
        format!("Error: {}", error_message)
    } else if input_text.is_empty() {
        "Create File/Dir".to_string()
    } else if input_text.ends_with('/') {
        "Create Folder".to_string()
    } else {
        "Create File".to_string()
    };

    let style = if is_error {
        theme.error
    } else {
        Style::default().fg(theme.mode_create)
    };

    Paragraph::new(input_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_set(border::ROUNDED)
                .title(title)
                .title_bottom(Line::from("end with / for a folder").right_aligned()),
        )
        .style(style)
}

/// Create the rename input popup widget.
pub fn create_rename_input_popup<'a>(input_text: &'a str, theme: &ThemeColors) -> Paragraph<'a> {
    Paragraph::new(input_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_set(border::ROUNDED)
                .title("Rename"),
        )
        .style(Style::default().fg(theme.mode_rename))
}

/// Draw the "Copy to…"/"Move to…" picker over the list: the query (with the cursor at
/// `cursor`) above the matching folders, each with its full path.
pub fn render_folder_picker(
    f: &mut Frame,
    picker: &FolderPicker,
    query: &str,
    cursor: usize,
    theme: &ThemeColors,
) {
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
                    theme.dotfile,
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
        .highlight_style(
            Style::default()
                .bg(theme.cursor_bg)
                .fg(theme.bright)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("❯ ");
    let mut list_state = ListState::default();
    list_state.select(picker.selected);

    f.render_widget(Clear, area);
    f.render_widget(input.style(theme.info), chunks[0]);
    f.render_stateful_widget(list.style(theme.normal), chunks[1], &mut list_state);
    let (x, y) = input_popup_cursor_position(chunks[0], cursor - scroll);
    f.set_cursor(x, y);
}

/// Draw Get Info centred and sized to its rows: labels in a column, and each value
/// wrapped beside its label, so long names and paths are never cut off.
pub fn render_info_popup(f: &mut Frame, rows: &[(&str, String)], theme: &ThemeColors) {
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
                Span::styled(format!("{label:<label_width$}"), theme.info),
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
        .style(theme.normal);
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
pub fn create_sort_options_popup<'a>(
    sort_by: &SortBy,
    sort_type: &SortType,
    theme: &ThemeColors,
) -> Paragraph<'a> {
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
        .style(Style::default().fg(theme.mode_sort))
}

/// The help's sections: each action and what it does. Copy to/Move to appear twice
/// because Dual Pane mode sends the item to the other pane instead. The palette lists
/// the same actions under the same groups.
const HELP_SECTIONS: [(&str, &[(Action, &str)]); 8] = [
    (
        "Navigation",
        &[
            (Action::NavDown, "Move down"),
            (Action::NavUp, "Move up"),
            (Action::NavTop, "Go to the first item"),
            (Action::NavBottom, "Go to the last item"),
            (Action::NavParent, "Go to parent directory"),
            (Action::NavEnter, "Enter directory"),
            (Action::NavChoose, "Open with IDE / copy path"),
        ],
    ),
    (
        "File Operations",
        &[
            (Action::FileCreate, "Create file (end with / for a folder)"),
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
        "Clipboard",
        &[
            (Action::CopyPath, "Copy path"),
            (Action::CopyName, "Copy name"),
            (Action::CopyParent, "Copy parent folder path"),
        ],
    ),
    (
        "Visual Mode",
        &[
            (
                Action::SelectVisual,
                "Select adjacent items; move to extend",
            ),
            (Action::FileTrash, "Move the selected items to Trash"),
            (Action::FileDelete, "Permanently delete the selected items"),
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
        &[
            (Action::Help, "Show this help"),
            (Action::Palette, "Command palette: run any action"),
            (Action::Quit, "Quit"),
        ],
    ),
];

/// Keys inside popups and visual mode, fixed so a keymap can never lock you out of one.
const FIXED_KEYS: [(&str, &str); 12] = [
    ("y / n or Esc", "Trash/Delete prompt: confirm / cancel"),
    ("o", "Visual mode: swap the selection's ends"),
    ("Esc", "Visual mode: cancel the selection"),
    ("n s t / a d", "Sort menu: name, size, date / ASC, DESC"),
    ("q or Esc", "Close the sort menu"),
    ("Enter / Esc", "Text field: confirm / leave"),
    ("← → Backspace", "Text field: move the cursor / delete"),
    ("↑ / ↓", "Palette, Copy to… / Move to…: choose"),
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
    theme: &ThemeColors,
) -> (usize, usize) {
    let lines = keybinding_help_lines(keymap, theme);
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
        .style(theme.normal);
    f.render_widget(Clear, area);
    f.render_widget(popup, area);
    (scroll, visible)
}

fn keybinding_help_lines(keymap: &KeymapSettings, theme: &ThemeColors) -> Vec<Line<'static>> {
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
        lines.push(Line::styled(format!("── {title} ──"), theme.info));
        for (action, description) in rows {
            let keys = key_text(&action_key_labels(&bindings, *action));
            lines.push(keybinding_help_row(
                &keys,
                description,
                widths,
                *action,
                theme,
            ));
        }
    }
    lines.push(Line::from(""));
    lines.push(Line::styled(
        "── Fixed keys (can't be rebound) ──",
        theme.info,
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
pub(crate) fn action_key_labels(bindings: &Bindings, action: Action) -> Vec<String> {
    let mut key_labels: Vec<(u8, String)> = bindings
        .normal
        .iter()
        .filter(|(_, bound)| *bound == action)
        .map(|(sequence, _)| {
            let rank = match sequence {
                KeySequence::Leader(_) | KeySequence::Pair(..) => 3,
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
        // "gg", or "Ctrl-w j" when a key's label is longer than one character.
        KeySequence::Pair(first, second) => {
            let (first, second) = (key_label(first), key_label(second));
            if first.chars().count() == 1 && second.chars().count() == 1 {
                format!("{first}{second}")
            } else {
                format!("{first} {second}")
            }
        }
    }
}

/// One help row: the keys ("no key", dimmed, for an unbound action), what the action
/// does, and its settings.toml name, dimmed.
fn keybinding_help_row(
    keys: &str,
    description: &str,
    (key_width, width): (usize, usize),
    action: Action,
    theme: &ThemeColors,
) -> Line<'static> {
    let key_style = if keys == NO_KEY {
        theme.dotfile
    } else {
        Style::default()
    };
    Line::from(vec![
        Span::raw("  "),
        Span::styled(format!("{keys:<key_width$}"), key_style),
        Span::raw(format!(" {description:<width$}  ")),
        Span::styled(action.name(), theme.dotfile),
    ])
}

/// Shown in place of the keys of an action nothing is bound to.
const NO_KEY: &str = "no key";

fn key_text(keys: &[String]) -> String {
    if keys.is_empty() {
        NO_KEY.to_string()
    } else {
        keys.join(" or ")
    }
}

/// Every action the palette offers, once each in the help's order, with what it does
/// and its group: all but the motions and the palette itself.
pub fn palette_actions() -> Vec<(Action, &'static str, &'static str)> {
    let mut listed = HashSet::new();
    HELP_SECTIONS
        .iter()
        .flat_map(|(group, rows)| {
            rows.iter()
                .map(move |(action, description)| (*action, *description, *group))
        })
        .filter(|(action, ..)| {
            let motion = matches!(
                action,
                Action::NavDown | Action::NavUp | Action::NavTop | Action::NavBottom
            );
            !motion && *action != Action::Palette && listed.insert(*action)
        })
        .collect()
}

/// What `action` does, as the help first describes it.
pub fn action_description(action: Action) -> &'static str {
    HELP_SECTIONS
        .iter()
        .flat_map(|(_, rows)| rows.iter())
        .find(|(listed, _)| *listed == action)
        .map_or("", |(_, description)| description)
}

/// Most matching actions the palette shows at once; it scrolls through the rest.
const PALETTE_ROWS: usize = 12;

/// Draw the command palette near the top: the typed filter and how many actions match,
/// then the matches, each with its keys from the live keymap and its group, scrolled
/// from `palette.scroll` just enough to show the highlight. Returns the scroll used.
pub fn render_palette(
    f: &mut Frame,
    palette: &Palette,
    keymap: &KeymapSettings,
    theme: &ThemeColors,
) -> usize {
    let actions = palette_actions();
    let matches = palette_matches(&palette.query);
    let bindings = keymap.resolve();
    let screen = f.size();
    // Inside the borders: the prompt, a divider, the rows, a blank line and the footer.
    // The box starts 2 rows down and leaves 2 free at the bottom.
    let rows = PALETTE_ROWS
        .min(actions.len())
        .min(usize::from(screen.height.saturating_sub(10)));
    let width = screen.width.saturating_sub(2).min(72);
    let area = Rect {
        x: screen.x + (screen.width - width) / 2,
        y: screen.y + 2,
        width,
        height: u16::try_from(rows + 6).unwrap_or(u16::MAX),
    }
    .intersection(screen);
    let inner = usize::from(width.saturating_sub(2));

    let blue = Style::default().fg(theme.blue).add_modifier(Modifier::BOLD);
    let hint = Style::default().fg(theme.light_gray);
    let count = format!("{} of {} actions ", matches.len(), actions.len());
    // Show the end of a query too long for the prompt, which leaves room for the cursor.
    let room = inner.saturating_sub(count.chars().count() + 4);
    let typed = palette.query.chars().count();
    let query: String = palette
        .query
        .chars()
        .skip(typed.saturating_sub(room))
        .collect();
    let query_width = query.chars().count();
    let mut lines = vec![
        Line::from(vec![
            Span::styled(" › ", blue),
            Span::styled(query, Style::default().fg(theme.bright)),
            Span::raw(" ".repeat(inner.saturating_sub(query_width + 3 + count.chars().count()))),
            Span::styled(count, hint),
        ]),
        Line::styled("─".repeat(inner), Style::default().fg(theme.gray)),
    ];

    // Columns: "▌ ", the description, the key chips, the group and a margin.
    let group_width = actions
        .iter()
        .map(|(.., group)| group.chars().count())
        .max()
        .unwrap_or(0);
    let key_width = 11;
    let description_width = inner.saturating_sub(group_width + key_width + 6);
    let selected = palette.selected.min(matches.len().saturating_sub(1));
    let scroll = if rows == 0 {
        0
    } else {
        palette
            .scroll
            .min(selected)
            .max((selected + 1).saturating_sub(rows))
            .min(matches.len().saturating_sub(rows))
    };
    for (i, found) in matches.iter().enumerate().skip(scroll).take(rows) {
        let base = if i == selected {
            Style::default().bg(theme.selection)
        } else {
            Style::default()
        };
        let text = base.fg(if i == selected {
            theme.bright
        } else {
            theme.fg
        });
        let mut spans = vec![Span::styled(
            if i == selected { "▌ " } else { "  " },
            base.fg(theme.blue),
        )];
        let description: String = found.description.chars().take(description_width).collect();
        let shown = description.chars().count();
        spans.extend(underline_matches(
            &description,
            &found.positions,
            text,
            base.patch(blue).add_modifier(Modifier::UNDERLINED),
        ));
        spans.push(Span::styled(
            " ".repeat(description_width - shown + 1),
            base,
        ));
        let keys = action_key_labels(&bindings, found.action);
        let chips_width: usize = keys.iter().map(|key| key.chars().count() + 3).sum();
        let keys = if chips_width > key_width + 1 {
            &keys[..1]
        } else {
            &keys[..]
        };
        let mut used = 0;
        let chip = Style::default()
            .bg(theme.gray)
            .fg(theme.bright)
            .add_modifier(Modifier::BOLD);
        for key in keys {
            if used > 0 {
                spans.push(Span::styled(" ", base));
                used += 1;
            }
            let label: String = key.chars().take(key_width - used - 2).collect();
            used += label.chars().count() + 2;
            spans.push(Span::styled(format!(" {label} "), chip));
        }
        if keys.is_empty() {
            spans.push(Span::styled(NO_KEY, base.fg(theme.gray)));
            used = NO_KEY.len();
        }
        spans.push(Span::styled(
            format!(
                "{:pad$} {:>group_width$}  ",
                "",
                found.group,
                pad = key_width - used
            ),
            base.fg(theme.light_gray),
        ));
        lines.push(Line::from(spans));
    }
    if matches.is_empty() && rows > 0 {
        lines.push(Line::styled(
            "  No action matches",
            Style::default().fg(theme.gray),
        ));
    }
    lines.resize(rows + 3, Line::from(""));
    lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled("⏎", blue),
        Span::styled(" run   ", hint),
        Span::styled("↑/↓", blue),
        Span::styled(" choose   ", hint),
        Span::styled("Esc", blue),
        Span::styled(" close", hint),
    ]));

    let popup = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_set(border::ROUNDED)
            .border_style(Style::default().fg(theme.blue))
            .title(Span::styled(" Run an action ", blue)),
    );
    f.render_widget(Clear, area);
    f.render_widget(popup, area);
    let cursor =
        (usize::from(area.x) + 4 + query_width).min(usize::from(area.right()).saturating_sub(2));
    f.set_cursor(u16::try_from(cursor).unwrap_or(area.x), area.y + 1);
    scroll
}

/// `text` as spans, the characters at `positions` in `matched` style and the rest in
/// `style`.
fn underline_matches(
    text: &str,
    positions: &[usize],
    style: Style,
    matched: Style,
) -> Vec<Span<'static>> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut run = String::new();
    let mut run_matched = false;
    for (i, c) in text.chars().enumerate() {
        let is_matched = positions.contains(&i);
        if is_matched != run_matched && !run.is_empty() {
            let run_style = if run_matched { matched } else { style };
            spans.push(Span::styled(std::mem::take(&mut run), run_style));
        }
        run_matched = is_matched;
        run.push(c);
    }
    if !run.is_empty() {
        spans.push(Span::styled(run, if run_matched { matched } else { style }));
    }
    spans
}

/// Draw the box naming each key that can follow `title`'s key and what it does, near
/// the bottom right, above the status bar.
pub fn render_which_key(
    f: &mut Frame,
    title: &str,
    keys: &[(String, Action)],
    theme: &ThemeColors,
) {
    let blue = Style::default().fg(theme.blue).add_modifier(Modifier::BOLD);
    let key_width = keys
        .iter()
        .map(|(key, _)| key.chars().count())
        .max()
        .unwrap_or(0);
    let lines: Vec<Line> = keys
        .iter()
        .map(|(key, action)| {
            Line::from(vec![
                Span::styled(format!("{key:<key_width$}  "), blue),
                Span::styled(action_description(*action), Style::default().fg(theme.fg)),
            ])
        })
        .collect();
    // Borders and padding take 4 columns and 4 rows.
    let width = lines
        .iter()
        .map(Line::width)
        .max()
        .unwrap_or(0)
        .max(title.chars().count() + 2)
        + 4;
    let width = u16::try_from(width).unwrap_or(u16::MAX);
    let height = u16::try_from(lines.len() + 4).unwrap_or(u16::MAX);
    let screen = f.size();
    // Two columns in from the right, above the status bar's 3 rows and the margin.
    let area = Rect {
        x: screen.x + screen.width.saturating_sub(width + 2),
        y: screen.y + screen.height.saturating_sub(height + 4),
        width,
        height,
    }
    .intersection(screen);
    let popup = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_set(border::ROUNDED)
            .border_style(Style::default().fg(theme.blue))
            .padding(Padding::uniform(1))
            .title(Span::styled(format!(" {title} "), blue)),
    );
    f.render_widget(Clear, area);
    f.render_widget(popup, area);
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
        keybinding_help_lines(keymap, &theme())
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
        assert!(has_row(&lines, "gg", "Go to the first item", "nav.top"));
        assert!(has_row(
            &lines,
            "Shift+G",
            "Go to the last item",
            "nav.bottom"
        ));
        assert!(lines.contains(&"── Visual Mode ──".to_string()));
        assert!(has_row(
            &lines,
            "Shift+V or v",
            "Select adjacent items; move to extend",
            "select.visual"
        ));
        assert!(has_row(
            &lines,
            "d",
            "Move the selected items to Trash",
            "file.trash"
        ));
        assert!(has_row(
            &lines,
            "Shift+D",
            "Permanently delete the selected items",
            "file.delete"
        ));
        assert!(
            lines.contains(&"  o               Visual mode: swap the selection's ends".to_string())
        );
        assert!(lines.contains(&"  Esc             Visual mode: cancel the selection".to_string()));
        assert!(lines.contains(&"  Ctrl-C          Quit (always)".to_string()));
        assert!(lines.contains(&"── Clipboard ──".to_string()));
        assert!(has_row(&lines, "no key", "Copy path", "copy.path"));
        assert!(has_row(&lines, "no key", "Copy name", "copy.name"));
        assert!(has_row(
            &lines,
            "no key",
            "Copy parent folder path",
            "copy.parent"
        ));
        assert!(has_row(
            &lines,
            ":",
            "Command palette: run any action",
            "palette"
        ));
        // Only the copy actions have no key.
        let unbound = lines.iter().filter(|line| line.contains("no key")).count();
        assert_eq!(unbound, 3);
        for (_, name) in Action::ALL {
            assert!(lines.iter().any(|line| line.ends_with(name)), "{name}");
        }
    }

    #[test]
    fn keybinding_help_dims_no_key() {
        let lines = keybinding_help_lines(&KeymapSettings::default(), &theme());
        let row = lines
            .iter()
            .find(|line| {
                line.spans
                    .last()
                    .is_some_and(|span| span.content == "copy.path")
            })
            .unwrap();
        let keys = row.spans.iter().find(|span| span.content.trim() == NO_KEY);
        assert_eq!(keys.unwrap().style, theme().dotfile);
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
            ("<C-w>j", "nav.top"),
            ("<leader>c", "copy.path"),
        ] {
            settings.normal.insert(key.to_string(), action.to_string());
        }

        let lines = help_text(&settings);
        assert!(has_row(
            &lines,
            "Ctrl-w j or gg",
            "Go to the first item",
            "nav.top"
        ));

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
        assert!(has_row(&lines, "no key", "Quit", "quit"));
        assert!(has_row(&lines, ", + c", "Copy path", "copy.path"));
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
                .draw(|f| {
                    view = render_keybindings_popup(f, &KeymapSettings::default(), scroll, &theme())
                })
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
        let total = keybinding_help_lines(&KeymapSettings::default(), &theme()).len();

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

    fn theme() -> ThemeColors {
        crate::config::Theme::default().to_colors().unwrap()
    }

    /// Draw the palette for `query` with `selected` highlighted after `scroll`; the
    /// screen's rows, the buffer and the scroll it used.
    fn palette_screen(
        keymap: &KeymapSettings,
        query: &str,
        (selected, scroll): (usize, usize),
        height: u16,
    ) -> (Vec<String>, ratatui::buffer::Buffer, usize) {
        use ratatui::{backend::TestBackend, Terminal};
        let palette = Palette {
            query: query.into(),
            selected,
            scroll,
        };
        let mut terminal = Terminal::new(TestBackend::new(100, height)).unwrap();
        let mut used = 0;
        terminal
            .draw(|f| used = render_palette(f, &palette, keymap, &theme()))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        let rows = buffer
            .content()
            .chunks(100)
            .map(|row| row.iter().map(|cell| cell.symbol()).collect())
            .collect();
        (rows, buffer, used)
    }

    #[test]
    fn palette_shows_the_count_and_each_match_with_its_live_keys_and_group() {
        let mut keymap = KeymapSettings::default();
        keymap.normal.insert("H".into(), "view.hidden".into());
        keymap.normal.insert(".".into(), "none".into());
        let (rows, buffer, _) = palette_screen(&keymap, "hid", (0, 0), 30);
        let screen = rows.join("\n");

        assert!(rows[2].contains("╭ Run an action ─"), "{screen}");
        assert!(rows[3].contains("› hid") && rows[3].contains("1 of 26 actions"));
        assert!(rows[4].contains("────"));
        let row = &rows[5];
        assert!(row.contains("▌ Toggle hidden files"), "{row}");
        assert!(row.contains(" Shift+H ") && !row.contains(" . "), "{row}");
        assert!(row.trim_end().ends_with("View & Sort  │"), "{row}");
        assert!(screen.contains("⏎ run   ↑/↓ choose   Esc close"));
        // The box fits 12 rows whatever matches, so it doesn't jump while typing.
        assert!(
            rows[18].contains("⏎ run") && rows[19].contains("╰"),
            "{screen}"
        );
        // Exactly the matched letters are underlined.
        let underlined: String = (0..100)
            .map(|x| buffer.get(x, 5))
            .filter(|cell| cell.modifier.contains(Modifier::UNDERLINED))
            .map(|cell| cell.symbol())
            .collect();
        assert_eq!(underlined, "hid");

        let (rows, ..) = palette_screen(&KeymapSettings::default(), "", (0, 0), 30);
        let screen = rows.join("\n");
        assert!(screen.contains("26 of 26 actions"), "{screen}");
        // Every key when they fit, the first otherwise.
        assert!(rows[5].contains("▌ Go to parent directory") && rows[5].contains(" h   ← "));
        let (rows, ..) = palette_screen(&KeymapSettings::default(), "root", (0, 0), 30);
        assert!(rows[5].contains("Search from root cache") && rows[5].contains(" Space + / "));
        let (rows, ..) = palette_screen(&KeymapSettings::default(), "adjacent", (0, 0), 30);
        assert!(rows[5].contains(" Shift+V ") && !rows[5].contains(" v "));

        let (rows, ..) = palette_screen(&KeymapSettings::default(), "copy p", (0, 0), 30);
        assert!(rows[5].contains("Copy path") && rows[5].contains("no key"));
        assert!(rows[5].contains("Clipboard"));
        assert!(rows[6].contains("Copy parent folder path") && rows[6].contains("no key"));

        let (rows, ..) = palette_screen(&KeymapSettings::default(), "zzz", (0, 0), 30);
        assert!(rows[3].contains("0 of 26 actions") && rows[5].contains("No action matches"));
    }

    #[test]
    fn palette_scrolls_only_as_far_as_keeps_the_highlight_in_view() {
        let keymap = KeymapSettings::default();
        let highlighted = |rows: &[String]| {
            rows.iter()
                .find(|row| row.contains('▌'))
                .map(|row| row.split("▌ ").nth(1).unwrap()[..20].to_string())
        };

        let (rows, _, scroll) = palette_screen(&keymap, "", (20, 0), 30);
        assert_eq!(scroll, 9);
        assert_eq!(highlighted(&rows).as_deref(), Some("Cycle view mode (Nor"));
        assert!(rows[16].contains("▌"));
        // Moving back up inside the view doesn't scroll it.
        let (rows, _, scroll) = palette_screen(&keymap, "", (15, 9), 30);
        assert_eq!(scroll, 9);
        assert!(rows[11].contains("▌"));
        let (_, _, scroll) = palette_screen(&keymap, "", (3, 9), 30);
        assert_eq!(scroll, 3);
        // A short terminal shows fewer rows.
        let (rows, _, scroll) = palette_screen(&keymap, "", (25, 0), 16);
        assert_eq!(scroll, 20);
        assert!(rows.iter().any(|row| row.contains("▌ Quit")));
        assert!(rows[12].contains("⏎ run"), "{rows:#?}");
    }

    #[test]
    fn which_key_box_names_each_next_key_near_the_bottom_right() {
        use ratatui::{backend::TestBackend, Terminal};
        let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
        let keys = [
            ("/".to_string(), Action::SearchRoot),
            ("c".to_string(), Action::CopyPath),
        ];
        terminal
            .draw(|f| render_which_key(f, "Space …", &keys, &theme()))
            .unwrap();
        let rows: Vec<String> = terminal
            .backend()
            .buffer()
            .content()
            .chunks(100)
            .map(|row| row.iter().map(|cell| cell.symbol()).collect())
            .collect();

        assert!(
            rows[20].ends_with("╭ Space … ──────────────────╮  "),
            "{rows:#?}"
        );
        assert!(
            rows[22].ends_with("│ /  Search from root cache │  "),
            "{rows:#?}"
        );
        assert!(
            rows[23].ends_with("│ c  Copy path              │  "),
            "{rows:#?}"
        );
        // Just above the status bar's 3 rows and the margin.
        assert!(
            rows[25].ends_with("╰───────────────────────────╯  "),
            "{rows:#?}"
        );
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
                info: EntryInfo::default(),
                original_index: 0,
            }],
            selected: Some(0),
        };
        let query = format!("/{}END", "x".repeat(200));
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal
            .draw(|f| render_folder_picker(f, &picker, &query, query.chars().count(), &theme()))
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
        terminal
            .draw(|f| render_info_popup(f, &rows, &theme()))
            .unwrap();

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
                    create_delete_confirmation_popup(
                        "notes.txt",
                        false,
                        None,
                        Some(5),
                        false,
                        &theme(),
                    ),
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
                    create_delete_confirmation_popup("notes", true, None, None, true, &theme()),
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

    fn batch_prompt(targets: &[String], to_trash: bool) -> String {
        use ratatui::{backend::TestBackend, Terminal};
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal
            .draw(|f| {
                render_batch_confirmation_popup(
                    f,
                    targets,
                    |path| Some(EntryInfo::read(Path::new(path))),
                    to_trash,
                    &theme(),
                )
            })
            .unwrap();
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect()
    }

    #[test]
    fn selection_prompts_count_the_items_and_name_the_first_five() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("photos")).unwrap();
        let targets: Vec<String> = ["notes.txt", "photos", "todo.md"]
            .iter()
            .map(|name| dir.path().join(name).to_string_lossy().into_owned())
            .collect();

        let trash = batch_prompt(&targets, true);
        assert!(trash.contains("Move to Trash"));
        assert!(trash.contains("Move 3 items to Trash?"));
        for name in ["notes.txt", "photos/", "todo.md"] {
            assert!(trash.contains(name), "{name}");
        }
        assert!(!trash.contains("more") && !trash.contains("can't be undone"));
        assert!(trash.contains("'y' to confirm, 'n' to cancel"));

        let many: Vec<String> = (1..=8).map(|i| format!("/w/file{i}.txt")).collect();
        let delete = batch_prompt(&many, false);
        assert!(delete.contains("Permanently delete 8 items?"));
        assert!(delete.contains("file1.txt") && delete.contains("file5.txt"));
        assert!(!delete.contains("file6.txt"));
        assert!(delete.contains("and 3 more"));
        assert!(delete.contains("This can't be undone."));
        assert!(delete.contains("'y' to confirm, 'n' to cancel"));

        assert!(batch_prompt(&many[..1], true).contains("Move 1 item to Trash?"));
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
    fn create_popup_title_follows_the_trailing_slash() {
        use ratatui::{buffer::Buffer, widgets::Widget};
        for (input, title) in [
            ("", "Create File/Dir"),
            ("Makefile", "Create File"),
            ("v1.2", "Create File"),
            ("docs/todo.md", "Create File"),
            ("photos/", "Create Folder"),
            ("v1.2/", "Create Folder"),
        ] {
            let area = Rect::new(0, 0, 40, 3);
            let mut buffer = Buffer::empty(area);
            create_create_input_popup(input, false, "", &theme()).render(area, &mut buffer);
            let rows: Vec<String> = buffer
                .content
                .chunks(40)
                .map(|row| row.iter().map(|cell| cell.symbol()).collect())
                .collect();

            assert!(
                rows[0].starts_with(&format!("╭{title}─")),
                "{input}: {}",
                rows[0]
            );
            assert!(rows[2].contains("end with / for a folder"), "{}", rows[2]);
        }
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
