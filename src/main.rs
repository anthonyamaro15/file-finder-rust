use app::{App, InputMode};
use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use file_reader_content::{FileContent, FileType};
use image::ImageReader;
use rayon::prelude::*;
use std::{
    fs::{self, File, Metadata},
    io::{self, ErrorKind, Stdout},
    path::{Path, PathBuf},
    process::Command,
    sync::{mpsc, Arc, Mutex},
    sync::atomic::{AtomicUsize, Ordering},
    thread,
};
use syntect::{highlighting::ThemeSet, parsing::SyntaxSet};
use walkdir::WalkDir;

use ratatui::{prelude::*, widgets::Clear};

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    style::{Color, Style},
    text::Text,
    widgets::{Block, Borders, List, ListState, Paragraph},
    Terminal,
};

mod utils;

use crate::{
    cli::{compute_effective_config, CliArgs},
    directory_store::{build_directory_from_store_async, load_directory_from_file},
    errors::{validation, FileOperationError, FileOperationResult},
    status_bar::StatusBar,
    theme::OneDarkTheme,
    ui::Ui,
    utils::init,
};
use log::{debug, warn};

extern crate copypasta;
use copypasta::{ClipboardContext, ClipboardProvider};

mod app;
mod cli;
mod config;
mod configuration;
mod directory_store;
mod errors;
mod file_reader_content;
mod highlight;
mod status_bar;
mod theme;
mod ui;
mod watcher;

#[derive(Clone)]
enum SortType {
    ASC,
    DESC,
}

#[derive(Clone)]
enum SortBy {
    Name,
    Size,
    DateAdded,
    Default,
}

#[derive(Clone)]
struct ImageGenerator {
    has_image: bool,
    image_info: String,
}

impl ImageGenerator {
    pub fn new() -> ImageGenerator {
        ImageGenerator {
            has_image: false,
            image_info: String::new(),
        }
    }

    pub fn load_img(&mut self, path: String) {
        match ImageReader::open(&path) {
            Ok(reader) => {
                // Save format before calling decode since decode consumes reader
                let format = reader
                    .format()
                    .map(|f| format!("{:?}", f))
                    .unwrap_or_else(|| "Unknown".to_string());
                match reader.decode() {
                    Ok(dyn_img) => {
                        let width = dyn_img.width();
                        let height = dyn_img.height();

                        self.image_info = format!(
                            "Image Preview\n\nDimensions: {}x{}\nFormat: {}\n\nNote: Image rendering in terminal is limited.\nUse external viewer for full image experience.",
                            width, height, format
                        );
                        self.has_image = true;
                    }
                    Err(e) => {
                        debug!("Failed to decode image {}: {}", path, e);
                        self.has_image = false;
                        self.image_info = format!("Failed to decode image: {}", e);
                    }
                }
            }
            Err(e) => {
                debug!("Failed to open image {}: {}", path, e);
                self.has_image = false;
                self.image_info = format!("Failed to open image: {}", e);
            }
        }
    }

    pub fn clear(&mut self) {
        self.has_image = false;
        self.image_info.clear();
    }
}

fn sort_entries_by_type(
    sort_by: SortBy,
    sort_type: SortType,
    mut entries: Vec<PathBuf>,
) -> Vec<PathBuf> {
    match sort_type {
        SortType::ASC => match sort_by {
            SortBy::Name => entries.sort_by(|a, b| {
                a.file_name()
                    .unwrap()
                    .to_ascii_lowercase()
                    .cmp(&b.file_name().unwrap().to_ascii_lowercase())
            }),
            SortBy::Size => entries.sort_by(|a, b| {
                a.metadata()
                    .ok()
                    .map(|meta| meta.len())
                    .unwrap_or(0)
                    .cmp(&b.metadata().ok().map(|meta| meta.len()).unwrap_or(0))
            }),

            SortBy::DateAdded => entries.sort_by(|a, b| {
                a.metadata()
                    .ok()
                    .and_then(|meta| meta.created().ok())
                    .unwrap_or(std::time::SystemTime::now())
                    .cmp(
                        &b.metadata()
                            .ok()
                            .and_then(|meta| meta.created().ok())
                            .unwrap_or(std::time::SystemTime::now()),
                    )
            }),
            _ => {}
        },
        SortType::DESC => match sort_by {
            SortBy::Name => entries.sort_by(|a, b| {
                b.file_name()
                    .unwrap()
                    .to_ascii_lowercase()
                    .cmp(&a.file_name().unwrap().to_ascii_lowercase())
            }),
            SortBy::Size => entries.sort_by(|a, b| {
                b.metadata()
                    .ok()
                    .map(|meta| meta.len())
                    .unwrap_or(0)
                    .cmp(&a.metadata().ok().map(|meta| meta.len()).unwrap_or(0))
            }),
            SortBy::DateAdded => entries.sort_by(|a, b| {
                b.metadata()
                    .ok()
                    .and_then(|meta| meta.created().ok())
                    .unwrap_or(std::time::SystemTime::now())
                    .cmp(
                        &a.metadata()
                            .ok()
                            .and_then(|meta| meta.created().ok())
                            .unwrap_or(std::time::SystemTime::now()),
                    )
            }),
            _ => {}
        },
    }

    entries
}

// Optimized parallel file processing with rayon
fn convert_file_path_to_string(
    entries: Vec<PathBuf>,
    show_hidden: bool,
    sort_by: SortBy,
    sort_type: SortType,
) -> Vec<String> {
    use rayon::prelude::*;

    let sort_entries = sort_entries_by_type(sort_by, sort_type, entries);

    // Filter and process files in parallel
    let filtered_entries: Vec<PathBuf> = sort_entries
        .into_par_iter()
        .filter(|value| {
            if value.is_dir() {
                true
            } else if value.is_file() {
                if !show_hidden {
                    value
                        .file_name()
                        .and_then(|name| name.to_str())
                        .map(|name| !name.starts_with("."))
                        .unwrap_or(false)
                } else {
                    true
                }
            } else {
                false
            }
        })
        .collect();

    // Convert to strings in parallel
    filtered_entries
        .into_par_iter()
        .filter_map(|entry| entry.to_str().map(|s| s.to_string()))
        .collect()
}

fn handle_file_selection(
    file: &str,
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &App,
) -> anyhow::Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    terminal.clear()?;

    let ide = app.get_selected_ide();
    if ide.is_some() {
        let selected_ide = ide.unwrap();

        if Path::new(file).exists() {
            let output = Command::new(selected_ide.to_owned())
                .arg(file.to_owned())
                .status()
                .expect("Failed to open file");

            if output.success() {
                println!("Successfully opened file with {}", selected_ide);
            } else {
                println!("Failed to open file with {}", selected_ide);
            }
        }
    } else {
        let mut ctx = ClipboardContext::new().unwrap();
        ctx.set_contents(file.to_owned()).unwrap();
    }

    Ok(())
}

fn get_inner_files_info(
    file: String,
    show_hidden_files: bool,
    sort_by: SortBy,
    sort_type: &SortType,
) -> anyhow::Result<Option<Vec<String>>> {
    let entries = match fs::read_dir(file) {
        Ok(en) => {
            let val = en.map(|res| res.map(|e| e.path())).collect();
            match val {
                Ok(v) => v,
                Err(e) => {
                    println!("Error: {}", e);
                    return Ok(None);
                }
            }
        }
        Err(e) => {
            println!("Error: {}", e);
            return Ok(None);
        }
    };

    let file_strings =
        convert_file_path_to_string(entries, show_hidden_files, sort_by, sort_type.clone());
    Ok(Some(file_strings))
}

fn get_content_from_path(path: String) -> Option<Vec<String>> {
    let mut file_name_list: Vec<String> = Vec::new();
    match fs::read_dir(path) {
        Ok(val) => {
            for name in val.into_iter() {
                match name {
                    Ok(result) => {
                        let file_name = result.file_name().to_str().unwrap().to_string();
                        file_name_list.push(file_name);
                    }
                    Err(e) => {
                        println!("error getting content from path: {:?}", e);
                        return None;
                    }
                }
            }
        }
        Err(e) => {
            println!("her: {:?}", e);
            return None;
        }
    };
    Some(file_name_list)
}

fn draw_popup(rect: Rect, percent_x: u16, percent_y: u16) -> Rect {
    let popup_layout = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .split(rect);

    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(popup_layout[1])[1]
}

fn delete_file(file_path: &str) -> FileOperationResult<()> {
    let path = Path::new(file_path);

    // Validate file exists
    validation::validate_path_exists(path)?;

    // Validate permissions
    validation::validate_permissions(path, "delete")?;

    // Attempt to delete the file
    fs::remove_file(path).map_err(|e| match e.kind() {
        io::ErrorKind::PermissionDenied => {
            FileOperationError::permission_denied(path, "Cannot delete file - check permissions")
        }
        io::ErrorKind::NotFound => FileOperationError::file_not_found(path),
        _ => FileOperationError::from(e),
    })?;

    Ok(())
}

fn delete_dir(dir_path: &str) -> FileOperationResult<()> {
    let path = Path::new(dir_path);

    // Validate directory exists
    validation::validate_path_exists(path)?;

    // Validate permissions
    validation::validate_permissions(path, "delete")?;

    // Attempt to delete the directory
    fs::remove_dir_all(path).map_err(|e| match e.kind() {
        io::ErrorKind::PermissionDenied => FileOperationError::permission_denied(
            path,
            "Cannot delete directory - check permissions",
        ),
        io::ErrorKind::NotFound => FileOperationError::file_not_found(path),
        _ => FileOperationError::from(e),
    })?;

    Ok(())
}

