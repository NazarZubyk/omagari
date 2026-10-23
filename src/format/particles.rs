//! The particle effect model ([`EffectEditor`]) and `produce()`, which builds a
//! bevy_hanabi [`EffectAsset`] from it.

use bevy::prelude::*;
use bevy_hanabi::prelude::*;
use serde::Deserialize;
use serde::Serialize;

use super::modifiers::*;

#[derive(Serialize, Deserialize)]
pub enum ModifierEditor {
    SetAttribute(SetAttributeModifierEditor),
    InheritAttribute(InheritAttributeModifierEditor),
    SetPositionCircle(SetPositionCircleModifierEditor),
    SetPositionSphere(SetPositionSphereModifierEditor),
    SetVelocityCircle(SetVelocityCircleModifierEditor),
    SetVelocitySphere(SetVelocitySphereModifierEditor),
    SetVelocityTangent(SetVelocityTangentModifierEditor),
    AccelModifier(AccelModifierEditor),
    LinearDragModifier(LinearDragModifierEditor),
    EmitSpawnEventModifier(EmitSpawnEventModifierEditor),
    ConformToSphereModifier(ConformToSphereModifierEditor),
}

impl ModifierEditor {
    fn produce(&self, writer: &ExprWriter) -> ProducedModifier {
        match self {
            ModifierEditor::SetPositionCircle(n) => {
                ProducedModifier::SetPositionCircle(n.produce(writer))
            }
            ModifierEditor::SetPositionSphere(n) => {
                ProducedModifier::SetPositionSphere(n.produce(writer))
            }
            ModifierEditor::SetVelocityCircle(n) => {
                ProducedModifier::SetVelocityCircle(n.produce(writer))
            }
            ModifierEditor::SetVelocitySphere(n) => {
                ProducedModifier::SetVelocitySphere(n.produce(writer))
            }
            ModifierEditor::SetVelocityTangent(n) => {
                ProducedModifier::SetVelocityTangent(n.produce(writer))
            }
            ModifierEditor::SetAttribute(n) => ProducedModifier::SetAttribute(n.produce(writer)),
            ModifierEditor::InheritAttribute(n) => {
                ProducedModifier::InheritAttribute(n.produce(writer))
            }
            ModifierEditor::AccelModifier(n) => ProducedModifier::AccelModifier(n.produce(writer)),
            ModifierEditor::LinearDragModifier(n) => {
                ProducedModifier::LinearDragModifier(n.produce(writer))
            }
            ModifierEditor::EmitSpawnEventModifier(n) => {
                ProducedModifier::EmitSpawnEventModifier(n.produce(writer))
            }
            ModifierEditor::ConformToSphereModifier(n) => {
                ProducedModifier::ConformToSphere(n.produce(writer))
            }
        }
    }
}

#[derive(Serialize, Deserialize)]
pub enum RenderModifierEditor {
    SizeOverLifetime(SizeOverLifetimeModifierEditor),
    ColorOverLifetime(ColorOverLifetimeModifierEditor),
}

#[derive(Serialize, Deserialize)]
enum ProducedModifier {
    SetVelocityTangent(SetVelocityTangentModifier),
    SetPositionSphere(SetPositionSphereModifier),
    SetPositionCircle(SetPositionCircleModifier),
    SetAttribute(SetAttributeModifier),
    InheritAttribute(InheritAttributeModifier),
    SetVelocityCircle(SetVelocityCircleModifier),
    SetVelocitySphere(SetVelocitySphereModifier),
    AccelModifier(AccelModifier),
    LinearDragModifier(LinearDragModifier),
    EmitSpawnEventModifier(EmitSpawnEventModifier),
    ConformToSphere(ConformToSphereModifier),
}

#[derive(Serialize, Deserialize)]
pub struct EffectEditor {
    pub name: String,
    pub parent: Option<String>,
    pub capacity: u32,
    pub spawner_settings: SpawnerSettings,
    pub texture_index: Option<usize>,
    pub init_modifiers: Vec<ModifierEditor>,
    pub update_modifiers: Vec<ModifierEditor>,
    pub render_modifiers: Vec<RenderModifierEditor>,
}

