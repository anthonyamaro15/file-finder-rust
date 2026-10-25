//! The command palette: type to filter the actions by what they do, and Enter runs
//! the highlighted one just as its key does in Normal mode.

use std::io;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{backend::CrosstermBackend, Terminal};

use crate::{
    app::InputMode,
    config::Action,
    event::{
        keymap::KeymapRuntime,
        navigation::{next_index_wrapping, previous_index_wrapping},
        normal::run_action,
        LoopControl, LoopState,
    },
    render::popups::palette_actions,
};

/// An action matching the palette's filter.
#[derive(Debug)]
pub struct PaletteMatch {
    pub action: Action,
    pub description: &'static str,
    pub group: &'static str,
    /// Characters of the description that matched, to underline.
    pub positions: Vec<usize>,
}

/// Rank of a match by letters in order but apart, shown only when nothing matches the
/// query as a run of letters.
const SCATTERED: u8 = 4;

/// The palette's actions matching `query`, ignoring case, best first: descriptions
/// starting with the query, then with a later word starting with it, then holding it
/// inside a word, then setting names holding it (`file.trash`), and only if none of
/// those, descriptions holding its letters in order. Equal matches keep the help's
/// order.
pub fn palette_matches(query: &str) -> Vec<PaletteMatch> {
    let query = lowercase_chars(query);
    let mut matches: Vec<(u8, PaletteMatch)> = palette_actions()
        .into_iter()
        .filter_map(|(action, description, group)| {
            let (rank, positions) = match_description(&query, &lowercase_chars(description))
                .or_else(|| {
                    let name = lowercase_chars(action.name());
                    name.windows(query.len())
                        .any(|window| window == query)
                        .then(|| (3, Vec::new()))
                })?;
            let found = PaletteMatch {
                action,
                description,
                group,
                positions,
            };
            Some((rank, found))
        })
        .collect();
    if matches.iter().any(|(rank, _)| *rank < SCATTERED) {
        matches.retain(|(rank, _)| *rank < SCATTERED);
    }
    matches.sort_by_key(|(rank, _)| *rank);
    matches.into_iter().map(|(_, found)| found).collect()
}

/// How `query` matches `text`, both lowercase: 0 at its start, 1 at the start of a
/// later word, 2 inside a word, [`SCATTERED`] as letters in order but apart; with the
/// characters that matched.
fn match_description(query: &[char], text: &[char]) -> Option<(u8, Vec<usize>)> {
    if query.is_empty() {
        return Some((0, Vec::new()));
    }
    let run = |start: usize| (start..start + query.len()).collect();
    let starts: Vec<usize> = (0..=text.len().saturating_sub(query.len()))
        .filter(|&start| text[start..].starts_with(query))
        .collect();
    let word_start = |&&start: &&usize| start == 0 || !text[start - 1].is_alphanumeric();
    if let Some(&start) = starts.iter().find(word_start) {
        return Some((if start == 0 { 0 } else { 1 }, run(start)));
    }
    if let Some(&start) = starts.first() {
        return Some((2, run(start)));
    }
    let mut positions = Vec::new();
    let mut rest = text.iter().enumerate();
    for wanted in query {
        let (i, _) = rest.find(|(_, c)| *c == wanted)?;
        positions.push(i);
    }
    Some((SCATTERED, positions))
}

/// `text` lowercased one character for one, so positions stay those of `text`.
fn lowercase_chars(text: &str) -> Vec<char> {
    text.chars()
        .map(|c| c.to_lowercase().next().unwrap_or(c))
        .collect()
}

