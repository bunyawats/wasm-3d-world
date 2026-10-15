//! Shared helpers for building articulated creatures (deer, birds) out of child
//! entities.

use bevy::prelude::*;

/// Shorthand for a translation-only transform.
pub fn at(x: f32, y: f32, z: f32) -> Transform {
    Transform::from_xyz(x, y, z)
}

/// Helper for spawning a rig's child entities.
///
/// It holds `&mut Commands` for a while, which needs a *lifetime parameter*: `'a`
/// is the borrow of the `Commands` value, and `'w, 's` are the lifetimes `Commands`
/// itself carries (the world and system-state borrows). Most of the time Rust infers
/// lifetimes for you; structs holding references are where you have to name them.
pub struct Rig<'a, 'w, 's> {
    pub commands: &'a mut Commands<'w, 's>,
}

impl Rig<'_, '_, '_> {
    /// An invisible joint that other parts hang from.
    pub fn pivot(&mut self, parent: Entity, transform: Transform) -> Entity {
        self.commands
            .spawn((transform, Visibility::default(), ChildOf(parent)))
            .id()
    }

    /// A visible mesh attached to `parent`.
    pub fn part(
        &mut self,
        parent: Entity,
        mesh: &Handle<Mesh>,
        material: &Handle<StandardMaterial>,
        transform: Transform,
    ) {
        self.commands.spawn((
            Mesh3d(mesh.clone()),
            MeshMaterial3d(material.clone()),
            transform,
            ChildOf(parent),
        ));
    }

    /// A mesh that is first offset by `offset`, then placed and rotated by
    /// `transform`, like a Three.js geometry that was `translate()`d before its
    /// mesh was positioned and rotated.
    ///
    /// A single Bevy `Transform` always applies scale, then rotation, then
    /// translation. "Shift, *then* rotate about the original point" needs two
    /// levels: a pivot holding `transform`, and the mesh inside it holding `offset`.
    pub fn offset_part(
        &mut self,
        parent: Entity,
        mesh: &Handle<Mesh>,
        material: &Handle<StandardMaterial>,
        transform: Transform,
        offset: Vec3,
    ) {
        let pivot = self.pivot(parent, transform);
        self.part(pivot, mesh, material, Transform::from_translation(offset));
    }
}
