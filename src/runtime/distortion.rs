//! Distortion layers at runtime: one entity per distortion layer and
//! instance (a child of the instance), drawn with
//! [`OmagariDistortionMaterial`].
//!
//! - **Shape sources**: the layer entity carries a generated mesh
//!   ([`shape_mesh`]) and is placed like a shape layer after transform
//!   propagation ([`place_distortions`]). With `attach: Segment` a capsule
//!   runs from one socket to the other: its mesh is rebuilt for the current
//!   length (in 5 mm steps, cached), its radius kept.
//! - **Surface sources**: overlay copies of the target's own meshes (like a
//!   surface layer's, one per mesh, children of the mesh entities, same skin)
//!   pushed `offset` metres outward in the vertex shader.
//! - **Play** ([`ShapePlayer`], shared with shapes and lights): `Loop`,
//!   `OnStart`, `OnTrigger` one-shots, `While`, `Between`; fades and
//!   `visible`. **Nothing is drawn** (entity or overlays hidden) while the
//!   strength × fade × visibility is 0, so Bevy makes no transmission copy
//!   for it.

use bevy::camera::visibility::{DynamicSkinnedMeshBounds, NoFrustumCulling};
use bevy::gltf::{GltfMaterialName, GltfMeshName};
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::mesh::morph::MeshMorphWeights;
use bevy::mesh::skinning::{SkinnedMesh, SkinnedMeshInverseBindposes};
use bevy::platform::collections::{HashMap, HashSet};
use bevy::prelude::*;

use super::attach::{SocketAttach, SocketResolution, TargetHierarchy};
use super::distortion_material::{
    DistortionInputs, DistortionNoiseValues, DistortionUniforms, OmagariDistortionMaterial,
};
use super::instance::{VfxInstance, VfxState};
use super::package::{PreparedDistortionLayer, VfxPackage};
use super::shape::{shape_world_transform, ShapeAnchor, ShapePlayer};
use super::shape_mesh::shape_mesh;
use super::surface::{skin_rest_scale, surface_meshes};
use super::triggers::TriggerArgs;
use crate::format::{
    AttachDef, DistortionEdgeDef, DistortionLayerDef, DistortionNoiseDef, DistortionSourceDef, FollowDef, ParamValue,
    PlayDef, ProjectV2, ShapeDef,
};

/// The camera components distortion layers need, for a game's camera:
///
/// ```ignore
/// commands.spawn((Camera3d::default(), omagari::distortion_camera(), /* projection, bloom, ... */));
/// ```
///
/// Bevy 0.19 already gives every `Camera3d` `ScreenSpaceTransmission {
/// steps: 1 }` (a required component) — distortion works with it as long as
/// `steps` stays above 0. This adds `Hdr` (so the haze bends the scene before
/// tonemapping and bloom, as in the editor) and states the transmission
/// settings explicitly. `quality` only affects Bevy's blur for rough
/// transmissive `StandardMaterial`s, not Omagari's distortion. Bevy copies
/// the screen once per frame and camera, and only while a distortion layer
/// (or another transmissive material) is drawn.
pub fn distortion_camera() -> impl Bundle {
    (
        bevy::camera::Hdr,
        bevy::pbr::ScreenSpaceTransmission { steps: 1, quality: bevy::pbr::ScreenSpaceTransmissionQuality::Low },
    )
}

/// Warns once when a distortion layer is drawn but no active 3D camera can
/// show it (`ScreenSpaceTransmission::steps` is 0 on all of them).
pub(crate) fn check_distortion_cameras(
    mut warned: Local<bool>,
    layers: Query<&DistortionLayerState>,
    cameras: Query<(&Camera, Option<&bevy::pbr::ScreenSpaceTransmission>), With<Camera3d>>,
) {
    if *warned || !layers.iter().any(|l| l.is_shown()) {
        return;
    }
    let mut active = cameras.iter().filter(|(c, _)| c.is_active).peekable();
    if active.peek().is_none() {
        return;
    }
    if active.all(|(_, t)| t.is_none_or(|t| t.steps == 0)) {
        warn!(
            "a distortion layer is playing, but no active 3D camera has ScreenSpaceTransmission with steps > 0;              nothing bends. Add omagari::distortion_camera() to the camera."
        );
        *warned = true;
    }
}

/// Segment capsules are rebuilt when their length changes by this much (m).
pub const SEGMENT_LENGTH_STEP: f32 = 0.005;

