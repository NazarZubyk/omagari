//! Trail and afterimage layers, headless (`MinimalPlugins` + transform
//! propagation, no rendering): trail samples follow moving sockets, the
//! ribbon mesh in world space, emission by triggers, ageing out after the
//! stop trigger and after `stop`, the sample cap; afterimage copies every
//! interval, the cap, copies staying where they were left, frozen joints of
//! skinned meshes, cleanup.

use std::time::Duration;

use bevy::asset::AssetPlugin;
use bevy::ecs::system::RunSystemOnce;
use bevy::image::ImagePlugin;
use bevy::mesh::skinning::SkinnedMesh;
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use bevy::transform::TransformPlugin;
use bevy_hanabi::prelude::*;
use omagari::format::{ProjectV2, MAX_TRAIL_POINTS, MAX_TRAIL_SAMPLES};
use omagari::runtime::{AfterimageLayerState, OmagariEffectMaterial, TrailLayerState, VfxGhost, VfxState};
use omagari::{OmagariPlugin, VfxCommandsExt, VfxControl, VfxFinished, VfxInstance, VfxPackage};

const DT: f32 = 0.05;

fn app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, TransformPlugin, AssetPlugin::default(), ImagePlugin::default()))
        .init_asset::<EffectAsset>()
        .init_asset::<Mesh>()
        .add_plugins(OmagariPlugin)
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(DT)))
        .init_resource::<Finished>()
        .add_systems(Last, record_finished);
    app
}

#[derive(Resource, Default)]
struct Finished(Vec<VfxFinished>);

fn record_finished(mut reader: MessageReader<VfxFinished>, mut finished: ResMut<Finished>) {
    finished.0.extend(reader.read().copied());
}

fn add_package(app: &mut App, ron: &str) -> Handle<VfxPackage> {
    let project = ProjectV2::from_ron_str(ron).unwrap();
    let world = app.world_mut();
    let package = world.resource_scope(|world, mut effects: Mut<Assets<EffectAsset>>| {
        let server = world.resource::<AssetServer>();
        let mut warnings = Vec::new();
        let package = VfxPackage::build(project, &mut (&mut *effects, server), &mut warnings);
        assert!(warnings.is_empty(), "{:?}", warnings);
        package
    });
    world.resource_mut::<Assets<VfxPackage>>().add(package)
}

fn spawn_vfx(app: &mut App, package: &Handle<VfxPackage>, target: Entity) -> Entity {
    let package = package.clone();
    app.world_mut()
        .run_system_once(move |mut commands: Commands| commands.spawn_vfx(&package, target))
        .unwrap()
}

fn control(app: &mut App, f: impl FnOnce(&mut VfxControl) + Send + Sync + 'static) {
    let mut f = Some(f);
    app.world_mut()
        .run_system_once(move |mut vfx: VfxControl| (f.take().unwrap())(&mut vfx))
        .unwrap();
}

fn fire(app: &mut App, instance: Entity, name: &'static str) {
    control(app, move |vfx| {
        vfx.trigger(instance, name);
    });
}

/// A target with a "Hand" child that the test swings around.
fn swinging_target(app: &mut App) -> (Entity, Entity) {
    let target = app.world_mut().spawn(Transform::default()).id();
    let hand = app.world_mut().spawn((Name::new("Hand"), Transform::from_xyz(0.5, 1.0, 0.0), ChildOf(target))).id();
    (target, hand)
}

/// Turns the hand around the target's Y axis by `step` radians.
fn swing(app: &mut App, hand: Entity, step: f32) {
    let mut t = app.world_mut().get_mut::<Transform>(hand).unwrap();
    let rotation = Quat::from_rotation_y(step);
    t.translation = rotation * t.translation;
    t.rotation = rotation * t.rotation;
}

fn trail_state(app: &App, instance: Entity) -> (Entity, TrailLayerState) {
    let e = app.world().get::<VfxInstance>(instance).unwrap().trail_layers()[0];
    (e, app.world().get::<TrailLayerState>(e).unwrap().clone())
}

fn vertex_count(app: &App, trail: Entity) -> usize {
    let handle = &app.world().get::<Mesh3d>(trail).unwrap().0;
    app.world().resource::<Assets<Mesh>>().get(handle).unwrap().count_vertices()
}

