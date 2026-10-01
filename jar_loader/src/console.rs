use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::*;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use crate::ipc;

const VK_ESCAPE_CODE: usize = 0x1B;

// 按钮 ID
const BTN_KILLAURA: usize = 1001;
const BTN_VELOCITY: usize = 1002;
const BTN_ESP: usize = 1003;
const BTN_SPEED: usize = 1004;

// 模块状态
static KA_ON: AtomicBool = AtomicBool::new(false);
static VEL_ON: AtomicBool = AtomicBool::new(false);
static ESP_ON: AtomicBool = AtomicBool::new(false);
static SPD_ON: AtomicBool = AtomicBool::new(false);

// 玩家数据缓存
static PLAYER_CACHE: Mutex<(f64, f64, f64, f32, f32, f32, f32, f32)> =
    Mutex::new((0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 20.0, 20.0));
static LOG_BUFFER: Mutex<Vec<String>> = Mutex::new(Vec::new());

// ─── 对外接口 ─────────────────────────────────

pub fn log(msg: &str) {
    if let Ok(mut buf) = LOG_BUFFER.lock() {
        let line = format!("[{}] {}", now_str(), msg);
        println!("[WeaveRift] {}", line);
        buf.push(line);
        if buf.len() > 20 { buf.remove(0); }
    }
}

pub fn update_player(x: f64, y: f64, z: f64, yaw: f32, pitch: f32,
                     health: f32, max_health: f32, tps: f32) {
    if let Ok(mut v) = PLAYER_CACHE.lock() {
        *v = (x, y, z, yaw, pitch, health, max_health, tps);
    }
}

fn now_str() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = now.as_secs();
    format!("{:02}:{:02}:{:02}", (secs/3600)%24, (secs/60)%60, secs%60)
}

// ─── 窗口过程 ─────────────────────────────────

