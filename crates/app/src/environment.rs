//! Lighting and the day/night cycle. The sun (or moon), ambient light, sky and fog
//! colours and the star field are driven by `sim::Clock`; the hemisphere light is
//! constant, as in the original. Ambient and hemisphere light together make up the
//! "fill light" (see `FillLight`).
//!
//! Keys: T starts or stops the clock; [ and ] move the time back or forward an hour.

use bevy::{
    asset::RenderAssetUsages, light::CascadeShadowConfigBuilder, light::NotShadowCaster, prelude::*,
};
use sim::{Clock, Daylight, Rng, daylight::DEFAULT_HOURS};

use crate::color::{IslandMaterial, hex, material, rgb};

/// Converting the original's Three.js light intensities to Bevy's physical units.
/// Midday sun (1.35) maps to bright daylight.
const SUN_LUX_PER_UNIT: f32 = light_consts::lux::AMBIENT_DAYLIGHT / 1.35;
/// Ambient and hemisphere light must keep the same strength *relative to the sun*
/// as in the original. Three.js r128 multiplies all three by π, so their ratio is
/// simply 1.35 : 0.52 : 0.65. Bevy is physically based: a sun of E lux lights a
/// matte surface by `albedo × E / π`, but ambient light of B cd/m² by `albedo × B`.
/// So one unit of ambient is worth one unit of sun divided by π.
///
/// This is also what one unit of the original's *output* brightness is worth, so
/// `scenery.rs` uses it to convert the crystals' emissive glow.
pub const AMBIENT_PER_UNIT: f32 = SUN_LUX_PER_UNIT / std::f32::consts::PI;

/// The original's `AmbientLight(0xdbeafe, …)`. Its strength comes from `sim::Daylight`.
const AMBIENT_COLOR: Color = rgb(0xdb, 0xea, 0xfe);

/// The original's `HemisphereLight(0x93c5fd, 0x3f4f29, 0.65)`: sky blue from above,
/// olive green from below. Unlike the sun and ambient light, it never changes with
/// the time of day.
const HEMISPHERE_SKY: Color = rgb(0x93, 0xc5, 0xfd);
const HEMISPHERE_GROUND: Color = rgb(0x3f, 0x4f, 0x29);
const HEMISPHERE_INTENSITY: f32 = 0.65;

const STAR_COUNT: usize = 600;
const STAR_SPHERE_RADIUS: f32 = 240.0;
const STAR_SEED: u64 = 99;

pub struct EnvironmentPlugin;

impl Plugin for EnvironmentPlugin {
    fn build(&self, app: &mut App) {
        // Made here rather than in a `Startup` system so it already exists when the
        // camera appears, whichever plugin spawns it first. `resource_mut` borrows a
        // resource straight out of the `World`; DefaultPlugins added `Assets<Image>`.
        let mut images = app.world_mut().resource_mut::<Assets<Image>>();
        let fill = FillLight::new(&mut images);

        app.insert_resource(WorldClock(Clock::default()))
            .insert_resource(ClearColor(hex(Daylight::at(DEFAULT_HOURS).sky_color)))
            .insert_resource(fill)
            // Bevy's own ambient light would also add reflections, which the
            // original's doesn't. The fill light replaces it, so switch it off.
            .insert_resource(GlobalAmbientLight::NONE)
            // An observer runs whenever its event happens, here "a `Camera3d` was
            // added to an entity". That works whichever plugin spawns the camera,
            // and whenever it does so, with no system ordering needed.
            .add_observer(add_fill_light)
            .add_systems(Startup, (spawn_sun, spawn_stars))
            .add_systems(Update, (clock_keys, tick_clock, apply_daylight).chain());
    }
}

/// The time of day, as a resource every system can read.
#[derive(Resource)]
pub struct WorldClock(pub Clock);

/// Marks the sun light.
#[derive(Component)]
struct Sun;

/// The parent of every star, so the whole field can be shown or hidden at once.
#[derive(Component)]
struct StarField;

/// The one material all stars share. Changing its alpha fades every star together.
#[derive(Resource)]
struct StarMaterial(Handle<IslandMaterial>);

