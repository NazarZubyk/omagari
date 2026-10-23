//! VFX instances: one playing copy of a package attached to a target entity.
//!
//! ```ignore
//! let package: Handle<VfxPackage> = asset_server.load("vfx/poison.vfx.ron");
//! let instance = commands.spawn_vfx(&package, target);
//! let burst = commands.spawn_vfx_at(&package, Transform::from_xyz(1.0, 0.0, 2.0));
//!
//! let tinted = commands.spawn_vfx_with(&package, target, VfxSpawn { params: &[("tint", Color::WHITE.into())] });
//!
//! fn later(mut vfx: VfxControl) {
//!     vfx.set_param(instance, "intensity", 0.8); // smoothed; names are the package's
//!     vfx.trigger(instance, "tick");
//!     vfx.trigger_at(instance, "hit", TriggerArgs::at(contact_point));
//!     vfx.stop(instance);    // graceful: no new particles, despawn when all died
//!     vfx.restart(instance); // while stopping: plays again, same instance
//! }
//! // when an instance ends, a `VfxFinished { instance, target }` message is
//! // sent once.
//! ```

use bevy::asset::{LoadState, RecursiveDependencyLoadState};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_hanabi::prelude::*;

use super::attach::{FitState, SocketAttach};
use super::package::{apply_texture_settings_to, VfxPackage};
use super::params::{sync_instance, ParamState, VfxWarnOnce};
use super::shape::{spawn_shape_layer, ShapeLayerState};
use super::surface::{SurfaceLayerState, VfxSurfaceLayer};
use super::triggers::TriggerArgs;
use crate::format::{AttachDef, ParamValue};

/// Sent once when an instance ends, right before the instance entity is
/// despawned:
/// - it was stopped and all its particles have died;
/// - it finished by itself (every layer is one-shot, see
///   [`VfxPackage::auto_finish`]);
/// - its target was despawned (then `target` no longer exists), or the
///   instance was despawned by other code.
///
/// For [`VfxCommandsExt::spawn_vfx_at`] instances `target` is the instance
/// itself.
#[derive(Message, Clone, Copy, PartialEq, Eq, Debug)]
pub struct VfxFinished {
    pub instance: Entity,
    pub target: Entity,
}

/// Lifecycle state of an instance.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum VfxState {
    /// Waiting for the package and its textures to load.
    Starting,
    /// Layers spawned and playing.
    Running,
    /// No new particles; despawned when `remaining` seconds have passed.
    Stopping { remaining: f32 },
    /// Done; the instance is being despawned.
    Finished,
}

/// The instance lifecycle as a plain state machine (no ECS), so it can be
/// tested and reused on its own.
///
/// `Starting → Running → Stopping → Finished`; stopping while `Starting`
/// finishes on the next tick without ever spawning layers. `restart` brings a
/// stopping instance back to `Running`. A running instance with an
/// auto-finish time finishes by itself when that time has passed.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct VfxLifecycle {
    state: VfxState,
    /// Seconds until a running instance finishes by itself.
    auto_finish: Option<f32>,
}

impl Default for VfxLifecycle {
    fn default() -> Self {
        VfxLifecycle {
            state: VfxState::Starting,
            auto_finish: None,
        }
    }
}

impl VfxLifecycle {
    pub fn state(&self) -> VfxState {
        self.state
    }

    /// The layers were spawned; the instance runs until stopped.
    pub fn started(&mut self) {
        self.started_for(None);
    }

    /// The layers were spawned; the instance finishes by itself after
    /// `auto_finish` seconds (`None` = runs until stopped).
    pub fn started_for(&mut self, auto_finish: Option<f32>) {
        if self.state == VfxState::Starting {
            self.state = VfxState::Running;
            self.auto_finish = auto_finish;
        }
    }

    /// Stops emitting; finishes after `linger` seconds (the longest particle
    /// lifetime). No effect when already stopping or finished.
    pub fn stop(&mut self, linger: f32) {
        self.state = match self.state {
            VfxState::Starting => VfxState::Stopping { remaining: 0.0 },
            VfxState::Running => VfxState::Stopping {
                remaining: linger.max(0.0),
            },
            other => other,
        };
    }

