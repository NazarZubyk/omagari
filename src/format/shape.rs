//! Shape layers (P4, F4b): simple 3D shapes drawn with Omagari's effect
//! shader ([`EffectMaterialDef`]), attached to the target but not bent by
//! its skeleton: a shield bubble, a glowing column, a ground ring that stays
//! flat while the character runs, an expanding shockwave, a slash arc.
//!
//! The shape's own values are bindings like the material's: a constant, a
//! curve over a parameter, or a curve over the layer's **age** (seconds since
//! it started, or since its trigger last played it), so one-shot shapes grow
//! and fade without code.
//!
//! ```ron
//! Shape((
//!     name: "shockwave",
//!     attach: Target,
//!     follow: PositionYaw,
//!     shape: Ring(inner: 0.8, outer: 1.0),
//!     transform: (offset: (0.0, -0.85, 0.0)),
//!     material: (blend: Add, color: Const((0.4, 0.9, 2.0, 1.0)), opacity: Age([(0.0, 1.0), (0.6, 0.0)])),
//!     scale: Age([(0.0, (0.2, 0.2, 0.2)), (0.6, (3.0, 3.0, 3.0))]),
//!     play: OnTrigger("slam"),
//!     duration: Some(0.6),
//!     fade: (in: 0.0, out: 0.1),
//! ))
//! ```

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::project::{AttachDef, Binding, PlayDef, TimedPlay, VisibleRangeDef};
use super::surface::{EffectMaterialDef, FadeDef};

/// Shapes above this size (metres, largest extent at scale 1 × the largest
/// scale) are reported as huge by the check.
pub const HUGE_SHAPE: f32 = 20.0;

/// How a shape follows the point it is attached to.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum FollowDef {
    /// Position and rotation (the shape turns and tilts with the target or
    /// bone).
    #[default]
    Full,
    /// Position only: the shape keeps its own orientation in the world.
    Position,
    /// Position and the turn around the vertical axis (yaw) only: it faces
    /// where the target faces but never tilts (a ground ring stays flat
    /// while the body leans and animates).
    PositionYaw,
}

impl FollowDef {
    pub const ALL: [FollowDef; 3] = [FollowDef::Full, FollowDef::Position, FollowDef::PositionYaw];

    pub fn name(self) -> &'static str {
        match self {
            FollowDef::Full => "Full",
            FollowDef::Position => "Position",
            FollowDef::PositionYaw => "PositionYaw",
        }
    }
}

/// Which way a quad faces.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum QuadFacingDef {
    /// Flat in the shape's XZ plane (facing up at rotation 0), drawn from both
    /// sides.
    #[default]
    Fixed,
    /// Always faces the camera (a glow sprite); the shape's rotation is
    /// ignored, its scale is kept (X = width, Z = height on screen).
    Camera,
}

fn one() -> f32 {
    1.0
}
fn half() -> f32 {
    0.5
}
fn quad_size() -> Vec2 {
    Vec2::ONE
}
fn arc_angle() -> f32 {
    120.0
}
fn ring_inner() -> f32 {
    0.8
}
fn torus_minor() -> f32 {
    0.1
}
fn arc_width() -> f32 {
    0.3
}

