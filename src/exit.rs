//! Leaving ff: quit confirmation, opening the selection with the system opener or
//! the editor, and the shell integration's result file.

use std::{io, path::Path, process::Command, sync::mpsc};

use copypasta::{ClipboardContext, ClipboardProvider};
use log::{debug, warn};
use ratatui::{
    backend::Backend,
    widgets::{Block, Borders, Paragraph},
    Terminal,
};

use crate::{app::App, operations::CopyMessage, status_bar::StatusBar};

/// Confirm once, then cancel and wait for cleanup before any exit route proceeds.
pub(crate) fn may_exit<B: Backend>(
    app: &mut App,
    status_bar: &mut StatusBar,
    key: &'static str,
    receiver: &mut Option<mpsc::Receiver<CopyMessage>>,
    terminal: &mut Terminal<B>,
    quit_error: &mut Option<String>,
) -> bool {
    use std::sync::atomic::Ordering;

    if let Err(warning) = app.confirm_quit(key, std::time::Instant::now()) {
        status_bar.show_warning(warning, None);
        return false;
    }
    let Some(operation) = app.busy_operation().filter(|kind| *kind != "delete") else {
        return true;
    };

    app.copy_cancel.store(true, Ordering::Relaxed);
    if let Some(job) = &app.job {
        job.cancel.store(true, Ordering::Relaxed);
    }
    let message = format!("Stopping the {operation} and removing partial files…");
    status_bar.show_info(message.clone(), None);
    // A draw failure must not bypass the wait and leave a worker writing.
    if let Err(error) = terminal.draw(|frame| {
        frame.render_widget(
            Paragraph::new(message)
                .block(Block::default().borders(Borders::ALL).title("Quitting"))
                .wrap(ratatui::widgets::Wrap { trim: true }),
            frame.size(),
        );
    }) {
        warn!("Could not redraw while stopping {operation}: {error}");
    }

    let result = if let Some(rx) = receiver.take() {
        let result = loop {
            match rx.recv() {
                Ok(CopyMessage::Completed { success: true, .. }) => break Ok(String::new()),
                Ok(CopyMessage::Completed { message, .. } | CopyMessage::Error(message)) => {
                    break Err(message)
                }
                Ok(_) => {} // Progress is not an acknowledgement of cancellation.
                Err(_) => break Err("Copy stopped unexpectedly before reporting a result".into()),
            }
        };
        app.copy_in_progress = false;
        app.copy_progress_message.clear();
        app.copy_files_processed = 0;
        app.copy_total_files = 0;
        app.copy_transfer = None;
        app.copy_destination = None;
        result
    } else if let Some(job) = app.job.take() {
        // A move already deleting its source deliberately ignores cancellation.
        job.wait()
    } else {
        Ok(String::new())
    };
    if let Err(error) = result {
        if error.contains("failed to remove partial copy") || !error.contains("cancelled") {
            *quit_error = Some(error);
        }
    }
    true
}

/// Program that opens a file with the system default application.
pub(crate) fn system_opener() -> &'static str {
    if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    }
}

/// Write the selected path to the shell integration's result file.
/// The file must already exist (the shell function creates it with mktemp).
pub(crate) fn write_result_file(result_file: &Path, selected: &str) -> io::Result<()> {
    use std::io::Write;
    std::fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(result_file)?
        .write_all(selected.as_bytes())
}

/// Open `file` with `editor` and wait for it to exit.
fn launch_editor(editor: &str, file: &str) -> Result<(), String> {
    if !Path::new(file).exists() {
        return Err(format!("'{}' no longer exists", file));
    }
    let status = Command::new(editor)
        .arg(file)
        .status()
        .map_err(|e| format!("failed to launch '{}': {}", editor, e))?;

    if !status.success() {
        return Err(format!("'{}' failed with {}", editor, status));
    }
    debug!("Opened {} with {}", file, editor);
    Ok(())
}

