//! Preview panel rendering utilities.

use ratatui::widgets::{Block, Borders, Paragraph};

use crate::config::ThemeColors;

/// Create the cache loading screen widget.
pub fn create_cache_loading_screen<'a>(
    directories_processed: usize,
    current_directory: &str,
    theme: &ThemeColors,
) -> Paragraph<'a> {
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
                .style(theme.warning),
        )
        .style(theme.normal)
        .alignment(ratatui::layout::Alignment::Center)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{buffer::Buffer, layout::Rect, widgets::Widget};

    #[test]
    fn test_create_cache_loading_screen() {
        let theme = crate::config::Theme::default().to_colors().unwrap();
        let area = Rect::new(0, 0, 80, 10);
        let mut buffer = Buffer::empty(area);
        create_cache_loading_screen(100, "/some/path", &theme).render(area, &mut buffer);
        let rendered: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
        assert!(rendered.contains("Directory Cache Loading"));
        assert!(rendered.contains("Directories processed: 100"));
        assert!(rendered.contains("Current: /some/path"));
        assert_eq!(buffer.get(0, 0).fg, theme.yellow);
    }
}
