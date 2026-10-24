//! Mesh particles and the draw order of particle layers, headless
//! (`MinimalPlugins`, no rendering): every built-in particle mesh reaches
//! bevy_hanabi as an `EffectMesh`, a mesh asset is loaded with the package
//! (layers wait for it; a mesh without UVs gets zero UVs), `sort_bias`
//! becomes a `ParticleSortBias`, and mesh assets are loaded by their glTF
//! label (`path#MeshI/PrimitiveJ`).

use std::time::Duration;

use bevy::asset::AssetPlugin;
use bevy::ecs::system::RunSystemOnce;
use bevy::image::ImagePlugin;
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use bevy::transform::TransformPlugin;
use bevy_hanabi::prelude::*;
use omagari::format::{MeshAssetDef, ParticleShapeDef, ProjectV2, TextureAssetDef};
use omagari::runtime::{particle_shape_vertex_count, PackageAssets, ParticleSortBias, PreparedMesh};
use omagari::{OmagariPlugin, VfxCommandsExt, VfxInstance, VfxPackage};

fn app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, TransformPlugin, AssetPlugin::default(), ImagePlugin::default()))
        .init_asset::<EffectAsset>()
        .init_asset::<Mesh>()
        .add_plugins(OmagariPlugin)
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(0.1)));
    app
}

/// Builds packages whose mesh assets are one mesh already in `Assets<Mesh>`
/// (recording the asset paths it was asked for).
struct FixedMesh<'a> {
    effects: &'a mut Assets<EffectAsset>,
    mesh: Handle<Mesh>,
    asked: Vec<String>,
}

impl PackageAssets for FixedMesh<'_> {
    fn add_effect(&mut self, _label: String, effect: EffectAsset) -> Handle<EffectAsset> {
        self.effects.add(effect)
    }

    fn load_texture(&mut self, _texture: &TextureAssetDef) -> Handle<Image> {
        Handle::default()
    }

    fn load_mesh(&mut self, mesh: &MeshAssetDef) -> Option<Handle<Mesh>> {
        self.asked.push(mesh.asset_path());
        Some(self.mesh.clone())
    }
}

fn build(app: &mut App, ron: &str, mesh: Handle<Mesh>) -> (Handle<VfxPackage>, Vec<String>) {
    let project = ProjectV2::from_ron_str(ron).unwrap();
    let world = app.world_mut();
    let (package, asked) = world.resource_scope(|_, mut effects: Mut<Assets<EffectAsset>>| {
        let mut assets = FixedMesh { effects: &mut effects, mesh, asked: Vec::new() };
        let mut warnings = Vec::new();
        let package = VfxPackage::build(project, &mut assets, &mut warnings);
        assert!(warnings.is_empty(), "{:?}", warnings);
        (package, assets.asked)
    });
    (world.resource_mut::<Assets<VfxPackage>>().add(package), asked)
}

fn spawn(app: &mut App, package: &Handle<VfxPackage>) -> Entity {
    let target = app.world_mut().spawn(Transform::default()).id();
    let package = package.clone();
    app.world_mut()
        .run_system_once(move |mut commands: Commands| commands.spawn_vfx(&package, target))
        .unwrap()
}

fn layers(app: &App, instance: Entity) -> Vec<Entity> {
    app.world().get::<VfxInstance>(instance).unwrap().layers().to_vec()
}

