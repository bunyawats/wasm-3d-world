//! Parity test: the Rust terrain must match the original hand-assembled WASM.
//!
//! Files under `tests/` are *integration tests*: each is compiled as a separate crate
//! that can only use `sim`'s public API, just like a real user of the library.
//!
//! The fixture is produced by `node tools/dump_terrain_reference.mjs`, which runs the
//! real WASM from `index.html`. Regenerate it if the original JS kernel changes.

use sim::Terrain;

/// `include_str!` embeds the file into the test binary at compile time, so the test
/// doesn't depend on the working directory it's run from.
const REFERENCE: &str = include_str!("fixtures/terrain_reference.txt");

/// Heights reach about ±30 m, so a millimetre-scale tolerance still means
/// "practically identical", while allowing last-bit differences in `sin`/`cos`
/// between V8 and Rust's standard library.
const HEIGHT_TOLERANCE: f32 = 1e-3;
const NORMAL_TOLERANCE: f32 = 1e-4;

/// One line of the fixture file.
struct Sample {
    terrain: Terrain,
    x: f32,
    z: f32,
    height: f32,
    normal: [f32; 3],
}

fn parse_samples() -> Vec<Sample> {
    REFERENCE
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            // Parse every whitespace-separated field as f32. `expect` panics with
            // the message if parsing fails, which is fine in a test.
            let v: Vec<f32> = line
                .split_whitespace()
                .map(|field| field.parse().expect("fixture field should be a number"))
                .collect();
            assert_eq!(v.len(), 8, "fixture line should have 8 fields: {line}");
            Sample {
                terrain: Terrain {
                    height_scale: v[0],
                    seed_offset: v[1],
                    ..Terrain::default()
                },
                x: v[2],
                z: v[3],
                height: v[4],
                normal: [v[5], v[6], v[7]],
            }
        })
        .collect()
}

#[test]
fn height_matches_original_wasm() {
    let samples = parse_samples();
    assert!(!samples.is_empty(), "fixture is empty");

    for s in &samples {
        let got = s.terrain.height(s.x, s.z);
        assert!(
            (got - s.height).abs() <= HEIGHT_TOLERANCE,
            "height({}, {}) with {:?}: got {got}, original WASM gave {}",
            s.x,
            s.z,
            s.terrain,
            s.height,
        );
    }
}

#[test]
fn normal_matches_original_wasm() {
    for s in &parse_samples() {
        let got = s.terrain.normal(s.x, s.z);
        // `zip` walks the two arrays side by side.
        for (axis, (g, want)) in got.to_array().iter().zip(s.normal).enumerate() {
            assert!(
                (g - want).abs() <= NORMAL_TOLERANCE,
                "normal({}, {}) axis {axis} with {:?}: got {g}, original WASM gave {want}",
                s.x,
                s.z,
                s.terrain,
            );
        }
    }
}
