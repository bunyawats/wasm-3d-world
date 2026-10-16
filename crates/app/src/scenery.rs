//! Draws the island's trees, rocks, grass, crystals and clouds from
//! `sim::Scenery`, and animates the crystals and clouds.
//!
//! Port of `populateFloraAndRocks()`, `createDetailedTree()` and `generateClouds()`
//! in `index.html`.

use bevy::{asset::RenderAssetUsages, light::NotShadowCaster, mesh::PrimitiveTopology, prelude::*};
use sim::{
    Scenery, TreeKind,
    scenery::{Crystal, Tree},
};

use crate::{
    color::{self, IslandMaterial, material},
    deer::HerdState,
    environment::AMBIENT_PER_UNIT,
    rig::{Rig, at},
    terrain::{IslandEdit, IslandScenery, IslandTerrain, SCENERY_SEED, TreeCount},
};

pub struct SceneryPlugin;

impl Plugin for SceneryPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_scenery).add_systems(
            Update,
            (apply_island_edits, animate_crystals, drift_clouds).chain(),
        );
    }
}

/// Which `Scenery::crystals` entry this entity shows.
#[derive(Component)]
struct CrystalId(usize);

/// Which `Scenery::clouds` entry this entity shows.
#[derive(Component)]
struct CloudId(usize);

/// Pine tiers: (cone radius, cone height, height of its centre), bottom to top.
const PINE_TIERS: [(f32, f32, f32); 4] = [
    (2.1, 2.8, 2.6),
    (1.7, 2.5, 4.1),
    (1.25, 2.2, 5.4),
    (0.75, 1.8, 6.5),
];
/// Oak crown puffs: (x, y, z, radius).
const OAK_PUFFS: [(f32, f32, f32, f32); 5] = [
    (0.0, 3.3, 0.0, 1.7),
    (0.9, 2.9, 0.6, 1.25),
    (-0.85, 3.0, -0.5, 1.3),
    (-0.5, 3.5, 0.8, 1.15),
    (0.6, 3.6, -0.7, 1.1),
];

/// Meshes and materials for everything in the scenery, built once. A `Resource`,
/// so rebuilds and planted trees reuse them.
#[derive(Resource)]
struct SceneryAssets {
    trunk: Handle<Mesh>,
    /// One cone mesh per pine tier, matching `PINE_TIERS`.
    pine_tiers: [Handle<Mesh>; 4],
    /// A unit-radius low-poly ball, scaled per use: oak puffs and cloud puffs.
    ball: Handle<Mesh>,
    rock: Handle<Mesh>,
    blade: Handle<Mesh>,
    crystal: Handle<Mesh>,

    /// Trees scattered at startup and trees added with "+ Tree" are coloured
    /// differently, like crystals.
    trees: TreeMaterials,
    planted_trees: TreeMaterials,
    stone: Handle<IslandMaterial>,
    grass: Handle<IslandMaterial>,
    /// Crystals scattered at startup are metallic; ones added with "+ Crystal" aren't.
    crystal_glow: Handle<IslandMaterial>,
    planted_crystal_glow: Handle<IslandMaterial>,
    cloud: Handle<IslandMaterial>,
}

