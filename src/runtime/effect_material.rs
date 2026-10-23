//! [`OmagariEffectMaterial`]: Omagari's effect shader as a Bevy material.
//! Surface layers use it on overlay copies of the target's meshes; shape,
//! trail and afterimage layers (later) use it on their own meshes. It works
//! on skinned, morphed and plain meshes alike: the vertex shader follows
//! Bevy's mesh vertex shader (see `src/shaders/omagari_effect.wgsl`).
//!
//! The shader is embedded in the library (`embedded://omagari/shaders/
//! omagari_effect.wgsl`), so games need no shader files.
//!
//! **Bevy upgrades:** the vertex stage mirrors `bevy_pbr`'s
//! `render/mesh.wgsl` (+ `skinning.wgsl`, `morph.wgsl`, `mesh_functions.wgsl`,
//! `forward_io.wgsl`); compare them after an upgrade.

use std::path::{Path, PathBuf};

use bevy::asset::io::embedded::EmbeddedAssetRegistry;
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::pbr::{MaterialPipeline, MaterialPipelineKey, MaterialPlugin};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError};
use bevy::shader::ShaderRef;

use crate::format::{AxisDef, ChannelDef, EffectBlendDef, EffectValues, NoiseMappingDef, NoiseValues};

/// Asset path of the embedded shader.
pub const EFFECT_SHADER_PATH: &str = "embedded://omagari/shaders/omagari_effect.wgsl";

/// The shader source (also used by the WGSL validation test).
pub const EFFECT_SHADER_SOURCE: &str = include_str!("../shaders/omagari_effect.wgsl");

/// Metres overlays are drawn closer to the camera than the surface they
/// cover, along the view ray (the screen position does not change), so a
/// shell at offset 0 never z-fights with the model. Scaled by fit. Small
/// enough not to show through an arm in front of the body.
pub const DEPTH_PUSH: f32 = 0.002;

/// Sort distance (metres) added per shell index: inner shells are drawn
/// before outer ones (Bevy adds a material's `depth_bias` to the transparent
/// sort distance).
pub const SHELL_SORT_STEP: f32 = 0.001;

/// The uniform block of the effect shader (`OmagariEffect` in the WGSL).
/// Sixteen `vec4`s, 256 bytes, no padding; the field comments give the
/// meaning of each component.
#[derive(ShaderType, Clone, Copy, PartialEq, Debug, Default)]
pub struct EffectUniforms {
    /// Linear RGBA (HDR).
    pub color: Vec4,
    /// Rim colour, linear RGBA.
    pub rim_color: Vec4,
    /// Dissolve edge colour, linear RGBA.
    pub edge_color: Vec4,
    /// x opacity, y emissive, z flash, w fade (layer fade × visibility ×
    /// shell opacity).
    pub params: Vec4,
    /// x power, y intensity (0 = off).
    pub rim: Vec4,
    /// x frequency (Hz), y amount (0 = off).
    pub pulse: Vec4,
    /// xy scroll (squares / s), z scale, w strength.
    pub noise_a: Vec4,
    /// x mapping (0 off, 1 UV, 2 triplanar), y channel (0..3 = R..A, 4 =
    /// luminance), z shell noise offset, w contrast.
    pub noise_a_mode: Vec4,
    pub noise_b: Vec4,
    pub noise_b_mode: Vec4,
    /// x mapping (0 off, 1 UV, 2 triplanar), y channel, z scale, w strength.
    pub mask: Vec4,
    /// x on (1) / off (0), y threshold, z edge width.
    pub dissolve: Vec4,
    /// x on / off, y from, z to, w softness (fractions of the model).
    pub fill: Vec4,
    /// xyz fill axis in the rest pose.
    pub fill_axis: Vec4,
    /// x rest-pose coordinate of the model's low end along the axis, y the
    /// model's length along it (rest-pose units).
    pub fill_range: Vec4,
    /// x shell offset (world metres, × fit), y fit scale (instance world
    /// scale), z depth push (world metres), w world metres per rest-pose
    /// unit of skinned meshes.
    pub shape: Vec4,
}

