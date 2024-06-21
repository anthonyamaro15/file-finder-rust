use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{self, BufReader, BufWriter};
use walkdir::WalkDir;

#[derive(Serialize, Deserialize, Default, Clone)]
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

pub fn build_directory_from_store(
    root_dir: &str,
    ignore_directories: Vec<String>,
) -> DirectoryStore {
    let mut store = DirectoryStore::new();

    for entry in WalkDir::new(root_dir).min_depth(1) {
        if let Ok(entry) = entry {
            if entry.file_type().is_dir() {
                let path = entry.path().to_string_lossy();
                let mut should_ignore = false;

                if ignore_directories.len() > 0 {
                    for ignore in ignore_directories.iter() {
                        let t = ignore.to_owned();
                        let update_type = t.as_str();
                        if path.contains(update_type) {
                            should_ignore = true;
                            break;
                        }
                    }
                }

                if !should_ignore {
                    //TODO:should we display All file path dir/dir2/Desktop/  OR
                    // ../../Desktop OR
                    // Desktop
                    store.insert(entry.path().to_str().unwrap());
                }
            }
        }
    }
    store
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
