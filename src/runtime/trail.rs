//! Trail layers at runtime: a ribbon between two moving sockets, kept in
//! world space for `length` seconds.
//!
//! - **Emission** ([`update_trails`], `Update`): `Always` while the instance
//!   runs, between the `start` and `stop` triggers (queued by
//!   `apply_triggers`), or while a parameter is high. The material's uniforms
//!   are written here (fade level × visibility; `Age` curves read the seconds
//!   since emission last started).
//! - **Samples and mesh** ([`build_trails`], `PostUpdate` after transform
//!   propagation, so the sockets are where this frame's animation put them):
//!   while emitting, a sample of both socket positions is kept whenever an
//!   end moved [`TrailLayerDef::min_sample_distance`] (the newest point always
//!   follows the sockets); samples older than `length` are dropped (the
//!   oldest end is cut exactly at `length`, so the tail slides out instead of
//!   popping); the points are smoothed with centripetal Catmull-Rom curves
//!   ([`smooth_points`]) and turned into a two-sided ribbon
//!   ([`ribbon_mesh`]): `u` = age / length (0 newest → 1 oldest), `v` = 0 at
//!   `from` → 1 at `to`, vertex colour alpha = `fade_over_u`. The mesh is in
//!   world space: the entity's `GlobalTransform` is the identity. Without at
//!   least two points the layer is hidden.

use std::collections::VecDeque;

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::NoFrustumCulling;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

use super::attach::{SocketAttach, TargetHierarchy};
use super::effect_material::{alpha_mode_for, EffectUniforms, OmagariEffectMaterial, ShellInputs};
use super::instance::{VfxInstance, VfxState};
use super::package::{PreparedTrailLayer, VfxPackage};
use crate::format::{AttachDef, EmitDef, ParamValue, ProjectV2, TrailLayerDef, MAX_TRAIL_POINTS, MAX_TRAIL_SAMPLES};

/// A trail layer entity of an instance (a child of the instance).
#[derive(Component, Clone, Copy, Debug)]
pub struct VfxTrailLayer {
    pub instance: Entity,
    /// Index into [`VfxPackage::trail_layers`].
    pub index: usize,
}

/// One sample of a trail: both ends (world) and when it was taken (the
/// layer's clock, seconds).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct TrailSample {
    /// The `from` end.
    pub a: Vec3,
    /// The `to` end.
    pub b: Vec3,
    pub time: f32,
}

impl TrailSample {
    pub fn new(a: Vec3, b: Vec3, time: f32) -> Self {
        TrailSample { a, b, time }
    }

    fn lerp(&self, other: &TrailSample, t: f32) -> TrailSample {
        TrailSample {
            a: self.a.lerp(other.a, t),
            b: self.b.lerp(other.b, t),
            time: self.time + (other.time - self.time) * t,
        }
    }
}

/// The samples of a trail, oldest first (plain data, no ECS).
#[derive(Clone, Debug, Default)]
pub struct TrailBuffer {
    samples: VecDeque<TrailSample>,
}

