//! Birds: flight kinematics, wing animation and flocking.
//!
//! - `Bird::step_kinematics` ports the `stepBirdFlock` WASM kernel (func 6 in
//!   `index.html`) and is tested against it bit for bit.
//! - `Bird::steer` ports the JavaScript "boid" steering that ran before the kernel
//!   each frame: circle the flock's orbit centre, keep a safe altitude, and bank
//!   into turns.

use glam::{Vec2, Vec3};

use crate::{Rng, Terrain};

/// How a bird is flying. The original stored this as a float, 0.0 or 1.0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flight {
    /// Beating its wings and climbing gently.
    Flapping,
    /// Wings held in a shallow V, sinking slowly on a thermal.
    Gliding,
}

impl Flight {
    /// The other mode. `self` by value is fine: `Flight` is `Copy`.
    fn toggled(self) -> Self {
        match self {
            Flight::Flapping => Flight::Gliding,
            Flight::Gliding => Flight::Flapping,
        }
    }
}

/// Wing beats per second, in radians of flap phase.
const FLAP_RATE: f32 = 11.5;
/// Cruising speeds (m/s). Gliding is faster: the bird trades height for speed.
const FLAP_SPEED: f32 = 10.5;
const GLIDE_SPEED: f32 = 12.2;
/// Altitude band (m) the flock stays inside.
const MIN_ALTITUDE: f32 = 18.0;
const MIN_HEIGHT_ABOVE_GROUND: f32 = 12.0;
const MAX_ALTITUDE: f32 = 46.0;
/// How quickly steering changes the velocity.
const STEER_RESPONSE: f32 = 4.5;
/// Maximum bank angle (radians) in a turn.
const MAX_BANK: f32 = 0.75;
/// Most birds the original's memory had room for.
pub const MAX_BIRDS: usize = 60;

#[derive(Debug, Clone, PartialEq)]
pub struct Bird {
    pub position: Vec3,
    pub velocity: Vec3,
    pub flight: Flight,
    /// Seconds until the bird switches between flapping and gliding.
    pub flight_timer: f32,
    /// Wingbeat phase in radians.
    pub flap_phase: f32,
    /// The point the flock circles around.
    pub orbit_centre: Vec3,
    /// Roll angle in a turn (radians); positive rolls left.
    pub bank: f32,
    /// Inner wing (shoulder) angle, radians, about the bird's forward axis.
    pub inner_wing: f32,
    /// Outer wing (wrist) angle relative to the inner wing.
    pub outer_wing: f32,
    /// Small vertical body movement from each wingbeat.
    pub body_bob: f32,
    /// Overall size relative to the modelled rig.
    pub scale: f32,
}

impl Bird {
    pub fn new(position: Vec3, velocity: Vec3, orbit_centre: Vec3) -> Self {
        Self {
            position,
            velocity,
            flight: Flight::Flapping,
            flight_timer: 0.0,
            flap_phase: 0.0,
            orbit_centre,
            bank: 0.0,
            inner_wing: 0.0,
            outer_wing: 0.0,
            body_bob: 0.0,
            scale: 1.0,
        }
    }

    /// Moves the bird along its velocity and animates its wings.
    /// Port of the `stepBirdFlock` kernel, in the same operation order.
    pub fn step_kinematics(&mut self, dt: f32) {
        // Written per component (not `self.position += self.velocity * dt`) only to
        // mirror the original exactly. The vector form does the same arithmetic.
        self.position.x += self.velocity.x * dt;
        self.position.y += self.velocity.y * dt;
        self.position.z += self.velocity.z * dt;

        match self.flight {
            Flight::Flapping => {
                self.flap_phase += dt * FLAP_RATE;
                let s = sin(self.flap_phase);
                let c = cos(self.flap_phase);
                self.inner_wing = s * 0.52 + 0.08;
                // The wrist folds on the upstroke: `|cos|` peaks between beats.
                self.outer_wing = s * 0.35 - c.abs() * 0.25;
                self.body_bob = s * -0.06;
            }
            Flight::Gliding => {
                // A fixed dihedral "V": inner wings up, tips slightly down.
                self.inner_wing = 0.14;
                self.outer_wing = -0.08;
                self.body_bob = 0.0;
            }
        }
    }