#[test]
fn every_built_in_mesh_reaches_bevy_hanabi() {
    let mut app = app();
    let mut ron = String::from("(layers: [");
    for shape in ParticleShapeDef::ALL {
        ron += &format!("Particles((name: \"{0:?}\", mesh: Some(Shape({0:?})), effect: (alpha_mode: Opaque))),", shape);
    }
    ron += "Particles((name: \"quad\")), Particles((name: \"on_top\", sort_bias: 0.75))])";
    let (package, asked) = build(&mut app, &ron, Handle::default());
    assert!(asked.is_empty());
    let instance = spawn(&mut app, &package);
    for _ in 0..3 {
        app.update();
    }
    let list = layers(&app, instance);
    assert_eq!(list.len(), ParticleShapeDef::ALL.len() + 2);
    for (shape, &layer) in ParticleShapeDef::ALL.iter().zip(list.iter()) {
        let EffectMesh(handle) = app.world().get::<EffectMesh>(layer).unwrap_or_else(|| panic!("{:?} has no mesh", shape)).clone();
        let mesh = app.world().resource::<Assets<Mesh>>().get(&handle).unwrap();
        assert_eq!(mesh.count_vertices(), particle_shape_vertex_count(*shape), "{:?}", shape);
        for attribute in [Mesh::ATTRIBUTE_POSITION, Mesh::ATTRIBUTE_UV_0, Mesh::ATTRIBUTE_NORMAL] {
            assert!(mesh.contains_attribute(attribute), "{:?} lacks {:?}", shape, attribute.name);
        }
        assert!(app.world().get::<ParticleSortBias>(layer).is_none());
    }
    let (quad, on_top) = (list[ParticleShapeDef::ALL.len()], list[ParticleShapeDef::ALL.len() + 1]);
    assert!(app.world().get::<EffectMesh>(quad).is_none(), "the default quad needs no mesh");
    assert_eq!(app.world().get::<ParticleSortBias>(on_top), Some(&ParticleSortBias(0.75)));

    // A second instance shares the built-in meshes.
    let again = spawn(&mut app, &package);
    for _ in 0..3 {
        app.update();
    }
    let first = app.world().get::<EffectMesh>(list[0]).unwrap().0.clone();
    let second = app.world().get::<EffectMesh>(layers(&app, again)[0]).unwrap().0.clone();
    assert_eq!(first, second);
}

#[test]
fn mesh_assets_load_with_the_package_and_get_uvs() {
    let mut app = app();
    // A triangle with positions only (like a glTF mesh without UVs).
    let bare = Mesh::new(PrimitiveTopology::TriangleList, Default::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]])
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0f32, 0.0, 1.0]; 3])
        .with_inserted_indices(Indices::U32(vec![0, 1, 2]));
    let handle = app.world_mut().resource_mut::<Assets<Mesh>>().add(bare);
    let (package, asked) = build(
        &mut app,
        r#"(
            assets: [(id: "rock", kind: Mesh((path: "models/debris.glb", mesh: 2, primitive: 1)))],
            layers: [
                Particles((name: "a", mesh: Some(Asset("rock")), effect: (alpha_mode: Opaque))),
                Particles((name: "b", mesh: Some(Asset("rock")), effect: (alpha_mode: Opaque))),
            ],
        )"#,
        handle.clone(),
    );
    assert_eq!(asked, vec!["models/debris.glb#Mesh2/Primitive1".to_string()], "loaded once, by its glTF label");
    {
        let packages = app.world().resource::<Assets<VfxPackage>>();
        let prepared = &packages.get(&package).unwrap().particle_layers;
        assert_eq!(prepared[0].mesh, Some(PreparedMesh::Loaded(handle.clone())));
        assert_eq!(packages.get(&package).unwrap().meshes().count(), 2);
    }
    let instance = spawn(&mut app, &package);
    for _ in 0..3 {
        app.update();
    }
    for layer in layers(&app, instance) {
        assert_eq!(app.world().get::<EffectMesh>(layer).unwrap().0, handle);
    }
    let meshes = app.world().resource::<Assets<Mesh>>();
    match meshes.get(&handle).unwrap().attribute(Mesh::ATTRIBUTE_UV_0) {
        Some(VertexAttributeValues::Float32x2(uvs)) => assert_eq!(uvs.len(), 3),
        other => panic!("no UVs added: {:?}", other.map(|v| v.len())),
    }
}

/// A missing mesh asset is a build warning and the layer draws quads.
#[test]
fn missing_mesh_assets_draw_quads() {
    let mut app = app();
    let project = ProjectV2::from_ron_str(r#"(layers: [Particles((name: "a", mesh: Some(Asset("nope"))))])"#).unwrap();
    let world = app.world_mut();
    let warnings = world.resource_scope(|_, mut effects: Mut<Assets<EffectAsset>>| {
        let mut assets = FixedMesh { effects: &mut effects, mesh: Handle::default(), asked: Vec::new() };
        let mut warnings = Vec::new();
        let package = VfxPackage::build(project, &mut assets, &mut warnings);
        assert_eq!(package.particle_layers[0].mesh, None);
        warnings
    });
    assert!(warnings[0].contains("mesh \"nope\" is not a mesh in the assets list"), "{:?}", warnings);
}
