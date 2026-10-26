use ratatui::style::{Color, Style};
use ratatui::text::Span;

use crate::config::ThemeColors;
use crate::sizes::{total_text, RowSize};
use crate::utils::format::format_file_size;

/// Very muted gray for sizes (yazi-style).
const SIZE_COLOR: Color = Color::Rgb(92, 99, 112);
/// Columns of a size bar.
pub const BAR_WIDTH: usize = 10;
/// Columns of a size, wide enough for "≥ 1023.9 GB".
pub const SIZE_WIDTH: usize = 11;

/// Create a formatted file size text span
///
/// Returns a styled span showing the human-readable file size (e.g., "1.5 MB")
pub fn create_size_text(file_size: u64) -> Span<'static> {
    if file_size == 0 {
        return Span::raw("");
    }

    let size_str = format_file_size(file_size);
    Span::styled(size_str, Style::default().fg(SIZE_COLOR))
}

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

/// The size column of a row sorted by size: the bar against `largest` (unless
/// `with_bar` is false), then the size, right-aligned.
pub fn size_column(
    size: RowSize,
    largest: u64,
    with_bar: bool,
    theme: &ThemeColors,
) -> Vec<Span<'static>> {
    let (bar_text, color, text) = match size {
        RowSize::Known(total) => (
            bar(total.bytes, largest),
            bar_color(total.bytes, theme),
            total_text(total),
        ),
        RowSize::Sizing => (" ".repeat(BAR_WIDTH), SIZE_COLOR, "sizing…".to_string()),
        RowSize::Unsized => (" ".repeat(BAR_WIDTH), SIZE_COLOR, String::new()),
    };
    let text = Span::styled(
        format!("{text:>SIZE_WIDTH$}"),
        Style::default().fg(SIZE_COLOR),
    );
    if with_bar {
        vec![
            Span::styled(bar_text, Style::default().fg(color)),
            Span::raw(" "),
            text,
        ]
    } else {
        vec![text]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sizes::FolderTotal;

    #[test]
    fn test_create_size_text_bytes() {
        let span = create_size_text(512);
        assert_eq!(span.content, "512 B");
    }

    #[test]
    fn test_create_size_text_kilobytes() {
        let span = create_size_text(1536);
        assert_eq!(span.content, "1.5 KB");
    }

    #[test]
    fn test_create_size_text_megabytes() {
        let span = create_size_text(1048576);
        assert_eq!(span.content, "1.0 MB");
    }

    #[test]
    fn test_create_size_text_zero() {
        let span = create_size_text(0);
        assert_eq!(span.content, "");
    }

    #[test]
    fn test_create_size_text_color() {
        let span = create_size_text(1024);
        // Muted gray color for file sizes
        assert!(matches!(span.style.fg, Some(Color::Rgb(92, 99, 112))));
    }

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
