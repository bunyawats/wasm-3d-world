//! Orbit camera, matching the original's Three.js `OrbitControls`:
//! left-drag rotates, right-drag pans, the scroll wheel zooms, all with smoothing.
//! It can also follow a deer or a bird (`CameraFocus`), still orbitable around it.

use std::f32::consts::FRAC_PI_2;

use bevy::{
    core_pipeline::tonemapping::Tonemapping,
    ecs::system::SystemParam,
    input::mouse::{AccumulatedMouseScroll, MouseScrollUnit},
    picking::mesh_picking::MeshPickingCamera,
    prelude::*,
    window::PrimaryWindow,
};
use bevy_egui::input::EguiWantsInput;

use crate::{birds::FlockState, deer::HerdState, terrain::IslandTerrain};

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
/// Never let the camera dip closer than this to the ground (metres).
const GROUND_CLEARANCE: f32 = 0.6;
/// How far above the ground the orbit centre stays (metres).
const TARGET_HEIGHT: f32 = 1.0;
/// Where the camera sits relative to a creature when it starts following one
/// (the original's offsets), and how high above a deer's hooves it looks.
const DEER_VIEW_OFFSET: Vec3 = Vec3::new(-4.5, 2.6, 5.5);
const BIRD_VIEW_OFFSET: Vec3 = Vec3::new(0.0, 2.2, -5.5);
const DEER_FOCUS_HEIGHT: f32 = 1.35;

pub struct OrbitCameraPlugin;

impl Plugin for OrbitCameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CameraFocus>()
            .add_systems(Startup, spawn_camera)
            // `.chain()` runs these in order: read input, follow the focus, then
            // move the camera.
            .add_systems(Update, (orbit_input, follow_focus, apply_orbit).chain());
    }
}

/// What the camera is looking at.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CameraFocus {
    /// Free orbiting, controlled entirely by the mouse.
    #[default]
    Free,
    /// Following the deer with this index in `HerdState`.
    Deer(usize),
    /// Following the bird with this index in `FlockState`.
    Bird(usize),
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
    // The original's starting view: the eye at (55, 32, 70), looking at (0, 4, 0).
    // The meadow there is about 7 m high, so that point is inside the hill, but
    // it sets the view's tilt: aiming at the ground instead tips the camera up
    // and drops the horizon down the screen. The first pan lifts the orbit
    // centre out of the hill (see `orbit_input`), and `GROUND_CLEARANCE`
    // keeps the camera itself above ground.
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
        // The original's `THREE.FogExp2(…, 0.0042)`. Bevy doesn't draw this fog
        // itself: `r128_shading.wesl` reads the density and colour from it and
        // applies the fog r128's way. The colour is set by `apply_daylight`.
        DistanceFog {
            falloff: FogFalloff::ExponentialSquared { density: 0.0042 },
            ..default()
        },
        Transform::from_translation(orbit.eye()).looking_at(orbit.target, Vec3::Y),
        // Lets this camera's clicks pick creatures (see `MeshPickingSettings` in main).
        MeshPickingCamera,
        OrbitCamera {
            current: orbit,
            goal: orbit,
        },
    ));
}

/// Turns mouse input into changes to the camera's goal orbit.
/// The mouse state the camera reads, bundled into one system parameter (see
/// `HudWorld` in `ui.rs` for how `#[derive(SystemParam)]` works).
#[derive(SystemParam)]
struct Mouse<'w, 's> {
    buttons: Res<'w, ButtonInput<MouseButton>>,
    scroll: Res<'w, AccumulatedMouseScroll>,
    // `Single` borrows query state too, which is what the `'s` lifetime is for.
    window: Single<'w, 's, &'static Window, With<PrimaryWindow>>,
    /// Whether egui (the HUD) is using the pointer right now.
    egui: Res<'w, EguiWantsInput>,
}

