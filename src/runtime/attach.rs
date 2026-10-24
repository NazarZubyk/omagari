//! Where layers sit on their target: sockets, segments, size fit, and the
//! target's own model (fit height, surface layers).
//!
//! - `Socket(name)`: the package socket `name` (a node with that `Name` under
//!   the target + an offset, or an offset from the target root), else a node
//!   named `name`. The layer entity becomes a child of the node. Resolution
//!   is retried every frame until the node exists (a glTF scene loads after
//!   the instance is spawned), cached, and redone when the hierarchy changes.
//!   Until then, and if nothing matches, the layer sits at the target root
//!   (+ the socket's offset); after [`SOCKET_TIMEOUT`] one warning is logged.
//! - `Segment(from, to)`: both sockets are resolved the same way; every frame
//!   their positions, in the layer's simulation space, go into the hanabi
//!   properties `segment_a` / `segment_b` when the layer's effect reads them.
//! - Fit ([`FitDef`]): the instance's world scale. bevy_hanabi applies the
//!   emitter's full transform (including scale) to `Local` layers, so they
//!   scale with the instance; `Global` layers only get the emitter position
//!   and keep their authored size.

use bevy::camera::primitives::Aabb;
use bevy::ecs::query::QueryFilter;
use bevy::ecs::system::SystemParam;
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;
use bevy::scene::ScenePatchInstance;
use bevy::world_serialization::{DynamicWorldRoot, WorldAssetRoot};
use bevy_hanabi::prelude::*;

use super::instance::{VfxInstance, VfxLayer};
use super::package::VfxPackage;
use super::afterimage::{VfxAfterimageLayer, VfxGhost};
use super::light::VfxLightLayer;
use super::shape::VfxShapeLayer;
use super::surface::VfxSurfaceOverlay;
use super::trail::VfxTrailLayer;
use crate::format::{AttachDef, FitDef, ProjectV2, SimulationSpaceDef};

/// Seconds a socket may stay unresolved before a warning is logged (the
/// layer sits at the target root meanwhile, and keeps trying).
pub const SOCKET_TIMEOUT: f32 = 3.0;

/// Seconds `TargetHeight` waits for the target's meshes before it warns and
/// uses the target's scale.
pub const FIT_TIMEOUT: f32 = 3.0;

/// Marks an entity under a target as a separate model (e.g. a weapon in the
/// hand): own-model searches (surface layers, `TargetHeight` fit) stop there.
/// glTF scene roots (`WorldAssetRoot`) below the target's own model are
/// separate models anyway.
#[derive(Component, Default, Clone, Copy, Debug)]
pub struct VfxSeparateModel;

// --- pure helpers ------------------------------------------------------------

/// Where a socket name points: a node name (or `None` = the target root) and
/// an offset. A package socket wins; otherwise the name is a node name.
pub fn socket_lookup(project: &ProjectV2, name: &str) -> (Option<String>, Vec3) {
    match project.socket(name) {
        Some(socket) => (socket.node.clone(), socket.offset),
        None => (Some(name.to_string()), Vec3::ZERO),
    }
}

/// The world scale an instance should have.
///
/// - no fit: 1 (authored size, whatever the target's scale);
/// - `TargetScale`: the target's world scale;
/// - `TargetHeight(reference)`: `measured height / reference`, uniform; the
///   target's scale until the height is known.
pub fn fit_world_scale(fit: Option<FitDef>, target_scale: Vec3, measured_height: Option<f32>) -> Vec3 {
    match fit {
        None => Vec3::ONE,
        Some(FitDef::TargetScale) => target_scale,
        Some(FitDef::TargetHeight(reference)) => match measured_height {
            Some(height) if reference > 0.0 && height > 0.0 => Vec3::splat(height / reference),
            _ => target_scale,
        },
    }
}

/// Local scale giving `world` under a parent of world scale `parent`
/// (component-wise; a zero parent axis keeps 1).
pub fn local_scale(world: Vec3, parent: Vec3) -> Vec3 {
    let div = |w: f32, p: f32| if p.abs() > 1e-6 { w / p } else { 1.0 };
    Vec3::new(div(world.x, parent.x), div(world.y, parent.y), div(world.z, parent.z))
}

