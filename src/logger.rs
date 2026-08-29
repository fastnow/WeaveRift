use std::fs::{OpenOptions, File};
use std::io::Write;
use std::sync::{Mutex, OnceLock};
use chrono::Local;

static LOG_FILE: OnceLock<Mutex<Option<File>>> = OnceLock::new();

fn get_time() -> String {
    Local::now().format("%H:%M:%S").to_string()
}

pub fn init_logger() -> Result<(), std::io::Error> {
    let path = std::env::temp_dir().join("WeaveRift.log");
    // 日志无轮转，为避免无限增大，超过 1MB 时截断重写
    const MAX_BYTES: u64 = 1_048_576;
    if let Ok(meta) = std::fs::metadata(&path) {
        if meta.len() > MAX_BYTES {
            let _ = std::fs::remove_file(&path);
        }
    }
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    let _ = LOG_FILE.set(Mutex::new(Some(file)));
    info("Logger initialized");
    Ok(())
}

fn log(level: &str, msg: &str) {
    let time = get_time();
    let line = format!("[{}] [{}] {}\n", time, level, msg);
    print!("{}", line);
    if let Some(mutex) = LOG_FILE.get() {
        if let Ok(mut guard) = mutex.lock() {
            if let Some(file) = guard.as_mut() {
                let _ = file.write_all(line.as_bytes());
                let _ = file.flush();
            }
        }
    }
}

pub fn info(msg: &str) { log("INFO", msg); }
pub fn warn(msg: &str) { log("WARN", msg); }
pub fn error(msg: &str) { log("ERROR", msg); }
pub fn debug(msg: &str) { log("DEBUG", msg); }