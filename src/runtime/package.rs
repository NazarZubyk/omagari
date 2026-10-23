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
    param_property_name, AttachDef, Binding, ParamKind, PlayDef, ProjectV2, SimulationSpaceDef,
    TextureAssetDef, TextureColorSpace, TextureFilter, TextureWrap, VisibleRangeDef,
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
    /// Seconds after which an instance finishes by itself: set when every
    /// root particle layer is one-shot (`play: OnStart` with a finite
    /// `cycle_count`); the last spawn plus the longest particle life.
    pub auto_finish: Option<f32>,
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
    pub attach: AttachDef,
    pub simulation_space: SimulationSpaceDef,
    /// Properties the produced effect declares (e.g. `segment_a`,
    /// `param_intensity`).
    pub properties: Vec<String>,
    /// Parameters the effect reads: (parameter name, property name).
    pub param_properties: Vec<(String, String)>,
    pub play: PlayDef,
    /// Spawn rate binding (particles per second).
    pub rate: Option<Binding<f32>>,
    pub visible: Option<VisibleRangeDef>,
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
    match texture_image_settings(texture) {
        (true, ImageSampler::Default) => None,
        settings => Some(settings),
    }
}

/// Whether a texture is sRGB and the sampler it needs: `(true,
/// ImageSampler::Default)` for the sprite defaults (Clamp, sRGB, Linear).
pub fn texture_image_settings(texture: &TextureAssetDef) -> (bool, ImageSampler) {
    let is_srgb = texture.color_space() == TextureColorSpace::Srgb;
    if texture.wrap() == TextureWrap::Clamp && texture.filter() == TextureFilter::Linear {
        return (is_srgb, ImageSampler::Default);
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
    (is_srgb, sampler)
}

/// [`apply_texture_settings`] on an image in `images`, touching the asset
/// (which re-uploads it) only when something changes. Returns whether it
/// did.
pub fn apply_texture_settings_to(images: &mut Assets<Image>, handle: &Handle<Image>, texture: &TextureAssetDef) -> bool {
    let Some(image) = images.get(handle) else {
        return false;
    };
    let mut probe = Image {
        texture_descriptor: image.texture_descriptor.clone(),
        sampler: image.sampler.clone(),
        ..default()
    };
    if !apply_texture_settings(&mut probe, texture) {
        return false;
    }
    match images.get_mut(handle) {
        Some(mut image) => apply_texture_settings(&mut image, texture),
        None => false,
    }
}

/// Gives an already loaded image a texture's settings (sampler, sRGB or
/// linear for 8-bit RGBA formats). Returns whether it changed anything.
///
/// Bevy keeps the image of a path loaded with the settings of its first
/// load, so a package rebuilt with other settings for the same file (the
/// editor's live preview) gets the old ones; this fixes the image in place.
pub fn apply_texture_settings(image: &mut Image, texture: &TextureAssetDef) -> bool {
    let (is_srgb, sampler) = texture_image_settings(texture);
    let format = image.texture_descriptor.format;
    let wanted = if is_srgb { format.add_srgb_suffix() } else { format.remove_srgb_suffix() };
    if wanted == format && image.sampler == sampler {
        return false;
    }
    image.texture_descriptor.format = wanted;
    image.sampler = sampler;
    true
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
        // When each layer's spawner has emitted its last particle (one-shot
        // layers only).
        let mut emit_ends: Vec<Option<f32>> = Vec::new();
        let layer_names: Vec<&str> = project.particle_layers().map(|(_, l)| l.name.as_str()).collect();

        for (layer_index, layer) in project.particle_layers() {
            if project.fit.is_some() && layer.simulation_space == SimulationSpaceDef::Global {
                warnings.push(format!(
                    "[{}] fit does not scale Global layers (bevy_hanabi applies only the emitter position to them)",
                    layer.name
                ));
            }
            // Parameters the CPU side reads must be Float parameters.
            let float_params = layer
                .rate
                .as_ref()
                .and_then(|r| r.param())
                .map(|p| ("rate", p))
                .into_iter()
                .chain(layer.visible.as_ref().map(|v| ("visible", v.param.as_str())))
                .chain(match &layer.play {
                    PlayDef::While(p, _) => Some(("play While", p.as_str())),
                    _ => None,
                });
            for (what, param) in float_params {
                match project.param(param).map(|p| p.kind()) {
                    Some(ParamKind::Float) => {}
                    Some(ParamKind::Color) => warnings.push(format!(
                        "[{}] {} reads \"{}\", a Color parameter; it needs a Float one",
                        layer.name, what, param
                    )),
                    None => warnings.push(format!(
                        "[{}] {} reads \"{}\", which is not a parameter of this package",
                        layer.name, what, param
                    )),
                }
            }
            if let Some(trigger) = layer.play.trigger() {
                if project.trigger(trigger).is_none() {
                    warnings.push(format!(
                        "[{}] plays on trigger \"{}\", which is not a trigger of this package",
                        layer.name, trigger
                    ));
                }
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

            let effect = layer.produce(&project.parameters, warnings);
            let properties: Vec<String> = effect.properties().iter().map(|p| p.name().to_string()).collect();
            let param_properties = project
                .parameters
                .iter()
                .map(|p| (p.name.clone(), param_property_name(&p.name)))
                .filter(|(_, property)| properties.contains(property))
                .collect();
            particle_layers.push(PreparedParticleLayer {
                layer_index,
                name: layer.name.clone(),
                parent,
                effect: assets.add_effect(format!("Layer{}", layer_index), effect),
                texture,
                linger: 0.0,
                attach: layer.attach.clone(),
                simulation_space: layer.simulation_space,
                properties,
                param_properties,
                play: layer.play.clone(),
                rate: layer.rate.clone(),
                visible: layer.visible.clone(),
            });
            emit_ends.push(match layer.play {
                PlayDef::OnStart => layer.effect.emit_duration(),
                _ => None,
            });
        }

        // A layer's particles can be spawned by its parents until their
        // particles die, so its linger includes the whole parent chain.
        let mut auto_finish = (!particle_layers.is_empty()).then_some(0.0f32);
        for i in 0..particle_layers.len() {
            let mut linger = lifetimes[i];
            let mut root = i;
            let mut current = particle_layers[i].parent;
            let mut depth = 0;
            while let Some(p) = current {
                if depth >= particle_layers.len() {
                    warnings.push(format!("[{}] parent links form a loop", particle_layers[i].name));
                    break;
                }
                linger += lifetimes[p];
                root = p;
                current = particle_layers[p].parent;
                depth += 1;
            }
            particle_layers[i].linger = linger;
            // Children are spawned by their root's particles, so the root's
            // spawner decides when they end.
            auto_finish = match (auto_finish, emit_ends[root]) {
                (Some(end), Some(emit)) => Some(end.max(emit + linger)),
                _ => None,
            };
        }

        VfxPackage {
            project,
            particle_layers,
            auto_finish,
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds without an app: effects and textures get placeholder handles.
    struct NoAssets;

    impl PackageAssets for NoAssets {
        fn add_effect(&mut self, _label: String, _effect: EffectAsset) -> Handle<EffectAsset> {
            Handle::default()
        }

        fn load_texture(&mut self, _texture: &TextureAssetDef) -> Handle<Image> {
            Handle::default()
        }
    }

    fn build(ron: &str) -> (VfxPackage, Vec<String>) {
        let mut warnings = Vec::new();
        let package = VfxPackage::build(ProjectV2::from_ron_str(ron).unwrap(), &mut NoAssets, &mut warnings);
        (package, warnings)
    }

    #[test]
    fn auto_finish_covers_children_of_one_shot_roots() {
        // A burst (particles up to 1 s) whose particles spawn children living
        // 0.5 s: done 1.5 s after the burst.
        let (package, warnings) = build(
            r#"(layers: [
                Particles((name: "rocket", play: OnStart, effect: (
                    spawner: (count: Single(5.0), spawn_duration: Single(0.0), period: Single(1.0), cycle_count: 1),
                ))),
                Particles((name: "spark", parent: Some("rocket"), effect: (
                    init_modifiers: [SetAttribute((attr: "lifetime", attr_expr: Float(0.5)))],
                ))),
            ])"#,
        );
        assert!(warnings.is_empty(), "{:?}", warnings);
        assert_eq!(package.auto_finish, Some(1.5));
        assert_eq!(package.stop_duration(), 1.5);

        // One looping root: runs until stopped.
        let (package, _) = build(
            r#"(layers: [
                Particles((name: "burst", play: OnStart, effect: (spawner: (cycle_count: 1)))),
                Particles((name: "loop", play: Loop)),
            ])"#,
        );
        assert_eq!(package.auto_finish, None);
    }

    #[test]
    fn loaded_images_take_changed_texture_settings() {
        use bevy::render::render_resource::TextureFormat;
        use crate::format::TexturePurpose;

        let mut image = Image::default();
        image.texture_descriptor.format = TextureFormat::Rgba8UnormSrgb;
        let sprite = TextureAssetDef::new("a.png", TexturePurpose::Sprite);
        assert!(!apply_texture_settings(&mut image, &sprite), "already the sprite defaults");

        let noise = TextureAssetDef::new("a.png", TexturePurpose::Noise);
        assert!(apply_texture_settings(&mut image, &noise));
        assert_eq!(image.texture_descriptor.format, TextureFormat::Rgba8Unorm);
        let ImageSampler::Descriptor(d) = &image.sampler else { panic!("{:?}", image.sampler) };
        assert_eq!(d.address_mode_u, ImageAddressMode::Repeat);
        assert!(!apply_texture_settings(&mut image, &noise), "nothing left to change");

        assert!(apply_texture_settings(&mut image, &sprite));
        assert_eq!(image.texture_descriptor.format, TextureFormat::Rgba8UnormSrgb);
        assert_eq!(image.sampler, ImageSampler::Default);
        assert_eq!(texture_loader_settings(&sprite), None);
    }

    #[test]
    fn fit_warns_about_global_layers() {
        let (_, warnings) = build(r#"(fit: Some(TargetScale), layers: [Particles((name: "smoke", simulation_space: Global))])"#);
        assert!(warnings[0].contains("fit does not scale Global layers"), "{:?}", warnings);
        let (package, warnings) = build(r#"(sockets: [(name: "s")], layers: [Particles((name: "a", attach: Socket("s")))])"#);
        assert!(warnings.is_empty(), "attach is supported: {:?}", warnings);
        assert_eq!(package.particle_layers[0].attach, AttachDef::Socket("s".into()));
    }
}
