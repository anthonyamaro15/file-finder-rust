use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;

use notify::{
    Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Result as NotifyResult, Watcher,
};
use log::{debug, error};

/// Events that the file watcher can send to the main application
#[derive(Debug, Clone)]
pub enum WatcherEvent {
    /// Files were created in the watched directory
    FilesCreated(Vec<PathBuf>),
    /// Files were deleted from the watched directory
    FilesDeleted(Vec<PathBuf>),
    /// Files were modified in the watched directory
    FilesModified(Vec<PathBuf>),
    /// Files were renamed in the watched directory
    FilesRenamed { from: Vec<PathBuf>, to: Vec<PathBuf> },
    /// Directory structure changed significantly - trigger full refresh
    DirectoryChanged,
    /// Watcher encountered an error
    WatcherError(String),
}

/// File system watcher that monitors a directory for changes
#[derive(Debug)]
pub struct FileSystemWatcher {
    watcher: Option<RecommendedWatcher>,
    event_receiver: Receiver<WatcherEvent>,
    _event_sender: Sender<WatcherEvent>, // Keep sender alive
}

impl FileSystemWatcher {
    /// Create a new file system watcher
    pub fn new() -> Result<Self, String> {
        let (event_sender, event_receiver) = mpsc::channel();
        
        Ok(FileSystemWatcher {
            watcher: None,
            event_receiver,
            _event_sender: event_sender,
        })
    }
    
    /// Start watching a directory for changes
    pub fn watch_directory<P: AsRef<Path>>(&mut self, path: P) -> Result<(), String> {
        let path = path.as_ref().to_path_buf();
        let (tx, rx) = mpsc::channel();
        let event_sender = self._event_sender.clone();
        
        // Create the watcher
        let mut watcher = RecommendedWatcher::new(
            move |res: NotifyResult<Event>| {
                if let Err(e) = tx.send(res) {
                    error!("Failed to send watcher event: {}", e);
                }
            },
            Config::default(),
        ).map_err(|e| format!("Failed to create watcher: {}", e))?;
        
        // Start watching the directory
        watcher
            .watch(&path, RecursiveMode::NonRecursive)
            .map_err(|e| format!("Failed to watch directory {}: {}", path.display(), e))?;
        
        debug!("Started watching directory: {}", path.display());
        
        // Store the watcher
        self.watcher = Some(watcher);
        
        // Spawn a thread to process file system events
        let path_clone = path.clone();
        thread::spawn(move || {
            Self::process_events(rx, event_sender, path_clone);
        });
        
        Ok(())
    }
    
    /// Stop watching the current directory
    pub fn stop_watching(&mut self) {
        if let Some(_watcher) = self.watcher.take() {
            // The watcher will automatically stop when dropped
            debug!("Stopped file system watcher");
        }
    }
    
    /// Check for new events from the watcher (non-blocking)
    pub fn poll_events(&self) -> Vec<WatcherEvent> {
        let mut events = Vec::new();
        
        // Collect all available events without blocking
        while let Ok(event) = self.event_receiver.try_recv() {
            events.push(event);
        }
        
        events
    }
    
    /// Process raw file system events and convert them to application events
    fn process_events(
        receiver: Receiver<NotifyResult<Event>>, 
        event_sender: Sender<WatcherEvent>,
        watched_path: PathBuf
    ) {
        debug!("Started processing file system events for: {}", watched_path.display());
        
        // Buffer events to avoid too frequent updates
        let mut event_buffer: Vec<Event> = Vec::new();
        let mut last_flush = std::time::Instant::now();
        let flush_interval = Duration::from_millis(200); // Flush events every 200ms
        
        loop {
            // Try to receive events with a timeout
            match receiver.recv_timeout(flush_interval) {
                Ok(Ok(event)) => {
                    // Filter events to only include files in the watched directory
                    if Self::event_in_directory(&event, &watched_path) {
                        event_buffer.push(event);
                    }
                },
                Ok(Err(e)) => {
                    error!("File watcher error: {}", e);
                    if let Err(send_err) = event_sender.send(WatcherEvent::WatcherError(e.to_string())) {
                        error!("Failed to send watcher error: {}", send_err);
                        break;
                    }
                },
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    // Timeout - flush any buffered events
                },
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    debug!("Event receiver disconnected, stopping event processing");
                    break;
                }
            }
            
