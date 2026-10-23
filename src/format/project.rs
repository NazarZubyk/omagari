//! Project format v2 (`*.omagari.ron`) and VFX packages (`*.vfx.ron`).
//!
//! A package is the same type as a project: export clears the editor-only
//! `preview` block. Every struct has `#[serde(default)]`, so partial files
//! load, and unknown struct fields are ignored (a newer editor's file still
//! loads in an older game).
//!
//! Parameters, triggers, `rate`, `visible` and `play` beyond `Loop` /
//! `OnStart` are data only for now; the runtime uses them in later phases.

use std::ops::{Add, Mul};

use bevy::prelude::*;
use bevy_hanabi::prelude::*;
use ron::ser::PrettyConfig;
use serde::{de::IgnoredAny, Deserialize, Deserializer, Serialize};

use super::migrate::migrate_v1;
use super::particles::ParticleEffectDef;
use super::OmagariProject;

/// The format version written by this library.
pub const FORMAT_VERSION: u32 = 2;

/// A v2 project or package.
#[derive(Resource, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct ProjectV2 {
    /// Format version (2). Files without it are v1 projects.
    pub version: u32,
    pub parameters: Vec<ParamDef>,
    pub triggers: Vec<TriggerDef>,
    pub assets: Vec<AssetDef>,
    /// How instances are scaled to their target; `None` = not scaled.
    pub fit: Option<FitDef>,
    pub layers: Vec<LayerDef>,
    pub sockets: Vec<SocketDef>,
    /// Editor-only preview settings; not written into packages.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview: Option<PreviewDef>,
}

impl Default for ProjectV2 {
    fn default() -> Self {
        ProjectV2 {
            version: FORMAT_VERSION,
            parameters: Vec::new(),
            triggers: Vec::new(),
            assets: Vec::new(),
            fit: None,
            layers: Vec::new(),
            sockets: Vec::new(),
            preview: None,
        }
    }
}

/// Just enough of a file to tell v1 from v2.
#[derive(Deserialize)]
struct VersionProbe {
    #[serde(default, deserialize_with = "present_u32")]
    version: Option<u32>,
    #[serde(default, deserialize_with = "present")]
    effects: bool,
}

fn present_u32<'de, D: Deserializer<'de>>(d: D) -> Result<Option<u32>, D::Error> {
    u32::deserialize(d).map(Some)
}

fn present<'de, D: Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    IgnoredAny::deserialize(d).map(|_| true)
}

impl ProjectV2 {
    /// Parses v1 or v2 text; v1 projects are migrated. Warnings (migration
    /// notes, newer format version) are logged.
    pub fn from_ron_str(text: &str) -> ron::error::SpannedResult<Self> {
        let (project, warnings) = Self::from_ron_str_with_warnings(text)?;
        for warning in warnings {
            warn!("{}", warning);
        }
        Ok(project)
    }

    /// Like [`ProjectV2::from_ron_str`], returning the warnings instead of
    /// logging them.
    pub fn from_ron_str_with_warnings(text: &str) -> ron::error::SpannedResult<(Self, Vec<String>)> {
        let probe: VersionProbe = ron::from_str(text)?;
        let mut warnings = Vec::new();
        if probe.version.is_none() && probe.effects {
            let v1 = OmagariProject::from_ron_str(text)?;
            let project = migrate_v1(v1, &mut warnings);
            return Ok((project, warnings));
        }
        let project: ProjectV2 = ron::from_str(text)?;
        if project.version > FORMAT_VERSION {
            warnings.push(format!(
                "project format version {} is newer than {}; unknown values are ignored",
                project.version, FORMAT_VERSION
            ));
        }
        Ok((project, warnings))
    }

    /// Serializes the project (pretty RON, `\n` line endings).
    pub fn to_ron_string(&self) -> Result<String, ron::Error> {
        ron::ser::to_string_pretty(self, PrettyConfig::new().new_line("\n".to_string()))
    }

