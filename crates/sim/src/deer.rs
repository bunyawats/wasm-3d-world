//! Deer: body state, leg kinematics and body pose.
//!
//! - `Deer::step_kinematics` is a port of the `stepDeerKinematics` WASM kernel
//!   (func 5 in `index.html`), and is tested against it bit for bit.
//! - `Deer::update_pose` ports the neck, head, ear and tail animation that the
//!   original did in JavaScript while copying WASM memory onto the Three.js rig.
//!
//! The deer's *decisions* (when to graze, walk or trot, and where to) come in
//! milestone 4. Here, something outside sets `behaviour`, `target_speed` and
//! `heading`, and this module animates the result.

use glam::{Vec2, Vec3};

use crate::{Rng, Terrain};

/// The three kinds of deer in the herd.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeerKind {
    Stag,
    Doe,
    Fawn,
}

impl DeerKind {
    /// Overall size relative to the rig's modelled size.
    pub fn scale(self) -> f32 {
        match self {
            DeerKind::Stag => 1.08,
            DeerKind::Doe => 0.94,
            DeerKind::Fawn => 0.68,
        }
    }
}

/// What a deer is doing. Stored as a float (`state`, 0–3) in the original's WASM
/// memory; an enum makes invalid states impossible to represent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Behaviour {
    Idle,
    Grazing,
    Walking,
    Trotting,
}

/// One leg's joint angles, in radians, as rotations about the deer's sideways axis.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Leg {
    pub hip: f32,
    pub knee: f32,
}

/// All four legs. Named fields are harder to mix up than `[Leg; 4]` indices.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Legs {
    pub front_left: Leg,
    pub front_right: Leg,
    pub back_left: Leg,
    pub back_right: Leg,
}

/// Angles (radians) and offsets for the rest of the rig, recomputed every frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pose {
    /// Height of the body pivot above the hooves (bobs while walking).
    pub body_lift: f32,
    /// Rocking of the body while trotting.
    pub body_pitch: f32,
    /// Neck tilt forward: about 1.4 reaches the grass, about 0.3 is heads-up.
    pub neck_pitch: f32,
    pub head_pitch: f32,
    pub head_yaw: f32,
    pub left_ear_roll: f32,
    pub right_ear_roll: f32,
    /// Tail raised in alarm while trotting.
    pub tail_pitch: f32,
    pub tail_yaw: f32,
}

impl Default for Pose {
    /// The rig's resting pose, matching the original's initial pivot rotations.
    fn default() -> Self {
        Self {
            body_lift: BODY_HEIGHT,
            body_pitch: 0.0,
            neck_pitch: 0.35,
            head_pitch: -0.30,
            head_yaw: 0.0,
            left_ear_roll: -EAR_ROLL,
            right_ear_roll: EAR_ROLL,
            tail_pitch: 0.0,
            tail_yaw: 0.0,
        }
    }
}

/// Height of the body pivot above the ground, in rig units (before `DeerKind::scale`).
pub const BODY_HEIGHT: f32 = 1.28;
/// Resting outward tilt of the ears.
const EAR_ROLL: f32 = 0.65;
/// Speed (m/s) at which the stride reaches its full amplitude.
const FULL_STRIDE_SPEED: f32 = 2.4;

/// Phase offset between left and right legs: half a stride. The original wrote
/// `3.1415926`, which rounds to a different `f32` than `std::f32::consts::PI`, so
/// that exact literal is kept to stay bit-identical with the original kernel.
#[allow(clippy::approx_constant)] // Deliberate: see above.
const HALF_STRIDE: f32 = 3.141_592_6;
/// The back legs trail the front ones slightly.
const BACK_LEG_LAG: f32 = 0.35;

#[derive(Debug, Clone, PartialEq)]
pub struct Deer {
    pub kind: DeerKind,
    /// World position; `y` follows the terrain.
    pub position: Vec3,
    /// Facing angle around the vertical axis, in radians. 0 faces +Z.
    pub heading: f32,
    pub speed: f32,
    /// Speed the deer is easing towards.
    pub target_speed: f32,
    /// Stride phase in radians; grows with distance walked.
    pub walk_cycle: f32,
    /// Smoothed ground normal, used to tilt the body on slopes.
    pub ground_normal: Vec3,
    pub legs: Legs,

    pub behaviour: Behaviour,
    /// Seconds left in the current behaviour (used by milestone 4's brain).
    pub behaviour_timer: f32,
    /// Where the deer is heading for (used by milestone 4's brain).
    pub target: Vec2,
    /// Where to look while idle.
    pub look_yaw: f32,

    pub pose: Pose,
    /// Seconds until the next ear twitch.
    ear_twitch_timer: f32,
    /// Offsets this deer's idle animations so the herd doesn't move in lockstep.
    phase: f32,
}

