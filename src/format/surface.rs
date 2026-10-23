//! Surface layers and the effect material (P4, F4).
//!
//! A surface layer draws Omagari's effect shader over the meshes of the
//! target's own model: the model and its materials are never changed, the
//! runtime adds overlay copies of the meshes (same mesh, same skin, so they
//! animate with the body) with [`EffectMaterialDef`]'s look. Each overlay is
//! a *shell*: pushed outward along the normals by `offset` metres, so 2–3
//! shells give a glowing layer with volume.
//!
//! The material is shared with the later shape, trail and afterimage layers.
//! Every value is a [`Binding`]: a constant or a curve over a parameter of
//! the package (any names).
//!
//! ```ron
//! Surface((
//!     name: "skin",
//!     targets: AllMeshes,
//!     material: (
//!         blend: Add,
//!         color: Const((0.6, 0.1, 1.0, 0.6)),
//!         noise_a: Some((texture: (asset: "noise"), mapping: Triplanar, scale: Const(3.0), scroll: Const((0.0, 0.2)), strength: Const(0.8))),
//!         rim: Some((color: Const((1.0, 0.5, 1.0, 1.0)), power: Const(2.5), intensity: Const(1.5))),
//!     ),
//!     shells: [(offset: 0.0), (offset: 0.02, opacity: 0.5, noise_offset: 0.37)],
//!     fade: (in: 0.2, out: 0.4),
//! ))
//! ```

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::project::{Binding, ParamValue, TextureSlotDef, VisibleRangeDef};

/// Which meshes of the target's own model a surface layer covers. Names are
/// glTF names: a mesh entity is `<mesh>.<material>` (e.g.
/// `Beta_Surface.Beta_HighLimbsGeoSG3`); `describe_model` lists them.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
pub enum MeshTargetsDef {
    /// Every mesh of the target's own model (a weapon in the hand is a
    /// separate model and not covered).
    #[default]
    AllMeshes,
    /// Meshes by mesh name (`Beta_Surface`) or mesh entity name
    /// (`Beta_Surface.Beta_HighLimbsGeoSG3`).
    Meshes(Vec<String>),
    /// Meshes using one of these materials (`Beta_HighLimbsGeoSG3`).
    Materials(Vec<String>),
}

impl MeshTargetsDef {
    /// Whether a mesh entity matches: its entity `Name`, its glTF mesh name
    /// and its glTF material name (each may be missing).
    pub fn matches(&self, entity_name: Option<&str>, mesh_name: Option<&str>, material_name: Option<&str>) -> bool {
        match self {
            MeshTargetsDef::AllMeshes => true,
            MeshTargetsDef::Meshes(names) => names
                .iter()
                .any(|n| Some(n.as_str()) == entity_name || Some(n.as_str()) == mesh_name),
            MeshTargetsDef::Materials(names) => {
                // Without a glTF material name, the part of the entity name
                // after the last '.' (`<mesh>.<material>`).
                let material = material_name.or_else(|| entity_name.and_then(|n| n.rsplit_once('.').map(|(_, m)| m)));
                names.iter().any(|n| Some(n.as_str()) == material)
            }
        }
    }

    /// The names a `Meshes` / `Materials` list holds.
    pub fn names(&self) -> &[String] {
        match self {
            MeshTargetsDef::AllMeshes => &[],
            MeshTargetsDef::Meshes(n) | MeshTargetsDef::Materials(n) => n,
        }
    }
}

/// One overlay of a surface layer: pushed `offset` metres outward along the
/// normals (scaled by fit), with its own opacity (× the material's) and a
/// shift of the noise pattern so stacked shells do not repeat it exactly.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
#[serde(default)]
pub struct ShellDef {
    pub offset: f32,
    pub opacity: f32,
    pub noise_offset: f32,
}

impl Default for ShellDef {
    fn default() -> Self {
        ShellDef {
            offset: 0.0,
            opacity: 1.0,
            noise_offset: 0.0,
        }
    }
}

/// Seconds a layer takes to fade in when it starts and to fade out when the
/// instance stops (the instance waits for the fade out).
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
#[serde(default)]
pub struct FadeDef {
    #[serde(rename = "in")]
    pub fade_in: f32,
    #[serde(rename = "out")]
    pub fade_out: f32,
}

impl Default for FadeDef {
    fn default() -> Self {
        FadeDef {
            fade_in: 0.2,
            fade_out: 0.3,
        }
    }
}

