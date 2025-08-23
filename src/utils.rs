use simplelog::*;
use std::fs::File;

pub fn init() {
    let log_file = File::create("debug.log").expect("Could not create log file");

    CombinedLogger::init(vec![WriteLogger::new(
        LevelFilter::Debug,
        Config::default(),
        log_file,
    )])
    .expect("Could not initialize logger");
}
