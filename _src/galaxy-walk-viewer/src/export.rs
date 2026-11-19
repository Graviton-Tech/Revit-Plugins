//! Saving rendered pictures: formats, watermark overlay, panorama stitching.

use glam::Vec3;
use image::{Rgba, RgbaImage};
use std::path::{Path, PathBuf};

/// The six cube-face camera bases (forward, up) used for panoramas.
pub fn cube_faces() -> [(Vec3, Vec3); 6] {
    [
        (Vec3::X, Vec3::Y),
        (-Vec3::X, Vec3::Y),
        (Vec3::Y, Vec3::Z),
        (-Vec3::Y, -Vec3::Z),
        (Vec3::Z, Vec3::Y),
        (-Vec3::Z, Vec3::Y),
    ]
}

/// Stitch six 90-degree faces into an equirectangular (long/lat) picture.
pub fn equirect_from_faces(faces: &[RgbaImage], width: u32) -> RgbaImage {
    let height = width / 2;
    let mut out = RgbaImage::new(width, height);
    let bases = cube_faces();
    let n = faces.first().map(|f| f.width()).unwrap_or(1) as f32;
    for y in 0..height {
        let polar = (y as f32 + 0.5) / height as f32 * std::f32::consts::PI;
        for x in 0..width {
            let az = ((x as f32 + 0.5) / width as f32 - 0.5) * std::f32::consts::TAU;
            let d = Vec3::new(az.sin() * polar.sin(), polar.cos(), -az.cos() * polar.sin());
            // pick the face whose forward axis is closest
            let mut best = 0;
            let mut bd = f32::MIN;
            for (i, (f, _)) in bases.iter().enumerate() {
                let k = d.dot(*f);
                if k > bd {
                    bd = k;
                    best = i;
                }
            }
            let (f, u) = bases[best];
            let r = f.cross(u);
            let fx = d.dot(r) / bd;
            let fy = d.dot(u) / bd;
            let px = (((fx + 1.0) * 0.5) * n).clamp(0.0, n - 1.0) as u32;
            let py = (((1.0 - (fy + 1.0) * 0.5)) * n).clamp(0.0, n - 1.0) as u32;
            if let Some(face) = faces.get(best) {
                out.put_pixel(x, y, *face.get_pixel(px.min(face.width() - 1), py.min(face.height() - 1)));
            }
        }
    }
    out
}

/// Put a watermark picture onto `img`. `anchor` follows the number pad: 7 top-left, 9 top-right, 1 bottom-left, 3 bottom-right, 5 centre.
pub fn apply_watermark(img: &mut RgbaImage, file: &str, anchor: i32) {
    if file.is_empty() {
        return;
    }
    let Ok(mark) = image::open(file) else { return };
    let mut mark = mark.to_rgba8();
    let max_w = (img.width() / 4).max(1);
    if mark.width() > max_w {
        let h = (mark.height() as f32 * max_w as f32 / mark.width() as f32) as u32;
        mark = image::imageops::resize(&mark, max_w, h.max(1), image::imageops::FilterType::Triangle);
    }
    let margin = 16i64;
    let (iw, ih, mw, mh) = (img.width() as i64, img.height() as i64, mark.width() as i64, mark.height() as i64);
    let col = (anchor - 1).rem_euclid(3) as i64; // 0 left 1 centre 2 right
    let row = 2 - (anchor - 1).div_euclid(3).clamp(0, 2) as i64; // 0 top .. 2 bottom
    let x = match col {
        0 => margin,
        1 => (iw - mw) / 2,
        _ => iw - mw - margin,
    };
    let y = match row {
        0 => margin,
        1 => (ih - mh) / 2,
        _ => ih - mh - margin,
    };
    image::imageops::overlay(img, &mark, x, y);
}

pub fn save_image(img: &RgbaImage, path: &Path, keep_alpha: bool, jpeg_quality: u8) -> Result<(), String> {
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if ext == "jpg" || ext == "jpeg" {
        let rgb = image::DynamicImage::ImageRgba8(img.clone()).to_rgb8();
        let f = std::fs::File::create(path).map_err(|e| e.to_string())?;
        let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(std::io::BufWriter::new(f), jpeg_quality);
        enc.encode_image(&rgb).map_err(|e| e.to_string())
    } else {
        let mut out = img.clone();
        if !keep_alpha {
            for p in out.pixels_mut() {
                p.0[3] = 255;
            }
        }
        out.save(path).map_err(|e| e.to_string())
    }
}

pub fn average(images: &[RgbaImage]) -> RgbaImage {
    if images.len() == 1 {
        return images[0].clone();
    }
    let (w, h) = images[0].dimensions();
    let mut out = RgbaImage::new(w, h);
    for (x, y, p) in out.enumerate_pixels_mut() {
        let mut acc = [0u32; 4];
        for im in images {
            let q = im.get_pixel(x, y);
            for c in 0..4 {
                acc[c] += q.0[c] as u32;
            }
        }
        let n = images.len() as u32;
        *p = Rgba([(acc[0] / n) as u8, (acc[1] / n) as u8, (acc[2] / n) as u8, (acc[3] / n) as u8]);
    }
    out
}

pub fn stack_vertical(top: &RgbaImage, bottom: &RgbaImage) -> RgbaImage {
    let mut out = RgbaImage::new(top.width(), top.height() + bottom.height());
    image::imageops::overlay(&mut out, top, 0, 0);
    image::imageops::overlay(&mut out, bottom, 0, top.height() as i64);
    out
}

/// Where the app saves by default: Pictures\Galaxy Walk in the user's folder
/// (Documents or the current folder when Pictures is missing). Created on demand.
pub fn default_output_dir() -> PathBuf {
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(PathBuf::from);
    let base = match home {
        Some(h) if h.join("Pictures").is_dir() => h.join("Pictures"),
        Some(h) if h.join("Documents").is_dir() => h.join("Documents"),
        Some(h) => h,
        None => std::env::current_dir().unwrap_or_default(),
    };
    let dir = base.join("Galaxy Walk");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// The folder saves go to: the "Default folder" from Output settings, else the default one.
pub fn output_dir(folder: &str) -> PathBuf {
    if folder.is_empty() {
        default_output_dir()
    } else {
        PathBuf::from(folder)
    }
}

/// A file dialog that starts in the save folder.
pub fn dialog(folder: &str) -> rfd::FileDialog {
    rfd::FileDialog::new().set_directory(output_dir(folder))
}

/// Show a file (selected) or a folder in the file manager.
pub fn reveal(path: &Path) {
    #[cfg(target_os = "windows")]
    {
        let _ = if path.is_file() {
            std::process::Command::new("explorer").arg(format!("/select,{}", path.display())).spawn()
        } else {
            std::process::Command::new("explorer").arg(path).spawn()
        };
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg("-R").arg(path).spawn();
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let dir = if path.is_file() { path.parent().unwrap_or(path) } else { path };
        let _ = std::process::Command::new("xdg-open").arg(dir).spawn();
    }
}

pub fn auto_name(folder: &str, prefix: &str, ext: &str) -> PathBuf {
    let base = output_dir(folder);
    let _ = std::fs::create_dir_all(&base);
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let mut i = 0;
    loop {
        let p = base.join(format!("{prefix}_{stamp}{}.{ext}", if i == 0 { String::new() } else { format!("_{i}") }));
        if !p.exists() {
            return p;
        }
        i += 1;
    }
}

pub fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let ext = path.extension().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "png".into());
    path.with_file_name(format!("{stem}{suffix}.{ext}"))
}
