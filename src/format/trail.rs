//! Trail and afterimage layers (P5, F5b / F5c): what a swung weapon leaves
//! behind.
//!
//! - A **trail** is a ribbon between two moving points (sockets, e.g. a
//!   blade's base and tip) sampled every frame, kept for `length` seconds and
//!   smoothed with Catmull-Rom curves, drawn in world space with Omagari's
//!   effect shader. Its UVs run `u` = 0 at the newest sample → 1 at the
//!   oldest (so `fill` along `U` and UV noise follow the age) and `v` = 0 at
//!   `from` → 1 at `to`.
//! - An **afterimage** layer leaves fading copies ("ghosts") of the target's
//!   own meshes along their path: every `interval` seconds while it emits it
//!   freezes the meshes where they are (skinned meshes in their current
//!   pose) and fades each copy out over `lifetime`.
//!
//! Both start and stop emitting by [`EmitDef`]: always, between two triggers
//! (`swing_start` / `swing_end` fired by the game's attack state), or while a
//! parameter is high. When emission stops, what is there ages out.
//!
//! ```ron
//! Trail((
//!     name: "slash",
//!     from: "blade_base",
//!     to: "blade_tip",
//!     length: 0.25,
//!     emit: Trigger(start: "swing_start", stop: "swing_end"),
//!     fade_over_u: [(0.0, 1.0), (1.0, 0.0)],
//!     material: (blend: Add, color: Const((2.5, 2.0, 1.4, 1.0))),
//! ))
//! Afterimage((
//!     name: "ghosts",
//!     source: TargetModel,
//!     interval: 0.04,
//!     lifetime: 0.25,
//!     emit: Trigger(start: "swing_start", stop: "swing_end"),
//!     material: (blend: Add, color: Const((0.4, 0.8, 2.0, 0.6))),
//! ))
//! ```

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::project::{curve_value, Binding, PlayDef, VisibleRangeDef};
use super::surface::{EffectBlendDef, EffectMaterialDef, FadeDef};

/// Most samples a trail keeps (older ones are dropped first).
pub const MAX_TRAIL_SAMPLES: usize = 200;

/// Most points of a trail's ribbon after smoothing (two vertices per point,
/// twice for the back side); the subdivisions are lowered to stay below it.
pub const MAX_TRAIL_POINTS: usize = 256;

/// Most ghosts an afterimage layer keeps at once, whatever `max_ghosts` says.
pub const MAX_GHOSTS_LIMIT: u32 = 64;

/// When a trail or afterimage layer emits.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
pub enum EmitDef {
    /// While the effect runs.
    #[default]
    Always,
    /// From the trigger `start` until the trigger `stop` (e.g.
    /// `swing_start` / `swing_end` from the game's attack state). When they
    /// are the same trigger, it switches emission on and off.
    Trigger { start: String, stop: String },
    /// While the Float parameter is at or above the threshold.
    While(String, f32),
}

impl EmitDef {
    /// `"Always"`, `"Trigger"`, `"While"`.
    pub fn kind_name(&self) -> &'static str {
        match self {
            EmitDef::Always => "Always",
            EmitDef::Trigger { .. } => "Trigger",
            EmitDef::While(..) => "While",
        }
    }

    /// The triggers it listens to: `[start, stop]` (once when they are the
    /// same).
    pub fn triggers(&self) -> Vec<&str> {
        match self {
            EmitDef::Trigger { start, stop } if start == stop => vec![start.as_str()],
            EmitDef::Trigger { start, stop } => vec![start.as_str(), stop.as_str()],
            _ => Vec::new(),
        }
    }

    /// The parameter a `While` reads.
    pub fn param(&self) -> Option<&str> {
        match self {
            EmitDef::While(p, _) => Some(p),
            _ => None,
        }
    }

    pub fn rename_param(&mut self, from: &str, to: &str) {
        if let EmitDef::While(name, _) = self {
            if name == from {
                *name = to.to_string();
            }
        }
    }

    pub fn rename_trigger(&mut self, from: &str, to: &str) {
        if let EmitDef::Trigger { start, stop } = self {
            for name in [start, stop] {
                if name == from {
                    *name = to.to_string();
                }
            }
        }
    }

    /// What firing the trigger `name` does to emission that is `on`: the new
    /// state, `None` when the trigger is not this layer's.
    pub fn on_trigger(&self, name: &str, on: bool) -> Option<bool> {
        match self {
            EmitDef::Trigger { start, stop } if start == stop && start == name => Some(!on),
            EmitDef::Trigger { start, .. } if start == name => Some(true),
            EmitDef::Trigger { stop, .. } if stop == name => Some(false),
            _ => None,
        }
    }

    /// The same idea as a layer's `play` (for shared UI and checks).
    pub fn as_play(&self) -> PlayDef {
        match self {
            EmitDef::Always => PlayDef::Loop,
            EmitDef::Trigger { start, stop } => PlayDef::Between { start: start.clone(), stop: stop.clone() },
            EmitDef::While(p, t) => PlayDef::While(p.clone(), *t),
        }
    }
}