/// Handle a key in the palette: typing and Backspace edit the filter, ↑/↓ choose
/// (wrapping around), Esc closes it, and Enter closes it and runs the highlighted
/// action. Ctrl-C was already handled; it quits.
pub fn handle_palette_key(
    key: KeyEvent,
    loop_state: &mut LoopState,
    keymap_runtime: &KeymapRuntime,
    terminal: &mut Terminal<CrosstermBackend<impl io::Write>>,
    fallback_start_path: &str,
) -> io::Result<LoopControl> {
    let app = &mut loop_state.app;
    let palette = &mut app.palette;
    let matches = palette_matches(&palette.query);
    match key.code {
        KeyCode::Esc => app.input_mode = InputMode::Normal,
        KeyCode::Up => {
            palette.selected =
                previous_index_wrapping(Some(palette.selected), matches.len()).unwrap_or(0)
        }
        KeyCode::Down => {
            palette.selected =
                next_index_wrapping(Some(palette.selected), matches.len()).unwrap_or(0)
        }
        KeyCode::Backspace => {
            palette.query.pop();
            palette.selected = 0;
        }
        KeyCode::Char(c) => {
            palette.query.push(c);
            palette.selected = 0;
        }
        KeyCode::Enter => {
            if let Some(found) = matches.get(palette.selected) {
                app.input_mode = InputMode::Normal;
                let key_label = keymap_runtime.key_for(found.action);
                return run_action(
                    found.action,
                    &key_label,
                    loop_state,
                    terminal,
                    fallback_start_path,
                );
            }
        }
        _ => {}
    }
    Ok(LoopControl::Continue)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(query: &str) -> Vec<(&'static str, String)> {
        palette_matches(query)
            .into_iter()
            .map(|found| {
                let underlined: String = found
                    .description
                    .chars()
                    .enumerate()
                    .map(|(i, c)| if found.positions.contains(&i) { c } else { '_' })
                    .collect();
                (found.action.name(), underlined)
            })
            .collect()
    }

    #[test]
    fn the_palette_lists_every_action_but_the_motions_in_the_helps_order_and_groups() {
        let all = palette_matches("");
        let names: Vec<&str> = all.iter().map(|found| found.action.name()).collect();
        assert_eq!(
            names,
            [
                "nav.parent",
                "nav.enter",
                "nav.choose",
                "file.create",
                "file.trash",
                "file.delete",
                "file.rename",
                "file.duplicate",
                "file.copy_to",
                "file.move_to",
                "file.extract",
                "file.compress",
                "file.open",
                "file.info",
                "copy.path",
                "copy.name",
                "copy.parent",
                "select.visual",
                "search.current",
                "search.root",
                "view.cycle",
                "view.sort",
                "view.hidden",
                "pane.switch",
                "help",
                "quit",
            ]
        );
        // Every other action is listed once.
        assert_eq!(all.len(), Action::ALL.len() - 5);
        let group = |action| {
            let found = all.iter().find(|found| found.action == action).unwrap();
            (found.group, found.description)
        };
        assert_eq!(
            group(Action::FileTrash),
            ("File Operations", "Move to Trash (with confirmation)")
        );
        assert_eq!(
            group(Action::CopyParent),
            ("Clipboard", "Copy parent folder path")
        );
        assert_eq!(
            group(Action::SelectVisual),
            ("Visual Mode", "Select adjacent items; move to extend")
        );
        assert_eq!(
            group(Action::PaneSwitch),
            ("Dual Pane Mode", "Switch active pane")
        );
        assert!(all.iter().all(|found| found.positions.is_empty()));
    }

    #[test]
    fn runs_of_letters_rank_by_where_they_start_and_the_matched_letters_are_marked() {
        assert_eq!(
            found("hid"),
            [("view.hidden", "_______hid_________".to_string())]
        );
        assert_eq!(
            found("TRASH"),
            [(
                "file.trash",
                "________Trash____________________".to_string()
            )]
        );
        let names =
            |query| -> Vec<&str> { found(query).into_iter().map(|(name, _)| name).collect() };
        // Descriptions starting with the query, then one with a later word starting so.
        assert_eq!(
            names("copy"),
            [
                "file.duplicate",
                "file.copy_to",
                "copy.path",
                "copy.name",
                "copy.parent",
                "nav.choose",
            ]
        );
        assert_eq!(found("copy")[5].1, "________________copy_____");
        assert_eq!(names("copy path"), ["copy.path", "nav.choose"]);
        // The start of the description, then of a later word, then inside a word.
        assert_eq!(
            names("to"),
            [
                "view.hidden",
                "nav.parent",
                "file.trash",
                "file.copy_to",
                "file.move_to",
                "file.compress",
                "select.visual",
                "nav.enter",
                "file.rename",
                "file.duplicate",
                "search.current",
            ]
        );
        // Letters found apart don't add to runs ("sort" is in "Move to Trash (with
        // confirmation)" that way).
        assert_eq!(names("sort"), ["view.sort"]);
    }

    #[test]
    fn setting_names_match_too_and_letters_apart_only_when_nothing_else_does() {
        assert_eq!(
            found("view"),
            [
                (
                    "view.cycle",
                    "______view________________________".to_string()
                ),
                ("view.sort", "_________________".to_string()),
                ("view.hidden", "___________________".to_string()),
            ]
        );
        assert_eq!(found("dup")[0].0, "file.duplicate");
        assert_eq!(found("FILE.TR")[0].0, "file.trash");
        assert_eq!(
            found("cpath"),
            [
                ("nav.choose", "________________c_p___ath".to_string()),
                ("copy.path", "C_p___ath".to_string()),
                ("copy.parent", "C_p___a___t___________h".to_string()),
            ]
        );
        assert!(found("zzz").is_empty());
    }
}
