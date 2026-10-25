use ratatui::widgets::ListState;

use crate::{
    app::{App, InputMode},
    status_bar::StatusBar,
    utils::{read_listing, SortBy},
};

/// Apply a sort to both panes' listings and persist the preference, keeping each
/// pane's highlighted item. A failed read shows an error and keeps the old listing.
pub fn apply_sort(
    app: &mut App,
    state: &mut ListState,
    sort_by: SortBy,
    status_bar: &mut StatusBar,
) {
    app.sort_by = sort_by;
    app.right_pane.sort_by = app.sort_by.clone();
    app.right_pane.sort_type = app.sort_type.clone();
    if app.view_mode.is_dual_pane() {
        app.right_pane.refresh_files();
    }

    let cur_path = app.left_directory_path().to_string_lossy().to_string();
    let listing = match read_listing(
        cur_path.clone(),
        app.show_hidden_files,
        app.sort_by.clone(),
        &app.sort_type,
    ) {
        Ok(listing) => listing,
        Err(e) => {
            status_bar.show_error(format!("Could not refresh '{cur_path}': {e:#}"), None);
            return;
        }
    };

    app.refresh_files_from_watcher(listing, state);
    app.input_mode = InputMode::Normal;
}
