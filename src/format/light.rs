//! Light layers (P6, F7): a point light at the attach point — a weapon glow
//! that lights the floor, a flash on a hit, a torch flicker.
//!
//! Colour, intensity and range are bindings like every other layer value: a
//! constant, a curve over a parameter, or a curve over the layer's age (a
//! flash that fades). Play works like a shape layer's ([`TimedPlay`]): `Loop`,
//! `OnStart`, `OnTrigger` (one-shot with a `duration`), `While`, `Between`.
//!
//! Units are Bevy's: `intensity` is the luminous power of a `PointLight` in
//! **lumens** (a 60 W bulb ≈ 800 lm; an effect light seen from a game camera
//! is usually 2 000–100 000 lm), `range` and `radius` are metres.
//!
//! ```ron
//! Light((
//!     name: "glow",
//!     attach: Socket("blade_mid"),
//!     color: Const((1.0, 0.55, 0.25, 1.0)),
//!     intensity: Param("charge", [(0.0, 0.0), (1.0, 40000.0)]),
//!     range: Const(4.0),
//!     flicker: (amount: 0.15, speed: 8.0),
//! ))
//! ```

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::project::{AttachDef, Binding, ParamValue, PlayDef, TimedPlay, VisibleRangeDef};
use super::surface::{evaluate_color_at, number_value, FadeDef};

/// Random brightness changes of a light ("flicker"): smooth noise on the
/// intensity, different for every instance.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
#[serde(default)]
pub struct LightFlickerDef {
    /// How much the intensity moves, as a fraction of it: 0 = steady, 0.2 =
    /// between 80% and 120%, 1 = between dark and twice as bright.
    pub amount: f32,
    /// How fast it changes: about this many new brightness values per second
    /// (a candle 4–8, an electric crackle 15–25).
    pub speed: f32,
}

impl Default for LightFlickerDef {
    fn default() -> Self {
        LightFlickerDef { amount: 0.0, speed: 6.0 }
    }
}

/// A hashed value in -1..1 for an integer lattice point and a seed.
fn lattice(i: i64, seed: u32) -> f32 {
    let mut h = (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (seed as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    h ^= h >> 31;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 29;
    ((h >> 40) as f32 / (1u64 << 24) as f32) * 2.0 - 1.0
}

/// Smooth 1D value noise in -1..1 (`x` in lattice units).
fn value_noise(x: f32, seed: u32) -> f32 {
    let i = x.floor();
    let f = x - i;
    let u = f * f * (3.0 - 2.0 * f);
    let a = lattice(i as i64, seed);
    let b = lattice(i as i64 + 1, seed);
    a + (b - a) * u
}

/// Smooth noise in -1..1 over time: two octaves of value noise, `x` =
/// seconds × speed. The same `(x, seed)` always gives the same value.
pub fn flicker_noise(x: f32, seed: u32) -> f32 {
    (0.65 * value_noise(x, seed) + 0.35 * value_noise(x * 2.3 + 17.0, seed ^ 0x5bd1_e995)).clamp(-1.0, 1.0)
}

impl LightFlickerDef {
    /// Whether it changes the intensity at all.
    pub fn is_on(&self) -> bool {
        self.amount > 0.0 && self.speed > 0.0
    }

    /// The intensity factor at `time` seconds for an instance's `seed`:
    /// `1 ± amount`, never below 0.
    pub fn factor(&self, time: f32, seed: u32) -> f32 {
        if !self.is_on() {
            return 1.0;
        }
        (1.0 + self.amount * flicker_noise(time * self.speed, seed)).max(0.0)
    }

    /// The smallest and largest factor.
    pub fn bounds(&self) -> (f32, f32) {
        if !self.is_on() {
            return (1.0, 1.0);
        }
        ((1.0 - self.amount).max(0.0), 1.0 + self.amount)
    }
}

/// A light layer's bound values at one moment (before fades, flicker and
/// the size fit).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct LightValues {
    /// Linear RGB (the colour's alpha is not used).
    pub color: Vec3,
    /// Lumens.
    pub intensity: f32,
    /// Metres.
    pub range: f32,
}

/// A point light at the layer's attach point.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct LightLayerDef {
    pub name: String,
    /// `Target` (the target root), `Socket("name")` (follows it), or
    /// `Segment(from, to)` (the middle of the two sockets).
    pub attach: AttachDef,
    /// Metres from the attach point, in its space (× the size fit).
    pub offset: Vec3,
    /// Linear RGB(A) colour; alpha is not used. A colour parameter can be
    /// bound directly: `Param("tint", [])`. Keep the largest channel at 1:
    /// the brightness comes from `intensity`.
    pub color: Binding<Vec4>,
    /// Luminous power in lumens (Bevy's `PointLight::intensity`).
    pub intensity: Binding<f32>,
    /// Metres; nothing farther is lit. Light fades smoothly to 0 there.
    pub range: Binding<f32>,
    /// Size of the light's source sphere (metres): softer highlights on
    /// shiny surfaces. It does not change how far it lights.
    pub radius: f32,
    pub flicker: LightFlickerDef,
    /// `Loop`, `OnStart`, `OnTrigger("name")` (a flash with a `duration`),
    /// `While("param", threshold)`, `Between(start: "a", stop: "b")`.
    pub play: PlayDef,
    /// One-shot length in seconds for `OnStart` / `OnTrigger`; for the other
    /// modes the age repeats every `duration` seconds.
    pub duration: Option<f32>,
    /// One-shots fade in / out inside their duration; other lights fade in
    /// when they start and out when they stop (and when the effect stops).
    pub fade: FadeDef,
    pub visible: Option<VisibleRangeDef>,
    /// Cast shadows (costly: six shadow maps per light; off by default).
    pub shadows: bool,
}

