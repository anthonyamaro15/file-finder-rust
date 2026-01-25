use std::path::Path;
use std::sync::mpsc;
use std::time::Instant;

use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use log::debug;

use crate::directory_store::DirectoryStore;
use crate::errors::{AppError, AppResult};
use crate::file_reader_content::FileType;
use crate::watcher::{FileSystemWatcher, WatcherEvent};

use crate::config::{Settings, Theme, ThemeColors};
use crate::utils::files::{SortBy, SortType};

extern crate copypasta;

/// Message sent from async preview loading thread
#[derive(Debug, Clone)]
pub enum PreviewMessage {
    /// Preview content loaded successfully
    Loaded {
        path: String,
        content: String,
        file_type: FileType,
        /// Pre-highlighted content for source code files
        highlighted_content: Option<String>,
    },
    /// Directory contents loaded
    DirectoryListing {
        path: String,
        entries: Vec<String>,
    },
    /// Preview loading failed
    Error {
        path: String,
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum IDE {
    NVIM,
    VSCODE,
    ZED,
}

#[derive(Debug, Clone, PartialEq)]
pub enum InputMode {
    Normal,
    Editing,
    WatchDelete,
    WatchCreate,
    WatchRename,
    WatchSort,
    WatchKeyBinding,
    CacheLoading,
}

/// View mode for the main content area
#[derive(Debug, Clone, PartialEq, Default)]
pub enum ViewMode {
    /// Normal: File list (50%) + Preview (50%) - default behavior
    #[default]
    Normal,
    /// FullList: File list only (100%) - no preview pane
    FullList,
    /// DualPane: Two file lists side by side (50%/50%)
    DualPane,
}

impl ViewMode {
    /// Cycle to the next view mode
    pub fn next(&self) -> Self {
        match self {
            ViewMode::Normal => ViewMode::FullList,
            ViewMode::FullList => ViewMode::DualPane,
            ViewMode::DualPane => ViewMode::Normal,
        }
    }

    /// Get display name for status bar
    pub fn display_name(&self) -> &'static str {
        match self {
            ViewMode::Normal => "Normal",
            ViewMode::FullList => "Full List",
            ViewMode::DualPane => "Dual Pane",
        }
    }

    /// Check if preview should be shown
    pub fn shows_preview(&self) -> bool {
        matches!(self, ViewMode::Normal)
    }

    /// Check if this is dual pane mode
    pub fn is_dual_pane(&self) -> bool {
        matches!(self, ViewMode::DualPane)
    }
}

/// Which pane is currently active in dual-pane mode
#[derive(Debug, Clone, PartialEq, Default)]
pub enum PanePosition {
    #[default]
    Left,
    Right,
}

impl PanePosition {
    /// Toggle to the other pane
    pub fn toggle(&self) -> Self {
        match self {
            PanePosition::Left => PanePosition::Right,
            PanePosition::Right => PanePosition::Left,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub file_path: String,
    pub display_name: String,
    pub score: i64,
    pub is_directory: bool,
    pub original_index: usize,
}


#[derive(Debug)]
pub struct App {
    pub input: String,
    pub character_index: usize,
    pub input_mode: InputMode,
    pub view_mode: ViewMode,
    /// Which pane is currently active (for dual-pane mode)
    pub active_pane: PanePosition,
    /// Right pane for dual-pane mode (left pane uses existing App fields)
    pub right_pane: crate::pane::Pane,
    pub message: Vec<String>,
    pub files: Vec<String>,
    pub read_only_files: Vec<String>,
    pub selected_id: Option<IDE>,
    /// Print path to stdout instead of copying to clipboard (for shell integration)
    pub print_path: bool,
    pub render_popup: bool,
    pub prev_dir: String,

    pub show_hidden_files: bool,
    // create and edit file name
    pub create_edit_file_name: String,
    pub char_index: usize,
    pub is_create_edit_error: bool,
    pub error_message: String,

    // edit
    pub current_path_to_edit: String,
    pub current_name_to_edit: String,

    pub loading: bool,
    pub curr_index: Option<usize>,
    pub curr_stats: String,

    pub preview_files: Vec<String>,
    pub preview_file_content: String,

    pub filtered_indexes: Vec<usize>,

    pub file_read_only_label_list: Vec<String>,

    // Search functionality
    pub search_results: Vec<SearchResult>,
    pub global_search_mode: bool,

    // Copy progress tracking
    pub copy_in_progress: bool,
    pub copy_progress_message: String,
    pub copy_files_processed: usize,
    pub copy_total_files: usize,


    // File system watcher for real-time updates
    pub file_watcher: Option<FileSystemWatcher>,
    pub current_directory: String,

    // Cache loading progress tracking
    pub cache_loading_progress:
        Option<std::sync::mpsc::Receiver<crate::directory_store::CacheBuildProgress>>,
    pub cache_directories_processed: usize,
    pub cache_current_directory: String,
    pub cache_loading_complete: bool,
    pub completed_cache_store: Option<crate::directory_store::DirectoryStore>,

    // Cache build timing
    pub cache_build_started_at: Option<Instant>,
    pub cache_build_elapsed_ms: Option<u128>,

    // Configuration and theming
    pub settings: Settings,
    pub theme_colors: ThemeColors,

    // Sort state - persisted across file operations
    pub sort_by: SortBy,
    pub sort_type: SortType,

    // Selection preservation for file operations
    pub preserved_selection_index: Option<usize>,

    // Delete confirmation info
    pub delete_target_name: String,
    pub delete_target_is_dir: bool,
    pub delete_target_file_count: Option<usize>,
    pub delete_target_total_size: Option<u64>,

    // Delete progress tracking (for async delete)
    pub delete_in_progress: bool,
    pub delete_progress_message: String,

    // Preview debounce tracking
    pub preview_pending_path: Option<String>,
    pub preview_pending_since: Option<Instant>,

    // Async preview loading
    pub preview_receiver: Option<mpsc::Receiver<PreviewMessage>>,
    pub preview_loading: bool,
    pub preview_loading_path: Option<String>,

    // Async image loading (separate channel because DynamicImage has different traits)
    pub image_receiver: Option<mpsc::Receiver<ImageLoadResult>>,
}

/// Result of async image loading
pub struct ImageLoadResult {
    pub path: String,
    pub result: Result<(image::DynamicImage, String), String>, // (image, format) or error message
}

impl App {
    pub fn new(files: Vec<String>) -> Self {
        let files_clone = files.clone();
        let mut all_indexes: Vec<usize> = Vec::new();
        let mut file_labels: Vec<String> = Vec::new();

        for (index, file) in files.iter().enumerate() {
            let new_path = Path::new(file);
            let get_file_name = new_path
                .file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.to_string())
                .unwrap_or_else(|| {
                    // Fallback to the full path if filename extraction fails
                    file.clone()
                });

            all_indexes.push(index);
            file_labels.push(get_file_name);
        }

        Self {
            input: String::new(),
            input_mode: InputMode::Normal,
            view_mode: ViewMode::default(),
            active_pane: PanePosition::default(),
            right_pane: crate::pane::Pane::default(),
            message: Vec::new(),
            files,
            read_only_files: files_clone,
            character_index: 0,
            selected_id: None,
            print_path: false,
            render_popup: false,
            prev_dir: String::new(),
            show_hidden_files: false,
            create_edit_file_name: String::new(),
            char_index: 0,
            is_create_edit_error: false,
            error_message: String::new(),
            current_path_to_edit: String::new(),
            current_name_to_edit: String::new(),
            loading: false,
            curr_index: Some(0),
            curr_stats: String::new(),

            preview_files: Vec::new(),
            preview_file_content: String::new(),

            filtered_indexes: all_indexes,
            file_read_only_label_list: file_labels,

            // Initialize search functionality
            search_results: Vec::new(),
            global_search_mode: false,

            // Initialize copy progress tracking
            copy_in_progress: false,
            copy_progress_message: String::new(),
            copy_files_processed: 0,
            copy_total_files: 0,


            // Initialize file watcher
            file_watcher: None,
            current_directory: String::new(),

            // Initialize cache loading progress tracking
            cache_loading_progress: None,
            cache_directories_processed: 0,
            cache_current_directory: String::new(),
            cache_loading_complete: false,
            completed_cache_store: None,

            // Cache build timing
            cache_build_started_at: None,
            cache_build_elapsed_ms: None,

            // Load default configuration - this will be replaced with proper loading
            settings: Settings::default(),
            theme_colors: Theme::default().to_colors().unwrap_or_else(|_| {
                // Fallback to hardcoded theme if parsing fails
                panic!("Failed to parse default theme")
            }),

            // Initialize sort state - defaults to ascending by name
            sort_by: SortBy::Default,
            sort_type: SortType::ASC,

            // Initialize selection preservation
            preserved_selection_index: None,

            // Initialize delete confirmation info
            delete_target_name: String::new(),
            delete_target_is_dir: false,
            delete_target_file_count: None,
            delete_target_total_size: None,

            // Initialize delete progress tracking
            delete_in_progress: false,
            delete_progress_message: String::new(),

            // Initialize preview debounce tracking
            preview_pending_path: None,
            preview_pending_since: None,

            // Initialize async preview loading
            preview_receiver: None,
            preview_loading: false,
            preview_loading_path: None,

            // Initialize async image loading
            image_receiver: None,
        }
    }

    pub fn move_cursor_left(&mut self) {
        let cursor_moved_left = self.character_index.saturating_sub(1);
        self.character_index = self.clamp_cursor(cursor_moved_left);
    }

    pub fn move_cursor_right(&mut self) {
        let cursor_moved_right = self.character_index.saturating_add(1);
        self.character_index = self.clamp_cursor(cursor_moved_right);
    }

    pub fn enter_char(&mut self, new_char: char, store: DirectoryStore) {
        let index = self.byte_index();
        self.input.insert(index, new_char);
        self.filter_files(self.input.clone(), store);
        self.move_cursor_right();
    }

    pub fn filter_files(&mut self, input: String, store: DirectoryStore) {
        if input.is_empty() {
            // Show all files when no search input
            self.filtered_indexes = (0..self.files.len()).collect();
            self.search_results.clear();
            self.global_search_mode = false;
            return;
        }

        // Determine if this should be a global search (when input starts with space or special char)
        let is_global_search = input.starts_with(' ') || input.starts_with('/');
        self.global_search_mode = is_global_search;

        if is_global_search {
            // Global search across directory cache
            self.perform_global_search(input.trim(), &store);
        } else {
            // Local search in current directory
            self.perform_local_search(&input);
        }
    }

    /// Perform fuzzy search in current directory with scoring
    pub fn perform_local_search(&mut self, query: &str) {
        let matcher = SkimMatcherV2::default();
        let mut search_results: Vec<SearchResult> = Vec::new();

        for (index, file_path) in self.files.iter().enumerate() {
            let path = Path::new(file_path);
            let file_name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or(file_path);

            // Try fuzzy matching on filename first (higher priority)
            let filename_score = matcher.fuzzy_match(file_name, query);

            // Also try matching on full path (lower priority)
            let path_score = matcher.fuzzy_match(file_path, query).map(|score| score / 2); // Reduce path match score

            // Use the best score
            if let Some(score) = filename_score.or(path_score) {
                let is_directory = path.is_dir();

                // Boost directory scores slightly for better organization
                let adjusted_score = if is_directory { score + 10 } else { score };

                search_results.push(SearchResult {
                    file_path: file_path.clone(),
                    display_name: file_name.to_string(),
                    score: adjusted_score,
                    is_directory,
                    original_index: index,
                });
            }
        }

        // Sort by score (descending) then by name
        search_results.sort_by(|a, b| {
            b.score.cmp(&a.score).then_with(|| {
                a.display_name
                    .to_lowercase()
                    .cmp(&b.display_name.to_lowercase())
            })
        });

        // Update filtered indexes based on search results
        self.filtered_indexes = search_results
            .iter()
            .map(|result| result.original_index)
            .collect();

        self.search_results = search_results;
    }

    /// Perform global search across directory cache
    pub fn perform_global_search(&mut self, query: &str, store: &DirectoryStore) {
        let matcher = SkimMatcherV2::default();
        let mut search_results: Vec<SearchResult> = Vec::new();

        // Search through the directory cache
        for (index, file_path) in store.directories.iter().enumerate() {
            let path = Path::new(file_path);
            let file_name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or(file_path);

            // Try fuzzy matching
            let filename_score = matcher.fuzzy_match(file_name, query);
            let path_score = matcher.fuzzy_match(file_path, query).map(|score| score / 3); // Even lower priority for global path matches

            if let Some(score) = filename_score.or(path_score) {
                let is_directory = path.is_dir();

                search_results.push(SearchResult {
                    file_path: file_path.clone(),
                    display_name: file_name.to_string(),
                    score,
                    is_directory,
                    original_index: index,
                });
            }
        }

        // Sort by score (descending)
        search_results.sort_by(|a, b| b.score.cmp(&a.score));

        // For global search, we don't use filtered_indexes since we're showing different files
        self.search_results = search_results;
        self.filtered_indexes.clear();
    }

    pub fn byte_index(&mut self) -> usize {
        self.input
            .char_indices()
            .map(|(i, _)| i)
            .nth(self.character_index)
            .unwrap_or(self.input.len())
    }

    pub fn delete_char(&mut self, store: DirectoryStore) {
        let is_not_cursor_leftmost = self.character_index != 0;
        if is_not_cursor_leftmost {
            let current_index = self.character_index;
            let from_left_to_current_index = current_index - 1;

            let before_char_to_delete = self.input.chars().take(from_left_to_current_index);

            let after_char_to_delete = self.input.chars().skip(current_index);

            self.input = before_char_to_delete.chain(after_char_to_delete).collect();
            self.filter_files(self.input.clone(), store);
            self.move_cursor_left();
        }
    }

    pub fn clamp_cursor(&mut self, new_cursor_pos: usize) -> usize {
        new_cursor_pos.clamp(0, self.input.chars().count())
    }

    pub fn reset_cursor(&mut self) {
        self.character_index = 0;
    }

    pub fn reset_create_edit_values(&mut self) {
        self.create_edit_file_name.clear();
        self.char_index = 0;

        // reset error vaules
        self.is_create_edit_error = false;
        self.error_message = String::new();
    }

    pub fn submit_message(&mut self) {
        self.message.push(self.input.clone());
        self.input.clear();
        self.reset_cursor();
    }

    // TODO: could we combine search, create, edit input field methods?
    // there is a lot of duplication here
    //
    //
    //
    //
    pub fn add_char(&mut self, new_char: char) {
        let index = self.byte_char_index();
        self.create_edit_file_name.insert(index, new_char);
        self.move_create_edit_cursor_right();
    }
    pub fn byte_char_index(&mut self) -> usize {
        self.create_edit_file_name
            .char_indices()
            .map(|(i, _)| i)
            .nth(self.char_index)
            .unwrap_or(self.create_edit_file_name.len())
    }

    pub fn delete_c(&mut self) {
        let is_not_cursor_leftmost = self.char_index != 0;
        if is_not_cursor_leftmost {
            let current_index = self.char_index;
            let from_left_to_current_index = current_index - 1;

            let before_char_to_delete = self
                .create_edit_file_name
                .chars()
                .take(from_left_to_current_index);

            let after_char_to_delete = self.create_edit_file_name.chars().skip(current_index);

            self.create_edit_file_name =
                before_char_to_delete.chain(after_char_to_delete).collect();
            self.move_create_edit_cursor_left();
        }
    }

    pub fn move_create_edit_cursor_left(&mut self) {
        let cursor_moved_left = self.char_index.saturating_sub(1);
        self.char_index = self.clamp_create_edit_cursor(cursor_moved_left);
    }

    pub fn move_create_edit_cursor_right(&mut self) {
        let cursor_moved_right = self.char_index.saturating_add(1);
        self.char_index = self.clamp_create_edit_cursor(cursor_moved_right);
    }

    pub fn clamp_create_edit_cursor(&mut self, new_cursor_pos: usize) -> usize {
        new_cursor_pos.clamp(0, self.create_edit_file_name.chars().count())
    }

    pub fn get_selected_ide(&self) -> Option<String> {
        if let Some(selection) = &self.selected_id {
            match selection {
                IDE::NVIM => Some("nvim".to_string()),
                IDE::VSCODE => Some("vscode".to_string()),
                IDE::ZED => Some("zed".to_string()),
            }
        } else {
            None
        }
    }

    /// Update filtered indexes and labels when file list changes
    pub fn update_file_references(&mut self) {
        self.update_file_references_with_selection_preservation(None);
    }

    /// Update file references with optional selection preservation
    pub fn update_file_references_with_selection_preservation(
        &mut self,
        preserve_selection_for: Option<String>,
    ) {
        let old_selection_path = preserve_selection_for;

        // Reset filtered indexes to show all files
        self.filtered_indexes.clear();
        self.file_read_only_label_list.clear();

        // Track the new index for the previously selected item
        let mut new_selection_index: Option<usize> = None;

        for (index, file) in self.files.iter().enumerate() {
            let new_path = Path::new(file);
            let get_file_name = new_path
                .file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.to_string())
                .unwrap_or_else(|| {
                    // Fallback to the full path if filename extraction fails
                    file.clone()
                });

            self.filtered_indexes.push(index);
            self.file_read_only_label_list.push(get_file_name);

            // Check if this is the previously selected item
            if let Some(ref old_path) = old_selection_path {
                if file == old_path {
                    new_selection_index = Some(index);
                }
            }
        }

        // Store the new selection index for the main loop to use
        self.preserved_selection_index = new_selection_index;

        // Clear any search input when navigating
        if !self.input.is_empty() && self.input_mode != InputMode::Editing {
            self.input.clear();
        }

    }


