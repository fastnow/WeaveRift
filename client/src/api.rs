pub type DrawLine = unsafe extern "C" fn(f32, f32, f32, f32, *const f32);
pub type DrawRect = unsafe extern "C" fn(f32, f32, f32, f32, *const f32);
pub type DrawText = unsafe extern "C" fn(f32, f32, *const u8, usize, *const f32);
pub type DrawTriangle = unsafe extern "C" fn(f32, f32, f32, f32, f32, f32, *const f32);
pub type ProjectFn = unsafe extern "C" fn(f32, f32, f32, *mut f32, *mut f32) -> i32;
pub type LogFn = unsafe extern "C" fn(*const u8, usize);
pub type GetPlayerFn = unsafe extern "C" fn(*mut f32);
pub type GetEntCountFn = unsafe extern "C" fn() -> i32;
pub type GetEntFn = unsafe extern "C" fn(i32, *mut f32, *mut i32) -> i32;
pub type ScreenSizeFn = unsafe extern "C" fn(*mut i32, *mut i32);
pub type SetMouseGrabbedFn = unsafe extern "C" fn(i32);
pub type IsGameFocusedFn = unsafe extern "C" fn() -> i32;

#[repr(C)]
pub struct RiftAPI {
    pub version: u32,
    pub log: Option<LogFn>,
    pub get_player: Option<GetPlayerFn>,
    pub get_entity_count: Option<GetEntCountFn>,
    pub get_entity: Option<GetEntFn>,
    pub screen_size: Option<ScreenSizeFn>,
    pub project: Option<ProjectFn>,
    pub draw_line: Option<DrawLine>,
    pub draw_rect: Option<DrawRect>,
    pub draw_text: Option<DrawText>,
    pub draw_triangle: Option<DrawTriangle>,
    pub set_mouse_grabbed: Option<SetMouseGrabbedFn>,
    pub is_game_focused: Option<IsGameFocusedFn>,
}

unsafe impl Sync for RiftAPI {}
unsafe impl Send for RiftAPI {}

pub static mut API: Option<&'static RiftAPI> = None;

pub unsafe fn api() -> &'static RiftAPI {
    API.expect("RiftAPI not set")
}

pub unsafe fn log(msg: &str) {
    let a = api();
    if let Some(f) = a.log {
        f(msg.as_ptr(), msg.len());
    }
}