    /// Plays again from `Stopping` (or restarts the auto-finish time of a
    /// running instance whose layers were respawned). Returns `false` when
    /// finished (nothing to restart).
    pub fn restart(&mut self, auto_finish: Option<f32>) -> bool {
        match self.state {
            VfxState::Finished => false,
            VfxState::Starting => true,
            VfxState::Running | VfxState::Stopping { .. } => {
                self.state = VfxState::Running;
                self.auto_finish = auto_finish;
                true
            }
        }
    }

    /// Advances time; returns `true` on the tick the instance finishes.
    pub fn tick(&mut self, dt: f32) -> bool {
        match self.state {
            VfxState::Stopping { remaining } => {
                let remaining = remaining - dt;
                if remaining <= 0.0 {
                    self.state = VfxState::Finished;
                    return true;
                }
                self.state = VfxState::Stopping { remaining };
            }
            VfxState::Running => {
                if let Some(remaining) = self.auto_finish {
                    let remaining = remaining - dt;
                    if remaining <= 0.0 {
                        self.state = VfxState::Finished;
                        self.auto_finish = None;
                        return true;
                    }
                    self.auto_finish = Some(remaining);
                }
            }
            _ => {}
        }
        false
    }

    /// Seconds until a running instance finishes by itself.
    pub fn auto_finish_remaining(&self) -> Option<f32> {
        self.auto_finish
    }

    pub fn is_alive(&self) -> bool {
        self.state != VfxState::Finished
    }

    pub fn is_stopping(&self) -> bool {
        matches!(self.state, VfxState::Stopping { .. })
    }
}

/// An instance of a [`VfxPackage`]: a child of its target, with one child
/// entity per layer (layers attached to a socket are children of the socket's
/// node). Spawn it with [`VfxCommandsExt::spawn_vfx`], control it with
/// [`VfxControl`].
#[derive(Component)]
#[require(Transform, Visibility)]
pub struct VfxInstance {
    pub(crate) package: Handle<VfxPackage>,
    pub(crate) target: Entity,
    pub(crate) lifecycle: VfxLifecycle,
    pub(crate) layers: Vec<Entity>,
    /// Surface layer entities, in the order of the package's surface layers.
    pub(crate) surfaces: Vec<Entity>,
    /// Shape layer entities, in the order of the package's shape layers.
    pub(crate) shapes: Vec<Entity>,
    /// Counts restarts (`restart` while stopping): layers that keep their
    /// own clock (age, one-shot shapes) start over when it changes.
    pub(crate) generation: u32,
    /// Layers of an earlier build (hot reload, package replaced): emitting
    /// stopped, despawned when the seconds run out.
    pub(crate) retired: Vec<(Entity, f32)>,
    /// Respawn the layers once the package and its textures are ready.
    pub(crate) rebuild: bool,
    /// Stop duration of the package the current layers were built from.
    pub(crate) linger: f32,
    /// Spawned with [`VfxCommandsExt::spawn_vfx_at`]: no target, the
    /// instance's own transform places it.
    pub(crate) in_world: bool,
    /// Parameter values; they survive rebuilds (hot reload, package
    /// replaced).
    pub(crate) params: ParamState,
    /// Triggers fired and not applied yet.
    pub(crate) triggers: Vec<(String, TriggerArgs)>,
}

impl VfxInstance {
    pub fn new(package: Handle<VfxPackage>, target: Entity) -> Self {
        VfxInstance {
            package,
            target,
            lifecycle: VfxLifecycle::default(),
            layers: Vec::new(),
            surfaces: Vec::new(),
            shapes: Vec::new(),
            generation: 0,
            retired: Vec::new(),
            rebuild: false,
            linger: 0.0,
            in_world: false,
            params: ParamState::default(),
            triggers: Vec::new(),
        }
    }