impl TrailBuffer {
    /// The kept samples, oldest first.
    pub fn samples(&self) -> impl Iterator<Item = &TrailSample> + '_ {
        self.samples.iter()
    }

    pub fn len(&self) -> usize {
        self.samples.len()
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    pub fn clear(&mut self) {
        self.samples.clear();
    }

    /// Keeps a sample when it is the first, `force`d (emission just
    /// stopped), or an end moved at least `min_distance` from the newest
    /// sample. At most [`MAX_TRAIL_SAMPLES`] are kept (the oldest goes).
    /// Returns whether it was kept.
    pub fn push(&mut self, sample: TrailSample, min_distance: f32, force: bool) -> bool {
        let keep = match self.samples.back() {
            None => true,
            Some(last) => {
                let moved = last.a.distance(sample.a).max(last.b.distance(sample.b));
                force && moved > 1e-6 || moved >= min_distance.max(1e-5)
            }
        };
        if keep {
            self.samples.push_back(sample);
            while self.samples.len() > MAX_TRAIL_SAMPLES {
                self.samples.pop_front();
            }
        }
        keep
    }

    /// Drops samples older than `length` seconds at time `now`, keeping the
    /// one just past it while a younger one remains (the tail is cut at
    /// `length` between them).
    pub fn age_out(&mut self, now: f32, length: f32) {
        while self.samples.len() >= 2 && now - self.samples[1].time >= length {
            self.samples.pop_front();
        }
        if self.samples.len() == 1 && now - self.samples[0].time >= length {
            self.samples.clear();
        }
    }

    /// The trail's points, newest first: the live `head` (the sockets now,
    /// while emitting), the samples, and the tail cut at `length`.
    pub fn points(&self, now: f32, length: f32, head: Option<(Vec3, Vec3)>) -> Vec<TrailSample> {
        let mut out = Vec::with_capacity(self.samples.len() + 1);
        if let Some((a, b)) = head {
            out.push(TrailSample::new(a, b, now));
        }
        for s in self.samples.iter().rev() {
            if now - s.time <= length {
                out.push(*s);
                continue;
            }
            // Past the length: cut between it and the next younger point.
            if let Some(younger) = out.last().copied() {
                let span = younger.time - s.time;
                let t = if span > 1e-6 { ((younger.time - (now - length)) / span).clamp(0.0, 1.0) } else { 1.0 };
                out.push(younger.lerp(s, t));
            }
            break;
        }
        // A head on top of an identical newest sample adds nothing.
        if out.len() >= 2 && out[0].a.distance(out[1].a) < 1e-6 && out[0].b.distance(out[1].b) < 1e-6 {
            out.remove(1);
        }
        out
    }
}

/// One centripetal Catmull-Rom point between `p1` and `p2` at `t` (0..1),
/// with neighbours `p0`, `p3`. Passes through `p1` at 0 and `p2` at 1.
pub fn catmull_rom(p0: Vec3, p1: Vec3, p2: Vec3, p3: Vec3, t: f32) -> Vec3 {
    let knot = |a: Vec3, b: Vec3| a.distance(b).max(1e-6).sqrt();
    let t0 = 0.0;
    let t1 = t0 + knot(p0, p1);
    let t2 = t1 + knot(p1, p2);
    let t3 = t2 + knot(p2, p3);
    let t = t1 + (t2 - t1) * t;
    let a1 = p0 * ((t1 - t) / (t1 - t0)) + p1 * ((t - t0) / (t1 - t0));
    let a2 = p1 * ((t2 - t) / (t2 - t1)) + p2 * ((t - t1) / (t2 - t1));
    let a3 = p2 * ((t3 - t) / (t3 - t2)) + p3 * ((t - t2) / (t3 - t2));
    let b1 = a1 * ((t2 - t) / (t2 - t0)) + a2 * ((t - t0) / (t2 - t0));
    let b2 = a2 * ((t3 - t) / (t3 - t1)) + a3 * ((t - t1) / (t3 - t1));
    b1 * ((t2 - t) / (t2 - t1)) + b2 * ((t - t1) / (t2 - t1))
}

/// The points smoothed with Catmull-Rom curves (both ends separately, times
/// linear): `subdivisions` segments between two points, so `(n - 1) ×
/// subdivisions + 1` points through every original one. Lowered so the
/// result stays within [`MAX_TRAIL_POINTS`].
pub fn smooth_points(points: &[TrailSample], subdivisions: u32) -> Vec<TrailSample> {
    let n = points.len();
    if n < 2 {
        return points.to_vec();
    }
    let most = ((MAX_TRAIL_POINTS - 1) / (n - 1)).max(1);
    let s = (subdivisions.max(1) as usize).min(most);
    if s == 1 {
        return points.to_vec();
    }
    let at = |i: isize| -> TrailSample {
        if i < 0 {
            // Mirrored past the ends, so the curve leaves the ends straight.
            let (p0, p1) = (points[0], points[1]);
            TrailSample::new(p0.a * 2.0 - p1.a, p0.b * 2.0 - p1.b, p0.time)
        } else if i as usize >= n {
            let (p0, p1) = (points[n - 1], points[n - 2]);
            TrailSample::new(p0.a * 2.0 - p1.a, p0.b * 2.0 - p1.b, p0.time)
        } else {
            points[i as usize]
        }
    };
    let mut out = Vec::with_capacity((n - 1) * s + 1);
    for i in 0..n - 1 {
        let (p0, p1, p2, p3) = (at(i as isize - 1), at(i as isize), at(i as isize + 1), at(i as isize + 2));
        for k in 0..s {
            let t = k as f32 / s as f32;
            if k == 0 {
                out.push(p1);
                continue;
            }
            out.push(TrailSample {
                a: catmull_rom(p0.a, p1.a, p2.a, p3.a, t),
                b: catmull_rom(p0.b, p1.b, p2.b, p3.b, t),
                time: p1.time + (p2.time - p1.time) * t,
            });
        }
    }
    out.push(points[n - 1]);
    out
}

