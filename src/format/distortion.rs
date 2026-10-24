//! Distortion layers (P7, F6): the air bends — heat haze around a blade,
//! a shockwave ripple, an aura shimmering around a body.
//!
//! A distortion layer draws a volume (a [`ShapeDef`] placed like a shape
//! layer, a capsule stretched between two sockets, or overlay copies of the
//! target's own meshes pushed outward) that shows what is behind it, moved
//! by scrolling noise. `strength` is the largest offset as a **fraction of the
//! view height** (0.005 ≈ 5 px on a 1080 px tall view), the same on
//! orthographic and perspective cameras.
//!
//! Limits (Bevy's screen-space transmission): only **opaque** things behind
//! the volume bend (the floor, the model, walls); transparent particles and
//! Omagari's transparent layers are drawn after it and stay sharp, and
//! particles cannot be distortion sources (use a shape around them).
//!
//! ```ron
//! Distortion((
//!     name: "haze",
//!     attach: Segment(from: "blade_base", to: "blade_tip"),
//!     source: Shape(shape: Capsule(radius: 0.12)),
//!     strength: Param("heat", [(0.0, 0.0), (1.0, 0.01)]),
//!     noise: (texture: (asset: "noise"), scale: 1.2, scroll: (0.0, 0.7)),
//! ))
//! ```

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::project::{AttachDef, Binding, ParamValue, PlayDef, TextureSlotDef, TimedPlay, VisibleRangeDef};
use super::shape::{FollowDef, ShapeDef, ShapeTransformDef};
use super::surface::{evaluate_color_at, number_value, vector_value, FadeDef, MeshTargetsDef, NoiseMappingDef};

/// Strengths above this (fraction of the view height) are reported by the
/// check: the picture tears instead of shimmering.
pub const STRONG_DISTORTION: f32 = 0.05;

fn default_shape() -> ShapeDef {
    ShapeDef::Sphere { radius: 0.5 }
}

fn default_offset() -> f32 {
    0.03
}

/// What the distortion is drawn on.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub enum DistortionSourceDef {
    /// A shape at the attach point (placed like a shape layer: `follow`,
    /// `transform`, the layer's `scale`). With `attach: Segment`, a
    /// `Capsule` runs from one socket to the other (its `length` is ignored,
    /// its radius kept); other shapes sit at the middle with Y along the
    /// segment.
    Shape {
        #[serde(default = "default_shape")]
        shape: ShapeDef,
        #[serde(default)]
        follow: FollowDef,
        #[serde(default)]
        transform: ShapeTransformDef,
    },
    /// Copies of the target's own meshes (like a surface layer's overlays,
    /// skinned with the body) pushed `offset` metres outward along the
    /// normals: the air around the body shimmers. The layer's `attach` is
    /// not used.
    Surface {
        #[serde(default)]
        targets: MeshTargetsDef,
        #[serde(default = "default_offset")]
        offset: f32,
    },
}

impl Default for DistortionSourceDef {
    fn default() -> Self {
        DistortionSourceDef::Shape {
            shape: default_shape(),
            follow: FollowDef::Full,
            transform: ShapeTransformDef::default(),
        }
    }
}

impl DistortionSourceDef {
    /// `"Shape"` / `"Surface"`.
    pub fn kind_name(&self) -> &'static str {
        match self {
            DistortionSourceDef::Shape { .. } => "Shape",
            DistortionSourceDef::Surface { .. } => "Surface",
        }
    }

    pub fn shape(&self) -> Option<&ShapeDef> {
        match self {
            DistortionSourceDef::Shape { shape, .. } => Some(shape),
            DistortionSourceDef::Surface { .. } => None,
        }
    }
}