impl FadeDef {
    /// One step of the fade level (0 = invisible, 1 = full): up by `dt /
    /// in` while `running`, down by `dt / out` otherwise; a fade of 0 s
    /// jumps.
    pub fn step(&self, level: f32, running: bool, dt: f32) -> f32 {
        let (target, time) = if running { (1.0, self.fade_in) } else { (0.0, self.fade_out) };
        if time <= 0.0 {
            return target;
        }
        let step = dt / time;
        if running {
            (level + step).min(1.0)
        } else {
            (level - step).max(0.0)
        }
    }
}

/// A layer of Omagari's effect shader over the target's own meshes.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct SurfaceLayerDef {
    pub name: String,
    pub targets: MeshTargetsDef,
    pub material: EffectMaterialDef,
    /// One overlay per shell; an empty list draws nothing.
    pub shells: Vec<ShellDef>,
    pub fade: FadeDef,
    pub visible: Option<VisibleRangeDef>,
}

impl Default for SurfaceLayerDef {
    fn default() -> Self {
        SurfaceLayerDef {
            name: "surface".to_string(),
            targets: MeshTargetsDef::AllMeshes,
            material: EffectMaterialDef::default(),
            shells: vec![ShellDef::default()],
            fade: FadeDef::default(),
            visible: None,
        }
    }
}

impl SurfaceLayerDef {
    /// Every parameter the layer reads (material bindings, `visible`),
    /// sorted, without duplicates.
    pub fn params_used(&self) -> Vec<String> {
        let mut names: Vec<String> = self.material.binding_params().into_iter().map(|(_, p)| p.to_string()).collect();
        if let Some(visible) = &self.visible {
            names.push(visible.param.clone());
        }
        names.sort();
        names.dedup();
        names
    }

    /// Renames the parameter `from` in every binding and `visible`.
    pub fn rename_param(&mut self, from: &str, to: &str) {
        self.material.rename_param(from, to);
        if let Some(visible) = &mut self.visible {
            if visible.param == from {
                visible.param = to.to_string();
            }
        }
    }

    /// Seconds the layer needs after a stop (its fade out).
    pub fn linger(&self) -> f32 {
        self.fade.fade_out.max(0.0)
    }
}

/// How the effect blends over what is behind it.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum EffectBlendDef {
    /// Normal transparency: covers the model by its opacity.
    #[default]
    Blend,
    /// Adds light (glow): never darkens, opacity scales the brightness.
    Add,
}

/// How a texture is laid over the mesh.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum NoiseMappingDef {
    /// Projected from three sides in the model's rest pose (metres): sticks
    /// to the body, needs no UVs. `scale` = repeats per metre.
    #[default]
    Triplanar,
    /// The mesh's own UVs; `scale` = repeats per UV square.
    Uv,
}

/// A scrolling noise texture that breaks up the colour and opacity.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct NoiseDef {
    pub texture: TextureSlotDef,
    pub mapping: NoiseMappingDef,
    /// Repeats per metre (Triplanar) or per UV square (Uv).
    pub scale: Binding<f32>,
    /// Texture squares per second.
    pub scroll: Binding<Vec2>,
    /// 0 = no effect, 1 = the texture fully shapes colour and opacity.
    pub strength: Binding<f32>,
    /// Sharpens the pattern around its middle grey: 1 = as the texture is,
    /// 2–4 = crisp patches and veins (soft cloudy noise needs it).
    pub contrast: Binding<f32>,
}

impl Default for NoiseDef {
    fn default() -> Self {
        NoiseDef {
            texture: TextureSlotDef::default(),
            mapping: NoiseMappingDef::Triplanar,
            scale: Binding::Const(3.0),
            scroll: Binding::Const(Vec2::new(0.0, 0.2)),
            strength: Binding::Const(0.8),
            contrast: Binding::Const(1.0),
        }
    }
}

/// A glow on the edges seen from the camera (fresnel).
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct RimDef {
    /// Linear RGBA; alpha = how much the rim adds to the opacity.
    pub color: Binding<Vec4>,
    /// Higher = thinner rim (1 = soft, 5 = thin).
    pub power: Binding<f32>,
    /// Brightness (above 1 blooms).
    pub intensity: Binding<f32>,
}

impl Default for RimDef {
    fn default() -> Self {
        RimDef {
            color: Binding::Const(Vec4::ONE),
            power: Binding::Const(2.5),
            intensity: Binding::Const(1.0),
        }
    }
}

/// A steady breathing of the opacity.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct EffectPulseDef {
    /// Pulses per second.
    pub frequency: Binding<f32>,
    /// 0 = none, 1 = fades fully out and in.
    pub amount: Binding<f32>,
}

impl Default for EffectPulseDef {
    fn default() -> Self {
        EffectPulseDef {
            frequency: Binding::Const(1.0),
            amount: Binding::Const(0.5),
        }
    }
}

