//! WGSL validation of the effect shader (`src/shaders/omagari_effect.wgsl`)
//! against the shader imports of the Bevy version in use, without a GPU.
//!
//! A headless app (`DefaultPlugins` without a window, rendering disabled)
//! loads Bevy's embedded shader libraries (`bevy_pbr::skinning`,
//! `mesh_functions`, ...); they and the effect shader are composed with
//! naga_oil through Bevy's own `ShaderCache` for the shader-def combinations
//! Bevy's mesh pipeline produces (rigid, skinned, morphed, uniform-buffer
//! skins, additive, tonemapping in the shader), and every result is checked
//! by naga's validator. A Bevy upgrade that changes the imported functions
//! fails here. The uniform block's layout is compared with `EffectUniforms`.

use std::time::Duration;

use bevy::prelude::*;
use bevy::render::render_resource::encase::ShaderSize;
use bevy::render::settings::{RenderCreation, WgpuSettings};
use bevy::render::render_resource::{DownlevelFlags, WgpuFeatures};
use bevy::render::RenderPlugin;
use bevy::shader::{Shader, ShaderCache, ShaderCacheError, ShaderCacheSource, ShaderDefVal, ValidateShader};
use bevy::window::ExitCondition;
use bevy::winit::WinitPlugin;
use omagari::runtime::{EffectUniforms, EFFECT_SHADER_PATH};

fn validate(_: &(), source: ShaderCacheSource, _: &ValidateShader) -> Result<naga::Module, ShaderCacheError> {
    let ShaderCacheSource::Naga(module) = source else {
        return Err(ShaderCacheError::CreateShaderModule("expected a naga module".into()));
    };
    let mut validator = naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::default());
    validator
        .validate(&module)
        .map_err(|e| ShaderCacheError::CreateShaderModule(format!("{:?}", e.into_inner())))?;
    Ok(module)
}

