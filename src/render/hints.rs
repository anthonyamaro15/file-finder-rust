//! The hint row: the keys that matter for the current mode and the highlighted item,
//! taken from the live keymap.

use std::path::Path;

use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};
use unicode_width::UnicodeWidthStr;

use crate::{
    app::{App, InputMode},
    config::Action,
    event::keymap::KeymapRuntime,
    ui::truncate,
};

/// A key and what it does.
type Hint = (String, String);

/// Between hints.
const GAP: &str = "   ";

/// Widest the other pane's folder name gets in "copy to …".
const FOLDER_NAME_WIDTH: usize = 20;

/// The hint row, `width` columns wide: each key bold in the mode's color, then what it
/// does. Hints that don't fit are dropped from the end. In Normal mode the help key
/// stays at the right.
pub fn hint_line(
    app: &App,
    keymap: &KeymapRuntime,
    selected: Option<usize>,
    width: u16,
) -> Line<'static> {
    let key_style = Style::default()
        .fg(app.mode_color())
        .add_modifier(Modifier::BOLD);
    let label_style = Style::default().fg(app.theme_colors.light_gray);
    let spans = |(key, label): Hint| {
        [
            Span::styled(key, key_style),
            Span::styled(format!(" {label}"), label_style),
        ]
    };
    let mut right = Vec::new();
    if app.input_mode == InputMode::Normal {
        if let Some(key) = keymap.hint_key(Action::Help) {
            right.extend(spans((key, "all keys".to_string())));
            right.push(Span::raw(" "));
        }
    }
    let right_width: usize = right.iter().map(Span::width).sum();

    // The hints get what the help key leaves, keeping a gap.
    let room = usize::from(width).saturating_sub(right_width + 2);
    let mut left = vec![Span::raw(" ")];
    let mut used = 1;
    for (i, (key, label)) in hints(app, keymap, selected).into_iter().enumerate() {
        let gap = if i == 0 { 0 } else { GAP.len() };
        let hint_width = gap + key.width() + 1 + label.width();
        if used + hint_width > room {
            break;
        }
        used += hint_width;
        if i > 0 {
            left.push(Span::raw(GAP));
        }
        left.extend(spans((key, label)));
    }
    let pad = usize::from(width).saturating_sub(used + right_width);
    left.push(Span::raw(" ".repeat(pad)));
    left.extend(right);
    Line::from(left)
}

/// The hints for the current mode and the active pane's highlighted item, most useful
/// first. Keys a keymap can rebind come from it, and an action without a key is left
/// out; keys in popups and text fields are fixed.
fn hints(app: &App, keymap: &KeymapRuntime, selected: Option<usize>) -> Vec<Hint> {
    let fixed = |key: &str, label: &str| (key.to_string(), label.to_string());
    let (query, global) = app.search_box();
    match app.input_mode {
        InputMode::Normal => normal_hints(app, keymap, selected),
        InputMode::Visual => {
            let count = app.visual.as_ref().map_or(0, |visual| visual.paths.len());
            let extend: Vec<String> = [Action::NavDown, Action::NavUp]
                .into_iter()
                .filter_map(|action| keymap.hint_key(action))
                .collect();
            let mut hints = Vec::new();
            if !extend.is_empty() {
                hints.push((extend.join("/"), "extend".to_string()));
            }
            hints.push(fixed("o", "swap ends"));
            hints.extend(bound(
                keymap,
                [
                    (Action::FileTrash, format!("trash {count}")),
                    (Action::FileDelete, format!("delete {count}")),
                ],
            ));
            hints.push(fixed("Esc", "cancel"));
            hints
        }
        InputMode::Editing => {
            let esc = if query.is_empty() {
                fixed("Esc", "cancel")
            } else {
                fixed("Esc", "browse the results")
            };
            if global {
                vec![esc, fixed("⌫", "in an empty box: search this folder")]
            } else {
                vec![
                    esc,
                    fixed("←/→", "move the cursor"),
                    fixed("␣", "in an empty box: search all folders"),
                ]
            }
        }
        InputMode::WatchTrash => vec![fixed("y", "move to Trash"), fixed("n or Esc", "cancel")],
        InputMode::WatchDelete => vec![fixed("y", "delete"), fixed("n or Esc", "cancel")],
        InputMode::WatchCreate => vec![
            fixed("⏎", "create"),
            fixed("Esc", "cancel"),
            fixed("/", "at the end makes a folder"),
        ],
        InputMode::WatchRename => vec![fixed("⏎", "rename"), fixed("Esc", "cancel")],
        InputMode::WatchSort => vec![
            fixed("n/s/t", "by name/size/date"),
            fixed("a/d", "order for the next sort"),
            fixed("q or Esc", "close"),
        ],
        InputMode::WatchKeyBinding => vec![
            fixed("j/k", "scroll"),
            fixed("PageUp/PageDown", "page"),
            fixed("Esc", "close"),
        ],
        InputMode::WatchInfo => vec![fixed("Esc", "close")],
        InputMode::PickFolder => {
            let verb = app
                .folder_picker
                .as_ref()
                .map_or("", |picker| picker.kind.title());
            vec![
                fixed("↑/↓", "choose"),
                fixed("⏎", &format!("{} here", verb.to_lowercase())),
                fixed("Esc", "cancel"),
            ]
        }
        InputMode::Palette => vec![
            fixed("↑/↓", "choose"),
            fixed("⏎", "run"),
            fixed("Esc", "close"),
        ],
        InputMode::CacheLoading => vec![fixed("Ctrl-C", "quit")],
    }
}

