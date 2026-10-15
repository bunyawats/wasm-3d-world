//! Articulated deer rigs: a tree of entities driven by `sim::Herd`.
//!
//! Each deer is a *hierarchy*: a root entity (position, heading, ground tilt) with
//! a body pivot below it, and hips, knees, neck, head, ears and tail below that.
//! A child's `Transform` is relative to its parent, so rotating a hip swings the
//! whole leg below it, knee and hoof included. That's what makes it a rig.
//!
//! Port of `createArticulatedDeer()` and the joint-sync part of
//! `stepSimulationWasmAndSync()` in `index.html`.

use std::f32::consts::PI;

use bevy::prelude::*;
use sim::{DeerKind, Herd, Pose};

use crate::{
    rig::{Rig, at},
    terrain::IslandTerrain,
};

/// Seed for the herd's placement. Change it to get a different herd.
const HERD_SEED: u64 = 2024;

pub struct DeerPlugin;

impl Plugin for DeerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_herd)
            .add_systems(Update, (startle_on_keypress, step_herd, sync_rigs).chain());
    }
}

/// The simulated herd, wrapped so it can be a Bevy resource (see `IslandTerrain`
/// for why the wrapper is needed).
#[derive(Resource)]
pub struct HerdState(pub Herd);

/// Which deer in `HerdState` this rig shows.
#[derive(Component)]
pub struct DeerId(pub usize);

/// The joint entities of one rig, so the sync system can find them directly.
///
/// `Entity` is just an ID (a small `Copy` value), so storing them is cheap. Looking
/// them up later with `Query::get_mut` is a fast indexed access, not a search.
#[derive(Component)]
struct DeerRig {
    body: Entity,
    neck: Entity,
    head: Entity,
    left_ear: Entity,
    right_ear: Entity,
    tail: Entity,
    /// Hip and knee pivots, in the order front-left, front-right, back-left, back-right.
    hips: [Entity; 4],
    knees: [Entity; 4],
}

/// Meshes and materials shared by every deer. `Handle`s are reference-counted IDs
/// into Bevy's asset storage, so cloning one is cheap and all deer share the same
/// GPU data.
struct DeerAssets {
    torso: Handle<Mesh>,
    chest: Handle<Mesh>,
    rump: Handle<Mesh>,
    belly: Handle<Mesh>,
    spot: Handle<Mesh>,
    tail_top: Handle<Mesh>,
    tail_under: Handle<Mesh>,
    neck: Handle<Mesh>,
    throat: Handle<Mesh>,
    cranium: Handle<Mesh>,
    snout: Handle<Mesh>,
    nose: Handle<Mesh>,
    eye: Handle<Mesh>,
    ear: Handle<Mesh>,
    antler_beam_lower: Handle<Mesh>,
    antler_beam_upper: Handle<Mesh>,
    /// Tines, in the order: brow, middle, crown 1, crown 2.
    antler_tines: [Handle<Mesh>; 4],
    leg_upper_front: Handle<Mesh>,
    leg_upper_back: Handle<Mesh>,
    leg_joint: Handle<Mesh>,
    leg_lower_front: Handle<Mesh>,
    leg_lower_back: Handle<Mesh>,
    hoof: Handle<Mesh>,

    stag_coat: Handle<StandardMaterial>,
    doe_coat: Handle<StandardMaterial>,
    fawn_coat: Handle<StandardMaterial>,
    belly_fur: Handle<StandardMaterial>,
    dark: Handle<StandardMaterial>,
    antler: Handle<StandardMaterial>,
}

