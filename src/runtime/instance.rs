//! VFX instances: one playing copy of a package attached to a target entity.
//!
//! ```ignore
//! let package: Handle<VfxPackage> = asset_server.load("vfx/poison.vfx.ron");
//! let instance = commands.spawn_vfx(&package, target);
//!
//! fn later(mut vfx: VfxControl) {
//!     vfx.stop(instance); // graceful: no new particles, despawn when all died
//! }
//! // then a `VfxFinished { instance, target }` message is sent once.
//! ```

use bevy::asset::RecursiveDependencyLoadState;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_hanabi::prelude::*;

use super::package::VfxPackage;

/// Sent once when an instance has stopped and all its particles have died,
/// right before the instance entity is despawned.
///
/// Not sent when the instance is despawned with its target.
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
/// finishes on the next tick without ever spawning layers.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct VfxLifecycle {
    state: VfxState,
}

impl Default for VfxLifecycle {
    fn default() -> Self {
        VfxLifecycle {
            state: VfxState::Starting,
        }
    }
}

impl VfxLifecycle {
    pub fn state(&self) -> VfxState {
        self.state
    }

    /// The layers were spawned.
    pub fn started(&mut self) {
        if self.state == VfxState::Starting {
            self.state = VfxState::Running;
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

    /// Advances time; returns `true` on the tick the instance finishes.
    pub fn tick(&mut self, dt: f32) -> bool {
        if let VfxState::Stopping { remaining } = self.state {
            let remaining = remaining - dt;
            if remaining <= 0.0 {
                self.state = VfxState::Finished;
                return true;
            }
            self.state = VfxState::Stopping { remaining };
        }
        false
    }

    pub fn is_alive(&self) -> bool {
        self.state != VfxState::Finished
    }

    pub fn is_stopping(&self) -> bool {
        matches!(self.state, VfxState::Stopping { .. })
    }
}

/// An instance of a [`VfxPackage`]: a child of its target, with one child
/// entity per layer. Spawn it with [`VfxCommandsExt::spawn_vfx`], control it
/// with [`VfxControl`].
#[derive(Component)]
#[require(Transform, Visibility)]
pub struct VfxInstance {
    package: Handle<VfxPackage>,
    target: Entity,
    lifecycle: VfxLifecycle,
    layers: Vec<Entity>,
}

impl VfxInstance {
    pub fn new(package: Handle<VfxPackage>, target: Entity) -> Self {
        VfxInstance {
            package,
            target,
            lifecycle: VfxLifecycle::default(),
            layers: Vec::new(),
        }
    }

    pub fn package(&self) -> &Handle<VfxPackage> {
        &self.package
    }

    pub fn target(&self) -> Entity {
        self.target
    }

    pub fn state(&self) -> VfxState {
        self.lifecycle.state()
    }

    /// Layer entities, in the order of the package's particle layers. Empty
    /// until the package has loaded.
    pub fn layers(&self) -> &[Entity] {
        &self.layers
    }
}

/// Spawning instances from [`Commands`].
pub trait VfxCommandsExt {
    /// Spawns an instance of `package` attached to `target` (as its child)
    /// and returns the instance entity. The layers appear once the package
    /// and its textures have loaded.
    fn spawn_vfx(&mut self, package: &Handle<VfxPackage>, target: Entity) -> Entity;
}

impl VfxCommandsExt for Commands<'_, '_> {
    fn spawn_vfx(&mut self, package: &Handle<VfxPackage>, target: Entity) -> Entity {
        self.spawn((
            Name::new("vfx"),
            VfxInstance::new(package.clone(), target),
            ChildOf(target),
        ))
        .id()
    }
}

/// Controls running instances.
#[derive(SystemParam)]
pub struct VfxControl<'w, 's> {
    instances: Query<'w, 's, &'static mut VfxInstance>,
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
        let linger = self
            .packages
            .get(&inst.package)
            .map(|p| p.stop_duration())
            .unwrap_or(0.0);
        inst.lifecycle.stop(linger);
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

/// Spawns the layers of instances whose package has loaded.
pub(crate) fn start_instances(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    packages: Res<Assets<VfxPackage>>,
    mut instances: Query<(Entity, &mut VfxInstance)>,
) {
    for (entity, mut instance) in instances.iter_mut() {
        if instance.state() != VfxState::Starting {
            continue;
        }
        let id = instance.package.id();
        if asset_server.load_state(id).is_failed() {
            warn!("VFX package {:?} failed to load; instance {} finishes", instance.package.path(), entity);
            instance.lifecycle.stop(0.0);
            continue;
        }
        let Some(package) = packages.get(id) else {
            continue;
        };
        // Packages added directly to `Assets` (not loaded by the server) are
        // ready as soon as they exist.
        match asset_server.get_recursive_dependency_load_state(id) {
            None | Some(RecursiveDependencyLoadState::Loaded) => {}
            Some(RecursiveDependencyLoadState::Failed(error)) => {
                warn!("VFX package {:?}: a texture failed to load: {}", instance.package.path(), error);
            }
            Some(_) => continue,
        }

        let mut layers = Vec::with_capacity(package.particle_layers.len());
        for layer in package.particle_layers.iter() {
            let mut e = commands.spawn((
                Name::new(layer.name.clone()),
                ParticleEffect::new(layer.effect.clone()),
                ChildOf(entity),
            ));
            if let Some(texture) = &layer.texture {
                e.insert(EffectMaterial {
                    images: vec![texture.clone()],
                });
            }
            layers.push(e.id());
        }
        for (layer, &e) in package.particle_layers.iter().zip(layers.iter()) {
            if let Some(parent) = layer.parent {
                commands.entity(e).insert(EffectParent::new(layers[parent]));
            }
        }
        instance.layers = layers;
        instance.lifecycle.started();
    }
}

/// Keeps the spawners of stopping instances inactive (bevy_hanabi inserts the
/// spawner component on the first tick, so this runs every frame).
pub(crate) fn stop_layer_spawners(
    instances: Query<&VfxInstance>,
    mut spawners: Query<&mut EffectSpawner>,
) {
    for instance in instances.iter() {
        if !instance.lifecycle.is_stopping() {
            continue;
        }
        for &layer in instance.layers.iter() {
            if let Ok(mut spawner) = spawners.get_mut(layer) {
                if spawner.active {
                    spawner.active = false;
                }
            }
        }
    }
}

/// Advances stopping instances; despawns finished ones and sends
/// [`VfxFinished`].
pub(crate) fn finish_instances(
    mut commands: Commands,
    time: Res<Time>,
    mut instances: Query<(Entity, &mut VfxInstance)>,
    mut finished: MessageWriter<VfxFinished>,
) {
    let dt = time.delta_secs();
    for (entity, mut instance) in instances.iter_mut() {
        if instance.lifecycle.tick(dt) {
            finished.write(VfxFinished {
                instance: entity,
                target: instance.target,
            });
            commands.entity(entity).try_despawn();
        }
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
}
