use std::sync::mpsc;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{backend::Backend, widgets::ListState, Terminal};

use crate::{
    app::App,
    directory_store::DirectoryIndex,
    exit::may_exit,
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
pub mod transfer;

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

/// Runs before every mode's handler. Ctrl-C quits like `q`; any other Ctrl or Alt
/// character is swallowed so it never acts as, or types, the plain character.
/// Shift passes through. Returns `None` for keys the current mode should handle.
pub fn handle_modifier_key<B: Backend>(
    key: KeyEvent,
    loop_state: &mut LoopState,
    terminal: &mut Terminal<B>,
) -> Option<LoopControl> {
    if !matches!(key.code, KeyCode::Char(_))
        || !key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
    {
        return None;
    }
    let LoopState {
        app,
        status_bar,
        copy_receiver,
        quit_error,
        ..
    } = loop_state;
    if key.code == KeyCode::Char('c')
        && key.modifiers.contains(KeyModifiers::CONTROL)
        && key.kind == KeyEventKind::Press
        && may_exit(
            app,
            status_bar,
            "Ctrl-C",
            copy_receiver,
            terminal,
            quit_error,
        )
    {
        return Some(LoopControl::Break);
    }
    Some(LoopControl::Continue)
}