/// A curve of numbers over 0..1 (the trail's `u`, a ghost's life),
/// piecewise linear, flat outside its keys; 1 without keys.
pub fn unit_curve(keys: &[(f32, f32)], x: f32) -> f32 {
    curve_value(keys, x).unwrap_or(1.0)
}

fn trail_material() -> EffectMaterialDef {
    EffectMaterialDef {
        blend: EffectBlendDef::Add,
        color: Binding::Const(Vec4::new(2.0, 1.6, 1.1, 1.0)),
        ..Default::default()
    }
}

/// A ribbon between two moving sockets, kept for `length` seconds.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct TrailLayerDef {
    pub name: String,
    /// Socket (package socket or node name) of the ribbon's `v` = 0 edge,
    /// e.g. the blade's base.
    pub from: String,
    /// Socket of the `v` = 1 edge, e.g. the blade's tip.
    pub to: String,
    /// Seconds a sample stays: the trail's length in time.
    pub length: f32,
    /// Curve segments between two samples (Catmull-Rom), so fast swings stay
    /// round; 1 = straight lines between samples.
    pub subdivisions: u32,
    /// A new sample is taken once an end moved this far (metres) since the
    /// last one; the newest point always follows the sockets.
    pub min_sample_distance: f32,
    pub emit: EmitDef,
    /// Width along the trail: `(u, fraction of the from → to distance)`,
    /// shrinking toward `to` (u = 0 newest, 1 oldest). Empty = full width.
    pub width_over_u: Vec<(f32, f32)>,
    /// Opacity along the trail: `(u, factor)`. Empty = 1.
    pub fade_over_u: Vec<(f32, f32)>,
    /// The look (section "surfaces"); `Age` curves read the seconds since
    /// emission last started.
    pub material: EffectMaterialDef,
    /// Lifecycle fade: in when the effect starts, out when it stops (the
    /// trail also ages out after a stop).
    pub fade: FadeDef,
    pub visible: Option<VisibleRangeDef>,
    /// Added to the trail's draw-order distance (metres).
    pub sort_bias: f32,
}

impl Default for TrailLayerDef {
    fn default() -> Self {
        TrailLayerDef {
            name: "trail".to_string(),
            from: "blade_base".to_string(),
            to: "blade_tip".to_string(),
            length: 0.25,
            subdivisions: 4,
            min_sample_distance: 0.01,
            emit: EmitDef::Always,
            width_over_u: Vec::new(),
            fade_over_u: vec![(0.0, 1.0), (1.0, 0.0)],
            material: trail_material(),
            fade: FadeDef { fade_in: 0.0, fade_out: 0.0 },
            visible: None,
            sort_bias: 0.0,
        }
    }
}

impl TrailLayerDef {
    /// The two sockets: `[from, to]`.
    pub fn socket_names(&self) -> Vec<&str> {
        vec![self.from.as_str(), self.to.as_str()]
    }

    /// The width fraction at `u` (clamped to 0..).
    pub fn width_at(&self, u: f32) -> f32 {
        unit_curve(&self.width_over_u, u).max(0.0)
    }