/// The surface breaks up where the noise is below `threshold` (noise A, or
/// noise B without A), with a glowing edge: appear / disappear effects.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct DissolveDef {
    /// 0 = whole, 1 = gone.
    pub threshold: Binding<f32>,
    /// Width of the glowing edge, in noise values (0.05 = thin).
    pub edge_width: Binding<f32>,
    pub edge_color: Binding<Vec4>,
}

impl Default for DissolveDef {
    fn default() -> Self {
        DissolveDef {
            threshold: Binding::Const(0.0),
            edge_width: Binding::Const(0.05),
            edge_color: Binding::Const(Vec4::new(4.0, 2.0, 0.5, 1.0)),
        }
    }
}

/// An axis of the model's rest pose (glTF models: Y is up).
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum AxisDef {
    X,
    #[default]
    Y,
    Z,
    NegX,
    NegY,
    NegZ,
    /// The mesh's U texture coordinate (0..1): along a shape (around a ring,
    /// along a slash arc from its left end).
    U,
    /// The mesh's V texture coordinate (0..1): across a ring or arc (inner →
    /// outer edge), up a sphere or cylinder.
    V,
}

impl AxisDef {
    pub const ALL: [AxisDef; 8] =
        [AxisDef::Y, AxisDef::NegY, AxisDef::X, AxisDef::NegX, AxisDef::Z, AxisDef::NegZ, AxisDef::U, AxisDef::V];

    /// The rest-pose direction; zero for U / V.
    pub fn vector(self) -> Vec3 {
        match self {
            AxisDef::X => Vec3::X,
            AxisDef::Y => Vec3::Y,
            AxisDef::Z => Vec3::Z,
            AxisDef::NegX => Vec3::NEG_X,
            AxisDef::NegY => Vec3::NEG_Y,
            AxisDef::NegZ => Vec3::NEG_Z,
            AxisDef::U | AxisDef::V => Vec3::ZERO,
        }
    }

    /// Whether the axis is a texture coordinate (U / V) instead of a
    /// direction.
    pub fn is_uv(self) -> bool {
        matches!(self, AxisDef::U | AxisDef::V)
    }

    pub fn name(self) -> &'static str {
        match self {
            AxisDef::X => "X",
            AxisDef::Y => "Y (up)",
            AxisDef::Z => "Z",
            AxisDef::NegX => "-X",
            AxisDef::NegY => "-Y (down)",
            AxisDef::NegZ => "-Z",
            AxisDef::U => "U (along the mesh)",
            AxisDef::V => "V (across the mesh)",
        }
    }
}

/// Shows the effect only on a band of the model along an axis of its rest
/// pose: 0 = the model's low end (feet for Y), 1 = its high end (head),
/// measured over the covered meshes (a shape: over the shape). `from` 0,
/// `to` bound to a parameter = frost rising from the feet. Axis `U` / `V`
/// uses the mesh's texture coordinates instead (0..1): a slash arc swept
/// along `U` by `to: Age(..)`.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct FillDef {
    pub axis: AxisDef,
    pub from: Binding<f32>,
    pub to: Binding<f32>,
    /// Width of the soft edges (fraction of the model, 0.1 = a tenth).
    pub softness: Binding<f32>,
}

impl Default for FillDef {
    fn default() -> Self {
        FillDef {
            axis: AxisDef::Y,
            from: Binding::Const(0.0),
            to: Binding::Const(0.5),
            softness: Binding::Const(0.1),
        }
    }
}

/// A mask texture: its value multiplies the opacity.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct MaskDef {
    pub texture: TextureSlotDef,
    pub mapping: NoiseMappingDef,
    pub scale: Binding<f32>,
    /// 0 = no effect, 1 = full mask.
    pub strength: Binding<f32>,
}

impl Default for MaskDef {
    fn default() -> Self {
        MaskDef {
            texture: TextureSlotDef::default(),
            mapping: NoiseMappingDef::Uv,
            scale: Binding::Const(1.0),
            strength: Binding::Const(1.0),
        }
    }
}

/// What a binding reads (see [`EffectMaterialDef::age_bindings`]).
#[derive(Clone, Copy, PartialEq, Debug)]
enum BindingInput<'a> {
    Param(&'a str),
    /// The largest key age.
    Age(f32),
}

/// The look of Omagari's effect shader (surface and shape layers; trail and
/// afterimage layers later). Optional parts are off when `None`.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct EffectMaterialDef {
    pub blend: EffectBlendDef,
    /// Linear RGBA (values above 1 glow with bloom). A Color parameter can be
    /// used directly: `Param("tint", [])`.
    pub color: Binding<Vec4>,
    /// × the colour's alpha.
    pub opacity: Binding<f32>,
    /// Brightness multiplier (above 1 blooms).
    pub emissive: Binding<f32>,
    pub noise_a: Option<NoiseDef>,
    pub noise_b: Option<NoiseDef>,
    pub rim: Option<RimDef>,
    pub pulse: Option<EffectPulseDef>,
    /// 0 = nothing, 1 = three times as bright and half-way to opaque. Bind
    /// it to a parameter that a trigger pulses (a hit flash).
    pub flash: Option<Binding<f32>>,
    pub dissolve: Option<DissolveDef>,
    pub fill: Option<FillDef>,
    pub mask: Option<MaskDef>,
}

