use crate::{
    app::{App, InputMode},
    status_bar::StatusBar,
    utils::{get_curr_path, get_file_path_data, SortBy},
};

/// Apply a sort operation to the current file list and persist the preference.
/// A failed read shows an error and keeps the old listing.
pub fn apply_sort(app: &mut App, sort_by: SortBy, status_bar: &mut StatusBar) {
    if app.files.is_empty() {
        return;
    }

    app.sort_by = sort_by.clone();

    let cur_path = get_curr_path(app.files[0].clone());
    let file_path_list = match get_file_path_data(
        cur_path.clone(),
        app.show_hidden_files,
        app.sort_by.clone(),
        &app.sort_type,
    ) {
        Ok(files) => files,
        Err(e) => {
            status_bar.show_error(format!("Could not refresh '{cur_path}': {e:#}"), None);
            return;
        }
    };

    app.files = file_path_list.clone();
    app.read_only_files = file_path_list;
    app.update_file_references();
    app.input_mode = InputMode::Normal;
}
