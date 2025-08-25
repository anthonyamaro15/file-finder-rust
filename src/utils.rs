use simplelog::*;
use std::fs::File;

pub fn init() {
    // Do not create a debug.log file by default.
    // If you want file logging, opt-in with: FF_LOG_TO_FILE=1
    let log_to_file = std::env::var("FF_LOG_TO_FILE").ok().as_deref() == Some("1");

    if log_to_file {
        // Opt-in file logger (debug level)
        match File::create("debug.log") {
            Ok(log_file) => {
                let _ = CombinedLogger::init(vec![WriteLogger::new(
                    LevelFilter::Debug,
                    Config::default(),
                    log_file,
                )]);
            }
            Err(_) => {
                // Fallback to console warn-level if file cannot be created
                let _ = SimpleLogger::init(LevelFilter::Warn, Config::default());
            }
        }
    } else {
        // Default: console logger at warn level (no file creation)
        let _ = SimpleLogger::init(LevelFilter::Warn, Config::default());
    }
}