/// World bounds `(min, max)` of local boxes `(min, max)` placed by their
/// transforms. `None` without boxes.
pub fn world_bounds(boxes: impl IntoIterator<Item = (Vec3, Vec3, GlobalTransform)>) -> Option<(Vec3, Vec3)> {
    let mut range: Option<(Vec3, Vec3)> = None;
    for (min, max, transform) in boxes {
        for i in 0..8 {
            let corner = Vec3::new(
                if i & 1 == 0 { min.x } else { max.x },
                if i & 2 == 0 { min.y } else { max.y },
                if i & 4 == 0 { min.z } else { max.z },
            );
            let p = transform.transform_point(corner);
            range = Some(match range {
                None => (p, p),
                Some((lo, hi)) => (lo.min(p), hi.max(p)),
            });
        }
    }
    range
}

/// Height (world Y extent) of local boxes `(min, max)` placed by their
/// transforms. `None` without boxes.
pub fn world_height(boxes: impl IntoIterator<Item = (Vec3, Vec3, GlobalTransform)>) -> Option<f32> {
    world_bounds(boxes).map(|(lo, hi)| hi.y - lo.y)
}

/// World position of a socket on `node`: the node's position plus the offset
/// rotated by the node and scaled by the instance's world scale (offsets are
/// authored at fit scale 1).
pub fn socket_world_position(node: &GlobalTransform, instance_scale: Vec3, offset: Vec3) -> Vec3 {
    let (_, rotation, translation) = node.to_scale_rotation_translation();
    translation + rotation * (offset * instance_scale)
}

/// The socket offset that puts a socket on `node` at `world`: the inverse
/// of [`socket_world_position`] (used to drag sockets in the editor).
pub fn socket_offset_for(node: &GlobalTransform, instance_scale: Vec3, world: Vec3) -> Vec3 {
    let (_, rotation, translation) = node.to_scale_rotation_translation();
    let local = rotation.inverse() * (world - translation);
    let div = |v: f32, s: f32| if s.abs() > 1e-6 { v / s } else { v };
    Vec3::new(div(local.x, instance_scale.x), div(local.y, instance_scale.y), div(local.z, instance_scale.z))
}

/// A world position in a layer's simulation space, the space position
/// modifiers write: layer-local for `Local`, relative to the emitter position
/// (world axes) for `Global`.
pub fn to_simulation_space(world: Vec3, layer: &GlobalTransform, space: SimulationSpaceDef) -> Vec3 {
    match space {
        SimulationSpaceDef::Local => layer.affine().inverse().transform_point3(world),
        SimulationSpaceDef::Global => world - layer.translation(),
    }
}

// --- hierarchy -----------------------------------------------------------------

/// Read access to a target's hierarchy: node lookup by name and the target's
/// own meshes. Instances, layers and surface overlays are skipped.
#[derive(SystemParam)]
pub struct TargetHierarchy<'w, 's> {
    children: Query<'w, 's, &'static Children>,
    names: Query<'w, 's, &'static Name>,
    vfx: Query<
        'w,
        's,
        (),
        Or<(
            With<VfxInstance>,
            With<VfxLayer>,
            With<VfxSurfaceOverlay>,
            With<VfxShapeLayer>,
            With<VfxTrailLayer>,
            With<VfxAfterimageLayer>,
            With<VfxGhost>,
            With<VfxLightLayer>,
        )>,
    >,
    model_roots: Query<'w, 's, (), Or<(With<WorldAssetRoot>, With<DynamicWorldRoot>, With<ScenePatchInstance>)>>,
    separate: Query<'w, 's, (), With<VfxSeparateModel>>,
    meshes: Query<'w, 's, (), With<Mesh3d>>,
}

impl TargetHierarchy<'_, '_> {
    /// The nearest descendant of `root` (breadth first, `root` excluded)
    /// whose `Name` is `name`.
    pub fn find_node(&self, root: Entity, name: &str) -> Option<Entity> {
        let mut queue = std::collections::VecDeque::new();
        queue.push_back(root);
        while let Some(entity) = queue.pop_front() {
            let Ok(children) = self.children.get(entity) else {
                continue;
            };
            for child in children.iter() {
                if self.vfx.contains(child) {
                    continue;
                }
                if self.names.get(child).is_ok_and(|n| n.as_str() == name) {
                    return Some(child);
                }
                queue.push_back(child);
            }
        }
        None
    }

