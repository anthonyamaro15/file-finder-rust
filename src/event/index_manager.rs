//! Keys in the index manager.

use std::path::Path;

use crossterm::event::{KeyCode, KeyEvent};

use crate::{
    app::InputMode,
    config::settings::{update_ignore_paths_in, Settings},
    directory_store::{DirectoryIndex, IndexRules},
    event::LoopState,
    index_manager::IndexManager,
};

/// Move, switch folders in or out, open or leave a folder, save or cancel.
pub fn handle_index_manager_key(key: KeyEvent, loop_state: &mut LoopState) {
    let LoopState {
        app,
        index,
        status_bar,
        ..
    } = loop_state;
    let Some(manager) = app.index_manager.as_mut() else {
        app.input_mode = InputMode::Normal;
        return;
    };
    // Any other key answers the discard prompt with "no" and clears a save error.
    if key.code != KeyCode::Esc {
        manager.confirm_discard = false;
    }
    if key.code != KeyCode::Enter {
        manager.error = None;
    }
    let mut close = false;
    match key.code {
        KeyCode::Down | KeyCode::Char('j') => manager.move_by(1),
        KeyCode::Up | KeyCode::Char('k') => manager.move_by(-1),
        KeyCode::PageDown => manager.move_by(10),
        KeyCode::PageUp => manager.move_by(-10),
        KeyCode::Char(' ') if manager.filling => {
            status_bar.show_warning("Wait until the folder index is built".to_string(), Some(3));
        }
        KeyCode::Char(' ') => manager.toggle(),
        KeyCode::Right | KeyCode::Char('l') => {
            if let Err(why) = manager.open_highlighted(index) {
                status_bar.show_warning(why, Some(4));
            }
        }
        KeyCode::Left | KeyCode::Char('h') => manager.back(index),
        KeyCode::Enter if manager.unsaved() == 0 => close = true,
        KeyCode::Enter => {
            let saved = crate::config::get_config_dir()
                .map_err(|e| e.to_string())
                .and_then(|dir| save(manager, index, &dir.join("settings.toml")));
            match saved {
                Ok(message) => {
                    status_bar.show_info(message, Some(4));
                    close = true;
                }
                Err(error) => {
                    status_bar.show_error(format!("Couldn't save: {error}"), Some(8));
                    manager.error = Some(error);
                }
            }
        }
        KeyCode::Esc if manager.unsaved() > 0 && !manager.confirm_discard => {
            manager.confirm_discard = true;
        }
        KeyCode::Esc => close = true,
        _ => {}
    }
    if close {
        app.index_manager = None;
        app.input_mode = InputMode::Normal;
    }
}

/// Write the manager's changes to `ignore_paths` in `settings_file`, then have the index
/// follow the settings as saved. Nothing changes in the index if the file can't be
/// written. Returns what to tell the user.
pub fn save(
    manager: &IndexManager,
    index: &mut DirectoryIndex,
    settings_file: &Path,
) -> Result<String, String> {
    // A settings.toml that doesn't load would be written but not followed: refuse.
    Settings::load_from_file(settings_file).map_err(|e| short_error(&e.to_string()))?;
    let (add, remove) = manager.changes();
    // Remove each entry as settings.toml spells it, whatever folder it named.
    let remove: Vec<String> = remove
        .iter()
        .map(|path| {
            index
                .rules()
                .sources
                .get(path)
                .cloned()
                .unwrap_or_else(|| path.clone())
        })
        .collect();
    update_ignore_paths_in(settings_file, &add, &remove)
        .map_err(|e| short_error(&e.to_string()))?;
    let settings =
        Settings::load_from_file(settings_file).map_err(|e| short_error(&e.to_string()))?;
    let root = index.root.clone();
    index.set_rules(IndexRules::new(
        &settings.ignore_directories,
        &settings.ignore_paths,
        &root,
    ));
    let mut message = format!(
        "Saved: the index now holds {} folders",
        crate::utils::format::format_thousands(index.store.directories.len() as u64)
    );
    if !remove.is_empty() {
        message.push_str("; folders switched back on are re-read in the background");
    }
    Ok(message)
}

