//! [`VfxPackage`] asset and its loader for `*.vfx.ron` packages and
//! `*.omagari.ron` projects (v1 or v2).

use bevy::asset::{AssetLoader, LoadContext};
use bevy::image::{
    ImageAddressMode, ImageFilterMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor,
};
use bevy::prelude::*;
use bevy_hanabi::prelude::*;

use super::builtin::{resolve_asset_path, resolve_asset_path_in};
use crate::format::{
    AttachDef, PlayDef, ProjectV2, TextureAssetDef, TextureColorSpace, TextureFilter, TextureWrap,
};

/// Stop duration used for a layer whose particle lifetime cannot be estimated.
pub const FALLBACK_LIFETIME: f32 = 5.0;

/// A loaded VFX package: the project data plus one bevy_hanabi effect per
/// particle layer, built once and shared by every instance.
#[derive(Asset, TypePath)]
pub struct VfxPackage {
    /// The package data (v1 files are migrated).
    pub project: ProjectV2,
    /// One entry per particle layer, in layer order.
    pub particle_layers: Vec<PreparedParticleLayer>,
}

/// A particle layer ready to be spawned.
pub struct PreparedParticleLayer {
    /// Index of the layer in `project.layers`.
    pub layer_index: usize,
    pub name: String,
    /// Index of the parent layer in `particle_layers`.
    pub parent: Option<usize>,
    pub effect: Handle<EffectAsset>,
    pub texture: Option<Handle<Image>>,
    /// Seconds particles of this layer (including those spawned by parent
    /// layers) can live after the instance stops emitting.
    pub linger: f32,
}

/// Where [`VfxPackage::build`] puts the effects and textures it creates.
pub trait PackageAssets {
    /// Stores a produced effect; `label` is unique within the package.
    fn add_effect(&mut self, label: String, effect: EffectAsset) -> Handle<EffectAsset>;
    /// Starts loading a texture.
    fn load_texture(&mut self, texture: &TextureAssetDef) -> Handle<Image>;
}

impl PackageAssets for LoadContext<'_> {
    fn add_effect(&mut self, label: String, effect: EffectAsset) -> Handle<EffectAsset> {
        self.add_labeled_asset(label, effect)
    }

    fn load_texture(&mut self, texture: &TextureAssetDef) -> Handle<Image> {
        let path = resolve_asset_path(&texture.path);
        match texture_loader_settings(texture) {
            None => self.load(path),
            Some((is_srgb, sampler)) => self
                .load_builder()
                .with_settings(move |s: &mut ImageLoaderSettings| {
                    s.is_srgb = is_srgb;
                    s.sampler = sampler.clone();
                })
                .load(path),
        }
    }
}

/// Builds packages from in-memory projects (editor preview, tests):
/// effects go into `Assets<EffectAsset>`, textures are loaded by the server
/// from the default asset source.
impl PackageAssets for (&mut Assets<EffectAsset>, &AssetServer) {
    fn add_effect(&mut self, label: String, effect: EffectAsset) -> Handle<EffectAsset> {
        InMemoryPackageAssets::new(self.0, self.1).add_effect(label, effect)
    }

    fn load_texture(&mut self, texture: &TextureAssetDef) -> Handle<Image> {
        InMemoryPackageAssets::new(self.0, self.1).load_texture(texture)
    }
}

/// Builds packages from in-memory projects, loading textures from a chosen
/// asset source (the editor loads game textures from its `game` source, see
/// [`resolve_asset_path_in`]).
pub struct InMemoryPackageAssets<'a> {
    pub effects: &'a mut Assets<EffectAsset>,
    pub server: &'a AssetServer,
    /// Asset source of paths relative to the game's `assets/` folder; `None`
    /// = the default source.
    pub source: Option<&'a str>,
}

impl<'a> InMemoryPackageAssets<'a> {
    pub fn new(effects: &'a mut Assets<EffectAsset>, server: &'a AssetServer) -> Self {
        InMemoryPackageAssets {
            effects,
            server,
            source: None,
        }
    }

    pub fn with_source(mut self, source: Option<&'a str>) -> Self {
        self.source = source;
        self
    }
}

impl PackageAssets for InMemoryPackageAssets<'_> {
    fn add_effect(&mut self, _label: String, effect: EffectAsset) -> Handle<EffectAsset> {
        self.effects.add(effect)
    }

    fn load_texture(&mut self, texture: &TextureAssetDef) -> Handle<Image> {
        let path = resolve_asset_path_in(&texture.path, self.source);
        match texture_loader_settings(texture) {
            None => self.server.load(path),
            Some((is_srgb, sampler)) => self
                .server
                .load_builder()
                .with_settings(move |s: &mut ImageLoaderSettings| {
                    s.is_srgb = is_srgb;
                    s.sampler = sampler.clone();
                })
                .load(path),
        }
    }
}

