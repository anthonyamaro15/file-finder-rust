//! Drawing the index manager.

use std::path::Path;

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    symbols::border,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};
use unicode_width::UnicodeWidthStr;

use crate::config::ThemeColors;
use crate::index_manager::{IndexManager, IndexRow, IndexView};
use crate::index_tree::{TreeKind, TreeNode};
use crate::info::plural;
use crate::ui::{truncate, truncate_start, INDEX_FILLING};
use crate::utils::format::{format_thousands, replace_home_with_tilde};

/// Below this inner width the share bar is left out, keeping the count and percent.
const BAR_MIN_WIDTH: usize = 60;
const BAR_WIDTH: usize = 10;

/// Draw the index manager near the top, sized like the palette: the folder shown and
/// its subfolders, biggest first, scrolled to keep the highlight in view. `keys` adds
/// a line of keys, for when the hint row is hidden. Returns the scroll used.
pub fn render_index_manager(
    f: &mut Frame,
    manager: &IndexManager,
    keys: bool,
    theme: &ThemeColors,
) -> usize {
    let screen = f.size();
    let width = screen.width.saturating_sub(2).min(72);
    let inner = usize::from(width.saturating_sub(2));
    // Borders, the folder line and the footer (and keys); at least one row.
    let extra = 4 + usize::from(keys);
    let room = usize::from(screen.height.saturating_sub(4))
        .saturating_sub(extra)
        .max(1);
    let shown = manager.tree_visible();
    let biggest = manager.view == IndexView::Biggest && manager.tree.is_some();
    let row_count = if biggest {
        shown.len()
    } else {
        manager.rows.len()
    };
    let visible = row_count.clamp(1, room);
    let area = Rect {
        x: screen.x + (screen.width - width) / 2,
        y: screen.y + 2,
        width,
        height: u16::try_from(visible + extra).unwrap_or(u16::MAX),
    }
    .intersection(screen);

    let blue = Style::default().fg(theme.blue).add_modifier(Modifier::BOLD);
    let muted = Style::default().fg(theme.light_gray);
    let wide = inner >= BAR_MIN_WIDTH;

    // The folder shown (or what the tree is), and the columns' headings.
    let folder = if biggest {
        "heaviest branches, opened down to the mess".to_string()
    } else {
        crumb(&manager.folder)
    };
    let headings = if wide {
        "folders   of index "
    } else {
        "folders "
    };
    // A deep folder keeps its own name: cut from the start. The tree's sentence: the end.
    let room = inner.saturating_sub(headings.len() + 2);
    let folder = if biggest {
        truncate(&folder, room)
    } else {
        truncate_start(&folder, room)
    };
    let gap = inner.saturating_sub(folder.width() + headings.len() + 1);
    let mut lines = vec![Line::from(vec![
        Span::styled(format!(" {folder}"), if biggest { muted } else { blue }),
        Span::raw(" ".repeat(gap)),
        Span::styled(headings, muted),
    ])];

    let (selected, scroll_from) = match (&manager.tree, biggest) {
        (Some(tree), true) => (
            shown.iter().position(|&i| i == tree.cursor).unwrap_or(0),
            tree.scroll,
        ),
        _ => (manager.selected, manager.scroll),
    };
    let selected = selected.min(row_count.saturating_sub(1));
    let scroll = scroll_from
        .min(selected)
        .max((selected + 1).saturating_sub(visible))
        .min(row_count.saturating_sub(visible));
    let (after, unknown) = manager.total_after();
    if row_count == 0 {
        let empty = if manager.filling {
            INDEX_FILLING
        } else {
            "No folders here"
        };
        lines.push(Line::styled(format!("   {empty}"), muted));
    }
    match (&manager.tree, biggest) {
        (Some(tree), true) => {
            for (at, &i) in shown.iter().enumerate().skip(scroll).take(visible) {
                let open = tree.is_open(i);
                lines.push(tree_line(
                    manager,
                    &tree.nodes[i],
                    manager.smaller_after(tree, i),
                    open,
                    at == selected,
                    after,
                    inner,
                    wide,
                    theme,
                ));
            }
        }
        _ => {
            for (i, row) in manager.rows.iter().enumerate().skip(scroll).take(visible) {
                lines.push(row_line(
                    manager,
                    row,
                    i == selected,
                    after,
                    inner,
                    wide,
                    theme,
                ));
            }
        }
    }
    lines.resize(visible + 1, Line::from(""));
    // By folder: what's never indexed here. The tree: where the highlighted line is.
    let footer = match (&manager.tree, biggest) {
        (Some(tree), true) => tree.nodes.get(tree.cursor).map_or(String::new(), |node| {
            truncate_start(
                &replace_home_with_tilde(&node.row.path),
                inner.saturating_sub(1),
            )
        }),
        _ => truncate(&never_indexed(manager), inner.saturating_sub(1)),
    };
    lines.push(Line::styled(
        format!(" {footer}"),
        Style::default().fg(theme.gray),
    ));
    if keys {
        let key = Style::default().fg(theme.blue).add_modifier(Modifier::BOLD);
        let mut spans = vec![Span::raw(" ")];
        for (k, label) in view_keys(manager.view) {
            spans.push(Span::styled(k, key));
            spans.push(Span::styled(label, muted));
        }
        lines.push(Line::from(spans));
    }

    let border_style = Style::default().fg(theme.blue);
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_set(border::ROUNDED)
        .border_style(border_style)
        .title(title(
            manager.view,
            inner.saturating_sub(totals(manager, after, unknown).width() + 2),
            theme,
        ))
        .title(
            Line::from(Span::styled(
                totals(manager, after, unknown),
                Style::default().fg(theme.yellow),
            ))
            .right_aligned(),
        );
    if let Some(bottom) = bottom_line(manager, theme) {
        block = block.title_bottom(bottom);
    }
    f.render_widget(Clear, area);
    f.render_widget(Paragraph::new(lines).block(block), area);
    scroll
}

