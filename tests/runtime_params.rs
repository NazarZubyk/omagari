//! Parameters and triggers at runtime, headless (`MinimalPlugins`, no
//! rendering): bevy_hanabi's spawners are stood in for, so these tests check
//! what the runtime writes (spawner switches, spawn counts, properties).

use std::time::Duration;

use bevy::asset::AssetPlugin;
use bevy::ecs::system::RunSystemOnce;
use bevy::image::ImagePlugin;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use bevy_hanabi::prelude::*;
use omagari::format::{param_property_name, ParamValue, ProjectV2};
use omagari::runtime::VfxLayerState;
use omagari::{OmagariPlugin, TriggerArgs, VfxCommandsExt, VfxControl, VfxInstance, VfxPackage, VfxSpawn};

const DT: f32 = 0.1;

/// Three parameters (any names), one trigger with a pulse, and a layer per
/// way of using them: bindings (`rate`, `visible`, expressions), a burst on
/// the trigger reading its payload, and a layer playing while a parameter is
/// high.
const PROJECT: &str = r#"(
    parameters: [
        (name: "power", default: Float(0.0), min: 0.0, max: 1.0, smoothing: 0.0),
        (name: "glow", default: Float(0.0), min: 0.0, max: 2.0, smoothing: 0.5),
        (name: "tint", default: Color((1.0, 0.0, 0.0, 1.0)), smoothing: 0.0),
    ],
    triggers: [
        (name: "hit", pulses: [(param: "glow", to: 2.0, decay: 0.5)]),
    ],
    layers: [
        Particles((
            name: "aura",
            rate: Some(Param("power", [(0.0, 0.0), (1.0, 20.0)])),
            visible: Some((param: "power", from: 0.5, to: 1.0, fade: 0.25)),
            effect: (
                spawner: (count: Single(10.0), spawn_duration: Single(1.0), period: Single(1.0), cycle_count: 0),
                init_modifiers: [
                    SetAttribute((attr: "lifetime", attr_expr: Float(0.5))),
                    SetAttribute((attr: "hdr_color", attr_expr: Operator(Mix(Param("tint"), Vec4((1.0, 1.0, 1.0, 1.0)), Param("power"))))),
                    SetAttribute((attr: "size", attr_expr: Operator(Multiply(Param("glow"), Property("visibility"))))),
                ],
            ),
        )),
        Particles((
            name: "burst",
            play: OnTrigger("hit"),
            effect: (
                spawner: (count: Single(30.0), spawn_duration: Single(0.0), period: Single(1.0), cycle_count: 1),
                init_modifiers: [
                    SetAttribute((attr: "lifetime", attr_expr: Float(0.3))),
                    SetAttribute((attr: "position", attr_expr: Property("trigger_pos"))),
                    SetAttribute((attr: "velocity", attr_expr: Operator(Multiply(Property("trigger_dir"), Property("trigger_strength"))))),
                ],
            ),
        )),
        Particles((
            name: "charge",
            play: While("power", 0.8),
            effect: (
                spawner: (count: Single(5.0), spawn_duration: Single(1.0), period: Single(1.0), cycle_count: 0),
                init_modifiers: [SetAttribute((attr: "lifetime", attr_expr: Float(0.2)))],
            ),
        )),
    ],
)"#;

fn app() -> App {
    let mut app = App::new();
    // Transforms are propagated: payloads are converted into layer space.
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin::default(),
        ImagePlugin::default(),
        bevy::transform::TransformPlugin,
    ))
        .init_asset::<EffectAsset>()
        .add_plugins(OmagariPlugin)
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(DT)));
    app
}

fn build(app: &mut App, ron: &str) -> VfxPackage {
    let project = ProjectV2::from_ron_str(ron).unwrap();
    app.world_mut().resource_scope(|world, mut effects: Mut<Assets<EffectAsset>>| {
        let server = world.resource::<AssetServer>();
        let mut warnings = Vec::new();
        let package = VfxPackage::build(project, &mut (&mut *effects, server), &mut warnings);
        assert!(warnings.is_empty(), "{:?}", warnings);
        package
    })
}

fn add_package(app: &mut App, ron: &str) -> Handle<VfxPackage> {
    let package = build(app, ron);
    app.world_mut().resource_mut::<Assets<VfxPackage>>().add(package)
}

