//! Sun position from place, date and time (NOAA solar-position approximation).
//! The original stores a view's sun as day, month, year, hour, GMT offset,
//! longitude, latitude and a direction vector (ViewSunPosition, see MECHANICS.md).

use glam::Vec3;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SunState {
    pub day: u32,
    pub month: u32,
    pub year: i32,
    pub hour: f32,
    pub gmt_offset: f32,
    pub latitude: f32,
    pub longitude: f32,
    /// Extra rotation of the whole sun path around the vertical axis (degrees), "solar rotation".
    pub rotation: f32,
    /// Extra altitude offset (degrees), "solar angle".
    pub angle_offset: f32,
}

impl Default for SunState {
    fn default() -> Self {
        Self {
            day: 21,
            month: 6,
            year: 2026,
            hour: 15.0,
            gmt_offset: 1.0,
            latitude: 50.94,
            longitude: 6.96,
            rotation: 0.0,
            angle_offset: 0.0,
        }
    }
}

fn day_of_year(day: u32, month: u32, year: i32) -> f32 {
    let days = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    let mut n = 0;
    for m in 0..(month.clamp(1, 12) - 1) as usize {
        n += days[m];
        if m == 1 && leap {
            n += 1;
        }
    }
    (n + day.clamp(1, 31)) as f32
}

impl SunState {
    /// (azimuth from north clockwise, altitude) in degrees.
    pub fn azimuth_altitude(&self) -> (f32, f32) {
        let n = day_of_year(self.day, self.month, self.year);
        let gamma = 2.0 * std::f32::consts::PI / 365.0 * (n - 1.0 + (self.hour - 12.0) / 24.0);
        let eqtime = 229.18
            * (0.000075 + 0.001868 * gamma.cos()
                - 0.032077 * gamma.sin()
                - 0.014615 * (2.0 * gamma).cos()
                - 0.040849 * (2.0 * gamma).sin());
        let decl = 0.006918 - 0.399912 * gamma.cos() + 0.070257 * gamma.sin()
            - 0.006758 * (2.0 * gamma).cos()
            + 0.000907 * (2.0 * gamma).sin()
            - 0.002697 * (3.0 * gamma).cos()
            + 0.00148 * (3.0 * gamma).sin();
        let time_offset = eqtime + 4.0 * self.longitude - 60.0 * self.gmt_offset;
        let tst = self.hour * 60.0 + time_offset;
        let ha = (tst / 4.0 - 180.0).to_radians();
        let lat = self.latitude.to_radians();
        let cos_zen = (lat.sin() * decl.sin() + lat.cos() * decl.cos() * ha.cos()).clamp(-1.0, 1.0);
        let zen = cos_zen.acos();
        let alt = 90.0 - zen.to_degrees();
        let sin_zen = zen.sin().max(1e-4);
        // azimuth measured clockwise from north
        let cos_az = ((decl.sin() - lat.sin() * cos_zen) / (lat.cos().max(1e-4) * sin_zen)).clamp(-1.0, 1.0);
        let mut az = cos_az.acos().to_degrees();
        if ha > 0.0 {
            az = 360.0 - az;
        }
        (az, alt)
    }

    /// Direction *towards* the sun in world space (+Y up, -Z north, +X east).
    pub fn direction(&self) -> Vec3 {
        let (az, alt) = self.azimuth_altitude();
        let az = (az + self.rotation).to_radians();
        let alt = (alt + self.angle_offset).clamp(-89.0, 89.0).to_radians();
        Vec3::new(az.sin() * alt.cos(), alt.sin(), -az.cos() * alt.cos()).normalize()
    }

    pub fn time_string(&self) -> String {
        let h = self.hour.rem_euclid(24.0);
        let hh = h.floor() as u32;
        let mm = ((h - h.floor()) * 60.0).floor() as u32;
        format!("{:02}:{:02}", hh, mm)
    }
}
