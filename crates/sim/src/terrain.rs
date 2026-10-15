//! Procedural island heightfield.
//!
//! Port of `terrainHeight` (func 3) and `computeNormal` (func 4) from `index.html`.
//! The original read its parameters from shared WASM memory (`f32[0..3]`); here they
//! are ordinary struct fields, and the normal is returned instead of being written
//! into a scratch slot.

/// Parameters of the island. One value of this type fully describes a terrain.
///
/// `#[derive(...)]` asks the compiler to generate trait implementations:
/// - `Debug` lets you print it with `{:?}`.
/// - `Clone` and `Copy` let it be duplicated by plain assignment (it's only four
///   floats, so copying is cheaper than borrowing).
/// - `PartialEq` allows `==`, which tests use.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Terrain {
    /// Vertical exaggeration. 3.0 is the original default; the formula divides by 3,
    /// so 3.0 means "unscaled".
    pub height_scale: f32,
    /// Phase shift for the sine layers. Changing it generates a different island.
    pub seed_offset: f32,
    /// Height of the sea surface.
    pub water_level: f32,
    /// Side length of the square world, in metres.
    pub world_size: f32,
}

/// `Default` is the standard trait for "a sensible starting value".
/// Callers write `Terrain::default()`, or override a single field with
/// `Terrain { seed_offset: 4.0, ..Terrain::default() }`.
impl Default for Terrain {
    fn default() -> Self {
        Self {
            height_scale: 3.0,
            seed_offset: 0.0,
            water_level: 0.8,
            world_size: 160.0,
        }
    }
}

/// Distance between the height samples used to estimate the surface slope.
const NORMAL_SAMPLE_OFFSET: f32 = 0.35;
/// Fixed vertical component of the unnormalised normal (from the original kernel).
/// Smaller values make slopes read as steeper.
const NORMAL_UP_BIAS: f32 = 0.70;

impl Terrain {
    /// Terrain height at world position `(x, z)`.
    ///
    /// `&self` means the method *borrows* the terrain read-only: calling it doesn't
    /// move or modify the `Terrain`, so you can call it as often as you like.
    pub fn height(&self, x: f32, z: f32) -> f32 {
        let s = self.seed_offset;
        let dist_sq = x * x + z * z;
        let dist = dist_sq.sqrt();
        // The island's radius, measured from the origin.
        let max_dist = self.world_size * 0.46;

        // 1. Coastline falloff: 1.0 inland, easing to 0.0 at the island's edge.
        //    `if` is an *expression* in Rust, so its result can be bound directly.
        let falloff = if dist > max_dist * 0.62 {
            let f = (1.0 - (dist - max_dist * 0.62) / (max_dist * 0.38)).max(0.0);
            smoothstep(f)
        } else {
            1.0
        };

        // 2. Layered sine waves give the rolling hills.
        let mut h = sin(x * 0.028 + s) * cos(z * 0.028 + s) * 10.5;
        h += sin(x * 0.065 - s) * cos(z * 0.060 + s * 0.5) * 4.2;
        h += sin((x + z) * 0.018 + 1.4) * 9.0;
        h += cos(x * 0.12) * sin(z * 0.12) * 1.1;
        h *= self.height_scale / 3.0;

        // 3. Flatten a grassy meadow around the origin, where the herd lives.
        //    `meadow` is 1.0 at the centre and fades out with distance (a Gaussian).
        let meadow = exp(-dist_sq / 1400.0);
        h = (h * (1.0 - meadow * 0.55) + 3.2 * meadow) * falloff;

        // 4. Beyond the beach, slope down into a deep ocean trench.
        if dist > max_dist * 0.78 {
            h -= (dist - max_dist * 0.78) * 0.45;
        }

        h
    }

    /// Unit-length surface normal at `(x, z)`, as `[x, y, z]`.
    ///
    /// It's estimated by sampling the height on either side (central differences),
    /// the same way the original kernel did.
    pub fn normal(&self, x: f32, z: f32) -> [f32; 3] {
        let d = NORMAL_SAMPLE_OFFSET;
        let nx = self.height(x - d, z) - self.height(x + d, z);
        let ny = NORMAL_UP_BIAS;
        let nz = self.height(x, z - d) - self.height(x, z + d);
        let len = (nx * nx + ny * ny + nz * nz).sqrt();
        [nx / len, ny / len, nz / len]
    }
}

/// Hermite smoothstep on an already-clamped `t` in `0.0..=1.0`.
/// A private helper (no `pub`), so it's invisible outside this module.
fn smoothstep(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

// The original WASM imported `sin`, `cos` and `exp` from JavaScript's `Math`, which
// works in 64-bit floats. To reproduce its output bit for bit, widen to `f64`,
// compute, and round back to `f32`. `as` is Rust's explicit numeric conversion:
// Rust never converts between number types implicitly.
fn sin(v: f32) -> f32 {
    f64::from(v).sin() as f32
}

fn cos(v: f32) -> f32 {
    f64::from(v).cos() as f32
}

fn exp(v: f32) -> f32 {
    f64::from(v).exp() as f32
}

// Unit tests live next to the code they test. `#[cfg(test)]` means this module is
// compiled only by `cargo test`, never into the real build.
#[cfg(test)]
mod tests {
    use super::*; // Bring everything from the parent module into scope.

    #[test]
    fn centre_is_a_raised_meadow_above_water() {
        let terrain = Terrain::default();
        let h = terrain.height(0.0, 0.0);
        assert!(
            h > terrain.water_level,
            "centre height {h} should be above water"
        );
    }

    #[test]
    fn world_corners_are_deep_ocean() {
        let terrain = Terrain::default();
        let half = terrain.world_size / 2.0;
        for (x, z) in [(half, half), (-half, half), (half, -half), (-half, -half)] {
            let h = terrain.height(x, z);
            assert!(
                h < -10.0,
                "corner ({x}, {z}) height {h} should be deep below water"
            );
        }
    }

    #[test]
    fn normals_have_unit_length_and_point_up() {
        let terrain = Terrain::default();
        for i in -8..=8 {
            // `as f32` converts the integer loop counter to a float.
            let x = i as f32 * 10.0;
            let [nx, ny, nz] = terrain.normal(x, x * 0.5); // Destructure the array.
            let len = (nx * nx + ny * ny + nz * nz).sqrt();
            assert!((len - 1.0).abs() < 1e-5, "normal at x={x} has length {len}");
            assert!(ny > 0.0, "normal at x={x} points downward");
        }
    }

    #[test]
    fn seed_offset_changes_the_island() {
        let a = Terrain::default();
        let b = Terrain {
            seed_offset: 42.0,
            ..Terrain::default()
        };
        assert_ne!(a.height(20.0, -15.0), b.height(20.0, -15.0));
    }
}
