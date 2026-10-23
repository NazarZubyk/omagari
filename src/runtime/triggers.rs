//! Triggers and the layers the runtime plays: `play: OnTrigger(name)`
//! bursts, trigger pulses on parameters, the trigger payload (position,
//! direction, strength), `play: While(param, threshold)`, and the CPU-side
//! spawn rate (`rate` binding × `visible` factor).
//!
//! The package declares its triggers (any names, any number); the game fires
//! them with [`super::VfxControl::trigger`] / [`super::VfxControl::trigger_at`].

use bevy::prelude::*;
use bevy_hanabi::prelude::*;

use super::attach::to_simulation_space;
use super::instance::{VfxInstance, VfxLayerState, VfxState};
use super::package::VfxPackage;
use super::params::VfxWarnOnce;
use super::shape::ShapeLayerState;
use crate::format::{PlayDef, SimulationSpaceDef};

/// What a trigger carries (`trigger_at`): an optional world position and
/// direction, and a strength. Layers that read the properties `trigger_pos`,
/// `trigger_dir`, `trigger_strength` get them, in the layer's simulation
/// space, before they burst. Without a position the layer's own origin is
/// used, without a direction +Y.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct TriggerArgs {
    pub position: Option<Vec3>,
    pub direction: Option<Vec3>,
    pub strength: f32,
}

impl Default for TriggerArgs {
    fn default() -> Self {
        TriggerArgs {
            position: None,
            direction: None,
            strength: 1.0,
        }
    }
}

impl TriggerArgs {
    /// A trigger at a world position.
    pub fn at(position: Vec3) -> Self {
        TriggerArgs {
            position: Some(position),
            ..default()
        }
    }

    /// The same, with a world direction.
    pub fn with_direction(mut self, direction: Vec3) -> Self {
        self.direction = Some(direction);
        self
    }

    pub fn with_strength(mut self, strength: f32) -> Self {
        self.strength = strength;
        self
    }

    /// `trigger_pos`, `trigger_dir` (unit length) and `trigger_strength` in
    /// the simulation space of a layer placed at `layer`: layer-local for
    /// `Local`, relative to the emitter position (world axes) for `Global`.
    pub fn in_layer_space(&self, layer: &GlobalTransform, space: SimulationSpaceDef) -> (Vec3, Vec3, f32) {
        let position = match self.position {
            Some(world) => to_simulation_space(world, layer, space),
            None => Vec3::ZERO,
        };
        let direction = match (self.direction, space) {
            (Some(world), SimulationSpaceDef::Local) => {
                layer.affine().inverse().transform_vector3(world).normalize_or(Vec3::Y)
            }
            (Some(world), SimulationSpaceDef::Global) => world.normalize_or(Vec3::Y),
            (None, _) => Vec3::Y,
        };
        (position, direction, self.strength)
    }
}

/// Applies the triggers fired since the last frame on running instances:
/// pulses start, `OnTrigger` layers get a burst queued (played by
/// [`drive_layer_spawners`] once their spawner exists), `OnTrigger` shape
/// layers start over (with the payload position). Triggers wait while
/// the instance is starting (so `spawn_vfx` + `trigger_at` in one frame
/// works) and are dropped while it stops.
pub(crate) fn apply_triggers(
    packages: Res<Assets<VfxPackage>>,
    mut warned: ResMut<VfxWarnOnce>,
    mut instances: Query<&mut VfxInstance>,
    mut layers: Query<&mut VfxLayerState>,
    mut shapes: Query<&mut ShapeLayerState>,
) {
    for mut instance in instances.iter_mut() {
        if instance.triggers.is_empty() {
            continue;
        }
        match instance.state() {
            VfxState::Starting => continue,
            VfxState::Stopping { .. } | VfxState::Finished => {
                instance.triggers.clear();
                continue;
            }
            VfxState::Running => {}
        }
        let Some(package) = packages.get(&instance.package) else { continue };
        // While the layers are being rebuilt (package replaced, hot reload)
        // the burst waits for the new ones.
        if instance.has_no_layers() || instance.rebuild || !instance.params.is_synced(instance.package.id()) {
            continue;
        }
        let instance = &mut *instance;
        for (name, args) in std::mem::take(&mut instance.triggers) {
            match package.project.trigger(&name) {
                Some(trigger) => {
                    for pulse in trigger.pulses.iter() {
                        if let Err(error) = instance.params.pulse(&pulse.param, pulse.to, pulse.decay) {
                            warned.param_error(&instance.package, &pulse.param, error);
                        }
                    }
                }
                None => warned.warn(&instance.package, &format!("trigger|{}", name), || {
                    format!("no trigger \"{}\"; it is ignored", name)
                }),
            }
            for (index, &entity) in instance.layers.iter().enumerate() {
                let fires = package
                    .particle_layers
                    .get(index)
                    .is_some_and(|l| l.play.trigger() == Some(name.as_str()));
                if fires {
                    if let Ok(mut state) = layers.get_mut(entity) {
                        state.burst = Some(args);
                    }
                }
            }
            for (index, &entity) in instance.shapes.iter().enumerate() {
                let fires = package
                    .shape_layers
                    .get(index)
                    .is_some_and(|l| l.def.play.trigger() == Some(name.as_str()));
                if fires {
                    if let Ok(mut state) = shapes.get_mut(entity) {
                        state.pending_trigger = Some(args);
                    }
                }
            }
        }
    }
}

