// The fragment shader for `IslandMaterial` (see color.rs): Bevy's standard PBR
// shader, with two r128 steps of its own:
// - the fill light (ambient + hemisphere) is added here with r128's formula;
// - Bevy mixes in fog and then tonemaps; Three.js r128, like the original,
//   tonemaps and then mixes in fog. Doing it in r128's order lets the fog use the
//   original's colours as they are.
//
// This is WESL, a superset of WGSL that adds `import` (and `@if` for features that
// may be switched off). The imports reuse Bevy's own shader functions.

import bevy_pbr::render::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, apply_pbr_lighting},
    pbr_types::{STANDARD_MATERIAL_FLAGS_UNLIT_BIT, STANDARD_MATERIAL_FLAGS_FOG_ENABLED_BIT},
    mesh_view_bindings::{view, fog},
};
import bevy_core_pipeline::tonemapping::tone_mapping;

@if(PREMULTIPLY_ALPHA)
import bevy_pbr::render::pbr_functions::premultiply_alpha;

// `R128Shading::fill_light` (binding 100): ambient, sky and ground colours in
// pixels 0, 1 and 2. `constants::MATERIAL_BIND_GROUP` is the material's bind
// group number, which Bevy fills in (Bevy's own shaders use it the same way).
@group(constants::MATERIAL_BIND_GROUP) @binding(100) var fill_light: texture_2d<f32>;

// r128's fill light on a surface: `AmbientLight` plus `HemisphereLight`, which
// blends from the ground colour (facing down) to the sky colour (facing up). Both
// light only the matte ("diffuse") part of the surface, and fully. Bevy's own
// ambient light takes away whatever a surface reflects as specular first, which
// made smooth, metallic surfaces seen edge-on (the far sea) up to 30% darker.
fn r128_fill_light(normal: vec3<f32>, base_color: vec3<f32>, metallic: f32) -> vec3<f32> {
    let ambient = textureLoad(fill_light, vec2(0, 0), 0).rgb;
    let sky = textureLoad(fill_light, vec2(1, 0), 0).rgb;
    let ground = textureLoad(fill_light, vec2(2, 0), 0).rgb;
    let irradiance = ambient + mix(ground, sky, 0.5 * normal.y + 0.5);
    // Metals have no matte part; r128 and Bevy both scale it by (1 − metallic).
    let diffuse_color = base_color * (1.0 - metallic);
    // Bevy multiplies all light by the camera's exposure; do the same here.
    return diffuse_color * irradiance * view.exposure;
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    // From here to the lighting, this is exactly what Bevy's standard material does.
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    pbr_input.material.base_color = alpha_discard(
        pbr_input.material.flags,
        pbr_input.material.alpha_cutoff,
        pbr_input.material.base_color,
    );
    let flags = pbr_input.material.flags;

    var color: vec4<f32>;
    if (flags & STANDARD_MATERIAL_FLAGS_UNLIT_BIT) == 0u {
        color = apply_pbr_lighting(pbr_input);
        color = vec4(color.rgb + r128_fill_light(
            pbr_input.N,
            pbr_input.material.base_color.rgb,
            pbr_input.material.metallic,
        ), color.a);
    } else {
        color = pbr_input.material.base_color;
    }

    // 1. Tonemap, with the camera's curve (ACES fitted, the same as r128's).
    color = tone_mapping(color, view.color_grading);

    // 2. Fog, as r128's `FogExp2`: the fog amount grows with the square of the
    //    distance *along the view direction* (not straight-line distance), and
    //    the fog colour is used as is. The camera's `DistanceFog` supplies the
    //    colour and density; `be.x` is the density.
    if (flags & STANDARD_MATERIAL_FLAGS_FOG_ENABLED_BIT) != 0u {
        let view_position = view.view_from_world * vec4(pbr_input.world_position.xyz, 1.0);
        let depth = -view_position.z; // The camera looks down its own −Z axis.
        let density = fog.be.x;
        let amount = 1.0 - exp(-density * density * depth * depth);
        color = vec4(mix(color.rgb, fog.base_color.rgb, amount), color.a);
    }

    // Transparent materials (the water, clouds) need their colour multiplied by
    // their alpha before blending; Bevy's own shader does this last, too.
@if(PREMULTIPLY_ALPHA) {
    color = premultiply_alpha(flags, color);
}

    var out: FragmentOutput;
    out.color = color;
    return out;
}