/// An error without its "Configuration error:" prefix, with home written "~".
fn short_error(error: &str) -> String {
    let error = error.trim_start_matches("Configuration error: ");
    match dirs::home_dir() {
        Some(home) => error.replace(&*home.to_string_lossy(), "~"),
        None => error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::directory_store::build_directory_from_store;
    use std::fs;

    #[test]
    fn save_writes_the_settings_then_the_index_follows_them() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("home");
        for path in ["Movies/a/b", "work/app", "music"] {
            fs::create_dir_all(root.join(path)).unwrap();
        }
        let settings_file = dir.path().join("settings.toml");
        fs::write(&settings_file, "# mine\ntheme = \"onedark\"\n").unwrap();
        let store = build_directory_from_store(root.to_str().unwrap(), Vec::new());
        let mut index = DirectoryIndex::new(store, root.clone(), IndexRules::default(), None);

        let mut manager = IndexManager::open(&index, false);
        let movies = manager
            .rows
            .iter()
            .position(|row| row.name == "Movies")
            .unwrap();
        manager.selected = movies;
        manager.toggle();
        let message = save(&manager, &mut index, &settings_file).unwrap();

        assert!(message.contains("holds 3 folders"), "{message}");
        let text = fs::read_to_string(&settings_file).unwrap();
        assert!(text.starts_with("# mine\n"), "{text}");
        let settings: Settings = toml::from_str(&text).unwrap();
        assert_eq!(
            settings.ignore_paths,
            [root.join("Movies").to_string_lossy()]
        );
        let movies = root.join("Movies").to_string_lossy().into_owned();
        assert_eq!(index.rules().ignore_paths, std::slice::from_ref(&movies));
        assert_eq!(index.store.excluded.get(&movies), Some(&3));

        // Reopened, Movies shows as off with its count; switching it back on saves that.
        let mut manager = IndexManager::open(&index, false);
        let row = manager
            .rows
            .iter()
            .position(|row| row.name == "Movies")
            .unwrap();
        assert!(manager.is_off(&manager.rows[row]));
        assert_eq!(manager.rows[row].count, Some(3));
        manager.selected = row;
        manager.toggle();
        save(&manager, &mut index, &settings_file).unwrap();
        let settings: Settings =
            toml::from_str(&fs::read_to_string(&settings_file).unwrap()).unwrap();
        assert!(settings.ignore_paths.is_empty());
        assert!(index.rules().ignore_paths.is_empty());
        assert!(index.is_refreshing());
    }

    #[test]
    fn a_failed_save_leaves_the_index_alone() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("home");
        fs::create_dir_all(root.join("Movies")).unwrap();
        let store = build_directory_from_store(root.to_str().unwrap(), Vec::new());
        let mut index = DirectoryIndex::new(store, root.clone(), IndexRules::default(), None);
        let mut manager = IndexManager::open(&index, false);
        manager.toggle();

        let missing = dir.path().join("no-such-settings.toml");
        assert!(save(&manager, &mut index, &missing).is_err());
        assert!(index.rules().ignore_paths.is_empty());
        assert_eq!(index.store.directories.len(), 1);
    }

    #[test]
    fn a_settings_file_that_does_not_load_is_left_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("home");
        fs::create_dir_all(root.join("Movies")).unwrap();
        let settings_file = dir.path().join("settings.toml");
        let broken = "show_hints = \"yes\"\n";
        fs::write(&settings_file, broken).unwrap();
        let store = build_directory_from_store(root.to_str().unwrap(), Vec::new());
        let mut index = DirectoryIndex::new(store, root.clone(), IndexRules::default(), None);
        let mut manager = IndexManager::open(&index, false);
        manager.toggle();

        assert!(save(&manager, &mut index, &settings_file).is_err());
        assert_eq!(fs::read_to_string(&settings_file).unwrap(), broken);
        assert!(index.rules().ignore_paths.is_empty());
    }
}
