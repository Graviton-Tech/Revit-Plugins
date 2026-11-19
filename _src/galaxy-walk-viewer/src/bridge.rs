//! Link to the Galaxy Walk Revit plugin.
//!
//! The plugin and this viewer share one folder (`--bridge <dir>`):
//!
//! * `scene.gws`        model geometry, materials and lights written by Revit (format below)
//! * `live.json`        Revit camera and sun, rewritten whenever the Revit view changes
//! * `walk-camera.json` written by this viewer so Revit can match the walkthrough camera
//!
//! `.gws` layout (little endian):
//!   "GWS1" | u32 header_len | header JSON (UTF-8) | binary blob
//! The blob holds, per object in header order, `vertices * 8` f32 (pos xyz, normal xyz, uv)
//! followed by `indices` u32. Positions are metres, +Y up, -Z north, +X east.

use crate::app::App;
use crate::camera::CameraPose;
use crate::scene::{GroupKind, Light, Material, Mesh, Scene, Vertex};
use glam::Mat4;
use std::sync::Arc;
use eframe::egui;
use glam::Vec3;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Deserialize)]
struct GwsMaterial {
    name: String,
    #[serde(default = "grey")]
    color: [f32; 3],
    #[serde(default = "half")]
    roughness: f32,
    #[serde(default)]
    metallic: f32,
    #[serde(default = "one")]
    opacity: f32,
    #[serde(default)]
    emissive: [f32; 3],
    #[serde(default)]
    luminance: f32,
    /// Diffuse bitmap from the Revit appearance asset, with its real-world size in metres.
    #[serde(default)]
    texture: String,
    #[serde(default)]
    tex_size: Option<[f32; 2]>,
    #[serde(default)]
    tex_rot: f32,
    #[serde(default = "one")]
    tex_amount: f32,
}

#[derive(Deserialize)]
struct GwsObject {
    name: String,
    material: usize,
    vertices: usize,
    indices: usize,
    /// Version 2: number of 4x4 placement matrices stored after the mesh (0 = draw once as is).
    #[serde(default)]
    instances: usize,
}

#[derive(Deserialize)]
struct GwsLight {
    #[serde(default)]
    pos: [f32; 3],
    #[serde(default = "warm")]
    color: [f32; 3],
    #[serde(default)]
    lumen: f32,
}

#[derive(Deserialize)]
struct GwsHeader {
    #[serde(default)]
    project: String,
    #[serde(default)]
    materials: Vec<GwsMaterial>,
    #[serde(default)]
    objects: Vec<GwsObject>,
    #[serde(default)]
    lights: Vec<GwsLight>,
}

fn grey() -> [f32; 3] {
    [0.75, 0.75, 0.75]
}
fn warm() -> [f32; 3] {
    [1.0, 0.9, 0.75]
}
fn half() -> f32 {
    0.5
}
fn one() -> f32 {
    1.0
}

