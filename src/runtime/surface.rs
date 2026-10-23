//! Surface layers at runtime: overlay copies of the target's own meshes with
//! [`OmagariEffectMaterial`].
//!
//! For every mesh of the target's own model that matches the layer's
//! `targets`, one overlay entity per shell is spawned as a **child of the
//! mesh entity** at the identity transform, with the same `Mesh3d`, the same
//! `SkinnedMesh` (the same joints, so it animates exactly like the mesh:
//! skinned vertices take their world transform from the joints, not from the
//! entity) and the same morph weights. The model and its materials are never
//! changed; removing the layer despawns the overlays. When meshes appear or
//! change under the target (a glTF scene finished loading, the body was
//! swapped), the overlays follow on the next frame.
//!
//! One material per instance, layer and shell; its uniforms are written from
//! the layer's bindings when they change (parameters, pulses, fades). Time in
//! the shader is Bevy's global shader time.

use bevy::camera::visibility::{DynamicSkinnedMeshBounds, NoFrustumCulling};
use bevy::gltf::{GltfMaterialName, GltfMeshName};
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::mesh::morph::MeshMorphWeights;
use bevy::mesh::skinning::{SkinnedMesh, SkinnedMeshInverseBindposes};
use bevy::prelude::*;

use super::attach::{mesh_bounds, TargetHierarchy};
use super::effect_material::{alpha_mode_for, EffectUniforms, OmagariEffectMaterial, ShellInputs, SHELL_SORT_STEP};
use super::instance::{VfxInstance, VfxState};
use super::package::{PreparedSurfaceLayer, VfxPackage};
use crate::format::MeshTargetsDef;

/// A surface layer entity of an instance (a child of the instance).
#[derive(Component, Clone, Copy, Debug)]
pub struct VfxSurfaceLayer {
    pub instance: Entity,
    /// Index into [`VfxPackage::surface_layers`].
    pub index: usize,
}

/// One overlay (one shell over one mesh) of a surface layer: a child of the
/// mesh entity it covers.
#[derive(Component, Clone, Copy, Debug)]
pub struct VfxSurfaceOverlay {
    pub layer: Entity,
    /// The mesh entity it copies.
    pub source: Entity,
    pub shell: usize,
}

/// Runtime state of a surface layer.
#[derive(Component, Clone, Debug, Default)]
pub struct SurfaceLayerState {
    /// Fade level, 0 (invisible) .. 1; up over `fade.in` while the instance
    /// runs, down over `fade.out` while it stops.
    pub level: f32,
    /// `visible` factor (1 without `visible`).
    pub visibility: f32,
    /// One material per shell.
    pub materials: Vec<Handle<OmagariEffectMaterial>>,
    /// Overlay entities.
    pub overlays: Vec<VfxSurfaceOverlay>,
    pub overlay_entities: Vec<Entity>,
    /// Rest-pose bounds of the covered meshes (fill mask).
    pub rest_bounds: Option<(Vec3, Vec3)>,
    /// World metres per rest-pose unit of the covered skinned meshes.
    pub rest_scale: f32,
    /// Seconds since the layer started (what `Age` bindings read; a restart
    /// sets it back to 0).
    pub age: f32,
    generation: u32,
    scanned: bool,
    hidden: bool,
}

impl SurfaceLayerState {
    /// A state starting at fade level `level` (a rebuilt layer keeps the
    /// level of the one it replaces, so live edits do not blink).
    pub fn with_level(level: f32) -> Self {
        SurfaceLayerState {
            level,
            visibility: 1.0,
            rest_scale: 1.0,
            ..default()
        }
    }

    /// The overlays: (mesh entity, shell, overlay entity).
    pub fn overlays(&self) -> impl Iterator<Item = (Entity, usize, Entity)> + '_ {
        self.overlays
            .iter()
            .zip(self.overlay_entities.iter())
            .map(|(o, e)| (o.source, o.shell, *e))
    }
}

