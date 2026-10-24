//! Pane module for dual-pane file management.
//!
//! A Pane represents a single file listing view with its own directory,
//! file list, selection state, and search functionality.

use std::path::Path;

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
                self.filtered_indexes
                    .get(idx)
                    .and_then(|&real_idx| self.files.get(real_idx).cloned())
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
                if i >= list_len - 1 {
                    0
                } else {
                    i + 1
                }
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
                if i == 0 {
                    list_len - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };

        self.selected_index = Some(new_index);
        self.list_state.select(Some(new_index));
    }

    /// Clear search state
    pub fn clear_search(&mut self) {
        self.search_input.clear();
        self.cursor_index = 0;
        self.search_results.clear();
        self.filtered_indexes = (0..self.files.len()).collect();
    }

    /// Refresh files from the current directory
    pub fn refresh_files(&mut self) {
        let new_files = get_inner_files_info(
            self.current_directory.clone(),
            self.show_hidden_files,
            SortBy::Default,
            &SortType::ASC,
        )
        .unwrap_or_default();

        self.files = new_files;
        self.update_file_references();
    }

    /// Navigate to a new directory
    pub fn navigate_to_directory(&mut self, path: &str) -> anyhow::Result<()> {
        let files = get_inner_files_info(
            path.to_string(),
            self.show_hidden_files,
            SortBy::Default,
            &SortType::ASC,
        )?;
        // Going to the parent selects the directory just left; otherwise use the first row.
        let selected_index = files
            .iter()
            .position(|file| file == &self.current_directory)
            .or_else(|| (!files.is_empty()).then_some(0));

        self.current_directory = path.to_string();
        self.clear_search();
        self.files = files;
        self.update_file_references();

        self.selected_index = selected_index;
        self.list_state.select(selected_index);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn navigation_failure_preserves_directory_listing_and_selection() {
        let dir = tempfile::tempdir().unwrap();
        let current = dir.path().to_string_lossy().into_owned();
        let files = vec![dir.path().join("file.txt").to_string_lossy().into_owned()];
        let mut pane = Pane::new(current.clone(), files.clone());
        pane.search_input = "file".into();

        let result = pane.navigate_to_directory(dir.path().join("missing").to_str().unwrap());

        assert!(result.is_err());
        assert_eq!(pane.current_directory, current);
        assert_eq!(pane.files, files);
        assert_eq!(pane.read_only_files, files);
        assert_eq!(pane.selected_index, Some(0));
        assert_eq!(pane.list_state.selected(), Some(0));
        assert_eq!(pane.search_input, "file");
    }

    #[test]
    fn navigating_to_parent_selects_the_directory_just_left() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("alpha")).unwrap();
        std::fs::create_dir(dir.path().join("zulu")).unwrap();
        let parent = dir.path().to_string_lossy().into_owned();
        let files = crate::utils::files::get_file_path_data(
            parent.clone(),
            false,
            SortBy::Default,
            &SortType::ASC,
        )
        .unwrap();
        // Choose a directory that isn't the first row, regardless of filesystem order.
        let previous = files[1].clone();
        let mut pane = Pane::new(previous.clone(), Vec::new());

        pane.navigate_to_directory(&parent).unwrap();

        assert_eq!(pane.current_directory, parent);
        assert_eq!(pane.get_selected_path(), Some(previous));
        assert_eq!(pane.list_state.selected(), pane.selected_index);
    }

    #[test]
    fn navigating_into_directory_selects_first_row_or_none_when_empty() {
        let dir = tempfile::tempdir().unwrap();
        let child = dir.path().join("child");
        std::fs::create_dir(&child).unwrap();
        let mut pane = Pane::new(dir.path().to_string_lossy().into_owned(), Vec::new());

        pane.navigate_to_directory(child.to_str().unwrap()).unwrap();
        assert_eq!(pane.selected_index, None);
        assert_eq!(pane.list_state.selected(), None);

        std::fs::write(child.join("file.txt"), "content").unwrap();
        pane.navigate_to_directory(child.to_str().unwrap()).unwrap();
        assert_eq!(pane.selected_index, Some(0));
        assert_eq!(pane.list_state.selected(), Some(0));
        assert_eq!(pane.read_only_files, pane.files);
    }

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

        assert_eq!(
            pane.get_selected_path(),
            Some("/home/user/a.txt".to_string())
        );

        pane.navigate_down();
        assert_eq!(
            pane.get_selected_path(),
            Some("/home/user/b.txt".to_string())
        );
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
