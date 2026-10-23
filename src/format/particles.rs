//! The particle effect model ([`EffectEditor`]) and `produce()`, which builds a
//! bevy_hanabi [`EffectAsset`] from it.

use bevy::prelude::*;
use bevy_hanabi::prelude::*;
use serde::Deserialize;
use serde::Serialize;

use super::hanabi_serde::{self, SpawnerDef};
use super::modifiers::*;

#[derive(Serialize, Deserialize, Clone)]
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

#[derive(Serialize, Deserialize, Clone)]
pub enum RenderModifierEditor {
    SizeOverLifetime(SizeOverLifetimeModifierEditor),
    ColorOverLifetime(ColorOverLifetimeModifierEditor),
    /// Explicit in v2 projects; v1 `produce()` always adds it.
    ParticleTexture(ParticleTextureModifierEditor),
    /// Explicit in v2 projects; v1 `produce()` always adds it.
    Orient(OrientModifierEditor),
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

/// One particle effect of a v1 project (`OmagariProject`).
#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct EffectEditor {
    pub name: String,
    // Missing `Option`s are `None`, as before `#[serde(default)]`.
    #[serde(default)]
    pub parent: Option<String>,
    pub capacity: u32,
    #[serde(with = "hanabi_serde::spawner_settings")]
    pub spawner_settings: SpawnerSettings,
    #[serde(default)]
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
                // v1 always adds texture sampling and orientation below.
                RenderModifierEditor::ParticleTexture(_) | RenderModifierEditor::Orient(_) => {}
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

impl ModifierEditor {
    /// Builds the bevy_hanabi modifier, boxed so it can be added to any
    /// context that supports it.
    pub fn produce_boxed(&self, writer: &ExprWriter) -> Box<dyn Modifier> {
        match self.produce(writer) {
            ProducedModifier::SetVelocityTangent(m) => Box::new(m),
            ProducedModifier::SetPositionSphere(m) => Box::new(m),
            ProducedModifier::SetPositionCircle(m) => Box::new(m),
            ProducedModifier::SetAttribute(m) => Box::new(m),
            ProducedModifier::InheritAttribute(m) => Box::new(m),
            ProducedModifier::SetVelocityCircle(m) => Box::new(m),
            ProducedModifier::SetVelocitySphere(m) => Box::new(m),
            ProducedModifier::AccelModifier(m) => Box::new(m),
            ProducedModifier::LinearDragModifier(m) => Box::new(m),
            ProducedModifier::EmitSpawnEventModifier(m) => Box::new(m),
            ProducedModifier::ConformToSphere(m) => Box::new(m),
        }
    }

    /// Name of the modifier kind, as written in project files.
    pub fn kind_name(&self) -> &'static str {
        match self {
            ModifierEditor::SetAttribute(_) => "SetAttribute",
            ModifierEditor::InheritAttribute(_) => "InheritAttribute",
            ModifierEditor::SetPositionCircle(_) => "SetPositionCircle",
            ModifierEditor::SetPositionSphere(_) => "SetPositionSphere",
            ModifierEditor::SetVelocityCircle(_) => "SetVelocityCircle",
            ModifierEditor::SetVelocitySphere(_) => "SetVelocitySphere",
            ModifierEditor::SetVelocityTangent(_) => "SetVelocityTangent",
            ModifierEditor::AccelModifier(_) => "AccelModifier",
            ModifierEditor::LinearDragModifier(_) => "LinearDragModifier",
            ModifierEditor::EmitSpawnEventModifier(_) => "EmitSpawnEventModifier",
            ModifierEditor::ConformToSphereModifier(_) => "ConformToSphereModifier",
        }
    }

    /// Whether v1 `EffectEditor::produce()` used this modifier in the init
    /// list (`init == true`) or in the update list. v1 silently dropped the
    /// others.
    pub(crate) fn v1_accepts(&self, init: bool) -> bool {
        let init_kind = matches!(
            self,
            ModifierEditor::SetVelocityTangent(_)
                | ModifierEditor::SetPositionCircle(_)
                | ModifierEditor::SetPositionSphere(_)
                | ModifierEditor::SetAttribute(_)
                | ModifierEditor::InheritAttribute(_)
                | ModifierEditor::SetVelocityCircle(_)
                | ModifierEditor::SetVelocitySphere(_)
        );
        init_kind == init
    }
}

/// Blending of a particle layer with what is behind it (bevy_hanabi's
/// `AlphaMode`).
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug, Default)]
pub enum AlphaModeDef {
    /// Regular alpha blending (the v1 look).
    #[default]
    Blend,
    Premultiply,
    /// Additive glow.
    Add,
    Multiply,
    /// Opaque cut-out: fragments with alpha below the cutoff are discarded.
    Mask(f32),
    Opaque,
}

