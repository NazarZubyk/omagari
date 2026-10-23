//! Sockets, segments, size fit and the own-model rule, headless
//! (`MinimalPlugins` + transform propagation, no rendering).

use std::time::Duration;

use bevy::asset::AssetPlugin;
use bevy::ecs::system::RunSystemOnce;
use bevy::image::ImagePlugin;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use bevy::transform::TransformPlugin;
use bevy::world_serialization::WorldAssetRoot;
use bevy_hanabi::prelude::*;
use omagari::format::ProjectV2;
use omagari::runtime::{TargetHierarchy, VfxLayer, SOCKET_TIMEOUT};
use omagari::{OmagariPlugin, VfxCommandsExt, VfxInstance, VfxPackage, VfxSeparateModel};

const DT: f32 = 0.1;

/// `tip` is on the node "Blade" (+1 m up), `base` is 0.5 m above the target
/// root; layer "glow" follows `tip`, layer "edge" spans base → tip, layer
/// "lost" looks for a node that never exists.
const PROJECT: &str = r#"(
    sockets: [
        (name: "tip", node: Some("Blade"), offset: (0.0, 1.0, 0.0)),
        (name: "base", node: None, offset: (0.0, 0.5, 0.0)),
    ],
    layers: [
        Particles((name: "glow", attach: Socket("tip"))),
        Particles((
            name: "edge",
            attach: Segment(from: "base", to: "tip"),
            effect: (init_modifiers: [
                SetAttribute((attr: "position", attr_expr: Operator(Add(Property("segment_a"), Operator(Multiply(Operator(Subtract(Property("segment_b"), Property("segment_a"))), RandFloat)))))),
            ]),
        )),
        Particles((name: "lost", attach: Socket("Nowhere"))),
    ],
)"#;

fn app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, TransformPlugin, AssetPlugin::default(), ImagePlugin::default()))
        .init_asset::<EffectAsset>()
        .init_asset::<Mesh>()
        .add_plugins(OmagariPlugin)
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(DT)));
    app
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

fn layers(app: &App, instance: Entity) -> Vec<Entity> {
    app.world().get::<VfxInstance>(instance).unwrap().layers().to_vec()
}

fn parent(app: &App, entity: Entity) -> Entity {
    app.world().get::<ChildOf>(entity).unwrap().parent()
}

fn world_position(app: &App, entity: Entity) -> Vec3 {
    app.world().get::<GlobalTransform>(entity).unwrap().translation()
}

fn property(app: &App, entity: Entity, name: &str) -> Option<Vec3> {
    match app.world().get::<EffectProperties>(entity)?.get_stored(name)? {
        Value::Vector(v) => Some(v.as_vec3()),
        _ => None,
    }
}

fn close(a: Vec3, b: Vec3) -> bool {
    (a - b).length() < 1e-4
}

