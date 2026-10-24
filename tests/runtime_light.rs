//! Light layers, headless (`MinimalPlugins` + transform propagation, no
//! rendering): one `PointLight` entity per layer, values from the bindings
//! (parameters, age), flicker, placement (target, socket, segment middle,
//! trigger payload), fades and the stop, one-shot flashes on a trigger,
//! `Between` lights, auto-finish, the global light cap and rebuilds.

use std::time::Duration;

use bevy::asset::AssetPlugin;
use bevy::ecs::system::RunSystemOnce;
use bevy::image::ImagePlugin;
use bevy::light::PointLight;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use bevy::transform::TransformPlugin;
use bevy_hanabi::prelude::*;
use omagari::format::ProjectV2;
use omagari::runtime::{LightLayerState, VfxState};
use omagari::{OmagariPlugin, TriggerArgs, VfxCommandsExt, VfxControl, VfxFinished, VfxInstance, VfxPackage, VfxSettings};

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

fn lights(app: &App, instance: Entity) -> Vec<Entity> {
    app.world().get::<VfxInstance>(instance).unwrap().light_layers().to_vec()
}

fn light(app: &App, e: Entity) -> PointLight {
    *app.world().get::<PointLight>(e).unwrap()
}

fn state(app: &App, e: Entity) -> LightLayerState {
    app.world().get::<LightLayerState>(e).unwrap().clone()
}

fn shown(app: &App, e: Entity) -> bool {
    *app.world().get::<Visibility>(e).unwrap() != Visibility::Hidden
}

fn position(app: &App, e: Entity) -> Vec3 {
    app.world().get::<GlobalTransform>(e).unwrap().translation()
}

fn updates(app: &mut App, n: usize) {
    for _ in 0..n {
        app.update();
    }
}

const GLOW: &str = r#"(
    parameters: [
        (name: "charge", default: Float(0.0), min: 0.0, max: 1.0, smoothing: 0.0),
        (name: "tint", default: Color((0.2, 0.4, 1.0, 1.0)), smoothing: 0.0),
    ],
    sockets: [(name: "hand", node: Some("Hand"), offset: (0.0, 0.1, 0.0))],
    layers: [
        Light((
            name: "glow",
            attach: Socket("hand"),
            offset: (0.0, 0.0, 0.5),
            color: Param("tint", []),
            intensity: Param("charge", [(0.0, 0.0), (1.0, 40000.0)]),
            range: Param("charge", [(0.0, 2.0), (1.0, 4.0)]),
            radius: 0.05,
            fade: (in: 0.0, out: 0.4),
        )),
    ],
)"#;