/// "Index · by folder │ biggest" with the active view bold; with less `room` beside
/// the totals, only the active one, then only "Index".
fn title(view: IndexView, room: usize, theme: &ThemeColors) -> Line<'static> {
    let blue = Style::default().fg(theme.blue).add_modifier(Modifier::BOLD);
    let muted = Style::default().fg(theme.light_gray);
    let active = match view {
        IndexView::ByFolder => "by folder",
        IndexView::Biggest => "biggest",
    };
    let short = format!(" Index · {active} ");
    if room < short.width() {
        return Line::styled(" Index ", blue);
    }
    if room < " Index · by folder │ biggest ".width() {
        return Line::styled(short, blue);
    }
    let style = |name: &str| if name == active { blue } else { muted };
    Line::from(vec![
        Span::styled(" Index · ", blue),
        Span::styled("by folder", style("by folder")),
        Span::styled(" │ ", Style::default().fg(theme.gray)),
        Span::styled("biggest", style("biggest")),
        Span::raw(" "),
    ])
}

/// The keys of `view`, for the hint row and the in-window key line.
pub fn view_keys(view: IndexView) -> [(&'static str, &'static str); 6] {
    match view {
        // Tab last: a narrow window cuts it before save and cancel.
        IndexView::ByFolder => [
            ("␣", " on/off  "),
            ("l", " open  "),
            ("h", " back  "),
            ("⏎", " save  "),
            ("Esc", " cancel  "),
            ("Tab", " biggest"),
        ],
        IndexView::Biggest => [
            ("␣", " on/off  "),
            ("l", " open  "),
            ("h", " fold  "),
            ("⏎", " save  "),
            ("Esc", " cancel  "),
            ("Tab", " by folder"),
        ],
    }
}

/// Deepest indent drawn; deeper lines share it.
const MAX_DEPTH: usize = 8;

