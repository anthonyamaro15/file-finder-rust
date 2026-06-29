use crate::{
    app::{App, InputMode},
    utils::{get_curr_path, get_file_path_data, SortBy},
};

/// Apply a sort operation to the current file list and persist the preference.
pub fn apply_sort(app: &mut App, sort_by: SortBy) -> anyhow::Result<()> {
    if app.files.is_empty() {
        return Ok(());
    }

    app.sort_by = sort_by.clone();

    let cur_path = get_curr_path(app.files[0].clone());
    let file_path_list = get_file_path_data(
        cur_path,
        app.show_hidden_files,
        app.sort_by.clone(),
        &app.sort_type,
    )?;

    app.files = file_path_list.clone();
    app.read_only_files = file_path_list;
    app.update_file_references();
    app.input_mode = InputMode::Normal;

    Ok(())
}