/// The original's ambient and hemisphere light, combined into one Bevy light.
///
/// Bevy has no hemisphere light, but it has *environment map* lights: a cube map
/// (six square images, one per direction) that says how much light arrives from
/// every direction. A surface facing up gets the "top" image's colour, one facing
/// down gets the "bottom", and normals in between blend between them. Fill the top
/// with sky colour and the bottom with ground colour, and that's a hemisphere light.
/// Adding the same ambient colour to all six faces adds an ambient light.
///
/// Why not use Bevy's `GlobalAmbientLight` for the ambient part? In Three.js r128,
/// ambient and hemisphere light only light the matte ("diffuse") part of a surface.
/// Bevy's ambient light also adds reflections ("specular"). On a metal, those take
/// the metal's own colour, which washed the 85%-metallic crystals out to pale blue.
/// An environment map has a separate specular map, and keeping that black keeps
/// the reflections out.
#[derive(Resource)]
struct FillLight(EnvironmentMapLight);

impl FillLight {
    fn new(images: &mut Assets<Image>) -> Self {
        // Start black; `apply_daylight` paints the faces on the first frame.
        // (`hemispherical_gradient` makes a 1×1-pixel cube map, which is all a
        // smooth gradient needs, since the GPU blends between neighbouring faces.)
        let black = Color::BLACK;
        let mut light = EnvironmentMapLight::hemispherical_gradient(images, black, black, black);
        // A separate all-black cube map for reflections.
        light.specular_map = EnvironmentMapLight::solid_color(images, black).specular_map;
        // Bevy normally frees an image's pixels on the CPU side once they're
        // uploaded to the GPU (`RENDER_WORLD` only). `apply_daylight` rewrites
        // them whenever the time changes, so keep a CPU copy too (the default
        // usage is `MAIN_WORLD | RENDER_WORLD`).
        if let Some(mut cube) = images.get_mut(&light.diffuse_map) {
            cube.asset_usage = RenderAssetUsages::default();
        }
        // The faces hold light in the original's units; this converts to cd/m².
        light.intensity = AMBIENT_PER_UNIT;
        FillLight(light)
    }
}

/// The colour of each cube-map face for a given ambient strength, in the order
/// the GPU expects: +X, −X, +Y (up), −Y (down), +Z, −Z.
fn fill_faces(ambient_intensity: f32) -> [LinearRgba; 6] {
    let ambient = LinearRgba::from(AMBIENT_COLOR) * ambient_intensity;
    let sky = LinearRgba::from(HEMISPHERE_SKY) * HEMISPHERE_INTENSITY;
    let ground = LinearRgba::from(HEMISPHERE_GROUND) * HEMISPHERE_INTENSITY;
    // Three.js mixes sky and ground by `0.5 * normal.y + 0.5`, so a sideways
    // surface gets an even half-and-half blend. Mix in linear space, where light adds.
    let horizon = sky.mix(&ground, 0.5);

    let (up, down, side) = (ambient + sky, ambient + ground, ambient + horizon);
    [side, side, up, down, side, side]
}

/// Gives each 3D camera the fill light.
fn add_fill_light(add: On<Add<Camera3d>>, mut commands: Commands, fill: Res<FillLight>) {
    // `clone` copies the handles, not the images: the camera and the resource
    // point at the same cube map, so repainting it updates the camera's light.
    commands.entity(add.entity).insert(fill.0.clone());
}

