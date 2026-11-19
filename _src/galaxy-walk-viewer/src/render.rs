//! OpenGL renderer: shadow map, HDR scene pass with sky, and the image pipeline.
//! Runs inside egui's paint callback; exports are queued as jobs and rendered off-screen there.

use crate::scene::{Material, Mesh, TransparencyType};
use crate::settings::{kelvin_to_rgb, Projection, RenderingStyle, VisualSettings};
use crate::shaders;
use eframe::glow::{self, HasContext};
use glam::{Mat4, Vec3, Vec4};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub struct DrawItem {
    pub mesh: Arc<Mesh>,
    pub model: Mat4,
    pub material: usize,
    pub object_id: u32,
    pub center: Vec3,
    /// Bounding-sphere radius in world units, for frustum culling.
    pub radius: f32,
}

/// Six planes of a view-projection matrix (Gribb/Hartmann), normalised; a sphere is outside when it is
/// fully behind any plane.
pub fn frustum_planes(vp: Mat4) -> [Vec4; 6] {
    let r0 = vp.row(0);
    let r1 = vp.row(1);
    let r2 = vp.row(2);
    let r3 = vp.row(3);
    let mut pl = [r3 + r0, r3 - r0, r3 + r1, r3 - r1, r3 + r2, r3 - r2];
    for p in pl.iter_mut() {
        let l = p.truncate().length();
        if l > 1e-6 {
            *p /= l;
        }
    }
    pl
}

pub fn sphere_visible(planes: &[Vec4; 6], c: Vec3, r: f32) -> bool {
    planes.iter().all(|p| p.truncate().dot(c) + p.w >= -r)
}

#[derive(Default)]
pub struct DrawList {
    pub items: Vec<DrawItem>,
    pub materials: Vec<Material>,
    pub lights: Vec<(Vec3, [f32; 3], f32)>,
    pub bounds: (Vec3, Vec3),
}

/// Radius (m) of the sharp shadow cascade that follows the camera.
const NEAR_CASCADE_RADIUS: f32 = 30.0;

#[derive(Clone)]
pub struct FrameParams {
    pub view: Mat4,
    pub proj: Mat4,
    pub cam_pos: Vec3,
    pub sun_dir: Vec3,
    pub vs: VisualSettings,
    pub time: f32,
    pub focal: f32,
    /// 0 beauty, 1 depth, 2 material id, 3 object id
    pub output: i32,
    pub render_scale: f32,
    pub post_effects: bool,
    pub keep_alpha: bool,
}

pub struct RenderJob {
    pub id: u64,
    pub width: u32,
    pub height: u32,
    pub frames: Vec<FrameParams>,
    pub draw: Option<Arc<DrawList>>,
}

pub struct JobResult {
    pub id: u64,
    pub images: Vec<image::RgbaImage>,
}

#[derive(Default)]
pub struct Shared {
    pub draw: Arc<DrawList>,
    pub frame: Option<FrameParams>,
    pub jobs: Vec<RenderJob>,
    pub results: Vec<JobResult>,
    pub skybox_sun: Option<Vec3>,
    pub error: Option<String>,
    pub sun_visible: bool,
}

pub type SharedRef = Arc<Mutex<Shared>>;

struct Prog {
    p: glow::Program,
    locs: RefCell<HashMap<String, Option<glow::UniformLocation>>>,
}

impl Prog {
    fn loc(&self, gl: &glow::Context, name: &str) -> Option<glow::UniformLocation> {
        let mut l = self.locs.borrow_mut();
        if let Some(v) = l.get(name) {
            return v.clone();
        }
        let v = unsafe { gl.get_uniform_location(self.p, name) };
        l.insert(name.to_string(), v.clone());
        v
    }
    fn f1(&self, gl: &glow::Context, n: &str, v: f32) {
        unsafe { gl.uniform_1_f32(self.loc(gl, n).as_ref(), v) }
    }
    fn i1(&self, gl: &glow::Context, n: &str, v: i32) {
        unsafe { gl.uniform_1_i32(self.loc(gl, n).as_ref(), v) }
    }
    fn f2(&self, gl: &glow::Context, n: &str, a: f32, b: f32) {
        unsafe { gl.uniform_2_f32(self.loc(gl, n).as_ref(), a, b) }
    }
    fn f3(&self, gl: &glow::Context, n: &str, v: [f32; 3]) {
        unsafe { gl.uniform_3_f32(self.loc(gl, n).as_ref(), v[0], v[1], v[2]) }
    }
    fn m4(&self, gl: &glow::Context, n: &str, m: &Mat4) {
        unsafe { gl.uniform_matrix_4_f32_slice(self.loc(gl, n).as_ref(), false, &m.to_cols_array()) }
    }
    fn f3v(&self, gl: &glow::Context, n: &str, v: &[f32]) {
        unsafe { gl.uniform_3_f32_slice(self.loc(gl, n).as_ref(), v) }
    }
}

struct GpuMesh {
    vao: glow::VertexArray,
    vbo: glow::Buffer,
    ebo: glow::Buffer,
    count: i32,
    used: bool,
}

