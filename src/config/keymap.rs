use std::collections::{BTreeMap, HashMap};

use crossterm::event::KeyCode;
use serde::{Deserialize, Serialize};

/// A Normal-mode command that `[keymap.normal]` can bind a key to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    NavDown,
    NavUp,
    NavTop,
    NavBottom,
    NavParent,
    NavEnter,
    NavChoose,
    SelectVisual,
    FileCreate,
    FileRename,
    FileDuplicate,
    FileCopyTo,
    FileMoveTo,
    FileTrash,
    FileDelete,
    FileExtract,
    FileCompress,
    FileOpen,
    FileInfo,
    CopyPath,
    CopyName,
    CopyParent,
    SearchCurrent,
    SearchRoot,
    ViewCycle,
    ViewSort,
    ViewHidden,
    PaneSwitch,
    Help,
    Palette,
    Quit,
}

impl Action {
    /// Every action with its name in settings.toml.
    pub const ALL: [(Self, &'static str); 31] = [
        (Self::NavDown, "nav.down"),
        (Self::NavUp, "nav.up"),
        (Self::NavTop, "nav.top"),
        (Self::NavBottom, "nav.bottom"),
        (Self::NavParent, "nav.parent"),
        (Self::NavEnter, "nav.enter"),
        (Self::NavChoose, "nav.choose"),
        (Self::SelectVisual, "select.visual"),
        (Self::FileCreate, "file.create"),
        (Self::FileRename, "file.rename"),
        (Self::FileDuplicate, "file.duplicate"),
        (Self::FileCopyTo, "file.copy_to"),
        (Self::FileMoveTo, "file.move_to"),
        (Self::FileTrash, "file.trash"),
        (Self::FileDelete, "file.delete"),
        (Self::FileExtract, "file.extract"),
        (Self::FileCompress, "file.compress"),
        (Self::FileOpen, "file.open"),
        (Self::FileInfo, "file.info"),
        (Self::CopyPath, "copy.path"),
        (Self::CopyName, "copy.name"),
        (Self::CopyParent, "copy.parent"),
        (Self::SearchCurrent, "search.current"),
        (Self::SearchRoot, "search.root"),
        (Self::ViewCycle, "view.cycle"),
        (Self::ViewSort, "view.sort"),
        (Self::ViewHidden, "view.hidden"),
        (Self::PaneSwitch, "pane.switch"),
        (Self::Help, "help"),
        (Self::Palette, "palette"),
        (Self::Quit, "quit"),
    ];

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .find(|(_, name)| *name == value)
            .map(|(action, _)| *action)
    }

    pub fn name(self) -> &'static str {
        Self::ALL
            .iter()
            .find(|(action, _)| *action == self)
            .map_or("", |(_, name)| name)
    }
}

/// A bound key in its canonical spelling (see [`parse_key`]): alone, after the leader,
/// or two keys in a row such as `gg`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum KeySequence {
    Single(String),
    Leader(String),
    Pair(String, String),
}

impl KeySequence {
    pub fn parse(value: &str) -> Option<Self> {
        match value.get(..8) {
            Some(prefix) if prefix.eq_ignore_ascii_case("<leader>") => {
                return Some(Self::Leader(parse_key(&value[8..])?));
            }
            _ => {}
        }
        if let Some(key) = parse_key(value) {
            return Some(Self::Single(key));
        }
        // Two keys: the first is one character or one `<…>` name.
        let split = if value.starts_with('<') {
            value.find('>')? + 1
        } else {
            value.chars().next()?.len_utf8()
        };
        Some(Self::Pair(
            parse_key(&value[..split])?,
            parse_key(&value[split..])?,
        ))
    }

    /// The keys pressed, not counting the leader.
    pub fn keys(&self) -> Vec<&str> {
        match self {
            Self::Single(key) | Self::Leader(key) => vec![key],
            Self::Pair(first, second) => vec![first, second],
        }
    }
}

/// Keys written `<Name>` in settings.toml, with how the help shows them.
pub const NAMED_KEYS: [(&str, KeyCode, &str); 12] = [
    ("Up", KeyCode::Up, "↑"),
    ("Down", KeyCode::Down, "↓"),
    ("Left", KeyCode::Left, "←"),
    ("Right", KeyCode::Right, "→"),
    ("Enter", KeyCode::Enter, "Enter"),
    ("Tab", KeyCode::Tab, "Tab"),
    ("Esc", KeyCode::Esc, "Esc"),
    ("Backspace", KeyCode::Backspace, "Backspace"),
    ("PageUp", KeyCode::PageUp, "PageUp"),
    ("PageDown", KeyCode::PageDown, "PageDown"),
    ("Home", KeyCode::Home, "Home"),
    ("End", KeyCode::End, "End"),
];