    /// Mesh entities of the target's own model: everything under `root`
    /// except separate models. A model root (`WorldAssetRoot`, ...) is the
    /// target's own model when it is `root` itself or the first one on the
    /// way down; a model root inside a model (a weapon in a hand bone) and
    /// entities marked [`VfxSeparateModel`] are separate models and skipped
    /// with everything under them.
    pub fn own_meshes(&self, root: Entity) -> Vec<Entity> {
        let mut found = Vec::new();
        let mut stack = vec![(root, self.model_roots.contains(root))];
        while let Some((entity, inside_model)) = stack.pop() {
            if self.meshes.contains(entity) {
                found.push(entity);
            }
            let Ok(children) = self.children.get(entity) else {
                continue;
            };
            for child in children.iter() {
                if self.vfx.contains(child) || self.separate.contains(child) {
                    continue;
                }
                let is_model_root = self.model_roots.contains(child);
                if is_model_root && inside_model {
                    continue;
                }
                stack.push((child, inside_model || is_model_root));
            }
        }
        found
    }

    /// Whether `node` still exists with this name.
    fn is_named(&self, node: Entity, name: &str) -> bool {
        self.names.get(node).is_ok_and(|n| n.as_str() == name)
    }
}

// --- sockets -------------------------------------------------------------------

/// How one socket of a layer is resolved.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum SocketResolution {
    /// The node does not exist (yet); the target root is used.
    Pending,
    /// The socket has no node: an offset from the target root.
    Root,
    /// The socket's node.
    Node(Entity),
}

/// One socket used by a layer.
#[derive(Clone, Debug)]
pub struct SocketBinding {
    pub name: String,
    /// Node name, `None` = the target root.
    pub node: Option<String>,
    pub offset: Vec3,
    pub resolution: SocketResolution,
}

impl SocketBinding {
    fn resolve(&mut self, hierarchy: &TargetHierarchy, target: Entity, force: bool) {
        let Some(node) = &self.node else {
            self.resolution = SocketResolution::Root;
            return;
        };
        let valid = match self.resolution {
            SocketResolution::Node(e) => hierarchy.is_named(e, node),
            _ => false,
        };
        if valid && !force {
            return;
        }
        self.resolution = match hierarchy.find_node(target, node) {
            Some(e) => SocketResolution::Node(e),
            None => SocketResolution::Pending,
        };
    }

    fn is_pending(&self) -> bool {
        self.resolution == SocketResolution::Pending
    }
}

/// Socket state of a layer attached to a `Socket` (one binding) or a
/// `Segment` (two bindings).
#[derive(Component, Clone, Debug)]
pub struct SocketAttach {
    pub sockets: Vec<SocketBinding>,
    pub segment: bool,
    /// Seconds since the layer was spawned while a socket is unresolved.
    waited: f32,
    warned: bool,
    first: bool,
}

impl SocketAttach {
    pub fn new(attach: &AttachDef, project: &ProjectV2) -> Self {
        let sockets = attach
            .socket_names()
            .into_iter()
            .map(|name| {
                let (node, offset) = socket_lookup(project, name);
                SocketBinding {
                    name: name.to_string(),
                    node,
                    offset,
                    resolution: SocketResolution::Pending,
                }
            })
            .collect();
        SocketAttach {
            sockets,
            segment: matches!(attach, AttachDef::Segment { .. }),
            waited: 0.0,
            warned: false,
            first: true,
        }
    }

    /// Resolves the sockets (all of them on the first call or when the
    /// hierarchy changed, else only lost ones) and warns once about sockets
    /// still unresolved after [`SOCKET_TIMEOUT`]; `who` names the layer.
    pub(crate) fn update(&mut self, hierarchy: &TargetHierarchy, target: Entity, any_change: bool, dt: f32, who: &str) {
        let force = any_change || self.first;
        self.first = false;
        for socket in self.sockets.iter_mut() {
            socket.resolve(hierarchy, target, force);
        }
        if self.sockets.iter().any(SocketBinding::is_pending) {
            self.waited += dt;
            if self.waited >= SOCKET_TIMEOUT && !self.warned {
                self.warned = true;
                for socket in self.sockets.iter().filter(|s| s.is_pending()) {
                    warn!(
                        "VFX layer {}: socket \"{}\" not found (no node named \"{}\" under the target); using the target root",
                        who,
                        socket.name,
                        socket.node.as_deref().unwrap_or_default()
                    );
                }
            }
        } else {
            self.waited = 0.0;
        }
    }

