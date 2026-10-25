//! Pane module for dual-pane file management.
//!
//! A Pane represents a single file listing view with its own directory,
//! file list, selection state, and search functionality.

use std::path::Path;

use log::debug;
use ratatui::widgets::ListState;

use crate::app::{global_search_results, local_search_results, SearchResult};
use crate::directory_store::DirectoryStore;
use crate::utils::files::{get_file_path_data, SortBy, SortType};
use crate::watcher::{FileSystemWatcher, WatcherEvent};

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
    /// Current selection index, a row of the visible list
    pub selected_index: Option<usize>,
    /// Rows shown instead of the listing while `search_input` filters it
    pub search_results: Vec<SearchResult>,
    /// Whether the search covers the folder index rather than this folder
    pub global_search_mode: bool,
    /// Query of the search filtering this pane; empty shows the whole listing
    pub search_input: String,
    /// ListState for ratatui rendering
    pub list_state: ListState,
    /// Whether to show hidden files
    pub show_hidden_files: bool,
    /// The app's sort, used for every read of the folder
    pub sort_by: SortBy,
    pub sort_type: SortType,
    /// Watches `current_directory` for changes made outside ff while dual pane is on
    pub watcher: Option<FileSystemWatcher>,
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
        let mut file_labels: Vec<String> = Vec::new();

        for file in files.iter() {
            let new_path = Path::new(file);
            let get_file_name = new_path
                .file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.to_string())
                .unwrap_or_else(|| file.clone());

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
            search_results: Vec::new(),
            global_search_mode: false,
            search_input: String::new(),
            list_state,
            show_hidden_files: false,
            sort_by: SortBy::Default,
            sort_type: SortType::ASC,
            watcher: None,
        }
    }

    /// Update file references after the file list changes
    pub fn update_file_references(&mut self) {
        self.read_only_files = self.files.clone();
        self.file_labels.clear();

        for file in self.files.iter() {
            let new_path = Path::new(file);
            let get_file_name = new_path
                .file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.to_string())
                .unwrap_or_else(|| file.clone());

            self.file_labels.push(get_file_name);
        }
    }

    /// Whether a search filters this pane, showing `search_results` instead of the listing
    pub fn is_searching(&self) -> bool {
        !self.search_input.is_empty()
    }

    /// Get the currently selected file path
    pub fn get_selected_path(&self) -> Option<String> {
        let index = self.selected_index?;
        if self.is_searching() {
            self.search_results
                .get(index)
                .map(|result| result.file_path.clone())
        } else {
            self.files.get(index).cloned()
        }
    }

    /// Get the number of items in the list (considering search filter)
    pub fn get_list_length(&self) -> usize {
        if self.is_searching() {
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

    /// Show the whole listing again, still highlighting the same item.
    pub fn clear_search(&mut self) {
        let selected = self.get_selected_path();
        self.search_input.clear();
        self.search_results.clear();
        self.global_search_mode = false;
        self.restore_selection(selected);
    }

    /// Filter the pane by `query` as the left listing's search does: by name in this
    /// folder, or across the folder index (`store`) for a global search. The top match
    /// is highlighted; an empty query shows the whole listing again.
    pub fn search(&mut self, query: &str, store: &DirectoryStore, max_results: usize) {
        let query = if self.global_search_mode {
            query.trim()
        } else {
            query
        };
        let selected = self.get_selected_path();
        self.search_input = query.to_string();
        if query.is_empty() {
            self.search_results.clear();
            self.restore_selection(selected);
            return;
        }
        self.search_results = if self.global_search_mode {
            global_search_results(store, query, max_results)
        } else {
            local_search_results(&self.files, query)
        };
        self.selected_index = (!self.search_results.is_empty()).then_some(0);
        self.list_state.select(self.selected_index);
    }

    /// Refresh files from the current directory
    pub fn refresh_files(&mut self) {
        let new_files = get_file_path_data(
            self.current_directory.clone(),
            self.show_hidden_files,
            self.sort_by.clone(),
            &self.sort_type,
        )
        .unwrap_or_default();

        let selected = self.get_selected_path();
        self.files = new_files;
        self.update_file_references();
        // A local search runs again on the new listing; global results don't come from it.
        if self.is_searching() && !self.global_search_mode {
            self.search_results = local_search_results(&self.files, &self.search_input);
        }
        self.restore_selection(selected);
    }

    /// Keep the highlight on `selected`; if it's gone, keep its row, clamped.
    fn restore_selection(&mut self, selected: Option<String>) {
        self.selected_index = self
            .get_list_length()
            .checked_sub(1)
            .map(|last_index| self.selected_index.unwrap_or(0).min(last_index));
        self.list_state.select(self.selected_index);
        if let Some(selected) = selected {
            self.select_path(&selected);
        }
    }

    /// Highlight `path` if it is a visible row; otherwise keep the current row.
    pub fn select_path(&mut self, path: &str) {
        let is_path = |file: &String| Path::new(file) == Path::new(path);
        let index = if self.is_searching() {
            self.search_results
                .iter()
                .position(|result| is_path(&result.file_path))
        } else {
            self.files.iter().position(is_path)
        };
        if let Some(index) = index {
            self.selected_index = Some(index);
            self.list_state.select(Some(index));
        }
    }

    /// Watch `current_directory` for changes made outside ff, replacing any earlier watch.
    pub fn watch_directory(&mut self) {
        let Ok(mut watcher) = FileSystemWatcher::new() else {
            return;
        };
        if let Err(e) = watcher.watch_directory(&self.current_directory) {
            debug!("Failed to watch '{}': {}", self.current_directory, e);
        }
        // Kept even if the folder can't be watched, so navigating elsewhere tries again.
        self.watcher = Some(watcher);
    }

    /// Re-read the folder, keeping the highlight, once its watcher reports a change.
    pub fn poll_watcher(&mut self) {
        let Some(watcher) = &mut self.watcher else {
            return;
        };
        let events = watcher.poll_events();
        if events.is_empty() {
            return;
        }
        if events
            .iter()
            .any(|event| matches!(event, WatcherEvent::WatcherError(_)))
        {
            watcher.stop_watching();
        }
        self.refresh_files();
    }

    /// Navigate to a new directory
    pub fn navigate_to_directory(&mut self, path: &str) -> anyhow::Result<()> {
        let files = get_file_path_data(
            path.to_string(),
            self.show_hidden_files,
            self.sort_by.clone(),
            &self.sort_type,
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
        if self.watcher.is_some() {
            self.watch_directory();
        }
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
    fn refresh_keeps_the_highlighted_item_when_its_row_moves() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["b.txt", "c.txt"] {
            std::fs::write(dir.path().join(name), name).unwrap();
        }
        let current = dir.path().to_string_lossy().into_owned();
        let on_disk =
            get_file_path_data(current.clone(), false, SortBy::Default, &SortType::ASC).unwrap();
        // A listing whose rows have since swapped: the highlighted first row is now last.
        let mut pane = Pane::new(current, on_disk.iter().rev().cloned().collect());
        let highlighted = pane.get_selected_path();

        pane.refresh_files();

        assert_eq!(pane.files, on_disk);
        assert_eq!(pane.get_selected_path(), highlighted);
        assert_eq!(pane.list_state.selected(), Some(1));
    }

    #[test]
    fn watcher_follows_navigation_and_refreshes_keeping_the_highlight() {
        let dir = tempfile::tempdir().unwrap();
        let child = dir.path().join("child");
        std::fs::create_dir(&child).unwrap();
        std::fs::write(child.join("b.txt"), "b").unwrap();
        let mut pane = Pane::new(dir.path().to_string_lossy().into_owned(), Vec::new());
        pane.sort_by = SortBy::Name;
        pane.watch_directory();
        pane.navigate_to_directory(child.to_str().unwrap()).unwrap();
        let highlighted = pane.get_selected_path();
        // Give the poll watcher a moment to read the folder first.
        std::thread::sleep(std::time::Duration::from_millis(100));

        let created = child.join("a.txt").to_string_lossy().into_owned();
        std::fs::write(&created, "a").unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !pane.files.contains(&created) && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(50));
            pane.poll_watcher();
        }

        assert!(pane.files.contains(&created));
        assert_eq!(pane.get_selected_path(), highlighted);
        assert_eq!(pane.list_state.selected(), Some(1));
    }

    #[test]
    fn test_pane_new_empty() {
        let pane = Pane::new(String::new(), Vec::new());
        assert!(pane.files.is_empty());
        assert!(pane.selected_index.is_none());
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

        pane.clear_search();

        assert!(pane.search_input.is_empty());
        assert!(pane.search_results.is_empty());
    }
}