    /// The package written by export: no `preview`, current format version.
    pub fn to_package(&self) -> ProjectV2 {
        ProjectV2 {
            version: FORMAT_VERSION,
            preview: None,
            ..self.clone()
        }
    }

    /// The asset with this id.
    pub fn asset(&self, id: &str) -> Option<&AssetDef> {
        self.assets.iter().find(|a| a.id == id)
    }

    /// The texture asset with this id.
    pub fn texture(&self, id: &str) -> Option<&TextureAssetDef> {
        self.asset(id).and_then(|a| match &a.kind {
            AssetKind::Texture(t) => Some(t),
        })
    }

    /// Names of the layers that use the asset `id`.
    pub fn asset_users(&self, id: &str) -> Vec<String> {
        self.layers
            .iter()
            .filter(|l| match l {
                LayerDef::Particles(p) => p.texture.as_deref() == Some(id),
            })
            .map(|l| l.name().to_string())
            .collect()
    }

    /// Renames the asset `from` to `to` and updates every layer that uses
    /// it. Fails when `to` is empty or taken, or `from` does not exist.
    pub fn rename_asset(&mut self, from: &str, to: &str) -> Result<(), String> {
        let to = to.trim();
        if to.is_empty() {
            return Err("an asset id cannot be empty".into());
        }
        if from == to {
            return Ok(());
        }
        if self.asset(to).is_some() {
            return Err(format!("an asset \"{}\" already exists", to));
        }
        let asset = self
            .assets
            .iter_mut()
            .find(|a| a.id == from)
            .ok_or_else(|| format!("no asset \"{}\"", from))?;
        asset.id = to.to_string();
        for layer in self.layers.iter_mut() {
            match layer {
                LayerDef::Particles(p) => {
                    if p.texture.as_deref() == Some(from) {
                        p.texture = Some(to.to_string());
                    }
                }
            }
        }
        Ok(())
    }

    /// Removes the asset `id`; refused while layers use it (the error names
    /// them).
    pub fn delete_asset(&mut self, id: &str) -> Result<AssetDef, String> {
        let users = self.asset_users(id);
        if !users.is_empty() {
            return Err(format!("\"{}\" is used by {}", id, quoted_list(&users)));
        }
        let index = self
            .assets
            .iter()
            .position(|a| a.id == id)
            .ok_or_else(|| format!("no asset \"{}\"", id))?;
        Ok(self.assets.remove(index))
    }

    /// An asset id based on `base` that no asset has yet: `base`, `base2`,
    /// `base3`, ... Characters other than letters, digits, `_` and `-` become
    /// `_`.
    pub fn unique_asset_id(&self, base: &str) -> String {
        let mut clean: String = base
            .trim()
            .chars()
            .map(|c| if c.is_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
            .collect();
        if clean.is_empty() {
            clean = "texture".into();
        }
        if self.asset(&clean).is_none() {
            return clean;
        }
        let mut n = 2;
        loop {
            let candidate = format!("{}{}", clean, n);
            if self.asset(&candidate).is_none() {
                return candidate;
            }
            n += 1;
        }
    }

    /// The package socket with this name.
    pub fn socket(&self, name: &str) -> Option<&SocketDef> {
        self.sockets.iter().find(|s| s.name == name)
    }

    /// The particle layers, with their index in `layers`.
    pub fn particle_layers(&self) -> impl Iterator<Item = (usize, &ParticlesLayerDef)> {
        self.layers
            .iter()
            .enumerate()
            .filter_map(|(i, l)| l.as_particles().map(|p| (i, p)))
    }
}

// --- parameters and triggers -------------------------------------------------

/// A parameter value: a number or an RGBA colour (linear, like particle
/// colours).
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
pub enum ParamValue {
    Float(f32),
    Color(Vec4),
}

impl Default for ParamValue {
    fn default() -> Self {
        ParamValue::Float(0.0)
    }
}

/// Kind of a parameter, given by its default value.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ParamKind {
    Float,
    Color,
}