/// One line of the tree: indent, ▾/▸, on/off/partly, the name (a joined chain's folders
/// above it dimmed), count, then "off" or its share; an open branch's bar is dimmed, so
/// colour lands on the folders to act on. "… N smaller" lines are all dim.
#[allow(clippy::too_many_arguments)]
fn tree_line(
    manager: &IndexManager,
    node: &TreeNode,
    smaller_after: usize,
    open: bool,
    highlighted: bool,
    after: usize,
    inner: usize,
    wide: bool,
    theme: &ThemeColors,
) -> Line<'static> {
    let base = if highlighted {
        Style::default().bg(theme.selection)
    } else {
        Style::default()
    };
    let per_level = if wide { 2 } else { 1 };
    let indent = " ".repeat(node.depth.min(MAX_DEPTH) * per_level);
    let share_width = if wide { BAR_WIDTH + 6 } else { 5 };
    let name_width = inner
        .saturating_sub(3 + indent.len() + 2 + 2 + 9 + share_width)
        .max(6);
    let cursor = Span::styled(
        if highlighted { " ❯ " } else { "   " },
        base.fg(theme.blue).add_modifier(Modifier::BOLD),
    );

    if let TreeKind::Smaller { .. } = node.kind {
        let dim = base.fg(theme.gray);
        let label = truncate(&format!("… {}", node.row.name), name_width + 4);
        let count = smaller_after;
        let percent = count as f64 * 100.0 / after.max(1) as f64;
        let mut spans = vec![
            cursor,
            Span::styled(indent, base),
            Span::styled(
                format!(
                    "{label}{}",
                    " ".repeat((name_width + 4).saturating_sub(label.width()))
                ),
                dim,
            ),
            Span::styled(format!("{:>7}  ", format_thousands(count as u64)), dim),
        ];
        if wide {
            spans.push(Span::styled(format!("{} ", bar(percent)), dim));
        }
        spans.push(Span::styled(format!("{:>4}", percent_label(percent)), dim));
        return pad(spans, inner, base);
    }

    let row = &node.row;
    let off = manager.tree_off(row);
    let off_inside = manager.off_inside(row);
    let has_big = matches!(node.kind, TreeKind::Folder { has_big: true, .. });
    let arrow = if open && !off {
        "▾ "
    } else if has_big {
        "▸ "
    } else {
        "  "
    };
    let (mark, mark_color) = if off {
        ("○ ", theme.light_gray)
    } else if off_inside > 0 {
        ("◐ ", theme.yellow)
    } else {
        ("● ", theme.green)
    };
    let text = if off {
        theme.light_gray
    } else if highlighted {
        theme.bright
    } else {
        theme.fg
    };
    // A long chain loses its middle first, then the name its end.
    let mut prefix = node.prefix.clone();
    if prefix.width() + row.name.width() > name_width {
        let parts: Vec<&str> = node.prefix.trim_end_matches(" / ").split(" / ").collect();
        if parts.len() > 1 {
            prefix = format!("{} / … / ", parts[0]);
        }
    }
    if prefix.width() + row.name.width() > name_width {
        prefix.clear();
    }
    let name = truncate(&row.name, name_width.saturating_sub(prefix.width()));
    let used = prefix.width() + name.width();
    let name_style = if highlighted || open {
        base.fg(text).add_modifier(Modifier::BOLD)
    } else {
        base.fg(text)
    };
    let mut spans = vec![
        cursor,
        Span::styled(indent, base),
        Span::styled(arrow, base.fg(theme.light_gray)),
        Span::styled(mark, base.fg(mark_color)),
        Span::styled(prefix, base.fg(theme.light_gray)),
        Span::styled(name, name_style),
        Span::styled(" ".repeat(name_width.saturating_sub(used)), base),
    ];
    // The count as it is, like the by-folder view; the share is after the changes.
    spans.push(Span::styled(
        format!("{:>7}  ", format_thousands(row.count.unwrap_or(0) as u64)),
        base.fg(text),
    ));
    let count = manager.count_after(row);
    if off {
        spans.push(Span::styled(
            "off",
            base.fg(theme.red).add_modifier(Modifier::BOLD),
        ));
    } else {
        let percent = count as f64 * 100.0 / after.max(1) as f64;
        let color = if open {
            theme.gray
        } else if percent >= 25.0 {
            theme.red
        } else if percent >= 5.0 {
            theme.yellow
        } else {
            theme.green
        };
        if wide {
            spans.push(Span::styled(format!("{} ", bar(percent)), base.fg(color)));
        }
        spans.push(Span::styled(
            format!("{:>4}", percent_label(percent)),
            base.fg(theme.light_gray),
        ));
    }
    pad(spans, inner, base)
}