fn uvs(app: &App, trail: Entity) -> Vec<[f32; 2]> {
    let handle = &app.world().get::<Mesh3d>(trail).unwrap().0;
    match app.world().resource::<Assets<Mesh>>().get(handle).unwrap().attribute(Mesh::ATTRIBUTE_UV_0) {
        Some(VertexAttributeValues::Float32x2(v)) => v.clone(),
        _ => Vec::new(),
    }
}

fn shown(app: &App, e: Entity) -> bool {
    *app.world().get::<Visibility>(e).unwrap() != Visibility::Hidden
}

const TRAIL: &str = r#"(
    triggers: [(name: "swing_start"), (name: "swing_end")],
    sockets: [
        (name: "base", node: Some("Hand"), offset: (0.0, 0.0, 0.0)),
        (name: "tip", node: Some("Hand"), offset: (0.0, 0.0, 1.0)),
    ],
    layers: [Trail((
        name: "slash",
        from: "base",
        to: "tip",
        length: 0.3,
        subdivisions: 4,
        emit: Trigger(start: "swing_start", stop: "swing_end"),
    ))],
)"#;

#[test]
fn trails_follow_the_sockets_between_their_triggers_and_age_out() {
    let mut app = app();
    let package = add_package(&mut app, TRAIL);
    let (target, hand) = swinging_target(&mut app);
    let instance = spawn_vfx(&mut app, &package, target);
    for _ in 0..3 {
        swing(&mut app, hand, 0.2);
        app.update();
    }
    let (trail, state) = trail_state(&app, instance);
    assert!(state.samples.is_empty() && !shown(&app, trail), "nothing before swing_start");
    assert_eq!(app.world().get::<ChildOf>(trail).unwrap().parent(), instance);
    assert!(app.world().get::<MeshMaterial3d<OmagariEffectMaterial>>(trail).is_some());

    fire(&mut app, instance, "swing_start");
    for _ in 0..4 {
        swing(&mut app, hand, 0.2);
        app.update();
    }
    let (_, state) = trail_state(&app, instance);
    assert!(state.emitter.on);
    assert!(state.samples.len() >= 3, "{} samples", state.samples.len());
    assert!(shown(&app, trail));
    // World space: identity transform; the newest sample is where the hand
    // is now (+ the tip offset along the hand's turned Z).
    assert_eq!(*app.world().get::<GlobalTransform>(trail).unwrap(), GlobalTransform::IDENTITY);
    let hand_global = *app.world().get::<GlobalTransform>(hand).unwrap();
    let newest = *state.samples.samples().last().unwrap();
    assert!(newest.a.distance(hand_global.translation()) < 1e-4, "{} vs {}", newest.a, hand_global.translation());
    assert!(newest.b.distance(hand_global.transform_point(Vec3::Z)) < 1e-4);
    // The ribbon: 2 vertices per smoothed point, front and back; u = age.
    let points = state.drawn_points;
    assert!(points >= (state.samples.len() - 1) * 4, "smoothed: {} points from {} samples", points, state.samples.len());
    assert_eq!(vertex_count(&app, trail), points * 4);
    let us: Vec<f32> = uvs(&app, trail).iter().map(|uv| uv[0]).collect();
    assert_eq!(us[0], 0.0, "the newest end");
    assert!(us.iter().all(|u| (0.0..=1.0).contains(u)));
    assert!(us.iter().cloned().fold(0.0, f32::max) > 0.4, "older samples further along u");

    // Long swing: never older than the length.
    for _ in 0..20 {
        swing(&mut app, hand, 0.2);
        app.update();
    }
    let (_, state) = trail_state(&app, instance);
    let oldest = state.samples.samples().next().unwrap().time;
    assert!(state.clock - oldest <= 0.3 + DT + 1e-4, "kept {} s", state.clock - oldest);

    // swing_end: no new samples, the rest ages out within the length.
    fire(&mut app, instance, "swing_end");
    swing(&mut app, hand, 0.2);
    app.update();
    let (_, state) = trail_state(&app, instance);
    assert!(!state.emitter.on);
    let n = state.samples.len();
    swing(&mut app, hand, 0.2);
    app.update();
    assert!(trail_state(&app, instance).1.samples.len() <= n, "no new samples after the stop");
    for _ in 0..8 {
        swing(&mut app, hand, 0.2);
        app.update();
    }
    let (_, state) = trail_state(&app, instance);
    assert!(state.samples.is_empty(), "{} left", state.samples.len());
    assert!(!shown(&app, trail) && vertex_count(&app, trail) == 0, "nothing drawn between swings");

    // A new swing, then stop: the instance waits for the trail to age out.
    fire(&mut app, instance, "swing_start");
    for _ in 0..4 {
        swing(&mut app, hand, 0.2);
        app.update();
    }
    control(&mut app, move |vfx| {
        vfx.stop(instance);
    });
    app.update();
    assert!(matches!(app.world().get::<VfxInstance>(instance).unwrap().state(), VfxState::Stopping { .. }));
    assert!(!trail_state(&app, instance).1.emitter.on, "stop ends emission");
    for _ in 0..8 {
        app.update();
    }
    assert!(app.world().get_entity(instance).is_err(), "finished after the trail's length");
    assert_eq!(app.world().resource::<Finished>().0.len(), 1);
}

