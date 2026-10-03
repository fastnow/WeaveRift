use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::time::Instant;

use windows::Win32::Graphics::Gdi::HDC;
use windows::Win32::Graphics::OpenGL::wglUseFontBitmapsA;
use windows::Win32::System::LibraryLoader::{
    GetModuleHandleW, GetProcAddress,
};
use windows::core::{PCSTR, PCWSTR};

use crate::jni_bridge::Snapshot;
use crate::logger;

type F1 = unsafe extern "system" fn(u32);
type F2 = unsafe extern "system" fn(u32, u32);
type F2f = unsafe extern "system" fn(f32, f32);
type F4f = unsafe extern "system" fn(f32, f32, f32, f32);
type F6d = unsafe extern "system" fn(f64, f64, f64, f64, f64, f64);
type V = unsafe extern "system" fn();
type CallLists = unsafe extern "system" fn(i32, u32, *const c_void);

struct GlFns {
    push_attrib: F1,
    pop_attrib: V,
    push_matrix: V,
    pop_matrix: V,
    matrix_mode: F1,
    load_identity: V,
    ortho: F6d,
    disable: F1,
    enable: F1,
    blend_func: F2,
    color4f: F4f,
    begin: F1,
    end: V,
    vertex2f: F2f,
    line_width: F1,
    raster_pos2f: F2f,
    list_base: F1,
    call_lists: CallLists,
}

static GL_FNS: OnceLock<GlFns> = OnceLock::new();
static FONT_BASE: OnceLock<u32> = OnceLock::new();
static HUD_VISIBLE: AtomicBool = AtomicBool::new(true);
static INJECT_TIME: OnceLock<Instant> = OnceLock::new();

pub fn toggle_hud() -> bool {
    let cur = HUD_VISIBLE.load(Ordering::Relaxed);
    let new = !cur;
    HUD_VISIBLE.store(new, Ordering::Relaxed);
    new
}

pub fn hud_visible() -> bool {
    HUD_VISIBLE.load(Ordering::Relaxed)
}

pub fn mark_injected() {
    let _ = INJECT_TIME.set(Instant::now());
}

pub unsafe fn load_gl() -> Result<(), String> {
    let opengl32_name: Vec<u16> = "opengl32.dll\0".encode_utf16().collect();
    let hmod = GetModuleHandleW(PCWSTR(opengl32_name.as_ptr()))
        .map_err(|_| "opengl32.dll not loaded")?;

    macro_rules! load {
        ($name:literal, $ty:ty) => {
            std::mem::transmute::<_, $ty>(
                GetProcAddress(hmod, PCSTR(concat!($name, "\0").as_ptr()))
                    .ok_or(concat!($name, " not found"))?)
        };
    }

    let fns = GlFns {
        push_attrib:   load!("glPushAttrib", F1),
        pop_attrib:    load!("glPopAttrib", V),
        push_matrix:   load!("glPushMatrix", V),
        pop_matrix:    load!("glPopMatrix", V),
        matrix_mode:   load!("glMatrixMode", F1),
        load_identity: load!("glLoadIdentity", V),
        ortho:         load!("glOrtho", F6d),
        disable:       load!("glDisable", F1),
        enable:        load!("glEnable", F1),
        blend_func:    load!("glBlendFunc", F2),
        color4f:       load!("glColor4f", F4f),
        begin:         load!("glBegin", F1),
        end:           load!("glEnd", V),
        vertex2f:      load!("glVertex2f", F2f),
        line_width:    load!("glLineWidth", F1),
        raster_pos2f:  load!("glRasterPos2f", F2f),
        list_base:     load!("glListBase", F1),
        call_lists:    load!("glCallLists", CallLists),
    };

    GL_FNS.set(fns).map_err(|_| "GL_FNS already set")?;
    logger::info("HUD GL 函数指针加载完成");
    Ok(())
}

fn gl() -> &'static GlFns {
    GL_FNS.get().expect("GL_FNS not loaded")
}

