//! Distortion layers, headless (`MinimalPlugins` + transform propagation, no
//! rendering): one entity per layer with a generated mesh and the
//! distortion material, uniforms from the bindings, **nothing drawn while
//! the strength is 0**, a capsule spanning a segment's two sockets, a ripple
//! played by a trigger (scale over its age, gone after its duration),
//! surface-source overlays on the target's meshes, fades, the stop,
//! auto-finish and rebuilds.

use std::time::Duration;

use bevy::asset::AssetPlugin;
use bevy::ecs::system::RunSystemOnce;
use bevy::image::ImagePlugin;
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use bevy::transform::TransformPlugin;
use bevy_hanabi::prelude::*;
use omagari::format::{ProjectV2, TextureAssetDef};
use omagari::runtime::{
    DistortionLayerState, OmagariDistortionMaterial, PackageAssets, VfxDistortionOverlay, VfxState,
    SEGMENT_LENGTH_STEP,
};
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

/// Textures come from memory (a headless app does not load files).
struct MemoryTextures<'a> {
    effects: &'a mut Assets<EffectAsset>,
    texture: Handle<Image>,
}

impl PackageAssets for MemoryTextures<'_> {
    fn add_effect(&mut self, _label: String, effect: EffectAsset) -> Handle<EffectAsset> {
        self.effects.add(effect)
    }

    fn load_texture(&mut self, _texture: &TextureAssetDef) -> Handle<Image> {
        self.texture.clone()
    }
}

fn package_of(app: &mut App, project: ProjectV2) -> Handle<VfxPackage> {
    let world = app.world_mut();
    let texture = world.resource_mut::<Assets<Image>>().add(Image::default());
    let package = world.resource_scope(|_, mut effects: Mut<Assets<EffectAsset>>| {
        let mut warnings = Vec::new();
        let mut assets = MemoryTextures { effects: &mut effects, texture };
        let package = VfxPackage::build(project, &mut assets, &mut warnings);
        assert!(warnings.is_empty(), "{:?}", warnings);
        package
    });
    world.resource_mut::<Assets<VfxPackage>>().add(package)
}

fn add_package(app: &mut App, ron: &str) -> Handle<VfxPackage> {
    package_of(app, ProjectV2::from_ron_str(ron).unwrap())
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

fn layers(app: &App, instance: Entity) -> Vec<Entity> {
    app.world().get::<VfxInstance>(instance).unwrap().distortion_layers().to_vec()
}

fn shown(app: &App, e: Entity) -> bool {
    *app.world().get::<Visibility>(e).unwrap() != Visibility::Hidden
}

fn material(app: &App, e: Entity) -> OmagariDistortionMaterial {
    let handle = &app.world().get::<MeshMaterial3d<OmagariDistortionMaterial>>(e).expect("a distortion material").0;
    app.world().resource::<Assets<OmagariDistortionMaterial>>().get(handle).unwrap().clone()
}

fn state(app: &App, e: Entity) -> DistortionLayerState {
    app.world().get::<DistortionLayerState>(e).unwrap().clone()
}

/// Local Y extent `(min, max)` of an entity's mesh.
fn mesh_y_range(app: &App, e: Entity) -> (f32, f32) {
    let handle = &app.world().get::<Mesh3d>(e).expect("a mesh").0;
    let mesh = app.world().resource::<Assets<Mesh>>().get(handle).unwrap();
    let Some(VertexAttributeValues::Float32x3(positions)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else {
        panic!("positions")
    };
    positions.iter().fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), p| (lo.min(p[1]), hi.max(p[1])))
}

/// Updates until the instance's layers exist, then two more frames.
fn start(app: &mut App, instance: Entity) {
    for _ in 0..10 {
        app.update();
        if !layers(app, instance).is_empty() {
            break;
        }
    }
    assert!(!layers(app, instance).is_empty(), "the layers did not start");
    updates(app, 2);
}

fn updates(app: &mut App, n: usize) {
    for _ in 0..n {
        app.update();
    }
}

