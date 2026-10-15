//! The deer's decision-making: what to do next, where to go, and how to steer.
//!
//! Port of the JavaScript "AI steering" half of `stepSimulationWasmAndSync()` and of
//! `pickWasmDeerTarget()` / `startleHerdWasm()` in `index.html`. The original ran it
//! in JS and only the leg kinematics in WASM; here both are plain Rust.
//!
//! This file adds more methods to `Deer`, even though `Deer` is defined in
//! `deer.rs`. Rust lets you split a type's `impl` blocks across modules of the
//! same crate, which keeps "how a deer moves" and "how a deer decides" in separate
//! files.

use std::f32::consts::{PI, TAU};

use glam::Vec2;

use crate::{Behaviour, Deer, Rng, Terrain};

/// Something deer walk around: a tree trunk or a boulder (added in milestone 6).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Obstacle {
    pub position: Vec2,
    pub radius: f32,
}

/// Walking speed (m/s).
pub const WALK_SPEED: f32 = 2.15;
/// Trotting speed (m/s).
pub const TROT_SPEED: f32 = 5.2;

/// Below this speed a deer doesn't steer (it's standing still).
const STEERING_MIN_SPEED: f32 = 0.06;
/// A deer this close to its target (m) stops and grazes.
const ARRIVAL_DISTANCE: f32 = 1.8;
/// How far ahead (m) a deer checks the ground for water or steep peaks.
const LOOK_AHEAD: f32 = 2.2;
/// Ground above this height (m) counts as too steep and rocky to walk onto.
const MAX_WALKABLE_HEIGHT: f32 = 13.5;
/// Extra personal space (m) kept around obstacles.
const OBSTACLE_MARGIN: f32 = 1.4;
/// How hard obstacles push a deer sideways.
const OBSTACLE_PUSH: f32 = 1.8;
/// Turning rate: the fraction of the heading error closed per second.
const TURN_RATE: f32 = 4.2;

impl Deer {
    /// Decides what to do this frame and steers towards it.
    ///
    /// `herd_centre` is the average position of all deer, so wandering deer drift
    /// back towards the group instead of scattering across the island.
    pub fn think(
        &mut self,
        terrain: &Terrain,
        obstacles: &[Obstacle],
        herd_centre: Vec2,
        dt: f32,
        rng: &mut Rng,
    ) {
        self.behaviour_timer -= dt;
        if self.behaviour_timer <= 0.0 {
            self.choose_behaviour(terrain, herd_centre, rng);
        }
        if self.speed > STEERING_MIN_SPEED {
            self.steer(terrain, obstacles, dt, rng);
        }
    }

    /// Rolls the dice for the next behaviour and how long it lasts.
    fn choose_behaviour(&mut self, terrain: &Terrain, herd_centre: Vec2, rng: &mut Rng) {
        let roll = rng.f32();
        // `r if r < 0.38` binds the value to `r` and adds a *guard* condition. Arms
        // are tried in order, so each one only sees values the earlier ones rejected,
        // which turns cumulative thresholds into probabilities: 38% graze, 18% idle,
        // 34% walk, 10% trot.
        match roll {
            r if r < 0.38 => self.start(Behaviour::Grazing, 0.0, rng.range(4.0..10.0)),
            r if r < 0.56 => {
                self.start(Behaviour::Idle, 0.0, rng.range(2.5..6.5));
                self.look_yaw = rng.range(-0.45..0.45);
            }
            r if r < 0.90 => {
                self.start(Behaviour::Walking, WALK_SPEED, rng.range(5.0..12.0));
                self.pick_target(terrain, herd_centre, 18.0, rng);
            }
            _ => {
                self.start(Behaviour::Trotting, TROT_SPEED, rng.range(3.5..7.0));
                self.pick_target(terrain, herd_centre, 32.0, rng);
            }
        }
    }

    fn start(&mut self, behaviour: Behaviour, target_speed: f32, duration: f32) {
        self.behaviour = behaviour;
        self.target_speed = target_speed;
        self.behaviour_timer = duration;
    }

    /// Picks a random walkable spot within `radius` of a point between this deer
    /// and the herd centre. Keeps the old target if 15 tries all fail.
    fn pick_target(&mut self, terrain: &Terrain, herd_centre: Vec2, radius: f32, rng: &mut Rng) {
        let here = Vec2::new(self.position.x, self.position.z);
        // 35% of the way towards the herd: wanderers are pulled back to the group.
        let anchor = here.lerp(herd_centre, 0.35);
        let limit = terrain.world_size * 0.4;

        for _ in 0..15 {
            let candidate =
                anchor + Vec2::new(rng.range(-radius..radius), rng.range(-radius..radius));
            let height = terrain.height(candidate.x, candidate.y);
            let on_dry_meadow = height > terrain.water_level + 0.7 && height < 11.5;
            let inside_island = candidate.x.abs() < limit && candidate.y.abs() < limit;
            if on_dry_meadow && inside_island {
                self.target = candidate;
                return;
            }
        }
    }