/// The shape's mesh, in metres at scale 1, centred on the layer's origin
/// with Y up. UVs: `u` runs around (or along) the shape, `v` from bottom to
/// top (sphere, capsule, cylinder side, cone side), across (ring: inner →
/// outer edge; arc: inner → outer), around the tube (torus); quads map X → u,
/// Z → v.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub enum ShapeDef {
    Sphere {
        #[serde(default = "one")]
        radius: f32,
    },
    /// A capsule along Y: `length` = distance between the centres of the two
    /// end half-spheres (total height = length + 2 × radius).
    Capsule {
        #[serde(default = "half")]
        radius: f32,
        #[serde(default = "one")]
        length: f32,
    },
    /// A cylinder along Y; `open_ends` = a tube without caps (drawn inside
    /// and out).
    Cylinder {
        #[serde(default = "half")]
        radius: f32,
        #[serde(default = "one")]
        height: f32,
        #[serde(default)]
        open_ends: bool,
    },
    /// A flat ring in the XZ plane (facing up), drawn from both sides.
    Ring {
        #[serde(default = "ring_inner")]
        inner: f32,
        #[serde(default = "one")]
        outer: f32,
    },
    /// A ring-shaped tube around Y: `major` = radius of the ring, `minor` =
    /// radius of the tube.
    Torus {
        #[serde(default = "one")]
        major: f32,
        #[serde(default = "torus_minor")]
        minor: f32,
    },
    /// A cone along Y: base (radius) at the bottom, tip at the top.
    Cone {
        #[serde(default = "half")]
        radius: f32,
        #[serde(default = "one")]
        height: f32,
    },
    /// A rectangle `size` = (X, Z) metres, flat in XZ, or facing the camera.
    Quad {
        #[serde(default = "quad_size")]
        size: Vec2,
        #[serde(default)]
        facing: QuadFacingDef,
    },
    /// A flat slash arc in XZ, centred on the forward direction (-Z): the
    /// middle of the band at `radius`, `width` across, spanning `angle_deg`
    /// degrees. `taper` 0 = even width, 1 = pointed ends. UVs: `u` 0 → 1 from
    /// the left end (-X side) to the right end along the arc, `v` 0 → 1 from
    /// the inner to the outer edge.
    Arc {
        #[serde(default = "one")]
        radius: f32,
        #[serde(default = "arc_width")]
        width: f32,
        #[serde(default = "arc_angle")]
        angle_deg: f32,
        #[serde(default)]
        taper: f32,
    },
}

impl Default for ShapeDef {
    fn default() -> Self {
        ShapeDef::Sphere { radius: 1.0 }
    }
}

impl ShapeDef {
    /// Every kind with its default dimensions (editor menus).
    pub fn all_defaults() -> [ShapeDef; 8] {
        [
            ShapeDef::Sphere { radius: 1.0 },
            ShapeDef::Capsule { radius: 0.5, length: 1.0 },
            ShapeDef::Cylinder { radius: 0.5, height: 1.0, open_ends: false },
            ShapeDef::Ring { inner: 0.8, outer: 1.0 },
            ShapeDef::Torus { major: 1.0, minor: 0.1 },
            ShapeDef::Cone { radius: 0.5, height: 1.0 },
            ShapeDef::Quad { size: Vec2::ONE, facing: QuadFacingDef::Fixed },
            ShapeDef::Arc { radius: 1.0, width: 0.3, angle_deg: 120.0, taper: 0.0 },
        ]
    }

