use crossterm::event::KeyCode;

use crate::{
    app::{App, InputMode, SearchScope},
    directory_store::DirectoryStore,
};

fn refresh_root_search(app: &mut App, store: &DirectoryStore) {
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

pub fn handle_search_key(app: &mut App, key_code: KeyCode, store: &DirectoryStore) {
    match key_code {
        KeyCode::Enter => app.submit_message(),
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
        KeyCode::Esc => {
            // The right pane keeps its query, so the box doesn't keep it for the left.
            if app.searching_right_pane() {
                app.input.clear();
                app.reset_cursor();
            }
            app.input_mode = InputMode::Normal;
        }
        _ => {}
    }
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
