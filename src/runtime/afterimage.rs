//! Afterimage layers at runtime: fading copies ("ghosts") of the target's
//! own meshes left along their path.
//!
//! - **Emission** ([`update_afterimages`], `Update`): the same modes as
//!   trails ([`Emitter`]). Ghosts age, fade (`fade_over_life` × the material
//!   at the ghost's age × the lifecycle fade × visibility) and are despawned
//!   at the end of their `lifetime`.
//! - **Snapshots** ([`snapshot_afterimages`], `PostUpdate` after transform
//!   propagation, so the copy shows this frame's pose): while emitting,
//!   every `interval` seconds the matched meshes are copied as root entities
//!   in the world: the same `Mesh3d` at the mesh's current `GlobalTransform`
//!   with one [`OmagariEffectMaterial`] per ghost. **Skinned meshes** get
//!   frozen joints: one entity per joint holding only a copy of the joint's
//!   `GlobalTransform`, and a `SkinnedMesh` pointing at them, so the copy
//!   keeps the pose (the effect vertex shader skins it). Meshes of one
//!   snapshot share the frozen joints. At most `max_ghosts` ghosts; the
//!   oldest goes first.
//! - Ghosts are not children of the target (they stay where they were left);
//!   they go with their layer ([`on_afterimage_layer_removed`]).

use std::collections::VecDeque;

use bevy::camera::visibility::NoFrustumCulling;
use bevy::gltf::GltfMeshName;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::mesh::morph::MeshMorphWeights;
use bevy::mesh::skinning::{SkinnedMesh, SkinnedMeshInverseBindposes};
use bevy::platform::collections::HashMap;
use bevy::prelude::*;

use super::attach::{mesh_bounds, TargetHierarchy};
use super::effect_material::{alpha_mode_for, EffectUniforms, OmagariEffectMaterial, ShellInputs};
use super::instance::{VfxInstance, VfxState};
use super::package::{PreparedAfterimageLayer, VfxPackage};
use super::surface::skin_rest_scale;
use super::trail::{write_material, Emitter};
use crate::format::{AfterimageLayerDef, ParamValue};

/// An afterimage layer entity of an instance (a child of the instance).
#[derive(Component, Clone, Copy, Debug)]
pub struct VfxAfterimageLayer {
    pub instance: Entity,
    /// Index into [`VfxPackage::afterimage_layers`].
    pub index: usize,
}

/// A ghost mesh or frozen joint entity of an afterimage layer.
#[derive(Component, Clone, Copy, Debug)]
pub struct VfxGhost {
    pub layer: Entity,
}

/// One copy of the target's meshes.
#[derive(Clone, Debug)]
pub struct Ghost {
    /// Mesh entities.
    pub meshes: Vec<Entity>,
    /// Frozen joint entities (skinned meshes).
    pub joints: Vec<Entity>,
    pub material: Handle<OmagariEffectMaterial>,
    /// Seconds since it was left.
    pub age: f32,
    /// Rest-pose bounds of its meshes (fill), metres per rest unit (skins).
    rest_bounds: Option<(Vec3, Vec3)>,
    rest_scale: f32,
    hidden: bool,
}

/// When a snapshot is due, as a plain state machine.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct GhostClock {
    since_last: f32,
}

impl GhostClock {
    /// Advances by `dt` while emitting; returns whether a copy is due (right
    /// when emission starts, then every `interval`).
    pub fn step(&mut self, emitting: bool, started: bool, dt: f32, interval: f32) -> bool {
        if !emitting {
            self.since_last = 0.0;
            return false;
        }
        if started {
            self.since_last = 0.0;
            return true;
        }
        self.since_last += dt;
        let interval = interval.max(1e-3);
        if self.since_last + 1e-6 >= interval {
            // At most one copy per frame; the rest of the time carries over.
            self.since_last = (self.since_last - interval).min(interval);
            return true;
        }
        false
    }
}

/// Runtime state of an afterimage layer.
#[derive(Component, Clone, Debug, Default)]
pub struct AfterimageLayerState {
    pub emitter: Emitter,
    pub clock: GhostClock,
    /// Live ghosts, oldest first.
    pub ghosts: VecDeque<Ghost>,
    pub level: f32,
    pub visibility: f32,
    /// A copy is due in the next `PostUpdate`.
    pub(crate) due: bool,
    pub(crate) pending: Vec<String>,
    generation: u32,
    started: bool,
    start_level: f32,
}

