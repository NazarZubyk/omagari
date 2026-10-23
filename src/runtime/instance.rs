//! VFX instances: one playing copy of a package attached to a target entity.
//!
//! ```ignore
//! let package: Handle<VfxPackage> = asset_server.load("vfx/poison.vfx.ron");
//! let instance = commands.spawn_vfx(&package, target);
//! let burst = commands.spawn_vfx_at(&package, Transform::from_xyz(1.0, 0.0, 2.0));
//!
//! fn later(mut vfx: VfxControl) {
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
use super::package::VfxPackage;
use crate::format::AttachDef;

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
}

impl VfxInstance {
    pub fn new(package: Handle<VfxPackage>, target: Entity) -> Self {
        VfxInstance {
            package,
            target,
            lifecycle: VfxLifecycle::default(),
            layers: Vec::new(),
            retired: Vec::new(),
            rebuild: false,
            linger: 0.0,
            in_world: false,
        }
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

/// Spawning instances from [`Commands`].
pub trait VfxCommandsExt {
    /// Spawns an instance of `package` attached to `target` (as its child)
    /// and returns the instance entity. The layers appear once the package
    /// and its textures have loaded.
    fn spawn_vfx(&mut self, package: &Handle<VfxPackage>, target: Entity) -> Entity;

    /// Spawns an instance of `package` in the world at `transform`, with no
    /// target (a one-shot hit, an explosion). It is cleaned up when it
    /// finishes: by itself when every layer is one-shot, else when stopped.
    fn spawn_vfx_at(&mut self, package: &Handle<VfxPackage>, transform: Transform) -> Entity;
}

impl VfxCommandsExt for Commands<'_, '_> {
    fn spawn_vfx(&mut self, package: &Handle<VfxPackage>, target: Entity) -> Entity {
        self.spawn((
            Name::new("vfx"),
            VfxInstance::new(package.clone(), target),
            FitState::default(),
            ChildOf(target),
        ))
        .id()
    }

    fn spawn_vfx_at(&mut self, package: &Handle<VfxPackage>, transform: Transform) -> Entity {
        let instance = self.spawn_empty().id();
        let mut component = VfxInstance::new(package.clone(), instance);
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
        if inst.layers.is_empty() {
            // Stopped before it started: start over.
            inst.lifecycle = VfxLifecycle::default();
            return true;
        }
        let auto_finish = self.packages.get(&inst.package).and_then(|p| p.auto_finish);
        inst.lifecycle.restart(auto_finish);
        for &layer in inst.layers.iter() {
            if let Ok(mut spawner) = self.spawners.get_mut(layer) {
                spawner.reset();
                spawner.active = true;
            }
        }
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
        if inst.layers.is_empty() {
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
        if !package
            .particle_layers
            .iter()
            .filter_map(|l| l.texture.as_ref())
            .all(|t| texture_settled(&asset_server, t))
        {
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
/// (hot reload of the package file).
pub(crate) fn reload_instances(
    mut events: MessageReader<AssetEvent<VfxPackage>>,
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
    for mut instance in instances.iter_mut() {
        if instance.state() == VfxState::Running && modified.contains(&instance.package.id()) {
            instance.rebuild = true;
        }
    }
}

/// Keeps the spawners of stopping instances and of retired layers inactive
/// (bevy_hanabi inserts the spawner component on the first tick, so this
/// runs every frame).
pub(crate) fn stop_layer_spawners(
    instances: Query<&VfxInstance>,
    mut spawners: Query<&mut EffectSpawner>,
) {
    for instance in instances.iter() {
        let stopping = instance.lifecycle.is_stopping();
        let layers = instance.layers.iter().filter(|_| stopping);
        for &layer in layers.chain(instance.retired.iter().map(|(e, _)| e)) {
            if let Ok(mut spawner) = spawners.get_mut(layer) {
                if spawner.active {
                    spawner.active = false;
                }
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
    for layer in instance.layers.iter().copied().chain(instance.retired_layers()) {
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
