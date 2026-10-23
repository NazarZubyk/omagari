//! VFX instance lifecycle and package loading, headless (`MinimalPlugins`, no
//! rendering): bevy_hanabi components exist, nothing is simulated.

use std::time::Duration;

use bevy::asset::AssetPlugin;
use bevy::ecs::system::RunSystemOnce;
use bevy::image::{CompressedImageFormats, ImageLoader, ImagePlugin};
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use bevy_hanabi::prelude::*;
use omagari::format::ProjectV2;
use omagari::runtime::VfxState;
use omagari::{OmagariPlugin, VfxCommandsExt, VfxControl, VfxFinished, VfxInstance, VfxPackage};

const DT: f32 = 0.1;

/// A parent layer whose particles live up to 1.5 s and a child layer (spawned
/// from parent particles) living 0.5 s: stopping takes 2.0 s.
const PROJECT: &str = r#"(
    layers: [
        Particles((
            name: "parent",
            effect: (init_modifiers: [
                SetAttribute((attr: "lifetime", attr_expr: Operator(Uniform(Float(1.0), Float(1.5))))),
            ]),
        )),
        Particles((
            name: "child",
            parent: Some("parent"),
            effect: (init_modifiers: [SetAttribute((attr: "lifetime", attr_expr: Float(0.5)))]),
        )),
    ],
)"#;

#[derive(Resource, Default)]
struct Finished(Vec<(VfxFinished, f32)>);

fn record_finished(
    mut reader: MessageReader<VfxFinished>,
    mut finished: ResMut<Finished>,
    time: Res<Time>,
) {
    for message in reader.read() {
        finished.0.push((*message, time.elapsed_secs()));
    }
}

fn app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default(), ImagePlugin::default()))
        .init_asset::<EffectAsset>()
        .add_plugins(OmagariPlugin)
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(DT)))
        .init_resource::<Finished>()
        .add_systems(Update, record_finished.after(omagari::runtime::VfxSystems));
    app
}

/// One-shot layers: a burst whose particles live up to 0.8 s, emitted twice
/// 0.5 s apart; finishes by itself 1.3 s after it starts.
const ONE_SHOT: &str = r#"(
    layers: [
        Particles((
            name: "burst",
            play: OnStart,
            effect: (
                spawner: (count: Single(10.0), spawn_duration: Single(0.0), period: Single(0.5), cycle_count: 2),
                init_modifiers: [
                    SetAttribute((attr: "lifetime", attr_expr: Operator(Uniform(Float(0.4), Float(0.8))))),
                ],
            ),
        )),
    ],
)"#;

/// Adds the test package directly to `Assets` (no file loading).
fn add_package(app: &mut App) -> Handle<VfxPackage> {
    add_package_from(app, PROJECT)
}

fn add_package_from(app: &mut App, ron: &str) -> Handle<VfxPackage> {
    let project = ProjectV2::from_ron_str(ron).unwrap();
    let world = app.world_mut();
    let package = world.resource_scope(|world, mut effects: Mut<Assets<EffectAsset>>| {
        let server = world.resource::<AssetServer>();
        let mut warnings = Vec::new();
        let mut assets = (&mut *effects, server);
        let package = VfxPackage::build(project, &mut assets, &mut warnings);
        assert!(warnings.is_empty(), "{:?}", warnings);
        package
    });
    world.resource_mut::<Assets<VfxPackage>>().add(package)
}

fn spawn_target(app: &mut App) -> Entity {
    app.world_mut()
        .spawn((Transform::default(), Visibility::default()))
        .id()
}

fn spawn_vfx(app: &mut App, package: &Handle<VfxPackage>, target: Entity) -> Entity {
    let package = package.clone();
    app.world_mut()
        .run_system_once(move |mut commands: Commands| commands.spawn_vfx(&package, target))
        .unwrap()
}

