//! Per-iteration polling of background work: previews, images, copies, moves,
//! deletes, file system events and the directory cache build.

use std::{
    path::{Path, PathBuf},
    sync::mpsc,
};

use log::{debug, warn};
use ratatui::widgets::ListState;

use crate::{
    app::{App, InputMode, PanePosition},
    directory_store::DirectoryIndex,
    event::file_actions::current_refresh_directory,
    file_reader_content::{self, FileContent, FileType},
    operations::{CopyMessage, DeleteMessage},
    render::image_renderer::ImageRenderer,
    status_bar::StatusBar,
    utils::{format_file_size, get_file_path_data, read_listing, SortBy},
};

/// Preview debounce delay in milliseconds
/// Prevents rapid preview updates when scrolling quickly through files
const PREVIEW_DEBOUNCE_MS: u64 = 50;

/// Check for debounced preview updates - use cache or spawn async loader
pub fn start_debounced_preview(
    app: &mut App,
    file_reader_content: &mut FileContent,
    image_renderer: &mut ImageRenderer,
) {
    if let Some(path) = app.take_ready_preview(PREVIEW_DEBOUNCE_MS) {
        // Results still loading for the previous selection must not overwrite this one
        app.cancel_preview_loads();

        // Check if this file is cached (for source code files with syntax highlighting)
        let is_file_path = std::path::Path::new(&path).is_file();
        let file_type = file_reader_content::detect_file_type(&path);
        let is_cacheable = matches!(
            file_type,
            FileType::SourceCode
                | FileType::ConfigFile
                | FileType::JSON
                | FileType::Markdown
                | FileType::TextFile
        );

        if is_file_path && is_cacheable && file_reader_content.is_cached(&path) {
            // Cache hit - use cached content directly (instant)
            file_reader_content.file_type = file_type;
            if file_reader_content.try_use_cached_preview(&path) {
                app.preview_file_content.clear();
                app.preview_files = Vec::new();
                image_renderer.clear();
            }
        } else {
            // Cache miss - use async loading
            // Update file type and clear all previous state to avoid showing stale preview
            file_reader_content.file_type = file_type;
            file_reader_content.hightlighted_content = None;
            file_reader_content.curr_selected_path = path.clone();
            app.preview_loading = true;
            app.preview_loading_path = Some(path.clone());
            app.preview_file_content = "Loading...".to_string();
            app.preview_files = Vec::new();
            image_renderer.clear();

            // Create channel for async preview result
            let (tx, rx) = mpsc::channel();
            app.preview_receiver = Some(rx);

            // Spawn thread to load preview
            let path_for_thread = path.clone();
            std::thread::spawn(move || {
                file_reader_content::load_preview_async(path_for_thread, tx);
            });
        }
    }
}

/// Check for async preview results
pub fn poll_preview_result(
    app: &mut App,
    file_reader_content: &mut FileContent,
    image_renderer: &mut ImageRenderer,
) {
    if let Some(ref rx) = app.preview_receiver {
        match rx.try_recv() {
            Ok(msg) => {
                use crate::app::PreviewMessage;
                match msg {
                    PreviewMessage::Loaded {
                        path,
                        content,
                        file_type,
                        ..
                    } => {
                        // Only update if this is still the file we're waiting for
                        if app.preview_loading_path.as_ref() == Some(&path) {
                            app.preview_loading = false;
                            file_reader_content.file_type = file_type.clone();

                            // For text-based files, apply syntax highlighting or markdown rendering
                            match file_type {
                                FileType::SourceCode | FileType::ConfigFile | FileType::JSON => {
                                    // Set current path BEFORE calling get_highlighted_content (used for cache lookup)
                                    file_reader_content.curr_selected_path = path.clone();
                                    let ext_type =
                                        file_reader_content.get_file_extension_type(path.clone());
                                    let highlighted = file_reader_content
                                        .get_highlighted_content(content.clone(), ext_type);
                                    app.preview_file_content = highlighted;
                                }
                                FileType::Markdown => {
                                    // Render markdown with formatting
                                    file_reader_content.render_markdown_content(&content, &path);
                                    app.preview_file_content.clear();
                                }
                                FileType::Image => {
                                    // Spawn async image loading to avoid blocking UI
                                    file_reader_content.hightlighted_content = None;
                                    file_reader_content.curr_asset_path = path.clone();
                                    app.preview_file_content = "Loading image...".to_string();

                                    // Create channel for async image result
                                    let (img_tx, img_rx) = mpsc::channel();
                                    app.image_receiver = Some(img_rx);

                                    // Spawn thread to decode image
                                    let path_for_thread = path;
                                    std::thread::spawn(move || {
                                        file_reader_content::load_image_async(
                                            path_for_thread,
                                            img_tx,
                                        );
                                    });
                                }
                                FileType::Binary => {
                                    file_reader_content.hightlighted_content = None;
                                    app.preview_file_content =
                                        file_reader_content.read_binary_hex_view(&path);
                                }
                                FileType::ZIP => {
                                    file_reader_content.hightlighted_content = None;
                                    file_reader_content.read_zip_content(path);
                                }
                                FileType::Archive => {
                                    file_reader_content.hightlighted_content = None;
                                    // Determine archive type and read
                                    let ext = std::path::Path::new(&path)
                                        .extension()
                                        .and_then(|e| e.to_str())
                                        .unwrap_or("")
                                        .to_lowercase();
                                    let entries = match ext.as_str() {
                                        "tar" => file_reader_content.read_tar_content(&path),
                                        "tgz" => file_reader_content.read_tar_gz_content(&path),
                                        "gz" if path.to_lowercase().ends_with(".tar.gz") => {
                                            file_reader_content.read_tar_gz_content(&path)
                                        }
                                        "bz2" | "tbz2"
                                            if path.to_lowercase().ends_with(".tar.bz2") =>
                                        {
                                            file_reader_content.read_tar_bz2_content(&path)
                                        }
                                        "xz" | "txz"
                                            if path.to_lowercase().ends_with(".tar.xz") =>
                                        {
                                            file_reader_content.read_tar_xz_content(&path)
                                        }
                                        _ => vec!["Archive preview not supported".to_string()],
                                    };
                                    app.preview_file_content = entries.join("\n");
                                }
                                FileType::PDF => {
                                    // PDF content is now loaded asynchronously
                                    file_reader_content.hightlighted_content = None;
                                    app.preview_file_content = content;
                                }
                                FileType::TextFile => {
                                    // Plain text - clear any previous highlighted content
                                    file_reader_content.hightlighted_content = None;
                                    app.preview_file_content = content;
                                }
                                _ => {
                                    // Other file types - clear highlighted content
                                    file_reader_content.hightlighted_content = None;
                                    app.preview_file_content = content;
                                }
                            }
                            app.preview_files = Vec::new();
                        }
                    }
                    PreviewMessage::DirectoryListing { path, entries } => {
                        if app.preview_loading_path.as_ref() == Some(&path) {
                            app.preview_loading = false;
                            app.preview_files = entries;
                            app.preview_file_content.clear();
                            file_reader_content.file_type = FileType::NotAvailable;
                            image_renderer.clear();
                        }
                    }
                    PreviewMessage::Error { path, message } => {
                        if app.preview_loading_path.as_ref() == Some(&path) {
                            app.preview_loading = false;
                            app.preview_file_content = format!("Error: {}", message);
                            file_reader_content.file_type = FileType::NotAvailable;
                        }
                    }
                }
                app.preview_receiver = None;
            }
            Err(mpsc::TryRecvError::Empty) => {
                // Still loading, keep waiting
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                // Thread finished unexpectedly
                app.preview_loading = false;
                app.preview_receiver = None;
            }
        }
    }
}