impl SceneryAssets {
    fn new(meshes: &mut Assets<Mesh>, materials: &mut Assets<IslandMaterial>) -> Self {
        // `map` on an array transforms each element and keeps the array's length,
        // so `[(f32, f32, f32); 4]` becomes `[Handle<Mesh>; 4]`.
        let pine_tiers = PINE_TIERS
            .map(|(radius, height, _)| meshes.add(Cone { radius, height }.mesh().resolution(8)));

        let mut matte = |rgb: [u8; 3], roughness: f32| {
            materials.add(material(StandardMaterial {
                base_color: color::rgb(rgb[0], rgb[1], rgb[2]),
                perceptual_roughness: roughness,
                ..default()
            }))
        };
        // The original's materials from `populateFloraAndRocks()`...
        let trees = TreeMaterials {
            bark: matte([0x4a, 0x35, 0x25], 0.92),
            pine_needles: [
                matte([0x1e, 0x4d, 0x2b], 0.82),
                matte([0x26, 0x5c, 0x34], 0.82),
            ],
            oak_leaves: matte([0x3b, 0x6e, 0x38], 0.8),
        };
        // ...and from `spawnObject('tree')`: one brighter green for every leafy
        // part. `Handle::clone` copies the handle, not the material, so all three
        // share one material.
        let leaves = matte([0x15, 0x80, 0x3d], 0.8);
        let planted_trees = TreeMaterials {
            bark: matte([0x4a, 0x35, 0x25], 0.9),
            pine_needles: [leaves.clone(), leaves.clone()],
            oak_leaves: leaves,
        };
        let stone = matte([0x64, 0x74, 0x8b], 0.88);
        let grass = matte([0x4d, 0x8c, 0x45], 0.8);

        Self {
            trunk: meshes.add(
                ConicalFrustum {
                    radius_top: 0.24,
                    radius_bottom: 0.45,
                    height: 2.6,
                }
                .mesh()
                .resolution(7),
            ),
            pine_tiers,
            ball: meshes.add(ico_ball(1.0, false)),
            rock: meshes.add(ico_ball(1.0, true)),
            blade: meshes.add(
                Cone {
                    radius: 0.08,
                    height: 0.55,
                }
                .mesh()
                .resolution(4),
            ),
            crystal: meshes.add(octahedron(0.9)),
            trees,
            planted_trees,
            stone,
            grass,
            // The original's two crystal materials, from `populateFloraAndRocks()`
            // and `spawnObject('crystal')`.
            crystal_glow: materials.add(material(crystal_material(0.5, 0.85))),
            planted_crystal_glow: materials.add(material(crystal_material(0.7, 0.0))),
            cloud: materials.add(material(StandardMaterial {
                base_color: Color::WHITE.with_alpha(0.86),
                alpha_mode: AlphaMode::Blend,
                perceptual_roughness: 0.92,
                ..default()
            })),
        }
    }
}

/// The materials for one look of tree.
struct TreeMaterials {
    bark: Handle<IslandMaterial>,
    /// Pine tiers alternate between two shades of green.
    pine_needles: [Handle<IslandMaterial>; 2],
    oak_leaves: Handle<IslandMaterial>,
}

/// A low-poly sphere (an icosphere subdivided once), optionally flat-shaded so
/// every face shows as a separate facet, like Three.js's `flatShading: true`.
fn ico_ball(radius: f32, faceted: bool) -> Mesh {
    let mut mesh = Sphere::new(radius)
        .mesh()
        .ico(1)
        .expect("ico(1) is always valid");
    if faceted {
        // Flat shading needs each triangle to have its own three vertices, so
        // neighbouring faces don't share (and average) normals.
        mesh.duplicate_vertices();
        mesh.compute_flat_normals();
    }
    mesh
}

/// An eight-sided gem shape (two square pyramids base to base). Bevy has no
/// octahedron primitive, so this builds the triangles by hand.
/// A glowing blue crystal material. `glow` and `metallic` are the original's
/// `emissiveIntensity` and `metalness`.
fn crystal_material(glow: f32, metallic: f32) -> StandardMaterial {
    StandardMaterial {
        base_color: color::rgb(0x38, 0xbd, 0xf8),
        // Emissive light is added on top of lighting, so crystals glow even at
        // night. By default Bevy adds it *after* the camera's exposure, as a fixed
        // screen brightness. Weight 1.0 runs it through the exposure like real
        // light, so it can use the same unit conversion as the ambient light and
        // keep the original's balance between glow and lighting.
        emissive: LinearRgba::from(color::rgb(0x02, 0x84, 0xc7)) * (glow * AMBIENT_PER_UNIT),
        emissive_exposure_weight: 1.0,
        perceptual_roughness: 0.15,
        metallic,
        ..default()
    }
}