fn orbit_input(
    camera: Single<(&mut OrbitCamera, &Transform)>,
    island: Res<IslandTerrain>,
    mouse: Mouse,
    mut focus: ResMut<CameraFocus>,
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
    let cursor = mouse.window.cursor_position();
    let drag = match (cursor, *last_cursor) {
        (Some(now), Some(before)) => now - before,
        _ => Vec2::ZERO, // Just entered the window: no movement yet.
    };
    *last_cursor = cursor;

    // Dragging a slider or scrolling a HUD panel shouldn't also move the camera.
    if mouse.egui.wants_any_pointer_input() {
        return;
    }

    if mouse.buttons.pressed(MouseButton::Left) {
        goal.yaw -= drag.x * ROTATE_SPEED;
        goal.pitch = (goal.pitch + drag.y * ROTATE_SPEED).clamp(MIN_PITCH, MAX_PITCH);
    }

    if mouse.buttons.pressed(MouseButton::Right) && drag != Vec2::ZERO {
        // Panning away from a followed creature means you want to look elsewhere.
        // Only write when it changes, so `focus.is_changed()` stays meaningful.
        if *focus != CameraFocus::Free {
            *focus = CameraFocus::Free;
        }
        // Slide the target in the camera's screen plane, so the ground follows the
        // cursor. The further away the camera, the bigger each pixel's worth.
        let scale = goal.distance * PAN_SPEED;
        goal.target += (transform.left() * drag.x + transform.up() * drag.y) * scale;
        // Don't let panning drag the orbit centre underground.
        let ground = island
            .0
            .height(goal.target.x, goal.target.z)
            .max(island.0.water_level);
        goal.target.y = goal.target.y.max(ground + TARGET_HEIGHT);
    }

    // Trackpads report scrolling in pixels and mouse wheels in lines; normalise to lines.
    let lines = match mouse.scroll.unit {
        MouseScrollUnit::Line => mouse.scroll.delta.y,
        MouseScrollUnit::Pixel => mouse.scroll.delta.y / 100.0,
    };
    if lines != 0.0 {
        // Multiplicative zoom feels even at every distance.
        goal.distance =
            (goal.distance * (-lines * ZOOM_SPEED).exp()).clamp(MIN_DISTANCE, MAX_DISTANCE);
    }
}

/// While following a creature, keeps the orbit centred on it. When the focus has
/// just changed, also jumps to the original's viewing offset for that creature.
fn follow_focus(
    focus: Res<CameraFocus>,
    herd: Res<HerdState>,
    flocks: Res<FlockState>,
    mut camera: Single<&mut OrbitCamera>,
) {
    // `Option` chaining: `get` returns `None` for an out-of-range index, and `map`
    // only runs on a `Some`. Either way the result is an `Option<(Vec3, Vec3)>`.
    let target = match *focus {
        CameraFocus::Free => None,
        CameraFocus::Deer(i) => herd.0.deer.get(i).map(|deer| {
            (
                deer.position + Vec3::Y * DEER_FOCUS_HEIGHT,
                DEER_VIEW_OFFSET,
            )
        }),
        CameraFocus::Bird(i) => flocks
            .0
            .birds
            .get(i)
            .map(|bird| (bird.position, BIRD_VIEW_OFFSET)),
    };
    let Some((target, offset)) = target else {
        return;
    };

    if focus.is_changed() {
        camera.goal = Orbit::looking_from(target + offset, target);
    } else {
        camera.goal.target = target;
    }
}

/// Eases the camera towards its goal and writes the result into its `Transform`.
fn apply_orbit(
    camera: Single<(&mut OrbitCamera, &mut Transform)>,
    island: Res<IslandTerrain>,
    time: Res<Time>,
) {
    let (mut orbit_camera, mut transform) = camera.into_inner();
    // Frame-rate independent smoothing: the same fraction of the gap closes per
    // second whether the game runs at 30 or 144 FPS.
    let t = 1.0 - (-SMOOTHING_RATE * time.delta_secs()).exp();
    // Copy `goal` out first. Borrowing `orbit_camera.goal` while also mutably
    // borrowing `orbit_camera.current` through the same `Mut` wrapper isn't allowed.
    let goal = orbit_camera.goal;
    orbit_camera.current.approach(&goal, t);

    let current = orbit_camera.current;
    // Close in, an orbit can swing the camera into a hillside. Lift it back out.
    // (The original's OrbitControls had no such check.)
    let mut eye = current.eye();
    let floor = island.0.height(eye.x, eye.z).max(island.0.water_level) + GROUND_CLEARANCE;
    eye.y = eye.y.max(floor);
    *transform = Transform::from_translation(eye).looking_at(current.target, Vec3::Y);
}