/// A named value the game sets per instance; any name, any number.
///
/// ```ron
/// (name: "intensity", default: Float(0.0), min: 0.0, max: 1.0, smoothing: 0.4)
/// (name: "tint", default: Color((0.65, 0.08, 0.95, 1.0)), smoothing: 0.2)
/// ```
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct ParamDef {
    pub name: String,
    /// Default value; its variant is the parameter's kind.
    pub default: ParamValue,
    /// Range of a `Float` parameter.
    pub min: f32,
    pub max: f32,
    /// Seconds to reach ~63% of a new target value; 0 = immediate.
    pub smoothing: f32,
}

impl Default for ParamDef {
    fn default() -> Self {
        ParamDef {
            name: String::new(),
            default: ParamValue::Float(0.0),
            min: 0.0,
            max: 1.0,
            smoothing: 0.0,
        }
    }
}

impl ParamDef {
    pub fn kind(&self) -> ParamKind {
        match self.default {
            ParamValue::Float(_) => ParamKind::Float,
            ParamValue::Color(_) => ParamKind::Color,
        }
    }
}

/// A named one-shot event the game fires; any name, any number.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
#[serde(default)]
pub struct TriggerDef {
    pub name: String,
    pub pulses: Vec<PulseDef>,
}

/// On a trigger, a parameter jumps to `to` and returns to its target over
/// `decay` seconds.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
#[serde(default)]
pub struct PulseDef {
    pub param: String,
    pub to: f32,
    pub decay: f32,
}

/// A layer value: a constant, or a piecewise linear curve over a parameter.
///
/// ```ron
/// Const(20.0)
/// Param("intensity", [(0.0, 10.0), (1.0, 30.0)])
/// ```
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub enum Binding<T> {
    Const(T),
    /// Parameter name and `(parameter value, layer value)` keys, sorted by
    /// parameter value. Outside the keys the nearest key's value is used.
    Param(String, Vec<(f32, T)>),
}

impl<T: Default> Default for Binding<T> {
    fn default() -> Self {
        Binding::Const(T::default())
    }
}

impl<T> Binding<T>
where
    T: Copy + Add<Output = T> + Mul<f32, Output = T>,
{
    /// Evaluates the binding; `param` returns a parameter's current value.
    /// `None` if the parameter is unknown or the curve has no keys.
    pub fn evaluate(&self, param: impl FnOnce(&str) -> Option<f32>) -> Option<T> {
        match self {
            Binding::Const(v) => Some(*v),
            Binding::Param(name, keys) => {
                let x = param(name)?;
                let (first, last) = (keys.first()?, keys.last()?);
                if x <= first.0 {
                    return Some(first.1);
                }
                for pair in keys.windows(2) {
                    let ((x0, v0), (x1, v1)) = (pair[0], pair[1]);
                    if x <= x1 {
                        let t = if x1 > x0 { (x - x0) / (x1 - x0) } else { 1.0 };
                        return Some(v0 * (1.0 - t) + v1 * t);
                    }
                }
                Some(last.1)
            }
        }
    }
}

// --- assets ------------------------------------------------------------------

/// An entry of the package's asset library, referenced by layers by `id`.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
#[serde(default)]
pub struct AssetDef {
    pub id: String,
    pub kind: AssetKind,
}

/// Asset kinds. More (models, meshes, animations) come later.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub enum AssetKind {
    Texture(TextureAssetDef),
}

impl Default for AssetKind {
    fn default() -> Self {
        AssetKind::Texture(TextureAssetDef::default())
    }
}

