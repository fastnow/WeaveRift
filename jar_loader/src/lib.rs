//! WeaveRift native loader —— v3

mod gl_ctx;
mod hud;
mod http_debug;
mod ipc;
mod jni_bridge;
mod logger;
mod module_hide;
mod state;

use std::ffi::c_void;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use windows::Win32::Foundation::{BOOL, HINSTANCE};
use windows::Win32::Graphics::Gdi::HDC;
use windows::Win32::Graphics::OpenGL::wglGetCurrentContext;
use windows::Win32::System::LibraryLoader::{
    GetModuleHandleW, GetProcAddress, DisableThreadLibraryCalls,
};
use windows::core::{PCSTR, PCWSTR};

use minhook::MinHook;

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

// ───────────────────────── Hook ─────────────────────────

unsafe extern "system" fn hooked_wgl_swap_buffers(hdc: HDC) -> BOOL {
    let hud_frame = FRAME_HUD.fetch_add(1, Ordering::Relaxed);
    let fps_frame = FRAME_FPS.fetch_add(1, Ordering::Relaxed);

    let _ = catch_unwind(AssertUnwindSafe(|| {
        let t0 = Instant::now();
        tick(hdc, hud_frame);
        let us = t0.elapsed().as_micros();
        if us > 8000 {
            logger::warn(&format!("本帧耗时 {}us（超 8ms 预算）", us));
        }
    }));

    // fps 统计
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
        if let Err(e) = MinHook::disable_all_hooks() {
            logger::error(&format!("卸载 hook 失败: {:?}", e));
        } else {
            UNHOOKED.store(true, Ordering::Relaxed);
            logger::info("已卸载 hook");
        }
    }

    let p = ORIG_SWAP.load(Ordering::SeqCst);
    if p == 0 {
        return BOOL(0);
    }
    let orig: SwapFn = std::mem::transmute::<isize, SwapFn>(p);
    orig(hdc)
}

fn fps() -> f32 {
    FPS.get().and_then(|m| m.lock().ok()).map(|v| *v).unwrap_or(0.0)
}

// ─────────────────────── 状态机 ───────────────────────

