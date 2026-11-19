//! Camera, walk/fly navigation, saved views and the video keyframe path.
//! Navigation helpers: walk/fly camera, saved views and video paths.
//! addresses are listed in MECHANICS.md.

use crate::scene::Scene;
use crate::settings::{GeneralSettings, Projection};
use crate::sun::SunState;
use glam::{Mat4, Vec3};
use serde::{Deserialize, Serialize};

/// Base move speed: a held movement key queues a move of 1.0 x the speed modifiers,
/// read as m/s by the command processor (0x18048bfa0 returns 1.0 x multipliers;
/// case 0 of the queue processor 0x1804fc0d0 sums them into the target velocity).
pub const BASE_SPEED: f32 = 1.0;
/// Speed modifiers (0x18048bfa0): Shift x3 walking / x6 flying, Ctrl x10 walking / x20 flying.
pub const SHIFT_WALK: f32 = 3.0;
pub const SHIFT_FLY: f32 = 6.0;
pub const CTRL_WALK: f32 = 10.0;
pub const CTRL_FLY: f32 = 20.0;
/// Mouse look: each pixel x mouse speed is accumulated and applied x 0.001 rad
/// (case 8 at 0x1804fc3a4, scale constant 0x18112c234).
pub const LOOK_RAD_PER_PX: f32 = 0.001;
pub const LOOK_DEG_PER_PX: f32 = LOOK_RAD_PER_PX * 57.295_78;
/// Shift + right drag changes the hour by 0.002 h per pixel (g_mouseSunSpeed, used at 0x18048c750).
pub const TOD_HOURS_PER_PX: f32 = 0.002;
/// Mouse wheel: 120 units per notch; without fly smoothing the camera moves 0.005 m per unit
/// (0x18009ca90, constant 0x18112c8b0); with fly smoothing above 0.02 it pushes a forward
/// move of 0.167 x units (constant 0x18112c30c).
pub const WHEEL_UNITS_PER_NOTCH: f32 = 120.0;
pub const WHEEL_STEP_M: f32 = 0.005;
pub const WHEEL_PUSH: f32 = 0.167;
/// egui reports about 40 points per wheel notch (estimated conversion to Windows units).
pub const EGUI_POINTS_PER_NOTCH: f32 = 40.0;
/// Orthographic zoom per wheel unit: 1 - zoomSpeedFactor(1.0) x 0.0001 x units (0x18009ca90).
pub const ORTHO_ZOOM_PER_UNIT: f32 = 0.0001;
pub const GRAVITY: f32 = 9.81;
/// Step height the walker climbs: sp_stepHeight default 0.3 m (0x18060335b).
pub const STEP_HEIGHT: f32 = 0.3;
/// Eye sits this far below the top of the body: sp_eyeOffset default 0.1 m (0x180603578),
/// applied in 0x18058ae20.
pub const EYE_OFFSET: f32 = 0.1;
/// Body mass sp_mass 80 kg (0x1806034df).
pub const BODY_MASS: f32 = 80.0;
/// Hover controller holding the body centre half its height above the ground
/// (SpectatorPawn constructor 0x18058a4f0, step 0x18058a960): Kp, Ki, Kd.
pub const HOVER_PID: [f32; 3] = [25000.0, 750.0, 3000.0];
/// Horizontal velocity controllers (0x18058a4f0): Kp, Ki, Kd.
pub const MOVE_PID: [f32; 3] = [750.0, 2.0, 10.0];
/// Ground ray reaches 0.5 h up and 0.75 h down from the body centre (0x18058a960).
pub const GROUND_RAY_DOWN: f32 = 0.75;
/// Linear damping: 0.9 on the ground, 0.01 in the air (0x18058a960).
pub const DAMPING_GROUND: f32 = 0.9;
pub const DAMPING_AIR: f32 = 0.01;
/// Physics step (estimated: the usual fixed 60 Hz step of the bundled physics engine).
pub const PHYS_DT: f32 = 1.0 / 60.0;
/// Renderer defaults: sp_mouseSmoothing 0.2 (range 0..0.5), sp_flySmoothing 0.2 (range 0..0.9999).
pub const MOUSE_SMOOTHING_DEFAULT: f32 = 0.2;
pub const FLY_SMOOTHING_DEFAULT: f32 = 0.2;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CameraPose {
    pub pos: Vec3,
    pub yaw: f32,   // degrees, 0 = looking north (-Z), clockwise positive
    pub pitch: f32, // degrees
}