/// The mesh entities of a target a surface layer covers: the target's own
/// meshes ([`TargetHierarchy::own_meshes`]) that match `targets`.
pub fn surface_meshes(
    hierarchy: &TargetHierarchy,
    target: Entity,
    targets: &MeshTargetsDef,
    names: impl Fn(Entity) -> (Option<String>, Option<String>, Option<String>),
) -> Vec<Entity> {
    let mut meshes: Vec<Entity> = hierarchy
        .own_meshes(target)
        .into_iter()
        .filter(|&e| {
            let (entity, mesh, material) = names(e);
            targets.matches(entity.as_deref(), mesh.as_deref(), material.as_deref())
        })
        .collect();
    meshes.sort();
    meshes
}

type SourceData<'a> = (
    &'a Mesh3d,
    Option<&'a Name>,
    Option<&'a GltfMeshName>,
    Option<&'a GltfMaterialName>,
    Option<&'a SkinnedMesh>,
    Option<&'a MeshMorphWeights>,
    Has<DynamicSkinnedMeshBounds>,
    Has<NoFrustumCulling>,
);

/// Creates the shell materials and keeps the overlays matching the target's
/// meshes.
pub(crate) fn sync_surface_overlays(
    mut commands: Commands,
    packages: Res<Assets<VfxPackage>>,
    meshes: Option<Res<Assets<Mesh>>>,
    bindposes: Option<Res<Assets<SkinnedMeshInverseBindposes>>>,
    materials: Option<ResMut<Assets<OmagariEffectMaterial>>>,
    hierarchy: TargetHierarchy,
    changed: Query<(), (Or<(Changed<Children>, Changed<Mesh3d>, Changed<SkinnedMesh>, Changed<Name>)>, Without<VfxSurfaceOverlay>)>,
    instances: Query<&VfxInstance>,
    sources: Query<SourceData, Without<VfxSurfaceOverlay>>,
    mut overlays: Query<(&mut Mesh3d, Option<&mut SkinnedMesh>), With<VfxSurfaceOverlay>>,
    globals: Query<&GlobalTransform>,
    mut layers: Query<(Entity, &VfxSurfaceLayer, &mut SurfaceLayerState)>,
) {
    let Some(mut materials) = materials else { return };
    if layers.is_empty() {
        return;
    }
    let any_change = !changed.is_empty();
    for (layer_entity, layer, mut state) in layers.iter_mut() {
        let Ok(instance) = instances.get(layer.instance) else { continue };
        let Some(package) = packages.get(&instance.package) else { continue };
        let Some(prepared) = package.surface_layers.get(layer.index) else { continue };

        // One material per shell.
        let shells = prepared.def.shells.len();
        if state.materials.len() != shells {
            state.materials = (0..shells).map(|i| materials.add(new_material(prepared, i))).collect();
            state.scanned = false;
            // Overlays point at the old materials: start them over.
            for e in state.overlay_entities.drain(..) {
                commands.entity(e).try_despawn();
            }
            state.overlays.clear();
        }
        if state.scanned && !any_change {
            continue;
        }
        state.scanned = true;

        let wanted = surface_meshes(&hierarchy, instance.target(), &prepared.def.targets, |e| match sources.get(e) {
            Ok((_, name, mesh, material, ..)) => (
                name.map(|n| n.to_string()),
                mesh.map(|m| m.0.clone()),
                material.map(|m| m.0.clone()),
            ),
            Err(_) => (None, None, None),
        });

        // Drop overlays whose mesh is gone or no longer matches (overlays
        // despawned with their mesh are gone already).
        let mut kept = Vec::new();
        for (overlay, entity) in state.overlays.iter().copied().zip(state.overlay_entities.iter().copied()) {
            let exists = overlays.contains(entity);
            if exists && wanted.contains(&overlay.source) && overlay.shell < shells {
                kept.push((overlay, entity));
            } else if exists {
                commands.entity(entity).try_despawn();
            }
        }

        // Keep the kept ones in step with their mesh (mesh or skin swapped).
        for (overlay, entity) in kept.iter() {
            let (Ok(source), Ok((mut mesh, skin))) = (sources.get(overlay.source), overlays.get_mut(*entity)) else {
                continue;
            };
            if mesh.0 != source.0 .0 {
                mesh.0 = source.0 .0.clone();
            }
            match (skin, source.4) {
                (Some(mut skin), Some(source_skin)) => {
                    if skin.joints != source_skin.joints || skin.inverse_bindposes != source_skin.inverse_bindposes {
                        *skin = source_skin.clone();
                    }
                }
                (None, Some(source_skin)) => {
                    commands.entity(*entity).insert(source_skin.clone());
                }
                (Some(_), None) => {
                    commands.entity(*entity).remove::<SkinnedMesh>();
                }
                (None, None) => {}
            }
        }

        // Spawn the missing ones.
        for &source_entity in wanted.iter() {
            let Ok((mesh, _, _, _, skin, morph, dynamic_bounds, no_culling)) = sources.get(source_entity) else {
                continue;
            };
            for shell in 0..shells {
                if kept.iter().any(|(o, _)| o.source == source_entity && o.shell == shell) {
                    continue;
                }
                let overlay = VfxSurfaceOverlay {
                    layer: layer_entity,
                    source: source_entity,
                    shell,
                };
                let mut e = commands.spawn((
                    Name::new(format!("{} (shell {})", prepared.def.name, shell)),
                    overlay,
                    Mesh3d(mesh.0.clone()),
                    MeshMaterial3d(state.materials[shell].clone()),
                    Transform::IDENTITY,
                    Visibility::Inherited,
                    NotShadowCaster,
                    NotShadowReceiver,
                    ChildOf(source_entity),
                ));
                if let Some(skin) = skin {
                    e.insert(skin.clone());
                }
                if let Some(morph) = morph {
                    e.insert(morph.clone());
                }
                if dynamic_bounds {
                    e.insert(DynamicSkinnedMeshBounds);
                }
                if no_culling {
                    e.insert(NoFrustumCulling);
                }
                kept.push((overlay, e.id()));
            }
        }
        state.overlays = kept.iter().map(|(o, _)| *o).collect();
        state.overlay_entities = kept.iter().map(|(_, e)| *e).collect();
        state.hidden = false;

        // Rest-pose bounds (fill) and metres per rest unit (patterns).
        let mut bounds: Option<(Vec3, Vec3)> = None;
        let mut rest_scale = None;
        for &source_entity in wanted.iter() {
            let Ok((mesh, _, _, _, skin, ..)) = sources.get(source_entity) else { continue };
            if let Some((lo, hi)) = mesh_bounds(None, meshes.as_deref().and_then(|m| m.get(&mesh.0))) {
                bounds = Some(match bounds {
                    None => (lo, hi),
                    Some((a, b)) => (a.min(lo), b.max(hi)),
                });
            }
            if rest_scale.is_none() {
                if let Some(skin) = skin {
                    rest_scale = skin_rest_scale(skin, bindposes.as_deref(), &globals);
                }
            }
        }
        state.rest_bounds = bounds;
        state.rest_scale = rest_scale.unwrap_or(1.0);
    }
}