    /// The opacity factor at `u` (0..1).
    pub fn fade_at(&self, u: f32) -> f32 {
        unit_curve(&self.fade_over_u, u).clamp(0.0, 1.0)
    }

    /// Every binding that reads a parameter (the material's).
    pub fn binding_params(&self) -> Vec<(&'static str, &str)> {
        self.material.binding_params()
    }

    /// Every parameter the layer reads (bindings, `visible`, `emit: While`),
    /// sorted, without duplicates.
    pub fn params_used(&self) -> Vec<String> {
        params_used(&self.material, self.visible.as_ref(), &self.emit)
    }

    pub fn rename_param(&mut self, from: &str, to: &str) {
        rename_param(&mut self.material, &mut self.visible, &mut self.emit, from, to);
    }

    /// Seconds the layer needs after a stop: the trail ages out over
    /// `length` (and the lifecycle fade).
    pub fn linger(&self) -> f32 {
        self.length.max(0.0).max(self.fade.fade_out.max(0.0))
    }
}

/// Which meshes an afterimage layer copies.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
pub enum GhostSourceDef {
    /// Every mesh of the target's own model (a weapon in the hand is a
    /// separate model: put the package on the weapon for weapon ghosts).
    #[default]
    TargetModel,
    /// Meshes by mesh name or mesh entity name (glTF names, as for surface
    /// layers).
    Meshes(Vec<String>),
}

impl GhostSourceDef {
    /// Whether a mesh entity (its `Name` and glTF mesh name) is copied.
    pub fn matches(&self, entity_name: Option<&str>, mesh_name: Option<&str>) -> bool {
        match self {
            GhostSourceDef::TargetModel => true,
            GhostSourceDef::Meshes(names) => names
                .iter()
                .any(|n| Some(n.as_str()) == entity_name || Some(n.as_str()) == mesh_name),
        }
    }
}

fn ghost_material() -> EffectMaterialDef {
    EffectMaterialDef {
        blend: EffectBlendDef::Add,
        color: Binding::Const(Vec4::new(0.4, 0.8, 2.0, 0.6)),
        ..Default::default()
    }
}

/// Fading copies of the target's meshes left along their path.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct AfterimageLayerDef {
    pub name: String,
    pub source: GhostSourceDef,
    /// Seconds between two copies while emitting.
    pub interval: f32,
    /// Seconds each copy stays (fading over `fade_over_life`).
    pub lifetime: f32,
    /// Most copies at once; the oldest goes first.
    pub max_ghosts: u32,
    pub emit: EmitDef,
    /// Opacity over a copy's life: `(fraction of the lifetime, factor)`.
    /// Empty = 1 until it goes.
    pub fade_over_life: Vec<(f32, f32)>,
    /// The look; `Age` curves read the copy's age in seconds.
    pub material: EffectMaterialDef,
    /// Lifecycle fade of every copy: in when the effect starts, out when it
    /// stops.
    pub fade: FadeDef,
    pub visible: Option<VisibleRangeDef>,
    pub sort_bias: f32,
}

impl Default for AfterimageLayerDef {
    fn default() -> Self {
        AfterimageLayerDef {
            name: "afterimages".to_string(),
            source: GhostSourceDef::TargetModel,
            interval: 0.05,
            lifetime: 0.3,
            max_ghosts: 8,
            emit: EmitDef::Always,
            fade_over_life: vec![(0.0, 1.0), (1.0, 0.0)],
            material: ghost_material(),
            fade: FadeDef { fade_in: 0.0, fade_out: 0.0 },
            visible: None,
            sort_bias: 0.0,
        }
    }
}

impl AfterimageLayerDef {
    /// The opacity factor of a copy `age` seconds old (0 once its lifetime
    /// is over).
    pub fn fade_at(&self, age: f32) -> f32 {
        if self.lifetime <= 0.0 || age >= self.lifetime {
            return 0.0;
        }
        unit_curve(&self.fade_over_life, (age / self.lifetime).clamp(0.0, 1.0)).clamp(0.0, 1.0)
    }