#[test]
fn always_trails_are_capped() {
    let mut app = app();
    let package = add_package(
        &mut app,
        r#"(
            sockets: [(name: "a", node: Some("Hand")), (name: "b", node: Some("Hand"), offset: (0.0, 1.0, 0.0))],
            layers: [Trail((name: "long", from: "a", to: "b", length: 1000.0, subdivisions: 8, min_sample_distance: 0.0))],
        )"#,
    );
    let (target, hand) = swinging_target(&mut app);
    let instance = spawn_vfx(&mut app, &package, target);
    for _ in 0..(MAX_TRAIL_SAMPLES + 40) {
        swing(&mut app, hand, 0.05);
        app.update();
    }
    let (_, state) = trail_state(&app, instance);
    assert_eq!(state.samples.len(), MAX_TRAIL_SAMPLES);
    assert!(state.drawn_points <= MAX_TRAIL_POINTS, "{}", state.drawn_points);
}

fn ghost_state(app: &App, instance: Entity) -> AfterimageLayerState {
    let e = app.world().get::<VfxInstance>(instance).unwrap().afterimage_layers()[0];
    app.world().get::<AfterimageLayerState>(e).unwrap().clone()
}

fn ghost_count(app: &mut App) -> usize {
    let mut q = app.world_mut().query_filtered::<Entity, (With<VfxGhost>, With<Mesh3d>)>();
    q.iter(app.world()).count()
}

#[test]
fn afterimages_leave_capped_copies_that_stay_and_go() {
    let mut app = app();
    let package = add_package(
        &mut app,
        r#"(layers: [Afterimage((name: "ghosts", interval: 0.1, lifetime: 0.5, max_ghosts: 3))])"#,
    );
    let mesh = app.world_mut().resource_mut::<Assets<Mesh>>().add(Cuboid::new(0.1, 1.0, 0.1));
    let target = app.world_mut().spawn(Transform::default()).id();
    let blade = app
        .world_mut()
        .spawn((Name::new("Blade"), Mesh3d(mesh.clone()), Transform::from_xyz(0.0, 0.5, 0.0), ChildOf(target)))
        .id();
    let instance = spawn_vfx(&mut app, &package, target);
    app.update();
    app.update();
    let first = ghost_state(&app, instance);
    assert_eq!(first.ghosts.len(), 1, "a copy right when emission starts");
    let ghost = first.ghosts[0].meshes[0];
    assert!(app.world().get::<ChildOf>(ghost).is_none(), "copies are in the world, not on the target");
    assert_eq!(app.world().get::<Mesh3d>(ghost).unwrap().0, mesh);
    let left_at = app.world().get::<GlobalTransform>(blade).unwrap().translation();
    // Move the target: the copy stays where it was left.
    for _ in 0..2 {
        app.world_mut().get_mut::<Transform>(target).unwrap().translation.x += 1.0;
        app.update();
    }
    assert!(app.world().get::<GlobalTransform>(ghost).unwrap().translation().distance(left_at) < 1e-5);
    let state = ghost_state(&app, instance);
    assert_eq!(state.ghosts.len(), 2, "one copy per 0.1 s at 0.05 s frames");
    let newest = state.ghosts.back().unwrap().meshes[0];
    let at = app.world().get::<GlobalTransform>(newest).unwrap().translation();
    assert!((at.x - 2.0).abs() < 1e-4 || (at.x - 1.0).abs() < 1e-4, "{}", at);
    // The copies fade: older ones are more transparent.
    let fades: Vec<f32> = state
        .ghosts
        .iter()
        .map(|g| app.world().resource::<Assets<OmagariEffectMaterial>>().get(&g.material).unwrap().uniforms.params.w)
        .collect();
    assert!(fades[0] < fades[1], "{:?}", fades);
    // The cap.
    for _ in 0..10 {
        app.update();
    }
    assert_eq!(ghost_state(&app, instance).ghosts.len(), 3);
    assert_eq!(ghost_count(&mut app), 3, "older copies despawned");
    // Stop: no new copies, the last ones fade out, then everything goes.
    control(&mut app, move |vfx| {
        vfx.stop(instance);
    });
    for _ in 0..12 {
        app.update();
    }
    assert!(app.world().get_entity(instance).is_err());
    assert_eq!(ghost_count(&mut app), 0);
    let mut joints = app.world_mut().query_filtered::<Entity, With<VfxGhost>>();
    assert_eq!(joints.iter(app.world()).count(), 0);
}