impl DeerAssets {
    fn new(meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>) -> Self {
        // Small closures keep the shape list readable. Each one borrows `meshes`
        // mutably, so they're called one at a time below (Rust won't allow two
        // closures to hold a mutable borrow of `meshes` at once).
        let mut sphere = |r: f32, sectors: u32, stacks: u32| {
            meshes.add(Sphere::new(r).mesh().uv(sectors, stacks))
        };
        let torso = sphere(0.42, 14, 12);
        let chest = sphere(0.43, 12, 10);
        let rump = sphere(0.40, 12, 10);
        let belly = sphere(0.37, 10, 8);
        let spot = sphere(0.055, 6, 6);
        let cranium = sphere(0.20, 12, 10);
        let nose = sphere(0.065, 8, 8);
        let eye = sphere(0.038, 8, 8);
        let leg_joint = sphere(0.068, 8, 8);

        // A tapered cylinder, like Three.js's `CylinderGeometry(top, bottom, height)`.
        let mut frustum = |top: f32, bottom: f32, height: f32, sides: u32| {
            meshes.add(
                ConicalFrustum {
                    radius_top: top,
                    radius_bottom: bottom,
                    height,
                }
                .mesh()
                .resolution(sides),
            )
        };
        let neck = frustum(0.15, 0.25, 0.78, 10);
        let throat = frustum(0.12, 0.18, 0.70, 8);
        let snout = frustum(0.085, 0.15, 0.42, 9);
        let antler_beam_lower = frustum(0.022, 0.032, 0.42, 6);
        let antler_beam_upper = frustum(0.014, 0.022, 0.40, 6);
        let leg_upper_front = frustum(0.11, 0.068, 0.52, 8);
        let leg_upper_back = frustum(0.14, 0.068, 0.56, 8);
        let leg_lower_front = frustum(0.058, 0.040, 0.56, 8);
        let leg_lower_back = frustum(0.058, 0.040, 0.58, 8);
        let hoof = frustum(0.044, 0.056, 0.10, 7);

        let mut cone = |radius: f32, height: f32, sides: u32| {
            meshes.add(Cone { radius, height }.mesh().resolution(sides))
        };
        let tail_top = cone(0.09, 0.34, 7);
        let tail_under = cone(0.08, 0.30, 7);
        let ear = cone(0.075, 0.32, 7);
        let antler_tines = [
            cone(0.018, 0.24, 5),
            cone(0.016, 0.26, 5),
            cone(0.015, 0.25, 5),
            cone(0.014, 0.22, 5),
        ];

        let mut material = |hex: [u8; 3], roughness: f32, metallic: f32| {
            materials.add(StandardMaterial {
                base_color: Color::srgb_u8(hex[0], hex[1], hex[2]),
                perceptual_roughness: roughness,
                metallic,
                ..default()
            })
        };

        Self {
            torso,
            chest,
            rump,
            belly,
            spot,
            tail_top,
            tail_under,
            neck,
            throat,
            cranium,
            snout,
            nose,
            eye,
            ear,
            antler_beam_lower,
            antler_beam_upper,
            antler_tines,
            leg_upper_front,
            leg_upper_back,
            leg_joint,
            leg_lower_front,
            leg_lower_back,
            hoof,
            stag_coat: material([0x8c, 0x53, 0x27], 0.78, 0.05),
            doe_coat: material([0xa6, 0x6a, 0x38], 0.75, 0.05),
            fawn_coat: material([0xb8, 0x7a, 0x44], 0.75, 0.05),
            belly_fur: material([0xe2, 0xce, 0xb5], 0.85, 0.0),
            dark: material([0x1e, 0x1b, 0x18], 0.4, 0.1),
            antler: material([0xdc, 0xd3, 0xc2], 0.6, 0.08),
        }
    }

    fn coat(&self, kind: DeerKind) -> Handle<StandardMaterial> {
        match kind {
            DeerKind::Stag => self.stag_coat.clone(),
            DeerKind::Doe => self.doe_coat.clone(),
            DeerKind::Fawn => self.fawn_coat.clone(),
        }
    }
}

fn spawn_herd(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    island: Res<IslandTerrain>,
) {
    let assets = DeerAssets::new(&mut meshes, &mut materials);
    let mut herd = Herd::initial(&island.0, HERD_SEED);

    for id in 0..herd.deer.len() {
        let kind = herd.deer[id].kind;
        spawn_rig(&mut commands, &assets, &mut herd.rng, id, kind);
    }
    commands.insert_resource(HerdState(herd));
}