/// Ctrl-C always quits, so nothing can be bound to it.
const RESERVED_KEY: &str = "<C-c>";

/// The action name that removes a binding.
const UNBOUND: &str = "none";

/// One key in its canonical spelling: a character as itself (uppercase means Shift),
/// `<C-x>` for Ctrl+x and `<Name>` for a named key. Names are case-insensitive and
/// `<Space>` is `" "`. `None` when `value` isn't one key.
pub fn parse_key(value: &str) -> Option<String> {
    if value.chars().count() == 1 {
        return Some(value.to_string());
    }
    let inner = value.strip_prefix('<')?.strip_suffix('>')?;
    if inner.eq_ignore_ascii_case("space") {
        return Some(" ".to_string());
    }
    if let Some((name, ..)) = NAMED_KEYS
        .iter()
        .find(|(name, ..)| name.eq_ignore_ascii_case(inner))
    {
        return Some(format!("<{name}>"));
    }
    let ctrl = inner
        .strip_prefix("C-")
        .or_else(|| inner.strip_prefix("c-"))?;
    let mut chars = ctrl.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) => Some(ctrl_key(c)),
        _ => None,
    }
}

/// Canonical spelling of Ctrl+`c`; terminals send the same chord with or without Shift.
pub fn ctrl_key(c: char) -> String {
    format!("<C-{}>", c.to_ascii_lowercase())
}

/// How the help and messages show a canonical key: `Space`, `Ctrl-d`, `↓`, `Shift+D`.
pub fn key_label(key: &str) -> String {
    if key == " " {
        return "Space".to_string();
    }
    if let Some(c) = key
        .strip_prefix("<C-")
        .and_then(|rest| rest.strip_suffix('>'))
    {
        return format!("Ctrl-{c}");
    }
    let name = key
        .strip_prefix('<')
        .and_then(|rest| rest.strip_suffix('>'));
    if let Some((.., label)) = NAMED_KEYS.iter().find(|(named, ..)| name == Some(named)) {
        return label.to_string();
    }
    match key.chars().next() {
        Some(c) if c.is_ascii_uppercase() && key.len() == 1 => format!("Shift+{c}"),
        _ => key.to_string(),
    }
}

/// Parse one `[keymap.normal]` entry; no action means the key is unbound ("none").
/// `Err` says why the entry is ignored.
pub fn parse_binding(key: &str, action: &str) -> Result<(KeySequence, Option<Action>), String> {
    let sequence = KeySequence::parse(key).ok_or_else(|| format!("{key:?} (bad key)"))?;
    if sequence.keys().contains(&RESERVED_KEY) {
        return Err(format!("{key:?} (Ctrl-C always quits)"));
    }
    if action == UNBOUND {
        return Ok((sequence, None));
    }
    let action =
        Action::parse(action).ok_or_else(|| format!("{key:?} = {action:?} (unknown action)"))?;
    Ok((sequence, Some(action)))
}