impl Default for EffectMaterialDef {
    fn default() -> Self {
        EffectMaterialDef {
            blend: EffectBlendDef::Blend,
            color: Binding::Const(Vec4::new(0.6, 0.1, 1.0, 0.5)),
            opacity: Binding::Const(1.0),
            emissive: Binding::Const(1.0),
            noise_a: None,
            noise_b: None,
            rim: None,
            pulse: None,
            flash: None,
            dissolve: None,
            fill: None,
            mask: None,
        }
    }
}

/// A Float binding's value at `age`; the binding's first key (or
/// `fallback`) when its parameter is unknown or not a Float.
fn float_value(b: &Binding<f32>, lookup: &dyn Fn(&str) -> Option<ParamValue>, age: f32, fallback: f32) -> f32 {
    b.evaluate_at(|n| lookup(n).and_then(|v| v.as_float()), age)
        .unwrap_or_else(|| first_key(b).unwrap_or(fallback))
}

/// A curve's first key value, or the constant.
fn first_key<T: Copy>(b: &Binding<T>) -> Option<T> {
    match b {
        Binding::Const(v) => Some(*v),
        Binding::Param(_, keys) | Binding::Age(keys) => keys.first().map(|k| k.1),
    }
}

/// A binding of a vector (`Vec2`, `Vec3`) at `age`: unknown parameters give
/// the curve's first key, no keys `fallback`.
pub fn vector_value<T>(b: &Binding<T>, lookup: &dyn Fn(&str) -> Option<ParamValue>, age: f32, fallback: T) -> T
where
    T: Copy + std::ops::Add<Output = T> + std::ops::Mul<f32, Output = T>,
{
    b.evaluate_at(|n| lookup(n).and_then(|v| v.as_float()), age)
        .unwrap_or_else(|| first_key(b).unwrap_or(fallback))
}

/// A number binding at `age` (public twin of the material's own lookup):
/// unknown parameters give the curve's first key, no keys `fallback`.
pub fn number_value(b: &Binding<f32>, lookup: &dyn Fn(&str) -> Option<ParamValue>, age: f32, fallback: f32) -> f32 {
    float_value(b, lookup, age, fallback)
}

/// A colour binding's value: a constant, a Color parameter itself
/// (`Param("tint", [])`), or a curve of colours over a Float parameter.
/// `None` when the parameter is unknown (or the curve has no keys).
pub fn evaluate_color(b: &Binding<Vec4>, lookup: &dyn Fn(&str) -> Option<ParamValue>) -> Option<Vec4> {
    evaluate_color_at(b, lookup, 0.0)
}

/// [`evaluate_color`] for a layer of `age` seconds (`Age` curves).
pub fn evaluate_color_at(b: &Binding<Vec4>, lookup: &dyn Fn(&str) -> Option<ParamValue>, age: f32) -> Option<Vec4> {
    match b {
        Binding::Const(c) => Some(*c),
        Binding::Param(name, _) => match lookup(name)? {
            ParamValue::Color(c) => Some(c),
            ParamValue::Float(x) => b.evaluate(|_| Some(x)),
        },
        Binding::Age(_) => b.evaluate_at(|_| None, age),
    }
}

fn color_value(b: &Binding<Vec4>, lookup: &dyn Fn(&str) -> Option<ParamValue>, age: f32, fallback: Vec4) -> Vec4 {
    evaluate_color_at(b, lookup, age).unwrap_or_else(|| first_key(b).unwrap_or(fallback))
}

/// [`EffectMaterialDef`] with every binding evaluated (CPU side; packed
/// into the shader's uniforms by the runtime). Parts that are off have
/// `None`.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct EffectValues {
    pub blend: EffectBlendDef,
    pub color: Vec4,
    pub opacity: f32,
    pub emissive: f32,
    pub noise_a: Option<NoiseValues>,
    pub noise_b: Option<NoiseValues>,
    /// (colour, power, intensity).
    pub rim: Option<(Vec4, f32, f32)>,
    /// (frequency, amount).
    pub pulse: Option<(f32, f32)>,
    pub flash: f32,
    /// (threshold, edge width, edge colour).
    pub dissolve: Option<(f32, f32, Vec4)>,
    /// (axis, from, to, softness).
    pub fill: Option<(AxisDef, f32, f32, f32)>,
    /// (mapping, channel, scale, strength).
    pub mask: Option<(NoiseMappingDef, super::project::ChannelDef, f32, f32)>,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct NoiseValues {
    pub mapping: NoiseMappingDef,
    pub channel: super::project::ChannelDef,
    pub scale: f32,
    pub scroll: Vec2,
    pub strength: f32,
    pub contrast: f32,
}

