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

/// Adds the test package directly to `Assets` (no file loading).
fn add_package(app: &mut App) -> Handle<VfxPackage> {
    let project = ProjectV2::from_ron_str(PROJECT).unwrap();
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

#[test]
fn target_despawn_removes_instance() {
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
    assert!(app.world().resource::<Finished>().0.is_empty());
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
            file_path: "examples".to_string(),
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
