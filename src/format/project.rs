//! Project format v2 (`*.omagari.ron`) and VFX packages (`*.vfx.ron`).
//!
//! A package is the same type as a project: export clears the editor-only
//! `preview` block. Every struct has `#[serde(default)]`, so partial files
//! load, and unknown struct fields are ignored (a newer editor's file still
//! loads in an older game).
//!
//! Parameters, triggers, sockets, fit and most layer settings are data only
//! for now; the runtime uses them in later phases.

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

// --- placement ---------------------------------------------------------------

/// How an instance is scaled to its target.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
pub enum FitDef {
    /// Inherit the target's scale.
    TargetScale,
    /// Scale by `target height / reference height` (metres).
    TargetHeight(f32),
}

/// A named point on the target, used when the target's model has no such
/// node: a node name (or none = the target root) plus a local offset.
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
    /// A socket (package socket or node name).
    Socket(String),
    /// The segment between two sockets.
    Segment { from: String, to: String },
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
    /// Circle of `radius` metres at `speed` m/s.
    Circle { radius: f32, speed: f32 },
    /// Back and forth over `distance` metres at `speed` m/s.
    BackAndForth { distance: f32, speed: f32 },
}
