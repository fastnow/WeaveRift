use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub const BOOT: u8 = 0;
pub const START_HTTP: u8 = 1;
pub const WAIT_GL: u8 = 2;
pub const FIND_VM: u8 = 3;
pub const CREATE_CTX: u8 = 4;
pub const LOAD_AGENT: u8 = 5;
pub const WAIT_AGENT: u8 = 6;
pub const RUNNING: u8 = 7;
pub const FAILED: u8 = 8;

static STATE: AtomicU8 = AtomicU8::new(BOOT);
static RETRY_COUNT: AtomicU8 = AtomicU8::new(0);
static LAST_RETRY: Mutex<Option<Instant>> = Mutex::new(None);

const RETRY_INTERVAL: Duration = Duration::from_millis(500);
const MAX_RETRIES: u8 = 60;

pub fn current() -> u8 {
    STATE.load(Ordering::Relaxed)
}

pub fn set(s: u8) {
    RETRY_COUNT.store(0, Ordering::Relaxed);
    *LAST_RETRY.lock().unwrap() = None;
    STATE.store(s, Ordering::Relaxed);
}

pub fn can_retry() -> bool {
    let mut last = LAST_RETRY.lock().unwrap();
    let now = Instant::now();
    if let Some(t) = *last {
        if now.duration_since(t) < RETRY_INTERVAL {
            return false;
        }
    }
    *last = Some(now);
    true
}

pub fn fail(reason: &str) {
    let n = RETRY_COUNT.fetch_add(1, Ordering::Relaxed) + 1;
    if n >= MAX_RETRIES {
        crate::logger::error(&format!(
            "state {} 重试 {} 次后放弃：{}",
            current(),
            n,
            reason
        ));
        STATE.store(FAILED, Ordering::Relaxed);
    }
}

pub fn recover() {
    RETRY_COUNT.store(0, Ordering::Relaxed);
    *LAST_RETRY.lock().unwrap() = None;
    STATE.store(WAIT_AGENT, Ordering::Relaxed);
    crate::logger::info("状态机已恢复，等待 agent");
}

pub fn name(s: u8) -> &'static str {
    match s {
        BOOT => "Boot",
        START_HTTP => "StartHttp",
        WAIT_GL => "WaitGL",
        FIND_VM => "FindVM",
        CREATE_CTX => "CreateCtx",
        LOAD_AGENT => "LoadAgent",
        WAIT_AGENT => "WaitAgent",
        RUNNING => "Running",
        _ => "Failed",
    }
}