impl CameraPose {
    pub fn forward(&self) -> Vec3 {
        let (y, p) = (self.yaw.to_radians(), self.pitch.to_radians());
        Vec3::new(y.sin() * p.cos(), p.sin(), -y.cos() * p.cos())
    }
    pub fn flat_forward(&self) -> Vec3 {
        let y = self.yaw.to_radians();
        Vec3::new(y.sin(), 0.0, -y.cos())
    }
    pub fn right(&self) -> Vec3 {
        let y = self.yaw.to_radians();
        Vec3::new(y.cos(), 0.0, y.sin())
    }
    pub fn lerp(&self, o: &CameraPose, t: f32) -> CameraPose {
        let mut dy = (o.yaw - self.yaw) % 360.0;
        if dy > 180.0 {
            dy -= 360.0;
        }
        if dy < -180.0 {
            dy += 360.0;
        }
        CameraPose { pos: self.pos.lerp(o.pos, t), yaw: self.yaw + dy * t, pitch: self.pitch + (o.pitch - self.pitch) * t }
    }
}

#[derive(Default, Clone)]
pub struct NavInput {
    pub forward: f32,
    pub right: f32,
    pub up: f32,
    pub fast: bool,
    pub faster: bool,
    pub look: (f32, f32),
    pub orbit: (f32, f32),
    pub pan: (f32, f32),
    pub wheel: f32,
    pub orbit_pivot: Option<Vec3>,
}

/// One PID controller as in 0x18057a660: e = target - current;
/// I = max(0, 1 - dt) * I + dt * e; out = Kp e + Ki I + Kd (e - e_prev) / dt.
#[derive(Clone, Copy, Default)]
pub struct Pid {
    pub prev: f32,
    pub integral: f32,
    pub active: bool,
}

impl Pid {
    pub fn step(&mut self, gains: [f32; 3], target: f32, current: f32, dt: f32) -> f32 {
        let e = target - current;
        self.integral = (1.0 - dt).max(0.0) * self.integral + dt * e;
        let d = if self.active { (e - self.prev) / dt.max(1e-5) } else { 0.0 };
        self.prev = e;
        self.active = true;
        gains[0] * e + gains[1] * self.integral + gains[2] * d
    }
}

pub struct Camera {
    pub pose: CameraPose,
    pub walk_mode: bool,
    pub vertical_speed: f32,
    pub velocity: Vec3,
    pub smooth_look: (f32, f32),
    pub ortho_size: f32,
    pub transition: Option<(CameraPose, CameraPose, f32, f32)>,
    pub body_vel: Vec3,
    pub hover: Pid,
    pub pid_x: Pid,
    pub pid_z: Pid,
    pub phys_accum: f32,
}

/// Map the app's mouse smoothing setting (default 1.5, as the original's settings page)
/// onto the renderer's sp_mouseSmoothing (default 0.2, max 0.5). Linear mapping estimated.
pub fn mouse_smoothing_factor(gs: &GeneralSettings) -> f32 {
    (MOUSE_SMOOTHING_DEFAULT * gs.mouse_smoothing / 1.5).clamp(0.0, 0.5)
}

/// Map the app's movement smoothing (default 0) onto sp_flySmoothing (default 0.2, max 0.9999).
/// Mapping estimated.
pub fn fly_smoothing_factor(gs: &GeneralSettings) -> f32 {
    (FLY_SMOOTHING_DEFAULT + gs.move_smoothing * 0.16).clamp(0.0, 0.9999)
}

impl Camera {
    pub fn new() -> Self {
        Self {
            pose: CameraPose { pos: Vec3::new(9.0, 2.0, 24.0), yaw: -20.0, pitch: -4.0 },
            walk_mode: false,
            vertical_speed: 0.0,
            velocity: Vec3::ZERO,
            smooth_look: (0.0, 0.0),
            ortho_size: 30.0,
            transition: None,
            body_vel: Vec3::ZERO,
            hover: Pid::default(),
            pid_x: Pid::default(),
            pid_z: Pid::default(),
            phys_accum: 0.0,
        }
    }