/// Check for async image loading results
pub fn poll_image_result(
    app: &mut App,
    file_reader_content: &FileContent,
    image_renderer: &mut ImageRenderer,
) {
    if let Some(ref rx) = app.image_receiver {
        match rx.try_recv() {
            Ok(result) => {
                // Only update if this is still the file we're waiting for
                if file_reader_content.curr_asset_path == result.path {
                    match result.result {
                        Ok((decoded_image, format)) => {
                            image_renderer.load_from_decoded(decoded_image, format);
                            app.preview_file_content.clear();
                        }
                        Err(error_msg) => {
                            image_renderer.set_error(error_msg);
                            app.preview_file_content.clear();
                        }
                    }
                }
                app.image_receiver = None;
            }
            Err(mpsc::TryRecvError::Empty) => {
                // Still loading, keep waiting
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                // Thread finished unexpectedly
                app.image_receiver = None;
            }
        }
    }
}

/// Drain copy updates, report terminal results, and return whether a copy succeeded.
fn poll_copy_messages(
    app: &mut App,
    status_bar: &mut StatusBar,
    receiver: &mut Option<mpsc::Receiver<CopyMessage>>,
) -> bool {
    let Some(rx) = receiver.as_ref() else {
        return false;
    };
    // Coalesce status/progress updates, but never discard a terminal result.
    let mut last_message = None;
    loop {
        match rx.try_recv() {
            Ok(message) => {
                let finished = matches!(
                    message,
                    CopyMessage::Completed { .. } | CopyMessage::Error(_)
                );
                last_message = Some(message);
                if finished {
                    break;
                }
            }
            Err(mpsc::TryRecvError::Empty) => break,
            Err(mpsc::TryRecvError::Disconnected) => {
                last_message = Some(CopyMessage::Error(
                    "Copy stopped unexpectedly before reporting a result".into(),
                ));
                break;
            }
        }
    }

    let success = match last_message {
        None => return false,
        Some(CopyMessage::Status(message)) => {
            app.copy_progress_message = message;
            return false;
        }
        Some(CopyMessage::Progress {
            files_processed,
            total_files,
            bytes_copied,
            total_bytes,
            current_file,
        }) => {
            app.copy_files_processed = files_processed;
            app.copy_total_files = total_files;
            let percent = if total_bytes > 0 {
                (bytes_copied as f64 / total_bytes as f64 * 100.0) as u8
            } else {
                0
            };
            app.copy_progress_message = format!(
                "Copying: {} ({}% - {} / {})",
                current_file,
                percent,
                format_file_size(bytes_copied),
                format_file_size(total_bytes),
            );
            return false;
        }
        Some(CopyMessage::Completed {
            success,
            message,
            skipped_special_files,
        }) => {
            // The engine names the destination, which a copy to another folder shares
            // with its source, so name the folder instead.
            let message = app
                .copy_transfer
                .as_ref()
                .and_then(|(name, destination)| {
                    let details = message.strip_prefix(&format!("Copied '{name}' to '{name}'"))?;
                    Some(format!("Copied '{name}' to {destination}{details}"))
                })
                .unwrap_or(message);
            if success {
                debug!("Copy completed: {}", message);
                if skipped_special_files > 0 {
                    status_bar.show_warning(message, None);
                } else {
                    status_bar.show_info(message, None);
                }
            } else {
                warn!("Copy failed: {}", message);
                status_bar.show_error(message, None);
            }
            success
        }
        Some(CopyMessage::Error(message)) => {
            warn!("Copy operation failed: {}", message);
            status_bar.show_error(message, None);
            false
        }
    };

    app.copy_in_progress = false;
    app.copy_progress_message.clear();
    app.copy_files_processed = 0;
    app.copy_total_files = 0;
    app.copy_transfer = None;
    app.copy_destination = None;
    *receiver = None;
    success
}