impl AfterimageLayerState {
    pub fn with_level(level: f32) -> Self {
        AfterimageLayerState { visibility: 1.0, level, start_level: level, ..default() }
    }

    /// Every entity of the layer's ghosts (meshes and frozen joints).
    pub fn ghost_entities(&self) -> impl Iterator<Item = Entity> + '_ {
        self.ghosts.iter().flat_map(|g| g.meshes.iter().chain(g.joints.iter()).copied())
    }
}

/// Spawns the entity of one afterimage layer of an instance.
pub(crate) fn spawn_afterimage_layer(commands: &mut Commands, instance: Entity, index: usize, name: &str, level: f32) -> Entity {
    commands
        .spawn((
            Name::new(name.to_string()),
            VfxAfterimageLayer { instance, index },
            AfterimageLayerState::with_level(level),
            Transform::default(),
            Visibility::default(),
            ChildOf(instance),
        ))
        .id()
}

fn fit_of(instance: &GlobalTransform) -> f32 {
    let s = instance.scale().abs();
    ((s.x + s.y + s.z) / 3.0).max(1e-4)
}

/// The uniforms of a ghost `age` seconds old.
fn ghost_uniforms(
    prepared: &PreparedAfterimageLayer,
    ghost: &Ghost,
    level: f32,
    visibility: f32,
    lookup: &dyn Fn(&str) -> Option<ParamValue>,
    fit: f32,
) -> EffectUniforms {
    let def = &prepared.def;
    let values = def.material.evaluate_at(lookup, ghost.age);
    let inputs = ShellInputs {
        offset: 0.0,
        fade: def.fade_at(ghost.age) * level * visibility,
        noise_offset: 0.0,
        fit_scale: fit,
        rest_scale: ghost.rest_scale,
        rest_bounds: ghost.rest_bounds,
        billboard: false,
    };
    let textures = [prepared.textures[0].is_some(), prepared.textures[1].is_some(), prepared.textures[2].is_some()];
    EffectUniforms::new(&values, &inputs, textures)
}

fn despawn_ghost(commands: &mut Commands, ghost: &Ghost) {
    for &e in ghost.meshes.iter().chain(ghost.joints.iter()) {
        commands.entity(e).try_despawn();
    }
}

/// Emission, ageing, fades and materials of afterimage layers.
#[allow(clippy::too_many_arguments)]
pub(crate) fn update_afterimages(
    mut commands: Commands,
    time: Res<Time>,
    packages: Res<Assets<VfxPackage>>,
    materials: Option<ResMut<Assets<OmagariEffectMaterial>>>,
    instances: Query<(&VfxInstance, &GlobalTransform)>,
    mut layers: Query<(&VfxAfterimageLayer, &mut AfterimageLayerState)>,
    mut visibilities: Query<&mut Visibility, With<VfxGhost>>,
) {
    let Some(mut materials) = materials else { return };
    let dt = time.delta_secs();
    for (layer, mut state) in layers.iter_mut() {
        let state = &mut *state;
        let Ok((instance, instance_global)) = instances.get(layer.instance) else { continue };
        let Some(prepared) = packages.get(&instance.package).and_then(|p| p.afterimage_layers.get(layer.index)) else {
            continue;
        };
        if !instance.params.is_synced(instance.package.id()) {
            continue;
        }
        let def = &prepared.def;
        if !state.started || state.generation != instance.generation {
            state.emitter.reset();
            state.clock = GhostClock::default();
            state.started = true;
            state.generation = instance.generation;
            state.level = state.start_level;
        }
        let running = matches!(instance.state(), VfxState::Running | VfxState::Starting);
        for name in std::mem::take(&mut state.pending) {
            if running {
                state.emitter.trigger(&def.emit, &name);
            }
        }
        let while_value = def.emit.param().and_then(|p| instance.params.float(p));
        let (started, _) = state.emitter.step(&def.emit, dt, running, while_value);
        state.due |= state.clock.step(state.emitter.on, started, dt, def.interval);
        state.level = def.fade.step(state.level, running, dt);
        state.visibility = match &def.visible {
            Some(visible) => instance.params.float(&visible.param).map(|v| visible.factor(v)).unwrap_or(1.0),
            None => 1.0,
        };

        // Age the ghosts; the ones past their lifetime go.
        for ghost in state.ghosts.iter_mut() {
            ghost.age += dt;
        }
        while state.ghosts.front().is_some_and(|g| g.age >= def.lifetime) {
            if let Some(ghost) = state.ghosts.pop_front() {
                despawn_ghost(&mut commands, &ghost);
            }
        }
        let lookup = |name: &str| -> Option<ParamValue> { instance.params.get(name) };
        let fit = fit_of(instance_global);
        let alpha_mode = alpha_mode_for(def.material.blend);
        let (level, visibility) = (state.level, state.visibility);
        for ghost in state.ghosts.iter_mut() {
            let uniforms = ghost_uniforms(prepared, ghost, level, visibility, &lookup, fit);
            write_material(&mut materials, &ghost.material, uniforms, alpha_mode, def.sort_bias);
            let hidden = uniforms.params.w <= 0.0;
            if hidden != ghost.hidden {
                ghost.hidden = hidden;
                for &e in ghost.meshes.iter() {
                    if let Ok(mut v) = visibilities.get_mut(e) {
                        *v = if hidden { Visibility::Hidden } else { Visibility::Visible };
                    }
                }
            }
        }
    }
}

