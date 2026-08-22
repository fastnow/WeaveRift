use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;
use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::*;
use windows::Win32::System::Threading::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use std::ptr;

// ─── 全局状态 ───
static GLOBAL_HINST: AtomicUsize = AtomicUsize::new(0);
static GLOBAL_HWND: AtomicUsize = AtomicUsize::new(0);
static GLOBAL_RUNNING: AtomicBool = AtomicBool::new(false);
static TARGET_HWND: AtomicUsize = AtomicUsize::new(0);

// 动画参数（UI 线程独占访问）
#[derive(Clone, Copy)]
struct AnimState {
    bg_alpha: u8,
    main_scale: f32,
    main_alpha: u8,
    sub_scale: f32,
    sub_alpha: u8,
}

static mut ANIM: AnimState = AnimState {
    bg_alpha: 0,
    main_scale: 0.15,
    main_alpha: 0,
    sub_scale: 0.6,
    sub_alpha: 0,
};

static mut STATE: u32 = 0;
static mut ELAPSED: u32 = 0;

const CK_COLOR: COLORREF = COLORREF(0x00010101);

// ─── 入口 ───
#[no_mangle]
extern "system" fn DllMain(hinst: HINSTANCE, reason: u32, _: *mut c_void) -> BOOL {
    match reason {
        1 => {
            GLOBAL_HINST.store(hinst.0 as usize, Ordering::Relaxed);
            unsafe { DisableThreadLibraryCalls(hinst) };
            thread::spawn(|| {
                thread::sleep(Duration::from_millis(300));
                unsafe { overlay_thread() };
            });
            BOOL(1)
        }
        _ => BOOL(1),
    }
}

// ─── 电影级缓动 ───
/// 指数衰减缓出：开始极快，结尾极慢，模拟镜头推进的减速感
fn ease_out_expo(t: f32) -> f32 {
    if t >= 1.0 { return 1.0; }
    1.0 - 2.0f32.powf(-10.0 * t)
}

fn ease_out_quart(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(4)
}

fn ease_in_quad(t: f32) -> f32 {
    t * t
}

// ─── 窗口匹配 ───
unsafe fn find_minecraft_window() -> Option<HWND> {
    let hwnd = FindWindowW(w!("GLFW30"), None);
    if hwnd.is_ok() {
        let h = hwnd.unwrap();
        if !h.0.is_null() && is_valid_game_window(h) { return Some(h); }
    }
    let hwnd = FindWindowW(w!("LWJGL"), None);
    if hwnd.is_ok() {
        let h = hwnd.unwrap();
        if !h.0.is_null() && is_valid_game_window(h) { return Some(h); }
    }
    let best: Option<HWND> = None;
    let best_score = 0i32;
    let _ = EnumWindows(Some(enum_mc_proc), LPARAM(&mut (best, best_score) as *mut _ as isize));
    best
}

unsafe fn is_valid_game_window(hwnd: HWND) -> bool {
    if !IsWindowVisible(hwnd).as_bool() { return false; }
    let mut rect = RECT::default();
    GetWindowRect(hwnd, &mut rect);
    let area = (rect.right - rect.left) * (rect.bottom - rect.top);
    area >= 10000
}

unsafe fn get_window_title(hwnd: HWND) -> String {
    let mut title = [0u16; 512];
    let len = GetWindowTextW(hwnd, &mut title);
    String::from_utf16_lossy(&title[..len as usize])
}

unsafe fn get_process_name(hwnd: HWND) -> String {
    let mut pid = 0u32;
    GetWindowThreadProcessId(hwnd, Some(&mut pid));
    if pid == 0 { return String::new(); }
    let hproc = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, FALSE, pid);
    if hproc.is_err() { return String::new(); }
    let hproc = hproc.unwrap();
    let mut path = [0u16; 512];
    let mut len = 512u32;
    let result = QueryFullProcessImageNameW(hproc, PROCESS_NAME_FORMAT(0), PWSTR(path.as_mut_ptr()), &mut len);
    let _ = CloseHandle(hproc);
    if result.is_ok() {
        let full = String::from_utf16_lossy(&path[..len as usize]);
        full.rsplit('\\').next().unwrap_or("").to_lowercase()
    } else {
        String::new()
    }
}

