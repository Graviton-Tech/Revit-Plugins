//! Application state, the 3D viewport, navigation input, exports and project files.

use crate::camera::{Camera, CameraPose, NavInput, SavedView, VideoPath};
use crate::export;
use crate::render::{DrawItem, DrawList, FrameParams, RenderJob, Renderer, Shared, SharedRef};
use crate::scene::{self, GroupKind, Material, Scene};
use crate::settings::{GeneralSettings, Preset, Projection, VisualSettings};
use crate::sun::SunState;
use eframe::egui::{self, Color32, Key, PointerButton, Pos2, Rect};
use glam::{Mat4, Vec3, Vec4};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub const ACCENT: Color32 = Color32::from_rgb(0x7C, 0x6C, 0xFF);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Walkthrough,
    Views,
    Video,
    Assets,
    Info,
    Issues,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Issue {
    pub title: String,
    pub description: String,
    pub status: String,
    pub priority: String,
    pub pose: CameraPose,
    pub point: Option<Vec3>,
}

pub enum ExportKind {
    Image { path: PathBuf, masks: bool },
    Batch { path: PathBuf },
    Panorama { path: PathBuf, width: u32, stereo: bool },
    VideoFrame { index: u32 },
}

pub struct VideoExport {
    pub dir: PathBuf,
    pub frame: u32,
    pub total: u32,
    pub size: (u32, u32),
    pub waiting: bool,
    pub ext: String,
}

pub struct BatchState {
    pub selected: Vec<bool>,
    pub panorama: bool,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Project {
    pub visual: VisualSettings,
    pub presets: Vec<Preset>,
    pub active_preset: usize,
    pub general: GeneralSettings,
    pub sun: SunState,
    pub views: Vec<SavedView>,
    pub video: VideoPath,
    pub issues: Vec<Issue>,
    pub materials: Vec<Material>,
    pub assets: Vec<(String, [f32; 3], f32, f32)>,
    pub imports: Vec<(String, [f32; 3], f32, f32)>,
    pub camera: Option<CameraPose>,
    pub walk_mode: bool,
}

pub struct App {
    pub scene: Scene,
    pub camera: Camera,
    pub sun: SunState,
    pub vs: VisualSettings,
    pub presets: Vec<Preset>,
    pub active_preset: usize,
    pub gs: GeneralSettings,
    pub views: Vec<SavedView>,
    pub video: VideoPath,
    pub issues: Vec<Issue>,
    pub shared: SharedRef,
    pub renderer: Arc<Mutex<Option<Renderer>>>,
    pub mode: Mode,
    pub show_visual: bool,
    pub show_general: bool,
    pub show_material: bool,
    pub show_batch: bool,
    pub show_sun: bool,
    pub show_about: bool,
    pub vis_tab: usize,
    pub gen_tab: usize,
    pub selected_material: usize,
    pub selected_object: Option<usize>,
    pub selected_group: Option<u32>,
    pub asset_to_place: Option<String>,
    pub asset_category: String,
    pub asset_rotation: f32,
    pub playing: Option<f32>,
    pub selected_keyframe: Option<usize>,
    pub exports: Vec<(u64, ExportKind)>,
    pub next_job: u64,
    pub toast: Option<(String, f64)>,
    pub live_updates: bool,
    pub watch: Vec<(u32, PathBuf, Option<std::time::SystemTime>, f32)>,
    pub last_watch_check: f64,
    pub screenshot: Option<PathBuf>,
    pub frames: u32,
    pub screenshot_requested: bool,
    pub time: f32,
    pub viewport_px: (u32, u32),
    pub viewport_rect: Rect,
    pub focal: f32,
    pub video_export: Option<VideoExport>,
    pub batch: BatchState,
    pub new_view_name: String,
    pub new_preset_name: String,
    pub rename_buf: String,
    pub project_path: Option<PathBuf>,
    pub import_scale: f32,
    pub issue_draft: Issue,
    pub sun_start: SunState,
    pub moved: bool,
    pub safe_frame: bool,
    pub prev_pointer: Option<Pos2>,
    pub orbit_pivot: Option<Vec3>,
    pub last_saved: Option<PathBuf>,
    /// Link to the Galaxy Walk Revit plugin (`--bridge <dir>`).
    pub bridge: Option<crate::bridge::Bridge>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, screenshot: Option<PathBuf>, open: Option<PathBuf>, bridge: Option<PathBuf>) -> Self {
        crate::ui::style(&cc.egui_ctx);
        let vs = VisualSettings::default();
        let mut app = Self {
            scene: scene::sample_scene(),
            camera: Camera::new(),
            sun: SunState::default(),
            presets: vec![Preset { name: "Default".into(), settings: vs.clone() }],
            active_preset: 0,
            vs,
            gs: GeneralSettings::default(),
            views: Vec::new(),
            video: VideoPath::default(),
            issues: Vec::new(),
            shared: Arc::new(Mutex::new(Shared::default())),
            renderer: Arc::new(Mutex::new(None)),
            mode: Mode::Walkthrough,
            show_visual: false,
            show_general: false,
            show_material: false,
            show_batch: false,
            show_sun: false,
            show_about: false,
            vis_tab: 0,
            gen_tab: 0,
            selected_material: 0,
            selected_object: None,
            selected_group: None,
            asset_to_place: None,
            asset_category: "Vegetation".into(),
            asset_rotation: 0.0,
            playing: None,
            selected_keyframe: None,
            exports: Vec::new(),
            next_job: 1,
            toast: None,
            live_updates: true,
            watch: Vec::new(),
            last_watch_check: 0.0,
            screenshot,
            frames: 0,
            screenshot_requested: false,
            time: 0.0,
            viewport_px: (1280, 720),
            viewport_rect: Rect::from_min_size(Pos2::ZERO, egui::vec2(1280.0, 720.0)),
            focal: 10.0,
            video_export: None,
            batch: BatchState { selected: Vec::new(), panorama: false },
            new_view_name: String::new(),
            new_preset_name: String::new(),
            rename_buf: String::new(),
            project_path: None,
            import_scale: 1.0,
            issue_draft: Issue { title: String::new(), description: String::new(), status: "Open".into(), priority: "Normal".into(), pose: CameraPose { pos: Vec3::ZERO, yaw: 0.0, pitch: 0.0 }, point: None },
            sun_start: SunState::default(),
            moved: false,
            safe_frame: false,
            prev_pointer: None,
            orbit_pivot: None,
            last_saved: None,
            bridge: bridge.clone().map(crate::bridge::Bridge::new),
        };
        app.add_default_views();
        if bridge.is_some() {
            // linked to Revit: start empty, the model arrives from the plugin
            app.scene = scene::Scene::default();
            app.views.clear();
        }
        if let Some(p) = open {
            app.load_project(&p);
        }
        app
    }