/// Open the selection in the configured editor, or copy its path to the clipboard.
/// Call after the terminal is restored; prints the path if the clipboard is unavailable.
pub fn handle_file_selection(file: &str, ide: Option<String>) -> Result<(), String> {
    if let Some(ide) = ide {
        return launch_editor(&ide, file);
    }

    match ClipboardContext::new().and_then(|mut ctx| ctx.set_contents(file.to_owned())) {
        Ok(()) => debug!("Copied path to clipboard: {}", file),
        Err(e) => {
            warn!("Clipboard unavailable: {}", e);
            eprintln!("ff: clipboard unavailable; path printed below");
            println!("{}", file);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::background::tests::{assert_copy_finished, copy_progress, running_copy_app};

    #[test]
    fn confirmed_quit_cancels_copy_and_waits_for_the_final_message() {
        use std::sync::atomic::Ordering;
        use std::time::Duration;

        for key in ["q", "o", "Enter"] {
            let mut app = running_copy_app();
            let cancel = app.copy_cancel.clone();
            let mut status = StatusBar::new();
            let mut terminal = Terminal::new(ratatui::backend::TestBackend::new(80, 6)).unwrap();
            let mut quit_error = None;
            let (tx, rx) = mpsc::channel();
            let mut receiver = Some(rx);
            assert!(!may_exit(
                &mut app,
                &mut status,
                key,
                &mut receiver,
                &mut terminal,
                &mut quit_error
            ));
            assert!(!cancel.load(Ordering::Relaxed));

            let (returned_tx, returned_rx) = mpsc::channel();
            let waiter = std::thread::spawn(move || {
                let allowed = may_exit(
                    &mut app,
                    &mut status,
                    key,
                    &mut receiver,
                    &mut terminal,
                    &mut quit_error,
                );
                assert_copy_finished(&app, &receiver);
                returned_tx
                    .send((
                        allowed,
                        app.copy_in_progress,
                        receiver.is_none(),
                        quit_error,
                    ))
                    .unwrap();
            });
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            while !cancel.load(Ordering::Relaxed) && std::time::Instant::now() < deadline {
                std::thread::yield_now();
            }
            assert!(cancel.load(Ordering::Relaxed));
            tx.send(copy_progress()).unwrap();
            assert!(matches!(
                returned_rx.recv_timeout(Duration::from_millis(25)),
                Err(mpsc::RecvTimeoutError::Timeout)
            ));
            let error =
                "Copy cancelled (failed to remove partial copy 'leftover': permission denied)";
            tx.send(CopyMessage::Error(error.into())).unwrap();
            let result = returned_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            assert_eq!(result, (true, false, true, Some(error.into())));
            waiter.join().unwrap();
        }
    }

    #[test]
    fn system_opener_matches_platform() {
        #[cfg(target_os = "macos")]
        assert_eq!(system_opener(), "open");
        #[cfg(not(target_os = "macos"))]
        assert_eq!(system_opener(), "xdg-open");
    }

    #[test]
    fn write_result_file_truncates_existing_file() {
        let result_file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(result_file.path(), "/a/much/longer/previous/path").unwrap();

        write_result_file(result_file.path(), "/tmp/selected").unwrap();

        assert_eq!(
            std::fs::read_to_string(result_file.path()).unwrap(),
            "/tmp/selected"
        );
    }

    #[test]
    fn write_result_file_does_not_create_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("ff_result");

        assert!(write_result_file(&missing, "/tmp/selected").is_err());
        assert!(!missing.exists());
    }

    #[test]
    fn launch_editor_reports_missing_editor() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let file = file.path().to_str().unwrap();

        let err = launch_editor("ff-definitely-not-installed-xyz", file).unwrap_err();

        assert!(err.starts_with("failed to launch 'ff-definitely-not-installed-xyz': "));
    }

    #[test]
    fn launch_editor_reports_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("gone.txt");
        let missing = missing.to_str().unwrap();

        let err = launch_editor("ff-definitely-not-installed-xyz", missing).unwrap_err();

        assert_eq!(err, format!("'{}' no longer exists", missing));
    }

    #[cfg(unix)]
    #[test]
    fn launch_editor_reports_non_zero_exit() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let file = file.path().to_str().unwrap();

        // `false` ignores its arguments and exits 1; `true` exits 0. Neither opens the file.
        let err = launch_editor("false", file).unwrap_err();

        assert_eq!(err, "'false' failed with exit status: 1");
        assert_eq!(launch_editor("true", file), Ok(()));
    }
}
