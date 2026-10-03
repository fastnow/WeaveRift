mod client_bridge;
mod gl_ctx;
mod hud;
mod jni_bridge;
mod logger;
mod math;
mod module_hide;
mod state;

use std::ffi::c_void;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use windows::Win32::Foundation::{BOOL, HINSTANCE, HWND};
use windows::Win32::Graphics::Gdi::{HDC, WindowFromDC};
use windows::Win32::Graphics::OpenGL::wglGetCurrentContext;
use windows::Win32::System::LibraryLoader::{
    GetModuleFileNameW, GetModuleHandleW, GetProcAddress, DisableThreadLibraryCalls,
};
use windows::core::{PCSTR, PCWSTR};

use jni::objects::{JClass, JObject, JValue};
use minhook::MinHook;

extern "C" {
    fn hook_entry(hdc: HDC) -> BOOL;
}

const BRIDGE_CLS: &str = "com/fastnow/weaverift/NativeBridge";
const AGENT_CLS: &str = "com/fastnow/weaverift/WeaveRiftAgent";

type SwapFn = unsafe extern "system" fn(HDC) -> BOOL;

static ORIG_SWAP: AtomicIsize = AtomicIsize::new(0);
static FRAME_HUD: AtomicU64 = AtomicU64::new(0);
static FRAME_FPS: AtomicU64 = AtomicU64::new(0);
static UNHOOK_REQ: AtomicBool = AtomicBool::new(false);
static UNHOOKED: AtomicBool = AtomicBool::new(false);
static SAMPLER: OnceLock<Mutex<jni_bridge::Sampler>> = OnceLock::new();
static LAST_SNAP: OnceLock<Mutex<Option<jni_bridge::Snapshot>>> = OnceLock::new();
static FPS: OnceLock<Mutex<f32>> = OnceLock::new();
static FPS_T0: OnceLock<Mutex<Instant>> = OnceLock::new();
static LOADER_GREF: AtomicIsize = AtomicIsize::new(0);
static GAME_HWND: AtomicIsize = AtomicIsize::new(0);

pub static CLIENT_DLL_PATH: OnceLock<PathBuf> = OnceLock::new();

fn sampler() -> &'static Mutex<jni_bridge::Sampler> {
    SAMPLER.get_or_init(|| Mutex::new(jni_bridge::Sampler::new(20)))
}

fn last_snap() -> &'static Mutex<Option<jni_bridge::Snapshot>> {
    LAST_SNAP.get_or_init(|| Mutex::new(None))
}

pub fn set_sample_hz(hz: u64) {
    if let Ok(mut s) = sampler().lock() {
        *s = jni_bridge::Sampler::new(hz as u32);
    }
}

pub fn request_unhook() {
    UNHOOK_REQ.store(true, Ordering::Relaxed);
}

pub fn is_game_focused() -> bool {
    unsafe {
        use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
        let game = HWND(GAME_HWND.load(Ordering::SeqCst) as *mut c_void);
        if game.0.is_null() { return false; }
        let fg = GetForegroundWindow();
        fg.0 == game.0
    }
}

#[no_mangle]
pub extern "C" fn rust_hook_body(hdc_raw: *mut c_void) -> i32 {
    let hdc = HDC(hdc_raw);
    let hud_frame = FRAME_HUD.fetch_add(1, Ordering::Relaxed);
    let fps_frame = FRAME_FPS.fetch_add(1, Ordering::Relaxed);

    let result = catch_unwind(AssertUnwindSafe(|| {
        let t0 = Instant::now();
        unsafe { tick(hdc, hud_frame); }
        let us = t0.elapsed().as_micros();
        if us > 8000 {
            logger::warn(&format!("本帧耗时 {}us（超 8ms 预算）", us));
        }
    }));

    if result.is_err() {
        logger::error("Rust panic caught in rust_hook_body");
        return -1;
    }

    {
        let t0 = FPS_T0.get_or_init(|| Mutex::new(Instant::now()));
        if let Ok(mut start) = t0.lock() {
            let el = start.elapsed().as_secs_f32();
            if el >= 1.0 {
                let v = fps_frame as f32 / el;
                if let Ok(mut f) = FPS.get_or_init(|| Mutex::new(0.0)).lock() {
                    *f = v;
                }
                FRAME_FPS.store(0, Ordering::Relaxed);
                *start = Instant::now();
            }
        }
    }

    if UNHOOK_REQ.load(Ordering::Relaxed) && !UNHOOKED.load(Ordering::Relaxed) {
        if let Err(e) = unsafe { MinHook::disable_all_hooks() } {
            logger::error(&format!("卸载 hook 失败: {:?}", e));
        } else {
            UNHOOKED.store(true, Ordering::Relaxed);
            logger::info("已卸载 hook");
        }
    }

    0
}