fn handle_delete_based_on_type(file_path: &str) -> FileOperationResult<()> {
    let path = Path::new(file_path);

    // Validate file exists
    validation::validate_path_exists(path)?;

    let metadata = fs::metadata(path).map_err(|e| match e.kind() {
        io::ErrorKind::PermissionDenied => {
            FileOperationError::permission_denied(path, "Cannot read file information")
        }
        io::ErrorKind::NotFound => FileOperationError::file_not_found(path),
        _ => FileOperationError::from(e),
    })?;

    if metadata.is_dir() {
        delete_dir(file_path)
    } else {
        delete_file(file_path)
    }
}

fn get_file_path_data(
    start_path: String,
    show_hidden: bool,
    sort_by: SortBy,
    sort_type: &SortType,
) -> anyhow::Result<Vec<String>> {
    let entries = fs::read_dir(start_path)?
        .map(|res| res.map(|e| e.path()))
        .collect::<Result<Vec<_>, io::Error>>()?;

    let file_strings =
        convert_file_path_to_string(entries, show_hidden, sort_by, sort_type.clone());

    Ok(file_strings)
}

fn create_new_dir(current_file_path: String, new_item: String) -> FileOperationResult<()> {
    // Validate the directory name
    validation::validate_filename(&new_item)?;

    let append_path = format!("{}/{}", current_file_path, new_item);
    let path = Path::new(&append_path);

    // Validate that the directory doesn't already exist
    validation::validate_path_not_exists(path)?;

    // Validate parent directory permissions
    validation::validate_permissions(path, "create")?;

    // Create the directory
    fs::create_dir(path).map_err(|e| match e.kind() {
        io::ErrorKind::PermissionDenied => FileOperationError::permission_denied(
            path,
            "Cannot create directory - check permissions",
        ),
        io::ErrorKind::AlreadyExists => FileOperationError::already_exists(path),
        io::ErrorKind::NotFound => {
            FileOperationError::file_not_found(Path::new(&current_file_path))
        }
        _ => FileOperationError::from(e),
    })?;

    Ok(())
}

fn create_new_file(current_file_path: String, file_name: String) -> FileOperationResult<()> {
    // Validate the filename
    validation::validate_filename(&file_name)?;

    let append_path = format!("{}/{}", current_file_path, file_name);
    let path = Path::new(&append_path);

    // Validate that the file doesn't already exist
    validation::validate_path_not_exists(path)?;

    // Validate parent directory permissions
    validation::validate_permissions(path, "create")?;

    // Create the file
    File::create_new(path).map_err(|e| match e.kind() {
        io::ErrorKind::PermissionDenied => {
            FileOperationError::permission_denied(path, "Cannot create file - check permissions")
        }
        io::ErrorKind::AlreadyExists => FileOperationError::already_exists(path),
        io::ErrorKind::NotFound => {
            FileOperationError::file_not_found(Path::new(&current_file_path))
        }
        _ => FileOperationError::from(e),
    })?;

    Ok(())
}

fn is_file(path: String) -> bool {
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

fn get_metadata_info(path: String) -> anyhow::Result<Option<Metadata>> {
    let metadata = match fs::metadata(path) {
        Ok(info) => Some(info),
        Err(_) => None,
    };

    Ok(metadata)
}

fn format_file_size(size: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
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

fn format_system_time(time: std::time::SystemTime) -> String {
    use std::time::UNIX_EPOCH;

    match time.duration_since(UNIX_EPOCH) {
        Ok(duration) => {
            let secs = duration.as_secs();
            let days = secs / 86400;
            let hours = (secs % 86400) / 3600;
            let minutes = (secs % 3600) / 60;

            if days > 0 {
                format!("{}d ago", days)
            } else if hours > 0 {
                format!("{}h ago", hours)
            } else if minutes > 0 {
                format!("{}m ago", minutes)
            } else {
                "just now".to_string()
            }
        }
        Err(_) => "unknown".to_string(),
    }
}

fn generate_metadata_str_info(metadata: anyhow::Result<Option<Metadata>>) -> String {
    let metadata_info = match metadata {
        Ok(res) => match res {
            Some(info) => {
                let size = format_file_size(info.len());
                let permissions = info.permissions();
                let readonly = if permissions.readonly() { "RO" } else { "RW" };

                // Try to get modification time
                let modified = info
                    .modified()
                    .map(format_system_time)
                    .unwrap_or_else(|_| "unknown".to_string());

                format!("{} | {} | modified {}", size, readonly, modified)
            }
            None => String::from("Info not available"),
        },
        Err(_) => String::from("Error reading metadata"),
    };

    metadata_info
}

fn generate_copy_file_dir_name(curr_path: String, new_path: String) -> String {
    let get_info = Path::new(&curr_path);
    let file_name = get_info.file_name().unwrap().to_str().unwrap().to_string();

    // Generate a unique name by prefixing copy_ repeatedly until it does not exist
    let mut copies = 1usize;
    loop {
        let prefix = "copy_".repeat(copies);
        let candidate = format!("{}/{}{}", new_path, prefix, file_name);
        if !Path::new(&candidate).exists() {
            return candidate;
        }
        copies += 1;
    }
}

fn create_item_based_on_type(
    current_file_path: String,
    new_item: String,
) -> FileOperationResult<()> {
    if new_item.contains(".") {
        create_new_file(current_file_path, new_item)
    } else {
        create_new_dir(current_file_path, new_item)
    }
}

fn handle_rename(app: &App) -> FileOperationResult<()> {
    // Validate the new filename
    validation::validate_filename(&app.create_edit_file_name)?;

    let curr_path = format!("{}/{}", app.current_path_to_edit, app.current_name_to_edit);
    let new_path = format!("{}/{}", app.current_path_to_edit, app.create_edit_file_name);

    let old_path = Path::new(&curr_path);
    let new_path_obj = Path::new(&new_path);

    // Validate the old file exists
    validation::validate_path_exists(old_path)?;

    // Validate the new path doesn't already exist
    validation::validate_path_not_exists(new_path_obj)?;

    // Validate permissions for rename operation
    validation::validate_permissions(new_path_obj, "rename")?;

    // Attempt the rename
    fs::rename(old_path, new_path_obj).map_err(|e| match e.kind() {
        io::ErrorKind::PermissionDenied => {
            FileOperationError::permission_denied(old_path, "Cannot rename - check permissions")
        }
        io::ErrorKind::NotFound => FileOperationError::file_not_found(old_path),
        io::ErrorKind::AlreadyExists => FileOperationError::already_exists(new_path_obj),
        _ => FileOperationError::from(e),
    })?;

    Ok(())
}

fn check_if_exists(new_path: String) -> bool {
    match Path::new(&new_path).try_exists() {
        Ok(value) => value,
        Err(_) => {
            // If we can't determine existence, assume it doesn't exist
            false
        }
    }
}

fn get_curr_path(path: String) -> String {
    let mut split_path = path.split("/").collect::<Vec<&str>>();
    split_path.pop();
    let vec_to_str = split_path.join("/");
    vec_to_str
}

fn copy_dir_file_helper(src: &Path, new_src: &Path) -> anyhow::Result<()> {
    // Check if source exists
    if !src.exists() {
        return Err(anyhow::anyhow!(
            "Source path does not exist: {}",
            src.display()
        ));
    }

    // Check if destination already exists
    if new_src.exists() {
        return Err(anyhow::anyhow!(
            "Destination already exists: {}",
            new_src.display()
        ));
    }

    if src.is_file() {
        // Ensure parent directory exists
        if let Some(parent) = new_src.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| anyhow::anyhow!("Failed to create parent directory: {}", e))?;
        }

        fs::copy(src, new_src).map_err(|e| {
            anyhow::anyhow!(
                "Failed to copy file '{}' to '{}': {}",
                src.display(),
                new_src.display(),
                e
            )
        })?;
    } else if src.is_dir() {
        // Create the destination directory
        fs::create_dir_all(new_src)
            .map_err(|e| anyhow::anyhow!("Failed to create destination directory: {}", e))?;

        let entries: Vec<_> = WalkDir::new(src)
            .into_iter()
            .filter_map(|entry| match entry {
                Ok(e) => Some(e),
                Err(err) => {
                    warn!("Skipping entry due to error: {}", err);
                    None
                }
            })
            .collect();

        entries.par_iter().try_for_each(|entry| {
            let entry_path = entry.path();
            let relative_path = entry_path.strip_prefix(src).map_err(|e| {
                io::Error::new(ErrorKind::Other, format!("Failed to strip prefix: {}", e))
            })?;
            let dst_path = new_src.join(relative_path);

            if entry_path.is_dir() {
                fs::create_dir_all(&dst_path).map_err(|e| {
                    io::Error::new(
                        ErrorKind::Other,
                        format!("Failed to create directory '{}': {}", dst_path.display(), e),
                    )
                })?;
            } else if entry_path.is_file() {
                if let Some(parent) = dst_path.parent() {
                    fs::create_dir_all(parent).map_err(|e| {
                        io::Error::new(
                            ErrorKind::Other,
                            format!("Failed to create parent directory: {}", e),
                        )
                    })?;
                }
                fs::copy(entry_path, &dst_path).map_err(|e| {
                    io::Error::new(
                        ErrorKind::Other,
                        format!(
                            "Failed to copy file '{}' to '{}': {}",
                            entry_path.display(),
                            dst_path.display(),
                            e
                        ),
                    )
                })?;
            } else {
                // Skip special files (symlinks, devices, etc.)
                warn!("Skipping unsupported file type: {}", entry_path.display());
            }

            Ok::<(), io::Error>(())
        })?;
    } else {
        return Err(anyhow::anyhow!(
            "Source is neither a file nor a directory: {}",
            src.display()
        ));
    }

    Ok(())
}

