//! Articulated birds: entity-tree rigs driven by `sim::Flocks`.
//!
//! Port of `createArticulatedBird()` and the bird half of the joint-sync loop in
//! `index.html`. Each wing has two segments: the inner wing hinges at the shoulder
//! and the outer wing at the wrist, so the tips can fold on the upstroke.

use std::f32::consts::PI;

use bevy::{picking::events::PointerClick, platform::time::Instant, prelude::*};
use sim::Flocks;

use crate::{
    camera::CameraFocus,
    rig::{Rig, at},
    terrain::IslandTerrain,
    ui::{SimTimings, Toast},
};

/// Seed for the flocks' randomness. Change it for a different sky.
const FLOCK_SEED: u64 = 7;
/// Birds per flock added by the B key or the HUD's "+ Bird Flock" button.
pub const FLOCK_SIZE: usize = 4;
const INNER_WING_LENGTH: f32 = 0.78;
const OUTER_WING_LENGTH: f32 = 0.92;

pub struct BirdPlugin;

impl Plugin for BirdPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_initial_flock).add_systems(
            Update,
            (
                spawn_flock_on_keypress,
                spawn_missing_rigs,
                step_flocks,
                sync_rigs,
            )
                .chain(),
        );
    }
}

#[derive(Resource)]
pub struct FlockState(pub Flocks);

/// Which bird in `FlockState` this rig shows.
#[derive(Component)]
pub struct BirdId(pub usize);

/// A display name like "Hawk #3" (the original called every bird a hawk).
pub fn bird_name(id: usize) -> String {
    format!("Hawk #{}", id + 1)
}

/// One bird rig's moving parts.
#[derive(Component)]
struct BirdRig {
    /// Rolls the whole body into turns, and carries the wingbeat bob.
    bank: Entity,
    /// `[left, right]` shoulder and wrist pivots.
    inner_wings: [Entity; 2],
    outer_wings: [Entity; 2],
}

/// Shared meshes and materials. A `Resource` (not a local, like `DeerAssets`)
/// because birds are spawned later too, whenever the B key adds a flock.
#[derive(Resource)]
struct BirdAssets {
    body: Handle<Mesh>,
    breast: Handle<Mesh>,
    head: Handle<Mesh>,
    beak: Handle<Mesh>,
    tail_feather: Handle<Mesh>,
    inner_wing: Handle<Mesh>,
    outer_wing: Handle<Mesh>,
    primary_feather: Handle<Mesh>,

    plumage: Handle<StandardMaterial>,
    breast_feathers: Handle<StandardMaterial>,
    wing_tip: Handle<StandardMaterial>,
    beak_colour: Handle<StandardMaterial>,
}

impl BirdAssets {
    fn new(meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>) -> Self {
        let mut material = |r: u8, g: u8, b: u8, roughness: f32| {
            materials.add(StandardMaterial {
                base_color: Color::srgb_u8(r, g, b),
                perceptual_roughness: roughness,
                ..default()
            })
        };
        Self {
            body: meshes.add(Sphere::new(0.32).mesh().uv(12, 10)),
            breast: meshes.add(Sphere::new(0.28).mesh().uv(10, 8)),
            head: meshes.add(Sphere::new(0.18).mesh().uv(10, 10)),
            beak: meshes.add(
                Cone {
                    radius: 0.065,
                    height: 0.28,
                }
                .mesh()
                .resolution(6),
            ),
            tail_feather: meshes.add(Cuboid::new(0.12, 0.02, 0.52)),
            inner_wing: meshes.add(Cuboid::new(INNER_WING_LENGTH, 0.045, 0.46)),
            outer_wing: meshes.add(Cuboid::new(OUTER_WING_LENGTH, 0.032, 0.38)),
            primary_feather: meshes.add(Cuboid::new(0.32, 0.02, 0.09)),
            plumage: material(0x3b, 0x28, 0x1e, 0.65),
            breast_feathers: material(0xd6, 0xcf, 0xc7, 0.75),
            wing_tip: material(0x17, 0x14, 0x12, 0.6),
            beak_colour: material(0xf5, 0x9e, 0x0b, 0.4),
        }
    }
}

fn spawn_initial_flock(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.insert_resource(BirdAssets::new(&mut meshes, &mut materials));
    // Rigs are created by `spawn_missing_rigs` on the first frame, the same path
    // that later handles birds added with the B key.
    commands.insert_resource(FlockState(Flocks::initial(FLOCK_SEED)));
}

/// Press B to add another flock (up to the sky's limit of 60 birds).
fn spawn_flock_on_keypress(
    keys: Res<ButtonInput<KeyCode>>,
    mut flocks: ResMut<FlockState>,
    mut toasts: MessageWriter<Toast>,
) {
    if keys.just_pressed(KeyCode::KeyB) {
        let added = flocks.0.spawn_flock(FLOCK_SIZE);
        toasts.write(Toast(format!(
            "Added {added} birds ({} in the sky)",
            flocks.0.birds.len()
        )));
    }
}

/// Clicking any part of a bird follows it. Same bubbling observer as the deer's.
fn follow_clicked_bird(
    click: On<PointerClick>,
    ids: Query<&BirdId>,
    mut focus: ResMut<CameraFocus>,
    mut toasts: MessageWriter<Toast>,
) {
    if click.button != PointerButton::Primary || click.duration.as_secs_f32() > 0.3 {
        return;
    }
    if let Ok(id) = ids.get(click.entity) {
        *focus = CameraFocus::Bird(id.0);
        toasts.write(Toast(format!("Following {}", bird_name(id.0))));
    }
}

