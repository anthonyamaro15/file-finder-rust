use std::{
    collections::VecDeque,
    fs,
    io::{self},
    path::Path,
    time::SystemTime,
};

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::{
    style::Color,
    widgets::Paragraph,
};

// Use centralized theme colors for markdown
use crate::ui::theme::markdown as markdown_colors;
use syntect::easy::HighlightLines;
use syntect::highlighting::{Style as SyntectStyle, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

/// Default maximum number of cached previews
const DEFAULT_CACHE_SIZE: usize = 50;

/// Maximum file size to attempt preview at all (5MB) - larger files show placeholder
const MAX_PREVIEW_FILE_SIZE: u64 = 5 * 1024 * 1024;

/// Maximum lines to display in preview
const MAX_PREVIEW_LINES: usize = 100;

/// Maximum archive entries to display in preview
const MAX_ARCHIVE_ENTRIES: usize = 100;

/// Maximum CSV rows to display in preview
const MAX_CSV_ROWS: usize = 100;

/// Maximum bytes to read for hex preview of binary files
const MAX_HEX_PREVIEW_BYTES: usize = 512;

/// A cached preview entry storing highlighted lines for a file
#[derive(Clone)]
struct PreviewCacheEntry {
    path: String,
    modified_time: Option<SystemTime>,
    /// Cached highlighted lines - each line contains styled spans (text, style)
    highlighted_lines: Vec<Line<'static>>,
}

/// LRU cache for file previews
pub struct PreviewCache {
    entries: VecDeque<PreviewCacheEntry>,
    max_size: usize,
}

impl PreviewCache {
    pub fn new(max_size: usize) -> Self {
        PreviewCache {
            entries: VecDeque::with_capacity(max_size),
            max_size,
        }
    }

    /// Get cached preview if it exists and file hasn't been modified
    pub fn get(&mut self, path: &str) -> Option<Vec<Line<'static>>> {
        // Check if file modification time matches
        let current_mtime = fs::metadata(path).ok().and_then(|m| m.modified().ok());

        // Find the entry
        let position = self.entries.iter().position(|e| e.path == path)?;

        let entry = &self.entries[position];

        // Check if file was modified since caching
        if entry.modified_time != current_mtime {
            // File was modified, remove stale entry
            self.entries.remove(position);
            return None;
        }

        // Move to front (most recently used)
        let entry = self.entries.remove(position)?;
        let lines = entry.highlighted_lines.clone();
        self.entries.push_front(entry);

        Some(lines)
    }

    /// Insert a new preview into the cache
    pub fn insert(&mut self, path: String, lines: Vec<Line<'static>>) {
        // Remove existing entry for this path if any
        if let Some(pos) = self.entries.iter().position(|e| e.path == path) {
            self.entries.remove(pos);
        }

        // Get file modification time
        let modified_time = fs::metadata(&path).ok().and_then(|m| m.modified().ok());

        // Add new entry at front
        self.entries.push_front(PreviewCacheEntry {
            path,
            modified_time,
            highlighted_lines: lines,
        });

        // Evict oldest if over capacity
        while self.entries.len() > self.max_size {
            self.entries.pop_back();
        }
    }

    /// Clear all cached entries
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Get current cache size
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if cache is empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Check if a path has a valid cached preview (file hasn't been modified)
    pub fn contains(&self, path: &str) -> bool {
        let current_mtime = fs::metadata(path).ok().and_then(|m| m.modified().ok());
        self.entries.iter().any(|e| e.path == path && e.modified_time == current_mtime)
    }
}
#[derive(Debug, Clone)]
pub enum FileType {
    SourceCode,
    Markdown,
    TextFile,
    ConfigFile,
    CSV,
    JSON,
    ZIP,
    Archive,
    Image,
    Binary,
    PDF,
    NotAvailable,
}

/// Standalone file type detection that can be called from any thread
pub fn detect_file_type(path: &str) -> FileType {
    let file_path = Path::new(path);
    let file_extension = file_path.extension();

    match file_extension {
        Some(extension) => {
            let ext = extension.to_str().unwrap_or("").to_lowercase();

            match ext.as_str() {
                // Source code files
                "rs" | "py" | "js" | "ts" | "jsx" | "tsx" | "c" | "cpp" | "cc" | "cxx"
                | "h" | "hpp" | "java" | "go" | "php" | "rb" | "scala" | "kt" | "swift"
                | "dart" | "css" | "scss" | "less" | "html" | "htm" | "xml" | "vue"
                | "svelte" | "sql" | "sh" | "bash" | "zsh" | "fish" | "ps1" | "bat" => {
                    FileType::SourceCode
                }

                // Markdown and documentation
                "md" | "markdown" | "rst" | "adoc" | "asciidoc" => FileType::Markdown,

                // Text files
                "txt" | "log" | "logs" | "out" | "err" | "readme" | "license" | "authors"
                | "changelog" | "news" | "todo" | "notes" => FileType::TextFile,

                // Configuration files
                "toml" | "yaml" | "yml" | "ini" | "conf" | "config" | "cfg" | "env"
                | "properties" | "plist" | "dockerfile" | "makefile" | "cmake" => {
                    FileType::ConfigFile
                }

                // Data files
                "json" => FileType::JSON,
                "csv" | "tsv" => FileType::CSV,

                // Archive files
                "zip" | "7z" | "rar" | "tar" | "gz" | "bz2" | "xz" | "tgz" | "tbz2" | "txz" => {
                    if ext == "zip" {
                        FileType::ZIP
                    } else {
                        FileType::Archive
                    }
                }

                // Image files
                "png" | "jpg" | "jpeg" | "gif" | "bmp" | "tiff" | "tif" | "svg" | "webp"
                | "ico" | "icns" => FileType::Image,

                // PDF files
                "pdf" => FileType::PDF,

                // Binary and executable files
                "exe" | "dll" | "so" | "dylib" | "bin" | "app" | "deb" | "rpm" | "dmg"
                | "iso" | "img" | "msi" => FileType::Binary,

                _ => detect_file_type_by_content_standalone(path),
            }
        }
        None => {
            // Files without extension - check common names
            let filename = file_path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("");

            match filename.to_lowercase().as_str() {
                "makefile" | "dockerfile" | "rakefile" | "gemfile" | "procfile" => {
                    FileType::ConfigFile
                }
                "readme" | "license" | "authors" | "changelog" | "news" | "todo" => {
                    FileType::TextFile
                }
                _ => detect_file_type_by_content_standalone(path),
            }
        }
    }
}

/// Detect file type by content for files without clear extensions (standalone version)
fn detect_file_type_by_content_standalone(path: &str) -> FileType {
    if let Ok(mut file) = fs::File::open(path) {
        let mut buffer = [0; 512];
        if let Ok(bytes_read) = io::Read::read(&mut file, &mut buffer) {
            if bytes_read > 0 {
                // Check for binary content
                let null_count = buffer[..bytes_read].iter().filter(|&&b| b == 0).count();
                let non_printable_count = buffer[..bytes_read]
                    .iter()
                    .filter(|&&b| b < 32 && b != 9 && b != 10 && b != 13)
                    .count();

                if null_count > 0 || non_printable_count as f32 / bytes_read as f32 > 0.1 {
                    return FileType::Binary;
                }

                return FileType::TextFile;
            }
        }
    }
    FileType::NotAvailable
}

/// Maximum PDF file size for preview (50MB)
const MAX_PDF_SIZE_STANDALONE: u64 = 50 * 1024 * 1024;

/// Read PDF content in a standalone function (can be called from any thread)
pub fn read_pdf_content_standalone(path: &str) -> String {
    // Check file size before attempting to extract
    if let Ok(metadata) = fs::metadata(path) {
        if metadata.len() > MAX_PDF_SIZE_STANDALONE {
            return format!(
                "PDF file is too large for preview ({:.1} MB).\n\nMaximum size: 50 MB\n\nUse an external PDF viewer.",
                metadata.len() as f64 / (1024.0 * 1024.0)
            );
        }
    }

    match pdf_extract::extract_text(path) {
        Ok(text) => {
            if text.trim().is_empty() {
                "PDF file contains no extractable text.\n\nThis may be a scanned document or image-based PDF.".to_string()
            } else {
                // Limit preview to 200 lines
                let all_lines: Vec<&str> = text.lines().collect();
                let total_lines = all_lines.len();
                let preview_lines: Vec<&str> = all_lines.into_iter().take(200).collect();
                let preview = preview_lines.join("\n");

                if total_lines > 200 {
                    format!("{}\n\n... (truncated, {} more lines)", preview, total_lines - 200)
                } else {
                    preview
                }
            }
        }
        Err(e) => {
            // Check for common PDF issues
            let error_str = e.to_string();
            if error_str.contains("password") || error_str.contains("encrypted") {
                "PDF file is password-protected.\n\nUnable to extract text without the password.".to_string()
            } else {
                format!("Unable to read PDF content.\n\nError: {}\n\nTry opening with an external PDF viewer.", e)
            }
        }
    }
}

/// Maximum image dimension for preview
const MAX_IMAGE_DIMENSION_ASYNC: u32 = 8192;

/// Load image asynchronously and send result through channel
pub fn load_image_async(
    path: String,
    sender: std::sync::mpsc::Sender<crate::app::ImageLoadResult>,
) {
    use crate::app::ImageLoadResult;
    use image::ImageReader;

    let result = match ImageReader::open(&path) {
        Ok(reader) => {
            let format = reader
                .format()
                .map(|f| format!("{:?}", f))
                .unwrap_or_else(|| "Unknown".to_string());

            match reader.decode() {
                Ok(decoded_image) => {
                    let width = decoded_image.width();
                    let height = decoded_image.height();

                    if width > MAX_IMAGE_DIMENSION_ASYNC || height > MAX_IMAGE_DIMENSION_ASYNC {
                        Err(format!(
                            "Image too large for preview ({}x{}, max {})",
                            width, height, MAX_IMAGE_DIMENSION_ASYNC
                        ))
                    } else {
                        Ok((decoded_image, format))
                    }
                }
                Err(e) => Err(format!("Failed to decode: {}", e)),
            }
        }
        Err(e) => Err(format!("Failed to open: {}", e)),
    };

    let _ = sender.send(ImageLoadResult { path, result });
}

/// Render markdown content to styled ratatui Lines
pub fn render_markdown(content: &str) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut in_code_block = false;
    let mut code_block_lang = String::new();

    for line in content.lines() {
        if line.starts_with("```") {
            in_code_block = !in_code_block;
            if in_code_block {
                code_block_lang = line.trim_start_matches('`').to_string();
                // Show language indicator if present
                if !code_block_lang.is_empty() {
                    lines.push(Line::from(Span::styled(
                        format!("─── {} ───", code_block_lang),
                        Style::default().fg(markdown_colors::CODE),
                    )));
                } else {
                    lines.push(Line::from(Span::styled(
                        "───────────",
                        Style::default().fg(markdown_colors::CODE),
                    )));
                }
            } else {
                lines.push(Line::from(Span::styled(
                    "───────────",
                    Style::default().fg(markdown_colors::CODE),
                )));
            }
            continue;
        }

        if in_code_block {
            // Code block content - monospace style with background hint
            lines.push(Line::from(Span::styled(
                format!("  {}", line),
                Style::default()
                    .fg(markdown_colors::CODE)
                    .bg(markdown_colors::CODE_BG),
            )));
            continue;
        }

        // Headers
        if line.starts_with("#### ") {
            lines.push(Line::from(Span::styled(
                line.trim_start_matches('#').trim().to_string(),
                Style::default()
                    .fg(markdown_colors::HEADER_4)
                    .add_modifier(Modifier::BOLD),
            )));
        } else if line.starts_with("### ") {
            lines.push(Line::from(Span::styled(
                line.trim_start_matches('#').trim().to_string(),
                Style::default()
                    .fg(markdown_colors::HEADER_3)
                    .add_modifier(Modifier::BOLD),
            )));
        } else if line.starts_with("## ") {
            lines.push(Line::from(Span::styled(
                line.trim_start_matches('#').trim().to_string(),
                Style::default()
                    .fg(markdown_colors::HEADER_2)
                    .add_modifier(Modifier::BOLD),
            )));
        } else if line.starts_with("# ") {
            lines.push(Line::from(Span::styled(
                line.trim_start_matches('#').trim().to_string(),
                Style::default()
                    .fg(markdown_colors::HEADER_1)
                    .add_modifier(Modifier::BOLD),
            )));
        }
        // Horizontal rule
        else if line.trim() == "---" || line.trim() == "***" || line.trim() == "___" {
            lines.push(Line::from(Span::styled(
                "────────────────────────────────────────",
                Style::default().fg(markdown_colors::HORIZONTAL_RULE),
            )));
        }
        // Blockquote
        else if line.starts_with("> ") {
            let quote_content = line.trim_start_matches('>').trim();
            lines.push(Line::from(vec![
                Span::styled("│ ", Style::default().fg(markdown_colors::BLOCKQUOTE)),
                Span::styled(
                    quote_content.to_string(),
                    Style::default()
                        .fg(markdown_colors::BLOCKQUOTE)
                        .add_modifier(Modifier::ITALIC),
                ),
            ]));
        }
        // Unordered list
        else if line.trim_start().starts_with("- ")
            || line.trim_start().starts_with("* ")
            || line.trim_start().starts_with("+ ")
        {
            let indent = line.len() - line.trim_start().len();
            let content = line.trim_start().get(2..).unwrap_or("");
            let indent_str = " ".repeat(indent);
            lines.push(Line::from(vec![
                Span::raw(indent_str),
                Span::styled("• ", Style::default().fg(markdown_colors::LIST_MARKER)),
                Span::raw(render_inline_markdown(content)),
            ]));
        }
        // Ordered list
        else if let Some(rest) = parse_ordered_list_item(line) {
            let indent = line.len() - line.trim_start().len();
            let indent_str = " ".repeat(indent);
            lines.push(Line::from(vec![
                Span::raw(indent_str),
                Span::styled(
                    format!("{}. ", rest.0),
                    Style::default().fg(markdown_colors::LIST_MARKER),
                ),
                Span::raw(render_inline_markdown(rest.1)),
            ]));
        }
        // Table row
        else if line.contains('|') && line.trim().starts_with('|') {
            // Check if it's a separator row
            if line.chars().all(|c| c == '|' || c == '-' || c == ':' || c == ' ') {
                lines.push(Line::from(Span::styled(
                    line.to_string(),
                    Style::default().fg(markdown_colors::TABLE_BORDER),
                )));
            } else {
                // Regular table row - style the pipes
                let styled_line = render_table_row(line);
                lines.push(styled_line);
            }
        }
        // Regular text with inline formatting
        else {
            lines.push(render_inline_markdown_line(line));
        }
    }

    lines
}