pub unsafe fn init_font(hdc: HDC) -> Result<(), String> {
    let font = windows::Win32::Graphics::Gdi::CreateFontW(
        14, 0, 0, 0,
        400, 0, 0, 0,
        1, 0, 0, 0,
        0,
        windows::core::PCWSTR(b"Consolas\0".as_ptr() as *const u16),
    );
    if font.is_invalid() {
        return Err("CreateFontW failed".into());
    }

    let old = windows::Win32::Graphics::Gdi::SelectObject(hdc, font);
    let base = gl_gen_lists(96);
    if base == 0 {
        windows::Win32::Graphics::Gdi::SelectObject(hdc, old);
        return Err("glGenLists returned 0".into());
    }
    let ok = wglUseFontBitmapsA(hdc, 32, 96, base).is_ok();
    windows::Win32::Graphics::Gdi::SelectObject(hdc, old);

    if !ok {
        return Err("wglUseFontBitmapsA failed".into());
    }
    let _ = FONT_BASE.set(base);
    logger::debug("HUD 字体初始化完成");
    Ok(())
}

unsafe fn gl_gen_lists(count: u32) -> u32 {
    let opengl32_name: Vec<u16> = "opengl32.dll\0".encode_utf16().collect();
    let hmod = GetModuleHandleW(PCWSTR(opengl32_name.as_ptr())).unwrap();
    type Fn = unsafe extern "system" fn(i32) -> u32;
    let ptr = GetProcAddress(hmod, PCSTR(b"glGenLists\0".as_ptr())).unwrap();
    let f: Fn = std::mem::transmute(ptr);
    f(count as i32)
}

pub unsafe fn draw(
    w: i32,
    h: i32,
    snap: Option<Snapshot>,
    fps: f32,
    frame: u64,
    state: &str,
    _vm_ok: bool,
    _ctx_ok: bool,
) {
    if !hud_visible() {
        return;
    }

    let g = gl();

    (g.push_attrib)(0x000FFFFF);
    (g.push_matrix)();

    (g.matrix_mode)(0x1701);
    (g.load_identity)();
    (g.ortho)(0.0, w as f64, h as f64, 0.0, -1.0, 1.0);

    (g.matrix_mode)(0x1700);
    (g.load_identity)();

    (g.disable)(0x0B71);
    (g.disable)(0x0DE1);
    (g.enable)(0x0BE2);
    (g.blend_func)(0x0302, 0x0303);

    draw_intro(w, h);

    let ok_c = (0.55, 0.95, 0.55, 1.0);
    let bad_c = (1.0, 0.4, 0.4, 1.0);
    let dim_c = (0.75, 0.78, 0.82, 1.0);

    draw_line(10.0, 40.0, dim_c, &format!("WeaveRift  f{}", frame));
    draw_line(10.0, 58.0, if fps > 5.0 { ok_c } else { bad_c },
              &format!("hook  {:.0} Hz", fps));
    draw_line(10.0, 76.0, dim_c, &format!("state {}", state));

    match snap {
        Some(s) if s.valid => {
            draw_line(10.0, 100.0, dim_c,
                      &format!("xyz   {:.1} {:.1} {:.1}", s.x, s.y, s.z));
            draw_line(10.0, 118.0, dim_c,
                      &format!("rot   {:.1} / {:.1}", s.yaw, s.pitch));
            draw_line(10.0, 136.0, dim_c,
                      &format!("hp    {:.0}/{:.0}   ents {}",
                               s.health, s.max_health, s.entity_list.len()));
        }
        _ => {
            draw_line(10.0, 100.0, bad_c, "no snapshot");
        }
    }

    (g.pop_matrix)();
    (g.pop_attrib)();
}

