use file_finder::watcher::{FileSystemWatcher, WatcherEvent};
use std::fs;
use std::time::{Duration, Instant};
use tempfile::tempdir;

fn wait_for_events(watcher: &FileSystemWatcher, timeout: Duration) -> Vec<WatcherEvent> {
    let deadline = Instant::now() + timeout;
    let mut events = Vec::new();

    while Instant::now() < deadline {
        events.extend(watcher.poll_events());
        if !events.is_empty() {
            return events;
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    events
}

#[test]
fn production_watcher_reports_created_files() {
    let dir = tempdir().unwrap();
    let mut watcher = FileSystemWatcher::new().unwrap();

    watcher.watch_directory(dir.path()).unwrap();

    let test_file = dir.path().join("created.txt");
    fs::write(&test_file, "created by integration test").unwrap();

    let events = wait_for_events(&watcher, Duration::from_secs(4));

    assert!(
        events.iter().any(
            |event| matches!(event, WatcherEvent::FilesCreated(paths) if paths.contains(&test_file))
        ),
        "expected created file event for {}, got {events:?}",
        test_file.display()
    );
}

#[test]
fn production_watcher_reports_deleted_files() {
    let dir = tempdir().unwrap();
    let test_file = dir.path().join("deleted.txt");
    fs::write(&test_file, "delete me").unwrap();

    let mut watcher = FileSystemWatcher::new().unwrap();
    watcher.watch_directory(dir.path()).unwrap();

    fs::remove_file(&test_file).unwrap();

    let events = wait_for_events(&watcher, Duration::from_secs(4));

    assert!(
        events.iter().any(
            |event| matches!(event, WatcherEvent::FilesDeleted(paths) if paths.contains(&test_file))
        ),
        "expected deleted file event for {}, got {events:?}",
        test_file.display()
    );
}
