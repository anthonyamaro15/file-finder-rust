use crossterm::event::KeyCode;

use crate::{
    app::{App, InputMode, SearchScope},
    directory_store::DirectoryStore,
};

fn refresh_root_search(app: &mut App, store: &DirectoryStore) {
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
}
