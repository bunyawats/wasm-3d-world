//! Parity test: bird kinematics must match the original `stepBirdFlock` WASM.
//!
//! The fixture comes from `node tools/dump_bird_reference.mjs`.

use glam::Vec3;
use sim::{Bird, Flight};

const REFERENCE: &str = include_str!("fixtures/bird_kinematics_reference.txt");
const TOLERANCE: f32 = 1e-5;

#[test]
fn kinematics_match_original_wasm() {
    let mut checked = 0;
    for (line_no, line) in REFERENCE.lines().enumerate() {
        let v: Vec<f32> = line
            .split_whitespace()
            .map(|f| f.parse().expect("fixture field should be a number"))
            .collect();

        let [
            dt,
            steps,
            x,
            y,
            z,
            vx,
            vy,
            vz,
            phase,
            gliding,
            expected @ ..,
        ] = v.as_slice()
        else {
            panic!("line {line_no}: wrong number of fields ({})", v.len());
        };
        assert_eq!(expected.len(), 7, "line {line_no}: wrong number of outputs");

        let mut bird = Bird::new(Vec3::new(*x, *y, *z), Vec3::new(*vx, *vy, *vz), Vec3::ZERO);
        bird.flap_phase = *phase;
        bird.flight = if *gliding > 0.5 {
            Flight::Gliding
        } else {
            Flight::Flapping
        };

        for _ in 0..*steps as usize {
            bird.step_kinematics(*dt);
        }

        let got = [
            bird.position.x,
            bird.position.y,
            bird.position.z,
            bird.flap_phase,
            bird.inner_wing,
            bird.outer_wing,
            bird.body_bob,
        ];
        let names = [
            "x",
            "y",
            "z",
            "flap_phase",
            "inner_wing",
            "outer_wing",
            "body_bob",
        ];
        for ((name, g), want) in names.iter().zip(got).zip(expected) {
            assert!(
                (g - want).abs() <= TOLERANCE,
                "line {line_no}, {name}: got {g}, original WASM gave {want}"
            );
        }
        checked += 1;
    }
    assert!(checked > 0, "fixture is empty");
}
