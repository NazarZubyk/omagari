//! Parameters at runtime: per instance a `target` (what the game asked for)
//! and a `current` value that follows it with exponential smoothing, plus
//! trigger pulses; uploaded into the hanabi properties of the instance's
//! layers, and used by the CPU-side bindings (`rate`, `visible`, `play:
//! While`).
//!
//! The package declares its parameters (any names, any number); the plugin
//! knows none of them. Unknown names and wrong kinds are warned about once
//! per (package, name) and otherwise ignored.

use bevy::platform::collections::HashSet;
use bevy::prelude::*;
use bevy_hanabi::prelude::*;

use super::instance::{VfxInstance, VfxLayerState};
use super::package::VfxPackage;
use crate::format::{Binding, ParamDef, ParamKind, ParamValue};

// --- pure helpers ------------------------------------------------------------

/// One smoothing step: `current` moves towards `target` by `1 - exp(-dt /
/// smoothing)` of the gap (~63% per `smoothing` seconds). `smoothing <= 0`
/// jumps to the target.
pub fn smooth(current: f32, target: f32, dt: f32, smoothing: f32) -> f32 {
    if smoothing <= 0.0 || !current.is_finite() {
        return target;
    }
    current + (target - current) * (1.0 - (-dt / smoothing).exp())
}

/// [`smooth`] for a parameter value; colours per channel. Values of
/// different kinds jump.
pub fn smooth_value(current: ParamValue, target: ParamValue, dt: f32, smoothing: f32) -> ParamValue {
    match (current, target) {
        (ParamValue::Float(c), ParamValue::Float(t)) => ParamValue::Float(smooth(c, t, dt, smoothing)),
        (ParamValue::Color(c), ParamValue::Color(t)) => ParamValue::Color(Vec4::new(
            smooth(c.x, t.x, dt, smoothing),
            smooth(c.y, t.y, dt, smoothing),
            smooth(c.z, t.z, dt, smoothing),
            smooth(c.w, t.w, dt, smoothing),
        )),
        (_, target) => target,
    }
}

/// Evaluates a binding over the instance's current parameter values (an
/// `Age` curve at age 0); `None` when it reads an unknown or non-Float
/// parameter (or has no keys).
pub fn evaluate_binding<T>(binding: &Binding<T>, params: &ParamState) -> Option<T>
where
    T: Copy + std::ops::Add<Output = T> + std::ops::Mul<f32, Output = T>,
{
    evaluate_binding_at(binding, params, 0.0)
}

/// [`evaluate_binding`] for a layer of `age` seconds. Shared by the CPU-side
/// bound values (spawn rate; surface and shape values go through
/// `EffectMaterialDef::evaluate_at`).
pub fn evaluate_binding_at<T>(binding: &Binding<T>, params: &ParamState, age: f32) -> Option<T>
where
    T: Copy + std::ops::Add<Output = T> + std::ops::Mul<f32, Output = T>,
{
    binding.evaluate_at(|name| params.float(name), age)
}

// --- per-instance state ------------------------------------------------------

/// A trigger pulse on a Float parameter: the value jumps to `to` and goes
/// back to the smoothed value linearly over `decay` seconds.
#[derive(Clone, Copy, PartialEq, Debug)]
struct Pulse {
    to: f32,
    decay: f32,
    elapsed: f32,
}

impl Pulse {
    /// Weight of `to` in the value: 1 when fired, 0 after `decay`.
    fn weight(&self) -> f32 {
        if self.decay <= 0.0 {
            if self.elapsed <= 0.0 { 1.0 } else { 0.0 }
        } else {
            (1.0 - self.elapsed / self.decay).clamp(0.0, 1.0)
        }
    }
}

/// One declared parameter of an instance.
#[derive(Clone, PartialEq, Debug)]
pub struct ParamSlot {
    pub def: ParamDef,
    /// What the game asked for (clamped).
    pub target: ParamValue,
    /// The smoothed value, without pulses.
    pub smoothed: ParamValue,
    pulse: Option<Pulse>,
}

impl ParamSlot {
    fn new(def: &ParamDef) -> Self {
        let default = def.clamp(def.default);
        ParamSlot {
            def: def.clone(),
            target: default,
            smoothed: default,
            pulse: None,
        }
    }

    /// The current value: smoothed, plus a running pulse, clamped.
    pub fn current(&self) -> ParamValue {
        match (self.smoothed, self.pulse) {
            (ParamValue::Float(v), Some(pulse)) => {
                let w = pulse.weight();
                self.def.clamp(ParamValue::Float(v + (pulse.to - v) * w))
            }
            (value, _) => value,
        }
    }
}

/// Why a parameter could not be set.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ParamError {
    /// The package declares no parameter with this name.
    Unknown,
    /// The value has the other kind (number for a colour, or the reverse).
    WrongKind { expected: ParamKind },
}

