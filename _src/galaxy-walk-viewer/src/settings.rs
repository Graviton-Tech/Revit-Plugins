//! Visual settings, presets and window/input preferences.
//! Defaults and slider ranges for the viewer settings.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RenderingStyle {
    None,
    White,
    Polystyrol,
    LightView,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Projection {
    Perspective,
    TwoPoint,
    Orthographic,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Quality {
    Draft,
    Medium,
    High,
    Ultra,
}

impl Quality {
    pub const ALL: [Quality; 4] = [Quality::Draft, Quality::Medium, Quality::High, Quality::Ultra];
    pub fn name(self) -> &'static str {
        match self {
            Quality::Draft => "Draft",
            Quality::Medium => "Medium",
            Quality::High => "High",
            Quality::Ultra => "Ultra",
        }
    }
    /// Internal render scale (estimated).
    pub fn render_scale(self) -> f32 {
        match self {
            Quality::Draft => 0.5,
            Quality::Medium => 0.75,
            Quality::High => 1.0,
            Quality::Ultra => 1.0,
        }
    }
    pub fn shadow_size(self) -> i32 {
        match self {
            Quality::Draft => 1024,
            Quality::Medium => 2048,
            Quality::High => 3072,
            Quality::Ultra => 4096,
        }
    }
    pub fn shadow_taps(self) -> i32 {
        match self {
            Quality::Draft => 1,
            Quality::Medium => 2,
            Quality::High => 3,
            Quality::Ultra => 4,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Horizon {
    Clear,
    Mountains,
    Forest,
    Town,
    Urban,
    Desert,
    Site,
    WhiteCubes,
    WhiteGround,
    Skybox,
}

impl Horizon {
    pub const ALL: [Horizon; 10] = [
        Horizon::Clear,
        Horizon::Mountains,
        Horizon::Forest,
        Horizon::Town,
        Horizon::Urban,
        Horizon::Desert,
        Horizon::Site,
        Horizon::WhiteCubes,
        Horizon::WhiteGround,
        Horizon::Skybox,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Horizon::Clear => "Clear",
            Horizon::Mountains => "Mountains",
            Horizon::Forest => "Forest",
            Horizon::Town => "Town",
            Horizon::Urban => "Urban",
            Horizon::Desert => "Desert",
            Horizon::Site => "Construction Site",
            Horizon::WhiteCubes => "White Cubes",
            Horizon::WhiteGround => "White Ground",
            Horizon::Skybox => "Skybox (from file)",
        }
    }
    pub fn index(self) -> i32 {
        Horizon::ALL.iter().position(|h| *h == self).unwrap_or(0) as i32
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CaptureResolution {
    Window,
    R1024,
    Hd,
    FullHd,
    UltraHd,
    Custom,
}

impl CaptureResolution {
    pub const ALL: [CaptureResolution; 6] = [
        CaptureResolution::Window,
        CaptureResolution::R1024,
        CaptureResolution::Hd,
        CaptureResolution::FullHd,
        CaptureResolution::UltraHd,
        CaptureResolution::Custom,
    ];
    pub fn name(self) -> &'static str {
        match self {
            CaptureResolution::Window => "Window",
            CaptureResolution::R1024 => "1024",
            CaptureResolution::Hd => "HD",
            CaptureResolution::FullHd => "Full HD",
            CaptureResolution::UltraHd => "Ultra HD",
            CaptureResolution::Custom => "Custom",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Bitrate {
    Email,
    Web,
    BluRay,
    Maximum,
    Lossless,
}

impl Bitrate {
    pub const ALL: [Bitrate; 5] = [Bitrate::Email, Bitrate::Web, Bitrate::BluRay, Bitrate::Maximum, Bitrate::Lossless];
    pub fn name(self) -> &'static str {
        match self {
            Bitrate::Email => "Email",
            Bitrate::Web => "Web",
            Bitrate::BluRay => "BluRay",
            Bitrate::Maximum => "Maximum",
            Bitrate::Lossless => "Lossless",
        }
    }
    /// JPEG quality used for image sequences (estimated mapping).
    pub fn jpeg_quality(self) -> u8 {
        match self {
            Bitrate::Email => 55,
            Bitrate::Web => 70,
            Bitrate::BluRay => 85,
            Bitrate::Maximum => 95,
            Bitrate::Lossless => 100,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VisualSettings {
    // ---- Main
    pub rendering_style: RenderingStyle,
    pub outline_thickness: f32,      // %
    pub polystyrol_transmission: f32, // %
    pub light_view_auto: bool,
    pub light_view_min: f32, // lx
    pub light_view_max: f32, // lx
    pub projection: Projection,
    pub auto_exposure: bool,
    pub exposure_brightness: f32, // %
    pub field_of_view: f32,       // degrees (horizontal)
    pub dof_strength: f32,        // %
    pub dof_auto_focus: bool,
    pub dof_focal_point: f32, // m
    pub quality: Quality,
    pub auto_resolution: bool,
    pub grass_rendering: bool,
    /// Ground plane under the model at project elevation 0, receiving shadows.
    pub ground_plane: bool,
    // ---- Image
    pub auto_contrast: bool,
    pub highlights: f32, // -100..100
    pub shadows: f32,    // -100..100
    pub saturation: f32, // 0..200
    pub color_temperature: f32, // K
    pub motion_blur: f32,
    pub lens_flare: f32,
    pub bloom: f32,
    pub vignette: f32,
    pub chromatic_aberration: f32,
    // ---- Atmosphere
    pub fog_intensity: f32, // %
    pub fog_height: f32,    // m
    pub sun_brightness: f32, // %
    pub night_sky_brightness: f32,
    pub shadow_sharpness: f32,
    pub artificial_light_brightness: f32,
    pub ambient_brightness: f32,
    pub wind_intensity: f32,
    pub wind_direction: f32, // degrees
    // ---- Sky
    pub white_background: bool,
    pub horizon: Horizon,
    pub horizon_rotation: f32,
    pub skybox_file: String,
    pub skybox_use_sun_spot: bool,
    pub skybox_normalize: bool,
    pub skybox_brightness: f32, // lux
    pub moon_size: f32,         // %
    pub cloud_density: f32,
    pub cloud_variety: f32,
    pub cloud_cirrus: f32,
    pub cloud_contrails: i32,
    pub cloud_x: f32,
    pub cloud_y: f32,
    // ---- Output
    pub capture_resolution: CaptureResolution,
    pub custom_width: u32,
    pub custom_height: u32,
    pub use_viewport_aspect: bool,
    pub show_safe_frame: bool,
    pub export_masks: bool,
    pub depth_range: f32, // m
    pub file_format: String,
    pub apply_alpha: bool,
    pub auto_naming: bool,
    pub default_folder: String,
    pub fps: u32,
    pub bitrate: Bitrate,
    pub panorama_resolution: u32,
}

impl Default for VisualSettings {
    fn default() -> Self {
        Self {
            rendering_style: RenderingStyle::None,
            outline_thickness: 0.0,
            polystyrol_transmission: 50.0,
            light_view_auto: true,
            light_view_min: 500.0,
            light_view_max: 90000.0,
            projection: Projection::Perspective,
            auto_exposure: true,
            exposure_brightness: 50.0,
            field_of_view: 90.0,
            dof_strength: 0.0,
            dof_auto_focus: true,
            dof_focal_point: 2.0,
            quality: Quality::High,
            auto_resolution: true,
            grass_rendering: true,
            ground_plane: true,
            auto_contrast: false,
            highlights: 0.0,
            shadows: 0.0,
            saturation: 100.0,
            color_temperature: 6600.0,
            motion_blur: 50.0,
            lens_flare: 50.0,
            bloom: 15.0,
            vignette: 30.0,
            chromatic_aberration: 0.0,
            fog_intensity: 10.0,
            fog_height: 70.0,
            sun_brightness: 80.0,
            night_sky_brightness: 100.0,
            shadow_sharpness: 50.0,
            artificial_light_brightness: 100.0,
            ambient_brightness: 50.0,
            wind_intensity: 25.0,
            wind_direction: 0.0,
            white_background: false,
            horizon: Horizon::Mountains,
            horizon_rotation: 0.0,
            skybox_file: String::new(),
            skybox_use_sun_spot: false,
            skybox_normalize: true,
            skybox_brightness: 2000.0,
            moon_size: 100.0,
            cloud_density: 50.0,
            cloud_variety: 50.0,
            cloud_cirrus: 50.0,
            cloud_contrails: 0,
            cloud_x: 5000.0,
            cloud_y: 5000.0,
            capture_resolution: CaptureResolution::FullHd,
            custom_width: 1920,
            custom_height: 1080,
            use_viewport_aspect: false,
            show_safe_frame: false,
            export_masks: false,
            depth_range: 20.0,
            file_format: "png".into(),
            apply_alpha: false,
            auto_naming: false,
            default_folder: String::new(),
            fps: 30,
            bitrate: Bitrate::Maximum,
            panorama_resolution: 4096,
        }
    }
}

impl VisualSettings {
    /// Output size for image exports, given the current viewport size in pixels.
    pub fn capture_size(&self, viewport: (u32, u32)) -> (u32, u32) {
        let (w, h) = match self.capture_resolution {
            CaptureResolution::Window => viewport,
            CaptureResolution::R1024 => (1024, 768),
            CaptureResolution::Hd => (1280, 720),
            CaptureResolution::FullHd => (1920, 1080),
            CaptureResolution::UltraHd => (3840, 2160),
            CaptureResolution::Custom => (self.custom_width, self.custom_height),
        };
        let (mut w, mut h) = (w.clamp(16, 8192), h.clamp(16, 8192));
        if self.use_viewport_aspect && self.capture_resolution != CaptureResolution::Window && viewport.1 > 0 {
            let aspect = viewport.0 as f32 / viewport.1 as f32;
            h = ((w as f32 / aspect).round() as u32).clamp(16, 8192);
            w = w.clamp(16, 8192);
        }
        (w, h)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Preset {
    pub name: String,
    pub settings: VisualSettings,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GeneralSettings {
    pub mouse_speed: f32,
    pub mouse_smoothing: f32,
    pub move_speed: f32,
    pub move_smoothing: f32,
    pub invert_x: bool,
    pub invert_y: bool,
    pub spectator_height: f32,
    pub spectator_width: f32,
    pub screenshot_hotkey_shift: bool,
    pub window_title: String,
    pub watermark_file: String,
    pub watermark_anchor: i32,
    pub show_help: bool,
    pub show_minimap: bool,
    pub sound: bool,
}

impl Default for GeneralSettings {
    fn default() -> Self {
        Self {
            mouse_speed: 1.0,
            mouse_smoothing: 1.5,
            move_speed: 1.0,
            move_smoothing: 0.0,
            invert_x: false,
            invert_y: false,
            spectator_height: 1.68,
            spectator_width: 0.5,
            screenshot_hotkey_shift: true,
            window_title: String::new(),
            watermark_file: String::new(),
            watermark_anchor: 7,
            show_help: true,
            show_minimap: false,
            sound: true,
        }
    }
}

/// Kelvin to linear RGB multiplier (Tanner Helland approximation), normalised so 6600 K is white.
pub fn kelvin_to_rgb(k: f32) -> [f32; 3] {
    let t = (k / 100.0).clamp(10.0, 400.0);
    let r = if t <= 66.0 { 255.0 } else { 329.698_73 * (t - 60.0).powf(-0.133_204_76) };
    let g = if t <= 66.0 {
        99.470_8 * t.ln() - 161.119_57
    } else {
        288.122_16 * (t - 60.0).powf(-0.075_514_85)
    };
    let b = if t >= 66.0 {
        255.0
    } else if t <= 19.0 {
        0.0
    } else {
        138.517_73 * (t - 10.0).ln() - 305.044_8
    };
    let c = |v: f32| (v.clamp(0.0, 255.0) / 255.0).powf(2.2);
    [c(r), c(g), c(b)]
}
