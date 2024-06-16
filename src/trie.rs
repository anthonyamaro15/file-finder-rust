use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufReader, BufWriter};
use std::path::Path;
use walkdir::WalkDir;

#[derive(Serialize, Deserialize, Default, Clone)]
struct TrieNode {
    pub children: HashMap<char, TrieNode>,
    pub is_end_of_word: bool,
    pub directories: Vec<String>,
}

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct Trie {
    pub root: TrieNode,
}

impl Trie {
    pub fn insert(&mut self, path: &str) {
        let mut node = &mut self.root;

        for char in path.chars() {
            node = node.children.entry(char).or_insert_with(TrieNode::default);
        }

        node.is_end_of_word = true;
        node.directories.push(path.to_string());
    }

    pub fn search(&self, prefix: &str) -> Vec<String> {
        let mut node = &self.root;

        for char in prefix.chars() {
            if let Some(next_node) = node.children.get(&char) {
                node = next_node;
            } else {
                return Vec::new();
            }
        }

        self.collect_all_directories(node)
    }

    pub fn collect_all_directories(&self, node: &TrieNode) -> Vec<String> {
        let mut results = Vec::new();

        if node.is_end_of_word {
            results.extend(node.directories.clone());
        }
        for child in node.children.values() {
            results.extend(self.collect_all_directories(child))
        }
        results
    }
}

pub fn build_trie_from_directories(root_dir: &str) -> Trie {
    let mut trie = Trie::default();

    for entry in WalkDir::new(root_dir).min_depth(1).max_depth(1) {
        if let Ok(entry) = entry {
            if entry.file_type().is_dir() {
                trie.insert(entry.path().to_str().unwrap());
            }
        }
    }
    trie
}

pub fn save_trie_to_file(trie: &Trie, path: &str) -> io::Result<()> {
    let file = File::create(path)?;
    let writer = BufWriter::new(file);

    serde_json::to_writer(writer, trie)?;
    Ok(())
}

pub fn load_trie_from_file(path: &str) -> io::Result<Trie> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);

    let trie = serde_json::from_reader(reader)?;
    Ok(trie)
}

