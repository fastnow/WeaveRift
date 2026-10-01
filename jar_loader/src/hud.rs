//! HUD 绘制。

use std::ffi::c_void;
use std::sync::OnceLock;

use windows::core::PCSTR;
use windows::Win32::Graphics::Gdi::{
    HDC, CreateFontW, SelectObject, HGDIOBJ, FW_NORMAL,
    ANTIALIASED_QUALITY, DEFAULT_CHARSET, OUT_DEFAULT_PRECIS,
    CLIP_DEFAULT_PRECIS, DEFAULT_PITCH,
};
use windows::Win32::Graphics::OpenGL::wglUseFontBitmapsA;

use crate::jni_bridge::Snapshot;
use crate::logger;

// GL 常量
const GL_PROJECTION: u32 = 0x1701;
const GL_MODELVIEW: u32 = 0x1700;
const GL_DEPTH_TEST: u32 = 0x0B71;
const GL_TEXTURE_2D: u32 = 0x0DE1;
const GL_BLEND: u32 = 0x0BE2;
const GL_LIGHTING: u32 = 0x0B50;
const GL_CULL_FACE: u32 = 0x0B44;
const GL_SRC_ALPHA: u32 = 0x0302;
const GL_ONE_MINUS_SRC_ALPHA: u32 = 0x0303;
const GL_QUADS: u32 = 0x0007;
const GL_LINES: u32 = 0x0001;
const GL_UNSIGNED_BYTE: u32 = 0x1401;
const GL_LINE_STRIP: u32 = 0x0003;

#[allow(non_camel_case_types)]
type fn_glViewport = unsafe extern "system" fn(i32, i32, i32, i32);
#[allow(non_camel_case_types)]
type fn_glMatrixMode = unsafe extern "system" fn(u32);
#[allow(non_camel_case_types)]
type fn_glLoadIdentity = unsafe extern "system" fn();
#[allow(non_camel_case_types)]
type fn_glOrtho = unsafe extern "system" fn(f64, f64, f64, f64, f64, f64);
#[allow(non_camel_case_types)]
type fn_glPushMatrix = unsafe extern "system" fn();
#[allow(non_camel_case_types)]
type fn_glPopMatrix = unsafe extern "system" fn();
#[allow(non_camel_case_types)]
type fn_glEnable = unsafe extern "system" fn(u32);
#[allow(non_camel_case_types)]
type fn_glDisable = unsafe extern "system" fn(u32);
#[allow(non_camel_case_types)]
type fn_glBlendFunc = unsafe extern "system" fn(u32, u32);
#[allow(non_camel_case_types)]
type fn_glColor4f = unsafe extern "system" fn(f32, f32, f32, f32);
#[allow(non_camel_case_types)]
type fn_glLineWidth = unsafe extern "system" fn(f32);
#[allow(non_camel_case_types)]
type fn_glBegin = unsafe extern "system" fn(u32);
#[allow(non_camel_case_types)]
type fn_glEnd = unsafe extern "system" fn();
#[allow(non_camel_case_types)]
type fn_glVertex2f = unsafe extern "system" fn(f32, f32);
#[allow(non_camel_case_types)]
type fn_glFlush = unsafe extern "system" fn();
#[allow(non_camel_case_types)]
type fn_glGenLists = unsafe extern "system" fn(u32) -> u32;
#[allow(non_camel_case_types)]
type fn_glListBase = unsafe extern "system" fn(u32);
#[allow(non_camel_case_types)]
type fn_glCallLists = unsafe extern "system" fn(i32, u32, *const c_void);
#[allow(non_camel_case_types)]
type fn_glRasterPos2f = unsafe extern "system" fn(f32, f32);

macro_rules! decl {
    ($($n:ident : $t:ty),* $(,)?) => {
        pub struct Gl { $(pub $n: $t,)* }
    };
}

decl! {
    glViewport: fn_glViewport,
    glMatrixMode: fn_glMatrixMode,
    glLoadIdentity: fn_glLoadIdentity,
    glOrtho: fn_glOrtho,
    glPushMatrix: fn_glPushMatrix,
    glPopMatrix: fn_glPopMatrix,
    glEnable: fn_glEnable,
    glDisable: fn_glDisable,
    glBlendFunc: fn_glBlendFunc,
    glColor4f: fn_glColor4f,
    glLineWidth: fn_glLineWidth,
    glBegin: fn_glBegin,
    glEnd: fn_glEnd,
    glVertex2f: fn_glVertex2f,
    glFlush: fn_glFlush,
    glGenLists: fn_glGenLists,
    glListBase: fn_glListBase,
    glCallLists: fn_glCallLists,
    glRasterPos2f: fn_glRasterPos2f,
}

