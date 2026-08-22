use std::fs::{OpenOptions, File};
use std::io::Write;
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

static LOG_FILE: OnceLock<Mutex<Option<File>>> = OnceLock::new();

fn get_time() -> String {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    let hours = (secs / 3600) % 24;
    let mins = (secs / 60) % 60;
    let secs = secs % 60;
    format!("{:02}:{:02}:{:02}", hours, mins, secs)
}

pub fn init_logger() -> Result<(), std::io::Error> {
    let path = std::env::temp_dir().join("FlashDllInjector.log");
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