impl Default for LightLayerDef {
    fn default() -> Self {
        LightLayerDef {
            name: "light".to_string(),
            attach: AttachDef::Target,
            offset: Vec3::ZERO,
            color: Binding::Const(Vec4::new(1.0, 0.7, 0.4, 1.0)),
            intensity: Binding::Const(20000.0),
            range: Binding::Const(4.0),
            radius: 0.05,
            flicker: LightFlickerDef::default(),
            play: PlayDef::Loop,
            duration: None,
            fade: FadeDef::default(),
            visible: None,
            shadows: false,
        }
    }
}

impl TimedPlay for LightLayerDef {
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

fn first_key<T: Copy>(b: &Binding<T>) -> Option<T> {
    match b {
        Binding::Const(v) => Some(*v),
        Binding::Param(_, keys) | Binding::Age(keys) => keys.first().map(|k| k.1),
    }
}

impl LightLayerDef {
    /// Whether colour values may read a Color parameter (`"color"`).
    pub fn is_color_binding(what: &str) -> bool {
        what == "color"
    }

    /// Every binding that reads a parameter: `(value name, parameter)`.
    pub fn binding_params(&self) -> Vec<(&'static str, &str)> {
        [("color", self.color.param()), ("intensity", self.intensity.param()), ("range", self.range.param())]
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
        if let Binding::Param(name, _) = &mut self.color {
            if name == from {
                *name = to.to_string();
            }
        }
        for b in [&mut self.intensity, &mut self.range] {
            if let Binding::Param(name, _) = b {
                if name == from {
                    *name = to.to_string();
                }
            }
        }
        if let Some(visible) = &mut self.visible {
            if visible.param == from {
                visible.param = to.to_string();
            }
        }
        self.play.rename_param(from, to);
    }

    /// The age keys of every `Age` binding: `(value name, largest key age)`.
    pub fn age_bindings(&self) -> Vec<(&'static str, f32)> {
        let last = |keys: &[(f32, f32)]| keys.iter().map(|k| k.0).fold(f32::NEG_INFINITY, f32::max);
        let mut out = Vec::new();
        if let Binding::Age(keys) = &self.color {
            out.push(("color", keys.iter().map(|k| k.0).fold(f32::NEG_INFINITY, f32::max)));
        }
        if let Binding::Age(keys) = &self.intensity {
            out.push(("intensity", last(keys)));
        }
        if let Binding::Age(keys) = &self.range {
            out.push(("range", last(keys)));
        }
        out
    }