/// A texture: a path relative to the game's `assets/` folder, or a built-in
/// `omagari://textures/<file>`.
///
/// `wrap`, `color_space` and `filter` default (when `None`) to what `purpose`
/// suggests; see [`TextureAssetDef::wrap`] etc.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
#[serde(default)]
pub struct TextureAssetDef {
    pub path: String,
    pub purpose: TexturePurpose,
    pub wrap: Option<TextureWrap>,
    pub color_space: Option<TextureColorSpace>,
    pub filter: Option<TextureFilter>,
    /// `(rows, columns)` of a sprite sheet.
    pub grid: Option<(u32, u32)>,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum TexturePurpose {
    #[default]
    Sprite,
    Noise,
    Mask,
    Gradient,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum TextureWrap {
    Repeat,
    Clamp,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum TextureColorSpace {
    /// Colour data.
    Srgb,
    /// Non-colour data: noise, masks.
    Linear,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum TextureFilter {
    Linear,
    Nearest,
}

impl TextureAssetDef {
    /// A texture used as-is, with every setting defaulted from the purpose.
    pub fn new(path: impl Into<String>, purpose: TexturePurpose) -> Self {
        TextureAssetDef {
            path: path.into(),
            purpose,
            ..default()
        }
    }

    pub fn wrap(&self) -> TextureWrap {
        self.wrap.unwrap_or(match self.purpose {
            TexturePurpose::Noise => TextureWrap::Repeat,
            _ => TextureWrap::Clamp,
        })
    }

    pub fn color_space(&self) -> TextureColorSpace {
        self.color_space.unwrap_or(match self.purpose {
            TexturePurpose::Sprite | TexturePurpose::Gradient => TextureColorSpace::Srgb,
            TexturePurpose::Noise | TexturePurpose::Mask => TextureColorSpace::Linear,
        })
    }

    pub fn filter(&self) -> TextureFilter {
        self.filter.unwrap_or(TextureFilter::Linear)
    }
}

impl TexturePurpose {
    pub const ALL: [TexturePurpose; 4] =
        [TexturePurpose::Sprite, TexturePurpose::Noise, TexturePurpose::Mask, TexturePurpose::Gradient];

    pub fn name(self) -> &'static str {
        match self {
            TexturePurpose::Sprite => "Sprite",
            TexturePurpose::Noise => "Noise",
            TexturePurpose::Mask => "Mask",
            TexturePurpose::Gradient => "Gradient",
        }
    }
}

/// A texture input of Omagari's own shaders (surface, shape, trail,
/// afterimage and distortion layers, from P4 on): the asset and the channel
/// the shader reads, so one channel-packed texture can feed several slots.
/// Not used by the runtime yet.
///
/// ```ron
/// (asset: "noise_pack", channel: G)
/// (asset: "smoke")              // channel: Luminance
/// ```
///
/// Particle layers cannot pick a channel (bevy_hanabi's texture sampling
/// reads RGBA or R); extract the channel to its own texture for them.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug, Default)]
#[serde(default)]
pub struct TextureSlotDef {
    /// Asset id in the package's `assets`.
    pub asset: String,
    pub channel: ChannelDef,
}

/// Which value of a texture a slot reads.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ChannelDef {
    R,
    G,
    B,
    A,
    /// Brightness of the colour (`0.2126 R + 0.7152 G + 0.0722 B`): right for
    /// one-layer grayscale or colour textures.
    #[default]
    Luminance,
}

impl ChannelDef {
    pub const ALL: [ChannelDef; 5] = [ChannelDef::R, ChannelDef::G, ChannelDef::B, ChannelDef::A, ChannelDef::Luminance];

    pub fn name(self) -> &'static str {
        match self {
            ChannelDef::R => "R",
            ChannelDef::G => "G",
            ChannelDef::B => "B",
            ChannelDef::A => "A",
            ChannelDef::Luminance => "Luminance",
        }
    }

    /// File name / asset id suffix of an extracted channel: `r`, `g`, `b`,
    /// `a`, `luminance`.
    pub fn suffix(self) -> &'static str {
        match self {
            ChannelDef::R => "r",
            ChannelDef::G => "g",
            ChannelDef::B => "b",
            ChannelDef::A => "a",
            ChannelDef::Luminance => "luminance",
        }
    }