// Progress message for copy operations
#[derive(Debug, Clone)]
enum CopyMessage {
    Progress {
        files_processed: usize,
        total_files: usize,
        current_file: String,
    },
    Completed {
        success: bool,
        message: String,
    },
    Error(String),
}

// Fast file copy helpers optimized for macOS/APFS
#[cfg(target_os = "macos")]
fn fast_copy_file(src: &Path, dst: &Path) -> io::Result<()> {
    // Allow disabling clonefile/copyfile via env for baseline comparisons
    let disable_clone = std::env::var("FF_DISABLE_CLONEFILE").ok().filter(|v| v == "1").is_some();
    if !disable_clone {
        use std::{ffi::CString, os::unix::ffi::OsStrExt};
        unsafe {
            let src_c = CString::new(src.as_os_str().as_bytes())
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
            let dst_c = CString::new(dst.as_os_str().as_bytes())
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;

            // 1) Try APFS clone-on-write for same-volume instant copy
            if libc::clonefile(src_c.as_ptr(), dst_c.as_ptr(), 0) == 0 {
                return Ok(());
            }

            // 2) Fallback to kernel-optimized copy with metadata preservation
            // Compose flags equivalent to COPYFILE_ALL (ACL | STAT | XATTR | DATA | SECURITY)
            let flags = libc::COPYFILE_ACL
                | libc::COPYFILE_STAT
                | libc::COPYFILE_XATTR
                | libc::COPYFILE_DATA
                | libc::COPYFILE_SECURITY;
            if libc::copyfile(
                src_c.as_ptr(),
                dst_c.as_ptr(),
                std::ptr::null_mut(),
                flags,
            ) == 0
            {
                return Ok(());
            }
        }
    }

    // 3) Last resort (or when disabled)
    std::fs::copy(src, dst).map(|_| ())
}

#[cfg(not(target_os = "macos"))]
fn fast_copy_file(src: &Path, dst: &Path) -> io::Result<()> {
    std::fs::copy(src, dst).map(|_| ())
}

// Async copy function with progress tracking
fn copy_dir_file_with_progress(src: &Path, new_src: &Path) -> mpsc::Receiver<CopyMessage> {
    let (tx, rx) = mpsc::channel();
    let src = src.to_path_buf();
    let new_src = new_src.to_path_buf();

    thread::spawn(move || {
        // Check if source exists
        if !src.exists() {
            let _ = tx.send(CopyMessage::Error(format!(
                "Source path does not exist: {}",
                src.display()
            )));
            return;
        }

        // Check if destination already exists
        if new_src.exists() {
            let _ = tx.send(CopyMessage::Error(format!(
                "Destination already exists: {}",
                new_src.display()
            )));
            return;
        }

        let result = if src.is_file() {
            // Single file copy
            if let Some(parent) = new_src.parent() {
                if let Err(e) = fs::create_dir_all(parent) {
                    let _ = tx.send(CopyMessage::Error(format!(
                        "Failed to create parent directory: {}",
                        e
                    )));
                    return;
                }
            }

            let _ = tx.send(CopyMessage::Progress {
                files_processed: 0,
                total_files: 1,
                current_file: src
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string(),
            });

            match fast_copy_file(&src, &new_src) {
                Ok(()) => {
                    let _ = tx.send(CopyMessage::Progress {
                        files_processed: 1,
                        total_files: 1,
                        current_file: src
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string(),
                    });
                    Ok(())
                }
                Err(e) => Err(anyhow::anyhow!(
                    "Failed to copy file '{}' to '{}': {}",
                    src.display(),
                    new_src.display(),
                    e
                )),
            }
        } else if src.is_dir() {
            // Directory copy - parallelized with batched progress updates
            if let Err(e) = fs::create_dir_all(&new_src) {
                let _ = tx.send(CopyMessage::Error(format!(
                    "Failed to create destination directory: {}",
                    e
                )));
                return;
            }

            // Collect all entries once
            let all_entries: Vec<_> = WalkDir::new(&src)
                .into_iter()
                .filter_map(|entry| match entry {
                    Ok(e) => Some(e),
                    Err(err) => {
                        warn!("Skipping entry due to error: {}", err);
                        None
                    }
                })
                .collect();

            // Create all directories first
            for entry in all_entries.iter().filter(|e| e.path().is_dir()) {
                let rel = match entry.path().strip_prefix(&src) {
                    Ok(p) => p,
                    Err(e) => {
                        let _ = tx.send(CopyMessage::Error(format!(
                            "Failed to strip prefix: {}",
                            e
                        )));
                        return;
                    }
                };
                if let Err(e) = fs::create_dir_all(new_src.join(rel)) {
                    let _ = tx.send(CopyMessage::Error(format!(
                        "Failed to create directory '{}': {}",
                        new_src.join(rel).display(),
                        e
                    )));
                    return;
                }
            }

            // Prepare items to copy: regular files and symlinks (we previously skipped symlinks, causing empty dirs)
            let files: Vec<_> = all_entries
                .iter()
                .filter(|e| {
                    let ft = e.file_type();
                    ft.is_file() || ft.is_symlink()
                })
                .map(|e| e.path().to_path_buf())
                .collect();

            let total_files = files.len();
            let processed = AtomicUsize::new(0);
            let progress_tx = tx.clone();
            const UPDATE_INTERVAL: usize = 10; // Update progress every 10 files

            // Limit concurrency: FF_COPY_THREADS overrides default (4). If set to 1, this simulates serial copy.
            let threads = std::env::var("FF_COPY_THREADS")
                .ok()
                .and_then(|s| s.parse::<usize>().ok())
                .filter(|&n| n >= 1 && n <= 64)
                .unwrap_or(4);
            let pool = rayon::ThreadPoolBuilder::new().num_threads(threads).build().unwrap();
            let par_result: Result<(), anyhow::Error> = pool.install(|| {
                files.par_iter().try_for_each(|entry_path| -> anyhow::Result<()> {
                    let rel = entry_path
                        .strip_prefix(&src)
                        .map_err(|e| anyhow::anyhow!("Failed to strip prefix: {}", e))?;
                    let dst_path = new_src.join(rel);

                    if let Some(parent) = dst_path.parent() {
                        fs::create_dir_all(parent)
                            .map_err(|e| anyhow::anyhow!("Failed to create parent directory: {}", e))?;
                    }

                    // Preserve symlinks instead of skipping them
                    let ft = std::fs::symlink_metadata(entry_path)
                        .map_err(|e| anyhow::anyhow!("Failed to lstat '{}': {}", entry_path.display(), e))?
                        .file_type();

                    if ft.is_symlink() {
                        let target = std::fs::read_link(entry_path)
                            .map_err(|e| anyhow::anyhow!("Failed to readlink '{}': {}", entry_path.display(), e))?;
                        #[cfg(unix)]
                        {
                            use std::os::unix::fs::symlink;
                            symlink(&target, &dst_path).map_err(|e| {
                                anyhow::anyhow!(
                                    "Failed to create symlink '{}' -> '{}' : {}",
                                    dst_path.display(),
                                    target.display(),
                                    e
                                )
                            })?;
                        }
                        #[cfg(not(unix))]
                        {
                            // On non-unix, best-effort: fall back to copying the target contents
                            if target.is_file() {
                                fast_copy_file(&target, &dst_path).map_err(|e| {
                                    anyhow::anyhow!(
                                        "Failed to copy symlink target '{}' -> '{}': {}",
                                        target.display(),
                                        dst_path.display(),
                                        e
                                    )
                                })?;
                            }
                        }
                    } else if ft.is_file() {
                        fast_copy_file(entry_path, &dst_path).map_err(|e| {
                            anyhow::anyhow!(
                                "Failed to copy '{}' -> '{}': {}",
                                entry_path.display(),
                                dst_path.display(),
                                e
                            )
                        })?;
                    } else {
                        // Skip special files (sockets, devices, etc.)
                        warn!("Skipping unsupported file type: {}", entry_path.display());
                    }

                    let count = processed.fetch_add(1, Ordering::Relaxed) + 1;
                    if count % UPDATE_INTERVAL == 0 || count == total_files {
                        let _ = progress_tx.send(CopyMessage::Progress {
                            files_processed: count,
                            total_files,
                            current_file: entry_path
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .to_string(),
                        });
                    }

                    Ok(())
                })
            });

            par_result
        } else {
            Err(anyhow::anyhow!(
                "Source is neither a file nor a directory: {}",
                src.display()
            ))
        };

        // Send completion message
        match result {
            Ok(_) => {
                let _ = tx.send(CopyMessage::Completed {
                    success: true,
                    message: format!("Successfully copied to {}", new_src.display()),
                });
            }
            Err(e) => {
                let _ = tx.send(CopyMessage::Error(e.to_string()));
            }
        }
    });

    rx
}