/// The vertex data of a trail's ribbon.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RibbonData {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub colors: Vec<[f32; 4]>,
    pub indices: Vec<u32>,
}

/// A two-sided ribbon through `points` (newest first): per point the `from`
/// end (v = 0; moved toward `to` by `width_over_u`) and the `to` end (v =
/// 1), u = age / `length`, vertex colour (1, 1, 1, `fade_over_u`). The front
/// side faces `cross(to − from, older − newer)`; the back side is a copy
/// with flipped normals and winding.
pub fn ribbon_mesh(points: &[TrailSample], now: f32, def: &TrailLayerDef) -> RibbonData {
    let mut data = RibbonData::default();
    let n = points.len();
    if n < 2 {
        return data;
    }
    let length = def.length.max(1e-4);
    for (i, p) in points.iter().enumerate() {
        let u = ((now - p.time) / length).clamp(0.0, 1.0);
        let w = def.width_at(u);
        let a = p.b + (p.a - p.b) * w;
        let mid = |q: &TrailSample| (q.a + q.b) * 0.5;
        let older = mid(&points[(i + 1).min(n - 1)]);
        let newer = mid(&points[i.saturating_sub(1)]);
        let along = older - newer;
        let across = p.b - p.a;
        let normal = across.cross(along).normalize_or(Vec3::Y);
        let alpha = def.fade_at(u);
        data.positions.push(a.to_array());
        data.positions.push(p.b.to_array());
        data.uvs.push([u, 0.0]);
        data.uvs.push([u, 1.0]);
        for _ in 0..2 {
            data.normals.push(normal.to_array());
            data.colors.push([1.0, 1.0, 1.0, alpha]);
        }
    }
    // Back side: the same vertices with flipped normals.
    for i in 0..2 * n {
        data.positions.push(data.positions[i]);
        data.uvs.push(data.uvs[i]);
        data.colors.push(data.colors[i]);
        let flipped = -Vec3::from_array(data.normals[i]);
        data.normals.push(flipped.to_array());
    }
    let back = 2 * n as u32;
    for i in 0..(n as u32 - 1) {
        let (a0, b0, a1, b1) = (2 * i, 2 * i + 1, 2 * i + 2, 2 * i + 3);
        data.indices.extend_from_slice(&[a0, b0, a1, b0, b1, a1]);
        data.indices.extend_from_slice(&[back + a0, back + a1, back + b0, back + b0, back + a1, back + b1]);
    }
    data
}

/// Writes ribbon data into a mesh (replacing its attributes and indices).
pub fn write_ribbon(mesh: &mut Mesh, data: RibbonData) {
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, data.positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, data.normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, data.uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, data.colors);
    mesh.insert_indices(Indices::U32(data.indices));
}

/// An empty trail mesh (filled every frame).
pub fn empty_trail_mesh() -> Mesh {
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    write_ribbon(&mut mesh, RibbonData::default());
    mesh
}

/// Emission of a trail or afterimage layer as a plain state machine.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Emitter {
    /// Emitting now.
    pub on: bool,
    /// The trigger state of a `Trigger` layer (on between start and stop).
    triggered: bool,
    /// Seconds since emission last started (what `Age` curves read).
    pub age: f32,
}

impl Emitter {
    /// The trigger `name` fired.
    pub fn trigger(&mut self, emit: &EmitDef, name: &str) {
        if let Some(on) = emit.on_trigger(name, self.triggered) {
            self.triggered = on;
        }
    }

    /// Starts over (a restart of the instance): a `Trigger` layer waits for
    /// its start trigger again.
    pub fn reset(&mut self) {
        *self = Emitter::default();
    }

