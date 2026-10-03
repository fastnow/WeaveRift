use crate::api::RiftAPI;
use crate::module::ModuleManager;

const PI: f32 = 3.14159265;

#[derive(Clone, Copy)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    fn contains(&self, mx: f32, my: f32) -> bool {
        mx >= self.x && mx <= self.x + self.w
            && my >= self.y && my <= self.y + self.h
    }
}

pub struct GuiState {
    pub open: bool,
    pub focused: bool,
    pub mouse_x: f32,
    pub mouse_y: f32,
    pub mouse_down: bool,
    pub prev_mouse_down: bool,
    pub anim: f32,
    pub category: usize,
    pub drag_x: f32,
    pub drag_y: f32,
    pub dragging: bool,
    pub drag_off_x: f32,
    pub drag_off_y: f32,
    pub hover_anim: [f32; 32],
}

impl GuiState {
    pub const fn new() -> Self {
        Self {
            open: false,
            focused: true,
            mouse_x: 0.0,
            mouse_y: 0.0,
            mouse_down: false,
            prev_mouse_down: false,
            anim: 0.0,
            category: 0,
            drag_x: 100.0,
            drag_y: 100.0,
            dragging: false,
            drag_off_x: 0.0,
            drag_off_y: 0.0,
            hover_anim: [0.0; 32],
        }
    }