fn generate_sort_by_string(sort_type: &SortType) -> String {
    let str_sort_type = match sort_type {
        SortType::ASC => "ASC",
        SortType::DESC => "DESC",
    };
    let join_str = format!("Sort By: '{}'", str_sort_type);
    join_str
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse command line arguments using clap
    let cli_args = CliArgs::parse_args();

    // Log the effective CLI values
    debug!("CLI Arguments: Start path: {:?}, Theme: {:?}, Editor: {:?}, Path: {:?}, Reset Config: {}, Rebuild Cache: {}", 
        cli_args.start, cli_args.theme, cli_args.editor, cli_args.path, cli_args.reset_config, cli_args.rebuild_cache);

    let ps = SyntaxSet::load_defaults_newlines();
    let ts = ThemeSet::load_defaults();

    init();

    // Check for --reset-config flag (from CLI args)
    if cli_args.reset_config {
        println!("Resetting configuration to defaults...");
        if let Err(e) = crate::config::reset_configuration() {
            eprintln!("Failed to reset configuration: {}", e);
            return Err(e.into());
        }
        println!("Configuration reset successfully!");
        return Ok(());
    }

    // Load configuration using new TOML system
    let settings = match crate::config::Settings::load() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Configuration error: {}", e);
            return Err(e.into());
        }
    };

    // Compute effective configuration using precedence rules
    let effective_config = match compute_effective_config(&cli_args, &settings) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("Failed to compute effective configuration: {}", e);
            return Err(e.into());
        }
    };

    debug!(
        "Using effective configuration: start_path={}, theme={:?}, editor={:?}",
        effective_config.start_path.display(),
        effective_config.theme,
        effective_config.editor
    );

    let mut sort_type = SortType::ASC;
    let mut file_reader_content = FileContent::new(ps, ts);
    let mut image_generator = ImageGenerator::new();
    // Setup terminal

    let file_strings = get_file_path_data(
        effective_config.start_path.to_string_lossy().to_string(),
        false,
        SortBy::Default,
        &sort_type,
    )?;
    let mut app = App::new(file_strings.clone());

    // Initialize config and theme after app creation
    if let Err(e) = app.initialize_config_and_theme() {
        eprintln!("Configuration error: {}", e);
        return Err(e.into());
    }

    // Start file system watching for the initial directory
    if let Err(e) = app.start_watching_directory(&settings.start_path) {
        debug!(
            "Failed to start watching directory '{}': {}",
            settings.start_path, e
        );
    } else {
        debug!("Started watching directory: {}", settings.start_path);
    }

    // Set editor from effective configuration (CLI args have precedence)
    if let Some(editor) = effective_config.editor {
        let ide_selection = match editor {
            crate::cli::Editor::Nvim => app::IDE::NVIM,
            crate::cli::Editor::Vscode => app::IDE::VSCODE,
            crate::cli::Editor::Zed => app::IDE::ZED,
        };
        app.selected_id = Some(ide_selection);
        debug!("Set editor from CLI: {:?}", editor);
    }

    let store = if !cli_args.rebuild_cache && Path::new(&settings.cache_directory).exists() {
        let res = load_directory_from_file(&settings.cache_directory.to_owned()).unwrap();
        println!("Loading directory cache from file");
        res
    } else {
        println!("Starting asynchronous directory cache build...");

        // Start async cache building
        let start_time = std::time::Instant::now();
        let rx = build_directory_from_store_async(
            settings.start_path.clone(),
            settings.ignore_directories.clone(),
        );

        // Set up the app to display loading progress and record start time
        app.start_cache_loading(rx, start_time);

        // Return empty store for now - it will be populated when cache building completes
        crate::directory_store::DirectoryStore {
            directories: Vec::new(),
        }
    };

    let widgets_ui = Ui::new(app.files.clone());
    let mut status_bar = StatusBar::new();

    debug!("{:?}", widgets_ui.files_list);

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    terminal.clear()?;
    // Initial selected state
    let mut state = ListState::default();
    state.select(Some(0)); // Select the first item by default
    debug!("{:?}", state);
    let mut read_only_state = ListState::default();
    read_only_state.select(Some(0));

    // Copy progress receiver (for async copy operations)
    let mut copy_receiver: Option<mpsc::Receiver<CopyMessage>> = None;

    // Main loop
    loop {
        // Check if we need to restore a preserved selection
        if let Some(preserved_index) = app.preserved_selection_index.take() {
            // Ensure the index is valid for the current file list
            let valid_index = if app.files.is_empty() {
                0
            } else {
                preserved_index.min(app.files.len() - 1)
            };
            state.select(Some(valid_index));
            app.curr_index = Some(valid_index);
        }

        // Update status bar with current app state
        status_bar.update(&app);

        // Extract image info before drawing if needed
        let has_image = image_generator.has_image;

        // Draw UI
        terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(1)
                .constraints(
                    [
                        Constraint::Length(2),
                        Constraint::Length(3),
                        Constraint::Min(1),
                        Constraint::Length(3),
                    ]
                    .as_ref(),
                )
                .split(f.size());

            let (msg, style) = match app.input_mode {
                InputMode::Normal => {
                    let search_indicator = if app.global_search_mode {
                        " [Global Search]".bold()
                    } else if !app.input.is_empty() {
                        " [Local Search]".bold() 
                    } else {
                        " find (i)".bold()
                    };
                    (
                        vec![
                            "Exit (q)".bold(),
                            search_indicator,
                            app.input.clone().bold(),
                            " Enter to select file (enter)".bold(),
                        ],
                        OneDarkTheme::normal(),
                    )
                },
                InputMode::Editing => {
                    let mode_text = if app.global_search_mode {
                        "Global Search Mode (Esc to exit)".bold()
                    } else {
                        "Local Search Mode (Esc to exit)".bold()
                    };
                    (vec![mode_text], OneDarkTheme::search_active())
                },
                InputMode::WatchDelete => (vec!["Delete Mode".bold()], OneDarkTheme::error()),
                InputMode::WatchCreate => (vec!["Create Mode".bold()], OneDarkTheme::success()),
                InputMode::WatchRename => (vec!["Rename Mode".bold()], OneDarkTheme::warning()),
                InputMode::WatchSort => (vec!["Sort Mode".bold()], OneDarkTheme::info()),
                InputMode::CacheLoading => {
                    let (directories_processed, current_directory) = app.get_cache_loading_info().unwrap_or((0, String::new()));
                    (
                        vec![
                            "⚡ Building directory cache...".bold(),
                            format!(" {} directories processed", directories_processed).bold(),
                            if !current_directory.is_empty() {
                                format!(" Processing: {}", current_directory).bold()
                            } else {
                                " Starting cache build...".bold()
                            },
                        ],
                        OneDarkTheme::loading(),
                    )
                },
                _ => (vec!["Default".bold()], OneDarkTheme::normal()),
            };

            let inner_layout = Layout::default()
                .direction(Direction::Horizontal)
                .constraints(vec![
                    Constraint::Percentage(50),
                    Constraint::Percentage(50),
                ])
                .split(chunks[2]);

            // Input field with OneDark theming
            let search_title = if app.global_search_mode {
                "🔍 Global Search"
            } else if app.input_mode == InputMode::Editing {
                "🔍 Local Search"
            } else {
                "🔍 Search"
            };
            let input_block = Paragraph::new(app.input.clone()).block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(search_title)
                        .style(match app.input_mode {
                            InputMode::Normal => OneDarkTheme::inactive_border(),
                            InputMode::Editing => {
                                if app.global_search_mode {
                                    OneDarkTheme::global_search()
                                } else {
                                    OneDarkTheme::local_search()
                                }
                            },
                            _ => OneDarkTheme::disabled(),
                        }),
                )
                .style(match app.input_mode {
                    InputMode::Editing => OneDarkTheme::normal(),
                    InputMode::Normal => OneDarkTheme::normal(),
                    _ => OneDarkTheme::disabled(),
                });

            let mut list_title = String::new();
            if app.loading {
                let title_with_loader = format!("Copying Files...");
                list_title.push_str(&title_with_loader);
            } else {
                list_title.push_str(&"List");
            }
                        // Render the enhanced status bar
            status_bar.render(f, chunks[3], &app);
            let text = Text::from(Line::from(msg)).patch_style(style);
            let help_message = Paragraph::new(text);

            let input_area = chunks[1];
            match app.input_mode {
                InputMode::Normal => {}
                InputMode::WatchDelete => {}
                InputMode::WatchCreate => {}
                InputMode::WatchRename => {}
                InputMode::WatchSort => {}
                InputMode::Editing => f.set_cursor(
                    input_area.x + app.character_index as u16 + 1,
                    input_area.y + 1,
                ),
                _ => {}
            }

            f.render_widget(help_message, chunks[0]);
            f.render_widget(input_block, chunks[1]);

            // Handle cache loading screen or normal list rendering
            match app.input_mode {
                InputMode::CacheLoading => {
                    // Render cache loading screen
                    let (directories_processed, current_directory) = app.get_cache_loading_info().unwrap_or((0, String::new()));
                    // Create a simple loading screen with progress info and spinner
                    let spinner_chars = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
                    let spinner_index = (directories_processed / 10) % spinner_chars.len(); // Update every 10 directories
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
                            current_directory.clone()
                        };
                        format!(
                            "{} Building directory cache...\n\nDirectories processed: {}\nCurrent: {}\n\nPlease wait while the cache is being built.",
                            spinner, directories_processed, display_dir
                        )
                    };
                    let loading_block = Paragraph::new(loading_text)
                        .block(
                            Block::default()
                                .borders(Borders::ALL)
                                .title("⚡ Directory Cache Loading")
                                .style(OneDarkTheme::loading())
                        )
                        .style(OneDarkTheme::normal())
                        .alignment(ratatui::layout::Alignment::Center);
                    // Render loading screen across both panels
                    f.render_widget(loading_block, chunks[2]);
                }
                _ => {
                    // Normal file list rendering
                    widgets_ui.clone().render_list_preview(f, &chunks, &mut state, &app);
                }
            }

            let t = file_reader_content.file_type.clone();
            match t {
                FileType::SourceCode | FileType::Markdown | FileType::TextFile |
                FileType::ConfigFile | FileType::JSON => {
                    image_generator.clear();
                    if let Some(highlighted_content) = file_reader_content.hightlighted_content.as_ref() {
                        let file_preview_text = highlighted_content.clone()
                            .block(Block::default().borders(Borders::ALL).title("File Preview"))
                            .style(Style::default());
                        f.render_widget(file_preview_text, inner_layout[1]);
                    } else {
                        // Fallback for when highlighted content is not available
                        let preview_text = Paragraph::new(app.preview_file_content.clone())
                            .block(Block::default().borders(Borders::ALL).title("File Preview"))
                            .style(Style::default());
                        f.render_widget(preview_text, inner_layout[1]);
                    }
                }
                FileType::Image => {
                    if has_image {
                        let image_info = Paragraph::new(image_generator.image_info.clone())
                            .block(Block::default().borders(Borders::ALL).title("Image Preview"))
                            .style(Style::default().fg(Color::Green));
                        f.render_widget(image_info, inner_layout[1]);
                    } else {
                        let image_info = Paragraph::new(if image_generator.image_info.is_empty() {
                            "Unable to load image preview\n\nPossible reasons:\n• Unsupported image format\n• Corrupted image file\n• Insufficient permissions\n\nSupported formats: PNG, JPEG, GIF, BMP".to_string()
                        } else {
                            image_generator.image_info.clone()
                        })
                            .block(Block::default().borders(Borders::ALL).title("Image Preview - Error"))
                            .style(Style::default().fg(Color::Yellow));
                        f.render_widget(image_info, inner_layout[1]);
                    }
                }
                FileType::ZIP => {
                    let zip_list_content = List::new(file_reader_content.curr_zip_content.clone()).block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title("ZIP Archive Contents")
                            .style(match app.input_mode {
                                InputMode::Normal => Style::default().fg(Color::Green),
                                InputMode::Editing => Style::default().fg(Color::Gray),
                                _ => Style::default().fg(Color::Gray),
                            })
                    )
                    .style(Style::default().fg(Color::DarkGray));
                    f.render_widget(zip_list_content, inner_layout[1]);
                }
                FileType::Archive => {
                    let archive_info = Paragraph::new(app.preview_file_content.clone())
                        .block(Block::default().borders(Borders::ALL).title("Archive Info"))
                        .style(Style::default());
                    f.render_widget(archive_info, inner_layout[1]);
                }
                FileType::CSV => {
                    let csv_list_content = List::new(file_reader_content.curr_csv_content.clone()).block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title("CSV Data Preview")
                            .style(match app.input_mode {
                                InputMode::Normal => Style::default().fg(Color::Green),
                                InputMode::Editing => Style::default().fg(Color::Gray),
                                _ => Style::default().fg(Color::Gray),
                            })
                    )
                    .style(Style::default().fg(Color::DarkGray));
                    f.render_widget(csv_list_content, inner_layout[1]);
                }
                FileType::Binary => {
                    let binary_info = Paragraph::new(app.preview_file_content.clone())
                        .block(Block::default().borders(Borders::ALL).title("Binary File"))
                        .style(Style::default().fg(Color::Yellow));
                    f.render_widget(binary_info, inner_layout[1]);
                }
                _ => {
                    image_generator.clear();
                    widgets_ui.clone().render_preview_window(f, &chunks, &mut state, &app);
                }
            }

            if app.render_popup {
                let block = Block::bordered()
                    .title("⚠️  Confirm to delete y/n")
                    .style(OneDarkTheme::error());
                let area = draw_popup(f.size(), 40, 7);
                let popup_chuncks = Layout::default()
                    .direction(Direction::Horizontal)
                    .margin(1)
                    .constraints([Constraint::Percentage(100)])
                    .split(area);
                f.render_widget(Clear, area);
                f.render_widget(block, popup_chuncks[0]);
            }

            let area = draw_popup(f.size(), 40, 7);
            let popup_chuncks = Layout::default()
                .direction(Direction::Horizontal)
                .margin(1)
                .constraints([Constraint::Percentage(100)])
                .split(area);

            let sort_option_area = draw_popup(f.size(), 90, 20);
            let sort_options_chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(1)
                .constraints([Constraint::Percentage(100)])
                .split(sort_option_area);

            let keybinding_area = draw_popup(f.size(), 80, 20);
            let keybinding_chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(1)
                .constraints([Constraint::Percentage(100)])
                .split(keybinding_area);

            match app.input_mode {
                InputMode::WatchCreate => {

                    let create_input_block = Paragraph::new(app.create_edit_file_name.clone())
                        .block(Block::default().borders(Borders::ALL).title(
                            match app.is_create_edit_error {
                                false => "✨ Create File/Dir".to_string(),
                                true => format!("❌ {}", app.error_message),
                            },
                        ))
                        .style(match app.is_create_edit_error {
                            true => OneDarkTheme::error(),
                            false => OneDarkTheme::success(),
                        });

                    f.render_widget(Clear, popup_chuncks[0]);
                    f.render_widget(create_input_block, popup_chuncks[0]);
                }
                InputMode::WatchRename => {
                    let create_input_block = Paragraph::new(app.create_edit_file_name.clone())
                        .block(Block::default().borders(Borders::ALL).title("✏️  Enter file/dir name"))
                        .style(OneDarkTheme::warning());

                    f.render_widget(create_input_block, popup_chuncks[0]);
                }
                InputMode::WatchSort => {
                    let lines = vec![
                        Line::from("Press (a) to sort ASC or (d) to sort DESC, (q) to exit"),
                        Line::from("Name: (n)"),
                        Line::from("Date Created: (t)"),
                        Line::from("Size: (s)"),
                    ];

                    let sort_by_text = generate_sort_by_string(&sort_type);
                    let list_items = Text::from(lines);
                    let p = Paragraph::new(list_items)
                        .block(Block::default().borders(Borders::ALL).title(format!("🔄 {}", sort_by_text)))
                        .style(OneDarkTheme::info());
                    f.render_widget(Clear, sort_options_chunks[0]);
                    f.render_widget(p, sort_options_chunks[0]);

                }
                InputMode::WatchKeyBinding => {
                    let lines = vec![
                        Line::from("<Enter>: Open directory with selected IDE. copy path if not IDE option provided."),
                        Line::from("<s>: Sort"),
                        Line::from("<a>: Create new"),
                        Line::from("<d>: Delete"),
                        Line::from("<i>: Search mode"),
                        Line::from("<c>: Copy dir/file"),
                        Line::from("<.>: Show hidden files"),
                        Line::from(""),
                        Line::from("-- Search Features --"),
                        Line::from("Type in search mode to use fuzzy search with scoring and ranking"),
                        Line::from("Start search with space or / to search across entire directory tree"),
                        Line::from("Results sorted by relevance with highlighting of matched text"),
                        Line::from("Search history is available using up/down arrow keys"),
                    ];

                    let list_items = Text::from(lines);
                    let paragraph = Paragraph::new(list_items)
                        .block(Block::default().borders(Borders::ALL).title("⌨️  Keybindings"))
                        .style(OneDarkTheme::info());
                    f.render_widget(Clear, keybinding_chunks[0]);
                    f.render_widget(paragraph, keybinding_chunks[0]);
                }
                _ => {}
            }
            // Render error notification overlay if there's an active error
            if status_bar.has_error() {
                status_bar.render_error_notification(f, f.size());
            }
        })?;

        // Handle copy progress messages
        if let Some(ref rx) = copy_receiver {
            // Drain all pending messages so we only render the most recent state.
            // This avoids a huge backlog of progress updates that would otherwise
            // make the UI appear to take minutes for large trees.
            let mut last_msg: Option<CopyMessage> = None;
            loop {
                match rx.try_recv() {
                    Ok(msg) => last_msg = Some(msg),
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        // Copy thread finished, reset state
                        app.copy_in_progress = false;
                        app.copy_progress_message.clear();
                        app.copy_files_processed = 0;
                        app.copy_total_files = 0;
                        copy_receiver = None;
                        last_msg = None;
                        break;
                    }
                }
            }

            if let Some(msg) = last_msg {
                match msg {
                    CopyMessage::Progress {
                        files_processed,
                        total_files,
                        current_file,
                    } => {
                        app.copy_files_processed = files_processed;
                        app.copy_total_files = total_files;
                        app.copy_progress_message = format!("Copying: {}", current_file);
                    }
                    CopyMessage::Completed { success, message } => {
                        if success {
                            debug!("Copy completed: {}", message);

                            // Refresh file lists to show the copied item in the CURRENT directory
                            let refresh_dir: String = if !app.files.is_empty() {
                                // Derive current directory from the first listed item
                                get_curr_path(app.files[0].clone())
                            } else {
                                // Fallback to configured start path
                                settings.start_path.to_owned()
                            };

                            if let Ok(file_path_list) = get_file_path_data(
                                refresh_dir,
                                app.show_hidden_files,
                                SortBy::Default,
                                &sort_type,
                            ) {
                                app.files = file_path_list.clone();
                                app.read_only_files = file_path_list;
                                app.update_file_references();
                            }
                        } else {
                            warn!("Copy failed: {}", message);
                        }

                        // Reset copy state
                        app.copy_in_progress = false;
                        app.copy_progress_message.clear();
                        app.copy_files_processed = 0;
                        app.copy_total_files = 0;
                        copy_receiver = None;
                    }
                    CopyMessage::Error(error_msg) => {
                        warn!("Copy operation failed: {}", error_msg);

                        // Reset copy state
                        app.copy_in_progress = false;
                        app.copy_progress_message = format!("Copy failed: {}", error_msg);
                        app.copy_files_processed = 0;
                        app.copy_total_files = 0;
                        copy_receiver = None;
                    }
                }
            }
        }

        // Process file system events
        let fs_events = app.process_file_system_events();
        if !fs_events.is_empty() {
            // File system changes detected, refresh the CURRENT directory (not the start path)
            let refresh_dir: String = if !app.files.is_empty() {
                get_curr_path(app.files[0].clone())
            } else {
                settings.start_path.to_owned()
            };

            if let Ok(file_path_list) = get_file_path_data(
                refresh_dir,
                app.show_hidden_files,
                SortBy::Default,
                &sort_type,
            ) {
                app.files = file_path_list.clone();
                app.read_only_files = file_path_list;
                app.update_file_references();

                // Log the file system events for debugging
                for event_msg in fs_events {
                    debug!("File system event: {}", event_msg);
                }
            }
        }

        // Process cache loading progress
        if app.input_mode == InputMode::CacheLoading {
            if app.process_cache_loading_progress() {
                // Cache loading is complete, finish setup
                debug!("Cache loading completed, finalizing...");

                // Use the completed cache if available, otherwise fallback to current directory
                let completed_directories = if let Some(ref cache_store) = app.completed_cache_store
                {
                    debug!(
                        "Using completed cache with {} directories",
                        cache_store.directories.len()
                    );

                    // Save the cache to file for future use
                    match crate::directory_store::save_directory_to_file(
                        cache_store,
                        &settings.cache_directory,
                    ) {
                        Ok(()) => {
                            debug!(
                                "Successfully saved cache to file: {}",
                                settings.cache_directory
                            );
                            if let Some(ms) = app.cache_build_elapsed_ms {
                                println!("Directory cache built and saved in {} ms", ms);
                            }
                        }
                        Err(e) => {
                            debug!("Failed to save cache to file: {}", e);
                        }
                    }

                    // Return the cache directories for the UI
                    cache_store.directories.clone()
                } else {
                    debug!("No completed cache available, falling back to current directory");
                    // Fallback to current directory files
                    get_file_path_data(
                        settings.start_path.clone(),
                        false,
                        SortBy::Default,
                        &sort_type,
                    )
                    .unwrap_or_else(|_| Vec::new())
                };

                // Update app with the directories and switch to normal mode
                app.finish_cache_loading(completed_directories);

                debug!("Cache loading finished, app is now in normal mode");
            }
        }

        // Handle input with timeout for dynamic progress updates
        let timeout = std::time::Duration::from_millis(100); // 100ms timeout
        if let Ok(available) = event::poll(timeout) {
            if available {
                if let Event::Key(key) = event::read()? {
                    match app.input_mode {
                        InputMode::Normal => match key.code {
                            KeyCode::Char('i') => {
                                app.input_mode = InputMode::Editing;
                                file_reader_content.file_type = FileType::NotAvailable;
                                image_generator.clear();
                            }
                            KeyCode::Char('q') => {
                                break;
                            }
                            KeyCode::Down | KeyCode::Char('j') => {
                                // Determine the list length based on current mode
                                // Use search results if we have any (either global or local search)
                                let list_len = if !app.search_results.is_empty() {
                                    app.search_results.len()
                                } else {
                                    app.files.len()
                                };

                                if list_len > 0 {
                                    let i = match state.selected() {
                                        Some(i) => {
                                            if i >= list_len - 1 {
                                                0
                                            } else {
                                                i + 1
                                            }
                                        }
                                        None => 0,
                                    };
                                    state.select(Some(i));
                                    app.curr_index = Some(i);

                                    // Get the selected path based on current mode
                                    // Use search results if we have any (either global or local search)
                                    let selected_cur_path = if !app.search_results.is_empty() {
                                        &app.search_results[i].file_path
                                    } else {
                                        &app.files[i]
                                    };

                                    debug!("check here: {:?}", selected_cur_path);
                                    let get_metadata =
                                        get_metadata_info(selected_cur_path.to_owned());
                                    let generated_metadata_str =
                                        generate_metadata_str_info(get_metadata);

                                    app.curr_stats = generated_metadata_str.clone();
                                    file_reader_content.curr_selected_path =
                                        selected_cur_path.clone();

                                    if !is_file(selected_cur_path.to_string()) {
                                        if let Some(file_names) =
                                            get_content_from_path(selected_cur_path.to_string())
                                        {
                                            image_generator.clear();
                                            file_reader_content.file_type = FileType::NotAvailable;
                                            app.preview_files = file_names;
                                        }
                                    } else {
                                        let file_extension = file_reader_content
                                            .get_file_extension(selected_cur_path.clone());

                                        match file_extension {
                                            FileType::SourceCode
                                            | FileType::Markdown
                                            | FileType::TextFile
                                            | FileType::ConfigFile
                                            | FileType::JSON => {
                                                image_generator.clear();
                                                file_reader_content.file_type =
                                                    file_extension.clone();
                                                let file_content = file_reader_content
                                                    .read_file_content(
                                                        selected_cur_path.to_string(),
                                                    );

                                                let curr_file_type = file_reader_content
                                                    .get_file_extension_type(
                                                        selected_cur_path.clone(),
                                                    );

                                                let highlighted_content = file_reader_content
                                                    .get_highlighted_content(
                                                        file_content,
                                                        curr_file_type,
                                                    );

                                                // only update if there are no errors
                                                if !file_reader_content.is_error {
                                                    app.preview_file_content = highlighted_content;
                                                }
                                            }
                                            FileType::Image => {
                                                image_generator.clear();
                                                file_reader_content.curr_asset_path =
                                                    selected_cur_path.to_string();

                                                image_generator.load_img(selected_cur_path.clone());
                                                file_reader_content.file_type = FileType::Image;
                                                // Clear any previous text content
                                                app.preview_file_content.clear();
                                                file_reader_content.hightlighted_content = None;
                                            }
                                            FileType::ZIP => {
                                                image_generator.clear();
                                                file_reader_content
                                                    .read_zip_content(selected_cur_path.clone());
                                                file_reader_content.file_type = FileType::ZIP;
                                            }
                                            FileType::Archive => {
                                                image_generator.clear();
                                                file_reader_content.file_type = FileType::Archive;
                                                // For now, show archive info as text
                                                let archive_info =
                                                    format!("Archive file: {}", selected_cur_path);
                                                app.preview_file_content = archive_info;
                                            }
                                            FileType::CSV => {
                                                image_generator.clear();
                                                file_reader_content.read_csv_content();
                                                file_reader_content.file_type = FileType::CSV;
                                            }
                                            FileType::Binary => {
                                                image_generator.clear();
                                                file_reader_content.file_type = FileType::Binary;
                                                let binary_info = format!(
                                                    "Binary file: {} (use external viewer)",
                                                    selected_cur_path
                                                );
                                                app.preview_file_content = binary_info;
                                            }
                                            _ => {
                                                image_generator.clear();
                                                file_reader_content.file_type =
                                                    FileType::NotAvailable;
                                            }
                                        }

                                        app.preview_files = Vec::new();
                                    }
                                }
                            }
                            KeyCode::Up | KeyCode::Char('k') => {
                                // Determine the list length based on current mode
                                // Use search results if we have any (either global or local search)
                                let list_len = if !app.search_results.is_empty() {
                                    app.search_results.len()
                                } else {
                                    app.files.len()
                                };

                                if list_len > 0 {
                                    let i = match state.selected() {
                                        Some(i) => {
                                            if i == 0 {
                                                list_len - 1
                                            } else {
                                                i - 1
                                            }
                                        }
                                        None => 0,
                                    };
                                    state.select(Some(i));
                                    app.curr_index = Some(i);

                                    // Get the selected path based on current mode
                                    // Use search results if we have any (either global or local search)
                                    let selected_cur_path = if !app.search_results.is_empty() {
                                        &app.search_results[i].file_path
                                    } else {
                                        &app.files[i]
                                    };

                                    let get_metadata =
                                        get_metadata_info(selected_cur_path.to_owned());
                                    let generated_metadata_str =
                                        generate_metadata_str_info(get_metadata);
                                    app.curr_stats = generated_metadata_str.clone();

                                    // INFO: update preview list

                                    file_reader_content.curr_selected_path =
                                        selected_cur_path.clone();
                                    if !is_file(selected_cur_path.clone()) {
                                        if let Some(file_names) =
                                            get_content_from_path(selected_cur_path.to_string())
                                        {
                                            image_generator.clear();
                                            file_reader_content.file_type = FileType::NotAvailable;
                                            app.preview_files = file_names;
                                        }
                                    } else {
                                        let file_extension = file_reader_content
                                            .get_file_extension(selected_cur_path.clone());

                                        match file_extension {
                                            FileType::SourceCode
                                            | FileType::Markdown
                                            | FileType::TextFile
                                            | FileType::ConfigFile
                                            | FileType::JSON => {
                                                image_generator.clear();
                                                file_reader_content.file_type =
                                                    file_extension.clone();
                                                let file_content = file_reader_content
                                                    .read_file_content(
                                                        selected_cur_path.to_string(),
                                                    );
                                                let curr_file_type = file_reader_content
                                                    .get_file_extension_type(
                                                        selected_cur_path.clone(),
                                                    );
                                                let highlighted_content = file_reader_content
                                                    .get_highlighted_content(
                                                        file_content,
                                                        curr_file_type,
                                                    );
                                                // only update if there are no errors
                                                if !file_reader_content.is_error {
                                                    app.preview_file_content = highlighted_content;
                                                }
                                            }
                                            FileType::Image => {
                                                image_generator.clear();
                                                file_reader_content.curr_asset_path =
                                                    selected_cur_path.to_string();

                                                image_generator.load_img(selected_cur_path.clone());
                                                file_reader_content.file_type = FileType::Image;
                                                // Clear any previous text content
                                                app.preview_file_content.clear();
                                                file_reader_content.hightlighted_content = None;
                                            }
                                            FileType::ZIP => {
                                                image_generator.clear();
                                                file_reader_content
                                                    .read_zip_content(selected_cur_path.clone());
                                                file_reader_content.file_type = FileType::ZIP;
                                            }
                                            FileType::Archive => {
                                                image_generator.clear();
                                                file_reader_content.file_type = FileType::Archive;
                                                let archive_info =
                                                    format!("Archive file: {}", selected_cur_path);
                                                app.preview_file_content = archive_info;
                                            }
                                            FileType::CSV => {
                                                image_generator.clear();
                                                file_reader_content.read_csv_content();
                                                file_reader_content.file_type = FileType::CSV;
                                            }
                                            FileType::Binary => {
                                                image_generator.clear();
                                                file_reader_content.file_type = FileType::Binary;
                                                let binary_info = format!(
                                                    "Binary file: {} (use external viewer)",
                                                    selected_cur_path
                                                );
                                                app.preview_file_content = binary_info;
                                            }
                                            _ => {
                                                image_generator.clear();
                                                file_reader_content.file_type =
                                                    FileType::NotAvailable;
                                            }
                                        }
                                        app.preview_files = Vec::new()
                                    }
                                }
                            }
                            KeyCode::Char('h') => {
                                if app.files.len() > 0 {
                                    let selected = &app.files[state.selected().unwrap()];
                                    let mut split_path = selected.split("/").collect::<Vec<&str>>();

                                    let sort_type_copy = sort_type.clone();
                                    // TODO: refactor this to be more idiomatic
                                    if split_path.len() > 4 {
                                        split_path.pop();
                                        split_path.pop();

                                        let new_path = split_path.join("/");
                                        let files_strings = get_inner_files_info(
                                            new_path.clone(),
                                            app.show_hidden_files,
                                            SortBy::Default,
                                            &sort_type_copy,
                                        )
                                        .unwrap();

                                        if let Some(f_s) = files_strings {
                                            app.read_only_files = f_s.clone();
                                            app.files = f_s;
                                            app.update_file_references();
                                            state.select(Some(0));

                                            // Start watching the new directory
                                            if let Err(e) = app.start_watching_directory(&new_path)
                                            {
                                                debug!(
                                                    "Failed to start watching directory '{}': {}",
                                                    new_path, e
                                                );
                                            } else {
                                                debug!("Started watching directory: {}", new_path);
                                            }
                                        }
                                    }
                                } else {
                                    let copy = sort_type.clone();
                                    let files_strings = get_inner_files_info(
                                        app.prev_dir.clone(),
                                        app.show_hidden_files,
                                        SortBy::Default,
                                        &copy,
                                    )
                                    .unwrap();

                                    if let Some(f_s) = files_strings {
                                        app.read_only_files = f_s.clone();
                                        app.files = f_s;
                                        app.update_file_references();
                                        state.select(Some(0));
                                    }
                                }
                            }
                            KeyCode::Char('l') => {
                                let selected_index = state.selected();

                                // Get the selected path based on current mode
                                // Use search results if we have any (either global or local search)
                                let selected_path = if !app.search_results.is_empty() {
                                    // Search mode (global or local) - get path from search results
                                    if let Some(selected_indx) = selected_index {
                                        if selected_indx < app.search_results.len() {
                                            Some(
                                                app.search_results[selected_indx].file_path.clone(),
                                            )
                                        } else {
                                            None
                                        }
                                    } else {
                                        None
                                    }
                                } else {
                                    // Normal mode - get path from files list
                                    if app.files.len() > 0 {
                                        if let Some(selected_indx) = selected_index {
                                            if selected_indx < app.files.len() {
                                                Some(app.files[selected_indx].clone())
                                            } else {
                                                None
                                            }
                                        } else {
                                            None
                                        }
                                    } else {
                                        None
                                    }
                                };

                                if let Some(selected) = selected_path {
                                    app.prev_dir = get_curr_path(selected.to_string());
                                    if !is_file(selected.to_string()) {
                                        match get_inner_files_info(
                                            selected.to_string(),
                                            app.show_hidden_files,
                                            SortBy::Default,
                                            &sort_type,
                                        ) {
                                            Ok(files_strings) => {
                                                if let Some(files_strs) = files_strings {
                                                    // Exit search mode when navigating into a directory
                                                    app.global_search_mode = false;
                                                    app.search_results.clear();
                                                    app.input.clear();
                                                    app.character_index = 0;
                                                    app.input_mode = InputMode::Normal;

                                                    app.read_only_files = files_strs.clone();
                                                    app.files = files_strs;
                                                    app.update_file_references();
                                                    state.select(Some(0));

                                                    // Start watching the new directory
                                                    if let Err(e) =
                                                        app.start_watching_directory(&selected)
                                                    {
                                                        debug!("Failed to start watching directory '{}': {}", selected, e);
                                                    } else {
                                                        debug!(
                                                            "Started watching directory: {}",
                                                            selected
                                                        );
                                                    }
                                                }
                                            }
                                            Err(e) => {
                                                println!("Error: {}", e);
                                            }
                                        }
                                    }
                                }
                            }

                            KeyCode::Char('o') => {
                                if let Some(index) = state.selected() {
                                    Command::new("open")
                                        .arg(&app.files[index])
                                        .spawn()
                                        .expect("failed to open file");

                                    break;
                                }
                            }
                            KeyCode::Char('d') => {
                                app.render_popup = true;
                                app.input_mode = InputMode::WatchDelete;
                            }
                            KeyCode::Char('a') => {
                                app.input_mode = InputMode::WatchCreate;
                            }
                            KeyCode::Char('y') => {
                                let curr_file_path = file_reader_content.curr_selected_path.clone();
                                let file_type =
                                    file_reader_content.get_file_extension(curr_file_path.clone());
                                match file_type {
                                    FileType::ZIP => {
                                        let t = file_reader_content.extract_zip_content();
                                        app.input = t;
                                    }
                                    _ => {}
                                }
                            }
                            KeyCode::Char('r') => {
                                let selected_index = state.selected();
                                if let Some(index) = selected_index {
                                    let selected = &app.files[index];
                                    let mut split_path = selected.split("/").collect::<Vec<&str>>();
                                    let placeholder_name = split_path.pop().unwrap();
                                    let new_path = split_path.join("/");
                                    let placeholder_name_copy = placeholder_name;
                                    app.current_path_to_edit = new_path;
                                    app.current_name_to_edit = placeholder_name_copy.to_string();
                                    app.create_edit_file_name = placeholder_name.to_string();
                                    app.char_index = placeholder_name.len();
                                }
                                app.input_mode = InputMode::WatchRename;
                            }
                            KeyCode::Char('.') => {
                                let is_hidden = !app.show_hidden_files;
                                app.show_hidden_files = is_hidden;
                                let selected_index = state.selected();
                                if let Some(indx) = selected_index {
                                    let selected = &app.files[indx];

                                    let mut split_path = selected.split("/").collect::<Vec<&str>>();
                                    split_path.pop();
                                    let new_path = split_path.join("/");
                                    match get_inner_files_info(
                                        new_path,
                                        is_hidden,
                                        SortBy::Default,
                                        &sort_type,
                                    ) {
                                        Ok(files) => {
                                            if let Some(file_strs) = files {
                                                app.read_only_files = file_strs.clone();
                                                app.files = file_strs;
                                                app.update_file_references();
                                            }
                                        }
                                        Err(e) => {
                                            println!("error  {}", e);
                                        }
                                    }
                                }
                            }
                            KeyCode::Char('c') => {
                                // Copy selected file/directory in current location using async operation
                                if let Some(index) = state.selected() {
                                    if index < app.files.len() {
                                        let selected_path = &app.files[index];
                                        let src_path = Path::new(selected_path);

                                        // Get current directory (parent of selected item)
                                        let current_dir = if src_path.is_file() {
                                            src_path.parent().unwrap_or(Path::new("."))
                                        } else {
                                            // For directories, get the parent directory
                                            src_path.parent().unwrap_or(Path::new("."))
                                        };

                                        // Generate copy name
                                        let new_path_with_new_name = generate_copy_file_dir_name(
                                            selected_path.clone(),
                                            current_dir.to_string_lossy().to_string(),
                                        );

                                        let new_src = Path::new(&new_path_with_new_name);

                                        // Start async copy operation
                                        let rx = copy_dir_file_with_progress(src_path, new_src);
                                        copy_receiver = Some(rx);

                                        // Initialize copy progress state
                                        app.copy_in_progress = true;
                                        app.copy_progress_message =
                                            String::from("Starting copy...");
                                        app.copy_files_processed = 0;
                                        app.copy_total_files = 0;
                                    }
                                }
                            }

                            KeyCode::Char('s') => {
                                app.input_mode = InputMode::WatchSort;
                            }

                            KeyCode::Char('?') => {
                                app.input_mode = InputMode::WatchKeyBinding;
                            }

                            KeyCode::Enter => {
                                // Get the selected path based on current mode
                                // Use search results if we have any (either global or local search)
                                let selected_path = if !app.search_results.is_empty() {
                                    // Search mode (global or local) - get path from search results
                                    if let Some(selected_indx) = state.selected() {
                                        if selected_indx < app.search_results.len() {
                                            Some(
                                                app.search_results[selected_indx].file_path.clone(),
                                            )
                                        } else {
                                            None
                                        }
                                    } else {
                                        None
                                    }
                                } else {
                                    // Normal mode - get path from files list
                                    if let Some(selected_indx) = state.selected() {
                                        if selected_indx < app.files.len() {
                                            Some(app.files[selected_indx].clone())
                                        } else {
                                            None
                                        }
                                    } else {
                                        None
                                    }
                                };

                                if let Some(selected) = selected_path {
                                    app.input = selected.clone();

                                    // Check if IDE is configured - if so, open file, otherwise copy to clipboard
                                    if app.get_selected_ide().is_some() {
                                        let _ =
                                            handle_file_selection(&selected, &mut terminal, &app);
                                        break;
                                    } else {
                                        // Copy path to clipboard instead of opening
                                        use copypasta::{ClipboardContext, ClipboardProvider};
                                        if let Ok(mut ctx) = ClipboardContext::new() {
                                            if let Ok(_) = ctx.set_contents(selected.clone()) {
                                                debug!("Copied path to clipboard: {}", selected);
                                                // Optionally show a brief message to user
                                                // For now just break to exit
                                                break;
                                            }
                                        }
                                        // Fallback to normal file selection if clipboard fails
                                        let _ =
                                            handle_file_selection(&selected, &mut terminal, &app);
                                        break;
                                    }
                                }
                            }
                            _ => {}
                        },

                        InputMode::WatchRename if key.kind == KeyEventKind::Press => match key.code
                        {
                            KeyCode::Char(c) => {
                                app.add_char(c);
                            }
                            KeyCode::Backspace => {
                                app.delete_c();
                            }
                            KeyCode::Esc => {
                                app.input_mode = InputMode::Normal;
                                app.reset_create_edit_values();
                            }
                            KeyCode::Enter => {
                                // rename file to new name
                                // validate tha the new name and the previous name are not the same,
                                // if names are equal then exit the current mode
                                if app.create_edit_file_name == app.current_name_to_edit {
                                    app.input_mode = InputMode::Normal;
                                    app.reset_create_edit_values();
                                } else {
                                    // proceed with operation
                                    let new_path = format!(
                                        "{}/{}",
                                        app.current_path_to_edit, app.create_edit_file_name
                                    );
                                    if !check_if_exists(new_path) {
                                        match handle_rename(&app) {
                                            Ok(_) => {
                                                app.reset_create_edit_values();
                                                let file_path_list = get_file_path_data(
                                                    settings.start_path.to_owned(),
                                                    app.show_hidden_files,
                                                    SortBy::Default,
                                                    &sort_type,
                                                )?;
                                                app.files = file_path_list.clone();
                                                app.read_only_files = file_path_list.clone();
                                                app.update_file_references();
                                                app.input_mode = InputMode::Normal;
                                            }
                                            Err(e) => {
                                                app.is_create_edit_error = true;
                                                app.error_message = e.user_message();
                                            }
                                        }
                                    } else {
                                        app.is_create_edit_error = true;
                                        app.error_message = "Already exist".to_string();
                                    }
                                }
                            }
                            _ => {}
                        },
                        InputMode::WatchCreate if key.kind == KeyEventKind::Press => match key.code
                        {
                            KeyCode::Char(c) => {
                                app.add_char(c);
                            }
                            KeyCode::Backspace => {
                                app.delete_c();
                            }
                            KeyCode::Left => {
                                app.move_create_edit_cursor_left();
                            }
                            KeyCode::Right => {
                                app.move_create_edit_cursor_right();
                            }
                            KeyCode::Esc => {
                                app.input_mode = InputMode::Normal;
                                app.reset_create_edit_values();
                                // create methods to reset the create_edit_file_name and state of it
                            }
                            KeyCode::Enter => {
                                // create file/dir
                                if !app.create_edit_file_name.is_empty() {
                                    let selected_index = state.selected();
                                    let selected = &app.files[selected_index.unwrap()];
                                    let mut split_path = selected.split("/").collect::<Vec<&str>>();
                                    split_path.pop();
                                    let new_path = split_path.join("/");
                                    match create_item_based_on_type(
                                        new_path,
                                        app.create_edit_file_name.clone(),
                                    ) {
                                        Ok(_) => {
                                            app.input_mode = InputMode::Normal;

                                            app.reset_create_edit_values();
                                            let file_path_list = get_file_path_data(
                                                settings.start_path.to_owned(),
                                                app.show_hidden_files,
                                                SortBy::Default,
                                                &sort_type,
                                            )?;
                                            app.files = file_path_list.clone();
                                            app.read_only_files = file_path_list.clone();
                                            app.update_file_references();
                                        }
                                        Err(e) => {
                                            app.is_create_edit_error = true;
                                            app.error_message = e.user_message();
                                        }
                                    }
                                }
                            }
                            _ => {}
                        },

                        InputMode::Editing if key.kind == KeyEventKind::Press => match key.code {
                            KeyCode::Enter => app.submit_message(),
                            KeyCode::Char(to_insert) => {
                                app.enter_char(to_insert, store.clone());
                            }
                            KeyCode::Backspace => {
                                app.delete_char(store.clone());
                            }
                            KeyCode::Left => {
                                app.move_cursor_left();
                            }

                            KeyCode::Right => {
                                app.move_cursor_right();
                            }
                            KeyCode::Esc => {
                                app.input_mode = InputMode::Normal;
                            }

                            _ => {}
                        },
                        InputMode::WatchDelete => match key.code {
                            KeyCode::Char('q') => {
                                app.render_popup = false;
                                app.input_mode = InputMode::Normal;
                                break;
                            }
                            KeyCode::Char('n') => {
                                app.render_popup = false;
                                app.input_mode = InputMode::Normal;
                            }

                            KeyCode::Char('y') => {
                                let selected_index = state.selected();

                                if let Some(selected_indx) = selected_index {
                                    let selected = &app.files[selected_indx];

                                    match handle_delete_based_on_type(selected) {
                                        Ok(_) => {
                                            // Preserve selection by finding a nearby item after deletion
                                            let next_selection_index = if selected_indx > 0
                                                && selected_indx >= app.files.len() - 1
                                            {
                                                // If we deleted the last item, select the new last item
                                                Some(app.files.len().saturating_sub(2))
                                            } else if selected_indx < app.files.len() - 1 {
                                                // Select the item that will take the deleted item's position
                                                Some(selected_indx)
                                            } else {
                                                // Fallback to previous item or first item
                                                Some(
                                                    selected_indx
                                                        .saturating_sub(1)
                                                        .min(app.files.len().saturating_sub(2)),
                                                )
                                            };

                                            let file_path_list = get_file_path_data(
                                                settings.start_path.to_owned(),
                                                app.show_hidden_files,
                                                SortBy::Default,
                                                &sort_type,
                                            )?;
                                            app.render_popup = false;
                                            app.files = file_path_list.clone();
                                            app.read_only_files = file_path_list.clone();
                                            app.update_file_references();

                                            // Restore selection to a nearby item
                                            app.preserved_selection_index = next_selection_index;
                                            app.input_mode = InputMode::Normal;
                                        }
                                        Err(e) => {
                                            // Show error in status bar
                                            status_bar.show_error(e.user_message(), None);
                                            warn!("Delete operation failed: {}", e.user_message());
                                            app.render_popup = false;
                                            app.input_mode = InputMode::Normal;
                                        }
                                    }
                                }
                            }
                            _ => {}
                        },
                        InputMode::WatchSort => match key.code {
                            KeyCode::Char('q') => {
                                app.input_mode = InputMode::Normal;
                            }
                            KeyCode::Char('n') => {
                                // sort by name

                                // we only care about the path not the selcted item
                                let get_path_from_list = &app.files[0];
                                let cur_path = get_curr_path(get_path_from_list.to_string());
                                let file_path_list = get_file_path_data(
                                    cur_path,
                                    app.show_hidden_files,
                                    SortBy::Name,
                                    &sort_type,
                                )?;
                                app.files = file_path_list.clone();
                                app.read_only_files = file_path_list.clone();
                                app.update_file_references();
                                app.input_mode = InputMode::Normal;
                            }

                            KeyCode::Char('s') => {
                                // TODO: this code should be refactor to into a reusable method since is
                                // used in multiple places
                                let get_path_from_list = &app.files[0];
                                let cur_path = get_curr_path(get_path_from_list.to_string());

                                let file_path_list = get_file_path_data(
                                    cur_path,
                                    app.show_hidden_files,
                                    SortBy::Size,
                                    &sort_type,
                                )?;
                                app.files = file_path_list.clone();
                                app.read_only_files = file_path_list.clone();
                                app.update_file_references();
                                app.input_mode = InputMode::Normal;
                            }

                            KeyCode::Char('t') => {
                                let get_path_from_list = &app.files[0];
                                let cur_path = get_curr_path(get_path_from_list.to_string());

                                let file_path_list = get_file_path_data(
                                    cur_path,
                                    app.show_hidden_files,
                                    SortBy::DateAdded,
                                    &sort_type,
                                )?;
                                app.files = file_path_list.clone();
                                app.read_only_files = file_path_list.clone();
                                app.update_file_references();
                                app.input_mode = InputMode::Normal;
                            }
                            KeyCode::Char('a') => {
                                sort_type = SortType::ASC;
                            }
                            KeyCode::Char('d') => {
                                sort_type = SortType::DESC;
                            }
                            _ => {}
                        },

                        InputMode::WatchKeyBinding => match key.code {
                            KeyCode::Char('q') => {
                                app.input_mode = InputMode::Normal;
                            }
                            _ => {}
                        },
                        _ => {}
                    }
                }
            }
        }
    }

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;

    terminal.show_cursor()?;
    terminal.clear()?;
    Ok(())
}