    /// Advances by `dt`; `running` = the instance runs, `while_value` = the
    /// `While` parameter's value. Returns `(started, stopped)` this step.
    pub fn step(&mut self, emit: &EmitDef, dt: f32, running: bool, while_value: Option<f32>) -> (bool, bool) {
        let wanted = running
            && match emit {
                EmitDef::Always => true,
                EmitDef::Trigger { .. } => self.triggered,
                EmitDef::While(_, threshold) => while_value.is_some_and(|v| v >= *threshold),
            };
        if !running {
            self.triggered = false;
        }
        let started = wanted && !self.on;
        let stopped = !wanted && self.on;
        self.on = wanted;
        if started {
            self.age = 0.0;
        } else {
            self.age += dt;
        }
        (started, stopped)
    }
}

/// Runtime state of a trail layer.
#[derive(Component, Clone, Debug, Default)]
pub struct TrailLayerState {
    pub emitter: Emitter,
    pub samples: TrailBuffer,
    /// The layer's clock (seconds since it was spawned): sample times.
    pub clock: f32,
    /// Lifecycle fade level 0..1.
    pub level: f32,
    pub visibility: f32,
    pub attach: Option<SocketAttach>,
    pub material: Option<Handle<OmagariEffectMaterial>>,
    pub mesh: Option<Handle<Mesh>>,
    /// Points drawn last frame (after smoothing).
    pub drawn_points: usize,
    /// Triggers waiting for the next update.
    pub(crate) pending: Vec<String>,
    /// Emission stopped this frame: a last sample is taken where the sockets
    /// are.
    stopped: bool,
    generation: u32,
    started: bool,
    start_level: f32,
}

impl TrailLayerState {
    pub fn with_level(level: f32) -> Self {
        TrailLayerState { visibility: 1.0, start_level: level, level, ..default() }
    }
}

/// Spawns the entity of one trail layer of an instance.
pub(crate) fn spawn_trail_layer(
    commands: &mut Commands,
    instance: Entity,
    index: usize,
    prepared: &PreparedTrailLayer,
    project: &ProjectV2,
    level: f32,
) -> Entity {
    let mut state = TrailLayerState::with_level(level);
    let attach = AttachDef::Segment { from: prepared.def.from.clone(), to: prepared.def.to.clone() };
    state.attach = Some(SocketAttach::new(&attach, project));
    commands
        .spawn((
            Name::new(prepared.def.name.clone()),
            VfxTrailLayer { instance, index },
            state,
            Transform::default(),
            Visibility::Hidden,
            NotShadowCaster,
            NotShadowReceiver,
            // The ribbon changes every frame; its bounds are not tracked.
            NoFrustumCulling,
            ChildOf(instance),
        ))
        .id()
}

fn fit_of(instance: &GlobalTransform) -> f32 {
    let s = instance.scale().abs();
    ((s.x + s.y + s.z) / 3.0).max(1e-4)
}

/// Writes uniforms, alpha mode and sort bias into a material when they
/// changed.
pub(crate) fn write_material(
    materials: &mut Assets<OmagariEffectMaterial>,
    handle: &Handle<OmagariEffectMaterial>,
    uniforms: EffectUniforms,
    alpha_mode: AlphaMode,
    sort_bias: f32,
) {
    let same = materials
        .get(handle)
        .is_some_and(|m| m.uniforms == uniforms && m.alpha_mode == alpha_mode && m.sort_bias == sort_bias);
    if !same {
        if let Some(mut material) = materials.get_mut(handle) {
            material.uniforms = uniforms;
            material.alpha_mode = alpha_mode;
            material.sort_bias = sort_bias;
        }
    }
}

