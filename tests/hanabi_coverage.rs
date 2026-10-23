//! bevy_hanabi coverage (plan F9): every hanabi modifier and operator is
//! exposed by the format or listed as skipped with a reason, and every
//! modifier and setting survives save → load through the runtime loader →
//! produce, and generates its shaders.
//!
//! Modifiers are enumerated from bevy_hanabi's type registry after its
//! `register_modifiers()` (the list hanabi itself uses to (de)serialize
//! effects), plus `register_missing_hanabi_modifiers()`, which adds the one
//! hanabi 0.19 forgets (`EmitSpawnEventModifier`). That is the most robust
//! list available at test time: it needs no source parsing, and a new
//! registered modifier fails this test after a hanabi upgrade. (A modifier
//! hanabi forgets to register would not show up; there is no other runtime
//! list.) Operators are covered at compile time: `omagari::format::coverage`
//! matches every variant of hanabi's operator enums without a `_` arm.

use std::any::TypeId;
use std::collections::BTreeSet;
use std::time::Duration;

use bevy::asset::AssetPlugin;
use bevy::image::{CompressedImageFormats, ImageLoader, ImagePlugin};
use bevy::prelude::*;
use bevy_hanabi::graph::expr::TernaryOperator;
use bevy_hanabi::prelude::*;
use omagari::format::coverage;
use omagari::format::*;
use omagari::{register_missing_hanabi_modifiers, OmagariPlugin, VfxPackage};

fn type_registry() -> AppTypeRegistry {
    let registry = AppTypeRegistry::default();
    bevy_hanabi::register_modifiers(&registry);
    register_missing_hanabi_modifiers(&registry);
    registry
}

/// Short type name of a reflected value (`KillSphereModifier`).
fn short_name(path: &str) -> &str {
    path.rsplit("::").next().unwrap_or(path)
}

/// The type of the bevy_hanabi modifier every Omagari modifier kind
/// produces.
fn produced_types() -> Vec<(String, TypeId, String)> {
    let mut out = Vec::new();
    for m in ModifierEditor::all_defaults() {
        let writer = EffectWriter::new();
        let produced = m.produce_boxed(&writer);
        let reflect = produced.as_reflect();
        out.push((m.kind_name().to_string(), reflect.type_id(), short_name(reflect.reflect_type_path()).to_string()));
    }
    for m in RenderModifierEditor::all_defaults() {
        let writer = EffectWriter::new();
        let produced = m.produce_boxed(&writer, Some((2, 2)), &mut Vec::new());
        let reflect = produced.as_modifier().as_reflect();
        out.push((m.kind_name().to_string(), reflect.type_id(), short_name(reflect.reflect_type_path()).to_string()));
    }
    out
}

#[test]
fn every_hanabi_modifier_is_mapped_or_skipped() {
    let registry = type_registry();
    let registry = registry.read();
    let registered: Vec<(TypeId, String)> = registry
        .iter_with_data::<ReflectModifier>()
        .map(|(registration, _)| (registration.type_id(), short_name(registration.type_info().type_path()).to_string()))
        .collect();
    assert!(registered.len() >= 25, "only {} modifiers registered: {:?}", registered.len(), registered);

    let produced = produced_types();
    let skipped: BTreeSet<&str> = coverage::SKIPPED_MODIFIERS.iter().map(|(name, _)| *name).collect();
    let mut missing = Vec::new();
    for (type_id, name) in registered.iter() {
        if !produced.iter().any(|(_, t, _)| t == type_id) && !skipped.contains(name.as_str()) {
            missing.push(name.clone());
        }
    }
    assert!(
        missing.is_empty(),
        "bevy_hanabi modifiers without an Omagari mapping or a skip entry (omagari::format::coverage::SKIPPED_MODIFIERS): {:?}",
        missing
    );
    for name in skipped {
        assert!(registered.iter().any(|(_, n)| n == name), "stale skip entry {}", name);
        assert!(!produced.iter().any(|(_, _, n)| n == name), "{} is skipped but also mapped", name);
    }
    // Every Omagari kind produces a registered (serializable) modifier.
    for (kind, type_id, name) in produced.iter() {
        assert!(registered.iter().any(|(t, _)| t == type_id), "{} produces unregistered {}", kind, name);
    }
}