#[no_mangle]
pub extern "C" fn rust_get_original_swap() -> *const c_void {
    ORIG_SWAP.load(Ordering::SeqCst) as *const c_void
}

#[no_mangle]
pub extern "C" fn rust_log_exception(code: u32, where_ptr: *const i8) {
    let where_str = if where_ptr.is_null() {
        "unknown".to_string()
    } else {
        unsafe {
            std::ffi::CStr::from_ptr(where_ptr)
                .to_string_lossy()
                .into_owned()
        }
    };
    logger::error(&format!("SEH exception 0x{:08X} at {}", code, where_str));
}

fn fps() -> f32 {
    FPS.get()
        .and_then(|m| m.lock().ok())
        .map(|v| *v)
        .unwrap_or(0.0)
}

unsafe fn env() -> Result<jni::JNIEnv<'static>, String> {
    let vm = jni_bridge::get_vm().ok_or("VM 未初始化")?;
    vm.attach_current_thread_as_daemon()
        .map_err(|e| format!("attach: {}", e))
}

unsafe fn check_exc(e: &mut jni::JNIEnv, tag: &str) -> Result<(), String> {
    if e.exception_check()
        .map_err(|_| format!("{}: exception_check 失败", tag))?
    {
        let _ = e.exception_describe();
        e.exception_clear()
            .map_err(|_| format!("{}: clear 失败", tag))?;
        return Err(format!("{} 抛出 Java 异常", tag));
    }
    Ok(())
}

unsafe fn load_agent() -> Result<(), String> {
    let mut e = env()?;

    let jar = read_shared_path()
        .map(|(p, _)| p)
        .ok_or("找不到共享内存")?;
    let jar_abs = absolute_no_unc(&jar)?;
    if !jar_abs.exists() {
        return Err(format!("agent jar 不存在: {}", jar_abs.display()));
    }

    let jar_url = format!("file:///{}", jar_abs.to_string_lossy().replace('\\', "/"));
    logger::info(&format!("加载 agent: {}", jar_abs.display()));

    let url_str = e.new_string(&jar_url)
        .map_err(|x| format!("new_string: {}", x))?;
    let url_obj = e.new_object(
        "java/net/URL",
        "(Ljava/lang/String;)V",
        &[JValue::Object(&url_str)],
    ).map_err(|x| format!("new URL: {}", x))?;
    check_exc(&mut e, "new URL")?;

    let urls = e.new_object_array(1, "java/net/URL", &url_obj)
        .map_err(|x| format!("URL[]: {}", x))?;
    check_exc(&mut e, "URL[]")?;

    let parent = e.call_static_method(
        "java/lang/ClassLoader",
        "getSystemClassLoader",
        "()Ljava/lang/ClassLoader;",
        &[],
    ).and_then(|v| v.l())
     .map_err(|x| format!("getSystemClassLoader: {}", x))?;
    check_exc(&mut e, "getSystemClassLoader")?;

    let loader = e.new_object(
        "java/net/URLClassLoader",
        "([Ljava/net/URL;Ljava/lang/ClassLoader;)V",
        &[JValue::Object(&urls), JValue::Object(&parent)],
    ).map_err(|x| format!("URLClassLoader: {}", x))?;
    check_exc(&mut e, "URLClassLoader")?;

    let g = e.new_global_ref(&loader)
        .map_err(|x| format!("global_ref loader: {}", x))?;
    let g = Box::leak(Box::new(g));
    LOADER_GREF.store(g.as_raw() as isize, Ordering::SeqCst);

    let loader_obj: &JObject = g.as_obj();

    let nb_name = e.new_string(BRIDGE_CLS.replace('/', "."))
        .map_err(|x| format!("new_string: {}", x))?;
    let nb = e.call_method(
        loader_obj,
        "loadClass",
        "(Ljava/lang/String;)Ljava/lang/Class;",
        &[JValue::Object(&nb_name)],
    ).and_then(|v| v.l())
     .map_err(|x| format!("loadClass NativeBridge: {}", x))?;
    check_exc(&mut e, "loadClass NativeBridge")?;

    jni_bridge::register_natives(&mut e, nb)?;
    check_exc(&mut e, "RegisterNatives")?;

    let ag_name = e.new_string(AGENT_CLS.replace('/', "."))
        .map_err(|x| format!("new_string: {}", x))?;
    let ag = e.call_method(
        loader_obj,
        "loadClass",
        "(Ljava/lang/String;)Ljava/lang/Class;",
        &[JValue::Object(&ag_name)],
    ).and_then(|v| v.l())
     .map_err(|x| format!("loadClass Agent: {}", x))?;
    check_exc(&mut e, "loadClass Agent")?;

    let srg_path = jar_abs.parent()
        .map(|p| p.join("obf2srg.srg"))
        .unwrap_or_else(|| PathBuf::from("obf2srg.srg"));
    let dll_path = jar_abs.parent()
        .map(|p| p.join("jar_loader.dll"))
        .unwrap_or_else(|| PathBuf::from("jar_loader.dll"));
    let args_str = format!("srg={};dll={}",
        srg_path.to_string_lossy(), dll_path.to_string_lossy());
    logger::info(&format!("agent 参数: {}", args_str));

    let arg = e.new_string(&args_str)
        .map_err(|x| format!("new_string args: {}", x))?;

    e.call_static_method(
        &JClass::from(ag),
        "entry",
        "(Ljava/lang/String;)V",
        &[JValue::Object(&arg)],
    ).map_err(|x| format!("entry: {}", x))?;
    check_exc(&mut e, "entry")?;

    logger::info("agent 已加载并完成 native 注册");
    Ok(())
}

