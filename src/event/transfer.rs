//! Copying or moving the highlighted item to another folder: to the other pane in
//! dual pane (`Shift+C`/`Shift+M`), or to a folder picked by name ("Copy to…").

use std::{
    fs,
    path::{Path, PathBuf},
    sync::mpsc,
};

use crossterm::event::{KeyCode, KeyEvent};

use crate::{
    app::{global_search_results, App, FolderPicker, InputMode, SearchResult, TransferKind},
    cli::path_utils::expand_home,
    directory_store::DirectoryStore,
    event::{
        file_actions::containing_directory_or_current,
        navigation::{next_index_wrapping, previous_index_wrapping},
    },
    operations::{self, copy_dir_file_with_progress_cancellable, CopyMessage},
    status_bar::StatusBar,
};

/// Where `source` goes when copied or moved into `dest_dir`, or why it mustn't: the
/// folder is gone, is the item itself or inside it, or (for a move) already holds it.
pub fn transfer_destination(
    kind: TransferKind,
    source: &Path,
    dest_dir: &Path,
) -> Result<PathBuf, String> {
    let verb = kind.title().to_lowercase();
    let name = item_name(source);
    let Some(source_name) = source.file_name() else {
        return Err(format!("Can't {verb} '{name}'"));
    };
    let folder = item_name(dest_dir);
    let dest = fs::canonicalize(dest_dir)
        .ok()
        .filter(|dest| dest.is_dir())
        .ok_or_else(|| format!("Folder '{folder}' no longer exists"))?;
    // Resolve the parent, not the item, so a link counts as itself, not its target.
    let parent = fs::canonicalize(containing_directory_or_current(source))
        .map_err(|e| format!("Could not read '{name}': {e}"))?;
    let item = parent.join(source_name);
    if dest.ancestors().any(|dir| same_item(dir, &item)) {
        return Err(format!("Can't {verb} '{name}' into itself"));
    }
    if kind == TransferKind::Move && same_item(&dest, &parent) {
        return Err(format!("'{name}' is already in '{folder}'"));
    }
    Ok(dest_dir.join(source_name))
}

/// Whether two paths name the same file or folder. One folder can have several
/// resolved spellings (firmlinks such as /System/Volumes/Data/Users, bind mounts), so
/// Unix compares device and inode. Links aren't followed: a link is never its target.
fn same_item(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if let (Ok(a), Ok(b)) = (fs::symlink_metadata(a), fs::symlink_metadata(b)) {
            return (a.dev(), a.ino()) == (b.dev(), b.ino());
        }
    }
    false
}

/// Copy or move `source` into `dest_dir` in the background, as `Shift+C`/`Shift+M`
/// always have, unless another operation runs or `transfer_destination` refuses; the
/// error is the warning to show. `destination` names the folder in the result, e.g.
/// "other pane".
pub fn start_transfer(
    app: &mut App,
    copy_receiver: &mut Option<mpsc::Receiver<CopyMessage>>,
    selected: Option<usize>,
    kind: TransferKind,
    source: String,
    dest_dir: String,
    destination: String,
) -> Result<(), String> {
    if let Some(warning) = app.busy_warning() {
        return Err(warning);
    }
    let dest = transfer_destination(kind, Path::new(&source), Path::new(&dest_dir))?;
    let name = item_name(Path::new(&source));
    match kind {
        TransferKind::Copy => {
            app.preserved_selection_index = selected;
            app.start_copy(dest.clone(), Some((name, destination)));
            *copy_receiver = Some(copy_dir_file_with_progress_cancellable(
                Path::new(&source),
                &dest,
                app.copy_cancel.clone(),
            ));
            Ok(())
        }
        TransferKind::Move => {
            let status = format!("Moving '{name}'…");
            let changes = vec![PathBuf::from(&source), dest];
            app.start_job("move", status, changes, move |cancel| {
                operations::move_file_or_dir_cancellable(&source, &dest_dir, &cancel)
                    .map(|()| format!("Moved '{name}' to {destination}"))
                    .map_err(|e| format!("Move failed: {e}"))
            })
        }
    }
}

