use ratatui::style::{Color, Style};
use ratatui::text::Span;
use std::fs;

use crate::utils::format::format_file_size;

/// Create a formatted file size text span
///
/// Returns a styled span showing the human-readable file size (e.g., "1.5 MB")
pub fn create_size_text(file_size: u64) -> Span<'static> {
    if file_size == 0 {
        return Span::raw("");
    }

    let size_str = format_file_size(file_size);
    // Use a very muted gray for file sizes (yazi-style)
    Span::styled(size_str, Style::default().fg(Color::Rgb(92, 99, 112)))
}

/// Get file size for a path (returns 0 for directories or errors)
pub fn get_file_size(path: &str) -> u64 {
    fs::metadata(path)
        .ok()
        .filter(|m| m.is_file())
        .map(|m| m.len())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