/// Per-overlay inputs of the uniforms besides the material's values.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ShellInputs {
    /// Shell offset in metres (authored, at fit scale 1).
    pub offset: f32,
    /// Shell opacity × layer fade × visibility.
    pub fade: f32,
    pub noise_offset: f32,
    /// The instance's world scale (fit).
    pub fit_scale: f32,
    /// World metres per rest-pose unit of the skinned meshes.
    pub rest_scale: f32,
    /// Rest-pose bounds of the covered meshes `(min, max)`.
    pub rest_bounds: Option<(Vec3, Vec3)>,
}

impl Default for ShellInputs {
    fn default() -> Self {
        ShellInputs {
            offset: 0.0,
            fade: 1.0,
            noise_offset: 0.0,
            fit_scale: 1.0,
            rest_scale: 1.0,
            rest_bounds: None,
        }
    }
}

fn channel_index(channel: ChannelDef) -> f32 {
    match channel {
        ChannelDef::R => 0.0,
        ChannelDef::G => 1.0,
        ChannelDef::B => 2.0,
        ChannelDef::A => 3.0,
        ChannelDef::Luminance => 4.0,
    }
}

fn mapping_index(mapping: NoiseMappingDef) -> f32 {
    match mapping {
        NoiseMappingDef::Uv => 1.0,
        NoiseMappingDef::Triplanar => 2.0,
    }
}

fn noise_uniforms(noise: Option<NoiseValues>, has_texture: bool, noise_offset: f32) -> (Vec4, Vec4) {
    match noise {
        Some(n) if has_texture => (
            Vec4::new(n.scroll.x, n.scroll.y, n.scale, n.strength),
            Vec4::new(mapping_index(n.mapping), channel_index(n.channel), noise_offset, n.contrast.max(0.0)),
        ),
        _ => (Vec4::ZERO, Vec4::ZERO),
    }
}

/// The range `(low end, length)` of `bounds` along `axis`.
pub fn axis_range(bounds: (Vec3, Vec3), axis: Vec3) -> (f32, f32) {
    let (min, max) = bounds;
    let mut lo = f32::INFINITY;
    let mut hi = f32::NEG_INFINITY;
    for i in 0..8 {
        let corner = Vec3::new(
            if i & 1 == 0 { min.x } else { max.x },
            if i & 2 == 0 { min.y } else { max.y },
            if i & 4 == 0 { min.z } else { max.z },
        );
        let d = corner.dot(axis);
        lo = lo.min(d);
        hi = hi.max(d);
    }
    (lo, (hi - lo).max(1e-4))
}

impl EffectUniforms {
    /// Packs evaluated material values and one shell's inputs. `textures` =
    /// whether noise A, noise B and the mask have a texture (a part without
    /// its texture is off).
    pub fn new(values: &EffectValues, shell: &ShellInputs, textures: [bool; 3]) -> Self {
        let (noise_a, noise_a_mode) = noise_uniforms(values.noise_a, textures[0], shell.noise_offset);
        let (noise_b, noise_b_mode) = noise_uniforms(values.noise_b, textures[1], shell.noise_offset);
        let (rim_color, rim) = match values.rim {
            Some((color, power, intensity)) => (color, Vec4::new(power.max(0.01), intensity, 0.0, 0.0)),
            None => (Vec4::ZERO, Vec4::ZERO),
        };
        let pulse = match values.pulse {
            Some((frequency, amount)) => Vec4::new(frequency, amount.clamp(0.0, 1.0), 0.0, 0.0),
            None => Vec4::ZERO,
        };
        let (edge_color, dissolve) = match values.dissolve {
            Some((threshold, width, color)) => (color, Vec4::new(1.0, threshold, width.max(0.0), 0.0)),
            None => (Vec4::ZERO, Vec4::ZERO),
        };
        let mask = match values.mask {
            Some((mapping, channel, scale, strength)) if textures[2] => {
                Vec4::new(mapping_index(mapping), channel_index(channel), scale, strength)
            }
            _ => Vec4::ZERO,
        };
        let (fill, axis) = match values.fill {
            Some((axis, from, to, softness)) => (Vec4::new(1.0, from, to, softness.max(0.0)), axis),
            None => (Vec4::ZERO, AxisDef::Y),
        };
        let axis = axis.vector();
        // Without bounds: a 1.8 m model standing at the origin.
        let bounds = shell.rest_bounds.unwrap_or((Vec3::new(-0.5, 0.0, -0.5), Vec3::new(0.5, 1.8, 0.5)));
        let (low, length) = axis_range(bounds, axis);
        let fit = shell.fit_scale.max(1e-4);
        EffectUniforms {
            color: values.color,
            rim_color,
            edge_color,
            params: Vec4::new(values.opacity, values.emissive, values.flash, shell.fade.clamp(0.0, 1.0)),
            rim,
            pulse,
            noise_a,
            noise_a_mode,
            noise_b,
            noise_b_mode,
            mask,
            dissolve,
            fill,
            fill_axis: axis.extend(0.0),
            fill_range: Vec4::new(low, length, 0.0, 0.0),
            shape: Vec4::new(shell.offset * fit, fit, DEPTH_PUSH * fit, shell.rest_scale),
        }
    }
}

