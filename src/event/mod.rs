pub mod background;
pub mod file_actions;
pub mod keymap;
pub mod navigation;
pub mod normal;
pub mod popups;
pub mod search;
pub mod sort;

/// What the main loop does after a key handler returns.
pub enum LoopControl {
    /// Go on to the next iteration.
    Continue,
    /// Leave the loop, then restore the terminal.
    Break,
    /// Return from `main` at once; the handler already restored the terminal.
    Return,
}