    pub fn start_transition(&mut self, to: CameraPose, seconds: f32) {
        self.transition = Some((self.pose, to, 0.0, seconds.max(0.01)));
    }

    /// Eye height above the ground in walk mode: body height (the "spectator height" setting,
    /// passed to sp_height) minus sp_eyeOffset.
    pub fn eye_height(gs: &GeneralSettings) -> f32 {
        (gs.spectator_height - EYE_OFFSET).max(0.2)
    }

    /// 0x18048bfa0: 1.0, Shift x3 walking / x6 flying, Ctrl x10 walking / x20 flying, x multiplier.
    pub fn speed_multiplier(&self, input: &NavInput, gs: &GeneralSettings) -> f32 {
        let mut m = 1.0;
        if input.fast {
            m = if self.walk_mode { SHIFT_WALK } else { SHIFT_FLY };
        }
        if input.faster {
            m *= if self.walk_mode { CTRL_WALK } else { CTRL_FLY };
        }
        m * gs.move_speed
    }

    pub fn update(&mut self, dt: f32, input: &NavInput, gs: &GeneralSettings, scene: &Scene) {
        if let Some((from, to, elapsed, dur)) = self.transition.as_mut() {
            *elapsed += dt;
            let k = (*elapsed / *dur).clamp(0.0, 1.0);
            let e = k * k * (3.0 - 2.0 * k);
            self.pose = from.lerp(to, e);
            if k >= 1.0 {
                self.transition = None;
            }
            return;
        }
        let dt = dt.max(1e-4);

        // ---- look: pixels x mouse speed, smoothed with k = s^(3 dt), applied x 0.001 rad
        let sx = if gs.invert_x { -1.0 } else { 1.0 };
        let sy = if gs.invert_y { -1.0 } else { 1.0 };
        let target = (input.look.0 * sx * gs.mouse_speed, input.look.1 * sy * gs.mouse_speed);
        let k = mouse_smoothing_factor(gs).powf(3.0 * dt);
        self.smooth_look.0 = self.smooth_look.0 * k + target.0 * (1.0 - k);
        self.smooth_look.1 = self.smooth_look.1 * k + target.1 * (1.0 - k);
        if target == (0.0, 0.0) && self.smooth_look.0.abs() < 1e-3 && self.smooth_look.1.abs() < 1e-3 {
            self.smooth_look = (0.0, 0.0);
        }
        self.pose.yaw = (self.pose.yaw + self.smooth_look.0 * LOOK_DEG_PER_PX) % 360.0;
        self.pose.pitch = (self.pose.pitch - self.smooth_look.1 * LOOK_DEG_PER_PX).clamp(-89.0, 89.0);

        // ---- orbit around a pivot (right drag)
        if input.orbit != (0.0, 0.0) {
            let pivot = input.orbit_pivot.unwrap_or(self.pose.pos + self.pose.forward() * 10.0);
            let off = self.pose.pos - pivot;
            let dyaw = input.orbit.0 * gs.mouse_speed * LOOK_RAD_PER_PX;
            let rot = Mat4::from_rotation_y(-dyaw);
            let mut off = rot.transform_vector3(off);
            let right = self.pose.right();
            let dp = (input.orbit.1 * gs.mouse_speed * LOOK_RAD_PER_PX).to_degrees();
            let new_pitch = (self.pose.pitch - dp).clamp(-89.0, 89.0);
            let applied = (self.pose.pitch - new_pitch).to_radians();
            off = Mat4::from_axis_angle(right, -applied).transform_vector3(off);
            self.pose.pos = pivot + off;
            self.pose.yaw = (self.pose.yaw + dyaw.to_degrees()) % 360.0;
            self.pose.pitch = new_pitch;
            self.walk_mode = false;
        }

        // ---- pan (middle drag) uses the same speed modifiers (0x18048c750)
        let speed = BASE_SPEED * self.speed_multiplier(input, gs);
        if input.pan != (0.0, 0.0) {
            let up = self.pose.right().cross(self.pose.forward()).normalize();
            let scale = 0.01 * speed;
            let delta = -self.pose.right() * input.pan.0 * scale + up * input.pan.1 * scale;
            self.try_move(delta, gs, scene);
        }

        // ---- wheel: always switches to fly mode (0x18009ca90 sets sp_fly)
        let wheel_units = input.wheel / EGUI_POINTS_PER_NOTCH * WHEEL_UNITS_PER_NOTCH;
        let fly_s = fly_smoothing_factor(gs);
        let mut push = Vec3::ZERO;
        if wheel_units != 0.0 {
            self.ortho_size = (self.ortho_size * (1.0 - ORTHO_ZOOM_PER_UNIT * wheel_units)).clamp(1.0, 500.0);
            self.walk_mode = false;
            if fly_s > 0.02 {
                push = self.pose.forward() * wheel_units * WHEEL_PUSH;
            } else {
                self.pose.pos += self.pose.forward() * wheel_units * WHEEL_STEP_M;
            }
        }

        if self.walk_mode {
            self.update_walk(dt, input, gs, scene, speed);
        } else {
            // fly: target velocity eased with k = flySmoothing^(5 dt) (0x1804fc0d0)
            let mut wish = self.pose.forward() * input.forward + self.pose.right() * input.right + Vec3::Y * input.up;
            if wish.length_squared() > 1.0 {
                wish = wish.normalize();
            }
            let target_v = wish * speed + push;
            let k = fly_s.powf(5.0 * dt);
            self.velocity = self.velocity * k + target_v * (1.0 - k);
            if self.velocity.length_squared() < 1e-8 {
                self.velocity = Vec3::ZERO;
            }
            self.pose.pos += self.velocity * dt;
            self.body_vel = Vec3::ZERO;
        }
    }