/// Omagari's effect material. One per instance, layer and shell; the
/// runtime writes its uniforms from the layer's bindings when they change.
#[derive(Asset, AsBindGroup, TypePath, Clone, Debug)]
pub struct OmagariEffectMaterial {
    #[uniform(0)]
    pub uniforms: EffectUniforms,
    /// Noise A; the fallback (white) texture when `None`.
    #[texture(1)]
    #[sampler(2)]
    pub noise_a: Option<Handle<Image>>,
    #[texture(3)]
    #[sampler(4)]
    pub noise_b: Option<Handle<Image>>,
    #[texture(5)]
    #[sampler(6)]
    pub mask: Option<Handle<Image>>,
    /// `Blend` or `Add`.
    pub alpha_mode: AlphaMode,
    /// Added to the transparent sort distance (metres).
    pub sort_bias: f32,
}

impl Default for OmagariEffectMaterial {
    fn default() -> Self {
        OmagariEffectMaterial {
            uniforms: EffectUniforms::default(),
            noise_a: None,
            noise_b: None,
            mask: None,
            alpha_mode: AlphaMode::Blend,
            sort_bias: 0.0,
        }
    }
}

/// Bevy's alpha mode for a blend setting.
pub fn alpha_mode_for(blend: EffectBlendDef) -> AlphaMode {
    match blend {
        EffectBlendDef::Blend => AlphaMode::Blend,
        EffectBlendDef::Add => AlphaMode::Add,
    }
}

impl Material for OmagariEffectMaterial {
    fn vertex_shader() -> ShaderRef {
        EFFECT_SHADER_PATH.into()
    }

    fn fragment_shader() -> ShaderRef {
        EFFECT_SHADER_PATH.into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        self.alpha_mode
    }

    fn depth_bias(&self) -> f32 {
        self.sort_bias
    }

    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }

    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        // Back faces stay culled (Bevy's default): a shell's far side would
        // double its opacity.
        descriptor.label = Some("omagari_effect_pipeline".into());
        Ok(())
    }
}