fn stop(app: &mut App, instance: Entity) -> bool {
    app.world_mut()
        .run_system_once(move |mut vfx: VfxControl| vfx.stop(instance))
        .unwrap()
}

fn is_alive(app: &mut App, instance: Entity) -> bool {
    app.world_mut()
        .run_system_once(move |vfx: VfxControl| vfx.is_alive(instance))
        .unwrap()
}

fn state(app: &App, instance: Entity) -> VfxState {
    app.world().get::<VfxInstance>(instance).unwrap().state()
}

#[test]
fn instance_spawns_layers_and_stops_gracefully() {
    let mut app = app();
    let package = add_package(&mut app);
    assert_eq!(app.world().resource::<Assets<VfxPackage>>().get(&package).unwrap().stop_duration(), 2.0);

    let target = spawn_target(&mut app);
    let instance = spawn_vfx(&mut app, &package, target);
    app.update();

    // The instance is a child of the target, the layers children of the instance.
    assert_eq!(app.world().get::<ChildOf>(instance).unwrap().parent(), target);
    assert_eq!(state(&app, instance), VfxState::Running);
    let layers = app.world().get::<VfxInstance>(instance).unwrap().layers().to_vec();
    assert_eq!(layers.len(), 2);
    for &layer in &layers {
        assert_eq!(app.world().get::<ChildOf>(layer).unwrap().parent(), instance);
        assert!(app.world().get::<ParticleEffect>(layer).is_some());
    }
    assert!(app.world().get::<EffectParent>(layers[0]).is_none());
    assert_eq!(app.world().get::<EffectParent>(layers[1]).unwrap().entity, layers[0]);

    // bevy_hanabi inserts the spawner when it ticks; stand in for it.
    for &layer in &layers {
        app.world_mut()
            .entity_mut(layer)
            .insert(EffectSpawner::new(&SpawnerSettings::rate(10.0.into())));
    }

    for _ in 0..5 {
        app.update();
    }
    assert!(is_alive(&mut app, instance));
    assert!(stop(&mut app, instance));
    let stopped_at = app.world().resource::<Time>().elapsed_secs();
    app.update();
    for &layer in &layers {
        assert!(!app.world().get::<EffectSpawner>(layer).unwrap().active, "spawners stop");
    }
    assert!(matches!(state(&app, instance), VfxState::Stopping { .. }));
    assert!(app.world().resource::<Finished>().0.is_empty());

    for _ in 0..40 {
        app.update();
    }
    let finished = &app.world().resource::<Finished>().0;
    assert_eq!(finished.len(), 1, "VfxFinished is sent once");
    let (message, at) = finished[0];
    assert_eq!(message, VfxFinished { instance, target });
    let took = at - stopped_at;
    assert!((took - 2.0).abs() <= DT * 1.5, "finished {} s after stop, expected 2.0", took);

    assert!(app.world().get_entity(instance).is_err(), "instance despawned");
    for &layer in &layers {
        assert!(app.world().get_entity(layer).is_err(), "layers despawned");
    }
    assert!(app.world().get_entity(target).is_ok(), "target kept");
    assert!(!is_alive(&mut app, instance));
    assert!(!stop(&mut app, instance));
}

/// Despawning the target despawns its instances and sends `VfxFinished`
/// with the (gone) target.
#[test]
fn target_despawn_removes_instance_and_reports_it() {
    let mut app = app();
    let package = add_package(&mut app);
    let target = spawn_target(&mut app);
    let instance = spawn_vfx(&mut app, &package, target);
    app.update();
    let layers = app.world().get::<VfxInstance>(instance).unwrap().layers().to_vec();

    app.world_mut().entity_mut(target).despawn();
    app.update();
    assert!(app.world().get_entity(instance).is_err());
    for layer in layers {
        assert!(app.world().get_entity(layer).is_err());
    }
    assert!(!is_alive(&mut app, instance));
    app.update();
    let finished = &app.world().resource::<Finished>().0;
    assert_eq!(finished.len(), 1, "VfxFinished is sent once");
    assert_eq!(finished[0].0, VfxFinished { instance, target });
}

