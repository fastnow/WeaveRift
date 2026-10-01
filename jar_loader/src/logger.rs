//! 双写日志：内存环形缓冲（供调试台拉取）+ 本地文件（供事后复盘）。
//!
//! 时间统一用墙钟（SystemTime），不要用 Instant / nanoTime —— 那是单调时钟，
//! 跨进程（Rust DLL 与 Java agent）对不上时间轴。

use std::collections::VecDeque;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

pub const TRACE: u8 = 0;
pub const DEBUG: u8 = 1;
pub const INFO: u8 = 2;
pub const WARN: u8 = 3;
pub const ERROR: u8 = 4;

const CAP: usize = 2000;

pub struct Record {
    pub seq: u64,
    pub ms: u64,   // 当日毫秒，前端用于排序
    pub level: u8,
    pub msg: String,
}

struct Ring {
    buf: VecDeque<Record>,
    seq: u64,
}

static RING: OnceLock<Mutex<Ring>> = OnceLock::new();
static FILE: OnceLock<Mutex<Option<File>>> = OnceLock::new();
static MIN_LEVEL: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(DEBUG);

fn ring() -> &'static Mutex<Ring> {
    RING.get_or_init(|| Mutex::new(Ring { buf: VecDeque::with_capacity(CAP), seq: 0 }))
}

/// 日志文件放到 %TEMP%\WeaveRift\weaverift.log
pub fn init_file() {
    let _ = FILE.get_or_init(|| {
        let mut path: PathBuf = std::env::temp_dir();
        path.push("WeaveRift");
        let _ = std::fs::create_dir_all(&path);
        path.push("weaverift.log");
        let f = File::options()
            .create(true)
            .append(true)
            .open(&path)
            .ok();
        Mutex::new(f)
    });
}

pub fn set_min_level(l: u8) {
    MIN_LEVEL.store(l, std::sync::atomic::Ordering::Relaxed);
}
pub fn min_level() -> u8 {
    MIN_LEVEL.load(std::sync::atomic::Ordering::Relaxed)
}

fn level_name(l: u8) -> &'static str {
    match l {
        TRACE => "TRACE",
        DEBUG => "DEBUG",
        INFO => "INFO",
        WARN => "WARN",
        _ => "ERROR",
    }
}

fn wall_clock() -> (u64, String) {
    let d = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    let ms_total = d.as_millis() as u64;
    let secs = d.as_secs();
    let hms = format!(
        "{:02}:{:02}:{:02}.{:03}",
        (secs / 3600) % 24,
        (secs / 60) % 60,
        secs % 60,
        d.subsec_millis()
    );
    (ms_total, hms)
}

pub fn log(level: u8, msg: &str) {
    if level < min_level() {
        return;
    }
    let (ms, hms) = wall_clock();

    let line = format!("[{}] [{}] {}", hms, level_name(level), msg);
    eprintln!("[WeaveRift] {}", line);

    if let Ok(mut f) = FILE.get_or_init(|| Mutex::new(None)).lock() {
        if let Some(file) = f.as_mut() {
            let _ = writeln!(file, "{}", line);
            let _ = file.flush();
        }
    }

    if let Ok(mut r) = ring().lock() {
        let seq = r.seq + 1;
        r.seq = seq;
        r.buf.push_back(Record { seq, ms, level, msg: msg.to_string() });
        while r.buf.len() > CAP {
            r.buf.pop_front();
        }
    }
}

#[inline] pub fn trace(m: &str) { log(TRACE, m); }
#[inline] pub fn debug(m: &str) { log(DEBUG, m); }
#[inline] pub fn info(m: &str)  { log(INFO, m); }
#[inline] pub fn warn(m: &str)  { log(WARN, m); }
#[inline] pub fn error(m: &str) { log(ERROR, m); }

/// 拉增量日志：since 为上次拿到的最大 seq
pub fn since(since_seq: u64) -> Vec<(u64, u64, u8, String)> {
    match ring().lock() {
        Ok(r) => r
            .buf
            .iter()
            .filter(|x| x.seq > since_seq)
            .map(|x| (x.seq, x.ms, x.level, x.msg.clone()))
            .collect(),
        Err(_) => Vec::new(),
    }
}

pub fn latest_seq() -> u64 {
    ring().lock().map(|r| r.seq).unwrap_or(0)
}