    /// Walk mode: a body of `spectator_height` held half its height above the ground by a
    /// PID spring, driven horizontally by two velocity PIDs (SpectatorPawn, 0x18058a960).
    fn update_walk(&mut self, dt: f32, input: &NavInput, gs: &GeneralSettings, scene: &Scene, speed: f32) {
        let h = gs.spectator_height.max(0.3);
        let mut wish = self.pose.flat_forward() * input.forward + self.pose.right() * input.right;
        if wish.length_squared() > 1.0 {
            wish = wish.normalize();
        }
        let target_v = wish * speed;
        self.velocity = Vec3::ZERO;
        self.phys_accum = (self.phys_accum + dt).min(0.25);
        while self.phys_accum >= PHYS_DT {
            self.phys_accum -= PHYS_DT;
            // body centre sits h/2 above the feet; the eye is h - eyeOffset above the feet
            let center = self.pose.pos - Vec3::Y * (h * 0.5 - EYE_OFFSET);
            let ground = scene
                .raycast(center + Vec3::Y * (h * 0.5), -Vec3::Y, h * 0.5 + h * GROUND_RAY_DOWN, true)
                .map(|hit| center.y - hit.point.y);
            let mut force = Vec3::new(0.0, -GRAVITY * BODY_MASS, 0.0);
            let damping;
            match ground {
                Some(dist) if dist < h * 0.5 || self.hover.active => {
                    force.y += self.hover.step(HOVER_PID, h * 0.5, dist, PHYS_DT);
                    force.x += self.pid_x.step(MOVE_PID, target_v.x, self.body_vel.x, PHYS_DT);
                    force.z += self.pid_z.step(MOVE_PID, target_v.z, self.body_vel.z, PHYS_DT);
                    damping = DAMPING_GROUND;
                }
                _ => {
                    self.hover = Pid::default();
                    damping = DAMPING_AIR;
                }
            }
            self.body_vel += force / BODY_MASS * PHYS_DT;
            // linear damping as v *= (1 - d)^dt
            let f = (1.0 - damping).max(0.0).powf(PHYS_DT);
            self.body_vel.y *= f;
            let horiz = Vec3::new(self.body_vel.x, 0.0, self.body_vel.z) * PHYS_DT;
            self.try_move(horiz, gs, scene);
            self.pose.pos.y += self.body_vel.y * PHYS_DT;
            if self.pose.pos.y < -200.0 {
                self.pose.pos.y = Self::eye_height(gs);
                self.body_vel = Vec3::ZERO;
            }
        }
        self.vertical_speed = self.body_vel.y;
    }