/// Builds `row` with placeholder arguments and returns the expression it
/// writes on top.
fn produced_expr(op: ExprOperatorEditor) -> Expr {
    let writer = EffectWriter::new();
    let handle = op.produce(&writer).expr();
    let (module, _) = writer.finish();
    *module.get(handle).expect("expression")
}

fn operator_row(name: &str) -> &'static OperatorInfo {
    OPERATORS.iter().find(|o| o.name == name).unwrap_or_else(|| panic!("no operator row {}", name))
}

#[test]
fn every_hanabi_operator_is_mapped() {
    for op in coverage::UNARY_OPERATORS {
        let row = operator_row(coverage::unary_operator(op));
        match produced_expr((row.make)()) {
            Expr::Unary { op: produced, .. } => assert_eq!(produced, op, "{}", row.name),
            other => panic!("{} produces {:?}", row.name, other),
        }
    }
    for op in coverage::BINARY_OPERATORS {
        let row = operator_row(coverage::binary_operator(op));
        match produced_expr((row.make)()) {
            Expr::Binary { op: produced, .. } => assert_eq!(produced, op, "{}", row.name),
            other => panic!("{} produces {:?}", row.name, other),
        }
    }
    for op in coverage::TERNARY_OPERATORS {
        let row = operator_row(coverage::ternary_operator(op));
        match produced_expr((row.make)()) {
            Expr::Ternary { op: produced, .. } => assert_eq!(produced, op, "{}", row.name),
            other => panic!("{} produces {:?}", row.name, other),
        }
    }
    let _: fn(TernaryOperator) -> &'static str = coverage::ternary_operator;
    for op in coverage::BUILTIN_OPERATORS {
        let name = coverage::builtin_operator(op);
        let value = EXPR_VALUES.iter().find(|v| v.name == name).unwrap_or_else(|| panic!("no value row {}", name));
        let writer = EffectWriter::new();
        let handle = (value.make)().produce(&writer).expr();
        let (module, _) = writer.finish();
        match module.get(handle) {
            Some(Expr::BuiltIn(b)) => assert_eq!(
                std::mem::discriminant(&b.operator),
                std::mem::discriminant(&op),
                "{}",
                name
            ),
            other => panic!("{} produces {:?}", name, other),
        }
    }
    // Every row has a working constructor and a matching name.
    for row in OPERATORS.iter() {
        assert_eq!((row.make)().name(), row.name);
        assert_eq!((row.make)().args().len(), row.args.len(), "{}", row.name);
        assert!(OPERATOR_GROUPS.contains(&row.group), "{} group {}", row.name, row.group);
    }
    for value in EXPR_VALUES.iter() {
        assert_eq!((value.make)().leaf_name(), Some(value.name));
    }
    // Every attribute and every float value type.
    for attr in Attribute::all() {
        assert!(ALL_ATTRS.iter().any(|(a, _)| a == attr), "attribute {} has no editor label", attr.name());
    }
    for t in ValueTypeDef::ALL {
        let writer = EffectWriter::new();
        let handle = ExprWriterEditor::Rand(t).produce(&writer).expr();
        let (module, _) = writer.finish();
        assert_eq!(module.get(handle).unwrap().value_type(), Some(t.to_hanabi()), "{}", t.name());
    }
    // Small enums convert both ways.
    for m in [MotionIntegration::None, MotionIntegration::PreUpdate, MotionIntegration::PostUpdate] {
        assert_eq!(MotionIntegration::from(coverage::motion_integration(m)), m);
    }
    for c in [SimulationCondition::WhenVisible, SimulationCondition::Always] {
        assert_eq!(SimulationCondition::from(coverage::simulation_condition(c)), c);
    }
    for m in [OrientMode::ParallelCameraDepthPlane, OrientMode::FaceCameraPosition, OrientMode::AlongVelocity] {
        assert_eq!(OrientMode::from(coverage::orient_mode(m)), m);
    }
    for m in [ImageSampleMapping::Modulate, ImageSampleMapping::ModulateRGB, ImageSampleMapping::ModulateOpacityFromR] {
        assert_eq!(ImageSampleMapping::from(coverage::image_sample_mapping(m)), m);
    }
    let writer = EffectWriter::new();
    let modes = [
        AlphaModeDef::Blend,
        AlphaModeDef::Premultiply,
        AlphaModeDef::Add,
        AlphaModeDef::Multiply,
        AlphaModeDef::Mask(0.5),
        AlphaModeDef::Opaque,
    ];
    assert_eq!(modes.len(), coverage::ALPHA_MODES.len());
    for (mode, name) in modes.iter().zip(coverage::ALPHA_MODES) {
        assert!(format!("{:?}", mode.to_hanabi(&writer)).starts_with(name));
    }
}