/// Refresh completed copies and select an in-place duplicate when visible. Any finished
/// copy re-reads its destination into the folder index: a failed one removes what it
/// wrote, but a background refresh may have seen it meanwhile.
pub fn refresh_after_copy(
    app: &mut App,
    state: &mut ListState,
    status_bar: &mut StatusBar,
    receiver: &mut Option<mpsc::Receiver<CopyMessage>>,
    index: &mut DirectoryIndex,
    fallback_start_path: &str,
) {
    let copied = app.copy_destination.clone();
    let destination = copied
        .as_ref()
        .filter(|_| app.copy_transfer.is_none())
        .map(|path| path.to_string_lossy().into_owned());
    let succeeded = poll_copy_messages(app, status_bar, receiver);
    if let Some(copied) = copied.as_ref().filter(|_| receiver.is_none()) {
        index.changed(copied);
        app.forget_sizes([copied.as_path()]);
    }
    if succeeded {
        let refresh_dir = current_refresh_directory(app, fallback_start_path);
        if let Ok(listing) = read_listing(
            refresh_dir,
            app.show_hidden_files,
            app.sort_by.clone(),
            &app.sort_type,
        ) {
            app.refresh_files_after_operation(listing, state, destination);
        }
        // Shift+C may have copied into the right pane's folder.
        if app.view_mode.is_dual_pane() {
            app.right_pane.refresh_files();
        }
    }
}

/// A finished move, extraction or compression may have changed both panes and the folder
/// index, even if it failed partway. On success, highlight what it made, if it says.
pub fn poll_job(
    app: &mut App,
    state: &mut ListState,
    status_bar: &mut StatusBar,
    index: &mut DirectoryIndex,
    fallback_start_path: &str,
) {
    let select = app.job.as_ref().and_then(|job| job.select.clone());
    if let Some((result, changes)) = app.poll_job() {
        index.changed_all(changes.iter().map(PathBuf::as_path));
        // A compression changes nothing the index holds, only the archive's folder.
        let made = select.iter().map(|(_, path)| Path::new(path));
        app.forget_sizes(changes.iter().map(PathBuf::as_path).chain(made));
        let (select_left, select_right) = match select.filter(|_| result.is_ok()) {
            Some((PanePosition::Right, path)) => (None, Some(path)),
            other => (other.map(|(_, path)| path), None),
        };
        let refresh_dir = current_refresh_directory(app, fallback_start_path);
        if let Ok(listing) = read_listing(
            refresh_dir,
            app.show_hidden_files,
            app.sort_by.clone(),
            &app.sort_type,
        ) {
            if result.is_ok() {
                app.refresh_files_after_operation(listing, state, select_left);
            } else {
                app.refresh_files_from_watcher(listing, state);
            }
        }
        if app.view_mode.is_dual_pane() {
            app.right_pane.refresh_files();
            if let Some(path) = select_right {
                app.right_pane.select_path(&path);
            }
        }
        match result {
            Ok(message) => status_bar.show_info(message, None),
            Err(message) => {
                warn!("Background operation failed: {}", message);
                status_bar.show_error(message, None);
            }
        }
    }
}

/// Handle delete progress messages
pub fn poll_delete(
    app: &mut App,
    state: &mut ListState,
    status_bar: &mut StatusBar,
    delete_receiver: &mut Option<mpsc::Receiver<DeleteMessage>>,
    index: &mut DirectoryIndex,
    fallback_start_path: &str,
) {
    if let Some(ref rx) = *delete_receiver {
        let mut last_msg: Option<DeleteMessage> = None;
        loop {
            match rx.try_recv() {
                Ok(msg) => last_msg = Some(msg),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    // A worker that dies without a result must not block later operations.
                    if !matches!(
                        last_msg,
                        Some(DeleteMessage::Completed { .. } | DeleteMessage::Error(_))
                    ) {
                        last_msg = Some(DeleteMessage::Error(
                            "Delete stopped unexpectedly before reporting a result".into(),
                        ));
                    }
                    *delete_receiver = None;
                    break;
                }
            }
        }

        if let Some(msg) = last_msg {
            // Even a failed delete may have removed some folders.
            if matches!(
                msg,
                DeleteMessage::Completed { .. } | DeleteMessage::Error(_)
            ) {
                if let Some(path) = app.delete_path.take() {
                    index.changed(Path::new(&path));
                    app.forget_sizes([Path::new(&path)]);
                }
            }
            match msg {
                DeleteMessage::Counting { files_counted } => {
                    app.delete_progress_message = format!("Counting files... {}", files_counted);
                }
                DeleteMessage::Progress {
                    files_deleted,
                    total_files,
                    current_file,
                } => {
                    let percent = if total_files > 0 {
                        (files_deleted as f64 / total_files as f64 * 100.0) as u8
                    } else {
                        0
                    };
                    app.delete_progress_message = format!(
                        "Deleting: {} ({}% - {}/{})",
                        current_file, percent, files_deleted, total_files
                    );
                }
                DeleteMessage::Completed { success, message } => {
                    if success {
                        debug!("Delete completed: {}", message);
                    } else {
                        warn!("Delete failed: {}", message);
                        status_bar.show_error(message.clone(), None);
                    }

                    // Refresh the directory being browsed, even for a global search target.
                    let refresh_dir = current_refresh_directory(app, fallback_start_path);
                    if let Ok(listing) = read_listing(
                        refresh_dir,
                        app.show_hidden_files,
                        app.sort_by.clone(),
                        &app.sort_type,
                    ) {
                        if success {
                            app.refresh_files_after_operation(listing, state, None);
                        } else {
                            app.refresh_files_from_watcher(listing, state);
                        }
                    }
                    if app.view_mode.is_dual_pane() {
                        app.right_pane.refresh_files();
                    }

                    // Reset delete state
                    app.delete_in_progress = false;
                    app.delete_progress_message.clear();
                    app.preserved_selection_index = None;
                    *delete_receiver = None;
                }
                DeleteMessage::Error(error_msg) => {
                    warn!("Delete operation failed: {}", error_msg);
                    status_bar.show_error(error_msg.clone(), None);

                    // Reset delete state
                    app.delete_in_progress = false;
                    app.delete_progress_message = format!("Delete failed: {}", error_msg);
                    app.preserved_selection_index = None;
                    *delete_receiver = None;
                }
            }
        }
    }
}

