//! 独立 GL context。

use std::sync::atomic::{AtomicIsize, AtomicPtr, Ordering};
use windows::Win32::Foundation::{HANDLE, RECT, HWND};
use windows::Win32::Graphics::Gdi::{WindowFromDC, HDC};
use windows::Win32::UI::WindowsAndMessaging::GetClientRect;
use windows::Win32::Graphics::OpenGL::{
    wglCreateContext, wglDeleteContext, wglGetCurrentContext, wglMakeCurrent, wglShareLists,
    HGLRC,
};

use crate::logger;

static OWN: AtomicIsize = AtomicIsize::new(0);
static GAME: AtomicIsize = AtomicIsize::new(0);
static HDC_CACHE: AtomicIsize = AtomicIsize::new(0);
static HWND_CACHE: AtomicPtr<HWND> = AtomicPtr::new(std::ptr::null_mut());

fn hglrc_from(v: isize) -> HGLRC {
    unsafe { std::mem::transmute::<isize, HGLRC>(v) }
}

fn hglrc_to(h: HGLRC) -> isize {
    unsafe { std::mem::transmute::<HGLRC, isize>(h) }
}

pub unsafe fn create_shared(hdc: HDC) -> Result<(), String> {
    let game = wglGetCurrentContext();
    if game.is_invalid() {
        return Err("当前线程没有 GL context".into());
    }

    // ★ wglCreateContext 返回 Result<HGLRC>
    let own = wglCreateContext(hdc)
        .map_err(|e| format!("wglCreateContext 失败: {:?}", e))?;

    if own.is_invalid() {
        return Err("wglCreateContext 返回无效句柄".into());
    }

    // ★ wglShareLists 返回 Result<()>
    let shared = wglShareLists(game, own).is_ok();
    if !shared {
        logger::warn("wglShareLists 失败（不影响画线，字体需自建）");
    } else {
        logger::debug("wglShareLists 成功");
    }

    OWN.store(hglrc_to(own), Ordering::SeqCst);
    GAME.store(hglrc_to(game), Ordering::SeqCst);
    HDC_CACHE.store(hdc.0 as isize, Ordering::SeqCst);

    let hwnd = WindowFromDC(hdc);
    HWND_CACHE.store(Box::into_raw(Box::new(hwnd)), Ordering::SeqCst);

    logger::info("独立 GL context 创建完成");
    Ok(())
}

pub fn valid() -> bool {
    OWN.load(Ordering::SeqCst) != 0
}

pub fn hdc_changed(hdc: HDC) -> bool {
    HDC_CACHE.load(Ordering::SeqCst) != hdc.0 as isize
}

pub unsafe fn begin() -> bool {
    let own = hglrc_from(OWN.load(Ordering::SeqCst));
    let hdc = HDC(HDC_CACHE.load(Ordering::SeqCst) as *mut _);
    if own.is_invalid() || hdc.is_invalid() {
        return false;
    }
    wglMakeCurrent(hdc, own).is_ok()
}

pub unsafe fn end() -> bool {
    let game = hglrc_from(GAME.load(Ordering::SeqCst));
    let hdc = HDC(HDC_CACHE.load(Ordering::SeqCst) as *mut _);
    if game.is_invalid() || hdc.is_invalid() {
        return false;
    }
    wglMakeCurrent(hdc, game).is_ok()
}

pub unsafe fn viewport_size(hdc: HDC) -> (i32, i32) {
    let hwnd = WindowFromDC(hdc);
    if hwnd.0.is_null() {
        return (854, 480);
    }
    let mut r = RECT::default();
    if GetClientRect(hwnd, &mut r).is_ok() {
        (r.right - r.left, r.bottom - r.top)
    } else {
        (854, 480)
    }
}

pub unsafe fn destroy() {
    let own = hglrc_from(OWN.load(Ordering::SeqCst));
    if !own.is_invalid() {
        let _ = wglMakeCurrent(HDC(std::ptr::null_mut()), hglrc_from(0));
        let _ = wglDeleteContext(own);
    }
    OWN.store(0, Ordering::SeqCst);
    GAME.store(0, Ordering::SeqCst);
    HDC_CACHE.store(0, Ordering::SeqCst);
}

pub fn hwnd() -> HWND {
    let p = HWND_CACHE.load(Ordering::SeqCst);
    if p.is_null() {
        HWND(std::ptr::null_mut())
    } else {
        unsafe { *p }
    }
}

#[allow(dead_code)]
fn _assert_handle(_: HANDLE) {}