use sysinfo::System;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::Win32::Foundation::*;
use windows::Win32::System::Threading::*;
use windows::core::PWSTR;
use std::collections::HashSet;

const MC_TITLE_KEYWORDS: [&str; 5] = ["Minecraft", "forge", "FML", "Lunar", "Badlion"];

#[derive(Debug, Clone)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub title: String,
}

struct EnumState {
    found: Vec<ProcessInfo>,
    seen: HashSet<u32>,
}

struct EnumPidState {
    target: u32,
    title: Option<String>,
}

pub fn find_minecraft_processes() -> Vec<ProcessInfo> {
    let mut state = Box::new(EnumState { found: Vec::new(), seen: HashSet::new() });
    let raw = Box::into_raw(state);
    unsafe {
        let _ = EnumWindows(Some(enum_window_proc), LPARAM(raw as isize));
        state = Box::from_raw(raw);
    }

    let sys = System::new_all();
    for p in sys.processes().values() {
        let nl = p.name().to_lowercase();
        if nl.contains("javaw") || nl.contains("java") {
            let pid = p.pid().as_u32();
            if !state.seen.contains(&pid) {
                let title = get_window_title_by_pid(pid).unwrap_or_default();
                state.found.push(ProcessInfo { pid, name: p.name().to_string(), title });
                state.seen.insert(pid);
            }
        }
    }
    state.found
}

unsafe extern "system" fn enum_window_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let s = &mut *(lparam.0 as *mut EnumState);
    let mut t = [0u16; 256];
    let len = GetWindowTextW(hwnd, &mut t);
    if len == 0 { return TRUE; }
    let ts = String::from_utf16_lossy(&t[..len as usize]);
    if !MC_TITLE_KEYWORDS.iter().any(|kw| ts.contains(kw)) {
        return TRUE;
    }
    let mut pid = 0u32;
    GetWindowThreadProcessId(hwnd, Some(&mut pid));
    if pid == 0 || s.seen.contains(&pid) { return TRUE; }
    if !is_java_process(pid) { return TRUE; }
    s.found.push(ProcessInfo { pid, name: get_process_name(pid).unwrap_or_else(|| "javaw.exe".to_string()), title: ts });
    s.seen.insert(pid);
    TRUE
}

fn get_window_title_by_pid(target: u32) -> Option<String> {
    let mut state = EnumPidState { target, title: None };
    unsafe {
        EnumWindows(Some(enum_pid_callback), LPARAM(&mut state as *mut _ as isize));
    }
    state.title
}

unsafe extern "system" fn enum_pid_callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let state = &mut *(lparam.0 as *mut EnumPidState);
    let mut pid = 0u32;
    GetWindowThreadProcessId(hwnd, Some(&mut pid));
    if pid == state.target {
        let mut buf = [0u16; 256];
        let len = GetWindowTextW(hwnd, &mut buf);
        if len > 0 {
            state.title = Some(String::from_utf16_lossy(&buf[..len as usize]));
            return FALSE;
        }
    }
    TRUE
}

fn is_java_process(pid: u32) -> bool {
    match get_process_name(pid) {
        Some(name) => {
            let nl = name.to_lowercase();
            nl.contains("javaw") || nl.contains("java")
        }
        None => false,
    }
}

fn get_process_name(pid: u32) -> Option<String> {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut e = [0u16; 260];
        let mut s = e.len() as u32;
        let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_FORMAT(0), PWSTR(e.as_mut_ptr()), &mut s).is_ok();
        let _ = CloseHandle(h);
        if !ok { return None; }
        let p = String::from_utf16_lossy(&e[..s as usize]);
        Some(p.rfind('\\').map(|i| p[i+1..].to_string()).unwrap_or(p))
    }
}