    /// Steers the bird for this frame: circle the orbit centre, keep within the
    /// altitude band, hold cruising speed and bank into the turn.
    pub fn steer(&mut self, terrain: &Terrain, dt: f32, rng: &mut Rng) {
        self.flight_timer -= dt;
        if self.flight_timer <= 0.0 {
            self.flight = self.flight.toggled();
            self.flight_timer = rng.range(2.5..6.5);
        }

        // Pull towards the orbit centre, plus a sideways (tangent) push that turns
        // straight-line approach into circling. Rotating a vector by 90° on the
        // ground plane is just `(x, z) -> (-z, x)`.
        let to_centre = Vec2::new(
            self.orbit_centre.x - self.position.x,
            self.orbit_centre.z - self.position.z,
        );
        let distance = non_zero(to_centre.length());
        let tangent = Vec2::new(-to_centre.y, to_centre.x) / distance;
        let horizontal = to_centre * 0.035 + tangent * 1.4;

        let mut climb = match self.flight {
            Flight::Flapping => 0.55,
            Flight::Gliding => -0.32,
        };
        let ground = terrain.height(self.position.x, self.position.z);
        if self.position.y < (ground + MIN_HEIGHT_ABOVE_GROUND).max(MIN_ALTITUDE) {
            // Too low: flap hard and climb.
            self.flight = Flight::Flapping;
            climb = 2.2;
        } else if self.position.y > MAX_ALTITUDE {
            // Too high: glide down.
            self.flight = Flight::Gliding;
            climb = -1.5;
        }

        let steering = Vec3::new(horizontal.x, climb, horizontal.y);
        let previous = self.velocity;
        let speed = match self.flight {
            Flight::Flapping => FLAP_SPEED,
            Flight::Gliding => GLIDE_SPEED,
        };
        let steered = previous + steering * (dt * STEER_RESPONSE);
        // Keep the direction, but always fly at cruising speed.
        self.velocity = steered / non_zero(steered.length()) * speed;

        // Bank into the turn. The 2D cross product of the old and new horizontal
        // velocity says how sharply (and which way) the bird is turning.
        let turn = (previous.x * self.velocity.z - previous.z * self.velocity.x) / (speed * speed);
        let target_bank = (-turn * 18.0).clamp(-MAX_BANK, MAX_BANK);
        self.bank += (target_bank - self.bank) * (dt * 5.0);
    }
}

/// `value`, or 1.0 if it's zero: avoids dividing by zero, like the original's
/// `Math.hypot(...) || 1`.
fn non_zero(value: f32) -> f32 {
    if value == 0.0 { 1.0 } else { value }
}

// JavaScript's `Math.sin`/`Math.cos` work in f64; the WASM kernel imported them.
fn sin(v: f32) -> f32 {
    f64::from(v).sin() as f32
}

fn cos(v: f32) -> f32 {
    f64::from(v).cos() as f32
}

/// Every bird in the sky, plus the generator for their randomness.
#[derive(Debug, Clone)]
pub struct Flocks {
    pub birds: Vec<Bird>,
    pub rng: Rng,
}

impl Flocks {
    /// The original's starting sky: one flock of 14 birds.
    pub fn initial(seed: u64) -> Self {
        let mut flocks = Self {
            birds: Vec::new(),
            rng: Rng::new(seed),
        };
        flocks.spawn_flock(14);
        flocks
    }

