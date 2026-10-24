//! Light layers at runtime: one `PointLight` entity per light layer and
//! instance, a child of the instance.
//!
//! - **Values** ([`update_lights`], `Update`): play state ([`ShapePlayer`],
//!   shared with shapes: `Loop`, `OnStart`, `OnTrigger` one-shots, `While`,
//!   `Between`), fades, `visible`, and the colour / intensity / range
//!   bindings at the layer's age; flicker (smooth noise, a seed per
//!   instance and layer) on the intensity; the size fit scales range and
//!   radius by `fit` and the intensity by `fit²` (a scaled-up copy lights its
//!   surroundings the same way).
//! - **Global cap** ([`apply_light_cap`]): at most [`VfxSettings::max_lights`]
//!   Omagari lights shine at once. The strongest win (ties: the newest); a
//!   light that shines keeps a small bonus so two similar lights do not swap
//!   every frame. A light that loses its place fades out over
//!   [`VfxSettings::light_cap_fade`] seconds (and back in when it gets one);
//!   a light that just turned on and has no place stays dark at once.
//! - **Placement** ([`place_lights`], `PostUpdate` after transform
//!   propagation): the attach point (target root, socket, the middle of a
//!   segment, or an `OnTrigger` light's payload position) + `offset`, written
//!   straight into the `GlobalTransform` (no frame lag).

use bevy::light::PointLight;
use bevy::prelude::*;

use super::attach::{SocketAttach, SocketResolution, TargetHierarchy};
use super::instance::{VfxInstance, VfxState};
use super::package::{PreparedLightLayer, VfxPackage};
use super::shape::ShapePlayer;
use super::triggers::TriggerArgs;
use crate::format::{AttachDef, LightLayerDef, ParamValue, PlayDef, ProjectV2};

/// Settings of the Omagari runtime that a game may change (insert the
/// resource before or after adding [`crate::OmagariPlugin`], or change it at
/// any time).
///
/// ```ignore
/// app.insert_resource(omagari::VfxSettings { max_lights: 4, ..default() });
/// ```
#[derive(Resource, Clone, Copy, PartialEq, Debug)]
pub struct VfxSettings {
    /// Most Omagari lights (light layers of every instance together) that
    /// shine at once; the weakest (then the oldest) are switched off. Each
    /// point light costs shading time on every lit pixel near it (and six
    /// shadow maps with `shadows: true`).
    pub max_lights: usize,
    /// Seconds a light switched off by the cap takes to fade out (and to fade
    /// back in when it gets a place again). 0 = at once.
    pub light_cap_fade: f32,
}

impl Default for VfxSettings {
    fn default() -> Self {
        VfxSettings { max_lights: 8, light_cap_fade: 0.25 }
    }
}

/// A light layer entity of an instance (a child of the instance).
#[derive(Component, Clone, Copy, Debug)]
pub struct VfxLightLayer {
    pub instance: Entity,
    /// Index into [`VfxPackage::light_layers`].
    pub index: usize,
}

/// Runtime state of a light layer.
#[derive(Component, Clone, Debug, Default)]
pub struct LightLayerState {
    pub player: ShapePlayer,
    /// `visible` factor (1 without `visible`).
    pub visibility: f32,
    /// Sockets of a `Socket` / `Segment` attach.
    pub attach: Option<SocketAttach>,
    /// World position of the last trigger's payload: an `OnTrigger` light
    /// shines there instead of at its attach point.
    pub trigger_position: Option<Vec3>,
    /// Linear RGB.
    pub color: Vec3,
    /// Lumens the layer asks for this frame (fades, `visible`, flicker and
    /// the size fit applied), before the global cap.
    pub intensity: f32,
    /// [`LightLayerState::intensity`] without the flicker: what the cap
    /// ranks.
    pub strength: f32,
    /// Metres (× the size fit).
    pub range: f32,
    /// Metres (× the size fit).
    pub radius: f32,
    pub shadows: bool,
    /// The global cap's level 0..1 (1 = not limited).
    pub cap_level: f32,
    /// Order of the layer's first frame (larger = newer), for the cap.
    pub serial: u64,
    /// Flicker seed (per instance and layer).
    pub seed: u32,
    /// Seconds since the layer was spawned (the flicker's clock).
    pub clock: f32,
    was_wanted: bool,
    /// A trigger waiting for the next update.
    pub(crate) pending_trigger: Option<TriggerArgs>,
    /// `Between` triggers waiting for the next update.
    pub(crate) pending_switches: Vec<String>,
    generation: u32,
    started: bool,
    start_level: f32,
}

