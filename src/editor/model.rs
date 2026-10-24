//! The CAPSULE and MODEL stages: the preview target gets a body (a capsule,
//! or `preview.model` loaded from the game assets folder as a glTF scene), an
//! optional weapon model held by one of its nodes, an animation clip (from
//! the model's file or another file with the same node names), and the
//! preview instance is spawned on the node the game would use
//! (`preview.attach_to`, on the body or the weapon).
//!
//! Also here: the socket gizmos (a dot + label per socket, a line per
//! segment layer, drawn over the model), dragging a socket's dot in the
//! viewport (in the camera plane), and the world bounds of the instance
//! target's own meshes (random trigger points).
//!
//! Layout under the moving [`PreviewTarget`] (the game's entity root):
//! `PreviewTarget` > body (`WorldAssetRoot`, `root_height` below the target,
//! turned so glTF +Z faces the direction of travel) > glTF nodes > the
//! weapon (`WorldAssetRoot`, a separate model: own-model searches skip it).

use std::f32::consts::PI;
use std::sync::Arc;

use bevy::animation::{AnimatedBy, AnimationTargetId, RepeatAnimation};
use bevy::camera::primitives::Aabb;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy::world_serialization::WorldAssetRoot;

use omagari::format::{AttachDef, PreviewDef, PreviewStage, PreviewTargetModel, PreviewWeaponDef, ProjectV2};
use omagari::runtime::{local_scale, mesh_bounds, resolve_asset_path_in, socket_offset_for, socket_world_position, world_bounds, TargetHierarchy};
use omagari::{TriggerArgs, VfxInstance};

use super::camera::PreviewCamera;
use super::scene::StageFloor;
use super::session::EditorSession;
use crate::controller::{PreviewCommand, PreviewState, PreviewTarget};
use crate::model_info::ModelInfo;
use crate::settings::{GameAssets, GAME_ASSET_SOURCE};

/// Size of the CAPSULE stage's capsule: 0.7 m wide, 1.8 m tall, origin at
/// its centre (like a game character's collider and root).
pub const CAPSULE_RADIUS: f32 = 0.35;
pub const CAPSULE_HEIGHT: f32 = 1.8;
/// Root height of the user's game player (the root is 0.9 m above the
/// feet), offered as a one-click value.
pub const PLAYER_ROOT_HEIGHT: f32 = 0.9;
/// Screen distance (logical pixels) at which a socket dot can be grabbed.
const GRAB_PIXELS: f32 = 10.0;

/// The body of the preview target (capsule or model).
#[derive(Component)]
pub struct PreviewBody;

/// The weapon model.
#[derive(Component)]
pub struct PreviewWeapon;

/// Socket gizmos: drawn on top of the model (depth bias).
#[derive(Default, Reflect, GizmoConfigGroup)]
pub struct SocketGizmos;

/// What the body is.
#[derive(Clone, PartialEq, Debug)]
pub enum BodyKind {
    Capsule,
    /// A glTF file in the game assets folder and the scene to spawn.
    Model { path: String, scene: usize },
}

/// Which weapon is spawned where.
#[derive(Clone, PartialEq, Debug)]
struct WeaponKey {
    model: String,
    parent: Entity,
}

/// The playing clip.
pub struct ClipState {
    /// (glTF file, clip name).
    pub key: (String, String),
    clip: Handle<AnimationClip>,
    graph: Handle<AnimationGraph>,
    node: AnimationNodeIndex,
    /// The model's `AnimationPlayer` once the scene exists.
    player: Option<Entity>,
    /// Clip time last frame (animation triggers).
    last_time: f32,
}

/// One socket shown in the viewport.
#[derive(Clone, Debug)]
pub struct SocketMarker {
    pub name: String,
    /// Index in `project.sockets`; `None` = a node name used by a layer.
    pub package: Option<usize>,
    pub world: Vec3,
    /// Window position (logical pixels) in the active camera.
    pub screen: Option<Vec2>,
    /// The node was not found: the marker sits at the target root.
    pub pending: bool,
    /// Turns a world point into the socket's offset.
    frame: MarkerFrame,
}

#[derive(Clone, Debug)]
enum MarkerFrame {
    /// Offset from the target root (instance space).
    Root(GlobalTransform),
    /// Offset from a node, in its axes, scaled by the instance.
    Node(GlobalTransform, Vec3),
}

impl SocketMarker {
    /// The offset that puts this socket at `world`.
    pub fn offset_for(&self, world: Vec3) -> Vec3 {
        match &self.frame {
            MarkerFrame::Root(space) => space.affine().inverse().transform_point3(world),
            MarkerFrame::Node(node, scale) => socket_offset_for(node, *scale, world),
        }
    }
}

/// A socket dot being dragged.
#[derive(Clone, Debug)]
struct SocketDrag {
    socket: usize,
}