/// Last component of `path` for messages, or the whole path (e.g. "/").
fn item_name(path: &Path) -> String {
    path.file_name()
        .unwrap_or(path.as_os_str())
        .to_string_lossy()
        .into_owned()
}

/// Picker rows for `query`: a typed folder path (absolute or starting with `~`) first,
/// then the folder index's matches, found and ranked exactly as global search does.
pub fn folder_picker_results(
    query: &str,
    store: &DirectoryStore,
    max_results: usize,
) -> Vec<SearchResult> {
    let query = query.trim();
    if query.is_empty() {
        return Vec::new();
    }
    let mut results = global_search_results(store, query, max_results);
    if query.starts_with('/') || query.starts_with('~') {
        let typed: PathBuf = expand_home(Path::new(query))
            .unwrap_or_else(|_| PathBuf::from(query))
            .components()
            .collect();
        if typed.is_absolute() && typed.is_dir() {
            let file_path = typed.to_string_lossy().into_owned();
            results.retain(|result| result.file_path != file_path);
            results.insert(
                0,
                SearchResult {
                    display_name: item_name(&typed),
                    file_path,
                    score: 0,
                    is_directory: true,
                    original_index: 0,
                },
            );
        }
    }
    results
}

/// Open "Copy to…"/"Move to…" for the highlighted item, unless an operation runs.
pub fn open_folder_picker(
    app: &mut App,
    status_bar: &mut StatusBar,
    selected: Option<usize>,
    kind: TransferKind,
) {
    if let Some(warning) = app.busy_warning() {
        status_bar.show_warning(warning, None);
        return;
    }
    let Some(source) = app.active_selected_path(selected) else {
        return;
    };
    app.reset_create_edit_values();
    app.folder_picker = Some(FolderPicker {
        kind,
        source,
        results: Vec::new(),
        selected: None,
    });
    app.input_mode = InputMode::PickFolder;
}

fn close_folder_picker(app: &mut App) {
    app.folder_picker = None;
    app.reset_create_edit_values();
    app.input_mode = InputMode::Normal;
}

fn refresh_folder_picker(app: &mut App, store: &DirectoryStore) {
    let results = folder_picker_results(&app.create_edit_file_name, store, app.max_search_results);
    if let Some(picker) = app.folder_picker.as_mut() {
        picker.selected = (!results.is_empty()).then_some(0);
        picker.results = results;
    }
}

