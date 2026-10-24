use std::sync::mpsc;

use ratatui::widgets::ListState;

use crate::{
    app::App,
    directory_store::DirectoryIndex,
    file_reader_content::FileContent,
    operations::{CopyMessage, DeleteMessage},
    render::image_renderer::ImageRenderer,
    status_bar::StatusBar,
};

pub mod background;
pub mod file_actions;
pub mod keymap;
pub mod navigation;
pub mod normal;
pub mod popups;
pub mod search;
pub mod sort;

/// The main loop's mutable state, shared by drawing and the key handlers.
pub struct LoopState {
    pub app: App,
    pub state: ListState,
    pub file_reader_content: FileContent<'static>,
    pub image_renderer: ImageRenderer,
    pub status_bar: StatusBar,
    pub index: DirectoryIndex,
    /// Copy progress receiver (for async copy operations)
    pub copy_receiver: Option<mpsc::Receiver<CopyMessage>>,
    /// Delete progress receiver (for async delete operations)
    pub delete_receiver: Option<mpsc::Receiver<DeleteMessage>>,
    /// File chosen with Enter, handled after the terminal is restored
    pub selected_file: Option<String>,
    pub quit_error: Option<String>,
}

/// What the main loop does after a key handler returns.
pub enum LoopControl {
    /// Go on to the next iteration.
    Continue,
    /// Leave the loop, then restore the terminal.
    Break,
    /// Return from `main` at once; the handler already restored the terminal.
    Return,
}