fn restart(app: &mut App, instance: Entity) -> bool {
    app.world_mut()
        .run_system_once(move |mut vfx: VfxControl| vfx.restart(instance))
        .unwrap()
}

/// bevy_hanabi inserts the spawner when it ticks; stand in for it.
fn insert_spawners(app: &mut App, layers: &[Entity]) {
    for &layer in layers {
        app.world_mut()
            .entity_mut(layer)
            .insert(EffectSpawner::new(&SpawnerSettings::rate(10.0.into())));
    }
}

#[test]
fn restart_while_stopping_reuses_the_instance() {
    let mut app = app();
    let package = add_package(&mut app);
    let target = spawn_target(&mut app);
    let instance = spawn_vfx(&mut app, &package, target);
    app.update();
    let layers = app.world().get::<VfxInstance>(instance).unwrap().layers().to_vec();
    insert_spawners(&mut app, &layers);

    assert!(restart(&mut app, instance), "restarting a running instance is a no-op");
    assert_eq!(state(&app, instance), VfxState::Running);

    assert!(stop(&mut app, instance));
    for _ in 0..10 {
        app.update();
    }
    assert!(matches!(state(&app, instance), VfxState::Stopping { .. }));
    assert!(!app.world().get::<EffectSpawner>(layers[0]).unwrap().active);

    assert!(restart(&mut app, instance));
    app.update();
    assert_eq!(state(&app, instance), VfxState::Running);
    assert_eq!(app.world().get::<VfxInstance>(instance).unwrap().layers(), &layers[..], "same layers");
    for &layer in &layers {
        assert!(app.world().get::<EffectSpawner>(layer).unwrap().active, "spawners run again");
    }
    // Past the old stop deadline: still running, nothing finished.
    for _ in 0..30 {
        app.update();
    }
    assert_eq!(state(&app, instance), VfxState::Running);
    assert!(app.world().resource::<Finished>().0.is_empty());

    // Stop again: finishes normally.
    assert!(stop(&mut app, instance));
    for _ in 0..30 {
        app.update();
    }
    assert_eq!(app.world().resource::<Finished>().0.len(), 1);
    assert!(!restart(&mut app, instance), "nothing to restart once finished");
}

#[test]
fn one_shot_instances_finish_by_themselves() {
    let mut app = app();
    let package = add_package_from(&mut app, ONE_SHOT);
    let auto = app.world().resource::<Assets<VfxPackage>>().get(&package).unwrap().auto_finish;
    assert!(auto.is_some_and(|a| (a - 1.3).abs() < 1e-5), "{:?}", auto);
    // Looping packages do not.
    let looping = add_package(&mut app);
    assert_eq!(app.world().resource::<Assets<VfxPackage>>().get(&looping).unwrap().auto_finish, None);

    let target = spawn_target(&mut app);
    let instance = spawn_vfx(&mut app, &package, target);
    let looping_instance = spawn_vfx(&mut app, &looping, target);
    app.update();
    let started = app.world().resource::<Time>().elapsed_secs();
    for _ in 0..30 {
        app.update();
    }
    let finished = &app.world().resource::<Finished>().0;
    assert_eq!(finished.len(), 1, "{:?}", finished);
    let (message, at) = finished[0];
    assert_eq!(message, VfxFinished { instance, target });
    assert!((at - started - 1.3).abs() <= DT * 1.5, "finished {} s after start", at - started);
    assert!(app.world().get_entity(instance).is_err());
    assert!(is_alive(&mut app, looping_instance));
}

