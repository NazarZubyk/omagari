// Omagari effect shader: surface layers (overlay copies of a model's meshes),
// shape layers (Omagari's own meshes), trail ribbons (vertex colours carry the
// fade along the trail) and afterimage ghosts (copies of meshes with frozen
// joints). Rust side:
// `src/runtime/effect_material.rs` (`OmagariEffectMaterial`, `EffectUniforms`).
//
// BEVY UPGRADES: the vertex stage lives in `omagari_vertex.wgsl` (the
// `omagari::vertex` library, shared with the distortion shader); it mirrors
// bevy_pbr's mesh vertex shader, see the notes there. The fragment stage only
// uses `mesh_view_bindings::{view, globals}` and
// `bevy_core_pipeline::tonemapping` (non-HDR cameras).

#import bevy_pbr::{
    forward_io::Vertex,
    mesh_view_bindings::{view, globals},
}
#import omagari::vertex::{EffectVertexOutput, VertexInputs, effect_vertex, direction_to_camera, sample_mapped, pick_channel}

#ifdef TONEMAP_IN_SHADER
#import bevy_core_pipeline::tonemapping::tone_mapping
#endif

// `EffectUniforms` (16 × vec4, same order).
struct OmagariEffect {
    color: vec4<f32>,
    rim_color: vec4<f32>,
    edge_color: vec4<f32>,
    // x opacity, y emissive, z flash, w fade
    params: vec4<f32>,
    // x power, y intensity
    rim: vec4<f32>,
    // x frequency, y amount
    pulse: vec4<f32>,
    // xy scroll, z scale, w strength
    noise_a: vec4<f32>,
    // x mapping (0 off, 1 uv, 2 triplanar), y channel, z shell noise offset, w contrast
    noise_a_mode: vec4<f32>,
    noise_b: vec4<f32>,
    noise_b_mode: vec4<f32>,
    // x mapping, y channel, z scale, w strength
    mask: vec4<f32>,
    // x on, y threshold, z edge width
    dissolve: vec4<f32>,
    // x on, y from, z to, w softness
    fill: vec4<f32>,
    // xyz axis; w 0 = along the axis, 1 = along U, 2 = along V
    fill_axis: vec4<f32>,
    // x low end, y length (rest-pose units), z 1 = quad facing the camera
    fill_range: vec4<f32>,
    // x shell offset (m), y fit scale, z depth push (m), w metres per rest unit (skinned)
    shape: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> material: OmagariEffect;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var noise_a_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var noise_a_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var noise_b_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var noise_b_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(5) var mask_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(6) var mask_sampler: sampler;

@vertex
fn vertex(vertex_no_morph: Vertex) -> EffectVertexOutput {
    let inputs = VertexInputs(
        material.shape.x,
        material.shape.z,
        material.shape.y,
        material.shape.w,
        material.fill_range.z,
    );
    var out = effect_vertex(vertex_no_morph, inputs);
    let rest = vertex_no_morph.position;
    out.fill_coord = (dot(rest, material.fill_axis.xyz) - material.fill_range.x) / max(material.fill_range.y, 0.0001);
    // Fill along the mesh's U or V coordinate (0..1).
    if material.fill_axis.w > 1.5 {
        out.fill_coord = out.uv.y;
    } else if material.fill_axis.w > 0.5 {
        out.fill_coord = out.uv.x;
    }
    return out;
}

// A noise value sharpened around middle grey (contrast 1 = unchanged).
fn contrast(n: f32, amount: f32) -> f32 {
    return clamp((n - 0.5) * amount + 0.5, 0.0, 1.0);
}

@fragment
fn fragment(in: EffectVertexOutput) -> @location(0) vec4<f32> {
    let time = globals.time;

    // Textures are sampled before any discard, in uniform control flow.
    let noise_a_shift = material.noise_a.xy * time + vec2<f32>(material.noise_a_mode.z);
    let noise_b_shift = material.noise_b.xy * time + vec2<f32>(material.noise_b_mode.z * 1.37);
    let noise_a_texel = sample_mapped(noise_a_texture, noise_a_sampler, material.noise_a_mode.x, material.noise_a.z, noise_a_shift, in);
    let noise_b_texel = sample_mapped(noise_b_texture, noise_b_sampler, material.noise_b_mode.x, material.noise_b.z, noise_b_shift, in);
    let mask_texel = sample_mapped(mask_texture, mask_sampler, material.mask.x, material.mask.z, vec2<f32>(0.0), in);

    var pattern = 1.0;
    var dissolve_noise = 1.0;
    if material.noise_b_mode.x > 0.5 {
        let n = contrast(pick_channel(noise_b_texel, material.noise_b_mode.y), material.noise_b_mode.w);
        pattern = pattern * mix(1.0, n, material.noise_b.w);
        dissolve_noise = n;
    }
    if material.noise_a_mode.x > 0.5 {
        let n = contrast(pick_channel(noise_a_texel, material.noise_a_mode.y), material.noise_a_mode.w);
        pattern = pattern * mix(1.0, n, material.noise_a.w);
        dissolve_noise = n;
    }

    let opacity = material.params.x;
    let emissive = material.params.y;
    let flash = material.params.z;
    let fade = material.params.w;

    var rgb = material.color.rgb * emissive * mix(1.0, pattern, 0.5) * in.tint.rgb;
    var alpha = material.color.a * opacity * pattern;

    // Rim / fresnel: brighter where the surface turns away from the camera;
    // the noise breaks it up like the rest of the layer.
    if material.rim.y > 0.0 {
        let n = normalize(in.world_normal);
        let v = direction_to_camera(in.world_position.xyz);
        let fresnel = pow(1.0 - clamp(dot(n, v), 0.0, 1.0), material.rim.x) * material.rim.y * pattern;
        rgb = rgb + material.rim_color.rgb * fresnel;
        alpha = alpha + fresnel * material.rim_color.a;
    }

    // Flash (trigger pulses): up to three times as bright, half-way to opaque.
    rgb = rgb * (1.0 + 2.0 * flash);
    alpha = alpha + (1.0 - clamp(alpha, 0.0, 1.0)) * 0.5 * clamp(flash, 0.0, 1.0);

    // Pulse: the opacity breathes between (1 - amount) and 1.
    if material.pulse.y > 0.0 {
        let wave = 0.5 - 0.5 * cos(6.2831853 * material.pulse.x * time);
        alpha = alpha * (1.0 - material.pulse.y * (1.0 - wave));
    }

    // Dissolve: gone where the noise is below the threshold, glowing edge.
    if material.dissolve.x > 0.5 {
        let threshold = material.dissolve.y;
        let d = dissolve_noise - threshold;
        if threshold >= 1.0 || (threshold > 0.0 && d < 0.0) {
            discard;
        }
        if threshold > 0.0 && material.dissolve.z > 0.0 {
            let edge = 1.0 - smoothstep(0.0, material.dissolve.z, d);
            rgb = mix(rgb, material.edge_color.rgb, edge);
            alpha = max(alpha, edge * material.edge_color.a);
        }
    }

    // Fill: a band of the model along the axis (0 = low end, 1 = high end).
    // The edges are soft inside the band, so `to` = 0 shows nothing and
    // `to` = 1 covers the model.
    if material.fill.x > 0.5 {
        let h = in.fill_coord;
        let soft = max(material.fill.w, 0.0001);
        let low = material.fill.y * (1.0 + soft) - soft;
        let high = material.fill.z * (1.0 + soft);
        let band = smoothstep(low, low + soft, h) * (1.0 - smoothstep(high - soft, high, h));
        alpha = alpha * band;
    }

    // Mask texture.
    if material.mask.x > 0.5 {
        let m = pick_channel(mask_texel, material.mask.y);
        alpha = alpha * mix(1.0, m, material.mask.w);
    }

    alpha = clamp(alpha * fade * in.tint.a, 0.0, 1.0);
    if alpha <= 0.0001 {
        discard;
    }

    var out = vec4<f32>(rgb, alpha);
#ifdef BLEND_PREMULTIPLIED_ALPHA
    // `AlphaMode::Add` (Bevy maps it to premultiplied blending): colour ×
    // alpha added, alpha 0 so the background is kept.
    out = vec4<f32>(rgb * alpha, 0.0);
#endif
#ifdef TONEMAP_IN_SHADER
    out = tone_mapping(out, view.color_grading);
#endif
    return out;
}
