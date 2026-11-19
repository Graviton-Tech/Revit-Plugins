//! Scene data: materials, meshes, objects grouped into models/assets, lights,
//! the built-in sample villa, the asset library, OBJ import and ray picking.

use glam::{Mat4, Vec2, Vec3};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;

// ------------------------------------------------------------------ materials

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MaterialType {
    Generic,
    Grass,
    Water,
    Foliage,
    Clearcoat,
    Carpet,
    SelfIlluminated,
}

impl MaterialType {
    pub const ALL: [MaterialType; 7] = [
        MaterialType::Generic,
        MaterialType::Grass,
        MaterialType::Water,
        MaterialType::Foliage,
        MaterialType::Clearcoat,
        MaterialType::Carpet,
        MaterialType::SelfIlluminated,
    ];
    pub fn name(self) -> &'static str {
        match self {
            MaterialType::Generic => "Generic",
            MaterialType::Grass => "Grass",
            MaterialType::Water => "Water",
            MaterialType::Foliage => "Foliage",
            MaterialType::Clearcoat => "Clearcoat",
            MaterialType::Carpet => "Carpet",
            MaterialType::SelfIlluminated => "Self-illuminated",
        }
    }
    pub fn id(self) -> i32 {
        MaterialType::ALL.iter().position(|m| *m == self).unwrap_or(0) as i32
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BumpType {
    Bump,
    Normal,
    Displacement,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransparencyType {
    Transmittance,
    Cutout,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Material {
    pub name: String,
    pub mtype: MaterialType,
    pub double_sided: bool,
    // albedo
    pub color: [f32; 3],
    pub texture: String,
    pub image_fade: f32, // 0..1 blends texture with color
    pub tint: [f32; 3],
    pub tex_width: f32,  // m
    pub tex_height: f32, // m
    pub tex_rotation: f32,
    // bump
    pub bump_type: BumpType,
    pub bump_amount: f32, // -1..1
    // reflections
    pub roughness: f32,
    pub metallic: f32,
    pub specular: f32,
    // transparency
    pub transparency: TransparencyType,
    pub opacity: f32,
    pub refractive_index: f32,
    pub glass_tint: [f32; 3],
    pub frosted: bool,
    // self illumination
    pub emissive_color: [f32; 3],
    pub luminance: f32, // cd/m2
    // water
    pub wave_height: f32,
    pub wave_scale: f32,
    pub caustics: f32,
    // grass / carpet
    pub grass_height: f32,
    pub grass_variation: f32,
}

impl Default for Material {
    fn default() -> Self {
        Self {
            name: "Material".into(),
            mtype: MaterialType::Generic,
            double_sided: false,
            color: [0.8, 0.8, 0.8],
            texture: String::new(),
            image_fade: 1.0,
            tint: [1.0, 1.0, 1.0],
            tex_width: 1.0,
            tex_height: 1.0,
            tex_rotation: 0.0,
            bump_type: BumpType::Bump,
            bump_amount: 0.0,
            roughness: 0.5,
            metallic: 0.0,
            specular: 0.5,
            transparency: TransparencyType::Transmittance,
            opacity: 1.0,
            refractive_index: 1.5,
            glass_tint: [1.0, 1.0, 1.0],
            frosted: false,
            emissive_color: [1.0, 0.9, 0.75],
            luminance: 0.0,
            wave_height: 0.5,
            wave_scale: 0.5,
            caustics: 0.5,
            grass_height: 0.1,
            grass_variation: 0.5,
        }
    }
}

impl Material {
    pub fn new(name: &str, color: [f32; 3], roughness: f32) -> Self {
        Self { name: name.into(), color, roughness, ..Default::default() }
    }
    pub fn is_transparent(&self) -> bool {
        (self.opacity < 0.999 && self.transparency == TransparencyType::Transmittance) || self.mtype == MaterialType::Water
    }
}

// ------------------------------------------------------------------ meshes

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
}

#[derive(Clone, Debug, Default)]
pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

impl Mesh {
    pub fn append(&mut self, other: &Mesh) {
        let base = self.vertices.len() as u32;
        self.vertices.extend_from_slice(&other.vertices);
        self.indices.extend(other.indices.iter().map(|i| i + base));
    }

    fn quad(&mut self, p: [Vec3; 4], n: Vec3, uv: [Vec2; 4]) {
        let base = self.vertices.len() as u32;
        for i in 0..4 {
            self.vertices.push(Vertex { pos: p[i].into(), normal: n.into(), uv: uv[i].into() });
        }
        self.indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    /// Unit ground quad, facing up, with UVs equal to local x/z.
    pub fn ground_quad() -> Mesh {
        let mut m = Mesh::default();
        let p = [Vec3::new(-0.5, 0.0, 0.5), Vec3::new(0.5, 0.0, 0.5), Vec3::new(0.5, 0.0, -0.5), Vec3::new(-0.5, 0.0, -0.5)];
        let uv = [Vec2::new(0.0, 0.0), Vec2::new(1.0, 0.0), Vec2::new(1.0, 1.0), Vec2::new(0.0, 1.0)];
        m.quad(p, Vec3::Y, uv);
        m
    }

    /// Axis-aligned box with world-space (metre) UVs.
    pub fn cuboid(min: Vec3, max: Vec3) -> Mesh {
        let mut m = Mesh::default();
        let (a, b) = (min, max);
        let uv = |u0: f32, v0: f32, u1: f32, v1: f32| [Vec2::new(u0, v0), Vec2::new(u1, v0), Vec2::new(u1, v1), Vec2::new(u0, v1)];
        // +Z
        m.quad([Vec3::new(a.x, a.y, b.z), Vec3::new(b.x, a.y, b.z), Vec3::new(b.x, b.y, b.z), Vec3::new(a.x, b.y, b.z)], Vec3::Z, uv(a.x, a.y, b.x, b.y));
        // -Z
        m.quad([Vec3::new(b.x, a.y, a.z), Vec3::new(a.x, a.y, a.z), Vec3::new(a.x, b.y, a.z), Vec3::new(b.x, b.y, a.z)], -Vec3::Z, uv(-b.x, a.y, -a.x, b.y));
        // +X
        m.quad([Vec3::new(b.x, a.y, b.z), Vec3::new(b.x, a.y, a.z), Vec3::new(b.x, b.y, a.z), Vec3::new(b.x, b.y, b.z)], Vec3::X, uv(-b.z, a.y, -a.z, b.y));
        // -X
        m.quad([Vec3::new(a.x, a.y, a.z), Vec3::new(a.x, a.y, b.z), Vec3::new(a.x, b.y, b.z), Vec3::new(a.x, b.y, a.z)], -Vec3::X, uv(a.z, a.y, b.z, b.y));
        // +Y
        m.quad([Vec3::new(a.x, b.y, b.z), Vec3::new(b.x, b.y, b.z), Vec3::new(b.x, b.y, a.z), Vec3::new(a.x, b.y, a.z)], Vec3::Y, uv(a.x, -b.z, b.x, -a.z));
        // -Y
        m.quad([Vec3::new(a.x, a.y, a.z), Vec3::new(b.x, a.y, a.z), Vec3::new(b.x, a.y, b.z), Vec3::new(a.x, a.y, b.z)], -Vec3::Y, uv(a.x, a.z, b.x, b.z));
        m
    }

    pub fn cuboid_c(center: Vec3, size: Vec3) -> Mesh {
        Mesh::cuboid(center - size * 0.5, center + size * 0.5)
    }

    pub fn plane(half: f32, y: f32) -> Mesh {
        let mut m = Mesh::default();
        m.quad(
            [Vec3::new(-half, y, half), Vec3::new(half, y, half), Vec3::new(half, y, -half), Vec3::new(-half, y, -half)],
            Vec3::Y,
            [Vec2::new(-half, -half), Vec2::new(half, -half), Vec2::new(half, half), Vec2::new(-half, half)],
        );
        m
    }

    pub fn cylinder(base: Vec3, radius: f32, height: f32, segments: u32) -> Mesh {
        let mut m = Mesh::default();
        let seg = segments.max(3);
        for i in 0..seg {
            let a0 = i as f32 / seg as f32 * std::f32::consts::TAU;
            let a1 = (i + 1) as f32 / seg as f32 * std::f32::consts::TAU;
            let d0 = Vec3::new(a0.cos(), 0.0, a0.sin());
            let d1 = Vec3::new(a1.cos(), 0.0, a1.sin());
            let p0 = base + d0 * radius;
            let p1 = base + d1 * radius;
            let up = Vec3::Y * height;
            let base_i = m.vertices.len() as u32;
            let u0 = a0 * radius;
            let u1 = a1 * radius;
            m.vertices.push(Vertex { pos: p1.into(), normal: d1.into(), uv: [u1, 0.0] });
            m.vertices.push(Vertex { pos: p0.into(), normal: d0.into(), uv: [u0, 0.0] });
            m.vertices.push(Vertex { pos: (p0 + up).into(), normal: d0.into(), uv: [u0, height] });
            m.vertices.push(Vertex { pos: (p1 + up).into(), normal: d1.into(), uv: [u1, height] });
            m.indices.extend_from_slice(&[base_i, base_i + 1, base_i + 2, base_i, base_i + 2, base_i + 3]);
            // caps
            let c = m.vertices.len() as u32;
            let top = base + up;
            m.vertices.push(Vertex { pos: top.into(), normal: [0.0, 1.0, 0.0], uv: [0.0, 0.0] });
            m.vertices.push(Vertex { pos: (p1 + up).into(), normal: [0.0, 1.0, 0.0], uv: [d1.x * radius, d1.z * radius] });
            m.vertices.push(Vertex { pos: (p0 + up).into(), normal: [0.0, 1.0, 0.0], uv: [d0.x * radius, d0.z * radius] });
            m.indices.extend_from_slice(&[c, c + 1, c + 2]);
            let c = m.vertices.len() as u32;
            m.vertices.push(Vertex { pos: base.into(), normal: [0.0, -1.0, 0.0], uv: [0.0, 0.0] });
            m.vertices.push(Vertex { pos: p0.into(), normal: [0.0, -1.0, 0.0], uv: [d0.x * radius, d0.z * radius] });
            m.vertices.push(Vertex { pos: p1.into(), normal: [0.0, -1.0, 0.0], uv: [d1.x * radius, d1.z * radius] });
            m.indices.extend_from_slice(&[c, c + 1, c + 2]);
        }
        m
    }

    /// Ellipsoid (sphere scaled by `radii`), optionally made lumpy for foliage.
    pub fn blob(center: Vec3, radii: Vec3, rings: u32, lump: f32, seed: f32) -> Mesh {
        let mut m = Mesh::default();
        let segs = rings * 2;
        for r in 0..=rings {
            let v = r as f32 / rings as f32;
            let theta = v * std::f32::consts::PI;
            for s in 0..=segs {
                let u = s as f32 / segs as f32;
                let phi = u * std::f32::consts::TAU;
                let d = Vec3::new(theta.sin() * phi.cos(), theta.cos(), theta.sin() * phi.sin());
                let noise = if lump > 0.0 {
                    let k = (d.x * 3.1 + seed).sin() * (d.y * 2.7 + seed * 1.3).cos() * (d.z * 3.7 - seed).sin();
                    1.0 + lump * k
                } else {
                    1.0
                };
                let p = center + d * radii * noise;
                let n = (d / radii.max(Vec3::splat(1e-3))).normalize();
                m.vertices.push(Vertex { pos: p.into(), normal: n.into(), uv: [u * radii.x * 6.0, v * radii.y * 3.0] });
            }
        }
        let row = segs + 1;
        for r in 0..rings {
            for s in 0..segs {
                let a = r * row + s;
                let b = a + row;
                m.indices.extend_from_slice(&[a, a + 1, b + 1, a, b + 1, b]);
            }
        }
        m
    }

    pub fn cone(base: Vec3, radius: f32, height: f32, segments: u32) -> Mesh {
        let mut m = Mesh::default();
        let tip = base + Vec3::Y * height;
        let slope = radius / height;
        for i in 0..segments {
            let a0 = i as f32 / segments as f32 * std::f32::consts::TAU;
            let a1 = (i + 1) as f32 / segments as f32 * std::f32::consts::TAU;
            let d0 = Vec3::new(a0.cos(), 0.0, a0.sin());
            let d1 = Vec3::new(a1.cos(), 0.0, a1.sin());
            let n0 = (d0 + Vec3::Y * slope).normalize();
            let n1 = (d1 + Vec3::Y * slope).normalize();
            let b = m.vertices.len() as u32;
            m.vertices.push(Vertex { pos: (base + d1 * radius).into(), normal: n1.into(), uv: [a1, 0.0] });
            m.vertices.push(Vertex { pos: (base + d0 * radius).into(), normal: n0.into(), uv: [a0, 0.0] });
            m.vertices.push(Vertex { pos: tip.into(), normal: ((n0 + n1) * 0.5).normalize().into(), uv: [a0, height] });
            m.indices.extend_from_slice(&[b, b + 1, b + 2]);
        }
        m
    }

    pub fn bounds(&self) -> (Vec3, Vec3) {
        let mut mn = Vec3::splat(f32::MAX);
        let mut mx = Vec3::splat(f32::MIN);
        for v in &self.vertices {
            let p = Vec3::from(v.pos);
            mn = mn.min(p);
            mx = mx.max(p);
        }
        (mn, mx)
    }
}

// ------------------------------------------------------------------ objects

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GroupKind {
    Building,
    Asset,
    Imported,
}

#[derive(Clone, Debug)]
pub struct Group {
    pub id: u32,
    pub name: String,
    pub kind: GroupKind,
    pub asset: Option<String>,
    pub source: Option<String>,
    pub offset: Vec3,
    pub rot_y: f32, // degrees
    pub scale: f32,
}

impl Group {
    pub fn matrix(&self) -> Mat4 {
        Mat4::from_translation(self.offset) * Mat4::from_rotation_y(self.rot_y.to_radians()) * Mat4::from_scale(Vec3::splat(self.scale))
    }
}

#[derive(Clone, Debug)]
pub struct Object {
    pub name: String,
    pub group: u32,
    pub material: usize,
    pub mesh: Arc<Mesh>,
    pub local_bounds: (Vec3, Vec3),
    /// Placement inside its group (identity except for instanced family geometry).
    pub transform: Mat4,
}

#[derive(Clone, Debug)]
pub struct Light {
    pub group: u32,
    pub local_pos: Vec3,
    pub color: [f32; 3],
    pub lumen: f32,
}

#[derive(Clone, Debug, Default)]
pub struct Scene {
    pub materials: Vec<Material>,
    pub groups: Vec<Group>,
    pub objects: Vec<Object>,
    pub lights: Vec<Light>,
    pub next_group: u32,
    /// Bumped whenever geometry changes so the renderer re-uploads buffers.
    pub geometry_rev: u64,
}

pub struct Hit {
    pub t: f32,
    pub point: Vec3,
    pub normal: Vec3,
    pub object: usize,
}

impl Scene {
    pub fn material(&mut self, name: &str, make: impl FnOnce() -> Material) -> usize {
        if let Some(i) = self.materials.iter().position(|m| m.name == name) {
            return i;
        }
        let mut m = make();
        m.name = name.to_string();
        self.materials.push(m);
        self.materials.len() - 1
    }

    pub fn add_group(&mut self, name: &str, kind: GroupKind) -> u32 {
        let id = self.next_group;
        self.next_group += 1;
        self.groups.push(Group { id, name: name.into(), kind, asset: None, source: None, offset: Vec3::ZERO, rot_y: 0.0, scale: 1.0 });
        id
    }

    pub fn group(&self, id: u32) -> Option<&Group> {
        self.groups.iter().find(|g| g.id == id)
    }
    pub fn group_mut(&mut self, id: u32) -> Option<&mut Group> {
        self.groups.iter_mut().find(|g| g.id == id)
    }

    pub fn add_object(&mut self, group: u32, name: &str, material: usize, mesh: Mesh) {
        let local_bounds = mesh.bounds();
        self.objects.push(Object { name: name.into(), group, material, mesh: Arc::new(mesh), local_bounds, transform: Mat4::IDENTITY });
        self.geometry_rev += 1;
    }

    /// Add one more placement of a mesh that is already uploaded (instancing): the GPU keeps one copy.
    pub fn add_instance(&mut self, group: u32, name: &str, material: usize, mesh: Arc<Mesh>, local_bounds: (Vec3, Vec3), transform: Mat4) {
        self.objects.push(Object { name: name.into(), group, material, mesh, local_bounds, transform });
        self.geometry_rev += 1;
    }

    pub fn remove_group(&mut self, id: u32) {
        self.objects.retain(|o| o.group != id);
        self.lights.retain(|l| l.group != id);
        self.groups.retain(|g| g.id != id);
        self.geometry_rev += 1;
    }

    pub fn object_matrix(&self, o: &Object) -> Mat4 {
        self.group(o.group).map(|g| g.matrix()).unwrap_or(Mat4::IDENTITY) * o.transform
    }

    pub fn world_lights(&self) -> Vec<(Vec3, [f32; 3], f32)> {
        self.lights
            .iter()
            .filter_map(|l| self.group(l.group).map(|g| (g.matrix().transform_point3(l.local_pos), l.color, l.lumen)))
            .collect()
    }

    pub fn world_bounds(&self) -> (Vec3, Vec3) {
        let mut mn = Vec3::splat(f32::MAX);
        let mut mx = Vec3::splat(f32::MIN);
        for o in &self.objects {
            if o.name == "Ground" {
                continue;
            }
            let m = self.object_matrix(o);
            let (a, b) = o.local_bounds;
            for i in 0..8 {
                let c = Vec3::new(if i & 1 == 0 { a.x } else { b.x }, if i & 2 == 0 { a.y } else { b.y }, if i & 4 == 0 { a.z } else { b.z });
                let p = m.transform_point3(c);
                mn = mn.min(p);
                mx = mx.max(p);
            }
        }
        if mn.x > mx.x {
            return (Vec3::splat(-10.0), Vec3::splat(10.0));
        }
        (mn, mx)
    }

    /// Nearest ray hit against every triangle (with a bounding-box test first).
    pub fn raycast(&self, origin: Vec3, dir: Vec3, max_t: f32, skip_transparent: bool) -> Option<Hit> {
        let mut best: Option<Hit> = None;
        let mut best_t = max_t;
        for (oi, o) in self.objects.iter().enumerate() {
            if skip_transparent && self.materials.get(o.material).map(|m| m.is_transparent()).unwrap_or(false) {
                continue;
            }
            let m = self.object_matrix(o);
            let inv = m.inverse();
            let lo = inv.transform_point3(origin);
            let ld = inv.transform_vector3(dir);
            if !ray_aabb(lo, ld, o.local_bounds.0, o.local_bounds.1) {
                continue;
            }
            let v = &o.mesh.vertices;
            for tri in o.mesh.indices.chunks_exact(3) {
                let a = Vec3::from(v[tri[0] as usize].pos);
                let b = Vec3::from(v[tri[1] as usize].pos);
                let c = Vec3::from(v[tri[2] as usize].pos);
                if let Some(tl) = ray_tri(lo, ld, a, b, c) {
                    let p = m.transform_point3(lo + ld * tl);
                    let t = (p - origin).dot(dir);
                    if t > 1e-4 && t < best_t {
                        best_t = t;
                        let n = m.transform_vector3((b - a).cross(c - a)).normalize_or_zero();
                        let n = if n.dot(dir) > 0.0 { -n } else { n };
                        best = Some(Hit { t, point: p, normal: n, object: oi });
                    }
                }
            }
        }
        best
    }
}

fn ray_aabb(o: Vec3, d: Vec3, mn: Vec3, mx: Vec3) -> bool {
    let inv = Vec3::ONE / d;
    let t0 = (mn - Vec3::splat(0.01) - o) * inv;
    let t1 = (mx + Vec3::splat(0.01) - o) * inv;
    let tmin = t0.min(t1).max_element();
    let tmax = t0.max(t1).min_element();
    tmax >= tmin.max(0.0)
}

fn ray_tri(o: Vec3, d: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Option<f32> {
    let e1 = b - a;
    let e2 = c - a;
    let p = d.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-9 {
        return None;
    }
    let inv = 1.0 / det;
    let s = o - a;
    let u = s.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = d.dot(q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = e2.dot(q) * inv;
    if t > 0.0 {
        Some(t)
    } else {
        None
    }
}

// ------------------------------------------------------------------ sample scene

fn mats(scene: &mut Scene) -> std::collections::HashMap<&'static str, usize> {
    let mut h = std::collections::HashMap::new();
    let defs: Vec<(&'static str, Material)> = vec![
        ("Lawn", Material { mtype: MaterialType::Grass, color: [0.20, 0.36, 0.10], roughness: 0.9, specular: 0.2, grass_height: 0.12, ..Default::default() }),
        ("Concrete", Material { color: [0.62, 0.61, 0.58], roughness: 0.85, bump_amount: 0.25, ..Default::default() }),
        ("White Plaster", Material { color: [0.90, 0.89, 0.86], roughness: 0.8, bump_amount: 0.1, ..Default::default() }),
        ("Dark Metal", Material { color: [0.10, 0.10, 0.11], roughness: 0.35, metallic: 1.0, ..Default::default() }),
        ("Glass", Material { color: [0.9, 0.95, 0.95], roughness: 0.02, opacity: 0.18, refractive_index: 1.52, glass_tint: [0.85, 0.95, 0.95], specular: 1.0, double_sided: true, ..Default::default() }),
        ("Larch Cladding", Material { color: [0.55, 0.36, 0.20], roughness: 0.6, bump_amount: 0.35, ..Default::default() }),
        ("Oak Floor", Material { mtype: MaterialType::Clearcoat, color: [0.58, 0.40, 0.24], roughness: 0.3, ..Default::default() }),
        ("Pool Water", Material { mtype: MaterialType::Water, color: [0.10, 0.45, 0.55], roughness: 0.02, opacity: 0.6, wave_height: 0.4, wave_scale: 0.5, caustics: 0.6, double_sided: true, ..Default::default() }),
        ("Pool Tile", Material { color: [0.55, 0.80, 0.85], roughness: 0.25, ..Default::default() }),
        ("Bark", Material { color: [0.28, 0.20, 0.14], roughness: 0.9, bump_amount: 0.6, ..Default::default() }),
        ("Leaves", Material { mtype: MaterialType::Foliage, color: [0.22, 0.42, 0.12], roughness: 0.7, double_sided: true, ..Default::default() }),
        ("Hedge", Material { mtype: MaterialType::Foliage, color: [0.14, 0.30, 0.09], roughness: 0.8, ..Default::default() }),
        ("Fabric Grey", Material { mtype: MaterialType::Carpet, color: [0.35, 0.36, 0.38], roughness: 0.95, ..Default::default() }),
        ("Rug", Material { mtype: MaterialType::Carpet, color: [0.65, 0.55, 0.42], roughness: 1.0, grass_height: 0.02, ..Default::default() }),
        ("Stone Path", Material { color: [0.48, 0.46, 0.43], roughness: 0.7, bump_amount: 0.3, ..Default::default() }),
        ("Kitchen White", Material { mtype: MaterialType::Clearcoat, color: [0.92, 0.92, 0.91], roughness: 0.15, ..Default::default() }),
        ("Ceiling Light", Material { mtype: MaterialType::SelfIlluminated, color: [1.0, 0.95, 0.85], emissive_color: [1.0, 0.92, 0.8], luminance: 3000.0, ..Default::default() }),
    ];
    for (n, m) in defs {
        let i = scene.material(n, || m);
        h.insert(n, i);
    }
    h
}

pub fn sample_scene() -> Scene {
    let mut s = Scene::default();
    let m = mats(&mut s);
    let site = s.add_group("Site", GroupKind::Building);
    s.add_object(site, "Ground", m["Lawn"], Mesh::plane(300.0, 0.0));

    // terrace around a pool (pool spans x 2..10, z 6..9)
    let deck_h = 0.3;
    let mut terrace = Mesh::default();
    terrace.append(&Mesh::cuboid(Vec3::new(-9.0, 0.0, -6.0), Vec3::new(12.0, deck_h, 6.0)));
    terrace.append(&Mesh::cuboid(Vec3::new(-9.0, 0.0, 6.0), Vec3::new(2.0, deck_h, 11.0)));
    terrace.append(&Mesh::cuboid(Vec3::new(10.0, 0.0, 6.0), Vec3::new(12.0, deck_h, 11.0)));
    terrace.append(&Mesh::cuboid(Vec3::new(2.0, 0.0, 9.0), Vec3::new(10.0, deck_h, 11.0)));
    s.add_object(site, "Terrace", m["Concrete"], terrace);
    s.add_object(site, "Pool Basin", m["Pool Tile"], Mesh::cuboid(Vec3::new(2.0, -1.4, 6.0), Vec3::new(10.0, -1.3, 9.0)));
    let mut basin_walls = Mesh::default();
    basin_walls.append(&Mesh::cuboid(Vec3::new(1.9, -1.4, 6.0), Vec3::new(2.0, deck_h, 9.0)));
    basin_walls.append(&Mesh::cuboid(Vec3::new(10.0, -1.4, 6.0), Vec3::new(10.1, deck_h, 9.0)));
    basin_walls.append(&Mesh::cuboid(Vec3::new(2.0, -1.4, 5.9), Vec3::new(10.0, deck_h, 6.0)));
    basin_walls.append(&Mesh::cuboid(Vec3::new(2.0, -1.4, 9.0), Vec3::new(10.0, deck_h, 9.1)));
    s.add_object(site, "Pool Walls", m["Pool Tile"], basin_walls);
    s.add_object(site, "Pool Water", m["Pool Water"], Mesh::cuboid(Vec3::new(2.0, 0.05, 6.0), Vec3::new(10.0, 0.18, 9.0)));

    // stone path to the street
    let mut path = Mesh::default();
    for i in 0..9 {
        let z = 12.0 + i as f32 * 1.3;
        path.append(&Mesh::cuboid(Vec3::new(-6.5, 0.0, z), Vec3::new(-4.7, 0.06, z + 0.9)));
    }
    s.add_object(site, "Stepping Stones", m["Stone Path"], path);

    // ---- house
    let house = s.add_group("Villa", GroupKind::Building);
    let f0 = deck_h;
    let h1 = 3.1;
    let t = 0.25;
    // ground floor footprint x -8..4, z -5..3
    let (x0, x1, z0, z1) = (-8.0, 4.0, -5.0, 3.0);
    let mut walls = Mesh::default();
    walls.append(&Mesh::cuboid(Vec3::new(x0, f0, z0), Vec3::new(x1, f0 + h1, z0 + t))); // back
    walls.append(&Mesh::cuboid(Vec3::new(x0, f0, z0), Vec3::new(x0 + t, f0 + h1, z1))); // west
    walls.append(&Mesh::cuboid(Vec3::new(x1 - t, f0, z0), Vec3::new(x1, f0 + h1, -1.0))); // east solid part
    walls.append(&Mesh::cuboid(Vec3::new(x1 - t, f0 + 2.3, -1.0), Vec3::new(x1, f0 + h1, z1))); // east lintel
    walls.append(&Mesh::cuboid(Vec3::new(x0, f0, z1 - t), Vec3::new(-5.5, f0 + h1, z1))); // front left pier
    walls.append(&Mesh::cuboid(Vec3::new(-3.5, f0 + 2.6, z1 - t), Vec3::new(x1, f0 + h1, z1))); // front lintel
    walls.append(&Mesh::cuboid(Vec3::new(-5.5, f0 + 2.3, z1 - t), Vec3::new(-3.5, f0 + h1, z1))); // door lintel
    s.add_object(house, "Ground Floor Walls", m["White Plaster"], walls);
    s.add_object(house, "Floor Slab", m["Oak Floor"], Mesh::cuboid(Vec3::new(x0 + t, f0, z0 + t), Vec3::new(x1 - t, f0 + 0.02, z1 - t)));
    // glazing front + east
    let mut glass = Mesh::default();
    glass.append(&Mesh::cuboid(Vec3::new(-3.5, f0, z1 - 0.14), Vec3::new(x1 - t, f0 + 2.6, z1 - 0.12)));
    glass.append(&Mesh::cuboid(Vec3::new(x1 - 0.14, f0, -1.0), Vec3::new(x1 - 0.12, f0 + 2.3, z1 - t)));
    s.add_object(house, "Glazing", m["Glass"], glass);
    let mut frames = Mesh::default();
    for i in 0..5 {
        let x = -3.5 + i as f32 * ((x1 - t + 3.5) / 4.0);
        frames.append(&Mesh::cuboid(Vec3::new(x - 0.04, f0, z1 - 0.18), Vec3::new(x + 0.04, f0 + 2.6, z1 - 0.08)));
    }
    frames.append(&Mesh::cuboid(Vec3::new(-3.5, f0, z1 - 0.18), Vec3::new(x1 - t, f0 + 0.06, z1 - 0.08)));
    for i in 0..3 {
        let z = -1.0 + i as f32 * ((z1 - t + 1.0) / 2.0);
        frames.append(&Mesh::cuboid(Vec3::new(x1 - 0.18, f0, z - 0.04), Vec3::new(x1 - 0.08, f0 + 2.3, z + 0.04)));
    }
    s.add_object(house, "Window Frames", m["Dark Metal"], frames);
    // upper floor: cantilevered timber box x -9..2, z -5.5..2
    let u0 = f0 + h1;
    let h2 = 3.0;
    s.add_object(house, "First Floor Slab", m["White Plaster"], Mesh::cuboid(Vec3::new(-9.0, u0, -5.5), Vec3::new(5.0, u0 + 0.35, 3.5)));
    let mut upper = Mesh::default();
    let (ux0, ux1, uz0, uz1) = (-9.0, 2.0, -5.5, 2.5);
    upper.append(&Mesh::cuboid(Vec3::new(ux0, u0 + 0.35, uz0), Vec3::new(ux1, u0 + h2, uz0 + t)));
    upper.append(&Mesh::cuboid(Vec3::new(ux0, u0 + 0.35, uz0), Vec3::new(ux0 + t, u0 + h2, uz1)));
    upper.append(&Mesh::cuboid(Vec3::new(ux1 - t, u0 + 0.35, uz0), Vec3::new(ux1, u0 + h2, uz1)));
    upper.append(&Mesh::cuboid(Vec3::new(ux0, u0 + 0.35, uz1 - t), Vec3::new(-6.0, u0 + h2, uz1)));
    upper.append(&Mesh::cuboid(Vec3::new(-1.0, u0 + 0.35, uz1 - t), Vec3::new(ux1, u0 + h2, uz1)));
    upper.append(&Mesh::cuboid(Vec3::new(-6.0, u0 + 0.35, uz1 - t), Vec3::new(-1.0, u0 + 0.9, uz1)));
    upper.append(&Mesh::cuboid(Vec3::new(-6.0, u0 + 2.5, uz1 - t), Vec3::new(-1.0, u0 + h2, uz1)));
    s.add_object(house, "Timber Volume", m["Larch Cladding"], upper);
    s.add_object(house, "Upper Glazing", m["Glass"], Mesh::cuboid(Vec3::new(-6.0, u0 + 0.9, uz1 - 0.14), Vec3::new(-1.0, u0 + 2.5, uz1 - 0.12)));
    s.add_object(house, "Roof", m["Dark Metal"], Mesh::cuboid(Vec3::new(-9.3, u0 + h2, -5.8), Vec3::new(2.3, u0 + h2 + 0.25, 2.8)));
    s.add_object(house, "Canopy Column", m["Dark Metal"], Mesh::cuboid(Vec3::new(4.6, f0, 3.1), Vec3::new(4.8, u0, 3.3)));

    // interior furniture
    let int = s.add_group("Interior", GroupKind::Building);
    let mut sofa = Mesh::default();
    sofa.append(&Mesh::cuboid(Vec3::new(-6.0, f0, -1.5), Vec3::new(-3.6, f0 + 0.42, -0.6)));
    sofa.append(&Mesh::cuboid(Vec3::new(-6.0, f0, -1.5), Vec3::new(-3.6, f0 + 0.8, -1.25)));
    s.add_object(int, "Sofa", m["Fabric Grey"], sofa);
    s.add_object(int, "Rug", m["Rug"], Mesh::cuboid(Vec3::new(-6.4, f0 + 0.02, -0.6), Vec3::new(-3.2, f0 + 0.035, 1.6)));
    s.add_object(int, "Kitchen Island", m["Kitchen White"], Mesh::cuboid(Vec3::new(-1.0, f0, -3.8), Vec3::new(2.5, f0 + 0.92, -2.9)));
    s.add_object(int, "Ceiling Panel", m["Ceiling Light"], Mesh::cuboid(Vec3::new(-5.2, u0 - 0.04, -0.6), Vec3::new(-4.4, u0 - 0.01, 0.2)));
    s.lights.push(Light { group: int, local_pos: Vec3::new(-4.8, u0 - 0.25, -0.2), color: [1.0, 0.85, 0.65], lumen: 1600.0 });
    s.lights.push(Light { group: int, local_pos: Vec3::new(0.7, u0 - 0.3, -3.3), color: [1.0, 0.9, 0.75], lumen: 1200.0 });

    // vegetation along the plot
    let veg = s.add_group("Planting", GroupKind::Building);
    let mut hedge = Mesh::default();
    hedge.append(&Mesh::cuboid(Vec3::new(-14.0, 0.0, -10.0), Vec3::new(16.0, 1.6, -9.2)));
    hedge.append(&Mesh::cuboid(Vec3::new(15.2, 0.0, -10.0), Vec3::new(16.0, 1.6, 12.0)));
    s.add_object(veg, "Hedges", m["Hedge"], hedge);
    for (i, (x, z, sz)) in [(-13.0, 6.0, 1.2f32), (14.0, 2.0, 1.0), (-12.0, -6.0, 1.4), (8.0, -8.0, 0.9), (-1.0, 14.0, 1.1)].iter().enumerate() {
        let (trunk, crown) = tree_meshes(Vec3::new(*x, 0.0, *z), *sz, i as f32 * 1.7);
        s.add_object(veg, "Tree Trunk", m["Bark"], trunk);
        s.add_object(veg, "Tree Crown", m["Leaves"], crown);
    }
    s
}

fn tree_meshes(base: Vec3, size: f32, seed: f32) -> (Mesh, Mesh) {
    let trunk = Mesh::cylinder(base, 0.15 * size, 3.0 * size, 10);
    let mut crown = Mesh::default();
    crown.append(&Mesh::blob(base + Vec3::new(0.0, 4.0 * size, 0.0), Vec3::new(2.2, 2.0, 2.2) * size, 10, 0.15, seed));
    crown.append(&Mesh::blob(base + Vec3::new(0.9 * size, 3.2 * size, 0.4 * size), Vec3::splat(1.4 * size), 8, 0.2, seed + 3.0));
    crown.append(&Mesh::blob(base + Vec3::new(-0.8 * size, 3.4 * size, -0.6 * size), Vec3::splat(1.3 * size), 8, 0.2, seed + 5.0));
    (trunk, crown)
}

// ------------------------------------------------------------------ asset library

pub struct AssetDef {
    pub name: &'static str,
    pub category: &'static str,
}

pub const ASSETS: &[AssetDef] = &[
    AssetDef { name: "Broadleaf Tree", category: "Vegetation" },
    AssetDef { name: "Conifer", category: "Vegetation" },
    AssetDef { name: "Shrub", category: "Vegetation" },
    AssetDef { name: "Planter", category: "Vegetation" },
    AssetDef { name: "Lounge Chair", category: "Furniture" },
    AssetDef { name: "Dining Table", category: "Furniture" },
    AssetDef { name: "Bench", category: "Furniture" },
    AssetDef { name: "Parasol", category: "Furniture" },
    AssetDef { name: "Standing Person", category: "People" },
    AssetDef { name: "Sitting Person", category: "People" },
    AssetDef { name: "Hatchback Car", category: "Vehicles" },
    AssetDef { name: "Bicycle", category: "Vehicles" },
    AssetDef { name: "Floor Lamp", category: "Lights" },
    AssetDef { name: "Garden Bollard", category: "Lights" },
    AssetDef { name: "Pendant Light", category: "Lights" },
];

pub fn asset_categories() -> Vec<&'static str> {
    let mut c: Vec<&'static str> = Vec::new();
    for a in ASSETS {
        if !c.contains(&a.category) {
            c.push(a.category);
        }
    }
    c
}

/// Place an asset at `pos`; returns its group id.
pub fn place_asset(scene: &mut Scene, name: &str, pos: Vec3, rot_y: f32) -> u32 {
    let g = scene.add_group(name, GroupKind::Asset);
    if let Some(gr) = scene.group_mut(g) {
        gr.asset = Some(name.into());
        gr.offset = pos;
        gr.rot_y = rot_y;
    }
    let mat = |s: &mut Scene, n: &str, m: Material| s.material(n, || m);
    let bark = mat(scene, "Bark", Material { color: [0.28, 0.20, 0.14], roughness: 0.9, bump_amount: 0.6, ..Default::default() });
    let leaves = mat(scene, "Leaves", Material { mtype: MaterialType::Foliage, color: [0.22, 0.42, 0.12], roughness: 0.7, double_sided: true, ..Default::default() });
    let metal = mat(scene, "Dark Metal", Material { color: [0.10, 0.10, 0.11], roughness: 0.35, metallic: 1.0, ..Default::default() });
    let fabric = mat(scene, "Fabric Grey", Material { mtype: MaterialType::Carpet, color: [0.35, 0.36, 0.38], roughness: 0.95, ..Default::default() });
    let wood = mat(scene, "Teak", Material { color: [0.45, 0.30, 0.18], roughness: 0.55, ..Default::default() });
    let person = mat(scene, "Person Abstract", Material { color: [0.85, 0.85, 0.85], roughness: 0.6, ..Default::default() });
    let lamp = mat(scene, "Lamp Glow", Material { mtype: MaterialType::SelfIlluminated, emissive_color: [1.0, 0.85, 0.6], luminance: 5000.0, ..Default::default() });
    let Z = Vec3::ZERO;
    match name {
        "Broadleaf Tree" => {
            let (t, c) = tree_meshes(Z, 1.0, pos.x + pos.z);
            scene.add_object(g, "Trunk", bark, t);
            scene.add_object(g, "Crown", leaves, c);
        }
        "Conifer" => {
            scene.add_object(g, "Trunk", bark, Mesh::cylinder(Z, 0.18, 2.0, 8));
            let mut c = Mesh::cone(Vec3::new(0.0, 1.2, 0.0), 1.8, 3.5, 14);
            c.append(&Mesh::cone(Vec3::new(0.0, 3.2, 0.0), 1.3, 3.0, 14));
            c.append(&Mesh::cone(Vec3::new(0.0, 5.0, 0.0), 0.8, 2.4, 14));
            scene.add_object(g, "Needles", leaves, c);
        }
        "Shrub" => {
            let mut c = Mesh::blob(Vec3::new(0.0, 0.5, 0.0), Vec3::new(0.8, 0.6, 0.8), 8, 0.25, pos.x);
            c.append(&Mesh::blob(Vec3::new(0.5, 0.4, 0.3), Vec3::splat(0.5), 6, 0.25, pos.z));
            scene.add_object(g, "Shrub", leaves, c);
        }
        "Planter" => {
            let conc = mat(scene, "Concrete", Material { color: [0.62, 0.61, 0.58], roughness: 0.85, bump_amount: 0.25, ..Default::default() });
            scene.add_object(g, "Pot", conc, Mesh::cuboid(Vec3::new(-0.4, 0.0, -0.4), Vec3::new(0.4, 0.6, 0.4)));
            scene.add_object(g, "Plant", leaves, Mesh::blob(Vec3::new(0.0, 0.9, 0.0), Vec3::new(0.45, 0.5, 0.45), 8, 0.2, pos.x));
        }
        "Lounge Chair" => {
            let mut m = Mesh::cuboid(Vec3::new(-0.35, 0.25, -0.9), Vec3::new(0.35, 0.35, 0.6));
            m.append(&Mesh::cuboid(Vec3::new(-0.35, 0.35, -0.95), Vec3::new(0.35, 0.95, -0.8)));
            scene.add_object(g, "Cushion", fabric, m);
            let mut f = Mesh::default();
            for (x, z) in [(-0.3, -0.85), (0.3, -0.85), (-0.3, 0.55), (0.3, 0.55)] {
                f.append(&Mesh::cuboid(Vec3::new(x - 0.03, 0.0, z - 0.03), Vec3::new(x + 0.03, 0.25, z + 0.03)));
            }
            scene.add_object(g, "Frame", metal, f);
        }
        "Dining Table" => {
            scene.add_object(g, "Top", wood, Mesh::cuboid(Vec3::new(-1.0, 0.72, -0.45), Vec3::new(1.0, 0.76, 0.45)));
            let mut f = Mesh::default();
            for (x, z) in [(-0.9, -0.38), (0.9, -0.38), (-0.9, 0.38), (0.9, 0.38)] {
                f.append(&Mesh::cuboid(Vec3::new(x - 0.03, 0.0, z - 0.03), Vec3::new(x + 0.03, 0.72, z + 0.03)));
            }
            scene.add_object(g, "Legs", metal, f);
        }
        "Bench" => {
            scene.add_object(g, "Seat", wood, Mesh::cuboid(Vec3::new(-0.9, 0.42, -0.2), Vec3::new(0.9, 0.47, 0.2)));
            let mut f = Mesh::cuboid(Vec3::new(-0.8, 0.0, -0.18), Vec3::new(-0.74, 0.42, 0.18));
            f.append(&Mesh::cuboid(Vec3::new(0.74, 0.0, -0.18), Vec3::new(0.8, 0.42, 0.18)));
            scene.add_object(g, "Legs", metal, f);
        }
        "Parasol" => {
            scene.add_object(g, "Pole", metal, Mesh::cylinder(Z, 0.03, 2.3, 8));
            let canvas = mat(scene, "Canvas", Material { color: [0.88, 0.85, 0.78], roughness: 0.9, double_sided: true, ..Default::default() });
            scene.add_object(g, "Canopy", canvas, Mesh::cone(Vec3::new(0.0, 2.0, 0.0), 1.6, 0.5, 16));
        }
        "Standing Person" => {
            let mut b = Mesh::cylinder(Z, 0.18, 1.45, 12);
            b.append(&Mesh::blob(Vec3::new(0.0, 1.6, 0.0), Vec3::new(0.11, 0.13, 0.11), 8, 0.0, 0.0));
            scene.add_object(g, "Figure", person, b);
        }
        "Sitting Person" => {
            let mut b = Mesh::cuboid(Vec3::new(-0.18, 0.45, -0.15), Vec3::new(0.18, 1.1, 0.15));
            b.append(&Mesh::cuboid(Vec3::new(-0.18, 0.4, -0.15), Vec3::new(0.18, 0.5, 0.35)));
            b.append(&Mesh::blob(Vec3::new(0.0, 1.25, 0.0), Vec3::new(0.11, 0.13, 0.11), 8, 0.0, 0.0));
            scene.add_object(g, "Figure", person, b);
        }
        "Hatchback Car" => {
            let paint = mat(scene, "Car Paint Red", Material { mtype: MaterialType::Clearcoat, color: [0.55, 0.05, 0.04], roughness: 0.2, metallic: 0.3, ..Default::default() });
            let glass = mat(scene, "Glass", Material { color: [0.9, 0.95, 0.95], roughness: 0.02, opacity: 0.18, refractive_index: 1.52, glass_tint: [0.85, 0.95, 0.95], specular: 1.0, double_sided: true, ..Default::default() });
            let tyre = mat(scene, "Rubber", Material { color: [0.03, 0.03, 0.03], roughness: 0.9, ..Default::default() });
            scene.add_object(g, "Body", paint, Mesh::cuboid(Vec3::new(-0.85, 0.3, -2.0), Vec3::new(0.85, 0.95, 2.0)));
            scene.add_object(g, "Cabin", glass, Mesh::cuboid(Vec3::new(-0.78, 0.95, -1.2), Vec3::new(0.78, 1.45, 1.3)));
            let mut w = Mesh::default();
            for (x, z) in [(-0.85, -1.3), (0.65, -1.3), (-0.85, 1.3), (0.65, 1.3)] {
                let mut c = Mesh::cylinder(Vec3::ZERO, 0.32, 0.2, 14);
                for v in &mut c.vertices {
                    let p = Vec3::from(v.pos);
                    let n = Vec3::from(v.normal);
                    v.pos = [p.y + x, p.x + 0.32, p.z + z];
                    v.normal = [n.y, n.x, n.z];
                }
                w.append(&c);
            }
            scene.add_object(g, "Wheels", tyre, w);
        }
        "Bicycle" => {
            let mut f = Mesh::cuboid(Vec3::new(-0.02, 0.35, -0.5), Vec3::new(0.02, 0.4, 0.5));
            f.append(&Mesh::cuboid(Vec3::new(-0.02, 0.35, -0.05), Vec3::new(0.02, 0.9, 0.05)));
            f.append(&Mesh::blob(Vec3::new(0.0, 0.35, -0.55), Vec3::new(0.03, 0.33, 0.33), 8, 0.0, 0.0));
            f.append(&Mesh::blob(Vec3::new(0.0, 0.35, 0.55), Vec3::new(0.03, 0.33, 0.33), 8, 0.0, 0.0));
            scene.add_object(g, "Frame", metal, f);
        }
        "Floor Lamp" => {
            scene.add_object(g, "Stand", metal, Mesh::cylinder(Z, 0.02, 1.5, 8));
            scene.add_object(g, "Shade", lamp, Mesh::cylinder(Vec3::new(0.0, 1.45, 0.0), 0.22, 0.3, 16));
            scene.lights.push(Light { group: g, local_pos: Vec3::new(0.0, 1.55, 0.0), color: [1.0, 0.82, 0.6], lumen: 800.0 });
        }
        "Garden Bollard" => {
            scene.add_object(g, "Post", metal, Mesh::cuboid(Vec3::new(-0.08, 0.0, -0.08), Vec3::new(0.08, 0.65, 0.08)));
            scene.add_object(g, "Lens", lamp, Mesh::cuboid(Vec3::new(-0.085, 0.5, -0.085), Vec3::new(0.085, 0.6, 0.085)));
            scene.lights.push(Light { group: g, local_pos: Vec3::new(0.0, 0.55, 0.0), color: [1.0, 0.8, 0.55], lumen: 300.0 });
        }
        _ => {
            // Pendant Light
            scene.add_object(g, "Cable", metal, Mesh::cylinder(Vec3::new(0.0, 2.2, 0.0), 0.005, 0.8, 6));
            scene.add_object(g, "Globe", lamp, Mesh::blob(Vec3::new(0.0, 2.1, 0.0), Vec3::splat(0.15), 10, 0.0, 0.0));
            scene.lights.push(Light { group: g, local_pos: Vec3::new(0.0, 2.0, 0.0), color: [1.0, 0.85, 0.65], lumen: 1000.0 });
        }
    }
    g
}

// ------------------------------------------------------------------ OBJ import

/// Read a Wavefront OBJ (+MTL) into a new group. Returns the group id.
pub fn import_obj(scene: &mut Scene, path: &Path, scale: f32) -> Result<u32, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("Could not read {}: {e}", path.display()))?;
    let dir = path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
    let mut pos: Vec<Vec3> = Vec::new();
    let mut nor: Vec<Vec3> = Vec::new();
    let mut tex: Vec<Vec2> = Vec::new();
    let mut parts: Vec<(String, Mesh)> = vec![("Default".into(), Mesh::default())];
    let mut mtl_defs: Vec<Material> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        let mut it = line.split_whitespace();
        let Some(tag) = it.next() else { continue };
        let nums = |it: std::str::SplitWhitespace| it.filter_map(|v| v.parse::<f32>().ok()).collect::<Vec<_>>();
        match tag {
            "v" => {
                let n = nums(it);
                if n.len() >= 3 {
                    pos.push(Vec3::new(n[0], n[1], n[2]) * scale);
                }
            }
            "vn" => {
                let n = nums(it);
                if n.len() >= 3 {
                    nor.push(Vec3::new(n[0], n[1], n[2]));
                }
            }
            "vt" => {
                let n = nums(it);
                if n.len() >= 2 {
                    tex.push(Vec2::new(n[0], n[1]));
                }
            }
            "usemtl" => {
                let name = it.collect::<Vec<_>>().join(" ");
                parts.push((name, Mesh::default()));
            }
            "mtllib" => {
                let f = it.collect::<Vec<_>>().join(" ");
                if let Ok(mt) = std::fs::read_to_string(dir.join(&f)) {
                    mtl_defs.extend(parse_mtl(&mt, &dir));
                }
            }
            "f" => {
                let mut idx: Vec<(usize, Option<usize>, Option<usize>)> = Vec::new();
                for tok in it {
                    let mut p = tok.split('/');
                    let fix = |s: Option<&str>, len: usize| -> Option<usize> {
                        let v: i64 = s?.parse().ok()?;
                        if v < 0 {
                            Some((len as i64 + v) as usize)
                        } else {
                            Some(v as usize - 1)
                        }
                    };
                    let vi = fix(p.next(), pos.len());
                    let ti = fix(p.next(), tex.len());
                    let ni = fix(p.next(), nor.len());
                    if let Some(vi) = vi {
                        if vi < pos.len() {
                            idx.push((vi, ti, ni));
                        }
                    }
                }
                if idx.len() < 3 {
                    continue;
                }
                let mesh = &mut parts.last_mut().unwrap().1;
                let p0 = pos[idx[0].0];
                let face_n = (pos[idx[1].0] - p0).cross(pos[idx[2].0] - p0).normalize_or_zero();
                for k in 1..idx.len() - 1 {
                    for &(vi, ti, ni) in [idx[0], idx[k], idx[k + 1]].iter() {
                        let n = ni.and_then(|i| nor.get(i).copied()).unwrap_or(face_n);
                        let p = pos[vi];
                        let uv = ti.and_then(|i| tex.get(i).copied()).unwrap_or(Vec2::new(p.x + p.z, p.y));
                        mesh.indices.push(mesh.vertices.len() as u32);
                        mesh.vertices.push(Vertex { pos: p.into(), normal: n.into(), uv: uv.into() });
                    }
                }
            }
            _ => {}
        }
    }
    let name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Model".into());
    let g = scene.add_group(&format!("Model: {name}"), GroupKind::Imported);
    // place on the ground
    let mut min_y = f32::MAX;
    for (_, m) in &parts {
        for v in &m.vertices {
            min_y = min_y.min(v.pos[1]);
        }
    }
    if let Some(gr) = scene.group_mut(g) {
        gr.source = Some(path.to_string_lossy().to_string());
        if min_y.is_finite() && min_y < 0.0 {
            gr.offset.y = -min_y;
        }
    }
    let mut count = 0;
    for (mname, mesh) in parts {
        if mesh.indices.is_empty() {
            continue;
        }
        let def = mtl_defs.iter().find(|m| m.name == mname).cloned();
        let mi = scene.material(&mname, || def.unwrap_or_else(|| Material::new(&mname, [0.75, 0.75, 0.75], 0.6)));
        scene.add_object(g, &mname, mi, mesh);
        count += 1;
    }
    if count == 0 {
        scene.remove_group(g);
        return Err("The file holds no faces.".into());
    }
    Ok(g)
}

