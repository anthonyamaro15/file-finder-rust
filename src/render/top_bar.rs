//! The top bar: the active pane's folder, how it's listed and what's running; or the
//! search typed there, and its query while its results are shown. In dual pane it's a
//! path header over each pane.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use ratatui::{
    prelude::*,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use unicode_width::UnicodeWidthStr;

use crate::{
    app::{App, InputMode},
    info::plural,
    ui::{truncate, truncate_start},
    utils::{
        files::{SortBy, SortType},
        format::replace_home_with_tilde,
    },
};

/// Between the parts at the right.
const SEPARATOR: &str = "  ·  ";

/// Draw the top bar on `area`'s first row, its sizing spinner turned for `now`. While
/// a search is typed the bar takes its tint. In dual pane (`split` is the column
/// between the panes) each pane gets a path header: the active one's marked ● in the
/// mode's color, with what runs at its right; the other's ○ and muted.
pub fn render(f: &mut Frame, area: Rect, app: &App, now: SystemTime, split: Option<u16>) {
    let area = Rect {
        height: area.height.min(1),
        ..area
    };
    let Some(split) = split.filter(|&x| x > area.x && x < area.right()) else {
        return bar(f, area, app, now, None);
    };
    let theme = &app.theme_colors;
    let left = Rect {
        width: split - area.x,
        ..area
    };
    let right = Rect {
        x: split + 1,
        width: area.right() - split - 1,
        ..area
    };
    let (active, other, other_dir) = if app.is_right_pane_active() {
        (right, left, app.left_directory_path())
    } else {
        (
            left,
            right,
            PathBuf::from(&app.right_pane.current_directory),
        )
    };
    bar(
        f,
        active,
        app,
        now,
        Some(Span::styled("● ", Style::default().fg(app.mode_color()))),
    );

    let gray = Style::default().fg(theme.gray);
    let room = usize::from(other.width).saturating_sub(4);
    let mut header = vec![Span::raw(" "), Span::styled("○ ", gray)];
    header.extend(path(
        &other_dir,
        room,
        gray,
        Style::default().fg(theme.light_gray),
    ));
    let bar_style = Style::default().bg(theme.statusbar_bg);
    f.render_widget(Paragraph::new(Line::from(header)).style(bar_style), other);
    let divider = Span::styled("│", Style::default().fg(theme.dim));
    f.render_widget(
        Paragraph::new(divider).style(bar_style),
        Rect {
            x: split,
            width: 1,
            ..area
        },
    );
}

/// Draw the bar on `area`: the folder or search at the left, after `marker` in dual
/// pane, and at the right what runs, then how the folder is listed or the search's
/// count. In dual pane there's no room for how it's listed.
fn bar(f: &mut Frame, area: Rect, app: &App, now: SystemTime, marker: Option<Span<'static>>) {
    let theme = &app.theme_colors;
    let (query, global) = app.search_box();
    let typing = app.input_mode == InputMode::Editing;
    let searching = typing || !query.is_empty();
    let bg = match (typing, global) {
        (true, false) => theme.search_bg,
        (true, true) => theme.global_bg,
        _ => theme.statusbar_bg,
    };
    let muted = Style::default().fg(theme.light_gray);
    let separator = Span::styled(SEPARATOR, Style::default().fg(theme.gray));

    // What runs comes first at the right.
    let mut right = Vec::new();
    for status in running(app, now) {
        right.push(Span::styled(status, Style::default().fg(theme.yellow)));
        right.push(separator.clone());
    }
    if searching {
        let color = match (typing, global) {
            (false, _) => theme.light_gray,
            (true, false) => theme.mode_search,
            (true, true) => theme.mode_global,
        };
        right.push(Span::styled(
            count(app, query, global),
            Style::default().fg(color),
        ));
    } else if marker.is_none() {
        for (i, part) in listing(app).into_iter().enumerate() {
            if i > 0 {
                right.push(separator.clone());
            }
            right.push(Span::styled(part, muted));
        }
    } else {
        // Nothing listed after what runs.
        right.pop();
    }
    right.push(Span::raw(" "));
    let right = Line::from(right);

    // The left part gets what the right leaves, keeping a gap.
    let marker_width = marker.as_ref().map_or(0, Span::width);
    let room = usize::from(area.width).saturating_sub(right.width() + 3 + marker_width);
    let mut left = vec![Span::raw(" ")];
    left.extend(marker);
    left.extend(if searching {
        prompt(app, query, global, typing)
    } else {
        let name = Style::default()
            .fg(theme.bright)
            .add_modifier(Modifier::BOLD);
        path(&app.current_directory_path(), room, muted, name)
    });

    let bar = Style::default().bg(bg);
    let left_area = Rect {
        width: u16::try_from(room + 1 + marker_width)
            .map_or(area.width, |width| width.min(area.width)),
        ..area
    };
    f.render_widget(
        Paragraph::new(right).alignment(Alignment::Right).style(bar),
        area,
    );
    f.render_widget(Paragraph::new(Line::from(left)).style(bar), left_area);
}

