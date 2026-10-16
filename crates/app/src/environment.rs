//! Lighting and the day/night cycle. The sun (or moon), ambient light, sky and fog
//! colours and the star field are driven by `sim::Clock`; the hemisphere light is
//! constant, as in the original.
//!
//! Keys: T starts or stops the clock; [ and ] move the time back or forward an hour.

use bevy::{light::CascadeShadowConfigBuilder, light::NotShadowCaster, prelude::*};
use sim::{Clock, Daylight, Rng, daylight::DEFAULT_HOURS};

/// Daytime fog colour, used for the camera's fog until the first frame updates it.
pub const FOG_COLOR: Color = Color::srgb_u8(0x93, 0xc5, 0xfd);

/// Converting the original's Three.js light intensities to Bevy's physical units.
/// Midday sun (1.35) maps to bright daylight.
const SUN_LUX_PER_UNIT: f32 = light_consts::lux::AMBIENT_DAYLIGHT / 1.35;
/// Ambient and hemisphere light must keep the same strength *relative to the sun*
/// as in the original. Three.js r128 multiplies all three by π, so their ratio is
/// simply 1.35 : 0.52 : 0.65. Bevy is physically based: a sun of E lux lights a
/// matte surface by `albedo × E / π`, but ambient light of B cd/m² by `albedo × B`.
/// So one unit of ambient is worth one unit of sun divided by π.
const AMBIENT_PER_UNIT: f32 = SUN_LUX_PER_UNIT / std::f32::consts::PI;

/// The original's `HemisphereLight(0x93c5fd, 0x3f4f29, 0.65)`: sky blue from above,
/// olive green from below. Unlike the sun and ambient light, it never changes with
/// the time of day.
const HEMISPHERE_SKY: Color = Color::srgb_u8(0x93, 0xc5, 0xfd);
const HEMISPHERE_GROUND: Color = Color::srgb_u8(0x3f, 0x4f, 0x29);
const HEMISPHERE_INTENSITY: f32 = 0.65;

const STAR_COUNT: usize = 600;
const STAR_SPHERE_RADIUS: f32 = 240.0;
const STAR_SEED: u64 = 99;

pub struct EnvironmentPlugin;

impl Plugin for EnvironmentPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(WorldClock(Clock::default()))
            .insert_resource(ClearColor(hex(Daylight::at(DEFAULT_HOURS).sky_color)))
            .insert_resource(GlobalAmbientLight {
                color: Color::srgb_u8(0xdb, 0xea, 0xfe),
                // Replaced by `apply_daylight` from the first frame on.
                brightness: 0.52 * AMBIENT_PER_UNIT,
                ..default()
            })
            // An observer runs whenever its event happens, here "a `Camera3d` was
            // added to an entity". That works whichever plugin spawns the camera,
            // and whenever it does so, with no system ordering needed.
            .add_observer(add_hemisphere_light)
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
struct StarMaterial(Handle<StandardMaterial>);

/// Converts a `0xRRGGBB` sRGB colour (the format `sim` uses) to a Bevy `Color`.
pub fn hex(rgb: u32) -> Color {
    // Shift each byte down to the bottom, then `as u8` keeps just the low 8 bits.
    Color::srgb_u8((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

/// Gives each 3D camera the original's hemisphere light.
///
/// Bevy has no hemisphere light, but it has *environment map* lights: a cube map
/// (six square images, one per direction) that says how much light arrives from
/// every direction. A surface facing up gets the "top" image's colour, one facing
/// down gets the "bottom", and normals in between blend between them. Fill the top
/// with sky colour and the bottom with ground colour, and that's a hemisphere light.
fn add_hemisphere_light(
    add: On<Add<Camera3d>>,
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
) {
    // Three.js mixes the two colours by `0.5 * normal.y + 0.5`, so a sideways
    // surface gets an even half-and-half blend. Mix in linear space, where light adds.
    let sky = LinearRgba::from(HEMISPHERE_SKY);
    let ground = LinearRgba::from(HEMISPHERE_GROUND);
    let horizon = sky.mix(&ground, 0.5);

    let mut light = EnvironmentMapLight::hemispherical_gradient(&mut images, sky, horizon, ground);
    // An environment map also lights shiny reflections (its "specular" map), but
    // Three.js's hemisphere light only lights the diffuse, matte part. A black
    // specular map keeps the reflections out.
    light.specular_map = EnvironmentMapLight::solid_color(&mut images, Color::BLACK).specular_map;
    // Same scale as the ambient light, since Bevy adds both to a surface the same way.
    light.intensity = HEMISPHERE_INTENSITY * AMBIENT_PER_UNIT;

    commands.entity(add.entity).insert(light);
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
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mesh = meshes.add(
        Sphere::new(0.6)
            .mesh()
            .ico(0)
            .expect("ico(0) is always valid"),
    );
    let material = materials.add(StandardMaterial {
        base_color: Color::WHITE.with_alpha(0.0),
        alpha_mode: AlphaMode::Blend,
        unlit: true,        // Stars glow; scene lighting shouldn't shade them.
        fog_enabled: false, // At 240 m the fog would otherwise swallow them.
        ..default()
    });
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
    mut ambient: ResMut<GlobalAmbientLight>,
    sun: Single<(&mut DirectionalLight, &mut Transform), With<Sun>>,
    mut fog: Query<&mut DistanceFog>,
    stars: Single<&mut Visibility, With<StarField>>,
    star_material: Res<StarMaterial>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Only redo the work when the time actually changed (or on the first frame).
    // `is_changed` is Bevy's change detection: it's true when someone wrote to the
    // resource since this system last ran.
    if !clock.is_changed() {
        return;
    }
    let light = clock.0.daylight();

    clear_color.0 = hex(light.sky_color);
    ambient.brightness = light.ambient_intensity * AMBIENT_PER_UNIT;
    for mut fog in &mut fog {
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
        material.base_color.set_alpha(light.star_opacity);
    }
}