const HAZE: &str = r#"(
    parameters: [(name: "heat", default: Float(0.0), min: 0.0, max: 1.0, smoothing: 0.0)],
    sockets: [(name: "base", node: Some("Hand")), (name: "tip", node: Some("Tip"))],
    assets: [(id: "noise", kind: Texture((path: "omagari://textures/noise.png", purpose: Noise)))],
    layers: [
        Distortion((
            name: "haze",
            attach: Segment(from: "base", to: "tip"),
            source: Shape(shape: Capsule(radius: 0.1, length: 3.0)),
            strength: Param("heat", [(0.0, 0.0), (1.0, 0.01)]),
            noise: (texture: (asset: "noise"), scale: 2.5, scroll: (0.0, 0.8)),
            tint: Const((1.1, 1.0, 0.9, 0.0)),
            fade: (in: 0.0, out: 0.4),
        )),
    ],
)"#;

/// A target with a hand node and a blade tip 0.8 m along the hand's +Y.
fn hand_target(app: &mut App) -> (Entity, Entity) {
    let target = app.world_mut().spawn((Transform::from_xyz(1.0, 0.0, 0.0), Visibility::default())).id();
    // The hand turned 90° about Z: its +Y (the blade) points to world -X.
    let hand = app
        .world_mut()
        .spawn((
            Name::new("Hand"),
            Transform::from_xyz(0.0, 1.0, 0.0).with_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)),
            Visibility::default(),
            ChildOf(target),
        ))
        .id();
    let tip = app
        .world_mut()
        .spawn((Name::new("Tip"), Transform::from_xyz(0.0, 0.8, 0.0), Visibility::default(), ChildOf(hand)))
        .id();
    (target, tip)
}

/// Strength 0 = the layer is not drawn at all; the strength follows its
/// parameter into the uniforms; the capsule spans the segment (middle,
/// along it, the mesh's length = the sockets' distance); the stop fades the
/// strength out and the instance waits for it.
#[test]
fn haze_spans_the_segment_and_strength_zero_draws_nothing() {
    let mut app = app();
    let package = add_package(&mut app, HAZE);
    let (target, _) = hand_target(&mut app);
    let instance = spawn_vfx(&mut app, &package, target);
    start(&mut app, instance);
    let [haze] = layers(&app, instance)[..] else { panic!("one distortion entity") };
    assert!(!shown(&app, haze), "heat 0: strength 0, nothing drawn");
    assert_eq!(material(&app, haze).uniforms.params.x, 0.0);
    assert!(material(&app, haze).noise_a.is_some(), "the noise texture");

    control(&mut app, move |vfx| {
        vfx.set_param(instance, "heat", 0.5);
    });
    updates(&mut app, 1);
    assert!(shown(&app, haze), "drawn once it bends something");
    let u = material(&app, haze).uniforms;
    assert!((u.params.x - 0.005).abs() < 1e-6, "strength from the parameter: {}", u.params);
    assert_eq!(u.params.w, 1.0, "full fade level");
    assert_eq!(u.noise_a, Vec4::new(0.0, 0.8, 2.5, 1.0), "scroll, scale, weight");

    // Hand at (1, 1, 0); the tip 0.8 m along the hand's +Y = world -X.
    let global = app.world().get::<GlobalTransform>(haze).unwrap();
    let (scale, rotation, translation) = global.to_scale_rotation_translation();
    assert!((translation - Vec3::new(0.6, 1.0, 0.0)).length() < 1e-4, "the middle: {}", translation);
    assert!((rotation * Vec3::Y - Vec3::NEG_X).length() < 1e-4, "along the segment: {}", rotation * Vec3::Y);
    assert!((scale - Vec3::ONE).length() < 1e-5, "{}", scale);
    let (lo, hi) = mesh_y_range(&app, haze);
    assert!((hi - lo - (0.8 + 0.2)).abs() < SEGMENT_LENGTH_STEP, "capsule length 0.8 + 2 × radius: {}", hi - lo);
    assert!(state(&app, haze).segment_length.is_some_and(|l| (l - 0.8).abs() < 1e-4));

    // Stop: the strength fades out over 0.4 s, then everything goes.
    control(&mut app, move |vfx| {
        vfx.stop(instance);
    });
    updates(&mut app, 1);
    let w = material(&app, haze).uniforms.params.w;
    assert!((w - 0.75).abs() < 1e-4, "fading: {}", w);
    updates(&mut app, 3);
    assert!(!shown(&app, haze), "faded out: not drawn");
    updates(&mut app, 2);
    assert!(app.world().get_entity(instance).is_err(), "finished after the fade");
    assert!(app.world().get_entity(haze).is_err());
    assert_eq!(app.world().resource::<Finished>().0.len(), 1);
}

