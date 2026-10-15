//! Orbit camera, matching the original's Three.js `OrbitControls`:
//! left-drag rotates, right-drag pans, the scroll wheel zooms, all with smoothing.

use std::f32::consts::FRAC_PI_2;

use bevy::{
    core_pipeline::tonemapping::Tonemapping,
    input::mouse::{AccumulatedMouseScroll, MouseScrollUnit},
    prelude::*,
    window::PrimaryWindow,
};

use crate::environment::FOG_COLOR;

const ROTATE_SPEED: f32 = 0.005; // radians per pixel dragged
const PAN_SPEED: f32 = 0.0015; // world units per pixel, per unit of distance
const ZOOM_SPEED: f32 = 0.1; // ~10% closer or further per scroll-wheel line
const MIN_DISTANCE: f32 = 2.2;
const MAX_DISTANCE: f32 = 220.0;
/// Keep the camera just above the horizon, so it can't look up from under the ground.
const MIN_PITCH: f32 = 0.02;
/// And just short of straight down, where yaw would become ambiguous.
const MAX_PITCH: f32 = FRAC_PI_2 - 0.05;
/// How quickly the camera catches up with the input (higher = snappier).
const SMOOTHING_RATE: f32 = 12.0;

pub struct OrbitCameraPlugin;

impl Plugin for OrbitCameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_camera)
            // `.chain()` runs these in order: read input first, then move the camera.
            .add_systems(Update, (orbit_input, apply_orbit).chain());
    }
}

/// A camera position described as "looking at `target` from `distance` away,
/// rotated `yaw` around the vertical axis and tilted `pitch` above the horizon".
#[derive(Debug, Clone, Copy)]
struct Orbit {
    target: Vec3,
    yaw: f32,
    pitch: f32,
    distance: f32,
}

impl Orbit {
    /// The orbit that places the camera at `eye`, looking at `target`.
    fn looking_from(eye: Vec3, target: Vec3) -> Self {
        let offset = eye - target;
        let distance = offset.length();
        Self {
            target,
            yaw: offset.x.atan2(offset.z),
            pitch: (offset.y / distance).asin(),
            distance,
        }
    }

    fn eye(&self) -> Vec3 {
        let horizontal = self.pitch.cos() * self.distance;
        self.target
            + Vec3::new(
                self.yaw.sin() * horizontal,
                self.pitch.sin() * self.distance,
                self.yaw.cos() * horizontal,
            )
    }

    /// Moves `self` a fraction `t` of the way towards `goal`.
    fn approach(&mut self, goal: &Orbit, t: f32) {
        self.target = self.target.lerp(goal.target, t);
        self.yaw = self.yaw.lerp(goal.yaw, t);
        self.pitch = self.pitch.lerp(goal.pitch, t);
        self.distance = self.distance.lerp(goal.distance, t);
    }
}

/// Input edits `goal`; every frame `current` eases towards it. That easing is the
/// smooth "damping" feel of the original's `controls.enableDamping`.
#[derive(Component)]
struct OrbitCamera {
    current: Orbit,
    goal: Orbit,
}

fn spawn_camera(mut commands: Commands) {
    // Same starting view as the original: eye at (55, 32, 70), looking at (0, 4, 0).
    let orbit = Orbit::looking_from(Vec3::new(55.0, 32.0, 70.0), Vec3::new(0.0, 4.0, 0.0));

    commands.spawn((
        Name::new("Camera"),
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection {
            fov: 48.0_f32.to_radians(),
            near: 0.25,
            far: 600.0,
            ..default()
        }),
        Tonemapping::AcesFitted,
        // Exponential fog, like the original's `THREE.FogExp2(…, 0.0042)`.
        DistanceFog {
            color: FOG_COLOR,
            falloff: FogFalloff::Exponential { density: 0.0042 },
            ..default()
        },
        Transform::from_translation(orbit.eye()).looking_at(orbit.target, Vec3::Y),
        OrbitCamera {
            current: orbit,
            goal: orbit,
        },
    ));
}

/// Turns mouse input into changes to the camera's goal orbit.
fn orbit_input(
    camera: Single<(&mut OrbitCamera, &Transform)>,
    buttons: Res<ButtonInput<MouseButton>>,
    scroll: Res<AccumulatedMouseScroll>,
    window: Single<&Window, With<PrimaryWindow>>,
    // `Local` is state private to this one system that survives between frames.
    // Here: where the cursor was last frame (`None` if it was outside the window).
    mut last_cursor: Local<Option<Vec2>>,
) {
    // Destructure the tuple. `into_inner` takes the data out of the `Single` wrapper.
    let (mut orbit_camera, transform) = camera.into_inner();
    let goal = &mut orbit_camera.goal;

    // How far the cursor moved since last frame, in pixels. We track the cursor
    // ourselves because raw mouse-motion events aren't reliable in the browser.
    // `match` on a tuple checks both values at once.
    let cursor = window.cursor_position();
    let drag = match (cursor, *last_cursor) {
        (Some(now), Some(before)) => now - before,
        _ => Vec2::ZERO, // Just entered the window: no movement yet.
    };
    *last_cursor = cursor;

    if buttons.pressed(MouseButton::Left) {
        goal.yaw -= drag.x * ROTATE_SPEED;
        goal.pitch = (goal.pitch + drag.y * ROTATE_SPEED).clamp(MIN_PITCH, MAX_PITCH);
    }

    if buttons.pressed(MouseButton::Right) {
        // Slide the target in the camera's screen plane, so the ground follows the
        // cursor. The further away the camera, the bigger each pixel's worth.
        let scale = goal.distance * PAN_SPEED;
        goal.target += (transform.left() * drag.x + transform.up() * drag.y) * scale;
    }

    // Trackpads report scrolling in pixels and mouse wheels in lines; normalise to lines.
    let lines = match scroll.unit {
        MouseScrollUnit::Line => scroll.delta.y,
        MouseScrollUnit::Pixel => scroll.delta.y / 100.0,
    };
    if lines != 0.0 {
        // Multiplicative zoom feels even at every distance.
        goal.distance =
            (goal.distance * (-lines * ZOOM_SPEED).exp()).clamp(MIN_DISTANCE, MAX_DISTANCE);
    }
}

/// Eases the camera towards its goal and writes the result into its `Transform`.
fn apply_orbit(camera: Single<(&mut OrbitCamera, &mut Transform)>, time: Res<Time>) {
    let (mut orbit_camera, mut transform) = camera.into_inner();
    // Frame-rate independent smoothing: the same fraction of the gap closes per
    // second whether the game runs at 30 or 144 FPS.
    let t = 1.0 - (-SMOOTHING_RATE * time.delta_secs()).exp();
    // Copy `goal` out first. Borrowing `orbit_camera.goal` while also mutably
    // borrowing `orbit_camera.current` through the same `Mut` wrapper isn't allowed.
    let goal = orbit_camera.goal;
    orbit_camera.current.approach(&goal, t);

    let current = orbit_camera.current;
    *transform = Transform::from_translation(current.eye()).looking_at(current.target, Vec3::Y);
}