impl Deer {
    /// A grazing deer at `position`, facing `heading`. `phase` desynchronises its
    /// idle animations from the rest of the herd (the original used its list index).
    pub fn new(kind: DeerKind, position: Vec3, heading: f32, phase: f32) -> Self {
        Self {
            kind,
            position,
            heading,
            speed: 0.0,
            target_speed: 0.0,
            walk_cycle: 0.0,
            ground_normal: Vec3::Y,
            legs: Legs::default(),
            behaviour: Behaviour::Grazing,
            behaviour_timer: 0.0,
            target: Vec2::new(position.x, position.z),
            look_yaw: 0.0,
            pose: Pose::default(),
            ear_twitch_timer: 0.0,
            phase,
        }
    }

    /// Places a deer at a random grassy spot near the middle of the island.
    /// Port of `spawnDeerWasm()`.
    pub fn spawn(kind: DeerKind, terrain: &Terrain, rng: &mut Rng, phase: f32) -> Self {
        let spread = terrain.world_size * 0.55;
        let mut position = Vec3::ZERO;
        // Try up to 40 random spots; keep the last one if none is ideal.
        for _ in 0..40 {
            let x = rng.range(-0.5..0.5) * spread;
            let z = rng.range(-0.5..0.5) * spread;
            let y = terrain.height(x, z);
            position = Vec3::new(x, y, z);
            if y > terrain.water_level + 0.8 && y < 9.5 {
                break;
            }
        }

        let mut deer = Deer::new(kind, position, rng.angle(), phase);
        deer.walk_cycle = rng.angle();
        deer.behaviour_timer = rng.range(2.0..7.0);
        deer.target = Vec2::new(
            position.x + rng.range(-9.0..9.0),
            position.z + rng.range(-9.0..9.0),
        );
        deer.ear_twitch_timer = rng.range(0.0..3.0);
        deer
    }

    /// Moves the deer one time step and computes its leg angles.
    ///
    /// Port of the `stepDeerKinematics` kernel. The arithmetic is written in the
    /// same order as the original so the results match exactly: floating-point
    /// maths isn't associative, so `(a * b) * c` and `a * (b * c)` can differ in
    /// the last bit.
    pub fn step_kinematics(&mut self, terrain: &Terrain, dt: f32) {
        // Ease speed towards the target speed.
        self.speed += (self.target_speed - self.speed) * (dt * 4.5);
        let speed = self.speed;

        if speed > 0.05 {
            self.position.x += sin(self.heading) * speed * dt;
            self.position.z += cos(self.heading) * speed * dt;
        }
        self.position.y = terrain.height(self.position.x, self.position.z);

        // Ease the body's tilt towards the ground's slope.
        let ground = terrain.normal(self.position.x, self.position.z);
        self.ground_normal += (ground - self.ground_normal) * (dt * 8.0);

        self.walk_cycle += dt * (speed * 2.85);
        let cycle = self.walk_cycle;
        let amplitude = (speed / FULL_STRIDE_SPEED).min(1.0);

        // Diagonal gait: each leg swings on its own phase offset. The front knees
        // bend backwards and the back hocks forwards, like a real deer's.
        self.legs = Legs {
            front_left: front_leg(cycle, amplitude),
            front_right: front_leg(cycle + HALF_STRIDE, amplitude),
            // The original precomputed HALF_STRIDE + BACK_LEG_LAG as one literal;
            // adding them here would round differently, so it's kept as written.
            back_left: back_leg(cycle + 3.491_592_6, amplitude),
            back_right: back_leg(cycle + BACK_LEG_LAG, amplitude),
        };
    }