impl LightLayerState {
    /// A state whose fade starts at `level` (a rebuilt looping layer keeps
    /// the level of the one it replaces).
    pub fn with_level(level: f32) -> Self {
        LightLayerState { visibility: 1.0, cap_level: 1.0, start_level: level, ..default() }
    }

    /// Lumens the light shines with after the cap.
    pub fn shining(&self) -> f32 {
        self.intensity * self.cap_level
    }
}

/// One light competing for a place under the cap.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct LightCandidate {
    /// Lumens it asks for (0 = dark, needs no place).
    pub strength: f32,
    /// Larger = newer.
    pub serial: u64,
    /// It shines now (keeps a small bonus against similar lights).
    pub shown: bool,
}

/// The bonus a shining light gets in the ranking (no swapping between two
/// similar lights every frame).
pub const LIGHT_CAP_HYSTERESIS: f32 = 1.25;

/// Which lights may shine: the `max` strongest of those that ask for light
/// (ties: the newest first; shining lights rank × [`LIGHT_CAP_HYSTERESIS`]).
pub fn light_cap_allowed(candidates: &[LightCandidate], max: usize) -> Vec<bool> {
    let mut order: Vec<usize> = (0..candidates.len()).filter(|&i| candidates[i].strength > 0.0).collect();
    let score = |c: &LightCandidate| c.strength * if c.shown { LIGHT_CAP_HYSTERESIS } else { 1.0 };
    order.sort_by(|&a, &b| {
        let (ca, cb) = (&candidates[a], &candidates[b]);
        score(cb).total_cmp(&score(ca)).then(cb.serial.cmp(&ca.serial)).then(a.cmp(&b))
    });
    let mut allowed = vec![false; candidates.len()];
    for &i in order.iter().take(max) {
        allowed[i] = true;
    }
    allowed
}

/// One step of a light's cap level: a light that just asked for light is
/// on or off at once (by `allowed`); one that kept asking fades toward on
/// or off over `fade` seconds; one that asks for nothing resets to 1.
pub fn step_cap_level(level: f32, was_wanted: bool, wanted: bool, allowed: bool, dt: f32, fade: f32) -> f32 {
    if !wanted {
        return 1.0;
    }
    if !was_wanted || fade <= 0.0 {
        return if allowed { 1.0 } else { 0.0 };
    }
    let step = dt / fade;
    if allowed {
        (level + step).min(1.0)
    } else {
        (level - step).max(0.0)
    }
}

/// A hash of the instance and layer: the flicker seed.
fn seed_of(instance: Entity, index: usize) -> u32 {
    let mut h = instance.to_bits() ^ ((index as u64) << 48) ^ 0x2545_F491_4F6C_DD1D;
    h ^= h >> 33;
    h = h.wrapping_mul(0xFF51_AFD7_ED55_8CCD);
    h ^= h >> 33;
    h as u32
}

fn fit_of(instance: &GlobalTransform) -> f32 {
    let s = instance.scale().abs();
    ((s.x + s.y + s.z) / 3.0).max(1e-4)
}

/// Spawns the entity of one light layer of an instance.
pub(crate) fn spawn_light_layer(
    commands: &mut Commands,
    instance: Entity,
    index: usize,
    prepared: &PreparedLightLayer,
    project: &ProjectV2,
    level: f32,
) -> Entity {
    let mut state = LightLayerState::with_level(level);
    if prepared.def.attach != AttachDef::Target {
        state.attach = Some(SocketAttach::new(&prepared.def.attach, project));
    }
    commands
        .spawn((
            Name::new(prepared.def.name.clone()),
            VfxLightLayer { instance, index },
            state,
            PointLight { intensity: 0.0, range: 0.0, radius: 0.0, shadow_maps_enabled: false, ..default() },
            Transform::default(),
            Visibility::Hidden,
            ChildOf(instance),
        ))
        .id()
}