    /// Starts with these parameter values (no smoothing from the defaults).
    pub fn with_params(mut self, params: &[(&str, ParamValue)]) -> Self {
        for (name, value) in params {
            self.params.set_later(name, *value, true);
        }
        self
    }

    /// The instance's parameter values.
    pub fn params(&self) -> &ParamState {
        &self.params
    }

    pub fn package(&self) -> &Handle<VfxPackage> {
        &self.package
    }

    /// The entity the instance is attached to (the instance itself for
    /// [`VfxCommandsExt::spawn_vfx_at`] instances).
    pub fn target(&self) -> Entity {
        self.target
    }

    /// Whether the instance was spawned in the world, without a target.
    pub fn is_in_world(&self) -> bool {
        self.in_world
    }

    pub fn state(&self) -> VfxState {
        self.lifecycle.state()
    }

    pub fn lifecycle(&self) -> &VfxLifecycle {
        &self.lifecycle
    }

    /// Layer entities, in the order of the package's particle layers. Empty
    /// until the package has loaded.
    pub fn layers(&self) -> &[Entity] {
        &self.layers
    }

    /// Surface layer entities, in the order of
    /// [`VfxPackage::surface_layers`]. Empty until the package has loaded.
    pub fn surface_layers(&self) -> &[Entity] {
        &self.surfaces
    }

    /// Shape layer entities, in the order of [`VfxPackage::shape_layers`].
    /// Empty until the package has loaded.
    pub fn shape_layers(&self) -> &[Entity] {
        &self.shapes
    }

    /// How often the instance was restarted (`restart` while stopping).
    pub fn generation(&self) -> u32 {
        self.generation
    }

    /// Whether no layer of any kind has been spawned yet.
    pub(crate) fn has_no_layers(&self) -> bool {
        self.layers.is_empty() && self.surfaces.is_empty() && self.shapes.is_empty()
    }

    /// Layers of an earlier build that are fading out.
    pub fn retired_layers(&self) -> impl Iterator<Item = Entity> + '_ {
        self.retired.iter().map(|(e, _)| *e)
    }
}

/// A layer entity of an instance.
#[derive(Component, Clone, Copy, Debug)]
pub struct VfxLayer {
    pub instance: Entity,
    /// Index into [`VfxPackage::particle_layers`].
    pub index: usize,
}

/// Runtime state of a layer entity: what the parameters and triggers ask of
/// its spawner.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct VfxLayerState {
    /// `visible` factor, 0..1 (1 without `visible`).
    pub visibility: f32,
    /// Spawn rate from the `rate` binding (particles per second), `None` =
    /// the spawner's own count.
    pub rate: Option<f32>,
    /// A burst waiting for the spawner (`OnTrigger` layers), with its
    /// payload.
    pub burst: Option<TriggerArgs>,
    /// A `While` layer's condition was true last frame.
    pub while_on: bool,
    /// Fraction of a particle carried over to the next frame.
    pub remainder: f32,
    /// Seconds since the layer started, or since its last burst / `While`
    /// switch-on (what `Age` bindings read).
    pub age: f32,
    /// The instance's restart count this state was made for.
    pub generation: u32,
}

impl Default for VfxLayerState {
    fn default() -> Self {
        VfxLayerState {
            visibility: 1.0,
            rate: None,
            burst: None,
            while_on: false,
            remainder: 0.0,
            age: 0.0,
            generation: 0,
        }
    }
}

/// Options of [`VfxCommandsExt::spawn_vfx_with`].
#[derive(Clone, Copy, Default, Debug)]
pub struct VfxSpawn<'a> {
    /// Starting parameter values (set at once, not smoothed from the
    /// defaults). Names are the package's parameters.
    pub params: &'a [(&'a str, ParamValue)],
}

/// Spawning instances from [`Commands`].
pub trait VfxCommandsExt {
    /// Spawns an instance of `package` attached to `target` (as its child)
    /// and returns the instance entity. The layers appear once the package
    /// and its textures have loaded.
    fn spawn_vfx(&mut self, package: &Handle<VfxPackage>, target: Entity) -> Entity {
        self.spawn_vfx_with(package, target, VfxSpawn::default())
    }

