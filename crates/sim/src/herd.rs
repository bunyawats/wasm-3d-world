//! The herd: every deer, the obstacles they avoid, and the random generator that
//! drives their choices.

use glam::Vec2;

use crate::{Deer, DeerKind, Obstacle, Rng, Terrain};

/// The starting herd, same as `spawnInitialWildlifeWasm()` in the original.
const INITIAL_HERD: [DeerKind; 9] = [
    DeerKind::Stag,
    DeerKind::Doe,
    DeerKind::Doe,
    DeerKind::Stag,
    DeerKind::Fawn,
    DeerKind::Doe,
    DeerKind::Stag,
    DeerKind::Doe,
    DeerKind::Fawn,
];

/// Most deer the original's memory had room for.
pub const MAX_DEER: usize = 60;

#[derive(Debug, Clone)]
pub struct Herd {
    pub deer: Vec<Deer>,
    /// Trees and rocks to walk around. Empty until milestone 6 adds them.
    pub obstacles: Vec<Obstacle>,
    pub rng: Rng,
}

impl Herd {
    /// The original nine-deer herd, placed randomly from `seed`.
    pub fn initial(terrain: &Terrain, seed: u64) -> Self {
        let mut rng = Rng::new(seed);
        // `iter().enumerate()` pairs each item with its index; `map` turns each
        // pair into a `Deer`; `collect` gathers them into a `Vec<Deer>`. The type
        // annotation on `deer` tells `collect` which collection to build.
        let deer: Vec<Deer> = INITIAL_HERD
            .iter()
            .enumerate()
            .map(|(i, &kind)| Deer::spawn(kind, terrain, &mut rng, i as f32))
            .collect();
        Self {
            deer,
            obstacles: Vec::new(),
            rng,
        }
    }

    /// Average position of the herd on the ground plane (`Vec2::ZERO` if empty).
    pub fn centre(&self) -> Vec2 {
        if self.deer.is_empty() {
            return Vec2::ZERO;
        }
        // `map` then `sum`: `Vec2` implements `Sum`, so this adds the vectors up.
        let total: Vec2 = self
            .deer
            .iter()
            .map(|d| Vec2::new(d.position.x, d.position.z))
            .sum();
        total / self.deer.len() as f32
    }

    /// Advances every deer by `dt` seconds. `elapsed` is the total time so far.
    pub fn step(&mut self, terrain: &Terrain, dt: f32, elapsed: f32) {
        // Measured once, before anyone moves, like the original.
        let centre = self.centre();

        // This borrows `self.deer` mutably, and `self.obstacles` and `self.rng` at
        // the same time. The borrow checker allows it because they're *different
        // fields*. Calling a `&mut self` method inside this loop would not be
        // allowed, because that would borrow all of `self` a second time.
        for deer in &mut self.deer {
            deer.think(terrain, &self.obstacles, centre, dt, &mut self.rng);
            deer.step_kinematics(terrain, dt);
            deer.update_pose(dt, elapsed, &mut self.rng);
        }
    }

    /// Adds one deer at a random grassy spot. `kind: None` picks a random kind.
    /// Returns the new deer's index, or `None` if the herd is already full.
    pub fn spawn(&mut self, terrain: &Terrain, kind: Option<DeerKind>) -> Option<usize> {
        if self.deer.len() >= MAX_DEER {
            return None;
        }
        // `unwrap_or_else` runs the closure only when `kind` is `None`.
        let kind = kind.unwrap_or_else(|| {
            *self
                .rng
                .pick(&[DeerKind::Stag, DeerKind::Doe, DeerKind::Fawn])
        });
        let id = self.deer.len();
        self.deer
            .push(Deer::spawn(kind, terrain, &mut self.rng, id as f32));
        Some(id)
    }

    /// Startles the whole herd into a trot. Port of `startleHerdWasm()`.
    pub fn startle(&mut self) {
        for deer in &mut self.deer {
            deer.startle(&mut self.rng);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Behaviour, Scenery, scenery::DEFAULT_TREE_COUNT};

    const FRAME: f32 = 1.0 / 60.0;

    #[test]
    fn initial_herd_spawns_on_dry_land() {
        let terrain = Terrain::default();
        let herd = Herd::initial(&terrain, 99);
        assert_eq!(herd.deer.len(), INITIAL_HERD.len());
        for deer in &herd.deer {
            assert!(
                deer.position.y > terrain.water_level,
                "{:?} spawned underwater at {}",
                deer.kind,
                deer.position
            );
        }
    }

    #[test]
    fn herd_stops_growing_at_the_cap() {
        let terrain = Terrain::default();
        let mut herd = Herd::initial(&terrain, 1);
        while herd.spawn(&terrain, None).is_some() {}
        assert_eq!(herd.deer.len(), MAX_DEER);
    }

    #[test]
    fn same_seed_gives_same_herd() {
        let terrain = Terrain::default();
        let a = Herd::initial(&terrain, 5);
        let b = Herd::initial(&terrain, 5);
        assert_eq!(a.deer, b.deer);
    }

    /// The big behavioural check: ten simulated minutes, several seeds, startled
    /// every 30 seconds. Deer must stay on the island and never swim out to sea.
    ///
    /// They *can* get their hooves wet. Faithful to the original: when a deer sees
    /// water ahead it turns towards the island's centre, and near the inland bay that
    /// path crosses the shallows. So the test allows wading, but not swimming out to sea.
    #[test]
    fn herd_stays_on_the_island_for_ten_minutes() {
        const MAX_WADING_DEPTH: f32 = 1.0;
        let terrain = Terrain::default();
        let half_world = terrain.world_size / 2.0;

        for seed in 0..5 {
            let mut herd = Herd::initial(&terrain, seed);
            // With the real trees and rocks to walk around.
            herd.obstacles = Scenery::generate(&terrain, DEFAULT_TREE_COUNT, seed).obstacles();
            for frame in 0..(10 * 60 * 60) {
                if frame % (30 * 60) == 0 {
                    herd.startle();
                }
                herd.step(&terrain, FRAME, frame as f32 * FRAME);

                for deer in &herd.deer {
                    let depth = terrain.water_level - deer.position.y;
                    assert!(
                        depth < MAX_WADING_DEPTH,
                        "seed {seed}, frame {frame}: deer at {} is {depth} m underwater",
                        deer.position
                    );
                    assert!(
                        deer.position.x.abs() < half_world && deer.position.z.abs() < half_world,
                        "seed {seed}, frame {frame}: deer at {} left the world",
                        deer.position
                    );
                }
            }
        }
    }

    #[test]
    fn herd_does_a_bit_of_everything() {
        let terrain = Terrain::default();
        let mut herd = Herd::initial(&terrain, 11);
        // Count frames spent in each behaviour across the whole herd.
        let mut seen = [0usize; 4];
        for frame in 0..(3 * 60 * 60) {
            herd.step(&terrain, FRAME, frame as f32 * FRAME);
            for deer in &herd.deer {
                let index = match deer.behaviour {
                    Behaviour::Idle => 0,
                    Behaviour::Grazing => 1,
                    Behaviour::Walking => 2,
                    Behaviour::Trotting => 3,
                };
                seen[index] += 1;
            }
        }
        assert!(
            seen.iter().all(|&n| n > 0),
            "some behaviour never happened: {seen:?}"
        );
    }
}
