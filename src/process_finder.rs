use sysinfo::System;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::Win32::Foundation::*;
use windows::Win32::System::Threading::*;
use windows::core::PWSTR;
use std::collections::HashSet;

#[derive(Debug, Clone)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub title: String,
}

/// ★ 只返回有可见窗口、且窗口标题匹配 Minecraft 的进程
pub fn find_minecraft_windows() -> Vec<ProcessInfo> {
    let mut found = Vec::new();
    let mut seen: HashSet<u32> = HashSet::new();   // ★ 加类型

    unsafe {
        let _ = EnumWindows(
            Some(enum_mc_window),
            LPARAM(&mut (&mut found, &mut seen) as *mut _ as isize),
        );
    }

    found
}

unsafe extern "system" fn enum_mc_window(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let (found, seen) = &mut *(lparam.0 as *mut (&mut Vec<ProcessInfo>, &mut HashSet<u32>));

    // ★ 只处理可见窗口
    if !IsWindowVisible(hwnd).as_bool() {
        return TRUE;
    }

    let mut title_buf = [0u16; 256];
    let len = GetWindowTextW(hwnd, &mut title_buf);
    if len == 0 {
        return TRUE;
    }
    let title = String::from_utf16_lossy(&title_buf[..len as usize]);

    // ★ 过滤：标题必须包含 Minecraft 相关关键词
    let lower = title.to_lowercase();
    let is_mc = lower.contains("minecraft")
        || title.contains("我的世界")
        || lower.contains("lunar client")
        || lower.contains("feather")
        || lower.contains("badlion")
        || lower.contains("forge")
        || lower.contains("fabric");
    if !is_mc {
        return TRUE;
    }

    let mut pid = 0u32;
    GetWindowThreadProcessId(hwnd, Some(&mut pid));
    if pid == 0 || seen.contains(&pid) {
        return TRUE;
    }

    // 校验进程名是 java/javaw
    let name = get_process_name(pid);
    let name_lower = name.to_lowercase();
    if !name_lower.contains("java") {
        return TRUE;
    }

    found.push(ProcessInfo {
        pid,
        name,
        title,
    });
    seen.insert(pid);
    TRUE
}

fn get_process_name(pid: u32) -> String {
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid);
        if let Ok(h) = handle {
            let mut exe = [0u16; 260];
            let mut size = exe.len() as u32;
            if QueryFullProcessImageNameW(h, PROCESS_NAME_FORMAT(0), PWSTR(exe.as_mut_ptr()), &mut size).is_ok() {
                let path = String::from_utf16_lossy(&exe[..size as usize]);
                let _ = CloseHandle(h);
                if let Some(pos) = path.rfind('\\') {
                    return path[pos + 1..].to_string();
                }
                return path;
            }
            let _ = CloseHandle(h);
        }
    }
    "unknown".to_string()
}

// ─── 保留原有函数 ────────────────────────────────

pub fn find_minecraft_processes() -> Vec<ProcessInfo> {
    let mut found = Vec::new();
    let mut seen: HashSet<u32> = HashSet::new();   // ★ 加类型

    unsafe {
        let _ = EnumWindows(
            Some(enum_mc_window),
            LPARAM(&mut (&mut found, &mut seen) as *mut _ as isize),
        );
    }

    if found.is_empty() {
        // fallback：遍历所有 java 进程
        let sys = System::new_all();
        for process in sys.processes().values() {
            let name = process.name().to_lowercase();
            if name.contains("java") {
                let pid = process.pid().as_u32();
                found.push(ProcessInfo {
                    pid,
                    name: process.name().to_string(),
                    title: String::new(),
                });
            }
        }
    }

    found
}