fn spawn_sun(mut commands: Commands) {
    commands.spawn((
        Name::new("Sun"),
        Sun,
        DirectionalLight {
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::default(), // Placed by `apply_daylight` every frame.
        // Shadow cascades: higher-resolution shadows near the camera, coarser far away.
        CascadeShadowConfigBuilder {
            maximum_distance: 260.0,
            first_cascade_far_bound: 40.0,
            ..default()
        }
        .build(),
    ));
}

/// 600 points scattered over the upper part of a big sphere. Port of
/// `createSkyAndStars()`; Bevy has no point-sprite primitive, so each star is a
/// tiny sphere, all sharing one mesh and one material.
fn spawn_stars(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<IslandMaterial>>,
) {
    let mesh = meshes.add(
        Sphere::new(0.6)
            .mesh()
            .ico(0)
            .expect("ico(0) is always valid"),
    );
    let material = materials.add(material(StandardMaterial {
        base_color: Color::WHITE.with_alpha(0.0),
        alpha_mode: AlphaMode::Blend,
        unlit: true,        // Stars glow; scene lighting shouldn't shade them.
        fog_enabled: false, // At 240 m the fog would otherwise swallow them.
        ..default()
    }));
    commands.insert_resource(StarMaterial(material.clone()));

    let field = commands
        .spawn((
            Name::new("Stars"),
            StarField,
            Transform::default(),
            Visibility::Hidden,
        ))
        .id();
    let mut rng = Rng::new(STAR_SEED);
    for _ in 0..STAR_COUNT {
        let theta = rng.angle();
        // `acos` of a value in 0.15..1.0 keeps stars above the horizon.
        let phi = rng.range(0.15..1.0).acos();
        let position = STAR_SPHERE_RADIUS
            * Vec3::new(phi.sin() * theta.cos(), phi.cos(), phi.sin() * theta.sin());
        commands.spawn((
            Mesh3d(mesh.clone()),
            MeshMaterial3d(material.clone()),
            Transform::from_translation(position),
            NotShadowCaster,
            ChildOf(field),
        ));
    }
}

/// Bevy's change detection counts any *mutable* access to a `ResMut` as a change,
/// even if nothing is actually written. So these two systems only touch the clock
/// mutably inside the branches that really change it; otherwise `apply_daylight`
/// would redo its work every single frame.
fn clock_keys(keys: Res<ButtonInput<KeyCode>>, mut clock: ResMut<WorldClock>) {
    if keys.just_pressed(KeyCode::KeyT) {
        clock.0.running = !clock.0.running;
    }
    if keys.just_pressed(KeyCode::BracketRight) {
        let hours = clock.0.hours;
        clock.0.set(hours + 1.0);
    }
    if keys.just_pressed(KeyCode::BracketLeft) {
        let hours = clock.0.hours;
        clock.0.set(hours - 1.0);
    }
}

fn tick_clock(mut clock: ResMut<WorldClock>, time: Res<Time>) {
    // Reading `clock.0.running` only needs shared access, so it doesn't count.
    if clock.0.running {
        clock.0.tick(time.delta_secs());
    }
}

/// Applies the current daylight to the sky, fog, lights and stars.
#[allow(clippy::too_many_arguments)] // One system touching every light-related part.
fn apply_daylight(
    clock: Res<WorldClock>,
    mut clear_color: ResMut<ClearColor>,
    fill: Res<FillLight>,
    mut images: ResMut<Assets<Image>>,
    sun: Single<(&mut DirectionalLight, &mut Transform), With<Sun>>,
    mut fog: Query<&mut DistanceFog>,
    stars: Single<&mut Visibility, With<StarField>>,
    star_material: Res<StarMaterial>,
    mut materials: ResMut<Assets<IslandMaterial>>,
) {
    // Only redo the work when the time actually changed (or on the first frame).
    // `is_changed` is Bevy's change detection: it's true when someone wrote to the
    // resource since this system last ran.
    if !clock.is_changed() {
        return;
    }
    let light = clock.0.daylight();

    clear_color.0 = hex(light.sky_color);
    if let Some(mut cube) = images.get_mut(&fill.0.diffuse_map) {
        for (face, color) in fill_faces(light.ambient_intensity).into_iter().enumerate() {
            // The cube map is 1×1 pixels per face, so each face is just pixel (0, 0)
            // of image layer `face`. `expect` would crash on an error, which can only
            // mean this code is wrong (e.g. the image isn't a cube map).
            cube.set_color_at_3d(0, 0, face as u32, color.into())
                .expect("the fill light is a 1×1 cube map");
        }
    }
    for mut fog in &mut fog {
        // Used as is: `r128_shading.wesl` mixes it in after tonemapping.
        fog.color = hex(light.fog_color);
    }

    let (mut sun, mut sun_transform) = sun.into_inner();
    sun.color = hex(light.light_color);
    sun.illuminance = light.light_intensity * SUN_LUX_PER_UNIT;
    *sun_transform =
        Transform::from_translation(light.light_position).looking_at(Vec3::ZERO, Vec3::Y);

    let mut visibility = stars.into_inner();
    *visibility = if light.star_opacity > 0.0 {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    if let Some(mut material) = materials.get_mut(&star_material.0) {
        // `base` is the `StandardMaterial` inside our `IslandMaterial`.
        material.base.base_color.set_alpha(light.star_opacity);
    }
}
