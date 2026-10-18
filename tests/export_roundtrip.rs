//! Checks that the `*.hanabi.ron` export format survives a save/load round trip.

use bevy::prelude::*;
use bevy_hanabi::prelude::*;
use bevy_hanabi::register_modifiers;
use omagari::{ExportedEffect, ExportedProject};

/// Builds an effect shaped like the ones the editor produces.
fn sample_effect(name: &str) -> EffectAsset {
    let writer = ExprWriter::new();
    let init_size = SetAttributeModifier::new(Attribute::SIZE, writer.lit(0.2).expr());
    let init_pos = SetPositionSphereModifier {
        center: writer.lit(Vec3::ZERO).expr(),
        radius: writer.lit(1.0).expr(),
        dimension: ShapeDimension::Surface,
    };
    let accel = AccelModifier::new(writer.lit(Vec3::new(0.0, -3.0, 0.0)).expr());
    let texture_slot = writer.lit(0u32).expr();
    let mut module = writer.finish();
    module.add_texture_slot("color");

    let mut gradient = bevy_hanabi::Gradient::new();
    gradient.add_key(0.0, Vec4::ONE);
    gradient.add_key(1.0, Vec4::ZERO);

    EffectAsset::new(256, SpawnerSettings::rate(50.0.into()), module)
        .with_alpha_mode(bevy_hanabi::AlphaMode::Blend)
        .with_name(name)
        .init(init_size)
        .init(init_pos)
        .update(accel)
        .render(ColorOverLifetimeModifier::new(gradient))
        .render(ParticleTextureModifier {
            texture_slot,
            sample_mapping: ImageSampleMapping::ModulateOpacityFromR,
        })
        .render(OrientModifier::new(OrientMode::AlongVelocity))
}

#[test]
fn exported_project_round_trips() {
    let app_registry = AppTypeRegistry::new_with_derived_types();
    register_modifiers(&app_registry);
    let registry = app_registry.read();

    let project = ExportedProject {
        effects: vec![
            ExportedEffect {
                name: "rocket".into(),
                parent: None,
                texture_index: Some(2),
                effect_asset: sample_effect("rocket"),
            },
            ExportedEffect {
                name: "sparks".into(),
                parent: Some("rocket".into()),
                texture_index: None,
                effect_asset: sample_effect("sparks"),
            },
        ],
    };

    let saved = project.to_ron_string(&registry).unwrap();
    let loaded = ExportedProject::from_ron_bytes(saved.as_bytes(), &registry).unwrap();

    assert_eq!(loaded.effects.len(), 2);
    assert_eq!(loaded.effects[1].name, "sparks");
    assert_eq!(loaded.effects[1].parent.as_deref(), Some("rocket"));
    assert_eq!(loaded.effects[0].texture_index, Some(2));
    assert_eq!(loaded.effects[1].texture_index, None);
    assert_eq!(loaded.effects[0].effect_asset.init_modifiers().count(), 2);
    assert_eq!(loaded.effects[0].effect_asset.render_modifiers().count(), 3);

    // Saving the loaded project again must give the exact same file.
    assert_eq!(loaded.to_ron_string(&registry).unwrap(), saved);
}