/// Where the volume is strongest.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum DistortionEdgeDef {
    /// Strongest where the surface faces the camera (the middle of a sphere,
    /// along a capsule's axis), fading to nothing at the outline: haze.
    #[default]
    Soft,
    /// Strongest in a band inside the outline, fading toward the outline and
    /// toward the middle: a ring on a sphere (shockwave), a shimmering
    /// outline around a body.
    Rim,
}

impl DistortionEdgeDef {
    pub const ALL: [DistortionEdgeDef; 2] = [DistortionEdgeDef::Soft, DistortionEdgeDef::Rim];

    pub fn name(self) -> &'static str {
        match self {
            DistortionEdgeDef::Soft => "Soft",
            DistortionEdgeDef::Rim => "Rim",
        }
    }
}

/// A scrolling noise texture that moves the picture: two lookups of its
/// channel give the x and y offsets.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct DistortionNoiseDef {
    /// A noise texture (asset id + channel); empty = no wobble.
    pub texture: TextureSlotDef,
    /// `Triplanar` (metres of the shape / rest pose of the model) or `Uv`.
    pub mapping: NoiseMappingDef,
    /// Repeats per metre (Triplanar) or per UV square: higher = finer
    /// ripples.
    pub scale: f32,
    /// Texture squares per second: `(0, 0.5)` rises slowly.
    pub scroll: Vec2,
    /// Sharpens the noise around middle grey (1 = as it is).
    pub contrast: f32,
    /// How much of the offset comes from this texture (0..1).
    pub weight: f32,
}

impl Default for DistortionNoiseDef {
    fn default() -> Self {
        DistortionNoiseDef {
            texture: TextureSlotDef::default(),
            mapping: NoiseMappingDef::Triplanar,
            scale: 1.5,
            scroll: Vec2::new(0.0, 0.6),
            contrast: 1.0,
            weight: 1.0,
        }
    }
}

impl DistortionNoiseDef {
    /// Whether a texture is set.
    pub fn has_texture(&self) -> bool {
        !self.texture.asset.trim().is_empty()
    }
}

/// A distortion layer's bound values at one moment (before fades).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct DistortionValues {
    /// Fraction of the view height.
    pub strength: f32,
    pub scale: Vec3,
    /// rgb multiplier, a amount.
    pub tint: Vec4,
}

/// The air bends inside a volume: heat haze, a shockwave ripple, a
/// shimmering aura.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct DistortionLayerDef {
    pub name: String,
    /// `Target`, `Socket("name")` or `Segment(from, to)` (a capsule source
    /// spans the segment). Not used by a `Surface` source.
    pub attach: AttachDef,
    pub source: DistortionSourceDef,
    /// Scale (X, Y, Z) of a shape source; over `Age` for a growing ripple.
    pub scale: Binding<Vec3>,
    /// Largest offset as a fraction of the view height: 0.005–0.012 is haze,
    /// 0.01–0.03 a strong ripple; 0 draws nothing (and costs nothing).
    pub strength: Binding<f32>,
    pub noise: DistortionNoiseDef,
    /// A second noise (another scale / scroll) mixed in for a less regular
    /// wobble.
    pub noise_b: Option<DistortionNoiseDef>,
    pub edge: DistortionEdgeDef,
    /// 0..1: how much of the offset follows the surface's direction on
    /// screen instead of the noise — a lens: 0 = pure shimmer, 0.6–1 = the
    /// clean bulge of a shockwave ring (with `edge: Rim` on a sphere).
    pub refraction: f32,
    /// 0..1: how far in from the outline the effect reaches full strength
    /// (`Soft`) or where its band peaks (`Rim`).
    pub edge_softness: f32,
    /// Colour multiplied into what is behind (rgb) and how much (a, 0..1):
    /// `(1.15, 1.0, 0.85, 0.3)` warms the haze slightly. A Color parameter
    /// can be bound directly.
    pub tint: Binding<Vec4>,
    /// `Loop`, `OnStart`, `OnTrigger("name")` (a ripple with a `duration`),
    /// `While("param", threshold)`, `Between(start: "a", stop: "b")`.
    pub play: PlayDef,
    pub duration: Option<f32>,
    pub fade: FadeDef,
    pub visible: Option<VisibleRangeDef>,
    /// Added to the distance Bevy sorts distortion volumes by (metres); only
    /// matters where two volumes overlap.
    pub sort_bias: f32,
}