impl NoiseDef {
    fn values(&self, lookup: &dyn Fn(&str) -> Option<ParamValue>, age: f32) -> NoiseValues {
        NoiseValues {
            mapping: self.mapping,
            channel: self.texture.channel,
            scale: float_value(&self.scale, lookup, age, 1.0),
            scroll: vector_value(&self.scroll, lookup, age, Vec2::ZERO),
            strength: float_value(&self.strength, lookup, age, 0.0),
            contrast: float_value(&self.contrast, lookup, age, 1.0),
        }
    }
}

impl EffectMaterialDef {
    /// Evaluates every binding at age 0; `lookup` gives a parameter's
    /// current value. Unknown parameters use the curve's first key.
    pub fn evaluate(&self, lookup: &dyn Fn(&str) -> Option<ParamValue>) -> EffectValues {
        self.evaluate_at(lookup, 0.0)
    }

    /// Evaluates every binding for a layer of `age` seconds (`Age` curves).
    pub fn evaluate_at(&self, lookup: &dyn Fn(&str) -> Option<ParamValue>, age: f32) -> EffectValues {
        let f = |b: &Binding<f32>, fallback: f32| float_value(b, lookup, age, fallback);
        let c = |b: &Binding<Vec4>, fallback: Vec4| color_value(b, lookup, age, fallback);
        EffectValues {
            blend: self.blend,
            color: c(&self.color, Vec4::ONE),
            opacity: f(&self.opacity, 1.0),
            emissive: f(&self.emissive, 1.0),
            noise_a: self.noise_a.as_ref().map(|n| n.values(lookup, age)),
            noise_b: self.noise_b.as_ref().map(|n| n.values(lookup, age)),
            rim: self.rim.as_ref().map(|r| (c(&r.color, Vec4::ONE), f(&r.power, 2.0), f(&r.intensity, 1.0))),
            pulse: self.pulse.as_ref().map(|p| (f(&p.frequency, 1.0), f(&p.amount, 0.0))),
            flash: self.flash.as_ref().map(|x| f(x, 0.0)).unwrap_or(0.0),
            dissolve: self
                .dissolve
                .as_ref()
                .map(|d| (f(&d.threshold, 0.0), f(&d.edge_width, 0.0), c(&d.edge_color, Vec4::ONE))),
            fill: self
                .fill
                .as_ref()
                .map(|x| (x.axis, f(&x.from, 0.0), f(&x.to, 1.0), f(&x.softness, 0.0))),
            mask: self
                .mask
                .as_ref()
                .map(|m| (m.mapping, m.texture.channel, f(&m.scale, 1.0), f(&m.strength, 1.0))),
        }
    }

    /// The texture slots in use: `("noise_a" | "noise_b" | "mask", slot)`.
    pub fn texture_slots(&self) -> Vec<(&'static str, &TextureSlotDef)> {
        let mut slots = Vec::new();
        if let Some(n) = &self.noise_a {
            slots.push(("noise_a", &n.texture));
        }
        if let Some(n) = &self.noise_b {
            slots.push(("noise_b", &n.texture));
        }
        if let Some(m) = &self.mask {
            slots.push(("mask", &m.texture));
        }
        slots
    }

    pub fn texture_slots_mut(&mut self) -> Vec<&mut TextureSlotDef> {
        let mut slots = Vec::new();
        if let Some(n) = &mut self.noise_a {
            slots.push(&mut n.texture);
        }
        if let Some(n) = &mut self.noise_b {
            slots.push(&mut n.texture);
        }
        if let Some(m) = &mut self.mask {
            slots.push(&mut m.texture);
        }
        slots
    }

    /// Every binding that reads a parameter: `(value name, parameter)`, e.g.
    /// `("noise_a.strength", "intensity")`.
    pub fn binding_params(&self) -> Vec<(&'static str, &str)> {
        let mut out = Vec::new();
        self.visit_bindings(&mut |what, input| {
            if let BindingInput::Param(param) = input {
                out.push((what, param));
            }
        });
        out
    }

    /// Every `Age` curve: `(value name, its last key's age)`.
    pub fn age_bindings(&self) -> Vec<(&'static str, f32)> {
        let mut out = Vec::new();
        self.visit_bindings(&mut |what, input| {
            if let BindingInput::Age(last) = input {
                out.push((what, last));
            }
        });
        out
    }