    fn try_move(&mut self, delta: Vec3, gs: &GeneralSettings, scene: &Scene) {
        if !self.walk_mode {
            self.pose.pos += delta;
            return;
        }
        // body radius = sp_diameter / 2 (the "spectator width" setting)
        let radius = gs.spectator_width * 0.5;
        let mut d = Vec3::new(delta.x, 0.0, delta.z);
        for axis in [Vec3::X, Vec3::Z] {
            let comp = axis * d.dot(axis);
            let len = comp.length();
            if len < 1e-6 {
                continue;
            }
            let dir = comp / len;
            let feet = self.pose.pos - Vec3::Y * Self::eye_height(gs);
            let mut blocked = false;
            for hh in [STEP_HEIGHT + 0.05, 1.0, Self::eye_height(gs)] {
                if let Some(hit) = scene.raycast(feet + Vec3::Y * hh, dir, len + radius, false) {
                    if hit.t < len + radius {
                        blocked = true;
                        break;
                    }
                }
            }
            if blocked {
                d -= comp;
                if axis == Vec3::X {
                    self.body_vel.x = 0.0;
                } else {
                    self.body_vel.z = 0.0;
                }
            }
        }
        self.pose.pos += d;
    }

    /// Move to a picked surface point (double-click teleport).
    pub fn teleport(&mut self, point: Vec3, normal: Vec3, gs: &GeneralSettings) {
        let to = if self.walk_mode {
            CameraPose { pos: point + Vec3::Y * Self::eye_height(gs), ..self.pose }
        } else {
            let back = (self.pose.pos - point).normalize_or_zero();
            CameraPose { pos: point + back * 1.5 + normal * 0.3, ..self.pose }
        };
        self.body_vel = Vec3::ZERO;
        self.hover = Pid::default();
        self.start_transition(to, 1.0);
    }

    pub fn view_matrix(&self, projection: Projection) -> Mat4 {
        let mut pose = self.pose;
        if projection == Projection::TwoPoint {
            pose.pitch = 0.0;
        }
        Mat4::look_to_rh(pose.pos, pose.forward(), Vec3::Y)
    }

    /// Projection matrix; `hfov` is the horizontal field of view in degrees.
    pub fn proj_matrix(&self, projection: Projection, hfov: f32, aspect: f32) -> Mat4 {
        let vfov = vertical_fov(hfov, aspect);
        match projection {
            Projection::Orthographic => {
                let h = self.ortho_size * 0.5;
                Mat4::orthographic_rh_gl(-h * aspect, h * aspect, -h, h, -2000.0, 4000.0)
            }
            Projection::Perspective => Mat4::perspective_rh_gl(vfov, aspect, 0.05, 4000.0),
            Projection::TwoPoint => {
                let mut p = Mat4::perspective_rh_gl(vfov, aspect, 0.05, 4000.0);
                let f = 1.0 / (vfov * 0.5).tan();
                p.z_axis.y += f * self.pose.pitch.to_radians().tan();
                p
            }
        }
    }
}

pub fn vertical_fov(hfov_deg: f32, aspect: f32) -> f32 {
    2.0 * ((hfov_deg.to_radians() * 0.5).tan() / aspect.max(0.01)).atan()
}

// ------------------------------------------------------------------ saved views

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SavedView {
    pub name: String,
    pub pose: CameraPose,
    pub sun: SunState,
    pub fov: f32,
    pub projection: Projection,
    pub favorite: bool,
    pub preset: Option<String>,
}