/// State of the model stage (not saved; the settings are in the project's
/// preview block).
#[derive(Resource)]
pub struct ModelPreview {
    body_kind: Option<BodyKind>,
    pub body: Option<Entity>,
    weapon_key: Option<WeaponKey>,
    pub weapon: Option<Entity>,
    /// The entity the preview instance is spawned on.
    pub instance_target: Option<Entity>,
    pub clip: Option<ClipState>,
    /// Animation controls (bottom bar).
    pub paused: bool,
    pub looping: bool,
    /// A clip time to jump to (time slider, `--anim-time`).
    pub seek: Option<f32>,
    /// Time and length of the playing clip.
    pub time: f32,
    pub duration: f32,
    /// Problems of the model stage, shown in the PREVIEW tab (rebuilt every
    /// frame).
    pub problems: Vec<String>,
    weapon_error: Option<String>,
    animation_error: Option<String>,
    /// Model files of the game assets folder (pickers); `None` = scan again.
    pub files: Option<Vec<String>>,
    /// World bounds of the instance target's own meshes.
    pub target_bounds: Option<(Vec3, Vec3)>,
    /// Socket gizmos.
    pub show_gizmos: bool,
    pub markers: Vec<SocketMarker>,
    /// Segment layers' ends (world), for the gizmo lines.
    pub segments: Vec<(Vec3, Vec3)>,
    /// The socket highlighted in the SOCKETS tab and the viewport.
    pub selected_socket: Option<String>,
    hover: Option<usize>,
    drag: Option<SocketDrag>,
    /// (body, skeleton root entity, its rest translation): "in place".
    root_bone: Option<(Entity, Entity, Vec3)>,
}

impl Default for ModelPreview {
    fn default() -> Self {
        ModelPreview {
            body_kind: None,
            body: None,
            weapon_key: None,
            weapon: None,
            instance_target: None,
            clip: None,
            paused: false,
            looping: true,
            seek: None,
            time: 0.0,
            duration: 0.0,
            problems: Vec::new(),
            weapon_error: None,
            animation_error: None,
            files: None,
            target_bounds: None,
            show_gizmos: true,
            markers: Vec::new(),
            segments: Vec::new(),
            selected_socket: None,
            hover: None,
            drag: None,
            root_bone: None,
        }
    }
}

impl ModelPreview {
    /// The model files of the game assets folder (scanned once).
    pub fn model_files(&mut self, game_assets: &GameAssets) -> &[String] {
        if self.files.is_none() {
            let found = crate::model_info::scan_models(&game_assets.assets_root(), 500);
            self.files = Some(found.into_iter().map(|(p, _)| p).collect());
        }
        self.files.as_deref().unwrap_or_default()
    }

    /// Whether a socket dot is under the cursor or being dragged (the orbit
    /// camera must not take the click).
    pub fn grabs_pointer(&self) -> bool {
        self.hover.is_some() || self.drag.is_some()
    }

    pub fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }
}

/// The CPU description of a model file of the game assets folder.
pub fn model_info(game_assets: &GameAssets, path: &str) -> Result<Arc<ModelInfo>, String> {
    let file = game_assets.assets_root().join(path);
    if !file.is_file() {
        return Err(format!("{} is not in the game assets folder ({})", path, game_assets.assets_root().display()));
    }
    ModelInfo::cached(&file)
}

/// The body a preview block asks for, and why it is not the model when the
/// model file is missing.
pub fn body_kind(preview: Option<&PreviewDef>, game_assets: &GameAssets) -> (Option<BodyKind>, Option<String>) {
    let Some(preview) = preview else { return (None, None) };
    match preview.stage {
        PreviewStage::Capsule => (Some(BodyKind::Capsule), None),
        PreviewStage::Model => match &preview.model {
            None => (Some(BodyKind::Capsule), None),
            Some(path) => match model_info(game_assets, path) {
                Ok(info) => (Some(BodyKind::Model { path: path.clone(), scene: info.scene }), None),
                Err(e) => (Some(BodyKind::Capsule), Some(format!("{}; showing the capsule", e))),
            },
        },
        _ => (None, None),
    }
}

/// The body's transform under the target: a model `root_height` below it and
/// turned half a turn (glTF models face +Z, Bevy's forward is −Z), the
/// capsule centred on it.
pub fn body_transform(kind: &BodyKind, root_height: f32) -> Transform {
    match kind {
        BodyKind::Capsule => Transform::default(),
        BodyKind::Model { .. } => Transform::from_xyz(0.0, -root_height, 0.0).with_rotation(Quat::from_rotation_y(PI)),
    }
}

/// How far below the target the floor is for a body (capsule: half its
/// height; model: the root height).
pub fn floor_depth(kind: &BodyKind, root_height: f32) -> f32 {
    match kind {
        BodyKind::Capsule => CAPSULE_HEIGHT * 0.5,
        BodyKind::Model { .. } => root_height,
    }
}

/// The weapon's local transform under a node of world scale `node_scale`:
/// offset in metres and scale independent of the node's scale (Mixamo bones
/// sit under a 0.01-scaled armature).
pub fn weapon_transform(weapon: &PreviewWeaponDef, node_scale: Vec3) -> Transform {
    let k = local_scale(Vec3::ONE, node_scale);
    Transform {
        translation: weapon.offset * k,
        rotation: weapon.quat(),
        scale: k * weapon.scale,
    }
}

/// The nearest entity named `name` below `root` (breadth first), not
/// looking inside `skip`.
fn find_named(children: &Query<&Children>, names: &Query<&Name>, root: Entity, name: &str, skip: Option<Entity>) -> Option<Entity> {
    let mut queue = std::collections::VecDeque::from([root]);
    while let Some(entity) = queue.pop_front() {
        let Ok(kids) = children.get(entity) else { continue };
        for child in kids.iter() {
            if Some(child) == skip {
                continue;
            }
            if names.get(child).is_ok_and(|n| n.as_str() == name) {
                return Some(child);
            }
            queue.push_back(child);
        }
    }
    None
}