/// Spawns one deer's entity tree and returns the root entity.
fn spawn_rig(
    commands: &mut Commands,
    assets: &DeerAssets,
    rng: &mut sim::Rng,
    id: usize,
    kind: DeerKind,
) -> Entity {
    let coat = assets.coat(kind);

    // The root entity. `sync_rigs` overwrites its transform every frame.
    let root = commands
        .spawn((
            Name::new(format!("{kind:?} #{}", id + 1)),
            DeerId(id),
            Transform::default(),
            Visibility::default(),
        ))
        .id();

    // `Rig` (in `rig.rs`) is a tiny helper that spawns children of a given
    // parent, so each body part is one readable line.
    let mut rig = Rig { commands };

    let body = rig.pivot(root, Transform::from_xyz(0.0, sim::deer::BODY_HEIGHT, 0.0));
    rig.part(
        body,
        &assets.torso,
        &coat,
        Transform::from_scale(Vec3::new(0.88, 0.92, 1.95)),
    );
    rig.part(
        body,
        &assets.chest,
        &coat,
        at(0.0, 0.04, 0.52).with_scale(Vec3::new(0.92, 1.04, 1.05)),
    );
    rig.part(
        body,
        &assets.rump,
        &coat,
        at(0.0, 0.05, -0.55).with_scale(Vec3::new(0.90, 0.98, 1.0)),
    );
    rig.part(
        body,
        &assets.belly,
        &assets.belly_fur,
        at(0.0, -0.14, -0.02).with_scale(Vec3::new(0.76, 0.65, 1.75)),
    );

    if kind == DeerKind::Fawn {
        // Ten pale spots, alternating sides. `step_by`-free trick: `s % 2`.
        for s in 0..10 {
            let side = if s % 2 == 0 { 1.0 } else { -1.0 };
            let spot = at(
                side * rng.range(0.31..0.35),
                0.1 + rng.range(-0.1..0.1),
                rng.range(-0.45..0.45),
            );
            rig.part(body, &assets.spot, &assets.belly_fur, spot);
        }
    }

    let tail = rig.pivot(body, at(0.0, 0.22, -0.88));
    let tail_angle = Quat::from_rotation_x(PI * 0.72);
    rig.part(
        tail,
        &assets.tail_top,
        &coat,
        at(0.0, -0.1, -0.1).with_rotation(tail_angle),
    );
    rig.part(
        tail,
        &assets.tail_under,
        &assets.belly_fur,
        at(0.0, -0.12, -0.08).with_rotation(tail_angle),
    );

    // Neck and head.
    let neck = rig.pivot(body, at(0.0, 0.24, 0.76));
    rig.part(neck, &assets.neck, &coat, at(0.0, 0.34, 0.0));
    rig.part(neck, &assets.throat, &assets.belly_fur, at(0.0, 0.34, 0.06));

    let head = rig.pivot(neck, at(0.0, 0.70, 0.04));
    rig.part(
        head,
        &assets.cranium,
        &coat,
        Transform::from_scale(Vec3::new(0.85, 0.90, 1.15)),
    );
    rig.part(
        head,
        &assets.snout,
        &coat,
        at(0.0, -0.04, 0.26).with_rotation(Quat::from_rotation_x(PI / 2.0)),
    );
    rig.part(
        head,
        &assets.nose,
        &assets.dark,
        at(0.0, -0.02, 0.47).with_scale(Vec3::new(1.1, 0.85, 0.7)),
    );
    rig.part(head, &assets.eye, &assets.dark, at(0.145, 0.05, 0.10));
    rig.part(head, &assets.eye, &assets.dark, at(-0.145, 0.05, 0.10));

    // Ears: a pivot at the base, the cone shifted up so it rotates about its root.
    let mut ear = |side: f32| {
        let pivot = rig.pivot(head, at(side * 0.13, 0.14, -0.06));
        rig.part(
            pivot,
            &assets.ear,
            &coat,
            at(0.0, 0.16, 0.0).with_scale(Vec3::new(0.55, 1.0, 1.0)),
        );
        pivot
    };
    let left_ear = ear(1.0);
    let right_ear = ear(-1.0);

    if kind == DeerKind::Stag {
        for side in [1.0, -1.0] {
            spawn_antler(&mut rig, assets, head, side);
        }
    }

    // Legs: hip pivot → upper leg; knee pivot → lower leg and hoof.
    // `let [a, b, c, d] = ...` destructures an array of tuples into named pieces.
    let leg_specs = [
        (Vec3::new(0.24, -0.14, 0.56), true),
        (Vec3::new(-0.24, -0.14, 0.56), true),
        (Vec3::new(0.25, -0.10, -0.58), false),
        (Vec3::new(-0.25, -0.10, -0.58), false),
    ];
    let legs = leg_specs
        .map(|(hip_at, is_front)| spawn_leg(&mut rig, assets, &coat, body, hip_at, is_front));

    commands.entity(root).insert(DeerRig {
        body,
        neck,
        head,
        left_ear,
        right_ear,
        tail,
        hips: legs.map(|(hip, _)| hip),
        knees: legs.map(|(_, knee)| knee),
    });
    root
}

