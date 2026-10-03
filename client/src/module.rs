use crate::api::*;

pub struct RenderCtx<'a> {
    pub api: &'a RiftAPI,
    pub player: [f32; 6],
    pub screen_w: i32,
    pub screen_h: i32,
    pub dt: f32,
}

pub trait Module: Send {
    fn name(&self) -> &'static str;
    fn category(&self) -> &'static str { "Misc" }
    fn default_enabled(&self) -> bool { false }
    fn enabled(&self) -> bool;
    fn set_enabled(&mut self, on: bool);
    fn on_tick(&mut self, _ctx: &mut TickCtx) {}
    fn on_render(&mut self, _ctx: &mut RenderCtx) {}
    fn draw_config(&mut self, _ui: &mut ConfigUi) {}
}

pub struct TickCtx {
    pub frame: u64,
}

pub struct ConfigUi<'a> {
    pub api: &'a RiftAPI,
    pub x: f32,
    pub y: f32,
    pub width: f32,
}

impl<'a> ConfigUi<'a> {
    pub unsafe fn text(&self, x: f32, y: f32, s: &str, color: [f32; 4]) {
        if let Some(f) = self.api.draw_text {
            f(x, y, s.as_ptr(), s.len(), color.as_ptr());
        }
    }
}

pub struct ModuleManager {
    modules: Vec<Box<dyn Module>>,
}

impl ModuleManager {
    pub fn new() -> Self {
        Self { modules: Vec::new() }
    }

    pub fn register(&mut self, mut m: Box<dyn Module>) {
        let def = m.default_enabled();
        m.set_enabled(def);
        self.modules.push(m);
    }

    pub fn on_tick(&mut self, ctx: &mut TickCtx) {
        for m in self.modules.iter_mut() {
            if m.enabled() {
                m.on_tick(ctx);
            }
        }
    }

    pub fn on_render(&mut self, ctx: &mut RenderCtx) {
        for m in self.modules.iter_mut() {
            if m.enabled() {
                m.on_render(ctx);
            }
        }
    }

    pub fn modules(&self) -> &[Box<dyn Module>] {
        &self.modules
    }

    pub fn modules_mut(&mut self) -> &mut [Box<dyn Module>] {
        &mut self.modules
    }
}