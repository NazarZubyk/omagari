//! Shape layers, headless (`MinimalPlugins` + transform propagation, no
//! rendering): one entity per layer with a generated mesh and the effect
//! material, placement by follow mode, sockets, triggers restarting the age,
//! one-shot timing and auto-finish, fades, `Age` bindings (shapes and
//! surfaces), restart and replace_package.

use std::time::Duration;

use bevy::asset::AssetPlugin;
use bevy::ecs::system::RunSystemOnce;
use bevy::image::ImagePlugin;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use bevy::transform::TransformPlugin;
use bevy_hanabi::prelude::*;
use omagari::format::ProjectV2;
use omagari::runtime::{OmagariEffectMaterial, ShapeLayerState, ShapeMeshCache, SurfaceLayerState, VfxState};
use omagari::{OmagariPlugin, TriggerArgs, VfxCommandsExt, VfxControl, VfxFinished, VfxInstance, VfxPackage};

const DT: f32 = 0.1;

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

fn shapes(app: &App, instance: Entity) -> Vec<Entity> {
    app.world().get::<VfxInstance>(instance).unwrap().shape_layers().to_vec()
}

fn state(app: &App, shape: Entity) -> ShapeLayerState {
    app.world().get::<ShapeLayerState>(shape).unwrap().clone()
}

fn fade(app: &App, shape: Entity) -> f32 {
    let handle = state(app, shape).material.unwrap();
    app.world().resource::<Assets<OmagariEffectMaterial>>().get(&handle).unwrap().uniforms.params.w
}

fn shown(app: &App, shape: Entity) -> bool {
    *app.world().get::<Visibility>(shape).unwrap() != Visibility::Hidden
}

fn global(app: &App, e: Entity) -> GlobalTransform {
    *app.world().get::<GlobalTransform>(e).unwrap()
}

const RING: &str = r#"(
    triggers: [(name: "slam")],
    layers: [
        Shape((
            name: "ring",
            shape: Ring(inner: 0.5, outer: 0.7),
            follow: PositionYaw,
            transform: (offset: (0.0, -0.9, 0.0)),
            fade: (in: 0.0, out: 0.2),
        )),
        Shape((
            name: "wave",
            shape: Ring(inner: 0.8, outer: 1.0),
            follow: Position,
            scale: Age([(0.0, (0.2, 1.0, 0.2)), (0.5, (2.2, 1.0, 2.2))]),
            play: OnTrigger("slam"),
            duration: Some(0.5),
            fade: (in: 0.0, out: 0.0),
        )),
    ],
)"#;

#[test]
fn shapes_spawn_with_meshes_and_follow_their_modes() {
    let mut app = app();
    let package = add_package(&mut app, RING);
    // A target leaning forward and turned left, at (2, 1, 0).
    let rotation = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2) * Quat::from_rotation_x(-0.4);
    let target = app.world_mut().spawn(Transform::from_xyz(2.0, 1.0, 0.0).with_rotation(rotation)).id();
    let instance = spawn_vfx(&mut app, &package, target);
    for _ in 0..3 {
        app.update();
    }
    let list = shapes(&app, instance);
    assert_eq!(list.len(), 2);
    for &shape in list.iter() {
        assert_eq!(app.world().get::<ChildOf>(shape).unwrap().parent(), instance);
        assert!(app.world().get::<Mesh3d>(shape).is_some());
        assert!(app.world().get::<MeshMaterial3d<OmagariEffectMaterial>>(shape).is_some());
    }
    let (ring, wave) = (list[0], list[1]);
    assert_eq!(app.world().resource::<ShapeMeshCache>().len(), 2, "one mesh per distinct shape");

    // The ground ring: level (up stays up) while the target leans, 0.9 m
    // below the root along world Y, facing the target's heading.
    let g = global(&app, ring);
    let (_, r, t) = g.to_scale_rotation_translation();
    assert!((r * Vec3::Y - Vec3::Y).length() < 1e-4, "flat: {}", r * Vec3::Y);
    assert!((t - Vec3::new(2.0, 0.1, 0.0)).length() < 1e-4, "{}", t);
    assert!(((r * Vec3::NEG_Z) - Vec3::NEG_X).length() < 1e-3, "turned like the target");
    assert!(shown(&app, ring) && fade(&app, ring) == 1.0);

    // The target moves: the ring is there the same frame (placed after
    // transform propagation).
    app.world_mut().get_mut::<Transform>(target).unwrap().translation = Vec3::new(5.0, 1.0, 3.0);
    app.update();
    assert!((global(&app, ring).translation() - Vec3::new(5.0, 0.1, 3.0)).length() < 1e-4);

    // The triggered wave waits.
    assert!(!shown(&app, wave));
    assert_eq!(fade(&app, wave), 0.0);
}