/// The keymap in effect: settings.toml's valid entries over the defaults.
#[derive(Debug, Clone)]
pub struct Bindings {
    /// Canonical leader key.
    pub leader: String,
    pub normal: Vec<(KeySequence, Action)>,
    /// Entries left out, each with the reason.
    pub ignored: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KeymapSettings {
    #[serde(default = "default_keymap_profile")]
    pub profile: String,
    #[serde(default = "default_leader")]
    pub leader: String,
    #[serde(default = "default_normal_keymap")]
    pub normal: BTreeMap<String, String>,
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

impl KeymapSettings {
    /// Resolve the configured entries; a default stays bound unless its key is
    /// configured. An ignored entry acts as if it weren't written. A key that acts
    /// alone (or is the leader) can't also start a two-key sequence, so such a
    /// sequence is ignored, defaults included.
    pub fn resolve(&self) -> Bindings {
        let mut ignored = Vec::new();
        let mut normal: HashMap<KeySequence, Option<Action>> = HashMap::new();
        for (key, action) in &self.normal {
            match parse_binding(key, action) {
                Ok((sequence, action)) => {
                    normal.insert(sequence, action);
                }
                Err(reason) => ignored.push(reason),
            }
        }
        for (key, action) in DEFAULT_NORMAL_KEYMAP {
            let (sequence, action) = parse_binding(key, action).expect("valid default binding");
            normal.entry(sequence).or_insert(action);
        }
        let leader = match parse_key(&self.leader).filter(|key| key != RESERVED_KEY) {
            Some(leader) => leader,
            None => {
                ignored.push(format!("leader = {:?} (bad key)", self.leader));
                default_leader()
            }
        };
        // A key that acts alone, or the leader, never waits for a second key.
        let shadowed = |first: &String| {
            if *first == leader {
                Some("is the leader")
            } else {
                let alone = normal.get(&KeySequence::Single(first.clone()));
                matches!(alone, Some(Some(_))).then_some("is bound alone")
            }
        };
        let mut bound = Vec::new();
        let mut shadowed_pairs = Vec::new();
        for (sequence, action) in &normal {
            let Some(action) = action else {
                continue;
            };
            if let KeySequence::Pair(first, second) = sequence {
                if let Some(reason) = shadowed(first) {
                    let key = format!("{first}{second}");
                    let label = key_label(first);
                    shadowed_pairs
                        .push(format!("{key:?} = {:?} ({label} {reason})", action.name()));
                    continue;
                }
            }
            bound.push((sequence.clone(), *action));
        }
        shadowed_pairs.sort();
        ignored.extend(shadowed_pairs);
        Bindings {
            leader,
            normal: bound,
            ignored,
        }
    }
}

fn default_keymap_profile() -> String {
    "vim".to_string()
}

fn default_leader() -> String {
    " ".to_string()
}

/// Every default Normal-mode key; the README's "Keyboard Shortcuts" and keymap table
/// list them. The `copy.*` actions have none until the user binds one.
const DEFAULT_NORMAL_KEYMAP: [(&str, &str); 34] = [
    ("j", "nav.down"),
    ("<Down>", "nav.down"),
    ("k", "nav.up"),
    ("<Up>", "nav.up"),
    ("gg", "nav.top"),
    ("G", "nav.bottom"),
    ("h", "nav.parent"),
    ("<Left>", "nav.parent"),
    ("l", "nav.enter"),
    ("<Right>", "nav.enter"),
    ("<Enter>", "nav.choose"),
    ("V", "select.visual"),
    ("v", "select.visual"),
    ("a", "file.create"),
    ("r", "file.rename"),
    ("c", "file.duplicate"),
    ("C", "file.copy_to"),
    ("M", "file.move_to"),
    ("d", "file.trash"),
    ("D", "file.delete"),
    ("y", "file.extract"),
    ("z", "file.compress"),
    ("o", "file.open"),
    ("I", "file.info"),
    ("/", "search.current"),
    ("i", "search.current"),
    ("<leader>/", "search.root"),
    ("p", "view.cycle"),
    ("s", "view.sort"),
    (".", "view.hidden"),
    ("<Tab>", "pane.switch"),
    ("?", "help"),
    (":", "palette"),
    ("q", "quit"),
];

pub fn default_normal_keymap() -> BTreeMap<String, String> {
    DEFAULT_NORMAL_KEYMAP
        .iter()
        .map(|(key, action)| (key.to_string(), action.to_string()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_action_name_back_to_itself() {
        for (action, name) in Action::ALL {
            assert_eq!(Action::parse(name), Some(action));
            assert_eq!(action.name(), name);
        }
        assert_eq!(Action::parse("search.everywhere"), None);
        assert_eq!(Action::parse("none"), None);
    }

    #[test]
    fn parses_every_key_form_case_insensitively() {
        let single = |key: &str| Some(KeySequence::Single(key.to_string()));
        assert_eq!(KeySequence::parse("j"), single("j"));
        assert_eq!(KeySequence::parse("D"), single("D"));
        assert_eq!(KeySequence::parse("."), single("."));
        assert_eq!(KeySequence::parse("?"), single("?"));
        assert_eq!(KeySequence::parse("<"), single("<"));
        assert_eq!(KeySequence::parse("<C-d>"), single("<C-d>"));
        assert_eq!(KeySequence::parse("<c-D>"), single("<C-d>"));
        assert_eq!(KeySequence::parse("<Space>"), single(" "));
        assert_eq!(KeySequence::parse("<SPACE>"), single(" "));
        for (name, ..) in NAMED_KEYS {
            let canonical = single(&format!("<{name}>"));
            assert_eq!(KeySequence::parse(&format!("<{name}>")), canonical);
            let lower = format!("<{}>", name.to_lowercase());
            assert_eq!(KeySequence::parse(&lower), canonical);
        }
        assert_eq!(
            KeySequence::parse("<leader>/"),
            Some(KeySequence::Leader("/".to_string()))
        );
        assert_eq!(
            KeySequence::parse("<Leader><pageup>"),
            Some(KeySequence::Leader("<PageUp>".to_string()))
        );
        assert_eq!(
            KeySequence::parse("<LEADER><c-x>"),
            Some(KeySequence::Leader("<C-x>".to_string()))
        );
    }

    #[test]
    fn parses_two_key_sequences() {
        let pair = |first: &str, second: &str| {
            Some(KeySequence::Pair(first.to_string(), second.to_string()))
        };
        assert_eq!(KeySequence::parse("gg"), pair("g", "g"));
        assert_eq!(KeySequence::parse("jj"), pair("j", "j"));
        assert_eq!(KeySequence::parse("g<down>"), pair("g", "<Down>"));
        assert_eq!(KeySequence::parse("<C-W>j"), pair("<C-w>", "j"));
        assert_eq!(KeySequence::parse("<Space><Space>"), pair(" ", " "));
        assert_eq!(pair("g", "<Down>").unwrap().keys(), ["g", "<Down>"]);
    }

    #[test]
    fn rejects_bad_keys() {
        for bad in [
            "",
            "ggg",
            "<C-q",
            "<C->",
            "<C-ab>",
            "<A-x>",
            "<Foo>",
            "<>",
            "<leader>",
            "Down",
            "<leader>gg",
            "g<Foo>",
            "<Foo>g",
            "<<",
        ] {
            assert_eq!(KeySequence::parse(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn labels_keys_for_the_help() {
        assert_eq!(key_label(" "), "Space");
        assert_eq!(key_label("<C-d>"), "Ctrl-d");
        assert_eq!(key_label("<Down>"), "↓");
        assert_eq!(key_label("<PageUp>"), "PageUp");
        assert_eq!(key_label("D"), "Shift+D");
        assert_eq!(key_label("j"), "j");
        assert_eq!(key_label("<"), "<");
    }

    #[test]
    fn none_unbinds_a_default_key_and_bad_entries_are_reported() {
        let mut settings = KeymapSettings::default();
        settings.normal.insert("d".into(), "none".into());
        settings.normal.insert("<Down>".into(), "none".into());
        settings.normal.insert("x".into(), "file.trsh".into());
        settings.normal.insert("<C-q".into(), "quit".into());
        settings.normal.insert("<C-c>".into(), "nav.down".into());
        settings
            .normal
            .insert("<leader><c-C>".into(), "quit".into());
        settings.normal.insert("g<C-c>".into(), "quit".into());
        settings.leader = "ab".into();

        let bindings = settings.resolve();
        let bound = |key: &str| {
            let sequence = KeySequence::parse(key).unwrap();
            bindings
                .normal
                .iter()
                .find(|(bound, _)| *bound == sequence)
                .map(|(_, action)| *action)
        };
        assert_eq!(bound("d"), None);
        assert_eq!(bound("<Down>"), None);
        assert_eq!(bound("x"), None);
        assert_eq!(bound("j"), Some(Action::NavDown));
        assert_eq!(bound("D"), Some(Action::FileDelete));
        assert_eq!(bindings.leader, " ");
        assert_eq!(
            bindings.ignored,
            [
                r#""<C-c>" (Ctrl-C always quits)"#,
                r#""<C-q" (bad key)"#,
                r#""<leader><c-C>" (Ctrl-C always quits)"#,
                r#""g<C-c>" (Ctrl-C always quits)"#,
                r#""x" = "file.trsh" (unknown action)"#,
                r#"leader = "ab" (bad key)"#,
            ]
        );
    }

    #[test]
    fn an_entry_for_a_default_key_replaces_its_default_action() {
        let mut settings = KeymapSettings::default();
        settings.normal.clear();
        settings.normal.insert("<down>".into(), "nav.up".into());
        settings.normal.insert("x".into(), "file.trash".into());
        let bindings = settings.resolve();
        let keys_for = |action: Action| {
            let mut keys: Vec<_> = bindings
                .normal
                .iter()
                .filter(|(_, bound)| *bound == action)
                .map(|(sequence, _)| sequence.keys().concat())
                .collect();
            keys.sort();
            keys
        };
        assert_eq!(keys_for(Action::NavUp), ["<Down>", "<Up>", "k"]);
        assert_eq!(keys_for(Action::NavDown), ["j"]);
        assert_eq!(keys_for(Action::FileTrash), ["d", "x"]);
        assert!(bindings.ignored.is_empty());
    }

    fn bound_in(bindings: &Bindings, key: &str) -> Option<Action> {
        let sequence = KeySequence::parse(key).unwrap();
        bindings
            .normal
            .iter()
            .find(|(bound, _)| *bound == sequence)
            .map(|(_, action)| *action)
    }

    #[test]
    fn a_key_bound_alone_or_as_leader_ignores_the_sequences_it_starts() {
        let mut settings = KeymapSettings::default();
        settings.normal.insert("g".into(), "view.cycle".into());
        settings.normal.insert("gt".into(), "pane.switch".into());
        settings.normal.insert(",x".into(), "quit".into());
        settings
            .normal
            .insert("<C-w>j".into(), "pane.switch".into());
        settings.leader = ",".into();

        let bindings = settings.resolve();
        assert_eq!(bound_in(&bindings, "g"), Some(Action::ViewCycle));
        assert_eq!(bound_in(&bindings, "gg"), None);
        assert_eq!(bound_in(&bindings, "gt"), None);
        assert_eq!(bound_in(&bindings, ",x"), None);
        assert_eq!(bound_in(&bindings, "<C-w>j"), Some(Action::PaneSwitch));
        assert_eq!(
            bindings.ignored,
            [
                r#"",x" = "quit" (, is the leader)"#,
                r#""gg" = "nav.top" (g is bound alone)"#,
                r#""gt" = "pane.switch" (g is bound alone)"#,
            ]
        );

        // Unbinding the single key frees it to start sequences again.
        settings.normal.insert("g".into(), "none".into());
        let bindings = settings.resolve();
        assert_eq!(bound_in(&bindings, "g"), None);
        assert_eq!(bound_in(&bindings, "gg"), Some(Action::NavTop));
        assert_eq!(bound_in(&bindings, "gt"), Some(Action::PaneSwitch));
        assert_eq!(bindings.ignored, [r#"",x" = "quit" (, is the leader)"#]);
    }

    #[test]
    fn the_copy_actions_have_no_default_key_until_one_is_bound() {
        let unbound = |bindings: &Bindings| -> Vec<&str> {
            Action::ALL
                .iter()
                .filter(|(action, _)| !bindings.normal.iter().any(|(_, bound)| bound == action))
                .map(|(_, name)| *name)
                .collect()
        };
        let mut settings = KeymapSettings::default();
        assert_eq!(
            unbound(&settings.resolve()),
            ["copy.path", "copy.name", "copy.parent"]
        );

        settings
            .normal
            .insert("<leader>c".into(), "copy.path".into());
        settings.normal.insert("gy".into(), "copy.name".into());
        let bindings = settings.resolve();
        assert_eq!(bound_in(&bindings, "<leader>c"), Some(Action::CopyPath));
        assert_eq!(bound_in(&bindings, "gy"), Some(Action::CopyName));
        assert_eq!(bound_in(&bindings, "gg"), Some(Action::NavTop));
        assert_eq!(unbound(&bindings), ["copy.parent"]);
        assert!(bindings.ignored.is_empty());
    }

    #[test]
    fn readme_keymap_table_lists_every_action_with_its_default_keys() {
        let readme = include_str!("../../README.md");
        let rows: Vec<&str> = readme
            .lines()
            .filter(|line| {
                Action::ALL
                    .iter()
                    .any(|(_, name)| line.starts_with(&format!("| `{name}`")))
            })
            .collect();
        assert_eq!(rows.len(), Action::ALL.len(), "{rows:#?}");
        let row = |name: &str| {
            rows.iter()
                .find(|row| row.starts_with(&format!("| `{name}` ")))
                .unwrap_or_else(|| panic!("{name} has no README row"))
        };
        for (key, name) in DEFAULT_NORMAL_KEYMAP {
            assert!(row(name).contains(&format!("`{key}`")), "{name}: {key}");
        }
        // An action without a default key says so in its "Default keys" cell.
        for (_, name) in Action::ALL {
            if !DEFAULT_NORMAL_KEYMAP
                .iter()
                .any(|(_, bound)| *bound == name)
            {
                let default_keys = row(name).split('|').nth(2).unwrap_or_default();
                assert_eq!(default_keys.trim(), "none", "{name}");
            }
        }
    }
}
