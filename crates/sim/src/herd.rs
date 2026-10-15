//! The herd: every deer plus the random generator that drives them.

use crate::{Deer, DeerKind, Rng, Terrain};

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

#[derive(Debug, Clone)]
pub struct Herd {
    pub deer: Vec<Deer>,
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
        Self { deer, rng }
    }

    /// Advances every deer by `dt` seconds. `elapsed` is the total time so far.
    pub fn step(&mut self, terrain: &Terrain, dt: f32, elapsed: f32) {
        // This borrows `self.deer` mutably (to change each deer) and `self.rng`
        // mutably at the same time. The borrow checker allows it because they are
        // *different fields*. Calling a `&mut self` method inside the loop would
        // not be allowed, since that would borrow all of `self` again.
        for deer in &mut self.deer {
            deer.step_kinematics(terrain, dt);
            deer.update_pose(dt, elapsed, &mut self.rng);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn same_seed_gives_same_herd() {
        let terrain = Terrain::default();
        let a = Herd::initial(&terrain, 5);
        let b = Herd::initial(&terrain, 5);
        assert_eq!(a.deer, b.deer);
    }
}
