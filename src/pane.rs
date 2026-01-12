//! Pane module for dual-pane file management.
//!
//! A Pane represents a single file listing view with its own directory,
//! file list, selection state, and search functionality.

use std::path::Path;

use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use ratatui::widgets::ListState;

use crate::app::SearchResult;
use crate::utils::files::{get_inner_files_info, SortBy, SortType};

/// Represents a single pane in the file browser.
/// Each pane has its own directory, file list, and selection state.
#[derive(Debug)]
pub struct Pane {
    /// Current directory path for this pane
    pub current_directory: String,
    /// Files in this pane's directory
    pub files: Vec<String>,
    /// Read-only copy of files for filtering/search
    pub read_only_files: Vec<String>,
    /// Display labels for files (filename only, not full path)
    pub file_labels: Vec<String>,
    /// Current selection index
    pub selected_index: Option<usize>,
    /// Filtered indexes after search
    pub filtered_indexes: Vec<usize>,
    /// Search results for fuzzy matching
    pub search_results: Vec<SearchResult>,
    /// Whether global search is active in this pane
    pub global_search_mode: bool,
    /// Search input text for this pane
    pub search_input: String,
    /// Cursor position in search input
    pub cursor_index: usize,
    /// ListState for ratatui rendering
    pub list_state: ListState,
    /// Whether to show hidden files
    pub show_hidden_files: bool,
}

impl Default for Pane {
    fn default() -> Self {
        Self::new(String::new(), Vec::new())
    }
}

impl Pane {
    /// Create a new Pane with the given directory and files
    pub fn new(current_directory: String, files: Vec<String>) -> Self {
        let has_files = !files.is_empty();
        let files_clone = files.clone();
        let mut all_indexes: Vec<usize> = Vec::new();
        let mut file_labels: Vec<String> = Vec::new();

        for (index, file) in files.iter().enumerate() {
            let new_path = Path::new(file);
            let get_file_name = new_path
                .file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.to_string())
                .unwrap_or_else(|| file.clone());

            all_indexes.push(index);
            file_labels.push(get_file_name);
        }

        let mut list_state = ListState::default();
        if has_files {
            list_state.select(Some(0));
        }