impl Default for DistortionLayerDef {
    fn default() -> Self {
        DistortionLayerDef {
            name: "distortion".to_string(),
            attach: AttachDef::Target,
            source: DistortionSourceDef::default(),
            scale: Binding::Const(Vec3::ONE),
            strength: Binding::Const(0.008),
            noise: DistortionNoiseDef::default(),
            noise_b: None,
            edge: DistortionEdgeDef::Soft,
            refraction: 0.0,
            edge_softness: 0.6,
            tint: Binding::Const(Vec4::new(1.0, 1.0, 1.0, 0.0)),
            play: PlayDef::Loop,
            duration: None,
            fade: FadeDef::default(),
            visible: None,
            sort_bias: 0.0,
        }
    }
}

impl TimedPlay for DistortionLayerDef {
    fn play_def(&self) -> &PlayDef {
        &self.play
    }

    fn duration_secs(&self) -> Option<f32> {
        self.duration
    }

    fn fade_def(&self) -> FadeDef {
        self.fade
    }
}

fn last_key<T>(keys: &[(f32, T)]) -> f32 {
    keys.iter().map(|k| k.0).fold(f32::NEG_INFINITY, f32::max)
}

impl DistortionLayerDef {
    /// Whether colour values may read a Color parameter (`"tint"`).
    pub fn is_color_binding(what: &str) -> bool {
        what == "tint"
    }

    /// Whether the volume is a capsule stretched between the two sockets of
    /// a `Segment` attach.
    pub fn is_segment_capsule(&self) -> bool {
        matches!(self.attach, AttachDef::Segment { .. })
            && matches!(&self.source, DistortionSourceDef::Shape { shape: ShapeDef::Capsule { .. }, .. })
    }

    /// The noise textures in use: `("noise" | "noise_b", slot)`.
    pub fn texture_slots(&self) -> Vec<(&'static str, &TextureSlotDef)> {
        let mut slots = Vec::new();
        if self.noise.has_texture() {
            slots.push(("noise", &self.noise.texture));
        }
        if let Some(n) = self.noise_b.as_ref().filter(|n| n.has_texture()) {
            slots.push(("noise_b", &n.texture));
        }
        slots
    }

    pub fn texture_slots_mut(&mut self) -> Vec<&mut TextureSlotDef> {
        let mut slots = vec![&mut self.noise.texture];
        if let Some(n) = &mut self.noise_b {
            slots.push(&mut n.texture);
        }
        slots
    }

    /// Every binding that reads a parameter: `(value name, parameter)`.
    pub fn binding_params(&self) -> Vec<(&'static str, &str)> {
        [("strength", self.strength.param()), ("scale", self.scale.param()), ("tint", self.tint.param())]
            .into_iter()
            .filter_map(|(what, p)| p.map(|p| (what, p)))
            .collect()
    }

    /// Every parameter the layer reads (bindings, `visible`, `play: While`),
    /// sorted, without duplicates.
    pub fn params_used(&self) -> Vec<String> {
        let mut names: Vec<String> = self.binding_params().into_iter().map(|(_, p)| p.to_string()).collect();
        if let Some(visible) = &self.visible {
            names.push(visible.param.clone());
        }
        if let Some(p) = self.play.param() {
            names.push(p.to_string());
        }
        names.sort();
        names.dedup();
        names
    }