fn score_window(title: &str, proc_name: &str) -> i32 {
    let t = title.to_lowercase();
    let p = proc_name.to_lowercase();
    let mut score = 0i32;
    if p.contains("javaw") || p.contains("java.exe") { score += 50; }
    if p.contains("hmcl") || p.contains("pcl") || p.contains("bakaxl") || p.contains("multimc") { score += 40; }
    if p.contains("netease") || p.contains("mc") || p.contains("minecraft") { score += 30; }
    if t.contains("minecraft") { score += 100; }
    if t.contains("我的世界") { score += 90; }
    if t.contains("布吉岛") || t.contains("花雨庭") || t.contains("easecation")
        || t.contains("hypixel") || t.contains("hyp") || t.contains("bedwars")
        || t.contains("skywars") || t.contains("duels") || t.contains("起床战争")
        || t.contains("空岛战争") || t.contains("小游戏") || t.contains("服务器")
        || t.contains("联机大厅") || t.contains("大厅") {
        score += 80;
    }
    if t.contains("网易") || t.contains("netease") { score += 60; }
    if t.contains("启动器") || t.contains("launcher") || t.contains("browser")
        || t.contains("chrome") || t.contains("edge") || t.contains("firefox") {
        score -= 200;
    }
    if t.contains("idea") || t.contains("vscode") || t.contains("visual studio")
        || t.contains("eclipse") || t.contains("notepad") {
        score -= 200;
    }
    score
}

unsafe extern "system" fn enum_mc_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let data = &mut *(lparam.0 as *mut (Option<HWND>, i32));
    let style = GetWindowLongW(hwnd, GWL_STYLE);
    if (style & (WS_CHILD.0 as i32)) != 0 { return TRUE; }
    if !IsWindowVisible(hwnd).as_bool() { return TRUE; }
    let mut rect = RECT::default();
    GetWindowRect(hwnd, &mut rect);
    let area = (rect.right - rect.left) * (rect.bottom - rect.top);
    if area < 10000 { return TRUE; }
    let title = get_window_title(hwnd);
    let proc_name = get_process_name(hwnd);
    let s = score_window(&title, &proc_name);
    if s > data.1 {
        data.1 = s;
        data.0 = Some(hwnd);
    }
    TRUE
}

// ─── GDI 缓存 ───
struct CachedRes {
    mem_dc: HDC,
    mem_bmp: HBITMAP,
    old_bmp: HGDIOBJ,
    width: i32,
    height: i32,
}

impl CachedRes {
    unsafe fn new(screen_dc: HDC, w: i32, h: i32) -> Self {
        let mem_dc = CreateCompatibleDC(screen_dc);
        let bmp = CreateCompatibleBitmap(screen_dc, w, h);
        let old_bmp = SelectObject(mem_dc, bmp);
        Self { mem_dc, mem_bmp: bmp, old_bmp, width: w, height: h }
    }
    unsafe fn ensure_size(&mut self, screen_dc: HDC, w: i32, h: i32) {
        if w != self.width || h != self.height {
            SelectObject(self.mem_dc, self.old_bmp);
            DeleteObject(self.mem_bmp);
            DeleteDC(self.mem_dc);
            *self = Self::new(screen_dc, w, h);
        }
    }
    unsafe fn cleanup(&mut self) {
        SelectObject(self.mem_dc, self.old_bmp);
        DeleteObject(self.mem_bmp);
        DeleteDC(self.mem_dc);
        self.mem_dc = HDC(ptr::null_mut());
    }
}

static mut RES: Option<CachedRes> = None;

// ─── 主线程 ───
unsafe fn overlay_thread() {
    let hinstance = HINSTANCE(GLOBAL_HINST.load(Ordering::Relaxed) as *mut c_void);
    if hinstance.0 == ptr::null_mut() { return; }

    let target = match find_minecraft_window() {
        Some(h) => h,
        None => return,
    };
    TARGET_HWND.store(target.0 as usize, Ordering::Relaxed);

    let class_name = w!("FDIOverlayClass");
    let wc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        style: CS_HREDRAW | CS_VREDRAW | CS_OWNDC,
        lpfnWndProc: Some(wndproc),
        hInstance: hinstance,
        hbrBackground: HBRUSH(ptr::null_mut()),
        lpszClassName: class_name,
        ..Default::default()
    };
    if RegisterClassExW(&wc) == 0 { return; }

    let mut rect = RECT::default();
    GetWindowRect(target, &mut rect);
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;

    let hwnd = CreateWindowExW(
        WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
        class_name,
        w!("FDI"),
        WS_POPUP,
        rect.left, rect.top, width, height,
        None, None, hinstance, None,
    );
    let hwnd = match hwnd { Ok(h) => h, Err(_) => return };

    let _ = SetLayeredWindowAttributes(hwnd, CK_COLOR, 0, LWA_COLORKEY | LWA_ALPHA);

    SetTimer(hwnd, 1, 16, None);
    GLOBAL_HWND.store(hwnd.0 as usize, Ordering::Relaxed);
    GLOBAL_RUNNING.store(true, Ordering::Relaxed);

    ShowWindow(hwnd, SW_SHOW);
    UpdateWindow(hwnd);

    let mut msg = MSG::default();
    while GetMessageW(&mut msg, None, 0, 0).into() {
        TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }

    GLOBAL_RUNNING.store(false, Ordering::Relaxed);
    GLOBAL_HWND.store(0, Ordering::Relaxed);
}