    /// Reads `r`, `G`, `alpha`, `lum`, `luminance`, ... (any case).
    pub fn parse(text: &str) -> Option<ChannelDef> {
        match text.trim().to_ascii_lowercase().as_str() {
            "r" | "red" => Some(ChannelDef::R),
            "g" | "green" => Some(ChannelDef::G),
            "b" | "blue" => Some(ChannelDef::B),
            "a" | "alpha" => Some(ChannelDef::A),
            "l" | "lum" | "luma" | "luminance" | "gray" | "grey" => Some(ChannelDef::Luminance),
            _ => None,
        }
    }
}

/// `"a", "b" and "c"`.
fn quoted_list(items: &[String]) -> String {
    let quoted: Vec<String> = items.iter().map(|i| format!("\"{}\"", i)).collect();
    match quoted.len() {
        0 => String::new(),
        1 => quoted[0].clone(),
        n => format!("{} and {}", quoted[..n - 1].join(", "), quoted[n - 1]),
    }
}

// --- placement ---------------------------------------------------------------

/// How an instance is scaled to its target. Without `fit` the instance
/// keeps its authored size (world scale 1) whatever the target's scale.
///
/// bevy_hanabi applies the emitter's scale only to `Local` layers; `Global`
/// layers keep their authored size.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
pub enum FitDef {
    /// Inherit the target's scale.
    TargetScale,
    /// Scale by `target height / reference height` (metres). The height is
    /// measured from the target's own meshes (their bounding box) once they
    /// exist.
    TargetHeight(f32),
}

/// A named point on the target: a node of the target's hierarchy (an entity
/// with this `Name`) or none = the target root, plus a local offset in
/// metres (scaled like the instance, see [`FitDef`]).
///
/// A layer attached to a socket name that no package socket has uses the
/// node with that name directly.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
#[serde(default)]
pub struct SocketDef {
    pub name: String,
    pub node: Option<String>,
    pub offset: Vec3,
}

/// Where a layer is attached.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
pub enum AttachDef {
    /// The target's root.
    #[default]
    Target,
    /// A socket (package socket or node name): the layer follows it.
    Socket(String),
    /// The segment between two sockets: the layer sits at the target root
    /// and the runtime writes both ends into the properties `segment_a` /
    /// `segment_b` every frame.
    Segment { from: String, to: String },
}

impl AttachDef {
    /// The socket names this attachment uses.
    pub fn socket_names(&self) -> Vec<&str> {
        match self {
            AttachDef::Target => Vec::new(),
            AttachDef::Socket(name) => vec![name.as_str()],
            AttachDef::Segment { from, to } => vec![from.as_str(), to.as_str()],
        }
    }
}

/// Space particles are simulated in.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum SimulationSpaceDef {
    /// Particles stay where they were emitted (leave a trail behind a moving
    /// target). The v1 look.
    Global,
    /// Particles move with the layer.
    #[default]
    Local,
}

impl From<SimulationSpaceDef> for SimulationSpace {
    fn from(value: SimulationSpaceDef) -> Self {
        match value {
            SimulationSpaceDef::Global => SimulationSpace::Global,
            SimulationSpaceDef::Local => SimulationSpace::Local,
        }
    }
}

/// When a layer plays.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
pub enum PlayDef {
    /// For as long as the instance runs.
    #[default]
    Loop,
    /// Once when the instance starts.
    OnStart,
    /// Each time the trigger fires.
    OnTrigger(String),
    /// While the parameter is at or above the threshold.
    While(String, f32),
}

/// Layer visible only in a parameter range, cross-fading over `fade`.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
#[serde(default)]
pub struct VisibleRangeDef {
    pub param: String,
    pub from: f32,
    pub to: f32,
    pub fade: f32,
}