    /// Turns towards the target, around obstacles, and away from water.
    fn steer(&mut self, terrain: &Terrain, obstacles: &[Obstacle], dt: f32, rng: &mut Rng) {
        let here = Vec2::new(self.position.x, self.position.z);
        let to_target = self.target - here;
        let distance = to_target.length();

        if distance < ARRIVAL_DISTANCE {
            self.start(Behaviour::Grazing, 0.0, rng.range(4.0..8.0));
            return;
        }
        let mut direction = to_target / distance;

        // Each nearby obstacle pushes the deer away, harder the closer it is.
        for obstacle in obstacles {
            let away = here - obstacle.position;
            let min_distance = obstacle.radius + OBSTACLE_MARGIN;
            let distance_sq = away.length_squared();
            if distance_sq < min_distance * min_distance && distance_sq > 0.001 {
                let d = distance_sq.sqrt();
                direction += away / d * (min_distance - d) * OBSTACLE_PUSH;
            }
        }

        // Peek ahead along the current heading. About to wade into the sea or climb
        // the bare peaks? Turn back towards the middle of the island instead.
        let ahead = here + Vec2::new(self.heading.sin(), self.heading.cos()) * LOOK_AHEAD;
        let ahead_height = terrain.height(ahead.x, ahead.y);
        if ahead_height < terrain.water_level + 0.45 || ahead_height > MAX_WALKABLE_HEIGHT {
            direction = -here * 0.1;
        }

        // Heading 0 faces +Z, so the angle is atan2(x, z), not the usual atan2(y, x).
        let target_heading = direction.x.atan2(direction.y);
        let turn = wrap_angle(target_heading - self.heading);
        self.heading += turn * (dt * TURN_RATE).min(1.0);
    }

    /// Bolts in a random direction at a trot. Port of `startleHerdWasm()`.
    pub fn startle(&mut self, rng: &mut Rng) {
        self.start(Behaviour::Trotting, TROT_SPEED, rng.range(4.5..7.5));
        let angle = rng.angle();
        let distance = rng.range(22.0..40.0);
        let flee = Vec2::new(self.position.x, self.position.z)
            + Vec2::new(angle.cos(), angle.sin()) * distance;
        self.target = flee.clamp(Vec2::splat(-55.0), Vec2::splat(55.0));
    }
}

/// Wraps an angle into `-π..=π`, so a deer always turns the short way round.
/// (The original used two `while` loops; `rem_euclid` does it in one step.)
fn wrap_angle(angle: f32) -> f32 {
    (angle + PI).rem_euclid(TAU) - PI
}

#[cfg(test)]
mod tests {
    use glam::Vec3;

    use super::*;
    use crate::DeerKind;

    /// A walking deer at `(x, z)` facing `heading`, steering towards `target`.
    fn walker(terrain: &Terrain, x: f32, z: f32, heading: f32, target: Vec2) -> Deer {
        let mut deer = Deer::new(
            DeerKind::Doe,
            Vec3::new(x, terrain.height(x, z), z),
            heading,
            0.0,
        );
        deer.behaviour = Behaviour::Walking;
        deer.speed = WALK_SPEED;
        deer.target_speed = WALK_SPEED;
        deer.behaviour_timer = 100.0;
        deer.target = target;
        deer
    }

    #[test]
    fn wrap_angle_takes_the_short_way_round() {
        assert!((wrap_angle(3.0 * PI / 2.0) - (-PI / 2.0)).abs() < 1e-5);
        assert!((wrap_angle(-3.0 * PI / 2.0) - (PI / 2.0)).abs() < 1e-5);
        assert!((wrap_angle(0.5) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn deer_turns_towards_its_target() {
        let terrain = Terrain::default();
        let mut rng = Rng::new(1);
        // Facing +Z, with the target off to the +X side.
        let mut deer = walker(&terrain, 0.0, 0.0, 0.0, Vec2::new(10.0, 0.0));
        for _ in 0..60 {
            deer.think(&terrain, &[], Vec2::ZERO, 1.0 / 60.0, &mut rng);
        }
        assert!(
            (deer.heading - PI / 2.0).abs() < 0.05,
            "heading {}",
            deer.heading
        );
    }

    #[test]
    fn arriving_at_the_target_starts_grazing() {
        let terrain = Terrain::default();
        let mut rng = Rng::new(1);
        let mut deer = walker(&terrain, 0.0, 0.0, 0.0, Vec2::new(1.0, 0.0));
        deer.think(&terrain, &[], Vec2::ZERO, 1.0 / 60.0, &mut rng);
        assert_eq!(deer.behaviour, Behaviour::Grazing);
        assert_eq!(deer.target_speed, 0.0);
    }

    #[test]
    fn obstacle_in_the_way_deflects_the_deer() {
        let terrain = Terrain::default();
        let mut rng = Rng::new(1);
        let target = Vec2::new(0.0, 20.0);
        let mut clear = walker(&terrain, 0.0, 0.0, 0.0, target);
        let mut blocked = clear.clone();
        // A boulder just ahead and slightly to the left (+X) of the deer.
        let boulder = [Obstacle {
            position: Vec2::new(0.5, 1.0),
            radius: 1.0,
        }];
        clear.think(&terrain, &[], Vec2::ZERO, 1.0 / 60.0, &mut rng);
        blocked.think(&terrain, &boulder, Vec2::ZERO, 1.0 / 60.0, &mut rng);
        assert_eq!(clear.heading, 0.0, "nothing in the way, so no turn");
        assert!(
            blocked.heading < 0.0,
            "should veer right, away from the boulder"
        );
    }

    #[test]
    fn startle_sends_the_deer_trotting_somewhere_new() {
        let terrain = Terrain::default();
        let mut rng = Rng::new(3);
        let mut deer = walker(&terrain, 0.0, 0.0, 0.0, Vec2::ZERO);
        deer.startle(&mut rng);
        assert_eq!(deer.behaviour, Behaviour::Trotting);
        assert_eq!(deer.target_speed, TROT_SPEED);
        assert!(
            deer.target.length() > 20.0,
            "target {} too close",
            deer.target
        );
    }
}
