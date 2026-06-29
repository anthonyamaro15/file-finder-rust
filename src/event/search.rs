use crossterm::event::KeyCode;

use crate::{
    app::{App, InputMode},
    directory_store::DirectoryStore,
};

pub fn handle_search_key(app: &mut App, key_code: KeyCode, store: &DirectoryStore) {
    match key_code {
        KeyCode::Enter => app.submit_message(),
        KeyCode::Char(to_insert) => {
            app.enter_char(to_insert, store.clone());
        }
        KeyCode::Backspace => {
            app.delete_char(store.clone());
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
