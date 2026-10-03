use std::ffi::c_void;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::OnceLock;

use windows::Win32::Foundation::HMODULE;
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows::core::{PCSTR, PCWSTR};

use crate::jni_bridge::Snapshot;
use crate::logger;

type ClientMain = unsafe extern "C" fn(*const RiftAPI) -> i32;
type ClientOnRender = unsafe extern "C" fn(
    f32, f32, f32, f32, f32, f32,
    i32, i32,
    f32, f32, f32, bool, i32,
);
type ClientOnTick = unsafe extern "C" fn();
type ClientOnKey = unsafe extern "C" fn(u32, bool, i32);

#[repr(C)]
pub struct RiftAPI {
    pub version: u32,
    pub log: Option<unsafe extern "C" fn(*const u8, usize)>,
    pub get_player: Option<unsafe extern "C" fn(*mut f32)>,
    pub get_entity_count: Option<unsafe extern "C" fn() -> i32>,
    pub get_entity: Option<unsafe extern "C" fn(i32, *mut f32, *mut i32) -> i32>,
    pub screen_size: Option<unsafe extern "C" fn(*mut i32, *mut i32)>,
    pub project: Option<unsafe extern "C" fn(f32, f32, f32, *mut f32, *mut f32) -> i32>,
    pub draw_line: Option<unsafe extern "C" fn(f32, f32, f32, f32, *const f32)>,
    pub draw_rect: Option<unsafe extern "C" fn(f32, f32, f32, f32, *const f32)>,
    pub draw_text: Option<unsafe extern "C" fn(f32, f32, *const u8, usize, *const f32)>,
    pub draw_triangle: Option<unsafe extern "C" fn(f32, f32, f32, f32, f32, f32, *const f32)>,
    pub set_mouse_grabbed: Option<unsafe extern "C" fn(i32)>,
    pub is_game_focused: Option<unsafe extern "C" fn() -> i32>,
}

struct ClientFns {
    on_render: ClientOnRender,
    on_tick: ClientOnTick,
    on_key: ClientOnKey,
}

static CLIENT: OnceLock<ClientFns> = OnceLock::new();
static API: OnceLock<RiftAPI> = OnceLock::new();
static SNAP: OnceLock<std::sync::Mutex<Option<Snapshot>>> = OnceLock::new();
static SCREEN: OnceLock<std::sync::Mutex<(i32, i32)>> = OnceLock::new();
static MOUSE_GRABBED: AtomicI32 = AtomicI32::new(1);

fn snap_lock() -> &'static std::sync::Mutex<Option<Snapshot>> {
    SNAP.get_or_init(|| std::sync::Mutex::new(None))
}

fn screen_lock() -> &'static std::sync::Mutex<(i32, i32)> {
    SCREEN.get_or_init(|| std::sync::Mutex::new((854, 480)))
}

pub fn mouse_grabbed() -> bool {
    MOUSE_GRABBED.load(Ordering::SeqCst) != 0
}

unsafe extern "C" fn api_log(ptr: *const u8, len: usize) {
    if ptr.is_null() || len == 0 { return; }
    let slice = std::slice::from_raw_parts(ptr, len);
    let msg = String::from_utf8_lossy(slice);
    logger::info(&format!("[client] {}", msg));
}

unsafe extern "C" fn api_get_player(out: *mut f32) {
    if out.is_null() { return; }
    let s = snap_lock().lock().ok().and_then(|g| g.clone());
    if let Some(s) = s {
        *out.add(0) = s.x as f32;
        *out.add(1) = s.y as f32;
        *out.add(2) = s.z as f32;
        *out.add(3) = s.yaw;
        *out.add(4) = s.pitch;
        *out.add(5) = s.health;
    }
}

unsafe extern "C" fn api_get_entity_count() -> i32 {
    let s = snap_lock().lock().ok().and_then(|g| g.clone());
    s.map(|s| s.entity_list.len() as i32).unwrap_or(0)
}

