//! Rendering utilities for popups and previews.

pub mod popups;
pub mod preview;

// Re-export items used by main.rs
pub use popups::{
    create_create_input_popup, create_delete_confirmation_block, create_keybindings_popup,
    create_rename_input_popup, create_sort_options_popup, draw_popup, split_popup_area,
    split_popup_area_vertical,
};
pub use preview::create_cache_loading_screen;
