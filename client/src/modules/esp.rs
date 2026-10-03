use crate::api::*;
use crate::module::*;

pub struct Esp { enabled: bool }

impl Esp {
    pub fn new() -> Self { Self { enabled: false } }
}

impl Module for Esp {
    fn name(&self) -> &'static str { "ESP" }
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

            let (ex, ey, ez) = (buf[0], buf[1], buf[2]);
            let corners = [
                [ex - 0.4, ey,       ez - 0.4],
                [ex + 0.4, ey,       ez - 0.4],
                [ex + 0.4, ey,       ez + 0.4],
                [ex - 0.4, ey,       ez + 0.4],
                [ex - 0.4, ey + 1.8, ez - 0.4],
                [ex + 0.4, ey + 1.8, ez - 0.4],
                [ex + 0.4, ey + 1.8, ez + 0.4],
                [ex - 0.4, ey + 1.8, ez + 0.4],
            ];

            let mut scr = [[0f32; 2]; 8];
            let mut ok_cnt = [0i32; 8];
            for j in 0..8 {
                let mut sx = 0f32;
                let mut sy = 0f32;
                ok_cnt[j] = unsafe {
                    project(corners[j][0], corners[j][1], corners[j][2], &mut sx, &mut sy)
                };
                scr[j] = [sx, sy];
            }

            let edges: [(usize, usize); 12] = [
                (0,1),(1,2),(2,3),(3,0),
                (4,5),(5,6),(6,7),(7,4),
                (0,4),(1,5),(2,6),(3,7),
            ];

            for (a, b) in edges.iter() {
                if ok_cnt[*a] != 0 && ok_cnt[*b] != 0 {
                    unsafe {
                        draw(scr[*a][0], scr[*a][1], scr[*b][0], scr[*b][1], color.as_ptr());
                    }
                }
            }
        }
    }
}