    /// Animates the body, neck, head, ears and tail for the current behaviour.
    /// `elapsed` is the total simulation time in seconds.
    pub fn update_pose(&mut self, dt: f32, elapsed: f32, rng: &mut Rng) {
        let cycle = self.walk_cycle;
        let amplitude = (self.speed / FULL_STRIDE_SPEED).min(1.0);
        let phase = self.phase;
        // Shadow `self.pose` with a short name. It's a `&mut`, so writes go
        // straight into the deer.
        let pose = &mut self.pose;

        pose.body_lift = BODY_HEIGHT
            + cycle.sin().abs() * 0.055 * amplitude
            + (elapsed * 2.4 + phase).sin() * 0.012;
        pose.body_pitch = if self.behaviour == Behaviour::Trotting {
            cycle.sin() * 0.05 * amplitude
        } else {
            0.0
        };

        // Each behaviour has a target pose; ease towards it at its own rate.
        // `match` on a tuple with a guard (`if ...`) picks the first arm that fits.
        match self.behaviour {
            Behaviour::Grazing if self.speed < 0.2 => {
                let nibble = (elapsed * 7.5 + phase).sin() * 0.07;
                ease(&mut pose.neck_pitch, 1.38, dt * 3.2);
                ease(&mut pose.head_pitch, -0.58 + nibble, dt * 5.0);
                ease(
                    &mut pose.head_yaw,
                    (elapsed * 1.4 + phase).sin() * 0.18,
                    dt * 3.0,
                );
            }
            Behaviour::Idle => {
                ease(&mut pose.neck_pitch, 0.30, dt * 3.5);
                ease(&mut pose.head_pitch, -0.26, dt * 3.5);
                ease(&mut pose.head_yaw, self.look_yaw, dt * 2.5);
            }
            // Walking, trotting, or grazing while still slowing down.
            _ => {
                let neck = if self.behaviour == Behaviour::Trotting {
                    0.52
                } else {
                    0.42
                };
                ease(&mut pose.neck_pitch, neck + cycle.sin() * 0.05, dt * 4.5);
                ease(&mut pose.head_pitch, -0.36, dt * 4.5);
                ease(&mut pose.head_yaw, 0.0, dt * 5.0);
            }
        }

        // Every few seconds, flick the ears for a quarter of a second.
        self.ear_twitch_timer -= dt;
        if self.ear_twitch_timer <= 0.0 {
            self.ear_twitch_timer = rng.range(2.0..6.5);
        }
        let twitch = if self.ear_twitch_timer < 0.25 {
            (elapsed * 35.0).sin() * 0.35
        } else {
            0.0
        };
        pose.left_ear_roll = -EAR_ROLL + twitch;
        pose.right_ear_roll = EAR_ROLL - twitch * 0.7;

        let tail_up = if self.behaviour == Behaviour::Trotting {
            0.85
        } else {
            0.0
        };
        ease(&mut pose.tail_pitch, tail_up, dt * 5.0);
        pose.tail_yaw = (elapsed * 4.0 + phase).sin() * 0.15;
    }
}

fn front_leg(phase: f32, amplitude: f32) -> Leg {
    Leg {
        hip: 0.04 + sin(phase) * 0.48 * amplitude,
        knee: cos(phase).max(0.0) * amplitude * -0.88 - 0.04,
    }
}

fn back_leg(phase: f32, amplitude: f32) -> Leg {
    Leg {
        hip: -0.05 + sin(phase) * 0.48 * amplitude,
        knee: cos(phase).max(0.0) * amplitude * 0.72 + 0.08,
    }
}

/// Moves `value` a fraction `t` of the way to `goal`, in place.
/// Taking `&mut f32` lets one helper update any field: `ease(&mut pose.neck_pitch, ...)`.
fn ease(value: &mut f32, goal: f32, t: f32) {
    *value += (goal - *value) * t; // `*value` *dereferences*: reads or writes the f32 itself.
}

// Same trick as in `terrain.rs`: compute in f64 like JavaScript's `Math.sin`, the
// function the original WASM imported, so the kernel port matches exactly.
fn sin(v: f32) -> f32 {
    f64::from(v).sin() as f32
}

fn cos(v: f32) -> f32 {
    f64::from(v).cos() as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deer_at(x: f32, z: f32) -> Deer {
        Deer::new(DeerKind::Doe, Vec3::new(x, 0.0, z), 0.0, 0.0)
    }

    #[test]
    fn standing_deer_stays_put_but_settles_onto_ground() {
        let terrain = Terrain::default();
        let mut deer = deer_at(5.0, -3.0);
        deer.step_kinematics(&terrain, 1.0 / 60.0);
        assert_eq!((deer.position.x, deer.position.z), (5.0, -3.0));
        assert_eq!(deer.position.y, terrain.height(5.0, -3.0));
    }

    #[test]
    fn walking_deer_moves_along_its_heading() {
        let terrain = Terrain::default();
        let mut deer = deer_at(0.0, 0.0);
        deer.speed = 2.0;
        deer.target_speed = 2.0;
        // Heading 0 faces +Z, so after a second it should be about 2 m along +Z.
        for _ in 0..60 {
            deer.step_kinematics(&terrain, 1.0 / 60.0);
        }
        assert!(
            deer.position.x.abs() < 1e-4,
            "drifted sideways: {}",
            deer.position.x
        );
        assert!(
            (deer.position.z - 2.0).abs() < 1e-3,
            "z = {}",
            deer.position.z
        );
    }

    #[test]
    fn grazing_lowers_the_neck_and_trotting_raises_the_tail() {
        let mut rng = Rng::new(1);
        let mut grazer = deer_at(0.0, 0.0);
        let mut trotter = deer_at(0.0, 0.0);
        trotter.behaviour = Behaviour::Trotting;
        trotter.speed = 5.0;
        for frame in 0..120 {
            let elapsed = frame as f32 / 60.0;
            grazer.update_pose(1.0 / 60.0, elapsed, &mut rng);
            trotter.update_pose(1.0 / 60.0, elapsed, &mut rng);
        }
        assert!(
            grazer.pose.neck_pitch > 1.2,
            "neck {}",
            grazer.pose.neck_pitch
        );
        assert!(
            trotter.pose.tail_pitch > 0.8,
            "tail {}",
            trotter.pose.tail_pitch
        );
    }
}