#[test]
fn sockets_resolve_when_the_hierarchy_appears() {
    let mut app = app();
    let package = add_package(&mut app, PROJECT);
    let target = app.world_mut().spawn(Transform::from_xyz(10.0, 0.0, 0.0)).id();
    let instance = spawn_vfx(&mut app, &package, target);
    app.update();
    app.update();
    let [glow, edge, lost] = layers(&app, instance)[..] else {
        panic!("three layers");
    };

    // No "Blade" yet: the socket layer sits at the target root + offset.
    assert_eq!(parent(&app, glow), instance);
    assert!(close(world_position(&app, glow), Vec3::new(10.0, 1.0, 0.0)));
    // The segment's ends: base, and tip at the root until the node exists.
    // The layer is Local at the target root, so ends are target-relative.
    assert!(close(property(&app, edge, "segment_a").unwrap(), Vec3::new(0.0, 0.5, 0.0)));
    assert!(close(property(&app, edge, "segment_b").unwrap(), Vec3::new(0.0, 1.0, 0.0)));

    // The model appears later (a glTF scene loading): target → Arm → Blade.
    let arm = app
        .world_mut()
        .spawn((Name::new("Arm"), Transform::from_xyz(1.0, 0.0, 0.0), ChildOf(target)))
        .id();
    let blade = app
        .world_mut()
        .spawn((Name::new("Blade"), Transform::from_xyz(0.0, 2.0, 0.0), ChildOf(arm)))
        .id();
    app.update();
    app.update();
    assert_eq!(parent(&app, glow), blade, "the layer moved under the node");
    assert!(close(world_position(&app, glow), Vec3::new(11.0, 3.0, 0.0)), "{}", world_position(&app, glow));
    assert!(close(property(&app, edge, "segment_b").unwrap(), Vec3::new(1.0, 3.0, 0.0)));
    assert!(close(property(&app, edge, "segment_a").unwrap(), Vec3::new(0.0, 0.5, 0.0)));

    // Moving the node moves the layer and the segment end.
    app.world_mut().get_mut::<Transform>(arm).unwrap().translation = Vec3::new(-1.0, 0.0, 0.0);
    app.update();
    assert!(close(world_position(&app, glow), Vec3::new(9.0, 3.0, 0.0)));
    assert!(close(property(&app, edge, "segment_b").unwrap(), Vec3::new(-1.0, 3.0, 0.0)));

    // A socket that never resolves stays at the target root (after the
    // timeout too: fallback, one warning).
    for _ in 0..((SOCKET_TIMEOUT / DT) as usize + 2) {
        app.update();
    }
    assert_eq!(parent(&app, lost), instance);
    assert!(close(world_position(&app, lost), Vec3::new(10.0, 0.0, 0.0)));

    // The node is despawned (weapon removed) with the layer under it: the
    // instance rebuilds its layers, which wait for a new node.
    app.world_mut().entity_mut(blade).despawn();
    app.update();
    app.update();
    let rebuilt = layers(&app, instance);
    assert_eq!(rebuilt.len(), 3);
    assert!(app.world().get_entity(glow).is_err());
    assert_eq!(parent(&app, rebuilt[0]), instance);
    let new_blade = app
        .world_mut()
        .spawn((Name::new("Blade"), Transform::from_xyz(0.0, 4.0, 0.0), ChildOf(arm)))
        .id();
    app.update();
    app.update();
    assert_eq!(parent(&app, rebuilt[0]), new_blade);
    assert!(close(world_position(&app, rebuilt[0]), Vec3::new(9.0, 5.0, 0.0)));
}

/// A socket name with no package socket is a node name; nodes inside VFX
/// instances are never matched.
#[test]
fn node_names_without_package_sockets() {
    let mut app = app();
    let package = add_package(&mut app, r#"(layers: [Particles((name: "Head", attach: Socket("Head")))])"#);
    let target = app.world_mut().spawn(Transform::default()).id();
    let instance = spawn_vfx(&mut app, &package, target);
    app.update();
    app.update();
    let layer = layers(&app, instance)[0];
    // The layer itself is named "Head" but is not a candidate.
    assert_eq!(parent(&app, layer), instance);
    let head = app
        .world_mut()
        .spawn((Name::new("Head"), Transform::from_xyz(0.0, 1.7, 0.0), ChildOf(target)))
        .id();
    app.update();
    app.update();
    assert_eq!(parent(&app, layer), head);
    assert!(close(world_position(&app, layer), Vec3::new(0.0, 1.7, 0.0)));
}

/// Global segment layers get positions relative to the emitter, on world
/// axes.
#[test]
fn global_segments_are_emitter_relative() {
    let mut app = app();
    let package = add_package(
        &mut app,
        r#"(
            sockets: [(name: "a", offset: (1.0, 0.0, 0.0)), (name: "b", offset: (0.0, 0.0, 2.0))],
            layers: [Particles((
                name: "line",
                simulation_space: Global,
                attach: Segment(from: "a", to: "b"),
                effect: (init_modifiers: [SetAttribute((attr: "position", attr_expr: Property("segment_b")))]),
            ))],
        )"#,
    );
    let target = app
        .world_mut()
        .spawn(Transform::from_xyz(5.0, 0.0, 0.0).with_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2)))
        .id();
    let instance = spawn_vfx(&mut app, &package, target);
    app.update();
    app.update();
    let layer = layers(&app, instance)[0];
    // Only segment_b is read, so only it is declared and written.
    assert_eq!(property(&app, layer, "segment_a"), None);
    // (0, 0, 2) turned 90° about Y is (2, 0, 0) on world axes.
    let b = property(&app, layer, "segment_b").unwrap();
    assert!(close(b, Vec3::new(2.0, 0.0, 0.0)), "{}", b);
}

fn spawn_mesh(app: &mut App, parent: Entity, mesh: &Handle<Mesh>, transform: Transform) -> Entity {
    app.world_mut()
        .spawn((Mesh3d(mesh.clone()), transform, ChildOf(parent)))
        .id()
}