    /// [`VfxCommandsExt::spawn_vfx`] with starting parameter values.
    fn spawn_vfx_with(&mut self, package: &Handle<VfxPackage>, target: Entity, options: VfxSpawn) -> Entity;

    /// Spawns an instance of `package` in the world at `transform`, with no
    /// target (a one-shot hit, an explosion). It is cleaned up when it
    /// finishes: by itself when every layer is one-shot, else when stopped.
    fn spawn_vfx_at(&mut self, package: &Handle<VfxPackage>, transform: Transform) -> Entity {
        self.spawn_vfx_at_with(package, transform, VfxSpawn::default())
    }

    /// [`VfxCommandsExt::spawn_vfx_at`] with starting parameter values.
    fn spawn_vfx_at_with(&mut self, package: &Handle<VfxPackage>, transform: Transform, options: VfxSpawn) -> Entity;
}

impl VfxCommandsExt for Commands<'_, '_> {
    fn spawn_vfx_with(&mut self, package: &Handle<VfxPackage>, target: Entity, options: VfxSpawn) -> Entity {
        self.spawn((
            Name::new("vfx"),
            VfxInstance::new(package.clone(), target).with_params(options.params),
            FitState::default(),
            ChildOf(target),
        ))
        .id()
    }

    fn spawn_vfx_at_with(&mut self, package: &Handle<VfxPackage>, transform: Transform, options: VfxSpawn) -> Entity {
        let instance = self.spawn_empty().id();
        let mut component = VfxInstance::new(package.clone(), instance).with_params(options.params);
        component.in_world = true;
        self.entity(instance)
            .insert((Name::new("vfx"), component, transform));
        instance
    }
}

/// Controls running instances.
#[derive(SystemParam)]
pub struct VfxControl<'w, 's> {
    instances: Query<'w, 's, &'static mut VfxInstance>,
    spawners: Query<'w, 's, &'static mut EffectSpawner>,
    packages: Res<'w, Assets<VfxPackage>>,
    warned: ResMut<'w, VfxWarnOnce>,
}

impl VfxControl<'_, '_> {
    /// Stops an instance gracefully: its layers stop emitting, and when the
    /// last particle has died the instance is despawned and [`VfxFinished`]
    /// is sent. Returns `false` if `instance` is not an instance.
    pub fn stop(&mut self, instance: Entity) -> bool {
        let Ok(mut inst) = self.instances.get_mut(instance) else {
            return false;
        };
        let linger = inst.linger;
        inst.lifecycle.stop(linger);
        true
    }

    /// Plays a stopping instance again, reusing the instance and its layers:
    /// spawners restart from their first cycle (one-shot layers play again)
    /// and the instance is `Running`. Does nothing to a running or starting
    /// instance. Returns `false` if `instance` is not an instance or has
    /// finished.
    pub fn restart(&mut self, instance: Entity) -> bool {
        let Ok(mut inst) = self.instances.get_mut(instance) else {
            return false;
        };
        if !inst.lifecycle.is_stopping() {
            return inst.lifecycle.is_alive();
        }
        if inst.has_no_layers() {
            // Stopped before it started: start over.
            inst.lifecycle = VfxLifecycle::default();
            return true;
        }
        let package = self.packages.get(&inst.package);
        let auto_finish = package.and_then(|p| p.auto_finish);
        inst.lifecycle.restart(auto_finish);
        inst.generation = inst.generation.wrapping_add(1);
        for (index, &layer) in inst.layers.iter().enumerate() {
            // OnTrigger / While layers are switched by their trigger or
            // parameter, never by a restart.
            let driven = package
                .and_then(|p| p.particle_layers.get(index))
                .is_some_and(|l| l.play.is_driven());
            if driven {
                continue;
            }
            if let Ok(mut spawner) = self.spawners.get_mut(layer) {
                spawner.reset();
                spawner.active = true;
            }
        }
        true
    }