/// `spawn_vfx_at`: an instance in the world, no target, cleaned up when it
/// finishes.
#[test]
fn world_instances_play_at_a_transform_and_clean_up() {
    let mut app = app();
    let package = add_package_from(&mut app, ONE_SHOT);
    let instance = app
        .world_mut()
        .run_system_once(move |mut commands: Commands| {
            commands.spawn_vfx_at(&package, Transform::from_xyz(1.0, 2.0, 3.0).with_scale(Vec3::splat(2.0)))
        })
        .unwrap();
    app.update();
    let inst = app.world().get::<VfxInstance>(instance).unwrap();
    assert!(inst.is_in_world());
    assert_eq!(inst.target(), instance);
    assert_eq!(inst.layers().len(), 1);
    assert!(app.world().get::<ChildOf>(instance).is_none());
    let transform = app.world().get::<Transform>(instance).unwrap();
    assert_eq!(transform.translation, Vec3::new(1.0, 2.0, 3.0));
    assert_eq!(transform.scale, Vec3::splat(2.0), "fit leaves world instances alone");
    for _ in 0..20 {
        app.update();
    }
    assert!(app.world().get_entity(instance).is_err());
    let finished = &app.world().resource::<Finished>().0;
    assert_eq!(finished.len(), 1);
    assert_eq!(finished[0].0, VfxFinished { instance, target: instance });
}

/// A modified package asset (hot reload) rebuilds the running instance's
/// layers in place: same instance, new layers, old ones fade out.
#[test]
fn hot_reload_rebuilds_layers_and_keeps_the_instance() {
    let mut app = app();
    let package = add_package(&mut app);
    let target = spawn_target(&mut app);
    let instance = spawn_vfx(&mut app, &package, target);
    app.update();
    let old_layers = app.world().get::<VfxInstance>(instance).unwrap().layers().to_vec();
    insert_spawners(&mut app, &old_layers);

    // Replace the package's content, as the loader does on a file change.
    let replacement = {
        let project = ProjectV2::from_ron_str(ONE_SHOT).unwrap();
        let world = app.world_mut();
        world.resource_scope(|world, mut effects: Mut<Assets<EffectAsset>>| {
            let server = world.resource::<AssetServer>();
            VfxPackage::build(project, &mut (&mut *effects, server), &mut Vec::new())
        })
    };
    app.world_mut()
        .resource_mut::<Assets<VfxPackage>>()
        .insert(package.id(), replacement)
        .unwrap();
    app.update();
    app.update();

    let inst = app.world().get::<VfxInstance>(instance).unwrap();
    assert_eq!(inst.state(), VfxState::Running);
    assert_eq!(inst.layers().len(), 1, "the new package has one layer");
    assert!(!old_layers.contains(&inst.layers()[0]));
    let retired: Vec<Entity> = inst.retired_layers().collect();
    assert_eq!(retired, old_layers, "old layers fade out");
    for &layer in &old_layers {
        assert!(!app.world().get::<EffectSpawner>(layer).unwrap().active, "old layers stop emitting");
    }
    // The old package's particles live up to 2 s.
    for _ in 0..25 {
        app.update();
    }
    for &layer in &old_layers {
        assert!(app.world().get_entity(layer).is_err(), "old layers despawned");
    }
    assert!(app.world().resource::<Finished>().0.iter().all(|(m, _)| m.instance == instance));
}

/// The editor's RENDER path: a new package replaces the instance's package.
#[test]
fn replace_package_restarts_a_stopping_instance() {
    let mut app = app();
    let package = add_package(&mut app);
    let other = add_package_from(&mut app, ONE_SHOT);
    let target = spawn_target(&mut app);
    let instance = spawn_vfx(&mut app, &package, target);
    app.update();
    assert!(stop(&mut app, instance));
    app.update();
    let replaced = app
        .world_mut()
        .run_system_once(move |mut vfx: VfxControl| vfx.replace_package(instance, other.clone()))
        .unwrap();
    assert!(replaced);
    app.update();
    let inst = app.world().get::<VfxInstance>(instance).unwrap();
    assert_eq!(inst.state(), VfxState::Running);
    assert_eq!(inst.layers().len(), 1);
    assert_eq!(app.world().get::<Name>(inst.layers()[0]).unwrap().as_str(), "burst");
}