    /// The cap actually used (at least 1, at most [`MAX_GHOSTS_LIMIT`]).
    pub fn ghost_cap(&self) -> usize {
        self.max_ghosts.clamp(1, MAX_GHOSTS_LIMIT) as usize
    }

    /// Copies on screen while it emits: `lifetime / interval`, at most the
    /// cap.
    pub fn ghosts_on_screen(&self) -> usize {
        if self.lifetime <= 0.0 {
            return 0;
        }
        let made = if self.interval > 0.0 { (self.lifetime / self.interval).ceil() as usize } else { usize::MAX };
        made.clamp(1, self.ghost_cap())
    }

    pub fn binding_params(&self) -> Vec<(&'static str, &str)> {
        self.material.binding_params()
    }

    pub fn params_used(&self) -> Vec<String> {
        params_used(&self.material, self.visible.as_ref(), &self.emit)
    }

    pub fn rename_param(&mut self, from: &str, to: &str) {
        rename_param(&mut self.material, &mut self.visible, &mut self.emit, from, to);
    }

    /// Seconds the layer needs after a stop: the last copies fade out.
    pub fn linger(&self) -> f32 {
        self.lifetime.max(0.0).max(self.fade.fade_out.max(0.0))
    }
}

fn params_used(material: &EffectMaterialDef, visible: Option<&VisibleRangeDef>, emit: &EmitDef) -> Vec<String> {
    let mut names: Vec<String> = material.binding_params().into_iter().map(|(_, p)| p.to_string()).collect();
    if let Some(visible) = visible {
        names.push(visible.param.clone());
    }
    if let Some(p) = emit.param() {
        names.push(p.to_string());
    }
    names.sort();
    names.dedup();
    names
}