/// Loader settings for a texture, or `None` for Bevy's defaults (sRGB, the
/// app's default sampler), which is what sprites use.
fn texture_loader_settings(texture: &TextureAssetDef) -> Option<(bool, ImageSampler)> {
    let is_srgb = texture.color_space() == TextureColorSpace::Srgb;
    if is_srgb && texture.wrap() == TextureWrap::Clamp && texture.filter() == TextureFilter::Linear
    {
        return None;
    }
    let address = match texture.wrap() {
        TextureWrap::Repeat => ImageAddressMode::Repeat,
        TextureWrap::Clamp => ImageAddressMode::ClampToEdge,
    };
    let filter = match texture.filter() {
        TextureFilter::Linear => ImageFilterMode::Linear,
        TextureFilter::Nearest => ImageFilterMode::Nearest,
    };
    let sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: address,
        address_mode_v: address,
        address_mode_w: address,
        mag_filter: filter,
        min_filter: filter,
        mipmap_filter: filter,
        ..default()
    });
    Some((is_srgb, sampler))
}

impl VfxPackage {
    /// Produces every particle layer and starts loading the textures they
    /// use. `warnings` receives a line for every value that cannot be used
    /// (yet); such values are skipped.
    pub fn build(
        project: ProjectV2,
        assets: &mut impl PackageAssets,
        warnings: &mut Vec<String>,
    ) -> Self {
        let mut textures: Vec<(String, Handle<Image>)> = Vec::new();
        let mut particle_layers = Vec::new();
        let mut lifetimes = Vec::new();
        let layer_names: Vec<&str> = project.particle_layers().map(|(_, l)| l.name.as_str()).collect();

        for (layer_index, layer) in project.particle_layers() {
            if layer.attach != AttachDef::Target {
                warnings.push(format!(
                    "[{}] attach {:?} is not supported yet; the layer is attached to the target",
                    layer.name, layer.attach
                ));
            }
            if matches!(layer.play, PlayDef::OnTrigger(_) | PlayDef::While(..)) {
                warnings.push(format!(
                    "[{}] play {:?} is not supported yet; the layer plays as authored",
                    layer.name, layer.play
                ));
            }

            let texture = layer.texture.as_ref().and_then(|id| {
                if let Some((_, handle)) = textures.iter().find(|(t, _)| t == id) {
                    return Some(handle.clone());
                }
                let Some(def) = project.texture(id) else {
                    warnings.push(format!("[{}] texture \"{}\" is not in the assets list", layer.name, id));
                    return None;
                };
                let handle = assets.load_texture(def);
                textures.push((id.clone(), handle.clone()));
                Some(handle)
            });

            let parent = layer.parent.as_ref().and_then(|parent| {
                let index = layer_names.iter().position(|n| n == parent);
                if index.is_none() {
                    warnings.push(format!(
                        "[{}] parent \"{}\" is not a particle layer of this package",
                        layer.name, parent
                    ));
                }
                index
            });

            lifetimes.push(layer.effect.max_lifetime().unwrap_or_else(|| {
                warnings.push(format!(
                    "[{}] particle lifetime cannot be estimated; stopping waits {} s",
                    layer.name, FALLBACK_LIFETIME
                ));
                FALLBACK_LIFETIME
            }));

            let effect = layer.produce(warnings);
            particle_layers.push(PreparedParticleLayer {
                layer_index,
                name: layer.name.clone(),
                parent,
                effect: assets.add_effect(format!("Layer{}", layer_index), effect),
                texture,
                linger: 0.0,
            });
        }

        // A layer's particles can be spawned by its parents until their
        // particles die, so its linger includes the whole parent chain.
        for i in 0..particle_layers.len() {
            let mut linger = lifetimes[i];
            let mut current = particle_layers[i].parent;
            let mut depth = 0;
            while let Some(p) = current {
                if depth >= particle_layers.len() {
                    warnings.push(format!("[{}] parent links form a loop", particle_layers[i].name));
                    break;
                }
                linger += lifetimes[p];
                current = particle_layers[p].parent;
                depth += 1;
            }
            particle_layers[i].linger = linger;
        }

        VfxPackage {
            project,
            particle_layers,
        }
    }

    /// Seconds from `stop` until no particle of the package can be alive.
    pub fn stop_duration(&self) -> f32 {
        self.particle_layers
            .iter()
            .map(|l| l.linger)
            .fold(0.0, f32::max)
    }
}

/// Loads `*.vfx.ron` packages and `*.omagari.ron` projects (v1 projects are
/// migrated) as [`VfxPackage`]s. Textures are loaded as dependencies; effects
/// are labeled sub-assets `Layer<index>`.
#[derive(Default, TypePath)]
pub struct VfxPackageLoader;

impl AssetLoader for VfxPackageLoader {
    type Asset = VfxPackage;
    type Settings = ();
    type Error = BevyError;

    async fn load(
        &self,
        reader: &mut dyn bevy::asset::io::Reader,
        _settings: &(),
        load_context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let text = std::str::from_utf8(&bytes)?;
        let (project, mut warnings) = ProjectV2::from_ron_str_with_warnings(text)?;
        let package = VfxPackage::build(project, load_context, &mut warnings);
        for warning in warnings {
            warn!("{}: {}", load_context.path(), warning);
        }
        Ok(package)
    }

    fn extensions(&self) -> &[&str] {
        &["vfx.ron", "omagari.ron"]
    }
}