/// Spawns, replaces and places the body and the weapon, and restarts the
/// preview instance when they change (its fit and sockets start over).
#[allow(clippy::too_many_arguments)]
pub fn sync_preview_models(
    mut commands: Commands,
    project: Res<ProjectV2>,
    game_assets: Res<GameAssets>,
    asset_server: Res<AssetServer>,
    mut model: ResMut<ModelPreview>,
    mut floor: ResMut<StageFloor>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    target: Single<Entity, With<PreviewTarget>>,
    state: Res<PreviewState>,
    mut preview: MessageWriter<PreviewCommand>,
    mut transforms: Query<&mut Transform>,
    globals: Query<&GlobalTransform>,
    children: Query<&Children>,
    names: Query<&Name>,
) {
    let target = *target;
    let preview_def = project.preview.as_ref();
    let root_height = preview_def.map(|p| p.root_height).unwrap_or(0.0);
    let (kind, problem) = body_kind(preview_def, &game_assets);
    model.problems.clear();
    model.problems.extend(problem);
    let mut restart = false;

    // The body.
    if kind != model.body_kind {
        if let Some(body) = model.body.take() {
            commands.entity(body).try_despawn();
        }
        model.weapon = None;
        model.weapon_key = None;
        model.clip = None;
        model.body = kind.as_ref().map(|kind| {
            let transform = body_transform(kind, root_height);
            match kind {
                BodyKind::Capsule => commands
                    .spawn((
                        Name::new("preview capsule"),
                        PreviewBody,
                        Mesh3d(meshes.add(Capsule3d::new(CAPSULE_RADIUS, CAPSULE_HEIGHT - 2.0 * CAPSULE_RADIUS))),
                        MeshMaterial3d(materials.add(StandardMaterial {
                            base_color: Color::srgb(0.55, 0.57, 0.62),
                            perceptual_roughness: 0.8,
                            ..default()
                        })),
                        transform,
                        ChildOf(target),
                    ))
                    .id(),
                BodyKind::Model { path, scene } => commands
                    .spawn((
                        Name::new("preview model"),
                        PreviewBody,
                        WorldAssetRoot(asset_server.load(
                            GltfAssetLabel::Scene(*scene).from_asset(resolve_asset_path_in(path, Some(GAME_ASSET_SOURCE))),
                        )),
                        transform,
                        ChildOf(target),
                    ))
                    .id(),
            }
        });
        model.body_kind = kind.clone();
        restart = true;
    } else if let (Some(body), Some(kind)) = (model.body, &kind) {
        let wanted = body_transform(kind, root_height);
        if let Ok(mut transform) = transforms.get_mut(body) {
            if *transform != wanted {
                *transform = wanted;
            }
        }
    }
    floor.set_if_neq(StageFloor(kind.as_ref().map(|k| floor_depth(k, root_height))));

    // The weapon: under its node of the body (once the scene has it), or
    // the target without a node.
    let weapon_def = preview_def.and_then(|p| p.weapon.as_ref()).filter(|w| !w.model.is_empty() && kind.is_some());
    let parent = weapon_def.and_then(|w| match (&w.node, model.body) {
        (None, _) => Some(target),
        (Some(node), Some(body)) => find_named(&children, &names, body, node, model.weapon),
        (Some(_), None) => None,
    });
    let wanted_key = weapon_def.zip(parent).map(|(w, parent)| WeaponKey { model: w.model.clone(), parent });
    if wanted_key != model.weapon_key {
        if let Some(weapon) = model.weapon.take() {
            commands.entity(weapon).try_despawn();
        }
        model.weapon_error = None;
        model.weapon = wanted_key.as_ref().and_then(|key| {
            let info = match model_info(&game_assets, &key.model) {
                Ok(info) => info,
                Err(e) => {
                    model.weapon_error = Some(format!("weapon: {}", e));
                    return None;
                }
            };
            let node_scale = globals.get(key.parent).map(|g| g.scale()).unwrap_or(Vec3::ONE);
            Some(
                commands
                    .spawn((
                        Name::new("preview weapon"),
                        PreviewWeapon,
                        WorldAssetRoot(asset_server.load(
                            GltfAssetLabel::Scene(info.scene)
                                .from_asset(resolve_asset_path_in(&key.model, Some(GAME_ASSET_SOURCE))),
                        )),
                        weapon_transform(weapon_def.unwrap(), node_scale),
                        ChildOf(key.parent),
                    ))
                    .id(),
            )
        });
        model.weapon_key = wanted_key;
        restart = true;
    } else if let (Some(weapon), Some(w), Some(key)) = (model.weapon, weapon_def, &model.weapon_key) {
        let node_scale = globals.get(key.parent).map(|g| g.scale()).unwrap_or(Vec3::ONE);
        let wanted = weapon_transform(w, node_scale);
        if let Ok(mut transform) = transforms.get_mut(weapon) {
            if !transform_close(&transform, &wanted) {
                *transform = wanted;
            }
        }
    }
    if let Some(e) = model.weapon_error.clone().filter(|_| weapon_def.is_some()) {
        model.problems.push(e);
    }
    if let Some(w) = weapon_def {
        if parent.is_none() {
            if let Some(node) = &w.node {
                model.problems.push(format!("weapon: waiting for node \"{}\" in the model", node));
            }
        }
    }

    if restart && !state.instances.is_empty() {
        preview.write(PreviewCommand::Restart);
    }
}

fn transform_close(a: &Transform, b: &Transform) -> bool {
    a.translation.abs_diff_eq(b.translation, 1e-5)
        && a.rotation.abs_diff_eq(b.rotation, 1e-5)
        && a.scale.abs_diff_eq(b.scale, 1e-4)
}

