// Runs once per screen pixel, after tonemapping (see `UndoSrgbEncoding` in color.rs).
//
// Bevy's screen texture stores sRGB-encoded bytes: the GPU encodes whatever this
// shader returns. Three.js r128 sent its numbers to the screen without encoding.
// So decode here, and the GPU's encoding turns it back into the original number.

// What Bevy's full-screen triangle hands each pixel: its texture coordinate.
struct FullscreenVertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

// The tonemapped image (binding 2, the unused settings uniform, is left out).
@group(0) @binding(0) var screen_texture: texture_2d<f32>;
@group(0) @binding(1) var screen_sampler: sampler;

// The standard sRGB decoding curve: linear near black, a 2.4 power above.
fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    let low = c / 12.92;
    let high = pow((c + 0.055) / 1.055, vec3(2.4));
    return select(high, low, c <= vec3(0.04045));
}

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let color = textureSample(screen_texture, screen_sampler, in.uv);
    return vec4(srgb_to_linear(color.rgb), color.a);
}
