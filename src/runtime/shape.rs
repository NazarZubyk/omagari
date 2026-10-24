//! Shape layers at runtime: one entity per shape layer and instance, a child
//! of the instance, drawn with [`OmagariEffectMaterial`] on a generated mesh
//! ([`shape_mesh`], one per distinct shape, shared).
//!
//! - **Placement** ([`place_shapes`], `PostUpdate` after transform
//!   propagation): the shape's world transform is worked out from the point
//!   it is attached to (target root, socket, middle of a segment, or the
//!   trigger's payload position) and its follow mode, and written straight
//!   into its `GlobalTransform`, so it is never a frame behind the target and
//!   never inherits rotations it should not (`Position`, `PositionYaw`).
//! - **Play** ([`ShapePlayer`]): `Loop`, `OnStart`, `OnTrigger` (each trigger
//!   restarts the age), `While`; a `duration` makes `OnStart` / `OnTrigger`
//!   one-shots and repeats the age of `Loop` / `While` shapes.
//! - **Values**: every binding (material, `scale`, `rotation_speed`) is
//!   evaluated each frame from the parameters and the layer's age.

use bevy::camera::visibility::NoFrustumCulling;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::platform::collections::{HashMap, HashSet};
use bevy::prelude::*;

use super::attach::{SocketAttach, TargetHierarchy};
use super::effect_material::{alpha_mode_for, EffectUniforms, OmagariEffectMaterial, ShellInputs};
use super::instance::{VfxInstance, VfxState};
use super::package::{PreparedShapeLayer, VfxPackage};
use super::shape_mesh::shape_mesh;
use super::triggers::TriggerArgs;
use crate::format::{number_value, vector_value, AttachDef, FollowDef, ParamValue, PlayDef, ShapeDef, ShapeLayerDef, TimedPlay};

/// A shape layer entity of an instance (a child of the instance).
#[derive(Component, Clone, Copy, Debug)]
pub struct VfxShapeLayer {
    pub instance: Entity,
    /// Index into [`VfxPackage::shape_layers`].
    pub index: usize,
}

/// The play state of a layer with its own clock (shape and light layers)
/// as a plain state machine (no ECS).
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct ShapePlayer {
    /// Seconds since the layer started (or its trigger last played it, or a
    /// `While` parameter / `Between` start trigger last switched it on).
    pub age: f32,
    /// Whether it plays: `Loop` / `OnStart` from the start, `OnTrigger` once
    /// triggered, `While` while the parameter is high, `Between` from its
    /// start to its stop trigger; one-shots stop after their `duration`.
    pub active: bool,
    /// Fade level 0..1 (the lifecycle fade; one-shots use their envelope
    /// over the age instead and keep this at 1 until the instance stops).
    pub level: f32,
    while_on: bool,
    /// A `Between` layer's trigger state (on between start and stop).
    held: bool,
}

impl ShapePlayer {
    /// A layer that just started, at fade level `level` (a rebuilt layer
    /// keeps the level of the one it replaces).
    pub fn new(def: &impl TimedPlay, level: f32) -> Self {
        ShapePlayer {
            age: 0.0,
            active: matches!(def.play_def(), PlayDef::Loop | PlayDef::OnStart),
            level: if def.is_one_shot() { 1.0 } else { level },
            while_on: false,
            held: false,
        }
    }

    /// The trigger of an `OnTrigger` layer fired: play from age 0.
    pub fn trigger(&mut self) {
        self.age = 0.0;
        self.active = true;
    }

    /// The trigger `name` fired on a `Between` layer: it switches on at its
    /// start trigger and off at its stop trigger (the age starts over when it
    /// switches on, at the next [`ShapePlayer::step`]).
    pub fn switch(&mut self, def: &impl TimedPlay, name: &str) {
        if let Some(on) = def.play_def().on_between(name, self.held) {
            self.held = on;
        }
    }

    /// Whether a `Between` layer is between its start and stop trigger.
    pub fn is_held(&self) -> bool {
        self.held
    }