// --- layers ------------------------------------------------------------------

/// One visual part of a package. More kinds (surface, shape, trail, ...) are
/// added as new variants.
#[derive(Serialize, Deserialize, Clone)]
pub enum LayerDef {
    Particles(ParticlesLayerDef),
}

impl LayerDef {
    pub fn name(&self) -> &str {
        match self {
            LayerDef::Particles(p) => &p.name,
        }
    }

    pub fn as_particles(&self) -> Option<&ParticlesLayerDef> {
        match self {
            LayerDef::Particles(p) => Some(p),
        }
    }

    pub fn as_particles_mut(&mut self) -> Option<&mut ParticlesLayerDef> {
        match self {
            LayerDef::Particles(p) => Some(p),
        }
    }
}

/// A bevy_hanabi particle layer.
#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct ParticlesLayerDef {
    pub name: String,
    /// Name of the parent particle layer (GPU spawn events, parent attributes).
    pub parent: Option<String>,
    pub attach: AttachDef,
    pub simulation_space: SimulationSpaceDef,
    pub play: PlayDef,
    /// Asset id of the texture bound to the `"color"` slot.
    pub texture: Option<String>,
    /// Spawn rate override (particles per second); `None` = the spawner's own.
    pub rate: Option<Binding<f32>>,
    pub visible: Option<VisibleRangeDef>,
    pub effect: ParticleEffectDef,
}

impl Default for ParticlesLayerDef {
    fn default() -> Self {
        ParticlesLayerDef {
            name: "particles".to_string(),
            parent: None,
            attach: AttachDef::Target,
            simulation_space: SimulationSpaceDef::Local,
            play: PlayDef::Loop,
            texture: None,
            rate: None,
            visible: None,
            effect: ParticleEffectDef::default(),
        }
    }
}

impl ParticlesLayerDef {
    /// Builds the layer's bevy_hanabi effect (named after the layer, in the
    /// layer's simulation space). See [`ParticleEffectDef::produce`].
    pub fn produce(&self, warnings: &mut Vec<String>) -> EffectAsset {
        self.effect
            .produce(&self.name, self.simulation_space.into(), warnings)
    }
}

// --- editor preview ----------------------------------------------------------

/// Editor-only preview settings.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct PreviewDef {
    pub stage: PreviewStage,
    pub camera: PreviewCamera,
    /// glTF model path (game assets folder).
    pub model: Option<String>,
    pub animation: Option<String>,
    pub animation_speed: f32,
    /// Node of the model the preview instance is attached to.
    pub attach_to: Option<String>,
    pub params: Vec<(String, ParamValue)>,
    pub motion: Option<PreviewMotion>,
    /// `(trigger, every N seconds)`.
    pub auto_triggers: Vec<(String, f32)>,
}