/// A distortion layer entity of an instance (a child of the instance).
#[derive(Component, Clone, Copy, Debug)]
pub struct VfxDistortionLayer {
    pub instance: Entity,
    /// Index into [`VfxPackage::distortion_layers`].
    pub index: usize,
}

/// A copy of one of the target's meshes drawn by a surface-source
/// distortion layer: a child of the mesh entity it copies.
#[derive(Component, Clone, Copy, Debug)]
pub struct VfxDistortionOverlay {
    pub layer: Entity,
    /// The mesh entity it copies.
    pub source: Entity,
}

/// Runtime state of a distortion layer.
#[derive(Component, Clone, Debug, Default)]
pub struct DistortionLayerState {
    pub player: ShapePlayer,
    /// `visible` factor (1 without `visible`).
    pub visibility: f32,
    /// The animated scale (shape sources).
    pub scale: Vec3,
    /// The strength this frame (the `strength` binding; fades not applied).
    pub strength: f32,
    /// World position of the last trigger's payload: an `OnTrigger` layer
    /// plays there instead of at its attach point.
    pub trigger_position: Option<Vec3>,
    /// Sockets of a `Socket` / `Segment` attach (shape sources).
    pub attach: Option<SocketAttach>,
    pub material: Option<Handle<OmagariDistortionMaterial>>,
    /// The mesh of a shape source.
    pub mesh: Option<Handle<Mesh>>,
    /// Length of a segment capsule's mesh (metres at fit 1, in
    /// [`SEGMENT_LENGTH_STEP`]s).
    pub segment_length: Option<f32>,
    /// Overlays of a surface source: (mesh entity, overlay entity).
    pub overlays: Vec<(Entity, Entity)>,
    /// World metres per rest-pose unit of the covered skinned meshes.
    pub rest_scale: f32,
    pub(crate) pending_trigger: Option<TriggerArgs>,
    pub(crate) pending_switches: Vec<String>,
    mesh_key: String,
    generation: u32,
    started: bool,
    hidden: bool,
    scanned: bool,
    start_level: f32,
}

impl DistortionLayerState {
    /// A state whose fade starts at `level` (a rebuilt looping layer keeps
    /// the level of the one it replaces).
    pub fn with_level(level: f32) -> Self {
        DistortionLayerState {
            visibility: 1.0,
            scale: Vec3::ONE,
            rest_scale: 1.0,
            start_level: level,
            hidden: true,
            ..default()
        }
    }

    /// Whether the layer is drawn this frame.
    pub fn is_shown(&self) -> bool {
        !self.hidden
    }
}

/// Meshes of distortion shape sources, one per distinct shape (segment
/// capsules: per length step).
#[derive(Resource, Default)]
pub struct DistortionMeshCache {
    meshes: HashMap<String, Handle<Mesh>>,
}

impl DistortionMeshCache {
    fn key(shape: &ShapeDef) -> String {
        format!("{:?}", shape)
    }

    fn get(&mut self, shape: &ShapeDef, meshes: &mut Assets<Mesh>) -> (String, Handle<Mesh>) {
        let key = Self::key(shape);
        let handle = self.meshes.entry(key.clone()).or_insert_with(|| meshes.add(shape_mesh(shape))).clone();
        (key, handle)
    }

    /// Number of cached meshes.
    pub fn len(&self) -> usize {
        self.meshes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.meshes.is_empty()
    }
}

/// The capsule of a segment source whose ends are `length` metres apart (at
/// fit 1), rounded to [`SEGMENT_LENGTH_STEP`].
pub fn segment_capsule(radius: f32, length: f32) -> ShapeDef {
    let steps = (length.max(0.0) / SEGMENT_LENGTH_STEP).round();
    ShapeDef::Capsule { radius, length: (steps * SEGMENT_LENGTH_STEP).max(SEGMENT_LENGTH_STEP) }
}

/// The shape a layer draws, given the current segment length.
fn current_shape(def: &DistortionLayerDef, segment_length: Option<f32>) -> Option<ShapeDef> {
    let shape = def.source.shape()?;
    match (shape, def.is_segment_capsule()) {
        (ShapeDef::Capsule { radius, length }, true) => Some(segment_capsule(*radius, segment_length.unwrap_or(*length))),
        _ => Some(shape.clone()),
    }
}

/// The average world scale of an instance (the size fit), positive.
fn fit_of(instance: &GlobalTransform) -> f32 {
    let s = instance.scale().abs();
    ((s.x + s.y + s.z) / 3.0).max(1e-4)
}

