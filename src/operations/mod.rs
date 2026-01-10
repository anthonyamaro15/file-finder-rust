//! File and copy operations module.

pub mod copy;
pub mod file_ops;

// Re-export commonly used items
pub use copy::{copy_dir_file_with_progress, CopyMessage};
pub use file_ops::{
    create_item_based_on_type, create_new_dir, create_new_file, delete_dir, delete_file,
    handle_delete_based_on_type, handle_rename,
};