static GL: OnceLock<Gl> = OnceLock::new();
static FONT_BASE: OnceLock<u32> = OnceLock::new();

unsafe fn sym<T>(hmod: windows::Win32::Foundation::HMODULE, name: &str) -> Result<T, String> {
    let p = windows::Win32::System::LibraryLoader::GetProcAddress(
        hmod,
        PCSTR(format!("{}\0", name).as_ptr()),
    )
    .ok_or_else(|| format!("GetProcAddress({}) 失败", name))?;
    Ok(std::mem::transmute_copy::<*const (), T>(&(p as *const ())))
}

pub unsafe fn load_gl() -> Result<(), String> {
    let hmod = windows::Win32::System::LibraryLoader::GetModuleHandleA(
        PCSTR(b"opengl32.dll\0".as_ptr()),
    )
    .map_err(|_| "opengl32.dll 未加载".to_string())?;

    let g = Gl {
        glViewport: sym(hmod, "glViewport")?,
        glMatrixMode: sym(hmod, "glMatrixMode")?,
        glLoadIdentity: sym(hmod, "glLoadIdentity")?,
        glOrtho: sym(hmod, "glOrtho")?,
        glPushMatrix: sym(hmod, "glPushMatrix")?,
        glPopMatrix: sym(hmod, "glPopMatrix")?,
        glEnable: sym(hmod, "glEnable")?,
        glDisable: sym(hmod, "glDisable")?,
        glBlendFunc: sym(hmod, "glBlendFunc")?,
        glColor4f: sym(hmod, "glColor4f")?,
        glLineWidth: sym(hmod, "glLineWidth")?,
        glBegin: sym(hmod, "glBegin")?,
        glEnd: sym(hmod, "glEnd")?,
        glVertex2f: sym(hmod, "glVertex2f")?,
        glFlush: sym(hmod, "glFlush")?,
        glGenLists: sym(hmod, "glGenLists")?,
        glListBase: sym(hmod, "glListBase")?,
        glCallLists: sym(hmod, "glCallLists")?,
        glRasterPos2f: sym(hmod, "glRasterPos2f")?,
    };

    GL.set(g).map_err(|_| "GL 已加载".to_string())?;
    logger::debug("GL 1.1 函数指针加载完成");
    Ok(())
}

pub unsafe fn init_font(hdc: HDC) -> Result<(), String> {
    let font = CreateFontW(
        14,                          // cHeight
        0,                           // cWidth
        0,                           // cEscapement
        0,                           // cOrientation
        FW_NORMAL.0 as i32,          // cWeight
        0,                           // bItalic
        0,                           // bUnderline
        0,                           // bStrikeOut
        DEFAULT_CHARSET.0 as u32,    // iCharSet
        OUT_DEFAULT_PRECIS.0 as u32, // iOutPrecision
        CLIP_DEFAULT_PRECIS.0 as u32,// iClipPrecision
        ANTIALIASED_QUALITY.0 as u32,// iQuality
        DEFAULT_PITCH.0 as u32,      // iPitchAndFamily  ← ★ 缺的是这个
        windows::core::PCWSTR(b"Consolas\0".as_ptr() as *const u16), // pszFaceName
    );
    if font.is_invalid() {
        return Err("CreateFontW 失败".into());
    }

    let old = SelectObject(hdc, HGDIOBJ(font.0));
    let g = GL.get().ok_or("GL 未加载")?;
    let base = (g.glGenLists)(96);
    if base == 0 {
        SelectObject(hdc, old);
        return Err("glGenLists 返回 0".into());
    }
    let ok = wglUseFontBitmapsA(hdc, 32, 96, base).is_ok();
    SelectObject(hdc, old);

    if !ok {
        return Err("wglUseFontBitmapsA 失败".into());
    }
    let _ = FONT_BASE.set(base);
    logger::debug("HUD 字体初始化完成");
    Ok(())
}

unsafe fn text(g: &Gl, x: f32, y: f32, s: &str) {
    let base = match FONT_BASE.get() {
        Some(b) => *b,
        None => return,
    };
    let bytes: Vec<u8> = s.bytes().filter(|b| *b >= 32 && *b < 128).collect();
    if bytes.is_empty() {
        return;
    }
    (g.glRasterPos2f)(x, y);
    (g.glListBase)(base - 32);
    (g.glCallLists)(bytes.len() as i32, GL_UNSIGNED_BYTE, bytes.as_ptr() as *const c_void);
}

unsafe fn rect_border(g: &Gl, x: f32, y: f32, w: f32, h: f32) {
    (g.glBegin)(GL_LINE_STRIP);
    (g.glVertex2f)(x, y);
    (g.glVertex2f)(x + w, y);
    (g.glVertex2f)(x + w, y + h);
    (g.glVertex2f)(x, y + h);
    (g.glVertex2f)(x, y);
    (g.glEnd)();
}