impl EffectEditor {
    pub fn name(&self) -> &str {
        self.name.as_str()
    }
    pub fn parent(&self) -> Option<String> {
        self.parent.clone()
    }
    pub fn texture_index(&self) -> Option<usize> {
        self.texture_index
    }
    pub fn produce(&self) -> EffectAsset {
        let writer = ExprWriter::new();

        let mut init_modifiers: Vec<ProducedModifier> = Vec::new();
        let mut update_modifiers: Vec<ProducedModifier> = Vec::new();
        for m in self.init_modifiers.iter() {
            init_modifiers.push(m.produce(&writer));
        }
        for m in self.update_modifiers.iter() {
            update_modifiers.push(m.produce(&writer));
        }

        let texture_slot = writer.lit(0u32).expr();

        let mut module = writer.finish();
        module.add_texture_slot("color");

        let mut e = EffectAsset::new(self.capacity, self.spawner_settings, module)
            .with_alpha_mode(bevy_hanabi::AlphaMode::Blend)
            .with_name(&self.name);

        for modifier_wrapper in init_modifiers {
            match modifier_wrapper {
                ProducedModifier::SetVelocityTangent(modifier) => {
                    e = e.init(modifier);
                }
                ProducedModifier::SetPositionCircle(modifier) => {
                    e = e.init(modifier);
                }
                ProducedModifier::SetPositionSphere(modifier) => {
                    e = e.init(modifier);
                }
                ProducedModifier::SetAttribute(modifier) => {
                    e = e.init(modifier);
                }
                ProducedModifier::InheritAttribute(modifier) => {
                    e = e.init(modifier);
                }
                ProducedModifier::SetVelocityCircle(modifier) => {
                    e = e.init(modifier);
                }
                ProducedModifier::SetVelocitySphere(modifier) => {
                    e = e.init(modifier);
                }
                _ => {}
            }
        }

        for modifier_wrapper in update_modifiers {
            match modifier_wrapper {
                ProducedModifier::AccelModifier(modifier) => {
                    e = e.update(modifier);
                }
                ProducedModifier::LinearDragModifier(modifier) => {
                    e = e.update(modifier);
                }
                ProducedModifier::EmitSpawnEventModifier(modifier) => {
                    e = e.update(modifier);
                }
                ProducedModifier::ConformToSphere(modifier) => {
                    e = e.update(modifier);
                }
                _ => {}
            }
        }

        for editor_wrapper in self.render_modifiers.iter() {
            match editor_wrapper {
                RenderModifierEditor::SizeOverLifetime(editor) => e = e.render(editor.produce()),
                RenderModifierEditor::ColorOverLifetime(editor) => e = e.render(editor.produce()),
            }
        }

        e.render(ParticleTextureModifier {
            texture_slot,
            sample_mapping: ImageSampleMapping::ModulateOpacityFromR,
        })
        .render(OrientModifier::new(OrientMode::AlongVelocity))
    }
}

impl Default for EffectEditor {
    fn default() -> Self {
        EffectEditor {
            name: "Name your effect".to_string(),
            parent: None,
            capacity: 16384,
            spawner_settings: SpawnerSettings::rate(500.0.into()),
            texture_index: Some(0),
            init_modifiers: Vec::new(),
            update_modifiers: Vec::new(),
            render_modifiers: Vec::new(),
        }
    }
}

pub struct ParticleTexture {
    pub filename: &'static str,
    pub ui_label: &'static str,
}

pub const PARTICLE_TEXTURES: [ParticleTexture; 7] = [
    ParticleTexture {
        filename: "cloud.png",
        ui_label: "Cloud1",
    },
    ParticleTexture {
        filename: "cloud2.png",
        ui_label: "Cloud2",
    },
    ParticleTexture {
        filename: "spark1.png",
        ui_label: "Spark1",
    },
    ParticleTexture {
        filename: "spark2.png",
        ui_label: "Spark2",
    },
    ParticleTexture {
        filename: "spark3.png",
        ui_label: "Spark3",
    },
    ParticleTexture {
        filename: "glow1.png",
        ui_label: "Glow1",
    },
    ParticleTexture {
        filename: "splat1.png",
        ui_label: "Splat1",
    },
];