    pub fn update(&mut self, dt: f32) {
        let target = if self.open { 1.0 } else { 0.0 };
        self.anim += (target - self.anim) * (dt * 12.0).min(1.0);

        let clicked = self.mouse_down && !self.prev_mouse_down;
        self.prev_mouse_down = self.mouse_down;

        if !self.open {
            return;
        }

        if clicked && self.focused {
            let panel = Rect {
                x: self.drag_x,
                y: self.drag_y,
                w: 460.0,
                h: 340.0,
            };
            let header = Rect {
                x: panel.x,
                y: panel.y,
                w: panel.w,
                h: 42.0,
            };
            if header.contains(self.mouse_x, self.mouse_y) {
                self.dragging = true;
                self.drag_off_x = self.mouse_x - panel.x;
                self.drag_off_y = self.mouse_y - panel.y;
            }
        }

        if !self.mouse_down {
            self.dragging = false;
        }

        if self.dragging {
            self.drag_x = self.mouse_x - self.drag_off_x;
            self.drag_y = self.mouse_y - self.drag_off_y;
        }
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 { a + (b - a) * t }

fn lerp4(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    [lerp(a[0], b[0], t), lerp(a[1], b[1], t), lerp(a[2], b[2], t), lerp(a[3], b[3], t)]
}

fn ease_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

unsafe fn draw_rounded_rect(api: &RiftAPI, rect: Rect, radius: f32, rgba: [f32; 4]) {
    let rect_fn = match api.draw_rect { Some(f) => f, None => return };
    let tri_fn = match api.draw_triangle { Some(f) => f, None => return };

    rect_fn(rect.x + radius, rect.y, rect.w - radius * 2.0, rect.h, rgba.as_ptr());
    rect_fn(rect.x, rect.y + radius, rect.w, rect.h - radius * 2.0, rgba.as_ptr());

    let corners = [
        (rect.x + radius, rect.y + radius, PI, PI * 1.5),
        (rect.x + rect.w - radius, rect.y + radius, PI * 1.5, PI * 2.0),
        (rect.x + rect.w - radius, rect.y + rect.h - radius, 0.0, PI * 0.5),
        (rect.x + radius, rect.y + rect.h - radius, PI * 0.5, PI),
    ];

    let steps = 10;
    for (cx, cy, a0, a1) in corners.iter() {
        for i in 0..steps {
            let t0 = *a0 + (*a1 - *a0) * (i as f32 / steps as f32);
            let t1 = *a0 + (*a1 - *a0) * ((i + 1) as f32 / steps as f32);
            let x0 = cx + t0.cos() * radius;
            let y0 = cy + t0.sin() * radius;
            let x1 = cx + t1.cos() * radius;
            let y1 = cy + t1.sin() * radius;
            tri_fn(*cx, *cy, x0, y0, x1, y1, rgba.as_ptr());
        }
    }
}

unsafe fn draw_text_rgba(api: &RiftAPI, x: f32, y: f32, text: &str, rgba: [f32; 4]) {
    let f = match api.draw_text { Some(f) => f, None => return };
    f(x, y, text.as_ptr(), text.len(), rgba.as_ptr());
}

unsafe fn draw_shadow(api: &RiftAPI, rect: Rect) {
    let rect_fn = match api.draw_rect { Some(f) => f, None => return };
    for i in 0..6 {
        let off = (i + 1) as f32 * 3.0;
        let alpha = 0.04 * (6 - i) as f32;
        let c = [0.0, 0.0, 0.0, alpha];
        rect_fn(rect.x - off, rect.y - off + 2.0,
                rect.w + off * 2.0, rect.h + off * 2.0, c.as_ptr());
    }
}

unsafe fn draw_toggle(api: &RiftAPI, rect: Rect, on: bool, anim: f32) {
    let rect_fn = match api.draw_rect { Some(f) => f, None => return };

    let track_color = if on {
        lerp4([0.25, 0.45, 0.95, 1.0], [0.35, 0.55, 1.0, 1.0], anim)
    } else {
        [0.3, 0.3, 0.35, 0.8]
    };

    rect_fn(rect.x, rect.y + 4.0, rect.w, rect.h - 8.0, track_color.as_ptr());

    let knob_w = (rect.h - 10.0).max(12.0);
    let knob_off = if on { rect.w - knob_w - 3.0 } else { 3.0 };
    let knob_x = rect.x + knob_off;
    let knob_y = rect.y + 5.0;
    let knob_c = if on { [1.0, 1.0, 1.0, 1.0] } else { [0.85, 0.85, 0.85, 1.0] };
    rect_fn(knob_x, knob_y, knob_w, knob_w, knob_c.as_ptr());
}

pub unsafe fn draw_and_update(
    api: &RiftAPI,
    _screen_w: i32,
    _screen_h: i32,
    mgr: &mut ModuleManager,
    state: &mut GuiState,
    dt: f32,
) {
    state.update(dt);

    let anim = ease_out_cubic(state.anim);
    if anim < 0.01 {
        return;
    }

    let categories = ["Render", "Combat", "Movement", "Misc"];
    let panel_w = 460.0 * anim;
    let panel_h = 340.0 * anim;
    let panel_x = state.drag_x;
    let panel_y = state.drag_y;

    let panel = Rect { x: panel_x, y: panel_y, w: panel_w, h: panel_h };

    draw_shadow(api, panel);

    let bg = [0.11, 0.11, 0.13, 0.92 * anim];
    draw_rounded_rect(api, panel, 10.0, bg);

    let header = Rect { x: panel_x, y: panel_y, w: panel_w, h: 42.0 * anim };
    let header_bg = [0.15, 0.15, 0.18, 0.95 * anim];
    draw_rounded_rect(api, header, 10.0, header_bg);

    let rect_fn = match api.draw_rect { Some(f) => f, None => return };
    let accent = [0.35, 0.55, 1.0, 0.9 * anim];
    rect_fn(panel_x, panel_y + 42.0 * anim - 1.0, panel_w, 1.0, accent.as_ptr());

    draw_text_rgba(api, panel_x + 16.0, panel_y + 14.0, "WeaveRift", [1.0, 1.0, 1.0, anim]);
    draw_text_rgba(api, panel_x + 120.0, panel_y + 14.0, "· Modules", [0.6, 0.6, 0.65, anim]);

    let clicked = state.focused && state.mouse_down && !state.prev_mouse_down;

    let cat_x = panel_x + 12.0;
    let mut cat_y = panel_y + 56.0;
    for (i, name) in categories.iter().enumerate() {
        let active = i == state.category;
        let hover = state.mouse_x >= cat_x && state.mouse_x <= cat_x + 100.0
            && state.mouse_y >= cat_y && state.mouse_y <= cat_y + 30.0
            && state.focused;

        let target = if active { 1.0 } else if hover { 0.5 } else { 0.0 };
        state.hover_anim[i] += (target - state.hover_anim[i]) * (dt * 10.0).min(1.0);
        let ha = state.hover_anim[i];

        let item = Rect { x: cat_x, y: cat_y, w: 100.0, h: 30.0 };
        if active || ha > 0.01 {
            let c = if active {
                [0.25, 0.4, 0.85, 0.9 * anim]
            } else {
                [0.25, 0.25, 0.3, 0.5 * ha * anim]
            };
            draw_rounded_rect(api, item, 6.0, c);
        }

        let txt_c = if active {
            [1.0, 1.0, 1.0, anim]
        } else {
            [0.7, 0.7, 0.75, (0.5 + 0.5 * ha) * anim]
        };
        draw_text_rgba(api, cat_x + 14.0, cat_y + 8.0, name, txt_c);

        if clicked && item.contains(state.mouse_x, state.mouse_y) {
            state.category = i;
        }

        cat_y += 36.0;
    }

    let divider = Rect {
        x: panel_x + 124.0,
        y: panel_y + 52.0,
        w: 1.0,
        h: panel_h - 64.0,
    };
    rect_fn(divider.x, divider.y, divider.w, divider.h,
            [0.25, 0.25, 0.3, 0.6 * anim].as_ptr());

    let list_x = panel_x + 140.0;
    let mut list_y = panel_y + 56.0;

    let mut idx = 0;
    let mods_len = mgr.modules_mut().len();
    let mut clicked_idx: Option<usize> = None;

    for i in 0..mods_len {
        let (name, category, enabled) = {
            let m = &mgr.modules_mut()[i];
            (m.name().to_string(), m.category().to_string(), m.enabled())
        };

        if category != categories[state.category] {
            continue;
        }

        let row = Rect { x: list_x, y: list_y, w: panel_w - 156.0, h: 34.0 };

        let hover = state.focused && row.contains(state.mouse_x, state.mouse_y);
        let target = if hover { 1.0 } else { 0.0 };
        state.hover_anim[8 + idx] += (target - state.hover_anim[8 + idx]) * (dt * 12.0).min(1.0);
        let ha = state.hover_anim[8 + idx];

        if ha > 0.01 {
            draw_rounded_rect(api, row, 6.0, [0.2, 0.2, 0.25, 0.6 * ha * anim]);
        }

        let dot_c = if enabled {
            [0.35, 0.9, 0.5, anim]
        } else {
            [0.4, 0.4, 0.45, anim]
        };
        rect_fn(row.x + 10.0, row.y + 15.0, 6.0, 6.0, dot_c.as_ptr());

        let name_c = if enabled {
            [1.0, 1.0, 1.0, anim]
        } else {
            [0.7, 0.7, 0.75, anim]
        };
        draw_text_rgba(api, row.x + 24.0, row.y + 10.0, &name, name_c);

        let toggle = Rect {
            x: row.x + row.w - 50.0,
            y: row.y + 4.0,
            w: 40.0,
            h: 22.0,
        };
        draw_toggle(api, toggle, enabled, ha);

        if clicked && row.contains(state.mouse_x, state.mouse_y) {
            clicked_idx = Some(i);
        }

        list_y += 38.0;
        idx += 1;
    }

    if let Some(i) = clicked_idx {
        let cur = mgr.modules_mut()[i].enabled();
        mgr.modules_mut()[i].set_enabled(!cur);
    }

    if idx == 0 {
        draw_text_rgba(api, list_x + 10.0, list_y, "（此分类下无模块）", [0.5, 0.5, 0.55, anim]);
    }

    let _ = cat_x;
    let _ = cat_y;
}