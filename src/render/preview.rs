//! Preview panel rendering utilities.

use ratatui::{
    style::{Color, Style},
    widgets::{Block, Borders, Paragraph},
};

/// Create a file preview paragraph widget with highlighted content.
pub fn create_file_preview<'a>(content: &'a str) -> Paragraph<'a> {
    Paragraph::new(content)
        .block(Block::default().borders(Borders::ALL).title("File Preview"))
        .style(Style::default())
}

/// Create an image preview info widget.
pub fn create_image_preview<'a>(image_info: &'a str, has_image: bool) -> Paragraph<'a> {
    if has_image {
        Paragraph::new(image_info)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Image Preview"),
            )
            .style(Style::default().fg(Color::Green))
    } else {
        let display_text = if image_info.is_empty() {
            "Unable to load image preview\n\nPossible reasons:\n• Unsupported image format\n• Corrupted image file\n• Insufficient permissions\n\nSupported formats: PNG, JPEG, GIF, BMP"
        } else {
            image_info
        };

        Paragraph::new(display_text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Image Preview - Error"),
            )
            .style(Style::default().fg(Color::Yellow))
    }
}

/// Create the cache loading screen widget.
pub fn create_cache_loading_screen<'a>(
    directories_processed: usize,
    current_directory: &str,
) -> Paragraph<'a> {
    use crate::theme::OneDarkTheme;

    let spinner_chars = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
    let spinner_index = (directories_processed / 10) % spinner_chars.len();
    let spinner = spinner_chars[spinner_index];

    let loading_text = if current_directory.is_empty() {
        format!(
            "{} Building directory cache...\n\nDirectories processed: {}\n\nPlease wait while the cache is being built.",
            spinner, directories_processed
        )
    } else {
        let display_dir = if current_directory.len() > 60 {
            format!("...{}", &current_directory[current_directory.len() - 57..])
        } else {
            current_directory.to_string()
        };
        format!(
            "{} Building directory cache...\n\nDirectories processed: {}\nCurrent: {}\n\nPlease wait while the cache is being built.",
            spinner, directories_processed, display_dir
        )
    };

    Paragraph::new(loading_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("⚡ Directory Cache Loading")
                .style(OneDarkTheme::loading()),
        )
        .style(OneDarkTheme::normal())
        .alignment(ratatui::layout::Alignment::Center)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{buffer::Buffer, layout::Rect, widgets::Widget};

    #[test]
    fn test_create_file_preview() {
        let area = Rect::new(0, 0, 40, 4);
        let mut buffer = Buffer::empty(area);
        create_file_preview("test content").render(area, &mut buffer);
        let rendered: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
        assert!(rendered.contains("File Preview"));
        assert!(rendered.contains("test content"));
    }

    #[test]
    fn test_create_image_preview_with_image() {
        let area = Rect::new(0, 0, 40, 4);
        let mut buffer = Buffer::empty(area);
        create_image_preview("100x100 PNG", true).render(area, &mut buffer);
        let rendered: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
        assert!(rendered.contains("Image Preview"));
        assert!(rendered.contains("100x100 PNG"));
        assert_eq!(buffer.get(1, 1).fg, Color::Green);
    }

    #[test]
    fn test_create_image_preview_error() {
        let area = Rect::new(0, 0, 80, 12);
        let mut buffer = Buffer::empty(area);
        create_image_preview("", false).render(area, &mut buffer);
        let rendered: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
        assert!(rendered.contains("Image Preview - Error"));
        assert!(rendered.contains("Unable to load image preview"));
        assert_eq!(buffer.get(1, 1).fg, Color::Yellow);
    }

    #[test]
    fn test_create_cache_loading_screen() {
        let area = Rect::new(0, 0, 80, 10);
        let mut buffer = Buffer::empty(area);
        create_cache_loading_screen(100, "/some/path").render(area, &mut buffer);
        let rendered: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
        assert!(rendered.contains("Directory Cache Loading"));
        assert!(rendered.contains("Directories processed: 100"));
        assert!(rendered.contains("Current: /some/path"));
    }
}
