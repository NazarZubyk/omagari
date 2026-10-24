// Omagari's shared vertex stage (`#import omagari::vertex`): skinning, morph
// targets, shells pushed along the normal, camera-facing quads, the depth
// push toward the camera and the rest-pose position for patterns. Used by
// the effect shader (`omagari_effect.wgsl`) and the distortion shader
// (`omagari_distortion.wgsl`). Rust side: `src/runtime/effect_material.rs`
// (embedded and loaded as a shader library).
//
// BEVY UPGRADES: mirrors bevy_pbr 0.19.1
//   - `src/render/mesh.wgsl`            `vertex()` and `morph_vertex()`
//   - `src/render/skinning.wgsl`        `skin_model()`, `skin_normals()`
//   - `src/render/morph.wgsl`           `layer_count()`, `weight_at()`, `morph_*()`
//   - `src/render/mesh_functions.wgsl`  world-from-local, normal transforms
//   - `src/render/forward_io.wgsl`      the `Vertex` input (attribute locations)
// Bevy sets the shader defs SKINNED, MORPH_TARGETS, SKINS_USE_UNIFORM_BUFFERS,
// VERTEX_NORMALS, VERTEX_UVS_A, ... from the mesh (`setup_morph_and_skinning_defs`
// and `MeshPipeline::specialize` in `src/render/mesh.rs`). After an upgrade,
// diff those files against the version named here and port the changes.

#define_import_path omagari::vertex

#import bevy_pbr::{
    mesh_bindings::mesh,
    mesh_functions,
    skinning,
    morph::{morph_position, morph_normal, morph_tangent},
    forward_io::Vertex,
    view_transformations::position_world_to_clip,
    mesh_view_bindings::view,
}

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
    // 0 at the model's low end along the fill axis, 1 at its high end (set
    // by the effect shader; 0 here).
    @location(5) fill_coord: f32,
    // The mesh's vertex colour (white without one): multiplies the colour
    // and opacity (trail fade along its length).
    @location(6) tint: vec4<f32>,
}

// What the vertex stage needs from a material's uniforms.
struct VertexInputs {
    // Metres along the skinned normal (shells), × fit already.
    shell_offset: f32,
    // Metres toward the camera along the view ray (same pixel), × fit.
    depth_push: f32,
    // The instance's world scale (fit).
    fit: f32,
    // World metres per rest-pose unit of skinned meshes (from the CPU).
    rest_scale: f32,
    // > 0.5: the mesh is a quad in XZ turned to face the camera.
    billboard: f32,
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

// The whole vertex stage except the fill coordinate (left at 0).
fn effect_vertex(vertex_no_morph: Vertex, inputs: VertexInputs) -> EffectVertexOutput {
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
    let rest_scale = inputs.rest_scale;
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
    world_position = vec4<f32>(world_position.xyz + world_normal * inputs.shell_offset, 1.0);

    // A quad facing the camera (shape layers): its local X runs along the
    // view's right, Z along the view's up, around the entity's origin, with
    // the entity's X / Z scale.
    if inputs.billboard > 0.5 {
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
    let pushed = world_position.xyz + direction_to_camera(world_position.xyz) * inputs.depth_push;
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

    out.rest_position = vertex_no_morph.position * (rest_scale / max(inputs.fit, 0.0001));
    out.rest_normal = rest_normal;
    out.fill_coord = 0.0;
    return out;
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
