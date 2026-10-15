//! Where trees, rocks, grass, crystals and clouds go.
//!
//! Port of `populateFloraAndRocks()` and `generateClouds()` in `index.html`. This
//! module only decides *placement* (positions, sizes, rotations); the renderer
//! builds the meshes. Trees and rocks also become `Obstacle`s that deer walk around.

use glam::{Vec2, Vec3};

use crate::{Obstacle, Rng, Terrain};

/// The original's default tree count (`config.treeDensity`).
pub const DEFAULT_TREE_COUNT: usize = 140;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeKind {
    /// Four stacked cones.
    Pine,
    /// A cluster of round leafy puffs.
    Oak,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tree {
    pub kind: TreeKind,
    /// Base of the trunk, sunk 10 cm into the ground so it never floats.
    pub position: Vec3,
    pub scale: f32,
    /// Rotation about the vertical axis, so trees don't all look identical.
    pub yaw: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rock {
    pub position: Vec3,
    /// Non-uniform scale squashes one boulder shape into many different rocks.
    pub scale: Vec3,
    /// Euler angles (radians), applied in X, then Y, then Z order.
    pub rotation: Vec3,
}

/// One blade of grass within a tuft.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Blade {
    /// Offset from the tuft's centre on the ground plane.
    pub offset: Vec2,
    /// Euler angles (radians): a small random lean and a random facing.
    pub rotation: Vec3,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GrassTuft {
    pub position: Vec3,
    pub blades: [Blade; 4],
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Crystal {
    /// Resting position; the crystal bobs above and below it.
    pub position: Vec3,
    pub scale: Vec3,
    pub rotation: Vec3,
    /// Offsets the bobbing so crystals don't move in unison.
    pub phase: f32,
}

impl Crystal {
    /// Where the crystal is, and how far it has spun, `elapsed` seconds in.
    /// Returns `(position, extra_yaw)`.
    pub fn animated(&self, elapsed: f32) -> (Vec3, f32) {
        let bob = (elapsed * 2.0 + self.phase).sin() * 0.25;
        (self.position + Vec3::Y * bob, elapsed * 0.6)
    }
}

/// One puff of a cloud.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Puff {
    pub offset: Vec3,
    pub radius: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Cloud {
    pub position: Vec3,
    /// A cloud is several flattened puffs side by side. The number varies, so
    /// this is a `Vec` rather than a fixed-size array.
    pub puffs: Vec<Puff>,
    /// Drift speed along +X, in m/s.
    pub drift_speed: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Scenery {
    pub trees: Vec<Tree>,
    pub rocks: Vec<Rock>,
    pub grass: Vec<GrassTuft>,
    pub crystals: Vec<Crystal>,
    pub clouds: Vec<Cloud>,
}

impl Scenery {
    /// Scatters everything over `terrain`, reproducibly from `seed`.
    pub fn generate(terrain: &Terrain, tree_count: usize, seed: u64) -> Self {
        let mut rng = Rng::new(seed);
        // Each helper takes `&mut rng`, so they draw from one shared sequence.
        Self {
            trees: place_trees(terrain, tree_count, &mut rng),
            rocks: place_rocks(terrain, &mut rng),
            grass: place_grass(terrain, &mut rng),
            crystals: place_crystals(terrain, &mut rng),
            clouds: place_clouds(terrain, &mut rng),
        }
    }

    /// Everything a deer should walk around: tree trunks and boulders.
    pub fn obstacles(&self) -> Vec<Obstacle> {
        let trees = self.trees.iter().map(|tree| Obstacle {
            position: Vec2::new(tree.position.x, tree.position.z),
            radius: 1.6 * tree.scale,
        });
        let rocks = self.rocks.iter().map(|rock| Obstacle {
            position: Vec2::new(rock.position.x, rock.position.z),
            radius: rock.scale.x.max(rock.scale.z) * 0.9,
        });
        // `chain` joins two iterators end to end. Nothing is computed until
        // `collect` pulls the values through: iterators are *lazy*.
        trees.chain(rocks).collect()
    }

    /// Blows the clouds along. Each one wraps back to the far side once it drifts
    /// past the edge of the world.
    pub fn drift_clouds(&mut self, terrain: &Terrain, dt: f32) {
        let edge = terrain.world_size * 0.85;
        for cloud in &mut self.clouds {
            cloud.position.x += dt * cloud.drift_speed;
            if cloud.position.x > edge {
                cloud.position.x = -edge;
            }
        }
    }
}

/// A random point in a square of side `fraction * world_size` around the origin.
fn random_spot(terrain: &Terrain, fraction: f32, rng: &mut Rng) -> Vec2 {
    let half = terrain.world_size * fraction / 2.0;
    Vec2::new(rng.range(-half..half), rng.range(-half..half))
}

/// A small random lean (up to ±0.2 rad) with a random facing.
fn random_lean(rng: &mut Rng) -> Vec3 {
    Vec3::new(
        rng.range(-0.2..0.2),
        rng.range(0.0..std::f32::consts::PI),
        rng.range(-0.2..0.2),
    )
}

fn place_trees(terrain: &Terrain, count: usize, rng: &mut Rng) -> Vec<Tree> {
    let mut trees = Vec::with_capacity(count);
    // Give up after 2.5 tries per tree; some spots are always unsuitable.
    let attempts = count * 5 / 2;
    for _ in 0..attempts {
        if trees.len() >= count {
            break;
        }
        let spot = random_spot(terrain, 0.82, rng);
        // Keep the central meadow mostly clear for the herd. `&&` short-circuits:
        // the random draw only happens for spots inside the meadow.
        if spot.length_squared() < 180.0 && rng.f32() < 0.7 {
            continue;
        }
        let y = terrain.height(spot.x, spot.y);
        let flat_enough = terrain.normal(spot.x, spot.y).y > 0.82;
        if y > terrain.water_level + 0.9 && y < 13.0 && flat_enough {
            let kind = if rng.f32() < 0.28 && y < 7.5 {
                TreeKind::Oak
            } else {
                TreeKind::Pine
            };
            trees.push(Tree {
                kind,
                position: Vec3::new(spot.x, y - 0.1, spot.y),
                scale: rng.range(0.75..1.4),
                yaw: rng.angle(),
            });
        }
    }
    trees
}

fn place_rocks(terrain: &Terrain, rng: &mut Rng) -> Vec<Rock> {
    let mut rocks = Vec::new();
    for _ in 0..45 {
        let spot = random_spot(terrain, 0.82, rng);
        let y = terrain.height(spot.x, spot.y);
        if y > terrain.water_level - 0.2 && y < 15.0 {
            let scale = Vec3::new(
                rng.range(0.6..2.0),
                rng.range(0.4..1.3),
                rng.range(0.6..1.9),
            );
            rocks.push(Rock {
                // Half-buried: lift by only 30% of its height.
                position: Vec3::new(spot.x, y + scale.y * 0.3, spot.y),
                scale,
                rotation: Vec3::new(rng.f32(), rng.range(0.0..std::f32::consts::PI), rng.f32()),
            });
        }
    }
    rocks
}

fn place_grass(terrain: &Terrain, rng: &mut Rng) -> Vec<GrassTuft> {
    let mut grass = Vec::new();
    for _ in 0..220 {
        let spot = random_spot(terrain, 0.75, rng);
        let y = terrain.height(spot.x, spot.y);
        if y > terrain.water_level + 0.5 && y < 9.0 {
            // `std::array::from_fn` builds a fixed-size array by calling the
            // closure once per index (the index is ignored here: `|_|`).
            let blades = std::array::from_fn(|_| Blade {
                offset: Vec2::new(rng.range(-0.125..0.125), rng.range(-0.125..0.125)),
                rotation: random_lean(rng),
            });
            grass.push(GrassTuft {
                position: Vec3::new(spot.x, y, spot.y),
                blades,
            });
        }
    }
    grass
}

fn place_crystals(terrain: &Terrain, rng: &mut Rng) -> Vec<Crystal> {
    let mut crystals = Vec::new();
    for _ in 0..18 {
        let spot = random_spot(terrain, 0.7, rng);
        let y = terrain.height(spot.x, spot.y);
        // Crystals grow on the high slopes, above the meadows.
        if y > 7.5 && y < 16.0 {
            let s = rng.range(0.6..1.4);
            crystals.push(Crystal {
                position: Vec3::new(spot.x, y + s * 0.9, spot.y),
                scale: Vec3::new(s, s * 1.8, s),
                rotation: random_lean(rng),
                phase: rng.angle(),
            });
        }
    }
    crystals
}

fn place_clouds(terrain: &Terrain, rng: &mut Rng) -> Vec<Cloud> {
    let spread = terrain.world_size * 0.8;
    // `(0..16).map(...).collect()` builds a `Vec` from a closure per index.
    (0..16)
        .map(|i| {
            let puff_count = 4 + (rng.f32() * 4.0) as usize;
            let puffs = (0..puff_count)
                .map(|p| Puff {
                    radius: rng.range(3.5..7.0),
                    // Puffs sit side by side along X, centred on the cloud.
                    offset: Vec3::new(
                        (p as f32 - puff_count as f32 / 2.0) * 4.2,
                        rng.range(-0.8..0.8),
                        rng.range(-1.5..1.5),
                    ),
                })
                .collect();
            Cloud {
                position: Vec3::new(
                    rng.range(-spread..spread),
                    rng.range(48.0..64.0),
                    rng.range(-spread..spread),
                ),
                puffs,
                // Three speeds, so clouds slowly overtake each other.
                drift_speed: 1.2 + (i % 3) as f32 * 0.4,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scenery() -> (Terrain, Scenery) {
        let terrain = Terrain::default();
        let scenery = Scenery::generate(&terrain, DEFAULT_TREE_COUNT, 42);
        (terrain, scenery)
    }

    #[test]
    fn trees_grow_on_dry_gentle_ground() {
        let (terrain, scenery) = scenery();
        // Most of the 140 should fit; the exact number depends on the seed.
        assert!(
            scenery.trees.len() > 100,
            "only {} trees placed",
            scenery.trees.len()
        );
        for tree in &scenery.trees {
            let (x, z) = (tree.position.x, tree.position.z);
            assert!(
                terrain.height(x, z) > terrain.water_level + 0.9,
                "tree in the water at {x}, {z}"
            );
            assert!(terrain.normal(x, z).y > 0.82, "tree on a cliff at {x}, {z}");
        }
    }

    #[test]
    fn oaks_only_grow_in_the_lowlands() {
        let (_, scenery) = scenery();
        // `filter` keeps only the items the closure accepts.
        let oaks: Vec<_> = scenery
            .trees
            .iter()
            .filter(|t| t.kind == TreeKind::Oak)
            .collect();
        assert!(!oaks.is_empty(), "expected some oaks");
        assert!(oaks.iter().all(|t| t.position.y < 7.5));
    }

    #[test]
    fn every_tree_and_rock_is_an_obstacle() {
        let (_, scenery) = scenery();
        assert_eq!(
            scenery.obstacles().len(),
            scenery.trees.len() + scenery.rocks.len()
        );
    }

    #[test]
    fn clouds_wrap_around_the_world() {
        let (terrain, mut scenery) = scenery();
        let edge = terrain.world_size * 0.85;
        scenery.clouds[0].position.x = edge - 0.01;
        scenery.drift_clouds(&terrain, 1.0);
        assert_eq!(scenery.clouds[0].position.x, -edge);
    }

    #[test]
    fn same_seed_gives_same_scenery() {
        let terrain = Terrain::default();
        let a = Scenery::generate(&terrain, 50, 9);
        let b = Scenery::generate(&terrain, 50, 9);
        assert_eq!(a, b);
    }
}