/// Returns `(hip_pivot, knee_pivot)`.
fn spawn_leg(
    rig: &mut Rig,
    assets: &DeerAssets,
    coat: &Handle<StandardMaterial>,
    body: Entity,
    hip_at: Vec3,
    is_front: bool,
) -> (Entity, Entity) {
    let (upper_len, lower_len) = if is_front { (0.52, 0.56) } else { (0.56, 0.58) };
    let (upper, lower) = if is_front {
        (&assets.leg_upper_front, &assets.leg_lower_front)
    } else {
        (&assets.leg_upper_back, &assets.leg_lower_back)
    };

    let hip = rig.pivot(body, Transform::from_translation(hip_at));
    // Shift each bone down by half its length so it hangs from its pivot.
    rig.part(hip, upper, coat, at(0.0, -upper_len / 2.0, 0.0));
    rig.part(hip, &assets.leg_joint, coat, at(0.0, -upper_len, 0.0));

    let knee = rig.pivot(hip, at(0.0, -upper_len, 0.0));
    rig.part(knee, lower, coat, at(0.0, -lower_len / 2.0, 0.0));
    rig.part(
        knee,
        &assets.hoof,
        &assets.dark,
        at(0.0, -lower_len - 0.04, 0.015),
    );
    (hip, knee)
}

fn spawn_antler(rig: &mut Rig, assets: &DeerAssets, head: Entity, side: f32) {
    let antler = rig.pivot(head, at(side * 0.09, 0.17, -0.04));
    let mat = &assets.antler;
    // (mesh, position, rotation about x, rotation about z), from the original.
    let pieces = [
        (
            &assets.antler_beam_lower,
            Vec3::new(0.09, 0.18, -0.08),
            -0.35,
            -side * 0.42,
        ),
        (
            &assets.antler_beam_upper,
            Vec3::new(0.22, 0.45, -0.12),
            0.22,
            -side * 0.18,
        ),
        (
            &assets.antler_tines[0],
            Vec3::new(0.09, 0.16, 0.05),
            0.85,
            -side * 0.2,
        ),
        (
            &assets.antler_tines[1],
            Vec3::new(0.20, 0.36, 0.02),
            0.55,
            -side * 0.35,
        ),
        (
            &assets.antler_tines[2],
            Vec3::new(0.27, 0.68, -0.04),
            0.25,
            -side * 0.1,
        ),
        (
            &assets.antler_tines[3],
            Vec3::new(0.22, 0.65, -0.20),
            -0.3,
            side * 0.15,
        ),
    ];
    for (mesh, offset, rx, rz) in pieces {
        let position = Vec3::new(side * offset.x, offset.y, offset.z);
        let rotation = Quat::from_euler(EulerRot::XYZ, rx, 0.0, rz);
        rig.part(
            antler,
            mesh,
            mat,
            Transform::from_translation(position).with_rotation(rotation),
        );
    }
}