/// Whether the bar is about the right pane: it's active in dual pane.
fn right_pane(app: &App) -> bool {
    app.view_mode.is_dual_pane() && app.is_right_pane_active()
}

/// What runs: a copy's, delete's or other job's progress, or a reload; then folder
/// sizing's.
fn running(app: &App, now: SystemTime) -> Vec<String> {
    let progress = |message: &str, preparing: &str| {
        if message.is_empty() {
            preparing.to_string()
        } else {
            message.to_string()
        }
    };
    let operation = if app.copy_in_progress {
        Some(progress(&app.copy_progress_message, "Preparing copy…"))
    } else if app.delete_in_progress {
        Some(progress(&app.delete_progress_message, "Preparing delete…"))
    } else if let Some(job) = &app.job {
        Some(job.status.clone())
    } else {
        app.loading.then(|| "Loading…".to_string())
    };
    operation
        .into_iter()
        .chain(app.active_sizing().progress_text(now))
        .collect()
}

/// The active pane's order, whether it shows hidden files and how many items it lists:
/// "name ↑", "hidden off", "36 items".
fn listing(app: &App) -> [String; 3] {
    let pane = &app.right_pane;
    let (sort_by, sort_type, hidden, items) = if right_pane(app) {
        (
            &pane.sort_by,
            &pane.sort_type,
            pane.show_hidden_files,
            pane.files.len(),
        )
    } else {
        (
            &app.sort_by,
            &app.sort_type,
            app.show_hidden_files,
            app.files.len(),
        )
    };
    let arrow = match sort_type {
        SortType::ASC => "↑",
        SortType::DESC => "↓",
    };
    let order = match sort_by {
        SortBy::Name => format!("name {arrow}"),
        SortBy::Size => format!("size {arrow}"),
        SortBy::DateAdded => format!("date {arrow}"),
        SortBy::Default => "default order".to_string(),
    };
    let hidden = if hidden { "hidden on" } else { "hidden off" };
    [order, hidden.to_string(), plural(items as u64, "item")]
}

/// "4 of 36" for a search of the folder, "6 folders" for one from the root.
fn count(app: &App, query: &str, global: bool) -> String {
    let (results, total) = if right_pane(app) {
        (
            app.right_pane.search_results.len(),
            app.right_pane.files.len(),
        )
    } else {
        (app.search_results.len(), app.files.len())
    };
    if global {
        plural(results as u64, "folder")
    } else if query.is_empty() {
        format!("{total} of {total}")
    } else {
        format!("{results} of {total}")
    }
}

/// The folder `dir`, `~` for home: its parent in `parent` and its name in `name`, the
/// parent cut from the start to fit `room` columns.
fn path(dir: &Path, room: usize, parent: Style, name: Style) -> Vec<Span<'static>> {
    let dir = replace_home_with_tilde(&dir.to_string_lossy());
    let split = match dir.rfind('/') {
        Some(i) if i + 1 < dir.len() => i + 1,
        _ => 0,
    };
    let (parent_text, name_text) = dir.split_at(split);
    let name_text = truncate(name_text, room);
    let parent_text = if room > name_text.width() {
        truncate_start(parent_text, room - name_text.width())
    } else {
        String::new()
    };
    vec![
        Span::styled(parent_text, parent),
        Span::styled(name_text, name),
    ]
}