fn octahedron(radius: f32) -> Mesh {
    let top = Vec3::Y * radius;
    let bottom = -top;
    // The four "equator" corners, going counter-clockwise when seen from above.
    let ring = [Vec3::X, Vec3::Z, Vec3::NEG_X, Vec3::NEG_Z].map(|v| v * radius);

    let mut positions = Vec::with_capacity(8 * 3);
    for i in 0..4 {
        let (a, b) = (ring[i], ring[(i + 1) % 4]);
        // Corner order decides which way a triangle faces. These orders make every
        // face point outwards.
        positions.extend([top, b, a]);
        positions.extend([bottom, a, b]);
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_computed_flat_normals()
}

fn spawn_scenery(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<IslandMaterial>>,
    scenery: Res<IslandScenery>,
) {
    let assets = SceneryAssets::new(&mut meshes, &mut materials);
    let groups = spawn_all(&mut commands, &assets, &scenery.0);
    commands.insert_resource(assets);
    commands.insert_resource(groups);
}

/// The parent entity of each category. One parent per category keeps the entity
/// list tidy, and despawning a parent despawns all of its children too.
#[derive(Resource)]
struct SceneryGroups {
    trees: Entity,
    crystals: Entity,
    /// Every group, for despawning the lot on a rebuild.
    all: [Entity; 5],
}

/// Spawns entities for everything in `scenery`, under fresh group parents.
fn spawn_all(commands: &mut Commands, assets: &SceneryAssets, scenery: &Scenery) -> SceneryGroups {
    let mut group = |name: &'static str| {
        commands
            .spawn((Name::new(name), Transform::default(), Visibility::default()))
            .id()
    };
    let (trees, rocks, grass, crystals, clouds) = (
        group("Trees"),
        group("Rocks"),
        group("Grass"),
        group("Crystals"),
        group("Clouds"),
    );

    for tree in &scenery.trees {
        spawn_tree(commands, assets, trees, tree);
    }

    for rock in &scenery.rocks {
        let r = rock.rotation;
        commands.spawn((
            Mesh3d(assets.rock.clone()),
            MeshMaterial3d(assets.stone.clone()),
            Transform {
                translation: rock.position,
                rotation: Quat::from_euler(EulerRot::XYZ, r.x, r.y, r.z),
                scale: rock.scale,
            },
            ChildOf(rocks),
        ));
    }

    for tuft in &scenery.grass {
        let tuft_entity = commands
            .spawn((
                Transform::from_translation(tuft.position),
                Visibility::default(),
                ChildOf(grass),
            ))
            .id();
        let mut rig = Rig {
            commands: &mut *commands,
            pickable: false,
        };
        for blade in &tuft.blades {
            let r = blade.rotation;
            let placement = at(blade.offset.x, 0.0, blade.offset.y)
                .with_rotation(Quat::from_euler(EulerRot::XYZ, r.x, r.y, r.z));
            // The blade's base sits at the pivot, so it leans from the root.
            rig.offset_part(
                tuft_entity,
                &assets.blade,
                &assets.grass,
                placement,
                Vec3::Y * 0.27,
            );
        }
    }

    for (id, crystal) in scenery.crystals.iter().enumerate() {
        spawn_crystal(commands, assets, crystals, id, crystal);
    }

    for (id, cloud) in scenery.clouds.iter().enumerate() {
        let cloud_entity = commands
            .spawn((
                CloudId(id),
                Transform::from_translation(cloud.position),
                Visibility::default(),
                ChildOf(clouds),
            ))
            .id();
        for puff in &cloud.puffs {
            commands.spawn((
                Mesh3d(assets.ball.clone()),
                MeshMaterial3d(assets.cloud.clone()),
                // Flattened to 62% height, like the original.
                Transform::from_translation(puff.offset)
                    .with_scale(Vec3::new(1.0, 0.62, 1.0) * puff.radius),
                // The original's clouds didn't cast shadows; huge moving shadows
                // would darken the whole island.
                NotShadowCaster,
                ChildOf(cloud_entity),
            ));
        }
    }

    SceneryGroups {
        trees,
        crystals,
        all: [trees, rocks, grass, crystals, clouds],
    }
}

fn spawn_crystal(
    commands: &mut Commands,
    assets: &SceneryAssets,
    parent: Entity,
    id: usize,
    crystal: &Crystal,
) {
    commands.spawn((
        CrystalId(id),
        Mesh3d(assets.crystal.clone()),
        MeshMaterial3d(if crystal.planted {
            assets.planted_crystal_glow.clone()
        } else {
            assets.crystal_glow.clone()
        }),
        Transform::from_translation(crystal.position).with_scale(crystal.scale),
        NotShadowCaster,
        ChildOf(parent),
    ));
}