/// Picks the entity the preview instance is spawned on (the target root,
/// the weapon, or the `attach_to` node of one of them) and restarts the
/// instance when it changes (e.g. the model finished loading).
#[allow(clippy::too_many_arguments)]
pub fn resolve_instance_target(
    project: Res<ProjectV2>,
    mut model: ResMut<ModelPreview>,
    target: Single<Entity, With<PreviewTarget>>,
    state: Res<PreviewState>,
    mut preview: MessageWriter<PreviewCommand>,
    children: Query<&Children>,
    names: Query<&Name>,
) {
    let root = *target;
    let preview_def = project.preview.as_ref();
    let has_body = model.body.is_some();
    let wanted = match preview_def {
        Some(p) if has_body => {
            let (base, skip) = match p.target {
                PreviewTargetModel::Weapon if model.weapon.is_some() => (model.weapon.unwrap(), None),
                _ => (root, model.weapon),
            };
            match &p.attach_to {
                None => base,
                Some(node) => find_named(&children, &names, base, node, skip).unwrap_or(base),
            }
        }
        _ => root,
    };
    if model.instance_target != Some(wanted) {
        let first = model.instance_target.is_none();
        model.instance_target = Some(wanted);
        if !first && !state.instances.is_empty() {
            preview.write(PreviewCommand::Restart);
        }
    }
}

/// Plays the preview animation on the body model: loads the clip (by name
/// from the model's file or `animation_file`), gives the model's
/// `AnimationPlayer` a graph with it, and applies speed, pause, loop and
/// seeking; fires the animation triggers when the clip passes their time.
#[allow(clippy::too_many_arguments)]
pub fn drive_model_animation(
    mut commands: Commands,
    project: Res<ProjectV2>,
    game_assets: Res<GameAssets>,
    asset_server: Res<AssetServer>,
    mut model: ResMut<ModelPreview>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    clips: Res<Assets<AnimationClip>>,
    mut state: ResMut<PreviewState>,
    mut players: Query<&mut AnimationPlayer>,
    children: Query<&Children>,
    names: Query<&Name>,
) {
    let preview = project.preview.as_ref();
    let wanted = preview.filter(|p| p.shows_model()).and_then(|p| {
        let clip = p.animation.clone()?;
        Some((p.animation_source()?.to_string(), clip))
    });
    if wanted.as_ref() != model.clip.as_ref().map(|c| &c.key) {
        // Stop what the old clip was doing.
        if let Some(player) = model.clip.as_ref().and_then(|c| c.player) {
            if let Ok(mut player) = players.get_mut(player) {
                player.stop_all();
            }
        }
        model.clip = None;
        model.time = 0.0;
        model.duration = 0.0;
        model.animation_error = None;
        if let Some((file, name)) = wanted {
            match model_info(&game_assets, &file).and_then(|info| {
                info.animation(&name).map(|a| (a.index, a.duration)).ok_or_else(|| {
                    let known: Vec<String> = info.animations.iter().map(|a| format!("\"{}\"", a.label())).collect();
                    format!("{} has no animation \"{}\" ({})", file, name, if known.is_empty() { "none".into() } else { known.join(", ") })
                })
            }) {
                Ok((index, duration)) => {
                    let clip: Handle<AnimationClip> = asset_server
                        .load(GltfAssetLabel::Animation(index).from_asset(resolve_asset_path_in(&file, Some(GAME_ASSET_SOURCE))));
                    let (graph, node) = AnimationGraph::from_clip(clip.clone());
                    model.duration = duration;
                    model.clip = Some(ClipState {
                        key: (file, name),
                        clip,
                        graph: graphs.add(graph),
                        node,
                        player: None,
                        last_time: 0.0,
                    });
                }
                Err(e) => model.animation_error = Some(format!("animation: {}", e)),
            }
        }
    }
    if let Some(e) = model.clip.as_ref().and_then(|c| {
        asset_server.load_state(&c.clip).is_failed().then(|| format!("animation: {} did not load", c.key.0))
    }) {
        model.animation_error = Some(e);
    }
    if let Some(e) = model.animation_error.clone() {
        model.problems.push(e);
    }

    let Some(body) = model.body else { return };
    let ModelPreview { clip, paused, looping, seek, time, duration, .. } = &mut *model;
    let Some(clip) = clip.as_mut() else { return };

    // The model's player: Bevy adds one to models with animations; a model
    // without any gets animation targets by node name here.
    let player = descendants(&children, body).into_iter().find(|e| players.contains(*e));
    let Some(player_entity) = player else {
        if let Some(scene) = children.get(body).ok().and_then(|c| c.first().copied()) {
            add_animation_targets(&mut commands, &children, &names, scene);
        }
        return;
    };
    let Ok(mut player) = players.get_mut(player_entity) else { return };
    if clip.player != Some(player_entity) {
        commands.entity(player_entity).insert(AnimationGraphHandle(clip.graph.clone()));
        player.stop_all();
        player.play(clip.node).repeat();
        clip.player = Some(player_entity);
        clip.last_time = 0.0;
    }
    if let Some(c) = clips.get(&clip.clip) {
        *duration = c.duration();
    }
    let speed = preview.map(|p| p.animation_speed).unwrap_or(1.0);
    let Some(active) = player.animation_mut(clip.node) else {
        player.play(clip.node);
        return;
    };
    active.set_speed(speed);
    active.set_repeat(if *looping { RepeatAnimation::Forever } else { RepeatAnimation::Never });
    if *looping && active.is_finished() {
        active.replay();
    }
    if *paused && !active.is_paused() {
        active.pause();
    } else if !*paused && active.is_paused() {
        active.resume();
    }
    let jumped = seek.take().map(|t| {
        active.set_seek_time(t.clamp(0.0, duration.max(0.0)));
    });
    *time = active.seek_time();

    // Animation triggers: fired when the clip time passes them.
    if jumped.is_none() && !*paused {
        let triggers = preview.map(|p| p.animation_triggers.as_slice()).unwrap_or_default();
        for (name, at) in triggers {
            if passes(clip.last_time, *time, *at, speed) {
                state.pending_fires.push((name.clone(), TriggerArgs::default(), 0));
            }
        }
    }
    clip.last_time = *time;
}