unsafe fn read_mouse_state() -> (f32, f32, bool, bool) {
    use windows::Win32::Foundation::{POINT, RECT};
    use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetClientRect, GetCursorPos, GetForegroundWindow, GetWindowRect,
    };

    let mut pt = POINT::default();
    if GetCursorPos(&mut pt).is_err() {
        return (0.0, 0.0, false, false);
    }

    let game_hwnd = HWND(GAME_HWND.load(Ordering::SeqCst) as *mut c_void);
    let focused = {
        let fg = GetForegroundWindow();
        if game_hwnd.0.is_null() { false } else { fg.0 == game_hwnd.0 }
    };

    let mut client_pt = pt;
    if !game_hwnd.0.is_null() {
        let mut win_rect = RECT::default();
        let mut cli_rect = RECT::default();
        let _ = GetWindowRect(game_hwnd, &mut win_rect);
        let _ = GetClientRect(game_hwnd, &mut cli_rect);

        let border_x = win_rect.left + (win_rect.right - win_rect.left - (cli_rect.right - cli_rect.left)) / 2;
        let border_y = win_rect.top + (win_rect.bottom - win_rect.top - (cli_rect.bottom - cli_rect.top));

        client_pt.x = pt.x - border_x;
        client_pt.y = pt.y - border_y;
    }

    let down = (GetAsyncKeyState(0x01) as u16 & 0x8000) != 0;

    (client_pt.x as f32, client_pt.y as f32, down, focused)
}

