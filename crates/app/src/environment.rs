//! Sky colour, sunlight and ambient light, fixed at the original's default
//! time of day (13:30). The day/night cycle comes in a later milestone.

use bevy::{light::CascadeShadowConfigBuilder, prelude::*};

/// Original default `config.timeOfDay`.
const TIME_OF_DAY_HOURS: f32 = 13.5;
/// Daytime sky colour (`0x60a5fa` in the original).
pub const SKY_COLOR: Color = Color::srgb_u8(0x60, 0xa5, 0xfa);
/// Daytime fog colour (`0x93c5fd`), used by the camera's fog.
pub const FOG_COLOR: Color = Color::srgb_u8(0x93, 0xc5, 0xfd);

pub struct EnvironmentPlugin;

impl Plugin for EnvironmentPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ClearColor(SKY_COLOR))
            .insert_resource(GlobalAmbientLight {
                color: Color::srgb_u8(0xdb, 0xea, 0xfe),
                brightness: 350.0,
                ..default()
            })
            .add_systems(Startup, spawn_sun);
    }
}

/// Where the sun sits for a given hour: it rises in the east at 06:00, peaks at
/// 12:00 and sets at 18:00, on a circle of radius 150 (same as the original).
fn sun_position(hours: f32) -> Vec3 {
    let angle = (hours - 6.0) / 24.0 * std::f32::consts::TAU;
    Vec3::new(angle.cos() * 150.0, angle.sin() * 150.0, 45.0)
}

fn spawn_sun(mut commands: Commands) {
    commands.spawn((
        Name::new("Sun"),
        DirectionalLight {
            color: Color::srgb_u8(0xff, 0xfb, 0xeb),
            illuminance: light_consts::lux::AMBIENT_DAYLIGHT,
            shadow_maps_enabled: true,
            ..default()
        },
        // A directional light shines along its forward axis; only its rotation
        // matters, so point it from the sun's position towards the origin.
        Transform::from_translation(sun_position(TIME_OF_DAY_HOURS))
            .looking_at(Vec3::ZERO, Vec3::Y),
        // Shadow cascades: higher-resolution shadows near the camera, coarser far away.
        CascadeShadowConfigBuilder {
            maximum_distance: 260.0,
            first_cascade_far_bound: 40.0,
            ..default()
        }
        .build(),
    ));
}