    /// Sets a parameter of the instance; its current value follows smoothly
    /// (the parameter's `smoothing`). Float values are clamped to the
    /// parameter's range. Names are the package's parameters: an unknown
    /// name or a value of the wrong kind (a number for a colour parameter)
    /// is ignored with one warning per package and name. Values set before
    /// the package has loaded apply when it has. Returns `false` if
    /// `instance` is not an instance.
    pub fn set_param(&mut self, instance: Entity, name: &str, value: impl Into<ParamValue>) -> bool {
        self.set(instance, name, value.into(), false)
    }

    /// Like [`VfxControl::set_param`], without smoothing: the current value
    /// jumps to `value`.
    pub fn set_param_now(&mut self, instance: Entity, name: &str, value: impl Into<ParamValue>) -> bool {
        self.set(instance, name, value.into(), true)
    }

    fn set(&mut self, instance: Entity, name: &str, value: ParamValue, now: bool) -> bool {
        let Ok(mut inst) = self.instances.get_mut(instance) else {
            return false;
        };
        let inst = &mut *inst;
        match self.packages.get(&inst.package) {
            Some(package) => {
                sync_instance(inst, package, &mut self.warned);
                if let Err(error) = inst.params.set(name, value, now) {
                    self.warned.param_error(&inst.package, name, error);
                }
            }
            None => inst.params.set_later(name, value, now),
        }
        true
    }

    /// The current (smoothed, pulsed) value of a parameter; `None` if
    /// `instance` is not an instance, its package has not loaded yet, or it
    /// has no such parameter.
    pub fn param(&self, instance: Entity, name: &str) -> Option<ParamValue> {
        let inst = self.instances.get(instance).ok()?;
        if !inst.params.is_synced(inst.package.id()) {
            return None;
        }
        inst.params.get(name)
    }

    /// Fires a trigger of the package: `OnTrigger` layers burst and the
    /// trigger's pulses run. Fired while the instance is starting, it applies
    /// once the layers exist; ignored while it stops. Unknown names are
    /// ignored with one warning per package and name. Returns `false` if
    /// `instance` is not an instance.
    pub fn trigger(&mut self, instance: Entity, name: &str) -> bool {
        self.trigger_at(instance, name, TriggerArgs::default())
    }

    /// [`VfxControl::trigger`] with a payload: a world position and
    /// direction and a strength, written into the properties `trigger_pos`,
    /// `trigger_dir`, `trigger_strength` of the bursting layers (in their
    /// simulation space) before they burst.
    pub fn trigger_at(&mut self, instance: Entity, name: &str, args: TriggerArgs) -> bool {
        let Ok(mut inst) = self.instances.get_mut(instance) else {
            return false;
        };
        inst.triggers.push((name.to_string(), args));
        true
    }

    /// Replaces the instance's package and rebuilds its layers in place once
    /// the new package (and its textures) are ready: the old layers stop
    /// emitting and fade out, the new ones start, the instance entity and its
    /// state are kept (a stopping instance runs again). This is what hot
    /// reload does when the package file changes. Returns `false` if
    /// `instance` is not an instance or has finished.
    pub fn replace_package(&mut self, instance: Entity, package: Handle<VfxPackage>) -> bool {
        let Ok(mut inst) = self.instances.get_mut(instance) else {
            return false;
        };
        if !inst.lifecycle.is_alive() {
            return false;
        }
        inst.package = package;
        if inst.has_no_layers() {
            inst.lifecycle = VfxLifecycle::default();
        } else {
            inst.lifecycle.restart(None);
            inst.rebuild = true;
        }
        true
    }

    /// Whether `instance` exists and has not finished.
    pub fn is_alive(&self, instance: Entity) -> bool {
        self.instances
            .get(instance)
            .is_ok_and(|i| i.lifecycle.is_alive())
    }

    /// The instance's state, or `None` if it is not an instance.
    pub fn state(&self, instance: Entity) -> Option<VfxState> {
        self.instances.get(instance).ok().map(|i| i.state())
    }
}

