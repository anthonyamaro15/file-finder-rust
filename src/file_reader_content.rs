use std::{
    fs,
    io::{self},
    path::Path,
};

use ratatui::style::Style;
use ratatui::text::{Line, Span, Text};
use ratatui::{
    style::Color,
    widgets::Paragraph,
};
use syntect::easy::HighlightLines;
use syntect::highlighting::{Style as SyntectStyle, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::as_24_bit_terminal_escaped;
use syntect::util::LinesWithEndings;
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
    NotAvailable,
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
            let mut res = String::from("");
            let mut spans = vec![];
            let syntax_set = self.syntax_set.clone();
            let theme = self.theme_set.themes["base16-ocean.dark"].clone();
            let mut h = HighlightLines::new(syntax, &theme);

            for line in LinesWithEndings::from(&content) {
                // LinesWithEndings enables use of newlines mode
                let mut lines: Vec<Span> = vec![];

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
                    lines.push(span);
                }
                spans.push(Line::from(lines));

                let escaped = as_24_bit_terminal_escaped(&ranges[..], true);
                res = escaped.clone();
            }
            let text = Text::from(spans);

            let paragraph = Paragraph::new(text);
            self.hightlighted_content = Some(paragraph);
            res
        } else {
            // If no syntax found, return content as-is and don't set highlighted_content
            self.is_error = true;
            self.error_message =
                format!("No syntax highlighting available for .{} files", extension);
            content
        }
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

        let content = match fs::read_to_string(path) {
            Ok(file_content) => file_content,
            Err(err) => {
                let err_kind = err.kind().to_string();
                let format_error = format!("Encounter Error: '{}'", err_kind);
                self.is_error = true;
                self.error_message = String::from("");
                return format_error;
            }
        };
        content
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
        for result in rdr.records() {
            // Skip records that fail to parse instead of crashing
            if let Ok(record) = result {
                for val in record.iter() {
                    file_content.push(val.to_string());
                }
            }
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

        for i in 0..archive.len() {
            // Skip entries that fail to read
            if let Ok(file) = archive.by_index(i) {
                if let Some(fil_path) = file.enclosed_name() {
                    list.push(fil_path.display().to_string());
                }
            }
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
}