/// The particle effect of a v2 particle layer: everything that becomes one
/// bevy_hanabi [`EffectAsset`].
///
/// Unlike v1 [`EffectEditor`], nothing is hard-coded: texture sampling and
/// orientation are ordinary render modifiers, blending is `alpha_mode`.
#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct ParticleEffectDef {
    pub capacity: u32,
    pub spawner: SpawnerDef,
    pub alpha_mode: AlphaModeDef,
    pub init_modifiers: Vec<ModifierEditor>,
    pub update_modifiers: Vec<ModifierEditor>,
    pub render_modifiers: Vec<RenderModifierEditor>,
}

impl Default for ParticleEffectDef {
    /// Same as a new v1 editor effect, with its formerly hard-coded render
    /// settings made explicit.
    fn default() -> Self {
        ParticleEffectDef {
            capacity: 16384,
            spawner: SpawnerDef::default(),
            alpha_mode: AlphaModeDef::Blend,
            init_modifiers: Vec::new(),
            update_modifiers: Vec::new(),
            render_modifiers: vec![
                RenderModifierEditor::ParticleTexture(ParticleTextureModifierEditor::default()),
                RenderModifierEditor::Orient(OrientModifierEditor::default()),
            ],
        }
    }
}

/// Texture slot used by [`ParticleTextureModifierEditor`]; the layer's texture
/// is bound to it.
pub const PARTICLE_TEXTURE_SLOT: &str = "color";

impl ParticleEffectDef {
    /// Builds the bevy_hanabi effect. `warnings` receives one line per value
    /// that could not be used (e.g. a modifier in a list that doesn't support
    /// it); such values are skipped.
    pub fn produce(
        &self,
        name: &str,
        simulation_space: SimulationSpace,
        warnings: &mut Vec<String>,
    ) -> EffectAsset {
        let writer = ExprWriter::new();

        let mut init_modifiers = Vec::new();
        for m in self.init_modifiers.iter() {
            init_modifiers.push((m.kind_name(), m.produce_boxed(&writer)));
        }
        let mut update_modifiers = Vec::new();
        for m in self.update_modifiers.iter() {
            update_modifiers.push((m.kind_name(), m.produce_boxed(&writer)));
        }

        let mut uses_texture = false;
        let mut render_modifiers: Vec<Box<dyn RenderModifier>> = Vec::new();
        for m in self.render_modifiers.iter() {
            render_modifiers.push(match m {
                RenderModifierEditor::SizeOverLifetime(m) => Box::new(m.produce()),
                RenderModifierEditor::ColorOverLifetime(m) => Box::new(m.produce()),
                RenderModifierEditor::ParticleTexture(m) => {
                    uses_texture = true;
                    Box::new(m.produce(&writer))
                }
                RenderModifierEditor::Orient(m) => Box::new(m.produce()),
            });
        }

        let alpha_mode = match self.alpha_mode {
            AlphaModeDef::Blend => bevy_hanabi::AlphaMode::Blend,
            AlphaModeDef::Premultiply => bevy_hanabi::AlphaMode::Premultiply,
            AlphaModeDef::Add => bevy_hanabi::AlphaMode::Add,
            AlphaModeDef::Multiply => bevy_hanabi::AlphaMode::Multiply,
            AlphaModeDef::Mask(cutoff) => bevy_hanabi::AlphaMode::Mask(writer.lit(cutoff).expr()),
            AlphaModeDef::Opaque => bevy_hanabi::AlphaMode::Opaque,
        };

        let mut module = writer.finish();
        if uses_texture {
            module.add_texture_slot(PARTICLE_TEXTURE_SLOT);
        }

        let mut e = EffectAsset::new(self.capacity, self.spawner.to_settings(), module)
            .with_alpha_mode(alpha_mode)
            .with_simulation_space(simulation_space)
            .with_name(name);

        for (context, list, label) in [
            (ModifierContext::Init, init_modifiers, "init"),
            (ModifierContext::Update, update_modifiers, "update"),
        ] {
            for (kind, modifier) in list {
                if modifier.context().contains(context) {
                    e = e.add_modifier(context, modifier);
                } else {
                    warnings.push(format!(
                        "[{}] {} cannot be used in the {} list; it is ignored",
                        name, kind, label
                    ));
                }
            }
        }
        for modifier in render_modifiers {
            e = e.add_render_modifier(modifier);
        }
        e
    }

    /// Longest time a particle of this effect can live, in seconds, estimated
    /// from the init `lifetime` attribute. `None` when it cannot be known
    /// without simulating (e.g. it reads other attributes).
    pub fn max_lifetime(&self) -> Option<f32> {
        // bevy_hanabi's default lifetime.
        let mut lifetime = Some(1.0);
        for m in self.init_modifiers.iter() {
            if let ModifierEditor::SetAttribute(m) = m {
                if m.attr == Attribute::LIFETIME {
                    lifetime = m.attr_expr.scalar_range().map(|(_, max)| max.max(0.0));
                }
            }
        }
        lifetime
    }
}