#[test]
fn triggers_play_one_shots_from_age_zero() {
    let mut app = app();
    let package = add_package(&mut app, RING);
    let target = app.world_mut().spawn(Transform::default()).id();
    let instance = spawn_vfx(&mut app, &package, target);
    for _ in 0..3 {
        app.update();
    }
    let wave = shapes(&app, instance)[1];
    control(&mut app, move |vfx| {
        vfx.trigger(instance, "slam");
    });
    app.update();
    assert!(shown(&app, wave));
    let s = state(&app, wave);
    assert!(s.player.active && (s.player.age - DT).abs() < 1e-5, "age {}", s.player.age);
    // Scale over age: 0.2 → 2.2 over 0.5 s; world axes (follow Position).
    for _ in 0..2 {
        app.update();
    }
    let scale = global(&app, wave).to_scale_rotation_translation().0;
    assert!((scale.x - 1.4).abs() < 1e-3 && (scale.y - 1.0).abs() < 1e-4, "at 0.3 s: {}", scale);
    // Gone after its duration.
    for _ in 0..3 {
        app.update();
    }
    assert!(!shown(&app, wave) && !state(&app, wave).player.active);
    // A trigger with a payload plays it again from 0, at the contact point.
    control(&mut app, move |vfx| {
        vfx.trigger_at(instance, "slam", TriggerArgs::at(Vec3::new(4.0, 0.0, -1.0)));
    });
    app.update();
    let s = state(&app, wave);
    assert!(s.player.active && s.player.age < 0.15);
    assert!((global(&app, wave).translation() - Vec3::new(4.0, 0.0, -1.0)).length() < 1e-4);
    // The looping ring was never interrupted; the instance runs on.
    assert_eq!(app.world().get::<VfxInstance>(instance).unwrap().state(), VfxState::Running);
}

#[test]
fn one_shot_shapes_finish_the_instance_and_stops_wait_for_fades() {
    let mut app = app();
    let package = add_package(
        &mut app,
        r#"(layers: [Shape((name: "pop", play: OnStart, duration: Some(0.4), fade: (in: 0.1, out: 0.1),
            material: (opacity: Age([(0.0, 1.0), (0.4, 0.5)]))))])"#,
    );
    assert_eq!(app.world().resource::<Assets<VfxPackage>>().get(&package).unwrap().auto_finish, Some(0.4));
    let target = app.world_mut().spawn(Transform::default()).id();
    let instance = spawn_vfx(&mut app, &package, target);
    app.update();
    app.update();
    let pop = shapes(&app, instance)[0];
    let first = fade(&app, pop);
    assert!(first > 0.0 && first <= 1.0, "fading in: {}", first);
    for _ in 0..8 {
        app.update();
    }
    assert!(app.world().get_entity(instance).is_err(), "finished by itself after the one-shot");
    assert_eq!(app.world().resource::<Finished>().0, vec![VfxFinished { instance, target }]);

    // A looping shape: the stop waits for its fade out.
    let package = add_package(&mut app, r#"(layers: [Shape((name: "orb", fade: (in: 0.0, out: 0.3)))])"#);
    assert_eq!(app.world().resource::<Assets<VfxPackage>>().get(&package).unwrap().auto_finish, None);
    let instance = spawn_vfx(&mut app, &package, target);
    for _ in 0..3 {
        app.update();
    }
    let orb = shapes(&app, instance)[0];
    assert_eq!(fade(&app, orb), 1.0);
    control(&mut app, move |vfx| {
        vfx.stop(instance);
    });
    app.update();
    let level = fade(&app, orb);
    assert!(level > 0.0 && level < 1.0, "fading out: {}", level);
    for _ in 0..5 {
        app.update();
    }
    assert!(app.world().get_entity(instance).is_err(), "gone after the fade out");
    assert!(app.world().get_entity(orb).is_err(), "the shape went with it");
}