unsafe fn tick(hdc: HDC, frame: u64) {
    if UNHOOKED.load(Ordering::Relaxed) {
        return;
    }

    match state::current() {
        state::BOOT => {
            state::set(state::START_HTTP);
        }

        state::START_HTTP => {
            http_debug::start();
            state::set(state::WAIT_GL);
        }

        state::WAIT_GL => {
            if wglGetCurrentContext().is_invalid() {
                state::fail("无 GL context");
                return;
            }
            logger::info("GL context 就绪");
            state::set(state::FIND_VM);
        }

        state::FIND_VM => match jni_bridge::find_vm() {
            Ok(()) => state::set(state::CREATE_CTX),
            Err(e) => state::fail(&e),
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

        state::LOAD_AGENT => {
            // ★ 自己加载 agent，不走 Attach API
            match load_agent_via_classloader() {
                Ok(()) => {
                    logger::info("agent 已加载");
                    state::set(state::WAIT_AGENT);
                }
                Err(e) => {
                    logger::error(&format!("agent 加载失败: {}", e));
                    state::set(state::FAILED);
                }
            }
        }

        state::WAIT_AGENT => {
            if gl_ctx::hdc_changed(hdc) {
                logger::info("HDC 变化，重建 GL context");
                gl_ctx::destroy();
                state::set(state::CREATE_CTX);
                return;
            }
            // 时间驱动重试
            if !state::can_retry() {
                return;
            }
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

            if !http_debug::HUD_ON.load(Ordering::Relaxed) {
                return;
            }

            let (w, h) = gl_ctx::viewport_size(hdc);
            let snap = last_snap().lock().ok().and_then(|g| *g);

            if gl_ctx::begin() {
                hud::draw(
                    w,
                    h,
                    snap,
                    fps(),
                    frame,
                    state::name(state::RUNNING),
                    jni_bridge::has_vm(),
                    gl_ctx::valid(),
                );
                if !gl_ctx::end() {
                    logger::warn("还原游戏 context 失败");
                }
            }
        }

        _ => {}
    }
}

// ─────────────────────── Agent 加载 ───────────────────────

fn absolute_no_unc(p: &str) -> Result<PathBuf, String> {
    let pb = PathBuf::from(p);
    let abs = if pb.is_absolute() {
        pb
    } else {
        std::env::current_dir()
            .map_err(|e| e.to_string())?
            .join(pb)
    };
    let mut clean = PathBuf::new();
    for comp in abs.components() {
        use std::path::Component;
        match comp {
            Component::ParentDir => {
                clean.pop();
            }
            Component::CurDir => {}
            other => clean.push(other.as_os_str()),
        }
    }
    Ok(clean)
}

/// 用 URLClassLoader 加载 agent jar，反射调 WeaveRiftAgent.agentmain
unsafe fn load_agent_via_classloader() -> Result<(), String> {
    // 1. 从共享内存读 JAR 路径（WeaveRift.exe 写的）
    let (jar_path, _mode) = read_shared_path().ok_or("找不到共享内存（WeaveRift.exe 没写？）")?;
    let jar_abs = absolute_no_unc(&jar_path)?;

    if !jar_abs.exists() {
        return Err(format!("agent jar 不存在: {}", jar_abs.display()));
    }

    // 从 JAR 路径推 SRG 和 DLL
    let parent = jar_abs.parent().ok_or("jar 没有父目录")?;
    let srg_path = parent.join("obf2srg.srg");
    let dll_path = parent.join("jar_loader.dll");

    logger::info(&format!("加载 agent: {}", jar_abs.display()));
    logger::info(&format!("SRG: {}", srg_path.display()));
    logger::info(&format!("DLL: {}", dll_path.display()));

    // 2. 拿 JNIEnv
    let vm = jni_bridge::get_vm().ok_or("VM 未初始化")?;
    let mut env = vm.get_env().map_err(|e| format!("get_env: {}", e))?;

    // 3. 构造 file:/// URL
    let jar_url = format!("file:///{}", jar_abs.to_string_lossy().replace('\\', "/"));
    logger::debug(&format!("jar url: {}", jar_url));

    // 4. new URL(urlStr)
    let url_cls = env
        .find_class("java/net/URL")
        .map_err(|e| format!("find URL: {}", e))?;
    let url_str = env.new_string(&jar_url).map_err(|e| format!("new_string: {}", e))?;
    let url_obj = env
        .new_object(&url_cls, "(Ljava/lang/String;)V", &[jni::objects::JValue::Object(&url_str)])
        .map_err(|e| format!("new URL: {}", e))?;

    // 5. URL[] 数组
    let url_arr = env
        .new_object_array(1, &url_cls, jni::objects::JObject::null())
        .map_err(|e| format!("new URL[]: {}", e))?;
    env.set_object_array_element(&url_arr, 0, &url_obj)
        .map_err(|e| format!("set array: {}", e))?;

    // 6. 拿 SystemClassLoader
    let parent_loader = env
        .call_static_method(
            "java/lang/ClassLoader",
            "getSystemClassLoader",
            "()Ljava/lang/ClassLoader;",
            &[],
        )
        .map_err(|e| format!("getSystemClassLoader: {}", e))?
        .l()
        .map_err(|_| "system loader null")?;

    // 7. new URLClassLoader(URL[], parent)
    let ucl_cls = env
        .find_class("java/net/URLClassLoader")
        .map_err(|e| format!("find URLClassLoader: {}", e))?;
    let ucl_obj = env
        .new_object(
            &ucl_cls,
            "([Ljava/net/URL;Ljava/lang/ClassLoader;)V",
            &[
                jni::objects::JValue::Object(&url_arr),
                jni::objects::JValue::Object(&parent_loader),
            ],
        )
        .map_err(|e| format!("new URLClassLoader: {}", e))?;

    // 8. setContextClassLoader（让后续 NativeBridge/Class.forName 能命中 agent 的类）
    let thread_cls = env
        .find_class("java/lang/Thread")
        .map_err(|e| format!("find Thread: {}", e))?;
    let current_thread = env
        .call_static_method(&thread_cls, "currentThread", "()Ljava/lang/Thread;", &[])
        .map_err(|e| format!("currentThread: {}", e))?
        .l()
        .map_err(|_| "thread null")?;
    env.call_method(
        &current_thread,
        "setContextClassLoader",
        "(Ljava/lang/ClassLoader;)V",
        &[jni::objects::JValue::Object(&ucl_obj)],
    )
    .map_err(|e| format!("setContextClassLoader: {}", e))?;

    // 9. 加载 WeaveRiftAgent 类
    let agent_cls_name = env
        .new_string("com.fastnow.weaverift.WeaveRiftAgent")
        .map_err(|e| format!("new_string agent: {}", e))?;
    let agent_cls = env
        .call_method(
            &ucl_obj,
            "loadClass",
            "(Ljava/lang/String;)Ljava/lang/Class;",
            &[jni::objects::JValue::Object(&agent_cls_name)],
        )
        .map_err(|e| format!("loadClass: {}", e))?
        .l()
        .map_err(|_| "agent class null")?;

    // 10. 反射调 WeaveRiftAgent.agentmain(String, Instrumentation)
    //     Instrumentation 传 null（我们不需要它）
    let agent_cls_jclass = jni::objects::JClass::from(agent_cls);
    let agentmain_mid = env
        .get_static_method_id(
            &agent_cls_jclass,
            "agentmain",
            "(Ljava/lang/String;Ljava/lang/instrument/Instrumentation;)V",
        )
        .map_err(|e| format!("get agentmain: {}", e))?;

    // 参数：srg=...;dll=...
    let options = format!(
        "srg={};dll={}",
        srg_path.to_string_lossy(),
        dll_path.to_string_lossy()
    );
    logger::info(&format!("agent 参数: {}", options));

    let options_jstr = env
        .new_string(&options)
        .map_err(|e| format!("new_string options: {}", e))?;

    let null_inst = jni::objects::JObject::null();
    let args: [jni::sys::jvalue; 2] = [
        jni::objects::JValue::Object(&options_jstr).as_jni(),
        jni::objects::JValue::Object(&null_inst).as_jni(),
    ];

    unsafe {
        env.call_static_method_unchecked(
            &agent_cls_jclass,
            agentmain_mid,
            jni::signature::ReturnType::Primitive(jni::signature::Primitive::Void),
            &args,
        )
        .map_err(|e| format!("agentmain 调用失败: {}", e))?;
    }

    logger::info("WeaveRiftAgent.agentmain 已调用");
    Ok(())
}

// ─────────────────── 共享内存 ───────────────────

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
    use windows::Win32::System::Memory::{
        OpenFileMappingW, MapViewOfFile, FILE_MAP_READ,
    };
    use windows::Win32::Foundation::{CloseHandle, BOOL};

    let name_wide: Vec<u16> = SHARED_NAME.encode_utf16().chain(Some(0)).collect();
    let handle = unsafe {
        OpenFileMappingW(FILE_MAP_READ.0, BOOL(0), PCWSTR(name_wide.as_ptr()))
    };
    let h = match handle {
        Ok(h) => h,
        Err(_) => return None,
    };

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
            let _ = windows::Win32::System::Memory::UnmapViewOfFile(view);
            let _ = CloseHandle(h);
        }
        return None;
    }

    let mode = unsafe { (*shared).mode };
    let path_buf = unsafe { (*shared).jar_path };
    let len = path_buf.iter().position(|&c| c == 0).unwrap_or(260);
    let path = String::from_utf16_lossy(&path_buf[..len]);

    unsafe {
        let _ = windows::Win32::System::Memory::UnmapViewOfFile(view);
        let _ = CloseHandle(h);
    }
    Some((path, mode))
}