/// Switches the spawners of an instance's layers: off while the instance
/// stops and for retired layers; `OnTrigger` layers burst (payload written,
/// spawner reset and switched on); `While` layers emit while their parameter
/// is at or above the threshold (restarting from their first cycle when it
/// rises). bevy_hanabi inserts the spawner on its first tick, so this runs
/// every frame.
pub(crate) fn drive_layer_spawners(
    packages: Res<Assets<VfxPackage>>,
    instances: Query<&VfxInstance>,
    mut spawners: Query<&mut EffectSpawner>,
    mut layers: Query<(&mut VfxLayerState, &GlobalTransform, Option<&mut EffectProperties>)>,
) {
    for instance in instances.iter() {
        for &(layer, _) in instance.retired.iter() {
            if let Ok(mut spawner) = spawners.get_mut(layer) {
                if spawner.active {
                    spawner.active = false;
                }
            }
        }
        let stopping = instance.lifecycle.is_stopping();
        let package = packages.get(&instance.package);
        for (index, &entity) in instance.layers.iter().enumerate() {
            let mut spawner = spawners.get_mut(entity).ok();
            if stopping {
                if let Some(spawner) = spawner.as_mut().filter(|s| s.active) {
                    spawner.active = false;
                }
                if let Ok((mut state, _, _)) = layers.get_mut(entity) {
                    state.burst = None;
                    state.while_on = false;
                }
                continue;
            }
            if instance.state() != VfxState::Running {
                continue;
            }
            let Some(prepared) = package.and_then(|p| p.particle_layers.get(index)) else { continue };
            let Ok((mut state, global, properties)) = layers.get_mut(entity) else { continue };
            if prepared.parent.is_some() {
                // Spawned by the parent's particles; play and rate do not apply.
                continue;
            }
            match &prepared.play {
                PlayDef::OnTrigger(_) => {
                    let Some(spawner) = spawner.as_mut() else { continue };
                    let Some(args) = state.burst.take() else { continue };
                    if let Some(mut properties) = properties {
                        let (position, direction, strength) = args.in_layer_space(global, prepared.simulation_space);
                        for (name, value) in [
                            ("trigger_pos", position.into()),
                            ("trigger_dir", direction.into()),
                            ("trigger_strength", strength.into()),
                        ] {
                            if prepared.properties.iter().any(|p| p == name) {
                                properties = EffectProperties::set_if_changed(properties, name, value);
                            }
                        }
                    }
                    spawner.reset();
                    spawner.active = true;
                    state.age = 0.0;
                }
                PlayDef::While(param, threshold) => {
                    let on = instance.params.float(param).is_some_and(|v| v >= *threshold);
                    let Some(spawner) = spawner.as_mut() else { continue };
                    if on && !state.while_on {
                        spawner.reset();
                        state.age = 0.0;
                    }
                    if spawner.active != on {
                        spawner.active = on;
                    }
                    state.while_on = on;
                }
                PlayDef::Loop | PlayDef::OnStart => {}
            }
        }
    }
}

/// After bevy_hanabi's spawner tick: a layer with a `rate` binding spawns
/// `rate × visibility` particles per second while its spawner plays (a
/// steady stream replacing the spawner's own count); without `rate`, the
/// spawner's count is multiplied by the visibility factor. Fractions carry
/// over between frames.
pub(crate) fn scale_spawn_counts(
    time: Res<Time>,
    mut layers: Query<(&mut VfxLayerState, &mut EffectSpawner, Option<&InheritedVisibility>)>,
) {
    let dt = time.delta_secs();
    for (mut state, mut spawner, visible) in layers.iter_mut() {
        if visible.is_some_and(|v| !v.get()) {
            continue;
        }
        let wanted = match state.rate {
            Some(rate) => {
                if !spawner.active || spawner.has_completed() {
                    state.remainder = 0.0;
                    continue;
                }
                rate.max(0.0) * state.visibility * dt
            }
            None if state.visibility < 1.0 => spawner.spawn_count as f32 * state.visibility.max(0.0),
            None => continue,
        };
        let total = state.remainder + wanted;
        let count = total.floor().max(0.0);
        state.remainder = total - count;
        if spawner.spawn_count != count as u32 {
            spawner.spawn_count = count as u32;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_goes_into_the_layer_space() {
        let layer = GlobalTransform::from(
            Transform::from_xyz(1.0, 0.0, 0.0).with_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2)),
        );
        let args = TriggerArgs::at(Vec3::new(1.0, 2.0, -3.0)).with_direction(Vec3::new(0.0, 0.0, -2.0)).with_strength(2.5);
        let (pos, dir, strength) = args.in_layer_space(&layer, SimulationSpaceDef::Local);
        // Turned 90° about Y: world -Z is local +X.
        assert!((pos - Vec3::new(3.0, 2.0, 0.0)).length() < 1e-5, "{}", pos);
        assert!((dir - Vec3::X).length() < 1e-5, "{}", dir);
        assert_eq!(strength, 2.5);
        let (pos, dir, _) = args.in_layer_space(&layer, SimulationSpaceDef::Global);
        assert!((pos - Vec3::new(0.0, 2.0, -3.0)).length() < 1e-5, "{}", pos);
        assert!((dir - Vec3::NEG_Z).length() < 1e-5, "{}", dir);
        // No payload: the layer's origin, +Y, strength 1.
        assert_eq!(TriggerArgs::default().in_layer_space(&layer, SimulationSpaceDef::Local), (Vec3::ZERO, Vec3::Y, 1.0));
    }
}
