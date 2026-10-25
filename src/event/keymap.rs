use std::collections::HashMap;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::{
    config::{
        keymap::{ctrl_key, key_label, NAMED_KEYS},
        Action, KeySequence, KeymapSettings,
    },
    render::configured_action_key_labels,
};

#[derive(Debug, Clone)]
pub struct KeymapRuntime {
    leader: String,
    normal: HashMap<KeySequence, Action>,
    pending_leader: bool,
    /// First key of a two-key sequence, waiting for the second.
    pending_key: Option<String>,
    ignored: Vec<String>,
    /// Shown for a key that does nothing while selecting, naming the keys that act.
    selection_hint: String,
}

impl KeymapRuntime {
    pub fn from_settings(settings: &KeymapSettings) -> Self {
        let bindings = settings.resolve();
        let keys: Vec<String> = [Action::FileTrash, Action::FileDelete]
            .into_iter()
            .filter_map(|action| {
                configured_action_key_labels(settings, action)
                    .into_iter()
                    .next()
            })
            .collect();
        let selection_hint = if keys.is_empty() {
            "Esc to cancel the selection".to_string()
        } else {
            format!(
                "Only {} can act on a selection · Esc to cancel",
                keys.join(" and ")
            )
        };

        Self {
            leader: bindings.leader,
            normal: bindings.normal.into_iter().collect(),
            pending_leader: false,
            pending_key: None,
            ignored: bindings.ignored,
            selection_hint,
        }
    }

    pub fn selection_hint(&self) -> &str {
        &self.selection_hint
    }

    /// Whether the last key started a sequence (the leader or a two-key one), so the
    /// next key goes to it.
    pub fn is_pending(&self) -> bool {
        self.pending_leader || self.pending_key.is_some()
    }

    /// The startup warning naming the settings.toml entries that were left out.
    pub fn warning(&self) -> Option<String> {
        (!self.ignored.is_empty())
            .then(|| format!("Ignored keymap entries: {}", self.ignored.join(", ")))
    }

    pub fn resolve_normal(&mut self, key: KeyEvent) -> Option<Action> {
        let pending_leader = std::mem::take(&mut self.pending_leader);
        let pending_key = self.pending_key.take();
        let key_string = key_to_string(key)?;

        if let Some(first) = pending_key {
            // As in Neovim, a key that doesn't finish the sequence is dropped.
            return self
                .normal
                .get(&KeySequence::Pair(first, key_string))
                .copied();
        }
        if pending_leader {
            let sequence = KeySequence::Leader(key_string.clone());
            // A key with no leader binding still does what it does alone.
            if let Some(action) = self.normal.get(&sequence) {
                return Some(*action);
            }
        } else if key_string == self.leader {
            self.pending_leader = true;
            return None;
        }
        let starts_pair = self
            .normal
            .keys()
            .any(|bound| matches!(bound, KeySequence::Pair(first, _) if *first == key_string));
        if starts_pair {
            self.pending_key = Some(key_string);
            return None;
        }

        self.normal.get(&KeySequence::Single(key_string)).copied()
    }

    /// Whether Normal mode uses `key` alone, in a sequence or as the leader, so the
    /// modifier guard lets a configured Ctrl chord through.
    pub fn binds(&self, key: KeyEvent) -> bool {
        let Some(key) = key_to_string(key) else {
            return false;
        };
        key == self.leader
            || self
                .normal
                .keys()
                .any(|bound| bound.keys().contains(&&*key))
    }
}

/// How `key` reads in the help and in "press … again" warnings.
pub fn pressed_key_label(key: KeyEvent) -> String {
    key_to_string(key).map_or_else(String::new, |key| key_label(&key))
}