    /// The bound values for parameter values from `lookup` at the binding
    /// age `age` (unknown parameters give a curve's first key).
    pub fn values_at(&self, lookup: &dyn Fn(&str) -> Option<ParamValue>, age: f32) -> LightValues {
        let color = evaluate_color_at(&self.color, lookup, age)
            .or_else(|| first_key(&self.color))
            .unwrap_or(Vec4::ONE);
        LightValues {
            color: color.truncate().max(Vec3::ZERO),
            intensity: number_value(&self.intensity, lookup, age, 0.0).max(0.0),
            range: number_value(&self.range, lookup, age, 0.0).max(0.0),
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
    fn light_layers_round_trip_with_defaults() {
        let project = ProjectV2::from_ron_str(
            r#"(version: 2,
                parameters: [(name: "charge", default: Float(0.0))],
                triggers: [(name: "hit"), (name: "swing_start"), (name: "swing_end")],
                layers: [
                    Light((name: "glow", attach: Socket("hand"), intensity: Param("charge", [(0.0, 0.0), (1.0, 40000.0)]),
                        flicker: (amount: 0.2), shadows: true)),
                    Light((name: "l")),
                    Light((name: "flash", play: OnTrigger("hit"), duration: Some(0.3), intensity: Age([(0.0, 80000.0), (0.3, 0.0)]))),
                    Light((name: "held", play: Between(start: "swing_start", stop: "swing_end"))),
                ])"#,
        )
        .unwrap();
        let glow = project.layers[0].as_light().unwrap();
        assert_eq!(glow.attach, AttachDef::Socket("hand".into()));
        assert_eq!(glow.flicker, LightFlickerDef { amount: 0.2, speed: 6.0 });
        assert!(glow.shadows);
        assert_eq!(project.layers[1].as_light(), Some(&LightLayerDef { name: "l".into(), ..Default::default() }));
        assert!(!LightLayerDef::default().shadows, "shadows are off by default");
        let flash = project.layers[2].as_light().unwrap();
        assert!(flash.is_one_shot());
        assert_eq!(flash.age_bindings(), vec![("intensity", 0.3)]);
        let held = project.layers[3].as_light().unwrap();
        assert_eq!(held.play, PlayDef::Between { start: "swing_start".into(), stop: "swing_end".into() });
        let text = project.to_ron_string().unwrap();
        assert!(text.contains("Between(\n"), "{}", text);
        let again = ProjectV2::from_ron_str(&text).unwrap();
        for i in 0..4 {
            assert_eq!(again.layers[i].as_light(), project.layers[i].as_light(), "{}", text);
        }
        assert_eq!(project.layers[0].kind_name(), "light");
        assert!(matches!(project.layers[0], LayerDef::Light(_)));
        assert_eq!(project.layers[0].socket_names(), vec!["hand"]);
        assert!(project.layers[0].material().is_none());
    }

    #[test]
    fn values_follow_parameters_and_age() {
        let light = LightLayerDef {
            color: Binding::Param("tint".into(), vec![]),
            intensity: Binding::Param("charge".into(), vec![(0.0, 0.0), (1.0, 40000.0)]),
            range: Binding::Age(vec![(0.0, 2.0), (1.0, 4.0)]),
            ..Default::default()
        };
        let lookup = |name: &str| match name {
            "charge" => Some(ParamValue::Float(0.25)),
            "tint" => Some(ParamValue::Color(Vec4::new(0.2, 0.4, 1.0, 0.5))),
            _ => None,
        };
        let v = light.values_at(&lookup, 0.5);
        assert_eq!(v.color, Vec3::new(0.2, 0.4, 1.0));
        assert_eq!(v.intensity, 10000.0);
        assert_eq!(v.range, 3.0);
        // Unknown parameters: the first key.
        let v = light.values_at(&|_| None, 0.0);
        assert_eq!(v.intensity, 0.0);
        assert_eq!(v.color, Vec3::ONE);
        // Never negative.
        let neg = LightLayerDef { intensity: Binding::Const(-5.0), ..Default::default() };
        assert_eq!(neg.values_at(&|_| None, 0.0).intensity, 0.0);
    }

    #[test]
    fn flicker_is_deterministic_smooth_and_bounded() {
        let f = LightFlickerDef { amount: 0.3, speed: 8.0 };
        let (lo, hi) = f.bounds();
        assert!((lo - 0.7).abs() < 1e-6 && (hi - 1.3).abs() < 1e-6);
        let mut min = f32::MAX;
        let mut max = f32::MIN;
        let mut previous = f.factor(0.0, 7);
        for i in 0..20000 {
            let t = i as f32 * 0.001;
            let v = f.factor(t, 7);
            assert_eq!(v, f.factor(t, 7), "the same time and seed give the same value");
            assert!(v >= lo - 1e-5 && v <= hi + 1e-5, "{} at {}", v, t);
            assert!((v - previous).abs() < 0.05, "smooth: {} -> {} at {}", previous, v, t);
            previous = v;
            min = min.min(v);
            max = max.max(v);
        }
        assert!(max - min > 0.3, "it actually moves: {}..{}", min, max);
        assert_ne!(f.factor(1.234, 7), f.factor(1.234, 8), "other seeds differ");
        assert_eq!(LightFlickerDef::default().factor(3.0, 1), 1.0, "off by default");
        let full = LightFlickerDef { amount: 2.0, speed: 5.0 };
        assert!((0..1000).all(|i| full.factor(i as f32 * 0.01, 3) >= 0.0), "never below 0");
    }

    #[test]
    fn renames_and_users_reach_lights() {
        let mut p = ProjectV2::from_ron_str(
            r#"(version: 2,
                parameters: [(name: "power", default: Float(0.5)), (name: "tint", default: Color((1.0, 1.0, 1.0, 1.0)))],
                triggers: [(name: "hit"), (name: "go"), (name: "halt")],
                layers: [
                    Light((name: "flash", play: OnTrigger("hit"), color: Param("tint", []),
                        intensity: Param("power", [(0.0, 0.0), (1.0, 1000.0)]), visible: Some((param: "power", from: 0.1, to: 1.0)))),
                    Light((name: "held", play: Between(start: "go", stop: "halt"), range: Param("power", [(0.0, 1.0), (1.0, 3.0)]))),
                    Light((name: "while", play: While("power", 0.5))),
                ],
            )"#,
        )
        .unwrap();
        assert_eq!(
            p.param_users("power"),
            vec![
                "layer \"flash\" (intensity, visible)".to_string(),
                "layer \"held\" (range)".to_string(),
                "layer \"while\" (play While)".to_string()
            ]
        );
        assert_eq!(p.param_users("tint"), vec!["layer \"flash\" (color)".to_string()]);
        assert_eq!(p.trigger_users("hit"), vec!["flash".to_string()]);
        assert_eq!(p.trigger_users("halt"), vec!["held".to_string()]);
        p.rename_param("power", "strength").unwrap();
        p.rename_trigger("hit", "impact").unwrap();
        p.rename_trigger("halt", "stop").unwrap();
        let flash = p.layers[0].as_light().unwrap();
        assert_eq!(flash.params_used(), vec!["strength", "tint"]);
        assert_eq!(flash.play, PlayDef::OnTrigger("impact".into()));
        assert_eq!(p.layers[1].as_light().unwrap().play, PlayDef::Between { start: "go".into(), stop: "stop".into() });
        assert_eq!(p.layers[2].as_light().unwrap().play, PlayDef::While("strength".into(), 0.5));
    }

    #[test]
    fn between_switches_on_its_triggers() {
        let play = PlayDef::Between { start: "a".into(), stop: "b".into() };
        assert_eq!(play.on_between("a", false), Some(true));
        assert_eq!(play.on_between("b", true), Some(false));
        assert_eq!(play.on_between("c", true), None);
        assert_eq!(play.triggers(), vec!["a", "b"]);
        assert!(play.is_driven());
        let toggle = PlayDef::Between { start: "t".into(), stop: "t".into() };
        assert_eq!(toggle.on_between("t", true), Some(false));
        assert_eq!(toggle.triggers(), vec!["t"]);
        assert_eq!(PlayDef::OnTrigger("x".into()).on_between("x", false), None);
        assert_eq!(PlayDef::OnTrigger("x".into()).triggers(), vec!["x"]);
    }
}
