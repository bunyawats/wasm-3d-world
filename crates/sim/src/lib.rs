//! Engine-independent simulation for the low-poly island.
//!
//! This crate is a port of the hand-assembled WebAssembly kernels in `index.html`.
//! It has no rendering code: the Bevy app reads state from here and draws it.
//!
//! A `lib.rs` is the root of a *library* crate. Each `pub mod` below pulls in a file
//! of the same name (`terrain.rs`) and makes it visible to users of the crate.

pub mod brain;
pub mod deer;
pub mod herd;
pub mod rng;
pub mod terrain;

// Re-export the main types at the crate root, so callers can write
// `sim::Terrain` instead of `sim::terrain::Terrain`.
pub use brain::{Obstacle, TROT_SPEED, WALK_SPEED};
pub use deer::{Behaviour, Deer, DeerKind, Leg, Legs, Pose};
pub use herd::Herd;
pub use rng::Rng;
pub use terrain::{Surface, Terrain};