/// Builds a rig for every simulated bird that doesn't have one yet.
///
/// Birds are only ever appended, so the IDs already rigged are exactly
/// `0..rigs.iter().len()`, and anything past that is new.
fn spawn_missing_rigs(
    mut commands: Commands,
    flocks: Res<FlockState>,
    assets: Res<BirdAssets>,
    rigs: Query<(), With<BirdId>>,
) {
    let rigged = rigs.iter().len();
    for id in rigged..flocks.0.birds.len() {
        spawn_rig(&mut commands, &assets, id);
    }
}

fn spawn_rig(commands: &mut Commands, assets: &BirdAssets, id: usize) {
    let root = commands
        .spawn((
            Name::new(bird_name(id)),
            BirdId(id),
            Transform::default(),
            Visibility::default(),
        ))
        .observe(follow_clicked_bird)
        .id();
    let mut rig = Rig {
        commands,
        pickable: true,
    };

    let bank = rig.pivot(root, Transform::default());
    rig.part(
        bank,
        &assets.body,
        &assets.plumage,
        Transform::from_scale(Vec3::new(0.78, 0.72, 1.85)),
    );
    rig.part(
        bank,
        &assets.breast,
        &assets.breast_feathers,
        at(0.0, -0.07, 0.05).with_scale(Vec3::new(0.72, 0.62, 1.55)),
    );

    let head = rig.pivot(bank, at(0.0, 0.10, 0.56));
    rig.part(
        head,
        &assets.head,
        &assets.breast_feathers,
        Transform::from_scale(Vec3::new(0.85, 0.9, 1.15)),
    );
    rig.part(
        head,
        &assets.beak,
        &assets.beak_colour,
        at(0.0, -0.02, 0.26).with_rotation(Quat::from_rotation_x(PI / 2.0 + 0.12)),
    );

    // Five tail feathers fanned out from one pivot. `-2..=2` is an *inclusive* range.
    let tail = rig.pivot(bank, at(0.0, 0.02, -0.54));
    for t in -2..=2 {
        let fan = Transform::from_rotation(Quat::from_rotation_y(t as f32 * 0.14));
        rig.offset_part(
            tail,
            &assets.tail_feather,
            &assets.plumage,
            fan,
            Vec3::new(0.0, 0.0, -0.24),
        );
    }

    // `side` is +1 for the left wing (+X) and -1 for the right; every X offset and
    // fan angle is mirrored by multiplying with it.
    let mut wing = |side: f32| {
        let inner = rig.pivot(bank, at(side * 0.20, 0.08, 0.12));
        rig.part(
            inner,
            &assets.inner_wing,
            &assets.plumage,
            at(side * INNER_WING_LENGTH / 2.0, 0.0, -0.04),
        );

        let outer = rig.pivot(inner, at(side * INNER_WING_LENGTH, 0.0, 0.0));
        rig.part(
            outer,
            &assets.outer_wing,
            &assets.wing_tip,
            at(side * OUTER_WING_LENGTH / 2.0, 0.0, -0.06),
        );
        for f in 0..3 {
            let f = f as f32; // Shadow the integer with a float of the same name.
            let placement = at(side * (OUTER_WING_LENGTH - 0.08), 0.0, 0.06 - f * 0.11)
                .with_rotation(Quat::from_rotation_y(-side * (0.12 + f * 0.14)));
            rig.offset_part(
                outer,
                &assets.primary_feather,
                &assets.wing_tip,
                placement,
                Vec3::new(side * 0.16, 0.0, 0.0),
            );
        }
        (inner, outer)
    };
    let (left_inner, left_outer) = wing(1.0);
    let (right_inner, right_outer) = wing(-1.0);

    commands.entity(root).insert(BirdRig {
        bank,
        inner_wings: [left_inner, right_inner],
        outer_wings: [left_outer, right_outer],
    });
}

fn step_flocks(
    mut flocks: ResMut<FlockState>,
    island: Res<IslandTerrain>,
    time: Res<Time>,
    mut timings: ResMut<SimTimings>,
) {
    let dt = time.delta_secs().min(0.1);
    let start = Instant::now();
    flocks.0.step(&island.0, dt);
    timings.record_flocks(start.elapsed());
}

/// Copies each simulated bird onto its rig. Same two-query pattern as the deer:
/// `Without<BirdId>` keeps the joint query disjoint from the root query.
fn sync_rigs(
    flocks: Res<FlockState>,
    mut roots: Query<(&BirdId, &BirdRig, &mut Transform)>,
    mut joints: Query<&mut Transform, Without<BirdId>>,
) {
    for (id, rig, mut transform) in &mut roots {
        let Some(bird) = flocks.0.birds.get(id.0) else {
            continue;
        };

        // The rig's beak points along +Z, but Bevy's `looking_to` aims -Z (the
        // camera convention). So aim -Z *away* from the flight direction, which
        // leaves +Z pointing along it.
        *transform = Transform::from_translation(bird.position)
            .looking_to(-bird.velocity, Vec3::Y)
            .with_scale(Vec3::splat(bird.scale));

        if let Ok(mut bank) = joints.get_mut(rig.bank) {
            bank.rotation = Quat::from_rotation_z(bird.bank);
            bank.translation.y = bird.body_bob;
        }
        // Mirror the angles for the right wing (index 1), so both wings beat
        // upwards together.
        for (i, side) in [1.0, -1.0].into_iter().enumerate() {
            if let Ok(mut inner) = joints.get_mut(rig.inner_wings[i]) {
                inner.rotation = Quat::from_rotation_z(side * bird.inner_wing);
            }
            if let Ok(mut outer) = joints.get_mut(rig.outer_wings[i]) {
                outer.rotation = Quat::from_rotation_z(side * bird.outer_wing);
            }
        }
    }
}