/// The own-model rule: a target's own meshes stop at nested model roots
/// (a weapon's glTF scene in a hand bone) and at `VfxSeparateModel`.
#[test]
fn own_meshes_stop_at_other_models() {
    let mut app = app();
    let mesh = app.world_mut().resource_mut::<Assets<Mesh>>().add(Cuboid::default());
    let world = app.world_mut();
    let target = world.spawn(Transform::default()).id();
    // The character's glTF scene under the target root is its own model.
    let body = world.spawn((WorldAssetRoot::default(), ChildOf(target))).id();
    let hand = world.spawn((Name::new("Hand"), Transform::default(), ChildOf(body))).id();
    let weapon = world.spawn((WorldAssetRoot::default(), ChildOf(hand))).id();
    let shield = world.spawn((VfxSeparateModel, Transform::default(), ChildOf(target))).id();
    let torso = spawn_mesh(&mut app, body, &mesh, Transform::default());
    let arm = spawn_mesh(&mut app, hand, &mesh, Transform::default());
    let blade = spawn_mesh(&mut app, weapon, &mesh, Transform::default());
    let _boss = spawn_mesh(&mut app, shield, &mesh, Transform::default());
    // A mesh directly on the target counts too.
    app.world_mut().entity_mut(target).insert(Mesh3d(mesh.clone()));

    let mut own = app
        .world_mut()
        .run_system_once(move |h: TargetHierarchy| h.own_meshes(target))
        .unwrap();
    own.sort();
    let mut expected = vec![target, torso, arm];
    expected.sort();
    assert_eq!(own, expected);

    // On the weapon itself, its own meshes are found.
    let weapon_meshes = app
        .world_mut()
        .run_system_once(move |h: TargetHierarchy| h.own_meshes(weapon))
        .unwrap();
    assert_eq!(weapon_meshes, vec![blade]);
}

#[test]
fn fit_scales_instances_to_their_target() {
    let mut app = app();
    let mesh = app.world_mut().resource_mut::<Assets<Mesh>>().add(Cuboid::new(1.0, 1.0, 1.0));
    let no_fit = add_package(&mut app, r#"(layers: [Particles((name: "a"))])"#);
    let scale_fit = add_package(&mut app, r#"(fit: Some(TargetScale), layers: [Particles((name: "a"))])"#);
    let height_fit = add_package(&mut app, r#"(fit: Some(TargetHeight(1.5)), layers: [Particles((name: "a"))])"#);

    // A target scaled ×2 whose model is a 1 m cube scaled ×2 under it: 4 m tall.
    let target = app.world_mut().spawn(Transform::from_scale(Vec3::splat(2.0))).id();
    spawn_mesh(&mut app, target, &mesh, Transform::from_xyz(0.0, 1.0, 0.0).with_scale(Vec3::splat(2.0)));
    let a = spawn_vfx(&mut app, &no_fit, target);
    let b = spawn_vfx(&mut app, &scale_fit, target);
    let c = spawn_vfx(&mut app, &height_fit, target);
    for _ in 0..4 {
        app.update();
    }
    let world_scale = |app: &App, e: Entity| app.world().get::<GlobalTransform>(e).unwrap().scale();
    assert!(close(world_scale(&app, a), Vec3::ONE), "no fit keeps the authored size");
    assert!(close(world_scale(&app, b), Vec3::splat(2.0)));
    assert!(close(world_scale(&app, c), Vec3::splat(4.0 / 1.5)), "{}", world_scale(&app, c));
}

/// Socket layers under a node get the instance's fit scale, not the node's.
#[test]
fn socket_layers_use_the_fit_scale() {
    let mut app = app();
    let package = add_package(
        &mut app,
        r#"(
            fit: Some(TargetScale),
            sockets: [(name: "tip", node: Some("Tip"), offset: (0.0, 1.0, 0.0))],
            layers: [Particles((name: "glow", attach: Socket("tip")))],
        )"#,
    );
    let target = app.world_mut().spawn(Transform::from_scale(Vec3::splat(2.0))).id();
    let tip = app
        .world_mut()
        .spawn((Name::new("Tip"), Transform::from_scale(Vec3::splat(5.0)), ChildOf(target)))
        .id();
    let instance = spawn_vfx(&mut app, &package, target);
    for _ in 0..4 {
        app.update();
    }
    let layer = layers(&app, instance)[0];
    assert_eq!(parent(&app, layer), tip);
    let global = app.world().get::<GlobalTransform>(layer).unwrap();
    assert!(close(global.scale(), Vec3::splat(2.0)), "{}", global.scale());
    // The offset scales with the fit too: 1 m × 2.
    assert!(close(global.translation(), Vec3::new(0.0, 2.0, 0.0)), "{}", global.translation());
    assert_eq!(app.world().get::<VfxLayer>(layer).unwrap().instance, instance);
}