// --- round trip ------------------------------------------------------------------

/// One layer per modifier kind (in a list it may be in), the asset settings,
/// a parent / child pair for the spawn-event modifiers, and every render
/// modifier on one layer.
fn coverage_project() -> ProjectV2 {
    let mut project = ProjectV2::default();
    let mut sheet = TextureAssetDef::new("omagari://textures/glow1.png", TexturePurpose::Sprite);
    sheet.grid = Some((2, 2));
    project.assets.push(AssetDef { id: "sheet".into(), kind: AssetKind::Texture(sheet) });
    let base = || {
        let mut effect = ParticleEffectDef::default();
        effect.capacity = 64;
        effect.init_modifiers.push(ModifierEditor::SetAttribute(SetAttributeModifierEditor {
            attr: Attribute::LIFETIME,
            attr_expr: ExprWriterEditor::Float(1.0),
        }));
        effect
    };
    let mut layer = |name: &str, effect: ParticleEffectDef| {
        project.layers.push(LayerDef::Particles(ParticlesLayerDef {
            name: name.into(),
            texture: Some("sheet".into()),
            effect,
            ..default()
        }));
    };
    for m in ModifierEditor::all_defaults() {
        if matches!(m, ModifierEditor::InheritAttribute(_) | ModifierEditor::EmitSpawnEventModifier(_)) {
            continue;
        }
        // The default SetAttribute (id = 0.0) is a placeholder to fill in.
        let m = match m {
            ModifierEditor::SetAttribute(_) => ModifierEditor::SetAttribute(SetAttributeModifierEditor {
                attr: Attribute::F32_0,
                attr_expr: ExprWriterEditor::Float(1.0),
            }),
            m => m,
        };
        let mut effect = base();
        if m.contexts().contains(ModifierContext::Init) {
            effect.init_modifiers.push(m.clone());
        }
        if m.contexts().contains(ModifierContext::Update) {
            effect.update_modifiers.push(m.clone());
        }
        layer(&format!("mod_{}", m.kind_name()), effect);
    }
    let mut render = base();
    render.render_modifiers = RenderModifierEditor::all_defaults();
    for m in render.render_modifiers.iter_mut() {
        if let RenderModifierEditor::Orient(o) = m {
            o.mode = OrientModeDef::FaceCameraPosition;
            o.rotation_expr = Some(ExprWriterEditor::Operator(Box::new(ExprOperatorEditor::Multiply(
                ExprWriterEditor::Age,
                ExprWriterEditor::Float(2.0),
            ))));
        }
    }
    render.update_modifiers.push(ModifierEditor::SetAttribute(sprite_index_over_lifetime(4)));
    layer("render", render);
    for (i, (motion, condition, seed, alpha)) in [
        (MotionIntegrationDef::None, SimulationConditionDef::Always, Some(7), AlphaModeDef::Add),
        (MotionIntegrationDef::PreUpdate, SimulationConditionDef::WhenVisible, None, AlphaModeDef::Mask(0.4)),
        (MotionIntegrationDef::PostUpdate, SimulationConditionDef::WhenVisible, Some(0), AlphaModeDef::Premultiply),
    ]
    .into_iter()
    .enumerate()
    {
        let mut effect = base();
        effect.motion_integration = motion;
        effect.simulation_condition = condition;
        effect.prng_seed = seed;
        effect.alpha_mode = alpha;
        effect.spawner = effect.spawner.with_preset([SpawnerPreset::Once, SpawnerPreset::Rate, SpawnerPreset::Burst][i]);
        layer(&format!("settings_{}", i), effect);
    }
    let mut parent = base();
    parent.update_modifiers.push(ModifierEditor::EmitSpawnEventModifier(EmitSpawnEventModifierEditor {
        count_expr: ExprWriterEditor::U32(2),
        ..default()
    }));
    layer("parent", parent);
    let mut child = base();
    child.init_modifiers.push(ModifierEditor::InheritAttribute(InheritAttributeModifierEditor { attr: Attribute::POSITION }));
    project.layers.push(LayerDef::Particles(ParticlesLayerDef {
        name: "child".into(),
        parent: Some("parent".into()),
        effect: child,
        ..default()
    }));
    project
}