fn noise_values(noise: &DistortionNoiseDef, has_texture: bool) -> Option<DistortionNoiseValues> {
    has_texture.then_some(DistortionNoiseValues {
        mapping: noise.mapping,
        channel: noise.texture.channel,
        scale: noise.scale,
        scroll: noise.scroll,
        weight: noise.weight.clamp(0.0, 1.0),
        contrast: noise.contrast,
    })
}

/// The shader inputs of a distortion layer for its bound values and fade
/// (`textures` = whether noise A / B have a texture).
pub fn distortion_inputs(
    def: &DistortionLayerDef,
    strength: f32,
    tint: Vec4,
    fade: f32,
    fit_scale: f32,
    rest_scale: f32,
    textures: [bool; 2],
) -> DistortionInputs {
    let shell_offset = match &def.source {
        DistortionSourceDef::Surface { offset, .. } => *offset,
        DistortionSourceDef::Shape { .. } => 0.0,
    };
    DistortionInputs {
        strength,
        edge_softness: def.edge_softness,
        rim: def.edge == DistortionEdgeDef::Rim,
        refraction: def.refraction,
        fade,
        tint,
        noise: [
            noise_values(&def.noise, textures[0]),
            def.noise_b.as_ref().and_then(|n| noise_values(n, textures[1])),
        ],
        shell_offset,
        fit_scale,
        rest_scale,
    }
}