// ------------------------------------------------------------------ video path

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Keyframe {
    pub pose: CameraPose,
    pub time: f32,
    pub time_of_day: Option<f32>,
    pub fov: Option<f32>,
    pub focal: Option<f32>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VideoPath {
    pub keyframes: Vec<Keyframe>,
    pub easing: bool,
    pub shaky: bool,
}

impl Default for VideoPath {
    fn default() -> Self {
        Self { keyframes: Vec::new(), easing: true, shaky: false }
    }
}

pub struct PathSample {
    pub pose: CameraPose,
    pub time_of_day: Option<f32>,
    pub fov: Option<f32>,
    pub focal: Option<f32>,
}

impl VideoPath {
    pub fn duration(&self) -> f32 {
        self.keyframes.last().map(|k| k.time).unwrap_or(0.0)
    }

    pub fn set_duration(&mut self, d: f32) {
        let cur = self.duration();
        if cur > 0.0 && d > 0.0 {
            let s = d / cur;
            for k in &mut self.keyframes {
                k.time *= s;
            }
        }
    }

    pub fn add(&mut self, pose: CameraPose, insert_after: Option<usize>) {
        let new = |time| Keyframe { pose, time, time_of_day: None, fov: None, focal: None };
        match insert_after {
            Some(i) if i + 1 < self.keyframes.len() => {
                let t = (self.keyframes[i].time + self.keyframes[i + 1].time) * 0.5;
                self.keyframes.insert(i + 1, new(t));
            }
            _ => {
                let t = match self.keyframes.last() {
                    None => 0.0,
                    Some(last) => last.time + (last.pose.pos.distance(pose.pos) / 2.0).max(2.0),
                };
                self.keyframes.push(new(t));
            }
        }
    }

    pub fn remove(&mut self, i: usize) {
        if i < self.keyframes.len() {
            self.keyframes.remove(i);
            if let Some(first) = self.keyframes.first().map(|k| k.time) {
                for k in &mut self.keyframes {
                    k.time -= first;
                }
            }
        }
    }

    /// Camera at time `t` seconds: Catmull-Rom through the keyframe positions,
    /// optional ease in/out over the whole path and optional hand-held shake.
    pub fn sample(&self, t: f32) -> Option<PathSample> {
        let n = self.keyframes.len();
        if n == 0 {
            return None;
        }
        if n == 1 {
            let k = &self.keyframes[0];
            return Some(PathSample { pose: k.pose, time_of_day: k.time_of_day, fov: k.fov, focal: k.focal });
        }
        let dur = self.duration().max(1e-3);
        let mut u = (t / dur).clamp(0.0, 1.0);
        if self.easing {
            u = u * u * (3.0 - 2.0 * u);
        }
        let tt = u * dur;
        let mut i = 0;
        while i + 2 < n && self.keyframes[i + 1].time < tt {
            i += 1;
        }
        let k1 = &self.keyframes[i];
        let k2 = &self.keyframes[i + 1];
        let seg = (k2.time - k1.time).max(1e-4);
        let s = ((tt - k1.time) / seg).clamp(0.0, 1.0);
        let k0 = &self.keyframes[i.saturating_sub(1)];
        let k3 = &self.keyframes[(i + 2).min(n - 1)];
        let cr = |p0: Vec3, p1: Vec3, p2: Vec3, p3: Vec3| {
            let s2 = s * s;
            let s3 = s2 * s;
            0.5 * ((2.0 * p1) + (-p0 + p2) * s + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * s2 + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * s3)
        };
        let pos = cr(k0.pose.pos, k1.pose.pos, k2.pose.pos, k3.pose.pos);
        let mut pose = k1.pose.lerp(&k2.pose, s * s * (3.0 - 2.0 * s));
        pose.pos = pos;
        if self.shaky {
            let w = t * 1.3;
            pose.pos += Vec3::new((w * 2.1).sin() * 0.02 + (w * 5.3).sin() * 0.008, (w * 1.7).sin() * 0.015, (w * 2.9).cos() * 0.01);
            pose.yaw += (w * 1.9).sin() * 0.35 + (w * 4.7).sin() * 0.1;
            pose.pitch += (w * 2.3).cos() * 0.25;
        }
        let lerp_opt = |a: Option<f32>, b: Option<f32>| match (a, b) {
            (Some(a), Some(b)) => Some(a + (b - a) * s),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            _ => None,
        };
        Some(PathSample {
            pose,
            time_of_day: lerp_opt(k1.time_of_day, k2.time_of_day),
            fov: lerp_opt(k1.fov, k2.fov),
            focal: lerp_opt(k1.focal, k2.focal),
        })
    }
}