    /// Renames the parameter `from` in every binding, `visible` and `play:
    /// While`.
    pub fn rename_param(&mut self, from: &str, to: &str) {
        let rename = |name: &mut String| {
            if name == from {
                *name = to.to_string();
            }
        };
        if let Binding::Param(name, _) = &mut self.strength {
            rename(name);
        }
        if let Binding::Param(name, _) = &mut self.scale {
            rename(name);
        }
        if let Binding::Param(name, _) = &mut self.tint {
            rename(name);
        }
        if let Some(visible) = &mut self.visible {
            rename(&mut visible.param);
        }
        self.play.rename_param(from, to);
    }

    /// The age keys of every `Age` binding: `(value name, largest key age)`.
    pub fn age_bindings(&self) -> Vec<(&'static str, f32)> {
        let mut out = Vec::new();
        if let Binding::Age(keys) = &self.strength {
            out.push(("strength", last_key(keys)));
        }
        if let Binding::Age(keys) = &self.scale {
            out.push(("scale", last_key(keys)));
        }
        if let Binding::Age(keys) = &self.tint {
            out.push(("tint", last_key(keys)));
        }
        out
    }

    /// The bound values for parameter values from `lookup` at the binding
    /// age `age` (unknown parameters give a curve's first key).
    pub fn values_at(&self, lookup: &dyn Fn(&str) -> Option<ParamValue>, age: f32) -> DistortionValues {
        let tint = evaluate_color_at(&self.tint, lookup, age)
            .or_else(|| self.tint.keys().and_then(|k| k.first()).map(|k| k.1))
            .unwrap_or(Vec4::new(1.0, 1.0, 1.0, 0.0));
        DistortionValues {
            strength: number_value(&self.strength, lookup, age, 0.0),
            scale: vector_value(&self.scale, lookup, age, Vec3::ONE),
            tint,
        }
    }