unsafe fn tick(hdc: HDC, frame: u64) {
    if UNHOOKED.load(Ordering::Relaxed) {
        return;
    }

    match state::current() {
        state::BOOT => {
            state::set(state::WAIT_GL);
        }

        state::WAIT_GL => {
            if wglGetCurrentContext().is_invalid() {
                state::fail("无 GL context");
                return;
            }

            let hwnd = WindowFromDC(hdc);
            if !hwnd.0.is_null() {
                GAME_HWND.store(hwnd.0 as isize, Ordering::SeqCst);
                logger::info(&format!("游戏窗口句柄: {:?}", hwnd));
            }

            logger::info("GL context 就绪");
            state::set(state::FIND_VM);
        }

        state::FIND_VM => match jni_bridge::find_vm() {
            Ok(()) => state::set(state::CREATE_CTX),
            Err(_) => { state::fail("等待 jvm.dll"); }
        },

        state::CREATE_CTX => {
            if let Err(e) = hud::load_gl() {
                state::fail(&e);
                return;
            }
            if let Err(e) = gl_ctx::create_shared(hdc) {
                state::fail(&e);
                return;
            }
            if gl_ctx::begin() {
                if let Err(e) = hud::init_font(hdc) {
                    logger::warn(&format!("字体初始化失败: {}", e));
                }
                gl_ctx::end();
            }
            state::set(state::LOAD_AGENT);
        }

        state::LOAD_AGENT => match load_agent() {
            Ok(()) => {
                logger::info("agent 已加载");
                state::set(state::WAIT_AGENT);
            }
            Err(e) => {
                logger::error(&format!("LOAD_AGENT 失败: {}", e));
                state::set(state::FAILED);
            }
        },

        state::WAIT_AGENT => {
            if gl_ctx::hdc_changed(hdc) {
                logger::info("HDC 变化，重建 GL context");
                gl_ctx::destroy();
                state::set(state::CREATE_CTX);
                return;
            }
            if !state::can_retry() { return; }

            match jni_bridge::read_snapshot() {
                Some(s) => {
                    if let Ok(mut g) = last_snap().lock() {
                        *g = Some(s);
                    }
                    logger::info("agent 就绪，进入 Running");
                    state::set(state::RUNNING);
                }
                None => state::fail("RiftBridge 不可见"),
            }
        }

        state::RUNNING => {
            {
                static MARKED: AtomicBool = AtomicBool::new(false);
                if !MARKED.swap(true, Ordering::Relaxed) {
                    hud::mark_injected();

                    match client_bridge::load_client() {
                        Ok(()) => logger::info("client 已加载"),
                        Err(e) => logger::warn(&format!("client 加载失败: {}", e)),
                    }
                }
            }

            if gl_ctx::hdc_changed(hdc) {
                logger::info("HDC 变化，重建 GL context");
                gl_ctx::destroy();
                state::set(state::CREATE_CTX);
                return;
            }

            if let Ok(mut sm) = sampler().lock() {
                if let Some(s) = sm.poll() {
                    if let Ok(mut g) = last_snap().lock() {
                        *g = Some(s);
                    }
                }
            }

            let (w, h) = gl_ctx::viewport_size(hdc);
            let snap = last_snap().lock().ok().and_then(|g| g.clone());

            if gl_ctx::begin() {
                hud::draw(w, h, snap.clone(), fps(), frame,
                          state::name(state::RUNNING),
                          jni_bridge::has_vm(), gl_ctx::valid());

                if let Some(s) = snap.as_ref() {
                    if s.valid {
                        client_bridge::update_snapshot(s.clone());

                        let now = Instant::now();
                        let dt_ms = {
                            static LAST: OnceLock<Mutex<Instant>> = OnceLock::new();
                            let m = LAST.get_or_init(|| Mutex::new(now));
                            let mut t = m.lock().unwrap();
                            let d = now.duration_since(*t).as_secs_f32() * 1000.0;
                            *t = now;
                            d
                        };

                        let (mx, my, mdown, focused) = read_mouse_state();

                        {
                            static RSHIFT_DOWN: AtomicBool = AtomicBool::new(false);
                            use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
                            let down = (GetAsyncKeyState(0xA1) as u16 & 0x8000) != 0;
                            let prev = RSHIFT_DOWN.swap(down, Ordering::Relaxed);
                            if down && !prev && focused {
                                client_bridge::on_key(0xA1, true, true);
                            }
                        }

                        client_bridge::on_render(
                            s.x as f32, s.y as f32, s.z as f32,
                            s.yaw, s.pitch, s.health,
                            w, h,
                            dt_ms,
                            mx, my, mdown, focused,
                        );

                        client_bridge::push_mouse_grabbed(client_bridge::mouse_grabbed());
                    }
                }

                if !gl_ctx::end() {
                    logger::warn("还原游戏 context 失败");
                }
            }

            client_bridge::on_tick();
        }

        _ => {}
    }
}

fn absolute_no_unc(p: &str) -> Result<PathBuf, String> {
    let pb = PathBuf::from(p);
    let abs = if pb.is_absolute() { pb }
        else { std::env::current_dir().map_err(|e| e.to_string())?.join(pb) };
    let mut clean = PathBuf::new();
    for comp in abs.components() {
        use std::path::Component;
        match comp {
            Component::ParentDir => { clean.pop(); }
            Component::CurDir => {}
            other => clean.push(other.as_os_str()),
        }
    }
    Ok(clean)
}