unsafe extern "system" fn wnd_proc(
    hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CREATE => {
            // ★ 创建按钮
            create_button(hwnd, BTN_KILLAURA, "KillAura: OFF", 10, 130, 180, 28);
            create_button(hwnd, BTN_VELOCITY, "Velocity: OFF", 200, 130, 180, 28);
            create_button(hwnd, BTN_ESP, "ESP: OFF", 10, 165, 180, 28);
            create_button(hwnd, BTN_SPEED, "Speed: OFF", 200, 165, 180, 28);
            LRESULT(0)
        }
        WM_COMMAND => {
            let id = (wparam.0 & 0xFFFF) as usize;
            match id {
                BTN_KILLAURA => {
                    let new = !KA_ON.load(Ordering::Relaxed);
                    KA_ON.store(new, Ordering::Relaxed);
                    ipc::send_command(ipc::CMD_KILLAURA_TOGGLE, if new { 1 } else { 0 });
                    update_button_text(hwnd, BTN_KILLAURA, "KillAura", new);
                }
                BTN_VELOCITY => {
                    let new = !VEL_ON.load(Ordering::Relaxed);
                    VEL_ON.store(new, Ordering::Relaxed);
                    ipc::send_command(ipc::CMD_VELOCITY_TOGGLE, if new { 1 } else { 0 });
                    update_button_text(hwnd, BTN_VELOCITY, "Velocity", new);
                }
                BTN_ESP => {
                    let new = !ESP_ON.load(Ordering::Relaxed);
                    ESP_ON.store(new, Ordering::Relaxed);
                    ipc::send_command(ipc::CMD_ESP_TOGGLE, if new { 1 } else { 0 });
                    update_button_text(hwnd, BTN_ESP, "ESP", new);
                }
                BTN_SPEED => {
                    let new = !SPD_ON.load(Ordering::Relaxed);
                    SPD_ON.store(new, Ordering::Relaxed);
                    ipc::send_command(ipc::CMD_SPEED_TOGGLE, if new { 1 } else { 0 });
                    update_button_text(hwnd, BTN_SPEED, "Speed", new);
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            if !hdc.0.is_null() { render_content(hwnd, hdc); }
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_TIMER => {
            InvalidateRect(hwnd, None, FALSE);
            LRESULT(0)
        }
        WM_KEYDOWN => {
            if wparam.0 == VK_ESCAPE_CODE {
                let _ = ShowWindow(hwnd, SW_HIDE);
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn create_button(parent: HWND, id: usize, text: &str, x: i32, y: i32, w: i32, h: i32) {
    let text_wide: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
    let class = w!("BUTTON");
    let _ = CreateWindowExW(
        WINDOW_EX_STYLE(0),
        class,
        PCWSTR(text_wide.as_ptr()),
        WS_CHILD | WS_VISIBLE | WINDOW_STYLE(BS_PUSHBUTTON as u32),
        x, y, w, h,
        parent,
        HMENU(id as isize as *mut c_void),
        GetModuleHandleW(None).unwrap_or_default(),
        None,
    );
}

unsafe fn update_button_text(parent: HWND, id: usize, name: &str, on: bool) {
    let text = format!("{}: {}", name, if on { "ON" } else { "OFF" });
    let text_wide: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
    let btn = GetDlgItem(parent, id as i32);
    if let Ok(b) = btn {
        let _ = SetWindowTextW(b, PCWSTR(text_wide.as_ptr()));
    }
}

unsafe fn render_content(hwnd: HWND, hdc: HDC) {
    let mut rect = RECT::default();
    let _ = GetClientRect(hwnd, &mut rect);

    let bg_brush = CreateSolidBrush(COLORREF(0x001E1E1E));
    FillRect(hdc, &rect, bg_brush);
    let _ = DeleteObject(bg_brush);

    SetBkMode(hdc, TRANSPARENT);

    let mut y = 10;
    let lh = 18;

    SetTextColor(hdc, COLORREF(0x0000D7FF));
    draw_text(hdc, 10, y, "=== WeaveRift Console ===");
    y += lh + 4;

    SetTextColor(hdc, COLORREF(0x00FFFFFF));
    let (px, py, pz, yaw, pitch, hp, mhp, tps) = *PLAYER_CACHE.lock().unwrap();
    draw_text(hdc, 10, y, &format!("Pos: {:.2}, {:.2}, {:.2}", px, py, pz)); y += lh;
    draw_text(hdc, 10, y, &format!("Rot: yaw={:.1} pitch={:.1}", yaw, pitch)); y += lh;
    draw_text(hdc, 10, y, &format!("HP: {:.1}/{:.1}  TPS: {:.1}", hp, mhp, tps)); y += lh + 4;

    SetTextColor(hdc, COLORREF(0x00FFD700));
    draw_text(hdc, 10, y, "=== Modules ===");
    y += lh;

    SetTextColor(hdc, COLORREF(0x00A0A0A0));
    draw_text(hdc, 10, 205, "--- Log ---");
    let mut ly = 225;
    if let Ok(buf) = LOG_BUFFER.lock() {
        for line in buf.iter().rev().take(10) {
            draw_text(hdc, 10, ly, line);
            ly += lh;
        }
    }

    SetTextColor(hdc, COLORREF(0x00808080));
    draw_text(hdc, 10, rect.bottom - 25, "Press ESC to hide");
}

unsafe fn draw_text(hdc: HDC, x: i32, y: i32, text: &str) {
    let wide: Vec<u16> = text.encode_utf16().collect();
    let _ = TextOutW(hdc, x, y, &wide);
}

// ─── 启动 ─────────────────────────────────────

pub fn start_console_thread() {
    std::thread::spawn(|| unsafe {
        let hmodule = match GetModuleHandleW(None) {
            Ok(h) => h,
            Err(_) => return,
        };
        let hinstance: HINSTANCE = hmodule.into();

        let class_name = w!("WeaveRiftConsole");
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wnd_proc),
            hInstance: hinstance,
            hbrBackground: HBRUSH(std::ptr::null_mut()),
            lpszClassName: class_name,
            ..Default::default()
        };
        RegisterClassExW(&wc);

        let hwnd = match CreateWindowExW(
            WINDOW_EX_STYLE(0), class_name, w!("WeaveRift Console"),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            CW_USEDEFAULT, CW_USEDEFAULT, 420, 520,
            None, None, hinstance, None,
        ) {
            Ok(h) => h,
            Err(_) => return,
        };

        SetTimer(hwnd, 1, 100, None);

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    });
}