    /// Advances by `dt`. `running` = the instance runs (not stopping);
    /// `while_value` = the current value of a `While` layer's parameter.
    pub fn step(&mut self, def: &impl TimedPlay, dt: f32, running: bool, while_value: Option<f32>) {
        self.age += dt;
        if !running {
            self.held = false;
        }
        let switched = match def.play_def() {
            PlayDef::While(_, threshold) => Some(running && while_value.is_some_and(|v| v >= *threshold)),
            PlayDef::Between { .. } => Some(running && self.held),
            _ => None,
        };
        if let Some(on) = switched {
            if on && !self.while_on {
                self.age = 0.0;
            }
            self.while_on = on;
            self.active = on;
        }
        let fade = def.fade_def();
        if let Some(duration) = def.one_shot_duration() {
            if self.active && self.age >= duration {
                self.active = false;
            }
            self.level = if running { 1.0 } else { fade.step(self.level, false, dt) };
        } else {
            self.level = fade.step(self.level, running && self.active, dt);
        }
    }

    /// Opacity factor: the fade level, times the envelope of a one-shot (0
    /// when it is not playing).
    pub fn opacity(&self, def: &impl TimedPlay) -> f32 {
        if def.is_one_shot() {
            if self.active {
                self.level * def.envelope(self.age)
            } else {
                0.0
            }
        } else {
            self.level
        }
    }

    /// The age `Age` bindings read (repeating for layers that are not
    /// one-shots and have a duration).
    pub fn binding_age(&self, def: &impl TimedPlay) -> f32 {
        def.binding_age(self.age)
    }
}

/// Runtime state of a shape layer.
#[derive(Component, Clone, Debug, Default)]
pub struct ShapeLayerState {
    pub player: ShapePlayer,
    /// `visible` factor (1 without `visible`).
    pub visibility: f32,
    /// The animated scale (the `scale` binding).
    pub scale: Vec3,
    /// Degrees turned around the rotation axis so far (`rotation_speed`
    /// integrated over time).
    pub spin: f32,
    /// World position of the last trigger's payload: an `OnTrigger` shape
    /// plays there instead of at its attach point.
    pub trigger_position: Option<Vec3>,
    /// Sockets of a `Socket` / `Segment` attach.
    pub attach: Option<SocketAttach>,
    pub material: Option<Handle<OmagariEffectMaterial>>,
    pub mesh: Option<Handle<Mesh>>,
    /// A trigger waiting for the next update.
    pub(crate) pending_trigger: Option<TriggerArgs>,
    /// `Between` triggers waiting for the next update.
    pub(crate) pending_switches: Vec<String>,
    mesh_key: String,
    generation: u32,
    started: bool,
    hidden: bool,
    start_level: f32,
}

impl ShapeLayerState {
    /// A state whose fade starts at `level` (a rebuilt looping layer keeps
    /// the level of the one it replaces, so live edits do not blink).
    pub fn with_level(level: f32) -> Self {
        ShapeLayerState {
            visibility: 1.0,
            scale: Vec3::ONE,
            start_level: level,
            ..default()
        }
    }

    /// Whether the layer is drawn this frame.
    pub fn is_shown(&self) -> bool {
        !self.hidden
    }
}

/// Meshes of the shapes in use, one per distinct shape definition.
#[derive(Resource, Default)]
pub struct ShapeMeshCache {
    meshes: HashMap<String, Handle<Mesh>>,
}

impl ShapeMeshCache {
    fn key(shape: &ShapeDef) -> String {
        // Quads facing the camera use the flat quad's mesh.
        format!("{:?}", shape)
    }

    /// The mesh of `shape`, made on first use.
    pub fn get(&mut self, shape: &ShapeDef, meshes: &mut Assets<Mesh>) -> Handle<Mesh> {
        self.meshes
            .entry(Self::key(shape))
            .or_insert_with(|| meshes.add(shape_mesh(shape)))
            .clone()
    }

