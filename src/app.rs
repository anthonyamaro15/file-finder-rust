use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{atomic::AtomicBool, mpsc, Arc};
use std::time::{Duration, Instant};

use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use log::debug;
use ratatui::widgets::ListState;
use rayon::prelude::*;

use crate::directory_store::DirectoryStore;
use crate::errors::AppResult;
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
    DirectoryListing { path: String, entries: Vec<String> },
    /// Preview loading failed
    Error { path: String, message: String },
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
    WatchTrash,
    WatchCreate,
    WatchRename,
    WatchSort,
    WatchKeyBinding,
    CacheLoading,
    /// Choosing the folder for "Copy to…" or "Move to…".
    PickFolder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchScope {
    CurrentDirectory,
    Root,
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

/// Copy or move, for transfers to another folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferKind {
    Copy,
    Move,
}

impl TransferKind {
    /// "Copy" or "Move", as the picker's title starts.
    pub fn title(self) -> &'static str {
        match self {
            TransferKind::Copy => "Copy",
            TransferKind::Move => "Move",
        }
    }
}

/// The open "Copy to…"/"Move to…" picker. Its query is edited in
/// `create_edit_file_name` like the create and rename inputs.
#[derive(Debug)]
pub struct FolderPicker {
    pub kind: TransferKind,
    /// The item to copy or move.
    pub source: String,
    pub results: Vec<SearchResult>,
    pub selected: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub file_path: String,
    pub display_name: String,
    pub score: i64,
    pub is_directory: bool,
    pub original_index: usize,
}

fn search_result_order(a: &SearchResult, b: &SearchResult) -> std::cmp::Ordering {
    b.score
        .cmp(&a.score)
        .then_with(|| {
            a.display_name
                .to_lowercase()
                .cmp(&b.display_name.to_lowercase())
        })
        .then_with(|| a.file_path.cmp(&b.file_path))
        .then_with(|| a.original_index.cmp(&b.original_index))
}

fn truncate_top_search_results(search_results: &mut Vec<SearchResult>, max_results: usize) {
    if max_results == 0 {
        search_results.clear();
        return;
    }

    search_results.sort_unstable_by(search_result_order);
    search_results.truncate(max_results);
}

fn deduplicate_sorted_search_results(search_results: &mut Vec<SearchResult>) {
    let mut seen_paths = HashSet::new();
    search_results.retain(|result| seen_paths.insert(result.file_path.clone()));
}

fn truncate_top_unique_search_results(search_results: &mut Vec<SearchResult>, max_results: usize) {
    if max_results == 0 {
        search_results.clear();
        return;
    }

    search_results.sort_unstable_by(search_result_order);
    deduplicate_sorted_search_results(search_results);
    search_results.truncate(max_results);
}

fn push_bounded_search_result(
    search_results: &mut Vec<SearchResult>,
    result: SearchResult,
    max_results: usize,
) {
    if max_results == 0 {
        return;
    }

    search_results.push(result);

    let trim_threshold = max_results
        .saturating_mul(2)
        .max(max_results.saturating_add(1));

    if search_results.len() > trim_threshold {
        truncate_top_search_results(search_results, max_results);
    }
}

fn path_label_components(file_path: &str) -> Vec<String> {
    let components: Vec<String> = Path::new(file_path)
        .components()
        .filter_map(|component| match component {
            std::path::Component::Prefix(prefix) => {
                Some(prefix.as_os_str().to_string_lossy().to_string())
            }
            std::path::Component::Normal(component) => {
                Some(component.to_string_lossy().to_string())
            }
            _ => None,
        })
        .collect();

    if components.is_empty() {
        vec![file_path.to_string()]
    } else {
        components
    }
}

fn suffix_label(components: &[String], depth: usize) -> String {
    let start = components.len().saturating_sub(depth.max(1));
    components[start..].join("/")
}

fn apply_short_unique_display_names(search_results: &mut [SearchResult]) {
    if search_results.is_empty() {
        return;
    }

    let components: Vec<Vec<String>> = search_results
        .iter()
        .map(|result| path_label_components(&result.file_path))
        .collect();
    let mut depths = vec![1; search_results.len()];

    loop {
        let labels: Vec<String> = components
            .iter()
            .zip(depths.iter())
            .map(|(components, depth)| suffix_label(components, *depth))
            .collect();
        let mut expanded = false;

        for i in 0..labels.len() {
            let has_duplicate = labels
                .iter()
                .enumerate()
                .any(|(j, label)| i != j && label == &labels[i]);

            if has_duplicate && depths[i] < components[i].len() {
                depths[i] += 1;
                expanded = true;
            }
        }

        if !expanded {
            for (result, label) in search_results.iter_mut().zip(labels) {
                result.display_name = label;
            }
            break;
        }
    }
}

fn global_search_result(
    matcher: &SkimMatcherV2,
    index: usize,
    file_path: &str,
    query: &str,
) -> Option<SearchResult> {
    let path = Path::new(file_path);
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(file_path);

    let filename_score = matcher.fuzzy_match(file_name, query);
    let path_score = matcher.fuzzy_match(file_path, query).map(|score| score / 3);
    let score = filename_score.or(path_score)?;

    Some(SearchResult {
        file_path: file_path.to_string(),
        display_name: file_name.to_string(),
        score,
        is_directory: path.is_dir(),
        original_index: index,
    })
}