            // Flush buffered events periodically or when buffer is large
            let now = std::time::Instant::now();
            if (!event_buffer.is_empty() && now.duration_since(last_flush) >= flush_interval) 
                || event_buffer.len() >= 50 {
                
                if let Some(processed_event) = Self::process_event_batch(&event_buffer, &watched_path) {
                    if let Err(e) = event_sender.send(processed_event) {
                        error!("Failed to send processed event: {}", e);
                        break;
                    }
                }
                
                event_buffer.clear();
                last_flush = now;
            }
        }
        
        debug!("Stopped processing file system events");
    }
    
    /// Check if an event occurred within the watched directory (not subdirectories)
    fn event_in_directory(event: &Event, watched_path: &Path) -> bool {
        for path in &event.paths {
            if let Some(parent) = path.parent() {
                if parent == watched_path {
                    return true;
                }
            }
            // Also include direct events on the watched path itself
            if path == watched_path {
                return true;
            }
        }
        false
    }
    
    /// Process a batch of events and return a single application event
    fn process_event_batch(events: &[Event], watched_path: &Path) -> Option<WatcherEvent> {
        let mut created_files = Vec::new();
        let mut deleted_files = Vec::new();
        let mut modified_files = Vec::new();
        let renamed_from = Vec::new();
        let renamed_to = Vec::new();
        let mut directory_changed = false;
        
        for event in events {
            match &event.kind {
                EventKind::Create(_) => {
                    for path in &event.paths {
                        if let Some(parent) = path.parent() {
                            if parent == watched_path && path.is_file() {
                                created_files.push(path.clone());
                                debug!("File created: {}", path.display());
                            } else if parent == watched_path && path.is_dir() {
                                directory_changed = true;
                            }
                        }
                    }
                },
                EventKind::Remove(_) => {
                    for path in &event.paths {
                        // For removed files, we can't check is_file() since they don't exist
                        // So we assume any removal in the watched directory is significant
                        if let Some(parent) = path.parent() {
                            if parent == watched_path {
                                deleted_files.push(path.clone());
                                debug!("File deleted: {}", path.display());
                            }
                        }
                    }
                },
                EventKind::Modify(_) => {
                    for path in &event.paths {
                        if let Some(parent) = path.parent() {
                            if parent == watched_path && path.is_file() {
                                modified_files.push(path.clone());
                                debug!("File modified: {}", path.display());
                            }
                        }
                    }
                },
                EventKind::Access(_) => {
                    // Ignore access events as they're too frequent and not useful for our purposes
                },
                EventKind::Other => {
                    // Some filesystems generate generic "other" events for various changes
                    directory_changed = true;
                },
                _ => {
                    // Handle any other event types as potential directory changes
                    directory_changed = true;
                }
            }
        }
        
        // Prioritize specific file events over generic directory changes
        if !created_files.is_empty() {
            Some(WatcherEvent::FilesCreated(created_files))
        } else if !deleted_files.is_empty() {
            Some(WatcherEvent::FilesDeleted(deleted_files))
        } else if !modified_files.is_empty() {
            Some(WatcherEvent::FilesModified(modified_files))
        } else if !renamed_from.is_empty() || !renamed_to.is_empty() {
            Some(WatcherEvent::FilesRenamed { 
                from: renamed_from, 
                to: renamed_to 
            })
        } else if directory_changed {
            Some(WatcherEvent::DirectoryChanged)
        } else {
            None
        }
    }
}

impl Drop for FileSystemWatcher {
    fn drop(&mut self) {
        self.stop_watching();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;
    
    #[test]
    fn test_watcher_creation() {
        let watcher = FileSystemWatcher::new();
        assert!(watcher.is_ok());
    }
    
    #[test]
    fn test_watch_directory() {
        let dir = tempdir().unwrap();
        let mut watcher = FileSystemWatcher::new().unwrap();
        
        let result = watcher.watch_directory(dir.path());
        assert!(result.is_ok());
    }
    
    #[test]
    fn test_file_creation_detection() {
        let dir = tempdir().unwrap();
        let mut watcher = FileSystemWatcher::new().unwrap();
        
        watcher.watch_directory(dir.path()).unwrap();
        
        // Give the watcher a moment to start
        std::thread::sleep(Duration::from_millis(100));
        
        // Create a file
        let test_file = dir.path().join("test.txt");
        fs::write(&test_file, "test content").unwrap();
        
        // Give the watcher time to detect the change
        std::thread::sleep(Duration::from_millis(300));
        
        // Check for events
        let events = watcher.poll_events();
        
        // We should receive at least one event
        assert!(!events.is_empty());
        
        // Check if any event is about file creation
        let has_creation_event = events.iter().any(|event| {
            matches!(event, WatcherEvent::FilesCreated(_))
        });
        
        assert!(has_creation_event || events.iter().any(|event| {
            matches!(event, WatcherEvent::DirectoryChanged)
        }));
    }
}