    /// World position of each socket: on its node, else the offset from the
    /// instance (target root, at fit scale).
    pub fn world_positions(&self, instance: &GlobalTransform, globals: &Query<&GlobalTransform, impl QueryFilter>) -> Vec<Vec3> {
        let instance_scale = instance.scale();
        self.sockets
            .iter()
            .map(|socket| match socket.resolution {
                SocketResolution::Node(node) => match globals.get(node) {
                    Ok(node_global) => socket_world_position(node_global, instance_scale, socket.offset),
                    Err(_) => instance.transform_point(socket.offset),
                },
                SocketResolution::Root | SocketResolution::Pending => instance.transform_point(socket.offset),
            })
            .collect()
    }
}

/// Whether anything in the world's hierarchy changed since the last run
/// (children added/removed, names changed).
fn hierarchy_changed(changed: &Query<(), Or<(Changed<Children>, Changed<Name>)>>) -> bool {
    !changed.is_empty()
}

/// Resolves the sockets of socket/segment layers and places socket layers
/// under their node.
pub(crate) fn resolve_sockets(
    mut commands: Commands,
    time: Res<Time>,
    hierarchy: TargetHierarchy,
    changed: Query<(), Or<(Changed<Children>, Changed<Name>)>>,
    instances: Query<(&VfxInstance, &GlobalTransform)>,
    globals: Query<&GlobalTransform>,
    mut layers: Query<(Entity, &VfxLayer, &mut SocketAttach, &mut Transform, Option<&ChildOf>)>,
) {
    if layers.is_empty() {
        return;
    }
    let dt = time.delta_secs();
    let any_change = hierarchy_changed(&changed);
    for (entity, layer, mut attach, mut transform, child_of) in layers.iter_mut() {
        let Ok((instance, instance_global)) = instances.get(layer.instance) else {
            continue;
        };
        let target = instance.target();
        attach.update(&hierarchy, target, any_change, dt, &entity.to_string());
        if attach.segment {
            continue;
        }

        // A socket layer: a child of its node, or of the instance (target
        // root, already at fit scale) when it has none.
        let socket = &attach.sockets[0];
        let (parent, wanted) = match socket.resolution {
            SocketResolution::Node(node) => {
                let node_scale = globals.get(node).map(|g| g.scale()).unwrap_or(Vec3::ONE);
                let k = local_scale(instance_global.scale(), node_scale);
                (node, Transform::from_translation(socket.offset * k).with_scale(k))
            }
            SocketResolution::Root | SocketResolution::Pending => {
                (layer.instance, Transform::from_translation(socket.offset))
            }
        };
        if child_of.map(|c| c.parent()) != Some(parent) {
            commands.entity(entity).insert(ChildOf(parent));
        }
        if *transform != wanted {
            *transform = wanted;
        }
    }
}

/// Writes the segment ends of segment layers into their effect properties
/// (`segment_a`, `segment_b`), after transforms are propagated.
pub(crate) fn write_segment_properties(
    packages: Res<Assets<VfxPackage>>,
    instances: Query<(&VfxInstance, &GlobalTransform)>,
    globals: Query<&GlobalTransform>,
    mut layers: Query<(&VfxLayer, &SocketAttach, &GlobalTransform, &mut EffectProperties)>,
) {
    for (layer, attach, layer_global, properties) in layers.iter_mut() {
        if !attach.segment || attach.sockets.len() != 2 {
            continue;
        }
        let Ok((instance, instance_global)) = instances.get(layer.instance) else {
            continue;
        };
        let Some(prepared) = packages
            .get(&instance.package)
            .and_then(|p| p.particle_layers.get(layer.index))
        else {
            continue;
        };
        let instance_scale = instance_global.scale();
        let ends = attach.sockets.iter().map(|socket| {
            let world = match socket.resolution {
                SocketResolution::Node(node) => match globals.get(node) {
                    Ok(node_global) => socket_world_position(node_global, instance_scale, socket.offset),
                    Err(_) => instance_global.transform_point(socket.offset),
                },
                SocketResolution::Root | SocketResolution::Pending => instance_global.transform_point(socket.offset),
            };
            to_simulation_space(world, layer_global, prepared.simulation_space)
        });
        let mut properties = properties;
        for (name, value) in ["segment_a", "segment_b"].into_iter().zip(ends) {
            if prepared.properties.iter().any(|p| p == name) {
                properties = EffectProperties::set_if_changed(properties, name, value.into());
            }
        }
    }
}