/// Whether a texture can be used: loaded, failed (used without), or not
/// managed by the asset server (added directly).
fn texture_settled(server: &AssetServer, texture: &Handle<Image>) -> bool {
    match server.get_load_state(texture.id()) {
        None | Some(LoadState::Loaded) | Some(LoadState::Failed(_)) => true,
        Some(LoadState::NotLoaded) | Some(LoadState::Loading) => false,
    }
}

/// Spawns the layers of instances whose package has loaded, and rebuilds the
/// layers of instances whose package changed.
pub(crate) fn start_instances(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    packages: Res<Assets<VfxPackage>>,
    mut instances: Query<(Entity, &mut VfxInstance)>,
    surfaces: Query<(&Name, &SurfaceLayerState)>,
    shapes: Query<(&Name, &ShapeLayerState)>,
) {
    for (entity, mut instance) in instances.iter_mut() {
        let rebuilding = instance.rebuild && instance.state() == VfxState::Running;
        if instance.state() != VfxState::Starting && !rebuilding {
            instance.rebuild = false;
            continue;
        }
        let id = instance.package.id();
        if asset_server.load_state(id).is_failed() {
            warn!("VFX package {:?} failed to load; instance {} finishes", instance.package.path(), entity);
            instance.rebuild = false;
            let linger = instance.linger;
            instance.lifecycle.stop(linger);
            continue;
        }
        let Some(package) = packages.get(id) else {
            continue;
        };
        // Packages added directly to `Assets` (not loaded by the server) have
        // no dependency state; their textures are checked one by one.
        match asset_server.get_recursive_dependency_load_state(id) {
            None | Some(RecursiveDependencyLoadState::Loaded) => {}
            Some(RecursiveDependencyLoadState::Failed(error)) => {
                warn!("VFX package {:?}: a texture failed to load: {}", instance.package.path(), error);
            }
            Some(_) => continue,
        }
        if !package.textures().all(|t| texture_settled(&asset_server, t)) {
            continue;
        }

        // Layers of the previous build fade out.
        let old_linger = instance.linger;
        let old: Vec<Entity> = instance.layers.drain(..).collect();
        instance.retired.extend(old.into_iter().map(|e| (e, old_linger)));

        let mut layers = Vec::with_capacity(package.particle_layers.len());
        for (index, layer) in package.particle_layers.iter().enumerate() {
            let mut e = commands.spawn((
                Name::new(layer.name.clone()),
                ParticleEffect::new(layer.effect.clone()),
                VfxLayer {
                    instance: entity,
                    index,
                },
                VfxLayerState::default(),
                ChildOf(entity),
            ));
            if let Some(texture) = &layer.texture {
                e.insert(EffectMaterial {
                    images: vec![texture.clone()],
                });
            }
            if !layer.properties.is_empty() {
                e.insert(EffectProperties::default());
            }
            if layer.attach != AttachDef::Target {
                e.insert(SocketAttach::new(&layer.attach, &package.project));
            }
            layers.push(e.id());
        }
        for (layer, &e) in package.particle_layers.iter().zip(layers.iter()) {
            if let Some(parent) = layer.parent {
                commands.entity(e).insert(EffectParent::new(layers[parent]));
            }
        }
        instance.layers = layers;

        // Surface layers are replaced at once (their overlays go with them);
        // a layer of the same name keeps its fade level, so a rebuild does
        // not blink.
        let old_surfaces: Vec<Entity> = instance.surfaces.drain(..).collect();
        let mut surface_layers = Vec::with_capacity(package.surface_layers.len());
        for (index, layer) in package.surface_layers.iter().enumerate() {
            let level = old_surfaces
                .iter()
                .filter_map(|&e| surfaces.get(e).ok())
                .find(|(name, _)| name.as_str() == layer.name())
                .map(|(_, state)| state.level)
                .unwrap_or(0.0);
            surface_layers.push(
                commands
                    .spawn((
                        Name::new(layer.name().to_string()),
                        VfxSurfaceLayer { instance: entity, index },
                        SurfaceLayerState::with_level(level),
                        Transform::default(),
                        Visibility::default(),
                        ChildOf(entity),
                    ))
                    .id(),
            );
        }
        for e in old_surfaces {
            commands.entity(e).try_despawn();
        }
        instance.surfaces = surface_layers;

        // Shape layers too; a looping shape of the same name keeps its fade
        // level (one-shots start over, like particle bursts).
        let old_shapes: Vec<Entity> = instance.shapes.drain(..).collect();
        let mut shape_layers = Vec::with_capacity(package.shape_layers.len());
        for (index, layer) in package.shape_layers.iter().enumerate() {
            let level = old_shapes
                .iter()
                .filter_map(|&e| shapes.get(e).ok())
                .find(|(name, _)| name.as_str() == layer.name())
                .map(|(_, state)| state.player.level)
                .unwrap_or(0.0);
            shape_layers.push(spawn_shape_layer(&mut commands, entity, index, layer, &package.project, level));
        }
        for e in old_shapes {
            commands.entity(e).try_despawn();
        }
        instance.shapes = shape_layers;
        instance.linger = package.stop_duration();
        instance.rebuild = false;
        if rebuilding {
            instance.lifecycle.restart(package.auto_finish);
        } else {
            instance.lifecycle.started_for(package.auto_finish);
        }
    }
}