/// Normal mode's hints: dual pane's, a search's results', or the highlighted folder's
/// or file's, or an empty folder's.
fn normal_hints(app: &App, keymap: &KeymapRuntime, selected: Option<usize>) -> Vec<Hint> {
    let item = app.active_selected_info(selected);
    let is_dir = item.is_some_and(|info| info.is_dir());
    let (query, global) = app.search_box();

    if app.view_mode.is_dual_pane() {
        let other = if app.is_right_pane_active() {
            app.left_directory_path()
        } else {
            app.right_pane.current_directory.clone().into()
        };
        let name = other
            .file_name()
            .map_or(other.to_string_lossy(), |name| name.to_string_lossy());
        let name = truncate(&name, FOLDER_NAME_WIDTH);
        let mut actions = vec![(Action::PaneSwitch, "switch pane".to_string())];
        if item.is_some() {
            actions.push((Action::FileCopyTo, format!("copy to {name}")));
            actions.push((Action::FileMoveTo, format!("move to {name}")));
        }
        actions.push((Action::ViewCycle, "next view".to_string()));
        return bound(keymap, actions);
    }

    let choose = (Action::NavChoose, choose_label(app));
    if global || !query.is_empty() {
        // A new search, left at once, shows the whole folder again.
        let full_listing = keymap
            .hint_key(Action::SearchCurrent)
            .map(|key| (format!("{key} Esc"), "full listing".to_string()));
        let open = match item {
            Some(_) if is_dir => Some((Action::NavEnter, "open".to_string())),
            Some(_) => Some(choose),
            None => None,
        };
        let mut hints = bound(keymap, open);
        hints.extend(full_listing);
        hints.extend(bound(
            keymap,
            [
                (Action::FileRename, "rename".to_string()),
                (Action::FileTrash, "trash".to_string()),
            ],
        ));
        return hints;
    }

    let actions: Vec<(Action, &str)> = match item {
        None => vec![
            (Action::NavParent, "up"),
            (Action::FileCreate, "new"),
            (Action::SearchRoot, "all folders"),
            (
                Action::ViewHidden,
                if app.show_hidden_files {
                    "hide hidden"
                } else {
                    "show hidden"
                },
            ),
            (Action::ViewCycle, "next view"),
        ],
        Some(_) if is_dir => vec![
            (Action::NavEnter, "open"),
            (Action::NavParent, "up"),
            (Action::SearchCurrent, "find"),
            (Action::SearchRoot, "all folders"),
            (Action::SelectVisual, "select"),
            (Action::FileCreate, "new inside"),
            (Action::FileRename, "rename"),
            (Action::FileTrash, "trash"),
        ],
        Some(_) => {
            let path = app.active_selected_path(selected).unwrap_or_default();
            let zip = Path::new(&path)
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("zip"));
            let mut actions = vec![(choose.0, choose.1.as_str())];
            if zip {
                actions.push((Action::FileExtract, "extract"));
            }
            actions.extend([
                (Action::FileOpen, "open with app"),
                (Action::FileDuplicate, "duplicate"),
                (Action::FileCopyTo, "copy to"),
                (Action::FileMoveTo, "move to"),
                (Action::FileCompress, "zip"),
                (Action::FileInfo, "info"),
            ]);
            actions
        }
    };
    bound(keymap, actions)
}

