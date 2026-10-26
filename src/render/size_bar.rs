use ratatui::style::{Color, Style};
use ratatui::text::Span;

use crate::config::ThemeColors;
use crate::sizes::{total_text, RowSize};

/// Very muted gray for sizes, dates and locations (yazi-style).
pub const SIZE_COLOR: Color = Color::Rgb(92, 99, 112);
/// Columns of a size bar.
pub const BAR_WIDTH: usize = 10;
/// Columns of a size, wide enough for "≥ 1023.9 GB".
pub const SIZE_WIDTH: usize = 11;

/// A bar's colour: red from 1 GB, yellow from 100 MB, green below.
pub fn bar_color(bytes: u64, theme: &ThemeColors) -> Color {
    const MB: u64 = 1024 * 1024;
    if bytes >= 1024 * MB {
        theme.red
    } else if bytes >= 100 * MB {
        theme.yellow
    } else {
        theme.green
    }
}

/// A bar `BAR_WIDTH` columns wide, filled in eighths of a column for `bytes` of
/// `largest`. Anything above nothing gets at least a sliver.
pub fn bar(bytes: u64, largest: u64) -> String {
    const PARTS: [char; 8] = [' ', '▏', '▎', '▍', '▌', '▋', '▊', '▉'];
    let eighths = (BAR_WIDTH * 8) as u128;
    let filled = match largest {
        0 => 0,
        _ => (u128::from(bytes) * eighths / u128::from(largest)).min(eighths) as usize,
    };
    let filled = if bytes > 0 { filled.max(1) } else { 0 };
    let mut bar = "█".repeat(filled / 8);
    if filled % 8 > 0 {
        bar.push(PARTS[filled % 8]);
    }
    format!("{bar:<BAR_WIDTH$}")
}

/// A row's size column: the bar against `largest` (when `with_bar`), then the size,
/// right-aligned.
pub fn size_column(
    size: RowSize,
    largest: u64,
    with_bar: bool,
    theme: &ThemeColors,
) -> Vec<Span<'static>> {
    let text = match size {
        RowSize::Known(total) => total_text(total),
        RowSize::Sizing => "sizing…".to_string(),
        RowSize::Unsized => String::new(),
    };
    let text = Span::styled(
        format!("{text:>SIZE_WIDTH$}"),
        Style::default().fg(SIZE_COLOR),
    );
    if !with_bar {
        return vec![text];
    }
    let (bar_text, color) = match size {
        RowSize::Known(total) => (bar(total.bytes, largest), bar_color(total.bytes, theme)),
        _ => (" ".repeat(BAR_WIDTH), SIZE_COLOR),
    };
    vec![
        Span::styled(bar_text, Style::default().fg(color)),
        Span::raw(" "),
        text,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sizes::FolderTotal;
    use crate::utils::format::format_file_size;

    #[test]
    fn bar_colour_turns_yellow_at_100_mb_and_red_at_1_gb() {
        let theme = crate::config::Theme::default().to_colors().unwrap();
        const MB: u64 = 1024 * 1024;
        for (bytes, color) in [
            (0, theme.green),
            (100 * MB - 1, theme.green),
            (100 * MB, theme.yellow),
            (1024 * MB - 1, theme.yellow),
            (1024 * MB, theme.red),
        ] {
            assert_eq!(bar_color(bytes, &theme), color, "{bytes}");
        }
        // What's labelled "1.0 GB" is red.
        assert_eq!(format_file_size(1024 * MB), "1.0 GB");
    }

    #[test]
    fn bar_length_is_relative_to_the_largest_item() {
        let filled = |bytes| bar(bytes, 1000).trim_end().chars().count();
        assert_eq!(bar(1000, 1000), "█".repeat(BAR_WIDTH));
        assert_eq!(
            bar(500, 1000),
            format!("{:<BAR_WIDTH$}", "█".repeat(BAR_WIDTH / 2))
        );
        assert_eq!(bar(0, 1000), " ".repeat(BAR_WIDTH));
        assert_eq!(bar(0, 0), " ".repeat(BAR_WIDTH));
        // A tiny item still shows, as a sliver, and partial columns use eighths.
        assert_eq!(bar(1, 1000).trim_end(), "▏");
        assert_eq!(bar(1100, 2000).trim_end(), "█████▌");
        assert!(filled(250) < filled(750));
        for bytes in [1, 250, 999, 1000] {
            assert_eq!(bar(bytes, 1000).chars().count(), BAR_WIDTH);
        }
    }

    #[test]
    fn size_column_aligns_its_text_and_drops_the_bar_when_asked() {
        let theme = crate::config::Theme::default().to_colors().unwrap();
        let text = |spans: Vec<Span>| {
            spans
                .iter()
                .map(|span| span.content.clone())
                .collect::<String>()
        };
        let partial = RowSize::Known(FolderTotal {
            bytes: 2048,
            partial: true,
        });
        assert_eq!(
            text(size_column(partial, 2048, true, &theme)),
            format!("{} {:>SIZE_WIDTH$}", "█".repeat(BAR_WIDTH), "≥ 2.0 KB")
        );
        assert_eq!(
            text(size_column(RowSize::Sizing, 2048, false, &theme)),
            format!("{:>SIZE_WIDTH$}", "sizing…")
        );
    }
}
