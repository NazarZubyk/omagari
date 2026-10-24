//! Project format v2 (`*.omagari.ron`) and VFX packages (`*.vfx.ron`).
//!
//! A package is the same type as a project: export clears the editor-only
//! `preview` block. Every struct has `#[serde(default)]`, so partial files
//! load, and unknown struct fields are ignored (a newer editor's file still
//! loads in an older game) but reported as warnings with their path
//! ([`ProjectV2::from_ron_str_with_report`]), so typos do not go unnoticed.
//!
//! Parameters and triggers are declared per package (any names, any
//! number); the runtime keeps their values per instance and fires triggers
//! (`runtime/params.rs`, `runtime/triggers.rs`).

use std::ops::{Add, Mul};

use bevy::prelude::*;
use bevy_hanabi::prelude::*;
use ron::ser::PrettyConfig;
use serde::{de::IgnoredAny, Deserialize, Deserializer, Serialize};

use super::expr::ExprWriterEditor;
use super::migrate::migrate_v1;
use super::particles::ParticleEffectDef;
use super::shape::ShapeLayerDef;
use super::light::LightLayerDef;
use super::surface::{EffectMaterialDef, FadeDef, SurfaceLayerDef};
use super::trail::{AfterimageLayerDef, TrailLayerDef};
use super::unknown_fields::{from_ron_str_tracking, FieldStep, UnknownField};
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

/// What reading a project text found besides the project
/// ([`ProjectV2::from_ron_str_with_report`]).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ParseReport {
    /// Migration notes and a newer format version.
    pub notes: Vec<String>,
    /// Fields the format does not know (typos, or a newer editor's fields),
    /// one message each with the path to the field; they are ignored.
    pub unknown_fields: Vec<String>,
}

impl ParseReport {
    /// Every message: notes, then unknown fields.
    pub fn all(&self) -> Vec<String> {
        self.notes.iter().chain(self.unknown_fields.iter()).cloned().collect()
    }
}

impl ProjectV2 {
    /// Parses v1 or v2 text; v1 projects are migrated. Warnings (migration
    /// notes, newer format version, unknown fields) are logged.
    pub fn from_ron_str(text: &str) -> ron::error::SpannedResult<Self> {
        let (project, warnings) = Self::from_ron_str_with_warnings(text)?;
        for warning in warnings {
            warn!("{}", warning);
        }
        Ok(project)
    }

    /// Like [`ProjectV2::from_ron_str`], returning the warnings (notes and
    /// unknown fields, [`ParseReport::all`]) instead of logging them.
    pub fn from_ron_str_with_warnings(text: &str) -> ron::error::SpannedResult<(Self, Vec<String>)> {
        let (project, report) = Self::from_ron_str_with_report(text)?;
        Ok((project, report.all()))
    }

    /// Like [`ProjectV2::from_ron_str`], with the migration notes and the
    /// unknown fields apart.
    pub fn from_ron_str_with_report(text: &str) -> ron::error::SpannedResult<(Self, ParseReport)> {
        let probe: VersionProbe = ron::from_str(text)?;
        let mut report = ParseReport::default();
        if probe.version.is_none() && probe.effects {
            let (v1, unknown): (OmagariProject, _) = from_ron_str_tracking(text)?;
            report.unknown_fields = unknown
                .iter()
                .filter(|u| !is_legacy_v1_field(u))
                .map(|u| describe_v1_unknown_field(&v1, u))
                .collect();
            let project = migrate_v1(v1, &mut report.notes);
            return Ok((project, report));
        }
        let (project, unknown): (ProjectV2, _) = from_ron_str_tracking(text)?;
        if project.version > FORMAT_VERSION {
            report.notes.push(format!(
                "project format version {} is newer than {}; unknown values are ignored",
                project.version, FORMAT_VERSION
            ));
        }
        report.unknown_fields = unknown.iter().map(|u| project.describe_unknown_field(u)).collect();
        Ok((project, report))
    }