/// Parse an ordered list item (e.g., "1. item") and return (number, content)
fn parse_ordered_list_item(line: &str) -> Option<(String, &str)> {
    let trimmed = line.trim_start();
    let mut num_end = 0;
    for (i, c) in trimmed.char_indices() {
        if c.is_ascii_digit() {
            num_end = i + 1;
        } else if c == '.' && num_end > 0 {
            if let Some(rest) = trimmed.get(num_end + 1..) {
                return Some((trimmed[..num_end].to_string(), rest.trim_start()));
            }
        } else {
            break;
        }
    }
    None
}

/// Render a table row with styled pipes
fn render_table_row(line: &str) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    for part in line.split('|') {
        if !spans.is_empty() {
            spans.push(Span::styled(
                "│",
                Style::default().fg(markdown_colors::TABLE_BORDER),
            ));
        }
        spans.push(Span::raw(part.to_string()));
    }
    Line::from(spans)
}

/// Render inline markdown (bold, italic, code, links) to a single string
/// This is a simplified version that just returns plain text for list items
fn render_inline_markdown(text: &str) -> String {
    // For simplicity, just return the text as-is
    // Full inline parsing would be more complex
    text.to_string()
}

/// Render a line with inline markdown formatting
fn render_inline_markdown_line(line: &str) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut current_text = String::new();
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        // Check for inline code
        if chars[i] == '`' && !is_escaped(&chars, i) {
            // Find closing backtick
            if let Some(end) = find_closing(&chars, i + 1, '`') {
                if !current_text.is_empty() {
                    spans.push(Span::raw(current_text.clone()));
                    current_text.clear();
                }
                let code: String = chars[i + 1..end].iter().collect();
                spans.push(Span::styled(
                    code,
                    Style::default()
                        .fg(markdown_colors::CODE)
                        .bg(markdown_colors::CODE_BG),
                ));
                i = end + 1;
                continue;
            }
        }

        // Check for bold (**text**)
        if i + 1 < chars.len() && chars[i] == '*' && chars[i + 1] == '*' {
            if let Some(end) = find_double_closing(&chars, i + 2, '*') {
                if !current_text.is_empty() {
                    spans.push(Span::raw(current_text.clone()));
                    current_text.clear();
                }
                let bold: String = chars[i + 2..end].iter().collect();
                spans.push(Span::styled(
                    bold,
                    Style::default()
                        .fg(markdown_colors::BOLD)
                        .add_modifier(Modifier::BOLD),
                ));
                i = end + 2;
                continue;
            }
        }

        // Check for italic (*text* or _text_)
        if (chars[i] == '*' || chars[i] == '_') && !is_escaped(&chars, i) {
            let delim = chars[i];
            if let Some(end) = find_closing(&chars, i + 1, delim) {
                // Make sure it's not bold (**)
                if delim == '*' && i + 1 < chars.len() && chars[i + 1] == '*' {
                    current_text.push(chars[i]);
                    i += 1;
                    continue;
                }
                if !current_text.is_empty() {
                    spans.push(Span::raw(current_text.clone()));
                    current_text.clear();
                }
                let italic: String = chars[i + 1..end].iter().collect();
                spans.push(Span::styled(
                    italic,
                    Style::default()
                        .fg(markdown_colors::ITALIC)
                        .add_modifier(Modifier::ITALIC),
                ));
                i = end + 1;
                continue;
            }
        }

        // Check for links [text](url)
        if chars[i] == '[' {
            if let Some((text, url, end)) = parse_link(&chars, i) {
                if !current_text.is_empty() {
                    spans.push(Span::raw(current_text.clone()));
                    current_text.clear();
                }
                spans.push(Span::styled(
                    text,
                    Style::default()
                        .fg(markdown_colors::LINK)
                        .add_modifier(Modifier::UNDERLINED),
                ));
                spans.push(Span::styled(
                    format!(" ({})", url),
                    Style::default().fg(markdown_colors::LINK_URL),
                ));
                i = end;
                continue;
            }
        }

        current_text.push(chars[i]);
        i += 1;
    }

    if !current_text.is_empty() {
        spans.push(Span::raw(current_text));
    }

    if spans.is_empty() {
        Line::from("")
    } else {
        Line::from(spans)
    }
}