impl Default for PreviewDef {
    fn default() -> Self {
        PreviewDef {
            stage: PreviewStage::Air,
            camera: PreviewCamera::Orbit,
            model: None,
            animation: None,
            animation_speed: 1.0,
            attach_to: None,
            params: Vec::new(),
            motion: None,
            auto_triggers: Vec::new(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum PreviewStage {
    #[default]
    Air,
    Floor,
    Object,
    Capsule,
    Model,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum PreviewCamera {
    #[default]
    Orbit,
    Game,
}

/// Movement of the preview target.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
pub enum PreviewMotion {
    /// Circle of `radius` metres around the origin at `speed` m/s.
    Circle { radius: f32, speed: f32 },
    /// Back and forth along X over `distance` metres (centred on the origin)
    /// at `speed` m/s.
    BackAndForth { distance: f32, speed: f32 },
}

/// Speed of a running character, the default preview motion speed (m/s).
pub const DEFAULT_MOTION_SPEED: f32 = 3.6;

impl PreviewMotion {
    pub fn speed(&self) -> f32 {
        match *self {
            PreviewMotion::Circle { speed, .. } | PreviewMotion::BackAndForth { speed, .. } => speed,
        }
    }

    /// Position and unit direction of travel after `t` seconds. Starts at
    /// `(radius, 0, 0)` / the origin.
    pub fn sample(&self, t: f32) -> (Vec3, Vec3) {
        match *self {
            PreviewMotion::Circle { radius, speed } => {
                let radius = radius.max(0.01);
                let angle = t * speed / radius;
                let (sin, cos) = angle.sin_cos();
                let position = Vec3::new(radius * cos, 0.0, radius * sin);
                let direction = Vec3::new(-sin, 0.0, cos) * speed.signum();
                (position, direction)
            }
            PreviewMotion::BackAndForth { distance, speed } => {
                let half = (distance * 0.5).max(0.005);
                // Triangle wave: 0 → +half → −half → 0 over one period.
                let period = 4.0 * half / speed.abs().max(0.001);
                let phase = (t / period).rem_euclid(1.0);
                let (x, forward) = if phase < 0.25 {
                    (phase * 4.0 * half, true)
                } else if phase < 0.75 {
                    (half - (phase - 0.25) * 4.0 * half, false)
                } else {
                    (-half + (phase - 0.75) * 4.0 * half, true)
                };
                (Vec3::new(x, 0.0, 0.0), if forward { Vec3::X } else { Vec3::NEG_X })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::*;

    #[test]
    fn preview_motion_paths() {
        let circle = PreviewMotion::Circle { radius: 2.0, speed: 4.0 };
        let (start, dir) = circle.sample(0.0);
        assert!((start - Vec3::new(2.0, 0.0, 0.0)).length() < 1e-5);
        assert!((dir - Vec3::Z).length() < 1e-5);
        // A quarter circle (π m) takes π / 4 s.
        let (p, _) = circle.sample(std::f32::consts::PI / 4.0);
        assert!((p - Vec3::new(0.0, 0.0, 2.0)).length() < 1e-4, "{}", p);

        let back = PreviewMotion::BackAndForth { distance: 4.0, speed: 2.0 };
        assert_eq!(back.sample(0.0).0, Vec3::ZERO);
        assert!((back.sample(1.0).0.x - 2.0).abs() < 1e-5, "end after 2 m at 2 m/s");
        assert_eq!(back.sample(1.5).1, Vec3::NEG_X);
        assert!((back.sample(3.0).0.x + 2.0).abs() < 1e-5);
        assert!(back.sample(4.0).0.length() < 1e-5, "back at the start after one period");
        assert_eq!(back.speed(), 2.0);
    }

    #[test]
    fn segment_properties_are_declared_once() {
        let project = ProjectV2::from_ron_str(
            r#"(layers: [Particles((
                name: "edge",
                attach: Segment(from: "a", to: "b"),
                effect: (init_modifiers: [
                    SetAttribute((attr: "position", attr_expr: Operator(Add(Property("segment_a"), Operator(Multiply(Operator(Subtract(Property("segment_b"), Property("segment_a"))), RandFloat)))))),
                ]),
            ))])"#,
        )
        .unwrap();
        let (_, layer) = project.particle_layers().next().unwrap();
        assert_eq!(layer.effect.properties_read(), vec!["segment_a".to_string(), "segment_b".to_string()]);
        let mut warnings = Vec::new();
        let effect = layer.produce(&mut warnings);
        assert!(warnings.is_empty(), "{:?}", warnings);
        let names: Vec<&str> = effect.properties().iter().map(|p| p.name()).collect();
        assert_eq!(names, vec!["segment_a", "segment_b"]);
        assert_eq!(project.layers[0].as_particles().unwrap().attach.socket_names(), vec!["a", "b"]);

        // An unknown property reads 0.0, with a warning.
        let mut def = layer.clone();
        def.effect.init_modifiers = vec![ModifierEditor::SetAttribute(SetAttributeModifierEditor {
            attr: Attribute::LIFETIME,
            attr_expr: ExprWriterEditor::Property("nope".into()),
        })];
        let mut warnings = Vec::new();
        let effect = def.produce(&mut warnings);
        assert!(effect.properties().is_empty());
        assert!(warnings[0].contains("Property(\"nope\") is not a runtime property"), "{:?}", warnings);
    }

    fn asset_project() -> ProjectV2 {
        ProjectV2::from_ron_str(
            r#"(version: 2,
                assets: [
                    (id: "smoke", kind: Texture((path: "vfx/textures/smoke.png"))),
                    (id: "spare", kind: Texture((path: "vfx/textures/spare.png"))),
                ],
                layers: [
                    Particles((name: "a", texture: Some("smoke"))),
                    Particles((name: "b", texture: Some("smoke"))),
                    Particles((name: "c")),
                ])"#,
        )
        .unwrap()
    }

    #[test]
    fn renaming_an_asset_updates_its_layers() {
        let mut p = asset_project();
        assert_eq!(p.asset_users("smoke"), vec!["a", "b"]);
        p.rename_asset("smoke", "fog").unwrap();
        assert!(p.asset("smoke").is_none());
        assert_eq!(p.asset_users("fog"), vec!["a", "b"]);
        assert_eq!(p.layers[2].as_particles().unwrap().texture, None);
        assert!(p.rename_asset("fog", "spare").unwrap_err().contains("already exists"));
        assert!(p.rename_asset("fog", " ").is_err());
        assert!(p.rename_asset("nope", "x").is_err());
        assert!(p.rename_asset("fog", "fog").is_ok());
    }

    #[test]
    fn deleting_a_used_asset_is_refused() {
        let mut p = asset_project();
        let err = p.delete_asset("smoke").unwrap_err();
        assert_eq!(err, "\"smoke\" is used by \"a\" and \"b\"");
        assert_eq!(p.assets.len(), 2);
        assert_eq!(p.delete_asset("spare").unwrap().id, "spare");
        assert_eq!(p.assets.len(), 1);
        assert!(p.delete_asset("spare").is_err());
    }

    #[test]
    fn unique_asset_ids() {
        let p = asset_project();
        assert_eq!(p.unique_asset_id("noise"), "noise");
        assert_eq!(p.unique_asset_id("smoke"), "smoke2");
        assert_eq!(p.unique_asset_id("my noise.v2"), "my_noise_v2");
        assert_eq!(p.unique_asset_id(""), "texture");
    }

    #[test]
    fn texture_slots_default_to_luminance() {
        let slot: TextureSlotDef = ron::from_str(r#"(asset: "noise")"#).unwrap();
        assert_eq!(slot, TextureSlotDef { asset: "noise".into(), channel: ChannelDef::Luminance });
        let slot: TextureSlotDef = ron::from_str(r#"(asset: "pack", channel: G)"#).unwrap();
        assert_eq!(slot.channel, ChannelDef::G);
        for c in ChannelDef::ALL {
            assert_eq!(ChannelDef::parse(c.suffix()), Some(c));
            assert_eq!(ChannelDef::parse(c.name()), Some(c));
        }
        assert_eq!(ChannelDef::parse("rgb"), None);
    }

    #[test]
    fn one_shot_emit_duration() {
        let mut effect = ParticleEffectDef::default();
        effect.spawner.cycle_count = 0;
        assert_eq!(effect.emit_duration(), None);
        effect.spawner.cycle_count = 1;
        effect.spawner.spawn_duration = crate::format::CpuValueDef::Single(0.5);
        assert_eq!(effect.emit_duration(), Some(0.5));
        effect.spawner.cycle_count = 3;
        effect.spawner.period = crate::format::CpuValueDef::Uniform((0.5, 2.0));
        // Two full cycles of the longest period, then the last spawn.
        assert_eq!(effect.emit_duration(), Some(4.5));
    }
}