/// The capsule follows the sockets when their distance changes: a new mesh
/// for the new length.
#[test]
fn segment_capsule_mesh_follows_the_length() {
    let mut app = app();
    let package = add_package(&mut app, &HAZE.replace("default: Float(0.0)", "default: Float(1.0)"));
    let (target, tip) = hand_target(&mut app);
    let instance = spawn_vfx(&mut app, &package, target);
    start(&mut app, instance);
    let [haze] = layers(&app, instance)[..] else { panic!() };
    let (lo, hi) = mesh_y_range(&app, haze);
    assert!((hi - lo - 1.0).abs() < SEGMENT_LENGTH_STEP);
    // The tip moves out to 1.6 m.
    app.world_mut().get_mut::<Transform>(tip).unwrap().translation.y = 1.6;
    updates(&mut app, 2);
    let (lo, hi) = mesh_y_range(&app, haze);
    assert!((hi - lo - 1.8).abs() < SEGMENT_LENGTH_STEP, "1.6 + 2 × radius: {}", hi - lo);
}

const RIPPLE: &str = r#"(
    triggers: [(name: "slam")],
    assets: [(id: "noise", kind: Texture((path: "omagari://textures/noise.png", purpose: Noise)))],
    layers: [
        Distortion((
            name: "ripple",
            source: Shape(shape: Sphere(radius: 1.0), follow: Position),
            edge: Rim,
            edge_softness: 0.3,
            scale: Age([(0.0, (0.2, 0.2, 0.2)), (0.6, (3.0, 3.0, 3.0))]),
            strength: Age([(0.0, 0.02), (0.6, 0.0)]),
            noise: (texture: (asset: "noise")),
            play: OnTrigger("slam"),
            duration: Some(0.6),
            fade: (in: 0.0, out: 0.0),
        )),
    ],
)"#;

/// A shockwave ripple: nothing until its trigger, then a sphere growing over
/// its age at the payload position with a falling strength, gone after its
/// duration; again on the next trigger.
#[test]
fn ripple_plays_on_its_trigger() {
    let mut app = app();
    let package = add_package(&mut app, RIPPLE);
    let target = app.world_mut().spawn((Transform::default(), Visibility::default())).id();
    let instance = spawn_vfx(&mut app, &package, target);
    start(&mut app, instance);
    let [ripple] = layers(&app, instance)[..] else { panic!() };
    assert!(!shown(&app, ripple), "waits for the trigger");

    control(&mut app, move |vfx| {
        vfx.trigger_at(instance, "slam", TriggerArgs::at(Vec3::new(2.0, 0.0, 3.0)));
    });
    // The trigger frame already counts: age 0.3 after three frames.
    updates(&mut app, 3);
    assert!(shown(&app, ripple));
    let u = material(&app, ripple).uniforms;
    assert_eq!((u.params.y, u.params.z), (0.3, 1.0), "softness, rim");
    let s = state(&app, ripple);
    assert!((s.player.age - 0.3).abs() < 1e-4, "{}", s.player.age);
    assert!((s.scale.x - 1.6).abs() < 1e-3, "half grown: {}", s.scale);
    assert!((u.params.x - 0.01).abs() < 1e-4, "half the strength: {}", u.params.x);
    let global = app.world().get::<GlobalTransform>(ripple).unwrap();
    assert!((global.translation() - Vec3::new(2.0, 0.0, 3.0)).length() < 1e-4, "at the payload");
    assert!((global.scale() - Vec3::splat(1.6)).length() < 1e-3);

    updates(&mut app, 4);
    assert!(!shown(&app, ripple), "gone after its duration");
    control(&mut app, move |vfx| {
        vfx.trigger(instance, "slam");
    });
    updates(&mut app, 2);
    assert!(shown(&app, ripple), "plays again");
    assert_eq!(app.world().get::<VfxInstance>(instance).unwrap().state(), VfxState::Running, "OnTrigger: no auto-finish");
}

/// An `OnStart` one-shot finishes the instance by itself.
#[test]
fn one_shot_distortion_finishes_by_itself() {
    let mut app = app();
    let package = add_package(
        &mut app,
        &RIPPLE.replace("play: OnTrigger(\"slam\")", "play: OnStart").replace("triggers: [(name: \"slam\")],", ""),
    );
    let target = app.world_mut().spawn((Transform::default(), Visibility::default())).id();
    let instance = spawn_vfx(&mut app, &package, target);
    start(&mut app, instance);
    let [ripple] = layers(&app, instance)[..] else { panic!() };
    assert!(shown(&app, ripple));
    updates(&mut app, 8);
    assert!(app.world().get_entity(instance).is_err(), "finished after its 0.6 s");
    assert_eq!(app.world().resource::<Finished>().0.len(), 1);
}

