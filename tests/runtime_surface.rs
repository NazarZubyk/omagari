//! Surface layers, headless (`MinimalPlugins` + transform propagation, no
//! rendering): overlays over the target's own meshes, shells, the own-model
//! rule, material uniforms from parameters, fade in / out and the stop.

use std::time::Duration;

use bevy::asset::AssetPlugin;
use bevy::ecs::system::RunSystemOnce;
use bevy::gltf::{GltfMaterialName, GltfMeshName};
use bevy::image::ImagePlugin;
use bevy::mesh::skinning::SkinnedMesh;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use bevy::transform::TransformPlugin;
use bevy::world_serialization::WorldAssetRoot;
use bevy_hanabi::prelude::*;
use omagari::format::ProjectV2;
use omagari::runtime::{
    OmagariEffectMaterial, SurfaceLayerState, TargetHierarchy, VfxState, VfxSurfaceOverlay, DEPTH_PUSH,
};
use omagari::{OmagariPlugin, VfxCommandsExt, VfxControl, VfxInstance, VfxPackage};

const DT: f32 = 0.1;

/// Two surface layers: "skin" covers every own mesh with two shells and
/// fades in over 0.5 s / out over 1 s; "eyes" covers the material "Glass"
/// with one shell. The opacity follows the parameter "power".
const PROJECT: &str = r#"(
    parameters: [(name: "power", default: Float(0.5))],
    layers: [
        Surface((
            name: "skin",
            material: (
                blend: Add,
                color: Const((1.0, 0.0, 1.0, 1.0)),
                opacity: Param("power", [(0.0, 0.0), (1.0, 1.0)]),
            ),
            shells: [(offset: 0.0), (offset: 0.02, opacity: 0.5)],
            fade: (in: 0.5, out: 1.0),
        )),
        Surface((name: "eyes", targets: Materials(["Glass"]), fade: (in: 0.0, out: 0.2))),
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

struct Model {
    target: Entity,
    body: Entity,
    eyes: Entity,
    blade: Entity,
    mesh: Handle<Mesh>,
}

/// target → glTF scene → "Body.Skin" (skinned) and "Eyes.Glass"; a sword
/// (its own glTF scene) in the hand.
fn spawn_model(app: &mut App) -> Model {
    let mesh = app.world_mut().resource_mut::<Assets<Mesh>>().add(Cuboid::default());
    let world = app.world_mut();
    let target = world.spawn(Transform::from_xyz(3.0, 0.0, 0.0)).id();
    let scene = world.spawn((WorldAssetRoot::default(), Transform::default(), ChildOf(target))).id();
    let hand = world.spawn((Name::new("Hand"), Transform::from_xyz(0.5, 1.0, 0.0), ChildOf(scene))).id();
    let body = world
        .spawn((
            Name::new("Body.Skin"),
            GltfMeshName("Body".into()),
            GltfMaterialName("Skin".into()),
            Mesh3d(mesh.clone()),
            SkinnedMesh { inverse_bindposes: Handle::default(), joints: vec![hand] },
            Transform::default(),
            ChildOf(scene),
        ))
        .id();
    let eyes = world
        .spawn((
            Name::new("Eyes.Glass"),
            GltfMeshName("Eyes".into()),
            GltfMaterialName("Glass".into()),
            Mesh3d(mesh.clone()),
            Transform::from_xyz(0.0, 1.7, 0.1),
            ChildOf(scene),
        ))
        .id();
    let sword = world.spawn((WorldAssetRoot::default(), Transform::default(), ChildOf(hand))).id();
    let blade = world
        .spawn((Name::new("Blade.Metal"), Mesh3d(mesh.clone()), Transform::default(), ChildOf(sword)))
        .id();
    Model { target, body, eyes, blade, mesh }
}

/// (source mesh, shell, layer name) of every overlay.
fn overlays(app: &mut App) -> Vec<(Entity, usize, String)> {
    let mut found: Vec<(Entity, usize, String)> = app
        .world_mut()
        .run_system_once(|q: Query<&VfxSurfaceOverlay>, names: Query<&Name>| {
            q.iter()
                .map(|o| (o.source, o.shell, names.get(o.layer).map(|n| n.to_string()).unwrap_or_default()))
                .collect::<Vec<_>>()
        })
        .unwrap();
    found.sort();
    found
}

fn surface_states(app: &App, instance: Entity) -> Vec<SurfaceLayerState> {
    let surfaces = app.world().get::<VfxInstance>(instance).unwrap().surface_layers().to_vec();
    surfaces.iter().map(|&e| app.world().get::<SurfaceLayerState>(e).unwrap().clone()).collect()
}

fn material(app: &App, state: &SurfaceLayerState, shell: usize) -> OmagariEffectMaterial {
    let materials = app.world().resource::<Assets<OmagariEffectMaterial>>();
    materials.get(&state.materials[shell]).unwrap().clone()
}

#[test]
fn overlays_cover_matching_own_meshes_per_shell() {
    let mut app = app();
    let package = add_package(&mut app, PROJECT);
    let model = spawn_model(&mut app);
    let instance = spawn_vfx(&mut app, &package, model.target);
    for _ in 0..3 {
        app.update();
    }

    // "skin": 2 own meshes × 2 shells; "eyes": the Glass mesh × 1 shell. The
    // sword in the hand is a separate model and never covered.
    let found = overlays(&mut app);
    let mut expected = vec![
        (model.body, 0, "skin".to_string()),
        (model.body, 1, "skin".to_string()),
        (model.eyes, 0, "eyes".to_string()),
        (model.eyes, 0, "skin".to_string()),
        (model.eyes, 1, "skin".to_string()),
    ];
    expected.sort();
    assert_eq!(found, expected);
    assert!(!found.iter().any(|(s, ..)| *s == model.blade));

    // Each overlay: a child of its mesh at the identity transform, same
    // mesh, same skin (same joints → animates with the body).
    let world = app.world_mut();
    let mut q = world.query::<(&VfxSurfaceOverlay, &ChildOf, &Transform, &Mesh3d, Option<&SkinnedMesh>, &GlobalTransform)>();
    for (overlay, parent, transform, mesh, skin, global) in q.iter(world) {
        assert_eq!(parent.parent(), overlay.source);
        assert_eq!(*transform, Transform::IDENTITY);
        assert_eq!(mesh.0, model.mesh);
        let source_global = world.get::<GlobalTransform>(overlay.source).unwrap();
        assert_eq!(global, source_global, "same place as the mesh it covers");
        assert_eq!(skin.is_some(), overlay.source == model.body);
        if let Some(skin) = skin {
            assert_eq!(skin.joints, world.get::<SkinnedMesh>(model.body).unwrap().joints);
        }
    }

    // Overlays are not part of the target's own meshes (fit, other layers).
    let target = model.target;
    let mut own = app.world_mut().run_system_once(move |h: TargetHierarchy| h.own_meshes(target)).unwrap();
    own.sort();
    let mut meshes = vec![model.body, model.eyes];
    meshes.sort();
    assert_eq!(own, meshes);

    // One material per shell, sorted inner to outer.
    let states = surface_states(&app, instance);
    assert_eq!(states[0].materials.len(), 2);
    assert_eq!(states[1].materials.len(), 1);
    assert!(material(&app, &states[0], 1).sort_bias > material(&app, &states[0], 0).sort_bias);
    assert_eq!(material(&app, &states[0], 0).alpha_mode, bevy::prelude::AlphaMode::Add);
    assert_eq!(material(&app, &states[1], 0).alpha_mode, bevy::prelude::AlphaMode::Blend);
}

#[test]
fn overlays_follow_meshes_that_appear_and_go() {
    let mut app = app();
    let package = add_package(&mut app, PROJECT);
    let target = app.world_mut().spawn(Transform::default()).id();
    let instance = spawn_vfx(&mut app, &package, target);
    for _ in 0..3 {
        app.update();
    }
    assert!(overlays(&mut app).is_empty(), "no model yet");
    assert_eq!(app.world().get::<VfxInstance>(instance).unwrap().state(), VfxState::Running);

    // The glTF scene finishes loading later.
    let mesh = app.world_mut().resource_mut::<Assets<Mesh>>().add(Sphere::default());
    let cape = app
        .world_mut()
        .spawn((Name::new("Cape.Cloth"), Mesh3d(mesh.clone()), Transform::default(), ChildOf(target)))
        .id();
    app.update();
    app.update();
    assert_eq!(overlays(&mut app).len(), 2, "two shells of \"skin\" (the cape has no Glass)");

    // A swapped mesh asset reaches the overlays.
    let other = app.world_mut().resource_mut::<Assets<Mesh>>().add(Cuboid::default());
    app.world_mut().entity_mut(cape).insert(Mesh3d(other.clone()));
    app.update();
    let world = app.world_mut();
    let mut q = world.query::<(&VfxSurfaceOverlay, &Mesh3d)>();
    assert!(q.iter(world).all(|(_, m)| m.0 == other));

    // The mesh goes (body swapped): its overlays go with it.
    app.world_mut().entity_mut(cape).despawn();
    app.update();
    app.update();
    assert!(overlays(&mut app).is_empty());
    assert!(surface_states(&app, instance)[0].overlay_entities.is_empty());

    // Despawning the instance removes overlays of a mesh that stays.
    let keep = app
        .world_mut()
        .spawn((Name::new("Hat.Felt"), Mesh3d(mesh), Transform::default(), ChildOf(target)))
        .id();
    app.update();
    app.update();
    assert_eq!(overlays(&mut app).len(), 2);
    app.world_mut().entity_mut(instance).despawn();
    app.update();
    assert!(overlays(&mut app).is_empty());
    assert!(app.world().get_entity(keep).is_ok(), "the model is never touched");
}

#[test]
fn uniforms_follow_parameters_and_fades_and_the_stop_waits() {
    let mut app = app();
    let package = add_package(&mut app, PROJECT);
    let model = spawn_model(&mut app);
    let instance = spawn_vfx(&mut app, &package, model.target);
    app.update();
    app.update();
    // Fading in over 0.5 s (0.1 s per frame).
    let fade = |app: &App, shell: usize| {
        let state = &surface_states(app, instance)[0];
        material(app, state, shell).uniforms.params.w
    };
    let first = fade(&app, 0);
    assert!(first > 0.0 && first < 0.5, "{}", first);
    for _ in 0..6 {
        app.update();
    }
    assert_eq!(fade(&app, 0), 1.0);
    assert_eq!(fade(&app, 1), 0.5, "shell opacity");
    let state = &surface_states(&app, instance)[0];
    let u = material(&app, state, 1).uniforms;
    assert_eq!(u.params.x, 0.5, "opacity = power 0.5 on the curve");
    assert!((u.shape.x - 0.02).abs() < 1e-6, "shell offset (fit scale 1)");
    assert!((u.shape.z - DEPTH_PUSH).abs() < 1e-6);

    // A parameter change reaches the uniforms.
    app.world_mut()
        .run_system_once(move |mut vfx: VfxControl| {
            vfx.set_param_now(instance, "power", 1.0);
        })
        .unwrap();
    app.update();
    let state = &surface_states(&app, instance)[0];
    assert_eq!(material(&app, state, 0).uniforms.params.x, 1.0);

    // Stop: fades out over the longest fade out (1 s), then finishes and
    // the overlays are gone.
    app.world_mut()
        .run_system_once(move |mut vfx: VfxControl| {
            vfx.stop(instance);
        })
        .unwrap();
    app.update();
    app.update();
    let level = surface_states(&app, instance)[0].level;
    assert!(level > 0.0 && level < 1.0, "fading out: {}", level);
    for _ in 0..6 {
        app.update();
    }
    assert!(app.world().get_entity(instance).is_ok(), "still fading at 0.8 s");
    for _ in 0..4 {
        app.update();
    }
    assert!(app.world().get_entity(instance).is_err(), "finished after the fade out");
    assert!(overlays(&mut app).is_empty());
}

#[test]
fn a_replaced_package_rebuilds_overlays_and_keeps_the_fade() {
    let mut app = app();
    let package = add_package(&mut app, PROJECT);
    let model = spawn_model(&mut app);
    let instance = spawn_vfx(&mut app, &package, model.target);
    for _ in 0..8 {
        app.update();
    }
    assert_eq!(overlays(&mut app).len(), 5);
    let one_shell = add_package(&mut app, &PROJECT.replace("shells: [(offset: 0.0), (offset: 0.02, opacity: 0.5)]", "shells: [(offset: 0.01)]"));
    app.world_mut()
        .run_system_once(move |mut vfx: VfxControl| {
            assert!(vfx.replace_package(instance, one_shell.clone()));
        })
        .unwrap();
    app.update();
    app.update();
    app.update();
    let found = overlays(&mut app);
    assert_eq!(found.len(), 3, "skin: 2 meshes × 1 shell, eyes: 1: {:?}", found);
    let states = surface_states(&app, instance);
    assert_eq!(states[0].level, 1.0, "a rebuilt layer does not fade in again");
}