/// The local translation of a skeleton root that keeps only the vertical
/// part of its animated motion: `animated` and `rest` are local
/// translations (in the parent's space), `parent_rotation` the parent's
/// world rotation. The horizontal (world XZ) offset from the rest position
/// is removed, so a clip with root motion plays in place.
pub fn in_place_translation(animated: Vec3, rest: Vec3, parent_rotation: Quat) -> Vec3 {
    let world = parent_rotation * (animated - rest);
    rest + parent_rotation.inverse() * Vec3::new(0.0, world.y, 0.0)
}

/// The skeleton root of a model: the first bone whose parent is not a bone
/// (Mixamo: `mixamorig:Hips`), with its rest translation.
pub fn skeleton_root(info: &ModelInfo) -> Option<(String, Vec3)> {
    info.nodes
        .iter()
        .find(|n| n.bone && n.parent.is_none_or(|p| !info.nodes[p].bone))
        .map(|n| (n.name.clone(), n.rest.to_scale_rotation_translation().2))
}

/// The preview's "in place" (`preview.in_place`, default on): after the
/// animation, the model's skeleton root loses its horizontal root motion so
/// the model stays on the target (editor preview only; the game decides
/// root motion itself).
#[allow(clippy::too_many_arguments)]
pub fn keep_model_in_place(
    project: Res<ProjectV2>,
    game_assets: Res<GameAssets>,
    mut model: ResMut<ModelPreview>,
    children: Query<&Children>,
    names: Query<&Name>,
    parents: Query<&ChildOf>,
    globals: Query<&GlobalTransform>,
    mut transforms: Query<&mut Transform>,
) {
    let Some(preview) = project.preview.as_ref().filter(|p| p.shows_model() && p.in_place) else { return };
    let (Some(body), Some(_)) = (model.body, model.clip.as_ref()) else { return };
    let Some(path) = preview.model.as_deref() else { return };
    let cached = model.root_bone.filter(|(b, e, _)| *b == body && transforms.contains(*e));
    let (entity, rest) = match cached {
        Some((_, e, rest)) => (e, rest),
        None => {
            let Ok(info) = model_info(&game_assets, path) else { return };
            let Some((name, rest)) = skeleton_root(&info) else { return };
            let Some(e) = find_named(&children, &names, body, &name, None) else { return };
            model.root_bone = Some((body, e, rest));
            (e, rest)
        }
    };
    let parent_rotation = parents
        .get(entity)
        .ok()
        .and_then(|p| globals.get(p.parent()).ok())
        .map(|g| g.to_scale_rotation_translation().1)
        .unwrap_or(Quat::IDENTITY);
    if let Ok(mut transform) = transforms.get_mut(entity) {
        let wanted = in_place_translation(transform.translation, rest, parent_rotation);
        if transform.translation != wanted {
            transform.translation = wanted;
        }
    }
}

/// Draws the samples of trail layers (the TRAIL inspector's "show
/// samples"): a line from From to To per sample, and both edges.
pub fn draw_trail_samples(
    session: Res<super::session::EditorSession>,
    trails: Query<&omagari::runtime::TrailLayerState>,
    mut gizmos: Gizmos<SocketGizmos>,
) {
    if !session.show_trail_samples {
        return;
    }
    for state in trails.iter() {
        let mut previous: Option<(Vec3, Vec3)> = None;
        for s in state.samples.samples() {
            gizmos.line(s.a, s.b, Color::srgb(1.0, 0.9, 0.2));
            if let Some((a, b)) = previous {
                gizmos.line(a, s.a, Color::srgb(0.3, 0.9, 1.0));
                gizmos.line(b, s.b, Color::srgb(1.0, 0.4, 0.9));
            }
            previous = Some((s.a, s.b));
        }
    }
}

/// Whether a clip going from `last` to `now` (forward when `speed` ≥ 0,
/// wrapping around at the loop) passed time `at`.
pub fn passes(last: f32, now: f32, at: f32, speed: f32) -> bool {
    if speed >= 0.0 {
        if now >= last { last < at && at <= now } else { at > last || at <= now }
    } else if now <= last {
        now <= at && at < last
    } else {
        at < last || at >= now
    }
}

/// Every entity below `root`.
fn descendants(children: &Query<&Children>, root: Entity) -> Vec<Entity> {
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(e) = stack.pop() {
        if let Ok(kids) = children.get(e) {
            for k in kids.iter() {
                out.push(k);
                stack.push(k);
            }
        }
    }
    out
}

/// Makes a model without animations animatable: each root node of the
/// glTF scene entity gets an `AnimationPlayer`, and every named node below
/// it the `AnimationTargetId` of its name path, like Bevy's loader does for
/// animated models, so clips from other files play on it by node names.
fn add_animation_targets(commands: &mut Commands, children: &Query<&Children>, names: &Query<&Name>, scene: Entity) {
    let Ok(roots) = children.get(scene) else { return };
    for root in roots.iter() {
        let Ok(name) = names.get(root) else { continue };
        commands.entity(root).insert(AnimationPlayer::default());
        let mut stack = vec![(root, vec![name.clone()])];
        while let Some((entity, path)) = stack.pop() {
            commands
                .entity(entity)
                .insert((AnimationTargetId::from_names(path.iter()), AnimatedBy(root)));
            if let Ok(kids) = children.get(entity) {
                for kid in kids.iter() {
                    if let Ok(n) = names.get(kid) {
                        let mut p = path.clone();
                        p.push(n.clone());
                        stack.push((kid, p));
                    }
                }
            }
        }
    }
}