/// The canonical spelling of a pressed key (see `config::keymap::parse_key`). Shift
/// is in the character itself, so only Ctrl is added.
fn key_to_string(key: KeyEvent) -> Option<String> {
    match key.code {
        KeyCode::Char(c) if key.modifiers.contains(KeyModifiers::CONTROL) => Some(ctrl_key(c)),
        KeyCode::Char(c) => Some(c.to_string()),
        code => NAMED_KEYS
            .iter()
            .find(|(_, named, _)| *named == code)
            .map(|(name, ..)| format!("<{name}>")),
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::*;

    fn key(c: char) -> KeyEvent {
        let modifiers = if c.is_ascii_uppercase() {
            KeyModifiers::SHIFT
        } else {
            KeyModifiers::NONE
        };
        KeyEvent::new(KeyCode::Char(c), modifiers)
    }

    fn runtime(entries: &[(&str, &str)]) -> KeymapRuntime {
        let mut settings = KeymapSettings::default();
        for (key, action) in entries {
            settings.normal.insert(key.to_string(), action.to_string());
        }
        KeymapRuntime::from_settings(&settings)
    }

    #[test]
    fn default_keymap_binds_every_key_ff_has_always_had() {
        let mut runtime = KeymapRuntime::from_settings(&KeymapSettings::default());
        let named = |code| KeyEvent::new(code, KeyModifiers::NONE);
        let table = [
            (key('j'), Action::NavDown),
            (named(KeyCode::Down), Action::NavDown),
            (key('k'), Action::NavUp),
            (named(KeyCode::Up), Action::NavUp),
            (key('h'), Action::NavParent),
            (named(KeyCode::Left), Action::NavParent),
            (key('l'), Action::NavEnter),
            (named(KeyCode::Right), Action::NavEnter),
            (named(KeyCode::Enter), Action::NavChoose),
            (key('G'), Action::NavBottom),
            (key('V'), Action::SelectVisual),
            (key('v'), Action::SelectVisual),
            (key('a'), Action::FileCreate),
            (key('r'), Action::FileRename),
            (key('c'), Action::FileDuplicate),
            (key('C'), Action::FileCopyTo),
            (key('M'), Action::FileMoveTo),
            (key('d'), Action::FileTrash),
            (key('D'), Action::FileDelete),
            (key('y'), Action::FileExtract),
            (key('z'), Action::FileCompress),
            (key('o'), Action::FileOpen),
            (key('I'), Action::FileInfo),
            (key('i'), Action::SearchCurrent),
            (key('/'), Action::SearchCurrent),
            (key('p'), Action::ViewCycle),
            (key('s'), Action::ViewSort),
            (key('.'), Action::ViewHidden),
            (named(KeyCode::Tab), Action::PaneSwitch),
            (key('?'), Action::Help),
            (key('q'), Action::Quit),
        ];
        for (pressed, action) in table {
            assert_eq!(runtime.resolve_normal(pressed), Some(action), "{pressed:?}");
        }
        assert_eq!(runtime.resolve_normal(key(' ')), None);
        assert_eq!(runtime.resolve_normal(key('/')), Some(Action::SearchRoot));
        assert_eq!(runtime.resolve_normal(key('g')), None);
        assert_eq!(runtime.resolve_normal(key('g')), Some(Action::NavTop));
        // Nothing else is bound, and every action has a key.
        assert_eq!(runtime.normal.len(), table.len() + 2);
        for (action, name) in Action::ALL {
            assert!(runtime.normal.values().any(|a| *a == action), "{name}");
        }
        assert!(runtime.warning().is_none());
    }

    #[test]
    fn resolves_leader_sequence_action() {
        let mut runtime = KeymapRuntime::from_settings(&KeymapSettings::default());

        assert_eq!(runtime.resolve_normal(key(' ')), None);
        assert_eq!(runtime.resolve_normal(key('/')), Some(Action::SearchRoot));
    }

    #[test]
    fn unbound_key_after_leader_clears_pending_state() {
        let mut runtime = KeymapRuntime::from_settings(&KeymapSettings::default());

        assert_eq!(runtime.resolve_normal(key(' ')), None);
        assert_eq!(runtime.resolve_normal(key('x')), None);
        assert_eq!(
            runtime.resolve_normal(key('/')),
            Some(Action::SearchCurrent)
        );
        // A key without a leader binding does what it does alone.
        assert_eq!(runtime.resolve_normal(key(' ')), None);
        assert_eq!(runtime.resolve_normal(key('j')), Some(Action::NavDown));
    }

    #[test]
    fn rebinding_moves_the_action_and_frees_the_old_key() {
        let mut runtime = runtime(&[("x", "file.trash"), ("d", "nav.down"), ("D", "none")]);

        assert_eq!(runtime.resolve_normal(key('x')), Some(Action::FileTrash));
        assert_eq!(runtime.resolve_normal(key('d')), Some(Action::NavDown));
        assert_eq!(runtime.resolve_normal(key('D')), None);
        assert_eq!(runtime.resolve_normal(key('j')), Some(Action::NavDown));
    }

    #[test]
    fn ctrl_chords_resolve_only_when_configured() {
        let mut runtime = runtime(&[("<C-d>", "nav.down"), ("<leader><C-u>", "nav.up")]);
        let ctrl = |c| KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL);

        assert!(runtime.binds(ctrl('d')) && runtime.binds(ctrl('u')));
        assert!(!runtime.binds(ctrl('j')) && !runtime.binds(ctrl('x')));
        assert_eq!(runtime.resolve_normal(ctrl('d')), Some(Action::NavDown));
        // Ctrl+j is never plain j.
        assert_eq!(runtime.resolve_normal(ctrl('j')), None);
        assert_eq!(runtime.resolve_normal(key(' ')), None);
        assert_eq!(runtime.resolve_normal(ctrl('u')), Some(Action::NavUp));
        assert_eq!(pressed_key_label(ctrl('d')), "Ctrl-d");
        assert_eq!(
            pressed_key_label(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            "Enter"
        );
    }

    #[test]
    fn invalid_entries_become_one_startup_warning() {
        let runtime = runtime(&[("x", "file.trsh"), ("<C-q", "quit"), ("<C-c>", "nav.down")]);

        assert_eq!(
            runtime.warning().as_deref(),
            Some(
                r#"Ignored keymap entries: "<C-c>" (Ctrl-C always quits), "<C-q" (bad key), "x" = "file.trsh" (unknown action)"#
            )
        );
        assert!(runtime
            .normal
            .contains_key(&KeySequence::Single("d".into())));
    }

    #[test]
    fn a_two_key_sequence_waits_for_its_second_key_and_drops_a_wrong_one() {
        let mut runtime = KeymapRuntime::from_settings(&KeymapSettings::default());
        let esc = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);

        assert_eq!(runtime.resolve_normal(key('g')), None);
        assert!(runtime.is_pending());
        assert_eq!(runtime.resolve_normal(key('g')), Some(Action::NavTop));
        assert!(!runtime.is_pending());
        // A key that doesn't finish it is consumed, as in Neovim.
        for wrong in [key('j'), key('x'), key('G'), esc] {
            assert_eq!(runtime.resolve_normal(key('g')), None);
            assert_eq!(runtime.resolve_normal(wrong), None, "{wrong:?}");
            assert!(!runtime.is_pending());
        }
        assert_eq!(runtime.resolve_normal(key('j')), Some(Action::NavDown));
        assert_eq!(runtime.resolve_normal(key('G')), Some(Action::NavBottom));
        // After the leader, a key without a leader binding can start a sequence.
        assert_eq!(runtime.resolve_normal(key(' ')), None);
        assert_eq!(runtime.resolve_normal(key('g')), None);
        assert_eq!(runtime.resolve_normal(key('g')), Some(Action::NavTop));
    }

    #[test]
    fn a_key_bound_alone_acts_at_once_and_its_sequences_are_reported() {
        let mut runtime = runtime(&[("g", "view.cycle")]);

        assert_eq!(runtime.resolve_normal(key('g')), Some(Action::ViewCycle));
        assert!(!runtime.is_pending());
        assert_eq!(runtime.resolve_normal(key('g')), Some(Action::ViewCycle));
        assert_eq!(
            runtime.warning().as_deref(),
            Some(r#"Ignored keymap entries: "gg" = "nav.top" (g is bound alone)"#)
        );
    }

    #[test]
    fn a_sequence_can_start_with_a_ctrl_chord() {
        let mut runtime = runtime(&[("<C-w>j", "pane.switch")]);
        let ctrl_w = KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL);

        assert!(runtime.binds(ctrl_w));
        assert_eq!(runtime.resolve_normal(ctrl_w), None);
        assert_eq!(runtime.resolve_normal(key('j')), Some(Action::PaneSwitch));
        assert_eq!(runtime.resolve_normal(key('j')), Some(Action::NavDown));
    }

    #[test]
    fn the_selection_hint_names_the_live_trash_and_delete_keys() {
        let hint = |entries: &[(&str, &str)]| runtime(entries).selection_hint().to_string();

        assert_eq!(
            hint(&[]),
            "Only d and Shift+D can act on a selection · Esc to cancel"
        );
        assert_eq!(
            hint(&[("d", "none"), ("x", "file.trash")]),
            "Only x and Shift+D can act on a selection · Esc to cancel"
        );
        assert_eq!(
            hint(&[("d", "none"), ("D", "none")]),
            "Esc to cancel the selection"
        );
    }
}
