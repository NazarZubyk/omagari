// Omagari effect shader: surface layers (overlay copies of a model's meshes),
// shape layers (Omagari's own meshes), trail ribbons (vertex colours carry the
// fade along the trail) and afterimage ghosts (copies of meshes with frozen
// joints). Rust side:
// `src/runtime/effect_material.rs` (`OmagariEffectMaterial`, `EffectUniforms`).
//
// BEVY UPGRADES: the vertex stage mirrors bevy_pbr 0.19.1
//   - `src/render/mesh.wgsl`            `vertex()` and `morph_vertex()`
//   - `src/render/skinning.wgsl`        `skin_model()`, `skin_normals()`
//   - `src/render/morph.wgsl`           `layer_count()`, `weight_at()`, `morph_*()`
//   - `src/render/mesh_functions.wgsl`  world-from-local, normal transforms
//   - `src/render/forward_io.wgsl`      the `Vertex` input (attribute locations)
// Bevy sets the shader defs SKINNED, MORPH_TARGETS, SKINS_USE_UNIFORM_BUFFERS,
// VERTEX_NORMALS, VERTEX_UVS_A, ... from the mesh (`setup_morph_and_skinning_defs`
// and `MeshPipeline::specialize` in `src/render/mesh.rs`). After an upgrade,
// diff those files against the version named here and port the changes. The
// fragment stage only uses `mesh_view_bindings::{view, globals}` and
// `bevy_core_pipeline::tonemapping` (non-HDR cameras).

#import bevy_pbr::{
    mesh_bindings::mesh,
    mesh_functions,
    skinning,
    morph::{morph_position, morph_normal, morph_tangent},
    forward_io::Vertex,
    view_transformations::position_world_to_clip,
    mesh_view_bindings::{view, globals},
}

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

struct EffectVertexOutput {
    @builtin(position) position: vec4<f32>,
    // Skinned world position, pushed out by the shell offset (not by the
    // depth push).
    @location(0) world_position: vec4<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    // Rest-pose (before skinning and morphing) position in pattern units:
    // metres of the model at fit scale 1. Patterns mapped from it stick to
    // the body while it moves.
    @location(3) rest_position: vec3<f32>,
    @location(4) rest_normal: vec3<f32>,
    // 0 at the model's low end along the fill axis, 1 at its high end.
    @location(5) fill_coord: f32,
    // The mesh's vertex colour (white without one): multiplies the colour
    // and opacity (trail fade along its length).
    @location(6) tint: vec4<f32>,
}

// Unit vector from a world position toward the camera (as
// `bevy_pbr::pbr_functions::calculate_view`): the view's back axis for
// orthographic projections, else toward the camera position.
fn direction_to_camera(world_position: vec3<f32>) -> vec3<f32> {
    if view.clip_from_view[3][3] == 1.0 {
        return normalize(view.world_from_view[2].xyz);
    }
    return normalize(view.world_position - world_position);
}

#ifdef MORPH_TARGETS
// Mirrors `morph_vertex` of bevy_pbr `render/mesh.wgsl`. The instance_index
// parameter must match vertex_in.instance_index (wgpu dx12 workaround, see
// https://github.com/gfx-rs/naga/issues/2416).
fn morph_vertex(vertex_in: Vertex, instance_index: u32) -> Vertex {
    var vertex = vertex_in;
    let first_vertex = mesh[instance_index].first_vertex_index;
    let vertex_index = vertex.index - first_vertex;

    let weight_count = bevy_pbr::morph::layer_count(instance_index);
    for (var i: u32 = 0u; i < weight_count; i ++) {
        let weight = bevy_pbr::morph::weight_at(i, instance_index);
        if weight == 0.0 {
            continue;
        }
        vertex.position += weight * morph_position(vertex_index, i, instance_index);
#ifdef VERTEX_NORMALS
        vertex.normal += weight * morph_normal(vertex_index, i, instance_index);
#endif
#ifdef VERTEX_TANGENTS
        vertex.tangent += vec4(weight * morph_tangent(vertex_index, i, instance_index), 0.0);
#endif
    }
    return vertex;
}
#endif