/// Picker keys: type to search, ↑/↓ to choose, Enter to copy or move there, Esc to
/// cancel. A refused folder keeps the picker open with the reason in the status bar.
/// `selected` is the list row to keep selected after a copy.
pub fn handle_folder_picker_key(
    key: KeyEvent,
    app: &mut App,
    selected: Option<usize>,
    status_bar: &mut StatusBar,
    store: &DirectoryStore,
    copy_receiver: &mut Option<mpsc::Receiver<CopyMessage>>,
) {
    let Some(picker) = app.folder_picker.as_mut() else {
        app.input_mode = InputMode::Normal;
        return;
    };
    match key.code {
        KeyCode::Esc => close_folder_picker(app),
        KeyCode::Up => {
            picker.selected = previous_index_wrapping(picker.selected, picker.results.len())
        }
        KeyCode::Down => {
            picker.selected = next_index_wrapping(picker.selected, picker.results.len())
        }
        KeyCode::Left => app.move_create_edit_cursor_left(),
        KeyCode::Right => app.move_create_edit_cursor_right(),
        KeyCode::Char(c) => {
            app.add_char(c);
            refresh_folder_picker(app, store);
        }
        KeyCode::Backspace => {
            app.delete_c();
            refresh_folder_picker(app, store);
        }
        KeyCode::Enter => {
            let Some(dest_dir) = picker
                .selected
                .and_then(|i| picker.results.get(i))
                .map(|result| result.file_path.clone())
            else {
                return;
            };
            let (kind, source) = (picker.kind, picker.source.clone());
            let destination = format!("'{}'", item_name(Path::new(&dest_dir)));
            match start_transfer(
                app,
                copy_receiver,
                selected,
                kind,
                source,
                dest_dir,
                destination,
            ) {
                Ok(()) => close_folder_picker(app),
                Err(refusal) => status_bar.show_warning(refusal, None),
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::background::{refresh_after_copy, tests::memory_index};
    use crossterm::event::KeyModifiers;
    use ratatui::widgets::ListState;
    use std::time::{Duration, Instant};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn type_query(app: &mut App, query: &str, store: &DirectoryStore) {
        for c in query.chars() {
            press(app, KeyCode::Char(c), store, &mut None);
        }
    }

    fn press(
        app: &mut App,
        code: KeyCode,
        store: &DirectoryStore,
        copy_receiver: &mut Option<mpsc::Receiver<CopyMessage>>,
    ) -> StatusBar {
        let mut status = StatusBar::new();
        handle_folder_picker_key(key(code), app, Some(0), &mut status, store, copy_receiver);
        status
    }

    fn paths(results: &[SearchResult]) -> Vec<&str> {
        results.iter().map(|r| r.file_path.as_str()).collect()
    }

    #[test]
    fn destination_refuses_the_item_itself_folders_inside_it_and_a_move_to_its_folder() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let project = root.join("proj");
        fs::create_dir_all(project.join("sub")).unwrap();
        fs::create_dir_all(root.join("proj2")).unwrap();
        let notes = root.join("notes.txt");
        fs::write(&notes, "notes").unwrap();
        let folder = root.file_name().unwrap().to_string_lossy();

        for (kind, verb) in [(TransferKind::Copy, "copy"), (TransferKind::Move, "move")] {
            for inside in [project.clone(), project.join("sub")] {
                assert_eq!(
                    transfer_destination(kind, &project, &inside),
                    Err(format!("Can't {verb} 'proj' into itself"))
                );
            }
            // A sibling that merely starts with the same name is a valid destination.
            assert_eq!(
                transfer_destination(kind, &project, &root.join("proj2")),
                Ok(root.join("proj2/proj"))
            );
            assert_eq!(
                transfer_destination(kind, &notes, &project),
                Ok(project.join("notes.txt"))
            );
            assert_eq!(
                transfer_destination(kind, &project, &root.join("gone")),
                Err("Folder 'gone' no longer exists".into())
            );
        }
        for source in [&project, &notes] {
            let name = source.file_name().unwrap().to_string_lossy();
            assert_eq!(
                transfer_destination(TransferKind::Move, source, root),
                Err(format!("'{name}' is already in '{folder}'"))
            );
        }
        // A copy beside itself reaches the copy engine, which reports the name conflict.
        assert_eq!(
            transfer_destination(TransferKind::Copy, &project, root),
            Ok(project.clone())
        );
    }

    #[cfg(unix)]
    #[test]
    fn destination_sees_through_aliases_but_not_a_linked_source() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("proj");
        fs::create_dir_all(project.join("sub")).unwrap();
        let alias = dir.path().join("alias");
        std::os::unix::fs::symlink(&project, &alias).unwrap();

        assert_eq!(
            transfer_destination(TransferKind::Copy, &project, &alias.join("sub")),
            Err("Can't copy 'proj' into itself".into())
        );
        // Copying the link itself into the folder it points to copies only the link.
        assert_eq!(
            transfer_destination(TransferKind::Copy, &alias, &project),
            Ok(project.join("alias"))
        );
    }

    /// macOS also reaches the data volume through /System/Volumes/Data, a second
    /// resolved spelling of the same folders.
    #[cfg(target_os = "macos")]
    #[test]
    fn destination_refuses_a_firmlinked_spelling_of_the_item_itself() {
        let dir = tempfile::tempdir().unwrap();
        let project = fs::canonicalize(dir.path()).unwrap().join("proj");
        fs::create_dir_all(project.join("sub")).unwrap();
        let firmlinked = Path::new("/System/Volumes/Data")
            .join(project.strip_prefix("/").unwrap())
            .join("sub");
        if !firmlinked.is_dir() {
            return; // No separate data volume on this system.
        }
        assert_ne!(fs::canonicalize(&firmlinked).unwrap(), project.join("sub"));

        for (kind, verb) in [(TransferKind::Copy, "copy"), (TransferKind::Move, "move")] {
            assert_eq!(
                transfer_destination(kind, &project, &firmlinked),
                Err(format!("Can't {verb} 'proj' into itself"))
            );
        }
        let notes = project.join("notes.txt");
        fs::write(&notes, "notes").unwrap();
        assert_eq!(
            transfer_destination(TransferKind::Move, &notes, firmlinked.parent().unwrap()),
            Err("'notes.txt' is already in 'proj'".into())
        );
    }

    #[test]
    fn transfers_run_on_the_dual_pane_machinery_and_name_the_folder() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let projects = root.join("Projects");
        fs::create_dir(&projects).unwrap();
        let source = root.join("proj");
        fs::create_dir_all(source.join("sub")).unwrap();
        fs::write(source.join("sub/a.txt"), "a").unwrap();
        let notes = root.join("notes.txt");
        fs::write(&notes, "notes").unwrap();
        let mut app = App::new(Vec::new());
        app.current_directory = root.to_string_lossy().into_owned();
        let mut index = memory_index(root);
        let mut receiver = None;
        let folder = "'Projects'".to_string();
        let path = |p: &Path| p.to_string_lossy().into_owned();

        start_transfer(
            &mut app,
            &mut receiver,
            Some(1),
            TransferKind::Copy,
            path(&source),
            path(&projects),
            folder.clone(),
        )
        .unwrap();
        assert!(app.copy_in_progress);
        assert_eq!(app.preserved_selection_index, Some(1));
        // Only one operation runs at a time.
        assert_eq!(
            start_transfer(
                &mut app,
                &mut None,
                None,
                TransferKind::Move,
                path(&notes),
                path(&projects),
                folder.clone(),
            ),
            Err("Wait for the current copy to finish".into())
        );
        let mut status = StatusBar::new();
        let deadline = Instant::now() + Duration::from_secs(5);
        while receiver.is_some() {
            let mut state = ListState::default();
            let fallback = path(root);
            refresh_after_copy(
                &mut app,
                &mut state,
                &mut status,
                &mut receiver,
                &mut index,
                &fallback,
            );
            assert!(Instant::now() < deadline, "copy never finished");
            std::thread::yield_now();
        }
        assert_eq!(
            status.error_message.as_deref(),
            Some("Copied 'proj' to 'Projects'")
        );
        assert_eq!(
            fs::read_to_string(projects.join("proj/sub/a.txt")).unwrap(),
            "a"
        );
        assert!(source.join("sub/a.txt").exists());

        start_transfer(
            &mut app,
            &mut receiver,
            None,
            TransferKind::Move,
            path(&notes),
            path(&projects),
            folder.clone(),
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let (result, changes) = loop {
            if let Some(finished) = app.poll_job() {
                break finished;
            }
            assert!(Instant::now() < deadline, "move never finished");
            std::thread::yield_now();
        };
        assert_eq!(result, Ok("Moved 'notes.txt' to 'Projects'".into()));
        assert_eq!(changes, [notes.clone(), projects.join("notes.txt")]);
        assert!(!notes.exists());

        // Same name already there: the copy engine and the move refuse, as before.
        let rx = start_copy_and_wait(&mut app, &source, &projects);
        assert!(rx.starts_with("Destination already exists"), "{rx}");
        fs::write(&notes, "again").unwrap();
        let moved = operations::move_file_or_dir_cancellable(
            &path(&notes),
            &path(&projects),
            &std::sync::atomic::AtomicBool::new(false),
        );
        assert!(moved.unwrap_err().user_message().contains("already exists"));
    }

    /// Start a copy through `start_transfer` and return its final message.
    fn start_copy_and_wait(app: &mut App, source: &Path, dest_dir: &Path) -> String {
        let mut receiver = None;
        start_transfer(
            app,
            &mut receiver,
            None,
            TransferKind::Copy,
            source.to_string_lossy().into_owned(),
            dest_dir.to_string_lossy().into_owned(),
            "'there'".into(),
        )
        .unwrap();
        let message = loop {
            match receiver
                .as_ref()
                .unwrap()
                .recv_timeout(Duration::from_secs(5))
                .unwrap()
            {
                CopyMessage::Completed { message, .. } | CopyMessage::Error(message) => {
                    break message
                }
                _ => {}
            }
        };
        app.copy_in_progress = false;
        message
    }

    #[test]
    fn refused_transfer_starts_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("proj");
        fs::create_dir_all(project.join("sub")).unwrap();
        let mut app = App::new(Vec::new());
        let mut receiver = None;

        let refused = start_transfer(
            &mut app,
            &mut receiver,
            None,
            TransferKind::Copy,
            project.to_string_lossy().into_owned(),
            project.join("sub").to_string_lossy().into_owned(),
            "other pane".into(),
        );

        assert_eq!(refused, Err("Can't copy 'proj' into itself".into()));
        assert_eq!(app.busy_operation(), None);
        assert!(receiver.is_none());
        assert_eq!(fs::read_dir(project.join("sub")).unwrap().count(), 0);
    }

    #[test]
    fn picker_results_rank_like_global_search_with_a_typed_path_first() {
        let dir = tempfile::tempdir().unwrap();
        let typed = dir.path().join("Typed");
        fs::create_dir(&typed).unwrap();
        fs::write(dir.path().join("file.txt"), "").unwrap();
        let mut store = DirectoryStore::new();
        for path in [
            "/r/Projects",
            "/r/work/Projects",
            "/r/photos",
            "/r/proj-notes",
        ] {
            store.insert(path);
        }
        store.insert(typed.to_str().unwrap());

        assert!(folder_picker_results("", &store, 500).is_empty());
        assert!(folder_picker_results("   ", &store, 500).is_empty());
        let results = folder_picker_results("proj", &store, 500);
        let global = global_search_results(&store, "proj", 500);
        assert_eq!(paths(&results), paths(&global));
        let labels = |results: &[SearchResult]| -> Vec<String> {
            results.iter().map(|r| r.display_name.clone()).collect()
        };
        assert_eq!(labels(&results), labels(&global));

        // Typed folder paths come first and once, indexed or not, with or without "/".
        let typed_str = typed.to_str().unwrap();
        for query in [typed_str.to_string(), format!("{typed_str}/")] {
            let results = folder_picker_results(&query, &store, 500);
            assert_eq!(results[0].file_path, typed_str);
            assert_eq!(results[0].display_name, "Typed");
            assert_eq!(
                paths(&results).iter().filter(|p| **p == typed_str).count(),
                1
            );
        }
        let unindexed = dir.path().to_str().unwrap();
        assert_eq!(
            folder_picker_results(unindexed, &store, 500)[0].file_path,
            unindexed
        );
        for not_a_folder in [dir.path().join("file.txt"), dir.path().join("missing")] {
            let results = folder_picker_results(not_a_folder.to_str().unwrap(), &store, 500);
            assert!(!paths(&results).contains(&not_a_folder.to_str().unwrap()));
        }
        let home = dirs::home_dir().unwrap();
        assert_eq!(
            folder_picker_results("~", &store, 500)[0].file_path,
            home.to_string_lossy()
        );
    }

    #[test]
    fn picker_keys_choose_a_folder_and_esc_cancels_without_side_effects() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        for name in ["Projects", "photos"] {
            fs::create_dir(root.join(name)).unwrap();
        }
        let notes = root.join("notes.txt");
        fs::write(&notes, "notes").unwrap();
        let mut store = DirectoryStore::new();
        for name in ["Projects", "photos"] {
            store.insert(root.join(name).to_str().unwrap());
        }
        let mut app = App::new(vec![notes.to_string_lossy().into_owned()]);
        let mut status = StatusBar::new();

        open_folder_picker(&mut app, &mut status, Some(0), TransferKind::Copy);
        assert_eq!(app.input_mode, InputMode::PickFolder);
        assert!(app.folder_picker.as_ref().unwrap().results.is_empty());
        type_query(&mut app, "proj", &store);
        let picker = app.folder_picker.as_ref().unwrap();
        assert_eq!(picker.source, notes.to_string_lossy());
        assert_eq!(picker.selected, Some(0));
        assert_eq!(
            picker.results[0].file_path,
            root.join("Projects").to_string_lossy()
        );
        let count = picker.results.len();
        press(&mut app, KeyCode::Up, &store, &mut None);
        assert_eq!(
            app.folder_picker.as_ref().unwrap().selected,
            Some(count - 1)
        );
        press(&mut app, KeyCode::Down, &store, &mut None);
        assert_eq!(app.folder_picker.as_ref().unwrap().selected, Some(0));

        let mut receiver = None;
        press(&mut app, KeyCode::Esc, &store, &mut receiver);
        assert!(app.folder_picker.is_none());
        assert_eq!(app.input_mode, InputMode::Normal);
        assert!(app.create_edit_file_name.is_empty());
        assert_eq!(app.busy_operation(), None);
        assert!(receiver.is_none());
        assert!(!root.join("Projects/notes.txt").exists());

        open_folder_picker(&mut app, &mut status, Some(0), TransferKind::Copy);
        type_query(&mut app, "proj", &store);
        press(&mut app, KeyCode::Enter, &store, &mut receiver);
        assert!(app.folder_picker.is_none());
        assert_eq!(app.input_mode, InputMode::Normal);
        assert!(app.copy_in_progress);
        assert_eq!(
            app.copy_transfer,
            Some(("notes.txt".into(), "'Projects'".into()))
        );
        assert_eq!(app.copy_destination, Some(root.join("Projects/notes.txt")));
        assert!(receiver.is_some());

        // While it runs, the picker doesn't open.
        open_folder_picker(&mut app, &mut status, Some(0), TransferKind::Move);
        assert!(app.folder_picker.is_none());
        assert_eq!(
            status.error_message.as_deref(),
            Some("Wait for the current copy to finish")
        );
    }

    #[test]
    fn picker_keeps_a_refused_folder_open_with_the_reason() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("Projects");
        fs::create_dir(&project).unwrap();
        let mut store = DirectoryStore::new();
        store.insert(project.to_str().unwrap());
        let mut app = App::new(vec![project.to_string_lossy().into_owned()]);
        let mut receiver = None;

        open_folder_picker(&mut app, &mut StatusBar::new(), Some(0), TransferKind::Move);
        type_query(&mut app, "Projects", &store);
        let status = press(&mut app, KeyCode::Enter, &store, &mut receiver);

        assert_eq!(
            status.error_message.as_deref(),
            Some("Can't move 'Projects' into itself")
        );
        assert_eq!(app.input_mode, InputMode::PickFolder);
        assert_eq!(app.create_edit_file_name, "Projects");
        assert_eq!(app.busy_operation(), None);
        assert!(receiver.is_none());
    }
}