/// Colour, intensity and range follow the parameters; the light sits at
/// its socket + offset in the socket's space; dark lights are hidden; a
/// stop fades it out over `fade.out` and the instance waits for it.
#[test]
fn bindings_drive_the_point_light_and_the_stop_waits_for_the_fade() {
    let mut app = app();
    let package = add_package(&mut app, GLOW);
    let target = app.world_mut().spawn((Transform::from_xyz(1.0, 0.0, 0.0), Visibility::default())).id();
    // The hand node, turned 90° about Y (its +Z points to world +X).
    app.world_mut().spawn((
        Name::new("Hand"),
        Transform::from_xyz(0.0, 1.0, 0.0).with_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2)),
        Visibility::default(),
        ChildOf(target),
    ));
    let instance = spawn_vfx(&mut app, &package, target);
    updates(&mut app, 3);
    let [glow] = lights(&app, instance)[..] else { panic!("one light entity") };
    assert_eq!(light(&app, glow).intensity, 0.0, "charge 0: dark");
    assert!(!shown(&app, glow), "a dark light is hidden");
    assert!(!light(&app, glow).shadow_maps_enabled, "no shadows by default");

    control(&mut app, move |vfx| {
        vfx.set_param(instance, "charge", 0.5);
    });
    updates(&mut app, 1);
    let l = light(&app, glow);
    assert!((l.intensity - 20000.0).abs() < 1.0, "{}", l.intensity);
    assert!((l.range - 3.0).abs() < 1e-4, "{}", l.range);
    assert!((l.radius - 0.05).abs() < 1e-6);
    assert_eq!(l.color.to_linear(), LinearRgba::rgb(0.2, 0.4, 1.0), "the Color parameter itself");
    assert!(shown(&app, glow));
    // Hand at (1, 1, 0) world; socket offset 0.1 up; light offset 0.5 along the hand's +Z = world +X.
    let p = position(&app, glow);
    assert!((p - Vec3::new(1.5, 1.1, 0.0)).length() < 1e-4, "{}", p);

    control(&mut app, move |vfx| {
        vfx.set_param(instance, "charge", 1.0);
    });
    updates(&mut app, 1);
    assert!((light(&app, glow).intensity - 40000.0).abs() < 1.0);

    // Stop: fades out over 0.4 s, then the instance finishes.
    control(&mut app, move |vfx| {
        vfx.stop(instance);
    });
    updates(&mut app, 1);
    let i = light(&app, glow).intensity;
    assert!((i - 30000.0).abs() < 1.0, "a quarter of the fade: {}", i);
    updates(&mut app, 2);
    assert!(app.world().get::<VfxInstance>(instance).is_some(), "still fading");
    let i = light(&app, glow).intensity;
    assert!(i > 0.0 && i < 15000.0, "{}", i);
    updates(&mut app, 3);
    assert!(app.world().get_entity(instance).is_err(), "finished after the fade");
    assert!(app.world().get_entity(glow).is_err(), "the light went with it");
    assert_eq!(app.world().resource::<Finished>().0.len(), 1);
}

const FLASH: &str = r#"(
    triggers: [(name: "hit"), (name: "swing_start"), (name: "swing_end")],
    layers: [
        Light((
            name: "flash",
            play: OnTrigger("hit"),
            duration: Some(0.4),
            fade: (in: 0.0, out: 0.0),
            intensity: Age([(0.0, 100000.0), (0.4, 0.0)]),
            range: Const(5.0),
        )),
        Light((
            name: "swing",
            play: Between(start: "swing_start", stop: "swing_end"),
            fade: (in: 0.0, out: 0.2),
            intensity: Const(10000.0),
            range: Const(3.0),
            flicker: (amount: 0.25, speed: 6.0),
        )),
    ],
)"#;

