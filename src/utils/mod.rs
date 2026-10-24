//! Utility modules for file operations, formatting, and logging.

pub mod files;
pub mod format;

use simplelog::*;
use std::fs::File;

/// Initialize logging. Never logs to the terminal, since the TUI owns stdout.
/// By default, warn-level logs go to `~/.config/ff/ff.log`, truncated on each start.
/// Set FF_LOG_TO_FILE=1 to enable debug logging to debug.log.
/// If the log file can't be created, no logger is installed.
pub fn init() {
    // Do not create a debug.log file by default.
    // If you want file logging, opt-in with: FF_LOG_TO_FILE=1
    let log_to_file = std::env::var("FF_LOG_TO_FILE").ok().as_deref() == Some("1");

    if log_to_file {
        // Opt-in file logger (debug level)
        if let Ok(log_file) = File::create("debug.log") {
            let _ = CombinedLogger::init(vec![WriteLogger::new(
                LevelFilter::Debug,
                Config::default(),
                log_file,
            )]);
        }
    } else if let Some(log_file) = default_log_file() {
        // Default: warn level to ~/.config/ff/ff.log
        let _ = WriteLogger::init(LevelFilter::Warn, Config::default(), log_file);
    }
}

/// Create (truncating) `<config dir>/ff.log`, creating the config dir if needed.
fn default_log_file() -> Option<File> {
    let config_dir = crate::config::get_config_dir().ok()?;
    std::fs::create_dir_all(&config_dir).ok()?;
    File::create(config_dir.join("ff.log")).ok()
}

// Re-export items used by main.rs
pub use files::{
    check_if_exists, generate_copy_file_dir_name, get_curr_path, get_file_path_data, is_file,
    SortBy, SortType,
};
pub use format::format_file_size;