struct Target {
    fbo: glow::Framebuffer,
    color: glow::Texture,
    depth: Option<glow::Texture>,
    w: i32,
    h: i32,
}

pub struct Renderer {
    scene: Prog,
    sky: Prog,
    shadow: Prog,
    post: Prog,
    empty_vao: glow::VertexArray,
    meshes: HashMap<usize, GpuMesh>,
    textures: HashMap<String, Option<glow::Texture>>,
    hdr: Option<Target>,
    out: Option<Target>,
    shadow_map: Option<(glow::Framebuffer, glow::Texture, i32)>,
    skybox: Option<(String, glow::Texture, f32)>,
    white_tex: glow::Texture,
}

fn compile(gl: &glow::Context, vs: &str, fs: &str) -> Result<Prog, String> {
    unsafe {
        let p = gl.create_program()?;
        let mut shaders_v = Vec::new();
        for (kind, src) in [(glow::VERTEX_SHADER, vs), (glow::FRAGMENT_SHADER, fs)] {
            let s = gl.create_shader(kind)?;
            gl.shader_source(s, src);
            gl.compile_shader(s);
            if !gl.get_shader_compile_status(s) {
                return Err(format!("shader compile error: {}", gl.get_shader_info_log(s)));
            }
            gl.attach_shader(p, s);
            shaders_v.push(s);
        }
        gl.link_program(p);
        if !gl.get_program_link_status(p) {
            return Err(format!("shader link error: {}", gl.get_program_info_log(p)));
        }
        for s in shaders_v {
            gl.detach_shader(p, s);
            gl.delete_shader(s);
        }
        Ok(Prog { p, locs: RefCell::new(HashMap::new()) })
    }
}

/// Physically scaled sun and sky values derived from the sun direction and settings.
pub struct Atmos {
    pub sun_color: Vec3, // lux per channel
    pub day: f32,
    pub sky_lum: f32,
}

pub fn atmosphere(sun: Vec3, vs: &VisualSettings) -> Atmos {
    let alt = sun.y.clamp(-1.0, 1.0).asin().to_degrees();
    let up = sun.y.max(0.0);
    let airmass = 1.0 / (up + 0.50572 * (alt.max(-5.0) + 6.07995).powf(-1.6364)).max(0.02);
    let k = Vec3::new(0.045, 0.085, 0.20);
    let trans = Vec3::new((-k.x * airmass).exp(), (-k.y * airmass).exp(), (-k.z * airmass).exp());
    let above = smoothstep(-0.03, 0.04, sun.y);
    let sun_color = trans * 115_000.0 * (vs.sun_brightness / 80.0) * above;
    let day = smoothstep(-0.18, 0.12, sun.y);
    let sky_lum = 2600.0 * day.powf(1.6) * (vs.sun_brightness / 80.0).sqrt() + 0.015 * (vs.night_sky_brightness / 100.0);
    Atmos { sun_color, day, sky_lum }
}

pub fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

