//! Preview panel rendering utilities.

use ratatui::{
    style::{Color, Style},
    widgets::{Block, Borders, List, Paragraph},
};

use crate::app::InputMode;

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
            .block(Block::default().borders(Borders::ALL).title("Image Preview"))
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

/// Create a ZIP archive contents list widget.
pub fn create_zip_preview<'a>(
    zip_contents: Vec<ratatui::text::Line<'a>>,
    input_mode: &InputMode,
) -> List<'a> {
    let border_style = match input_mode {
        InputMode::Normal => Style::default().fg(Color::Green),
        InputMode::Editing => Style::default().fg(Color::Gray),
        _ => Style::default().fg(Color::Gray),
    };

    List::new(zip_contents)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("ZIP Archive Contents")
                .style(border_style),
        )
        .style(Style::default().fg(Color::DarkGray))
}

/// Create a CSV data preview list widget.
pub fn create_csv_preview<'a>(
    csv_contents: Vec<ratatui::text::Line<'a>>,
    input_mode: &InputMode,
) -> List<'a> {
    let border_style = match input_mode {
        InputMode::Normal => Style::default().fg(Color::Green),
        InputMode::Editing => Style::default().fg(Color::Gray),
        _ => Style::default().fg(Color::Gray),
    };

    List::new(csv_contents)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("CSV Data Preview")
                .style(border_style),
        )
        .style(Style::default().fg(Color::DarkGray))
}

/// Create an archive info preview widget.
pub fn create_archive_preview<'a>(content: &'a str) -> Paragraph<'a> {
    Paragraph::new(content)
        .block(Block::default().borders(Borders::ALL).title("Archive Info"))
        .style(Style::default())
}

/// Create a binary file info preview widget.
pub fn create_binary_preview<'a>(content: &'a str) -> Paragraph<'a> {
    Paragraph::new(content)
        .block(Block::default().borders(Borders::ALL).title("Binary File"))
        .style(Style::default().fg(Color::Yellow))
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

    #[test]
    fn test_create_file_preview() {
        let preview = create_file_preview("test content");
        // Just verify it doesn't panic
        assert!(true);
    }

    #[test]
    fn test_create_image_preview_with_image() {
        let preview = create_image_preview("100x100 PNG", true);
        assert!(true);
    }

    #[test]
    fn test_create_image_preview_error() {
        let preview = create_image_preview("", false);
        assert!(true);
    }

    #[test]
    fn test_create_cache_loading_screen() {
        let screen = create_cache_loading_screen(100, "/some/path");
        assert!(true);
    }
}