/// Show the latest folder size count in the Get Info popup. It runs beside other
/// operations rather than as a job, since it changes nothing.
pub fn poll_info(app: &mut App) {
    let Some(info) = app.info.as_mut() else {
        return;
    };
    let Some(receiver) = &info.folder_count else {
        return;
    };
    let mut latest = None;
    let disconnected = loop {
        match receiver.try_recv() {
            Ok(count) => latest = Some(count),
            Err(mpsc::TryRecvError::Empty) => break false,
            Err(mpsc::TryRecvError::Disconnected) => break true,
        }
    };
    if let Some(count) = latest {
        info.show_folder_count(&count);
        if count.finished {
            info.folder_count = None;
            return;
        }
    }
    if disconnected {
        info.folder_count = None;
        info.set_row("Size", "Couldn't finish calculating the size".into());
        info.set_row("Contents", "Unknown".into());
    }
}

/// Process file system events
pub fn poll_file_system_events(app: &mut App, state: &mut ListState, fallback_start_path: &str) {
    // The right pane watches its own folder, even when it's the same as the left's.
    let changed = app.right_pane.poll_watcher();
    if !changed.is_empty() {
        app.forget_sizes(changed.iter().map(PathBuf::as_path));
    }
    let fs_events = app.process_file_system_events();
    if !fs_events.is_empty() {
        // File system changes detected, refresh the CURRENT directory (not the start path)
        let refresh_dir = current_refresh_directory(app, fallback_start_path);

        if let Ok(listing) = read_listing(
            refresh_dir,
            app.show_hidden_files,
            app.sort_by.clone(),
            &app.sort_type,
        ) {
            app.refresh_files_from_watcher(listing, state);

            // Log the file system events for debugging
            for event_msg in fs_events {
                debug!("File system event: {}", event_msg);
            }
        }
    }
}

/// Size the folders of each listing sorted by size, and move each into place once it's
/// sized. A listing being selected in, searched, or behind a prompt or popup keeps its
/// order until Normal mode. Runs before drawing, so no frame shows a listing unsorted.
pub fn poll_sizes(app: &mut App, state: &mut ListState) {
    let right_shown = app.view_mode.is_dual_pane();
    let left = (app.sort_by == SortBy::Size).then(|| app.left_directory_path());
    let right = (right_shown && app.right_pane.sort_by == SortBy::Size)
        .then(|| PathBuf::from(&app.right_pane.current_directory));
    app.sizing
        .follow(left.clone(), right.as_deref(), &mut app.folder_sizes);
    app.right_pane
        .sizing
        .follow(right, left.as_deref(), &mut app.folder_sizes);

    let arrived = app.folder_sizes.poll();
    app.sizing
        .update(&app.files, &app.file_info, &mut app.folder_sizes, arrived);
    let pane = &mut app.right_pane;
    pane.sizing
        .update(&pane.files, &pane.file_info, &mut app.folder_sizes, arrived);

    let right_active = right_shown && app.is_right_pane_active();
    let normal = app.input_mode == InputMode::Normal;
    if (normal || right_active)
        && app.search_results.is_empty()
        && app
            .sizing
            .take_resort(&app.files, &app.file_info, &app.folder_sizes)
    {
        app.resort_by_size(state);
    }
    let pane = &mut app.right_pane;
    if (normal || !right_active)
        && !pane.is_searching()
        && pane
            .sizing
            .take_resort(&pane.files, &pane.file_info, &app.folder_sizes)
    {
        pane.resort_by_size();
    }
}

/// Process cache loading progress
pub fn poll_cache_loading(app: &mut App, index: &mut DirectoryIndex, fallback_start_path: &str) {
    if app.input_mode == InputMode::CacheLoading && app.process_cache_loading_progress() {
        // Cache loading is complete, finish setup
        debug!("Cache loading completed, finalizing...");

        // Use the completed cache if available, otherwise fallback to current directory
        // Capture the completed cache and persist it; also update the in-memory store used for global search
        let completed_directories = if let Some(ref cache_store) = app.completed_cache_store {
            debug!(
                "Using completed cache with {} directories",
                cache_store.directories.len()
            );

            // Update the in-memory store so global search starts working immediately
            index.store = cache_store.clone();

            // Save the cache to file for future use
            index.save();
            if let Some(ms) = app.cache_build_elapsed_ms {
                debug!("Directory cache built in {} ms", ms);
            }

            // We no longer replace the current file list with the entire cache; keep current directory view.
            // Still return directories for compatibility (ignored by finish_cache_loading).
            cache_store.directories.clone()
        } else {
            debug!("No completed cache available, falling back to current directory");
            // Fallback to current directory files
            get_file_path_data(
                fallback_start_path.to_string(),
                false,
                app.sort_by.clone(),
                &app.sort_type,
            )
            .unwrap_or_else(|_| Vec::new())
        };

        // Finalize cache loading without changing the current directory view
        app.finish_cache_loading(completed_directories);

        debug!("Cache loading finished, app is now in normal mode");
    }
}