    /// `"Sphere"`, `"Ring"`, ...
    pub fn kind_name(&self) -> &'static str {
        match self {
            ShapeDef::Sphere { .. } => "Sphere",
            ShapeDef::Capsule { .. } => "Capsule",
            ShapeDef::Cylinder { .. } => "Cylinder",
            ShapeDef::Ring { .. } => "Ring",
            ShapeDef::Torus { .. } => "Torus",
            ShapeDef::Cone { .. } => "Cone",
            ShapeDef::Quad { .. } => "Quad",
            ShapeDef::Arc { .. } => "Arc",
        }
    }

    /// Whether the shape is flat (ring, quad, arc): drawn from both sides,
    /// and a `fill` along an axis needs U or V.
    pub fn is_flat(&self) -> bool {
        matches!(self, ShapeDef::Ring { .. } | ShapeDef::Quad { .. } | ShapeDef::Arc { .. })
    }

    /// Whether it is a quad facing the camera.
    pub fn faces_camera(&self) -> bool {
        matches!(self, ShapeDef::Quad { facing: QuadFacingDef::Camera, .. })
    }

    /// Dimensions that must be above 0: `(name, value)`.
    pub fn dimensions(&self) -> Vec<(&'static str, f32)> {
        match *self {
            ShapeDef::Sphere { radius } => vec![("radius", radius)],
            ShapeDef::Capsule { radius, length } => vec![("radius", radius), ("length", length)],
            ShapeDef::Cylinder { radius, height, .. } => vec![("radius", radius), ("height", height)],
            ShapeDef::Ring { inner, outer } => vec![("outer", outer), ("outer - inner", outer - inner)],
            ShapeDef::Torus { major, minor } => vec![("major", major), ("minor", minor)],
            ShapeDef::Cone { radius, height } => vec![("radius", radius), ("height", height)],
            ShapeDef::Quad { size, .. } => vec![("size x", size.x), ("size z", size.y)],
            ShapeDef::Arc { radius, width, angle_deg, .. } => {
                vec![("radius", radius), ("width", width), ("angle_deg", angle_deg)]
            }
        }
    }

    /// Local bounds `(min, max)` at scale 1 (what `fill` measures along an
    /// axis).
    pub fn bounds(&self) -> (Vec3, Vec3) {
        let sym = |x: f32, y: f32, z: f32| (Vec3::new(-x, -y, -z), Vec3::new(x, y, z));
        match *self {
            ShapeDef::Sphere { radius } => sym(radius, radius, radius),
            ShapeDef::Capsule { radius, length } => sym(radius, length * 0.5 + radius, radius),
            ShapeDef::Cylinder { radius, height, .. } => sym(radius, height * 0.5, radius),
            ShapeDef::Ring { outer, .. } => sym(outer, 0.0, outer),
            ShapeDef::Torus { major, minor } => sym(major + minor, minor, major + minor),
            ShapeDef::Cone { radius, height } => sym(radius, height * 0.5, radius),
            ShapeDef::Quad { size, .. } => sym(size.x * 0.5, 0.0, size.y * 0.5),
            ShapeDef::Arc { radius, width, angle_deg, taper } => {
                let mut lo = Vec3::splat(f32::INFINITY);
                let mut hi = Vec3::splat(f32::NEG_INFINITY);
                let steps = 256;
                for i in 0..=steps {
                    let u = i as f32 / steps as f32;
                    let half = arc_half_width(u, width, taper);
                    for r in [radius - half, radius + half] {
                        let p = arc_point(u, angle_deg) * r.max(0.0);
                        lo = lo.min(p);
                        hi = hi.max(p);
                    }
                }
                (lo, hi)
            }
        }
    }

    /// The largest extent (metres at scale 1).
    pub fn size(&self) -> f32 {
        let (lo, hi) = self.bounds();
        (hi - lo).max_element()
    }

    /// The dimensions in words: `sphere, radius 1 m`.
    pub fn describe(&self) -> String {
        let n = |x: f32| {
            let s = format!("{:.3}", x);
            let s = s.trim_end_matches('0').trim_end_matches('.').to_string();
            if s.is_empty() || s == "-0" { "0".to_string() } else { s }
        };
        match *self {
            ShapeDef::Sphere { radius } => format!("sphere, radius {} m", n(radius)),
            ShapeDef::Capsule { radius, length } => format!(
                "capsule along Y, radius {} m, {} m tall in all",
                n(radius),
                n(length + 2.0 * radius)
            ),
            ShapeDef::Cylinder { radius, height, open_ends } => format!(
                "{} along Y, radius {} m, {} m tall",
                if open_ends { "open tube (no caps, seen inside and out)" } else { "closed cylinder" },
                n(radius),
                n(height)
            ),
            ShapeDef::Ring { inner, outer } => {
                format!("flat ring in XZ (facing up), from {} m to {} m from the centre", n(inner), n(outer))
            }
            ShapeDef::Torus { major, minor } => format!("torus around Y, ring radius {} m, tube radius {} m", n(major), n(minor)),
            ShapeDef::Cone { radius, height } => format!("cone along Y, base radius {} m, {} m tall, tip up", n(radius), n(height)),
            ShapeDef::Quad { size, facing } => format!(
                "quad {} × {} m{}",
                n(size.x),
                n(size.y),
                match facing {
                    QuadFacingDef::Fixed => ", flat in XZ (facing up)",
                    QuadFacingDef::Camera => ", always facing the camera",
                }
            ),
            ShapeDef::Arc { radius, width, angle_deg, taper } => format!(
                "flat arc in XZ in front (-Z), radius {} m, {} m wide, over {}°{}",
                n(radius),
                n(width),
                n(angle_deg),
                if taper > 0.0 { format!(", ends tapered {}", n(taper)) } else { String::new() }
            ),
        }
    }
}