    /// Seconds the layer needs after a stop (its fade out).
    pub fn linger(&self) -> f32 {
        self.fade.fade_out.max(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::{LayerDef, ProjectV2};

    #[test]
    fn distortion_layers_round_trip_with_defaults() {
        let project = ProjectV2::from_ron_str(
            r#"(version: 2,
                parameters: [(name: "heat", default: Float(0.5))],
                triggers: [(name: "slam")],
                sockets: [(name: "a"), (name: "b", offset: (0.0, 1.0, 0.0))],
                assets: [(id: "noise", kind: Texture((path: "omagari://textures/noise.png", purpose: Noise)))],
                layers: [
                    Distortion((name: "haze", attach: Segment(from: "a", to: "b"), source: Shape(shape: Capsule(radius: 0.1)),
                        strength: Param("heat", [(0.0, 0.0), (1.0, 0.008)]), noise: (texture: (asset: "noise"), scroll: (0.0, 1.0)))),
                    Distortion((name: "d")),
                    Distortion((name: "ripple", source: Shape(shape: Sphere(radius: 1.0), follow: PositionYaw), edge: Rim,
                        scale: Age([(0.0, (0.2, 0.2, 0.2)), (0.5, (3.0, 3.0, 3.0))]), play: OnTrigger("slam"), duration: Some(0.5))),
                    Distortion((name: "aura", source: Surface(), tint: Const((1.2, 1.0, 0.8, 0.3)))),
                ])"#,
        )
        .unwrap();
        let haze = project.layers[0].as_distortion().unwrap();
        assert!(haze.is_segment_capsule());
        assert_eq!(haze.source.shape(), Some(&ShapeDef::Capsule { radius: 0.1, length: 1.0 }));
        assert_eq!(haze.noise.scale, 1.5, "default scale");
        assert_eq!(haze.noise.scroll, Vec2::new(0.0, 1.0));
        let d = project.layers[1].as_distortion().unwrap();
        assert_eq!(d, &DistortionLayerDef { name: "d".into(), ..Default::default() });
        assert!(d.texture_slots().is_empty(), "no texture by default");
        let ripple = project.layers[2].as_distortion().unwrap();
        assert!(ripple.is_one_shot());
        assert_eq!(ripple.values_at(&|_| None, 0.25).scale, Vec3::splat(1.6));
        let aura = project.layers[3].as_distortion().unwrap();
        assert_eq!(aura.source, DistortionSourceDef::Surface { targets: MeshTargetsDef::AllMeshes, offset: 0.03 });
        assert_eq!(aura.values_at(&|_| None, 0.0).tint, Vec4::new(1.2, 1.0, 0.8, 0.3));

        let text = project.to_ron_string().unwrap();
        let again = ProjectV2::from_ron_str(&text).unwrap();
        for i in 0..4 {
            assert_eq!(again.layers[i].as_distortion(), project.layers[i].as_distortion(), "{}", text);
        }
        assert_eq!(project.layers[0].kind_name(), "distortion");
        assert!(matches!(project.layers[0], LayerDef::Distortion(_)));
        assert_eq!(project.layers[0].socket_names(), vec!["a", "b"]);
        assert!(project.layers[3].socket_names().is_empty(), "a surface source does not use the attach");
    }

    #[test]
    fn values_follow_parameters_and_age() {
        let def = DistortionLayerDef {
            strength: Binding::Param("heat".into(), vec![(0.0, 0.0), (1.0, 0.01)]),
            tint: Binding::Param("color".into(), vec![]),
            ..Default::default()
        };
        let lookup = |name: &str| match name {
            "heat" => Some(ParamValue::Float(0.5)),
            "color" => Some(ParamValue::Color(Vec4::new(1.0, 0.5, 0.2, 0.4))),
            _ => None,
        };
        let v = def.values_at(&lookup, 0.0);
        assert!((v.strength - 0.005).abs() < 1e-7);
        assert_eq!(v.tint, Vec4::new(1.0, 0.5, 0.2, 0.4), "a Color parameter bound directly");
        // Unknown parameter: the first key.
        assert_eq!(def.values_at(&|_| None, 0.0).strength, 0.0);
        assert_eq!(def.binding_params(), vec![("strength", "heat"), ("tint", "color")]);
    }

    #[test]
    fn renames_and_users_reach_distortion_layers() {
        let mut p = ProjectV2::from_ron_str(
            r#"(version: 2,
                parameters: [(name: "heat", default: Float(0.5))],
                triggers: [(name: "on"), (name: "off")],
                assets: [(id: "noise", kind: Texture((path: "omagari://textures/noise.png", purpose: Noise)))],
                layers: [Distortion((
                    name: "haze",
                    strength: Param("heat", [(0.0, 0.0), (1.0, 0.01)]),
                    noise: (texture: (asset: "noise")),
                    noise_b: Some((texture: (asset: "noise"), scale: 6.0)),
                    play: Between(start: "on", stop: "off"),
                    visible: Some((param: "heat", from: 0.1, to: 1.0)),
                ))],
            )"#,
        )
        .unwrap();
        assert_eq!(p.asset_users("noise"), vec!["haze"]);
        assert!(p.delete_asset("noise").is_err(), "in use");
        assert_eq!(p.param_users("heat"), vec!["layer \"haze\" (strength, visible)".to_string()]);
        assert_eq!(p.trigger_users("on"), vec!["haze".to_string()]);
        assert_eq!(p.trigger_users("off"), vec!["haze".to_string()]);
        p.rename_param("heat", "temperature").unwrap();
        p.rename_trigger("on", "ignite").unwrap();
        p.rename_asset("noise", "haze_noise").unwrap();
        let haze = p.layers[0].as_distortion().unwrap();
        assert_eq!(haze.params_used(), vec!["temperature"]);
        assert_eq!(haze.play, PlayDef::Between { start: "ignite".into(), stop: "off".into() });
        assert_eq!(haze.noise.texture.asset, "haze_noise");
        assert_eq!(haze.noise_b.as_ref().unwrap().texture.asset, "haze_noise");
    }
}