/// The parameter values of one instance.
#[derive(Clone, Default, Debug)]
pub struct ParamState {
    slots: Vec<ParamSlot>,
    /// Values set before the package was loaded: (name, value, immediate).
    pending: Vec<(String, ParamValue, bool)>,
    /// The package the slots were made for.
    synced: Option<AssetId<VfxPackage>>,
    /// The package asset changed (hot reload): make the slots again.
    pub(crate) resync: bool,
}

impl ParamState {
    /// Makes the slots for `defs`, keeping the values of parameters that
    /// still exist with the same kind (clamped to the new range), then applies
    /// the values set before. Returns the names that could not be set.
    pub fn sync(&mut self, defs: &[ParamDef]) -> Vec<(String, ParamError)> {
        let old = std::mem::take(&mut self.slots);
        self.slots = defs
            .iter()
            .map(|def| {
                let mut slot = ParamSlot::new(def);
                if let Some(previous) = old.iter().find(|s| s.def.name == def.name && s.def.kind() == def.kind()) {
                    slot.target = def.clamp(previous.target);
                    slot.smoothed = def.clamp(previous.smoothed);
                    slot.pulse = previous.pulse;
                }
                slot
            })
            .collect();
        let mut errors = Vec::new();
        for (name, value, now) in std::mem::take(&mut self.pending) {
            if let Err(e) = self.set(&name, value, now) {
                errors.push((name, e));
            }
        }
        errors
    }

    /// Whether the slots match the package (else [`ParamState::sync`] is due).
    pub fn is_synced(&self, package: AssetId<VfxPackage>) -> bool {
        self.synced == Some(package) && !self.resync
    }

    pub(crate) fn mark_synced(&mut self, package: AssetId<VfxPackage>) {
        self.synced = Some(package);
        self.resync = false;
    }

    /// Remembers a value until the package is loaded.
    pub(crate) fn set_later(&mut self, name: &str, value: ParamValue, now: bool) {
        self.pending.push((name.to_string(), value, now));
    }

    /// Sets the target of a parameter (clamped); `now` also sets the current
    /// value (no smoothing).
    pub fn set(&mut self, name: &str, value: ParamValue, now: bool) -> Result<(), ParamError> {
        let slot = self.slots.iter_mut().find(|s| s.def.name == name).ok_or(ParamError::Unknown)?;
        if value.kind() != slot.def.kind() {
            return Err(ParamError::WrongKind { expected: slot.def.kind() });
        }
        slot.target = slot.def.clamp(value);
        if now {
            slot.smoothed = slot.target;
        }
        Ok(())
    }

    /// Starts a pulse on a Float parameter.
    pub fn pulse(&mut self, name: &str, to: f32, decay: f32) -> Result<(), ParamError> {
        let slot = self.slots.iter_mut().find(|s| s.def.name == name).ok_or(ParamError::Unknown)?;
        if slot.def.kind() != ParamKind::Float {
            return Err(ParamError::WrongKind { expected: ParamKind::Float });
        }
        slot.pulse = Some(Pulse {
            to,
            decay: decay.max(0.0),
            elapsed: 0.0,
        });
        Ok(())
    }

    /// Advances smoothing and pulses by `dt` seconds.
    pub fn tick(&mut self, dt: f32) {
        for slot in self.slots.iter_mut() {
            slot.smoothed = smooth_value(slot.smoothed, slot.target, dt, slot.def.smoothing);
            if let Some(pulse) = &mut slot.pulse {
                pulse.elapsed += dt;
                if pulse.elapsed > pulse.decay {
                    slot.pulse = None;
                }
            }
        }
    }

    /// The current value of a parameter.
    pub fn get(&self, name: &str) -> Option<ParamValue> {
        self.slot(name).map(ParamSlot::current)
    }

    /// The current value of a Float parameter.
    pub fn float(&self, name: &str) -> Option<f32> {
        self.get(name).and_then(|v| v.as_float())
    }

    pub fn slot(&self, name: &str) -> Option<&ParamSlot> {
        self.slots.iter().find(|s| s.def.name == name)
    }

    pub fn slots(&self) -> &[ParamSlot] {
        &self.slots
    }
}

// --- warnings ------------------------------------------------------------------

/// Warnings already logged, so each (package, name) problem is logged once.
#[derive(Resource, Default)]
pub struct VfxWarnOnce(HashSet<String>);

impl VfxWarnOnce {
    /// Logs `message` the first time `key` is seen for `package`.
    pub fn warn(&mut self, package: &Handle<VfxPackage>, key: &str, message: impl FnOnce() -> String) {
        if self.0.insert(format!("{:?}|{}", package.id(), key)) {
            let name = package
                .path()
                .map(|p| p.to_string())
                .unwrap_or_else(|| format!("{:?}", package.id()));
            warn!("VFX package {}: {}", name, message());
        }
    }