    /// A warning for a field the format does not know, naming the layer
    /// (and modifier), asset or parameter it is in.
    pub fn describe_unknown_field(&self, field: &UnknownField) -> String {
        let mut context = Vec::new();
        if let Some(layer) = field.index_after("layers").and_then(|i| self.layers.get(i)) {
            context.push(format!("{} layer \"{}\"", layer.kind_name(), layer.name()));
            if let LayerDef::Particles(p) = layer {
                if let Some(kind) = modifier_kind_at(&field.location[2..], &p.effect) {
                    context.push(kind.to_string());
                }
            }
        } else if let Some(asset) = field.index_after("assets").and_then(|i| self.assets.get(i)) {
            context.push(format!("asset \"{}\"", asset.id));
        } else if let Some(param) = field.index_after("parameters").and_then(|i| self.parameters.get(i)) {
            context.push(format!("parameter \"{}\"", param.name));
        }
        unknown_field_text(field, &context)
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

    /// `(rows, columns)` of the sprite sheet of a layer's texture, if it has
    /// one.
    pub fn texture_grid(&self, layer: &ParticlesLayerDef) -> Option<(u32, u32)> {
        layer.texture.as_ref().and_then(|id| self.texture(id)).and_then(|t| t.grid)
    }

    /// The texture asset with this id.
    pub fn texture(&self, id: &str) -> Option<&TextureAssetDef> {
        self.asset(id).and_then(|a| match &a.kind {
            AssetKind::Texture(t) => Some(t),
        })
    }

    /// Names of the layers that use the asset `id` (particle textures,
    /// texture slots of surface, shape, trail and afterimage layers).
    pub fn asset_users(&self, id: &str) -> Vec<String> {
        self.layers
            .iter()
            .filter(|l| match l {
                LayerDef::Particles(p) => p.texture.as_deref() == Some(id),
                _ => l.material().is_some_and(|m| m.texture_slots().iter().any(|(_, slot)| slot.asset == id)),
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
                _ => {
                    for slot in layer.material_mut().into_iter().flat_map(|m| m.texture_slots_mut()) {
                        if slot.asset == from {
                            slot.asset = to.to_string();
                        }
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

    /// The surface layers, with their index in `layers`.
    pub fn surface_layers(&self) -> impl Iterator<Item = (usize, &SurfaceLayerDef)> {
        self.layers
            .iter()
            .enumerate()
            .filter_map(|(i, l)| l.as_surface().map(|s| (i, s)))
    }

    /// The shape layers, with their index in `layers`.
    pub fn shape_layers(&self) -> impl Iterator<Item = (usize, &ShapeLayerDef)> {
        self.layers
            .iter()
            .enumerate()
            .filter_map(|(i, l)| l.as_shape().map(|s| (i, s)))
    }

    /// The trail layers, with their index in `layers`.
    pub fn trail_layers(&self) -> impl Iterator<Item = (usize, &TrailLayerDef)> {
        self.layers
            .iter()
            .enumerate()
            .filter_map(|(i, l)| l.as_trail().map(|s| (i, s)))
    }

    /// The afterimage layers, with their index in `layers`.
    pub fn afterimage_layers(&self) -> impl Iterator<Item = (usize, &AfterimageLayerDef)> {
        self.layers
            .iter()
            .enumerate()
            .filter_map(|(i, l)| l.as_afterimage().map(|s| (i, s)))
    }

    /// The light layers, with their index in `layers`.
    pub fn light_layers(&self) -> impl Iterator<Item = (usize, &LightLayerDef)> {
        self.layers
            .iter()
            .enumerate()
            .filter_map(|(i, l)| l.as_light().map(|s| (i, s)))
    }

    /// The parameter with this name.
    pub fn param(&self, name: &str) -> Option<&ParamDef> {
        self.parameters.iter().find(|p| p.name == name)
    }

    /// The trigger with this name.
    pub fn trigger(&self, name: &str) -> Option<&TriggerDef> {
        self.triggers.iter().find(|t| t.name == name)
    }

    /// What reads the parameter `name`, in words (`layer "mist" (expressions,
    /// rate)`, `trigger "tick" (pulse)`); the preview block does not count.
    pub fn param_users(&self, name: &str) -> Vec<String> {
        let mut users = Vec::new();
        for (_, layer) in self.particle_layers() {
            let mut what = Vec::new();
            if layer.effect.params_read().iter().any(|p| p == name) {
                what.push("expressions");
            }
            if layer.rate.as_ref().and_then(|r| r.param()) == Some(name) {
                what.push("rate");
            }
            if layer.visible.as_ref().is_some_and(|v| v.param == name) {
                what.push("visible");
            }
            if matches!(&layer.play, PlayDef::While(p, _) if p == name) {
                what.push("play While");
            }
            if !what.is_empty() {
                users.push(format!("layer \"{}\" ({})", layer.name, what.join(", ")));
            }
        }
        for (_, layer) in self.surface_layers() {
            let mut what: Vec<&str> = Vec::new();
            for (value, param) in layer.material.binding_params() {
                if param == name && !what.contains(&value) {
                    what.push(value);
                }
            }
            if layer.visible.as_ref().is_some_and(|v| v.param == name) {
                what.push("visible");
            }
            if !what.is_empty() {
                users.push(format!("layer \"{}\" ({})", layer.name, what.join(", ")));
            }
        }
        for (_, layer) in self.shape_layers() {
            let mut what: Vec<&str> = Vec::new();
            for (value, param) in layer.binding_params() {
                if param == name && !what.contains(&value) {
                    what.push(value);
                }
            }
            if layer.visible.as_ref().is_some_and(|v| v.param == name) {
                what.push("visible");
            }
            if matches!(&layer.play, PlayDef::While(p, _) if p == name) {
                what.push("play While");
            }
            if !what.is_empty() {
                users.push(format!("layer \"{}\" ({})", layer.name, what.join(", ")));
            }
        }
        let emitters = self
            .trail_layers()
            .map(|(_, l)| (l.name.as_str(), l.binding_params(), l.visible.as_ref(), &l.emit))
            .chain(
                self.afterimage_layers()
                    .map(|(_, l)| (l.name.as_str(), l.binding_params(), l.visible.as_ref(), &l.emit)),
            );
        for (layer, bindings, visible, emit) in emitters {
            let mut what: Vec<&str> = Vec::new();
            for (value, param) in bindings {
                if param == name && !what.contains(&value) {
                    what.push(value);
                }
            }
            if visible.is_some_and(|v| v.param == name) {
                what.push("visible");
            }
            if emit.param() == Some(name) {
                what.push("emit While");
            }
            if !what.is_empty() {
                users.push(format!("layer \"{}\" ({})", layer, what.join(", ")));
            }
        }
        for (_, layer) in self.light_layers() {
            let mut what: Vec<&str> = Vec::new();
            for (value, param) in layer.binding_params() {
                if param == name && !what.contains(&value) {
                    what.push(value);
                }
            }
            if layer.visible.as_ref().is_some_and(|v| v.param == name) {
                what.push("visible");
            }
            if layer.play.param() == Some(name) {
                what.push("play While");
            }
            if !what.is_empty() {
                users.push(format!("layer \"{}\" ({})", layer.name, what.join(", ")));
            }
        }
        for trigger in self.triggers.iter() {
            if trigger.pulses.iter().any(|p| p.param == name) {
                users.push(format!("trigger \"{}\" (pulse)", trigger.name));
            }
        }
        users
    }

    /// Names of the layers played by the trigger `name` (`play: OnTrigger`)
    /// or switched by it (`play: Between(start, stop)`, `emit: Trigger(start,
    /// stop)` of trails and afterimages).
    pub fn trigger_users(&self, name: &str) -> Vec<String> {
        self.particle_layers()
            .filter(|(_, l)| l.play.triggers().contains(&name))
            .map(|(_, l)| l.name.clone())
            .chain(self.shape_layers().filter(|(_, l)| l.play.triggers().contains(&name)).map(|(_, l)| l.name.clone()))
            .chain(self.light_layers().filter(|(_, l)| l.play.triggers().contains(&name)).map(|(_, l)| l.name.clone()))
            .chain(self.trail_layers().filter(|(_, l)| l.emit.triggers().contains(&name)).map(|(_, l)| l.name.clone()))
            .chain(
                self.afterimage_layers()
                    .filter(|(_, l)| l.emit.triggers().contains(&name))
                    .map(|(_, l)| l.name.clone()),
            )
            .collect()
    }

    /// Renames the parameter `from` to `to` and every reference to it:
    /// expressions, `rate`, `visible`, `play: While`, surface and shape
    /// bindings, pulses and the preview values. Fails when `to` is empty or taken, or `from` does not exist.
    pub fn rename_param(&mut self, from: &str, to: &str) -> Result<(), String> {
        let to = to.trim();
        if to.is_empty() {
            return Err("a parameter name cannot be empty".into());
        }
        if from == to {
            return Ok(());
        }
        if self.param(to).is_some() {
            return Err(format!("a parameter \"{}\" already exists", to));
        }
        let param = self
            .parameters
            .iter_mut()
            .find(|p| p.name == from)
            .ok_or_else(|| format!("no parameter \"{}\"", from))?;
        param.name = to.to_string();
        for layer in self.layers.iter_mut() {
            if let LayerDef::Surface(s) = layer {
                s.rename_param(from, to);
            }
            if let LayerDef::Shape(s) = layer {
                s.rename_param(from, to);
            }
            if let LayerDef::Trail(s) = layer {
                s.rename_param(from, to);
            }
            if let LayerDef::Afterimage(s) = layer {
                s.rename_param(from, to);
            }
            if let LayerDef::Light(s) = layer {
                s.rename_param(from, to);
            }
            let Some(layer) = layer.as_particles_mut() else { continue };
            for e in layer.effect.exprs_mut() {
                e.visit_mut(&mut |e| {
                    if let ExprWriterEditor::Param(name) = e {
                        if name == from {
                            *name = to.to_string();
                        }
                    }
                });
            }
            if let Some(Binding::Param(name, _)) = &mut layer.rate {
                if name == from {
                    *name = to.to_string();
                }
            }
            if let Some(visible) = &mut layer.visible {
                if visible.param == from {
                    visible.param = to.to_string();
                }
            }
            if let PlayDef::While(name, _) = &mut layer.play {
                if name == from {
                    *name = to.to_string();
                }
            }
        }
        for pulse in self.triggers.iter_mut().flat_map(|t| t.pulses.iter_mut()) {
            if pulse.param == from {
                pulse.param = to.to_string();
            }
        }
        if let Some(preview) = &mut self.preview {
            for (name, _) in preview.params.iter_mut() {
                if name == from {
                    *name = to.to_string();
                }
            }
        }
        Ok(())
    }

    /// Renames the trigger `from` to `to` and every reference to it (`play:
    /// OnTrigger` / `Between`, `emit: Trigger`, preview auto and animation triggers). Fails when `to` is empty or taken,
    /// or `from` does not exist.
    pub fn rename_trigger(&mut self, from: &str, to: &str) -> Result<(), String> {
        let to = to.trim();
        if to.is_empty() {
            return Err("a trigger name cannot be empty".into());
        }
        if from == to {
            return Ok(());
        }
        if self.trigger(to).is_some() {
            return Err(format!("a trigger \"{}\" already exists", to));
        }
        let trigger = self
            .triggers
            .iter_mut()
            .find(|t| t.name == from)
            .ok_or_else(|| format!("no trigger \"{}\"", from))?;
        trigger.name = to.to_string();
        for layer in self.layers.iter_mut() {
            match layer {
                LayerDef::Particles(p) => p.play.rename_trigger(from, to),
                LayerDef::Shape(s) => s.play.rename_trigger(from, to),
                LayerDef::Light(l) => l.play.rename_trigger(from, to),
                LayerDef::Trail(t) => t.emit.rename_trigger(from, to),
                LayerDef::Afterimage(a) => a.emit.rename_trigger(from, to),
                LayerDef::Surface(_) => {}
            }
        }
        if let Some(preview) = &mut self.preview {
            for (name, _) in preview.auto_triggers.iter_mut().chain(preview.animation_triggers.iter_mut()) {
                if name == from {
                    *name = to.to_string();
                }
            }
        }
        Ok(())
    }

    /// A parameter name based on `base` that no parameter has yet: `base`,
    /// `base2`, ...
    pub fn unique_param_name(&self, base: &str) -> String {
        unique_name(base, |n| self.param(n).is_some())
    }

    /// A trigger name based on `base` that no trigger has yet.
    pub fn unique_trigger_name(&self, base: &str) -> String {
        unique_name(base, |n| self.trigger(n).is_some())
    }
}

/// `base`, `base2`, `base3`, ... : the first one `taken` refuses.
fn unique_name(base: &str, taken: impl Fn(&str) -> bool) -> String {
    let base = base.trim();
    if !taken(base) {
        return base.to_string();
    }
    let mut n = 2;
    loop {
        let candidate = format!("{}{}", base, n);
        if !taken(&candidate) {
            return candidate;
        }
        n += 1;
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

impl ParamValue {
    pub fn kind(&self) -> ParamKind {
        match self {
            ParamValue::Float(_) => ParamKind::Float,
            ParamValue::Color(_) => ParamKind::Color,
        }
    }

    pub fn as_float(&self) -> Option<f32> {
        match *self {
            ParamValue::Float(v) => Some(v),
            ParamValue::Color(_) => None,
        }
    }

    /// Linear RGBA.
    pub fn as_color(&self) -> Option<Vec4> {
        match *self {
            ParamValue::Color(c) => Some(c),
            ParamValue::Float(_) => None,
        }
    }

    /// The bevy_hanabi property value: f32 or vec4.
    pub fn to_value(&self) -> Value {
        match *self {
            ParamValue::Float(v) => Value::from(v),
            ParamValue::Color(c) => Value::from(c),
        }
    }
}

impl From<f32> for ParamValue {
    fn from(value: f32) -> Self {
        ParamValue::Float(value)
    }
}

/// Linear RGBA.
impl From<Vec4> for ParamValue {
    fn from(value: Vec4) -> Self {
        ParamValue::Color(value)
    }
}

impl From<LinearRgba> for ParamValue {
    fn from(value: LinearRgba) -> Self {
        ParamValue::Color(value.to_vec4())
    }
}

/// Converted to linear RGBA (like particle colours).
impl From<Color> for ParamValue {
    fn from(value: Color) -> Self {
        ParamValue::Color(value.to_linear().to_vec4())
    }
}

/// Kind of a parameter, given by its default value.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ParamKind {
    Float,
    Color,
}

impl ParamKind {
    pub fn name(self) -> &'static str {
        match self {
            ParamKind::Float => "Float",
            ParamKind::Color => "Color",
        }
    }
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
        self.default.kind()
    }

    /// `value` clamped to `[min, max]` (Float; colours are kept as they are).
    pub fn clamp(&self, value: ParamValue) -> ParamValue {
        match value {
            ParamValue::Float(v) => {
                let (lo, hi) = (self.min.min(self.max), self.max.max(self.min));
                ParamValue::Float(v.clamp(lo, hi))
            }
            color => color,
        }
    }

    /// The middle of a Float parameter's range.
    pub fn middle(&self) -> f32 {
        (self.min + self.max) * 0.5
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

/// A layer value: a constant, a piecewise linear curve over a parameter, or
/// a curve over the layer's **age** (seconds since the layer started, or
/// since its trigger last played it).
///
/// ```ron
/// Const(20.0)
/// Param("intensity", [(0.0, 10.0), (1.0, 30.0)])
/// Age([(0.0, 0.2), (0.6, 3.0)])
/// ```
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub enum Binding<T> {
    Const(T),
    /// Parameter name and `(parameter value, layer value)` keys, sorted by
    /// parameter value. Outside the keys the nearest key's value is used.
    Param(String, Vec<(f32, T)>),
    /// `(age in seconds, layer value)` keys, sorted by age: the layer's age
    /// is the time since it started (a one-shot or triggered layer: since it
    /// last (re)started). Outside the keys the nearest key's value is used.
    Age(Vec<(f32, T)>),
}

impl<T: Default> Default for Binding<T> {
    fn default() -> Self {
        Binding::Const(T::default())
    }
}

impl<T> Binding<T> {
    /// The parameter a curve reads, `None` for a constant or an age curve.
    pub fn param(&self) -> Option<&str> {
        match self {
            Binding::Const(_) | Binding::Age(_) => None,
            Binding::Param(name, _) => Some(name),
        }
    }

    /// Whether the value is a curve over the layer's age.
    pub fn reads_age(&self) -> bool {
        matches!(self, Binding::Age(_))
    }

    /// The keys of a curve (over a parameter or the age); `None` for a
    /// constant.
    pub fn keys(&self) -> Option<&[(f32, T)]> {
        match self {
            Binding::Const(_) => None,
            Binding::Param(_, keys) | Binding::Age(keys) => Some(keys),
        }
    }
}

/// The value of piecewise linear `keys` at `x` (flat outside the keys);
/// `None` without keys.
pub fn curve_value<T>(keys: &[(f32, T)], x: f32) -> Option<T>
where
    T: Copy + Add<Output = T> + Mul<f32, Output = T>,
{
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

impl<T> Binding<T>
where
    T: Copy + Add<Output = T> + Mul<f32, Output = T>,
{
    /// Evaluates the binding at age 0; `param` returns a parameter's current
    /// value. `None` if the parameter is unknown or the curve has no keys.
    /// Use [`Binding::evaluate_at`] where the layer's age is known.
    pub fn evaluate(&self, param: impl FnOnce(&str) -> Option<f32>) -> Option<T> {
        self.evaluate_at(param, 0.0)
    }

    /// Evaluates the binding for a layer of `age` seconds.
    pub fn evaluate_at(&self, param: impl FnOnce(&str) -> Option<f32>, age: f32) -> Option<T> {
        match self {
            Binding::Const(v) => Some(*v),
            Binding::Param(name, keys) => curve_value(keys, param(name)?),
            Binding::Age(keys) => curve_value(keys, age),
        }
    }
}

/// The modifier kind a location inside a particle layer points at
/// (`effect.update_modifiers[2]` → `AccelModifier`).
fn modifier_kind_at(location: &[FieldStep], effect: &ParticleEffectDef) -> Option<&'static str> {
    match location {
        [FieldStep::Key(e), FieldStep::Key(list), FieldStep::Index(i), ..] if e == "effect" => match list.as_str() {
            "init_modifiers" => effect.init_modifiers.get(*i).map(|m| m.kind_name()),
            "update_modifiers" => effect.update_modifiers.get(*i).map(|m| m.kind_name()),
            "render_modifiers" => effect.render_modifiers.get(*i).map(|m| m.kind_name()),
            _ => None,
        },
        _ => None,
    }
}

fn unknown_field_text(field: &UnknownField, context: &[String]) -> String {
    let location = field.location_text();
    let place = if location.is_empty() { "at the top level".to_string() } else { format!("at {}", location) };
    let context = if context.is_empty() { String::new() } else { format!(" ({})", context.join(", ")) };
    format!(
        "unknown field \"{}\" {}{}; it is ignored (a typo, or a field of a newer format?)",
        field.name, place, context
    )
}

/// Fields old v1 editors wrote that the v1 model no longer reads: the `id`
/// of a modifier (its position in the editor's list). Not reported.
fn is_legacy_v1_field(field: &UnknownField) -> bool {
    field.name == "id"
        && matches!(
            field.location.as_slice(),
            [FieldStep::Key(e), FieldStep::Index(_), FieldStep::Key(list), FieldStep::Index(_)]
                if e == "effects" && list.ends_with("_modifiers")
        )
}

/// [`ProjectV2::describe_unknown_field`] for a v1 project.
fn describe_v1_unknown_field(project: &OmagariProject, field: &UnknownField) -> String {
    let context: Vec<String> = field
        .index_after("effects")
        .and_then(|i| project.effects.get(i))
        .map(|e| format!("effect \"{}\"", e.name))
        .into_iter()
        .collect();
    unknown_field_text(field, &context)
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

/// A texture input of Omagari's own shaders (surface layers now; shape,
/// trail, afterimage and distortion layers later): the asset and the channel
/// the shader reads, so one channel-packed texture can feed several slots.
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
    /// From the trigger `start` until the trigger `stop` (held emission, e.g.
    /// sparks along a blade between `swing_start` and `swing_end`; the same
    /// semantics as a trail's `emit: Trigger(start, stop)`). When they are
    /// the same trigger, it switches the layer on and off.
    ///
    /// ```ron
    /// play: Between(start: "swing_start", stop: "swing_end")
    /// ```
    Between { start: String, stop: String },
}

impl PlayDef {
    /// The trigger an `OnTrigger` layer waits for.
    pub fn trigger(&self) -> Option<&str> {
        match self {
            PlayDef::OnTrigger(name) => Some(name),
            _ => None,
        }
    }

    /// Every trigger the layer listens to: an `OnTrigger`'s trigger, a
    /// `Between`'s start and stop (once when they are the same).
    pub fn triggers(&self) -> Vec<&str> {
        match self {
            PlayDef::OnTrigger(name) => vec![name.as_str()],
            PlayDef::Between { start, stop } if start == stop => vec![start.as_str()],
            PlayDef::Between { start, stop } => vec![start.as_str(), stop.as_str()],
            _ => Vec::new(),
        }
    }

    /// `"Loop"`, `"OnStart"`, `"OnTrigger"`, `"While"`, `"Between"`.
    pub fn kind_name(&self) -> &'static str {
        match self {
            PlayDef::Loop => "Loop",
            PlayDef::OnStart => "OnStart",
            PlayDef::OnTrigger(_) => "OnTrigger",
            PlayDef::While(..) => "While",
            PlayDef::Between { .. } => "Between",
        }
    }

    /// What firing the trigger `name` does to a `Between` layer that is
    /// `held` (on): the new state, `None` when the trigger is not one of its
    /// two (or the layer is not `Between`).
    pub fn on_between(&self, name: &str, held: bool) -> Option<bool> {
        match self {
            PlayDef::Between { start, stop } if start == stop && start == name => Some(!held),
            PlayDef::Between { start, .. } if start == name => Some(true),
            PlayDef::Between { stop, .. } if stop == name => Some(false),
            _ => None,
        }
    }

    /// Renames the trigger `from` in `OnTrigger` and `Between`.
    pub fn rename_trigger(&mut self, from: &str, to: &str) {
        match self {
            PlayDef::OnTrigger(name) if name == from => *name = to.to_string(),
            PlayDef::Between { start, stop } => {
                for name in [start, stop] {
                    if name == from {
                        *name = to.to_string();
                    }
                }
            }
            _ => {}
        }
    }

    /// The parameter a `While` reads.
    pub fn param(&self) -> Option<&str> {
        match self {
            PlayDef::While(p, _) => Some(p),
            _ => None,
        }
    }

    /// Renames the parameter of a `While`.
    pub fn rename_param(&mut self, from: &str, to: &str) {
        if let PlayDef::While(name, _) = self {
            if name == from {
                *name = to.to_string();
            }
        }
    }

    /// Whether the runtime switches the layer's spawner on and off
    /// (`OnTrigger`, `While`, `Between`): such layers do not start by
    /// themselves.
    pub fn is_driven(&self) -> bool {
        matches!(self, PlayDef::OnTrigger(_) | PlayDef::While(..) | PlayDef::Between { .. })
    }
}

/// When a layer with its own clock plays (shape and light layers): its
/// `play`, an optional `duration` and its fades. One-shots (`OnStart` /
/// `OnTrigger` with a duration) fade in and out inside the duration; for
/// `Loop` / `While` / `Between` the age repeats every `duration` seconds.
pub trait TimedPlay {
    fn play_def(&self) -> &PlayDef;
    fn duration_secs(&self) -> Option<f32>;
    fn fade_def(&self) -> FadeDef;

    /// Whether the layer plays once per start / trigger for `duration`
    /// seconds (`OnStart` / `OnTrigger` with a duration).
    fn is_one_shot(&self) -> bool {
        self.duration_secs().is_some() && matches!(self.play_def(), PlayDef::OnStart | PlayDef::OnTrigger(_))
    }

    /// The one-shot length (`duration`, at least 0), for one-shots.
    fn one_shot_duration(&self) -> Option<f32> {
        self.duration_secs().filter(|_| self.is_one_shot()).map(|d| d.max(0.0))
    }

    /// The age bindings see: the age, wrapped by `duration` for layers that
    /// are not one-shots (the animation repeats).
    fn binding_age(&self, age: f32) -> f32 {
        match self.duration_secs() {
            Some(d) if d > 0.0 && !self.is_one_shot() => age.rem_euclid(d),
            _ => age,
        }
    }

    /// Level of a one-shot at `age`: up over `fade.in`, down over
    /// `fade.out` before `duration`, 0 after it. 1 for other layers.
    fn envelope(&self, age: f32) -> f32 {
        let Some(d) = self.one_shot_duration() else { return 1.0 };
        if age < 0.0 || age >= d {
            return 0.0;
        }
        let fade = self.fade_def();
        let up = if fade.fade_in > 0.0 { (age / fade.fade_in).clamp(0.0, 1.0) } else { 1.0 };
        let down = if fade.fade_out > 0.0 { ((d - age) / fade.fade_out).clamp(0.0, 1.0) } else { 1.0 };
        up * down
    }
}

/// Layer visible only while a Float parameter is in `[from, to]`, fading out
/// over `fade` (parameter units) outside the range, so neighbouring ranges
/// cross-fade. The factor multiplies the spawn rate and is readable as
/// `Property("visibility")` (multiply alpha by it to fade live particles).
///
/// ```ron
/// visible: Some((param: "intensity", from: 0.6, to: 1.0, fade: 0.1))
/// ```
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
#[serde(default)]
pub struct VisibleRangeDef {
    pub param: String,
    pub from: f32,
    pub to: f32,
    pub fade: f32,
}

impl VisibleRangeDef {
    /// 1 inside `[from, to]`, falling linearly to 0 over `fade` outside it
    /// (0 outside right away when `fade` is 0).
    pub fn factor(&self, value: f32) -> f32 {
        let (from, to) = (self.from.min(self.to), self.to.max(self.from));
        let distance = if value < from {
            from - value
        } else if value > to {
            value - to
        } else {
            0.0
        };
        if distance <= 0.0 {
            1.0
        } else if self.fade <= 0.0 {
            0.0
        } else {
            (1.0 - distance / self.fade).clamp(0.0, 1.0)
        }
    }
}

// --- layers ------------------------------------------------------------------

/// One visual part of a package. More kinds (distortion) are added as new
/// variants.
#[derive(Serialize, Deserialize, Clone)]
pub enum LayerDef {
    Particles(ParticlesLayerDef),
    /// Omagari's effect shader over the target's own meshes (see
    /// [`SurfaceLayerDef`]).
    Surface(SurfaceLayerDef),
    /// A simple 3D shape with the effect shader (see [`ShapeLayerDef`]).
    Shape(ShapeLayerDef),
    /// A ribbon between two moving sockets (see [`TrailLayerDef`]).
    Trail(TrailLayerDef),
    /// Fading copies of the target's meshes (see [`AfterimageLayerDef`]).
    Afterimage(AfterimageLayerDef),
    /// A point light at the attach point (see [`LightLayerDef`]).
    Light(LightLayerDef),
}

impl LayerDef {
    pub fn name(&self) -> &str {
        match self {
            LayerDef::Particles(p) => &p.name,
            LayerDef::Surface(s) => &s.name,
            LayerDef::Shape(s) => &s.name,
            LayerDef::Trail(s) => &s.name,
            LayerDef::Afterimage(s) => &s.name,
            LayerDef::Light(s) => &s.name,
        }
    }

    pub fn name_mut(&mut self) -> &mut String {
        match self {
            LayerDef::Particles(p) => &mut p.name,
            LayerDef::Surface(s) => &mut s.name,
            LayerDef::Shape(s) => &mut s.name,
            LayerDef::Trail(s) => &mut s.name,
            LayerDef::Afterimage(s) => &mut s.name,
            LayerDef::Light(s) => &mut s.name,
        }
    }

    /// `"particles"` / `"surface"` / `"shape"` / `"trail"` / `"afterimage"` /
    /// `"light"`.
    pub fn kind_name(&self) -> &'static str {
        match self {
            LayerDef::Particles(_) => "particles",
            LayerDef::Surface(_) => "surface",
            LayerDef::Shape(_) => "shape",
            LayerDef::Trail(_) => "trail",
            LayerDef::Afterimage(_) => "afterimage",
            LayerDef::Light(_) => "light",
        }
    }

    /// Where a particle, shape or light layer is attached (surface layers
    /// cover the target's meshes instead; trails use
    /// [`LayerDef::socket_names`]).
    pub fn attach(&self) -> Option<&AttachDef> {
        match self {
            LayerDef::Particles(p) => Some(&p.attach),
            LayerDef::Shape(s) => Some(&s.attach),
            LayerDef::Light(l) => Some(&l.attach),
            LayerDef::Surface(_) | LayerDef::Trail(_) | LayerDef::Afterimage(_) => None,
        }
    }

    /// Every socket name the layer uses (its attach, or a trail's two ends).
    pub fn socket_names(&self) -> Vec<&str> {
        match self {
            LayerDef::Trail(t) => t.socket_names(),
            _ => self.attach().map(|a| a.socket_names()).unwrap_or_default(),
        }
    }

    /// The effect material of a surface, shape, trail or afterimage layer.
    pub fn material(&self) -> Option<&EffectMaterialDef> {
        match self {
            LayerDef::Particles(_) | LayerDef::Light(_) => None,
            LayerDef::Surface(s) => Some(&s.material),
            LayerDef::Shape(s) => Some(&s.material),
            LayerDef::Trail(s) => Some(&s.material),
            LayerDef::Afterimage(s) => Some(&s.material),
        }
    }

    pub fn material_mut(&mut self) -> Option<&mut EffectMaterialDef> {
        match self {
            LayerDef::Particles(_) | LayerDef::Light(_) => None,
            LayerDef::Surface(s) => Some(&mut s.material),
            LayerDef::Shape(s) => Some(&mut s.material),
            LayerDef::Trail(s) => Some(&mut s.material),
            LayerDef::Afterimage(s) => Some(&mut s.material),
        }
    }

    pub fn as_light(&self) -> Option<&LightLayerDef> {
        match self {
            LayerDef::Light(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_light_mut(&mut self) -> Option<&mut LightLayerDef> {
        match self {
            LayerDef::Light(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_trail(&self) -> Option<&TrailLayerDef> {
        match self {
            LayerDef::Trail(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_trail_mut(&mut self) -> Option<&mut TrailLayerDef> {
        match self {
            LayerDef::Trail(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_afterimage(&self) -> Option<&AfterimageLayerDef> {
        match self {
            LayerDef::Afterimage(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_afterimage_mut(&mut self) -> Option<&mut AfterimageLayerDef> {
        match self {
            LayerDef::Afterimage(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_shape(&self) -> Option<&ShapeLayerDef> {
        match self {
            LayerDef::Shape(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_shape_mut(&mut self) -> Option<&mut ShapeLayerDef> {
        match self {
            LayerDef::Shape(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_particles(&self) -> Option<&ParticlesLayerDef> {
        match self {
            LayerDef::Particles(p) => Some(p),
            _ => None,
        }
    }

    pub fn as_particles_mut(&mut self) -> Option<&mut ParticlesLayerDef> {
        match self {
            LayerDef::Particles(p) => Some(p),
            _ => None,
        }
    }

    pub fn as_surface(&self) -> Option<&SurfaceLayerDef> {
        match self {
            LayerDef::Surface(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_surface_mut(&mut self) -> Option<&mut SurfaceLayerDef> {
        match self {
            LayerDef::Surface(s) => Some(s),
            _ => None,
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
    /// layer's simulation space), with `Param(..)` reads resolved against the
    /// package's `params`. See [`ParticleEffectDef::produce`]. Root layers
    /// played by the runtime (`OnTrigger`, `While`) do not start active.
    ///
    /// A Flipbook without its own grid gets none here; use
    /// [`ParticlesLayerDef::produce_in`], which reads the texture's grid.
    pub fn produce(&self, params: &[ParamDef], warnings: &mut Vec<String>) -> EffectAsset {
        self.produce_with_grid(params, None, warnings)
    }

    /// Builds the layer's effect as part of `project`: its parameters and
    /// the sprite-sheet grid of the layer's texture (for Flipbook). What the
    /// runtime uses.
    pub fn produce_in(&self, project: &ProjectV2, warnings: &mut Vec<String>) -> EffectAsset {
        self.produce_with_grid(&project.parameters, project.texture_grid(self), warnings)
    }

    fn produce_with_grid(&self, params: &[ParamDef], grid: Option<(u32, u32)>, warnings: &mut Vec<String>) -> EffectAsset {
        let mut effect = self
            .effect
            .produce_with_grid(&self.name, self.simulation_space.into(), params, grid, warnings);
        if self.play.is_driven() && self.parent.is_none() {
            effect.spawner = effect.spawner.with_starts_active(false);
        }
        effect
    }

    /// Every parameter the layer reads: expressions, `rate`, `visible` and
    /// `play: While`, sorted, without duplicates.
    pub fn params_used(&self) -> Vec<String> {
        let mut names = self.effect.params_read();
        if let Some(name) = self.rate.as_ref().and_then(|r| r.param()) {
            names.push(name.to_string());
        }
        if let Some(visible) = &self.visible {
            names.push(visible.param.clone());
        }
        if let PlayDef::While(name, _) = &self.play {
            names.push(name.clone());
        }
        names.sort();
        names.dedup();
        names
    }
}

// --- editor preview ----------------------------------------------------------

/// Editor-only preview settings.
///
/// The model fields (P3) are written only when they are not their default,
/// so older files and editors are unaffected.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct PreviewDef {
    pub stage: PreviewStage,
    pub camera: PreviewCamera,
    /// glTF model of the `Model` stage (`.glb` / `.gltf`, path in the game
    /// assets folder). Without one the stage shows the capsule.
    pub model: Option<String>,
    /// Animation clip: its name in the glTF file (`Gltf::named_animations`,
    /// e.g. Mixamo's `"mixamo.com"`) or `"Animation<index>"`.
    pub animation: Option<String>,
    pub animation_speed: f32,
    /// Node of the target model the preview instance is attached to (the
    /// game's target entity); `None` = the target model's root.
    pub attach_to: Option<String>,
    pub params: Vec<(String, ParamValue)>,
    pub motion: Option<PreviewMotion>,
    /// `(trigger, every N seconds)`.
    pub auto_triggers: Vec<(String, f32)>,
    /// Height of the preview target (the game's entity root) above the
    /// model's origin, metres: the model is placed this much below the target
    /// (e.g. 0.9 for a player root 0.9 m above the feet). 0 = the target is
    /// the model's origin.
    #[serde(skip_serializing_if = "is_default")]
    pub root_height: f32,
    /// glTF file the animation clip is taken from (game assets folder), e.g.
    /// a Mixamo clip exported without skin; `None` = the model's own file.
    /// Clips play on nodes with the same names (no retargeting).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub animation_file: Option<String>,
    /// `(trigger, clip time in seconds)`: fired each time the animation
    /// passes that time (every loop), e.g. at the hit frame of a swing.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub animation_triggers: Vec<(String, f32)>,
    /// Which preview model is the instance target; `attach_to` names a node
    /// of that model.
    #[serde(skip_serializing_if = "is_default")]
    pub target: PreviewTargetModel,
    /// A second model (a weapon) attached to a node of the first.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weapon: Option<PreviewWeaponDef>,
    /// "In place": the preview removes the horizontal root motion of the
    /// model's skeleton root (e.g. Mixamo's hips) after the animation, so
    /// the model stays on the target; its vertical bob is kept. Editor
    /// only; default on (written only when off).
    #[serde(skip_serializing_if = "is_true")]
    pub in_place: bool,
}

fn is_true(value: &bool) -> bool {
    *value
}

fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
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
            root_height: 0.0,
            animation_file: None,
            animation_triggers: Vec::new(),
            target: PreviewTargetModel::Body,
            weapon: None,
            in_place: true,
        }
    }
}

impl PreviewDef {
    /// The glTF file the animation clip comes from: `animation_file`, else
    /// the model.
    pub fn animation_source(&self) -> Option<&str> {
        self.animation_file.as_deref().or(self.model.as_deref())
    }

    /// Whether the stage shows a model (`Model` with a model path).
    pub fn shows_model(&self) -> bool {
        self.stage == PreviewStage::Model && self.model.is_some()
    }
}

/// The preview model the instance is attached to.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum PreviewTargetModel {
    /// The character (`model`, or the capsule).
    #[default]
    Body,
    /// The weapon model (`weapon`).
    Weapon,
}

/// A second preview model (a weapon) held by a node of the first, e.g.
/// `(model: "weapons/sword.glb", node: Some("mixamorig:RightHand"))`.
/// The weapon is a separate model: effects on the body do not cover it.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct PreviewWeaponDef {
    /// glTF file in the game assets folder.
    pub model: String,
    /// Node of the body model that holds it; `None` = the preview target.
    pub node: Option<String>,
    /// Local offset from the node, metres (in the node's axes, ignoring the
    /// node's scale).
    pub offset: Vec3,
    /// Local rotation, degrees (X, then Y, then Z).
    pub rotation: Vec3,
    /// Uniform scale.
    pub scale: f32,
}

impl Default for PreviewWeaponDef {
    fn default() -> Self {
        PreviewWeaponDef {
            model: String::new(),
            node: None,
            offset: Vec3::ZERO,
            rotation: Vec3::ZERO,
            scale: 1.0,
        }
    }
}

impl PreviewWeaponDef {
    /// The weapon's rotation from `rotation` (degrees, XYZ).
    pub fn quat(&self) -> Quat {
        Quat::from_euler(
            EulerRot::XYZ,
            self.rotation.x.to_radians(),
            self.rotation.y.to_radians(),
            self.rotation.z.to_radians(),
        )
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum PreviewStage {
    #[default]
    Air,
    Floor,
    Object,
    /// A 0.7 × 1.8 m capsule (origin at its centre) standing on the floor.
    Capsule,
    /// `preview.model` standing on the floor (the capsule without a model).
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

    /// The P3 preview fields default when missing and are written only when
    /// set, so older files (and the goldens) are unchanged.
    #[test]
    fn preview_model_fields_default_and_round_trip() {
        let old: ProjectV2 = ProjectV2::from_ron_str(r#"(version: 2, preview: Some((stage: Model, model: Some("a.glb"))))"#).unwrap();
        let preview = old.preview.clone().unwrap();
        assert_eq!(preview.root_height, 0.0);
        assert_eq!(preview.animation_file, None);
        assert!(preview.animation_triggers.is_empty());
        assert_eq!(preview.target, PreviewTargetModel::Body);
        assert_eq!(preview.weapon, None);
        assert_eq!(preview.animation_source(), Some("a.glb"));
        assert!(preview.shows_model());
        let text = old.to_ron_string().unwrap();
        for field in ["root_height", "animation_file", "animation_triggers", "target", "weapon", "in_place"] {
            assert!(!text.contains(field), "{} written: {}", field, text);
        }
        assert!(preview.in_place, "in place by default");
        let mut off = old.clone();
        off.preview.as_mut().unwrap().in_place = false;
        let text = off.to_ron_string().unwrap();
        assert!(text.contains("in_place: false"), "{}", text);
        assert!(!ProjectV2::from_ron_str(&text).unwrap().preview.unwrap().in_place);

        let full = r#"(version: 2, preview: Some((
            stage: Model, model: Some("anim/tpose.glb"), root_height: 0.9,
            animation: Some("mixamo.com"), animation_file: Some("anim/run.glb"),
            animation_triggers: [("swing", 0.3)], attach_to: Some("Grip"), target: Weapon,
            weapon: Some((model: "sword.glb", node: Some("mixamorig:RightHand"), rotation: (0.0, 90.0, 0.0))),
        )))"#;
        let project = ProjectV2::from_ron_str(full).unwrap();
        let preview = project.preview.clone().unwrap();
        assert_eq!(preview.animation_source(), Some("anim/run.glb"));
        let weapon = preview.weapon.clone().unwrap();
        assert_eq!((weapon.scale, weapon.offset), (1.0, Vec3::ZERO), "weapon defaults");
        assert!((weapon.quat() * Vec3::X - Vec3::NEG_Z).length() < 1e-5, "90° around Y");
        let again = ProjectV2::from_ron_str(&project.to_ron_string().unwrap()).unwrap();
        assert_eq!(again.preview, project.preview);
        assert!(!PreviewDef { stage: PreviewStage::Capsule, model: Some("a.glb".into()), ..default() }.shows_model());

        // Renaming a trigger renames the preview's animation triggers.
        let mut project = ProjectV2::from_ron_str(
            r#"(version: 2, triggers: [(name: "swing")], preview: Some((animation_triggers: [("swing", 0.3)])))"#,
        )
        .unwrap();
        project.rename_trigger("swing", "slash").unwrap();
        assert_eq!(project.preview.unwrap().animation_triggers, vec![("slash".to_string(), 0.3)]);
    }

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
        let effect = layer.produce(&project.parameters, &mut warnings);
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
        let effect = def.produce(&project.parameters, &mut warnings);
        assert!(effect.properties().is_empty());
        assert!(warnings[0].contains("Property(\"nope\") is not a runtime property"), "{:?}", warnings);
    }

    const PARAM_PROJECT: &str = r#"(
        parameters: [
            (name: "intensity", default: Float(0.25), min: 0.0, max: 1.0, smoothing: 0.4),
            (name: "tint", default: Color((0.65, 0.08, 0.95, 1.0))),
            (name: "hit flash", default: Float(0.0)),
        ],
        triggers: [(name: "tick", pulses: [(param: "intensity", to: 1.0, decay: 0.3)]), (name: "hit")],
        layers: [
            Particles((
                name: "mist",
                rate: Some(Param("intensity", [(0.0, 5.0), (1.0, 40.0)])),
                visible: Some((param: "intensity", from: 0.2, to: 1.0, fade: 0.1)),
                effect: (init_modifiers: [
                    SetAttribute((attr: "hdr_color", attr_expr: Operator(Mix(Param("tint"), Vec4((1.0, 1.0, 1.0, 1.0)), Param("intensity"))))),
                    SetAttribute((attr: "size", attr_expr: Operator(Clamp(Operator(Multiply(Param("intensity"), Param("hit flash"))), Float(0.05), Float(0.4))))),
                    SetAttribute((attr: "lifetime", attr_expr: Operator(Max(Float(0.2), Operator(SmoothStep(Float(0.0), Float(1.0), Param("intensity"))))))),
                    SetAttribute((attr: "alpha", attr_expr: Operator(Min(Float(1.0), Property("visibility"))))),
                ]),
            )),
            Particles((name: "burst", play: OnTrigger("hit"), effect: (spawner: (cycle_count: 1)))),
            Particles((name: "charge", play: While("intensity", 0.9))),
        ],
        preview: Some((params: [("intensity", Float(0.5))], auto_triggers: [("hit", 1.0)])),
    )"#;

    /// Each parameter an effect reads is one property (f32 / vec4, default
    /// value), however often it is read; bevy_hanabi generates the shaders.
    #[test]
    fn parameters_become_properties_declared_once() {
        let project = ProjectV2::from_ron_str(PARAM_PROJECT).unwrap();
        let (_, mist) = project.particle_layers().next().unwrap();
        assert_eq!(mist.effect.params_read(), vec!["hit flash", "intensity", "tint"]);
        assert_eq!(mist.params_used(), vec!["hit flash", "intensity", "tint"]);
        let mut warnings = Vec::new();
        let effect = mist.produce(&project.parameters, &mut warnings);
        assert!(warnings.is_empty(), "{:?}", warnings);
        let properties: Vec<(&str, Value)> =
            effect.properties().iter().map(|p| (p.name(), *p.default_value())).collect();
        assert_eq!(
            properties,
            vec![
                ("param_tint", Value::from(Vec4::new(0.65, 0.08, 0.95, 1.0))),
                ("param_intensity", Value::from(0.25f32)),
                ("param_hit_20_flash", Value::from(0.0f32)),
                ("visibility", Value::from(1.0f32)),
            ]
        );
        EffectShaderSources::generate(&effect, None, 0).expect("shaders with parameter properties");

        // Runtime-played layers do not start by themselves.
        let burst = project.layers[1].as_particles().unwrap().produce(&project.parameters, &mut warnings);
        assert!(!burst.spawner.starts_active());
        assert!(mist.produce(&project.parameters, &mut warnings).spawner.starts_active());

        // An unknown parameter reads 0.0, with a warning.
        let mut warnings = Vec::new();
        mist.produce(&[], &mut warnings);
        assert!(warnings.iter().any(|w| w.contains("Param(\"tint\") is not a parameter")), "{:?}", warnings);
    }

    #[test]
    fn parameter_property_names_are_valid_and_distinct() {
        assert_eq!(param_property_name("intensity"), "param_intensity");
        assert_eq!(param_property_name("hit_flash"), "param_hit__flash");
        assert_eq!(param_property_name("hit flash"), "param_hit_20_flash");
        assert_eq!(param_property_name("Glut-Stärke"), "param_Glut_2d_St_e4_rke");
        assert_ne!(param_property_name("a_b"), param_property_name("a b"));
        for name in ["x", "loop", "fn", "_", "1st"] {
            let property = param_property_name(name);
            assert!(property.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'), "{}", property);
        }
    }

    #[test]
    fn renaming_a_parameter_or_trigger_updates_every_reference() {
        let mut p = ProjectV2::from_ron_str(PARAM_PROJECT).unwrap();
        assert_eq!(
            p.param_users("intensity"),
            vec![
                "layer \"mist\" (expressions, rate, visible)".to_string(),
                "layer \"charge\" (play While)".to_string(),
                "trigger \"tick\" (pulse)".to_string(),
            ]
        );
        p.rename_param("intensity", "power").unwrap();
        assert!(p.param("intensity").is_none());
        assert_eq!(p.param_users("intensity"), Vec::<String>::new());
        assert_eq!(p.param_users("power").len(), 3);
        let mist = p.layers[0].as_particles().unwrap();
        assert_eq!(mist.rate.as_ref().unwrap().param(), Some("power"));
        assert_eq!(mist.visible.as_ref().unwrap().param, "power");
        assert!(mist.effect.params_read().contains(&"power".to_string()));
        assert_eq!(p.layers[2].as_particles().unwrap().play, PlayDef::While("power".into(), 0.9));
        assert_eq!(p.triggers[0].pulses[0].param, "power");
        assert_eq!(p.preview.as_ref().unwrap().params[0].0, "power");
        assert!(p.rename_param("power", "tint").unwrap_err().contains("already exists"));
        assert!(p.rename_param("power", "  ").is_err());
        assert!(p.rename_param("nope", "x").is_err());

        assert_eq!(p.trigger_users("hit"), vec!["burst".to_string()]);
        p.rename_trigger("hit", "impact").unwrap();
        assert_eq!(p.layers[1].as_particles().unwrap().play, PlayDef::OnTrigger("impact".into()));
        assert_eq!(p.preview.as_ref().unwrap().auto_triggers[0].0, "impact");
        assert!(p.rename_trigger("impact", "tick").is_err());

        assert_eq!(p.unique_param_name("power"), "power2");
        assert_eq!(p.unique_trigger_name("boom"), "boom");
    }

    #[test]
    fn age_bindings_evaluate_and_old_bindings_stay_as_they_were() {
        let age: Binding<f32> = ron::from_str("Age([(0.0, 1.0), (2.0, 3.0)])").unwrap();
        assert!(age.reads_age() && age.param().is_none());
        assert_eq!(age.evaluate_at(|_| None, 1.0), Some(2.0));
        assert_eq!(age.evaluate_at(|_| None, 5.0), Some(3.0), "flat past the last key");
        assert_eq!(age.evaluate(|_| None), Some(1.0), "evaluate = age 0");
        assert_eq!(age.keys().map(|k| k.len()), Some(2));
        assert_eq!(ron::to_string(&age).unwrap(), "Age([(0.0,1.0),(2.0,3.0)])");
        // The parameter form is unchanged (old files, goldens).
        let param: Binding<f32> = ron::from_str(r#"Param("x", [(0.0, 1.0), (1.0, 3.0)])"#).unwrap();
        assert_eq!(param.evaluate_at(|_| Some(0.5), 99.0), Some(2.0), "a parameter curve ignores the age");
        assert_eq!(ron::to_string(&param).unwrap(), r#"Param("x",[(0.0,1.0),(1.0,3.0)])"#);
        assert!(!param.reads_age());
        let colour: Binding<Vec4> = ron::from_str("Age([(0.0, (1.0, 0.0, 0.0, 1.0)), (1.0, (0.0, 0.0, 1.0, 1.0))])").unwrap();
        assert_eq!(evaluate_color_at(&colour, &|_| None, 0.5), Some(Vec4::new(0.5, 0.0, 0.5, 1.0)));
        let empty: Binding<f32> = Binding::Age(Vec::new());
        assert_eq!(empty.evaluate_at(|_| None, 1.0), None);

        // A material's age curves.
        let m: EffectMaterialDef = ron::from_str(
            r#"(opacity: Age([(0.0, 1.0), (0.5, 0.0)]), fill: Some((axis: U, to: Age([(0.0, 0.0), (0.2, 1.0)]))), emissive: Param("p", [(0.0, 1.0)]))"#,
        )
        .unwrap();
        assert_eq!(m.age_bindings(), vec![("opacity", 0.5), ("fill.to", 0.2)]);
        assert_eq!(m.binding_params(), vec![("emissive", "p")]);
        let v = m.evaluate_at(&|_| None, 0.1);
        assert!((v.opacity - 0.8).abs() < 1e-6 && (v.fill.unwrap().2 - 0.5).abs() < 1e-6);
        assert_eq!(m.evaluate(&|_| None).opacity, 1.0);
    }

    #[test]
    fn visible_ranges_fade_outside() {
        let v = VisibleRangeDef { param: "p".into(), from: 0.4, to: 0.6, fade: 0.2 };
        assert_eq!(v.factor(0.5), 1.0);
        assert_eq!(v.factor(0.4), 1.0);
        assert!((v.factor(0.3) - 0.5).abs() < 1e-5);
        assert!((v.factor(0.7) - 0.5).abs() < 1e-5);
        assert_eq!(v.factor(0.9), 0.0);
        let hard = VisibleRangeDef { fade: 0.0, ..v };
        assert_eq!(hard.factor(0.61), 0.0);
        assert_eq!(hard.factor(0.6), 1.0);
    }

    #[test]
    fn param_values_convert_from_numbers_and_colours() {
        assert_eq!(ParamValue::from(0.5), ParamValue::Float(0.5));
        let red = ParamValue::from(Color::srgb(1.0, 0.0, 0.0));
        assert_eq!(red, ParamValue::Color(Vec4::new(1.0, 0.0, 0.0, 1.0)));
        // sRGB 0.5 is about 0.214 linear.
        let grey = ParamValue::from(Color::srgb(0.5, 0.5, 0.5)).as_color().unwrap();
        assert!((grey.x - 0.214).abs() < 1e-3, "{}", grey);
        let def = ParamDef { min: 0.0, max: 2.0, ..default() };
        assert_eq!(def.clamp(ParamValue::Float(3.0)), ParamValue::Float(2.0));
        assert_eq!(def.middle(), 1.0);
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