#[test]
fn skinned_copies_get_frozen_joints() {
    let mut app = app();
    let package = add_package(
        &mut app,
        r#"(triggers: [(name: "go")], layers: [Afterimage((name: "ghosts", interval: 10.0, lifetime: 5.0, emit: Trigger(start: "go", stop: "go")))])"#,
    );
    let mesh = app.world_mut().resource_mut::<Assets<Mesh>>().add(Cuboid::new(0.1, 1.0, 0.1));
    let target = app.world_mut().spawn(Transform::default()).id();
    let root = app.world_mut().spawn((Name::new("Hips"), Transform::from_xyz(0.0, 1.0, 0.0), ChildOf(target))).id();
    let arm = app.world_mut().spawn((Name::new("Arm"), Transform::from_xyz(0.3, 0.2, 0.0), ChildOf(root))).id();
    let skin = SkinnedMesh { inverse_bindposes: Handle::default(), joints: vec![root, arm] };
    let body = app
        .world_mut()
        .spawn((Name::new("Body"), Mesh3d(mesh.clone()), skin.clone(), Transform::default(), ChildOf(target)))
        .id();
    app.world_mut().spawn((Name::new("Cloth"), Mesh3d(mesh), skin, Transform::default(), ChildOf(target)));
    let instance = spawn_vfx(&mut app, &package, target);
    app.update();
    app.update();
    assert!(ghost_state(&app, instance).ghosts.is_empty(), "waits for the trigger");
    fire(&mut app, instance, "go");
    app.update();
    let state = ghost_state(&app, instance);
    assert_eq!(state.ghosts.len(), 1);
    let ghost = &state.ghosts[0];
    assert_eq!(ghost.meshes.len(), 2, "both skinned meshes");
    assert_eq!(ghost.joints.len(), 2, "the meshes share the frozen joints");
    let arm_then = *app.world().get::<GlobalTransform>(arm).unwrap();
    let copy = app.world().get::<SkinnedMesh>(ghost.meshes[0]).unwrap().clone();
    assert_ne!(copy.joints, vec![root, arm], "the copy's own joints");
    assert!(app.world().get::<SkinnedMesh>(body).unwrap().joints == vec![root, arm], "the model is untouched");
    // The pose moves on; the frozen joints keep the old one.
    app.world_mut().get_mut::<Transform>(arm).unwrap().translation.y += 0.5;
    app.update();
    app.update();
    let frozen = *app.world().get::<GlobalTransform>(copy.joints[1]).unwrap();
    assert_eq!(frozen, arm_then);
    assert_ne!(*app.world().get::<GlobalTransform>(arm).unwrap(), arm_then);
    // Despawning the instance takes the copies and joints along.
    app.world_mut().entity_mut(instance).despawn();
    app.update();
    assert_eq!(ghost_count(&mut app), 0);
    assert!(app.world().get_entity(copy.joints[0]).is_err());
}