// --- fit -------------------------------------------------------------------------

/// Size-fit state of an instance with a target.
#[derive(Component, Default, Clone, Debug)]
pub struct FitState {
    /// Measured height of the target's own meshes (world units).
    pub height: Option<f32>,
    waited: f32,
    warned: bool,
}

/// Local bounds `(min, max)` of a mesh entity: its `Aabb` when Bevy computed
/// one (skinned glTF meshes get one that follows the animation), else from
/// the mesh asset.
pub fn mesh_bounds(aabb: Option<&Aabb>, mesh: Option<&Mesh>) -> Option<(Vec3, Vec3)> {
    if let Some(aabb) = aabb {
        return Some((Vec3::from(aabb.min()), Vec3::from(aabb.max())));
    }
    let mesh = mesh?;
    if let Some(bounds) = mesh.final_aabb {
        return Some((Vec3::from(bounds.min), Vec3::from(bounds.max)));
    }
    let Ok(VertexAttributeValues::Float32x3(positions)) = mesh.try_attribute(Mesh::ATTRIBUTE_POSITION) else {
        return None;
    };
    let mut iter = positions.iter().map(|p| Vec3::from_array(*p));
    let first = iter.next()?;
    Some(iter.fold((first, first), |(lo, hi), p| (lo.min(p), hi.max(p))))
}

/// Sets the scale of instances with a target from the package's `fit`.
pub(crate) fn apply_fit(
    time: Res<Time>,
    packages: Res<Assets<VfxPackage>>,
    meshes: Option<Res<Assets<Mesh>>>,
    hierarchy: TargetHierarchy,
    globals: Query<Ref<GlobalTransform>>,
    mesh_entities: Query<(&Mesh3d, Option<&Aabb>)>,
    mut instances: Query<(&VfxInstance, &mut FitState, &mut Transform)>,
) {
    let dt = time.delta_secs();
    for (instance, mut state, mut transform) in instances.iter_mut() {
        if instance.is_in_world() {
            continue;
        }
        let Some(package) = packages.get(&instance.package) else {
            continue;
        };
        let fit = package.project.fit;
        let target_scale = globals
            .get(instance.target())
            .map(|g| g.scale())
            .unwrap_or(Vec3::ONE);

        if let (Some(FitDef::TargetHeight(_)), None) = (fit, state.height) {
            state.height = measure_height(instance.target(), &hierarchy, &globals, &mesh_entities, meshes.as_deref());
            if state.height.is_none() {
                state.waited += dt;
                if state.waited >= FIT_TIMEOUT && !state.warned {
                    state.warned = true;
                    warn!(
                        "VFX instance on {}: fit TargetHeight found no meshes under the target; using the target's scale",
                        instance.target()
                    );
                }
            }
        }

        let world = fit_world_scale(fit, target_scale, state.height);
        let scale = local_scale(world, target_scale);
        if transform.scale != scale {
            transform.scale = scale;
        }
    }
}