    /// Adds up to `count` birds circling a new random point above the island
    /// (fewer if the sky is full). Port of `spawnBirdFlockWasm()`.
    /// Returns how many were added.
    pub fn spawn_flock(&mut self, count: usize) -> usize {
        let rng = &mut self.rng;
        let centre = Vec3::new(
            rng.range(-27.5..27.5),
            rng.range(26.0..40.0),
            rng.range(-27.5..27.5),
        );

        let room = MAX_BIRDS.saturating_sub(self.birds.len());
        let count = count.min(room);
        for _ in 0..count {
            let position = centre
                + Vec3::new(
                    rng.range(-7.0..7.0),
                    rng.range(-2.5..2.5),
                    rng.range(-7.0..7.0),
                );
            let velocity = Vec3::new(rng.range(-4.5..4.5), 0.0, rng.range(-4.5..4.5));
            let mut bird = Bird::new(position, velocity, centre);
            bird.scale = rng.range(0.85..1.10);
            bird.flap_phase = rng.angle();
            bird.flight_timer = rng.range(2.0..6.0);
            self.birds.push(bird);
        }
        count
    }

    /// Advances every bird by `dt` seconds: steer first, then fly.
    pub fn step(&mut self, terrain: &Terrain, dt: f32) {
        for bird in &mut self.birds {
            bird.steer(terrain, dt, &mut self.rng);
            bird.step_kinematics(dt);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME: f32 = 1.0 / 60.0;

    #[test]
    fn gliding_holds_a_fixed_v() {
        let mut bird = Bird::new(Vec3::new(0.0, 30.0, 0.0), Vec3::X, Vec3::ZERO);
        bird.flight = Flight::Gliding;
        let phase_before = bird.flap_phase;
        bird.step_kinematics(FRAME);
        assert_eq!(
            (bird.inner_wing, bird.outer_wing, bird.body_bob),
            (0.14, -0.08, 0.0)
        );
        assert_eq!(
            bird.flap_phase, phase_before,
            "gliding birds don't beat their wings"
        );
    }

    #[test]
    fn steering_keeps_cruising_speed() {
        let terrain = Terrain::default();
        let mut rng = Rng::new(1);
        let mut bird = Bird::new(
            Vec3::new(10.0, 30.0, 0.0),
            Vec3::new(0.0, 0.0, 3.0),
            Vec3::ZERO,
        );
        bird.flight_timer = 100.0;
        bird.steer(&terrain, FRAME, &mut rng);
        assert!((bird.velocity.length() - FLAP_SPEED).abs() < 1e-4);
    }

    #[test]
    fn low_birds_climb_and_high_birds_descend() {
        let terrain = Terrain::default();
        let mut rng = Rng::new(1);
        let mut low = Bird::new(Vec3::new(0.0, 5.0, 0.0), Vec3::X, Vec3::ZERO);
        let mut high = Bird::new(Vec3::new(0.0, 60.0, 0.0), Vec3::X, Vec3::ZERO);
        low.flight_timer = 100.0;
        high.flight_timer = 100.0;
        for _ in 0..30 {
            low.steer(&terrain, FRAME, &mut rng);
            high.steer(&terrain, FRAME, &mut rng);
        }
        assert!(low.velocity.y > 0.0 && low.flight == Flight::Flapping);
        assert!(high.velocity.y < 0.0 && high.flight == Flight::Gliding);
    }

    /// Property test: over five simulated minutes, the flock never hits the ground
    /// and never drifts far from its orbit.
    #[test]
    fn flock_circles_safely_for_five_minutes() {
        let terrain = Terrain::default();
        for seed in 0..5 {
            let mut flocks = Flocks::initial(seed);
            for frame in 0..(5 * 60 * 60) {
                flocks.step(&terrain, FRAME);
                for bird in &flocks.birds {
                    let ground = terrain.height(bird.position.x, bird.position.z);
                    let clearance = bird.position.y - ground;
                    let drift = (bird.position - bird.orbit_centre).length();
                    assert!(
                        clearance > 3.0,
                        "seed {seed} frame {frame}: bird {clearance} m above ground"
                    );
                    assert!(
                        drift < 80.0,
                        "seed {seed} frame {frame}: bird {drift} m from its orbit"
                    );
                }
            }
        }
    }

    #[test]
    fn sky_has_room_for_sixty_birds() {
        let mut flocks = Flocks::initial(3);
        while flocks.spawn_flock(4) > 0 {}
        assert_eq!(flocks.birds.len(), MAX_BIRDS);
    }
}