/// A hit flash: nothing until the trigger, then the age curve over its
/// duration at the payload position, off after it, again on the next
/// trigger. A Between light shines from its start to its stop trigger, with
/// a flicker that stays inside its bounds and is the same for the same
/// clock and seed.
#[test]
fn trigger_flashes_and_held_lights() {
    let mut app = app();
    let package = add_package(&mut app, FLASH);
    let target = app.world_mut().spawn((Transform::from_xyz(0.0, 1.0, 0.0), Visibility::default())).id();
    let instance = spawn_vfx(&mut app, &package, target);
    updates(&mut app, 3);
    let [flash, swing] = lights(&app, instance)[..] else { panic!() };
    assert_eq!(light(&app, flash).intensity, 0.0, "waits for the trigger");
    assert_eq!(light(&app, swing).intensity, 0.0, "waits for its start");

    control(&mut app, move |vfx| {
        vfx.trigger_at(instance, "hit", TriggerArgs::at(Vec3::new(3.0, 0.5, -2.0)));
    });
    updates(&mut app, 1);
    // The frame of the trigger already shows age dt (as shapes do).
    let i = light(&app, flash).intensity;
    assert!((i - 75000.0).abs() < 1.0, "the start of the flash: {}", i);
    assert!((position(&app, flash) - Vec3::new(3.0, 0.5, -2.0)).length() < 1e-4, "at the hit point");
    updates(&mut app, 1);
    let i = light(&app, flash).intensity;
    assert!((i - 50000.0).abs() < 1.0, "half way down the age curve: {}", i);
    updates(&mut app, 2);
    assert_eq!(light(&app, flash).intensity, 0.0, "off after its duration");
    assert!(!shown(&app, flash));
    control(&mut app, move |vfx| {
        vfx.trigger(instance, "hit");
    });
    updates(&mut app, 1);
    assert!((light(&app, flash).intensity - 75000.0).abs() < 1.0, "flashes again");
    assert!((position(&app, flash) - Vec3::new(0.0, 1.0, 0.0)).length() < 1e-4, "no payload: at the target");

    // Between: on during the swing, flickering within its bounds.
    control(&mut app, move |vfx| {
        vfx.trigger(instance, "swing_start");
    });
    let mut seen = Vec::new();
    for _ in 0..10 {
        app.update();
        let i = light(&app, swing).intensity;
        assert!((7500.0 - 1.0..=12500.0 + 1.0).contains(&i), "inside the flicker bounds: {}", i);
        seen.push(i);
    }
    assert!(seen.windows(2).any(|w| (w[0] - w[1]).abs() > 1.0), "it flickers: {:?}", seen);
    let s = state(&app, swing);
    let def = omagari::format::LightFlickerDef { amount: 0.25, speed: 6.0 };
    assert!((s.intensity - 10000.0 * def.factor(s.clock, s.seed)).abs() < 0.5, "deterministic for the clock and seed");
    control(&mut app, move |vfx| {
        vfx.trigger(instance, "swing_end");
    });
    updates(&mut app, 1);
    let i = light(&app, swing).intensity;
    assert!(i > 0.0 && i < 7000.0, "fading out after the stop trigger: {}", i);
    updates(&mut app, 2);
    assert_eq!(light(&app, swing).intensity, 0.0, "off");

    // OnTrigger / Between lights never let the package finish by itself.
    assert_eq!(app.world().get::<VfxInstance>(instance).unwrap().state(), VfxState::Running);
}

/// A light whose play is OnStart with a duration is a one-shot: the
/// package finishes by itself; a segment light sits in the middle of its two
/// sockets; the size fit scales range by fit and intensity by fit².
#[test]
fn one_shot_lights_finish_and_fit_scales_them() {
    let mut app = app();
    let package = add_package(
        &mut app,
        r#"(
            fit: Some(TargetScale),
            sockets: [(name: "a", offset: (0.0, 0.0, 0.0)), (name: "b", offset: (0.0, 2.0, 0.0))],
            layers: [Light((name: "pop", attach: Segment(from: "a", to: "b"), play: OnStart, duration: Some(0.5),
                fade: (in: 0.0, out: 0.0), intensity: Const(1000.0), range: Const(2.0), radius: 0.1))],
        )"#,
    );
    assert_eq!(app.world().resource::<Assets<VfxPackage>>().get(&package).unwrap().auto_finish, Some(0.5));
    let target = app.world_mut().spawn((Transform::from_xyz(0.0, 0.0, 0.0).with_scale(Vec3::splat(2.0)), Visibility::default())).id();
    let instance = spawn_vfx(&mut app, &package, target);
    updates(&mut app, 2);
    let [pop] = lights(&app, instance)[..] else { panic!() };
    let l = light(&app, pop);
    assert!((l.intensity - 4000.0).abs() < 1e-2, "× fit²: {}", l.intensity);
    assert!((l.range - 4.0).abs() < 1e-4 && (l.radius - 0.2).abs() < 1e-5, "× fit");
    assert!((position(&app, pop) - Vec3::new(0.0, 2.0, 0.0)).length() < 1e-4, "segment middle × fit: {}", position(&app, pop));
    updates(&mut app, 6);
    assert!(app.world().get_entity(instance).is_err(), "finished by itself");
    assert_eq!(app.world().resource::<Finished>().0.len(), 1);
}