/// Apply preserved selection and clamp after all refreshes, before handling a key.
pub fn sync_selection(
    app: &mut App,
    state: &mut ListState,
    file_reader_content: &mut FileContent,
    image_renderer: &mut ImageRenderer,
) {
    let selected_index = app.preserved_selection_index.take().or(state.selected());
    let valid_index = app
        .get_list_length()
        .checked_sub(1)
        .map(|last_index| selected_index.unwrap_or(0).min(last_index));
    state.select(valid_index);
    app.curr_index = valid_index;
    // Keep the preview on the selected item, however the selection changed.
    if app.sync_preview_with_selection(valid_index) {
        file_reader_content.file_type = FileType::NotAvailable;
        file_reader_content.hightlighted_content = None;
        file_reader_content.curr_selected_path.clear();
        image_renderer.clear();
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::app::ViewMode;
    use crate::operations::copy_dir_file_with_progress_cancellable;
    use crate::status_bar::NotificationKind;
    use crate::utils::{generate_copy_file_dir_name, SortBy, SortType};
    use std::path::PathBuf;

    /// A folder index over `root` that is never saved.
    pub(crate) fn memory_index(root: &std::path::Path) -> DirectoryIndex {
        DirectoryIndex::new(
            crate::directory_store::DirectoryStore::new(),
            root.to_path_buf(),
            Vec::new(),
            None,
        )
    }

    pub(crate) fn running_copy_app() -> App {
        let mut app = App::new(Vec::new());
        app.start_copy(PathBuf::from("copy_target"), None);
        app.copy_progress_message = "Copying...".into();
        app.copy_files_processed = 2;
        app.copy_total_files = 10;
        app
    }

    pub(crate) fn assert_copy_finished(app: &App, receiver: &Option<mpsc::Receiver<CopyMessage>>) {
        assert!(!app.copy_in_progress);
        assert!(app.copy_progress_message.is_empty());
        assert_eq!(app.copy_files_processed, 0);
        assert_eq!(app.copy_total_files, 0);
        assert!(app.copy_destination.is_none());
        assert!(receiver.is_none());
    }

    pub(crate) fn copy_progress() -> CopyMessage {
        CopyMessage::Progress {
            files_processed: 3,
            total_files: 10,
            bytes_copied: 30,
            total_bytes: 100,
            current_file: "data.txt".into(),
        }
    }

    #[test]
    fn copy_refresh_selects_successful_destination_and_updates_preview_and_metadata() {
        for fail in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let source = dir.path().join("zulu.txt");
            std::fs::write(&source, "copy contents").unwrap();
            std::fs::write(dir.path().join("alpha.txt"), "existing").unwrap();
            std::fs::write(dir.path().join("middle.txt"), "existing").unwrap();
            let destination = generate_copy_file_dir_name(&source, dir.path());
            if fail {
                std::fs::write(&destination, "existing destination").unwrap();
            }
            let directory = dir.path().to_string_lossy().into_owned();
            let files =
                get_file_path_data(directory.clone(), false, SortBy::Name, &SortType::ASC).unwrap();
            let mut app = App::new(files);
            app.current_directory = directory.clone();
            app.sort_by = SortBy::Name;
            app.sort_type = SortType::ASC;
            let mut state = ListState::default();
            state.select(Some(app.files.len() - 1));
            app.curr_index = state.selected();
            app.preserved_selection_index = state.selected();
            app.sync_preview_with_selection(state.selected());
            app.start_copy(destination.clone(), None);
            let mut receiver = Some(copy_dir_file_with_progress_cancellable(
                &source,
                &destination,
                app.copy_cancel.clone(),
            ));
            let mut status = StatusBar::new();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while receiver.is_some() {
                refresh_after_copy(
                    &mut app,
                    &mut state,
                    &mut status,
                    &mut receiver,
                    &mut memory_index(dir.path()),
                    &directory,
                );
                assert!(std::time::Instant::now() < deadline, "copy did not finish");
                std::thread::yield_now();
            }

            let expected = if fail { &source } else { &destination }
                .to_string_lossy()
                .into_owned();
            assert_eq!(
                app.get_selected_path(state.selected()),
                Some(expected.clone())
            );
            assert_eq!(app.curr_index, state.selected());
            assert_copy_finished(&app, &receiver);
            assert_eq!(
                std::fs::read_to_string(&destination).unwrap(),
                if fail {
                    "existing destination"
                } else {
                    "copy contents"
                },
            );
            // The main loop synchronizes previews after refreshing all operation results.
            app.sync_preview_with_selection(state.selected());
            assert_eq!(app.take_ready_preview(0), Some(expected));
            status.update(&app);
            assert!(status.selected_item_info.starts_with(if fail {
                "zulu.txt |"
            } else {
                "copy_zulu.txt |"
            }));
        }
    }

    #[test]
    fn copy_refresh_preserves_selection_after_navigation_or_cross_pane_copy() {
        for cross_pane in [false, true] {
            let browsed = tempfile::tempdir().unwrap();
            let other = tempfile::tempdir().unwrap();
            for directory in [browsed.path(), other.path()] {
                for name in ["alpha.txt", "copy_target.txt", "zulu.txt"] {
                    std::fs::write(directory.join(name), "contents").unwrap();
                }
            }
            let directory = browsed.path().to_string_lossy().into_owned();
            let files =
                get_file_path_data(directory.clone(), false, SortBy::Name, &SortType::ASC).unwrap();
            let mut app = App::new(files);
            app.current_directory = directory.clone();
            app.sort_by = SortBy::Name;
            app.sort_type = SortType::ASC;
            app.view_mode = ViewMode::DualPane;
            app.right_pane =
                crate::pane::Pane::new(other.path().to_string_lossy().into_owned(), Vec::new());
            app.right_pane.selected_index = Some(2);
            app.right_pane.list_state.select(Some(2));
            let mut state = ListState::default();
            state.select(Some(0));
            app.preserved_selection_index = state.selected();
            // A right-to-left Shift+C keeps selection even when its destination is visible.
            // A local copy finishing after navigation must not select an unrelated namesake.
            let destination_dir = if cross_pane {
                browsed.path()
            } else {
                other.path()
            };
            app.start_copy(
                destination_dir.join("copy_target.txt"),
                cross_pane.then(|| ("copy_target.txt".into(), "other pane".into())),
            );
            let (tx, rx) = mpsc::channel();
            tx.send(CopyMessage::Completed {
                success: true,
                message: "Copied".into(),
                skipped_special_files: 0,
            })
            .unwrap();
            let mut receiver = Some(rx);

            refresh_after_copy(
                &mut app,
                &mut state,
                &mut StatusBar::new(),
                &mut receiver,
                &mut memory_index(browsed.path()),
                &directory,
            );

            assert_eq!(state.selected(), Some(0));
            assert_eq!(
                app.get_selected_path(state.selected()),
                Some(
                    browsed
                        .path()
                        .join("alpha.txt")
                        .to_string_lossy()
                        .into_owned()
                )
            );
            assert_eq!(app.right_pane.list_state.selected(), Some(2));
            assert_copy_finished(&app, &receiver);
        }
    }

    #[test]
    fn copy_completion_notifies_with_severity_from_skipped_count() {
        for (skipped_special_files, kind) in
            [(0, NotificationKind::Info), (2, NotificationKind::Warning)]
        {
            let mut app = running_copy_app();
            let mut status = StatusBar::new();
            let (tx, rx) = mpsc::channel();
            tx.send(copy_progress()).unwrap();
            tx.send(CopyMessage::Completed {
                success: true,
                message: "Copied 'data' to 'copy_data'".into(),
                skipped_special_files,
            })
            .unwrap();
            // Even a late progress update must not hide the final result.
            tx.send(copy_progress()).unwrap();
            drop(tx);
            let mut receiver = Some(rx);

            assert!(poll_copy_messages(&mut app, &mut status, &mut receiver));
            assert!(status.has_error());
            assert_eq!(status.notification_kind, kind);
            assert!(status
                .error_message
                .as_deref()
                .unwrap()
                .contains("copy_data"));
            assert_copy_finished(&app, &receiver);
        }
    }

    #[test]
    fn copy_to_other_pane_names_the_pane_and_keeps_details() {
        for (skipped_special_files, details) in [
            (0, ""),
            (1, " (skipped 1 special file: sockets/pipes/devices)"),
        ] {
            let mut app = running_copy_app();
            app.copy_transfer = Some(("data".into(), "other pane".into()));
            let mut status = StatusBar::new();
            let (tx, rx) = mpsc::channel();
            tx.send(CopyMessage::Completed {
                success: true,
                message: format!("Copied 'data' to 'data'{details}"),
                skipped_special_files,
            })
            .unwrap();
            let mut receiver = Some(rx);

            assert!(poll_copy_messages(&mut app, &mut status, &mut receiver));
            assert_eq!(
                status.error_message,
                Some(format!("Copied 'data' to other pane{details}"))
            );
            assert_eq!(app.copy_transfer, None);
            assert_copy_finished(&app, &receiver);
        }
    }

    #[test]
    fn copy_failures_notify_with_reason_and_clear_progress() {
        for message in [
            CopyMessage::Error("permission denied".into()),
            CopyMessage::Completed {
                success: false,
                message: "permission denied".into(),
                skipped_special_files: 0,
            },
        ] {
            let mut app = running_copy_app();
            let mut status = StatusBar::new();
            let (tx, rx) = mpsc::channel();
            tx.send(message).unwrap();
            let mut receiver = Some(rx);

            assert!(!poll_copy_messages(&mut app, &mut status, &mut receiver));
            assert_eq!(status.notification_kind, NotificationKind::Error);
            assert_eq!(status.error_message.as_deref(), Some("permission denied"));
            assert_copy_finished(&app, &receiver);
        }
    }

    #[test]
    fn copy_disconnect_without_result_reports_error_even_after_progress() {
        for send_progress in [false, true] {
            let mut app = running_copy_app();
            let mut status = StatusBar::new();
            let (tx, rx) = mpsc::channel();
            if send_progress {
                tx.send(copy_progress()).unwrap();
            }
            drop(tx);
            let mut receiver = Some(rx);

            assert!(!poll_copy_messages(&mut app, &mut status, &mut receiver));
            assert_eq!(status.notification_kind, NotificationKind::Error);
            assert!(status
                .error_message
                .as_deref()
                .unwrap()
                .contains("Copy stopped unexpectedly"));
            assert_copy_finished(&app, &receiver);
        }
    }

    #[test]
    fn copy_progress_and_empty_channel_keep_copy_running() {
        let mut app = running_copy_app();
        let mut status = StatusBar::new();
        let (tx, rx) = mpsc::channel();
        let mut receiver = Some(rx);
        assert!(!poll_copy_messages(&mut app, &mut status, &mut receiver));
        assert!(app.copy_in_progress);

        tx.send(copy_progress()).unwrap();
        assert!(!poll_copy_messages(&mut app, &mut status, &mut receiver));
        assert_eq!(app.copy_files_processed, 3);
        assert_eq!(app.copy_total_files, 10);
        assert!(app.copy_progress_message.contains("30%"));
        assert!(app.copy_in_progress);
        assert!(receiver.is_some());
        assert!(!status.has_error());
    }

    #[test]
    fn copy_status_keeps_receiver_alive_and_yields_to_progress() {
        let mut app = running_copy_app();
        let mut status = StatusBar::new();
        let (tx, rx) = mpsc::channel();
        let mut receiver = Some(rx);
        tx.send(CopyMessage::Status("Cloning 'source'…".into()))
            .unwrap();
        assert!(!poll_copy_messages(&mut app, &mut status, &mut receiver));
        assert_eq!(app.copy_progress_message, "Cloning 'source'…");
        assert!(app.copy_in_progress);
        assert!(receiver.is_some());
        assert!(!status.has_error());

        tx.send(copy_progress()).unwrap();
        assert!(!poll_copy_messages(&mut app, &mut status, &mut receiver));
        assert!(app.copy_progress_message.contains("30%"));
        assert!(app.copy_in_progress);
    }

    #[test]
    fn copy_status_does_not_hide_completion_or_disconnect() {
        for completed in [false, true] {
            let mut app = running_copy_app();
            let mut status = StatusBar::new();
            let (tx, rx) = mpsc::channel();
            let mut receiver = Some(rx);
            tx.send(CopyMessage::Status("Cloning 'source'…".into()))
                .unwrap();
            if completed {
                tx.send(CopyMessage::Completed {
                    success: true,
                    message: "Copied 'source' to 'copy_source'".into(),
                    skipped_special_files: 0,
                })
                .unwrap();
            }
            drop(tx);
            assert_eq!(
                poll_copy_messages(&mut app, &mut status, &mut receiver),
                completed
            );
            assert_copy_finished(&app, &receiver);
            assert_eq!(
                status.notification_kind,
                if completed {
                    NotificationKind::Info
                } else {
                    NotificationKind::Error
                }
            );
        }
    }

    fn indexed(index: &DirectoryIndex) -> Vec<String> {
        let mut dirs = index.store.directories.clone();
        dirs.sort();
        dirs
    }

    fn spelled(path: &std::path::Path) -> String {
        path.to_string_lossy().into_owned()
    }

    #[test]
    fn failed_copy_drops_folders_a_refresh_may_have_seen() {
        let dir = tempfile::tempdir().unwrap();
        let destination = dir.path().join("Projects/proj");
        std::fs::create_dir(dir.path().join("Projects")).unwrap();
        let mut index = memory_index(dir.path());
        // What a refresh walking the partial copy would have recorded.
        for path in [dir.path().join("Projects"), destination.clone()] {
            index.store.insert(&spelled(&path));
        }
        let mut app = running_copy_app();
        app.copy_destination = Some(destination);
        let (tx, rx) = mpsc::channel();
        tx.send(CopyMessage::Error(
            "Permission denied; partial copy removed".into(),
        ))
        .unwrap();
        let mut receiver = Some(rx);

        refresh_after_copy(
            &mut app,
            &mut ListState::default(),
            &mut StatusBar::new(),
            &mut receiver,
            &mut index,
            &spelled(dir.path()),
        );

        assert_copy_finished(&app, &receiver);
        assert_eq!(
            index.store.directories,
            [spelled(&dir.path().join("Projects"))]
        );
    }

    #[test]
    fn finished_copy_leaves_the_folder_index_matching_disk() {
        for exists in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let source = dir.path().join("proj");
            std::fs::create_dir_all(source.join("sub")).unwrap();
            let destination = dir.path().join("Projects/proj");
            std::fs::create_dir_all(&destination).unwrap();
            if !exists {
                std::fs::remove_dir(&destination).unwrap();
            }
            let mut index = memory_index(dir.path());
            index.changed(&source);
            index.changed(&dir.path().join("Projects"));
            let before = indexed(&index);
            let mut app = App::new(Vec::new());
            app.start_copy(
                destination.clone(),
                Some(("proj".into(), "'Projects'".into())),
            );
            let mut receiver = Some(copy_dir_file_with_progress_cancellable(
                &source,
                &destination,
                app.copy_cancel.clone(),
            ));
            let directory = spelled(dir.path());
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while receiver.is_some() {
                refresh_after_copy(
                    &mut app,
                    &mut ListState::default(),
                    &mut StatusBar::new(),
                    &mut receiver,
                    &mut index,
                    &directory,
                );
                assert!(std::time::Instant::now() < deadline, "copy did not finish");
                std::thread::yield_now();
            }

            if exists {
                assert_eq!(indexed(&index), before);
            } else {
                let mut expected = before.clone();
                expected.extend([spelled(&destination), spelled(&destination.join("sub"))]);
                expected.sort();
                assert_eq!(indexed(&index), expected);
            }
        }
    }

    #[test]
    fn finished_moves_and_deletes_re_read_their_folders() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("proj/sub")).unwrap();
        std::fs::create_dir_all(root.join("old/inner")).unwrap();
        std::fs::create_dir(root.join("Projects")).unwrap();
        let mut index = memory_index(root);
        for name in ["proj", "old", "Projects"] {
            index.changed(&root.join(name));
        }
        let directory = spelled(root);
        let mut app = App::new(Vec::new());
        app.current_directory = directory.clone();
        let mut state = ListState::default();
        let mut status = StatusBar::new();

        let (source, dest_dir) = (spelled(&root.join("proj")), spelled(&root.join("Projects")));
        app.start_job(
            "move",
            "Moving 'proj'…".into(),
            vec![root.join("proj"), root.join("Projects/proj")],
            move |cancel| {
                crate::operations::move_file_or_dir_cancellable(&source, &dest_dir, &cancel)
                    .map(|()| "Moved".to_string())
                    .map_err(|e| e.to_string())
            },
        )
        .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while app.job.is_some() {
            poll_job(&mut app, &mut state, &mut status, &mut index, &directory);
            assert!(std::time::Instant::now() < deadline, "move did not finish");
            std::thread::yield_now();
        }

        std::fs::remove_dir_all(root.join("old")).unwrap();
        app.delete_in_progress = true;
        app.delete_path = Some(spelled(&root.join("old")));
        let (tx, rx) = mpsc::channel();
        tx.send(DeleteMessage::Completed {
            success: true,
            message: "Deleted 2 items".into(),
        })
        .unwrap();
        poll_delete(
            &mut app,
            &mut state,
            &mut status,
            &mut Some(rx),
            &mut index,
            &directory,
        );

        assert_eq!(app.delete_path, None);
        let expected: Vec<String> = ["Projects", "Projects/proj", "Projects/proj/sub"]
            .iter()
            .map(|name| spelled(&root.join(name)))
            .collect();
        assert_eq!(indexed(&index), expected);
    }

    #[test]
    fn watcher_events_refresh_both_panes_showing_the_same_folder() {
        let dir = tempfile::tempdir().unwrap();
        let directory = dir.path().to_string_lossy().into_owned();
        std::fs::write(dir.path().join("b.txt"), "b").unwrap();
        let files =
            get_file_path_data(directory.clone(), false, SortBy::Name, &SortType::ASC).unwrap();
        let mut app = App::new(files);
        app.sort_by = SortBy::Name;
        app.start_watching_directory(&directory).unwrap();
        app.view_mode = ViewMode::DualPane;
        app.init_right_pane();
        let mut state = ListState::default().with_selected(Some(0));
        let highlighted = app.right_pane.get_selected_path();
        // Give the poll watchers a moment to read the folder first.
        std::thread::sleep(std::time::Duration::from_millis(100));

        let created = dir.path().join("a.txt").to_string_lossy().into_owned();
        std::fs::write(&created, "a").unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !(app.files.contains(&created) && app.right_pane.files.contains(&created))
            && std::time::Instant::now() < deadline
        {
            std::thread::sleep(std::time::Duration::from_millis(50));
            poll_file_system_events(&mut app, &mut state, &directory);
        }

        assert!(app.files.contains(&created));
        assert!(app.right_pane.files.contains(&created));
        assert_eq!(app.right_pane.get_selected_path(), highlighted);
        assert_eq!(app.current_directory, directory);
    }

    /// The root folder sorted by size, biggest first: big/ (2 GB, sparse), mid.txt
    /// (1000 B) and small/ (1 B), read before any folder is sized.
    fn size_ordered(root: &Path) -> (App, ListState) {
        for (folder, len) in [("big", 2 << 30), ("small", 1)] {
            std::fs::create_dir(root.join(folder)).unwrap();
            let data = std::fs::File::create(root.join(folder).join("data")).unwrap();
            data.set_len(len).unwrap();
        }
        std::fs::write(root.join("mid.txt"), "x".repeat(1000)).unwrap();
        let mut app = App::with_listing(size_listing(root));
        app.current_directory = spelled(root);
        app.sort_by = SortBy::Size;
        app.sort_type = SortType::DESC;
        (app, ListState::default().with_selected(Some(0)))
    }

    fn size_listing(root: &Path) -> crate::utils::Listing {
        read_listing(spelled(root), false, SortBy::Size, &SortType::DESC).unwrap()
    }

    fn names(files: &[String]) -> Vec<String> {
        files
            .iter()
            .map(|file| {
                Path::new(file)
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect()
    }

    /// Poll until every folder of the left listing is sized.
    fn size_left(app: &mut App, state: &mut ListState) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        poll_sizes(app, state);
        while app.sizing.here.is_none() {
            assert!(
                std::time::Instant::now() < deadline,
                "sizing did not finish"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
            poll_sizes(app, state);
        }
    }

    #[test]
    fn sized_folders_move_into_place_only_once_back_in_normal_mode() {
        let dir = tempfile::tempdir().unwrap();
        let (mut app, mut state) = size_ordered(dir.path());
        // Unsized folders come after every sized item.
        assert_eq!(names(&app.files)[0], "mid.txt");
        let before = app.files.clone();
        app.start_visual(state.selected()).unwrap();

        size_left(&mut app, &mut state);
        // Read again, as after a watcher event: still the order the selection was made in.
        app.refresh_files_from_watcher(size_listing(dir.path()), &mut state);
        poll_sizes(&mut app, &mut state);
        assert_eq!(app.sync_visual(&mut state), None);
        assert_eq!(app.input_mode, InputMode::Visual);
        assert_eq!(app.files, before);
        // The totals show in place meanwhile.
        let big = app.folder_sizes.total(&dir.path().join("big"));
        assert_eq!(big.map(|total| total.bytes), Some(2 << 30));
        let here = app.sizing.here.map(|here| here.bytes);
        assert_eq!(here, Some((2 << 30) + 1000 + 1));

        app.end_visual();
        poll_sizes(&mut app, &mut state);
        assert_eq!(names(&app.files), ["big", "mid.txt", "small"]);
        assert_eq!(app.read_only_files, app.files);
        // The highlight stayed on the same item.
        assert_eq!(
            app.get_selected_path(state.selected()),
            Some(spelled(&dir.path().join("mid.txt")))
        );
    }

    #[test]
    fn leaving_size_order_stops_sizing_and_drops_late_totals() {
        let dir = tempfile::tempdir().unwrap();
        let (mut app, mut state) = size_ordered(dir.path());
        poll_sizes(&mut app, &mut state);
        assert_eq!(app.sizing.progress, Some((0, 2)));
        // Let the walks finish: their totals wait for the next poll.
        std::thread::sleep(std::time::Duration::from_millis(200));

        app.sort_by = SortBy::Name;
        poll_sizes(&mut app, &mut state);

        assert_eq!(app.sizing.folder, None);
        assert_eq!(app.sizing.progress, None);
        for folder in ["big", "small"] {
            assert_eq!(app.folder_sizes.total(&dir.path().join(folder)), None);
        }
    }

    #[test]
    fn a_finished_operation_drops_the_totals_of_the_folders_above_it() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("a/b/c")).unwrap();
        std::fs::create_dir(root.join("a/x")).unwrap();
        let mut app = App::new(Vec::new());
        let folders = ["a", "a/b", "a/b/c", "a/x"].map(|folder| root.join(folder));
        for folder in &folders {
            app.folder_sizes.request(folder);
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while folders
            .iter()
            .any(|folder| app.folder_sizes.total(folder).is_none())
        {
            assert!(
                std::time::Instant::now() < deadline,
                "sizing did not finish"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
            app.folder_sizes.poll();
        }
        let (mut state, mut status, mut index) =
            (ListState::default(), StatusBar::new(), memory_index(root));

        let photo = root.join("a/b/c/photo.jpg");
        app.start_job("trash", "Moving…".into(), vec![photo], |_| {
            Ok("Moved".into())
        })
        .unwrap();
        while app.job.is_some() {
            poll_job(
                &mut app,
                &mut state,
                &mut status,
                &mut index,
                &spelled(root),
            );
            assert!(std::time::Instant::now() < deadline, "trash did not finish");
            std::thread::yield_now();
        }

        for gone in &folders[..3] {
            assert_eq!(app.folder_sizes.total(gone), None, "{}", gone.display());
        }
        assert!(app.folder_sizes.total(&folders[3]).is_some());
    }

    #[test]
    fn a_later_reorder_keeps_the_highlight_where_it_was_moved() {
        let dir = tempfile::tempdir().unwrap();
        let (mut app, mut state) = size_ordered(dir.path());
        size_left(&mut app, &mut state);
        assert_eq!(names(&app.files), ["big", "mid.txt", "small"]);
        app.select_active_row(&mut state, 2);

        // small grows past big and is sized again.
        let data = std::fs::File::create(dir.path().join("small/data")).unwrap();
        data.set_len(3 << 30).unwrap();
        app.forget_sizes([dir.path().join("small/data").as_path()]);
        size_left(&mut app, &mut state);

        assert_eq!(names(&app.files), ["small", "big", "mid.txt"]);
        assert_eq!(
            app.get_selected_path(state.selected()),
            Some(spelled(&dir.path().join("small")))
        );
        // The status line shows the folder's total, not the folder entry's size.
        let mut status_bar = StatusBar::new();
        status_bar.update(&app);
        assert_eq!(status_bar.selected_item_info, "small | Directory | 3.0 GB");
    }
}