    /// Number of cached meshes.
    pub fn len(&self) -> usize {
        self.meshes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.meshes.is_empty()
    }
}

/// The point a shape is attached to: a world position and rotation.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ShapeAnchor {
    pub position: Vec3,
    pub rotation: Quat,
}

/// The rotation about world Y only (the heading) of `rotation`.
pub fn yaw_only(rotation: Quat) -> Quat {
    let forward = rotation * Vec3::NEG_Z;
    let flat = Vec3::new(forward.x, 0.0, forward.z);
    let flat = if flat.length_squared() > 1e-8 {
        flat
    } else {
        // Looking straight up or down: the heading of its right axis.
        let right = rotation * Vec3::X;
        Vec3::new(-right.z, 0.0, right.x)
    };
    if flat.length_squared() <= 1e-8 {
        return Quat::IDENTITY;
    }
    let flat = flat.normalize();
    // Quat::from_rotation_y(a) turns -Z to (-sin a, 0, -cos a).
    Quat::from_rotation_y(f32::atan2(-flat.x, -flat.z))
}

/// The world transform of a shape: `offset` (metres × `fit`) and `rotation`
/// in the space the follow mode keeps from the anchor (`Full`: its rotation;
/// `Position`: world axes; `PositionYaw`: its heading), scaled by `scale`
/// × `fit`.
pub fn shape_world_transform(
    anchor: ShapeAnchor,
    follow: FollowDef,
    fit: f32,
    offset: Vec3,
    rotation: Quat,
    scale: Vec3,
) -> Transform {
    let frame = match follow {
        FollowDef::Full => anchor.rotation,
        FollowDef::Position => Quat::IDENTITY,
        FollowDef::PositionYaw => yaw_only(anchor.rotation),
    };
    Transform {
        translation: anchor.position + frame * (offset * fit),
        rotation: (frame * rotation).normalize(),
        scale: scale * fit,
    }
}

/// The average world scale of an instance (the size fit), positive.
fn fit_of(instance: &GlobalTransform) -> f32 {
    let s = instance.scale().abs();
    ((s.x + s.y + s.z) / 3.0).max(1e-4)
}

/// Spawns the entity of one shape layer of an instance.
pub(crate) fn spawn_shape_layer(
    commands: &mut Commands,
    instance: Entity,
    index: usize,
    prepared: &PreparedShapeLayer,
    project: &crate::format::ProjectV2,
    level: f32,
) -> Entity {
    let mut state = ShapeLayerState::with_level(level);
    if prepared.def.attach != AttachDef::Target {
        state.attach = Some(SocketAttach::new(&prepared.def.attach, project));
    }
    let mut e = commands.spawn((
        Name::new(prepared.name().to_string()),
        VfxShapeLayer { instance, index },
        state,
        Transform::default(),
        Visibility::Hidden,
        NotShadowCaster,
        NotShadowReceiver,
        ChildOf(instance),
    ));
    if prepared.def.shape.faces_camera() {
        // Its bounds are the flat quad's; the shader turns it to the camera.
        e.insert(NoFrustumCulling);
    }
    e.id()
}