fn control<R: 'static>(app: &mut App, mut f: impl FnMut(&mut VfxControl) -> R + Send + Sync + 'static) -> R
where
    R: Send,
{
    app.world_mut()
        .run_system_once(move |mut vfx: VfxControl| f(&mut vfx))
        .unwrap()
}

fn layers(app: &App, instance: Entity) -> Vec<Entity> {
    app.world().get::<VfxInstance>(instance).unwrap().layers().to_vec()
}

/// Stands in for bevy_hanabi, which inserts each layer's spawner from its
/// effect asset on the first tick, and for visibility propagation (spawn
/// counts are written for visible layers only).
fn insert_spawners(app: &mut App, instance: Entity) {
    let package = app.world().get::<VfxInstance>(instance).unwrap().package().clone();
    for (index, layer) in layers(app, instance).into_iter().enumerate() {
        let world = app.world();
        let handle = world.resource::<Assets<VfxPackage>>().get(&package).unwrap().particle_layers[index].effect.clone();
        let spawner = EffectSpawner::new(&world.resource::<Assets<EffectAsset>>().get(&handle).unwrap().spawner);
        app.world_mut().entity_mut(layer).insert((spawner, InheritedVisibility::VISIBLE));
    }
}

fn spawner(app: &App, layer: Entity) -> &EffectSpawner {
    app.world().get::<EffectSpawner>(layer).unwrap()
}

fn property(app: &App, layer: Entity, name: &str) -> Option<Value> {
    app.world().get::<EffectProperties>(layer).and_then(|p| p.get_stored(name))
}

fn float(app: &mut App, instance: Entity, name: &'static str) -> f32 {
    control(app, move |vfx| vfx.param(instance, name)).and_then(|v| v.as_float()).unwrap()
}