/// A random trigger point on the surface of world bounds: around the box's
/// vertical axis, between 20% and 90% of its height, pointing outward (a hit
/// on a body). `a`, `b`, `c` are random numbers in 0..1.
pub fn random_point_on_bounds(min: Vec3, max: Vec3, a: f32, b: f32, c: f32) -> TriggerArgs {
    let center = (min + max) * 0.5;
    let half = ((max - min) * 0.5).max(Vec3::splat(0.01));
    let angle = a * std::f32::consts::TAU;
    let (sin, cos) = angle.sin_cos();
    let outward = Vec3::new(cos, 0.0, sin);
    let y = min.y + (0.2 + 0.7 * b) * (max.y - min.y);
    // On the ellipse through the box's sides, a little inside or outside.
    let reach = 0.9 + 0.2 * c;
    let point = Vec3::new(center.x + cos * half.x * reach, y, center.z + sin * half.z * reach);
    TriggerArgs::at(point).with_direction(outward)
}

/// After transforms are propagated: the world bounds of the instance
/// target's own meshes, the socket markers and segment ends, and the socket
/// gizmos.
#[allow(clippy::too_many_arguments)]
pub fn update_socket_markers(
    project: Res<ProjectV2>,
    mut model: ResMut<ModelPreview>,
    state: Res<PreviewState>,
    hierarchy: TargetHierarchy,
    globals: Query<&GlobalTransform>,
    instances: Query<&GlobalTransform, With<VfxInstance>>,
    mesh_entities: Query<(&Mesh3d, Option<&Aabb>)>,
    meshes: Res<Assets<Mesh>>,
    cameras: Query<(&Camera, &GlobalTransform), With<PreviewCamera>>,
    root: Single<Entity, With<PreviewTarget>>,
    mut gizmos: Gizmos<SocketGizmos>,
) {
    let target = model.instance_target.unwrap_or(*root);
    let Ok(target_global) = globals.get(target) else { return };

    let boxes = hierarchy.own_meshes(target).into_iter().filter_map(|e| {
        let (mesh, aabb) = mesh_entities.get(e).ok()?;
        let (min, max) = mesh_bounds(aabb, meshes.get(&mesh.0))?;
        Some((min, max, *globals.get(e).ok()?))
    });
    model.target_bounds = world_bounds(boxes);

    // Instance space: the live instance (fit scale), else the target.
    let space = state
        .instances
        .last()
        .and_then(|&i| instances.get(i).ok())
        .copied()
        .unwrap_or(*target_global);
    let instance_scale = space.scale();
    let camera = cameras.iter().find(|(c, _)| c.is_active);

    let mut names: Vec<(String, Option<usize>)> =
        project.sockets.iter().enumerate().map(|(i, s)| (s.name.clone(), Some(i))).collect();
    for layer in project.layers.iter() {
        for name in layer.socket_names() {
            if !name.is_empty() && !names.iter().any(|(n, _)| n == name) {
                names.push((name.to_string(), None));
            }
        }
    }
    let markers: Vec<SocketMarker> = names
        .into_iter()
        .map(|(name, package)| {
            let (node, offset) = omagari::runtime::socket_lookup(&project, &name);
            let found = node.as_deref().map(|n| hierarchy.find_node(target, n));
            let (world, frame, pending) = match found {
                Some(Some(node)) => {
                    let node_global = globals.get(node).copied().unwrap_or_default();
                    (
                        socket_world_position(&node_global, instance_scale, offset),
                        MarkerFrame::Node(node_global, instance_scale),
                        false,
                    )
                }
                other => (space.transform_point(offset), MarkerFrame::Root(space), other.is_some()),
            };
            let screen = camera.and_then(|(c, g)| c.world_to_viewport(g, world).ok());
            SocketMarker { name, package, world, screen, pending, frame }
        })
        .collect();
    let at = |n: &str| markers.iter().find(|m| m.name == n).map(|m| m.world);
    let segments: Vec<(Vec3, Vec3)> = project
        .layers
        .iter()
        .filter_map(|l| match (l.attach(), l.as_trail()) {
            (Some(AttachDef::Segment { from, to }), _) => Some((at(from)?, at(to)?)),
            (_, Some(trail)) => Some((at(&trail.from)?, at(&trail.to)?)),
            _ => None,
        })
        .collect();

    if model.show_gizmos {
        for (a, b) in segments.iter() {
            gizmos.line(*a, *b, Color::srgb(1.0, 0.85, 0.2));
        }
        for (i, m) in markers.iter().enumerate() {
            let selected = model.selected_socket.as_deref() == Some(m.name.as_str());
            let hot = selected || model.hover == Some(i) || model.drag.as_ref().is_some_and(|d| Some(d.socket) == m.package);
            let color = if m.pending {
                Color::srgb(1.0, 0.45, 0.3)
            } else if hot {
                Color::srgb(1.0, 0.9, 0.2)
            } else {
                Color::srgb(0.3, 0.9, 1.0)
            };
            gizmos.sphere(Isometry3d::from_translation(m.world), if hot { 0.035 } else { 0.022 }, color);
            if hot {
                gizmos.cross(Isometry3d::from_translation(m.world), 0.08, color);
            }
        }
    }
    model.markers = markers;
    model.segments = segments;
}