/// Spawns the entity of one distortion layer of an instance.
pub(crate) fn spawn_distortion_layer(
    commands: &mut Commands,
    instance: Entity,
    index: usize,
    prepared: &PreparedDistortionLayer,
    project: &ProjectV2,
    level: f32,
) -> Entity {
    let def = &prepared.def;
    let mut state = DistortionLayerState::with_level(level);
    let shape_source = matches!(def.source, DistortionSourceDef::Shape { .. });
    if shape_source && def.attach != AttachDef::Target {
        state.attach = Some(SocketAttach::new(&def.attach, project));
    }
    let mut e = commands.spawn((
        Name::new(def.name.clone()),
        VfxDistortionLayer { instance, index },
        state,
        Transform::default(),
        Visibility::Hidden,
        NotShadowCaster,
        NotShadowReceiver,
        ChildOf(instance),
    ));
    if def.is_segment_capsule() {
        // Its mesh changes with the segment's length after the bounds were
        // computed for the frame.
        e.insert(NoFrustumCulling);
    }
    e.id()
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

/// Keeps the overlays of surface-source distortion layers matching the
/// target's meshes (one per covered mesh, sharing the layer's material).
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(crate) fn sync_distortion_overlays(
    mut commands: Commands,
    packages: Res<Assets<VfxPackage>>,
    bindposes: Option<Res<Assets<SkinnedMeshInverseBindposes>>>,
    materials: Option<ResMut<Assets<OmagariDistortionMaterial>>>,
    hierarchy: TargetHierarchy,
    changed: Query<(), (Or<(Changed<Children>, Changed<Mesh3d>, Changed<SkinnedMesh>, Changed<Name>)>, Without<VfxDistortionOverlay>)>,
    instances: Query<&VfxInstance>,
    sources: Query<SourceData, Without<VfxDistortionOverlay>>,
    mut overlays: Query<(&mut Mesh3d, Option<&mut SkinnedMesh>), With<VfxDistortionOverlay>>,
    globals: Query<&GlobalTransform>,
    mut layers: Query<(Entity, &VfxDistortionLayer, &mut DistortionLayerState)>,
) {
    let Some(mut materials) = materials else { return };
    if layers.is_empty() {
        return;
    }
    let any_change = !changed.is_empty();
    for (layer_entity, layer, mut state) in layers.iter_mut() {
        let Ok(instance) = instances.get(layer.instance) else { continue };
        let Some(package) = packages.get(&instance.package) else { continue };
        let Some(prepared) = package.distortion_layers.get(layer.index) else { continue };
        let DistortionSourceDef::Surface { targets, .. } = &prepared.def.source else { continue };
        let state = &mut *state;
        if state.material.is_none() {
            state.material = Some(materials.add(new_material(prepared)));
        }
        if state.scanned && !any_change {
            continue;
        }
        state.scanned = true;
        let material = state.material.clone().unwrap();

        let wanted = surface_meshes(&hierarchy, instance.target(), targets, |e| match sources.get(e) {
            Ok((_, name, mesh, material, ..)) => (
                name.map(|n| n.to_string()),
                mesh.map(|m| m.0.clone()),
                material.map(|m| m.0.clone()),
            ),
            Err(_) => (None, None, None),
        });

        let mut kept = Vec::new();
        for (source, entity) in state.overlays.iter().copied() {
            let exists = overlays.contains(entity);
            if exists && wanted.contains(&source) {
                kept.push((source, entity));
            } else if exists {
                commands.entity(entity).try_despawn();
            }
        }
        for (source, entity) in kept.iter() {
            let (Ok(src), Ok((mut mesh, skin))) = (sources.get(*source), overlays.get_mut(*entity)) else { continue };
            if mesh.0 != src.0 .0 {
                mesh.0 = src.0 .0.clone();
            }
            match (skin, src.4) {
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
        for &source in wanted.iter() {
            if kept.iter().any(|(s, _)| *s == source) {
                continue;
            }
            let Ok((mesh, _, _, _, skin, morph, dynamic_bounds, no_culling)) = sources.get(source) else { continue };
            let mut e = commands.spawn((
                Name::new(format!("{} (distortion)", prepared.def.name)),
                VfxDistortionOverlay { layer: layer_entity, source },
                Mesh3d(mesh.0.clone()),
                MeshMaterial3d(material.clone()),
                Transform::IDENTITY,
                if state.hidden { Visibility::Hidden } else { Visibility::Inherited },
                NotShadowCaster,
                NotShadowReceiver,
                ChildOf(source),
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
            kept.push((source, e.id()));
        }
        state.overlays = kept;

        let mut rest_scale = None;
        for &source in wanted.iter() {
            if let Ok((_, _, _, _, Some(skin), ..)) = sources.get(source) {
                rest_scale = skin_rest_scale(skin, bindposes.as_deref(), &globals);
                if rest_scale.is_some() {
                    break;
                }
            }
        }
        state.rest_scale = rest_scale.unwrap_or(1.0);
    }
}

fn new_material(prepared: &PreparedDistortionLayer) -> OmagariDistortionMaterial {
    OmagariDistortionMaterial {
        uniforms: DistortionUniforms::default(),
        noise_a: prepared.textures[0].clone(),
        noise_b: prepared.textures[1].clone(),
        sort_bias: prepared.def.sort_bias,
    }
}

/// Plays distortion layers: meshes and materials, sockets, triggers, age,
/// fades, visibility and the bound values (written into the material when
/// they change). A layer with nothing to show is hidden.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(crate) fn update_distortions(
    mut commands: Commands,
    time: Res<Time>,
    packages: Res<Assets<VfxPackage>>,
    materials: Option<ResMut<Assets<OmagariDistortionMaterial>>>,
    meshes: Option<ResMut<Assets<Mesh>>>,
    mut cache: ResMut<DistortionMeshCache>,
    hierarchy: TargetHierarchy,
    changed: Query<(), (Or<(Changed<Children>, Changed<Name>)>, Without<VfxDistortionLayer>)>,
    instances: Query<(&VfxInstance, &GlobalTransform)>,
    mut layers: Query<(Entity, &VfxDistortionLayer, &mut DistortionLayerState, &mut Visibility)>,
    mut overlay_visibility: Query<&mut Visibility, (With<VfxDistortionOverlay>, Without<VfxDistortionLayer>)>,
) {
    let (Some(mut materials), Some(mut meshes)) = (materials, meshes) else { return };
    if layers.is_empty() {
        if !cache.is_empty() {
            cache.meshes.clear();
        }
        return;
    }
    let dt = time.delta_secs();
    let any_change = !changed.is_empty();
    let mut used: HashSet<String> = HashSet::default();
    for (entity, layer, mut state, mut visibility) in layers.iter_mut() {
        let state = &mut *state;
        if !state.mesh_key.is_empty() {
            used.insert(state.mesh_key.clone());
        }
        let Ok((instance, instance_global)) = instances.get(layer.instance) else { continue };
        let Some(package) = packages.get(&instance.package) else { continue };
        let Some(prepared) = package.distortion_layers.get(layer.index) else { continue };
        if !instance.params.is_synced(instance.package.id()) {
            continue;
        }
        let def = &prepared.def;

        // Mesh (shape sources) and material.
        if let Some(shape) = current_shape(def, state.segment_length) {
            let key = DistortionMeshCache::key(&shape);
            if state.mesh.is_none() || state.mesh_key != key {
                let (key, mesh) = cache.get(&shape, &mut meshes);
                commands.entity(entity).insert(Mesh3d(mesh.clone()));
                state.mesh = Some(mesh);
                state.mesh_key = key;
            }
            used.insert(state.mesh_key.clone());
        }
        if state.material.is_none() {
            let material = materials.add(new_material(prepared));
            if def.source.shape().is_some() {
                commands.entity(entity).insert(MeshMaterial3d(material.clone()));
            }
            state.material = Some(material);
        }

        // Start (and start over on a restart).
        if !state.started || state.generation != instance.generation {
            state.player = ShapePlayer::new(def, state.start_level);
            state.started = true;
            state.generation = instance.generation;
            state.trigger_position = None;
        }
        if let Some(attach) = &mut state.attach {
            attach.update(&hierarchy, instance.target(), any_change, dt, def.name.as_str());
        }

        let running = matches!(instance.state(), VfxState::Running | VfxState::Starting);
        if let Some(args) = state.pending_trigger.take()
            && running
        {
            state.player.trigger();
            state.trigger_position = args.position;
        }
        for name in std::mem::take(&mut state.pending_switches) {
            if running {
                state.player.switch(def, &name);
            }
        }
        let while_value = match &def.play {
            PlayDef::While(param, _) => instance.params.float(param),
            _ => None,
        };
        state.player.step(def, dt, running, while_value);
        state.visibility = match &def.visible {
            Some(visible) => instance.params.float(&visible.param).map(|v| visible.factor(v)).unwrap_or(1.0),
            None => 1.0,
        };

        // Bound values.
        let age = state.player.binding_age(def);
        let lookup = |name: &str| -> Option<ParamValue> { instance.params.get(name) };
        let values = def.values_at(&lookup, age);
        state.scale = values.scale;
        state.strength = values.strength;
        let fade = state.player.opacity(def) * state.visibility;
        let textures = [prepared.textures[0].is_some(), prepared.textures[1].is_some()];
        let inputs = distortion_inputs(
            def,
            values.strength,
            values.tint,
            fade,
            fit_of(instance_global),
            state.rest_scale,
            textures,
        );
        let uniforms = DistortionUniforms::new(&inputs);
        if let Some(handle) = &state.material {
            let same = materials.get(handle).is_some_and(|m| m.uniforms == uniforms && m.sort_bias == def.sort_bias);
            if !same && let Some(mut material) = materials.get_mut(handle) {
                material.uniforms = uniforms;
                material.sort_bias = def.sort_bias;
            }
        }

        // Nothing to show: not drawn at all (no transmission copy).
        let hidden = !uniforms.is_visible();
        let wanted = if hidden { Visibility::Hidden } else { Visibility::Inherited };
        if def.source.shape().is_some() {
            if *visibility != wanted {
                *visibility = wanted;
            }
        } else {
            if *visibility != Visibility::Inherited {
                *visibility = Visibility::Inherited;
            }
            if hidden != state.hidden || any_change {
                for &(_, overlay) in state.overlays.iter() {
                    if let Ok(mut v) = overlay_visibility.get_mut(overlay)
                        && *v != wanted
                    {
                        *v = wanted;
                    }
                }
            }
        }
        state.hidden = hidden;
    }
    cache.meshes.retain(|key, _| used.contains(key));
}

/// Places distortion shape sources in the world (after transform
/// propagation, before visibility checks): attach point, follow mode,
/// offset, rotation and the animated scale × fit, like shape layers; a
/// segment capsule spans its two sockets (its mesh follows the length).
#[allow(clippy::type_complexity)]
pub(crate) fn place_distortions(
    packages: Res<Assets<VfxPackage>>,
    meshes: Option<ResMut<Assets<Mesh>>>,
    mut cache: ResMut<DistortionMeshCache>,
    instances: Query<(&VfxInstance, &GlobalTransform), Without<VfxDistortionLayer>>,
    globals: Query<&GlobalTransform, Without<VfxDistortionLayer>>,
    mut layers: Query<(&VfxDistortionLayer, &mut DistortionLayerState, &mut GlobalTransform, Option<&mut Mesh3d>)>,
) {
    let mut meshes = meshes;
    for (layer, mut state, mut global, mesh) in layers.iter_mut() {
        let Ok((instance, instance_global)) = instances.get(layer.instance) else { continue };
        let Some(prepared) = packages.get(&instance.package).and_then(|p| p.distortion_layers.get(layer.index)) else {
            continue;
        };
        let def = &prepared.def;
        let DistortionSourceDef::Shape { follow, transform, .. } = &def.source else { continue };
        let fit = fit_of(instance_global);
        let (_, instance_rotation, instance_position) = instance_global.to_scale_rotation_translation();
        let mut segment = None;
        let mut anchor = match (&def.attach, &state.attach) {
            (AttachDef::Socket(_), Some(attach)) => {
                let position = attach.world_positions(instance_global, &globals)[0];
                let rotation = match attach.sockets[0].resolution {
                    SocketResolution::Node(node) => globals
                        .get(node)
                        .map(|g| g.to_scale_rotation_translation().1)
                        .unwrap_or(instance_rotation),
                    _ => instance_rotation,
                };
                ShapeAnchor { position, rotation }
            }
            (AttachDef::Segment { .. }, Some(attach)) => {
                let ends = attach.world_positions(instance_global, &globals);
                let (a, b) = (ends[0], ends[1]);
                let along = b - a;
                segment = Some(along.length());
                let rotation = if along.length_squared() > 1e-10 {
                    Quat::from_rotation_arc(Vec3::Y, along.normalize())
                } else {
                    instance_rotation
                };
                ShapeAnchor { position: (a + b) * 0.5, rotation }
            }
            _ => ShapeAnchor { position: instance_position, rotation: instance_rotation },
        };
        if let Some(position) = state.trigger_position.filter(|_| matches!(def.play, PlayDef::OnTrigger(_))) {
            anchor.position = position;
        }
        let mut scale = state.scale;
        let mut follow = *follow;
        let mut offset = transform.offset;
        let mut rotation = transform.rotation();
        if def.is_segment_capsule() {
            // The capsule spans the segment: no length scale, no offset or
            // rotation away from it.
            scale.y = 1.0;
            follow = FollowDef::Full;
            offset = Vec3::ZERO;
            rotation = Quat::IDENTITY;
            if let Some(length) = segment {
                let length = length / fit;
                let changed = state.segment_length.is_none_or(|l| (l - length).abs() >= SEGMENT_LENGTH_STEP * 0.5);
                if changed {
                    state.segment_length = Some(length);
                    if let (Some(shape), Some(meshes), Some(mut mesh)) =
                        (current_shape(def, Some(length)), meshes.as_deref_mut(), mesh)
                    {
                        let (key, handle) = cache.get(&shape, meshes);
                        if mesh.0 != handle {
                            mesh.0 = handle.clone();
                        }
                        state.mesh = Some(handle);
                        state.mesh_key = key;
                    }
                }
            }
        }
        let world = shape_world_transform(anchor, follow, fit, offset, rotation, scale);
        let wanted = GlobalTransform::from(world);
        if *global != wanted {
            *global = wanted;
        }
    }
}

/// When a distortion layer goes away, its overlays go too (they are
/// children of the model's meshes, not of the layer).
pub(crate) fn on_distortion_layer_removed(
    removed: On<Remove, VfxDistortionLayer>,
    mut commands: Commands,
    states: Query<&DistortionLayerState>,
) {
    let Ok(state) = states.get(removed.entity) else { return };
    for &(_, entity) in state.overlays.iter() {
        commands.entity(entity).try_despawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segment_capsules_round_their_length() {
        assert_eq!(segment_capsule(0.1, 0.8712), ShapeDef::Capsule { radius: 0.1, length: 0.87 });
        assert_eq!(segment_capsule(0.1, 0.0), ShapeDef::Capsule { radius: 0.1, length: SEGMENT_LENGTH_STEP });
        let def = DistortionLayerDef {
            attach: AttachDef::Segment { from: "a".into(), to: "b".into() },
            source: DistortionSourceDef::Shape {
                shape: ShapeDef::Capsule { radius: 0.2, length: 3.0 },
                follow: FollowDef::Full,
                transform: Default::default(),
            },
            ..Default::default()
        };
        assert_eq!(current_shape(&def, None), Some(ShapeDef::Capsule { radius: 0.2, length: 3.0 }), "before the first placement");
        assert_eq!(current_shape(&def, Some(1.0)), Some(ShapeDef::Capsule { radius: 0.2, length: 1.0 }));
        let sphere = DistortionLayerDef::default();
        assert_eq!(current_shape(&sphere, Some(1.0)), Some(ShapeDef::Sphere { radius: 0.5 }));
    }
}