/// Height of the target's own meshes, once every one of them has bounds and
/// a propagated transform.
fn measure_height(
    target: Entity,
    hierarchy: &TargetHierarchy,
    globals: &Query<Ref<GlobalTransform>>,
    mesh_entities: &Query<(&Mesh3d, Option<&Aabb>)>,
    meshes: Option<&Assets<Mesh>>,
) -> Option<f32> {
    let own = hierarchy.own_meshes(target);
    if own.is_empty() {
        return None;
    }
    let mut boxes = Vec::with_capacity(own.len());
    for entity in own {
        let (mesh, aabb) = mesh_entities.get(entity).ok()?;
        let global = globals.get(entity).ok()?;
        // Spawned this frame: its transform is not propagated yet.
        if global.is_added() {
            return None;
        }
        let (min, max) = mesh_bounds(aabb, meshes.and_then(|m| m.get(&mesh.0)))?;
        boxes.push((min, max, *global));
    }
    world_height(boxes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::SocketDef;

    #[test]
    fn fit_scale_math() {
        let target = Vec3::new(2.0, 2.0, 2.0);
        assert_eq!(fit_world_scale(None, target, Some(4.0)), Vec3::ONE);
        assert_eq!(fit_world_scale(Some(FitDef::TargetScale), target, None), target);
        // A 4 m model, effect authored for 1.8 m.
        let s = fit_world_scale(Some(FitDef::TargetHeight(1.8)), Vec3::ONE, Some(4.0));
        assert!((s - Vec3::splat(4.0 / 1.8)).length() < 1e-5);
        // Until measured: the target's scale.
        assert_eq!(fit_world_scale(Some(FitDef::TargetHeight(1.8)), target, None), target);
        assert_eq!(fit_world_scale(Some(FitDef::TargetHeight(0.0)), target, Some(4.0)), target);

        // The instance's local scale cancels the target's.
        assert_eq!(local_scale(Vec3::ONE, target), Vec3::splat(0.5));
        assert_eq!(local_scale(target, target), Vec3::ONE);
        assert_eq!(local_scale(Vec3::splat(3.0), Vec3::new(1.0, 0.0, 3.0)), Vec3::new(3.0, 1.0, 1.0));
    }

    #[test]
    fn world_height_of_boxes() {
        assert_eq!(world_height(Vec::new()), None);
        let unit = (Vec3::splat(-0.5), Vec3::splat(0.5));
        let h = world_height([
            (unit.0, unit.1, GlobalTransform::from_xyz(0.0, 0.5, 0.0)),
            (unit.0, unit.1, GlobalTransform::from(Transform::from_xyz(0.0, 2.0, 0.0).with_scale(Vec3::splat(2.0)))),
        ])
        .unwrap();
        // From y = 0 to y = 3.
        assert!((h - 3.0).abs() < 1e-5, "{}", h);
        let (lo, hi) = world_bounds([(unit.0, unit.1, GlobalTransform::from_xyz(1.0, 0.5, 0.0))]).unwrap();
        assert!((lo - Vec3::new(0.5, 0.0, -0.5)).length() < 1e-5 && (hi - Vec3::new(1.5, 1.0, 0.5)).length() < 1e-5);
    }

    #[test]
    fn socket_positions_and_spaces() {
        let node = GlobalTransform::from(
            Transform::from_xyz(1.0, 2.0, 0.0)
                .with_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2))
                .with_scale(Vec3::splat(10.0)),
        );
        // The node's scale is ignored, the instance's applies; the offset
        // turns with the node (+X becomes +Y).
        let p = socket_world_position(&node, Vec3::splat(2.0), Vec3::X);
        assert!((p - Vec3::new(1.0, 4.0, 0.0)).length() < 1e-5, "{}", p);
        // Dragging a socket to a world point gives back the offset.
        let offset = Vec3::new(0.3, -0.2, 0.5);
        let world = socket_world_position(&node, Vec3::new(2.0, 1.0, 0.5), offset);
        let back = socket_offset_for(&node, Vec3::new(2.0, 1.0, 0.5), world);
        assert!((back - offset).length() < 1e-5, "{}", back);

        let layer = GlobalTransform::from(Transform::from_xyz(1.0, 0.0, 0.0).with_scale(Vec3::splat(2.0)));
        let local = to_simulation_space(Vec3::new(3.0, 2.0, 0.0), &layer, SimulationSpaceDef::Local);
        assert!((local - Vec3::new(1.0, 1.0, 0.0)).length() < 1e-5, "{}", local);
        let global = to_simulation_space(Vec3::new(3.0, 2.0, 0.0), &layer, SimulationSpaceDef::Global);
        assert!((global - Vec3::new(2.0, 2.0, 0.0)).length() < 1e-5, "{}", global);
    }

    #[test]
    fn socket_lookup_prefers_package_sockets() {
        let mut project = ProjectV2::default();
        project.sockets.push(SocketDef {
            name: "tip".into(),
            node: None,
            offset: Vec3::Y,
        });
        project.sockets.push(SocketDef {
            name: "hand".into(),
            node: Some("mixamorig:RightHand".into()),
            offset: Vec3::X,
        });
        assert_eq!(socket_lookup(&project, "tip"), (None, Vec3::Y));
        assert_eq!(socket_lookup(&project, "hand"), (Some("mixamorig:RightHand".into()), Vec3::X));
        assert_eq!(socket_lookup(&project, "Head"), (Some("Head".into()), Vec3::ZERO));
    }
}