fn parse_mtl(text: &str, dir: &Path) -> Vec<Material> {
    let mut out: Vec<Material> = Vec::new();
    for line in text.lines() {
        let mut it = line.split_whitespace();
        let Some(tag) = it.next() else { continue };
        let rest: Vec<&str> = it.collect();
        let f = |i: usize| rest.get(i).and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0);
        match tag {
            "newmtl" => out.push(Material::new(&rest.join(" "), [0.8, 0.8, 0.8], 0.6)),
            "Kd" => {
                if let Some(m) = out.last_mut() {
                    m.color = [f(0), f(1), f(2)];
                }
            }
            "Ns" => {
                if let Some(m) = out.last_mut() {
                    m.roughness = (1.0 - (f(0) / 1000.0).sqrt()).clamp(0.02, 1.0);
                }
            }
            "d" => {
                if let Some(m) = out.last_mut() {
                    m.opacity = f(0).clamp(0.0, 1.0);
                }
            }
            "Tr" => {
                if let Some(m) = out.last_mut() {
                    m.opacity = (1.0 - f(0)).clamp(0.0, 1.0);
                }
            }
            "map_Kd" => {
                if let Some(m) = out.last_mut() {
                    if let Some(file) = rest.last() {
                        m.texture = dir.join(file).to_string_lossy().to_string();
                        m.color = [1.0, 1.0, 1.0];
                    }
                }
            }
            _ => {}
        }
    }
    out
}
