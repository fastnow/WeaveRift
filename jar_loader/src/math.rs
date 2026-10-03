pub type Mat4 = [f32; 16];
pub type Vec3 = [f32; 3];

pub fn vec3_sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0]-b[0], a[1]-b[1], a[2]-b[2]]
}

pub fn vec3_add(a: Vec3, b: Vec3) -> Vec3 {
    [a[0]+b[0], a[1]+b[1], a[2]+b[2]]
}

pub fn vec3_dot(a: Vec3, b: Vec3) -> f32 {
    a[0]*b[0] + a[1]*b[1] + a[2]*b[2]
}

pub fn vec3_cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1]*b[2] - a[2]*b[1],
        a[2]*b[0] - a[0]*b[2],
        a[0]*b[1] - a[1]*b[0],
    ]
}

pub fn vec3_normalize(a: Vec3) -> Vec3 {
    let len = (a[0]*a[0] + a[1]*a[1] + a[2]*a[2]).sqrt();
    if len < 1e-6 { [0.0, 0.0, 0.0] } else { [a[0]/len, a[1]/len, a[2]/len] }
}

pub fn mat4_mul_vec4(m: &Mat4, v: [f32; 4]) -> [f32; 4] {
    [
        m[0]*v[0] + m[4]*v[1] + m[8]*v[2]  + m[12]*v[3],
        m[1]*v[0] + m[5]*v[1] + m[9]*v[2]  + m[13]*v[3],
        m[2]*v[0] + m[6]*v[1] + m[10]*v[2] + m[14]*v[3],
        m[3]*v[0] + m[7]*v[1] + m[11]*v[2] + m[15]*v[3],
    ]
}

pub fn look_at(eye: Vec3, center: Vec3, up: Vec3) -> Mat4 {
    let f = vec3_normalize(vec3_sub(center, eye));
    let s = vec3_normalize(vec3_cross(f, up));
    let u = vec3_cross(s, f);

    [
        s[0],  u[0], -f[0], 0.0,
        s[1],  u[1], -f[1], 0.0,
        s[2],  u[2], -f[2], 0.0,
        -vec3_dot(s, eye),
        -vec3_dot(u, eye),
         vec3_dot(f, eye),
        1.0,
    ]
}

pub fn perspective(fov_y_deg: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
    let f = 1.0 / (fov_y_deg.to_radians() / 2.0).tan();
    let mut m = [0.0; 16];
    m[0] = f / aspect;
    m[5] = f;
    m[10] = (far + near) / (near - far);
    m[11] = -1.0;
    m[14] = (2.0 * far * near) / (near - far);
    m
}

pub struct Projector {
    pub view: Mat4,
    pub proj: Mat4,
    pub vp: [i32; 4],
}

impl Projector {
    pub fn new(eye: Vec3, yaw: f32, pitch: f32, fov: f32,
               vp_x: i32, vp_y: i32, vp_w: i32, vp_h: i32) -> Self {
        let yaw_r = yaw.to_radians();
        let pitch_r = pitch.to_radians();

        let dir = [
            -yaw_r.sin() * pitch_r.cos(),
            -pitch_r.sin(),
             yaw_r.cos() * pitch_r.cos(),
        ];
        let center = vec3_add(eye, dir);

        let view = look_at(eye, center, [0.0, 1.0, 0.0]);
        let aspect = vp_w as f32 / vp_h.max(1) as f32;
        let proj = perspective(fov, aspect, 0.05, 512.0);

        Self {
            view,
            proj,
            vp: [vp_x, vp_y, vp_w, vp_h],
        }
    }

    pub fn project(&self, world: Vec3) -> Option<[f32; 2]> {
        let view_pos = mat4_mul_vec4(&self.view, [world[0], world[1], world[2], 1.0]);

        if view_pos[2] > -0.05 {
            return None;
        }

        let clip = mat4_mul_vec4(&self.proj, view_pos);

        if clip[3] <= 0.0 {
            return None;
        }

        let ndc_x = clip[0] / clip[3];
        let ndc_y = clip[1] / clip[3];

        if ndc_x < -1.5 || ndc_x > 1.5 || ndc_y < -1.5 || ndc_y > 1.5 {
            return None;
        }

        let sx = (ndc_x * 0.5 + 0.5) * self.vp[2] as f32 + self.vp[0] as f32;
        let sy = (1.0 - (ndc_y * 0.5 + 0.5)) * self.vp[3] as f32 + self.vp[1] as f32;

        Some([sx, sy])
    }
}

pub struct ProjectorCache {
    last_eye: Vec3,
    last_yaw: f32,
    last_pitch: f32,
    last_vp: [i32; 4],
    proj: Option<Projector>,
}

impl ProjectorCache {
    pub fn new() -> Self {
        Self {
            last_eye: [0.0; 3],
            last_yaw: 999.0,
            last_pitch: 999.0,
            last_vp: [0; 4],
            proj: None,
        }
    }

    pub fn get(&mut self, eye: Vec3, yaw: f32, pitch: f32,
               fov: f32, vp: [i32; 4]) -> &Projector {
        let changed = self.proj.is_none()
            || (eye[0] - self.last_eye[0]).abs() > 0.01
            || (eye[1] - self.last_eye[1]).abs() > 0.01
            || (eye[2] - self.last_eye[2]).abs() > 0.01
            || (yaw - self.last_yaw).abs() > 0.1
            || (pitch - self.last_pitch).abs() > 0.1
            || self.last_vp != vp;

        if changed {
            self.proj = Some(Projector::new(
                eye, yaw, pitch, fov, vp[0], vp[1], vp[2], vp[3]));
            self.last_eye = eye;
            self.last_yaw = yaw;
            self.last_pitch = pitch;
            self.last_vp = vp;
        }

        self.proj.as_ref().unwrap()
    }
}