    fn add_default_views(&mut self) {
        let mk = |name: &str, pos: Vec3, yaw: f32, pitch: f32, hour: f32, fav: bool| SavedView {
            name: name.into(),
            pose: CameraPose { pos, yaw, pitch },
            sun: SunState { hour, ..SunState::default() },
            fov: 90.0,
            projection: Projection::Perspective,
            favorite: fav,
            preset: None,
        };
        self.views.push(mk("Garden view", Vec3::new(9.0, 2.0, 24.0), -20.0, -4.0, 15.0, true));
        self.views.push(mk("Pool terrace", Vec3::new(13.0, 1.98, 12.5), -60.0, -6.0, 18.5, true));
        self.views.push(mk("Living room", Vec3::new(1.5, 1.98, 1.2), -95.0, -6.0, 11.0, false));
        self.views.push(mk("Aerial", Vec3::new(30.0, 22.0, 30.0), -45.0, -30.0, 16.0, false));
    }

    /// File > New: the sample scene with default settings.
    pub fn load_defaults(&mut self) {
        self.scene = scene::sample_scene();
        self.vs = VisualSettings::default();
        self.presets = vec![Preset { name: "Default".into(), settings: self.vs.clone() }];
        self.active_preset = 0;
        self.gs = GeneralSettings::default();
        self.sun = SunState::default();
        self.views.clear();
        self.add_default_views();
        self.video = VideoPath::default();
        self.issues.clear();
        self.watch.clear();
        self.camera = Camera::new();
        self.selected_group = None;
        self.selected_object = None;
        self.project_path = None;
    }

    pub fn apply_preset(&mut self, i: usize) {
        if let Some(p) = self.presets.get(i) {
            self.active_preset = i;
            self.vs = p.settings.clone();
        }
    }

    pub fn preset_modified(&self) -> bool {
        self.presets.get(self.active_preset).map(|p| p.settings != self.vs).unwrap_or(false)
    }

    pub fn notify(&mut self, ctx: &egui::Context, text: impl Into<String>) {
        self.toast = Some((text.into(), ctx.input(|i| i.time) + 3.0));
    }

    // ------------------------------------------------------------------ frame data

    pub fn build_draw_list(&self) -> DrawList {
        let mut items = Vec::with_capacity(self.scene.objects.len());
        for (i, o) in self.scene.objects.iter().enumerate() {
            let model = self.scene.object_matrix(o);
            let center = model.transform_point3((o.local_bounds.0 + o.local_bounds.1) * 0.5);
            items.push(DrawItem { mesh: o.mesh.clone(), model, material: o.material, object_id: i as u32 + 1, center });
        }
        DrawList { items, materials: self.scene.materials.clone(), lights: self.scene.world_lights(), bounds: self.scene.world_bounds() }
    }

    pub fn sun_dir(&self) -> Vec3 {
        if self.vs.horizon == crate::settings::Horizon::Skybox && self.vs.skybox_use_sun_spot {
            if let Some(d) = self.shared.lock().ok().and_then(|s| s.skybox_sun) {
                return d;
            }
        }
        self.sun.direction()
    }

    pub fn frame_params(&self, pose: &CameraPose, aspect: f32, vs: &VisualSettings, output: i32) -> FrameParams {
        let mut cam = Camera::new();
        cam.pose = *pose;
        cam.ortho_size = self.camera.ortho_size;
        let view = cam.view_matrix(vs.projection);
        let proj = cam.proj_matrix(vs.projection, vs.field_of_view, aspect);
        FrameParams {
            view,
            proj,
            cam_pos: pose.pos,
            sun_dir: self.sun_dir(),
            vs: vs.clone(),
            time: self.time,
            focal: if vs.dof_auto_focus { self.focal } else { vs.dof_focal_point },
            output,
            render_scale: 1.0,
            post_effects: true,
            keep_alpha: false,
        }
    }