/// bevy_hanabi's own reflection serialization of an effect, to compare two.
fn effect_text(effect: &EffectAsset, registry: &AppTypeRegistry) -> String {
    let registry = registry.read();
    ron::ser::to_string(&EffectAssetSerializer::new(effect, &registry)).unwrap()
}

#[test]
fn every_modifier_and_setting_round_trips_through_the_loader() {
    let project = coverage_project();
    let text = project.to_ron_string().unwrap();
    let reloaded = ProjectV2::from_ron_str(&text).unwrap();
    assert_eq!(reloaded.to_ron_string().unwrap(), text, "text round trip");

    // Every layer builds without warnings and generates its shaders.
    let layers: Vec<&ParticlesLayerDef> = project.particle_layers().map(|(_, l)| l).collect();
    let mut built = Vec::new();
    for layer in layers.iter() {
        let mut warnings = Vec::new();
        let effect = layer.produce_in(&project, &mut warnings);
        assert!(warnings.is_empty(), "{}: {:?}", layer.name, warnings);
        built.push(effect);
    }
    for (i, layer) in layers.iter().enumerate() {
        let parent = layer.parent.as_ref().map(|p| {
            let pi = layers.iter().position(|l| &l.name == p).unwrap();
            built[pi].particle_layout()
        });
        let children = layers.iter().filter(|l| l.parent.as_deref() == Some(layer.name.as_str())).count() as u32;
        if let Err(e) = EffectShaderSources::generate(&built[i], parent.as_ref(), children) {
            panic!("{}: shader generation failed: {}", layer.name, e);
        }
    }

    // Save, load through the runtime's package loader, compare.
    let dir = std::env::temp_dir().join(format!("omagari_hanabi_coverage_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("coverage.vfx.ron"), project.to_package().to_ron_string().unwrap()).unwrap();
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin { file_path: dir.to_string_lossy().to_string(), ..default() },
        ImagePlugin::default(),
    ))
    .register_asset_loader(ImageLoader::new(CompressedImageFormats::empty()))
    .init_asset::<EffectAsset>()
    .add_plugins(OmagariPlugin);
    let package: Handle<VfxPackage> = app.world().resource::<AssetServer>().load("coverage.vfx.ron");
    for _ in 0..500 {
        app.update();
        let server = app.world().resource::<AssetServer>();
        if server.is_loaded_with_dependencies(&package) || server.load_state(&package).is_failed() {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let server = app.world().resource::<AssetServer>();
    assert!(server.is_loaded_with_dependencies(&package), "{:?}", server.recursive_dependency_load_state(&package));
    let registry = type_registry();
    let packages = app.world().resource::<Assets<VfxPackage>>();
    let effects = app.world().resource::<Assets<EffectAsset>>();
    let package = packages.get(&package).unwrap();
    assert_eq!(package.particle_layers.len(), layers.len());
    for (prepared, expected) in package.particle_layers.iter().zip(built.iter()) {
        let loaded = effects.get(&prepared.effect).unwrap();
        assert_eq!(effect_text(loaded, &registry), effect_text(expected, &registry), "{}", prepared.name);
    }
    // The settings reached the asset.
    let settings = effects.get(&package.particle_layers.iter().find(|l| l.name == "settings_0").unwrap().effect).unwrap();
    assert_eq!(settings.motion_integration, MotionIntegration::None);
    assert_eq!(settings.simulation_condition, SimulationCondition::Always);
    assert_eq!(settings.prng_seed, 7);
    assert!(settings.spawner.is_once());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn hanabi_contexts_are_enforced() {
    // SetPositionCone3d reads the spawner transform, which only the init
    // shader has: Omagari refuses it in the update list.
    let mut effect = ParticleEffectDef::default();
    effect.update_modifiers.push(ModifierEditor::SetPositionCone3d(default()));
    effect.init_modifiers.push(ModifierEditor::KillSphere(default()));
    let mut warnings = Vec::new();
    let asset = effect.produce("a", SimulationSpace::Local, &[], &mut warnings);
    assert_eq!(warnings.len(), 2, "{:?}", warnings);
    assert!(warnings[0].contains("KillSphere cannot be used in the init list"), "{:?}", warnings);
    assert!(warnings[1].contains("SetPositionCone3d cannot be used in the update list"), "{:?}", warnings);
    assert_eq!(asset.init_modifiers().count() + asset.update_modifiers().count(), 0);

    // A Flipbook takes the texture's grid as (columns, rows).
    let flipbook = FlipbookModifierEditor::default();
    assert_eq!(flipbook.produce(Some((2, 4)), &mut Vec::new()).sprite_grid_size, UVec2::new(4, 2));
    let mut warnings = Vec::new();
    assert_eq!(flipbook.produce(None, &mut warnings).sprite_grid_size, UVec2::ONE);
    assert_eq!(warnings.len(), 1);
    // Spawner presets keep a random count.
    let mut spawner = SpawnerDef::default();
    spawner.count = CpuValueDef::Uniform((3.0, 5.0));
    let burst = spawner.with_preset(SpawnerPreset::Burst);
    assert_eq!(burst.count, CpuValueDef::Uniform((3.0, 5.0)));
    assert_eq!((burst.cycle_count, burst.spawn_duration), (0, CpuValueDef::Single(0.0)));
    assert_eq!(burst.preset(), Some(SpawnerPreset::Burst));
    assert_eq!(SpawnerDef::default().preset(), Some(SpawnerPreset::Rate));
}

/// The Position preset "random point on segment with radius" builds for a
/// Segment layer: it reads both runtime segment properties and bevy_hanabi
/// generates its shaders.
#[test]
fn segment_with_radius_preset_generates_shaders() {
    let mut layer = ParticlesLayerDef {
        name: "blade".into(),
        attach: AttachDef::Segment { from: "base".into(), to: "tip".into() },
        ..default()
    };
    layer.effect.init_modifiers.push(ModifierEditor::SetAttribute(segment_point_with_radius(0.05)));
    let mut project = ProjectV2::default();
    project.layers.push(LayerDef::Particles(layer.clone()));
    let mut warnings = Vec::new();
    let effect = layer.produce_in(&project, &mut warnings);
    assert!(warnings.is_empty(), "{:?}", warnings);
    let reads = layer.effect.properties_read();
    assert!(reads.iter().any(|p| p == "segment_a") && reads.iter().any(|p| p == "segment_b"), "{:?}", reads);
    let properties: Vec<&str> = effect.properties().iter().map(|p| p.name()).collect();
    assert!(properties.contains(&"segment_a") && properties.contains(&"segment_b"), "{:?}", properties);
    if let Err(e) = EffectShaderSources::generate(&effect, None, 0) {
        panic!("shader generation failed: {}", e);
    }
}