/// `spans` padded with `base` to `inner` columns.
fn pad(mut spans: Vec<Span<'static>>, inner: usize, base: Style) -> Line<'static> {
    let used: usize = spans.iter().map(|span| span.content.width()).sum();
    spans.push(Span::styled(" ".repeat(inner.saturating_sub(used)), base));
    Line::from(spans)
}

/// One subfolder: the cursor, on/off/partly, name, count, then "off" or its share.
fn row_line(
    manager: &IndexManager,
    row: &IndexRow,
    highlighted: bool,
    after: usize,
    inner: usize,
    wide: bool,
    theme: &ThemeColors,
) -> Line<'static> {
    let off_inside = manager.off_inside(row);
    let base = if highlighted {
        Style::default().bg(theme.selection)
    } else {
        Style::default()
    };
    let off = manager.is_off(row);
    let (mark, mark_color) = if off {
        ("○", theme.light_gray)
    } else if off_inside > 0 {
        ("◐", theme.yellow)
    } else {
        ("●", theme.green)
    };
    let text = if off {
        theme.light_gray
    } else if highlighted {
        theme.bright
    } else {
        theme.fg
    };
    let note = if off_inside > 0 && !off {
        format!("  {off_inside} off inside")
    } else {
        String::new()
    };
    let share = if wide { BAR_WIDTH + 6 } else { 5 };
    let name_width = inner
        .saturating_sub(2 + 2 + 9 + 2 + share + note.width())
        .max(6);
    let name = truncate(&row.name, name_width);
    let count = row
        .count
        .map_or("—".to_string(), |count| format_thousands(count as u64));

    let mut spans = vec![
        Span::styled(
            if highlighted { " ❯ " } else { "   " },
            base.fg(theme.blue).add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("{mark} "), base.fg(mark_color)),
        Span::styled(
            // Padded by display width, so wide and combining characters line up.
            format!(
                "{name}{}",
                " ".repeat(name_width.saturating_sub(name.width()))
            ),
            if highlighted {
                base.fg(text).add_modifier(Modifier::BOLD)
            } else {
                base.fg(text)
            },
        ),
        Span::styled(format!("{count:>8}  "), base.fg(text)),
    ];
    if off {
        spans.push(Span::styled(
            "off",
            base.fg(theme.red).add_modifier(Modifier::BOLD),
        ));
    } else {
        let percent = manager.count_after(row) as f64 * 100.0 / after.max(1) as f64;
        let color = if percent >= 25.0 {
            theme.red
        } else if percent >= 5.0 {
            theme.yellow
        } else {
            theme.green
        };
        if wide {
            spans.push(Span::styled(bar(percent), base.fg(color)));
            spans.push(Span::styled(" ", base));
        }
        spans.push(Span::styled(
            format!("{:>4}", percent_label(percent)),
            base.fg(theme.light_gray),
        ));
        spans.push(Span::styled(note, base.fg(theme.yellow)));
    }
    let used: usize = spans.iter().map(|span| span.content.width()).sum();
    spans.push(Span::styled(" ".repeat(inner.saturating_sub(used)), base));
    Line::from(spans)
}

/// "~ / Downloads / Apps" for the folder shown; "/" for the filesystem root.
fn crumb(folder: &Path) -> String {
    let crumb = replace_home_with_tilde(&folder.to_string_lossy())
        .split('/')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" / ");
    if crumb.is_empty() {
        "/".to_string()
    } else {
        crumb
    }
}

/// Bars sit in the lower part of the cell (▆, ▖), so bars on rows next to each other
/// stay apart in terminals that draw lines with no gap.
/// `BAR_WIDTH` columns, filled in proportion to `percent`; a sliver for a small share.
fn bar(percent: f64) -> String {
    let filled = ((percent / 100.0) * BAR_WIDTH as f64).round() as usize;
    let filled = filled.min(BAR_WIDTH);
    let bar = if filled == 0 {
        "▖".to_string()
    } else {
        "▆".repeat(filled)
    };
    format!("{bar:<BAR_WIDTH$}")
}