/// World metres per bind-pose unit of a skin: the scale of its first
/// joint's skinning matrix (joint world transform × inverse bind pose).
fn skin_rest_scale(
    skin: &SkinnedMesh,
    bindposes: Option<&Assets<SkinnedMeshInverseBindposes>>,
    globals: &Query<&GlobalTransform>,
) -> Option<f32> {
    let joint = *skin.joints.first()?;
    let inverse = *bindposes?.get(&skin.inverse_bindposes)?.first()?;
    let joint = globals.get(joint).ok()?;
    let matrix = joint.to_matrix() * inverse;
    let scale = matrix.x_axis.truncate().length();
    (scale.is_finite() && scale > 1e-6).then_some(scale)
}

fn new_material(prepared: &PreparedSurfaceLayer, shell: usize) -> OmagariEffectMaterial {
    OmagariEffectMaterial {
        uniforms: EffectUniforms::default(),
        noise_a: prepared.textures[0].clone(),
        noise_b: prepared.textures[1].clone(),
        mask: prepared.textures[2].clone(),
        alpha_mode: alpha_mode_for(prepared.def.material.blend),
        sort_bias: shell as f32 * SHELL_SORT_STEP,
    }
}

/// The uniforms of every shell of a surface layer, from its bindings, the
/// fade level and visibility, the fit scale and the rest-pose data.
pub fn surface_uniforms(
    prepared: &PreparedSurfaceLayer,
    state: &SurfaceLayerState,
    lookup: &dyn Fn(&str) -> Option<crate::format::ParamValue>,
    fit_scale: f32,
) -> Vec<EffectUniforms> {
    let values = prepared.def.material.evaluate_at(lookup, state.age);
    let textures = [prepared.textures[0].is_some(), prepared.textures[1].is_some(), prepared.textures[2].is_some()];
    prepared
        .def
        .shells
        .iter()
        .map(|shell| {
            let inputs = ShellInputs {
                offset: shell.offset,
                fade: shell.opacity * state.level * state.visibility,
                noise_offset: shell.noise_offset,
                fit_scale,
                rest_scale: state.rest_scale,
                rest_bounds: state.rest_bounds,
                billboard: false,
            };
            EffectUniforms::new(&values, &inputs, textures)
        })
        .collect()
}