unsafe fn rect_fill(g: &Gl, x: f32, y: f32, w: f32, h: f32) {
    (g.glBegin)(GL_QUADS);
    (g.glVertex2f)(x, y);
    (g.glVertex2f)(x + w, y);
    (g.glVertex2f)(x + w, y + h);
    (g.glVertex2f)(x, y + h);
    (g.glEnd)();
}

/// 主绘制。调用前必须已 gl_ctx::begin()，调用后必须 gl_ctx::end()。
pub unsafe fn draw(
    w: i32,
    h: i32,
    snap: Option<Snapshot>,
    fps: f32,
    frame: u64,
    state: &str,
    vm_ok: bool,
    ctx_ok: bool,
) {
    let g = match GL.get() {
        Some(g) => g,
        None => return,
    };

    let wf = w as f32;
    let hf = h as f32;

    // ── 正交投影：原点左上，1 单位 = 1 像素 ──
    (g.glViewport)(0, 0, w, h);
    (g.glMatrixMode)(GL_PROJECTION);
    (g.glPushMatrix)();
    (g.glLoadIdentity)();
    (g.glOrtho)(0.0, wf as f64, hf as f64, 0.0, -1.0, 1.0);
    (g.glMatrixMode)(GL_MODELVIEW);
    (g.glPushMatrix)();
    (g.glLoadIdentity)();

    (g.glDisable)(GL_DEPTH_TEST);
    (g.glDisable)(GL_TEXTURE_2D);
    (g.glDisable)(GL_LIGHTING);
    (g.glDisable)(GL_CULL_FACE);
    (g.glEnable)(GL_BLEND);
    (g.glBlendFunc)(GL_SRC_ALPHA, GL_ONE_MINUS_SRC_ALPHA);

    // ── 红色边框：证明链路通了 ──
    (g.glColor4f)(1.0, 0.15, 0.15, 0.85);
    (g.glLineWidth)(3.0);
    rect_border(g, 8.0, 8.0, wf - 16.0, hf - 16.0);

    // ── HUD 面板 ──
    let px = 20.0_f32;
    let py = 20.0_f32;
    let pw = 300.0_f32;
    let ph = 132.0_f32;

    (g.glColor4f)(0.0, 0.0, 0.0, 0.55);
    rect_fill(g, px, py, pw, ph);
    (g.glColor4f)(0.35, 0.75, 1.0, 0.9);
    (g.glLineWidth)(1.0);
    rect_border(g, px, py, pw, ph);

    let mut ly = py + 20.0;
    let step = 16.0_f32;

    macro_rules! line {
        ($color:expr, $fmt:literal $(, $arg:expr)*) => {
            let s = format!($fmt $(, $arg)*);
            (g.glColor4f)($color.0, $color.1, $color.2, $color.3);
            text(g, px + 12.0, ly, &s);
            ly += step;
        };
    }

    // 前 3 行是自检：任何一行红了，立刻知道是哪一层断的
    let ok_c = (0.55_f32, 0.95_f32, 0.55_f32, 1.0_f32);
    let bad_c = (1.0_f32, 0.4_f32, 0.4_f32, 1.0_f32);
    let dim_c = (0.75_f32, 0.78_f32, 0.82_f32, 1.0_f32);

    let hz_c = if fps > 5.0 { ok_c } else { bad_c };
    let vm_c = if vm_ok { ok_c } else { bad_c };
    let ctx_c = if ctx_ok { ok_c } else { bad_c };

    line!(dim_c, "WeaveRift  f{}", frame);
    line!(hz_c, "hook  {:.0} Hz", fps);
    line!(vm_c, "jni   {}", if vm_ok { "ok" } else { "NO VM" });
    line!(ctx_c, "ctx   {}", if ctx_ok { "ok" } else { "NO CTX" });
    line!(dim_c, "state {}", state);

    match snap {
        Some(s) if s.valid => {
            line!(dim_c, "xyz   {:.1} {:.1} {:.1}", s.x, s.y, s.z);
            line!(dim_c, "rot   {:.1} / {:.1}", s.yaw, s.pitch);
            line!(dim_c, "hp    {:.0}/{:.0}   ents {}", s.health, s.max_health, s.entities);
        }
        _ => {
            line!(bad_c, "no snapshot (agent 未就绪?)");
        }
    }

    // ── 还原 ──
    (g.glColor4f)(1.0, 1.0, 1.0, 1.0);
    (g.glFlush)();

    (g.glMatrixMode)(GL_MODELVIEW);
    (g.glPopMatrix)();
    (g.glMatrixMode)(GL_PROJECTION);
    (g.glPopMatrix)();
}
