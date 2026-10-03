use crate::api::*;
use crate::module::*;

pub struct Tracers {
    enabled: bool,
}

impl Tracers {
    pub fn new() -> Self { Self { enabled: false } }
}

impl Module for Tracers {
    fn name(&self) -> &'static str { "Tracers" }
    fn category(&self) -> &'static str { "Render" }
    fn default_enabled(&self) -> bool { true }
    fn enabled(&self) -> bool { self.enabled }
    fn set_enabled(&mut self, on: bool) { self.enabled = on; }

    fn on_render(&mut self, ctx: &mut RenderCtx) {
        let api = ctx.api;
        let project = match api.project { Some(f) => f, None => return };
        let draw = match api.draw_line { Some(f) => f, None => return };
        let count = match api.get_entity_count { Some(f) => f, None => return };
        let get_ent = match api.get_entity { Some(f) => f, None => return };

        let n = unsafe { count() };
        if n <= 0 { return; }

        let cx = ctx.screen_w as f32 / 2.0;
        let cy = ctx.screen_h as f32;

        for i in 0..n {
            let mut buf = [0f32; 6];
            let mut kind = 0i32;
            let ok = unsafe { get_ent(i, buf.as_mut_ptr(), &mut kind) };
            if ok == 0 { continue; }

            let color = match kind {
                1 => [1.0, 0.2, 0.2, 0.9],
                2 => [1.0, 0.6, 0.0, 0.9],
                3 => [0.4, 1.0, 0.4, 0.9],
                _ => continue,
            };

            let mut sx = 0f32;
            let mut sy = 0f32;
            let proj_ok = unsafe {
                project(buf[0], buf[1] + 0.9, buf[2], &mut sx, &mut sy)
            };
            if proj_ok == 0 { continue; }

            unsafe {
                draw(cx, cy, sx, sy, color.as_ptr());
            }
        }
    }
}