    /// Logs a [`ParamError`] once.
    pub fn param_error(&mut self, package: &Handle<VfxPackage>, name: &str, error: ParamError) {
        self.warn(package, &format!("param|{}", name), || match error {
            ParamError::Unknown => format!("no parameter \"{}\"; the value is ignored", name),
            ParamError::WrongKind { expected } => {
                format!("parameter \"{}\" is a {}; a value of the other kind is ignored", name, expected.name())
            }
        });
    }
}

// --- systems ---------------------------------------------------------------------

/// Makes the parameter slots of instances whose package is loaded (or
/// changed), then advances smoothing and pulses.
pub(crate) fn update_params(
    time: Res<Time>,
    packages: Res<Assets<VfxPackage>>,
    mut warned: ResMut<VfxWarnOnce>,
    mut instances: Query<&mut VfxInstance>,
) {
    let dt = time.delta_secs();
    for mut instance in instances.iter_mut() {
        let instance = &mut *instance;
        if let Some(package) = packages.get(&instance.package) {
            sync_instance(instance, package, &mut warned);
        }
        instance.params.tick(dt);
    }
}

/// Makes the instance's parameter slots when they are not for its package.
pub(crate) fn sync_instance(instance: &mut VfxInstance, package: &VfxPackage, warned: &mut VfxWarnOnce) {
    let id = instance.package.id();
    if instance.params.is_synced(id) {
        return;
    }
    let errors = instance.params.sync(&package.project.parameters);
    instance.params.mark_synced(id);
    for (name, error) in errors {
        warned.param_error(&instance.package, &name, error);
    }
}

