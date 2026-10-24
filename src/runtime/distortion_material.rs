//! [`OmagariDistortionMaterial`]: the distortion shader (heat haze,
//! shockwave ripples, aura shimmer) as a Bevy material.
//!
//! It reads Bevy's **screen-space transmission texture**: a copy of the
//! camera's colour target taken after the opaque pass. A material whose
//! [`Material::reads_view_transmission_texture`] is true and whose alpha mode
//! is opaque is drawn in Bevy's `Transmissive3d` phase (after opaque, before
//! transparent); Bevy makes the copy only while such a material is drawn,
//! so a hidden distortion layer costs nothing. Every `Camera3d` has
//! `ScreenSpaceTransmission { steps: 1, .. }` by default (a required
//! component); `steps: 0` turns the copy off and distortion with it.
//!
//! The shader outputs the copy sampled at the pixel plus an offset from
//! scrolling noise (`src/shaders/omagari_distortion.wgsl`), writes no depth
//! (particles behind the volume still draw), and where its weight is 0 the
//! output equals what was there. Limits: the copy holds opaque geometry only,
//! so transparent particles and Omagari's transparent overlays behind the
//! volume are not distorted (they draw later, over it).

use std::path::{Path, PathBuf};

use bevy::asset::io::embedded::EmbeddedAssetRegistry;
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::material::OpaqueRendererMethod;
use bevy::pbr::{MaterialPipeline, MaterialPipelineKey, MaterialPlugin};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError};
use bevy::shader::ShaderRef;

use super::effect_material::DEPTH_PUSH;
use crate::format::{ChannelDef, NoiseMappingDef};

/// Asset path of the embedded distortion shader.
pub const DISTORTION_SHADER_PATH: &str = "embedded://omagari/shaders/omagari_distortion.wgsl";

/// The distortion shader source (also used by the WGSL validation test).
pub const DISTORTION_SHADER_SOURCE: &str = include_str!("../shaders/omagari_distortion.wgsl");

/// The uniform block of the distortion shader (`OmagariDistortion` in the
/// WGSL). Eight `vec4`s, 128 bytes, no padding.
#[derive(ShaderType, Clone, Copy, PartialEq, Debug, Default)]
pub struct DistortionUniforms {
    /// x strength (largest offset as a fraction of the view height), y edge
    /// softness (0..1), z edge mode (0 = strongest in the middle, 1 = rim),
    /// w fade (layer fade × visibility).
    pub params: Vec4,
    /// rgb tint (multiplies what is behind), a amount (0..1).
    pub tint: Vec4,
    /// xy scroll (texture squares / s), z scale, w weight (0 = off).
    pub noise_a: Vec4,
    /// x mapping (1 UV, 2 triplanar), y channel (0..3 = R..A, 4 =
    /// luminance), z contrast.
    pub noise_a_mode: Vec4,
    pub noise_b: Vec4,
    pub noise_b_mode: Vec4,
    /// x shell offset (world metres, × fit), y fit scale, z depth push
    /// (world metres), w world metres per rest-pose unit of skinned meshes.
    pub shape: Vec4,
    /// x refraction (0..1: the share of the offset along the surface's
    /// screen direction instead of the noise); yzw reserved (zero).
    pub extra: Vec4,
}

/// One noise texture of a distortion layer, evaluated.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct DistortionNoiseValues {
    pub mapping: NoiseMappingDef,
    pub channel: ChannelDef,
    pub scale: f32,
    pub scroll: Vec2,
    /// How much of the offset comes from it (0..1).
    pub weight: f32,
    pub contrast: f32,
}

/// A distortion layer's values for one frame (bindings evaluated, fades
/// applied).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct DistortionInputs {
    /// Largest offset, fraction of the view height.
    pub strength: f32,
    pub edge_softness: f32,
    /// Strongest at the rim instead of the middle.
    pub rim: bool,
    /// Share of the offset along the surface's screen direction (a lens).
    pub refraction: f32,
    /// Lifecycle fade × envelope × visibility.
    pub fade: f32,
    /// rgb tint, a amount.
    pub tint: Vec4,
    /// Noise A / B (`None` = off or no texture).
    pub noise: [Option<DistortionNoiseValues>; 2],
    /// Metres along the normal (surface sources), at fit 1.
    pub shell_offset: f32,
    pub fit_scale: f32,
    /// World metres per rest-pose unit of skinned meshes.
    pub rest_scale: f32,
}