unsafe extern "C" fn api_get_entity(idx: i32, out: *mut f32, kind: *mut i32) -> i32 {
    if out.is_null() || kind.is_null() || idx < 0 { return 0; }
    let s = snap_lock().lock().ok().and_then(|g| g.clone());
    let s = match s { Some(s) => s, None => return 0 };
    let i = idx as usize;
    if i >= s.entity_list.len() { return 0; }
    let e = &s.entity_list[i];
    *out.add(0) = e.x as f32;
    *out.add(1) = e.y as f32;
    *out.add(2) = e.z as f32;
    *out.add(3) = e.health;
    *out.add(4) = e.kind as f32;
    *out.add(5) = 0.0;
    *kind = e.kind;
    1
}

unsafe extern "C" fn api_screen_size(w: *mut i32, h: *mut i32) {
    if w.is_null() || h.is_null() { return; }
    let s = *screen_lock().lock().unwrap();
    *w = s.0;
    *h = s.1;
}

unsafe extern "C" fn api_project(wx: f32, wy: f32, wz: f32,
                                 sx: *mut f32, sy: *mut f32) -> i32 {
    if sx.is_null() || sy.is_null() { return 0; }
    let s = snap_lock().lock().ok().and_then(|g| g.clone());
    let s = match s { Some(s) => s, None => return 0 };

    let yaw_norm = {
        let y = ((s.yaw % 360.0) + 360.0) % 360.0;
        if y > 180.0 { y - 360.0 } else { y }
    };

    let eye = [s.x as f32, (s.y + 1.62) as f32, s.z as f32];
    let (w, h) = *screen_lock().lock().unwrap();

    let proj = crate::math::Projector::new(
        eye, yaw_norm, s.pitch, 70.0,
        0, 0, w, h,
    );

    match proj.project([wx, wy, wz]) {
        Some([px, py]) => { *sx = px; *sy = py; 1 }
        None => 0,
    }
}

unsafe extern "C" fn api_draw_line(x1: f32, y1: f32, x2: f32, y2: f32,
                                   rgba: *const f32) {
    if rgba.is_null() { return; }
    let c = std::slice::from_raw_parts(rgba, 4);
    crate::hud::draw_raw_line(x1, y1, x2, y2, c[0], c[1], c[2], c[3]);
}

unsafe extern "C" fn api_draw_rect(x: f32, y: f32, w: f32, h: f32,
                                   rgba: *const f32) {
    if rgba.is_null() { return; }
    let c = std::slice::from_raw_parts(rgba, 4);
    crate::hud::draw_raw_rect(x, y, w, h, c[0], c[1], c[2], c[3]);
}

unsafe extern "C" fn api_draw_text(x: f32, y: f32, ptr: *const u8, len: usize,
                                   rgba: *const f32) {
    if ptr.is_null() || rgba.is_null() { return; }
    let slice = std::slice::from_raw_parts(ptr, len);
    let text = String::from_utf8_lossy(slice);
    let c = std::slice::from_raw_parts(rgba, 4);
    crate::hud::draw_raw_text(x, y, &text, c[0], c[1], c[2], c[3]);
}

unsafe extern "C" fn api_draw_triangle(x1: f32, y1: f32, x2: f32, y2: f32,
                                       x3: f32, y3: f32, rgba: *const f32) {
    if rgba.is_null() { return; }
    let c = std::slice::from_raw_parts(rgba, 4);
    crate::hud::draw_raw_triangle(x1, y1, x2, y2, x3, y3, c[0], c[1], c[2], c[3]);
}

unsafe extern "C" fn api_set_mouse_grabbed(grabbed: i32) {
    MOUSE_GRABBED.store(grabbed, Ordering::SeqCst);
}

unsafe extern "C" fn api_is_game_focused() -> i32 {
    if crate::is_game_focused() { 1 } else { 0 }
}