/// Replace the scene with the contents of a `.gws` file. Returns the project name.
pub fn load_gws(scene: &mut Scene, path: &Path) -> Result<String, String> {
    let data = std::fs::read(path).map_err(|e| format!("Could not read {}: {e}", path.display()))?;
    if data.len() < 8 || &data[0..4] != b"GWS1" {
        return Err("Not a Galaxy Walk scene file.".into());
    }
    let hlen = u32::from_le_bytes([data[4], data[5], data[6], data[7]]) as usize;
    if 8 + hlen > data.len() {
        return Err("The scene file is cut short.".into());
    }
    let header: GwsHeader = serde_json::from_slice(&data[8..8 + hlen]).map_err(|e| format!("Scene header: {e}"))?;
    let blob = &data[8 + hlen..];

    let mut fresh = Scene::default();
    for m in &header.materials {
        let mut mat = Material::new(&m.name, m.color, m.roughness);
        mat.metallic = m.metallic;
        mat.opacity = m.opacity.clamp(0.0, 1.0);
        if mat.opacity < 0.999 {
            mat.double_sided = true;
        }
        if !m.texture.is_empty() && std::path::Path::new(&m.texture).exists() {
            mat.texture = m.texture.clone();
            let [w, h] = m.tex_size.unwrap_or([1.0, 1.0]);
            mat.tex_width = w.max(0.01);
            mat.tex_height = h.max(0.01);
            mat.tex_rotation = m.tex_rot;
            mat.image_fade = m.tex_amount.clamp(0.0, 1.0);
            mat.tint = [1.0, 1.0, 1.0];
        }
        if m.luminance > 0.0 {
            mat.emissive_color = m.emissive;
            mat.luminance = m.luminance;
        }
        fresh.materials.push(mat);
    }
    if fresh.materials.is_empty() {
        fresh.materials.push(Material::new("Default", grey(), 0.6));
    }
    let g = fresh.add_group(if header.project.is_empty() { "Revit model" } else { &header.project }, GroupKind::Imported);

    let mut at = 0usize;
    for o in &header.objects {
        let vbytes = o.vertices * 32;
        let ibytes = o.indices * 4;
        let mbytes = o.instances * 64;
        if at + vbytes + ibytes + mbytes > blob.len() {
            return Err("The scene file is cut short.".into());
        }
        let f = |at: usize, i: usize| f32::from_le_bytes([blob[at + i * 4], blob[at + i * 4 + 1], blob[at + i * 4 + 2], blob[at + i * 4 + 3]]);
        let mut mesh = Mesh::default();
        mesh.vertices.reserve(o.vertices);
        for v in 0..o.vertices {
            let b = v * 8;
            mesh.vertices.push(Vertex { pos: [f(at, b), f(at, b + 1), f(at, b + 2)], normal: [f(at, b + 3), f(at, b + 4), f(at, b + 5)], uv: [f(at, b + 6), f(at, b + 7)] });
        }
        let ioff = at + vbytes;
        mesh.indices.reserve(o.indices);
        for i in 0..o.indices {
            let p = ioff + i * 4;
            let idx = u32::from_le_bytes([blob[p], blob[p + 1], blob[p + 2], blob[p + 3]]);
            mesh.indices.push(if (idx as usize) < o.vertices { idx } else { 0 });
        }
        let moff = ioff + ibytes;
        at += vbytes + ibytes + mbytes;
        if mesh.indices.len() < 3 {
            continue;
        }
        let mi = o.material.min(fresh.materials.len() - 1);
        if o.instances == 0 {
            fresh.add_object(g, &o.name, mi, mesh);
        } else {
            let bounds = mesh.bounds();
            let shared = Arc::new(mesh);
            for k in 0..o.instances {
                let base = moff + k * 64;
                let mut cols = [0f32; 16];
                for (j, c) in cols.iter_mut().enumerate() {
                    *c = f(base, j);
                }
                fresh.add_instance(g, &o.name, mi, shared.clone(), bounds, Mat4::from_cols_array(&cols));
            }
        }
    }
    for l in &header.lights {
        fresh.lights.push(Light { group: g, local_pos: Vec3::from(l.pos), color: l.color, lumen: l.lumen });
    }
    fresh.geometry_rev = scene.geometry_rev + 1;
    *scene = fresh;
    Ok(header.project)
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct LiveCamera {
    eye: [f32; 3],
    yaw: f32,
    pitch: f32,
    /// Orthographic Revit view: the eye point is arbitrary, so frame the model instead.
    ortho: bool,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct LiveSun {
    year: i32,
    month: u32,
    day: u32,
    hour: f32,
    gmt: f32,
    lat: f32,
    lon: f32,
    /// Extra rotation (degrees) between Revit project north and true north.
    rot: f32,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Live {
    rev: u64,
    scene_rev: u64,
    scene: String,
    camera: Option<LiveCamera>,
    sun: Option<LiveSun>,
    progress: Option<LiveProgress>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct LiveProgress {
    done: u64,
    total: u64,
}

#[derive(Serialize)]
struct WalkCamera {
    rev: u64,
    eye: [f32; 3],
    yaw: f32,
    pitch: f32,
}

/// Place the camera so the whole model fits on screen, looking along (yaw, pitch).
/// Used for orthographic Revit views, whose eye point is not a usable viewpoint.
fn frame_model(app: &mut App, yaw: f32, pitch: f32) -> CameraPose {
    let (mn, mx) = app.scene.world_bounds();
    let centre = (mn + mx) * 0.5;
    let radius = ((mx - mn).length() * 0.5).max(1.0);
    let vfov = crate::camera::vertical_fov(app.vs.field_of_view, 16.0 / 9.0).clamp(0.2, 2.5);
    let dist = radius / (vfov * 0.5).sin() * 1.05;
    let mut pose = CameraPose { pos: centre, yaw, pitch };
    pose.pos = centre - pose.forward() * dist;
    app.camera.transition = None;
    app.camera.walk_mode = false;
    app.camera.pose = pose;
    pose
}

pub struct Bridge {
    pub dir: PathBuf,
    /// Follow the Revit camera and sun whenever they change.
    pub follow: bool,
    /// Write the walkthrough camera so Revit can match it.
    pub send_back: bool,
    pub status: String,
    live_rev: u64,
    scene_rev: u64,
    scene_mtime: Option<SystemTime>,
    last_poll: f64,
    last_sent: Option<CameraPose>,
    last_send_time: f64,
    send_rev: u64,
    /// Direction (yaw, pitch) of the last orthographic Revit view, kept so the model can be
    /// framed again once more of it has loaded.
    ortho_dir: Option<(f32, f32)>,
}

impl Bridge {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            follow: true,
            send_back: true,
            status: "Waiting for Revit...".into(),
            live_rev: 0,
            scene_rev: 0,
            scene_mtime: None,
            last_poll: -10.0,
            last_sent: None,
            last_send_time: 0.0,
            send_rev: 0,
            ortho_dir: None,
        }
    }

    fn read_live(&self) -> Option<Live> {
        let text = std::fs::read_to_string(self.dir.join("live.json")).ok()?;
        serde_json::from_str(&text).ok()
    }

    /// Run a few times a second from `App::update`.
    pub fn poll(&mut self, app: &mut App, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        if now - self.last_poll >= 0.15 {
            self.last_poll = now;
            self.pull(app, ctx, now);
        }
        self.push(app, now);
    }

    fn pull(&mut self, app: &mut App, ctx: &egui::Context, now: f64) {
        // geometry
        let scene_path = self.dir.join("scene.gws");
        let mt = std::fs::metadata(&scene_path).and_then(|m| m.modified()).ok();
        if mt.is_some() && mt != self.scene_mtime {
            // the writer renames a finished file into place, so a read is safe
            match load_gws(&mut app.scene, &scene_path) {
                Ok(name) => {
                    self.scene_mtime = mt;
                    app.selected_object = None;
                    app.selected_group = None;
                    app.selected_material = 0;
                    if !self.status.starts_with("Reading") {
                        self.status = format!("Linked: {} ({} objects)", name, app.scene.objects.len());
                    }
                    app.toast = Some((format!("Model updated from Revit ({} objects)", app.scene.objects.len()), now + 2.5));
                    if let (true, Some((yaw, pitch))) = (self.follow, self.ortho_dir) {
                        self.last_sent = Some(frame_model(app, yaw, pitch));
                    }
                }
                Err(e) => self.status = e,
            }
        }
        // camera and sun
        let Some(live) = self.read_live() else { return };
        if live.rev == self.live_rev {
            return;
        }
        self.live_rev = live.rev;
        self.scene_rev = live.scene_rev;
        match &live.progress {
            Some(p) if p.total > 0 && p.done < p.total => {
                self.status = format!("Reading model from Revit: {}%", p.done * 100 / p.total);
            }
            _ => {
                if self.status.starts_with("Reading") || self.status.starts_with("Waiting") {
                    self.status = format!("Linked ({} objects)", app.scene.objects.len());
                }
            }
        }
        if !self.follow {
            return;
        }
        if let Some(c) = live.camera {
            let pitch = c.pitch.clamp(-89.0, 89.0);
            if c.ortho {
                self.ortho_dir = Some((c.yaw, pitch));
                self.last_sent = Some(frame_model(app, c.yaw, pitch));
            } else {
                self.ortho_dir = None;
                let pose = CameraPose { pos: Vec3::from(c.eye), yaw: c.yaw, pitch };
                app.camera.transition = None;
                app.camera.pose = pose;
                self.last_sent = Some(pose);
            }
        }
        if let Some(s) = live.sun {
            if s.month >= 1 && s.day >= 1 {
                app.sun.year = s.year;
                app.sun.month = s.month;
                app.sun.day = s.day;
                app.sun.hour = s.hour;
                app.sun.gmt_offset = s.gmt;
                app.sun.latitude = s.lat;
                app.sun.longitude = s.lon;
                app.sun.rotation = s.rot;
                app.sun.angle_offset = 0.0;
            }
        }
        ctx.request_repaint();
    }

    fn push(&mut self, app: &App, now: f64) {
        if !self.send_back || now - self.last_send_time < 0.3 {
            return;
        }
        let pose = app.camera.pose;
        if let Some(last) = self.last_sent {
            if last.pos.distance(pose.pos) < 0.01 && (last.yaw - pose.yaw).abs() < 0.05 && (last.pitch - pose.pitch).abs() < 0.05 {
                return;
            }
        }
        self.last_send_time = now;
        self.last_sent = Some(pose);
        self.send_rev += 1;
        let out = WalkCamera { rev: self.send_rev, eye: pose.pos.into(), yaw: pose.yaw, pitch: pose.pitch };
        if let Ok(text) = serde_json::to_string(&out) {
            let tmp = self.dir.join("walk-camera.json.tmp");
            if std::fs::write(&tmp, text).is_ok() {
                let _ = std::fs::rename(&tmp, self.dir.join("walk-camera.json"));
            }
        }
    }
}