impl Default for DistortionInputs {
    fn default() -> Self {
        DistortionInputs {
            strength: 0.0,
            edge_softness: 0.5,
            rim: false,
            refraction: 0.0,
            fade: 1.0,
            tint: Vec4::new(1.0, 1.0, 1.0, 0.0),
            noise: [None, None],
            shell_offset: 0.0,
            fit_scale: 1.0,
            rest_scale: 1.0,
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

fn noise_uniforms(noise: Option<DistortionNoiseValues>) -> (Vec4, Vec4) {
    match noise {
        Some(n) if n.weight > 0.0 => (
            Vec4::new(n.scroll.x, n.scroll.y, n.scale, n.weight.min(1.0)),
            Vec4::new(
                match n.mapping {
                    NoiseMappingDef::Uv => 1.0,
                    NoiseMappingDef::Triplanar => 2.0,
                },
                channel_index(n.channel),
                n.contrast.max(0.0),
                0.0,
            ),
        ),
        _ => (Vec4::ZERO, Vec4::ZERO),
    }
}

impl DistortionUniforms {
    pub fn new(inputs: &DistortionInputs) -> Self {
        let (noise_a, noise_a_mode) = noise_uniforms(inputs.noise[0]);
        let (noise_b, noise_b_mode) = noise_uniforms(inputs.noise[1]);
        let fit = inputs.fit_scale.max(1e-4);
        DistortionUniforms {
            params: Vec4::new(
                inputs.strength.max(0.0),
                inputs.edge_softness.clamp(0.0, 1.0),
                if inputs.rim { 1.0 } else { 0.0 },
                inputs.fade.clamp(0.0, 1.0),
            ),
            tint: Vec4::new(inputs.tint.x, inputs.tint.y, inputs.tint.z, inputs.tint.w.clamp(0.0, 1.0)),
            noise_a,
            noise_a_mode,
            noise_b,
            noise_b_mode,
            shape: Vec4::new(inputs.shell_offset * fit, fit, DEPTH_PUSH * fit, inputs.rest_scale),
            extra: Vec4::new(inputs.refraction.clamp(0.0, 1.0), 0.0, 0.0, 0.0),
        }
    }

    /// Whether drawing it would change any pixel (some offset or tint).
    pub fn is_visible(&self) -> bool {
        let fade = self.params.w;
        let moves = self.params.x > 0.0 && (self.noise_a.w > 0.0 || self.noise_b.w > 0.0 || self.extra.x > 0.0);
        let tints = self.tint.w > 0.0 && self.tint.truncate() != Vec3::ONE;
        fade > 0.0 && (moves || tints)
    }
}

/// Omagari's distortion material. One per instance and layer (and per
/// covered mesh for surface sources: one shared material); the runtime
/// writes its uniforms from the layer's bindings when they change.
#[derive(Asset, AsBindGroup, TypePath, Clone, Debug, Default)]
pub struct OmagariDistortionMaterial {
    #[uniform(0)]
    pub uniforms: DistortionUniforms,
    /// Noise A; the fallback (white) texture when `None`.
    #[texture(1)]
    #[sampler(2)]
    pub noise_a: Option<Handle<Image>>,
    #[texture(3)]
    #[sampler(4)]
    pub noise_b: Option<Handle<Image>>,
    /// Added to the distance Bevy sorts transmissive draws by (metres).
    pub sort_bias: f32,
}

impl Material for OmagariDistortionMaterial {
    fn vertex_shader() -> ShaderRef {
        DISTORTION_SHADER_PATH.into()
    }

    fn fragment_shader() -> ShaderRef {
        DISTORTION_SHADER_PATH.into()
    }

    /// Opaque + reading the transmission texture = Bevy's `Transmissive3d`
    /// phase.
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Opaque
    }

    fn reads_view_transmission_texture(&self) -> bool {
        true
    }

    fn opaque_render_method(&self) -> OpaqueRendererMethod {
        OpaqueRendererMethod::Forward
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
        descriptor.label = Some("omagari_distortion_pipeline".into());
        // No depth writes: transparent particles behind the volume still
        // draw (undistorted) instead of being cut out by it.
        if let Some(depth) = descriptor.depth_stencil.as_mut() {
            depth.depth_write_enabled = Some(false);
        }
        Ok(())
    }
}

/// Embeds the shader and registers the material (the full material plugin
/// when PBR rendering is present, else only the asset type).
pub(crate) fn build(app: &mut App) {
    if let Some(registry) = app.world().get_resource::<EmbeddedAssetRegistry>() {
        let full_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/shaders/omagari_distortion.wgsl");
        registry.insert_asset(
            full_path,
            Path::new("omagari/shaders/omagari_distortion.wgsl"),
            DISTORTION_SHADER_SOURCE.as_bytes(),
        );
    }
    if app.is_plugin_added::<bevy::pbr::PbrPlugin>() {
        app.add_plugins(MaterialPlugin::<OmagariDistortionMaterial>::default());
    } else if app.world().contains_resource::<AssetServer>() {
        app.init_asset::<OmagariDistortionMaterial>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::render::render_resource::encase::{ShaderSize, UniformBuffer};

    #[test]
    fn uniforms_are_eight_packed_vec4s() {
        assert_eq!(DistortionUniforms::SHADER_SIZE.get(), 128);
        let uniforms = DistortionUniforms {
            params: Vec4::new(1.0, 2.0, 3.0, 4.0),
            extra: Vec4::new(5.0, 6.0, 7.0, 8.0),
            ..default()
        };
        let mut buffer = UniformBuffer::new(Vec::<u8>::new());
        buffer.write(&uniforms).unwrap();
        let bytes = buffer.into_inner();
        assert_eq!(bytes.len(), 128);
        let floats: Vec<f32> = bytes.chunks(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect();
        assert_eq!(&floats[0..4], &[1.0, 2.0, 3.0, 4.0]);
        assert_eq!(&floats[28..32], &[5.0, 6.0, 7.0, 8.0]);
    }

    #[test]
    fn inputs_map_to_uniforms() {
        let inputs = DistortionInputs {
            strength: 0.01,
            edge_softness: 1.5,
            rim: true,
            refraction: 0.25,
            fade: 0.5,
            tint: Vec4::new(1.2, 1.0, 0.8, 2.0),
            noise: [
                Some(DistortionNoiseValues {
                    mapping: NoiseMappingDef::Uv,
                    channel: ChannelDef::G,
                    scale: 3.0,
                    scroll: Vec2::new(0.0, 0.5),
                    weight: 1.0,
                    contrast: 1.5,
                }),
                Some(DistortionNoiseValues {
                    mapping: NoiseMappingDef::Triplanar,
                    channel: ChannelDef::Luminance,
                    scale: 1.0,
                    scroll: Vec2::ZERO,
                    weight: 0.0,
                    contrast: 1.0,
                }),
            ],
            shell_offset: 0.03,
            fit_scale: 2.0,
            rest_scale: 0.01,
        };
        let u = DistortionUniforms::new(&inputs);
        assert_eq!(u.params, Vec4::new(0.01, 1.0, 1.0, 0.5), "softness clamped, rim");
        assert_eq!(u.tint, Vec4::new(1.2, 1.0, 0.8, 1.0), "amount clamped");
        assert_eq!(u.noise_a, Vec4::new(0.0, 0.5, 3.0, 1.0));
        assert_eq!(u.noise_a_mode, Vec4::new(1.0, 1.0, 1.5, 0.0));
        assert_eq!((u.noise_b, u.noise_b_mode), (Vec4::ZERO, Vec4::ZERO), "weight 0 = off");
        assert!((u.shape - Vec4::new(0.06, 2.0, DEPTH_PUSH * 2.0, 0.01)).length() < 1e-6);
        assert_eq!(u.extra, Vec4::new(0.25, 0.0, 0.0, 0.0), "refraction");
        assert!(u.is_visible());

        // Nothing to draw: strength 0 without a tint, or faded out.
        let zero = DistortionUniforms::new(&DistortionInputs { strength: 0.0, ..inputs });
        assert!(zero.is_visible(), "the tint still shows");
        let zero = DistortionUniforms::new(&DistortionInputs { strength: 0.0, tint: Vec4::new(1.0, 1.0, 1.0, 0.0), ..inputs });
        assert!(!zero.is_visible());
        assert!(!DistortionUniforms::new(&DistortionInputs { fade: 0.0, ..inputs }).is_visible());
        let lens_only = DistortionInputs { noise: [None, None], tint: Vec4::ZERO, ..inputs };
        assert!(DistortionUniforms::new(&lens_only).is_visible(), "refraction alone moves the picture");
        assert!(!DistortionUniforms::new(&DistortionInputs { refraction: 0.0, ..lens_only }).is_visible());
    }
}