/// Handles the HUD's island edits: re-scatter everything after a terrain change,
/// or add one tree or crystal. Keeps the deer's obstacle list in step.
#[allow(clippy::too_many_arguments)] // A system that coordinates several resources.
fn apply_island_edits(
    mut edits: MessageReader<IslandEdit>,
    mut commands: Commands,
    assets: Res<SceneryAssets>,
    mut groups: ResMut<SceneryGroups>,
    mut scenery: ResMut<IslandScenery>,
    island: Res<IslandTerrain>,
    tree_count: Res<TreeCount>,
    mut herd: ResMut<HerdState>,
) {
    for edit in edits.read() {
        match edit {
            IslandEdit::Rebuild => {
                for group in groups.all {
                    commands.entity(group).despawn(); // Recursive: children go too.
                }
                scenery.0 = Scenery::generate(&island.0, tree_count.0, SCENERY_SEED);
                *groups = spawn_all(&mut commands, &assets, &scenery.0);
            }
            IslandEdit::PlantTree => {
                let tree = scenery.0.plant_tree(&island.0);
                spawn_tree(&mut commands, &assets, groups.trees, &tree);
            }
            IslandEdit::GrowCrystal => {
                let crystal = scenery.0.grow_crystal(&island.0);
                let id = scenery.0.crystals.len() - 1;
                spawn_crystal(&mut commands, &assets, groups.crystals, id, &crystal);
            }
        }
        // Rebuilt or planted, the trees may have moved: refresh what deer avoid.
        // (The original forgot to do this for planted trees.)
        herd.0.obstacles = scenery.0.obstacles();
    }
}

fn spawn_tree(commands: &mut Commands, assets: &SceneryAssets, parent: Entity, tree: &Tree) {
    let root = commands
        .spawn((
            Transform::from_translation(tree.position)
                .with_rotation(Quat::from_rotation_y(tree.yaw))
                .with_scale(Vec3::splat(tree.scale)),
            Visibility::default(),
            ChildOf(parent),
        ))
        .id();
    let mut rig = Rig {
        commands,
        pickable: false,
    };
    let materials = if tree.planted {
        &assets.planted_trees
    } else {
        &assets.trees
    };
    rig.part(root, &assets.trunk, &materials.bark, at(0.0, 1.3, 0.0));

    match tree.kind {
        TreeKind::Pine => {
            // `zip` pairs each tier's numbers with its mesh; `enumerate` adds the
            // index, used to alternate the two needle colours.
            for (i, ((_, _, y), mesh)) in PINE_TIERS.iter().zip(&assets.pine_tiers).enumerate() {
                rig.part(root, mesh, &materials.pine_needles[i % 2], at(0.0, *y, 0.0));
            }
        }
        TreeKind::Oak => {
            for (x, y, z, radius) in OAK_PUFFS {
                rig.part(
                    root,
                    &assets.ball,
                    &materials.oak_leaves,
                    at(x, y, z).with_scale(Vec3::splat(radius)),
                );
            }
        }
    }
}

fn animate_crystals(
    scenery: Res<IslandScenery>,
    time: Res<Time>,
    mut crystals: Query<(&CrystalId, &mut Transform)>,
) {
    let elapsed = time.elapsed_secs();
    for (id, mut transform) in &mut crystals {
        // `get` rather than `[]`: right after a rebuild, an old entity may still
        // carry an index the new scenery doesn't have.
        let Some(crystal) = scenery.0.crystals.get(id.0) else {
            continue;
        };
        let (position, spin) = crystal.animated(elapsed);
        let r = crystal.rotation;
        transform.translation = position;
        transform.rotation = Quat::from_euler(EulerRot::XYZ, r.x, r.y + spin, r.z);
    }
}

fn drift_clouds(
    mut scenery: ResMut<IslandScenery>,
    island: Res<IslandTerrain>,
    time: Res<Time>,
    mut clouds: Query<(&CloudId, &mut Transform)>,
) {
    scenery
        .0
        .drift_clouds(&island.0, time.delta_secs().min(0.1));
    for (id, mut transform) in &mut clouds {
        if let Some(cloud) = scenery.0.clouds.get(id.0) {
            transform.translation = cloud.position;
        }
    }
}
