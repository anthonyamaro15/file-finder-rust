use std::fmt::Display;
use std::fs::OpenOptions;
use std::io::Write;

/* pub fn log_to_file<T: Display>(msg: T) {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open("debug.log")
        .unwrap();

    writeln!(file, "{}", msg).unwrap();
} */
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
