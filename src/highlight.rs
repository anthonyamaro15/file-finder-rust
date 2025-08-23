use ratatui::style::Style;
use ratatui::text::{Line, Span};

/// Creates a Line with highlighted search terms
pub fn highlight_search_term(
    text: &str,
    search_term: &str,
    normal_style: Style,
    highlight_style: Style,
) -> Line<'static> {
    if search_term.is_empty() {
        return Line::from(Span::styled(text.to_string(), normal_style));
    }

    let mut spans = Vec::new();
    let text_lower = text.to_lowercase();
    let search_lower = search_term.to_lowercase();

    let mut last_end = 0;
    let mut search_start = 0;

    // Find all occurrences of the search term (case-insensitive)
    while let Some(pos) = text_lower[search_start..].find(&search_lower) {
        let actual_pos = search_start + pos;

        // Add text before the match (if any)
        if actual_pos > last_end {
            spans.push(Span::styled(
                text[last_end..actual_pos].to_string(),
                normal_style,
            ));
        }

        // Add the highlighted match (preserve original case)
        let match_end = actual_pos + search_term.len();
        spans.push(Span::styled(
            text[actual_pos..match_end].to_string(),
            highlight_style,
        ));

        last_end = match_end;
        search_start = match_end;
    }

    // Add remaining text after the last match
    if last_end < text.len() {
        spans.push(Span::styled(text[last_end..].to_string(), normal_style));
    }

    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::{Color, Style};

    #[test]
    fn test_basic_highlighting() {
        let normal = Style::default().fg(Color::White);
        let highlight = Style::default().fg(Color::Yellow);

        let result = highlight_search_term("build.rs", "bu", normal, highlight);

        // Should have 2 spans: highlighted "bu" + normal "ild.rs"
        assert_eq!(result.spans.len(), 2);
    }

    #[test]
    fn test_case_insensitive() {
        let normal = Style::default().fg(Color::White);
        let highlight = Style::default().fg(Color::Yellow);

        let result = highlight_search_term("Buffer.rs", "buf", normal, highlight);

        // Should highlight "Buf" (preserving case) in "Buffer.rs"
        assert_eq!(result.spans.len(), 2);
        assert_eq!(result.spans[0].content, "Buf");
    }

    #[test]
    fn test_multiple_matches() {
        let normal = Style::default().fg(Color::White);
        let highlight = Style::default().fg(Color::Yellow);

        let result = highlight_search_term("test_test.rs", "test", normal, highlight);

        // Should have 3 spans: "test" + "_" + "test" + ".rs"
        assert_eq!(result.spans.len(), 4);
    }
}