fn spawn(app: &mut App, package: &Handle<VfxPackage>, target: Entity, params: Vec<(&'static str, ParamValue)>) -> Entity {
    let package = package.clone();
    app.world_mut()
        .run_system_once(move |mut commands: Commands| {
            commands.spawn_vfx_with(&package, target, VfxSpawn { params: &params })
        })
        .unwrap()
}

/// Starting values, smoothing, clamping, the upload into the layers'
/// properties, the visible factor, and values surviving a package
/// replacement and a hot reload.
#[test]
fn parameters_smooth_upload_and_survive_rebuilds() {
    let mut app = app();
    let package = add_package(&mut app, PROJECT);
    let target = app.world_mut().spawn((Transform::default(), Visibility::default())).id();
    let instance = spawn(&mut app, &package, target, vec![("power", 0.6.into())]);
    app.update();
    let aura = layers(&app, instance)[0];

    assert_eq!(float(&mut app, instance, "power"), 0.6, "starting values are not smoothed");
    app.update();
    assert_eq!(property(&app, aura, &param_property_name("power")), Some(Value::from(0.6f32)));
    assert_eq!(
        property(&app, aura, &param_property_name("tint")),
        Some(Value::from(Vec4::new(1.0, 0.0, 0.0, 1.0))),
        "colours are vec4 properties"
    );

    // Smoothing 0.5 s: one 0.1 s frame moves 1 - e^(-0.2) of the way.
    control(&mut app, move |vfx| vfx.set_param(instance, "glow", 1.0));
    app.update();
    let glow = float(&mut app, instance, "glow");
    assert!((glow - (1.0 - (-0.2f32).exp())).abs() < 1e-4, "{}", glow);
    assert_eq!(property(&app, aura, &param_property_name("glow")), Some(Value::from(glow)));

    // Clamped to the range; outside [from, to] the layer fades over `fade`.
    control(&mut app, move |vfx| vfx.set_param(instance, "power", 7.0));
    app.update();
    assert_eq!(float(&mut app, instance, "power"), 1.0);
    control(&mut app, move |vfx| vfx.set_param_now(instance, "power", 0.4));
    app.update();
    let state = *app.world().get::<VfxLayerState>(aura).unwrap();
    assert!((state.visibility - 0.6).abs() < 1e-5, "{:?}", state);
    assert_eq!(state.rate, Some(8.0), "rate curve at 0.4");
    assert_eq!(property(&app, aura, "visibility"), Some(Value::from(state.visibility)));

    // Unknown names and wrong kinds are ignored (with a warning), no panic.
    assert!(control(&mut app, move |vfx| vfx.set_param(instance, "nope", 1.0)));
    assert!(control(&mut app, move |vfx| vfx.set_param(instance, "tint", 1.0)));
    assert!(control(&mut app, move |vfx| vfx.set_param(instance, "power", Color::WHITE)));
    assert_eq!(float(&mut app, instance, "power"), 0.4);

    // A rebuilt package (the editor's RENDER): new layers, same values,
    // uploaded into the new layers.
    let replacement = build(&mut app, PROJECT);
    let replacement = app.world_mut().resource_mut::<Assets<VfxPackage>>().add(replacement);
    control(&mut app, move |vfx| vfx.replace_package(instance, replacement.clone()));
    app.update();
    app.update();
    let new_aura = layers(&app, instance)[0];
    assert_ne!(new_aura, aura);
    assert_eq!(float(&mut app, instance, "power"), 0.4);
    assert_eq!(property(&app, new_aura, &param_property_name("power")), Some(Value::from(0.4f32)));

    // Hot reload of a package whose range shrank: the value is kept and
    // clamped to the new range.
    let package_id = app.world().get::<VfxInstance>(instance).unwrap().package().id();
    let narrower = build(&mut app, &PROJECT.replace("min: 0.0, max: 1.0, smoothing: 0.0", "min: 0.0, max: 0.3, smoothing: 0.0"));
    app.world_mut().resource_mut::<Assets<VfxPackage>>().insert(package_id, narrower).unwrap();
    app.update();
    app.update();
    assert_eq!(float(&mut app, instance, "power"), 0.3);
    let reloaded_aura = layers(&app, instance)[0];
    assert_ne!(reloaded_aura, new_aura, "layers were rebuilt");
    assert_eq!(property(&app, reloaded_aura, &param_property_name("power")), Some(Value::from(0.3f32)));
}

/// `OnTrigger` layers burst on their trigger (with the payload in their
/// properties), not at the start and not on restart; pulses jump and decay;
/// triggers fired before the layers exist wait for them.
#[test]
fn triggers_burst_pulse_and_carry_a_payload() {
    let mut app = app();
    let package = add_package(&mut app, PROJECT);
    let target = app.world_mut().spawn((Transform::from_xyz(5.0, 0.0, 0.0), Visibility::default())).id();
    let instance = spawn(&mut app, &package, target, Vec::new());
    // Fired in the same frame as the spawn: waits for the layers.
    control(&mut app, move |vfx| vfx.trigger_at(instance, "hit", TriggerArgs::at(Vec3::new(6.0, 2.0, 0.0)).with_direction(Vec3::X * 3.0).with_strength(2.0)));
    app.update();
    let [aura, burst, _] = layers(&app, instance)[..] else { panic!() };
    assert_eq!(float(&mut app, instance, "glow"), 2.0, "the pulse jumps to `to`");
    insert_spawners(&mut app, instance);
    assert!(!spawner(&app, burst).active, "OnTrigger layers do not start by themselves");
    assert!(spawner(&app, aura).active);

    app.update();
    assert!(spawner(&app, burst).active, "the waiting burst plays once the spawner exists");
    assert_eq!(property(&app, burst, "trigger_pos"), Some(Value::from(Vec3::new(1.0, 2.0, 0.0))), "layer space");
    assert_eq!(property(&app, burst, "trigger_dir"), Some(Value::from(Vec3::X)));
    assert_eq!(property(&app, burst, "trigger_strength"), Some(Value::from(2.0f32)));
    // 0.1 s into a 0.5 s decay back to 0.
    let glow = float(&mut app, instance, "glow");
    assert!((glow - 1.6).abs() < 1e-4, "{}", glow);
    for _ in 0..5 {
        app.update();
    }
    assert!(float(&mut app, instance, "glow").abs() < 1e-5, "back to the target");

    // Without a payload: the layer's origin, +Y, strength 1.
    app.world_mut().get_mut::<EffectSpawner>(burst).unwrap().active = false;
    control(&mut app, move |vfx| vfx.trigger(instance, "hit"));
    app.update();
    assert!(spawner(&app, burst).active);
    assert_eq!(property(&app, burst, "trigger_pos"), Some(Value::from(Vec3::ZERO)));
    assert_eq!(property(&app, burst, "trigger_dir"), Some(Value::from(Vec3::Y)));

    // Stop + restart: looping layers play again, the burst does not.
    control(&mut app, move |vfx| vfx.stop(instance));
    app.update();
    assert!(!spawner(&app, burst).active && !spawner(&app, aura).active);
    assert!(control(&mut app, move |vfx| vfx.restart(instance)));
    app.update();
    assert!(spawner(&app, aura).active);
    assert!(!spawner(&app, burst).active, "restart does not burst OnTrigger layers");

    // Unknown triggers are ignored.
    assert!(control(&mut app, move |vfx| vfx.trigger(instance, "nope")));
    app.update();
    assert!(!spawner(&app, burst).active);
}

/// `While` layers emit while their parameter is at or above the threshold;
/// a `rate` binding sets the spawn count per frame.
#[test]
fn while_layers_and_bound_rates_follow_their_parameter() {
    let mut app = app();
    let package = add_package(&mut app, PROJECT);
    let target = app.world_mut().spawn((Transform::default(), Visibility::default())).id();
    let instance = spawn(&mut app, &package, target, vec![("power", 0.6.into())]);
    app.update();
    insert_spawners(&mut app, instance);
    let [aura, _, charge] = layers(&app, instance)[..] else { panic!() };
    app.update();
    assert!(!spawner(&app, charge).active, "0.6 < 0.8");

    control(&mut app, move |vfx| vfx.set_param(instance, "power", 0.9));
    app.update();
    assert!(spawner(&app, charge).active, "0.9 ≥ 0.8");

    // 0.9 → 18 particles per second: 1.8 per 0.1 s frame, fractions carried.
    let mut spawned = 0;
    for _ in 0..10 {
        app.update();
        spawned += spawner(&app, aura).spawn_count;
    }
    assert!((17..=19).contains(&spawned), "{} spawned in 1 s", spawned);

    control(&mut app, move |vfx| vfx.set_param(instance, "power", 0.0));
    app.update();
    assert!(!spawner(&app, charge).active);
    app.update();
    assert_eq!(spawner(&app, aura).spawn_count, 0, "rate 0 and invisible: nothing spawns");
}

/// Builds packages whose texture is one image already in `Assets<Image>`.
struct FixedTexture<'a> {
    effects: &'a mut Assets<EffectAsset>,
    image: Handle<Image>,
}