// ─────────────────────── 安装 Hook ───────────────────────

unsafe fn install_hook() -> Result<(), String> {
    let opengl32_name: Vec<u16> = "opengl32.dll\0".encode_utf16().collect();
    let opengl32 = GetModuleHandleW(PCWSTR(opengl32_name.as_ptr()))
        .map_err(|_| "opengl32.dll not loaded")?;

    let target = GetProcAddress(opengl32, PCSTR("wglSwapBuffers\0".as_ptr()))
        .ok_or("wglSwapBuffers not found")?;

    let trampoline = MinHook::create_hook(
        target as *mut c_void,
        hooked_wgl_swap_buffers as *mut c_void,
    )
    .map_err(|e| format!("create_hook failed: {:?}", e))?;

    MinHook::enable_all_hooks()
        .map_err(|e| format!("enable_all_hooks failed: {:?}", e))?;

    ORIG_SWAP.store(trampoline as isize, Ordering::SeqCst);

    logger::info(&format!(
        "wglSwapBuffers 已 hook (orig @ {:#x})",
        trampoline as isize
    ));
    Ok(())
}

// ─────────────────────── DllMain ───────────────────────

#[no_mangle]
pub extern "system" fn DllMain(hinst: HINSTANCE, reason: u32, _reserved: *mut c_void) -> BOOL {
    if reason == 1 {
        logger::init_file();

        // PEB 断链
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