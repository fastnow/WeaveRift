mod api;
mod module;
mod modules;
mod clickgui;

use api::RiftAPI;
use module::{ModuleManager, RenderCtx, TickCtx};
use clickgui::GuiState;

static mut MANAGER: Option<ModuleManager> = None;
static mut GUI: Option<GuiState> = None;
static mut FRAME: u64 = 0;

unsafe fn manager() -> &'static mut ModuleManager {
    MANAGER.get_or_insert_with(|| {
        let mut m = ModuleManager::new();
        m.register(Box::new(modules::hud::Hud::new()));
        m.register(Box::new(modules::tracers::Tracers::new()));
        m.register(Box::new(modules::esp::Esp::new()));
        m
    })
}

unsafe fn gui() -> &'static mut GuiState {
    GUI.get_or_insert_with(GuiState::new)
}

#[no_mangle]
pub unsafe extern "C" fn client_main(api: *const RiftAPI) -> i32 {
    api::API = Some(&*api);
    let _ = manager();
    let _ = gui();
    api::log("client 已加载");
    0
}

#[no_mangle]
pub unsafe extern "C" fn client_on_render(
    px: f32, py: f32, pz: f32,
    yaw: f32, pitch: f32, hp: f32,
    screen_w: i32, screen_h: i32,
    dt_ms: f32,
    mouse_x: f32, mouse_y: f32,
    mouse_down: bool,
    focused: i32,
) {
    let a = match api::API { Some(a) => a, None => return };
    FRAME += 1;

    let dt = (dt_ms / 1000.0).clamp(0.0, 0.1);

    let g = gui();
    g.focused = focused != 0;
    g.mouse_x = mouse_x;
    g.mouse_y = mouse_y;
    g.mouse_down = mouse_down && g.focused;

    let mut ctx = RenderCtx {
        api: a,
        player: [px, py, pz, yaw, pitch, hp],
        screen_w,
        screen_h,
        dt,
    };

    manager().on_render(&mut ctx);

    if g.open {
        clickgui::draw_and_update(a, screen_w, screen_h, manager(), g, dt);
    }
}

#[no_mangle]
pub unsafe extern "C" fn client_on_tick() {
    let mut ctx = TickCtx { frame: FRAME };
    manager().on_tick(&mut ctx);
}

#[no_mangle]
pub unsafe extern "C" fn client_on_key(vk: u32, down: bool, focused: i32) {
    if focused == 0 { return; }
    if vk == 0xA1 {
        let g = gui();
        if down {
            g.open = !g.open;
            let a = api::API;
            if let Some(a) = a {
                if let Some(f) = a.set_mouse_grabbed {
                    f(if g.open { 0 } else { 1 });
                }
            }
            api::log(&format!("gui: {}", if g.open { "on" } else { "off" }));
        }
    }
}