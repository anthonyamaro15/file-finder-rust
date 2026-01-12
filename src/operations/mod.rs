//! File and copy operations module.

pub mod copy;
pub mod file_ops;

// Re-export items used by main.rs
pub use copy::{copy_dir_file_helper, copy_dir_file_with_progress, CopyMessage};
pub use file_ops::{
    create_item_based_on_type, handle_delete_based_on_type, handle_rename, move_file_or_dir,
};
