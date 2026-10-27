use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols::border,
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Padding},
    Frame,
};
use std::{ops::RangeInclusive, path::Path};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::app::{App, InputMode, SearchResult, ViewMode};
use crate::config::{Settings, ThemeColors};
use crate::highlight::highlight_search_term;
use crate::icons::IconProvider;
use crate::render::size_bar::{size_column, BAR_WIDTH, SIZE_WIDTH};
use crate::sizes::{FolderSizes, FolderTotal, RowSize};
use crate::utils::files::{EntryInfo, SortBy};
use crate::utils::format::{date_label, replace_home_with_tilde, LocalStamp};

/// Said in place of a global search's results while the folder index is empty until
/// its first walk finishes.
pub const INDEX_FILLING: &str = "Building the folder index…";

/// The row saying the folder index is still being built.
fn filling_row(theme: &ThemeColors) -> ListItem<'static> {
    ListItem::new(Span::styled(
        INDEX_FILLING,
        Style::default().fg(theme.light_gray),
    ))
}

/// The preview's block: a box with borders, else a dim divider on its left and a
/// column of padding each side, like the list's.
pub fn create_preview_block(settings: &Settings, theme: &ThemeColors) -> Block<'static> {
    if settings.show_borders {
        Block::default()
            .borders(Borders::ALL)
            .border_set(border::ROUNDED)
    } else {
        Block::default()
            .borders(Borders::LEFT)
            .border_style(Style::default().fg(theme.dim))
            .padding(Padding::horizontal(1))
    }
}

#[derive(Debug, Clone)]
pub struct ListFileItem {
    pub label: String,
    pub path: String,
}
#[derive(Debug, Clone)]
pub struct FileListContent {
    pub items: Vec<ListFileItem>,
}

#[derive(Debug, Clone)]
pub struct Ui {
    pub files_list: FileListContent,
    pub icon_provider: IconProvider,
}

impl Ui {
    pub fn new(files: Vec<String>, settings: &Settings) -> Self {
        let mut current_item_list: Vec<ListFileItem> = Vec::new();
        for path in files.iter() {
            let new_path = Path::new(path);
            // Safely extract file name, skip if path is invalid
            let get_file_name = new_path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_else(|| path.rsplit('/').next().unwrap_or(path))
                .to_string();
            let create_item_list = ListFileItem {
                label: get_file_name,
                path: String::from(path),
            };

            current_item_list.push(create_item_list);
        }

        //const item = ListFileItem
        let file_list = FileListContent {
            items: current_item_list,
        };

        let icon_provider = IconProvider::new(&settings.use_nerd_fonts);

        Self {
            files_list: file_list,
            icon_provider,
        }
    }

    /// Draw the left listing on the left part of `area`, and return its rows' area and
    /// how many rows it has, for its scrollbar.
    pub fn render_list_preview(
        &mut self,
        f: &mut Frame<'_>,
        area: Rect,
        state: &mut ListState,
        app: &App,
        settings: &Settings,
        today: LocalStamp,
    ) -> (Rect, usize) {
        // Dynamic layout based on view mode
        let inner_constraints = match app.view_mode {
            ViewMode::FullList => vec![Constraint::Percentage(100)],
            _ => vec![Constraint::Percentage(50), Constraint::Percentage(50)],
        };
        let inner_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(inner_constraints)
            .split(area);

        // Create block with or without borders based on settings
        let theme = &app.theme_colors;
        let block = if settings.show_borders {
            Block::default()
                .borders(Borders::ALL)
                .border_set(border::ROUNDED)
                .style(match app.input_mode {
                    _ if app.searching_right_pane() => theme.inactive_border,
                    InputMode::Normal | InputMode::Visual => {
                        if app.view_mode == ViewMode::DualPane && app.is_right_pane_active() {
                            theme.inactive_border
                        } else {
                            theme.active_border
                        }
                    }
                    InputMode::Editing => Style::default()
                        .fg(app.mode_color())
                        .add_modifier(Modifier::BOLD),
                    _ => theme.inactive_border,
                })
        } else {
            // Modern mode: no borders, clean look
            Block::default()
                .borders(Borders::NONE)
                .padding(Padding::horizontal(1))
        };

        // Generate list items based on search mode, as wide as the list.
        let rows = block.inner(inner_layout[0]);
        let filtered_read_only_items =
            self.generate_list_items(app, settings, rows.width.into(), today, state.selected());
        let count = filtered_read_only_items.len();

        let list_block =
            List::new(filtered_read_only_items)
                .block(block)
                .style(match app.input_mode {
                    InputMode::Normal | InputMode::Visual | InputMode::Editing => theme.normal,
                    _ => theme.disabled,
                });

        f.render_stateful_widget(list_block, inner_layout[0], state);
        (rows, count)
    }

    /// Draw the folder preview's items on `area`, from its top, leaving out what `.`
    /// hides, as the list does.
    pub fn render_preview_window(
        &self,
        f: &mut Frame<'_>,
        area: Rect,
        app: &App,
        settings: &Settings,
        today: LocalStamp,
    ) {
        let rows = Rows::new(self, app, settings, area.width.into(), today);
        let items: Vec<ListItem> = app
            .preview_items()
            .take(area.height.into())
            .enumerate()
            .map(|(i, (path, info))| {
                let name = Path::new(path)
                    .file_name()
                    .map_or_else(|| path.into(), |name| name.to_string_lossy());
                rows.row(i, path, &name, info, None)
            })
            .collect();
        f.render_widget(List::new(items).style(app.theme_colors.normal), area);
    }