impl Renderer {
    pub fn new(gl: &glow::Context) -> Result<Self, String> {
        let common = shaders::COMMON;
        let scene = compile(gl, &format!("{common}{}", shaders::SCENE_VS), &format!("{common}{}{}{}", shaders::SKY, shaders::FOG, shaders::SCENE_FS))?;
        let sky = compile(gl, &format!("{common}{}", shaders::SKY_VS), &format!("{common}{}{}{}", shaders::SKY, shaders::FOG, shaders::SKY_FS))?;
        let shadow = compile(gl, &format!("{common}{}", shaders::SHADOW_VS), &format!("{common}{}", shaders::SHADOW_FS))?;
        let post = compile(gl, &format!("{common}{}", shaders::SKY_VS), &format!("{common}{}", shaders::POST_FS))?;
        unsafe {
            let empty_vao = gl.create_vertex_array()?;
            let white_tex = gl.create_texture()?;
            gl.bind_texture(glow::TEXTURE_2D, Some(white_tex));
            gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8 as i32, 1, 1, 0, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelUnpackData::Slice(Some(&[255, 255, 255, 255])));
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
            Ok(Self {
                scene,
                sky,
                shadow,
                post,
                empty_vao,
                meshes: HashMap::new(),
                textures: HashMap::new(),
                hdr: None,
                out: None,
                shadow_map: None,
                skybox: None,
                white_tex,
            })
        }
    }

    fn mesh(&mut self, gl: &glow::Context, mesh: &Arc<Mesh>) -> Option<(glow::VertexArray, i32)> {
        let key = Arc::as_ptr(mesh) as usize;
        if let Some(g) = self.meshes.get_mut(&key) {
            g.used = true;
            return Some((g.vao, g.count));
        }
        unsafe {
            let vao = gl.create_vertex_array().ok()?;
            let vbo = gl.create_buffer().ok()?;
            let ebo = gl.create_buffer().ok()?;
            gl.bind_vertex_array(Some(vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
            let vbytes = std::slice::from_raw_parts(mesh.vertices.as_ptr() as *const u8, mesh.vertices.len() * std::mem::size_of::<crate::scene::Vertex>());
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, vbytes, glow::STATIC_DRAW);
            gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(ebo));
            let ibytes = std::slice::from_raw_parts(mesh.indices.as_ptr() as *const u8, mesh.indices.len() * 4);
            gl.buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER, ibytes, glow::STATIC_DRAW);
            let stride = 32;
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(0, 3, glow::FLOAT, false, stride, 0);
            gl.enable_vertex_attrib_array(1);
            gl.vertex_attrib_pointer_f32(1, 3, glow::FLOAT, false, stride, 12);
            gl.enable_vertex_attrib_array(2);
            gl.vertex_attrib_pointer_f32(2, 2, glow::FLOAT, false, stride, 24);
            gl.bind_vertex_array(None);
            let count = mesh.indices.len() as i32;
            self.meshes.insert(key, GpuMesh { vao, vbo, ebo, count, used: true });
            Some((vao, count))
        }
    }

    fn texture(&mut self, gl: &glow::Context, path: &str) -> Option<glow::Texture> {
        if let Some(t) = self.textures.get(path) {
            return *t;
        }
        let tex = image::open(path).ok().map(|img| {
            let img = img.to_rgba8();
            unsafe {
                let t = gl.create_texture().unwrap();
                gl.bind_texture(glow::TEXTURE_2D, Some(t));
                gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8 as i32, img.width() as i32, img.height() as i32, 0, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelUnpackData::Slice(Some(img.as_raw())));
                gl.generate_mipmap(glow::TEXTURE_2D);
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR_MIPMAP_LINEAR as i32);
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::REPEAT as i32);
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::REPEAT as i32);
                t
            }
        });
        self.textures.insert(path.to_string(), tex);
        tex
    }

    /// Load (or reuse) the skybox; returns the brightest direction when found.
    fn skybox(&mut self, gl: &glow::Context, vs: &VisualSettings) -> Option<(glow::Texture, f32, Option<Vec3>)> {
        if vs.skybox_file.is_empty() {
            return None;
        }
        if let Some((p, t, avg)) = &self.skybox {
            if *p == vs.skybox_file {
                return Some((*t, *avg, None));
            }
        }
        let img = image::open(&vs.skybox_file).ok()?.to_rgb32f();
        let (w, h) = (img.width(), img.height());
        let mut data: Vec<f32> = Vec::with_capacity((w * h * 3) as usize);
        let mut sum = 0.0f64;
        let mut best = (0.0f32, 0u32, 0u32);
        let is_hdr = vs.skybox_file.to_lowercase().ends_with(".hdr");
        for (x, y, p) in img.enumerate_pixels() {
            let c = if is_hdr { [p[0], p[1], p[2]] } else { [p[0].powf(2.2), p[1].powf(2.2), p[2].powf(2.2)] };
            let l = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
            sum += l as f64;
            if l > best.0 {
                best = (l, x, y);
            }
            data.extend_from_slice(&c);
        }
        let avg = (sum / (w * h).max(1) as f64) as f32;
        unsafe {
            let t = gl.create_texture().ok()?;
            gl.bind_texture(glow::TEXTURE_2D, Some(t));
            let bytes = std::slice::from_raw_parts(data.as_ptr() as *const u8, data.len() * 4);
            gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGB16F as i32, w as i32, h as i32, 0, glow::RGB, glow::FLOAT, glow::PixelUnpackData::Slice(Some(bytes)));
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::REPEAT as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
            if let Some((_, old, _)) = self.skybox.take() {
                gl.delete_texture(old);
            }
            self.skybox = Some((vs.skybox_file.clone(), t, avg));
            let u = best.1 as f32 / w as f32;
            let v = best.2 as f32 / h as f32;
            let az = (u - 0.5) * std::f32::consts::TAU;
            let polar = v * std::f32::consts::PI;
            let dir = Vec3::new(az.sin() * polar.sin(), polar.cos(), -az.cos() * polar.sin());
            Some((t, avg, Some(dir)))
        }
    }

    fn ensure_target(gl: &glow::Context, t: &mut Option<Target>, w: i32, h: i32, hdr: bool) {
        if let Some(cur) = t {
            if cur.w == w && cur.h == h {
                return;
            }
            unsafe {
                gl.delete_framebuffer(cur.fbo);
                gl.delete_texture(cur.color);
                if let Some(d) = cur.depth {
                    gl.delete_texture(d);
                }
            }
            *t = None;
        }
        unsafe {
            let fbo = gl.create_framebuffer().unwrap();
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            let color = gl.create_texture().unwrap();
            gl.bind_texture(glow::TEXTURE_2D, Some(color));
            if hdr {
                gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA16F as i32, w, h, 0, glow::RGBA, glow::HALF_FLOAT, glow::PixelUnpackData::Slice(None));
                gl.generate_mipmap(glow::TEXTURE_2D);
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR_MIPMAP_LINEAR as i32);
            } else {
                gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8 as i32, w, h, 0, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelUnpackData::Slice(None));
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
            }
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
            gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(color), 0);
            let depth = if hdr {
                let d = gl.create_texture().unwrap();
                gl.bind_texture(glow::TEXTURE_2D, Some(d));
                gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::DEPTH_COMPONENT24 as i32, w, h, 0, glow::DEPTH_COMPONENT, glow::UNSIGNED_INT, glow::PixelUnpackData::Slice(None));
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32);
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32);
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
                gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::DEPTH_ATTACHMENT, glow::TEXTURE_2D, Some(d), 0);
                Some(d)
            } else {
                None
            };
            *t = Some(Target { fbo, color, depth, w, h });
        }
    }

    fn ensure_shadow(&mut self, gl: &glow::Context, size: i32) {
        if let Some((_, _, s)) = self.shadow_map {
            if s == size {
                return;
            }
        }
        unsafe {
            if let Some((f, t, _)) = self.shadow_map.take() {
                gl.delete_framebuffer(f);
                gl.delete_texture(t);
            }
            let fbo = gl.create_framebuffer().unwrap();
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            let t = gl.create_texture().unwrap();
            gl.bind_texture(glow::TEXTURE_2D, Some(t));
            gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::DEPTH_COMPONENT24 as i32, size * 2, size, 0, glow::DEPTH_COMPONENT, glow::UNSIGNED_INT, glow::PixelUnpackData::Slice(None));
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_COMPARE_MODE, glow::COMPARE_REF_TO_TEXTURE as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_COMPARE_FUNC, glow::LEQUAL as i32);
            gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::DEPTH_ATTACHMENT, glow::TEXTURE_2D, Some(t), 0);
            gl.draw_buffers(&[glow::NONE]);
            gl.read_buffer(glow::NONE);
            self.shadow_map = Some((fbo, t, size));
        }
    }

    /// Sun shadow matrix for a sphere (centre, radius). `snap` is the shadow map size: the centre is
    /// snapped to whole texels so the near cascade does not shimmer while walking.
    fn light_matrix_for(center: Vec3, radius: f32, sun: Vec3, snap: i32) -> Mat4 {
        let sun = if sun.y < 0.05 { Vec3::new(sun.x, 0.05, sun.z).normalize() } else { sun };
        let up = if sun.y.abs() > 0.98 { Vec3::Z } else { Vec3::Y };
        let view0 = Mat4::look_at_rh(sun * radius * 2.0, Vec3::ZERO, up);
        let mut c = view0.transform_point3(center);
        let texel = 2.0 * radius / snap.max(1) as f32;
        c.x = (c.x / texel).round() * texel;
        c.y = (c.y / texel).round() * texel;
        let proj = Mat4::orthographic_rh_gl(c.x - radius, c.x + radius, c.y - radius, c.y + radius, -c.z - radius * 4.0, -c.z + radius * 4.0);
        proj * view0
    }

    /// Two cascades: [0] a sharp one around the camera, [1] the whole model.
    fn light_matrices(draw: &DrawList, sun: Vec3, cam: Vec3, size: i32) -> ([Mat4; 2], [f32; 2]) {
        let (mn, mx) = draw.bounds;
        let center = (mn + mx) * 0.5;
        let far_r = ((mx - mn).length() * 0.5).clamp(5.0, 600.0) + 4.0;
        let near_r = NEAR_CASCADE_RADIUS.min(far_r);
        let near_c = cam.clamp(mn - Vec3::splat(near_r), mx + Vec3::splat(near_r));
        (
            [Self::light_matrix_for(near_c, near_r, sun, size), Self::light_matrix_for(center, far_r, sun, size)],
            [2.0 * near_r / size as f32, 2.0 * far_r / size as f32],
        )
    }

    /// Shadow + HDR scene pass into the internal HDR target of size w x h.
    fn render_hdr(&mut self, gl: &glow::Context, draw: &DrawList, p: &FrameParams, w: i32, h: i32, shared: &SharedRef) {
        let vs = &p.vs;
        let atm = atmosphere(p.sun_dir, vs);
        let size = vs.quality.shadow_size();
        let (cascades, texels) = Self::light_matrices(draw, p.sun_dir, p.cam_pos, size);
        let light_vp = cascades[1];
        unsafe {
            gl.disable(glow::SCISSOR_TEST);
            gl.disable(glow::BLEND);
            gl.disable(glow::CULL_FACE);
            gl.enable(glow::DEPTH_TEST);
            gl.depth_func(glow::LEQUAL);
            gl.depth_mask(true);
            gl.color_mask(true, true, true, true);

            // ---- shadow map: two cascades side by side in one atlas
            self.ensure_shadow(gl, size);
            let (sfbo, stex, _) = self.shadow_map.unwrap();
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(sfbo));
            gl.viewport(0, 0, size * 2, size);
            gl.clear(glow::DEPTH_BUFFER_BIT);
            gl.use_program(Some(self.shadow.p));
            gl.enable(glow::POLYGON_OFFSET_FILL);
            gl.polygon_offset(2.0, 4.0);
            for (ci, cvp) in cascades.iter().enumerate() {
                gl.viewport(ci as i32 * size, 0, size, size);
                self.shadow.m4(gl, "uLightVP", cvp);
                let planes = frustum_planes(*cvp);
                for it in &draw.items {
                    let mat = &draw.materials[it.material.min(draw.materials.len() - 1)];
                    if mat.is_transparent() && mat.mtype != crate::scene::MaterialType::Foliage {
                        continue;
                    }
                    if !sphere_visible(&planes, it.center, it.radius) {
                        continue;
                    }
                    if let Some((vao, count)) = self.mesh(gl, &it.mesh) {
                        self.shadow.m4(gl, "uModel", &it.model);
                        gl.bind_vertex_array(Some(vao));
                        gl.draw_elements(glow::TRIANGLES, count, glow::UNSIGNED_INT, 0);
                    }
                }
            }
            gl.disable(glow::POLYGON_OFFSET_FILL);

            // ---- HDR target
            Self::ensure_target(gl, &mut self.hdr, w, h, true);
            let (hdr_fbo, hdr_color) = { let t = self.hdr.as_ref().unwrap(); (t.fbo, t.color) };
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(hdr_fbo));
            gl.viewport(0, 0, w, h);
            gl.clear_color(0.0, 0.0, 0.0, 0.0);
            gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
            let vp = p.proj * p.view;

            // skybox
            let sky = self.skybox(gl, vs);
            if let Some((_, _, Some(dir))) = sky {
                if let Ok(mut s) = shared.lock() {
                    s.skybox_sun = Some(dir);
                }
            }
            let use_skybox = vs.horizon == crate::settings::Horizon::Skybox && sky.is_some();
            let sky_scale = match sky {
                Some((_, avg, _)) if vs.skybox_normalize => (vs.skybox_brightness / std::f32::consts::PI) / avg.max(1e-4),
                Some(_) => 1000.0,
                None => 1.0,
            };

            let set_common = |prog: &Prog, gl: &glow::Context| {
                prog.f3(gl, "uSunDir", p.sun_dir.into());
                prog.f3(gl, "uSunColor", atm.sun_color.into());
                prog.f1(gl, "uDay", atm.day);
                prog.f1(gl, "uSkyLum", atm.sky_lum);
                prog.f1(gl, "uNightSky", vs.night_sky_brightness / 100.0);
                prog.f1(gl, "uMoonSize", vs.moon_size);
                prog.f1(gl, "uCloudDensity", vs.cloud_density);
                prog.f1(gl, "uCloudVariety", vs.cloud_variety);
                prog.f1(gl, "uCloudCirrus", vs.cloud_cirrus);
                prog.i1(gl, "uContrails", vs.cloud_contrails);
                let wind = vs.wind_intensity / 100.0;
                let wd = vs.wind_direction.to_radians();
                prog.f2(gl, "uCloudOffset", vs.cloud_x / 1000.0 + p.time * 0.004 * wind * wd.sin(), vs.cloud_y / 1000.0 - p.time * 0.004 * wind * wd.cos());
                prog.i1(gl, "uHorizon", if use_skybox { 0 } else { vs.horizon.index() });
                prog.f1(gl, "uHorizonRot", vs.horizon_rotation);
                prog.i1(gl, "uWhiteBg", vs.white_background as i32);
                prog.i1(gl, "uUseSkybox", use_skybox as i32);
                prog.i1(gl, "uSkybox", 2);
                prog.f1(gl, "uSkyboxScale", sky_scale);
                prog.f1(gl, "uTime", p.time);
                prog.f1(gl, "uTimeV", p.time);
                prog.f1(gl, "uFogDensity", vs.fog_intensity / 100.0 * 0.012);
                prog.f1(gl, "uFogHeight", vs.fog_height);
                prog.f3(gl, "uCamPos", p.cam_pos.into());
                prog.i1(gl, "uOutput", p.output);
            };
            gl.active_texture(glow::TEXTURE2);
            gl.bind_texture(glow::TEXTURE_2D, Some(sky.map(|s| s.0).unwrap_or(self.white_tex)));

            gl.depth_mask(false);
            gl.use_program(Some(self.sky.p));
            set_common(&self.sky, gl);
            self.sky.m4(gl, "uInvViewProj", &vp.inverse());
            gl.bind_vertex_array(Some(self.empty_vao));
            gl.draw_arrays(glow::TRIANGLES, 0, 3);
            gl.depth_mask(true);

            // geometry
            gl.use_program(Some(self.scene.p));
            set_common(&self.scene, gl);
            self.scene.m4(gl, "uViewProj", &vp);
            self.scene.m4(gl, "uLightVP", &light_vp);
            self.scene.m4(gl, "uCascade0", &cascades[0]);
            self.scene.m4(gl, "uCascade1", &cascades[1]);
            self.scene.f2(gl, "uCascadeTexel", texels[0], texels[1]);
            self.scene.i1(gl, "uShadow", 1);
            self.scene.i1(gl, "uTex", 0);
            gl.active_texture(glow::TEXTURE1);
            gl.bind_texture(glow::TEXTURE_2D, Some(stex));
            let soft = 0.5 + (100.0 - vs.shadow_sharpness) / 100.0 * 3.5;
            self.scene.f1(gl, "uShadowSoft", soft);
            self.scene.i1(gl, "uShadowTaps", vs.quality.shadow_taps());
            let style = match vs.rendering_style {
                RenderingStyle::None => 0,
                RenderingStyle::White => 1,
                RenderingStyle::Polystyrol => 2,
                RenderingStyle::LightView => 3,
            };
            self.scene.i1(gl, "uStyle", style);
            self.scene.f1(gl, "uTransmission", vs.polystyrol_transmission / 100.0);
            self.scene.f1(gl, "uAmbient", vs.ambient_brightness / 50.0);
            self.scene.f1(gl, "uLightScale", vs.artificial_light_brightness / 100.0);
            self.scene.i1(gl, "uGrassOn", vs.grass_rendering as i32);
            self.scene.f1(gl, "uDepthRange", vs.depth_range);
            let wd = vs.wind_direction.to_radians();
            let wind = vs.wind_intensity / 25.0;
            self.scene.f2(gl, "uWind", wd.sin() * wind, -wd.cos() * wind);
            self.scene.f2(gl, "uWindDir", wd.sin() * vs.wind_intensity / 100.0, -wd.cos() * vs.wind_intensity / 100.0);
            // lights: nearest 16
            let mut lights = draw.lights.clone();
            lights.sort_by(|a, b| a.0.distance_squared(p.cam_pos).partial_cmp(&b.0.distance_squared(p.cam_pos)).unwrap_or(std::cmp::Ordering::Equal));
            lights.truncate(16);
            let mut lp = Vec::new();
            let mut lc = Vec::new();
            for (pos, col, lm) in &lights {
                lp.extend_from_slice(&[pos.x, pos.y, pos.z]);
                let i = lm / (4.0 * std::f32::consts::PI);
                lc.extend_from_slice(&[col[0] * i, col[1] * i, col[2] * i]);
            }
            self.scene.i1(gl, "uNumLights", lights.len() as i32);
            if !lights.is_empty() {
                self.scene.f3v(gl, "uLightPos[0]", &lp);
                self.scene.f3v(gl, "uLightCol[0]", &lc);
            }

            let mut order: Vec<usize> = (0..draw.items.len()).collect();
            let transparent = |i: &usize| draw.materials.get(draw.items[*i].material).map(|m| m.is_transparent()).unwrap_or(false);
            let (mut opaque, mut trans): (Vec<usize>, Vec<usize>) = order.drain(..).partition(|i| !transparent(i));
            trans.sort_by(|a, b| {
                let da = draw.items[*a].center.distance_squared(p.cam_pos);
                let db = draw.items[*b].center.distance_squared(p.cam_pos);
                db.partial_cmp(&da).unwrap_or(std::cmp::Ordering::Equal)
            });
            opaque.extend(trans.iter().copied());
            let mut blending = false;
            let planes = frustum_planes(p.proj * p.view);
            for idx in opaque {
                let it = &draw.items[idx];
                if !sphere_visible(&planes, it.center, it.radius) {
                    continue;
                }
                let mat = &draw.materials[it.material.min(draw.materials.len() - 1)];
                let is_t = mat.is_transparent() && p.output == 0 && vs.rendering_style != RenderingStyle::LightView;
                if is_t && !blending {
                    gl.enable(glow::BLEND);
                    gl.blend_func_separate(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA, glow::ONE, glow::ONE_MINUS_SRC_ALPHA);
                    gl.depth_mask(false);
                    blending = true;
                }
                if mat.is_transparent() && (p.output != 0 || vs.rendering_style == RenderingStyle::LightView) {
                    continue;
                }
                let tex = if mat.texture.is_empty() { None } else { self.texture(gl, &mat.texture.clone()) };
                let Some((vao, count)) = self.mesh(gl, &it.mesh) else { continue };
                let s = &self.scene;
                gl.active_texture(glow::TEXTURE0);
                gl.bind_texture(glow::TEXTURE_2D, Some(tex.unwrap_or(self.white_tex)));
                s.i1(gl, "uHasTex", tex.is_some() as i32);
                s.m4(gl, "uModel", &it.model);
                s.i1(gl, "uMatType", mat.mtype.id());
                s.f3(gl, "uAlbedo", mat.color);
                s.f3(gl, "uTint", mat.tint);
                s.f1(gl, "uFade", mat.image_fade);
                s.f2(gl, "uTexScale", mat.tex_width, mat.tex_height);
                s.f1(gl, "uTexRot", mat.tex_rotation);
                s.f1(gl, "uBump", mat.bump_amount);
                s.f1(gl, "uRough", mat.roughness);
                s.f1(gl, "uMetal", mat.metallic);
                s.f1(gl, "uSpec", mat.specular);
                s.f1(gl, "uOpacity", if mat.transparency == TransparencyType::Cutout { 1.0 } else { mat.opacity });
                s.f1(gl, "uIor", mat.refractive_index);
                s.f3(gl, "uGlassTint", mat.glass_tint);
                s.i1(gl, "uFrosted", mat.frosted as i32);
                s.f3(gl, "uEmissive", mat.emissive_color);
                s.f1(gl, "uLuminance", mat.luminance);
                s.f1(gl, "uWaveH", mat.wave_height);
                s.f1(gl, "uWaveS", mat.wave_scale);
                s.f1(gl, "uCaustics", mat.caustics);
                s.f1(gl, "uGrassH", mat.grass_height);
                s.i1(gl, "uDoubleSided", mat.double_sided as i32);
                s.f3(gl, "uIdColor", id_color(it.object_id));
                s.f3(gl, "uMatIdColor", id_color(it.material as u32 * 7919 + 13));
                gl.bind_vertex_array(Some(vao));
                gl.draw_elements(glow::TRIANGLES, count, glow::UNSIGNED_INT, 0);
            }
            gl.disable(glow::BLEND);
            gl.depth_mask(true);
            gl.bind_vertex_array(None);
            gl.bind_texture(glow::TEXTURE_2D, Some(hdr_color));
            gl.generate_mipmap(glow::TEXTURE_2D);
        }
    }

    fn post(&self, gl: &glow::Context, p: &FrameParams, flip: bool, sun_screen: Vec3) {
        let vs = &p.vs;
        let hdr = self.hdr.as_ref().unwrap();
        let s = &self.post;
        unsafe {
            gl.disable(glow::DEPTH_TEST);
            gl.disable(glow::BLEND);
            gl.disable(glow::SCISSOR_TEST);
            gl.use_program(Some(s.p));
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(hdr.color));
            gl.active_texture(glow::TEXTURE1);
            gl.bind_texture(glow::TEXTURE_2D, hdr.depth);
            s.i1(gl, "uHdr", 0);
            s.i1(gl, "uDepth", 1);
            let max_lod = (hdr.w.max(hdr.h) as f32).log2().floor();
            s.f1(gl, "uMaxLod", max_lod);
            s.i1(gl, "uAutoExposure", vs.auto_exposure as i32);
            s.f1(gl, "uExposure", 1.0 / 2500.0);
            s.f1(gl, "uBrightness", (vs.exposure_brightness - 50.0) / 12.5);
            s.f1(gl, "uHighlights", vs.highlights / 100.0);
            s.f1(gl, "uShadows", vs.shadows / 100.0);
            s.f1(gl, "uSaturation", vs.saturation / 100.0);
            let wt = kelvin_to_rgb(vs.color_temperature);
            let w0 = kelvin_to_rgb(6600.0);
            let wl = |i: usize| wt[i] / w0[i].max(1e-3);
            let norm = 0.2126 * wl(0) + 0.7152 * wl(1) + 0.0722 * wl(2);
            s.f3(gl, "uWhite", [wl(0) / norm, wl(1) / norm, wl(2) / norm]);
            s.i1(gl, "uAutoContrast", vs.auto_contrast as i32);
            let fx = p.post_effects;
            s.f1(gl, "uBloom", if fx { vs.bloom / 100.0 } else { 0.0 });
            s.f1(gl, "uVignette", if fx { vs.vignette / 100.0 } else { 0.0 });
            s.f1(gl, "uChromatic", if fx { vs.chromatic_aberration / 100.0 } else { 0.0 });
            s.f1(gl, "uFlare", if fx { vs.lens_flare / 100.0 } else { 0.0 });
            s.f1(gl, "uDof", if fx && vs.projection != Projection::Orthographic { vs.dof_strength / 100.0 } else { 0.0 });
            s.f1(gl, "uFocal", p.focal);
            let ortho = vs.projection == Projection::Orthographic;
            s.i1(gl, "uOrtho", ortho as i32);
            s.f1(gl, "uNear", if ortho { -2000.0 } else { 0.05 });
            s.f1(gl, "uFar", if ortho { 4000.0 } else { 4000.0 });
            s.f3(gl, "uSunScreen", sun_screen.into());
            s.f1(gl, "uOutline", vs.outline_thickness / 100.0 * 3.0);
            let style = if vs.rendering_style == RenderingStyle::LightView && p.output == 0 { 3 } else { 0 };
            s.i1(gl, "uStyle", style);
            s.i1(gl, "uLightAuto", vs.light_view_auto as i32);
            s.f1(gl, "uLvMin", vs.light_view_min.max(0.1));
            s.f1(gl, "uLvMax", vs.light_view_max.max(vs.light_view_min + 1.0));
            s.i1(gl, "uRawOutput", (p.output != 0) as i32);
            s.i1(gl, "uFlipY", flip as i32);
            s.i1(gl, "uKeepAlpha", p.keep_alpha as i32);
            s.f1(gl, "uAo", if vs.quality == crate::settings::Quality::Draft { 0.0 } else { 0.85 });
            s.f1(gl, "uProjY", p.proj.y_axis.y);
            gl.bind_vertex_array(Some(self.empty_vao));
            gl.draw_arrays(glow::TRIANGLES, 0, 3);
            gl.bind_vertex_array(None);
            gl.active_texture(glow::TEXTURE0);
        }
    }

    fn sun_screen(p: &FrameParams) -> Vec3 {
        if p.sun_dir.y < -0.02 {
            return Vec3::ZERO;
        }
        let clip = p.proj * p.view * Vec4::new(p.cam_pos.x + p.sun_dir.x * 1000.0, p.cam_pos.y + p.sun_dir.y * 1000.0, p.cam_pos.z + p.sun_dir.z * 1000.0, 1.0);
        if clip.w <= 0.0 {
            return Vec3::ZERO;
        }
        let ndc = clip.truncate() / clip.w;
        if ndc.x.abs() > 1.2 || ndc.y.abs() > 1.2 {
            return Vec3::ZERO;
        }
        Vec3::new(ndc.x * 0.5 + 0.5, ndc.y * 0.5 + 0.5, 1.0)
    }

    /// Is the sun's pixel open sky in the last HDR frame (for the lens flare)?
    fn sun_occluded(&self, gl: &glow::Context, ss: Vec3) -> bool {
        let Some(hdr) = self.hdr.as_ref() else { return true };
        if ss.z < 0.5 {
            return true;
        }
        let x = ((ss.x * hdr.w as f32) as i32).clamp(0, hdr.w - 1);
        let y = ((ss.y * hdr.h as f32) as i32).clamp(0, hdr.h - 1);
        let mut px = [0u8; 4];
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(hdr.fbo));
            gl.read_pixels(x, y, 1, 1, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelPackData::Slice(Some(&mut px)));
        }
        px[3] > 128
    }

    /// Draw one frame into the currently bound egui framebuffer at the given viewport.
    pub fn paint_viewport(&mut self, gl: &glow::Context, shared: &SharedRef, vp: (i32, i32, i32, i32), screen_fbo: Option<glow::Framebuffer>) {
        for m in self.meshes.values_mut() {
            m.used = false;
        }
        // exports first
        let jobs: Vec<RenderJob> = shared.lock().map(|mut s| std::mem::take(&mut s.jobs)).unwrap_or_default();
        let draw_default = shared.lock().map(|s| s.draw.clone()).unwrap_or_default();
        for job in jobs {
            let draw = job.draw.clone().unwrap_or_else(|| draw_default.clone());
            let mut images = Vec::new();
            for fp in &job.frames {
                images.push(self.render_offscreen(gl, &draw, fp, job.width as i32, job.height as i32, shared));
            }
            if let Ok(mut s) = shared.lock() {
                s.results.push(JobResult { id: job.id, images });
            }
        }
        let frame = shared.lock().ok().and_then(|s| s.frame.clone());
        if let Some(fp) = frame {
            let scale = fp.render_scale.clamp(0.25, 1.0);
            let w = ((vp.2 as f32 * scale) as i32).max(16);
            let h = ((vp.3 as f32 * scale) as i32).max(16);
            self.render_hdr(gl, &draw_default, &fp, w, h, shared);
            let mut ss = Self::sun_screen(&fp);
            if self.sun_occluded(gl, ss) {
                ss.z = 0.0;
            }
            if let Ok(mut s) = shared.lock() {
                s.sun_visible = ss.z > 0.5;
            }
            unsafe {
                gl.bind_framebuffer(glow::FRAMEBUFFER, screen_fbo);
                gl.viewport(vp.0, vp.1, vp.2, vp.3);
            }
            self.post(gl, &fp, false, ss);
        }
        // free meshes no longer used
        let stale: Vec<usize> = self.meshes.iter().filter(|(_, m)| !m.used).map(|(k, _)| *k).collect();
        if !stale.is_empty() && self.meshes.len() > 0 {
            for k in stale {
                if let Some(m) = self.meshes.remove(&k) {
                    unsafe {
                        gl.delete_vertex_array(m.vao);
                        gl.delete_buffer(m.vbo);
                        gl.delete_buffer(m.ebo);
                    }
                }
            }
        }
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, screen_fbo);
            gl.disable(glow::DEPTH_TEST);
            gl.active_texture(glow::TEXTURE0);
        }
    }

    fn render_offscreen(&mut self, gl: &glow::Context, draw: &DrawList, fp: &FrameParams, w: i32, h: i32, shared: &SharedRef) -> image::RgbaImage {
        self.render_hdr(gl, draw, fp, w, h, shared);
        let mut ss = Self::sun_screen(fp);
        if self.sun_occluded(gl, ss) {
            ss.z = 0.0;
        }
        Self::ensure_target(gl, &mut self.out, w, h, false);
        let out = self.out.as_ref().unwrap();
        let fbo = out.fbo;
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            gl.viewport(0, 0, w, h);
        }
        self.post(gl, fp, false, ss);
        let mut buf = vec![0u8; (w * h * 4) as usize];
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            gl.pixel_store_i32(glow::PACK_ALIGNMENT, 1);
            gl.read_pixels(0, 0, w, h, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelPackData::Slice(Some(&mut buf)));
        }
        // flip rows (GL origin is bottom-left)
        let row = (w * 4) as usize;
        let mut flipped = vec![0u8; buf.len()];
        for y in 0..h as usize {
            let src = &buf[y * row..(y + 1) * row];
            let dst_y = h as usize - 1 - y;
            flipped[dst_y * row..(dst_y + 1) * row].copy_from_slice(src);
        }
        image::RgbaImage::from_raw(w as u32, h as u32, flipped).unwrap_or_else(|| image::RgbaImage::new(1, 1))
    }
}

pub fn id_color(id: u32) -> [f32; 3] {
    let mut x = id.wrapping_mul(2654435761).wrapping_add(0x9e3779b9);
    x ^= x >> 15;
    x = x.wrapping_mul(2246822519);
    x ^= x >> 13;
    [((x & 255) as f32) / 255.0, (((x >> 8) & 255) as f32) / 255.0, (((x >> 16) & 255) as f32) / 255.0]
}
