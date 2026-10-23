//! Legacy `*.hanabi.ron` effects: `EffectComplex::spawn` returns the effect
//! entities, and the despawn timer stops them gracefully.

use std::time::Duration;

use bevy::asset::AssetPlugin;
use bevy::ecs::system::RunSystemOnce;
use bevy::image::ImagePlugin;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use bevy_hanabi::prelude::*;
use bevy_hanabi::register_modifiers;
use omagari::{EffectComplex, OmagariPlugin};

const DT: f32 = 0.1;

#[test]
fn legacy_spawn_returns_entities_and_stops_gracefully() {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin {
            file_path: "tests/golden".to_string(),
            ..default()
        },
        ImagePlugin::default(),
    ))
    .init_asset::<EffectAsset>()
    .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(DT)));
    // HanabiPlugin registers its modifiers for reflection; without the
    // render plugin, do it by hand.
    register_modifiers(app.world().resource::<AppTypeRegistry>());
    app.add_plugins(OmagariPlugin);

    // Two effects, lifetimes up to 5.51 s.
    let handle: Handle<EffectComplex> = app.world().resource::<AssetServer>().load("explode.hanabi.ron");
    for _ in 0..500 {
        app.update();
        let server = app.world().resource::<AssetServer>();
        if server.is_loaded(&handle) || server.load_state(&handle).is_failed() {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let complexes = app.world().resource::<Assets<EffectComplex>>();
    let complex = complexes.get(&handle).expect("legacy file loads");
    let lingers = complex.stop_durations();
    assert!(!lingers.is_empty());
    let longest = lingers.iter().copied().fold(0.0, f32::max);
    assert!((longest - 5.51).abs() < 1e-3, "{:?}", lingers);

    let entities = app
        .world_mut()
        .run_system_once(move |mut commands: Commands, complexes: Res<Assets<EffectComplex>>| {
            let textures = vec![Handle::<Image>::default(); 7];
            complexes
                .get(&handle)
                .unwrap()
                .spawn(&mut commands, &textures, Vec3::new(1.0, 0.0, 0.0), Some(Duration::from_secs_f32(0.5)))
        })
        .unwrap();
    assert_eq!(entities.len(), lingers.len());
    app.update();
    for &e in &entities {
        assert!(app.world().get::<ParticleEffect>(e).is_some());
        app.world_mut()
            .entity_mut(e)
            .insert(EffectSpawner::new(&SpawnerSettings::rate(10.0.into())));
    }

    // After 0.5 s the spawners stop, but the effects live on.
    for _ in 0..8 {
        app.update();
    }
    for &e in &entities {
        assert!(app.world().get_entity(e).is_ok(), "not despawned abruptly");
        assert!(!app.world().get::<EffectSpawner>(e).unwrap().active, "spawner stopped");
    }
    // Despawned once the longest particle life has passed.
    for _ in 0..((longest / DT) as usize + 3) {
        app.update();
    }
    for &e in &entities {
        assert!(app.world().get_entity(e).is_err());
    }
}
