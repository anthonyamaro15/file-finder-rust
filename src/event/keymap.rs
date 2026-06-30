use std::collections::HashMap;

use crossterm::event::{KeyCode, KeyEvent};

use crate::config::{Action, KeySequence, KeymapSettings};

#[derive(Debug, Clone)]
pub struct KeymapRuntime {
    leader: String,
    normal: HashMap<KeySequence, Action>,
    pending_leader: bool,
}

impl KeymapRuntime {
    pub fn from_settings(settings: &KeymapSettings) -> Self {
        let normal = settings
            .normal
            .iter()
            .filter_map(|(key, action)| Some((KeySequence::parse(key)?, Action::parse(action)?)))
            .collect();

        Self {
            leader: settings.leader.clone(),
            normal,
            pending_leader: false,
        }
    }

    pub fn resolve_normal(&mut self, key: KeyEvent) -> Option<Action> {
        let key_string = key_to_string(key.code)?;

        if self.pending_leader {
            self.pending_leader = false;
            return self.normal.get(&KeySequence::Leader(key_string)).cloned();
        }

        if key_string == self.leader {
            self.pending_leader = true;
            return None;
        }

        self.normal.get(&KeySequence::Single(key_string)).cloned()
    }
}

fn key_to_string(key_code: KeyCode) -> Option<String> {
    match key_code {
        KeyCode::Char(c) => Some(c.to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::*;

    fn key(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
    }

    #[test]
    fn resolves_single_key_action() {
        let mut runtime = KeymapRuntime::from_settings(&KeymapSettings::default());

        assert_eq!(
            runtime.resolve_normal(key('/')),
            Some(Action::SearchCurrent)
        );
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
    }
}