    /// Whether any value is a curve over the layer's age.
    pub fn reads_age(&self) -> bool {
        !self.age_bindings().is_empty()
    }

    fn visit_bindings<'a>(&'a self, f: &mut dyn FnMut(&'static str, BindingInput<'a>)) {
        fn p<'a, T>(what: &'static str, b: &'a Binding<T>, f: &mut dyn FnMut(&'static str, BindingInput<'a>)) {
            match b {
                Binding::Const(_) => {}
                Binding::Param(name, _) => f(what, BindingInput::Param(name)),
                Binding::Age(keys) => f(what, BindingInput::Age(keys.iter().map(|k| k.0).fold(f32::NEG_INFINITY, f32::max))),
            }
        }
        p("color", &self.color, f);
        p("opacity", &self.opacity, f);
        p("emissive", &self.emissive, f);
        for (prefix, noise) in [("noise_a", &self.noise_a), ("noise_b", &self.noise_b)] {
            if let Some(n) = noise {
                let names: [&'static str; 4] = if prefix == "noise_a" {
                    ["noise_a.scale", "noise_a.scroll", "noise_a.strength", "noise_a.contrast"]
                } else {
                    ["noise_b.scale", "noise_b.scroll", "noise_b.strength", "noise_b.contrast"]
                };
                p(names[0], &n.scale, f);
                p(names[1], &n.scroll, f);
                p(names[2], &n.strength, f);
                p(names[3], &n.contrast, f);
            }
        }
        if let Some(r) = &self.rim {
            p("rim.color", &r.color, f);
            p("rim.power", &r.power, f);
            p("rim.intensity", &r.intensity, f);
        }
        if let Some(pulse) = &self.pulse {
            p("pulse.frequency", &pulse.frequency, f);
            p("pulse.amount", &pulse.amount, f);
        }
        if let Some(flash) = &self.flash {
            p("flash", flash, f);
        }
        if let Some(d) = &self.dissolve {
            p("dissolve.threshold", &d.threshold, f);
            p("dissolve.edge_width", &d.edge_width, f);
            p("dissolve.edge_color", &d.edge_color, f);
        }
        if let Some(fill) = &self.fill {
            p("fill.from", &fill.from, f);
            p("fill.to", &fill.to, f);
            p("fill.softness", &fill.softness, f);
        }
        if let Some(m) = &self.mask {
            p("mask.scale", &m.scale, f);
            p("mask.strength", &m.strength, f);
        }
    }

    /// Renames the parameter `from` in every binding.
    pub fn rename_param(&mut self, from: &str, to: &str) {
        fn r<T>(b: &mut Binding<T>, from: &str, to: &str) {
            if let Binding::Param(name, _) = b {
                if name == from {
                    *name = to.to_string();
                }
            }
        }
        r(&mut self.color, from, to);
        r(&mut self.opacity, from, to);
        r(&mut self.emissive, from, to);
        for n in [&mut self.noise_a, &mut self.noise_b].into_iter().flatten() {
            r(&mut n.scale, from, to);
            r(&mut n.scroll, from, to);
            r(&mut n.strength, from, to);
            r(&mut n.contrast, from, to);
        }
        if let Some(rim) = &mut self.rim {
            r(&mut rim.color, from, to);
            r(&mut rim.power, from, to);
            r(&mut rim.intensity, from, to);
        }
        if let Some(pulse) = &mut self.pulse {
            r(&mut pulse.frequency, from, to);
            r(&mut pulse.amount, from, to);
        }
        if let Some(flash) = &mut self.flash {
            r(flash, from, to);
        }
        if let Some(d) = &mut self.dissolve {
            r(&mut d.threshold, from, to);
            r(&mut d.edge_width, from, to);
            r(&mut d.edge_color, from, to);
        }
        if let Some(fill) = &mut self.fill {
            r(&mut fill.from, from, to);
            r(&mut fill.to, from, to);
            r(&mut fill.softness, from, to);
        }
        if let Some(m) = &mut self.mask {
            r(&mut m.scale, from, to);
            r(&mut m.strength, from, to);
        }
    }

    /// Whether a value of [`EffectMaterialDef::binding_params`] is a colour:
    /// colours may read a Color parameter directly, every other value needs
    /// a Float one.
    pub fn is_color_binding(what: &str) -> bool {
        matches!(what, "color" | "rim.color" | "dissolve.edge_color")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::{ChannelDef, ProjectV2};

    #[test]
    fn targets_match_by_entity_mesh_or_material_name() {
        let entity = Some("Beta_Surface.Beta_HighLimbsGeoSG3");
        let (mesh, material) = (Some("Beta_Surface"), Some("Beta_HighLimbsGeoSG3"));
        assert!(MeshTargetsDef::AllMeshes.matches(None, None, None));
        assert!(MeshTargetsDef::Meshes(vec!["Beta_Surface".into()]).matches(entity, mesh, material));
        assert!(MeshTargetsDef::Meshes(vec!["Beta_Surface.Beta_HighLimbsGeoSG3".into()]).matches(entity, None, None));
        assert!(!MeshTargetsDef::Meshes(vec!["Beta_Joints".into()]).matches(entity, mesh, material));
        assert!(MeshTargetsDef::Materials(vec!["Beta_HighLimbsGeoSG3".into()]).matches(entity, mesh, material));
        // Without glTF names: the material part of the entity name.
        assert!(MeshTargetsDef::Materials(vec!["Beta_HighLimbsGeoSG3".into()]).matches(entity, None, None));
        assert!(!MeshTargetsDef::Materials(vec!["Beta_Surface".into()]).matches(entity, mesh, material));
    }

    #[test]
    fn fades_step_linearly_and_reverse_smoothly() {
        let fade = FadeDef { fade_in: 0.5, fade_out: 1.0 };
        let mut level = 0.0;
        level = fade.step(level, true, 0.25);
        assert!((level - 0.5).abs() < 1e-6);
        level = fade.step(level, true, 1.0);
        assert_eq!(level, 1.0);
        level = fade.step(level, false, 0.25);
        assert!((level - 0.75).abs() < 1e-6);
        // Restarting during the fade out goes back up from where it is.
        level = fade.step(level, true, 0.125);
        assert!((level - 1.0).abs() < 1e-6);
        let instant = FadeDef { fade_in: 0.0, fade_out: 0.0 };
        assert_eq!(instant.step(0.0, true, 0.01), 1.0);
        assert_eq!(instant.step(1.0, false, 0.01), 0.0);
    }

    const LAYER: &str = r#"(version: 2,
        parameters: [
            (name: "power", default: Float(0.5)),
            (name: "tint", default: Color((0.2, 0.4, 0.6, 1.0))),
            (name: "flash", default: Float(0.0)),
        ],
        layers: [Surface((
            name: "skin",
            targets: Meshes(["Beta_Surface"]),
            material: (
                blend: Add,
                color: Param("tint", []),
                opacity: Param("power", [(0.0, 0.0), (1.0, 1.0)]),
                noise_a: Some((texture: (asset: "noise", channel: G), mapping: Uv, scale: Const(2.0), scroll: Param("power", [(0.0, (0.0, 0.0)), (1.0, (1.0, 2.0))]), strength: Const(0.5))),
                rim: Some((color: Param("power", [(0.0, (0.0, 0.0, 0.0, 1.0)), (1.0, (2.0, 2.0, 2.0, 1.0))]), power: Const(3.0), intensity: Const(2.0))),
                flash: Some(Param("flash", [(0.0, 0.0), (1.0, 1.0)])),
                fill: Some((axis: Y, to: Param("power", [(0.0, 0.0), (1.0, 1.0)]))),
            ),
            shells: [(offset: 0.0), (offset: 0.02, opacity: 0.5, noise_offset: 0.3)],
            fade: (in: 0.1, out: 0.5),
            visible: Some((param: "power", from: 0.2, to: 1.0, fade: 0.1)),
        ))],
    )"#;

    #[test]
    fn surface_layers_round_trip_and_default() {
        let project = ProjectV2::from_ron_str(LAYER).unwrap();
        let layer = project.layers[0].as_surface().unwrap();
        assert_eq!(layer.shells.len(), 2);
        assert_eq!(layer.shells[0], ShellDef::default());
        assert_eq!(layer.shells[1].noise_offset, 0.3);
        assert_eq!(layer.fade, FadeDef { fade_in: 0.1, fade_out: 0.5 });
        assert_eq!(layer.material.noise_a.as_ref().unwrap().texture.channel, ChannelDef::G);
        let text = project.to_ron_string().unwrap();
        assert!(text.contains("in: 0.1") && text.contains("out: 0.5"), "{}", text);
        let again = ProjectV2::from_ron_str(&text).unwrap();
        assert_eq!(again.layers[0].as_surface(), Some(layer));

        // A minimal layer gets one shell at 0 and the default fades.
        let minimal = ProjectV2::from_ron_str(r#"(version: 2, layers: [Surface((name: "s"))])"#).unwrap();
        let s = minimal.layers[0].as_surface().unwrap();
        assert_eq!(s.shells, vec![ShellDef::default()]);
        assert_eq!(s.targets, MeshTargetsDef::AllMeshes);
        assert_eq!(s.linger(), 0.3);
    }

    #[test]
    fn bindings_evaluate_from_parameters() {
        let project = ProjectV2::from_ron_str(LAYER).unwrap();
        let layer = project.layers[0].as_surface().unwrap();
        let lookup = |power: f32| {
            move |name: &str| match name {
                "power" => Some(ParamValue::Float(power)),
                "tint" => Some(ParamValue::Color(Vec4::new(0.2, 0.4, 0.6, 1.0))),
                "flash" => Some(ParamValue::Float(1.0)),
                _ => None,
            }
        };
        let v = layer.material.evaluate(&lookup(0.5));
        assert_eq!(v.blend, EffectBlendDef::Add);
        assert_eq!(v.color, Vec4::new(0.2, 0.4, 0.6, 1.0), "a Color parameter is used directly");
        assert_eq!(v.opacity, 0.5);
        let noise = v.noise_a.unwrap();
        assert_eq!((noise.mapping, noise.channel, noise.scale), (NoiseMappingDef::Uv, ChannelDef::G, 2.0));
        assert_eq!(noise.scroll, Vec2::new(0.5, 1.0));
        assert_eq!(v.rim.unwrap().0, Vec4::new(1.0, 1.0, 1.0, 1.0), "colour curve over a Float parameter");
        assert_eq!(v.flash, 1.0);
        assert_eq!(v.fill, Some((AxisDef::Y, 0.0, 0.5, 0.1)));
        assert_eq!(v.noise_b, None);
        assert_eq!(v.dissolve, None);

        // Unknown parameters: the first key.
        let none = layer.material.evaluate(&|_| None);
        assert_eq!(none.opacity, 0.0);
        assert_eq!(none.color, Vec4::ONE, "no keys: white");

        assert_eq!(layer.params_used(), vec!["flash", "power", "tint"]);
        let reads: Vec<&str> = layer.material.binding_params().iter().map(|(w, _)| *w).collect();
        assert_eq!(reads, vec!["color", "opacity", "noise_a.scroll", "rim.color", "flash", "fill.to"]);
    }

    #[test]
    fn renames_and_users_reach_surface_layers() {
        let mut p = ProjectV2::from_ron_str(
            r#"(version: 2,
                parameters: [(name: "power", default: Float(0.5)), (name: "tint", default: Color((1.0, 1.0, 1.0, 1.0)))],
                triggers: [(name: "tick", pulses: [(param: "power", to: 1.0, decay: 0.2)])],
                assets: [(id: "noise", kind: Texture((path: "omagari://textures/noise.png", purpose: Noise)))],
                layers: [
                    Surface((
                        name: "skin",
                        material: (
                            color: Param("tint", []),
                            noise_a: Some((texture: (asset: "noise"), strength: Param("power", [(0.0, 0.0), (1.0, 1.0)]))),
                            mask: Some((texture: (asset: "noise", channel: R))),
                            dissolve: Some((threshold: Param("power", [(0.0, 1.0), (1.0, 0.0)]))),
                        ),
                        visible: Some((param: "power", from: 0.1, to: 1.0)),
                    )),
                    Particles((name: "dust")),
                ],
            )"#,
        )
        .unwrap();
        assert_eq!(p.asset_users("noise"), vec!["skin"]);
        assert_eq!(
            p.param_users("power"),
            vec!["layer \"skin\" (noise_a.strength, dissolve.threshold, visible)".to_string(), "trigger \"tick\" (pulse)".to_string()]
        );
        assert_eq!(p.param_users("tint"), vec!["layer \"skin\" (color)".to_string()]);
        assert!(p.delete_asset("noise").unwrap_err().contains("\"skin\""), "used by the surface layer");

        p.rename_asset("noise", "clouds").unwrap();
        let skin = p.layers[0].as_surface().unwrap();
        assert_eq!(skin.material.noise_a.as_ref().unwrap().texture.asset, "clouds");
        assert_eq!(skin.material.mask.as_ref().unwrap().texture.asset, "clouds");
        assert_eq!(skin.material.mask.as_ref().unwrap().texture.channel, ChannelDef::R);

        p.rename_param("power", "strength").unwrap();
        p.rename_param("tint", "colour").unwrap();
        let skin = p.layers[0].as_surface().unwrap();
        assert_eq!(skin.params_used(), vec!["colour", "strength"]);
        assert_eq!(skin.visible.as_ref().unwrap().param, "strength");
        assert_eq!(skin.material.dissolve.as_ref().unwrap().threshold.param(), Some("strength"));
        assert!(p.param_users("power").is_empty());
        assert_eq!(p.layers[1].kind_name(), "particles");
        assert_eq!(p.layers[0].kind_name(), "surface");
    }
}