/// A package built in memory waits for its textures before spawning layers.
#[test]
fn in_memory_packages_wait_for_their_textures() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default(), ImagePlugin::default()))
        .register_asset_loader(ImageLoader::new(CompressedImageFormats::empty()))
        .init_asset::<EffectAsset>()
        .add_plugins(OmagariPlugin);
    // `glow1.png` is in this repository's `assets/`.
    let package = add_package_from(
        &mut app,
        r#"(
            assets: [(id: "glow", kind: Texture((path: "glow1.png")))],
            layers: [Particles((name: "dots", texture: Some("glow")))],
        )"#,
    );
    let target = spawn_target(&mut app);
    let instance = spawn_vfx(&mut app, &package, target);
    let texture = app.world().resource::<Assets<VfxPackage>>().get(&package).unwrap().particle_layers[0]
        .texture
        .clone()
        .unwrap();
    for _ in 0..500 {
        app.update();
        let loaded = app.world().resource::<AssetServer>().is_loaded(&texture);
        let running = state(&app, instance) == VfxState::Running;
        assert!(loaded || !running, "layers spawned before the texture loaded");
        if running {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(state(&app, instance), VfxState::Running);
    assert!(app.world().resource::<Assets<Image>>().get(&texture).is_some());
}

#[test]
fn stop_before_package_loads_finishes_without_layers() {
    let mut app = app();
    let target = spawn_target(&mut app);
    // A handle whose asset never appears.
    let package = Handle::<VfxPackage>::default();
    let instance = spawn_vfx(&mut app, &package, target);
    app.update();
    assert_eq!(state(&app, instance), VfxState::Starting);
    assert!(stop(&mut app, instance));
    app.update();
    app.update();
    let finished = &app.world().resource::<Finished>().0;
    assert_eq!(finished.len(), 1);
    assert!(app.world().get_entity(instance).is_err());
}

/// A v1 project file loads through the asset server as a package (migrated),
/// with its built-in texture loaded from the library's embedded textures.
#[test]
fn loader_reads_v1_project_and_builtin_textures() {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin {
            file_path: "tests/golden/inputs".to_string(),
            ..default()
        },
        ImagePlugin::default(),
    ))
    // The render plugin registers the image loader in real apps.
    .register_asset_loader(ImageLoader::new(CompressedImageFormats::empty()))
    .init_asset::<EffectAsset>()
    .add_plugins(OmagariPlugin);

    let package: Handle<VfxPackage> = app
        .world()
        .resource::<AssetServer>()
        .load("fireworks.omagari.ron");
    for _ in 0..500 {
        app.update();
        let server = app.world().resource::<AssetServer>();
        if server.is_loaded_with_dependencies(&package) || server.load_state(&package).is_failed() {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let server = app.world().resource::<AssetServer>();
    assert!(
        server.is_loaded_with_dependencies(&package),
        "{:?}",
        server.recursive_dependency_load_state(&package)
    );

    let packages = app.world().resource::<Assets<VfxPackage>>();
    let package = packages.get(&package).unwrap();
    assert_eq!(package.particle_layers.len(), 3);
    let effects = app.world().resource::<Assets<EffectAsset>>();
    let images = app.world().resource::<Assets<Image>>();
    for layer in package.particle_layers.iter() {
        assert!(effects.get(&layer.effect).is_some(), "{}", layer.name);
        let texture = layer.texture.as_ref().expect("texture");
        assert!(images.get(texture).is_some(), "{} texture", layer.name);
        assert_eq!(
            texture.path().unwrap().to_string(),
            "embedded://omagari/textures/glow1.png"
        );
    }
    assert_eq!(package.particle_layers[1].parent, Some(0));
}