/// Drags package sockets in the viewport: press on a socket dot, move in the
/// camera plane; the socket's offset follows (rounded to millimetres).
#[allow(clippy::too_many_arguments)]
pub fn drag_sockets(
    mouse: Res<ButtonInput<MouseButton>>,
    window: Single<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), With<PreviewCamera>>,
    mut model: ResMut<ModelPreview>,
    mut project: ResMut<ProjectV2>,
    session: Res<EditorSession>,
) {
    let cursor = window.cursor_position();
    let camera = cameras.iter().find(|(c, _)| c.is_active);
    if model.drag.is_none() {
        let hover = cursor.filter(|_| model.show_gizmos && !session.pointer_on_ui).and_then(|cursor| {
            model
                .markers
                .iter()
                .enumerate()
                .filter(|(_, m)| m.package.is_some())
                .filter_map(|(i, m)| Some((i, m.screen?.distance(cursor))))
                .filter(|(_, d)| *d <= GRAB_PIXELS)
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(i, _)| i)
        });
        if model.hover != hover {
            model.hover = hover;
        }
        if let Some(i) = hover.filter(|_| mouse.just_pressed(MouseButton::Left)) {
            let marker = &model.markers[i];
            let (socket, name) = (marker.package.unwrap(), marker.name.clone());
            model.selected_socket = Some(name);
            model.drag = Some(SocketDrag { socket });
        }
        return;
    }
    let drag = model.drag.clone().unwrap();
    if !mouse.pressed(MouseButton::Left) {
        model.drag = None;
        return;
    }
    let (Some(cursor), Some((camera, camera_global))) = (cursor, camera) else { return };
    let Some(marker) = model.markers.iter().find(|m| m.package == Some(drag.socket)) else { return };
    let Ok(ray) = camera.viewport_to_world(camera_global, cursor) else { return };
    let plane = InfinitePlane3d::new(camera_global.forward());
    let Some(distance) = ray.intersect_plane(marker.world, plane) else { return };
    let offset = (marker.offset_for(ray.get_point(distance)) * 1000.0).round() / 1000.0;
    if let Some(socket) = project.sockets.get(drag.socket) {
        if socket.offset != offset {
            project.sockets[drag.socket].offset = offset;
        }
    }
}