/// "41%", or "<1%" rather than a misleading "0%".
fn percent_label(percent: f64) -> String {
    if percent > 0.0 && percent < 1.0 {
        "<1%".to_string()
    } else {
        format!("{}%", percent.round() as u64)
    }
}

/// "36,715 folders", or "36,715 → 15,978 folders (−56%)" with changes to save; "≥"
/// when a folder switched back on has no known count.
fn totals(manager: &IndexManager, after: usize, unknown: bool) -> String {
    let total = format_thousands(manager.total as u64);
    if manager.unsaved() == 0 {
        return format!(" {total} folders ");
    }
    if unknown {
        return format!(" {total} → ≥{} folders ", format_thousands(after as u64));
    }
    let change = (after as f64 - manager.total as f64) * 100.0 / manager.total.max(1) as f64;
    let sign = if change < 0.0 { "−" } else { "+" };
    format!(
        " {total} → {} folders ({sign}{}%) ",
        format_thousands(after as u64),
        change.abs().round() as u64
    )
}

/// What the index never holds here, for the footer line.
fn never_indexed(manager: &IndexManager) -> String {
    let mut parts = Vec::new();
    if manager.at_home {
        parts.push("Library".to_string());
    }
    if manager.hidden_here > 0 {
        parts.push(plural(manager.hidden_here as u64, "hidden folder"));
    }
    parts.push("ignore_directories".to_string());
    format!("Never indexed: {}", parts.join(", "))
}