/// Embeds the shader and registers the material: the full Bevy material
/// plugin when PBR rendering is present (`DefaultPlugins`), else only the
/// asset type (headless tests).
pub(crate) fn build(app: &mut App) {
    if let Some(registry) = app.world().get_resource::<EmbeddedAssetRegistry>() {
        let full_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/shaders/omagari_effect.wgsl");
        registry.insert_asset(
            full_path,
            Path::new("omagari/shaders/omagari_effect.wgsl"),
            EFFECT_SHADER_SOURCE.as_bytes(),
        );
    }
    if app.is_plugin_added::<bevy::pbr::PbrPlugin>() {
        app.add_plugins(MaterialPlugin::<OmagariEffectMaterial>::default());
    } else if app.world().contains_resource::<AssetServer>() {
        app.init_asset::<OmagariEffectMaterial>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::render::render_resource::encase::{ShaderSize, UniformBuffer};
    use crate::format::{EffectMaterialDef, ParamValue};

    #[test]
    fn uniforms_are_sixteen_packed_vec4s() {
        assert_eq!(EffectUniforms::SHADER_SIZE.get(), 256);
        assert_eq!(EffectUniforms::min_size().get(), 256);
        // Field order = WGSL order: write a marker in `shape` and find it at
        // the last 16 bytes.
        let uniforms = EffectUniforms {
            color: Vec4::new(1.0, 2.0, 3.0, 4.0),
            shape: Vec4::new(5.0, 6.0, 7.0, 8.0),
            ..default()
        };
        let mut buffer = UniformBuffer::new(Vec::<u8>::new());
        buffer.write(&uniforms).unwrap();
        let bytes = buffer.into_inner();
        assert_eq!(bytes.len(), 256);
        let floats: Vec<f32> = bytes.chunks(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect();
        assert_eq!(&floats[0..4], &[1.0, 2.0, 3.0, 4.0]);
        assert_eq!(&floats[60..64], &[5.0, 6.0, 7.0, 8.0]);
    }

    #[test]
    fn values_map_to_uniforms() {
        let def: EffectMaterialDef = ron::from_str(
            r#"(
                blend: Add,
                color: Const((0.5, 0.1, 1.0, 0.6)),
                opacity: Const(0.8),
                emissive: Const(2.0),
                noise_a: Some((texture: (asset: "n", channel: B), mapping: Uv, scale: Const(4.0), scroll: Const((0.1, 0.2)), strength: Const(0.7))),
                noise_b: Some((texture: (asset: "missing"))),
                rim: Some((color: Const((1.0, 1.0, 1.0, 1.0)), power: Const(3.0), intensity: Const(1.5))),
                pulse: Some((frequency: Const(2.0), amount: Const(1.5))),
                flash: Some(Const(0.25)),
                dissolve: Some((threshold: Const(0.3), edge_width: Const(0.1))),
                fill: Some((axis: NegY, from: Const(0.0), to: Const(0.4), softness: Const(0.05))),
            )"#,
        )
        .unwrap();
        let values = def.evaluate(&|_: &str| None::<ParamValue>);
        let shell = ShellInputs {
            offset: 0.02,
            fade: 0.5,
            noise_offset: 0.3,
            fit_scale: 2.0,
            rest_scale: 1.0,
            rest_bounds: Some((Vec3::new(-1.0, 0.0, -0.2), Vec3::new(1.0, 1.8, 0.2))),
        };
        let u = EffectUniforms::new(&values, &shell, [true, false, false]);
        assert_eq!(u.color, Vec4::new(0.5, 0.1, 1.0, 0.6));
        assert_eq!(u.params, Vec4::new(0.8, 2.0, 0.25, 0.5));
        assert_eq!(u.noise_a, Vec4::new(0.1, 0.2, 4.0, 0.7));
        assert_eq!(u.noise_a_mode, Vec4::new(1.0, 2.0, 0.3, 1.0), "UV, channel B, shell offset, contrast");
        assert_eq!(u.noise_b_mode.x, 0.0, "no texture: off");
        assert_eq!(u.rim, Vec4::new(3.0, 1.5, 0.0, 0.0));
        assert_eq!(u.pulse, Vec4::new(2.0, 1.0, 0.0, 0.0), "amount clamped to 1");
        assert_eq!(u.dissolve, Vec4::new(1.0, 0.3, 0.1, 0.0));
        assert_eq!(u.mask, Vec4::ZERO);
        assert_eq!(u.fill, Vec4::new(1.0, 0.0, 0.4, 0.05));
        assert_eq!(u.fill_axis, Vec4::new(0.0, -1.0, 0.0, 0.0));
        // Along -Y the model runs from -1.8 (head) to 0 (feet).
        assert!((u.fill_range.x + 1.8).abs() < 1e-5 && (u.fill_range.y - 1.8).abs() < 1e-5, "{}", u.fill_range);
        assert!((u.shape - Vec4::new(0.04, 2.0, DEPTH_PUSH * 2.0, 1.0)).length() < 1e-6, "offset and push × fit");
        assert_eq!(alpha_mode_for(values.blend), AlphaMode::Add);
    }
}