/// Marks running instances for a rebuild when their package asset changes
/// (hot reload of the package file); their parameter values are kept. The
/// package's textures get its (possibly edited) settings: Bevy keeps an
/// image loaded with the settings of its first load.
pub(crate) fn reload_instances(
    mut events: MessageReader<AssetEvent<VfxPackage>>,
    packages: Res<Assets<VfxPackage>>,
    images: Option<ResMut<Assets<Image>>>,
    mut instances: Query<&mut VfxInstance>,
) {
    let modified: Vec<AssetId<VfxPackage>> = events
        .read()
        .filter_map(|e| match e {
            AssetEvent::Modified { id } => Some(*id),
            _ => None,
        })
        .collect();
    if modified.is_empty() {
        return;
    }
    if let Some(mut images) = images {
        for package in modified.iter().filter_map(|id| packages.get(*id)) {
            for layer in package.particle_layers.iter() {
                let Some(handle) = &layer.texture else { continue };
                let def = package
                    .project
                    .layers
                    .get(layer.layer_index)
                    .and_then(|l| l.as_particles())
                    .and_then(|l| l.texture.as_ref())
                    .and_then(|id| package.project.texture(id));
                if let Some(def) = def {
                    apply_texture_settings_to(&mut images, handle, def);
                }
            }
            for (material, textures) in package.material_textures() {
                let slots = [
                    material.noise_a.as_ref().map(|n| &n.texture),
                    material.noise_b.as_ref().map(|n| &n.texture),
                    material.mask.as_ref().map(|m| &m.texture),
                ];
                for (handle, slot) in textures.iter().zip(slots) {
                    let (Some(handle), Some(slot)) = (handle, slot) else { continue };
                    if let Some(def) = package.project.texture(&slot.asset) {
                        apply_texture_settings_to(&mut images, handle, def);
                    }
                }
            }
        }
    }
    for mut instance in instances.iter_mut() {
        if modified.contains(&instance.package.id()) {
            instance.params.resync = true;
            if instance.state() == VfxState::Running {
                instance.rebuild = true;
            }
        }
    }
}

/// Advances instances; despawns finished ones (sending [`VfxFinished`]) and
/// retired layers whose particles have died.
pub(crate) fn finish_instances(
    mut commands: Commands,
    time: Res<Time>,
    mut instances: Query<(Entity, &mut VfxInstance)>,
    mut finished: MessageWriter<VfxFinished>,
) {
    let dt = time.delta_secs();
    for (entity, mut instance) in instances.iter_mut() {
        instance.retired.retain_mut(|(layer, remaining)| {
            *remaining -= dt;
            if *remaining <= 0.0 {
                commands.entity(*layer).try_despawn();
                return false;
            }
            true
        });
        if instance.lifecycle.tick(dt) {
            finished.write(VfxFinished {
                instance: entity,
                target: instance.target,
            });
            commands.entity(entity).try_despawn();
        }
    }
}

