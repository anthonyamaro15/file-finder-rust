use std::{
    collections::VecDeque,
    fs,
    io::{self},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, SystemTime},
};

use log::debug;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::{style::Color, widgets::Paragraph};

// Use centralized theme colors for markdown
use crate::ui::theme::markdown as markdown_colors;
use crate::utils::files::{EntryInfo, Listing};
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

    /// Check if a path has a valid cached preview (file hasn't been modified)
    pub fn contains(&self, path: &str) -> bool {
        let current_mtime = fs::metadata(path).ok().and_then(|m| m.modified().ok());
        self.entries
            .iter()
            .any(|e| e.path == path && e.modified_time == current_mtime)
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
                "rs" | "py" | "js" | "ts" | "jsx" | "tsx" | "c" | "cpp" | "cc" | "cxx" | "h"
                | "hpp" | "java" | "go" | "php" | "rb" | "scala" | "kt" | "swift" | "dart"
                | "css" | "scss" | "less" | "html" | "htm" | "xml" | "vue" | "svelte" | "sql"
                | "sh" | "bash" | "zsh" | "fish" | "ps1" | "bat" => FileType::SourceCode,

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
                "exe" | "dll" | "so" | "dylib" | "bin" | "app" | "deb" | "rpm" | "dmg" | "iso"
                | "img" | "msi" => FileType::Binary,

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
                    format!(
                        "{}\n\n... (truncated, {} more lines)",
                        preview,
                        total_lines - 200
                    )
                } else {
                    preview
                }
            }
        }
        Err(e) => {
            // Check for common PDF issues
            let error_str = e.to_string();
            if error_str.contains("password") || error_str.contains("encrypted") {
                "PDF file is password-protected.\n\nUnable to extract text without the password."
                    .to_string()
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

    for line in content.lines() {
        if line.starts_with("```") {
            in_code_block = !in_code_block;
            if in_code_block {
                let code_block_lang = line.trim_start_matches('`').to_string();
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
            if line
                .chars()
                .all(|c| c == '|' || c == '-' || c == ':' || c == ' ')
            {
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
    (start..chars.len()).find(|&i| chars[i] == delim && !is_escaped(chars, i))
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
                let mut items: Vec<(String, EntryInfo)> = entries
                    .filter_map(|e| e.ok())
                    .map(|e| {
                        let path = e.path();
                        let info = EntryInfo::read(&path);
                        (path.to_string_lossy().into_owned(), info)
                    })
                    .collect();
                items.sort_by(|(a, _), (b, _)| a.cmp(b));
                let (files, info) = items.into_iter().unzip();
                let _ = sender.send(PreviewMessage::DirectoryListing {
                    path,
                    listing: Listing { files, info },
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
    if matches!(
        file_type,
        FileType::Binary | FileType::ZIP | FileType::Archive
    ) {
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
                        // Terminals don't draw tab characters, so expand them
                        lines.push(line.replace('\t', "    "));
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
            let theme = self
                .theme_set
                .themes
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

        let curr_extension_type =
            file_extension.map(|extension| extension.to_owned().into_string().unwrap());

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
                    list.push(format!(
                        "{} ({})",
                        fil_path.display(),
                        Self::format_file_size(size)
                    ));
                }
            }
        }

        // Add truncation message if there are more entries
        if total_entries > MAX_ARCHIVE_ENTRIES {
            list.push(format!(
                "... and {} more entries",
                total_entries - MAX_ARCHIVE_ENTRIES
            ));
        }

        self.curr_zip_content = list;
        0
    }

    /// Modification time stored for a ZIP entry: the UTC extended timestamp if present,
    /// otherwise the DOS date/time read as local time, like `unzip`.
    fn zip_modified_time(file: &zip::read::ZipFile<'_>) -> Option<SystemTime> {
        for field in file.extra_data_fields() {
            match field {
                zip::extra_fields::ExtraField::ExtendedTimestamp(timestamp) => {
                    if let Some(seconds) = timestamp.mod_time() {
                        return SystemTime::UNIX_EPOCH
                            .checked_add(Duration::from_secs(u64::from(seconds)));
                    }
                }
            }
        }

        #[cfg(unix)]
        {
            let date = file.last_modified()?;
            // DOS timestamps have no zone: interpret them as local time, like unzip.
            // SAFETY: zero initializes all tm fields, including platform-specific fields.
            let mut local: libc::tm = unsafe { std::mem::zeroed() };
            local.tm_year = i32::from(date.year()) - 1900;
            local.tm_mon = i32::from(date.month()) - 1;
            local.tm_mday = i32::from(date.day());
            local.tm_hour = i32::from(date.hour());
            local.tm_min = i32::from(date.minute());
            local.tm_sec = i32::from(date.second());
            // Let mktime determine daylight saving time for this date.
            local.tm_isdst = -1;
            // SAFETY: mktime receives a valid, initialized tm and retains no pointer.
            let seconds = unsafe { libc::mktime(&mut local) };
            SystemTime::UNIX_EPOCH.checked_add(Duration::from_secs(seconds.try_into().ok()?))
        }
        #[cfg(not(unix))]
        None
    }

    /// Extract the ZIP at `path` into a new sibling directory named after it.
    /// Returns the success message, or an error message if anything failed.
    pub fn extract_zip_content(path: &str) -> Result<String, String> {
        Self::extract_zip_content_cancellable(path, &AtomicBool::new(false))
    }

    /// Folder a ZIP is extracted into: beside it, named after it without the extension.
    pub fn zip_extraction_dir(path: &Path) -> Option<PathBuf> {
        Some(path.with_file_name(path.file_stem()?))
    }

    /// Stop between entries and remove only the directory reserved by this extraction.
    pub fn extract_zip_content_cancellable(
        path: &str,
        cancel: &AtomicBool,
    ) -> Result<String, String> {
        let fname = Path::new(path);
        let file = fs::File::open(fname).map_err(|e| format!("Error opening ZIP: {}", e))?;
        let mut archive =
            zip::ZipArchive::new(file).map_err(|e| format!("Error reading ZIP archive: {}", e))?;
        let target_dir = Self::zip_extraction_dir(fname)
            .ok_or_else(|| "ZIP path has no file stem".to_string())?;
        // Atomically reserve a new directory; files and dangling links also block extraction.
        fs::create_dir(&target_dir).map_err(|e| {
            format!(
                "Error creating extraction directory '{}': {}",
                target_dir.display(),
                e
            )
        })?;

        let mut extracted = 0;
        let mut failed = 0;
        let mut first_error = None;
        let mut directory_times = Vec::new();
        for i in 0..archive.len() {
            if cancel.load(Ordering::Relaxed) {
                return Err(crate::operations::copy::remove_partial_copy(
                    &target_dir,
                    anyhow::anyhow!("Extraction cancelled"),
                )
                .to_string());
            }
            let result = (|| -> io::Result<bool> {
                let mut file = archive.by_index(i)?;
                let enclosed_name = match file.enclosed_name() {
                    Some(path) => path.to_owned(),
                    None => return Ok(false),
                };
                let outpath = target_dir.join(&enclosed_name);
                if file
                    .unix_mode()
                    .is_some_and(|mode| mode & 0o170000 == 0o120000)
                {
                    return Err(io::Error::other(format!(
                        "Refusing ZIP symlink '{}'",
                        file.name()
                    )));
                }

                // Check each directory component without following links, including directories
                // already created for earlier entries. Never merge into an existing symlink.
                let directory = if file.is_dir() {
                    enclosed_name.as_path()
                } else {
                    enclosed_name.parent().unwrap_or(Path::new(""))
                };
                let mut parent = target_dir.clone();
                for component in directory.components() {
                    parent.push(component);
                    match fs::create_dir(&parent) {
                        Ok(()) => {}
                        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                            if !fs::symlink_metadata(&parent)?.is_dir() {
                                return Err(io::Error::other(format!(
                                    "Not a directory: {}",
                                    parent.display()
                                )));
                            }
                        }
                        Err(e) => return Err(e),
                    }
                }
                let modified = Self::zip_modified_time(&file);
                if !file.is_dir() {
                    // Exclusive creation also refuses duplicate entries and dangling symlinks.
                    let mut outfile = fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(&outpath)?;
                    io::copy(&mut file, &mut outfile)?;
                    if let Some(modified) = modified {
                        if let Err(e) =
                            outfile.set_times(fs::FileTimes::new().set_modified(modified))
                        {
                            debug!(
                                "Failed to set ZIP modification time for '{}': {}",
                                outpath.display(),
                                e
                            );
                        }
                    }
                }
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    if let Some(mode) = file.unix_mode() {
                        // Drop setuid/setgid/sticky bits, like unzip does by default.
                        fs::set_permissions(&outpath, fs::Permissions::from_mode(mode & 0o777))?;
                    }
                }
                if file.is_dir() {
                    if let Some(modified) = modified {
                        directory_times.push((outpath, modified));
                    }
                }
                Ok(true)
            })();
            match result {
                Ok(true) => extracted += 1,
                Ok(false) => {}
                Err(e) => {
                    failed += 1;
                    first_error.get_or_insert_with(|| format!("entry {}: {}", i, e));
                }
            }
        }
        // Creating children changes directory mtimes, so restore explicit entries last.
        directory_times.sort_by_key(|(path, _)| std::cmp::Reverse(path.components().count()));
        for (path, modified) in directory_times {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            if let Err(e) = fs::File::open(&path)
                .and_then(|file| file.set_times(fs::FileTimes::new().set_modified(modified)))
            {
                debug!(
                    "Failed to set ZIP modification time for '{}': {}",
                    path.display(),
                    e
                );
            }
        }
        if cancel.load(Ordering::Relaxed) {
            return Err(crate::operations::copy::remove_partial_copy(
                &target_dir,
                anyhow::anyhow!("Extraction cancelled"),
            )
            .to_string());
        }
        let message = format!(
            "Extracted {} entries to {}",
            extracted,
            target_dir.display()
        );
        match first_error {
            Some(error) => Err(format!(
                "{}; {} failed (first error: {})",
                message, failed, error
            )),
            None => Ok(message),
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

#[cfg(test)]
mod extraction_tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    fn write_zip(path: &Path, entries: &[(&str, &str)]) {
        let mut archive = zip::ZipWriter::new(fs::File::create(path).unwrap());
        for (name, contents) in entries {
            archive
                .start_file(*name, SimpleFileOptions::default())
                .unwrap();
            archive.write_all(contents.as_bytes()).unwrap();
        }
        archive.finish().unwrap();
    }

    fn extract(path: &Path) -> Result<String, String> {
        FileContent::extract_zip_content(path.to_str().unwrap())
    }

    #[cfg(unix)]
    #[test]
    fn zip_preserves_local_dos_mtime() {
        // Winter and summer instants exercise local time and daylight-saving inference.
        for timestamp in [946_730_096, 962_454_896] {
            let timestamp: libc::time_t = timestamp;
            // SAFETY: zero is valid for tm, and both pointers refer to live values.
            let mut local: libc::tm = unsafe { std::mem::zeroed() };
            assert!(!unsafe { libc::localtime_r(&timestamp, &mut local) }.is_null());
            let date = zip::DateTime::from_date_and_time(
                (local.tm_year + 1900) as u16,
                (local.tm_mon + 1) as u8,
                local.tm_mday as u8,
                local.tm_hour as u8,
                local.tm_min as u8,
                local.tm_sec as u8,
            )
            .unwrap();
            let tmp = tempfile::tempdir().unwrap();
            let archive = tmp.path().join("dated.zip");
            let mut writer = zip::ZipWriter::new(fs::File::create(&archive).unwrap());
            writer
                .start_file(
                    "file.txt",
                    SimpleFileOptions::default().last_modified_time(date),
                )
                .unwrap();
            writer.write_all(b"contents").unwrap();
            writer.finish().unwrap();

            extract(&archive).unwrap();

            assert_eq!(
                fs::metadata(tmp.path().join("dated/file.txt"))
                    .unwrap()
                    .modified()
                    .unwrap(),
                SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(timestamp as u64)
            );
        }
    }

    #[test]
    fn zip_prefers_extended_mtime_for_files_and_directories() {
        let tmp = tempfile::tempdir().unwrap();
        let archive = tmp.path().join("dated.zip");
        let mut writer = zip::ZipWriter::new(fs::File::create(&archive).unwrap());
        let entries = [
            ("nested/", 1_234_567_891_u32),
            ("nested/deep/", 1_234_567_893),
            ("nested/deep/file.txt", 1_234_567_895),
        ];
        for (name, timestamp) in entries {
            let mut options = zip::write::FullFileOptions::default().last_modified_time(
                zip::DateTime::from_date_and_time(2001, 1, 1, 0, 0, 0).unwrap(),
            );
            let mut extra = vec![1]; // The extended timestamp contains an mtime.
            extra.extend_from_slice(&timestamp.to_le_bytes());
            options
                .add_extra_data(0x5455, extra.into_boxed_slice(), false)
                .unwrap();
            if name.ends_with('/') {
                writer.add_directory(name, options).unwrap();
            } else {
                writer.start_file(name, options).unwrap();
                writer.write_all(b"contents").unwrap();
            }
        }
        writer.finish().unwrap();

        extract(&archive).unwrap();

        for (name, timestamp) in entries {
            assert_eq!(
                fs::metadata(tmp.path().join("dated").join(name))
                    .unwrap()
                    .modified()
                    .unwrap(),
                SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(u64::from(timestamp)),
                "{name}"
            );
        }
    }

    #[test]
    fn cancelled_extraction_removes_target_and_preserves_archive() {
        let tmp = tempfile::tempdir().unwrap();
        let archive = tmp.path().join("archive.zip");
        let target = tmp.path().join("archive");
        write_zip(&archive, &[("a.txt", "alpha"), ("sub/b.txt", "beta")]);
        let original = fs::read(&archive).unwrap();
        let cancel = AtomicBool::new(true);
        let error =
            FileContent::extract_zip_content_cancellable(archive.to_str().unwrap(), &cancel)
                .unwrap_err();
        assert!(error.contains("cancelled"), "{error}");
        assert!(!target.exists());
        assert_eq!(fs::read(&archive).unwrap(), original);

        fs::create_dir(&target).unwrap();
        fs::write(target.join("keep"), "unrelated").unwrap();
        assert!(
            FileContent::extract_zip_content_cancellable(archive.to_str().unwrap(), &cancel)
                .is_err()
        );
        assert_eq!(
            fs::read_to_string(target.join("keep")).unwrap(),
            "unrelated"
        );
    }

    #[test]
    fn zip_extracts_beside_archive() {
        let tmp = tempfile::tempdir().unwrap();
        let archive = tmp.path().join("archive.zip");
        let entries = [
            ("a.txt", "alpha"),
            ("sub/b.txt", "beta"),
            (".zshrc", "safe"),
        ];
        write_zip(&archive, &entries);
        let target = tmp.path().join("archive");
        let message = extract(&archive).unwrap();
        assert!(message.contains("Extracted 3 entries"), "{message}");
        assert!(message.contains(target.to_str().unwrap()), "{message}");
        for (name, contents) in entries {
            assert_eq!(fs::read_to_string(target.join(name)).unwrap(), contents);
            assert!(!tmp.path().join(name).exists());
        }
        assert_eq!(fs::read_dir(tmp.path()).unwrap().count(), 2);
    }

    #[test]
    fn zip_rejects_existing_target() {
        let tmp = tempfile::tempdir().unwrap();
        let archive = tmp.path().join("archive.zip");
        write_zip(&archive, &[]);
        let target = tmp.path().join("archive");
        fs::create_dir(&target).unwrap();
        fs::write(target.join("keep.txt"), "keep").unwrap();
        assert!(extract(&archive).is_err());
        assert_eq!(fs::read_to_string(target.join("keep.txt")).unwrap(), "keep");
        assert_eq!(fs::read_dir(&target).unwrap().count(), 1);
    }

    #[test]
    fn zip_skips_parent_traversal() {
        let tmp = tempfile::tempdir().unwrap();
        let archive = tmp.path().join("archive.zip");
        write_zip(&archive, &[("../evil.txt", "evil"), ("good.txt", "good")]);
        let message = extract(&archive).unwrap();
        assert!(message.contains("Extracted 1 entries"), "{message}");
        assert!(!tmp.path().join("evil.txt").exists());
        assert_eq!(
            fs::read_to_string(tmp.path().join("archive/good.txt")).unwrap(),
            "good"
        );
    }

    #[test]
    fn zip_reports_entry_failures() {
        let tmp = tempfile::tempdir().unwrap();
        let archive = tmp.path().join("archive.zip");
        write_zip(
            &archive,
            &[("sub", "original"), ("sub/child.txt", "blocked")],
        );
        let message = extract(&archive).unwrap_err();
        assert!(message.contains("1 entries"), "{message}");
        assert!(message.contains("1 failed"), "{message}");
        assert_eq!(
            fs::read_to_string(tmp.path().join("archive/sub")).unwrap(),
            "original"
        );
    }

    #[cfg(unix)]
    #[test]
    fn zip_rejects_symlink_target_and_entries() {
        use std::os::unix::fs::symlink;
        let tmp = tempfile::tempdir().unwrap();
        let archive = tmp.path().join("archive.zip");
        let outside = tmp.path().join("outside");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("keep.txt"), "keep").unwrap();
        let mut writer = zip::ZipWriter::new(fs::File::create(&archive).unwrap());
        writer
            .add_symlink("link", "../outside", SimpleFileOptions::default())
            .unwrap();
        writer.finish().unwrap();
        let target = tmp.path().join("archive");
        for link_target in [&outside, &tmp.path().join("missing")] {
            symlink(link_target, &target).unwrap();
            assert!(extract(&archive).is_err());
            assert_eq!(fs::read_link(&target).unwrap(), *link_target);
            fs::remove_file(&target).unwrap();
        }
        let message = extract(&archive).unwrap_err();
        assert!(message.contains("1 failed"), "{message}");
        assert!(fs::symlink_metadata(target.join("link")).is_err());
        assert_eq!(
            fs::read_to_string(outside.join("keep.txt")).unwrap(),
            "keep"
        );
    }

    #[cfg(unix)]
    #[test]
    fn zip_strips_setuid_bits() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let archive = tmp.path().join("archive.zip");
        let mut bytes = Vec::new();
        let mut writer = zip::ZipWriter::new(io::Cursor::new(&mut bytes));
        writer
            .start_file("tool", SimpleFileOptions::default().unix_permissions(0o755))
            .unwrap();
        writer.write_all(b"#!/bin/sh\n").unwrap();
        writer.finish().unwrap();
        // unix_permissions() masks to 0o777, so set setuid in the central directory's
        // external attributes (offset 38; the mode is the high 16 bits).
        let attrs = bytes.windows(4).position(|w| w == b"PK\x01\x02").unwrap() + 38;
        let mode = u32::from_le_bytes(bytes[attrs..attrs + 4].try_into().unwrap()) | 0o4000 << 16;
        bytes[attrs..attrs + 4].copy_from_slice(&mode.to_le_bytes());
        fs::write(&archive, bytes).unwrap();
        let message = extract(&archive).unwrap();
        assert!(message.contains("Extracted 1 entries"), "{message}");
        let mode = fs::metadata(tmp.path().join("archive/tool"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o7777, 0o755);
    }
}

#[cfg(test)]
mod preview_tests {
    use super::*;
    use crate::app::PreviewMessage;

    #[test]
    fn tsv_preview_expands_tabs() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("table.tsv");
        fs::write(&path, "id\tvalue\n1\tfoo\n").unwrap();

        let (tx, rx) = std::sync::mpsc::channel();
        load_preview_async(path.to_string_lossy().to_string(), tx);

        match rx.recv().unwrap() {
            PreviewMessage::Loaded {
                content, file_type, ..
            } => {
                assert!(matches!(file_type, FileType::CSV));
                assert_eq!(content, "id    value\n1    foo");
            }
            _ => panic!("expected a loaded preview"),
        }
    }
}