fn rename_param(material: &mut EffectMaterialDef, visible: &mut Option<VisibleRangeDef>, emit: &mut EmitDef, from: &str, to: &str) {
    material.rename_param(from, to);
    if let Some(visible) = visible {
        if visible.param == from {
            visible.param = to.to_string();
        }
    }
    emit.rename_param(from, to);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::{LayerDef, ProjectV2};

    #[test]
    fn trail_and_afterimage_layers_round_trip_with_defaults() {
        let project = ProjectV2::from_ron_str(
            r#"(version: 2, triggers: [(name: "swing_start"), (name: "swing_end")], layers: [
                Trail((name: "slash", length: 0.3, emit: Trigger(start: "swing_start", stop: "swing_end"),
                    width_over_u: [(0.0, 1.0), (1.0, 0.5)])),
                Trail((name: "t")),
                Afterimage((name: "ghosts", source: Meshes(["Sword"]), interval: 0.04, emit: While("speed", 0.5))),
                Afterimage((name: "a")),
            ])"#,
        )
        .unwrap();
        let slash = project.layers[0].as_trail().unwrap();
        assert_eq!(slash.emit, EmitDef::Trigger { start: "swing_start".into(), stop: "swing_end".into() });
        assert_eq!((slash.from.as_str(), slash.to.as_str(), slash.subdivisions), ("blade_base", "blade_tip", 4));
        assert!((slash.width_at(0.5) - 0.75).abs() < 1e-6);
        assert_eq!(slash.fade_at(0.0), 1.0);
        assert_eq!(slash.fade_at(1.0), 0.0);
        assert_eq!(project.layers[1].as_trail(), Some(&TrailLayerDef { name: "t".into(), ..Default::default() }));
        let ghosts = project.layers[2].as_afterimage().unwrap();
        assert_eq!(ghosts.source, GhostSourceDef::Meshes(vec!["Sword".into()]));
        assert_eq!(ghosts.emit.param(), Some("speed"));
        assert_eq!(project.layers[3].as_afterimage(), Some(&AfterimageLayerDef { name: "a".into(), ..Default::default() }));
        let text = project.to_ron_string().unwrap();
        let again = ProjectV2::from_ron_str(&text).unwrap();
        for i in 0..4 {
            assert_eq!(again.layers[i].as_trail(), project.layers[i].as_trail(), "{}", text);
            assert_eq!(again.layers[i].as_afterimage(), project.layers[i].as_afterimage(), "{}", text);
        }
        assert_eq!(project.layers[0].kind_name(), "trail");
        assert_eq!(project.layers[2].kind_name(), "afterimage");
        assert!(matches!(project.layers[0], LayerDef::Trail(_)));
        assert!(project.layers[0].material().is_some() && project.layers[2].material().is_some());
    }

    #[test]
    fn emission_switches_on_triggers() {
        let emit = EmitDef::Trigger { start: "a".into(), stop: "b".into() };
        assert_eq!(emit.on_trigger("a", false), Some(true));
        assert_eq!(emit.on_trigger("a", true), Some(true));
        assert_eq!(emit.on_trigger("b", true), Some(false));
        assert_eq!(emit.on_trigger("c", true), None);
        assert_eq!(emit.triggers(), vec!["a", "b"]);
        let toggle = EmitDef::Trigger { start: "t".into(), stop: "t".into() };
        assert_eq!(toggle.on_trigger("t", false), Some(true));
        assert_eq!(toggle.on_trigger("t", true), Some(false));
        assert_eq!(toggle.triggers(), vec!["t"]);
        assert_eq!(EmitDef::Always.on_trigger("a", false), None);
    }

    #[test]
    fn ghost_fades_and_counts() {
        let a = AfterimageLayerDef { interval: 0.05, lifetime: 0.3, max_ghosts: 8, ..Default::default() };
        assert_eq!(a.ghosts_on_screen(), 6);
        assert_eq!(a.fade_at(0.0), 1.0);
        assert!((a.fade_at(0.15) - 0.5).abs() < 1e-5);
        assert_eq!(a.fade_at(0.3), 0.0);
        let capped = AfterimageLayerDef { interval: 0.01, lifetime: 1.0, max_ghosts: 4, ..Default::default() };
        assert_eq!(capped.ghosts_on_screen(), 4);
        assert_eq!(AfterimageLayerDef { max_ghosts: 0, ..Default::default() }.ghost_cap(), 1);
        assert_eq!(a.linger(), 0.3);
        assert_eq!(TrailLayerDef::default().linger(), 0.25);
    }

    #[test]
    fn renames_reach_trails_and_afterimages() {
        let mut p = ProjectV2::from_ron_str(
            r#"(version: 2,
                parameters: [(name: "power", default: Float(0.5))],
                triggers: [(name: "go"), (name: "halt")],
                assets: [(id: "noise", kind: Texture((path: "omagari://textures/noise.png", purpose: Noise)))],
                layers: [
                    Trail((name: "trail", emit: Trigger(start: "go", stop: "halt"),
                        material: (opacity: Param("power", [(0.0, 0.0), (1.0, 1.0)]), noise_a: Some((texture: (asset: "noise")))))),
                    Afterimage((name: "ghosts", emit: While("power", 0.5), visible: Some((param: "power", from: 0.1, to: 1.0)))),
                ],
            )"#,
        )
        .unwrap();
        assert_eq!(p.asset_users("noise"), vec!["trail"]);
        assert_eq!(
            p.param_users("power"),
            vec!["layer \"trail\" (opacity)".to_string(), "layer \"ghosts\" (visible, emit While)".to_string()]
        );
        assert_eq!(p.trigger_users("go"), vec!["trail".to_string()]);
        assert_eq!(p.trigger_users("halt"), vec!["trail".to_string()]);
        p.rename_param("power", "strength").unwrap();
        p.rename_trigger("halt", "stop").unwrap();
        p.rename_asset("noise", "clouds").unwrap();
        let trail = p.layers[0].as_trail().unwrap();
        assert_eq!(trail.params_used(), vec!["strength"]);
        assert_eq!(trail.emit, EmitDef::Trigger { start: "go".into(), stop: "stop".into() });
        assert_eq!(trail.material.noise_a.as_ref().unwrap().texture.asset, "clouds");
        let ghosts = p.layers[1].as_afterimage().unwrap();
        assert_eq!(ghosts.params_used(), vec!["strength"]);
        assert_eq!(ghosts.emit, EmitDef::While("strength".into(), 0.5));
    }
}