/// Writes the current parameter values into the hanabi properties of every
/// running layer that declares them (only changed values are written, so
/// hanabi uploads only then), and works out the layers' CPU-side values:
/// age, visibility factor and bound spawn rate.
pub(crate) fn upload_layer_values(
    time: Res<Time>,
    packages: Res<Assets<VfxPackage>>,
    mut warned: ResMut<VfxWarnOnce>,
    instances: Query<&VfxInstance>,
    mut layers: Query<(&mut VfxLayerState, Option<&mut EffectProperties>)>,
) {
    let dt = time.delta_secs();
    for instance in instances.iter() {
        let Some(package) = packages.get(&instance.package) else { continue };
        if !instance.params.is_synced(instance.package.id()) {
            continue;
        }
        for (index, &entity) in instance.layers.iter().enumerate() {
            let Some(prepared) = package.particle_layers.get(index) else { continue };
            let Ok((mut state, properties)) = layers.get_mut(entity) else { continue };
            if state.generation != instance.generation {
                // Restarted: layers play again from age 0.
                state.generation = instance.generation;
                state.age = 0.0;
            } else {
                state.age += dt;
            }

            let visibility = match &prepared.visible {
                Some(visible) => match instance.params.float(&visible.param) {
                    Some(value) => visible.factor(value),
                    None => {
                        warned.warn(&instance.package, &format!("visible|{}", prepared.name), || {
                            format!(
                                "layer \"{}\": visible reads \"{}\", which is not a Float parameter; the layer stays visible",
                                prepared.name, visible.param
                            )
                        });
                        1.0
                    }
                },
                None => 1.0,
            };
            // Child layers are spawned by their parent's particles.
            let rate = prepared.rate.as_ref().filter(|_| prepared.parent.is_none()).and_then(|binding| {
                let rate = evaluate_binding_at(binding, &instance.params, state.age);
                if rate.is_none() {
                    warned.warn(&instance.package, &format!("rate|{}", prepared.name), || {
                        format!(
                            "layer \"{}\": rate reads \"{}\", which is not a Float parameter (or the curve has no keys); the spawner's own count applies",
                            prepared.name,
                            binding.param().unwrap_or_default()
                        )
                    });
                }
                rate
            });
            if state.visibility != visibility {
                state.visibility = visibility;
            }
            if state.rate != rate {
                state.rate = rate;
            }

            let Some(mut properties) = properties else { continue };
            for (param, property) in prepared.param_properties.iter() {
                if let Some(value) = instance.params.get(param) {
                    properties = EffectProperties::set_if_changed(properties, property, value.to_value());
                }
            }
            if prepared.properties.iter().any(|p| p == "visibility") {
                EffectProperties::set_if_changed(properties, "visibility", visibility.into());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def(name: &str, default: ParamValue, smoothing: f32) -> ParamDef {
        ParamDef {
            name: name.into(),
            default,
            min: 0.0,
            max: 1.0,
            smoothing,
        }
    }

    #[test]
    fn smoothing_reaches_63_percent_after_one_time_constant() {
        // Many small steps over `smoothing` seconds: 1 - 1/e of the gap.
        let mut v = 0.0;
        for _ in 0..100 {
            v = smooth(v, 1.0, 0.004, 0.4);
        }
        assert!((v - (1.0 - (-1.0f32).exp())).abs() < 1e-3, "{}", v);
        // Frame-rate independent: one big step gives the same.
        assert!((smooth(0.0, 1.0, 0.4, 0.4) - v).abs() < 1e-3);
        assert_eq!(smooth(0.2, 0.9, 0.016, 0.0), 0.9, "0 = immediate");
    }

    #[test]
    fn floats_are_clamped_and_colours_smooth_per_channel() {
        let mut state = ParamState::default();
        let errors = state.sync(&[
            def("power", ParamValue::Float(0.0), 0.0),
            def("tint", ParamValue::Color(Vec4::new(1.0, 0.0, 0.0, 1.0)), 0.5),
        ]);
        assert!(errors.is_empty());
        state.set("power", 3.0.into(), false).unwrap();
        state.tick(0.016);
        assert_eq!(state.float("power"), Some(1.0), "clamped to max");
        state.set("power", (-1.0).into(), false).unwrap();
        state.tick(0.016);
        assert_eq!(state.float("power"), Some(0.0), "clamped to min");

        state.set("tint", Vec4::new(0.0, 1.0, 0.0, 1.0).into(), false).unwrap();
        state.tick(0.5);
        let c = state.get("tint").unwrap().as_color().unwrap();
        let k = 1.0 - (-1.0f32).exp();
        assert!((c.x - (1.0 - k)).abs() < 1e-4 && (c.y - k).abs() < 1e-4 && c.w == 1.0, "{}", c);
        state.set("tint", Color::srgb(0.0, 0.0, 1.0).into(), true).unwrap();
        assert_eq!(state.get("tint"), Some(ParamValue::Color(Vec4::new(0.0, 0.0, 1.0, 1.0))), "now = no smoothing");

        assert_eq!(state.set("nope", 1.0.into(), false), Err(ParamError::Unknown));
        assert_eq!(
            state.set("tint", 1.0.into(), false),
            Err(ParamError::WrongKind { expected: ParamKind::Color })
        );
    }

    #[test]
    fn values_survive_a_resync_and_pending_values_apply() {
        let mut state = ParamState::default();
        state.set_later("power", 0.7.into(), true);
        state.set_later("missing", 1.0.into(), false);
        let errors = state.sync(&[def("power", ParamValue::Float(0.0), 0.3)]);
        assert_eq!(errors, vec![("missing".to_string(), ParamError::Unknown)]);
        assert_eq!(state.float("power"), Some(0.7));

        // The package changed: power now goes to 0.5 at most, a new
        // parameter starts at its default, a kind change resets.
        let mut narrower = def("power", ParamValue::Float(0.0), 0.3);
        narrower.max = 0.5;
        state.sync(&[narrower, def("extra", ParamValue::Float(0.25), 0.0)]);
        assert_eq!(state.float("power"), Some(0.5));
        assert_eq!(state.float("extra"), Some(0.25));
        state.sync(&[def("power", ParamValue::Color(Vec4::ONE), 0.0)]);
        assert_eq!(state.get("power"), Some(ParamValue::Color(Vec4::ONE)));
    }

    #[test]
    fn pulses_jump_and_decay_back_to_the_target() {
        let mut state = ParamState::default();
        state.sync(&[def("flash", ParamValue::Float(0.0), 0.0), def("tint", ParamValue::Color(Vec4::ONE), 0.0)]);
        state.pulse("flash", 1.0, 0.4).unwrap();
        assert_eq!(state.float("flash"), Some(1.0), "jumps at once");
        state.tick(0.1);
        assert!((state.float("flash").unwrap() - 0.75).abs() < 1e-5);
        state.tick(0.2);
        assert!((state.float("flash").unwrap() - 0.25).abs() < 1e-5);
        state.tick(0.2);
        assert_eq!(state.float("flash"), Some(0.0), "back to the target");
        assert_eq!(
            state.pulse("tint", 1.0, 0.1),
            Err(ParamError::WrongKind { expected: ParamKind::Float }),
            "colours cannot pulse"
        );
    }

    #[test]
    fn bindings_read_current_float_values() {
        let mut state = ParamState::default();
        state.sync(&[def("power", ParamValue::Float(0.5), 0.0)]);
        let rate = Binding::Param("power".into(), vec![(0.0, 10.0), (1.0, 30.0)]);
        assert_eq!(evaluate_binding(&rate, &state), Some(20.0));
        assert_eq!(evaluate_binding(&Binding::Const(7.0), &state), Some(7.0));
        assert_eq!(evaluate_binding(&Binding::Param("nope".into(), vec![(0.0, 1.0)]), &state), None);
        assert_eq!(evaluate_binding(&Binding::<f32>::Param("power".into(), vec![]), &state), None);
    }
}