#[repr(C)]
struct SharedPath {
    magic: u32,
    mode: u32,
    target_pid: u32,
    jar_path: [u16; 260],
}

const SHARED_NAME: &str = "Local\\WeaveRift.Path";
const MAGIC: u32 = 0x57415645;

fn read_shared_path() -> Option<(String, u32)> {
    use windows::Win32::Foundation::{CloseHandle, BOOL};
    use windows::Win32::System::Memory::{
        MapViewOfFile, OpenFileMappingW, UnmapViewOfFile, FILE_MAP_READ,
    };

    let name_wide: Vec<u16> = SHARED_NAME.encode_utf16().chain(Some(0)).collect();
    let handle = unsafe {
        OpenFileMappingW(FILE_MAP_READ.0, BOOL(0), PCWSTR(name_wide.as_ptr()))
    };
    let h = match handle { Ok(h) => h, Err(_) => return None };

    let view = unsafe {
        MapViewOfFile(h, FILE_MAP_READ, 0, 0, std::mem::size_of::<SharedPath>())
    };
    if view.Value.is_null() {
        unsafe { let _ = CloseHandle(h); }
        return None;
    }

    let shared = view.Value as *const SharedPath;
    if unsafe { (*shared).magic } != MAGIC {
        unsafe {
            let _ = UnmapViewOfFile(view);
            let _ = CloseHandle(h);
        }
        return None;
    }

    let mode = unsafe { (*shared).mode };
    let path_buf = unsafe { (*shared).jar_path };
    let len = path_buf.iter().position(|&c| c == 0).unwrap_or(260);
    let path = String::from_utf16_lossy(&path_buf[..len]);

    unsafe {
        let _ = UnmapViewOfFile(view);
        let _ = CloseHandle(h);
    }
    Some((path, mode))
}

unsafe fn install_hook() -> Result<(), String> {
    let opengl32_name: Vec<u16> = "opengl32.dll\0".encode_utf16().collect();
    let opengl32 = GetModuleHandleW(PCWSTR(opengl32_name.as_ptr()))
        .map_err(|_| "opengl32.dll not loaded")?;

    let target = GetProcAddress(opengl32, PCSTR(b"wglSwapBuffers\0".as_ptr()))
        .ok_or("wglSwapBuffers not found")?;

    let trampoline = MinHook::create_hook(target as *mut c_void, hook_entry as *mut c_void)
        .map_err(|e| format!("create_hook failed: {:?}", e))?;

    MinHook::enable_all_hooks().map_err(|e| format!("enable_all_hooks failed: {:?}", e))?;

    ORIG_SWAP.store(trampoline as isize, Ordering::SeqCst);

    logger::info(&format!("wglSwapBuffers 已 hook (orig @ {:#x})", trampoline as isize));
    Ok(())
}

#[no_mangle]
pub extern "system" fn DllMain(hinst: HINSTANCE, reason: u32, _reserved: *mut c_void) -> BOOL {
    if reason == 1 {
        logger::init_file();

        unsafe {
            let mut buf = [0u16; 260];
            let len = GetModuleFileNameW(hinst, &mut buf);
            if len > 0 {
                let p = PathBuf::from(String::from_utf16_lossy(&buf[..len as usize]));
                if let Some(dir) = p.parent() {
                    let client = dir.join("client.dll");
                    logger::info(&format!("client.dll 路径: {}", client.display()));
                    let _ = CLIENT_DLL_PATH.set(client);
                }
            }
        }

        unsafe {
            let base = hinst.0 as *mut c_void;
            if module_hide::hide_module(base) {
                logger::info("PEB 断链完成");
            } else {
                logger::warn("PEB 断链失败");
            }
        }

        let _ = unsafe { DisableThreadLibraryCalls(hinst) };

        unsafe {
            match install_hook() {
                Ok(()) => logger::info("DllMain 完成（未建任何线程）"),
                Err(e) => {
                    logger::error(&format!("hook 安装失败: {}", e));
                    state::set(state::FAILED);
                }
            }
        }
    }
    BOOL(1)
}