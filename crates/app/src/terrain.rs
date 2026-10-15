//! Builds the island mesh from `sim::Terrain`.

use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};
use sim::{Surface, Terrain};

/// Grid resolution: the island is `TERRAIN_SEGMENTS` x `TERRAIN_SEGMENTS` quads.
/// Same as `config.terrainSegments` in the original.
const TERRAIN_SEGMENTS: u32 = 96;

pub struct TerrainPlugin;

impl Plugin for TerrainPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<IslandTerrain>()
            .add_systems(Startup, spawn_terrain);
    }
}

/// The island's terrain parameters, shared by every system that needs them
/// (the water reads `water_level`; later, deer will read heights).
///
/// Why not put `#[derive(Resource)]` on `sim::Terrain` directly? Rust's *orphan rule*:
/// you may implement a trait for a type only if your crate owns the trait or the type.
/// `Resource` belongs to Bevy and `Terrain` belongs to `sim`, so this crate owns
/// neither. Wrapping it in a one-field *newtype* that we do own is the standard fix,
/// and it also keeps `sim` free of any Bevy dependency.
#[derive(Resource, Default)]
pub struct IslandTerrain(pub Terrain);

/// `Commands` queues entity spawns; `ResMut<Assets<Mesh>>` is mutable access to the
/// mesh storage. `Res<IslandTerrain>` is read-only access to our resource.
fn spawn_terrain(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    island: Res<IslandTerrain>,
) {
    let mesh = build_terrain_mesh(&island.0, TERRAIN_SEGMENTS);

    commands.spawn((
        Name::new("Terrain"),
        Mesh3d(meshes.add(mesh)),
        MeshMaterial3d(materials.add(StandardMaterial {
            // White, because the vertex colours carry the actual colour
            // (Bevy multiplies the two together).
            base_color: Color::WHITE,
            perceptual_roughness: 0.88,
            metallic: 0.04,
            ..default()
        })),
    ));
}

/// Builds a square grid mesh, sampling the terrain for each vertex's height and colour.
fn build_terrain_mesh(terrain: &Terrain, segments: u32) -> Mesh {
    let size = terrain.world_size;
    let step = size / segments as f32;
    let half = size / 2.0;
    let verts_per_side = segments + 1;

    // `Vec::with_capacity` allocates once up front, since we know the final length.
    let vertex_count = (verts_per_side * verts_per_side) as usize;
    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(vertex_count);
    let mut colors: Vec<[f32; 4]> = Vec::with_capacity(vertex_count);

    for row in 0..verts_per_side {
        for col in 0..verts_per_side {
            let x = -half + col as f32 * step;
            let z = -half + row as f32 * step;
            positions.push([x, terrain.height(x, z), z]);
            colors.push(surface_color(terrain.surface(x, z)).to_f32_array());
        }
    }

    // Two triangles per grid square. Vertices are listed counter-clockwise when seen
    // from above, which is how Bevy decides which side of a triangle is the front.
    let mut indices: Vec<u32> = Vec::with_capacity((segments * segments * 6) as usize);
    for row in 0..segments {
        for col in 0..segments {
            let a = row * verts_per_side + col; //  a --- b    (+x →)
            let b = a + 1; //                       |   / |
            let c = a + verts_per_side; //          | /   |
            let d = c + 1; //                       c --- d    (+z ↓)
            indices.extend_from_slice(&[a, c, b, b, c, d]);
        }
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
    .with_inserted_indices(Indices::U32(indices));
    // Average each vertex's neighbouring face normals, like Three.js's
    // `computeVertexNormals()` did in the original.
    mesh.compute_smooth_normals();
    mesh
}

/// Colour for each kind of ground, using the original's hex values.
///
/// `match` must cover every `Surface` variant. Delete an arm and the code won't compile.
fn surface_color(surface: Surface) -> LinearRgba {
    let color = match surface {
        Surface::Sand => Srgba::rgb_u8(0xd6, 0xc7, 0xa9),
        Surface::Cliff => Srgba::rgb_u8(0x64, 0x74, 0x62),
        // `Meadow { blend }` *destructures* the variant, binding its field to `blend`.
        Surface::Meadow { blend } => {
            Srgba::rgb_u8(0x3b, 0x7a, 0x3e).mix(&Srgba::rgb_u8(0x52, 0x8c, 0x42), blend)
        }
        Surface::Rock => Srgba::rgb_u8(0x5c, 0x66, 0x58),
        Surface::Snow => Srgba::rgb_u8(0xf1, 0xf5, 0xf9),
    };
    // Mesh vertex colours are stored in linear space, while hex codes are sRGB.
    // `.into()` converts using the `From<Srgba> for LinearRgba` impl.
    color.into()
}