    pub fn queue_job(&mut self, width: u32, height: u32, frames: Vec<FrameParams>, kind: ExportKind) -> u64 {
        let id = self.next_job;
        self.next_job += 1;
        let draw = Arc::new(self.build_draw_list());
        if let Ok(mut s) = self.shared.lock() {
            s.jobs.push(RenderJob { id, width, height, frames, draw: Some(draw) });
        }
        self.exports.push((id, kind));
        id
    }

    // ------------------------------------------------------------------ exports

    pub fn render_image(&mut self, ctx: &egui::Context, path: Option<PathBuf>) {
        let ext = if self.vs.file_format == "jpg" { "jpg" } else { "png" };
        let path = match path {
            Some(p) => p,
            None if self.vs.auto_naming => export::auto_name(&self.vs.default_folder, "rendering", ext),
            None => match crate::export::dialog(&self.vs.default_folder).set_title("Save Screenshot").add_filter("PNG", &["png"]).add_filter("JPEG", &["jpg"]).set_file_name(format!("rendering.{ext}")).save_file() {
                Some(p) => p,
                None => return,
            },
        };
        let (w, h) = self.vs.capture_size(self.viewport_px);
        let aspect = w as f32 / h as f32;
        let pose = self.camera.pose;
        let mut frames = vec![self.frame_params(&pose, aspect, &self.vs, 0)];
        frames[0].keep_alpha = self.vs.apply_alpha;
        if self.vs.export_masks {
            for o in 1..=3 {
                frames.push(self.frame_params(&pose, aspect, &self.vs, o));
            }
        }
        let masks = self.vs.export_masks;
        self.queue_job(w, h, frames, ExportKind::Image { path, masks });
        self.notify(ctx, format!("Rendering image {w} x {h}..."));
    }

    pub fn render_panorama(&mut self, ctx: &egui::Context, stereo: bool, pose: CameraPose, path: Option<PathBuf>) {
        let path = match path {
            Some(p) => p,
            None => match crate::export::dialog(&self.vs.default_folder).set_title("Save Panorama").add_filter("PNG", &["png"]).add_filter("JPEG", &["jpg"]).set_file_name("panorama.png").save_file() {
                Some(p) => p,
                None => return,
            },
        };
        let width = self.vs.panorama_resolution.clamp(1024, 8192);
        let face = (width / 4).max(256);
        let mut vs = self.vs.clone();
        vs.projection = Projection::Perspective;
        let eyes: Vec<f32> = if stereo { vec![-0.032, 0.032] } else { vec![0.0] };
        let mut frames = Vec::new();
        for eye in eyes {
            for (f, u) in export::cube_faces() {
                let right = pose.right();
                let pos = pose.pos + right * eye;
                let view = Mat4::look_to_rh(pos, f, u);
                let proj = Mat4::perspective_rh_gl(90f32.to_radians(), 1.0, 0.05, 4000.0);
                frames.push(FrameParams {
                    view,
                    proj,
                    cam_pos: pos,
                    sun_dir: self.sun_dir(),
                    vs: vs.clone(),
                    time: self.time,
                    focal: self.focal,
                    output: 0,
                    render_scale: 1.0,
                    post_effects: false,
                    keep_alpha: false,
                });
            }
        }
        self.queue_job(face, face, frames, ExportKind::Panorama { path, width, stereo });
        self.notify(ctx, "Rendering panorama...");
    }

    pub fn start_video_export(&mut self, ctx: &egui::Context) {
        if self.video.keyframes.len() < 2 {
            self.notify(ctx, "A video path needs at least two keyframes.");
            return;
        }
        let Some(dir) = crate::export::dialog(&self.vs.default_folder).set_title("Choose a folder for the video frames").pick_folder() else { return };
        let fps = self.vs.fps.max(1);
        let total = ((self.video.duration() * fps as f32).ceil() as u32).max(1) + 1;
        let size = self.vs.capture_size(self.viewport_px);
        let ext = if self.vs.bitrate == crate::settings::Bitrate::Lossless || self.vs.file_format == "png" { "png" } else { "jpg" };
        self.video_export = Some(VideoExport { dir, frame: 0, total, size, waiting: false, ext: ext.into() });
    }

    fn step_video_export(&mut self) {
        let Some(ve) = self.video_export.as_ref() else { return };
        if ve.waiting {
            return;
        }
        if ve.frame >= ve.total {
            self.video_export = None;
            return;
        }
        let fps = self.vs.fps.max(1) as f32;
        let (w, h) = ve.size;
        let idx = ve.frame;
        let t = idx as f32 / fps;
        // motion blur: average sub-frames across part of the frame interval
        let subframes = if self.vs.motion_blur > 0.5 { 3 } else { 1 };
        let shutter = self.vs.motion_blur / 100.0 * 0.5 / fps;
        let mut frames = Vec::new();
        for k in 0..subframes {
            let tk = t + if subframes > 1 { shutter * k as f32 / (subframes - 1) as f32 } else { 0.0 };
            if let Some(s) = self.video.sample(tk) {
                let mut vs = self.vs.clone();
                if let Some(f) = s.fov {
                    vs.field_of_view = f;
                }
                let mut fp = self.frame_params(&s.pose, w as f32 / h as f32, &vs, 0);
                if let Some(tod) = s.time_of_day {
                    let mut sun = self.sun;
                    sun.hour = tod;
                    fp.sun_dir = sun.direction();
                }
                if let Some(f) = s.focal {
                    fp.focal = f;
                }
                fp.time = self.time + tk;
                frames.push(fp);
            }
        }
        if let Some(ve) = self.video_export.as_mut() {
            ve.waiting = true;
        }
        self.queue_job(w, h, frames, ExportKind::VideoFrame { index: idx });
    }