        Pane {
            current_directory,
            files,
            read_only_files: files_clone,
            file_labels,
            selected_index: if has_files { Some(0) } else { None },
            filtered_indexes: all_indexes,
            search_results: Vec::new(),
            global_search_mode: false,
            search_input: String::new(),
            cursor_index: 0,
            list_state,
            show_hidden_files: false,
        }
    }

    /// Create a pane from a directory path
    pub fn from_directory(path: &str, show_hidden: bool) -> Self {
        let files = get_inner_files_info(
            path.to_string(),
            show_hidden,
            SortBy::Default,
            &SortType::ASC,
        )
        .ok()
        .flatten()
        .unwrap_or_default();

        Self::new(path.to_string(), files)
    }

    /// Update file references after the file list changes
    pub fn update_file_references(&mut self) {
        self.read_only_files = self.files.clone();
        self.file_labels.clear();
        self.filtered_indexes.clear();

        for (index, file) in self.files.iter().enumerate() {
            let new_path = Path::new(file);
            let get_file_name = new_path
                .file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.to_string())
                .unwrap_or_else(|| file.clone());

            self.filtered_indexes.push(index);
            self.file_labels.push(get_file_name);
        }

        // Reset selection if needed
        if self.files.is_empty() {
            self.selected_index = None;
            self.list_state.select(None);
        } else if let Some(idx) = self.selected_index {
            if idx >= self.files.len() {
                self.selected_index = Some(self.files.len() - 1);
                self.list_state.select(self.selected_index);
            }
        }
    }

    /// Get the currently selected file path
    pub fn get_selected_path(&self) -> Option<String> {
        self.selected_index.and_then(|idx| {
            if !self.search_results.is_empty() {
                // In search mode, use filtered indexes
                self.filtered_indexes.get(idx).and_then(|&real_idx| {
                    self.files.get(real_idx).cloned()
                })
            } else {
                self.files.get(idx).cloned()
            }
        })
    }

    /// Get the number of items in the list (considering search filter)
    pub fn get_list_length(&self) -> usize {
        if !self.search_results.is_empty() {
            self.search_results.len()
        } else {
            self.files.len()
        }
    }

    /// Navigate down in the list
    pub fn navigate_down(&mut self) {
        let list_len = self.get_list_length();
        if list_len == 0 {
            return;
        }

        let new_index = match self.selected_index {
            Some(i) => {
                if i >= list_len - 1 { 0 } else { i + 1 }
            }
            None => 0,
        };

        self.selected_index = Some(new_index);
        self.list_state.select(Some(new_index));
    }

    /// Navigate up in the list
    pub fn navigate_up(&mut self) {
        let list_len = self.get_list_length();
        if list_len == 0 {
            return;
        }

        let new_index = match self.selected_index {
            Some(i) => {
                if i == 0 { list_len - 1 } else { i - 1 }
            }
            None => 0,
        };

        self.selected_index = Some(new_index);
        self.list_state.select(Some(new_index));
    }

    /// Perform local fuzzy search on file names
    pub fn perform_local_search(&mut self, query: &str) {
        if query.is_empty() {
            self.search_results.clear();
            self.filtered_indexes = (0..self.files.len()).collect();
            return;
        }

        let matcher = SkimMatcherV2::default();
        let mut results: Vec<SearchResult> = Vec::new();

        for (index, file) in self.files.iter().enumerate() {
            let file_name = Path::new(file)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(file);

            if let Some(score) = matcher.fuzzy_match(file_name, query) {
                let is_directory = Path::new(file).is_dir();
                results.push(SearchResult {
                    file_path: file.clone(),
                    display_name: file_name.to_string(),
                    score,
                    is_directory,
                    original_index: index,
                });
            }
        }

        // Sort by score descending
        results.sort_by(|a, b| b.score.cmp(&a.score));

        // Update filtered indexes based on search results
        self.filtered_indexes = results.iter().map(|r| r.original_index).collect();
        self.search_results = results;

        // Reset selection to first result
        if !self.search_results.is_empty() {
            self.selected_index = Some(0);
            self.list_state.select(Some(0));
        }
    }

    /// Clear search state
    pub fn clear_search(&mut self) {
        self.search_input.clear();
        self.cursor_index = 0;
        self.search_results.clear();
        self.filtered_indexes = (0..self.files.len()).collect();
    }

    /// Check if this pane is in search mode
    pub fn is_searching(&self) -> bool {
        !self.search_input.is_empty() || !self.search_results.is_empty()
    }

    /// Refresh files from the current directory
    pub fn refresh_files(&mut self) {
        let new_files = get_inner_files_info(
            self.current_directory.clone(),
            self.show_hidden_files,
            SortBy::Default,
            &SortType::ASC,
        )
        .ok()
        .flatten()
        .unwrap_or_default();

        self.files = new_files;
        self.update_file_references();
    }

    /// Navigate to a new directory
    pub fn navigate_to_directory(&mut self, path: &str) {
        self.current_directory = path.to_string();
        self.clear_search();
        self.refresh_files();

        // Reset selection to first item
        if !self.files.is_empty() {
            self.selected_index = Some(0);
            self.list_state.select(Some(0));
        } else {
            self.selected_index = None;
            self.list_state.select(None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pane_new_empty() {
        let pane = Pane::new(String::new(), Vec::new());
        assert!(pane.files.is_empty());
        assert!(pane.selected_index.is_none());
        assert!(pane.filtered_indexes.is_empty());
    }

    #[test]
    fn test_pane_new_with_files() {
        let files = vec![
            "/home/user/file1.txt".to_string(),
            "/home/user/file2.rs".to_string(),
        ];
        let pane = Pane::new("/home/user".to_string(), files.clone());

        assert_eq!(pane.files.len(), 2);
        assert_eq!(pane.selected_index, Some(0));
        assert_eq!(pane.filtered_indexes, vec![0, 1]);
        assert_eq!(pane.file_labels, vec!["file1.txt", "file2.rs"]);
    }

    #[test]
    fn test_pane_navigate_down() {
        let files = vec![
            "/home/user/a.txt".to_string(),
            "/home/user/b.txt".to_string(),
            "/home/user/c.txt".to_string(),
        ];
        let mut pane = Pane::new("/home/user".to_string(), files);

        assert_eq!(pane.selected_index, Some(0));

        pane.navigate_down();
        assert_eq!(pane.selected_index, Some(1));

        pane.navigate_down();
        assert_eq!(pane.selected_index, Some(2));

        // Wrap around
        pane.navigate_down();
        assert_eq!(pane.selected_index, Some(0));
    }

    #[test]
    fn test_pane_navigate_up() {
        let files = vec![
            "/home/user/a.txt".to_string(),
            "/home/user/b.txt".to_string(),
            "/home/user/c.txt".to_string(),
        ];
        let mut pane = Pane::new("/home/user".to_string(), files);

        // Wrap around from 0
        pane.navigate_up();
        assert_eq!(pane.selected_index, Some(2));

        pane.navigate_up();
        assert_eq!(pane.selected_index, Some(1));
    }

    #[test]
    fn test_pane_get_selected_path() {
        let files = vec![
            "/home/user/a.txt".to_string(),
            "/home/user/b.txt".to_string(),
        ];
        let mut pane = Pane::new("/home/user".to_string(), files);

        assert_eq!(pane.get_selected_path(), Some("/home/user/a.txt".to_string()));

        pane.navigate_down();
        assert_eq!(pane.get_selected_path(), Some("/home/user/b.txt".to_string()));
    }

    #[test]
    fn test_pane_clear_search() {
        let files = vec!["/home/user/test.txt".to_string()];
        let mut pane = Pane::new("/home/user".to_string(), files);

        pane.search_input = "test".to_string();
        pane.cursor_index = 4;

        pane.clear_search();

        assert!(pane.search_input.is_empty());
        assert_eq!(pane.cursor_index, 0);
        assert!(pane.search_results.is_empty());
    }
}