/// Plays shape layers: meshes and materials, sockets, triggers, age, fades,
/// visibility, and the bound values (written into the material when they
/// change).
#[allow(clippy::too_many_arguments)]
pub(crate) fn update_shapes(
    mut commands: Commands,
    time: Res<Time>,
    packages: Res<Assets<VfxPackage>>,
    materials: Option<ResMut<Assets<OmagariEffectMaterial>>>,
    meshes: Option<ResMut<Assets<Mesh>>>,
    mut cache: ResMut<ShapeMeshCache>,
    hierarchy: TargetHierarchy,
    changed: Query<(), (Or<(Changed<Children>, Changed<Name>)>, Without<VfxShapeLayer>)>,
    instances: Query<(&VfxInstance, &GlobalTransform)>,
    mut layers: Query<(Entity, &VfxShapeLayer, &mut ShapeLayerState, &mut Visibility)>,
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
        let Some(prepared) = package.shape_layers.get(layer.index) else { continue };
        if !instance.params.is_synced(instance.package.id()) {
            continue;
        }
        let def = &prepared.def;

        // Mesh and material.
        let key = ShapeMeshCache::key(&def.shape);
        if state.mesh.is_none() || state.mesh_key != key {
            let mesh = cache.get(&def.shape, &mut meshes);
            commands.entity(entity).insert(Mesh3d(mesh.clone()));
            state.mesh = Some(mesh);
            state.mesh_key = key.clone();
        }
        used.insert(key);
        if state.material.is_none() {
            let material = materials.add(OmagariEffectMaterial {
                uniforms: EffectUniforms::default(),
                noise_a: prepared.textures[0].clone(),
                noise_b: prepared.textures[1].clone(),
                mask: prepared.textures[2].clone(),
                alpha_mode: alpha_mode_for(def.material.blend),
                sort_bias: def.sort_bias,
            });
            commands.entity(entity).insert(MeshMaterial3d(material.clone()));
            state.material = Some(material);
        }

        // Start (and start over on a restart).
        if !state.started || state.generation != instance.generation {
            state.player = ShapePlayer::new(def, state.start_level);
            state.started = true;
            state.generation = instance.generation;
            state.spin = 0.0;
            state.trigger_position = None;
        }

        if let Some(attach) = &mut state.attach {
            attach.update(&hierarchy, instance.target(), any_change, dt, def.name.as_str());
        }

        let running = matches!(instance.state(), VfxState::Running | VfxState::Starting);
        if let Some(args) = state.pending_trigger.take() {
            if running {
                state.player.trigger();
                state.trigger_position = args.position;
            }
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
        state.scale = vector_value(&def.scale, &lookup, age, Vec3::ONE);
        let speed = number_value(&def.rotation_speed, &lookup, age, 0.0);
        if state.player.active {
            state.spin = (state.spin + speed * dt).rem_euclid(360.0);
        }
        let opacity = state.player.opacity(def) * state.visibility;
        let values = def.material.evaluate_at(&lookup, age);
        let inputs = ShellInputs {
            offset: 0.0,
            fade: opacity,
            noise_offset: 0.0,
            fit_scale: fit_of(instance_global),
            rest_scale: 1.0,
            rest_bounds: Some(def.shape.bounds()),
            billboard: def.shape.faces_camera(),
        };
        let textures = [prepared.textures[0].is_some(), prepared.textures[1].is_some(), prepared.textures[2].is_some()];
        let uniforms = EffectUniforms::new(&values, &inputs, textures);
        let alpha_mode = alpha_mode_for(def.material.blend);
        if let Some(handle) = &state.material {
            let same = materials
                .get(handle)
                .is_some_and(|m| m.uniforms == uniforms && m.alpha_mode == alpha_mode && m.sort_bias == def.sort_bias);
            if !same {
                if let Some(mut material) = materials.get_mut(handle) {
                    material.uniforms = uniforms;
                    material.alpha_mode = alpha_mode;
                    material.sort_bias = def.sort_bias;
                }
            }
        }

        let hidden = opacity <= 0.0;
        state.hidden = hidden;
        let wanted = if hidden { Visibility::Hidden } else { Visibility::Inherited };
        if *visibility != wanted {
            *visibility = wanted;
        }
    }
    cache.meshes.retain(|key, _| used.contains(key));
}