    /// Draw the visual selection's items on `area`, what `d` and `D` act on, ending
    /// with "and N more" when they don't all fit.
    pub fn render_selection(
        &self,
        f: &mut Frame<'_>,
        area: Rect,
        app: &App,
        settings: &Settings,
        today: LocalStamp,
    ) {
        let Some(selected) = app.visual_rows() else {
            return;
        };
        let total = selected.clone().count();
        let height = usize::from(area.height);
        let shown = if total > height {
            height.saturating_sub(1)
        } else {
            total
        };
        let rows = Rows::new(self, app, settings, area.width.into(), today);
        let mut items: Vec<ListItem> = selected
            .take(shown)
            .enumerate()
            .map(|(i, index)| {
                rows.row(
                    i,
                    &app.files[index],
                    &app.file_read_only_label_list[index],
                    &app.file_info[index],
                    None,
                )
            })
            .collect();
        if shown < total {
            items.push(ListItem::new(Span::styled(
                format!("and {} more", total - shown),
                Style::default().fg(app.theme_colors.light_gray),
            )));
        }
        f.render_widget(List::new(items).style(app.theme_colors.normal), area);
    }

    /// The left listing's rows, `width` columns wide with `cursor` highlighted: its
    /// items, or a global search's results.
    fn generate_list_items(
        &self,
        app: &App,
        settings: &Settings,
        width: usize,
        today: LocalStamp,
        cursor: Option<usize>,
    ) -> Vec<ListItem<'static>> {
        let term = highlight_term(app, false);
        // Only while typing, when no key acts on the rows it hides.
        if app.index_filling
            && app.global_search_mode
            && app.search_results.is_empty()
            && app.input_mode == InputMode::Editing
            && !app.searching_right_pane()
        {
            return vec![filling_row(&app.theme_colors)];
        }
        if app.global_search_mode && !app.search_results.is_empty() {
            let rows = Rows {
                term,
                ..Rows::new(self, app, settings, width, today)
            }
            .with_cursor(app, false, cursor);
            return app
                .search_results
                .iter()
                .take(50) // Limit to top 50 results for performance
                .enumerate()
                .map(|(i, result)| rows.result(i, result, true))
                .collect();
        }
        let size_order = app.sort_by == SortBy::Size
            && settings.show_size_bars
            && app.search_results.is_empty()
            && term.is_empty();
        let rows = Rows {
            term,
            size_order: size_order.then(|| {
                let largest = largest(&app.files, &app.file_info, &app.folder_sizes);
                (&app.folder_sizes, largest)
            }),
            ..Rows::new(self, app, settings, width, today)
        }
        .with_cursor(app, false, cursor);

        // Use all filtered indexes (or all files if no filter)
        let indexes: Vec<usize> = if !app.filtered_indexes.is_empty() {
            app.filtered_indexes.clone()
        } else {
            (0..app.files.len()).collect()
        };
        indexes
            .iter()
            .enumerate()
            .map(|(i, &index)| {
                rows.row(
                    i,
                    &app.files[index],
                    &app.file_read_only_label_list[index],
                    &app.file_info[index],
                    None,
                )
            })
            .collect()
    }

    /// The right pane's rows, `width` columns wide with its cursor highlighted: its
    /// listing, or its search's results, highlighted like the left listing's while the
    /// query is typed.
    pub fn right_pane_items(
        &self,
        app: &App,
        settings: &Settings,
        width: usize,
        today: LocalStamp,
    ) -> Vec<ListItem<'static>> {
        let pane = &app.right_pane;
        let size_order =
            pane.sort_by == SortBy::Size && settings.show_size_bars && !pane.is_searching();
        let rows = Rows {
            term: highlight_term(app, true),
            size_order: size_order.then(|| {
                let largest = largest(&pane.files, &pane.file_info, &app.folder_sizes);
                (&app.folder_sizes, largest)
            }),
            ..Rows::new(self, app, settings, width, today)
        }
        .with_cursor(app, true, pane.list_state.selected());
        if app.index_filling
            && app.searching_right_pane()
            && pane.global_search_mode
            && pane.search_results.is_empty()
        {
            vec![filling_row(&app.theme_colors)]
        } else if pane.is_searching() {
            pane.search_results
                .iter()
                .enumerate()
                .map(|(i, result)| rows.result(i, result, pane.global_search_mode))
                .collect()
        } else {
            pane.files
                .iter()
                .zip(&pane.file_labels)
                .zip(&pane.file_info)
                .enumerate()
                .map(|(i, ((path, label), info))| rows.row(i, path, label, info, None))
                .collect()
        }
    }
}

/// Draw a scrollbar's thumb in `color` on column `x` beside `rows`, for a list of
/// `total` rows scrolled down `offset`, when they don't all fit: as tall as the share
/// shown, and as far down as the scroll.
pub fn draw_scrollbar(
    buffer: &mut Buffer,
    x: u16,
    rows: Rect,
    total: usize,
    offset: usize,
    color: Color,
) {
    let height = usize::from(rows.height);
    if total <= height || x >= buffer.area.right() {
        return;
    }
    let thumb = ((height * height + total / 2) / total).clamp(2.min(height), height);
    let start = ((offset * height + total / 2) / total).min(height - thumb);
    for y in start..start + thumb {
        buffer
            .get_mut(x, rows.y + y as u16)
            .set_symbol("┃")
            .set_fg(color);
    }
}

/// Fewest columns a name keeps before its row gives up the date, the bar, then the size.
const MIN_NAME_WIDTH: usize = 12;
/// Most columns an icon and the space after it take.
const ICON_WIDTH: usize = 3;
/// Columns of a modified date, wide enough for "Oct 21".
const DATE_WIDTH: usize = 6;

/// The largest known size of a listing sorted by size, which the bars are relative to.
fn largest(files: &[String], info: &[EntryInfo], sizes: &FolderSizes) -> u64 {
    files
        .iter()
        .zip(info)
        .filter_map(|(file, info)| match sizes.row_size(file, info) {
            RowSize::Known(total) => Some(total.bytes),
            _ => None,
        })
        .max()
        .unwrap_or(0)
}

