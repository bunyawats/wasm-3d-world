//! Parity test: deer kinematics must match the original `stepDeerKinematics` WASM.
//!
//! The fixture comes from `node tools/dump_deer_reference.mjs`. Each line is one deer
//! with its starting state, how many steps to run, and the original's results.

use glam::Vec3;
use sim::{Deer, DeerKind, Terrain};

const REFERENCE: &str = include_str!("fixtures/deer_kinematics_reference.txt");

/// Results should be identical. A tiny tolerance just keeps the failure message
/// readable if a future change introduces last-bit noise.
const TOLERANCE: f32 = 1e-5;

#[test]
fn kinematics_match_original_wasm() {
    let terrain = Terrain::default();
    let mut checked = 0;

    for (line_no, line) in REFERENCE.lines().enumerate() {
        let v: Vec<f32> = line
            .split_whitespace()
            .map(|f| f.parse().expect("fixture field should be a number"))
            .collect();
        assert_eq!(v.len(), 27, "line {line_no} should have 27 fields");

        // Slice patterns: name the pieces of a slice in one go. `rest @ ..` binds
        // whatever is left over to `rest`.
        let [
            dt,
            steps,
            x,
            z,
            heading,
            speed,
            target_speed,
            cycle,
            nx,
            ny,
            nz,
            rest @ ..,
        ] = v.as_slice()
        else {
            unreachable!("length checked above");
        };

        let mut deer = Deer::new(DeerKind::Doe, Vec3::new(*x, 0.0, *z), *heading, 0.0);
        deer.speed = *speed;
        deer.target_speed = *target_speed;
        deer.walk_cycle = *cycle;
        deer.ground_normal = Vec3::new(*nx, *ny, *nz);

        for _ in 0..*steps as usize {
            deer.step_kinematics(&terrain, *dt);
        }

        let legs = deer.legs;
        let got = [
            deer.position.x,
            deer.position.y,
            deer.position.z,
            deer.speed,
            deer.walk_cycle,
            deer.ground_normal.x,
            deer.ground_normal.y,
            deer.ground_normal.z,
            legs.front_left.hip,
            legs.front_left.knee,
            legs.front_right.hip,
            legs.front_right.knee,
            legs.back_left.hip,
            legs.back_left.knee,
            legs.back_right.hip,
            legs.back_right.knee,
        ];
        const NAMES: [&str; 16] = [
            "x",
            "y",
            "z",
            "speed",
            "walk_cycle",
            "normal.x",
            "normal.y",
            "normal.z",
            "hip FL",
            "knee FL",
            "hip FR",
            "knee FR",
            "hip BL",
            "knee BL",
            "hip BR",
            "knee BR",
        ];
        for ((name, g), want) in NAMES.iter().zip(got).zip(rest) {
            assert!(
                (g - want).abs() <= TOLERANCE,
                "line {line_no}, {name}: got {g}, original WASM gave {want}",
            );
        }
        checked += 1;
    }
    assert!(checked > 0, "fixture is empty");
}
