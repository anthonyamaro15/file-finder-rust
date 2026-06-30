use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    SearchCurrent,
    SearchRoot,
}

impl Action {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "search.current" => Some(Self::SearchCurrent),
            "search.root" => Some(Self::SearchRoot),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum KeySequence {
    Single(String),
    Leader(String),
}

impl KeySequence {
    pub fn parse(value: &str) -> Option<Self> {
        if value.is_empty() {
            return None;
        }

        if let Some(rest) = value.strip_prefix("<leader>") {
            if rest.is_empty() {
                return None;
            }
            return Some(Self::Leader(rest.to_string()));
        }

        Some(Self::Single(value.to_string()))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KeymapSettings {
    #[serde(default = "default_keymap_profile")]
    pub profile: String,
    #[serde(default = "default_leader")]
    pub leader: String,
    #[serde(default = "default_normal_keymap")]
    pub normal: HashMap<String, String>,
}

impl Default for KeymapSettings {
    fn default() -> Self {
        Self {
            profile: default_keymap_profile(),
            leader: default_leader(),
            normal: default_normal_keymap(),
        }
    }
}

fn default_keymap_profile() -> String {
    "vim".to_string()
}

fn default_leader() -> String {
    " ".to_string()
}

pub fn default_normal_keymap() -> HashMap<String, String> {
    HashMap::from([
        ("/".to_string(), "search.current".to_string()),
        ("<leader>/".to_string(), "search.root".to_string()),
        ("i".to_string(), "search.current".to_string()),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_search_current_action() {
        assert_eq!(Action::parse("search.current"), Some(Action::SearchCurrent));
    }

    #[test]
    fn parses_search_root_action() {
        assert_eq!(Action::parse("search.root"), Some(Action::SearchRoot));
    }

    #[test]
    fn rejects_unknown_action() {
        assert_eq!(Action::parse("search.everywhere"), None);
    }

    #[test]
    fn parses_single_key_sequence() {
        assert_eq!(
            KeySequence::parse("/"),
            Some(KeySequence::Single("/".to_string()))
        );
    }

    #[test]
    fn parses_leader_key_sequence() {
        assert_eq!(
            KeySequence::parse("<leader>/"),
            Some(KeySequence::Leader("/".to_string()))
        );
    }

    #[test]
    fn rejects_empty_key_sequence() {
        assert_eq!(KeySequence::parse(""), None);
    }

    #[test]
    fn default_keymap_contains_vim_search_bindings() {
        let settings = KeymapSettings::default();
        assert_eq!(
            settings.normal.get("/"),
            Some(&"search.current".to_string())
        );
        assert_eq!(
            settings.normal.get("<leader>/"),
            Some(&"search.root".to_string())
        );
        assert_eq!(
            settings.normal.get("i"),
            Some(&"search.current".to_string())
        );
    }
}