/// When an instance entity goes away: despawns its layers (layers attached
/// to sockets are not its children) and sends [`VfxFinished`] if the instance
/// did not finish normally (target despawned, despawned by other code).
pub(crate) fn on_instance_removed(
    removed: On<Remove, VfxInstance>,
    mut commands: Commands,
    instances: Query<&VfxInstance>,
    mut finished: MessageWriter<VfxFinished>,
) {
    let entity = removed.entity;
    let Ok(instance) = instances.get(entity) else {
        return;
    };
    for layer in instance
        .layers
        .iter()
        .copied()
        .chain(instance.surfaces.iter().copied())
        .chain(instance.shapes.iter().copied())
        .chain(instance.retired_layers())
    {
        commands.entity(layer).try_despawn();
    }
    if instance.lifecycle.is_alive() {
        finished.write(VfxFinished {
            instance: entity,
            target: instance.target,
        });
    }
}

/// When a layer entity goes away while its instance runs (e.g. the socket
/// node it was attached to was despawned with a weapon), the instance
/// rebuilds its layers.
pub(crate) fn on_layer_removed(
    removed: On<Remove, VfxLayer>,
    layers: Query<&VfxLayer>,
    mut instances: Query<&mut VfxInstance>,
) {
    let entity = removed.entity;
    let Ok(layer) = layers.get(entity) else {
        return;
    };
    let Ok(mut instance) = instances.get_mut(layer.instance) else {
        return;
    };
    if instance.state() == VfxState::Running && instance.layers.contains(&entity) {
        instance.layers.retain(|&e| e != entity);
        instance.rebuild = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stop_finishes_after_linger() {
        let mut l = VfxLifecycle::default();
        assert_eq!(l.state(), VfxState::Starting);
        assert!(!l.tick(10.0), "starting instances do not time out");
        l.started();
        assert_eq!(l.state(), VfxState::Running);
        assert!(!l.tick(10.0));
        l.stop(1.0);
        assert!(l.is_stopping());
        assert!(!l.tick(0.6));
        assert!(l.tick(0.6));
        assert_eq!(l.state(), VfxState::Finished);
        assert!(!l.is_alive());
        assert!(!l.tick(1.0), "finishes only once");
    }

    #[test]
    fn stop_twice_keeps_first_deadline() {
        let mut l = VfxLifecycle::default();
        l.started();
        l.stop(1.0);
        l.tick(0.5);
        l.stop(1.0);
        assert!(l.tick(0.5));
    }

    #[test]
    fn stop_while_starting_finishes_next_tick() {
        let mut l = VfxLifecycle::default();
        l.stop(5.0);
        l.started();
        assert!(l.is_stopping(), "a stopped instance never starts");
        assert!(l.tick(0.0));
    }

    #[test]
    fn restart_while_stopping_runs_again() {
        let mut l = VfxLifecycle::default();
        l.started();
        l.stop(1.0);
        assert!(!l.tick(0.5));
        assert!(l.restart(None));
        assert_eq!(l.state(), VfxState::Running);
        assert!(!l.tick(10.0), "runs until stopped again");
        l.stop(1.0);
        assert!(l.tick(1.0));
        assert!(!l.restart(None), "finished instances cannot restart");
    }

    #[test]
    fn one_shot_instances_finish_by_themselves() {
        let mut l = VfxLifecycle::default();
        l.started_for(Some(2.0));
        assert!(!l.tick(1.5));
        assert_eq!(l.auto_finish_remaining(), Some(0.5));
        assert!(l.tick(0.6));
        assert_eq!(l.state(), VfxState::Finished);

        // Restarting during the stop resets the time.
        let mut l = VfxLifecycle::default();
        l.started_for(Some(2.0));
        l.tick(1.5);
        l.stop(1.0);
        l.restart(Some(2.0));
        assert!(!l.tick(1.5));
        assert!(l.tick(0.6));
    }
}