/// Half the width of an arc at `u` (0..1 along it): `width / 2`, pointed
/// at the ends by `taper` (0..1).
pub fn arc_half_width(u: f32, width: f32, taper: f32) -> f32 {
    let taper = taper.clamp(0.0, 1.0);
    width.max(0.0) * 0.5 * (1.0 - taper + taper * (std::f32::consts::PI * u).sin())
}

/// A unit direction on an arc spanning `angle_deg` degrees around -Z at
/// `u` (0 = the left end, -X side; 1 = the right end).
pub fn arc_point(u: f32, angle_deg: f32) -> Vec3 {
    let theta = (u - 0.5) * angle_deg.to_radians();
    Vec3::new(theta.sin(), 0.0, -theta.cos())
}

/// Where a shape sits relative to the point it is attached to: an offset
/// (metres, × the size fit; in the attach point's space with follow `Full`,
/// in world axes with `Position`, turned by the yaw with `PositionYaw`) and
/// a rotation (degrees about X, then Y, then Z).
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug, Default)]
#[serde(default)]
pub struct ShapeTransformDef {
    pub offset: Vec3,
    pub rotation_deg: Vec3,
}

impl ShapeTransformDef {
    pub fn rotation(&self) -> Quat {
        let r = self.rotation_deg;
        Quat::from_euler(EulerRot::XYZ, r.x.to_radians(), r.y.to_radians(), r.z.to_radians())
    }
}

/// A shape drawn with the effect material, attached to the target.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct ShapeLayerDef {
    pub name: String,
    pub attach: AttachDef,
    pub follow: FollowDef,
    pub shape: ShapeDef,
    pub transform: ShapeTransformDef,
    pub material: EffectMaterialDef,
    /// Scale (X, Y, Z) on top of the shape's size; animate it over `Age` for
    /// a growing shockwave.
    pub scale: Binding<Vec3>,
    /// Degrees per second around `rotation_axis` (in the shape's own space).
    pub rotation_speed: Binding<f32>,
    pub rotation_axis: Vec3,
    /// `Loop` (always), `OnStart` (once), `OnTrigger("name")` (each time the
    /// trigger fires; restarts the age), `While("param", threshold)`,
    /// `Between(start, stop)` (from one trigger to the other).
    pub play: PlayDef,
    /// One-shot length in seconds for `OnStart` / `OnTrigger` (the shape is
    /// gone after it); for `Loop` / `While` the age starts over every
    /// `duration` seconds.
    pub duration: Option<f32>,
    /// Fades: a one-shot fades in over `in` after it starts and out over
    /// `out` before its end; other shapes fade with the instance (and a
    /// `While` shape with its parameter). Every shape fades out over `out`
    /// when the instance stops.
    pub fade: FadeDef,
    pub visible: Option<VisibleRangeDef>,
    /// Added to the shape's distance in the transparent draw order (metres):
    /// positive = drawn later (over particles and overlays at a similar
    /// distance), negative = earlier.
    pub sort_bias: f32,
}

impl Default for ShapeLayerDef {
    fn default() -> Self {
        ShapeLayerDef {
            name: "shape".to_string(),
            attach: AttachDef::Target,
            follow: FollowDef::Full,
            shape: ShapeDef::default(),
            transform: ShapeTransformDef::default(),
            material: EffectMaterialDef::default(),
            scale: Binding::Const(Vec3::ONE),
            rotation_speed: Binding::Const(0.0),
            rotation_axis: Vec3::Y,
            play: PlayDef::Loop,
            duration: None,
            fade: FadeDef::default(),
            visible: None,
            sort_bias: 0.0,
        }
    }
}

impl TimedPlay for ShapeLayerDef {
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

impl ShapeLayerDef {
    /// Whether the shape plays once per start / trigger for `duration`
    /// seconds (`OnStart` / `OnTrigger` with a duration).
    pub fn is_one_shot(&self) -> bool {
        TimedPlay::is_one_shot(self)
    }

    /// The one-shot length (`duration`, at least 0), for one-shot shapes.
    pub fn one_shot_duration(&self) -> Option<f32> {
        TimedPlay::one_shot_duration(self)
    }

    /// The age bindings see: the age, wrapped by `duration` for `Loop` /
    /// `While` / `Between` shapes (the animation repeats).
    pub fn binding_age(&self, age: f32) -> f32 {
        TimedPlay::binding_age(self, age)
    }