/// Emission, sockets, fades, triggers and the material of trail layers.
#[allow(clippy::too_many_arguments)]
pub(crate) fn update_trails(
    mut commands: Commands,
    time: Res<Time>,
    packages: Res<Assets<VfxPackage>>,
    materials: Option<ResMut<Assets<OmagariEffectMaterial>>>,
    meshes: Option<ResMut<Assets<Mesh>>>,
    hierarchy: TargetHierarchy,
    changed: Query<(), (Or<(Changed<Children>, Changed<Name>)>, Without<VfxTrailLayer>)>,
    instances: Query<(&VfxInstance, &GlobalTransform)>,
    mut layers: Query<(Entity, &VfxTrailLayer, &mut TrailLayerState)>,
) {
    let (Some(mut materials), Some(mut meshes)) = (materials, meshes) else { return };
    let dt = time.delta_secs();
    let any_change = !changed.is_empty();
    for (entity, layer, mut state) in layers.iter_mut() {
        let state = &mut *state;
        let Ok((instance, instance_global)) = instances.get(layer.instance) else { continue };
        let Some(prepared) = packages.get(&instance.package).and_then(|p| p.trail_layers.get(layer.index)) else {
            continue;
        };
        if !instance.params.is_synced(instance.package.id()) {
            continue;
        }
        let def = &prepared.def;
        if state.mesh.is_none() {
            let mesh = meshes.add(empty_trail_mesh());
            commands.entity(entity).insert(Mesh3d(mesh.clone()));
            state.mesh = Some(mesh);
        }
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
        if !state.started || state.generation != instance.generation {
            state.emitter.reset();
            state.started = true;
            state.generation = instance.generation;
            state.level = state.start_level;
        }
        if let Some(attach) = &mut state.attach {
            attach.update(&hierarchy, instance.target(), any_change, dt, def.name.as_str());
        }

        let running = matches!(instance.state(), VfxState::Running | VfxState::Starting);
        for name in std::mem::take(&mut state.pending) {
            if running {
                state.emitter.trigger(&def.emit, &name);
            }
        }
        let while_value = def.emit.param().and_then(|p| instance.params.float(p));
        let (_, stopped) = state.emitter.step(&def.emit, dt, running, while_value);
        state.stopped |= stopped;
        state.clock += dt;
        state.level = def.fade.step(state.level, running, dt);
        state.visibility = match &def.visible {
            Some(visible) => instance.params.float(&visible.param).map(|v| visible.factor(v)).unwrap_or(1.0),
            None => 1.0,
        };

        let lookup = |name: &str| -> Option<ParamValue> { instance.params.get(name) };
        let values = def.material.evaluate_at(&lookup, state.emitter.age);
        let bounds = state.samples.samples().fold(None, |acc: Option<(Vec3, Vec3)>, s| {
            let (lo, hi) = (s.a.min(s.b), s.a.max(s.b));
            Some(match acc {
                None => (lo, hi),
                Some((a, b)) => (a.min(lo), b.max(hi)),
            })
        });
        let inputs = ShellInputs {
            offset: 0.0,
            fade: state.level * state.visibility,
            noise_offset: 0.0,
            fit_scale: fit_of(instance_global),
            rest_scale: 1.0,
            rest_bounds: bounds,
            billboard: false,
        };
        let textures = [prepared.textures[0].is_some(), prepared.textures[1].is_some(), prepared.textures[2].is_some()];
        let uniforms = EffectUniforms::new(&values, &inputs, textures);
        if let Some(handle) = &state.material {
            write_material(&mut materials, handle, uniforms, alpha_mode_for(def.material.blend), def.sort_bias);
        }
    }
}