/// Global search over the folder index: the best `max_results` matches for `query`,
/// with short labels that tell same-named folders apart.
pub fn global_search_results(
    store: &DirectoryStore,
    query: &str,
    max_results: usize,
) -> Vec<SearchResult> {
    if max_results == 0 {
        return Vec::new();
    }

    let mut search_results = store
        .directories
        .par_iter()
        .enumerate()
        .map_init(SkimMatcherV2::default, |matcher, (index, file_path)| {
            global_search_result(matcher, index, file_path, query)
        })
        .filter_map(|result| result)
        .fold(Vec::new, |mut search_results, result| {
            push_bounded_search_result(&mut search_results, result, max_results);
            search_results
        })
        .reduce(Vec::new, |mut left, mut right| {
            left.append(&mut right);
            truncate_top_search_results(&mut left, max_results);
            left
        });

    truncate_top_unique_search_results(&mut search_results, max_results);
    apply_short_unique_display_names(&mut search_results);
    search_results
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

    pub preview_files: Vec<String>,
    pub preview_file_content: String,

    pub filtered_indexes: Vec<usize>,

    pub file_read_only_label_list: Vec<String>,

    // Search functionality
    pub search_results: Vec<SearchResult>,
    pub search_scope: SearchScope,
    pub global_search_mode: bool,
    pub max_search_results: usize,

    // Copy progress tracking
    pub copy_in_progress: bool,
    pub copy_cancel: Arc<AtomicBool>,
    pub copy_progress_message: String,
    pub copy_files_processed: usize,
    pub copy_total_files: usize,
    /// Source name and destination label ("other pane", "'Projects'") of a running
    /// copy to another folder, which keeps its name there.
    pub copy_transfer: Option<(String, String)>,
    pub copy_destination: Option<PathBuf>,

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
    pub delete_target_path: Option<String>,
    pub delete_target_name: String,
    pub delete_target_is_dir: bool,
    pub delete_target_file_count: Option<usize>,
    pub delete_target_total_size: Option<u64>,

    // Delete progress tracking (for async delete)
    pub delete_in_progress: bool,
    pub delete_progress_message: String,
    /// Folder being deleted in the background.
    pub delete_path: Option<String>,

    // Preview debounce tracking
    pub preview_pending_path: Option<String>,
    pub preview_pending_since: Option<Instant>,
    /// Path of the most recent preview request, so selection changes can be detected.
    pub preview_requested_path: Option<String>,

    // Async preview loading
    pub preview_receiver: Option<mpsc::Receiver<PreviewMessage>>,
    pub preview_loading: bool,
    pub preview_loading_path: Option<String>,

    // Async image loading (separate channel because DynamicImage has different traits)
    pub image_receiver: Option<mpsc::Receiver<ImageLoadResult>>,

    /// Running move, trash, or ZIP extraction, if any.
    pub job: Option<BackgroundJob>,
    /// The open "Copy to…"/"Move to…" picker.
    pub folder_picker: Option<FolderPicker>,
    /// Exit key and time of the last "operation in progress" quit warning.
    pub quit_warning: Option<(&'static str, Instant)>,
}

/// How long the same exit key, pressed again, leaves despite a running operation.
pub const QUIT_CONFIRM_WINDOW: Duration = Duration::from_secs(3);

/// Result of async image loading
pub struct ImageLoadResult {
    pub path: String,
    pub result: Result<(image::DynamicImage, String), String>, // (image, format) or error message
}

/// One-shot background operation (move, trash, ZIP extraction) that reports a single result.
#[derive(Debug)]
pub struct BackgroundJob {
    /// Operation name for warnings, e.g. "move".
    pub kind: &'static str,
    /// List title while it runs, e.g. "Moving 'notes'…".
    pub status: String,
    receiver: mpsc::Receiver<Result<String, String>>,
    pub cancel: Arc<AtomicBool>,
    /// Paths the job changes, re-read into the folder index when it ends.
    pub changes: Vec<PathBuf>,
}

impl BackgroundJob {
    /// Run `work` on its own thread; its result is the notification, info or error.
    pub fn spawn(
        kind: &'static str,
        status: String,
        changes: Vec<PathBuf>,
        work: impl FnOnce(Arc<AtomicBool>) -> Result<String, String> + Send + 'static,
    ) -> Self {
        let (tx, receiver) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        std::thread::spawn(move || {
            let _ = tx.send(work(worker_cancel));
        });
        Self {
            kind,
            status,
            receiver,
            cancel,
            changes,
        }
    }

    /// Wait until the worker has finished writing and cleaning up.
    pub fn wait(self) -> Result<String, String> {
        self.receiver.recv().unwrap_or_else(|_| {
            Err(format!(
                "{} stopped unexpectedly before reporting a result",
                sentence_case(self.kind)
            ))
        })
    }

    /// The result once the job finishes; a job that dies without one reports an error.
    pub fn poll(&self) -> Option<Result<String, String>> {
        match self.receiver.try_recv() {
            Ok(result) => Some(result),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => Some(Err(format!(
                "{} stopped unexpectedly before reporting a result",
                sentence_case(self.kind)
            ))),
        }
    }
}

/// Capitalize an operation name that starts a message ("move" -> "Move").
fn sentence_case(word: &str) -> String {
    let mut chars = word.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
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

            preview_files: Vec::new(),
            preview_file_content: String::new(),

            filtered_indexes: all_indexes,
            file_read_only_label_list: file_labels,

            // Initialize search functionality
            search_results: Vec::new(),
            search_scope: SearchScope::CurrentDirectory,
            global_search_mode: false,
            max_search_results: 500,

            // Initialize copy progress tracking
            copy_in_progress: false,
            copy_cancel: Arc::new(AtomicBool::new(false)),
            copy_progress_message: String::new(),
            copy_files_processed: 0,
            copy_total_files: 0,
            copy_transfer: None,
            copy_destination: None,

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
            delete_target_path: None,
            delete_target_name: String::new(),
            delete_target_is_dir: false,
            delete_target_file_count: None,
            delete_target_total_size: None,

            // Initialize delete progress tracking
            delete_in_progress: false,
            delete_progress_message: String::new(),
            delete_path: None,

            // Initialize preview debounce tracking
            preview_pending_path: None,
            preview_pending_since: None,
            preview_requested_path: None,

            // Initialize async preview loading
            preview_receiver: None,
            preview_loading: false,
            preview_loading_path: None,

            // Initialize async image loading
            image_receiver: None,

            job: None,
            folder_picker: None,
            quit_warning: None,
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
        self.insert_char_without_filter(new_char);
        self.filter_files(self.input.clone(), store);
        self.move_cursor_right();
    }

    pub fn insert_char_without_filter(&mut self, new_char: char) {
        let index = self.byte_index();
        self.input.insert(index, new_char);
    }

    pub fn enter_search(&mut self, scope: SearchScope) {
        self.input_mode = InputMode::Editing;
        self.search_scope = scope;
        self.input.clear();
        self.character_index = 0;
        self.filtered_indexes = (0..self.files.len()).collect();
        self.search_results.clear();
        self.global_search_mode = matches!(scope, SearchScope::Root);
    }

    pub fn filter_files(&mut self, input: String, store: DirectoryStore) {
        if input.is_empty() {
            // Show all files when no search input
            self.filtered_indexes = (0..self.files.len()).collect();
            self.search_results.clear();
            self.global_search_mode = matches!(self.search_scope, SearchScope::Root);
            return;
        }

        self.global_search_mode = matches!(self.search_scope, SearchScope::Root);

        if self.global_search_mode {
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
        // For global search, we don't use filtered_indexes since we're showing different files
        self.search_results = global_search_results(store, query, self.max_search_results);
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
        if self.delete_char_without_filter() {
            self.filter_files(self.input.clone(), store);
        }
    }

    pub fn delete_char_without_filter(&mut self) -> bool {
        let is_not_cursor_leftmost = self.character_index != 0;
        if is_not_cursor_leftmost {
            let current_index = self.character_index;
            let from_left_to_current_index = current_index - 1;

            let before_char_to_delete = self.input.chars().take(from_left_to_current_index);

            let after_char_to_delete = self.input.chars().skip(current_index);

            self.input = before_char_to_delete.chain(after_char_to_delete).collect();
            self.move_cursor_left();
            return true;
        }

        false
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

    pub fn validate_user_input(&self, input: &str) -> Option<IDE> {
        match input {
            "nvim" => Some(IDE::NVIM),
            "vscode" => Some(IDE::VSCODE),
            "zed" => Some(IDE::ZED),
            _ => None,
        }
    }

    pub fn get_selected_ide(&self) -> Option<String> {
        if let Some(selection) = &self.selected_id {
            match selection {
                IDE::NVIM => Some("nvim".to_string()),
                // VS Code's CLI binary is `code`
                IDE::VSCODE => Some("code".to_string()),
                IDE::ZED => Some("zed".to_string()),
            }
        } else {
            None
        }
    }

    /// Refresh directory files after a watcher event, preserving the visible selection.
    /// An active local search is re-run on the new files so the visible list,
    /// `search_results` and `filtered_indexes` stay consistent.
    pub fn refresh_files_from_watcher(&mut self, files: Vec<String>, state: &mut ListState) {
        let selected_path = self.get_selected_path(state.selected());
        let local_query = (!self.global_search_mode
            && !self.input.is_empty()
            && (self.input_mode == InputMode::Editing || !self.search_results.is_empty()))
        .then(|| self.input.clone());
        if local_query.is_none() && !self.global_search_mode {
            // Local results without a query can't be re-run; show the refreshed listing.
            self.search_results.clear();
        }

        self.curr_index = state.selected();
        self.files = files.clone();
        self.read_only_files = files;
        self.update_file_references_with_selection_preservation(selected_path.clone());

        if let Some(query) = local_query {
            self.perform_local_search(&query);
            // update_file_references clears the input outside of Editing mode.
            self.input = query;
            let new_index = self.search_results.len().checked_sub(1).map(|last_index| {
                selected_path
                    .and_then(|path| self.search_results.iter().position(|r| r.file_path == path))
                    .or(self.curr_index)
                    .unwrap_or(0)
                    .min(last_index)
            });
            self.curr_index = new_index;
            self.preserved_selection_index = None;
            state.select(new_index);
        } else if self.search_results.is_empty() {
            state.select(self.curr_index);
        }
    }

    /// Update filtered indexes and labels when file list changes
    pub fn update_file_references(&mut self) {
        self.update_file_references_with_selection_preservation(None);
    }

    /// Show the refreshed directory after a successful file operation.
    pub fn refresh_files_after_operation(
        &mut self,
        files: Vec<String>,
        state: &mut ListState,
        select_path: Option<String>,
    ) {
        self.curr_index = self.preserved_selection_index.take().or(state.selected());
        self.clear_search();
        self.files = files.clone();
        self.read_only_files = files;
        // Match directory aliases (e.g. /tmp and /private/tmp), but keep symlink names distinct.
        let select_path = select_path.and_then(|path| {
            let path = Path::new(&path);
            self.files
                .iter()
                .find(|file| {
                    let file = Path::new(file);
                    file == path
                        || (file.file_name() == path.file_name()
                            && file
                                .parent()
                                .and_then(|p| p.canonicalize().ok())
                                .zip(path.parent().and_then(|p| p.canonicalize().ok()))
                                .is_some_and(|(a, b)| a == b))
                })
                .cloned()
        });
        self.update_file_references_with_selection_preservation(select_path);
        state.select(self.curr_index);
        self.preserved_selection_index = self.curr_index;
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

        // Search selection indexes refer to search_results, not files.
        self.preserved_selection_index = None;
        if self.search_results.is_empty() {
            self.preserved_selection_index = new_selection_index;
            self.curr_index = self.files.len().checked_sub(1).map(|last_index| {
                new_selection_index
                    .or(self.curr_index)
                    .unwrap_or(0)
                    .min(last_index)
            });
        }

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

    /// Name of the running background operation, if any; only one runs at a time.
    pub fn busy_operation(&self) -> Option<&'static str> {
        if self.copy_in_progress {
            Some("copy")
        } else if self.delete_in_progress {
            Some("delete")
        } else {
            self.job.as_ref().map(|job| job.kind)
        }
    }

    /// Warning for starting another operation while one runs; `None` means go ahead.
    pub fn busy_warning(&self) -> Option<String> {
        self.busy_operation()
            .map(|operation| format!("Wait for the current {operation} to finish"))
    }

    /// Run `work` in the background unless another operation runs (returns its warning).
    /// `changes` are the paths it changes, for the folder index.
    pub fn start_job(
        &mut self,
        kind: &'static str,
        status: String,
        changes: Vec<PathBuf>,
        work: impl FnOnce(Arc<AtomicBool>) -> Result<String, String> + Send + 'static,
    ) -> Result<(), String> {
        if let Some(warning) = self.busy_warning() {
            return Err(warning);
        }
        self.quit_warning = None;
        self.job = Some(BackgroundJob::spawn(kind, status, changes, work));
        Ok(())
    }

    /// Take the job's result and changed paths once it finishes, which frees the slot
    /// for the next operation.
    pub fn poll_job(&mut self) -> Option<(Result<String, String>, Vec<PathBuf>)> {
        let result = self.job.as_ref()?.poll()?;
        let job = self.job.take()?;
        Some((result, job.changes))
    }

    /// Allow leaving `ff` with `key` unless an operation runs. Then the first press
    /// returns a warning, and the same key again within the window leaves anyway.
    pub fn confirm_quit(&mut self, key: &'static str, now: Instant) -> Result<(), String> {
        let Some(operation) = self.busy_operation() else {
            return Ok(());
        };
        let repeated = self.quit_warning.is_some_and(|(warned_key, warned_at)| {
            warned_key == key && now.saturating_duration_since(warned_at) <= QUIT_CONFIRM_WINDOW
        });
        if repeated {
            return Ok(());
        }
        self.quit_warning = Some((key, now));
        Err(format!(
            "{} in progress — press {key} again to quit anyway",
            sentence_case(operation)
        ))
    }

    /// Show a copy as running; its updates arrive on the copy channel. `transfer` is the
    /// source name and destination label of a copy to another folder.
    pub fn start_copy(&mut self, destination: PathBuf, transfer: Option<(String, String)>) {
        self.quit_warning = None;
        self.copy_cancel = Arc::new(AtomicBool::new(false));
        self.copy_in_progress = true;
        self.copy_progress_message = String::from("Starting copy...");
        self.copy_files_processed = 0;
        self.copy_total_files = 0;
        self.copy_transfer = transfer;
        self.copy_destination = Some(destination);
    }

    /// Source and destination folder for `Shift+C`/`Shift+M`: the active pane's
    /// highlighted item goes to the other pane's folder.
    pub fn other_pane_transfer(&self, selected_index: Option<usize>) -> Option<(String, String)> {
        if self.is_right_pane_active() {
            let destination = self.left_directory_path().to_string_lossy().to_string();
            Some((self.right_pane.get_selected_path()?, destination))
        } else {
            let source = self.get_selected_path(selected_index)?;
            Some((source, self.right_pane.current_directory.clone()))
        }
    }

    /// Freeze the highlighted target for the delete confirmation popup.
    pub fn prepare_delete(&mut self, selected_index: Option<usize>) {
        let Some(selected) = self.get_selected_path(selected_index) else {
            return;
        };
        let path = Path::new(&selected);
        let metadata = path.symlink_metadata().ok();
        self.delete_target_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(&selected)
            .to_string();
        self.delete_target_is_dir = metadata.as_ref().is_some_and(|m| m.is_dir());
        // Directory stats are counted by the worker, never while opening the popup.
        self.delete_target_file_count = None;
        self.delete_target_total_size = metadata.filter(|m| !m.is_dir()).map(|m| m.len());
        self.delete_target_path = Some(selected);
        self.render_popup = true;
        self.input_mode = InputMode::WatchDelete;
    }

    /// Freeze the highlighted target for the Trash confirmation popup.
    pub fn prepare_trash(&mut self, selected_index: Option<usize>) {
        self.prepare_delete(selected_index);
        if self.delete_target_path.is_some() {
            self.input_mode = InputMode::WatchTrash;
        }
    }

    /// Close a delete popup and consume its target, whether confirmed or cancelled.
    pub fn close_delete_popup(&mut self) -> Option<String> {
        self.render_popup = false;
        self.input_mode = InputMode::Normal;
        self.delete_target_path.take()
    }

    /// Get the active current directory as a path.
    pub fn current_directory_path(&self) -> PathBuf {
        if self.view_mode.is_dual_pane() && self.is_right_pane_active() {
            PathBuf::from(&self.right_pane.current_directory)
        } else {
            self.left_directory_path()
        }
    }

    /// Get the directory shown by the left (main) listing, whichever pane is active.
    pub fn left_directory_path(&self) -> PathBuf {
        if !self.current_directory.is_empty() {
            PathBuf::from(&self.current_directory)
        } else if let Some(first_file) = self.files.first() {
            Path::new(first_file)
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from("."))
        } else {
            PathBuf::from(".")
        }
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

    /// Check if the right pane is active
    pub fn is_right_pane_active(&self) -> bool {
        self.active_pane == PanePosition::Right
    }

    /// Initialize the right pane with the current directory
    /// Called when entering dual-pane mode
    pub fn init_right_pane(&mut self) {
        // Clone current directory and files to right pane
        self.right_pane =
            crate::pane::Pane::new(self.current_directory.clone(), self.files.clone());
        self.right_pane.show_hidden_files = self.show_hidden_files;
    }

    /// Request a preview update with debouncing
    /// Instead of loading immediately, stores the path and timestamp
    pub fn request_preview(&mut self, path: String) {
        self.preview_requested_path = Some(path.clone());
        self.preview_pending_path = Some(path);
        self.preview_pending_since = Some(Instant::now());
    }

    /// Request a preview when the selected path differs from the last request.
    /// Returns true once when the selection becomes empty, after dropping the
    /// app-side preview state, so the caller can clear the rest of the preview.
    pub fn sync_preview_with_selection(&mut self, selected_index: Option<usize>) -> bool {
        let path = self.get_selected_path(selected_index);
        if path == self.preview_requested_path {
            return false;
        }
        match path {
            Some(path) => {
                self.request_preview(path);
                false
            }
            None => {
                self.preview_requested_path = None;
                self.clear_pending_preview();
                self.cancel_preview_loads();
                self.preview_files.clear();
                self.preview_file_content.clear();
                true
            }
        }
    }

    /// Check if a pending preview is ready to load (debounce period elapsed)
    /// Returns the path if ready, None otherwise
    pub fn take_ready_preview(&mut self, debounce_ms: u64) -> Option<String> {
        if let (Some(path), Some(since)) = (&self.preview_pending_path, self.preview_pending_since)
        {
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

    /// Drop in-flight preview and image loads so their late results
    /// can't land on whatever is selected next.
    pub fn cancel_preview_loads(&mut self) {
        self.preview_receiver = None;
        self.image_receiver = None;
        self.preview_loading = false;
        self.preview_loading_path = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn local_search_app(files: &[&str], query: &str) -> App {
        let mut app = App::new(files.iter().map(|file| file.to_string()).collect());
        app.enter_search(SearchScope::CurrentDirectory);
        app.input = query.to_string();
        app.perform_local_search(query);
        app
    }

    fn select_path(app: &App, state: &mut ListState, path: &str) {
        let index = app.search_results.iter().position(|r| r.file_path == path);
        state.select(index);
    }

    fn assert_local_search_consistent(app: &App) {
        let shown: Vec<&str> = app
            .filtered_indexes
            .iter()
            .map(|&index| app.files[index].as_str())
            .collect();
        let results: Vec<&str> = app
            .search_results
            .iter()
            .map(|r| r.file_path.as_str())
            .collect();
        assert_eq!(shown, results);
    }

    #[test]
    fn selected_path_resolves_visible_list_and_rejects_invalid_indexes() {
        let mut app = App::new(vec![
            "alpha.txt".into(),
            "bravo.txt".into(),
            "charlie.txt".into(),
        ]);
        assert_eq!(app.get_selected_path(Some(0)).as_deref(), Some("alpha.txt"));
        assert_eq!(app.get_selected_path(None), None);
        assert_eq!(app.get_selected_path(Some(3)), None);

        app.perform_local_search("charlie");
        assert_eq!(
            app.get_selected_path(Some(0)).as_deref(),
            Some("charlie.txt")
        );
        assert_eq!(app.get_selected_path(None), None);
        assert_eq!(app.get_selected_path(Some(1)), None);

        app.global_search_mode = true;
        app.search_results[0].file_path = "/elsewhere/charlie.txt".into();
        app.files.clear();
        assert_eq!(
            app.get_selected_path(Some(0)).as_deref(),
            Some("/elsewhere/charlie.txt")
        );
        assert_eq!(app.get_selected_path(Some(1)), None);
        app.clear_search();
        assert_eq!(app.get_selected_path(Some(0)), None);
    }

    #[test]
    fn delete_confirmation_keeps_the_displayed_target_after_selection_changes() {
        let tmp = tempfile::tempdir().unwrap();
        let alpha = tmp.path().join("alpha.txt").to_string_lossy().to_string();
        let charlie = tmp.path().join("charlie.txt").to_string_lossy().to_string();
        std::fs::write(&alpha, "alpha").unwrap();
        std::fs::write(&charlie, "charlie").unwrap();
        let mut app = local_search_app(&[&alpha, &charlie], "charlie");

        app.prepare_delete(Some(0));
        assert_eq!(app.delete_target_name, "charlie.txt");
        assert_eq!(app.delete_target_total_size, Some(7));
        assert_eq!(app.delete_target_path.as_deref(), Some(charlie.as_str()));
        assert!(app.render_popup);
        assert_eq!(app.input_mode, InputMode::WatchDelete);

        app.clear_search();
        app.files = vec![alpha.clone()];
        let target = app.close_delete_popup().unwrap();
        assert_eq!(target, charlie);
        crate::operations::handle_delete_based_on_type(&target).unwrap();
        assert!(Path::new(&alpha).exists());
        assert!(!Path::new(&charlie).exists());
        assert_eq!(app.delete_target_path, None);
        assert!(!app.render_popup);
        assert_eq!(app.input_mode, InputMode::Normal);
        assert_eq!(app.close_delete_popup(), None);
    }

    #[test]
    fn delete_confirmation_ignores_missing_or_out_of_range_selection() {
        for selected in [None, Some(1), Some(usize::MAX)] {
            let mut app = App::new(vec!["alpha.txt".into()]);
            app.prepare_delete(selected);
            assert!(!app.render_popup);
            assert_eq!(app.input_mode, InputMode::Normal);
            assert_eq!(app.delete_target_path, None);
        }
    }

    #[test]
    fn operation_refresh_selects_new_path_after_sorting_and_clears_local_search() {
        let mut app = local_search_app(&["cwd/alpha.txt", "cwd/zulu.txt"], "alpha");
        app.preserved_selection_index = Some(0);
        let mut state = ListState::default();
        state.select(Some(0));

        app.refresh_files_after_operation(
            vec![
                "cwd/alpha.txt".into(),
                "cwd/bravo.txt".into(),
                "cwd/zulu.txt".into(),
            ],
            &mut state,
            Some("cwd/zulu.txt".into()),
        );

        assert_eq!(state.selected(), Some(2));
        assert_eq!(
            app.get_selected_path(state.selected()).as_deref(),
            Some("cwd/zulu.txt")
        );
        assert_eq!(app.curr_index, Some(2));
        assert_eq!(app.preserved_selection_index, Some(2));
        assert!(app.search_results.is_empty());
        assert!(app.input.is_empty());
    }

    #[test]
    fn parent_listing_selects_previous_directory_or_first_row() {
        for (previous, files, expected) in [
            ("parent/zulu", vec!["parent/alpha", "parent/zulu"], Some(1)),
            (
                "parent/absent",
                vec!["parent/alpha", "parent/zulu"],
                Some(0),
            ),
            ("parent/absent", Vec::new(), None),
        ] {
            let mut app = App::new(vec![format!("{previous}/child")]);
            let mut state = ListState::default();
            state.select(Some(9));
            app.preserved_selection_index = Some(0);

            app.refresh_files_after_operation(
                files.into_iter().map(String::from).collect(),
                &mut state,
                Some(previous.into()),
            );

            assert_eq!(state.selected(), expected);
            assert_eq!(app.curr_index, expected);
            assert_eq!(app.preserved_selection_index, expected);
            assert_eq!(app.read_only_files, app.files);
        }
    }

    #[test]
    fn operation_refresh_missing_path_keeps_the_clamped_selection() {
        for (preserved, selected, files, expected) in [
            (
                Some(1),
                Some(0),
                vec!["cwd/alpha", "cwd/bravo", "cwd/charlie"],
                Some(1),
            ),
            (
                None,
                Some(1),
                vec!["cwd/alpha", "cwd/bravo", "cwd/charlie"],
                Some(1),
            ),
            (Some(9), Some(0), vec!["cwd/alpha", "cwd/bravo"], Some(1)),
            (None, None, vec!["cwd/alpha"], Some(0)),
            (Some(1), Some(1), Vec::new(), None),
        ] {
            let mut app = App::new(Vec::new());
            app.preserved_selection_index = preserved;
            let mut state = ListState::default();
            state.select(selected);

            app.refresh_files_after_operation(
                files.into_iter().map(String::from).collect(),
                &mut state,
                Some("elsewhere/alpha".into()),
            );

            assert_eq!(state.selected(), expected);
            assert_eq!(app.curr_index, expected);
            assert_eq!(app.preserved_selection_index, expected);
        }
    }

    #[cfg(unix)]
    #[test]
    fn operation_refresh_matches_directory_aliases_without_following_leaf_symlinks() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("real");
        let alias = dir.path().join("alias");
        std::fs::create_dir(&real).unwrap();
        std::os::unix::fs::symlink(&real, &alias).unwrap();
        std::fs::write(real.join("alpha.txt"), "source").unwrap();
        std::os::unix::fs::symlink("alpha.txt", real.join("zulu.txt")).unwrap();
        let files = crate::utils::get_file_path_data(
            real.to_string_lossy().into_owned(),
            false,
            SortBy::Name,
            &SortType::ASC,
        )
        .unwrap();
        let mut app = App::new(files.clone());
        let mut state = ListState::default();
        state.select(Some(0));

        app.refresh_files_after_operation(
            files,
            &mut state,
            Some(alias.join("zulu.txt").to_string_lossy().into_owned()),
        );

        assert_eq!(state.selected(), Some(1));
        assert_eq!(
            app.get_selected_path(state.selected()),
            Some(real.join("zulu.txt").to_string_lossy().into_owned())
        );
    }

    #[test]
    fn operation_refresh_clears_search_and_clamps_preserved_selection() {
        for (files, expected) in [
            (
                vec!["cwd/alpha.txt".into(), "cwd/bravo.txt".into()],
                Some(1),
            ),
            (Vec::new(), None),
        ] {
            let mut app = local_search_app(&["elsewhere/charlie.txt"], "charlie");
            app.global_search_mode = true;
            app.current_directory = "cwd".into();
            app.character_index = 7;
            app.preserved_selection_index = Some(20);
            let mut state = ListState::default();
            state.select(Some(0));

            app.refresh_files_after_operation(files.clone(), &mut state, None);

            assert_eq!(app.current_directory, "cwd");
            assert!(app.search_results.is_empty());
            assert!(app.input.is_empty());
            assert!(!app.global_search_mode);
            assert_eq!(app.character_index, 0);
            assert_eq!(app.files, files);
            assert_eq!(app.read_only_files, app.files);
            assert_eq!(app.filtered_indexes.len(), app.files.len());
            assert_eq!(state.selected(), expected);
            assert_eq!(app.curr_index, expected);
            assert_eq!(app.preserved_selection_index, expected);
        }
    }

    #[test]
    fn watcher_refresh_preserves_local_search_selection() {
        let mut app = local_search_app(&["a_foo.rs", "b.rs", "c_foo.rs"], "foo");
        let mut state = ListState::default();
        select_path(&app, &mut state, "c_foo.rs");
        app.preserved_selection_index = Some(2);

        app.refresh_files_from_watcher(
            vec![
                "a_foo.rs".into(),
                "b.rs".into(),
                "c_foo.rs".into(),
                "d.rs".into(),
            ],
            &mut state,
        );

        assert_eq!(app.input, "foo");
        assert_eq!(app.preserved_selection_index, None);
        assert_eq!(app.curr_index, state.selected());
        assert_eq!(
            app.get_selected_path(state.selected()).as_deref(),
            Some("c_foo.rs")
        );
        assert_eq!(app.files, vec!["a_foo.rs", "b.rs", "c_foo.rs", "d.rs"]);
        assert_eq!(app.read_only_files, app.files);
        assert_eq!(app.search_results.len(), 2);
        assert_local_search_consistent(&app);
    }

    #[test]
    fn watcher_refresh_adds_new_local_search_match() {
        let mut app = local_search_app(&["a_foo.rs", "b.rs"], "foo");
        let mut state = ListState::default();
        select_path(&app, &mut state, "a_foo.rs");

        app.refresh_files_from_watcher(
            vec!["0_foo.rs".into(), "a_foo.rs".into(), "b.rs".into()],
            &mut state,
        );

        let mut results: Vec<&str> = app
            .search_results
            .iter()
            .map(|r| r.file_path.as_str())
            .collect();
        results.sort();
        assert_eq!(results, ["0_foo.rs", "a_foo.rs"]);
        assert_local_search_consistent(&app);
        assert_eq!(
            app.get_selected_path(state.selected()).as_deref(),
            Some("a_foo.rs")
        );
    }

    #[test]
    fn watcher_refresh_clamps_when_selected_local_match_disappears() {
        let mut app = local_search_app(&["a_foo.rs", "b.rs", "c_foo.rs"], "foo");
        // After Esc the search stays applied in Normal mode.
        app.input_mode = InputMode::Normal;
        let mut state = ListState::default();
        select_path(&app, &mut state, "c_foo.rs");

        app.refresh_files_from_watcher(vec!["a_foo.rs".into(), "b.rs".into()], &mut state);

        assert_eq!(app.input, "foo");
        assert_eq!(state.selected(), Some(0));
        assert_eq!(app.curr_index, Some(0));
        assert_eq!(
            app.get_selected_path(state.selected()).as_deref(),
            Some("a_foo.rs")
        );
        assert_local_search_consistent(&app);

        app.refresh_files_from_watcher(vec!["b.rs".into()], &mut state);

        assert!(app.search_results.is_empty());
        assert_eq!(state.selected(), None);
        assert_eq!(app.curr_index, None);
    }

    #[test]
    fn watcher_refresh_preserves_root_search_selection_with_fewer_directory_files() {
        let mut app = App::new(vec!["cwd/a".into()]);
        app.search_scope = SearchScope::Root;
        app.global_search_mode = true;
        app.search_results = (0..50)
            .map(|index| SearchResult {
                file_path: format!("elsewhere/result_{index}"),
                display_name: format!("result_{index}"),
                score: 100,
                is_directory: false,
                original_index: index,
            })
            .collect();
        let mut state = ListState::default();
        state.select(Some(30));

        for files in [vec!["cwd/b".into()], Vec::new()] {
            app.refresh_files_from_watcher(files, &mut state);

            assert_eq!(state.selected(), Some(30));
            assert_eq!(app.curr_index, Some(30));
            assert_eq!(app.preserved_selection_index, None);
            assert_eq!(app.get_list_length(), 50);
        }
    }

    #[test]
    fn watcher_refresh_preserves_or_clamps_directory_selection() {
        for (files, expected) in [
            (vec!["c".into(), "a".into()], Some(0)),
            (vec!["a".into(), "b".into()], Some(1)),
            (Vec::new(), None),
        ] {
            let mut app = App::new(vec!["a".into(), "b".into(), "c".into()]);
            let mut state = ListState::default();
            state.select(Some(2));

            app.refresh_files_from_watcher(files, &mut state);

            assert_eq!(state.selected(), expected);
            assert_eq!(app.curr_index, expected);
        }
    }

    #[test]
    fn refresh_selection_clamps_when_selected_file_is_removed() {
        let mut app = App::new(vec!["a".into(), "b".into(), "c".into()]);
        app.curr_index = Some(2);
        let selected_path = app.get_selected_path(app.curr_index);
        app.files = vec!["a".into(), "b".into()];
        app.update_file_references_with_selection_preservation(selected_path);

        assert_eq!(app.curr_index, Some(1));
    }

    #[test]
    fn refresh_selection_preserves_path_after_reordering() {
        let mut app = App::new(vec!["a".into(), "b".into(), "c".into()]);
        app.curr_index = Some(2);
        let selected_path = app.get_selected_path(app.curr_index);
        app.files = vec!["c".into(), "a".into()];
        app.update_file_references_with_selection_preservation(selected_path);

        assert_eq!(app.curr_index, Some(0));
        assert_eq!(app.preserved_selection_index, Some(0));
        assert_eq!(app.get_selected_path(app.curr_index).as_deref(), Some("c"));
    }

    #[test]
    fn refresh_selection_clears_when_list_is_empty() {
        let mut app = App::new(vec!["a".into()]);
        let selected_path = app.get_selected_path(app.curr_index);
        app.files.clear();
        app.update_file_references_with_selection_preservation(selected_path);

        assert_eq!(app.curr_index, None);
        assert_eq!(app.preserved_selection_index, None);
    }

    #[test]
    fn update_file_references_clamps_selection_without_path() {
        let mut app = App::new(vec!["a".into(), "b".into()]);
        app.curr_index = Some(1);
        app.files = vec!["a".into()];
        app.update_file_references();

        assert_eq!(app.curr_index, Some(0));
    }

    /// Spin until the app's job reports, failing instead of hanging if it never does.
    fn wait_for_job(app: &mut App) -> Result<String, String> {
        let deadline = Instant::now() + std::time::Duration::from_secs(5);
        loop {
            if let Some((result, _)) = app.poll_job() {
                return result;
            }
            assert!(Instant::now() < deadline, "job never reported a result");
            std::thread::yield_now();
        }
    }

    /// Make `kind` the running operation; job kinds run until the sender is used.
    fn start_operation(app: &mut App, kind: &'static str) -> Option<mpsc::Sender<()>> {
        match kind {
            "copy" => app.start_copy(PathBuf::from("copy_target"), None),
            "delete" => app.delete_in_progress = true,
            _ => {
                let (release, wait) = mpsc::channel::<()>();
                app.start_job(kind, format!("Running {kind}"), Vec::new(), move |_| {
                    wait.recv().map_err(|e| e.to_string())?;
                    Ok(format!("Finished {kind}"))
                })
                .unwrap();
                return Some(release);
            }
        }
        None
    }

    #[test]
    fn busy_guard_blocks_a_second_operation_of_each_kind_and_allows_browsing() {
        for kind in ["copy", "delete", "move", "extraction"] {
            let mut app = App::new(vec!["alpha.txt".into(), "bravo.txt".into()]);
            assert_eq!(app.busy_warning(), None);
            let release = start_operation(&mut app, kind);
            let warning = format!("Wait for the current {kind} to finish");
            assert_eq!(app.busy_operation(), Some(kind));
            assert_eq!(app.busy_warning().as_deref(), Some(warning.as_str()));

            // Another operation is refused and leaves the running one in place.
            for other in ["move", "extraction"] {
                let refused =
                    app.start_job(other, "Second".into(), Vec::new(), |_| Ok(String::new()));
                assert_eq!(refused, Err(warning.clone()));
            }
            assert_eq!(app.busy_operation(), Some(kind));
            if release.is_some() {
                assert_eq!(app.job.as_ref().unwrap().status, format!("Running {kind}"));
            }

            // Browsing neither waits for nor ends the operation.
            app.enter_search(SearchScope::CurrentDirectory);
            app.perform_local_search("bravo");
            assert_eq!(app.get_selected_path(Some(0)).as_deref(), Some("bravo.txt"));
            app.clear_search();
            app.view_mode = app.view_mode.next();
            app.sync_preview_with_selection(Some(1));
            assert_eq!(app.take_ready_preview(0).as_deref(), Some("bravo.txt"));
            assert_eq!(app.busy_operation(), Some(kind));

            if let Some(release) = release {
                assert_eq!(app.poll_job(), None);
                release.send(()).unwrap();
                assert_eq!(wait_for_job(&mut app), Ok(format!("Finished {kind}")));
                assert_eq!(app.busy_operation(), None);
            }
        }
    }

    #[test]
    fn panicking_background_job_reports_an_error() {
        let mut app = App::new(Vec::new());
        app.start_job(
            "extraction",
            "Extracting 'a.zip'…".into(),
            Vec::new(),
            |_| panic!("extraction worker crashed"),
        )
        .unwrap();
        assert_eq!(
            wait_for_job(&mut app),
            Err("Extraction stopped unexpectedly before reporting a result".into())
        );
        assert_eq!(app.busy_operation(), None);
    }

    #[test]
    fn other_pane_transfer_uses_the_highlighted_item_of_the_active_pane() {
        let mut app = local_search_app(&["/l/alpha.txt", "/l/charlie.txt"], "charlie");
        app.current_directory = "/l".into();
        app.view_mode = ViewMode::DualPane;
        app.right_pane = crate::pane::Pane::new("/r".into(), vec!["/r/bravo.txt".into()]);

        // Row 0 is the visible search result, not the folder's first file.
        assert_eq!(
            app.other_pane_transfer(Some(0)),
            Some(("/l/charlie.txt".into(), "/r".into()))
        );
        assert_eq!(app.other_pane_transfer(Some(1)), None);

        app.active_pane = PanePosition::Right;
        assert_eq!(
            app.other_pane_transfer(None),
            Some(("/r/bravo.txt".into(), "/l".into()))
        );
    }

    #[test]
    fn quit_while_busy_needs_the_same_key_again_within_the_window() {
        let mut app = App::new(Vec::new());
        let start = Instant::now();
        assert_eq!(app.confirm_quit("q", start), Ok(()));

        app.start_copy(PathBuf::from("copy_target"), None);
        let warning = "Copy in progress — press q again to quit anyway".to_string();
        assert_eq!(app.confirm_quit("q", start), Err(warning.clone()));
        assert_eq!(
            app.confirm_quit("q", start + Duration::from_secs(2)),
            Ok(())
        );

        // Once the window passes, the next press warns again and restarts it.
        let late = start + QUIT_CONFIRM_WINDOW + Duration::from_secs(1);
        assert_eq!(app.confirm_quit("q", late), Err(warning));
        // Another exit key is confirmed on its own.
        let second = late + Duration::from_secs(1);
        assert_eq!(
            app.confirm_quit("Enter", second),
            Err("Copy in progress — press Enter again to quit anyway".into())
        );
        assert_eq!(app.confirm_quit("Enter", second), Ok(()));

        app.copy_in_progress = false;
        app.delete_in_progress = true;
        let later = second + QUIT_CONFIRM_WINDOW * 2;
        assert_eq!(
            app.confirm_quit("q", later),
            Err("Delete in progress — press q again to quit anyway".into())
        );
        app.delete_in_progress = false;
        assert_eq!(app.confirm_quit("o", later), Ok(()));
    }

    #[test]
    fn current_directory_path_falls_back_to_dot_without_state() {
        let app = App::new(Vec::new());

        assert_eq!(app.current_directory_path(), PathBuf::from("."));
    }

    #[test]
    fn current_directory_path_uses_watched_directory_when_files_empty() {
        let mut app = App::new(Vec::new());
        app.current_directory = "/tmp/ff-empty-current".to_string();

        assert_eq!(
            app.current_directory_path(),
            PathBuf::from("/tmp/ff-empty-current")
        );
    }

    #[test]
    fn current_directory_path_uses_active_right_pane_directory() {
        let mut app = App::new(vec!["/tmp/ff-left/file.txt".to_string()]);
        app.current_directory = "/tmp/ff-left".to_string();
        app.view_mode = ViewMode::DualPane;
        app.active_pane = PanePosition::Right;
        app.right_pane = crate::pane::Pane::new("/tmp/ff-right".to_string(), Vec::new());

        assert_eq!(app.current_directory_path(), PathBuf::from("/tmp/ff-right"));
        assert_eq!(app.left_directory_path(), PathBuf::from("/tmp/ff-left"));
    }

    #[test]
    fn sync_preview_follows_selection_changes_only() {
        let mut app = App::new(vec!["a".into(), "b".into()]);

        assert!(!app.sync_preview_with_selection(Some(0)));
        assert_eq!(app.take_ready_preview(0).as_deref(), Some("a"));
        assert!(!app.sync_preview_with_selection(Some(0)));
        assert_eq!(app.take_ready_preview(0), None);

        // A replaced list (e.g. entering a folder) changes the path at the same index.
        app.files = vec!["dir/a.zip".into()];
        assert!(!app.sync_preview_with_selection(Some(0)));
        assert_eq!(app.take_ready_preview(0).as_deref(), Some("dir/a.zip"));

        app.files.clear();
        app.preview_files = vec!["stale".into()];
        assert!(app.sync_preview_with_selection(None));
        assert!(app.preview_files.is_empty());
        assert!(!app.sync_preview_with_selection(None));
    }

    #[test]
    fn cancel_preview_loads_drops_results_for_old_selection() {
        let mut app = App::new(vec!["a.png".into()]);
        let (preview_tx, preview_rx) = mpsc::channel();
        let (image_tx, image_rx) = mpsc::channel();
        app.preview_receiver = Some(preview_rx);
        app.image_receiver = Some(image_rx);
        app.preview_loading = true;
        app.preview_loading_path = Some("a.png".into());

        app.cancel_preview_loads();

        assert!(app.preview_receiver.is_none());
        assert!(app.image_receiver.is_none());
        assert!(!app.preview_loading);
        assert_eq!(app.preview_loading_path, None);
        // The old loaders' late results go nowhere.
        let stale_preview = PreviewMessage::Error {
            path: "a.png".into(),
            message: String::new(),
        };
        assert!(preview_tx.send(stale_preview).is_err());
        let stale_image = ImageLoadResult {
            path: "a.png".into(),
            result: Err(String::new()),
        };
        assert!(image_tx.send(stale_image).is_err());
    }
}
