// Omagari distortion shader (heat haze, shockwave ripples, aura shimmer).
// Rust side: `src/runtime/distortion.rs` (`OmagariDistortionMaterial`,
// `DistortionUniforms`).
//
// The material reads Bevy's screen-space transmission texture
// (`mesh_view_bindings::view_transmission_texture`): a copy of the main
// colour target taken after the opaque pass, so it holds the opaque scene
// only (no transparent particles or overlays). The material is drawn in
// Bevy's `Transmissive3d` phase (an opaque alpha mode that reads the
// transmission texture), writes no depth, and outputs the copy sampled at the
// pixel plus an offset from scrolling noise. Where the weight is 0 the output
// equals what was there, so the volume's edges are invisible.
//
// BEVY UPGRADES: the vertex stage is `omagari::vertex` (see
// `omagari_vertex.wgsl`); this file uses `mesh_view_bindings::{view, globals,
// view_transmission_texture, view_transmission_sampler}` (bevy_pbr 0.19.1
// `render/mesh_view_bindings.wgsl`, bindings 24 / 25).

#import bevy_pbr::{
    forward_io::Vertex,
    mesh_view_bindings::{view, globals, view_transmission_texture, view_transmission_sampler},
}
#import omagari::vertex::{EffectVertexOutput, VertexInputs, effect_vertex, direction_to_camera, sample_mapped, pick_channel}

// `DistortionUniforms` (8 × vec4, same order).
struct OmagariDistortion {
    // x strength (largest offset, fraction of the view height), y edge
    // softness, z edge mode (0 = strongest in the middle, 1 = rim), w fade
    // (layer fade × visibility)
    params: vec4<f32>,
    // rgb tint (multiplies what is behind), a amount
    tint: vec4<f32>,
    // xy scroll (texture squares / s), z scale, w weight (0 = off)
    noise_a: vec4<f32>,
    // x mapping (1 uv, 2 triplanar), y channel, z contrast
    noise_a_mode: vec4<f32>,
    noise_b: vec4<f32>,
    noise_b_mode: vec4<f32>,
    // x shell offset (m, × fit), y fit scale, z depth push (m), w metres per
    // rest unit (skinned)
    shape: vec4<f32>,
    // x refraction (0..1: share of the offset along the surface's screen
    // direction instead of the noise), yzw reserved
    extra: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> material: OmagariDistortion;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var noise_a_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var noise_a_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var noise_b_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var noise_b_sampler: sampler;

@vertex
fn vertex(vertex_no_morph: Vertex) -> EffectVertexOutput {
    let inputs = VertexInputs(material.shape.x, material.shape.z, material.shape.y, material.shape.w, 0.0);
    return effect_vertex(vertex_no_morph, inputs);
}

fn contrast(n: f32, amount: f32) -> f32 {
    return clamp((n - 0.5) * amount + 0.5, 0.0, 1.0);
}

// A 2D offset in -1..1 from one noise texture: two lookups of its channel,
// the second shifted and scrolled the other way, so x and y wobble apart.
fn noise_offset(
    t: texture_2d<f32>,
    s: sampler,
    settings: vec4<f32>,
    mode: vec4<f32>,
    time: f32,
    in: EffectVertexOutput,
) -> vec2<f32> {
    let shift = settings.xy * time;
    let a = sample_mapped(t, s, mode.x, settings.z, shift, in);
    let b = sample_mapped(t, s, mode.x, settings.z * 0.83, vec2<f32>(0.37, 0.61) + shift * vec2<f32>(-0.7, 1.13), in);
    let x = contrast(pick_channel(a, mode.y), mode.z) * 2.0 - 1.0;
    let y = contrast(pick_channel(b, mode.y), mode.z) * 2.0 - 1.0;
    return vec2<f32>(x, y) * settings.w;
}

@fragment
fn fragment(in: EffectVertexOutput) -> @location(0) vec4<f32> {
    let time = globals.time;

    // Every texture is sampled in uniform control flow.
    let offset_a = noise_offset(noise_a_texture, noise_a_sampler, material.noise_a, material.noise_a_mode, time, in);
    let offset_b = noise_offset(noise_b_texture, noise_b_sampler, material.noise_b, material.noise_b_mode, time, in);
    var offset = offset_a + offset_b;
    let total = material.noise_a.w + material.noise_b.w;
    if total > 1.0 {
        offset = offset / total;
    }

    let n = normalize(in.world_normal);
    // A lens: the surface's direction on screen (outward, longest at the
    // outline), mixed in by `refraction`.
    let n_view = (view.view_from_world * vec4<f32>(n, 0.0)).xy;
    offset = mix(offset, vec2<f32>(n_view.x, -n_view.y), clamp(material.extra.x, 0.0, 1.0));

    // Edge falloff: how directly the surface faces the camera (1 in the
    // middle of a sphere or along a capsule's axis, 0 at its outline).
    let v = direction_to_camera(in.world_position.xyz);
    let facing = clamp(abs(dot(n, v)), 0.0, 1.0);
    let soft = clamp(material.params.y, 0.0001, 0.9999);
    var weight = smoothstep(0.0, soft, facing);
    if material.params.z > 0.5 {
        // Rim: strongest at `soft` from the outline, fading toward the
        // outline and toward the middle (a ring on a sphere).
        weight = weight * (1.0 - smoothstep(soft, 1.0, facing));
    }
    weight = weight * clamp(material.params.w, 0.0, 1.0);

    // Offset in pixels: a fraction of the view height (the same on every
    // projection), the same in x and y.
    let pixels = offset * material.params.x * weight * view.viewport.w;
    let full_size = vec2<f32>(textureDimensions(view_transmission_texture));
    let lo = view.viewport.xy + vec2<f32>(0.5);
    let hi = view.viewport.xy + view.viewport.zw - vec2<f32>(0.5);
    let sample_at = clamp(in.position.xy + pixels, lo, hi) / full_size;
    var behind = textureSampleLevel(view_transmission_texture, view_transmission_sampler, sample_at, 0.0);

    // The copy is already exposed (and tonemapped on non-HDR cameras):
    // written back as it is.
    let tint = mix(vec3<f32>(1.0), material.tint.rgb, clamp(material.tint.a * weight, 0.0, 1.0));
    return vec4<f32>(behind.rgb * tint, 1.0);
}