/// Samples the sockets (after this frame's animation and transform
/// propagation), ages the samples out and rebuilds the ribbon mesh.
pub(crate) fn build_trails(
    packages: Res<Assets<VfxPackage>>,
    meshes: Option<ResMut<Assets<Mesh>>>,
    instances: Query<(&VfxInstance, &GlobalTransform), Without<VfxTrailLayer>>,
    globals: Query<&GlobalTransform, Without<VfxTrailLayer>>,
    mut layers: Query<(&VfxTrailLayer, &mut TrailLayerState, &mut GlobalTransform, &mut Visibility)>,
) {
    let Some(mut meshes) = meshes else { return };
    for (layer, mut state, mut global, mut visibility) in layers.iter_mut() {
        let state = &mut *state;
        let Ok((instance, instance_global)) = instances.get(layer.instance) else { continue };
        let Some(prepared) = packages.get(&instance.package).and_then(|p| p.trail_layers.get(layer.index)) else {
            continue;
        };
        let def = &prepared.def;
        let now = state.clock;
        let ends = state
            .attach
            .as_ref()
            .map(|a| a.world_positions(instance_global, &globals))
            .unwrap_or_default();
        let head = (ends.len() == 2).then(|| (ends[0], ends[1]));
        if let Some((a, b)) = head {
            if state.emitter.on {
                state.samples.push(TrailSample::new(a, b, now), def.min_sample_distance, false);
            } else if state.stopped {
                state.samples.push(TrailSample::new(a, b, now), def.min_sample_distance, true);
            }
        }
        state.stopped = false;
        state.samples.age_out(now, def.length.max(0.0));
        let points = state.samples.points(now, def.length.max(0.0), head.filter(|_| state.emitter.on));
        let points = smooth_points(&points, def.subdivisions);
        state.drawn_points = if points.len() >= 2 { points.len() } else { 0 };
        if *global != GlobalTransform::IDENTITY {
            *global = GlobalTransform::IDENTITY;
        }
        let shown = points.len() >= 2 && state.level * state.visibility > 0.0;
        if let Some(handle) = &state.mesh {
            if shown || meshes.get(handle).is_some_and(|m| m.count_vertices() > 0) {
                if let Some(mut mesh) = meshes.get_mut(handle) {
                    let data = if shown { ribbon_mesh(&points, now, def) } else { RibbonData::default() };
                    write_ribbon(&mut mesh, data);
                }
            }
        }
        let wanted = if shown { Visibility::Inherited } else { Visibility::Hidden };
        if *visibility != wanted {
            *visibility = wanted;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(n: usize, dt: f32) -> Vec<TrailSample> {
        (0..n)
            .map(|i| {
                let angle = i as f32 * 0.4;
                let dir = Vec3::new(angle.cos(), 0.0, angle.sin());
                TrailSample::new(dir * 0.2, dir * 1.0, i as f32 * dt)
            })
            .collect()
    }

    #[test]
    fn samples_age_out_and_the_tail_is_cut_at_the_length() {
        let mut buffer = TrailBuffer::default();
        for (i, s) in line(6, 0.1).into_iter().enumerate() {
            assert!(buffer.push(s, 0.01, false), "sample {}", i);
        }
        // Not far enough from the newest: not kept.
        let last = *buffer.samples().last().unwrap();
        assert!(!buffer.push(TrailSample::new(last.a + Vec3::splat(0.001), last.b, 0.55), 0.01, false));
        assert!(buffer.push(TrailSample::new(last.a + Vec3::splat(0.001), last.b, 0.55), 0.01, true), "forced");
        let mut buffer = TrailBuffer::default();
        for s in line(6, 0.1) {
            buffer.push(s, 0.01, false);
        }
        // At 0.6 with length 0.25: samples at 0.4 and 0.5 are young; 0.3 is
        // kept as the cut's older end.
        buffer.age_out(0.6, 0.25);
        let times: Vec<f32> = buffer.samples().map(|s| s.time).collect();
        assert_eq!(times.len(), 3, "{:?}", times);
        assert!((times[0] - 0.3).abs() < 1e-5);
        let points = buffer.points(0.6, 0.25, None);
        assert_eq!(points.len(), 3, "0.5, 0.4 and the cut at 0.35");
        assert!((points.last().unwrap().time - 0.35).abs() < 1e-5, "{:?}", points.last());
        // Everything ages out.
        buffer.age_out(2.0, 0.25);
        assert!(buffer.is_empty());
        assert!(buffer.points(2.0, 0.25, None).is_empty());
        // The cap.
        let mut buffer = TrailBuffer::default();
        for i in 0..(MAX_TRAIL_SAMPLES + 50) {
            buffer.push(TrailSample::new(Vec3::X * i as f32, Vec3::Y, i as f32), 0.01, false);
        }
        assert_eq!(buffer.len(), MAX_TRAIL_SAMPLES);
        assert_eq!(buffer.samples().next().unwrap().time, 50.0, "the oldest went first");
    }

    #[test]
    fn catmull_rom_passes_through_the_samples() {
        let points: Vec<TrailSample> = line(5, 0.1).into_iter().rev().collect();
        let smooth = smooth_points(&points, 4);
        assert_eq!(smooth.len(), (points.len() - 1) * 4 + 1);
        for (i, p) in points.iter().enumerate() {
            let s = smooth[i * 4];
            assert!(s.a.distance(p.a) < 1e-5 && s.b.distance(p.b) < 1e-5 && (s.time - p.time).abs() < 1e-6, "point {}", i);
        }
        // Between samples on a circle the curve bulges outward like the
        // circle (a straight line would cut inside).
        let mid = smooth[2];
        let chord = (points[0].b + points[1].b) * 0.5;
        assert!(mid.b.length() > chord.length() + 1e-3, "{} vs {}", mid.b.length(), chord.length());
        assert!((mid.b.length() - 1.0).abs() < 0.02, "close to the circle: {}", mid.b.length());
        // Times run monotonically.
        assert!(smooth.windows(2).all(|w| w[1].time <= w[0].time));
        // Subdivision 1 = the points; the point cap.
        assert_eq!(smooth_points(&points, 1), points);
        let many: Vec<TrailSample> = (0..150).map(|i| TrailSample::new(Vec3::X * i as f32, Vec3::Y, i as f32)).collect();
        assert!(smooth_points(&many, 8).len() <= MAX_TRAIL_POINTS);
        assert_eq!(smooth_points(&many[..1], 4).len(), 1);
    }

    #[test]
    fn ribbon_vertices_and_uvs() {
        let def = TrailLayerDef {
            length: 0.4,
            width_over_u: vec![(0.0, 1.0), (1.0, 0.5)],
            fade_over_u: vec![(0.0, 1.0), (1.0, 0.0)],
            ..Default::default()
        };
        let points = vec![
            TrailSample::new(Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 1.0, 0.0), 1.0),
            TrailSample::new(Vec3::new(1.0, 0.0, 0.0), Vec3::new(1.0, 1.0, 0.0), 0.8),
            TrailSample::new(Vec3::new(2.0, 0.0, 0.0), Vec3::new(2.0, 1.0, 0.0), 0.6),
        ];
        let data = ribbon_mesh(&points, 1.0, &def);
        assert_eq!(data.positions.len(), 12, "2 per point, front and back");
        assert_eq!(data.indices.len(), 2 * 2 * 6);
        assert_eq!(data.uvs[0], [0.0, 0.0]);
        assert_eq!(data.uvs[1], [0.0, 1.0]);
        assert!((data.uvs[2][0] - 0.5).abs() < 1e-5 && (data.uvs[4][0] - 1.0).abs() < 1e-5, "u = age / length");
        assert_eq!(data.colors[0][3], 1.0);
        assert!((data.colors[2][3] - 0.5).abs() < 1e-5);
        assert!(data.colors[4][3].abs() < 1e-5);
        // Width 0.5 at the oldest point: its `from` end moved half-way to `to`.
        assert!((Vec3::from_array(data.positions[4]) - Vec3::new(2.0, 0.5, 0.0)).length() < 1e-5);
        assert_eq!(Vec3::from_array(data.positions[5]), Vec3::new(2.0, 1.0, 0.0));
        // Front faces point along their normals; the back copy the other way.
        for tri in data.indices.chunks(3) {
            let [a, b, c] = [tri[0], tri[1], tri[2]].map(|i| Vec3::from_array(data.positions[i as usize]));
            let face = (b - a).cross(c - a);
            let normal = Vec3::from_array(data.normals[tri[0] as usize]);
            assert!(face.dot(normal) > 0.0, "winding matches the normal");
        }
        assert!(ribbon_mesh(&points[..1], 1.0, &def).positions.is_empty());
    }

    #[test]
    fn emitters_follow_their_modes() {
        let emit = EmitDef::Trigger { start: "go".into(), stop: "halt".into() };
        let mut e = Emitter::default();
        assert_eq!(e.step(&emit, 0.1, true, None), (false, false));
        assert!(!e.on);
        e.trigger(&emit, "go");
        assert_eq!(e.step(&emit, 0.1, true, None), (true, false));
        assert!(e.on && e.age == 0.0);
        e.step(&emit, 0.1, true, None);
        assert!((e.age - 0.1).abs() < 1e-6);
        e.trigger(&emit, "halt");
        assert_eq!(e.step(&emit, 0.1, true, None), (false, true));
        // Stopping the instance ends emission and forgets the trigger.
        e.trigger(&emit, "go");
        e.step(&emit, 0.1, true, None);
        assert_eq!(e.step(&emit, 0.1, false, None), (false, true));
        assert!(!e.step(&emit, 0.1, true, None).0, "waits for the start trigger again");
        // While and Always.
        let w = EmitDef::While("speed".into(), 0.5);
        let mut e = Emitter::default();
        assert!(!e.step(&w, 0.1, true, Some(0.2)).0);
        assert!(e.step(&w, 0.1, true, Some(0.7)).0);
        assert!(e.step(&w, 0.1, true, Some(0.1)).1);
        let mut e = Emitter::default();
        assert!(e.step(&EmitDef::Always, 0.1, true, None).0);
        assert!(e.step(&EmitDef::Always, 0.1, false, None).1);
    }
}