// ─── 窗口过程 ───
unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_CREATE => {
            let hdc = GetDC(hwnd);
            let mut rc = RECT::default();
            GetClientRect(hwnd, &mut rc);
            RES = Some(CachedRes::new(hdc, rc.right, rc.bottom));
            ReleaseDC(hwnd, hdc);
            LRESULT(0)
        }

        WM_TIMER => {
            ELAPSED += 16;
            match STATE {
                // ── 阶段0：黑色背景淡入 + 主标题从远处放大推进 ──
                // 参考福克斯开场：文字从极远处（很小、很淡）快速向镜头推进，
                // 减速定格在屏幕正中央
                0 => {
                    let p = (ELAPSED as f32 / 900.0).min(1.0);
                    let t = ease_out_expo(p);

                    // 背景：纯黑，alpha 0→230
                    ANIM.bg_alpha = (t * 230.0).min(230.0) as u8;

                    // 主标题：从 scale 0.12（极远）推进到 1.0，alpha 同步 0→255
                    // ease_out_expo 让开头极快、结尾极慢，有强烈的镜头推进感
                    ANIM.main_scale = 0.12 + t * 0.88;
                    ANIM.main_alpha = (t * 255.0).min(255.0) as u8;

                    // 副标题：此阶段保持隐藏
                    ANIM.sub_scale = 0.6;
                    ANIM.sub_alpha = 0;

                    if p >= 1.0 {
                        STATE = 1;
                        ELAPSED = 0;
                    }
                }
                // ── 阶段1：副标题从下方淡入（主标题已定格） ──
                1 => {
                    let p = (ELAPSED as f32 / 500.0).min(1.0);
                    let t = ease_out_quart(p);

                    ANIM.bg_alpha = 230;
                    ANIM.main_scale = 1.0;
                    ANIM.main_alpha = 255;

                    // 副标题：从 0.7 放大到 1.0，alpha 0→200
                    ANIM.sub_scale = 0.7 + t * 0.3;
                    ANIM.sub_alpha = (t * 200.0).min(200.0) as u8;

                    if p >= 1.0 {
                        STATE = 2;
                        ELAPSED = 0;
                    }
                }
                // ── 阶段2：定格展示 ──
                2 => {
                    ANIM.bg_alpha = 230;
                    ANIM.main_scale = 1.0;
                    ANIM.main_alpha = 255;
                    ANIM.sub_scale = 1.0;
                    ANIM.sub_alpha = 200;

                    if ELAPSED >= 2200 {
                        STATE = 3;
                        ELAPSED = 0;
                    }
                }
                // ── 阶段3：整体直接淡出（不移动、不缩小，像电影结束） ──
                3 => {
                    let p = (ELAPSED as f32 / 700.0).min(1.0);
                    let t = ease_in_quad(p);

                    // 所有元素同步淡出，scale 保持不变
                    let fade = 1.0 - t;
                    ANIM.bg_alpha = (fade * 230.0).max(0.0) as u8;
                    ANIM.main_alpha = (fade * 255.0).max(0.0) as u8;
                    ANIM.sub_alpha = (fade * 200.0).max(0.0) as u8;
                    // scale 始终定格在 1.0，不飘走
                    ANIM.main_scale = 1.0;
                    ANIM.sub_scale = 1.0;

                    if p >= 1.0 {
                        STATE = 4;
                        PostQuitMessage(0);
                        return LRESULT(0);
                    }
                }
                _ => return LRESULT(0),
            }

            let _ = SetLayeredWindowAttributes(hwnd, CK_COLOR, ANIM.bg_alpha, LWA_COLORKEY | LWA_ALPHA);

            let target = HWND(TARGET_HWND.load(Ordering::Relaxed) as *mut c_void);
            if !target.0.is_null() && IsWindow(target).as_bool() {
                let mut rc = RECT::default();
                GetWindowRect(target, &mut rc);
                let mut cur = RECT::default();
                GetWindowRect(hwnd, &mut cur);
                let w = rc.right - rc.left;
                let h = rc.bottom - rc.top;
                if cur.left != rc.left || cur.top != rc.top
                    || (cur.right - cur.left) != w || (cur.bottom - cur.top) != h {
                    SetWindowPos(hwnd, HWND_TOPMOST, rc.left, rc.top, w, h, SWP_NOACTIVATE);
                }
            }

            InvalidateRect(hwnd, None, FALSE);
            LRESULT(0)
        }

        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);

            let mut client = RECT::default();
            GetClientRect(hwnd, &mut client);
            let w = client.right - client.left;
            let h = client.bottom - client.top;

            if let Some(ref mut res) = RES {
                res.ensure_size(hdc, w, h);

                // 1. 纯黑背景
                let bg_brush = CreateSolidBrush(COLORREF(0x000000));
                FillRect(res.mem_dc, &client, bg_brush);
                DeleteObject(bg_brush);

                SetBkMode(res.mem_dc, TRANSPARENT);

                // 2. 自适应字体大小（基于窗口高度）
                let base_main = (h as f32 * 0.20).max(36.0);
                let base_sub = (h as f32 * 0.050).max(14.0);
                let main_size = (base_main * ANIM.main_scale) as i32;
                let sub_size = (base_sub * ANIM.sub_scale) as i32;
                let gap = (base_main * 0.08) as i32;
                let total_h = main_size + gap + sub_size;
                let center_y = (h - total_h) / 2;

                // 3. 主标题 "FDI"
                let hfont_main = CreateFontW(
                    main_size, 0, 0, 0,
                    FW_BOLD.0 as i32,
                    0, 0, 0,
                    DEFAULT_CHARSET.0 as u32,
                    OUT_DEFAULT_PRECIS.0 as u32,
                    CLIP_DEFAULT_PRECIS.0 as u32,
                    ANTIALIASED_QUALITY.0 as u32,
                    (DEFAULT_PITCH.0 | FF_DONTCARE.0) as u32,
                    w!("Segoe UI"),
                );
                let old_font = SelectObject(res.mem_dc, hfont_main);

                let mut main_text: Vec<u16> = "FDI".encode_utf16().collect();
                let main_top = center_y;
                let mut rc_main = RECT {
                    left: 0,
                    top: main_top,
                    right: w,
                    bottom: main_top + main_size,
                };

                // 外发光（暖橙，随主标题 alpha 衰减）
                let glow = (main_size as f32 * 0.06) as i32;
                let glow_alpha = ANIM.main_alpha / 3;
                if glow_alpha > 0 {
                    SetTextColor(res.mem_dc, COLORREF(0xFF8844));
                    let mut r = rc_main;
                    r.left -= glow; r.top -= glow; r.right -= glow; r.bottom -= glow;
                    DrawTextW(res.mem_dc, &mut main_text, &mut r, DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOCLIP);
                }

                // 黑色投影
                let shadow = (main_size as f32 * 0.04) as i32;
                SetTextColor(res.mem_dc, COLORREF(0x000000));
                let mut r = rc_main;
                r.left += shadow; r.top += shadow; r.right += shadow; r.bottom += shadow;
                DrawTextW(res.mem_dc, &mut main_text, &mut r, DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOCLIP);

                // 主文字纯白
                SetTextColor(res.mem_dc, COLORREF(0x00FFFFFF));
                DrawTextW(res.mem_dc, &mut main_text, &mut rc_main, DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOCLIP);

                SelectObject(res.mem_dc, old_font);
                DeleteObject(hfont_main);

                // 4. 副标题
                let hfont_sub = CreateFontW(
                    sub_size, 0, 0, 0,
                    FW_NORMAL.0 as i32,
                    0, 0, 0,
                    DEFAULT_CHARSET.0 as u32,
                    OUT_DEFAULT_PRECIS.0 as u32,
                    CLIP_DEFAULT_PRECIS.0 as u32,
                    ANTIALIASED_QUALITY.0 as u32,
                    (DEFAULT_PITCH.0 | FF_DONTCARE.0) as u32,
                    w!("Segoe UI"),
                );
                let old_font2 = SelectObject(res.mem_dc, hfont_sub);

                let mut sub_text: Vec<u16> = "By FastNow Studio".encode_utf16().collect();
                let sub_top = center_y + main_size + gap;
                let mut rc_sub = RECT {
                    left: 0,
                    top: sub_top,
                    right: w,
                    bottom: sub_top + sub_size,
                };

                // 副标题投影
                let sub_shadow = (sub_size as f32 * 0.08) as i32;
                SetTextColor(res.mem_dc, COLORREF(0x000000));
                let mut r = rc_sub;
                r.left += sub_shadow; r.top += sub_shadow; r.right += sub_shadow; r.bottom += sub_shadow;
                DrawTextW(res.mem_dc, &mut sub_text, &mut r, DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOCLIP);

                // 副标题本体
                SetTextColor(res.mem_dc, COLORREF(0xCCCCCC));
                DrawTextW(res.mem_dc, &mut sub_text, &mut rc_sub, DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOCLIP);

                SelectObject(res.mem_dc, old_font2);
                DeleteObject(hfont_sub);

                // 5. 输出
                BitBlt(hdc, 0, 0, w, h, res.mem_dc, 0, 0, SRCCOPY);
            }

            EndPaint(hwnd, &ps);
            LRESULT(0)
        }

        WM_ERASEBKGND => LRESULT(1),

        WM_DESTROY => {
            if let Some(ref mut res) = RES {
                res.cleanup();
                RES = None;
            }
            PostQuitMessage(0);
            LRESULT(0)
        }

        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}