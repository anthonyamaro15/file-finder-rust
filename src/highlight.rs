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
    let mut last_end = 0;

    // Find all occurrences of the search term (case-insensitive)
    for (actual_pos, _) in text.char_indices() {
        if actual_pos < last_end {
            continue;
        }

        let mut chars = text[actual_pos..].char_indices();
        let mut match_end = actual_pos;
        let matches = search_term.chars().all(|search_char| {
            if let Some((offset, text_char)) = chars.next() {
                match_end = actual_pos + offset + text_char.len_utf8();
                text_char.to_lowercase().eq(search_char.to_lowercase())
            } else {
                false
            }
        });
        if !matches {
            continue;
        }

        // Add text before the match (if any)
        if actual_pos > last_end {
            spans.push(Span::styled(
                text[last_end..actual_pos].to_string(),
                normal_style,
            ));
        }

        // Add the highlighted match (preserve original case)
        spans.push(Span::styled(
            text[actual_pos..match_end].to_string(),
            highlight_style,
        ));

        last_end = match_end;
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
    fn test_unicode_lowercase_expansion_before_match() {
        let normal = Style::default();
        let highlight = Style::default().fg(Color::Yellow);
        let result = highlight_search_term("İé.txt", "é", normal, highlight);

        assert_eq!(
            result.spans,
            vec![
                Span::styled("İ", normal),
                Span::styled("é", highlight),
                Span::styled(".txt", normal),
            ]
        );
    }

    #[test]
    fn test_mixed_case_ascii_preserves_original_text() {
        let normal = Style::default();
        let highlight = Style::default().fg(Color::Yellow);
        let result = highlight_search_term("aBuF_buf.rs", "bUf", normal, highlight);

        assert_eq!(
            result.spans,
            vec![
                Span::styled("a", normal),
                Span::styled("BuF", highlight),
                Span::styled("_", normal),
                Span::styled("buf", highlight),
                Span::styled(".rs", normal),
            ]
        );
    }

    #[test]
    fn test_unicode_match_at_end() {
        let normal = Style::default();
        let highlight = Style::default().fg(Color::Yellow);
        let result = highlight_search_term("İ.txtÉ", "é", normal, highlight);

        assert_eq!(
            result.spans,
            vec![Span::styled("İ.txt", normal), Span::styled("É", highlight),]
        );
    }

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