    /// Start watching a directory for file system changes
    pub fn start_watching_directory<P: AsRef<Path>>(
        &mut self,
        directory_path: P,
    ) -> Result<(), String> {
        let path = directory_path.as_ref();

        // Stop any existing watcher
        self.stop_watching();

        // Create new watcher
        let mut watcher = FileSystemWatcher::new()
            .map_err(|e| format!("Failed to create file watcher: {}", e))?;

        // Start watching the directory
        watcher
            .watch_directory(path)
            .map_err(|e| format!("Failed to start watching directory: {}", e))?;

        // Store the watcher and current directory
        self.file_watcher = Some(watcher);
        self.current_directory = path.to_string_lossy().to_string();

        debug!("Started watching directory: {}", path.display());
        Ok(())
    }

    /// Stop watching the current directory
    pub fn stop_watching(&mut self) {
        if let Some(mut watcher) = self.file_watcher.take() {
            watcher.stop_watching();
            debug!("Stopped watching directory: {}", self.current_directory);
        }
        self.current_directory.clear();
    }

    /// Check for file system events and update the application state accordingly
    pub fn process_file_system_events(&mut self) -> Vec<String> {
        let mut messages = Vec::new();

        if let Some(ref watcher) = self.file_watcher {
            let events = watcher.poll_events();

            for event in events {
                match event {
                    WatcherEvent::FilesCreated(paths) => {
                        let count = paths.len();
                        if count == 1 {
                            if let Some(path) = paths.first() {
                                if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                                    messages.push(format!("File created: {}", filename));
                                }
                            }
                        } else {
                            messages.push(format!("{} files created", count));
                        }
                        self.refresh_file_list();
                    }
                    WatcherEvent::FilesDeleted(paths) => {
                        let count = paths.len();
                        if count == 1 {
                            if let Some(path) = paths.first() {
                                if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                                    messages.push(format!("File deleted: {}", filename));
                                }
                            }
                        } else {
                            messages.push(format!("{} files deleted", count));
                        }
                        self.refresh_file_list();
                    }
                    WatcherEvent::FilesModified(paths) => {
                        // Only show modification messages for single files to avoid spam
                        if paths.len() == 1 {
                            if let Some(path) = paths.first() {
                                if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                                    messages.push(format!("File modified: {}", filename));
                                }
                            }
                        }
                        // Don't refresh file list for modifications unless it's a significant change
                    }
                    WatcherEvent::FilesRenamed { from, to } => {
                        if from.len() == 1 && to.len() == 1 {
                            if let (Some(old_name), Some(new_name)) = (
                                from.first()
                                    .and_then(|p| p.file_name())
                                    .and_then(|n| n.to_str()),
                                to.first()
                                    .and_then(|p| p.file_name())
                                    .and_then(|n| n.to_str()),
                            ) {
                                messages.push(format!("File renamed: {} → {}", old_name, new_name));
                            }
                        } else {
                            messages.push(format!("{} files renamed", from.len().max(to.len())));
                        }
                        self.refresh_file_list();
                    }
                    WatcherEvent::DirectoryChanged => {
                        messages.push("Directory changed".to_string());
                        self.refresh_file_list();
                    }
                    WatcherEvent::WatcherError(error) => {
                        messages.push(format!("File watcher error: {}", error));
                        // Consider stopping the watcher on persistent errors
                        self.stop_watching();
                    }
                }
            }
        }

        messages
    }

    /// Refresh the file list from the current directory
    /// This should be called when file system events indicate changes
    fn refresh_file_list(&mut self) {
        if self.current_directory.is_empty() {
            return;
        }

        // This is a placeholder - in a real implementation, you would:
        // 1. Re-scan the current directory
        // 2. Update self.files with the new list
        // 3. Call self.update_file_references() to update indexes and labels
        // 4. Reset pagination and search state if needed

        debug!(
            "File list refresh requested for: {}",
            self.current_directory
        );
    }

    /// Start cache loading and set up progress tracking
    pub fn start_cache_loading(
        &mut self,
        receiver: std::sync::mpsc::Receiver<crate::directory_store::CacheBuildProgress>,
        started_at: Instant,
    ) {
        self.input_mode = InputMode::CacheLoading;
        self.cache_loading_progress = Some(receiver);
        self.cache_directories_processed = 0;
        self.cache_current_directory = String::new();
        self.cache_loading_complete = false;
        self.cache_build_started_at = Some(started_at);
        self.cache_build_elapsed_ms = None;
    }

    /// Process cache loading progress messages
    pub fn process_cache_loading_progress(&mut self) -> bool {
        if let Some(ref receiver) = self.cache_loading_progress {
            // Check for new progress messages
            while let Ok(progress) = receiver.try_recv() {
                match progress {
                    crate::directory_store::CacheBuildProgress::Progress {
                        directories_found,
                        current_path,
                    } => {
                        self.cache_directories_processed = directories_found;
                        self.cache_current_directory = current_path;
                    }
                    crate::directory_store::CacheBuildProgress::Completed { store } => {
                        // Store the completed cache for later use
                        self.completed_cache_store = Some(store);
                        self.cache_loading_complete = true;
                        self.cache_loading_progress = None;

                        // Compute elapsed time if we have a start timestamp
                        if let Some(started) = self.cache_build_started_at.take() {
                            let elapsed = started.elapsed();
                            self.cache_build_elapsed_ms = Some(elapsed.as_millis());
                            debug!(
                                "Cache build completed in {:.3}s ({} ms) - processed {} directories",
                                elapsed.as_secs_f64(),
                                elapsed.as_millis(),
                                self.cache_directories_processed
                            );
                        }
                        return true; // Cache loading is complete
                    }
                    crate::directory_store::CacheBuildProgress::Error(error_msg) => {
                        // Handle cache loading error
                        debug!("Cache loading error: {}", error_msg);
                        self.cache_loading_complete = true;
                        self.cache_loading_progress = None;
                        // Set input mode back to normal even on error
                        self.input_mode = InputMode::Normal;
                        return true; // Stop loading even on error
                    }
                }
            }
        }
        false // Cache loading is still in progress
    }

    /// Finish cache loading and switch back to normal mode
    pub fn finish_cache_loading(&mut self, _directories: Vec<String>) {
        // Do NOT override the current file list here. We want to remain focused on the
        // current directory (e.g., start_path) after cache creation completes.
        // The completed cache is used for global search and saved to disk separately.

        // Keep current files, but refresh labels/indexes just in case other state needs updating.
        self.update_file_references();

        // Switch back to normal input mode
        self.input_mode = InputMode::Normal;

        // Clear cache loading state
        self.cache_loading_progress = None;
        self.cache_directories_processed = 0;
        self.cache_current_directory = String::new();
        self.cache_loading_complete = false;
        self.completed_cache_store = None;
    }

    /// Get cache loading progress information for display
    pub fn get_cache_loading_info(&self) -> Option<(usize, String)> {
        if self.input_mode == InputMode::CacheLoading {
            Some((
                self.cache_directories_processed,
                self.cache_current_directory.clone(),
            ))
        } else {
            None
        }
    }

    /// Initialize configuration and theme from files or defaults
    pub fn initialize_config_and_theme(&mut self) -> AppResult<()> {
        // Ensure config directories exist
        crate::config::ensure_config_directories()?;

        // Load settings (creates default if missing)
        self.settings = Settings::load()?;

        // Load theme specified in settings (creates default if missing)
        let theme = Theme::load(&self.settings.theme)?;

        // Convert theme to processed colors
        self.theme_colors = theme.to_colors()?;

        // Debug info moved to log file to avoid interfering with --print-path
        debug!("Loaded configuration with theme: {}", self.settings.theme);

        Ok(())
    }

    // ============================================================
    // Helper methods for navigation and selection
    // ============================================================

    /// Get the list length based on current mode (search results or files).
    /// Returns the count of items available for navigation.
    pub fn get_list_length(&self) -> usize {
        if !self.search_results.is_empty() {
            self.search_results.len()
        } else {
            self.files.len()
        }
    }

    /// Get the selected path based on current mode (search results or files).
    /// Returns None if the index is invalid or the list is empty.
    pub fn get_selected_path(&self, selected_index: Option<usize>) -> Option<String> {
        let index = selected_index?;

        if !self.search_results.is_empty() {
            // Search mode (global or local) - get path from search results
            self.search_results
                .get(index)
                .map(|result| result.file_path.clone())
        } else {
            // Normal mode - get path from files list
            self.files.get(index).cloned()
        }
    }

    /// Get the current directory path from the first file in the list.
    /// Returns the parent directory of the first file, or an empty string if no files.
    pub fn get_current_dir(&self) -> String {
        if let Some(first_file) = self.files.first() {
            let path = Path::new(first_file);
            path.parent()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default()
        } else {
            String::new()
        }
    }

    /// Check if we're currently in search mode (either global or local).
    pub fn is_in_search_mode(&self) -> bool {
        !self.search_results.is_empty()
    }

    /// Clear search state and return to normal file list view.
    pub fn clear_search(&mut self) {
        self.global_search_mode = false;
        self.search_results.clear();
        self.input.clear();
        self.character_index = 0;
    }

    // =========================================================================
    // Dual Pane Helper Methods
    // =========================================================================

    /// Toggle active pane (for dual-pane mode)
    pub fn toggle_active_pane(&mut self) {
        self.active_pane = self.active_pane.toggle();
    }

    /// Check if the left pane is active
    pub fn is_left_pane_active(&self) -> bool {
        self.active_pane == PanePosition::Left
    }

    /// Check if the right pane is active
    pub fn is_right_pane_active(&self) -> bool {
        self.active_pane == PanePosition::Right
    }

    /// Initialize the right pane with the current directory
    /// Called when entering dual-pane mode
    pub fn init_right_pane(&mut self) {
        // Clone current directory and files to right pane
        self.right_pane = crate::pane::Pane::new(
            self.current_directory.clone(),
            self.files.clone(),
        );
        self.right_pane.show_hidden_files = self.show_hidden_files;
    }

    /// Get the directory of the inactive pane (for cross-pane operations)
    pub fn get_inactive_pane_directory(&self) -> String {
        match self.active_pane {
            PanePosition::Left => self.right_pane.current_directory.clone(),
            PanePosition::Right => self.current_directory.clone(),
        }
    }

    /// Refresh the right pane's file list
    pub fn refresh_right_pane(&mut self) {
        self.right_pane.refresh_files();
    }

    /// Request a preview update with debouncing
    /// Instead of loading immediately, stores the path and timestamp
    pub fn request_preview(&mut self, path: String) {
        self.preview_pending_path = Some(path);
        self.preview_pending_since = Some(Instant::now());
    }

    /// Check if a pending preview is ready to load (debounce period elapsed)
    /// Returns the path if ready, None otherwise
    pub fn take_ready_preview(&mut self, debounce_ms: u64) -> Option<String> {
        if let (Some(path), Some(since)) = (&self.preview_pending_path, self.preview_pending_since) {
            if since.elapsed().as_millis() >= debounce_ms as u128 {
                let path = path.clone();
                self.preview_pending_path = None;
                self.preview_pending_since = None;
                return Some(path);
            }
        }
        None
    }

    /// Clear any pending preview request (e.g., when selection changes)
    pub fn clear_pending_preview(&mut self) {
        self.preview_pending_path = None;
        self.preview_pending_since = None;
    }
}


