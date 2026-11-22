use crossterm::event::KeyCode;

use crate::{
    app::{App, InputMode, SearchScope},
    directory_store::DirectoryStore,
};

pub(crate) fn refresh_root_search(app: &mut App, store: &DirectoryStore) {
    if app.searching_right_pane() {
        app.right_pane
            .search(&app.input, store, app.max_search_results);
        return;
    }
    let query = app.input.trim().to_string();
    if query.is_empty() {
        app.search_results.clear();
        app.filtered_indexes.clear();
    } else {
        app.perform_global_search(&query, store);
    }
}

/// Leave the search box; results stay on screen to browse.
fn leave_search(app: &mut App) {
    // The right pane keeps its query, so the box doesn't keep it for the left.
    if app.searching_right_pane() {
        app.input.clear();
        app.reset_cursor();
    }
    app.input_mode = InputMode::Normal;
}

/// Handle a key while typing a search. Returns true when Enter in a root search with
/// results should choose the highlighted (top) result, as `Esc` then `Enter` would.
pub fn handle_search_key(app: &mut App, key_code: KeyCode, store: &DirectoryStore) -> bool {
    match key_code {
        KeyCode::Enter => {
            let choose = app.search_scope == SearchScope::Root && app.has_search_results();
            leave_search(app);
            return choose;
        }
        // Leading space or slash in a local search jumps to root search (pre-keymap behavior).
        KeyCode::Char(' ' | '/')
            if app.search_scope == SearchScope::CurrentDirectory && app.input.is_empty() =>
        {
            app.enter_search(SearchScope::Root);
        }
        // Backspace on an empty root search returns to local search.
        KeyCode::Backspace if app.search_scope == SearchScope::Root && app.input.is_empty() => {
            app.enter_search(SearchScope::CurrentDirectory);
        }
        KeyCode::Char(to_insert) => {
            if app.search_scope == SearchScope::Root {
                app.insert_char_without_filter(to_insert);
                app.move_cursor_right();
                refresh_root_search(app, store);
            } else {
                app.enter_char(to_insert, store.clone());
            }
        }
        KeyCode::Backspace => {
            if app.search_scope == SearchScope::Root {
                if app.delete_char_without_filter() {
                    refresh_root_search(app, store);
                }
            } else {
                app.delete_char(store.clone());
            }
        }
        KeyCode::Left => {
            app.move_cursor_left();
        }
        KeyCode::Right => {
            app.move_cursor_right();
        }
        KeyCode::Esc => leave_search(app),
        _ => {}
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_search_key_updates_results_immediately() {
        let mut app = App::new(Vec::new());
        let mut store = DirectoryStore::new();
        store.insert("/workspace/project/src");

        app.enter_search(SearchScope::Root);
        handle_search_key(&mut app, KeyCode::Char('s'), &store);

        assert_eq!(app.input, "s");
        assert_eq!(app.search_results.len(), 1);
        assert_eq!(app.search_results[0].file_path, "/workspace/project/src");
    }

    fn assert_prefix_switches_to_root_search(prefix: char) {
        let mut app = App::new(Vec::new());
        let mut store = DirectoryStore::new();
        store.insert("/workspace/project/src");

        app.enter_search(SearchScope::CurrentDirectory);
        handle_search_key(&mut app, KeyCode::Char(prefix), &store);

        assert_eq!(app.search_scope, SearchScope::Root);
        assert!(app.global_search_mode);
        assert_eq!(app.input, "");
        assert_eq!(app.character_index, 0);

        handle_search_key(&mut app, KeyCode::Char('s'), &store);

        assert_eq!(app.input, "s");
        assert_eq!(app.character_index, 1);
        assert_eq!(app.search_results.len(), 1);
        assert_eq!(app.search_results[0].file_path, "/workspace/project/src");
    }

    #[test]
    fn leading_space_in_local_search_switches_to_root_search() {
        assert_prefix_switches_to_root_search(' ');
    }

    #[test]
    fn leading_slash_in_local_search_switches_to_root_search() {
        assert_prefix_switches_to_root_search('/');
    }

    #[test]
    fn space_after_text_in_local_search_stays_local() {
        let mut app = App::new(Vec::new());
        let store = DirectoryStore::new();

        app.enter_search(SearchScope::CurrentDirectory);
        for c in "foo bar".chars() {
            handle_search_key(&mut app, KeyCode::Char(c), &store);
        }

        assert_eq!(app.search_scope, SearchScope::CurrentDirectory);
        assert!(!app.global_search_mode);
        assert_eq!(app.input, "foo bar");
        assert_eq!(app.character_index, 7);
    }

    #[test]
    fn backspace_on_empty_root_search_returns_to_local_search() {
        let files = vec![
            "/workspace/a.txt".to_string(),
            "/workspace/b.txt".to_string(),
        ];
        let mut app = App::new(files);
        let mut store = DirectoryStore::new();
        store.insert("/workspace/project/src");

        app.enter_search(SearchScope::CurrentDirectory);
        handle_search_key(&mut app, KeyCode::Char('/'), &store);
        assert_eq!(app.search_scope, SearchScope::Root);

        handle_search_key(&mut app, KeyCode::Backspace, &store);

        assert_eq!(app.input_mode, InputMode::Editing);
        assert_eq!(app.search_scope, SearchScope::CurrentDirectory);
        assert!(!app.global_search_mode);
        assert_eq!(app.input, "");
        assert!(app.search_results.is_empty());
        assert_eq!(app.filtered_indexes, vec![0, 1]);
    }

    #[test]
    fn enter_in_root_search_chooses_the_top_result() {
        let mut app = App::new(Vec::new());
        let mut store = DirectoryStore::new();
        store.insert("/workspace/project/src");

        app.enter_search(SearchScope::Root);
        handle_search_key(&mut app, KeyCode::Char('s'), &store);

        assert!(handle_search_key(&mut app, KeyCode::Enter, &store));
        assert_eq!(app.input_mode, InputMode::Normal);
        assert_eq!(
            app.get_selected_path(Some(0)).as_deref(),
            Some("/workspace/project/src")
        );
    }

    #[test]
    fn enter_without_root_results_only_leaves_the_box() {
        let mut app = App::new(vec!["/workspace/a.txt".to_string()]);
        let mut store = DirectoryStore::new();
        store.insert("/workspace/project/src");

        // No match: nothing to choose, so the listing's item isn't chosen instead.
        app.enter_search(SearchScope::Root);
        handle_search_key(&mut app, KeyCode::Char('z'), &store);
        assert!(!handle_search_key(&mut app, KeyCode::Enter, &store));
        assert_eq!(app.input_mode, InputMode::Normal);

        // A local search keeps its filter to browse, like Esc.
        app.enter_search(SearchScope::CurrentDirectory);
        handle_search_key(&mut app, KeyCode::Char('a'), &store);
        assert!(!handle_search_key(&mut app, KeyCode::Enter, &store));
        assert_eq!(app.input_mode, InputMode::Normal);
        assert_eq!(app.input, "a");
    }

    #[test]
    fn operation_finishing_while_typing_root_search_keeps_query_and_targets() {
        let mut app = App::new(vec!["/cwd/a.txt".to_string()]);
        let mut store = DirectoryStore::new();
        store.insert("/workspace/project/src");
        let mut state = ratatui::widgets::ListState::default();
        state.select(Some(0));

        app.enter_search(SearchScope::Root);
        handle_search_key(&mut app, KeyCode::Char('s'), &store);
        // A copy into the browsed folder finishes mid-query.
        app.refresh_files_after_operation(
            vec!["/cwd/a.txt".to_string(), "/cwd/b.txt".to_string()].into(),
            &mut state,
            Some("/cwd/b.txt".into()),
        );
        handle_search_key(&mut app, KeyCode::Char('r'), &store);

        // The left listing draws results only in global mode; actions take them anyway.
        assert!(app.global_search_mode);
        assert_eq!(app.get_list_length(), 1);
        assert_eq!(
            app.get_selected_path(state.selected()).as_deref(),
            Some("/workspace/project/src")
        );
        assert_eq!(app.input, "sr");
    }

    #[test]
    fn root_search_typed_after_its_results_were_cleared_draws_what_it_acts_on() {
        let mut app = App::new(vec!["/cwd/a.txt".to_string()]);
        let mut store = DirectoryStore::new();
        store.insert("/workspace/project/src");

        app.enter_search(SearchScope::Root);
        handle_search_key(&mut app, KeyCode::Char('s'), &store);
        app.clear_search();
        handle_search_key(&mut app, KeyCode::Char('s'), &store);

        assert!(app.global_search_mode);
        assert_eq!(
            app.get_selected_path(Some(0)).as_deref(),
            Some("/workspace/project/src")
        );
    }

    #[test]
    fn backspace_with_text_in_root_search_deletes_char() {
        let mut app = App::new(Vec::new());
        let mut store = DirectoryStore::new();
        store.insert("/workspace/project/src");

        app.enter_search(SearchScope::Root);
        for c in "sr".chars() {
            handle_search_key(&mut app, KeyCode::Char(c), &store);
        }
        handle_search_key(&mut app, KeyCode::Backspace, &store);

        assert_eq!(app.search_scope, SearchScope::Root);
        assert!(app.global_search_mode);
        assert_eq!(app.input, "s");
        assert_eq!(app.search_results.len(), 1);
    }
}