type GhostSource<'a> = (
    &'a Mesh3d,
    &'a GlobalTransform,
    Option<&'a Name>,
    Option<&'a GltfMeshName>,
    Option<&'a SkinnedMesh>,
    Option<&'a MeshMorphWeights>,
);

/// The mesh entities an afterimage layer copies: the target's own meshes
/// that match its source.
pub fn ghost_sources(
    hierarchy: &TargetHierarchy,
    target: Entity,
    def: &AfterimageLayerDef,
    names: impl Fn(Entity) -> (Option<String>, Option<String>),
) -> Vec<Entity> {
    let mut meshes: Vec<Entity> = hierarchy
        .own_meshes(target)
        .into_iter()
        .filter(|&e| {
            let (entity, mesh) = names(e);
            def.source.matches(entity.as_deref(), mesh.as_deref())
        })
        .collect();
    meshes.sort();
    meshes
}

/// Leaves a copy of the matched meshes where they are now (after transform
/// propagation), when one is due.
#[allow(clippy::too_many_arguments)]
pub(crate) fn snapshot_afterimages(
    mut commands: Commands,
    packages: Res<Assets<VfxPackage>>,
    materials: Option<ResMut<Assets<OmagariEffectMaterial>>>,
    meshes: Option<Res<Assets<Mesh>>>,
    bindposes: Option<Res<Assets<SkinnedMeshInverseBindposes>>>,
    hierarchy: TargetHierarchy,
    instances: Query<(&VfxInstance, &GlobalTransform)>,
    sources: Query<GhostSource, Without<VfxGhost>>,
    globals: Query<&GlobalTransform>,
    mut layers: Query<(Entity, &VfxAfterimageLayer, &mut AfterimageLayerState)>,
) {
    let Some(mut materials) = materials else { return };
    for (layer_entity, layer, mut state) in layers.iter_mut() {
        if !state.due {
            continue;
        }
        state.due = false;
        let state = &mut *state;
        let Ok((instance, instance_global)) = instances.get(layer.instance) else { continue };
        let Some(prepared) = packages.get(&instance.package).and_then(|p| p.afterimage_layers.get(layer.index)) else {
            continue;
        };
        let def = &prepared.def;
        if def.lifetime <= 0.0 {
            continue;
        }
        let wanted = ghost_sources(&hierarchy, instance.target(), def, |e| match sources.get(e) {
            Ok((_, _, name, mesh, ..)) => (name.map(|n| n.to_string()), mesh.map(|m| m.0.clone())),
            Err(_) => (None, None),
        });
        if wanted.is_empty() {
            continue;
        }
        while state.ghosts.len() >= def.ghost_cap() {
            if let Some(old) = state.ghosts.pop_front() {
                despawn_ghost(&mut commands, &old);
            }
        }

        let mut ghost = Ghost {
            meshes: Vec::new(),
            joints: Vec::new(),
            material: materials.add(OmagariEffectMaterial {
                uniforms: EffectUniforms::default(),
                noise_a: prepared.textures[0].clone(),
                noise_b: prepared.textures[1].clone(),
                mask: prepared.textures[2].clone(),
                alpha_mode: alpha_mode_for(def.material.blend),
                sort_bias: def.sort_bias,
            }),
            age: 0.0,
            rest_bounds: None,
            rest_scale: 1.0,
            hidden: false,
        };
        let mut frozen: HashMap<Entity, Entity> = HashMap::default();
        let mut rest_scale = None;
        for source in wanted {
            let Ok((mesh, global, name, _, skin, morph)) = sources.get(source) else { continue };
            if let Some((lo, hi)) = mesh_bounds(None, meshes.as_deref().and_then(|m| m.get(&mesh.0))) {
                ghost.rest_bounds = Some(match ghost.rest_bounds {
                    None => (lo, hi),
                    Some((a, b)) => (a.min(lo), b.max(hi)),
                });
            }
            let skinned = skin.map(|skin| {
                if rest_scale.is_none() {
                    rest_scale = skin_rest_scale(skin, bindposes.as_deref(), &globals);
                }
                let joints: Vec<Entity> = skin
                    .joints
                    .iter()
                    .map(|&joint| {
                        *frozen.entry(joint).or_insert_with(|| {
                            let pose = globals.get(joint).copied().unwrap_or_default();
                            let id = commands.spawn((VfxGhost { layer: layer_entity }, pose)).id();
                            ghost.joints.push(id);
                            id
                        })
                    })
                    .collect();
                SkinnedMesh { inverse_bindposes: skin.inverse_bindposes.clone(), joints }
            });
            let mut e = commands.spawn((
                Name::new(format!("{} ghost of {}", def.name, name.map(|n| n.as_str()).unwrap_or("mesh"))),
                VfxGhost { layer: layer_entity },
                Mesh3d(mesh.0.clone()),
                MeshMaterial3d(ghost.material.clone()),
                global.compute_transform(),
                *global,
                Visibility::Visible,
                NotShadowCaster,
                NotShadowReceiver,
                NoFrustumCulling,
            ));
            if let Some(skin) = skinned {
                e.insert(skin);
            }
            if let Some(morph) = morph {
                e.insert(morph.clone());
            }
            ghost.meshes.push(e.id());
        }
        ghost.rest_scale = rest_scale.unwrap_or(1.0);
        let lookup = |name: &str| -> Option<ParamValue> { instance.params.get(name) };
        let uniforms = ghost_uniforms(prepared, &ghost, state.level, state.visibility, &lookup, fit_of(instance_global));
        write_material(&mut materials, &ghost.material, uniforms, alpha_mode_for(def.material.blend), def.sort_bias);
        state.ghosts.push_back(ghost);
    }
}

/// When an afterimage layer goes away, its ghosts go too (they are not its
/// children).
pub(crate) fn on_afterimage_layer_removed(
    removed: On<Remove, VfxAfterimageLayer>,
    mut commands: Commands,
    states: Query<&AfterimageLayerState>,
) {
    let Ok(state) = states.get(removed.entity) else { return };
    for e in state.ghost_entities() {
        commands.entity(e).try_despawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_come_at_the_start_and_every_interval() {
        let mut clock = GhostClock::default();
        assert!(!clock.step(false, false, 0.1, 0.05));
        assert!(clock.step(true, true, 0.016, 0.05), "right when emission starts");
        let mut copies = 1;
        for _ in 0..60 {
            if clock.step(true, false, 0.01, 0.05) {
                copies += 1;
            }
        }
        // 0.6 s at one copy per 0.05 s: 12 more.
        assert_eq!(copies, 13);
        // Frames longer than the interval: one copy per frame.
        let mut clock = GhostClock::default();
        clock.step(true, true, 0.0, 0.01);
        assert!(clock.step(true, false, 0.1, 0.01));
        assert!(clock.step(true, false, 0.001, 0.01), "the rest carries over");
        assert!(!clock.step(false, false, 0.1, 0.01));
    }
}