/// Keeps the socket gizmos on top of the model.
pub fn configure_socket_gizmos(mut store: ResMut<GizmoConfigStore>) {
    let (config, _) = store.config_mut::<SocketGizmos>();
    config.depth_bias = -1.0;
    config.line.width = 2.5;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The model stage on generated glTF files, through Bevy's loader: a
    /// model without animations gets a clip from another file with the same
    /// node names (played by name, no retargeting), the body stands
    /// `root_height` below the target, and the instance target follows
    /// `attach_to` once the scene exists.
    #[test]
    fn clips_from_other_files_play_by_node_names() {
        use crate::controller::spawn_preview_target;
        use crate::model_info::tests::write_fixture;
        use bevy::asset::AssetPlugin;
        use std::time::Duration;

        let dir = crate::asset_library::tests::temp_dir("model_stage");
        write_fixture(&dir.join("chars/hero.gltf"), None);
        write_fixture(&dir.join("anim/swing.gltf"), Some(("Swing", 0.5)));
        let game_assets = GameAssets::resolve(Some(dir.to_string_lossy().to_string()), None, &Default::default());
        let mut app = App::new();
        game_assets.register_source(&mut app);
        app.add_plugins((
            MinimalPlugins,
            AssetPlugin::default(),
            bevy::transform::TransformPlugin,
            bevy::image::ImagePlugin::default(),
            bevy::mesh::MeshPlugin,
            bevy::animation::AnimationPlugin,
            bevy::world_serialization::WorldSerializationPlugin,
            bevy::gltf::GltfPlugin::default(),
        ))
        .insert_resource(game_assets)
        .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(Duration::from_millis(50)))
        .init_asset::<StandardMaterial>()
        .init_resource::<ModelPreview>()
        .init_resource::<StageFloor>()
        .init_resource::<PreviewState>()
        .add_message::<PreviewCommand>()
        .add_systems(Startup, spawn_preview_target)
        .add_systems(Update, (sync_preview_models, resolve_instance_target, drive_model_animation).chain());
        let mut project = ProjectV2::default();
        project.triggers.push(omagari::format::TriggerDef { name: "swing".into(), pulses: Vec::new() });
        project.preview = Some(PreviewDef {
            stage: PreviewStage::Model,
            model: Some("chars/hero.gltf".into()),
            root_height: 0.9,
            animation: Some("Swing".into()),
            animation_file: Some("anim/swing.gltf".into()),
            attach_to: Some("RightHand".into()),
            animation_triggers: vec![("swing".into(), 0.2)],
            ..default()
        });
        app.insert_resource(project);
        // The glTF loader is registered in `Plugin::finish`.
        app.finish();
        app.cleanup();

        let mut hand = None;
        let mut turned = 0.0;
        let mut fired = false;
        for _ in 0..400 {
            app.update();
            fired |= !app.world().resource::<PreviewState>().pending_fires.is_empty();
            let world = app.world_mut();
            if hand.is_none() {
                hand = world
                    .query::<(Entity, &Name)>()
                    .iter(world)
                    .find(|(_, n)| n.as_str() == "RightHand")
                    .map(|(e, _)| e);
            }
            if let Some(hand) = hand {
                let rotation = world.get::<Transform>(hand).unwrap().rotation;
                turned = rotation.angle_between(Quat::IDENTITY);
                if turned > 0.3 && fired {
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let hand = hand.expect("the model's scene was spawned");
        assert!(turned > 0.3, "the clip of the other file turned RightHand by {} rad", turned);
        assert!(fired, "the animation trigger at 0.2 s was fired");
        let model = app.world().resource::<ModelPreview>();
        assert_eq!(model.instance_target, Some(hand), "attach_to RightHand");
        let body = model.body.unwrap();
        assert_eq!(app.world().get::<Transform>(body).unwrap().translation, Vec3::new(0.0, -0.9, 0.0));
        assert_eq!(app.world().resource::<StageFloor>().0, Some(0.9));
        assert!(model.duration > 0.49 && model.duration < 0.51, "{}", model.duration);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn bodies_stand_on_the_floor_and_face_forward() {
        let model = BodyKind::Model { path: "a.glb".into(), scene: 0 };
        let t = body_transform(&model, 0.9);
        assert_eq!(t.translation, Vec3::new(0.0, -0.9, 0.0));
        // glTF +Z (the model's front) becomes the target's forward (−Z).
        assert!((t.rotation * Vec3::Z - Vec3::NEG_Z).length() < 1e-5);
        assert_eq!(floor_depth(&model, 0.9), 0.9);
        assert_eq!(body_transform(&BodyKind::Capsule, 0.9), Transform::default());
        assert_eq!(floor_depth(&BodyKind::Capsule, 0.0), 0.9, "half the capsule");
    }

    #[test]
    fn in_place_removes_only_horizontal_root_motion() {
        let rest = Vec3::new(0.0, 100.0, 0.0);
        // Parent without rotation: X / Z go back to rest, Y (the bob) stays.
        let t = in_place_translation(Vec3::new(30.0, 95.0, -70.0), rest, Quat::IDENTITY);
        assert!((t - Vec3::new(0.0, 95.0, 0.0)).length() < 1e-4, "{}", t);
        // A parent turned 90° about X (local Z is world up): local Z is the
        // vertical that stays, local X / Y are horizontal.
        let parent = Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);
        let rest = Vec3::new(0.0, 0.0, 1.0);
        let t = in_place_translation(Vec3::new(0.4, 0.3, 0.9), rest, parent);
        let world = parent * (t - rest);
        assert!(world.x.abs() < 1e-5 && world.z.abs() < 1e-5, "no horizontal motion: {}", world);
        assert!((world.y - (parent * Vec3::new(0.4, 0.3, -0.1)).y).abs() < 1e-5, "vertical kept");
        // Idempotent: applying it twice changes nothing.
        let again = in_place_translation(t, rest, parent);
        assert!((again - t).length() < 1e-6);
    }

    #[test]
    fn skeleton_roots_are_the_top_bones() {
        let dir = crate::asset_library::tests::temp_dir("skeleton_root");
        crate::model_info::tests::write_fixture(&dir.join("hero.gltf"), Some(("Wave", 1.0)));
        let info = ModelInfo::load(&dir.join("hero.gltf")).unwrap();
        let (name, rest) = skeleton_root(&info).expect("a skeleton root");
        assert_eq!(name, "Hips", "the first bone below the (non-bone) armature");
        assert!((rest - Vec3::new(0.0, 0.9, 0.0)).length() < 1e-6);
    }

    #[test]
    fn weapons_ignore_the_bone_scale() {
        let weapon = PreviewWeaponDef {
            model: "sword.glb".into(),
            offset: Vec3::new(0.0, 0.1, 0.0),
            rotation: Vec3::new(0.0, 0.0, 90.0),
            scale: 2.0,
            ..default()
        };
        // A Mixamo bone: world scale 0.01.
        let t = weapon_transform(&weapon, Vec3::splat(0.01));
        let world = GlobalTransform::from_scale(Vec3::splat(0.01)) * GlobalTransform::from(t);
        assert!((world.translation() - Vec3::new(0.0, 0.1, 0.0)).length() < 1e-5);
        assert!((world.scale() - Vec3::splat(2.0)).length() < 1e-4);
        assert!((t.rotation * Vec3::X - Vec3::Y).length() < 1e-5);
    }

    #[test]
    fn animation_times_are_passed_once_per_loop() {
        assert!(passes(0.2, 0.35, 0.3, 1.0));
        assert!(!passes(0.3, 0.4, 0.3, 1.0), "already fired at 0.3");
        assert!(!passes(0.1, 0.2, 0.3, 1.0));
        // Wrapped from 0.9 to 0.05: times after 0.9 and up to 0.05 passed.
        assert!(passes(0.9, 0.05, 0.95, 1.0) && passes(0.9, 0.05, 0.0, 1.0));
        assert!(!passes(0.9, 0.05, 0.5, 1.0));
        // Backwards.
        assert!(passes(0.4, 0.2, 0.3, -1.0) && !passes(0.4, 0.2, 0.5, -1.0));
    }

    #[test]
    fn random_points_lie_on_the_bounds_and_point_outward() {
        let (min, max) = (Vec3::new(-0.3, 0.0, -0.2), Vec3::new(0.3, 1.8, 0.2));
        for (a, b, c) in [(0.0, 0.0, 0.0), (0.25, 1.0, 1.0), (0.6, 0.5, 0.5)] {
            let args = random_point_on_bounds(min, max, a, b, c);
            let p = args.position.unwrap();
            assert!(p.y >= 0.36 - 1e-5 && p.y <= 1.62 + 1e-5, "{}", p);
            assert!(p.x.abs() <= 0.34 && p.z.abs() <= 0.23, "{}", p);
            let d = args.direction.unwrap();
            assert!(d.y == 0.0 && (d.length() - 1.0).abs() < 1e-5);
            assert!(d.dot(Vec3::new(p.x, 0.0, p.z)) >= 0.0, "outward");
        }
    }
}