unsafe fn draw_intro(w: i32, h: i32) {
    let t = match INJECT_TIME.get() {
        Some(t) => t,
        None => return,
    };
    let elapsed_ms = t.elapsed().as_millis() as f32;
    if elapsed_ms > 4500.0 {
        return;
    }

    let alpha = if elapsed_ms < 2500.0 {
        1.0
    } else {
        1.0 - (elapsed_ms - 2500.0) / 2000.0
    };

    let g = gl();

    (g.color4f)(0.0, 0.0, 0.0, alpha * 0.85);
    (g.begin)(0x0007);
    (g.vertex2f)(0.0, 0.0);
    (g.vertex2f)(w as f32, 0.0);
    (g.vertex2f)(w as f32, h as f32);
    (g.vertex2f)(0.0, h as f32);
    (g.end)();

    (g.matrix_mode)(0x1701);
    (g.push_matrix)();
    (g.load_identity)();
    (g.ortho)(0.0, (w as f32 / 3.0) as f64, (h as f32 / 3.0) as f64, 0.0, -1.0, 1.0);

    (g.matrix_mode)(0x1700);
    (g.push_matrix)();
    (g.load_identity)();

    let cx = (w as f32 / 2.0) / 3.0;
    let cy = (h as f32 / 2.0) / 3.0;

    (g.color4f)(0.2, 0.7, 1.0, alpha);
    draw_text_centered(cx, cy - 6.0, "WeaveRift");

    (g.color4f)(0.5, 0.5, 0.5, alpha * 0.8);
    draw_text_centered(cx, cy + 14.0, "Weave through the rift.");

    (g.matrix_mode)(0x1700);
    (g.pop_matrix)();

    (g.matrix_mode)(0x1701);
    (g.pop_matrix)();

    (g.matrix_mode)(0x1700);
}

unsafe fn draw_text_centered(x: f32, y: f32, text: &str) {
    let base = match FONT_BASE.get() {
        Some(b) => *b,
        None => return,
    };
    let g = gl();
    let approx_w = text.len() as f32 * 7.5;
    (g.raster_pos2f)(x - approx_w / 2.0, y);
    let bytes: Vec<u8> = text.bytes().collect();
    (g.list_base)(base - 32);
    (g.call_lists)(bytes.len() as i32, 0x1401, bytes.as_ptr() as *const c_void);
}

unsafe fn draw_line(x: f32, y: f32, color: (f32, f32, f32, f32), text: &str) {
    let base = match FONT_BASE.get() {
        Some(b) => *b,
        None => return,
    };

    let g = gl();

    (g.color4f)(color.0, color.1, color.2, color.3);
    (g.raster_pos2f)(x, y);

    let bytes: Vec<u8> = text.bytes().collect();
    (g.list_base)(base - 32);
    (g.call_lists)(bytes.len() as i32, 0x1401, bytes.as_ptr() as *const c_void);
}

pub unsafe fn draw_raw_line(x1: f32, y1: f32, x2: f32, y2: f32,
                             r: f32, g: f32, b: f32, a: f32) {
    let gl = match GL_FNS.get() { Some(g) => g, None => return };
    (gl.color4f)(r, g, b, a);
    (gl.begin)(0x0001);
    (gl.vertex2f)(x1, y1);
    (gl.vertex2f)(x2, y2);
    (gl.end)();
}

pub unsafe fn draw_raw_rect(x: f32, y: f32, w: f32, h: f32,
                             r: f32, g: f32, b: f32, a: f32) {
    let gl = match GL_FNS.get() { Some(g) => g, None => return };
    (gl.color4f)(r, g, b, a);
    (gl.begin)(0x0007);
    (gl.vertex2f)(x, y);
    (gl.vertex2f)(x + w, y);
    (gl.vertex2f)(x + w, y + h);
    (gl.vertex2f)(x, y + h);
    (gl.end)();
}

pub unsafe fn draw_raw_text(x: f32, y: f32, text: &str,
                             r: f32, g: f32, b: f32, a: f32) {
    let base = match FONT_BASE.get() { Some(b) => *b, None => return };
    let gl = match GL_FNS.get() { Some(g) => g, None => return };
    (gl.color4f)(r, g, b, a);
    (gl.raster_pos2f)(x, y);
    let bytes: Vec<u8> = text.bytes().collect();
    (gl.list_base)(base - 32);
    (gl.call_lists)(bytes.len() as i32, 0x1401, bytes.as_ptr() as *const c_void);
}

pub unsafe fn draw_raw_triangle(x1: f32, y1: f32, x2: f32, y2: f32,
                                 x3: f32, y3: f32,
                                 r: f32, g: f32, b: f32, a: f32) {
    let gl = match GL_FNS.get() { Some(g) => g, None => return };
    (gl.color4f)(r, g, b, a);
    (gl.begin)(0x0006);
    (gl.vertex2f)(x1, y1);
    (gl.vertex2f)(x2, y2);
    (gl.vertex2f)(x3, y3);
    (gl.end)();
}