    fn collect_results(&mut self, ctx: &egui::Context) {
        let results: Vec<crate::render::JobResult> = self.shared.lock().map(|mut s| std::mem::take(&mut s.results)).unwrap_or_default();
        for r in results {
            let Some(pos) = self.exports.iter().position(|(id, _)| *id == r.id) else { continue };
            let (_, kind) = self.exports.remove(pos);
            let q = self.vs.bitrate.jpeg_quality();
            match kind {
                ExportKind::Image { path, masks } => {
                    if let Some(img) = r.images.first() {
                        let mut img = img.clone();
                        export::apply_watermark(&mut img, &self.gs.watermark_file, self.gs.watermark_anchor);
                        let res = export::save_image(&img, &path, self.vs.apply_alpha, 95);
                        if masks {
                            for (i, suffix) in ["_depth", "_material_id", "_object_id"].iter().enumerate() {
                                if let Some(m) = r.images.get(i + 1) {
                                    let _ = export::save_image(m, &export::with_suffix(&path, suffix), false, 95);
                                }
                            }
                        }
                        match res {
                            Ok(()) => {
                                self.notify(ctx, format!("Saved {}", path.display()));
                                self.last_saved = Some(path.clone());
                            }
                            Err(e) => self.notify(ctx, format!("Could not save: {e}")),
                        }
                    }
                }
                ExportKind::Batch { path } => {
                    if let Some(img) = r.images.first() {
                        let mut img = img.clone();
                        export::apply_watermark(&mut img, &self.gs.watermark_file, self.gs.watermark_anchor);
                        if export::save_image(&img, &path, false, 95).is_ok() {
                            self.last_saved = Some(path.clone());
                        }
                    }
                    if !self.exports.iter().any(|(_, k)| matches!(k, ExportKind::Batch { .. })) {
                        self.notify(ctx, "Batch rendering finished.");
                    }
                }
                ExportKind::Panorama { path, width, stereo } => {
                    let img = if stereo && r.images.len() >= 12 {
                        let l = export::equirect_from_faces(&r.images[0..6], width);
                        let rr = export::equirect_from_faces(&r.images[6..12], width);
                        export::stack_vertical(&l, &rr)
                    } else {
                        export::equirect_from_faces(&r.images, width)
                    };
                    match export::save_image(&img, &path, false, q.max(90)) {
                        Ok(()) => {
                            self.notify(ctx, format!("Saved panorama {}", path.display()));
                            self.last_saved = Some(path.clone());
                        }
                        Err(e) => self.notify(ctx, format!("Could not save: {e}")),
                    }
                }
                ExportKind::VideoFrame { index } => {
                    if !r.images.is_empty() {
                        let mut img = export::average(&r.images);
                        export::apply_watermark(&mut img, &self.gs.watermark_file, self.gs.watermark_anchor);
                        if let Some(ve) = self.video_export.as_mut() {
                            let p = ve.dir.join(format!("frame_{:05}.{}", index, ve.ext));
                            let _ = export::save_image(&img, &p, false, q);
                            ve.frame += 1;
                            ve.waiting = false;
                            if ve.frame >= ve.total {
                                let d = ve.dir.display().to_string();
                                self.video_export = None;
                                self.notify(ctx, format!("Video frames saved to {d}"));
                                self.last_saved = Some(PathBuf::from(&d));
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn batch_render(&mut self, ctx: &egui::Context) {
        let Some(dir) = crate::export::dialog(&self.vs.default_folder).set_title("Select output directory").pick_folder() else { return };
        let ext = if self.vs.file_format == "jpg" { "jpg" } else { "png" };
        let picks: Vec<usize> = self.batch.selected.iter().enumerate().filter(|(_, s)| **s).map(|(i, _)| i).collect();
        if picks.is_empty() {
            self.notify(ctx, "Select at least one view.");
            return;
        }
        for i in picks {
            let Some(v) = self.views.get(i).cloned() else { continue };
            let safe: String = v.name.chars().map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' { c } else { '_' }).collect();
            let mut vs = self.vs.clone();
            if let Some(pn) = &v.preset {
                if let Some(p) = self.presets.iter().find(|p| &p.name == pn) {
                    vs = p.settings.clone();
                }
            }
            vs.field_of_view = v.fov;
            vs.projection = v.projection;
            if self.batch.panorama {
                let saved_sun = self.sun;
                self.sun = v.sun;
                self.render_panorama(ctx, false, v.pose, Some(dir.join(format!("{safe}_panorama.{ext}"))));
                self.sun = saved_sun;
            } else {
                let (w, h) = vs.capture_size(self.viewport_px);
                let saved_sun = self.sun;
                self.sun = v.sun;
                let fp = self.frame_params(&v.pose, w as f32 / h as f32, &vs, 0);
                self.sun = saved_sun;
                self.queue_job(w, h, vec![fp], ExportKind::Batch { path: dir.join(format!("{safe}.{ext}")) });
            }
        }
        self.notify(ctx, "Batch rendering started...");
    }

    // ------------------------------------------------------------------ project files

    pub fn to_project(&self) -> Project {
        let mut assets = Vec::new();
        let mut imports = Vec::new();
        for g in &self.scene.groups {
            match g.kind {
                GroupKind::Asset => assets.push((g.asset.clone().unwrap_or_default(), g.offset.into(), g.rot_y, g.scale)),
                GroupKind::Imported => {
                    let scale = self.watch.iter().find(|w| w.0 == g.id).map(|w| w.3).unwrap_or(1.0);
                    imports.push((g.source.clone().unwrap_or_default(), g.offset.into(), g.rot_y, scale * g.scale))
                }
                _ => {}
            }
        }
        Project {
            visual: self.vs.clone(),
            presets: self.presets.clone(),
            active_preset: self.active_preset,
            general: self.gs.clone(),
            sun: self.sun,
            views: self.views.clone(),
            video: self.video.clone(),
            issues: self.issues.clone(),
            materials: self.scene.materials.clone(),
            assets,
            imports,
            camera: Some(self.camera.pose),
            walk_mode: self.camera.walk_mode,
        }
    }

    pub fn save_project(&mut self, ctx: &egui::Context, ask: bool) {
        let path = if ask || self.project_path.is_none() {
            match crate::export::dialog(&self.vs.default_folder).add_filter("Galaxy Walk project", &["gwproj"]).set_file_name("project.gwproj").save_file() {
                Some(p) => p,
                None => return,
            }
        } else {
            self.project_path.clone().unwrap()
        };
        match serde_json::to_string_pretty(&self.to_project()).map_err(|e| e.to_string()).and_then(|s| std::fs::write(&path, s).map_err(|e| e.to_string())) {
            Ok(()) => {
                self.notify(ctx, format!("Saved {}", path.display()));
                self.last_saved = Some(path.clone());
                self.project_path = Some(path);
            }
            Err(e) => self.notify(ctx, format!("Could not save: {e}")),
        }
    }

    pub fn load_project(&mut self, path: &std::path::Path) {
        let Ok(text) = std::fs::read_to_string(path) else { return };
        let Ok(p) = serde_json::from_str::<Project>(&text) else { return };
        self.scene = scene::sample_scene();
        for m in &p.materials {
            if let Some(cur) = self.scene.materials.iter_mut().find(|c| c.name == m.name) {
                *cur = m.clone();
            } else {
                self.scene.materials.push(m.clone());
            }
        }
        for (name, off, rot, sc) in &p.assets {
            let g = scene::place_asset(&mut self.scene, name, Vec3::from(*off), *rot);
            if let Some(gr) = self.scene.group_mut(g) {
                gr.scale = *sc;
            }
        }
        self.watch.clear();
        for (src, off, rot, sc) in &p.imports {
            if let Ok(g) = scene::import_obj(&mut self.scene, std::path::Path::new(src), *sc) {
                if let Some(gr) = self.scene.group_mut(g) {
                    gr.offset = Vec3::from(*off);
                    gr.rot_y = *rot;
                }
                let mt = std::fs::metadata(src).and_then(|m| m.modified()).ok();
                self.watch.push((g, PathBuf::from(src), mt, *sc));
            }
        }
        self.vs = p.visual;
        if !p.presets.is_empty() {
            self.presets = p.presets;
        }
        self.active_preset = p.active_preset.min(self.presets.len() - 1);
        self.gs = p.general;
        self.sun = p.sun;
        self.views = p.views;
        self.video = p.video;
        self.issues = p.issues;
        if let Some(c) = p.camera {
            self.camera.pose = c;
        }
        self.camera.walk_mode = p.walk_mode;
        self.project_path = Some(path.to_path_buf());
    }

    pub fn import_model(&mut self, ctx: &egui::Context) {
        let Some(path) = crate::export::dialog(&self.vs.default_folder).add_filter("Wavefront OBJ", &["obj"]).pick_file() else { return };
        match scene::import_obj(&mut self.scene, &path, self.import_scale) {
            Ok(g) => {
                let mt = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
                self.watch.push((g, path.clone(), mt, self.import_scale));
                self.notify(ctx, format!("Imported {}", path.display()));
            }
            Err(e) => self.notify(ctx, e),
        }
    }

    /// Live updates: reload imported models whose files changed on disk.
    fn check_live_updates(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        if !self.live_updates || now - self.last_watch_check < 1.0 {
            return;
        }
        self.last_watch_check = now;
        let mut reload = Vec::new();
        for (i, (_, path, mt, _)) in self.watch.iter().enumerate() {
            let cur = std::fs::metadata(path).and_then(|m| m.modified()).ok();
            if cur.is_some() && cur != *mt {
                reload.push(i);
            }
        }
        for i in reload {
            let (g, path, _, scale) = self.watch[i].clone();
            let (off, rot) = self.scene.group(g).map(|gr| (gr.offset, gr.rot_y)).unwrap_or((Vec3::ZERO, 0.0));
            self.scene.remove_group(g);
            if let Ok(ng) = scene::import_obj(&mut self.scene, &path, scale) {
                if let Some(gr) = self.scene.group_mut(ng) {
                    gr.offset = off;
                    gr.rot_y = rot;
                }
                self.watch[i] = (ng, path.clone(), std::fs::metadata(&path).and_then(|m| m.modified()).ok(), scale);
                self.notify(ctx, format!("Live update: reloaded {}", path.display()));
            }
        }
    }

    // ------------------------------------------------------------------ viewport

    pub fn pick_ray(&self, pos: Pos2) -> (Vec3, Vec3) {
        let r = self.viewport_rect;
        let nx = (pos.x - r.left()) / r.width() * 2.0 - 1.0;
        let ny = 1.0 - (pos.y - r.top()) / r.height() * 2.0;
        let aspect = r.width() / r.height().max(1.0);
        let view = self.camera.view_matrix(self.vs.projection);
        let proj = self.camera.proj_matrix(self.vs.projection, self.vs.field_of_view, aspect);
        let inv = (proj * view).inverse();
        let a = inv * Vec4::new(nx, ny, -1.0, 1.0);
        let b = inv * Vec4::new(nx, ny, 1.0, 1.0);
        let a = a.truncate() / a.w;
        let b = b.truncate() / b.w;
        (a, (b - a).normalize())
    }

    pub fn viewport(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let rect = ui.available_rect_before_wrap();
        self.viewport_rect = rect;
        let ppp = ctx.pixels_per_point();
        self.viewport_px = (((rect.width() * ppp) as u32).max(16), ((rect.height() * ppp) as u32).max(16));
        let response = ui.allocate_rect(rect, egui::Sense::click_and_drag());

        // ---- input
        let mut input = NavInput::default();
        let typing = ctx.wants_keyboard_input();
        let shift = ctx.input(|i| i.modifiers.shift);
        if !typing && self.playing.is_none() {
            ctx.input(|i| {
                let k = |a: Key, b: Key| i.key_down(a) || i.key_down(b);
                if k(Key::W, Key::ArrowUp) {
                    input.forward += 1.0;
                }
                if k(Key::S, Key::ArrowDown) {
                    input.forward -= 1.0;
                }
                if k(Key::D, Key::ArrowRight) {
                    input.right += 1.0;
                }
                if k(Key::A, Key::ArrowLeft) {
                    input.right -= 1.0;
                }
                if i.key_down(Key::E) {
                    input.up += 1.0;
                }
                if i.key_down(Key::Q) {
                    input.up -= 1.0;
                }
                input.fast = i.modifiers.shift;
                input.faster = i.modifiers.ctrl;
            });
        }
        let delta = ctx.input(|i| i.pointer.delta());
        let primary = response.dragged_by(PointerButton::Primary);
        let secondary = response.dragged_by(PointerButton::Secondary);
        let middle = response.dragged_by(PointerButton::Middle);
        if self.playing.is_none() {
            if secondary && shift {
                // original: Shift + right drag sets g_hour = start - dx * g_mouseSunSpeed (0x18048c750)
                self.sun.hour = (self.sun.hour - delta.x * crate::camera::TOD_HOURS_PER_PX).rem_euclid(24.0);
                input.fast = false;
            } else if primary && shift {
                // extra in this app: Shift + left drag turns the sun path
                self.sun.rotation = (self.sun.rotation + delta.x * 0.3).rem_euclid(360.0);
                self.sun.angle_offset = (self.sun.angle_offset - delta.y * 0.2).clamp(-60.0, 60.0);
                input.fast = false;
            } else if primary && ctx.input(|i| i.pointer.button_down(PointerButton::Secondary)) {
                input.pan = (delta.x, delta.y);
            } else if primary {
                input.look = (delta.x, delta.y);
            } else if secondary {
                if response.drag_started_by(PointerButton::Secondary) {
                    if let Some(p) = response.interact_pointer_pos() {
                        let (o, d) = self.pick_ray(p);
                        self.orbit_pivot = self.scene.raycast(o, d, 2000.0, true).map(|h| h.point);
                    }
                }
                input.orbit = (delta.x, delta.y);
                input.orbit_pivot = self.orbit_pivot;
            } else if middle {
                input.pan = (delta.x, delta.y);
            }
            if response.hovered() {
                input.wheel = ctx.input(|i| i.smooth_scroll_delta.y);
            }
        }
        if input.forward != 0.0 || input.right != 0.0 || input.up != 0.0 || input.look != (0.0, 0.0) || input.orbit != (0.0, 0.0) || input.pan != (0.0, 0.0) || input.wheel != 0.0 {
            self.moved = true;
        }
        let dt = ctx.input(|i| i.stable_dt).min(0.1);
        if self.playing.is_none() {
            self.camera.update(dt, &input, &self.gs, &self.scene);
        }

        // ---- clicks
        if response.double_clicked() && self.mode != Mode::Assets {
            if let Some(p) = response.interact_pointer_pos() {
                let (o, d) = self.pick_ray(p);
                if let Some(h) = self.scene.raycast(o, d, 2000.0, true) {
                    self.camera.teleport(h.point, h.normal, &self.gs);
                }
            }
        } else if response.clicked() {
            if let Some(p) = response.interact_pointer_pos() {
                let (o, d) = self.pick_ray(p);
                let hit = self.scene.raycast(o, d, 2000.0, false);
                match self.mode {
                    Mode::Assets => {
                        if let Some(name) = self.asset_to_place.clone() {
                            if let Some(h) = self.scene.raycast(o, d, 2000.0, true) {
                                let g = scene::place_asset(&mut self.scene, &name, h.point, self.asset_rotation);
                                self.selected_group = Some(g);
                                if !shift {
                                    self.asset_to_place = None;
                                }
                            }
                        } else {
                            self.selected_group = hit.and_then(|h| {
                                let gid = self.scene.objects[h.object].group;
                                self.scene.group(gid).filter(|g| g.kind != GroupKind::Building).map(|g| g.id)
                            });
                        }
                    }
                    Mode::Info => self.selected_object = hit.map(|h| h.object),
                    Mode::Issues => {
                        if let Some(h) = hit {
                            self.issue_draft.point = Some(h.point);
                        }
                    }
                    _ => {
                        if self.show_material {
                            if let Some(h) = hit {
                                self.selected_material = self.scene.objects[h.object].material;
                            }
                        }
                    }
                }
            }
        }

        // ---- keyboard shortcuts
        if !typing {
            let (space, home, pgup, pgdn, help, f11, esc, num) = ctx.input(|i| {
                let num = [Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5].iter().position(|k| i.key_pressed(*k));
                (i.key_pressed(Key::Space), i.key_pressed(Key::Home), i.key_pressed(Key::PageUp), i.key_pressed(Key::PageDown), i.key_pressed(Key::H), i.key_pressed(Key::F11), i.key_pressed(Key::Escape), num)
            });
            if space {
                self.camera.walk_mode = !self.camera.walk_mode;
            }
            if home {
                self.sun = self.sun_start;
            }
            if pgup || pgdn {
                self.cycle_view(if pgdn { 1 } else { -1 });
            }
            if help {
                self.gs.show_help = !self.gs.show_help;
            }
            if f11 && (!self.gs.screenshot_hotkey_shift || shift) {
                self.render_image(ctx, None);
            }
            if esc {
                if self.show_visual || self.show_general || self.show_material || self.show_batch || self.show_sun {
                    self.show_visual = false;
                    self.show_general = false;
                    self.show_material = false;
                    self.show_batch = false;
                    self.show_sun = false;
                } else if self.asset_to_place.is_some() {
                    self.asset_to_place = None;
                } else {
                    self.mode = Mode::Walkthrough;
                    self.playing = None;
                }
            }
            if let Some(n) = num {
                self.standard_view(n);
            }
            if self.mode == Mode::Video && ctx.input(|i| i.key_pressed(Key::Plus) || i.key_pressed(Key::Equals)) {
                self.video.add(self.camera.pose, self.selected_keyframe);
            }
            if self.mode == Mode::Assets && ctx.input(|i| i.key_pressed(Key::Delete)) {
                if let Some(g) = self.selected_group.take() {
                    self.scene.remove_group(g);
                }
            }
            if self.mode == Mode::Video && ctx.input(|i| i.key_pressed(Key::Delete)) {
                if let Some(k) = self.selected_keyframe.take() {
                    self.video.remove(k);
                }
            }
        }

        // ---- video preview
        if let Some(t) = self.playing.as_mut() {
            *t += dt;
            let t = *t;
            if let Some(s) = self.video.sample(t) {
                self.camera.pose = s.pose;
                if let Some(tod) = s.time_of_day {
                    self.sun.hour = tod;
                }
                if let Some(f) = s.fov {
                    self.vs.field_of_view = f;
                }
            }
            if t > self.video.duration() {
                self.playing = None;
            }
        }

        // ---- auto focus every few frames
        if self.vs.dof_auto_focus && self.frames % 6 == 0 {
            let (o, d) = (self.camera.pose.pos, self.camera.pose.forward());
            self.focal = self.scene.raycast(o, d, 2000.0, true).map(|h| h.t).unwrap_or(50.0);
        }

        // ---- frame parameters for the paint callback
        let aspect = rect.width() / rect.height().max(1.0);
        let mut fp = self.frame_params(&self.camera.pose, aspect, &self.vs, 0);
        let mut scale = self.vs.quality.render_scale();
        if self.vs.auto_resolution && self.moved {
            scale *= 0.7;
        }
        fp.render_scale = scale;
        let draw = Arc::new(self.build_draw_list());
        if let Ok(mut s) = self.shared.lock() {
            s.frame = Some(fp);
            s.draw = draw;
        }
        let shared = self.shared.clone();
        let renderer = self.renderer.clone();
        let cb = eframe::egui_glow::CallbackFn::new(move |info, painter| {
            let gl = painter.gl();
            let mut r = renderer.lock().unwrap();
            if r.is_none() {
                match Renderer::new(gl) {
                    Ok(rr) => *r = Some(rr),
                    Err(e) => {
                        if let Ok(mut s) = shared.lock() {
                            s.error = Some(e);
                        }
                        return;
                    }
                }
            }
            let vp = info.viewport_in_pixels();
            if let Some(r) = r.as_mut() {
                r.paint_viewport(gl, &shared, (vp.left_px, vp.from_bottom_px, vp.width_px, vp.height_px), painter.intermediate_fbo());
            }
        });
        ui.painter().add(egui::PaintCallback { rect, callback: Arc::new(cb) });
        self.moved = false;

        crate::ui::viewport_overlay(self, ui, ctx, rect);
    }

    pub fn cycle_view(&mut self, dir: i32) {
        let favs: Vec<usize> = self.views.iter().enumerate().filter(|(_, v)| v.favorite).map(|(i, _)| i).collect();
        let list: Vec<usize> = if favs.is_empty() { (0..self.views.len()).collect() } else { favs };
        if list.is_empty() {
            return;
        }
        let cur = self.views.iter().position(|v| v.pose.pos.distance(self.camera.pose.pos) < 0.05);
        let idx = match cur.and_then(|c| list.iter().position(|i| *i == c)) {
            Some(p) => (p as i32 + dir).rem_euclid(list.len() as i32) as usize,
            None => 0,
        };
        self.jump_to_view(list[idx], false);
    }

    pub fn jump_to_view(&mut self, i: usize, instant: bool) {
        let Some(v) = self.views.get(i).cloned() else { return };
        if instant {
            self.camera.pose = v.pose;
        } else {
            self.camera.start_transition(v.pose, 1.0);
        }
        self.sun = v.sun;
        self.vs.field_of_view = v.fov;
        self.vs.projection = v.projection;
        if let Some(pn) = &v.preset {
            if let Some(pi) = self.presets.iter().position(|p| &p.name == pn) {
                self.active_preset = pi;
                self.vs = self.presets[pi].settings.clone();
                self.vs.field_of_view = v.fov;
                self.vs.projection = v.projection;
            }
        }
    }

    pub fn standard_view(&mut self, n: usize) {
        let (mn, mx) = self.scene.world_bounds();
        let c = (mn + mx) * 0.5;
        let r = (mx - mn).length() * 0.6 + 5.0;
        let (pos, yaw, pitch) = match n {
            0 => (c + Vec3::new(0.0, r * 0.25, r), 0.0, -14.0),
            1 => (c + Vec3::new(r, r * 0.25, 0.0), -90.0, -14.0),
            2 => (c + Vec3::new(0.0, r * 0.25, -r), 180.0, -14.0),
            3 => (c + Vec3::new(-r, r * 0.25, 0.0), 90.0, -14.0),
            _ => (c + Vec3::new(0.0, r * 1.2, 0.01), 0.0, -89.0),
        };
        self.camera.walk_mode = false;
        self.camera.start_transition(CameraPose { pos, yaw, pitch }, 0.8);
    }

    pub fn create_view(&mut self, name: String) {
        let preset = self.presets.get(self.active_preset).map(|p| p.name.clone());
        self.views.push(SavedView { name, pose: self.camera.pose, sun: self.sun, fov: self.vs.field_of_view, projection: self.vs.projection, favorite: false, preset });
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.frames += 1;
        self.time += ctx.input(|i| i.stable_dt).min(0.1);
        self.collect_results(ctx);
        self.step_video_export();
        self.check_live_updates(ctx);
        if let Some(mut b) = self.bridge.take() {
            b.poll(self, ctx);
            self.bridge = Some(b);
        }

        if let Some(e) = self.shared.lock().ok().and_then(|s| s.error.clone()) {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.heading("The 3D view could not start");
                ui.label(e);
            });
            return;
        }

        let title = if self.gs.window_title.is_empty() { "Galaxy Walk".to_string() } else { self.gs.window_title.clone() };
        if self.frames % 30 == 1 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
        }

        crate::ui::menu_bar(self, ctx);
        crate::ui::toolbar(self, ctx);
        crate::ui::status_bar(self, ctx);
        crate::ui::side_panels(self, ctx);
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(Color32::BLACK)).show(ctx, |ui| {
            self.viewport(ui, ctx);
        });
        crate::ui::windows(self, ctx);

        // --screenshot: draw ~120 frames, save the window picture and quit
        if let Some(path) = self.screenshot.clone() {
            if self.frames >= 120 && !self.screenshot_requested {
                self.screenshot_requested = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            }
            let shot = ctx.input(|i| {
                i.events.iter().find_map(|e| if let egui::Event::Screenshot { image, .. } = e { Some(image.clone()) } else { None })
            });
            if let Some(img) = shot {
                let (w, h) = (img.size[0] as u32, img.size[1] as u32);
                let mut buf = Vec::with_capacity((w * h * 4) as usize);
                for c in &img.pixels {
                    buf.extend_from_slice(&[c.r(), c.g(), c.b(), 255]);
                }
                if let Some(im) = image::RgbaImage::from_raw(w, h, buf) {
                    if let Some(dir) = path.parent() {
                        let _ = std::fs::create_dir_all(dir);
                    }
                    let _ = im.save(&path);
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            if self.frames > 600 {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
        ctx.request_repaint();
    }
}
