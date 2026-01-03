use std::fs::OpenOptions;
use std::io::Write;

const LOG_FILE_PATH: &str = "yassh_debug.log";

pub fn log(msg: &str) {
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(LOG_FILE_PATH)
    {
        let _ = writeln!(file, "{}", msg);
        let _ = file.flush();
    }
}