fn build_api() -> RiftAPI {
    RiftAPI {
        version: 2,
        log: Some(api_log),
        get_player: Some(api_get_player),
        get_entity_count: Some(api_get_entity_count),
        get_entity: Some(api_get_entity),
        screen_size: Some(api_screen_size),
        project: Some(api_project),
        draw_line: Some(api_draw_line),
        draw_rect: Some(api_draw_rect),
        draw_text: Some(api_draw_text),
        draw_triangle: Some(api_draw_triangle),
        set_mouse_grabbed: Some(api_set_mouse_grabbed),
        is_game_focused: Some(api_is_game_focused),
    }
}

pub unsafe fn load_client() -> Result<(), String> {
    let client_path = crate::CLIENT_DLL_PATH
        .get()
        .ok_or("CLIENT_DLL_PATH 未设置")?
        .clone();

    logger::info(&format!("加载 client: {}", client_path.display()));

    if !client_path.exists() {
        return Err(format!("client.dll 不存在: {}", client_path.display()));
    }

    let path: Vec<u16> = client_path
        .to_string_lossy()
        .encode_utf16()
        .chain(Some(0))
        .collect();

    let hmod: HMODULE = LoadLibraryW(PCWSTR(path.as_ptr()))
        .map_err(|e| format!("LoadLibrary {}: {:?}", client_path.display(), e))?;

    let main_ptr = GetProcAddress(hmod, PCSTR(b"client_main\0".as_ptr()))
        .ok_or("client_main not found")?;
    let render_ptr = GetProcAddress(hmod, PCSTR(b"client_on_render\0".as_ptr()))
        .ok_or("client_on_render not found")?;
    let tick_ptr = GetProcAddress(hmod, PCSTR(b"client_on_tick\0".as_ptr()))
        .ok_or("client_on_tick not found")?;
    let key_ptr = GetProcAddress(hmod, PCSTR(b"client_on_key\0".as_ptr()))
        .ok_or("client_on_key not found")?;

    let main: ClientMain = std::mem::transmute(main_ptr);
    let on_render: ClientOnRender = std::mem::transmute(render_ptr);
    let on_tick: ClientOnTick = std::mem::transmute(tick_ptr);
    let on_key: ClientOnKey = std::mem::transmute(key_ptr);

    let api = API.get_or_init(build_api);

    let rc = main(api as *const RiftAPI);
    if rc != 0 {
        return Err(format!("client_main rc={}", rc));
    }

    let _ = CLIENT.set(ClientFns { on_render, on_tick, on_key });
    Ok(())
}

pub fn on_render(px: f32, py: f32, pz: f32,
                 yaw: f32, pitch: f32, hp: f32,
                 w: i32, h: i32,
                 dt_ms: f32,
                 mx: f32, my: f32, mdown: bool, focused: bool) {
    if let Ok(mut s) = screen_lock().lock() { *s = (w, h); }
    if let Some(c) = CLIENT.get() {
        unsafe {
            (c.on_render)(px, py, pz, yaw, pitch, hp, w, h,
                          dt_ms, mx, my, mdown,
                          if focused { 1 } else { 0 });
        }
    }
}

pub fn on_tick() {
    if let Some(c) = CLIENT.get() {
        unsafe { (c.on_tick)(); }
    }
}

pub fn on_key(vk: u32, down: bool, focused: bool) {
    if let Some(c) = CLIENT.get() {
        unsafe {
            (c.on_key)(vk, down, if focused { 1 } else { 0 });
        }
    }
}

pub fn update_snapshot(s: Snapshot) {
    if let Ok(mut g) = snap_lock().lock() {
        *g = Some(s);
    }
}

pub fn is_loaded() -> bool {
    CLIENT.get().is_some()
}

pub fn push_mouse_grabbed(grabbed: bool) {
    crate::jni_bridge::push_mouse_grabbed(grabbed);
}