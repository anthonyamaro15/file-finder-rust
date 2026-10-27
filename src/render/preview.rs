//! Preview panel rendering utilities.

use std::path::Path;

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};
use unicode_width::UnicodeWidthStr;

use crate::{
    app::App,
    config::ThemeColors,
    file_reader_content::{FileContent, FileType, MAX_PREVIEW_LINES},
    icons::IconProvider,
    info::plural,
    status_bar::{selection_kinds, type_label},
    ui::truncate,
    utils::format_file_size,
};

/// Draw the preview's header on `area`'s first row and a dim rule under it, and return
/// the rows below. It names the highlighted item, with its icon, and shows a folder's
/// item count or a file's type and size at the right, all from the records; an image's
/// pixel size too once it's decoded (`image_size`). In visual mode it's the selection.
pub fn render_header(
    f: &mut Frame,
    area: Rect,
    app: &App,
    icons: &IconProvider,
    content: &FileContent,
    image_size: Option<&str>,
    selected: Option<usize>,
) -> Rect {
    let theme = &app.theme_colors;
    let bright = Style::default()
        .fg(theme.bright)
        .add_modifier(Modifier::BOLD);
    let (icon, name, details) = if app.visual.is_some() {
        (None, "Selection".to_string(), selection_kinds(app))
    } else {
        let (Some(path), Some(info)) = (
            app.active_selected_path(selected),
            app.active_selected_info(selected),
        ) else {
            return area;
        };
        let item = Path::new(&path);
        let mut name = item
            .file_name()
            .map_or_else(|| path.clone(), |name| name.to_string_lossy().into_owned());
        let icon = icons.get_colored_for_path(item, info.is_dir(), theme);
        let icon_style = if name.starts_with('.') {
            theme.dotfile
        } else {
            Style::default().fg(icon.color)
        };
        // The preview's own findings count once they are this item's.
        let previewed = content.curr_selected_path == path;
        let details = if info.is_dir() {
            name.push('/');
            app.preview_count(&path)
                .map(|count| match app.preview_hidden() {
                    0 => plural(count as u64, "item"),
                    hidden => format!("{} · {hidden} hidden", plural(count as u64, "item")),
                })
                .unwrap_or_default()
        } else if info.is_file() {
            let label = match (type_label(item), &content.file_type) {
                (label, FileType::Binary) if label == "file" && previewed => "binary".into(),
                (label, FileType::TextFile) if label == "file" && previewed => "text".into(),
                (label, FileType::ConfigFile) if label == "file" && previewed => "config".into(),
                (label, _) => label,
            };
            let mut details = vec![label];
            details.extend(image_size.filter(|_| previewed).map(str::to_string));
            details.push(format_file_size(info.size));
            details.join(" · ")
        } else {
            String::new()
        };
        (Some(Span::styled(icon.icon, icon_style)), name, details)
    };

    // The name gets what the details leave, keeping a gap.
    let width = usize::from(area.width);
    let icon_width = icon.as_ref().map_or(0, |icon| icon.width() + 1);
    let gap = usize::from(!details.is_empty());
    let room = width.saturating_sub(icon_width + gap + details.width());
    let name = truncate(&name, room);
    let fill = width.saturating_sub(icon_width + name.width() + details.width());
    let mut spans = Vec::new();
    if let Some(icon) = icon {
        spans.extend([icon, Span::raw(" ")]);
    }
    spans.extend([
        Span::styled(name, bright),
        Span::raw(" ".repeat(fill)),
        Span::styled(details, Style::default().fg(theme.light_gray)),
    ]);
    let row = |y: u16| Rect {
        y: area.y + y,
        height: 1,
        ..area
    };
    let rule = Span::styled("─".repeat(width), Style::default().fg(theme.dim));
    for (y, line) in [(0, Line::from(spans)), (1, Line::from(rule))] {
        if y < area.height {
            f.render_widget(Paragraph::new(line), row(y));
        }
    }
    Rect {
        y: area.y + area.height.min(2),
        height: area.height.saturating_sub(2),
        ..area
    }
}

/// `lines` behind a gutter of their numbers, right-aligned and dim. A note after the
/// lines the preview read, of how many more there are, gets no number.
pub fn numbered<'a>(lines: Vec<Line<'a>>, theme: &ThemeColors) -> Vec<Line<'a>> {
    let dim = Style::default().fg(theme.dim);
    lines
        .into_iter()
        .enumerate()
        .map(|(i, mut line)| {
            let number = if i < MAX_PREVIEW_LINES {
                format!("{:>3}  ", i + 1)
            } else {
                " ".repeat(5)
            };
            line.spans.insert(0, Span::styled(number, dim));
            line
        })
        .collect()
}