/// Plays light layers: sockets, triggers, age, fades, visibility, the bound
/// values and the flicker.
#[allow(clippy::too_many_arguments)]
pub(crate) fn update_lights(
    time: Res<Time>,
    packages: Res<Assets<VfxPackage>>,
    hierarchy: TargetHierarchy,
    changed: Query<(), (Or<(Changed<Children>, Changed<Name>)>, Without<VfxLightLayer>)>,
    instances: Query<(&VfxInstance, &GlobalTransform)>,
    mut layers: Query<(&VfxLightLayer, &mut LightLayerState)>,
    mut serial: Local<u64>,
) {
    let dt = time.delta_secs();
    let any_change = !changed.is_empty();
    for (layer, mut state) in layers.iter_mut() {
        let state = &mut *state;
        let Ok((instance, instance_global)) = instances.get(layer.instance) else { continue };
        let Some(prepared) = packages.get(&instance.package).and_then(|p| p.light_layers.get(layer.index)) else {
            continue;
        };
        if !instance.params.is_synced(instance.package.id()) {
            continue;
        }
        let def = &prepared.def;

        if !state.started || state.generation != instance.generation {
            state.player = ShapePlayer::new(def, state.start_level);
            state.started = true;
            state.generation = instance.generation;
            state.trigger_position = None;
            state.seed = seed_of(layer.instance, layer.index);
        }
        if state.serial == 0 {
            *serial += 1;
            state.serial = *serial;
        }
        if let Some(attach) = &mut state.attach {
            attach.update(&hierarchy, instance.target(), any_change, dt, def.name.as_str());
        }

        let running = matches!(instance.state(), VfxState::Running | VfxState::Starting);
        if let Some(args) = state.pending_trigger.take() {
            if running {
                state.player.trigger();
                state.trigger_position = args.position;
            }
        }
        for name in std::mem::take(&mut state.pending_switches) {
            if running {
                state.player.switch(def, &name);
            }
        }
        let while_value = def.play.param().and_then(|p| instance.params.float(p));
        state.player.step(def, dt, running, while_value);
        state.visibility = match &def.visible {
            Some(visible) => instance.params.float(&visible.param).map(|v| visible.factor(v)).unwrap_or(1.0),
            None => 1.0,
        };
        state.clock += dt;

        let age = state.player.binding_age(def);
        let lookup = |name: &str| -> Option<ParamValue> { instance.params.get(name) };
        let values = def.values_at(&lookup, age);
        let fit = fit_of(instance_global);
        let level = state.player.opacity(def) * state.visibility;
        state.color = values.color;
        state.strength = values.intensity * level * fit * fit;
        state.intensity = state.strength * def.flicker.factor(state.clock, state.seed);
        state.range = values.range * fit;
        state.radius = def.radius.max(0.0) * fit;
        state.shadows = def.shadows;
    }
}

/// The global light cap ([`VfxSettings::max_lights`]), then the
/// `PointLight` components (written only when they change) and visibility.
pub(crate) fn apply_light_cap(
    time: Res<Time>,
    settings: Res<VfxSettings>,
    mut layers: Query<(&mut LightLayerState, &mut PointLight, &mut Visibility), With<VfxLightLayer>>,
) {
    let dt = time.delta_secs();
    let candidates: Vec<LightCandidate> = layers
        .iter()
        .map(|(state, _, _)| LightCandidate {
            strength: state.strength,
            serial: state.serial,
            shown: state.was_wanted && state.cap_level > 0.0,
        })
        .collect();
    let allowed = light_cap_allowed(&candidates, settings.max_lights);
    for ((mut state, mut light, mut visibility), allowed) in layers.iter_mut().zip(allowed) {
        let wanted = state.strength > 0.0;
        state.cap_level = step_cap_level(state.cap_level, state.was_wanted, wanted, allowed, dt, settings.light_cap_fade);
        state.was_wanted = wanted;
        let intensity = state.shining();
        let color = Color::linear_rgb(state.color.x, state.color.y, state.color.z);
        if light.intensity != intensity
            || light.color != color
            || light.range != state.range
            || light.radius != state.radius
            || light.shadow_maps_enabled != state.shadows
        {
            light.intensity = intensity;
            light.color = color;
            light.range = state.range;
            light.radius = state.radius;
            light.shadow_maps_enabled = state.shadows;
        }
        let wanted_visibility = if intensity > 0.0 && state.range > 0.0 { Visibility::Inherited } else { Visibility::Hidden };
        if *visibility != wanted_visibility {
            *visibility = wanted_visibility;
        }
    }
}

