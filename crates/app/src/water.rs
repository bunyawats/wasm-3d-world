//! Animated sea surface.

use bevy::{mesh::VertexAttributeValues, prelude::*};

use crate::terrain::IslandTerrain;

/// The sea extends far past the island, so its edge fades into the fog instead of
/// showing as a hard line against the sky (the original used 2.2).
const WATER_SIZE_FACTOR: f32 = 6.0;
/// Grid lines across the water plane (more = smoother waves, more work per frame).
const WATER_SUBDIVISIONS: u32 = 120;

pub struct WaterPlugin;

impl Plugin for WaterPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_water)
            .add_systems(Update, animate_waves);
    }
}

/// A *marker component*: it has no data and exists only so systems can find the
/// water entity with `With<Water>`.
#[derive(Component)]
struct Water;

fn spawn_water(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    island: Res<IslandTerrain>,
) {
    let size = island.0.world_size * WATER_SIZE_FACTOR;
    let mesh = Plane3d::default()
        .mesh()
        .size(size, size)
        .subdivisions(WATER_SUBDIVISIONS);

    commands.spawn((
        Name::new("Water"),
        Water,
        Mesh3d(meshes.add(mesh)),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgba_u8(0x0e, 0x74, 0x90, 219), // 0.86 opacity
            alpha_mode: AlphaMode::Blend,
            perceptual_roughness: 0.14,
            // The original used metalness 0.75, but without an environment map a
            // metal surface has nothing to reflect and renders almost black.
            metallic: 0.1,
            reflectance: 0.6,
            ..default()
        })),
        Transform::from_xyz(0.0, island.0.water_level, 0.0),
    ));
}

/// Moves each water vertex up and down every frame.
///
/// `Single<...>` asks for exactly one matching entity (Bevy skips the system if there
/// isn't exactly one). `Res<Time>` gives the elapsed time.
fn animate_waves(
    water: Single<&Mesh3d, With<Water>>,
    mut meshes: ResMut<Assets<Mesh>>,
    time: Res<Time>,
) {
    let t = time.elapsed_secs();
    // `let Some(x) = ... else { return };` is *let-else*: unwrap the `Option`, or leave
    // the function early if it's `None`. No mesh, no work.
    let Some(mut mesh) = meshes.get_mut(&water.0) else {
        return;
    };
    // Attributes are stored as an enum over every possible vertex format, so we
    // `match` on the variant we expect (positions are always three f32s).
    let Some(VertexAttributeValues::Float32x3(positions)) =
        mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION)
    else {
        return;
    };

    // `iter_mut()` yields `&mut [f32; 3]`, so writing through `p` edits the mesh.
    for p in positions.iter_mut() {
        let (x, z) = (p[0], p[2]);
        p[1] = (x * 0.12 + t * 2.0).sin() * 0.09 + (z * 0.14 + t * 1.6).cos() * 0.08;
    }
    mesh.compute_smooth_normals();
}