impl omagari::runtime::PackageAssets for FixedTexture<'_> {
    fn add_effect(&mut self, _label: String, effect: EffectAsset) -> Handle<EffectAsset> {
        self.effects.add(effect)
    }

    fn load_texture(&mut self, _texture: &omagari::format::TextureAssetDef) -> Handle<Image> {
        self.image.clone()
    }
}

/// A hot reload that changes a texture's settings applies them to the image
/// already loaded (Bevy keeps the settings of a path's first load).
#[test]
fn hot_reload_applies_changed_texture_settings() {
    use bevy::image::{ImageAddressMode, ImageSampler};
    use bevy::render::render_resource::TextureFormat;

    let mut app = app();
    let mut image = Image::default();
    image.texture_descriptor.format = TextureFormat::Rgba8UnormSrgb;
    let image = app.world_mut().resource_mut::<Assets<Image>>().add(image);
    let project = |purpose: &str| {
        format!(
            r#"(assets: [(id: "t", kind: Texture((path: "vfx/textures/t.png", purpose: {})))],
                layers: [Particles((name: "a", texture: Some("t"), effect: (render_modifiers: [ParticleTexture(())])))])"#,
            purpose
        )
    };
    let build = |app: &mut App, ron: &str| {
        let project = ProjectV2::from_ron_str(ron).unwrap();
        let image = image.clone();
        app.world_mut().resource_scope(|_, mut effects: Mut<Assets<EffectAsset>>| {
            VfxPackage::build(project, &mut FixedTexture { effects: &mut effects, image }, &mut Vec::new())
        })
    };
    let package = build(&mut app, &project("Sprite"));
    let package = app.world_mut().resource_mut::<Assets<VfxPackage>>().add(package);
    let target = app.world_mut().spawn((Transform::default(), Visibility::default())).id();
    spawn(&mut app, &package, target, Vec::new());
    app.update();

    let reloaded = build(&mut app, &project("Noise"));
    app.world_mut().resource_mut::<Assets<VfxPackage>>().insert(package.id(), reloaded).unwrap();
    app.update();
    app.update();
    let images = app.world().resource::<Assets<Image>>();
    let image = images.get(&image).unwrap();
    assert_eq!(image.texture_descriptor.format, TextureFormat::Rgba8Unorm, "noise is linear");
    let ImageSampler::Descriptor(sampler) = &image.sampler else { panic!("{:?}", image.sampler) };
    assert_eq!(sampler.address_mode_u, ImageAddressMode::Repeat, "noise repeats");
}