/// Bevy's shader libraries and the effect shader, loaded by a headless app.
fn load_shaders() -> (Vec<(AssetId<Shader>, Shader)>, AssetId<Shader>) {
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .build()
            .disable::<WinitPlugin>()
            .set(WindowPlugin { primary_window: None, exit_condition: ExitCondition::DontExit, ..default() })
            .set(RenderPlugin {
                render_creation: RenderCreation::Automatic(Box::new(WgpuSettings { backends: None, ..default() })),
                ..default()
            }),
    )
    .add_plugins(omagari::OmagariPlugin);
    app.finish();
    app.cleanup();
    let effect: Handle<Shader> = app.world().resource::<AssetServer>().load(EFFECT_SHADER_PATH);
    for _ in 0..500 {
        app.update();
        let server = app.world().resource::<AssetServer>();
        if server.is_loaded_with_dependencies(&effect) {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    // A few more frames for every embedded library to finish loading.
    for _ in 0..20 {
        app.update();
    }
    let shaders = app.world().resource::<Assets<Shader>>();
    assert!(shaders.get(&effect).is_some(), "the effect shader did not load from {}", EFFECT_SHADER_PATH);
    (shaders.iter().map(|(id, s)| (id, s.clone())).collect(), effect.id())
}

fn defs(names: &[&str]) -> Vec<ShaderDefVal> {
    let mut defs: Vec<ShaderDefVal> = vec![
        ShaderDefVal::UInt("MATERIAL_BIND_GROUP".into(), 3),
        // Added by Bevy's PipelineCache for every shader (desktop limits).
        ShaderDefVal::UInt("AVAILABLE_STORAGE_BUFFER_BINDINGS".into(), 8),
        "MESH_PIPELINE".into(),
        "VERTEX_OUTPUT_INSTANCE_INDEX".into(),
        "VIEW_PROJECTION_PERSPECTIVE".into(),
    ];
    for name in names {
        if *name == "TONEMAP_IN_SHADER" {
            defs.push(ShaderDefVal::UInt("TONEMAPPING_LUT_TEXTURE_BINDING_INDEX".into(), 18));
            defs.push(ShaderDefVal::UInt("TONEMAPPING_LUT_SAMPLER_BINDING_INDEX".into(), 19));
            defs.push("TONEMAP_METHOD_TONY_MC_MAPFACE".into());
        }
        defs.push((*name).into());
    }
    defs
}

#[test]
fn effect_shader_composes_with_bevy_imports_and_validates() {
    let (shaders, effect) = load_shaders();
    let mut cache: ShaderCache<naga::Module, ()> =
        ShaderCache::new((), WgpuFeatures::empty(), DownlevelFlags::all(), validate);
    for (id, shader) in shaders {
        cache.set_shader(id, shader);
    }

    let mesh = ["VERTEX_POSITIONS", "VERTEX_NORMALS", "VERTEX_UVS", "VERTEX_UVS_A"];
    let variants: Vec<(&str, Vec<&str>)> = vec![
        ("rigid mesh (shape layers)", mesh.to_vec()),
        ("skinned (Mixamo character)", [&mesh[..], &["SKINNED"]].concat()),
        ("skinned + morph targets + tangents", [&mesh[..], &["SKINNED", "MORPH_TARGETS", "VERTEX_TANGENTS"]].concat()),
        (
            "skins in uniform buffers (WebGL path)",
            [&mesh[..], &["SKINNED", "MORPH_TARGETS", "SKINS_USE_UNIFORM_BUFFERS"]].concat(),
        ),
        (
            "additive, tonemapping in the shader (non-HDR camera)",
            [&mesh[..], &["SKINNED", "BLEND_PREMULTIPLIED_ALPHA", "PREMULTIPLY_ALPHA", "TONEMAP_IN_SHADER"]].concat(),
        ),
        ("positions only (no normals, no UVs)", vec!["VERTEX_POSITIONS"]),
        ("trail ribbon (vertex colours), additive", [&mesh[..], &["VERTEX_COLORS", "BLEND_PREMULTIPLIED_ALPHA", "PREMULTIPLY_ALPHA"]].concat()),
        ("skinned ghost with vertex colours", [&mesh[..], &["SKINNED", "VERTEX_COLORS", "VERTEX_TANGENTS"]].concat()),
    ];
    for (i, (label, names)) in variants.iter().enumerate() {
        let module = match cache.get(i, effect, &defs(names)) {
            Ok(module) => module,
            Err(ShaderCacheError::ProcessShaderError(e)) => {
                panic!("{}: composing failed:\n{}", label, e.emit_to_string(&cache.composer))
            }
            Err(e) => panic!("{}: {}", label, e),
        };
        let entries: Vec<&str> = module.entry_points.iter().map(|e| e.name.as_str()).collect();
        assert!(entries.contains(&"vertex") && entries.contains(&"fragment"), "{}: {:?}", label, entries);

        // The uniform block matches `EffectUniforms`: 16 vec4s in order.
        let (_, ty) = module
            .types
            .iter()
            .find(|(_, t)| t.name.as_deref().is_some_and(|n| n.ends_with("OmagariEffect")))
            .unwrap_or_else(|| panic!("{}: no OmagariEffect struct", label));
        let naga::TypeInner::Struct { members, span } = &ty.inner else { panic!("not a struct") };
        let names: Vec<&str> = members.iter().map(|m| m.name.as_deref().unwrap_or("")).collect();
        assert_eq!(
            names,
            [
                "color", "rim_color", "edge_color", "params", "rim", "pulse", "noise_a", "noise_a_mode", "noise_b",
                "noise_b_mode", "mask", "dissolve", "fill", "fill_axis", "fill_range", "shape"
            ],
            "{}: field order of OmagariEffect vs EffectUniforms",
            label
        );
        for (k, m) in members.iter().enumerate() {
            assert_eq!(m.offset, 16 * k as u32);
        }
        assert_eq!(*span as u64, EffectUniforms::SHADER_SIZE.get());
    }
}
