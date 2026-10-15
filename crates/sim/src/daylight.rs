//! The day/night cycle: sun path, sky colours and light levels for any hour.
//!
//! Port of `updateLightingAndSky()` in `index.html`. Colours are plain `0xRRGGBB`
//! numbers (sRGB, like the original's hex codes), because this crate has no colour
//! type. The renderer converts them. Intensities use the original's Three.js
//! scale (sun 1.35 at midday), and the renderer maps them to physical light units.

use std::f32::consts::TAU;

use glam::Vec3;

/// The original's default time: 1:30 pm.
pub const DEFAULT_HOURS: f32 = 13.5;
/// Game hours that pass per real second while the clock runs (a day lasts 96 s).
const HOURS_PER_SECOND: f32 = 0.25;
/// Radius of the sun's circle around the island.
const SUN_ORBIT_RADIUS: f32 = 150.0;

/// The broad part of the day. Each has its own palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Dawn,
    Day,
    Dusk,
    Night,
}

impl Phase {
    /// Which phase an hour (0.0..24.0) falls in, with the original's boundaries.
    pub fn at(hours: f32) -> Self {
        // A `match` on a float with guards. Ranges like `5.5..7.8` read clearly,
        // and `contains` checks start-inclusive, end-exclusive.
        match hours {
            h if (5.5..7.8).contains(&h) => Phase::Dawn,
            h if (7.8..=17.2).contains(&h) => Phase::Day,
            h if h > 17.2 && h <= 19.5 => Phase::Dusk,
            _ => Phase::Night,
        }
    }
}

/// Everything the renderer needs to light the scene at one moment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Daylight {
    pub phase: Phase,
    /// Where the sun is on its circle. Below the horizon at night.
    pub sun_position: Vec3,
    /// Where the scene's main light shines from: the sun by day, and at night a
    /// moon opposite the sun. (The original lit the island from *below* at night,
    /// through the ground. Mirroring it above the horizon keeps moonlight sensible.)
    pub light_position: Vec3,
    pub sky_color: u32,
    pub fog_color: u32,
    pub light_color: u32,
    /// Main light strength, on the original's scale (1.35 at midday).
    pub light_intensity: f32,
    /// Ambient light strength, on the original's scale (0.52 at midday).
    pub ambient_intensity: f32,
    /// How visible the stars are, from 0.0 (hidden) to 1.0.
    pub star_opacity: f32,
}

impl Daylight {
    pub fn at(hours: f32) -> Self {
        let phase = Phase::at(hours);
        // The sun rises at 06:00 (angle 0, due east) and sets at 18:00.
        let angle = (hours - 6.0) / 24.0 * TAU;
        let sun_position = Vec3::new(
            angle.cos() * SUN_ORBIT_RADIUS,
            angle.sin() * SUN_ORBIT_RADIUS,
            45.0,
        );
        let light_position = if sun_position.y >= 0.0 {
            sun_position
        } else {
            Vec3::new(-sun_position.x, -sun_position.y, sun_position.z)
        };

        // A tuple lets one `match` produce several values at once, which the
        // `let (a, b, ...) =` pattern then unpacks into named variables.
        let (sky_color, fog_color, light_color, light_intensity, ambient_intensity, star_opacity) =
            match phase {
                Phase::Dawn => (0xfdba74, 0xe29578, 0xffedd5, 0.95, 0.42, 0.0),
                Phase::Day => (0x60a5fa, 0x93c5fd, 0xfffbeb, 1.35, 0.52, 0.0),
                Phase::Dusk => (0xf43f5e, 0x881337, 0xfda4af, 0.85, 0.35, 0.3),
                Phase::Night => (0x070b14, 0x0f172a, 0x818cf8, 0.28, 0.22, 0.9),
            };

        Self {
            phase,
            sun_position,
            light_position,
            sky_color,
            fog_color,
            light_color,
            light_intensity,
            ambient_intensity,
            star_opacity,
        }
    }
}

/// The time of day, and whether it's moving.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Clock {
    /// Hours since midnight, always in `0.0..24.0`.
    pub hours: f32,
    /// When `false`, time stands still (the original's default).
    pub running: bool,
}

impl Default for Clock {
    fn default() -> Self {
        Self {
            hours: DEFAULT_HOURS,
            running: false,
        }
    }
}

impl Clock {
    /// Moves the clock forward by `dt` real seconds, if it's running.
    pub fn tick(&mut self, dt: f32) {
        if self.running {
            self.set(self.hours + dt * HOURS_PER_SECOND);
        }
    }

    /// Sets the time, wrapping into `0.0..24.0` (so 25.0 becomes 1.0, and -1.0 becomes 23.0).
    pub fn set(&mut self, hours: f32) {
        self.hours = hours.rem_euclid(24.0);
    }

    pub fn daylight(&self) -> Daylight {
        Daylight::at(self.hours)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phases_follow_the_original_boundaries() {
        assert_eq!(Phase::at(5.0), Phase::Night);
        assert_eq!(Phase::at(6.0), Phase::Dawn);
        assert_eq!(Phase::at(12.0), Phase::Day);
        assert_eq!(Phase::at(17.2), Phase::Day);
        assert_eq!(Phase::at(18.0), Phase::Dusk);
        assert_eq!(Phase::at(23.0), Phase::Night);
    }

    #[test]
    fn sun_is_up_by_day_and_down_at_night() {
        assert!(
            Daylight::at(12.0).sun_position.y > 140.0,
            "noon sun should be high"
        );
        assert!(
            Daylight::at(0.0).sun_position.y < -140.0,
            "midnight sun should be low"
        );
    }

    #[test]
    fn the_light_always_shines_from_above() {
        for tenth in 0..240 {
            let hours = tenth as f32 / 10.0;
            let light = Daylight::at(hours).light_position;
            assert!(
                light.y >= -1e-3,
                "at {hours}h the light is below the horizon: {light}"
            );
        }
    }

    #[test]
    fn midday_is_brighter_than_midnight_and_stars_only_show_after_dark() {
        let noon = Daylight::at(12.0);
        let midnight = Daylight::at(0.0);
        assert!(noon.light_intensity > midnight.light_intensity);
        assert!(noon.ambient_intensity > midnight.ambient_intensity);
        assert_eq!(noon.star_opacity, 0.0);
        assert!(midnight.star_opacity > 0.5);
    }

    #[test]
    fn clock_wraps_around_midnight_and_only_ticks_when_running() {
        let mut clock = Clock {
            hours: 23.5,
            running: true,
        };
        clock.tick(4.0); // 4 real seconds = 1 game hour.
        assert!((clock.hours - 0.5).abs() < 1e-4, "hours = {}", clock.hours);

        clock.running = false;
        clock.tick(100.0);
        assert!((clock.hours - 0.5).abs() < 1e-4, "a stopped clock moved");

        clock.set(-1.0);
        assert!((clock.hours - 23.0).abs() < 1e-4);
    }
}