    /// Opacity of a one-shot at `age`: up over `fade.in`, down over
    /// `fade.out` before `duration`, 0 after it. 1 for other shapes.
    pub fn envelope(&self, age: f32) -> f32 {
        TimedPlay::envelope(self, age)
    }

    /// Every binding that reads a parameter: material values, `scale`,
    /// `rotation_speed`.
    pub fn binding_params(&self) -> Vec<(&'static str, &str)> {
        let mut out = self.material.binding_params();
        if let Some(p) = self.scale.param() {
            out.push(("scale", p));
        }
        if let Some(p) = self.rotation_speed.param() {
            out.push(("rotation_speed", p));
        }
        out
    }

    /// Every parameter the layer reads (bindings, `visible`, `play: While`),
    /// sorted, without duplicates.
    pub fn params_used(&self) -> Vec<String> {
        let mut names: Vec<String> = self.binding_params().into_iter().map(|(_, p)| p.to_string()).collect();
        if let Some(visible) = &self.visible {
            names.push(visible.param.clone());
        }
        if let PlayDef::While(p, _) = &self.play {
            names.push(p.clone());
        }
        names.sort();
        names.dedup();
        names
    }

    /// Renames the parameter `from` in every binding, `visible` and `play:
    /// While`.
    pub fn rename_param(&mut self, from: &str, to: &str) {
        self.material.rename_param(from, to);
        if let Binding::Param(name, _) = &mut self.rotation_speed {
            if name == from {
                *name = to.to_string();
            }
        }
        if let Binding::Param(name, _) = &mut self.scale {
            if name == from {
                *name = to.to_string();
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
        let mut out = self.material.age_bindings();
        if let Binding::Age(keys) = &self.scale {
            out.push(("scale", keys.iter().map(|k| k.0).fold(f32::NEG_INFINITY, f32::max)));
        }
        if let Binding::Age(keys) = &self.rotation_speed {
            out.push(("rotation_speed", keys.iter().map(|k| k.0).fold(f32::NEG_INFINITY, f32::max)));
        }
        out
    }

    /// Seconds the layer needs after a stop (its fade out).
    pub fn linger(&self) -> f32 {
        self.fade.fade_out.max(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::ProjectV2;

    #[test]
    fn shape_layers_round_trip_with_defaults() {
        let project = ProjectV2::from_ron_str(
            r#"(version: 2, triggers: [(name: "slam")], layers: [
                Shape((name: "wave", shape: Ring(inner: 0.5), follow: PositionYaw, play: OnTrigger("slam"), duration: Some(0.6),
                    scale: Age([(0.0, (0.2, 0.2, 0.2)), (0.6, (3.0, 3.0, 3.0))]))),
                Shape((name: "s")),
                Shape((name: "arc", shape: Arc(angle_deg: 90.0, taper: 1.0), transform: (rotation_deg: (0.0, 0.0, 30.0)))),
            ])"#,
        )
        .unwrap();
        let wave = project.layers[0].as_shape().unwrap();
        assert_eq!(wave.shape, ShapeDef::Ring { inner: 0.5, outer: 1.0 });
        assert_eq!(wave.follow, FollowDef::PositionYaw);
        assert!(wave.is_one_shot());
        assert_eq!(wave.scale.evaluate_at(|_| None, 0.3), Some(Vec3::splat(1.6)));
        let s = project.layers[1].as_shape().unwrap();
        assert_eq!(s, &ShapeLayerDef { name: "s".into(), ..Default::default() });
        assert_eq!(s.linger(), 0.3);
        let arc = project.layers[2].as_shape().unwrap();
        assert_eq!(arc.shape, ShapeDef::Arc { radius: 1.0, width: 0.3, angle_deg: 90.0, taper: 1.0 });
        let text = project.to_ron_string().unwrap();
        let again = ProjectV2::from_ron_str(&text).unwrap();
        for i in 0..3 {
            assert_eq!(again.layers[i].as_shape(), project.layers[i].as_shape(), "{}", text);
        }
        assert_eq!(project.layers[0].kind_name(), "shape");
    }

    #[test]
    fn one_shot_envelopes_and_age_wrap() {
        let shape = ShapeLayerDef {
            play: PlayDef::OnStart,
            duration: Some(1.0),
            fade: FadeDef { fade_in: 0.2, fade_out: 0.5 },
            ..Default::default()
        };
        assert_eq!(shape.envelope(0.0), 0.0);
        assert!((shape.envelope(0.1) - 0.5).abs() < 1e-6);
        assert_eq!(shape.envelope(0.4), 1.0);
        assert!((shape.envelope(0.75) - 0.5).abs() < 1e-6);
        assert_eq!(shape.envelope(1.0), 0.0);
        assert_eq!(shape.binding_age(1.5), 1.5, "one-shots do not wrap");
        // A looping shape with a duration repeats its age.
        let looping = ShapeLayerDef { duration: Some(0.5), ..Default::default() };
        assert!(!looping.is_one_shot());
        assert_eq!(looping.envelope(3.0), 1.0);
        assert!((looping.binding_age(1.2) - 0.2).abs() < 1e-5);
        // OnTrigger without a duration is not a one-shot.
        let held = ShapeLayerDef { play: PlayDef::OnTrigger("t".into()), ..Default::default() };
        assert!(!held.is_one_shot() && held.one_shot_duration().is_none());
    }

    #[test]
    fn bounds_and_dimensions() {
        assert_eq!(ShapeDef::Capsule { radius: 0.5, length: 1.0 }.bounds().1, Vec3::new(0.5, 1.0, 0.5));
        assert_eq!(ShapeDef::Ring { inner: 0.2, outer: 1.0 }.bounds().1.y, 0.0, "flat");
        let (lo, hi) = ShapeDef::Arc { radius: 1.0, width: 0.2, angle_deg: 180.0, taper: 0.0 }.bounds();
        assert!((lo.x + 1.1).abs() < 1e-4 && (hi.x - 1.1).abs() < 1e-4 && (lo.z + 1.1).abs() < 1e-4 && hi.z.abs() < 1e-4, "{} {}", lo, hi);
        assert!(ShapeDef::Ring { inner: 1.0, outer: 0.5 }.dimensions().iter().any(|(_, v)| *v <= 0.0));
        assert_eq!(ShapeDef::Torus { major: 1.0, minor: 0.25 }.size(), 2.5);
        assert!(arc_point(0.5, 90.0).abs_diff_eq(Vec3::NEG_Z, 1e-6), "the middle points forward");
        assert!(arc_point(0.0, 180.0).abs_diff_eq(Vec3::NEG_X, 1e-6), "u = 0 is the left end");
    }

    #[test]
    fn renames_reach_shape_bindings() {
        let mut p = ProjectV2::from_ron_str(
            r#"(version: 2,
                parameters: [(name: "power", default: Float(0.5))],
                triggers: [(name: "hit")],
                assets: [(id: "noise", kind: Texture((path: "omagari://textures/noise.png", purpose: Noise)))],
                layers: [Shape((
                    name: "shield",
                    material: (opacity: Param("power", [(0.0, 0.0), (1.0, 1.0)]), noise_a: Some((texture: (asset: "noise")))),
                    scale: Param("power", [(0.0, (1.0, 1.0, 1.0)), (1.0, (2.0, 2.0, 2.0))]),
                    play: OnTrigger("hit"),
                    visible: Some((param: "power", from: 0.1, to: 1.0)),
                ))],
            )"#,
        )
        .unwrap();
        assert_eq!(p.asset_users("noise"), vec!["shield"]);
        assert_eq!(p.param_users("power"), vec!["layer \"shield\" (opacity, scale, visible)".to_string()]);
        assert_eq!(p.trigger_users("hit"), vec!["shield".to_string()]);
        p.rename_param("power", "strength").unwrap();
        p.rename_trigger("hit", "impact").unwrap();
        p.rename_asset("noise", "clouds").unwrap();
        let shield = p.layers[0].as_shape().unwrap();
        assert_eq!(shield.params_used(), vec!["strength"]);
        assert_eq!(shield.scale.param(), Some("strength"));
        assert_eq!(shield.play, PlayDef::OnTrigger("impact".into()));
        assert_eq!(shield.material.noise_a.as_ref().unwrap().texture.asset, "clouds");
    }
}