#[test]
fn sockets_place_shapes_on_nodes() {
    let mut app = app();
    let package = add_package(
        &mut app,
        r#"(sockets: [(name: "hand", node: Some("Hand"), offset: (0.0, 0.1, 0.0))],
            layers: [Shape((name: "orb", attach: Socket("hand"), follow: Full, shape: Sphere(radius: 0.1)))])"#,
    );
    let target = app.world_mut().spawn(Transform::default()).id();
    let hand = app
        .world_mut()
        .spawn((
            Name::new("Hand"),
            Transform::from_xyz(0.5, 1.2, 0.0)
                .with_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2))
                .with_scale(Vec3::splat(0.01)),
            ChildOf(target),
        ))
        .id();
    let instance = spawn_vfx(&mut app, &package, target);
    for _ in 0..4 {
        app.update();
    }
    let orb = shapes(&app, instance)[0];
    let (scale, rotation, translation) = global(&app, orb).to_scale_rotation_translation();
    // Offset turned by the node (+Y → -X), the node's 0.01 scale ignored.
    assert!((translation - Vec3::new(0.4, 1.2, 0.0)).length() < 1e-4, "{}", translation);
    assert!(rotation.abs_diff_eq(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2), 1e-4));
    assert!((scale - Vec3::ONE).length() < 1e-4, "{}", scale);
    let _ = hand;
}

#[test]
fn age_bindings_reach_surfaces_and_restart_starts_them_over() {
    let mut app = app();
    let package = add_package(
        &mut app,
        r#"(layers: [
            Surface((name: "skin", material: (opacity: Age([(0.0, 0.0), (1.0, 1.0)])), fade: (in: 0.0, out: 0.5))),
            Shape((name: "orb", play: OnStart, rotation_speed: Age([(0.0, 90.0), (10.0, 90.0)]), fade: (in: 0.0, out: 0.5))),
        ])"#,
    );
    let target = app.world_mut().spawn(Transform::default()).id();
    let instance = spawn_vfx(&mut app, &package, target);
    for _ in 0..6 {
        app.update();
    }
    let surface = app.world().get::<VfxInstance>(instance).unwrap().surface_layers()[0];
    let age = app.world().get::<SurfaceLayerState>(surface).unwrap().age;
    assert!(age > 0.3, "{}", age);
    let materials = app.world().resource::<Assets<OmagariEffectMaterial>>();
    let handle = app.world().get::<SurfaceLayerState>(surface).unwrap().materials[0].clone();
    let opacity = materials.get(&handle).unwrap().uniforms.params.x;
    assert!((opacity - age).abs() < 1e-4, "opacity follows the age: {} vs {}", opacity, age);
    let orb = shapes(&app, instance)[0];
    let spin = state(&app, orb).spin;
    assert!(spin > 20.0, "spins 90°/s: {}", spin);

    // Stop, then restart while stopping: ages start over.
    control(&mut app, move |vfx| {
        vfx.stop(instance);
    });
    app.update();
    control(&mut app, move |vfx| {
        assert!(vfx.restart(instance));
    });
    app.update();
    assert!(app.world().get::<SurfaceLayerState>(surface).unwrap().age < 0.15);
    assert!(state(&app, orb).player.age < 0.15);
    assert_eq!(app.world().get::<VfxInstance>(instance).unwrap().generation(), 1);
}

#[test]
fn replace_package_rebuilds_shapes_and_keeps_the_fade() {
    let mut app = app();
    let package = add_package(&mut app, r#"(layers: [Shape((name: "orb", fade: (in: 0.2, out: 0.2)))])"#);
    let target = app.world_mut().spawn(Transform::default()).id();
    let instance = spawn_vfx(&mut app, &package, target);
    for _ in 0..5 {
        app.update();
    }
    let old = shapes(&app, instance)[0];
    assert_eq!(fade(&app, old), 1.0);
    let bigger = add_package(&mut app, r#"(layers: [Shape((name: "orb", shape: Sphere(radius: 2.0), fade: (in: 0.2, out: 0.2)))])"#);
    control(&mut app, move |vfx| {
        assert!(vfx.replace_package(instance, bigger.clone()));
    });
    app.update();
    app.update();
    let new = shapes(&app, instance)[0];
    assert_ne!(old, new);
    assert!(app.world().get_entity(old).is_err());
    assert_eq!(fade(&app, new), 1.0, "a rebuilt looping shape does not fade in again");
    app.update();
    assert_eq!(app.world().resource::<ShapeMeshCache>().len(), 1, "the old sphere's mesh is dropped from the cache");
}
