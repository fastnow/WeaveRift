use crate::api::*;
use crate::module::*;

pub struct Hud { enabled: bool }

impl Hud {
    pub fn new() -> Self { Self { enabled: true } }
}

impl Module for Hud {
    fn name(&self) -> &'static str { "HUD" }
    fn category(&self) -> &'static str { "Render" }
    fn default_enabled(&self) -> bool { true }
    fn enabled(&self) -> bool { self.enabled }
    fn set_enabled(&mut self, on: bool) { self.enabled = on; }

    fn on_render(&mut self, ctx: &mut RenderCtx) {
        let api = ctx.api;
        let text = match api.draw_text { Some(f) => f, None => return };

        let p = &ctx.player;
        let dim = [0.75, 0.78, 0.82, 1.0];

        let s1 = format!("WeaveRift  {}x{}", ctx.screen_w, ctx.screen_h);
        let s2 = format!("xyz   {:.1} {:.1} {:.1}", p[0], p[1], p[2]);
        let s3 = format!("rot   {:.1} / {:.1}", p[3], p[4]);
        let s4 = format!("hp    {:.0}/{:.0}", p[5], 20.0);

        unsafe {
            text(10.0, 40.0, s1.as_ptr(), s1.len(), dim.as_ptr());
            text(10.0, 60.0, s2.as_ptr(), s2.len(), dim.as_ptr());
            text(10.0, 78.0, s3.as_ptr(), s3.len(), dim.as_ptr());
            text(10.0, 96.0, s4.as_ptr(), s4.len(), dim.as_ptr());
        }
    }
}