/// What Enter does with the highlighted item: ff quits and opens it in the editor, or
/// copies its path (the shell integration copies a file's).
fn choose_label(app: &App) -> String {
    match app.get_selected_ide() {
        Some(editor) if !app.print_path => format!("open in {editor}"),
        _ => "copy path".to_string(),
    }
}

/// Each action's first key with its label; actions without a key are left out.
fn bound<L: Into<String>>(
    keymap: &KeymapRuntime,
    actions: impl IntoIterator<Item = (Action, L)>,
) -> Vec<Hint> {
    actions
        .into_iter()
        .filter_map(|(action, label)| Some((keymap.hint_key(action)?, label.into())))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        app::{SearchScope, ViewMode, VisualSelection, IDE},
        config::KeymapSettings,
        utils::files::{EntryInfo, EntryKind, Listing},
    };

    /// `/w` listing `names`; a name ending in `/` is a folder. The first is highlighted.
    fn app(names: &[&str]) -> App {
        let listing = Listing {
            files: names
                .iter()
                .map(|name| format!("/w/{}", name.trim_end_matches('/')))
                .collect(),
            info: names
                .iter()
                .map(|name| EntryInfo {
                    kind: Some(if name.ends_with('/') {
                        EntryKind::Folder
                    } else {
                        EntryKind::File
                    }),
                    ..EntryInfo::default()
                })
                .collect(),
        };
        let mut app = App::with_listing(listing);
        app.current_directory = "/w".into();
        app
    }

    fn keymap(entries: &[(&str, &str)]) -> KeymapRuntime {
        let mut settings = KeymapSettings::default();
        for (key, action) in entries {
            settings.normal.insert(key.to_string(), action.to_string());
        }
        KeymapRuntime::from_settings(&settings)
    }

    /// Each hint as "key label".
    fn shown(app: &App, keymap: &KeymapRuntime) -> Vec<String> {
        hints(app, keymap, Some(0))
            .into_iter()
            .map(|(key, label)| format!("{key} {label}"))
            .collect()
    }

    fn text(line: &Line) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    }

    const FOLDER: [&str; 8] = [
        "l open",
        "h up",
        "/ find",
        "␣/ all folders",
        "V select",
        "a new inside",
        "r rename",
        "d trash",
    ];

    #[test]
    fn a_folder_a_file_and_a_zip_get_their_own_keys() {
        let defaults = keymap(&[]);
        assert_eq!(shown(&app(&["src/"]), &defaults), FOLDER);

        let file = [
            "⏎ copy path",
            "o open with app",
            "c duplicate",
            "C copy to",
            "M move to",
            "z zip",
            "I info",
        ];
        assert_eq!(shown(&app(&["notes.md"]), &defaults), file);

        // A zip can be extracted; Enter opens it in the editor once one is set.
        let mut zip = app(&["photos.ZIP"]);
        zip.selected_id = Some(IDE::NVIM);
        let hints = shown(&zip, &defaults);
        assert_eq!(
            hints[..3],
            ["⏎ open in nvim", "y extract", "o open with app"]
        );
        assert_eq!(hints.len(), 8);
    }

    #[test]
    fn an_empty_folder_offers_what_works_without_an_item() {
        let mut empty = app(&[]);
        let hints = [
            "h up",
            "a new",
            "␣/ all folders",
            ". show hidden",
            "p next view",
        ];
        assert_eq!(shown(&empty, &keymap(&[])), hints);
        empty.show_hidden_files = true;
        assert_eq!(shown(&empty, &keymap(&[]))[3], ". hide hidden");
    }

    #[test]
    fn dual_pane_names_the_other_panes_folder() {
        let mut app = app(&["src/"]);
        app.view_mode = ViewMode::DualPane;
        app.right_pane.current_directory = "/w/archive".into();
        assert_eq!(
            shown(&app, &keymap(&[])),
            [
                "Tab switch pane",
                "C copy to archive",
                "M move to archive",
                "p next view"
            ]
        );
    }

    #[test]
    fn visual_mode_counts_the_selection() {
        let names = ["a/", "b/", "c/", "d/", "e/", "f/", "g"];
        let mut app = app(&names);
        app.input_mode = InputMode::Visual;
        app.visual = Some(VisualSelection {
            anchor: "/w/a".into(),
            cursor: "/w/g".into(),
            paths: app.files.clone(),
        });
        assert_eq!(
            shown(&app, &keymap(&[])),
            [
                "j/k extend",
                "o swap ends",
                "d trash 7",
                "D delete 7",
                "Esc cancel"
            ]
        );
    }

    #[test]
    fn typing_a_search_names_its_own_keys() {
        let mut app = app(&["src/"]);
        app.input_mode = InputMode::Editing;
        assert_eq!(
            shown(&app, &keymap(&[])),
            [
                "Esc cancel",
                "←/→ move the cursor",
                "␣ in an empty box: search all folders"
            ]
        );

        app.input = "serv".into();
        assert_eq!(shown(&app, &keymap(&[]))[0], "Esc browse the results");

        app.search_scope = SearchScope::Root;
        app.global_search_mode = true;
        assert_eq!(
            shown(&app, &keymap(&[])),
            [
                "Esc browse the results",
                "⌫ in an empty box: search this folder"
            ]
        );

        // Browsing the results afterwards.
        app.input_mode = InputMode::Normal;
        assert_eq!(
            shown(&app, &keymap(&[])),
            ["l open", "/ Esc full listing", "r rename", "d trash"]
        );
    }

    #[test]
    fn prompts_and_popups_name_their_fixed_keys() {
        let mut app = app(&["src/"]);
        app.input_mode = InputMode::WatchTrash;
        assert_eq!(
            shown(&app, &keymap(&[])),
            ["y move to Trash", "n or Esc cancel"]
        );
        app.input_mode = InputMode::WatchSort;
        assert_eq!(
            shown(&app, &keymap(&[])),
            [
                "n/s/t by name/size/date",
                "a/d order for the next sort",
                "q or Esc close"
            ]
        );
        // `?` would type into a field, so only Normal mode names it.
        let line = hint_line(&app, &keymap(&[]), Some(0), 100);
        assert!(!text(&line).contains("all keys"), "{}", text(&line));
    }

    #[test]
    fn a_rebound_key_shows_up_and_an_unbound_action_is_left_out() {
        let rebound = keymap(&[("x", "file.trash"), ("d", "none"), ("r", "none")]);
        let hints = shown(&app(&["src/"]), &rebound);
        assert_eq!(hints.last().map(String::as_str), Some("x trash"));
        assert!(
            !hints.iter().any(|hint| hint.ends_with("rename")),
            "{hints:?}"
        );
    }

    #[test]
    fn hints_drop_from_the_end_to_fit_and_the_help_key_stays() {
        let app = app(&["src/"]);
        let line = hint_line(&app, &keymap(&[]), Some(0), 100);
        let wide = text(&line);
        assert_eq!(wide.width(), 100);
        assert_eq!(wide, format!(" {}  ? all keys ", FOLDER.join("   ")));
        // Each key bold in the mode's color, its label muted.
        let theme = &app.theme_colors;
        let key = Style::default()
            .fg(theme.mode_normal)
            .add_modifier(Modifier::BOLD);
        assert_eq!(line.spans[1].style, key);
        assert_eq!(line.spans[2].style, Style::default().fg(theme.light_gray));

        let narrow = text(&hint_line(&app, &keymap(&[]), Some(0), 40));
        assert_eq!(narrow.width(), 40);
        assert!(narrow.starts_with(" l open   h up   / find   "), "{narrow}");
        assert!(!narrow.contains("trash"), "{narrow}");
        assert!(narrow.ends_with("? all keys "), "{narrow}");
    }
}
