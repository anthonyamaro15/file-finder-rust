//! Image preview rendering through terminal graphics protocols.

use ratatui_image::{picker::Picker, protocol::StatefulProtocol};

/// Default font size for terminal graphics protocol picker (width, height in pixels)
const DEFAULT_FONT_SIZE: (u16, u16) = (8, 16);

/// Image renderer that supports terminal graphics protocols (Sixel, Kitty, iTerm2)
/// with fallback to Unicode halfblocks for unsupported terminals.
pub struct ImageRenderer {
    /// Terminal graphics protocol picker
    picker: Picker,
    /// Current image protocol state for rendering
    pub(crate) image_state: Option<Box<dyn StatefulProtocol>>,
    /// Whether an image is currently loaded
    has_image: bool,
    /// Image metadata info (dimensions, format)
    image_info: String,
    /// Error message if image loading failed
    error_message: Option<String>,
}

impl ImageRenderer {
    /// Create a new ImageRenderer with terminal protocol detection
    // No `Default`: construction queries the terminal, which `default()` shouldn't hide.
    #[allow(clippy::new_without_default)]
    pub fn new() -> ImageRenderer {
        let mut picker = Picker::new(DEFAULT_FONT_SIZE);
        with_stdin_state_restored(|| picker.guess_protocol());

        ImageRenderer {
            picker,
            image_state: None,
            has_image: false,
            image_info: String::new(),
            error_message: None,
        }
    }

    /// Load an image from a pre-decoded DynamicImage (for async loading)
    pub(crate) fn load_from_decoded(&mut self, decoded_image: image::DynamicImage, format: String) {
        self.clear();

        let width = decoded_image.width();
        let height = decoded_image.height();

        // Store metadata
        self.image_info = format!("{}x{} {}", width, height, format);

        // Create protocol state for rendering
        let protocol = self.picker.new_resize_protocol(decoded_image);
        self.image_state = Some(protocol);
        self.has_image = true;
        self.error_message = None;
    }

    /// Set an error message for image loading
    pub(crate) fn set_error(&mut self, message: String) {
        self.clear();
        self.error_message = Some(message);
    }

    /// Clear the current image
    pub(crate) fn clear(&mut self) {
        self.image_state = None;
        self.has_image = false;
        self.image_info.clear();
        self.error_message = None;
    }

    /// Check if an image is loaded and ready to render
    pub(crate) fn has_image(&self) -> bool {
        self.has_image && self.image_state.is_some()
    }

    /// Get the image info string
    pub(crate) fn get_info(&self) -> &str {
        &self.image_info
    }

    /// Get error message if any
    pub(crate) fn get_error(&self) -> Option<&str> {
        self.error_message.as_deref()
    }
}

/// Run `f`, then restore stdin's fd flags and termios to what they were before.
/// ratatui-image's graphics query sets O_NONBLOCK and clears ICANON/ECHO on stdin,
/// and returns early without restoring them when the terminal doesn't reply
/// (e.g. detached tmux), which breaks crossterm reads with WouldBlock.
#[cfg(unix)]
fn with_stdin_state_restored<T>(f: impl FnOnce() -> T) -> T {
    // SAFETY: isatty only inspects fd 0.
    if unsafe { libc::isatty(libc::STDIN_FILENO) } == 0 {
        return f();
    }
    // SAFETY: fd 0 is open; termios is a valid out-pointer for tcgetattr.
    let (flags, termios) = unsafe {
        let mut termios: libc::termios = std::mem::zeroed();
        let flags = libc::fcntl(libc::STDIN_FILENO, libc::F_GETFL);
        let saved = libc::tcgetattr(libc::STDIN_FILENO, &mut termios) == 0;
        (flags, saved.then_some(termios))
    };

    let result = f();

    // SAFETY: restores the flags and termios read from fd 0 above.
    unsafe {
        if flags != -1 {
            libc::fcntl(libc::STDIN_FILENO, libc::F_SETFL, flags);
        }
        if let Some(termios) = &termios {
            libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, termios);
        }
    }
    result
}

#[cfg(not(unix))]
fn with_stdin_state_restored<T>(f: impl FnOnce() -> T) -> T {
    f()
}