/// The search's prompt, "/" or "␣/" for one from the root, and its query: bright with
/// a cursor in the search's color while it's typed, quieter while its results are
/// browsed.
fn prompt(app: &App, query: &str, global: bool, typing: bool) -> Vec<Span<'static>> {
    let theme = &app.theme_colors;
    let (glyph, color) = if global {
        ("␣/", theme.mode_global)
    } else {
        ("/", theme.mode_search)
    };
    if !typing {
        return vec![
            Span::styled(glyph, Style::default().fg(color)),
            Span::raw(" "),
            Span::styled(query.to_string(), Style::default().fg(theme.fg)),
        ];
    }
    let cursor = app.character_index;
    let at = query.chars().nth(cursor).unwrap_or(' ');
    let bright = Style::default().fg(theme.bright);
    vec![
        Span::styled(
            glyph,
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(query.chars().take(cursor).collect::<String>(), bright),
        Span::styled(at.to_string(), Style::default().fg(theme.black).bg(color)),
        Span::styled(query.chars().skip(cursor + 1).collect::<String>(), bright),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{BackgroundJob, SearchResult};
    use crate::utils::files::{EntryInfo, EntryKind, Listing};
    use ratatui::{backend::TestBackend, buffer::Buffer, Terminal};
    use std::time::{Duration, UNIX_EPOCH};

    /// A moment the sizing spinner shows ⠼ at.
    fn now() -> SystemTime {
        UNIX_EPOCH + Duration::from_millis(400)
    }

    /// ~/Desktop/work/projects, listing 36 items in name order.
    fn projects() -> App {
        let home = dirs::home_dir().unwrap();
        let dir = home.join("Desktop/work/projects");
        let files: Vec<String> = (0..36)
            .map(|i| {
                dir.join(format!("project-{i:02}"))
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        let info = vec![
            EntryInfo {
                kind: Some(EntryKind::Folder),
                ..EntryInfo::default()
            };
            36
        ];
        let mut app = App::with_listing(Listing { files, info });
        app.current_directory = dir.to_string_lossy().into_owned();
        app.sort_by = SortBy::Name;
        app.sort_type = SortType::ASC;
        app
    }

    fn draw(app: &App, width: u16) -> Buffer {
        let mut terminal = Terminal::new(TestBackend::new(width, 1)).unwrap();
        terminal
            .draw(|f| render(f, f.size(), app, now(), None))
            .unwrap();
        terminal.backend().buffer().clone()
    }

    fn text(buffer: &Buffer) -> String {
        buffer.content().iter().map(|cell| cell.symbol()).collect()
    }

    /// The cell `offset` columns into the first `needle` on the bar.
    fn cell<'a>(buffer: &'a Buffer, needle: &str, offset: usize) -> &'a ratatui::buffer::Cell {
        let text = text(buffer);
        let x = text[..text.find(needle).unwrap()].chars().count() + offset;
        &buffer.content()[x]
    }

    #[test]
    fn the_bar_shows_the_folder_its_order_and_its_item_count() {
        let mut app = projects();
        let theme = app.theme_colors.clone();
        let buffer = draw(&app, 100);
        let bar = text(&buffer);
        assert!(bar.starts_with(" ~/Desktop/work/projects "), "{bar}");
        assert!(
            bar.ends_with("name ↑  ·  hidden off  ·  36 items "),
            "{bar}"
        );
        let parent = cell(&buffer, "~/", 0);
        assert_eq!(
            (parent.fg, parent.bg),
            (theme.light_gray, theme.statusbar_bg)
        );
        let name = cell(&buffer, "projects", 0);
        assert_eq!(name.fg, theme.bright);
        assert!(name.modifier.contains(Modifier::BOLD));
        assert_eq!(cell(&buffer, "·", 0).fg, theme.gray);

        // Too narrow for the whole path: the parent is cut from the start.
        let bar = text(&draw(&app, 50));
        assert!(bar.starts_with(" …rk/projects  name"), "{bar}");

        app.sort_type = SortType::DESC;
        app.show_hidden_files = true;
        let bar = text(&draw(&app, 100));
        assert!(bar.ends_with("name ↓  ·  hidden on  ·  36 items "), "{bar}");
    }

    #[test]
    fn what_runs_comes_first_at_the_right() {
        let mut app = projects();
        app.job = Some(BackgroundJob::spawn(
            "move",
            "Moving 'notes'…".into(),
            Vec::new(),
            |_| Ok("moved".into()),
        ));
        app.sizing.progress = Some((3, 5));
        let buffer = draw(&app, 120);
        let bar = text(&buffer);
        assert!(
            bar.ends_with(
                "Moving 'notes'…  ·  ⠼ 3 of 5 folders sized  ·  name ↑  ·  hidden off  ·  36 items "
            ),
            "{bar}"
        );
        assert_eq!(cell(&buffer, "Moving", 0).fg, app.theme_colors.yellow);

        app.job = None;
        app.sizing.progress = None;
        app.copy_in_progress = true;
        assert!(text(&draw(&app, 120)).contains("Preparing copy…  ·  name ↑"));
    }

    #[test]
    fn a_search_of_the_folder_is_typed_in_the_bar_in_its_color() {
        let mut app = projects();
        let theme = app.theme_colors.clone();
        app.input_mode = InputMode::Editing;
        app.input = "-0".into();
        app.character_index = 2;
        app.perform_local_search("-0");
        let matches = app.search_results.len();
        assert!(matches > 0 && matches < 36);

        let buffer = draw(&app, 100);
        let bar = text(&buffer);
        assert!(bar.starts_with(" / -0  "), "{bar}");
        assert!(bar.ends_with(&format!("{matches} of 36 ")), "{bar}");
        let slash = cell(&buffer, "/", 0);
        assert_eq!((slash.fg, slash.bg), (theme.mode_search, theme.search_bg));
        assert!(slash.modifier.contains(Modifier::BOLD));
        assert_eq!(cell(&buffer, "-0", 0).fg, theme.bright);
        // The cursor, after the query.
        assert_eq!(cell(&buffer, "-0", 2).bg, theme.mode_search);
        assert_eq!(cell(&buffer, " of ", 0).fg, theme.mode_search);
        assert_eq!(buffer.content()[99].bg, theme.search_bg);

        // Moved back over the query, the cursor sits on its letter.
        app.character_index = 1;
        let cursor = cell(&draw(&app, 100), "-0", 1).clone();
        assert_eq!(
            (cursor.symbol(), cursor.fg, cursor.bg),
            ("0", theme.black, theme.mode_search)
        );

        // After Esc the results stay, and so do the query and count, quieter.
        app.input_mode = InputMode::Normal;
        let buffer = draw(&app, 100);
        let bar = text(&buffer);
        assert!(bar.starts_with(" / -0 "), "{bar}");
        assert!(bar.ends_with(&format!("{matches} of 36 ")), "{bar}");
        let slash = cell(&buffer, "/", 0);
        assert_eq!(
            (slash.fg, slash.bg),
            (theme.mode_search, theme.statusbar_bg)
        );
        assert!(!slash.modifier.contains(Modifier::BOLD));
        assert_eq!(cell(&buffer, "-0", 2).bg, theme.statusbar_bg);
        assert_eq!(cell(&buffer, " of ", 0).fg, theme.light_gray);
    }

    #[test]
    fn a_search_from_the_root_is_typed_in_the_bar_in_its_color() {
        let mut app = projects();
        let theme = app.theme_colors.clone();
        app.input_mode = InputMode::Editing;
        app.global_search_mode = true;
        app.input = "pipe".into();
        app.character_index = 4;
        app.search_results = (0..6)
            .map(|i| SearchResult {
                file_path: format!("/w/pipe-{i}"),
                display_name: format!("pipe-{i}"),
                score: 1,
                info: EntryInfo::default(),
                original_index: 0,
            })
            .collect();

        let buffer = draw(&app, 100);
        let bar = text(&buffer);
        assert!(bar.starts_with(" ␣/ pipe  "), "{bar}");
        assert!(bar.ends_with("6 folders "), "{bar}");
        let prompt = cell(&buffer, "␣/", 0);
        assert_eq!((prompt.fg, prompt.bg), (theme.mode_global, theme.global_bg));
        assert_eq!(cell(&buffer, "pipe", 4).bg, theme.mode_global);
        assert_eq!(cell(&buffer, "6 folders", 0).fg, theme.mode_global);

        app.input_mode = InputMode::Normal;
        let buffer = draw(&app, 100);
        assert!(text(&buffer).starts_with(" ␣/ pipe "));
        assert_eq!(cell(&buffer, "6 folders", 0).fg, theme.light_gray);
        assert_eq!(buffer.content()[0].bg, theme.statusbar_bg);
    }
}
