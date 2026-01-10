use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{self, BufReader, BufWriter};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    mpsc, Arc, Mutex,
};
use std::thread;
use ignore::WalkBuilder;
use walkdir::WalkDir;

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct DirectoryStore {
    pub directories: Vec<String>,
}

impl DirectoryStore {
    pub fn new() -> Self {
        DirectoryStore {
            directories: Vec::new(),
        }
    }

    pub fn insert(&mut self, path: &str) {
        self.directories.push(path.to_string());
    }

    pub fn search(&self, prefix: &str) -> Vec<String> {
        let mut new_files: Vec<String> = Vec::new();
        for file in self.directories.iter() {
            if file.contains(&prefix) {
                new_files.push(file.clone());
            }
        }
        new_files
    }
}

/// Legacy implementation using walkdir for benchmarking comparison
pub fn build_directory_from_store_walkdir(
    root_dir: &str,
    ignore_directories: Vec<String>,
) -> DirectoryStore {
    let mut store = DirectoryStore::new();

    for entry in WalkDir::new(root_dir).min_depth(1) {
        if let Ok(entry) = entry {
            if entry.file_type().is_dir() {
                let path = entry.path().to_string_lossy();
                let mut should_ignore = false;

                if !ignore_directories.is_empty() {
                    for ignore in ignore_directories.iter() {
                        if path.contains(ignore) {
                            should_ignore = true;
                            break;
                        }
                    }
                }

                if !should_ignore {
                    // Skip paths that aren't valid UTF-8
                    if let Some(path_str) = entry.path().to_str() {
                        store.insert(path_str);
                    }
                }
            }
        }
    }
    store
}

/// New implementation using ignore::WalkBuilder (gitignore-aware, early pruning)
pub fn build_directory_from_store_ignore(
    root_dir: &str,
    ignore_directories: Vec<String>,
) -> DirectoryStore {
    let mut store = DirectoryStore::new();

    let ignore_lower: Vec<String> = ignore_directories
        .into_iter()
        .map(|s| s.to_lowercase())
        .collect();

    let mut builder = WalkBuilder::new(root_dir);
    builder
        .hidden(true) // ignore hidden files/dirs like .git
        .follow_links(false)
        .git_ignore(true)
        .git_exclude(true)
        .parents(true)
        .ignore(true)
        .max_depth(None);

    // Early pruning based on configured ignore substrings
    let ignore_lower_clone = ignore_lower.clone();
    builder.filter_entry(move |e| {
        let p = e.path().to_string_lossy().to_lowercase();
        !ignore_lower_clone.iter().any(|ig| p.contains(ig))
    });

    for result in builder.build() {
        match result {
            Ok(entry) => {
                if entry.depth() == 0 {
                    continue; // skip the root itself
                }
                if entry
                    .file_type()
                    .map(|ft| ft.is_dir())
                    .unwrap_or(false)
                {
                    if let Some(s) = entry.path().to_str() {
                        store.insert(s);
                    }
                }
            }
            Err(err) => {
                eprintln!("Error processing directory entry: {}", err);
            }
        }
    }

    store
}

/// Default function now delegates to the ignore-based implementation
pub fn build_directory_from_store(
    root_dir: &str,
    ignore_directories: Vec<String>,
) -> DirectoryStore {
    build_directory_from_store_ignore(root_dir, ignore_directories)
}

pub fn save_directory_to_file(store: &DirectoryStore, path: &str) -> io::Result<()> {
    let file = File::create(path)?;
    let writer = BufWriter::new(file);

    serde_json::to_writer(writer, store)?;
    Ok(())
}

pub fn load_directory_from_file(path: &str) -> io::Result<DirectoryStore> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);

    let trie = serde_json::from_reader(reader)?;
    Ok(trie)
}

// Progress message for directory cache building
#[derive(Debug, Clone)]
pub enum CacheBuildProgress {
    Progress {
        directories_found: usize,
        current_path: String,
    },
    Completed {
        store: DirectoryStore,
    },
    Error(String),
}

// Async directory building with progress tracking (ignore::WalkBuilder, parallel)
pub fn build_directory_from_store_async(
    root_dir: String,
    ignore_directories: Vec<String>,
) -> mpsc::Receiver<CacheBuildProgress> {
    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        let store = Arc::new(Mutex::new(DirectoryStore::new()));
        const UPDATE_INTERVAL: usize = 100; // Update progress every 100 directories
        let counter = Arc::new(AtomicUsize::new(0));

        let ignore_lower: Vec<String> = ignore_directories
            .into_iter()
            .map(|s| s.to_lowercase())
            .collect();

        let mut builder = WalkBuilder::new(&root_dir);
        builder
            .hidden(true)
            .follow_links(false)
            .git_ignore(true)
            .git_exclude(true)
            .parents(true)
            .ignore(true)
            .max_depth(None)
            .threads(num_cpus::get());

        let ignore_lower_arc = Arc::new(ignore_lower);
        let ignore_filter = ignore_lower_arc.clone();
        builder.filter_entry(move |e| {
            let p = e.path().to_string_lossy().to_lowercase();
            !ignore_filter.iter().any(|ig| p.contains(ig))
        });

        let walker = builder.build_parallel();
        let tx_progress = tx.clone();
        let store_arc = store.clone();
        let counter_arc = counter.clone();

        walker.run(|| {
            let tx_progress = tx_progress.clone();
            let store_arc = store_arc.clone();
            let counter_arc = counter_arc.clone();
            Box::new(move |res| {
                match res {
                    Ok(entry) => {
                        if entry.depth() == 0 {
                            return ignore::WalkState::Continue;
                        }
                        if entry
                            .file_type()
                            .map(|ft| ft.is_dir())
                            .unwrap_or(false)
                        {
                            if let Some(s) = entry.path().to_str() {
                                {
                                    let mut guard = store_arc.lock().unwrap();
                                    guard.insert(s);
                                }
                                let new_count = counter_arc.fetch_add(1, Ordering::Relaxed) + 1;
                                if new_count % UPDATE_INTERVAL == 0 {
                                    let _ = tx_progress.send(CacheBuildProgress::Progress {
                                        directories_found: new_count,
                                        current_path: s.to_string(),
                                    });
                                }
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Error processing directory entry: {}", e);
                    }
                }
                ignore::WalkState::Continue
            })
        });

        // Final progress update
        let final_count = counter.load(Ordering::Relaxed);
        let _ = tx.send(CacheBuildProgress::Progress {
            directories_found: final_count,
            current_path: "Finalizing...".to_string(),
        });

        // Send completion message without risking a panic if Arc still has clones
        let store_cloned = {
            let guard = store.lock().unwrap();
            guard.clone()
        };
        let _ = tx.send(CacheBuildProgress::Completed { store: store_cloned });
    });

    rx
}