/// The global cap: with `max_lights` 2 and three instances, the weakest
/// light is dark; when a stronger one stops, it fades back in; a light
/// pushed out while shining fades out over `light_cap_fade`.
#[test]
fn the_global_cap_keeps_the_strongest_lights() {
    let mut app = app();
    app.insert_resource(VfxSettings { max_lights: 2, light_cap_fade: 0.2 });
    let ron = |lm: f32| {
        format!(
            r#"(parameters: [(name: "power", default: Float(1.0), min: 0.0, max: 10.0, smoothing: 0.0)],
                layers: [Light((name: "l", fade: (in: 0.0, out: 0.0), intensity: Param("power", [(0.0, 0.0), (10.0, {})]), range: Const(3.0)))])"#,
            lm * 10.0
        )
    };
    let weak = add_package(&mut app, &ron(1000.0));
    let mid = add_package(&mut app, &ron(2000.0));
    let strong = add_package(&mut app, &ron(3000.0));
    let mut spawned = Vec::new();
    for package in [&weak, &mid, &strong] {
        let target = app.world_mut().spawn((Transform::default(), Visibility::default())).id();
        spawned.push(spawn_vfx(&mut app, package, target));
    }
    updates(&mut app, 3);
    let [w, m, s] = [lights(&app, spawned[0])[0], lights(&app, spawned[1])[0], lights(&app, spawned[2])[0]];
    assert_eq!(light(&app, w).intensity, 0.0, "the weakest is switched off at once");
    assert!(!shown(&app, w));
    assert!((light(&app, m).intensity - 2000.0).abs() < 1e-2);
    assert!((light(&app, s).intensity - 3000.0).abs() < 1e-2);

    // The strongest goes: the weak one fades in.
    let stopped = spawned[2];
    control(&mut app, move |vfx| {
        vfx.stop(stopped);
    });
    updates(&mut app, 1);
    let i = light(&app, w).intensity;
    assert!(i > 0.0 && i < 1000.0, "fading in: {}", i);
    updates(&mut app, 2);
    assert!((light(&app, w).intensity - 1000.0).abs() < 1e-2, "fully on");

    // The mid light grows weaker than a new strong one: it is pushed out
    // while shining, so it fades out instead of popping.
    let mid_instance = spawned[1];
    control(&mut app, move |vfx| {
        vfx.set_param_now(mid_instance, "power", 0.1);
    });
    let target = app.world_mut().spawn((Transform::default(), Visibility::default())).id();
    let new_strong = spawn_vfx(&mut app, &strong, target);
    updates(&mut app, 1);
    let n = lights(&app, new_strong)[0];
    assert!((light(&app, n).intensity - 3000.0).abs() < 1e-2, "the new strong light gets a place");
    let i = light(&app, m).intensity;
    assert!(i > 0.0 && i < 200.0, "the pushed-out light fades: {}", i);
    updates(&mut app, 2);
    assert_eq!(light(&app, m).intensity, 0.0, "and is off");
    let on = [w, m, n].iter().filter(|&&e| light(&app, e).intensity > 0.0).count();
    assert_eq!(on, 2, "never more than the cap once the fades are done");
}

/// `replace_package` (the editor's live edit, hot reload) rebuilds the
/// light: a looping light of the same name keeps its fade level.
#[test]
fn replace_package_keeps_a_looping_lights_level() {
    let mut app = app();
    let first = add_package(&mut app, r#"(layers: [Light((name: "glow", fade: (in: 1.0, out: 0.5), intensity: Const(1000.0)))])"#);
    let second = add_package(&mut app, r#"(layers: [Light((name: "glow", fade: (in: 1.0, out: 0.5), intensity: Const(2000.0)))])"#);
    let target = app.world_mut().spawn((Transform::default(), Visibility::default())).id();
    let instance = spawn_vfx(&mut app, &first, target);
    updates(&mut app, 12);
    let old = lights(&app, instance)[0];
    assert!((light(&app, old).intensity - 1000.0).abs() < 1e-2, "faded in");
    control(&mut app, move |vfx| {
        vfx.replace_package(instance, second);
    });
    updates(&mut app, 2);
    let new = lights(&app, instance)[0];
    assert_ne!(old, new, "a new light entity");
    assert!(app.world().get_entity(old).is_err(), "the old one is gone");
    assert!((light(&app, new).intensity - 2000.0).abs() < 1e-2, "no fade in again: {}", light(&app, new).intensity);
}