const AURA: &str = r#"(
    parameters: [(name: "power", default: Float(1.0), min: 0.0, max: 1.0, smoothing: 0.0)],
    fit: Some(TargetScale),
    assets: [(id: "noise", kind: Texture((path: "omagari://textures/noise.png", purpose: Noise)))],
    layers: [
        Distortion((
            name: "aura",
            source: Surface(offset: 0.05),
            edge: Rim,
            strength: Param("power", [(0.0, 0.0), (1.0, 0.004)]),
            noise: (texture: (asset: "noise")),
            fade: (in: 0.0, out: 0.0),
        )),
    ],
)"#;

/// A surface source: one overlay per mesh of the target's own model, a
/// child of the mesh with the same mesh and the layer's material (pushed
/// out by the offset × fit); hidden at strength 0; gone with the layer.
#[test]
fn surface_source_overlays_the_target_meshes() {
    let mut app = app();
    let package = add_package(&mut app, AURA);
    let mesh = app.world_mut().resource_mut::<Assets<Mesh>>().add(Cuboid::new(0.5, 1.8, 0.3));
    let target = app.world_mut().spawn((Transform::from_scale(Vec3::splat(2.0)), Visibility::default())).id();
    let body = app.world_mut().spawn((Name::new("Body"), Mesh3d(mesh.clone()), Transform::default(), Visibility::default(), ChildOf(target))).id();
    let instance = spawn_vfx(&mut app, &package, target);
    start(&mut app, instance);
    let [aura] = layers(&app, instance)[..] else { panic!() };
    let overlays: Vec<(Entity, VfxDistortionOverlay)> = app
        .world_mut()
        .query::<(Entity, &VfxDistortionOverlay)>()
        .iter(app.world())
        .map(|(e, o)| (e, *o))
        .collect();
    let [(overlay, info)] = overlays[..] else { panic!("one overlay: {:?}", overlays) };
    assert_eq!((info.layer, info.source), (aura, body));
    assert_eq!(app.world().get::<ChildOf>(overlay).unwrap().parent(), body, "a child of the mesh");
    assert_eq!(app.world().get::<Mesh3d>(overlay).unwrap().0, mesh);
    assert!(shown(&app, overlay));
    let u = material(&app, overlay).uniforms;
    assert!((u.shape.x - 0.1).abs() < 1e-5, "offset × fit: {}", u.shape);
    assert_eq!(u.params.z, 1.0, "rim");
    assert!(app.world().get::<Mesh3d>(aura).is_none(), "the layer entity draws nothing itself");

    control(&mut app, move |vfx| {
        vfx.set_param(instance, "power", 0.0);
    });
    updates(&mut app, 1);
    assert!(!shown(&app, overlay), "strength 0: not drawn");

    app.world_mut().entity_mut(instance).despawn();
    updates(&mut app, 1);
    assert!(app.world().get_entity(overlay).is_err(), "overlays go with the layer");
    assert!(app.world().get_entity(body).is_ok(), "the model stays");
}

/// `replace_package` rebuilds the layer; a looping layer of the same name
/// keeps its fade level (no blink).
#[test]
fn rebuild_keeps_the_level() {
    let mut app = app();
    let ron = HAZE.replace("default: Float(0.0)", "default: Float(1.0)").replace("in: 0.0", "in: 1.0");
    let package = add_package(&mut app, &ron);
    let (target, _) = hand_target(&mut app);
    let instance = spawn_vfx(&mut app, &package, target);
    updates(&mut app, 12);
    let [old] = layers(&app, instance)[..] else { panic!() };
    assert_eq!(state(&app, old).player.level, 1.0);
    let other = add_package(&mut app, &ron.replace("scale: 2.5", "scale: 4.0"));
    control(&mut app, move |vfx| {
        vfx.replace_package(instance, other);
    });
    updates(&mut app, 2);
    let [new] = layers(&app, instance)[..] else { panic!() };
    assert_ne!(new, old);
    assert!(app.world().get_entity(old).is_err());
    assert!(state(&app, new).player.level > 0.99, "kept its level");
    assert_eq!(material(&app, new).uniforms.noise_a.z, 4.0);
}