/// Places shape layers in the world (after transform propagation, before
/// visibility checks): attach point, follow mode, offset, rotation + spin,
/// animated scale × fit.
pub(crate) fn place_shapes(
    packages: Res<Assets<VfxPackage>>,
    instances: Query<(&VfxInstance, &GlobalTransform), Without<VfxShapeLayer>>,
    globals: Query<&GlobalTransform, Without<VfxShapeLayer>>,
    mut layers: Query<(&VfxShapeLayer, &ShapeLayerState, &mut GlobalTransform)>,
) {
    for (layer, state, mut global) in layers.iter_mut() {
        let Ok((instance, instance_global)) = instances.get(layer.instance) else { continue };
        let Some(prepared) = packages.get(&instance.package).and_then(|p| p.shape_layers.get(layer.index)) else {
            continue;
        };
        let def = &prepared.def;
        let (_, instance_rotation, instance_position) = instance_global.to_scale_rotation_translation();
        let mut anchor = match (&def.attach, &state.attach) {
            (AttachDef::Socket(_), Some(attach)) => {
                let position = attach.world_positions(instance_global, &globals)[0];
                let rotation = match attach.sockets[0].resolution {
                    super::attach::SocketResolution::Node(node) => globals
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
        let axis = def.rotation_axis.normalize_or(Vec3::Y);
        let rotation = def.transform.rotation() * Quat::from_axis_angle(axis, state.spin.to_radians());
        let world = shape_world_transform(anchor, def.follow, fit_of(instance_global), def.transform.offset, rotation, state.scale);
        let wanted = GlobalTransform::from(world);
        if *global != wanted {
            *global = wanted;
        }
    }
}

/// The world transform a shape layer would have, for tools (e.g. gizmos):
/// see [`shape_world_transform`].
pub fn shape_transform_for(def: &ShapeLayerDef, anchor: ShapeAnchor, fit: f32, scale: Vec3, spin_deg: f32) -> Transform {
    let axis = def.rotation_axis.normalize_or(Vec3::Y);
    let rotation = def.transform.rotation() * Quat::from_axis_angle(axis, spin_deg.to_radians());
    shape_world_transform(anchor, def.follow, fit, def.transform.offset, rotation, scale)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::FadeDef;

    #[test]
    fn follow_modes() {
        // A target leaning forward 30° and turned 90° to the left.
        let rotation = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2) * Quat::from_rotation_x(-0.5);
        let anchor = ShapeAnchor { position: Vec3::new(1.0, 2.0, 3.0), rotation };
        let offset = Vec3::new(0.0, -0.9, -0.5);

        let full = shape_world_transform(anchor, FollowDef::Full, 2.0, offset, Quat::IDENTITY, Vec3::ONE);
        assert!((full.translation - (anchor.position + rotation * (offset * 2.0))).length() < 1e-5);
        assert!(full.rotation.abs_diff_eq(rotation, 1e-5));
        assert_eq!(full.scale, Vec3::splat(2.0));

        let pos = shape_world_transform(anchor, FollowDef::Position, 1.0, offset, Quat::IDENTITY, Vec3::new(1.0, 2.0, 3.0));
        assert!((pos.translation - (anchor.position + offset)).length() < 1e-5, "world axes");
        assert!(pos.rotation.abs_diff_eq(Quat::IDENTITY, 1e-6));
        assert_eq!(pos.scale, Vec3::new(1.0, 2.0, 3.0));

        // PositionYaw: flat (up stays up), facing where the target faces.
        let yaw = shape_world_transform(anchor, FollowDef::PositionYaw, 1.0, offset, Quat::IDENTITY, Vec3::ONE);
        assert!((yaw.rotation * Vec3::Y - Vec3::Y).length() < 1e-5, "stays flat");
        let forward = yaw.rotation * Vec3::NEG_Z;
        assert!((forward - Vec3::NEG_X).length() < 1e-4, "turned left like the target: {}", forward);
        // The offset's -Z (forward) turned left with the heading: -X.
        assert!((yaw.translation - (anchor.position + Vec3::new(-0.5, -0.9, 0.0))).length() < 1e-4, "{}", yaw.translation);
        // Straight down: still a pure heading.
        let down = yaw_only(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2));
        assert!((down * Vec3::Y - Vec3::Y).length() < 1e-5);
    }

    fn one_shot(play: PlayDef) -> ShapeLayerDef {
        ShapeLayerDef { play, duration: Some(0.5), fade: FadeDef { fade_in: 0.0, fade_out: 0.1 }, ..Default::default() }
    }

    #[test]
    fn players_follow_play_modes() {
        // OnTrigger one-shot: nothing until triggered, then 0.5 s, again on
        // the next trigger (age from 0).
        let def = one_shot(PlayDef::OnTrigger("hit".into()));
        let mut p = ShapePlayer::new(&def, 0.0);
        p.step(&def, 0.1, true, None);
        assert_eq!(p.opacity(&def), 0.0, "waits for the trigger");
        p.trigger();
        p.step(&def, 0.1, true, None);
        assert!((p.age - 0.1).abs() < 1e-6 && p.opacity(&def) == 1.0);
        for _ in 0..3 {
            p.step(&def, 0.1, true, None);
        }
        assert!((p.opacity(&def) - 1.0).abs() < 1e-4, "full until the fade out: {}", p.opacity(&def));
        p.step(&def, 0.05, true, None);
        assert!((p.opacity(&def) - 0.5).abs() < 1e-3, "fading out before the end: {}", p.opacity(&def));
        p.step(&def, 0.05, true, None);
        assert!(!p.active && p.opacity(&def) == 0.0, "done after the duration");
        p.trigger();
        assert_eq!(p.age, 0.0);
        p.step(&def, 0.05, true, None);
        assert!(p.active && p.opacity(&def) > 0.0, "plays again");

        // While: on above the threshold, age from the rising edge, fades.
        let def = ShapeLayerDef {
            play: PlayDef::While("charge".into(), 0.5),
            fade: FadeDef { fade_in: 0.2, fade_out: 0.2 },
            ..Default::default()
        };
        let mut p = ShapePlayer::new(&def, 0.0);
        p.step(&def, 0.1, true, Some(0.2));
        assert_eq!(p.opacity(&def), 0.0);
        p.step(&def, 0.1, true, Some(0.8));
        assert_eq!(p.age, 0.0, "age restarts when it switches on");
        assert!((p.opacity(&def) - 0.5).abs() < 1e-5);
        p.step(&def, 0.1, true, Some(0.8));
        assert_eq!(p.opacity(&def), 1.0);
        p.step(&def, 0.1, true, Some(0.1));
        assert!((p.opacity(&def) - 0.5).abs() < 1e-5, "fades out");

        // Between: on from the start trigger to the stop trigger, age from
        // the start; a stop of the instance lets go.
        let def = ShapeLayerDef {
            play: PlayDef::Between { start: "a".into(), stop: "b".into() },
            fade: FadeDef { fade_in: 0.0, fade_out: 0.2 },
            ..Default::default()
        };
        let mut p = ShapePlayer::new(&def, 0.0);
        p.step(&def, 0.1, true, None);
        assert_eq!(p.opacity(&def), 0.0, "waits for its start");
        p.switch(&def, "b");
        p.step(&def, 0.1, true, None);
        assert_eq!(p.opacity(&def), 0.0, "a stop alone does nothing");
        p.switch(&def, "a");
        p.step(&def, 0.1, true, None);
        assert!(p.is_held() && p.active && p.age == 0.0 && p.opacity(&def) == 1.0);
        p.step(&def, 0.1, true, None);
        assert!((p.age - 0.1).abs() < 1e-6);
        p.switch(&def, "b");
        p.step(&def, 0.1, true, None);
        assert!(!p.active && (p.opacity(&def) - 0.5).abs() < 1e-5, "fades out after the stop trigger");
        p.switch(&def, "a");
        p.step(&def, 0.1, false, None);
        assert!(!p.is_held() && !p.active, "a stopping instance lets go");

        // Loop: fades in, and out when the instance stops.
        let def = ShapeLayerDef { fade: FadeDef { fade_in: 0.2, fade_out: 0.4 }, ..Default::default() };
        let mut p = ShapePlayer::new(&def, 0.0);
        p.step(&def, 0.2, true, None);
        assert_eq!(p.opacity(&def), 1.0);
        p.step(&def, 0.2, false, None);
        assert!((p.opacity(&def) - 0.5).abs() < 1e-5);
    }
}
