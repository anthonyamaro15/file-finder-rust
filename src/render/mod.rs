//! Rendering utilities for popups and previews.

pub mod popups;
pub mod preview;

// Re-export commonly used items
pub use popups::{
    create_create_input_popup, create_delete_confirmation_block, create_keybindings_popup,
    create_rename_input_popup, create_sort_options_popup, draw_popup, generate_sort_by_string,
    split_popup_area, split_popup_area_vertical,
};

pub use preview::{
    create_archive_preview, create_binary_preview, create_cache_loading_screen, create_csv_preview,
    create_file_preview, create_image_preview, create_zip_preview,
};