/// What every row of one list shares, so that the same item looks the same in any list.
struct Rows<'a> {
    icons: &'a IconProvider,
    theme: &'a ThemeColors,
    /// Columns beside the mark.
    width: usize,
    /// Show sizes, and bars in size order.
    sizes: bool,
    /// In size order, the folder totals and the largest size, which bars are relative to.
    size_order: Option<(&'a FolderSizes, u64)>,
    today: LocalStamp,
    /// The query highlighted in names.
    term: &'a str,
    /// The query's letters: bold and underlined in the search's color.
    hit: Style,
    /// The highlighted row. While there is one, every row keeps a column for a mark.
    cursor: Option<usize>,
    /// The cursor's mark color in the pane keys go to; the other pane's cursor is
    /// fainter and has none.
    mark: Option<Color>,
    /// The visual selection's rows, in the pane keys go to.
    selection: Option<RangeInclusive<usize>>,
}

impl<'a> Rows<'a> {
    /// Rows `width` columns wide outside size order, with nothing highlighted.
    fn new(ui: &'a Ui, app: &'a App, settings: &Settings, width: usize, today: LocalStamp) -> Self {
        Self {
            icons: &ui.icon_provider,
            theme: &app.theme_colors,
            width,
            sizes: settings.show_size_bars,
            size_order: None,
            today,
            term: "",
            hit: Style::default()
                .fg(app.mode_color())
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            cursor: None,
            mark: None,
            selection: None,
        }
    }

    /// These rows with `cursor` highlighted, in the right pane's list when `right_pane`.
    /// In the pane keys go to, it's marked in the mode's color and the selection shows.
    fn with_cursor(self, app: &App, right_pane: bool, cursor: Option<usize>) -> Self {
        let active = right_pane == (app.view_mode.is_dual_pane() && app.is_right_pane_active());
        Self {
            width: self.width.saturating_sub(usize::from(cursor.is_some())),
            cursor,
            mark: active.then(|| app.cursor_color()),
            selection: app.visual_rows().filter(|_| active),
            ..self
        }
    }

    /// Row `index`, of a search result, showing where the folder lives when `global`.
    fn result(&self, index: usize, result: &SearchResult, global: bool) -> ListItem<'static> {
        let location = Path::new(&result.file_path)
            .parent()
            .filter(|_| global)
            .map(|parent| replace_home_with_tilde(&parent.to_string_lossy()));
        self.row(
            index,
            &result.file_path,
            &result.display_name,
            &result.info,
            location.as_deref(),
        )
    }

    /// Row `index`, of the item at `path`: its mark, its icon and `name`, cut short with
    /// … and with the query highlighted, then `location` dimmed, and the size and date
    /// columns at the right edge, so they line up down the list.
    fn row(
        &self,
        index: usize,
        path: &str,
        name: &str,
        info: &EntryInfo,
        location: Option<&str>,
    ) -> ListItem<'static> {
        let theme = self.theme;
        let icon = self
            .icons
            .get_colored_for_path(Path::new(path), info.is_dir(), theme);
        let at_cursor = self.cursor == Some(index);
        let selected = self
            .selection
            .as_ref()
            .is_some_and(|rows| rows.contains(&index));
        let visual = Style::default().fg(theme.mode_visual);
        // The cursor's mark is in the mode's color, or the selection's inside it.
        let (mark, band) = match (at_cursor, self.mark) {
            (true, Some(_)) if selected => {
                (Span::styled("▌", visual), Some(theme.visual_cursor_bg))
            }
            (true, Some(color)) => (
                Span::styled("▌", Style::default().fg(color)),
                Some(theme.cursor_bg),
            ),
            (true, None) => (Span::raw(" "), Some(theme.inactive_cursor_bg)),
            (false, _) if selected => (Span::styled("┃", visual), Some(theme.visual_bg)),
            (false, _) => (Span::raw(" "), None),
        };
        let (icon_style, name_style) = if name.starts_with('.') {
            (theme.dotfile, theme.dotfile)
        } else {
            (Style::default().fg(icon.color), Style::default())
        };
        let name_style = if at_cursor && self.mark.is_some() {
            Style::default()
                .fg(theme.bright)
                .add_modifier(Modifier::BOLD)
        } else {
            name_style
        };
        let icon = Span::styled(icon.icon, icon_style);
        let columns = self.columns(path, info);
        let columns_width: usize = columns.iter().map(Span::width).sum();
        // The name's room, keeping a space before the columns.
        let room = self
            .width
            .saturating_sub(icon.width() + 1 + columns_width + usize::from(columns_width > 0));
        let name = truncate(name, room);
        let mut used = name.width();
        let mut spans = Vec::new();
        if self.cursor.is_some() {
            spans.push(mark);
        }
        spans.extend([icon, Span::raw(" ")]);
        if self.term.is_empty() {
            spans.push(Span::styled(name, name_style));
        } else {
            spans.extend(highlight_search_term(&name, self.term, name_style, self.hit).spans);
        }
        // Where it lives gets what the name leaves, cut from the start.
        if let Some(location) = location.filter(|_| room > used + 2) {
            let location = truncate_start(location, room - used - 1);
            used += location.width() + 1;
            spans.push(Span::raw(" "));
            spans.push(Span::styled(location, Style::default().fg(theme.muted)));
        }
        if !columns.is_empty() {
            spans.push(Span::raw(" ".repeat(room - used + 1)));
            spans.extend(columns);
        }
        let item = ListItem::from(Line::from(spans));
        match band {
            Some(band) => item.style(Style::default().bg(band)),
            None => item,
        }
    }

    /// The bar, size and date columns that fit, for the item at `path`. Outside size
    /// order a folder shows only its date.
    fn columns(&self, path: &str, info: &EntryInfo) -> Vec<Span<'static>> {
        let needed = |bar: bool, size: bool, date: bool| {
            ICON_WIDTH
                + MIN_NAME_WIDTH
                + usize::from(bar) * (BAR_WIDTH + 1)
                + usize::from(size) * (SIZE_WIDTH + 1)
                + usize::from(date) * (DATE_WIDTH + 1)
        };
        let (bar, size) = (self.sizes && self.size_order.is_some(), self.sizes);
        let date = needed(bar, size, true) <= self.width;
        let bar = bar && needed(bar, size, date) <= self.width;
        let size = size && needed(bar, size, date) <= self.width;

        let mut spans = Vec::new();
        if size {
            let (row_size, largest) = match self.size_order {
                Some((sizes, largest)) => (sizes.row_size(path, info), largest),
                None if info.is_file() => {
                    let total = FolderTotal {
                        bytes: info.size,
                        partial: false,
                    };
                    (RowSize::Known(total), 0)
                }
                None => (RowSize::Unsized, 0),
            };
            spans.extend(size_column(row_size, largest, bar, self.theme));
        }
        if date {
            if size {
                spans.push(Span::raw(" "));
            }
            let label = date_label(info.modified_local, self.today);
            spans.push(Span::styled(
                format!("{label:>DATE_WIDTH$}"),
                Style::default().fg(self.theme.muted),
            ));
        }
        spans
    }
}