/// The bottom border: why saving failed, the discard prompt, or the unsaved count.
fn bottom_line(manager: &IndexManager, theme: &ThemeColors) -> Option<Line<'static>> {
    let key = Style::default()
        .fg(theme.bright)
        .add_modifier(Modifier::BOLD);
    let muted = Style::default().fg(theme.light_gray);
    let yellow = Style::default().fg(theme.yellow);
    if manager.error.is_some() {
        // The reason is in the message row, where it has room.
        return Some(Line::styled(
            " Couldn't save: see below ",
            Style::default().fg(theme.red),
        ));
    }
    let unsaved = manager.unsaved();
    if unsaved == 0 {
        return None;
    }
    let count = format!(" {} ", plural(unsaved as u64, "unsaved change"));
    if manager.confirm_discard {
        return Some(Line::from(vec![
            Span::styled(format!("{}· ", count), yellow),
            Span::styled("⏎", key),
            Span::styled(" save · ", muted),
            Span::styled("Esc", key),
            Span::styled(" again to discard ", muted),
        ]));
    }
    Some(Line::styled(count, yellow))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::directory_store::{DirectoryIndex, DirectoryStore, IndexRules};
    use ratatui::{backend::TestBackend, Terminal};

    fn screen(manager: &IndexManager, width: u16) -> Vec<String> {
        let mut terminal = Terminal::new(TestBackend::new(width, 20)).unwrap();
        let theme = crate::config::Theme::default().to_colors().unwrap();
        terminal
            .draw(|f| {
                render_index_manager(f, manager, false, &theme);
            })
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        buffer
            .content()
            .chunks(usize::from(width))
            .map(|row| row.iter().map(|cell| cell.symbol()).collect())
            .collect()
    }

    fn manager() -> (IndexManager, DirectoryIndex) {
        let root = Path::new("/h");
        let mut store = DirectoryStore {
            directories: [
                "work",
                "work/a",
                "work/b",
                "music",
                "Downloads",
                "Downloads/x",
            ]
            .iter()
            .map(|dir| root.join(dir).to_string_lossy().into_owned())
            .collect(),
            root: Some("/h".into()),
            ..Default::default()
        };
        store.excluded.insert("/h/Movies".into(), 12_000);
        let paths = ["/h/Movies".to_string(), "/h/Downloads/big.app".to_string()];
        let index = DirectoryIndex::new(
            store,
            root.to_path_buf(),
            IndexRules::new(&[], &paths, root),
            None,
        );
        (IndexManager::open(&index, false), index)
    }

    #[test]
    fn the_window_lists_the_folders_with_their_state_counts_and_shares() {
        let (mut manager, _) = manager();
        let rows = screen(&manager, 80);
        let text = rows.join("\n");
        assert!(
            rows[2].contains("Index · by folder") && rows[2].contains("6 folders"),
            "{text}"
        );
        assert!(rows[3].contains("folders   of index"), "{text}");
        assert!(
            rows[4].contains("❯ ○ Movies") && rows[4].contains("12,000  off"),
            "{text}"
        );
        assert!(
            rows[5].contains("● work") && rows[5].contains("50%"),
            "{text}"
        );
        assert!(
            rows[6].contains("◐ Downloads") && rows[6].contains("1 off inside"),
            "{text}"
        );
        assert!(text.contains("Never indexed:"), "{text}");

        // Switching work off: totals in the title, the change in the bottom border.
        manager.selected = 1;
        manager.toggle();
        let rows = screen(&manager, 80);
        let text = rows.join("\n");
        assert!(rows[2].contains("6 → 3 folders (−50%)"), "{text}");
        assert!(
            rows[5].contains("○ work") && rows[5].contains("off"),
            "{text}"
        );
        assert!(text.contains("1 unsaved change "), "{text}");
        manager.confirm_discard = true;
        assert!(screen(&manager, 80)
            .join("\n")
            .contains("⏎ save · Esc again to discard"));

        // Narrow: no bar, but still the count and share.
        let rows = screen(&manager, 50);
        assert!(
            rows[6].contains("◐ Downloads") && !rows[6].contains('▆'),
            "{}",
            rows.join("\n")
        );
    }

    #[test]
    fn wide_names_keep_the_columns_lined_up_and_tiny_screens_draw() {
        let root = Path::new("/h");
        let store = DirectoryStore {
            directories: ["日本語のフォルダ", "plain"]
                .iter()
                .map(|dir| root.join(dir).to_string_lossy().into_owned())
                .collect(),
            root: Some("/h".into()),
            ..Default::default()
        };
        let index = DirectoryIndex::new(store, root.to_path_buf(), IndexRules::default(), None);
        let manager = IndexManager::open(&index, false);
        let rows = screen(&manager, 80);
        // Cells, not display width: the test screen stores a wide character as two.
        let column = |row: &str| {
            let at = row.find("50%").unwrap();
            row[..at].chars().count()
        };
        let wide = rows.iter().find(|row| row.contains('日')).unwrap();
        let plain = rows.iter().find(|row| row.contains("plain")).unwrap();
        assert_eq!(column(wide), column(plain), "{wide}\n{plain}");

        for (width, height) in [(20, 6), (1, 1), (0, 0)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            let theme = crate::config::Theme::default().to_colors().unwrap();
            terminal
                .draw(|f| {
                    render_index_manager(f, &manager, true, &theme);
                })
                .unwrap();
        }
    }

    #[test]
    fn the_tree_view_draws_its_branches_and_the_highlighted_path() {
        let root = Path::new("/h");
        let mut directories: Vec<String> = [
            "media",
            "media/app",
            "media/app/cache",
            "media/app/x",
            "music",
        ]
        .iter()
        .map(|dir| format!("/h/{dir}"))
        .collect();
        directories.extend((0..60).map(|i| format!("/h/media/app/cache/f{i}")));
        let store = DirectoryStore {
            directories,
            root: Some("/h".into()),
            ..Default::default()
        };
        let index = DirectoryIndex::new(store, root.to_path_buf(), IndexRules::default(), None);
        let mut manager = IndexManager::open(&index, false);
        manager.switch_view(&index);
        let rows = screen(&manager, 80);
        let text = rows.join("\n");
        assert!(rows[2].contains("Index · by folder │ biggest"), "{text}");
        assert!(rows[3].contains("heaviest branches"), "{text}");
        assert!(
            rows[4].contains("❯ ▾ ● media / app") && rows[4].contains("63"),
            "{text}"
        );
        assert!(
            rows[5].contains("● cache") && rows[5].contains("61"),
            "{text}"
        );
        assert!(text.contains("/h/media/app"), "{text}");

        manager.toggle();
        let text = screen(&manager, 80).join("\n");
        assert!(
            text.contains("▸ ○ media / app") && text.contains("off"),
            "{text}"
        );
        assert!(!text.contains("cache"), "{text}");
    }
}
