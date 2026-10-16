//! Colours the way Three.js r128 handled them, so the port looks like the original.
//!
//! Modern renderers (Bevy, and Three.js since r152) are *colour managed*: a hex
//! colour like `0x5c6658` is sRGB, a curve made for screens, so it's converted to
//! linear light before lighting, and the result is converted back to sRGB for the
//! screen. The original runs on r128 with the defaults of its day, which did
//! neither. It used the hex bytes as raw numbers (`0x5c` = 0.36), lit them, and
//! sent the result straight to the screen.
//!
//! That changes how everything looks. Mid and dark colours come out paler (0.36
//! instead of sRGB's 0.107), and colours blend evenly across a triangle, which is
//! why the original's snow line is a soft fade and Bevy's was a hard edge. To
//! match, this module:
//!
//! 1. gives Bevy raw colours: [`rgb`] and [`hex`] treat the bytes as linear values;
//! 2. sets the camera's exposure so the brightness going into tonemapping equals
//!    r128's (`renderer.toneMappingExposure = 1.05`, which r128 divides by 0.6);
//! 3. shades every mesh with [`IslandMaterial`], Bevy's standard material with the
//!    last step swapped for r128's: tonemap *first*, then mix in the fog (Bevy
//!    does it the other way round), using r128's `FogExp2` formula;
//! 4. adds a full-screen pass that undoes Bevy's final sRGB encoding, so the
//!    screen shows the tonemapped numbers as they are, like r128 did.
//!
//! Both renderers use the same "ACES fitted" tonemapping curve, so with those
//! steps the result matches.

use bevy::{
    asset::embedded_asset,
    camera::Exposure,
    core_pipeline::{
        Core3dSystems,
        fullscreen_material::{FullscreenMaterial, FullscreenMaterialPlugin},
        tonemapping::tonemapping,
    },
    ecs::schedule::ScheduleConfigs,
    ecs::system::BoxedSystem,
    pbr::{ExtendedMaterial, MaterialExtension},
    prelude::*,
    render::{
        RenderApp,
        extract_component::ExtractComponent,
        render_resource::{AsBindGroup, ShaderType},
    },
    shader::ShaderRef,
};

use crate::environment::AMBIENT_PER_UNIT;

/// The original's `renderer.toneMappingExposure`.
const TONE_MAPPING_EXPOSURE: f32 = 1.05;
/// r128's ACES tonemapping multiplies by `toneMappingExposure / 0.6` first.
const TONE_MAPPING_SCALE: f32 = TONE_MAPPING_EXPOSURE / 0.6;

pub struct R128ColorPlugin;

impl Plugin for R128ColorPlugin {
    fn build(&self, app: &mut App) {
        // Compiles `undo_srgb_encoding.wgsl` into the program, so it needs no
        // separate download. It's then available at "embedded://app/…".
        embedded_asset!(app, "undo_srgb_encoding.wgsl");
        embedded_asset!(app, "r128_shading.wesl");
        app.add_plugins((
            FullscreenMaterialPlugin::<UndoSrgbEncoding>::default(),
            MaterialPlugin::<IslandMaterial>::default(),
        ))
        .add_observer(set_up_camera_color);
    }
}

/// The original's colour from separate bytes, used raw (see the module docs).
///
/// A `const fn` can run at compile time, so this works in `const` items too.
pub const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::linear_rgb(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0)
}

/// The original's colour from a `0xRRGGBB` number (the format `sim` uses), used raw.
pub const fn hex(rgb_hex: u32) -> Color {
    // Shift each byte down to the bottom, then `as u8` keeps just the low 8 bits.
    rgb((rgb_hex >> 16) as u8, (rgb_hex >> 8) as u8, rgb_hex as u8)
}

/// Every mesh's material: Bevy's `StandardMaterial`, shaded the r128 way.
///
/// `ExtendedMaterial` wraps a base material and adds an *extension* that can
/// replace some of its shaders. A `type` alias just gives the long name a short one.
pub type IslandMaterial = ExtendedMaterial<StandardMaterial, R128Shading>;

/// Wraps a `StandardMaterial` so it's shaded like the original.
pub fn material(base: StandardMaterial) -> IslandMaterial {
    ExtendedMaterial {
        base,
        extension: R128Shading::default(),
    }
}

/// The extension half of [`IslandMaterial`]. It has no settings of its own; it
/// only swaps in `r128_shading.wesl` as the fragment shader.
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone, Default)]
pub struct R128Shading {}

impl MaterialExtension for R128Shading {
    fn fragment_shader() -> ShaderRef {
        "embedded://app/r128_shading.wesl".into()
    }
}

/// Gives each 3D camera r128's exposure and the pass that undoes sRGB encoding.
fn set_up_camera_color(add: On<Add<Camera3d>>, mut commands: Commands) {
    // Bevy's exposure scales all light by `2^-ev100 / 1.2`. We need one unit of the
    // original's light (`AMBIENT_PER_UNIT` cd/m²) to come out as
    // `TONE_MAPPING_SCALE`, so solve `2^-ev100 / 1.2 × AMBIENT_PER_UNIT = SCALE`.
    let ev100 = (AMBIENT_PER_UNIT / (1.2 * TONE_MAPPING_SCALE)).log2();
    commands
        .entity(add.entity)
        .insert((Exposure { ev100 }, UndoSrgbEncoding::default()));
}

/// Marks a camera for the full-screen pass in `undo_srgb_encoding.wgsl`.
///
/// `FullscreenMaterial` hands the shader one uniform value of this type. The
/// shader needs no settings, but a uniform can't be empty, and WebGL2 wants
/// uniforms in 16-byte blocks, so it carries one unused `Vec4`.
/// `ExtractComponent` copies it to the render world, where the pass runs.
#[derive(Component, ExtractComponent, Clone, Copy, ShaderType, Default)]
#[extract_app(RenderApp)]
struct UndoSrgbEncoding {
    unused: Vec4,
}

impl FullscreenMaterial for UndoSrgbEncoding {
    fn fragment_shader() -> ShaderRef {
        "embedded://app/undo_srgb_encoding.wgsl".into()
    }

    /// The default runs before tonemapping. This pass must see the tonemapped
    /// image, so it runs after. (The HUD draws after all post-processing, so
    /// the pass doesn't touch it.)
    fn schedule_configs(system: ScheduleConfigs<BoxedSystem>) -> ScheduleConfigs<BoxedSystem> {
        system.in_set(Core3dSystems::PostProcess).after(tonemapping)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_uses_the_bytes_as_raw_linear_values() {
        let color = LinearRgba::from(hex(0x336699));
        assert_eq!(
            [color.red, color.green, color.blue],
            [
                0x33 as f32 / 255.0,
                0x66 as f32 / 255.0,
                0x99 as f32 / 255.0
            ]
        );
    }
}