/// `name` cut to `width` columns, ending in … when it doesn't fit.
pub(crate) fn truncate(name: &str, width: usize) -> String {
    if name.width() <= width {
        return name.to_string();
    }
    let mut cut = String::new();
    let mut used = 0;
    for c in name.chars() {
        let c_width = c.width().unwrap_or(0);
        if used + c_width + 1 > width {
            break;
        }
        used += c_width;
        cut.push(c);
    }
    if width > 0 {
        cut.push('…');
    }
    cut
}

/// `text` cut to `width` columns from the start, beginning with … when it doesn't fit.
pub(crate) fn truncate_start(text: &str, width: usize) -> String {
    if text.width() <= width {
        return text.to_string();
    }
    let mut kept = Vec::new();
    let mut used = 1;
    for c in text.chars().rev() {
        let c_width = c.width().unwrap_or(0);
        if used + c_width > width {
            break;
        }
        used += c_width;
        kept.push(c);
    }
    std::iter::once('…').chain(kept.into_iter().rev()).collect()
}

/// The search box's query to highlight in a pane's rows while it's typed there:
/// the left listing's, or the right pane's when `right_pane`.
pub fn highlight_term(app: &App, right_pane: bool) -> &str {
    if app.input_mode != InputMode::Editing
        || app.input.is_empty()
        || app.searching_right_pane() != right_pane
    {
        return "";
    }
    let term = app.input.trim();
    // Remove leading search indicators for highlighting
    if term.starts_with(' ') || term.starts_with('/') {
        term.trim_start_matches(' ').trim_start_matches('/')
    } else {
        term
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{PanePosition, VisualSelection};
    use crate::config::settings::NerdFontSetting;
    use crate::pane::Pane;
    use crate::utils::files::{EntryKind, Listing};
    use crate::utils::format::local_stamp;
    use crate::utils::{read_listing, SortBy, SortType};
    use ratatui::{
        backend::TestBackend,
        buffer::{Buffer, Cell},
        Terminal,
    };
    use std::{fs, time::SystemTime};

    /// What the fixture's dates are judged against.
    const TODAY: LocalStamp = LocalStamp {
        year: 2026,
        month: 9,
        day: 25,
        hour: 14,
        minute: 30,
    };

    /// The fixture's size and date columns, by name. A folder shows only its date.
    const FIXTURE_COLUMNS: [(&str, &str, &str); 4] = [
        ("notes.txt", "5 B", "09:15"),
        ("photos", "", "Oct 21"),
        (".zshrc", "2.0 KB", "2024"),
        ("old.bin", "3.0 MB", ""),
    ];

    /// `/w`'s items as read: a 5-byte notes.txt from this morning, a photos folder from
    /// Oct 21, a 2 KB .zshrc from 2024 and a 3 MB old.bin without a modified time.
    fn fixture() -> Listing {
        let entry = |kind, size, at: Option<(i32, u8, u8, u8, u8)>| EntryInfo {
            kind: Some(kind),
            size,
            modified_local: at.map(|(year, month, day, hour, minute)| LocalStamp {
                year,
                month,
                day,
                hour,
                minute,
            }),
            ..EntryInfo::default()
        };
        Listing {
            files: FIXTURE_COLUMNS
                .map(|(name, _, _)| format!("/w/{name}"))
                .to_vec(),
            info: vec![
                entry(EntryKind::File, 5, Some((2026, 9, 25, 9, 15))),
                entry(EntryKind::Folder, 96, Some((2026, 9, 21, 18, 0))),
                entry(EntryKind::File, 2048, Some((2024, 2, 3, 0, 0))),
                entry(EntryKind::File, 3 * 1024 * 1024, None),
            ],
        }
    }

    /// Nerd font icons are one character in one column.
    fn nerd_settings() -> Settings {
        Settings {
            use_nerd_fonts: NerdFontSetting::Always,
            ..Settings::default()
        }
    }

    /// The size and date columns of `row`, one character a column, ending at `end`.
    fn columns_at(row: &str, end: usize) -> String {
        let start = end + 1 - (SIZE_WIDTH + 1 + DATE_WIDTH);
        row.chars().skip(start).take(end + 1 - start).collect()
    }

    /// Fail unless the rows of each of `names` end their fixture size and date at `end`.
    fn assert_columns(rows: &[String], end: usize, names: &[&str]) {
        for (name, size, date) in FIXTURE_COLUMNS {
            if names.contains(&name) {
                let expected = format!("{size:>SIZE_WIDTH$} {date:>DATE_WIDTH$}");
                assert_eq!(columns_at(row(rows, name), end), expected, "{name}");
            }
        }
    }

    /// A folder listing a 5-byte notes.txt and a photos/ folder.
    fn notes_and_photos() -> (tempfile::TempDir, String) {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("notes.txt"), "notes").unwrap();
        fs::create_dir(dir.path().join("photos")).unwrap();
        let start = dir.path().to_string_lossy().into_owned();
        (dir, start)
    }

    fn listing(start: &str) -> crate::utils::Listing {
        read_listing(start.to_string(), false, SortBy::Name, &SortType::ASC).unwrap()
    }

    fn now() -> LocalStamp {
        local_stamp(SystemTime::now()).unwrap()
    }

    /// Grow notes.txt to 2 KB and make photos a 3000-byte file, behind ff's back.
    fn change_on_disk(dir: &Path) {
        fs::write(dir.join("notes.txt"), "x".repeat(2048)).unwrap();
        fs::remove_dir(dir.join("photos")).unwrap();
        fs::write(dir.join("photos"), "x".repeat(3000)).unwrap();
    }

    /// The screen after `render`.
    fn draw_buffer(width: u16, render: impl FnOnce(&mut Frame)) -> Buffer {
        let mut terminal = Terminal::new(TestBackend::new(width, 6)).unwrap();
        terminal.draw(render).unwrap();
        terminal.backend().buffer().clone()
    }

    /// The screen's rows.
    fn draw(width: u16, render: impl FnOnce(&mut Frame)) -> Vec<String> {
        text(&draw_buffer(width, render))
    }

    fn text(buffer: &Buffer) -> Vec<String> {
        buffer
            .content()
            .chunks(buffer.area.width as usize)
            .map(|row| row.iter().map(|cell| cell.symbol()).collect())
            .collect()
    }

    fn row<'a>(rows: &'a [String], name: &str) -> &'a str {
        rows.iter().find(|row| row.contains(name)).unwrap()
    }

    /// The cell `x` columns into the row of `name`, or `offset` columns into the name
    /// when `x` is `None`.
    fn cell(buffer: &Buffer, name: &str, x: Option<u16>, offset: usize) -> Cell {
        let rows = text(buffer);
        let y = rows.iter().position(|row| row.contains(name)).unwrap();
        let x = x.unwrap_or_else(|| {
            let start = rows[y][..rows[y].find(name).unwrap()].chars().count();
            (start + offset) as u16
        });
        buffer.get(x, y as u16).clone()
    }

    /// Whether `cell` shows `symbol` in `fg` on `bg`, and is bold.
    fn looks(cell: &Cell, symbol: &str, fg: Color, bg: Color) -> (bool, bool) {
        (
            (cell.symbol(), cell.fg, cell.bg) == (symbol, fg, bg),
            cell.modifier.contains(Modifier::BOLD),
        )
    }

    /// The left listing, `width` columns wide, with `selected` highlighted.
    fn draw_left(
        ui: &mut Ui,
        app: &App,
        settings: &Settings,
        width: u16,
        selected: Option<usize>,
    ) -> Buffer {
        draw_buffer(width, |f| {
            let mut state = ListState::default().with_selected(selected);
            ui.render_list_preview(f, f.size(), &mut state, app, settings, TODAY);
        })
    }

    #[test]
    fn left_rows_draw_from_the_records_until_the_listing_is_read_again() {
        let (dir, start) = notes_and_photos();
        let settings = Settings::default();
        let mut ui = Ui::new(Vec::new(), &settings);
        let mut app = App::with_listing(listing(&start));
        let mut state = ListState::default();
        let mut draw_left = |app: &App| {
            draw(100, |f| {
                let mut state = ListState::default();
                ui.render_list_preview(f, f.size(), &mut state, app, &settings, now());
            })
        };

        let before = draw_left(&app);
        assert!(row(&before, "notes.txt").contains(" 5 B "));
        assert!(!row(&before, "photos").contains(" B"));

        change_on_disk(dir.path());
        assert_eq!(draw_left(&app), before);

        app.refresh_files_from_watcher(listing(&start), &mut state);
        let after = draw_left(&app);
        assert!(row(&after, "notes.txt").contains(" 2.0 KB "));
        assert!(row(&after, "photos").contains(" 2.9 KB "));
    }

    #[test]
    fn size_order_rows_line_their_sizes_up_at_the_right_edge() {
        let dir = tempfile::tempdir().unwrap();
        let long = "a-name-far-too-long-to-fit-beside-the-size-column.bin";
        fs::write(dir.path().join(long), "x".repeat(2048)).unwrap();
        fs::write(dir.path().join("notes.txt"), "notes").unwrap();
        fs::create_dir(dir.path().join("photos")).unwrap();
        let start = dir.path().to_string_lossy().into_owned();
        let listing = read_listing(start, false, SortBy::Size, &SortType::DESC).unwrap();
        let mut app = App::with_listing(listing);
        app.sort_by = SortBy::Size;
        app.view_mode = ViewMode::FullList;
        let settings = nerd_settings();
        let mut ui = Ui::new(Vec::new(), &settings);

        let wide = text(&draw_left(&mut ui, &app, &settings, 60, Some(0)));
        let long_row = row(&wide, "a-name");
        assert!(long_row.contains('…') && !long_row.contains(".bin"));
        assert!(long_row.contains("██████████ "));
        assert!(row(&wide, "photos").contains("sizing…"));
        // Every date ends in the list's last column, inside its padding, and every size
        // a column before the date's.
        for (name, size) in [
            ("a-name", "2.0 KB"),
            ("notes.txt", "5 B"),
            ("photos", "sizing…"),
        ] {
            let size = format!("{size:>SIZE_WIDTH$} ");
            assert!(
                columns_at(row(&wide, name), 58).starts_with(&size),
                "{name}"
            );
            assert_eq!(row(&wide, name).trim_end().chars().count(), 59, "{name}");
        }

        // Too narrow for the date and the bar.
        let narrow = text(&draw_left(&mut ui, &app, &settings, 36, Some(0)));
        assert!(!narrow.iter().any(|row| row.contains('█')));
        let long_row = row(&narrow, "a-name").trim_end();
        assert!(long_row.contains("lon… ") && long_row.ends_with(" 2.0 KB"));
        assert!(row(&narrow, "notes.txt").contains("5 B"));
    }

    #[test]
    fn every_list_lines_its_sizes_and_dates_up_at_the_right_edge() {
        let settings = nerd_settings();
        let mut ui = Ui::new(Vec::new(), &settings);
        let mut app = App::with_listing(fixture());
        app.view_mode = ViewMode::FullList;
        let names = FIXTURE_COLUMNS.map(|(name, _, _)| name);

        let left = text(&draw_left(&mut ui, &app, &settings, 60, Some(0)));
        assert_columns(&left, 58, &names);

        app.right_pane = Pane::with_listing("/w".into(), fixture());
        let right = draw(40, |f| {
            let items = ui.right_pane_items(&app, &settings, 40, TODAY);
            f.render_widget(List::new(items), f.size())
        });
        assert_columns(&right, 39, &names);

        // The folder preview, which leaves out what `.` hides until it's toggled.
        app.preview_files = fixture();
        let preview = draw(40, |f| {
            ui.render_preview_window(f, f.size(), &app, &settings, TODAY);
        });
        assert_columns(&preview, 39, &["notes.txt", "photos", "old.bin"]);
        assert!(!preview.iter().any(|row| row.contains(".zshrc")));
        assert!(!preview.iter().any(|row| row.contains("photos/")));
        app.show_hidden_files = true;
        let preview = draw(40, |f| {
            ui.render_preview_window(f, f.size(), &app, &settings, TODAY);
        });
        assert_columns(&preview, 39, &names);
    }

    #[test]
    fn local_search_rows_keep_the_highlight_and_drop_the_score() {
        let settings = nerd_settings();
        let mut ui = Ui::new(Vec::new(), &settings);
        let mut app = App::with_listing(fixture());
        app.view_mode = ViewMode::FullList;
        app.input_mode = InputMode::Editing;
        app.input = "o".into();
        app.perform_local_search("o");
        assert_eq!(app.search_results.len(), 3);

        // Nothing highlighted, so no column for a mark: rows start a column sooner and
        // still end at the right edge.
        let buffer = draw_left(&mut ui, &app, &settings, 60, None);
        let rows = text(&buffer);
        assert_columns(&rows, 58, &["notes.txt", "photos", "old.bin"]);
        assert!(!rows.iter().any(|row| row.contains('(')));
        let start = row(&rows, "notes.txt").find("notes.txt").unwrap();
        assert_eq!(row(&rows, "notes.txt")[..start].chars().count(), 3);
        assert_eq!(cell(&buffer, "notes.txt", None, 1).symbol(), "o");
    }

    #[test]
    fn search_matches_are_underlined_in_the_search_color() {
        let settings = nerd_settings();
        let mut ui = Ui::new(Vec::new(), &settings);
        let mut app = App::with_listing(fixture());
        app.view_mode = ViewMode::FullList;
        app.input_mode = InputMode::Editing;
        app.input = "o".into();
        app.perform_local_search("o");
        let theme = app.theme_colors.clone();
        let underlined = |cell: &Cell| {
            cell.modifier
                .contains(Modifier::BOLD | Modifier::UNDERLINED)
        };

        let buffer = draw_left(&mut ui, &app, &settings, 60, None);
        let (n, o) = (
            cell(&buffer, "notes.txt", None, 0),
            cell(&buffer, "notes.txt", None, 1),
        );
        assert_eq!(
            (o.symbol(), o.fg, o.bg),
            ("o", theme.mode_search, Color::Reset)
        );
        assert!(underlined(&o) && !underlined(&n));

        // A search from the root's, on the cursor's band rather than a box.
        app.global_search_mode = true;
        app.input = "pho".into();
        app.search_results = vec![SearchResult {
            file_path: "/w/photos".into(),
            display_name: "photos".into(),
            score: 87,
            info: fixture().info[1],
            original_index: 0,
        }];
        let buffer = draw_left(&mut ui, &app, &settings, 60, Some(0));
        let p = cell(&buffer, "photos", None, 0);
        assert_eq!(
            (p.symbol(), p.fg, p.bg),
            ("p", theme.mode_global, theme.cursor_bg)
        );
        assert!(underlined(&p) && !underlined(&cell(&buffer, "photos", None, 3)));
    }

    #[test]
    fn global_search_says_the_folder_index_is_being_built_while_it_is_empty() {
        let settings = nerd_settings();
        let mut ui = Ui::new(Vec::new(), &settings);
        let mut app = App::with_listing(fixture());
        app.view_mode = ViewMode::FullList;
        app.index_filling = true;
        let says = |rows: Vec<String>| rows.iter().any(|row| row.contains(INDEX_FILLING));

        // Typing a search from the root finds nothing yet, so it says why.
        app.enter_search(crate::app::SearchScope::Root);
        app.input = "pho".into();
        assert!(says(text(&draw_left(
            &mut ui,
            &app,
            &settings,
            60,
            Some(0)
        ))));
        // Out of the box the folder's rows stay, as keys act on them.
        app.input_mode = InputMode::Normal;
        let rows = text(&draw_left(&mut ui, &app, &settings, 60, Some(0)));
        assert!(!says(rows.clone()) && rows.iter().any(|row| row.contains("photos")));
        // A search of the folder doesn't use the index.
        app.enter_search(crate::app::SearchScope::CurrentDirectory);
        assert!(!says(text(&draw_left(
            &mut ui,
            &app,
            &settings,
            60,
            Some(0)
        ))));
        // Once built, an empty result is just that.
        app.enter_search(crate::app::SearchScope::Root);
        app.index_filling = false;
        assert!(!says(text(&draw_left(
            &mut ui,
            &app,
            &settings,
            60,
            Some(0)
        ))));

        // The right pane's search says it too.
        app.index_filling = true;
        app.view_mode = ViewMode::DualPane;
        app.active_pane = PanePosition::Right;
        app.right_pane = Pane::with_listing("/w".into(), fixture());
        app.enter_search(crate::app::SearchScope::Root);
        let right = |app: &App| {
            draw(60, |f| {
                let items = ui.right_pane_items(app, &settings, 60, TODAY);
                f.render_widget(List::new(items), f.size())
            })
        };
        assert!(says(right(&app)));
        app.index_filling = false;
        assert!(!says(right(&app)));
    }

    #[test]
    fn the_cursor_gets_a_band_a_mark_in_the_mode_color_and_a_bold_name() {
        let settings = nerd_settings();
        let mut ui = Ui::new(Vec::new(), &settings);
        let mut app = App::with_listing(fixture());
        app.view_mode = ViewMode::FullList;
        let theme = app.theme_colors.clone();

        let buffer = draw_left(&mut ui, &app, &settings, 60, Some(1));
        // The mark sits where the highlight symbol did, beside the icon and the name.
        let mark = cell(&buffer, "photos", Some(1), 0);
        assert_eq!(
            looks(&mark, "▌", theme.mode_normal, theme.cursor_bg),
            (true, false)
        );
        let name = cell(&buffer, "photos", Some(4), 0);
        assert_eq!(
            looks(&name, "p", theme.bright, theme.cursor_bg),
            (true, true)
        );
        // The date stays muted on the band.
        let date = cell(&buffer, "photos", Some(58), 0);
        assert_eq!((date.fg, date.bg), (theme.muted, theme.cursor_bg));
        let other = cell(&buffer, "notes.txt", Some(1), 0);
        assert_eq!((other.symbol(), other.bg), (" ", Color::Reset));
        assert_eq!(cell(&buffer, "notes.txt", Some(4), 0).fg, theme.fg);

        // Browsing a local search's results, the mark takes the search's color.
        app.input = "o".into();
        app.perform_local_search("o");
        let buffer = draw_left(&mut ui, &app, &settings, 60, Some(0));
        // On the first row: the top bar names the search, so the list has no title.
        assert_eq!(buffer.get(1, 0).fg, theme.mode_search);
    }

    #[test]
    fn the_other_panes_cursor_is_fainter_and_unmarked() {
        let settings = nerd_settings();
        let mut ui = Ui::new(Vec::new(), &settings);
        let mut app = App::with_listing(fixture());
        app.view_mode = ViewMode::DualPane;
        app.right_pane = Pane::with_listing("/w".into(), fixture());
        let theme = app.theme_colors.clone();
        let right = |ui: &Ui, app: &App| {
            draw_buffer(40, |f| {
                let items = ui.right_pane_items(app, &settings, 40, TODAY);
                f.render_widget(List::new(items), f.size())
            })
        };

        app.active_pane = PanePosition::Right;
        let buffer = right(&ui, &app);
        let mark = cell(&buffer, "notes.txt", Some(0), 0);
        assert_eq!(
            looks(&mark, "▌", theme.mode_normal, theme.cursor_bg),
            (true, false)
        );
        // The left pane's cursor, in the left half.
        let buffer = draw_left(&mut ui, &app, &settings, 120, Some(0));
        let mark = cell(&buffer, "notes.txt", Some(1), 0);
        assert_eq!(
            looks(&mark, " ", theme.fg, theme.inactive_cursor_bg),
            (true, false)
        );
        let name = cell(&buffer, "notes.txt", Some(4), 0);
        assert_eq!(
            looks(&name, "n", theme.fg, theme.inactive_cursor_bg),
            (true, false)
        );

        app.active_pane = PanePosition::Left;
        let buffer = right(&ui, &app);
        let mark = cell(&buffer, "notes.txt", Some(0), 0);
        assert_eq!((mark.symbol(), mark.bg), (" ", theme.inactive_cursor_bg));
        let buffer = draw_left(&mut ui, &app, &settings, 120, Some(0));
        assert_eq!(cell(&buffer, "notes.txt", Some(1), 0).symbol(), "▌");
    }

    #[test]
    fn global_results_show_where_each_folder_lives_without_a_score() {
        let home = dirs::home_dir().unwrap();
        let settings = nerd_settings();
        let mut ui = Ui::new(Vec::new(), &settings);
        let mut app = App::with_listing(Listing::default());
        app.view_mode = ViewMode::FullList;
        app.global_search_mode = true;
        let photos = fixture().info[1];
        let result = |path: &str, name: &str| SearchResult {
            file_path: home.join(path).to_string_lossy().into_owned(),
            display_name: name.into(),
            score: 87,
            info: photos,
            original_index: 0,
        };
        app.search_results = vec![
            result("projects/ff", "ff"),
            result("a/very/long/path/that/never/seems/to/end/at/all/src", "src"),
        ];

        let rows = text(&draw_left(&mut ui, &app, &settings, 60, Some(0)));
        let ff = row(&rows, "ff ");
        assert!(ff.contains("ff ~/projects ") && !ff.contains("87"), "{ff}");
        // A location too long for its room is cut from the start.
        let src = row(&rows, "src ");
        assert!(
            src.contains("src …") && src.contains("/end/at/all "),
            "{src}"
        );
        for name in ["ff ", "src "] {
            let expected = format!("{:>SIZE_WIDTH$} {:>DATE_WIDTH$}", "", "Oct 21");
            assert_eq!(columns_at(row(&rows, name), 58), expected, "{name}");
        }
    }

    #[test]
    fn narrow_lists_drop_the_date_then_the_bar_then_the_size() {
        let settings = nerd_settings();
        let ui = Ui::new(Vec::new(), &settings);
        let mut app = App::new(Vec::new());
        app.right_pane = Pane::with_listing("/w".into(), fixture());
        app.right_pane.sort_by = SortBy::Size;
        // A name keeps 12 columns beside the mark's column and an icon of up to 3, so 46
        // fit everything.
        for (width, bar, size, date) in [
            (46, true, true, true),
            (45, true, true, false),
            (39, true, true, false),
            (38, false, true, false),
            (28, false, true, false),
            (27, false, false, false),
        ] {
            let rows = draw(width, |f| {
                let items = ui.right_pane_items(&app, &settings, width.into(), TODAY);
                f.render_widget(List::new(items), f.size())
            });
            // old.bin is the largest, with a full bar.
            assert_eq!(row(&rows, "old.bin").contains('█'), bar, "{width}");
            assert_eq!(row(&rows, "old.bin").contains("3.0 MB"), size, "{width}");
            assert_eq!(row(&rows, "notes.txt").contains("09:15"), date, "{width}");
        }
    }

    #[test]
    fn hiding_sizes_keeps_the_dates() {
        let settings = Settings {
            show_size_bars: false,
            ..nerd_settings()
        };
        let ui = Ui::new(Vec::new(), &settings);
        let mut app = App::new(Vec::new());
        app.right_pane = Pane::with_listing("/w".into(), fixture());
        let rows = draw(40, |f| {
            let items = ui.right_pane_items(&app, &settings, 40, TODAY);
            f.render_widget(List::new(items), f.size())
        });
        assert!(row(&rows, ".zshrc").ends_with("  2024"));
        assert!(!rows.iter().any(|row| row.contains(" B")));
    }

    #[test]
    fn visual_rows_get_the_band_and_marks_with_names_in_their_own_colors() {
        let settings = nerd_settings();
        let mut ui = Ui::new(Vec::new(), &settings);
        let mut app = App::with_listing(fixture());
        app.view_mode = ViewMode::FullList;
        app.input_mode = InputMode::Visual;
        app.visual = Some(VisualSelection {
            anchor: "/w/.zshrc".into(),
            cursor: "/w/notes.txt".into(),
            paths: vec![
                "/w/notes.txt".into(),
                "/w/photos".into(),
                "/w/.zshrc".into(),
            ],
        });
        let theme = app.theme_colors.clone();

        // notes.txt is the cursor, with a stronger tint.
        let buffer = draw_left(&mut ui, &app, &settings, 60, Some(0));
        let mark = cell(&buffer, "notes.txt", Some(1), 0);
        assert_eq!(
            looks(&mark, "▌", theme.mode_visual, theme.visual_cursor_bg),
            (true, false)
        );
        let name = cell(&buffer, "notes.txt", Some(4), 0);
        assert_eq!(
            looks(&name, "n", theme.bright, theme.visual_cursor_bg),
            (true, true)
        );
        // The rest of the selection: names in their own colors, a dotfile's dimmed, and
        // the date muted, all on the band.
        for (name, x, symbol, fg) in [
            ("photos", 1, "┃", theme.mode_visual),
            ("photos", 4, "p", theme.fg),
            ("photos", 58, "1", theme.muted),
            (".zshrc", 1, "┃", theme.mode_visual),
            (".zshrc", 4, ".", theme.gray),
        ] {
            let shown = cell(&buffer, name, Some(x), 0);
            assert_eq!(
                looks(&shown, symbol, fg, theme.visual_bg),
                (true, false),
                "{name} {x}"
            );
        }
        let after = cell(&buffer, "old.bin", Some(1), 0);
        assert_eq!((after.symbol(), after.bg), (" ", Color::Reset));
    }

    #[test]
    fn right_pane_rows_draw_from_the_records_until_the_listing_is_read_again() {
        let (dir, start) = notes_and_photos();
        let settings = Settings::default();
        let ui = Ui::new(Vec::new(), &settings);
        let mut app = App::new(Vec::new());
        app.view_mode = ViewMode::DualPane;
        app.right_pane = crate::pane::Pane::with_listing(start.clone(), listing(&start));
        let photos = dir.path().join("photos");
        let folder_row = format!("{} photos", ui.icon_provider.get_for_path(&photos, true));
        let file_row = format!("{} photos", ui.icon_provider.get_for_path(&photos, false));
        assert_ne!(folder_row, file_row);
        let draw_right = |app: &App| {
            draw(40, |f| {
                let items = ui.right_pane_items(app, &settings, 40, now());
                f.render_widget(List::new(items), f.size())
            })
        };

        assert!(row(&draw_right(&app), "photos").contains(&folder_row));
        change_on_disk(dir.path());
        assert!(row(&draw_right(&app), "photos").contains(&folder_row));

        app.right_pane.refresh_files();
        assert!(row(&draw_right(&app), "photos").contains(&file_row));
    }

    #[test]
    fn right_pane_dims_dotfiles_like_the_left_listing() {
        let settings = Settings::default();
        let ui = Ui::new(Vec::new(), &settings);
        let mut app = App::new(Vec::new());
        app.right_pane = crate::pane::Pane::new("/w".into(), vec!["/w/.hidden".into()]);
        let mut terminal = Terminal::new(TestBackend::new(20, 1)).unwrap();
        terminal
            .draw(|f| {
                f.render_widget(
                    List::new(ui.right_pane_items(&app, &settings, 20, TODAY)),
                    f.size(),
                )
            })
            .unwrap();
        let buffer = terminal.backend().buffer();
        let dot = buffer
            .content()
            .iter()
            .find(|cell| cell.symbol() == ".")
            .unwrap();
        assert_eq!(Some(dot.fg), app.theme_colors.dotfile.fg);
    }
}