/// Where a light layer shines: its attach point (an `OnTrigger` light: the
/// last payload position) plus `offset` in the attach point's space, × the
/// size fit. Returns the position and the attach point's rotation.
pub fn light_world_position(
    def: &LightLayerDef,
    state: &LightLayerState,
    instance: &GlobalTransform,
    globals: &Query<&GlobalTransform, Without<VfxLightLayer>>,
) -> (Vec3, Quat) {
    let (_, instance_rotation, instance_position) = instance.to_scale_rotation_translation();
    let (mut position, rotation) = match (&def.attach, &state.attach) {
        (AttachDef::Socket(_), Some(attach)) => {
            let position = attach.world_positions(instance, globals)[0];
            let rotation = match attach.sockets[0].resolution {
                SocketResolution::Node(node) => {
                    globals.get(node).map(|g| g.to_scale_rotation_translation().1).unwrap_or(instance_rotation)
                }
                _ => instance_rotation,
            };
            (position, rotation)
        }
        (AttachDef::Segment { .. }, Some(attach)) => {
            let ends = attach.world_positions(instance, globals);
            let along = ends[1] - ends[0];
            let rotation = if along.length_squared() > 1e-10 {
                Quat::from_rotation_arc(Vec3::Y, along.normalize())
            } else {
                instance_rotation
            };
            ((ends[0] + ends[1]) * 0.5, rotation)
        }
        _ => (instance_position, instance_rotation),
    };
    if let Some(p) = state.trigger_position.filter(|_| matches!(def.play, PlayDef::OnTrigger(_))) {
        position = p;
    }
    (position + rotation * (def.offset * fit_of(instance)), rotation)
}

/// Places light layers in the world (after transform propagation, before
/// visibility checks and light clustering).
pub(crate) fn place_lights(
    packages: Res<Assets<VfxPackage>>,
    instances: Query<(&VfxInstance, &GlobalTransform), Without<VfxLightLayer>>,
    globals: Query<&GlobalTransform, Without<VfxLightLayer>>,
    mut layers: Query<(&VfxLightLayer, &LightLayerState, &mut GlobalTransform)>,
) {
    for (layer, state, mut global) in layers.iter_mut() {
        let Ok((instance, instance_global)) = instances.get(layer.instance) else { continue };
        let Some(prepared) = packages.get(&instance.package).and_then(|p| p.light_layers.get(layer.index)) else {
            continue;
        };
        let (position, rotation) = light_world_position(&prepared.def, state, instance_global, &globals);
        let wanted = GlobalTransform::from(Transform::from_translation(position).with_rotation(rotation));
        if *global != wanted {
            *global = wanted;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(strength: f32, serial: u64, shown: bool) -> LightCandidate {
        LightCandidate { strength, serial, shown }
    }

    #[test]
    fn the_cap_keeps_the_strongest_then_the_newest() {
        let lights = [c(100.0, 1, false), c(500.0, 2, false), c(0.0, 3, false), c(300.0, 4, false), c(300.0, 5, false)];
        assert_eq!(light_cap_allowed(&lights, 2), vec![false, true, false, false, true], "500, then the newer 300");
        assert_eq!(light_cap_allowed(&lights, 10), vec![true, true, false, true, true], "dark lights need no place");
        assert_eq!(light_cap_allowed(&lights, 0), vec![false; 5]);
        // A shining light keeps its place against a slightly stronger newcomer.
        let lights = [c(100.0, 1, true), c(110.0, 2, false)];
        assert_eq!(light_cap_allowed(&lights, 1), vec![true, false]);
        let lights = [c(100.0, 1, true), c(200.0, 2, false)];
        assert_eq!(light_cap_allowed(&lights, 1), vec![false, true], "a much stronger one wins");
    }

    #[test]
    fn cap_levels_fade_only_lights_that_were_shining() {
        // A light that just turned on: at once.
        assert_eq!(step_cap_level(1.0, false, true, true, 0.1, 0.25), 1.0);
        assert_eq!(step_cap_level(1.0, false, true, false, 0.1, 0.25), 0.0, "no place: dark at once");
        // Pushed out while shining: fades.
        let l = step_cap_level(1.0, true, true, false, 0.1, 0.25);
        assert!((l - 0.6).abs() < 1e-5);
        let l = step_cap_level(l, true, true, true, 0.05, 0.25);
        assert!((l - 0.8).abs() < 1e-5, "and back");
        assert_eq!(step_cap_level(0.3, true, false, false, 0.1, 0.25), 1.0, "a dark light is ready for its next turn");
        assert_eq!(step_cap_level(1.0, true, true, false, 0.1, 0.0), 0.0, "no fade: at once");
    }
}