@vertex
fn vertex(vertex_no_morph: Vertex) -> EffectVertexOutput {
    var out: EffectVertexOutput;

#ifdef MORPH_TARGETS
    var vertex = morph_vertex(vertex_no_morph, vertex_no_morph.instance_index);
#else
    var vertex = vertex_no_morph;
#endif

    // As bevy_pbr `mesh.wgsl`: skinned meshes take their world transform
    // from the joints (the entity's own transform is not used).
    let mesh_world_from_local = mesh_functions::get_world_from_local(vertex_no_morph.instance_index);
#ifdef SKINNED
    var world_from_local = skinning::skin_model(
        vertex.joint_indices,
        vertex.joint_weights,
        vertex_no_morph.instance_index
    );
    // A blend of joint matrices is not a pure rotation, so the metres per
    // rest unit come from the CPU (one joint's matrix).
    let rest_scale = material.shape.w;
#else
    var world_from_local = mesh_world_from_local;
    let rest_scale = length(mesh_world_from_local[0].xyz);
#endif

#ifdef VERTEX_NORMALS
#ifdef SKINNED
    var world_normal = skinning::skin_normals(world_from_local, vertex.normal);
#else
    var world_normal = mesh_functions::mesh_normal_local_to_world(vertex.normal, vertex_no_morph.instance_index);
#endif
    let rest_normal = normalize(vertex_no_morph.normal);
#else
    var world_normal = vec3<f32>(0.0, 1.0, 0.0);
    let rest_normal = vec3<f32>(0.0, 1.0, 0.0);
#endif

    // Shells: pushed out along the skinned normal (metres, × fit).
    var world_position = mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(vertex.position, 1.0));
    world_position = vec4<f32>(world_position.xyz + world_normal * material.shape.x, 1.0);

    // A quad facing the camera (shape layers): its local X runs along the
    // view's right, Z along the view's up, around the entity's origin, with
    // the entity's X / Z scale.
    if material.fill_range.z > 0.5 {
        let centre = (world_from_local * vec4<f32>(0.0, 0.0, 0.0, 1.0)).xyz;
        let right = normalize(view.world_from_view[0].xyz);
        let up = normalize(view.world_from_view[1].xyz);
        let sx = length(world_from_local[0].xyz);
        let sz = length(world_from_local[2].xyz);
        world_position = vec4<f32>(centre + right * (vertex.position.x * sx) + up * (vertex.position.z * sz), 1.0);
        world_normal = direction_to_camera(centre);
    }
    out.world_position = world_position;
    out.world_normal = world_normal;
    // Drawn a little closer to the camera along the view ray (same pixel):
    // no z-fighting with the model under a shell at offset 0.
    let pushed = world_position.xyz + direction_to_camera(world_position.xyz) * material.shape.z;
    out.position = position_world_to_clip(pushed);

#ifdef VERTEX_UVS_A
    out.uv = vertex.uv;
#else
    out.uv = vec2<f32>(0.0);
#endif
#ifdef VERTEX_COLORS
    out.tint = vertex.color;
#else
    out.tint = vec4<f32>(1.0);
#endif

    let rest = vertex_no_morph.position;
    out.rest_position = rest * (rest_scale / max(material.shape.y, 0.0001));
    out.rest_normal = rest_normal;
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

// One channel of a texel: 0..3 = R, G, B, A; 4 = luminance.
fn pick_channel(c: vec4<f32>, channel: f32) -> f32 {
    let i = u32(channel + 0.5);
    if i == 0u {
        return c.r;
    } else if i == 1u {
        return c.g;
    } else if i == 2u {
        return c.b;
    } else if i == 3u {
        return c.a;
    }
    return dot(c.rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
}

// Samples a texture by UV (mapping 1) or triplanar in the rest pose (mapping
// 2); `shift` scrolls it. Both are sampled and one is selected, so every
// sample stays in uniform control flow (WGSL derivative rules).
fn sample_mapped(
    t: texture_2d<f32>,
    s: sampler,
    mapping: f32,
    scale: f32,
    shift: vec2<f32>,
    in: EffectVertexOutput,
) -> vec4<f32> {
    let by_uv = textureSample(t, s, in.uv * scale + shift);
    let p = in.rest_position * scale;
    var w = pow(abs(in.rest_normal), vec3<f32>(4.0));
    w = w / max(w.x + w.y + w.z, 0.0001);
    let x = textureSample(t, s, p.zy + shift);
    let y = textureSample(t, s, p.xz + shift);
    let z = textureSample(t, s, p.xy + shift);
    let triplanar = x * w.x + y * w.y + z * w.z;
    return select(triplanar, by_uv, mapping < 1.5);
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