/// Fades surface layers with the instance lifecycle and writes their
/// materials' uniforms when they change; overlays of invisible layers are
/// hidden (not drawn).
pub(crate) fn update_surface_materials(
    time: Res<Time>,
    packages: Res<Assets<VfxPackage>>,
    materials: Option<ResMut<Assets<OmagariEffectMaterial>>>,
    instances: Query<(&VfxInstance, &GlobalTransform)>,
    mut layers: Query<(&VfxSurfaceLayer, &mut SurfaceLayerState)>,
    mut visibilities: Query<&mut Visibility, With<VfxSurfaceOverlay>>,
) {
    let Some(mut materials) = materials else { return };
    let dt = time.delta_secs();
    for (layer, mut state) in layers.iter_mut() {
        let Ok((instance, instance_global)) = instances.get(layer.instance) else { continue };
        let Some(package) = packages.get(&instance.package) else { continue };
        let Some(prepared) = package.surface_layers.get(layer.index) else { continue };
        if !instance.params.is_synced(instance.package.id()) {
            continue;
        }
        let running = matches!(instance.state(), VfxState::Running | VfxState::Starting);
        if state.generation != instance.generation {
            state.generation = instance.generation;
            state.age = 0.0;
        } else {
            state.age += dt;
        }
        state.level = prepared.def.fade.step(state.level, running, dt);
        state.visibility = match &prepared.def.visible {
            Some(visible) => instance.params.float(&visible.param).map(|v| visible.factor(v)).unwrap_or(1.0),
            None => 1.0,
        };

        let scale = instance_global.scale().abs();
        let fit_scale = (scale.x + scale.y + scale.z) / 3.0;
        let uniforms = surface_uniforms(prepared, &state, &|name| instance.params.get(name), fit_scale);
        let alpha_mode = alpha_mode_for(prepared.def.material.blend);
        for (handle, wanted) in state.materials.iter().zip(uniforms.iter()) {
            let same = materials
                .get(handle)
                .is_some_and(|m| m.uniforms == *wanted && m.alpha_mode == alpha_mode);
            if !same {
                if let Some(mut material) = materials.get_mut(handle) {
                    material.uniforms = *wanted;
                    material.alpha_mode = alpha_mode;
                }
            }
        }

        let hidden = state.level * state.visibility <= 0.0;
        if hidden != state.hidden {
            state.hidden = hidden;
            for &entity in state.overlay_entities.iter() {
                if let Ok(mut visibility) = visibilities.get_mut(entity) {
                    *visibility = if hidden { Visibility::Hidden } else { Visibility::Inherited };
                }
            }
        }
    }
}

/// When a surface layer goes away (instance despawned, rebuilt, finished),
/// its overlays go too: they are children of the model's meshes, not of the
/// layer.
pub(crate) fn on_surface_layer_removed(
    removed: On<Remove, VfxSurfaceLayer>,
    mut commands: Commands,
    states: Query<&SurfaceLayerState>,
) {
    let Ok(state) = states.get(removed.entity) else { return };
    for &entity in state.overlay_entities.iter() {
        commands.entity(entity).try_despawn();
    }
}
