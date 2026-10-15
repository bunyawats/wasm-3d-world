//! Bevy frontend for the low-poly island.
//!
//! Bevy apps are built from *plugins*. Each module below exposes one plugin that
//! registers its own systems and resources, so `main` reads like a table of contents.
//!
//! Bevy in one paragraph: the world holds *entities* (plain IDs), each with a set of
//! *components* (plain structs, like `Transform` or our `OrbitCamera`). *Systems* are
//! ordinary functions whose parameters say what data they need (`Query`, `Res`, ...).
//! Bevy calls them for you, every frame (`Update`) or once at launch (`Startup`).

mod camera;
mod environment;
mod terrain;
mod water;

use bevy::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "WASM 3D World (Rust + Bevy)".into(),
                // In the browser, render into `<canvas id="bevy">` from index.html,
                // and keep it sized to its parent element.
                canvas: Some("#bevy".into()),
                fit_canvas_to_parent: true,
                ..default()
            }),
            ..default()
        }))
        .add_plugins((
            terrain::TerrainPlugin,
            water::WaterPlugin,
            environment::EnvironmentPlugin,
            camera::OrbitCameraPlugin,
        ))
        .run();
}