/// Press S to startle the herd into a trot (the original's "WASM Startle" button;
/// on-screen buttons arrive with the HUD in milestone 7).
fn startle_on_keypress(keys: Res<ButtonInput<KeyCode>>, mut herd: ResMut<HerdState>) {
    if keys.just_pressed(KeyCode::KeyS) {
        herd.0.startle();
    }
}

fn step_herd(mut herd: ResMut<HerdState>, island: Res<IslandTerrain>, time: Res<Time>) {
    // Clamp the time step like the original (`Math.min(delta, 0.1)`), so a long
    // pause (a backgrounded browser tab, say) can't make deer leap.
    let dt = time.delta_secs().min(0.1);
    herd.0.step(&island.0, dt, time.elapsed_secs());
}

/// Copies the simulation's state onto each rig's entities.
///
/// Both queries touch `Transform`, one for roots and one for joints. Bevy only
/// allows that if they can never match the same entity, which `Without<DeerId>`
/// guarantees (joints never have a `DeerId`). Without it, Bevy refuses to run
/// the system, because it would be two mutable borrows of the same data.
fn sync_rigs(
    herd: Res<HerdState>,
    mut roots: Query<(&DeerId, &DeerRig, &mut Transform)>,
    mut joints: Query<&mut Transform, Without<DeerId>>,
) {
    for (id, rig, mut transform) in &mut roots {
        let Some(deer) = herd.0.deer.get(id.0) else {
            continue;
        };

        // Stand the deer on the slope: "up" is the ground normal, "forward" is the
        // heading, flattened onto the slope. These three axes form its rotation.
        let up = deer.ground_normal.normalize();
        let forward = Vec3::new(deer.heading.sin(), 0.0, deer.heading.cos());
        let right = up.cross(forward).normalize();
        let forward = right.cross(up).normalize();
        *transform = Transform {
            translation: deer.position,
            rotation: Quat::from_mat3(&Mat3::from_cols(right, up, forward)),
            scale: Vec3::splat(deer.kind.scale()),
        };

        let pose: &Pose = &deer.pose;
        if let Ok(mut body) = joints.get_mut(rig.body) {
            body.translation.y = pose.body_lift;
            body.rotation = Quat::from_rotation_x(pose.body_pitch);
        }
        set_rotation(
            &mut joints,
            rig.neck,
            Quat::from_rotation_x(pose.neck_pitch),
        );
        set_rotation(
            &mut joints,
            rig.head,
            Quat::from_euler(EulerRot::XYZ, pose.head_pitch, pose.head_yaw, 0.0),
        );
        set_rotation(
            &mut joints,
            rig.left_ear,
            Quat::from_euler(EulerRot::XYZ, -0.25, 0.2, pose.left_ear_roll),
        );
        set_rotation(
            &mut joints,
            rig.right_ear,
            Quat::from_euler(EulerRot::XYZ, -0.25, -0.2, pose.right_ear_roll),
        );
        set_rotation(
            &mut joints,
            rig.tail,
            Quat::from_euler(EulerRot::XYZ, pose.tail_pitch, 0.0, pose.tail_yaw),
        );

        let legs = deer.legs;
        let angles = [
            legs.front_left,
            legs.front_right,
            legs.back_left,
            legs.back_right,
        ];
        // `zip` three sequences together: hip entity, knee entity, and angles.
        for ((&hip, &knee), leg) in rig.hips.iter().zip(&rig.knees).zip(angles) {
            set_rotation(&mut joints, hip, Quat::from_rotation_x(leg.hip));
            set_rotation(&mut joints, knee, Quat::from_rotation_x(leg.knee));
        }
    }
}

fn set_rotation(
    joints: &mut Query<&mut Transform, Without<DeerId>>,
    entity: Entity,
    rotation: Quat,
) {
    if let Ok(mut transform) = joints.get_mut(entity) {
        transform.rotation = rotation;
    }
}