/// Check if character at position is escaped
fn is_escaped(chars: &[char], pos: usize) -> bool {
    pos > 0 && chars[pos - 1] == '\\'
}

/// Find closing delimiter
fn find_closing(chars: &[char], start: usize, delim: char) -> Option<usize> {
    for i in start..chars.len() {
        if chars[i] == delim && !is_escaped(chars, i) {
            return Some(i);
        }
    }
    None
}

/// Find double closing delimiter (**)
fn find_double_closing(chars: &[char], start: usize, delim: char) -> Option<usize> {
    let mut i = start;
    while i + 1 < chars.len() {
        if chars[i] == delim && chars[i + 1] == delim && !is_escaped(chars, i) {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Parse a markdown link [text](url)
fn parse_link(chars: &[char], start: usize) -> Option<(String, String, usize)> {
    if chars[start] != '[' {
        return None;
    }

    // Find ]
    let mut bracket_end = None;
    for i in start + 1..chars.len() {
        if chars[i] == ']' && !is_escaped(chars, i) {
            bracket_end = Some(i);
            break;
        }
    }

    let bracket_end = bracket_end?;

    // Check for (
    if bracket_end + 1 >= chars.len() || chars[bracket_end + 1] != '(' {
        return None;
    }

    // Find )
    let mut paren_end = None;
    for i in bracket_end + 2..chars.len() {
        if chars[i] == ')' && !is_escaped(chars, i) {
            paren_end = Some(i);
            break;
        }
    }

    let paren_end = paren_end?;

    let text: String = chars[start + 1..bracket_end].iter().collect();
    let url: String = chars[bracket_end + 2..paren_end].iter().collect();

    Some((text, url, paren_end + 1))
}

/// Load file content asynchronously and send result through channel
pub fn load_preview_async(
    path: String,
    sender: std::sync::mpsc::Sender<crate::app::PreviewMessage>,
) {
    use crate::app::PreviewMessage;
    use std::io::{BufRead, BufReader};

    // Check if it's a directory
    let path_obj = Path::new(&path);
    if path_obj.is_dir() {
        // Load directory listing
        match fs::read_dir(&path) {
            Ok(entries) => {
                let mut items: Vec<String> = entries
                    .filter_map(|e| e.ok())
                    .map(|e| {
                        let name = e.file_name().to_string_lossy().to_string();
                        if e.path().is_dir() {
                            format!("{}/", name)
                        } else {
                            name
                        }
                    })
                    .collect();
                items.sort();
                let _ = sender.send(PreviewMessage::DirectoryListing {
                    path,
                    entries: items,
                });
            }
            Err(e) => {
                let _ = sender.send(PreviewMessage::Error {
                    path,
                    message: format!("Failed to read directory: {}", e),
                });
            }
        }
        return;
    }

    // Check file size first
    let file_size = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    if file_size > MAX_PREVIEW_FILE_SIZE {
        let _ = sender.send(PreviewMessage::Loaded {
            path: path.clone(),
            content: format!(
                "File too large for preview\n\nSize: {:.2} MB\nLimit: {} MB\n\nUse an external editor to view this file.",
                file_size as f64 / (1024.0 * 1024.0),
                MAX_PREVIEW_FILE_SIZE / (1024 * 1024)
            ),
            file_type: detect_file_type(&path),
            highlighted_content: None,
        });
        return;
    }

    // Detect file type
    let file_type = detect_file_type(&path);

    // For binary files, don't try to read as text (but PDF and Image are handled specially)
    if matches!(file_type, FileType::Binary | FileType::ZIP | FileType::Archive) {
        let _ = sender.send(PreviewMessage::Loaded {
            path,
            content: String::new(), // Content will be handled specially in main
            file_type,
            highlighted_content: None,
        });
        return;
    }

    // Image files are handled via separate channel (see load_image_async)
    if matches!(file_type, FileType::Image) {
        let _ = sender.send(PreviewMessage::Loaded {
            path,
            content: String::new(),
            file_type,
            highlighted_content: None,
        });
        return;
    }

    // Handle PDF files asynchronously
    if matches!(file_type, FileType::PDF) {
        let content = read_pdf_content_standalone(&path);
        let _ = sender.send(PreviewMessage::Loaded {
            path,
            content,
            file_type,
            highlighted_content: None,
        });
        return;
    }

    // Read text file with line limit
    match fs::File::open(&path) {
        Ok(file) => {
            let reader = BufReader::new(file);
            let mut lines: Vec<String> = Vec::new();
            let mut total_lines = 0;

            for line_result in reader.lines() {
                total_lines += 1;
                if lines.len() < MAX_PREVIEW_LINES {
                    if let Ok(line) = line_result {
                        lines.push(line);
                    }
                }
            }

            let mut content = lines.join("\n");
            if total_lines > MAX_PREVIEW_LINES {
                content.push_str(&format!(
                    "\n\n... ({} more lines, {} total)",
                    total_lines - MAX_PREVIEW_LINES,
                    total_lines
                ));
            }

            let _ = sender.send(PreviewMessage::Loaded {
                path,
                content,
                file_type,
                highlighted_content: None,
            });
        }
        Err(e) => {
            let _ = sender.send(PreviewMessage::Error {
                path,
                message: format!("Failed to read file: {}", e),
            });
        }
    }
}

pub struct FileContent<'a> {
    pub file_type: FileType,
    pub is_error: bool,
    pub error_message: String,
    pub curr_asset_path: String,
    pub curr_zip_content: Vec<String>,
    pub curr_selected_path: String,
    pub curr_csv_content: Vec<String>,
    pub curr_extension_tpe: Option<String>,
    pub syntax_set: SyntaxSet,
    pub theme_set: ThemeSet,
    pub hightlighted_content: Option<Paragraph<'a>>,
    /// LRU cache for highlighted file previews
    preview_cache: PreviewCache,
    /// Configured syntax highlighting theme name
    syntax_theme_name: String,
}

impl FileContent<'_> {
    pub fn new(ps: SyntaxSet, ts: ThemeSet) -> FileContent<'static> {
        FileContent {
            file_type: FileType::NotAvailable,
            is_error: false,
            error_message: String::from(""),
            curr_asset_path: String::from(""),
            curr_zip_content: Vec::new(),
            curr_selected_path: String::from(""),
            curr_csv_content: Vec::new(),
            curr_extension_tpe: None,
            syntax_set: ps,
            theme_set: ts,
            hightlighted_content: None,
            preview_cache: PreviewCache::new(DEFAULT_CACHE_SIZE),
            syntax_theme_name: "base16-ocean.dark".to_string(),
        }
    }

    /// Set the syntax highlighting theme
    /// Available themes: "base16-ocean.dark", "base16-eighties.dark", "base16-mocha.dark",
    /// "base16-ocean.light", "InspiredGitHub", "Solarized (dark)", "Solarized (light)"
    pub fn set_syntax_theme(&mut self, theme_name: &str) {
        // Validate theme exists, fall back to default if not found
        if self.theme_set.themes.contains_key(theme_name) {
            self.syntax_theme_name = theme_name.to_string();
            // Clear cache when theme changes so files are re-highlighted
            self.preview_cache.clear();
        }
    }

    /// Get list of available syntax themes
    pub fn available_themes(&self) -> Vec<&str> {
        self.theme_set.themes.keys().map(|s| s.as_str()).collect()
    }

    /// Clear the preview cache
    pub fn clear_preview_cache(&mut self) {
        self.preview_cache.clear();
    }

    /// Get the current number of cached previews
    pub fn cache_size(&self) -> usize {
        self.preview_cache.len()
    }

    /// Check if a path has a valid cached preview (without retrieving it)
    /// This is used to decide whether to use sync or async loading
    pub fn is_cached(&self, path: &str) -> bool {
        self.preview_cache.contains(path)
    }

    /// Get cached preview content directly (for cache hits)
    /// Returns true if cache was used, false if not cached
    pub fn try_use_cached_preview(&mut self, path: &str) -> bool {
        self.curr_selected_path = path.to_string();
        if let Some(cached_lines) = self.preview_cache.get(path) {
            let text = Text::from(cached_lines);
            let paragraph = Paragraph::new(text);
            self.hightlighted_content = Some(paragraph);
            true
        } else {
            false
        }
    }

    pub fn is_curr_path_file(path: String) -> bool {
        match fs::metadata(path) {
            Ok(file) => {
                let file_t = file.file_type();
                if file_t.is_file() {
                    true
                } else {
                    false
                }
            }
            Err(_) => false,
        }
    }

    fn convert_color(&mut self, color: syntect::highlighting::Color) -> Color {
        Color::Rgb(color.r, color.g, color.b)
    }

    pub fn get_highlighted_content(
        &mut self,
        content: String,
        extension_type: Option<String>,
    ) -> String {
        if extension_type.is_none() {
            return content;
        }

        let extension = extension_type.unwrap();
        let current_path = self.curr_selected_path.clone();

        // Check cache first - if we have a valid cached version, use it
        if let Some(cached_lines) = self.preview_cache.get(&current_path) {
            let text = Text::from(cached_lines);
            let paragraph = Paragraph::new(text);
            self.hightlighted_content = Some(paragraph);
            return String::new(); // Return value is unused, just return empty
        }

        // Cache miss - perform syntax highlighting
        // Try to find syntax by extension, with fallback options
        let syntax = self
            .syntax_set
            .find_syntax_by_extension(&extension)
            .or_else(|| {
                // Try some common mappings for extensions that might not be directly supported
                match extension.as_str() {
                    "ts" => self.syntax_set.find_syntax_by_extension("js"),
                    "tsx" => self.syntax_set.find_syntax_by_extension("jsx"),
                    "vue" => self.syntax_set.find_syntax_by_extension("html"),
                    "svelte" => self.syntax_set.find_syntax_by_extension("html"),
                    _ => None,
                }
            })
            .or_else(|| Some(self.syntax_set.find_syntax_plain_text()));

        if let Some(syntax) = syntax {
            let mut spans: Vec<Line<'static>> = vec![];
            let syntax_set = self.syntax_set.clone();
            // Use configured theme, fall back to default if not found
            let theme = self.theme_set.themes
                .get(&self.syntax_theme_name)
                .cloned()
                .unwrap_or_else(|| self.theme_set.themes["base16-ocean.dark"].clone());
            let mut h = HighlightLines::new(syntax, &theme);

            for line in LinesWithEndings::from(&content) {
                // LinesWithEndings enables use of newlines mode
                let mut line_spans: Vec<Span<'static>> = vec![];

                // Handle potential highlighting errors gracefully
                let ranges = match h.highlight_line(line, &syntax_set) {
                    Ok(ranges) => ranges,
                    Err(_) => {
                        // Fallback to plain text if highlighting fails
                        vec![(SyntectStyle::default(), line)]
                    }
                };

                for (style, text) in ranges.iter() {
                    let fg_color = self.convert_color(style.foreground);
                    let span = Span::styled(text.to_string(), Style::default().fg(fg_color));
                    line_spans.push(span);
                }
                spans.push(Line::from(line_spans));
            }

            // Store in cache before creating the Paragraph
            self.preview_cache.insert(current_path, spans.clone());

            let text = Text::from(spans);
            let paragraph = Paragraph::new(text);
            self.hightlighted_content = Some(paragraph);
            String::new() // Return value is unused
        } else {
            // If no syntax found, return content as-is and don't set highlighted_content
            self.is_error = true;
            self.error_message =
                format!("No syntax highlighting available for .{} files", extension);
            content
        }
    }

    /// Render markdown content with formatting (headers, bold, italic, code, links, etc.)
    pub fn render_markdown_content(&mut self, content: &str, path: &str) {
        self.curr_selected_path = path.to_string();

        // Check cache first
        if let Some(cached_lines) = self.preview_cache.get(path) {
            let text = Text::from(cached_lines);
            let paragraph = Paragraph::new(text);
            self.hightlighted_content = Some(paragraph);
            return;
        }

        // Render markdown
        let lines = render_markdown(content);

        // Store in cache
        self.preview_cache.insert(path.to_string(), lines.clone());

        let text = Text::from(lines);
        let paragraph = Paragraph::new(text);
        self.hightlighted_content = Some(paragraph);
    }

    pub fn read_file_content(&mut self, path: String) -> String {
        // Check if this is an image file first to avoid trying to read binary data as text
        let file_type = self.get_file_extension(path.clone());
        if matches!(file_type, FileType::Image | FileType::Binary) {
            return format!(
                "Cannot display {} file as text",
                match file_type {
                    FileType::Image => "image",
                    FileType::Binary => "binary",
                    _ => "unknown",
                }
            );
        }

        // Check file size first
        if let Ok(metadata) = fs::metadata(&path) {
            let file_size = metadata.len();

            // Skip preview entirely for very large files (> 5MB)
            if file_size > MAX_PREVIEW_FILE_SIZE {
                return self.create_large_file_placeholder(&path, file_size);
            }
        }

        // Always use line-limited reader for consistent performance
        // This ensures we never read more than 100 lines regardless of file size
        self.read_file_lines_limited(&path)
    }

    /// Create a placeholder message for files too large to preview
    fn create_large_file_placeholder(&self, path: &str, file_size: u64) -> String {
        let filename = Path::new(path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Unknown");

        let size_str = Self::format_file_size(file_size);

        format!(
            "  File too large to preview\n\n\
             \n\
               {}
             \n\n\
               Size: {}\n\
               Limit: 5 MB\n\n\
             \n\
               Press Enter to open in editor",
            filename, size_str
        )
    }

    /// Read only the first N lines of a file for preview (default: 100 lines)
    /// This is used for ALL files to ensure consistent performance
    fn read_file_lines_limited(&self, path: &str) -> String {
        use std::io::{BufRead, BufReader};

        let file = match fs::File::open(path) {
            Ok(f) => f,
            Err(e) => return format!("Error opening file: {}", e),
        };

        let total_size = file.metadata().map(|m| m.len()).unwrap_or(0);
        let reader = BufReader::new(file);
        let mut lines = Vec::with_capacity(MAX_PREVIEW_LINES);
        let mut total_lines_count = 0;
        let mut truncated = false;

        for line_result in reader.lines() {
            total_lines_count += 1;

            if lines.len() < MAX_PREVIEW_LINES {
                match line_result {
                    Ok(line) => lines.push(line),
                    Err(_) => break, // Stop on read error (likely binary content)
                }
            } else {
                truncated = true;
                // Count remaining lines without storing them
                if line_result.is_err() {
                    break;
                }
            }
        }

        let preview = lines.join("\n");

        if truncated {
            let size_str = Self::format_file_size(total_size);
            format!(
                "{}\n\n... ({} more lines, {} total)",
                preview,
                total_lines_count - MAX_PREVIEW_LINES,
                size_str
            )
        } else {
            preview
        }
    }

    /// Format file size for display
    fn format_file_size(size: u64) -> String {
        const UNITS: &[&str] = &["B", "KB", "MB", "GB"];
        let mut size = size as f64;
        let mut unit_index = 0;

        while size >= 1024.0 && unit_index < UNITS.len() - 1 {
            size /= 1024.0;
            unit_index += 1;
        }

        if unit_index == 0 {
            format!("{} {}", size as u64, UNITS[unit_index])
        } else {
            format!("{:.1} {}", size, UNITS[unit_index])
        }
    }

    pub fn get_file_extension_type(&mut self, path: String) -> Option<String> {
        let file_extension = Path::new(&path).extension();

        let curr_extension_type = match file_extension {
            Some(extension) => Some(extension.to_owned().into_string().unwrap()),
            None => None, // TODO: find out what would be the best default
        };

        //self.curr_extension_tpe = curr_extension_type;

        curr_extension_type
    }

    pub fn get_file_extension(&mut self, path: String) -> FileType {
        let file_extension = Path::new(&path).extension();

        match file_extension {
            Some(extension) => {
                let ext = extension.to_str().unwrap().to_lowercase();

                match ext.as_str() {
                    // Source code files
                    "rs" | "py" | "js" | "ts" | "jsx" | "tsx" | "c" | "cpp" | "cc" | "cxx"
                    | "h" | "hpp" | "java" | "go" | "php" | "rb" | "scala" | "kt" | "swift"
                    | "dart" | "css" | "scss" | "less" | "html" | "htm" | "xml" | "vue"
                    | "svelte" | "sql" | "sh" | "bash" | "zsh" | "fish" | "ps1" | "bat" => {
                        FileType::SourceCode
                    }

                    // Markdown and documentation
                    "md" | "markdown" | "rst" | "adoc" | "asciidoc" => FileType::Markdown,

                    // Text files
                    "txt" | "log" | "logs" | "out" | "err" | "readme" | "license" | "authors"
                    | "changelog" | "news" | "todo" | "notes" => FileType::TextFile,

                    // Configuration files
                    "toml" | "yaml" | "yml" | "ini" | "conf" | "config" | "cfg" | "env"
                    | "properties" | "plist" | "dockerfile" | "makefile" | "cmake" => {
                        FileType::ConfigFile
                    }

                    // Data files
                    "json" => FileType::JSON,
                    "csv" | "tsv" => FileType::CSV,

                    // Archive files
                    "zip" | "7z" | "rar" | "tar" | "gz" | "bz2" | "xz" | "tgz" | "tbz2" | "txz" => {
                        if ext == "zip" {
                            FileType::ZIP
                        } else {
                            FileType::Archive
                        }
                    }

                    // Image files
                    "png" | "jpg" | "jpeg" | "gif" | "bmp" | "tiff" | "tif" | "svg" | "webp"
                    | "ico" | "icns" => FileType::Image,

                    // PDF files
                    "pdf" => FileType::PDF,

                    // Binary and executable files
                    "exe" | "dll" | "so" | "dylib" | "bin" | "app" | "deb" | "rpm" | "dmg"
                    | "iso" | "img" | "msi" => FileType::Binary,

                    _ => {
                        // Check if it's a known binary file by content or treat as text
                        self.detect_file_type_by_content(&path)
                    }
                }
            }
            None => {
                // Files without extension - check common names or content
                let filename = Path::new(&path)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("");

                match filename.to_lowercase().as_str() {
                    "makefile" | "dockerfile" | "rakefile" | "gemfile" | "procfile" => {
                        FileType::ConfigFile
                    }
                    "readme" | "license" | "authors" | "changelog" | "news" | "todo" => {
                        FileType::TextFile
                    }
                    _ => self.detect_file_type_by_content(&path),
                }
            }
        }
    }

    /// Detect file type by examining file content for files without clear extensions
    fn detect_file_type_by_content(&self, path: &str) -> FileType {
        if let Ok(mut file) = fs::File::open(path) {
            let mut buffer = [0; 512]; // Read first 512 bytes
            if let Ok(bytes_read) = io::Read::read(&mut file, &mut buffer) {
                if bytes_read > 0 {
                    // Check for binary content (null bytes or high percentage of non-printable chars)
                    let null_count = buffer[..bytes_read].iter().filter(|&&b| b == 0).count();
                    let non_printable_count = buffer[..bytes_read]
                        .iter()
                        .filter(|&&b| b < 32 && b != 9 && b != 10 && b != 13)
                        .count();

                    if null_count > 0 || non_printable_count as f32 / bytes_read as f32 > 0.1 {
                        return FileType::Binary;
                    }

                    // Check for common file signatures
                    if buffer.starts_with(b"\x89PNG") {
                        return FileType::Image;
                    }
                    if buffer.starts_with(b"\xFF\xD8\xFF") {
                        return FileType::Image; // JPEG
                    }
                    if buffer.starts_with(b"GIF87a") || buffer.starts_with(b"GIF89a") {
                        return FileType::Image;
                    }
                    if buffer.starts_with(b"PK\x03\x04") {
                        return FileType::ZIP;
                    }
                    if buffer.starts_with(b"%PDF") {
                        return FileType::PDF;
                    }
                }
            }
        }

        // Default to text file if we can't determine otherwise
        FileType::TextFile
    }

    pub fn read_csv_content(&mut self) {
        let file = match fs::File::open(self.curr_selected_path.clone()) {
            Ok(f) => f,
            Err(_) => {
                self.curr_csv_content = vec!["Error: Could not open CSV file".to_string()];
                return;
            }
        };

        let mut rdr = csv::Reader::from_reader(file);

        let mut file_content: Vec<String> = Vec::new();
        let mut row_count = 0;
        let mut total_rows = 0;

        for result in rdr.records() {
            total_rows += 1;
            if row_count >= MAX_CSV_ROWS {
                continue; // Keep counting total but don't store more
            }
            // Skip records that fail to parse instead of crashing
            if let Ok(record) = result {
                for val in record.iter() {
                    file_content.push(val.to_string());
                }
                row_count += 1;
            }
        }

        // Add truncation message if there are more rows
        if total_rows > MAX_CSV_ROWS {
            file_content.push(format!("... and {} more rows", total_rows - MAX_CSV_ROWS));
        }

        self.curr_csv_content = file_content;
    }

    pub fn read_zip_content(&mut self, path: String) -> i32 {
        let filename = std::path::Path::new(&path);
        let file = match fs::File::open(filename) {
            Ok(f) => f,
            Err(_) => {
                self.curr_zip_content = vec!["Error: Could not open ZIP file".to_string()];
                return -1;
            }
        };

        let mut archive = match zip::ZipArchive::new(file) {
            Ok(a) => a,
            Err(_) => {
                self.curr_zip_content = vec!["Error: Invalid ZIP archive".to_string()];
                return -1;
            }
        };

        let mut list: Vec<String> = Vec::new();
        let total_entries = archive.len();
        let entries_to_show = total_entries.min(MAX_ARCHIVE_ENTRIES);

        for i in 0..entries_to_show {
            // Skip entries that fail to read
            if let Ok(file) = archive.by_index(i) {
                if let Some(fil_path) = file.enclosed_name() {
                    let size = file.size();
                    list.push(format!("{} ({})", fil_path.display(), Self::format_file_size(size)));
                }
            }
        }

        // Add truncation message if there are more entries
        if total_entries > MAX_ARCHIVE_ENTRIES {
            list.push(format!("... and {} more entries", total_entries - MAX_ARCHIVE_ENTRIES));
        }

        self.curr_zip_content = list;
        0
    }

    pub fn extract_zip_content(&mut self) -> String {
        let fname = std::path::Path::new(&self.curr_selected_path);
        let file = match fs::File::open(fname) {
            Ok(f) => f,
            Err(e) => return format!("Error opening ZIP: {}", e),
        };

        let mut archive = match zip::ZipArchive::new(file) {
            Ok(a) => a,
            Err(e) => return format!("Error reading ZIP archive: {}", e),
        };

        for i in 0..archive.len() {
            let mut file = match archive.by_index(i) {
                Ok(f) => f,
                Err(_) => continue, // Skip problematic entries
            };

            let outpath = match file.enclosed_name() {
                Some(f_path) => f_path.to_owned(),
                None => continue,
            };

            {
                let comment = file.comment();
                if !comment.is_empty() {
                    println!("File {i} comment: {comment}");
                }
            }

            if file.is_dir() {
                if let Err(e) = fs::create_dir_all(&outpath) {
                    eprintln!("Warning: Could not create directory {:?}: {}", outpath, e);
                    continue;
                }
            } else {
                if let Some(p) = outpath.parent() {
                    if !p.exists() {
                        if let Err(e) = fs::create_dir_all(p) {
                            eprintln!("Warning: Could not create parent directory {:?}: {}", p, e);
                            continue;
                        }
                    }
                }

                let mut outfile = match fs::File::create(&outpath) {
                    Ok(f) => f,
                    Err(e) => {
                        eprintln!("Warning: Could not create file {:?}: {}", outpath, e);
                        continue;
                    }
                };

                if let Err(e) = io::copy(&mut file, &mut outfile) {
                    eprintln!("Warning: Could not write to file {:?}: {}", outpath, e);
                }
            }

            {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    if let Some(mode) = file.unix_mode() {
                        let _ = fs::set_permissions(&outpath, fs::Permissions::from_mode(mode));
                    }
                }
            }
        }
        self.curr_selected_path.clone()
    }

    /// Maximum PDF file size for preview (50MB)
    const MAX_PDF_SIZE: u64 = 50 * 1024 * 1024;

    /// Read and extract text content from a PDF file
    pub fn read_pdf_content(&mut self, path: &str) -> String {
        // Check file size before attempting to extract
        if let Ok(metadata) = fs::metadata(path) {
            if metadata.len() > Self::MAX_PDF_SIZE {
                return format!(
                    "PDF file is too large for preview ({:.1} MB).\n\nMaximum size: 50 MB\n\nUse an external PDF viewer.",
                    metadata.len() as f64 / (1024.0 * 1024.0)
                );
            }
        }

        match pdf_extract::extract_text(path) {
            Ok(text) => {
                if text.trim().is_empty() {
                    "PDF file contains no extractable text.\n\nThis may be a scanned document or image-based PDF.".to_string()
                } else {
                    // Count lines once, then take for preview
                    let all_lines: Vec<&str> = text.lines().collect();
                    let total_lines = all_lines.len();
                    let preview_lines: Vec<&str> = all_lines.into_iter().take(200).collect();
                    let preview = preview_lines.join("\n");

                    if total_lines > 200 {
                        format!("{}\n\n... (truncated, {} more lines)", preview, total_lines - 200)
                    } else {
                        preview
                    }
                }
            }
            Err(e) => {
                // Check for common PDF issues
                let error_str = e.to_string();
                if error_str.contains("password") || error_str.contains("encrypted") {
                    "PDF file is password-protected.\n\nUnable to extract text without the password.".to_string()
                } else {
                    format!("Unable to read PDF content.\n\nError: {}\n\nTry opening with an external PDF viewer.", e)
                }
            }
        }
    }

    /// Read tar archive contents (limited entries for preview)
    pub fn read_tar_content(&mut self, path: &str) -> Vec<String> {
        use std::io::BufReader;
        use tar::Archive;

        let file = match fs::File::open(path) {
            Ok(f) => f,
            Err(e) => return vec![format!("Error: Could not open archive: {}", e)],
        };

        let mut archive = Archive::new(BufReader::new(file));
        let mut list = Vec::new();
        let mut count = 0;

        match archive.entries() {
            Ok(entries) => {
                for entry_result in entries {
                    if count >= MAX_ARCHIVE_ENTRIES {
                        list.push("... (more entries truncated)".to_string());
                        break;
                    }
                    if let Ok(entry) = entry_result {
                        if let Ok(entry_path) = entry.path() {
                            let size = entry.size();
                            list.push(format!(
                                "{} ({})",
                                entry_path.display(),
                                Self::format_file_size(size)
                            ));
                            count += 1;
                        }
                    }
                }
            }
            Err(e) => {
                return vec![format!("Error reading archive: {}", e)];
            }
        }

        if list.is_empty() {
            list.push("Empty or unreadable archive".to_string());
        }

        list
    }

    /// Read tar.gz archive contents (limited entries for preview)
    pub fn read_tar_gz_content(&mut self, path: &str) -> Vec<String> {
        use flate2::read::GzDecoder;
        use tar::Archive;

        let file = match fs::File::open(path) {
            Ok(f) => f,
            Err(e) => return vec![format!("Error: Could not open archive: {}", e)],
        };

        let decoder = GzDecoder::new(file);
        let mut archive = Archive::new(decoder);
        let mut list = Vec::new();
        let mut count = 0;

        match archive.entries() {
            Ok(entries) => {
                for entry_result in entries {
                    if count >= MAX_ARCHIVE_ENTRIES {
                        list.push("... (more entries truncated)".to_string());
                        break;
                    }
                    if let Ok(entry) = entry_result {
                        if let Ok(entry_path) = entry.path() {
                            let size = entry.size();
                            list.push(format!(
                                "{} ({})",
                                entry_path.display(),
                                Self::format_file_size(size)
                            ));
                            count += 1;
                        }
                    }
                }
            }
            Err(e) => {
                return vec![format!("Error reading archive: {}", e)];
            }
        }

        if list.is_empty() {
            list.push("Empty or unreadable archive".to_string());
        }

        list
    }

    /// Read tar.bz2 archive contents (limited entries for preview)
    /// Note: Requires bzip2 crate if needed - for now returns placeholder
    pub fn read_tar_bz2_content(&mut self, _path: &str) -> Vec<String> {
        vec!["tar.bz2 preview not yet implemented".to_string()]
    }

    /// Read tar.xz archive contents (limited entries for preview)
    /// Note: Requires xz2 crate if needed - for now returns placeholder
    pub fn read_tar_xz_content(&mut self, _path: &str) -> Vec<String> {
        vec!["tar.xz preview not yet implemented".to_string()]
    }

    /// Read binary file and return hex dump preview
    pub fn read_binary_hex_view(&self, path: &str) -> String {
        use pretty_hex::PrettyHex;
        use std::io::Read;

        let mut file = match fs::File::open(path) {
            Ok(f) => f,
            Err(e) => return format!("Error opening file: {}", e),
        };

        // Get file size for header
        let file_size = file.metadata().map(|m| m.len()).unwrap_or(0);

        // Read first N bytes
        let mut buffer = vec![0u8; MAX_HEX_PREVIEW_BYTES];
        let bytes_read = match file.read(&mut buffer) {
            Ok(n) => n,
            Err(e) => return format!("Error reading file: {}", e),
        };
        buffer.truncate(bytes_read);

        // Generate hex dump
        let hex_dump = format!("{:?}", buffer.hex_dump());

        // Format output with header
        let size_str = Self::format_file_size(file_size);
        let mut output = format!("Binary file ({}):\n\n", size_str);
        output.push_str(&hex_dump);

        if file_size > MAX_HEX_PREVIEW_BYTES as u64 {
            output.push_str(&format!(
                "\n\n... (showing first {} of {} bytes)",
                bytes_read, size_str
            ));
        }

        output
    }
}
