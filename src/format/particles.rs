//! The particle effect model ([`EffectEditor`]) and `produce()`, which builds a
//! bevy_hanabi [`EffectAsset`] from it.

use bevy::prelude::*;
use bevy_hanabi::prelude::*;
use serde::Deserialize;
use serde::Serialize;

use super::hanabi_serde::{self, SpawnerDef};
use super::expr::{EffectWriter, ExprWriterEditor};
use super::modifiers::*;
use super::project::ParamDef;

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
    SetPositionCone3d(SetPositionCone3dModifierEditor),
    RadialAccel(RadialAccelModifierEditor),
    TangentAccel(TangentAccelModifierEditor),
    KillSphere(KillSphereModifierEditor),
    KillAabb(KillAabbModifierEditor),
}

impl ModifierEditor {
    fn produce(&self, writer: &EffectWriter) -> ProducedModifier {
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
            ModifierEditor::SetPositionCone3d(n) => ProducedModifier::SetPositionCone3d(n.produce(writer)),
            ModifierEditor::RadialAccel(n) => ProducedModifier::RadialAccel(n.produce(writer)),
            ModifierEditor::TangentAccel(n) => ProducedModifier::TangentAccel(n.produce(writer)),
            ModifierEditor::KillSphere(n) => ProducedModifier::KillSphere(n.produce(writer)),
            ModifierEditor::KillAabb(n) => ProducedModifier::KillAabb(n.produce(writer)),
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
    SetColor(SetColorModifierEditor),
    SetSize(SetSizeModifierEditor),
    Flipbook(FlipbookModifierEditor),
    Round(RoundModifierEditor),
    ScreenSpaceSize(ScreenSpaceSizeModifierEditor),
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
    SetPositionCone3d(SetPositionCone3dModifier),
    RadialAccel(RadialAccelModifier),
    TangentAccel(TangentAccelModifier),
    KillSphere(KillSphereModifier),
    KillAabb(KillAabbModifier),
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
        let writer = EffectWriter::new();

        let mut init_modifiers: Vec<ProducedModifier> = Vec::new();
        let mut update_modifiers: Vec<ProducedModifier> = Vec::new();
        for m in self.init_modifiers.iter() {
            init_modifiers.push(m.produce(&writer));
        }
        for m in self.update_modifiers.iter() {
            update_modifiers.push(m.produce(&writer));
        }

        let texture_slot = writer.lit(0u32).expr();

        let (mut module, _) = writer.finish();
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
                // v1 always adds texture sampling and orientation below; the
                // other render modifiers did not exist in v1.
                _ => {}
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
    pub fn produce_boxed(&self, writer: &EffectWriter) -> Box<dyn Modifier> {
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
            ProducedModifier::SetPositionCone3d(m) => Box::new(m),
            ProducedModifier::RadialAccel(m) => Box::new(m),
            ProducedModifier::TangentAccel(m) => Box::new(m),
            ProducedModifier::KillSphere(m) => Box::new(m),
            ProducedModifier::KillAabb(m) => Box::new(m),
        }
    }

    /// The modifier's expressions.
    pub fn exprs(&self) -> Vec<&ExprWriterEditor> {
        match self {
            ModifierEditor::SetAttribute(m) => vec![&m.attr_expr],
            ModifierEditor::InheritAttribute(_) => vec![],
            ModifierEditor::SetPositionCircle(m) => vec![&m.center_expr, &m.axis_expr, &m.radius_expr],
            ModifierEditor::SetPositionSphere(m) => vec![&m.center_expr, &m.radius_expr],
            ModifierEditor::SetVelocityCircle(m) => vec![&m.center_expr, &m.axis_expr, &m.speed_expr],
            ModifierEditor::SetVelocitySphere(m) => vec![&m.center_expr, &m.speed_expr],
            ModifierEditor::SetVelocityTangent(m) => vec![&m.origin_expr, &m.axis_expr, &m.speed_expr],
            ModifierEditor::AccelModifier(m) => vec![&m.accel_expr],
            ModifierEditor::LinearDragModifier(m) => vec![&m.drag_expr],
            ModifierEditor::EmitSpawnEventModifier(m) => vec![&m.count_expr],
            ModifierEditor::ConformToSphereModifier(m) => vec![
                &m.origin_expr,
                &m.radius_expr,
                &m.influence_dist_expr,
                &m.attraction_accel_expr,
                &m.max_attraction_speed_expr,
            ],
            ModifierEditor::SetPositionCone3d(m) => vec![&m.height_expr, &m.base_radius_expr, &m.top_radius_expr],
            ModifierEditor::RadialAccel(m) => vec![&m.origin_expr, &m.accel_expr],
            ModifierEditor::TangentAccel(m) => vec![&m.origin_expr, &m.axis_expr, &m.accel_expr],
            ModifierEditor::KillSphere(m) => vec![&m.center_expr, &m.radius_expr],
            ModifierEditor::KillAabb(m) => vec![&m.center_expr, &m.half_size_expr],
        }
    }

    /// The modifier's expressions, mutable.
    pub fn exprs_mut(&mut self) -> Vec<&mut ExprWriterEditor> {
        match self {
            ModifierEditor::SetAttribute(m) => vec![&mut m.attr_expr],
            ModifierEditor::InheritAttribute(_) => vec![],
            ModifierEditor::SetPositionCircle(m) => vec![&mut m.center_expr, &mut m.axis_expr, &mut m.radius_expr],
            ModifierEditor::SetPositionSphere(m) => vec![&mut m.center_expr, &mut m.radius_expr],
            ModifierEditor::SetVelocityCircle(m) => vec![&mut m.center_expr, &mut m.axis_expr, &mut m.speed_expr],
            ModifierEditor::SetVelocitySphere(m) => vec![&mut m.center_expr, &mut m.speed_expr],
            ModifierEditor::SetVelocityTangent(m) => vec![&mut m.origin_expr, &mut m.axis_expr, &mut m.speed_expr],
            ModifierEditor::AccelModifier(m) => vec![&mut m.accel_expr],
            ModifierEditor::LinearDragModifier(m) => vec![&mut m.drag_expr],
            ModifierEditor::EmitSpawnEventModifier(m) => vec![&mut m.count_expr],
            ModifierEditor::ConformToSphereModifier(m) => vec![
                &mut m.origin_expr,
                &mut m.radius_expr,
                &mut m.influence_dist_expr,
                &mut m.attraction_accel_expr,
                &mut m.max_attraction_speed_expr,
            ],
            ModifierEditor::SetPositionCone3d(m) => {
                vec![&mut m.height_expr, &mut m.base_radius_expr, &mut m.top_radius_expr]
            }
            ModifierEditor::RadialAccel(m) => vec![&mut m.origin_expr, &mut m.accel_expr],
            ModifierEditor::TangentAccel(m) => vec![&mut m.origin_expr, &mut m.axis_expr, &mut m.accel_expr],
            ModifierEditor::KillSphere(m) => vec![&mut m.center_expr, &mut m.radius_expr],
            ModifierEditor::KillAabb(m) => vec![&mut m.center_expr, &mut m.half_size_expr],
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
            ModifierEditor::SetPositionCone3d(_) => "SetPositionCone3d",
            ModifierEditor::RadialAccel(_) => "RadialAccel",
            ModifierEditor::TangentAccel(_) => "TangentAccel",
            ModifierEditor::KillSphere(_) => "KillSphere",
            ModifierEditor::KillAabb(_) => "KillAabb",
        }
    }

    /// The lists (init, update) the modifier may be in: bevy_hanabi's
    /// `Modifier::context()`, except `SetPositionCone3d`, which bevy_hanabi
    /// allows in update too although its code reads the spawner transform
    /// that only the init shader has.
    pub fn contexts(&self) -> ModifierContext {
        let (init, update) = (ModifierContext::Init, ModifierContext::Update);
        match self {
            ModifierEditor::SetAttribute(_)
            | ModifierEditor::SetPositionCircle(_)
            | ModifierEditor::SetPositionSphere(_)
            | ModifierEditor::SetVelocityCircle(_)
            | ModifierEditor::SetVelocitySphere(_)
            | ModifierEditor::SetVelocityTangent(_) => init | update,
            ModifierEditor::InheritAttribute(_) | ModifierEditor::SetPositionCone3d(_) => init,
            ModifierEditor::AccelModifier(_)
            | ModifierEditor::LinearDragModifier(_)
            | ModifierEditor::EmitSpawnEventModifier(_)
            | ModifierEditor::ConformToSphereModifier(_)
            | ModifierEditor::RadialAccel(_)
            | ModifierEditor::TangentAccel(_)
            | ModifierEditor::KillSphere(_)
            | ModifierEditor::KillAabb(_) => update,
        }
    }

    /// A default modifier of every kind, in menu order.
    pub fn all_defaults() -> Vec<ModifierEditor> {
        vec![
            ModifierEditor::SetAttribute(default()),
            ModifierEditor::InheritAttribute(default()),
            ModifierEditor::SetPositionCircle(default()),
            ModifierEditor::SetPositionSphere(default()),
            ModifierEditor::SetPositionCone3d(default()),
            ModifierEditor::SetVelocityCircle(default()),
            ModifierEditor::SetVelocitySphere(default()),
            ModifierEditor::SetVelocityTangent(default()),
            ModifierEditor::AccelModifier(default()),
            ModifierEditor::RadialAccel(default()),
            ModifierEditor::TangentAccel(default()),
            ModifierEditor::LinearDragModifier(default()),
            ModifierEditor::ConformToSphereModifier(default()),
            ModifierEditor::KillSphere(default()),
            ModifierEditor::KillAabb(default()),
            ModifierEditor::EmitSpawnEventModifier(default()),
        ]
    }

    /// Whether v1 `EffectEditor::produce()` used this modifier in the init
    /// list (`init == true`) or in the update list. v1 silently dropped the
    /// others (and every kind added after v1).
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
        let update_kind = matches!(
            self,
            ModifierEditor::AccelModifier(_)
                | ModifierEditor::LinearDragModifier(_)
                | ModifierEditor::EmitSpawnEventModifier(_)
                | ModifierEditor::ConformToSphereModifier(_)
        );
        if init { init_kind } else { update_kind }
    }
}

impl RenderModifierEditor {
    /// Name of the modifier kind, as written in project files.
    pub fn kind_name(&self) -> &'static str {
        match self {
            RenderModifierEditor::SizeOverLifetime(_) => "SizeOverLifetime",
            RenderModifierEditor::ColorOverLifetime(_) => "ColorOverLifetime",
            RenderModifierEditor::ParticleTexture(_) => "ParticleTexture",
            RenderModifierEditor::Orient(_) => "Orient",
            RenderModifierEditor::SetColor(_) => "SetColor",
            RenderModifierEditor::SetSize(_) => "SetSize",
            RenderModifierEditor::Flipbook(_) => "Flipbook",
            RenderModifierEditor::Round(_) => "Round",
            RenderModifierEditor::ScreenSpaceSize(_) => "ScreenSpaceSize",
        }
    }

    /// The modifier's expressions (evaluated when particles are drawn).
    pub fn exprs(&self) -> Vec<&ExprWriterEditor> {
        match self {
            RenderModifierEditor::Orient(m) => m.rotation_expr.iter().collect(),
            RenderModifierEditor::Round(m) => vec![&m.roundness_expr],
            _ => vec![],
        }
    }

    /// The modifier's expressions, mutable.
    pub fn exprs_mut(&mut self) -> Vec<&mut ExprWriterEditor> {
        match self {
            RenderModifierEditor::Orient(m) => m.rotation_expr.iter_mut().collect(),
            RenderModifierEditor::Round(m) => vec![&mut m.roundness_expr],
            _ => vec![],
        }
    }

    /// A default modifier of every kind, in menu order.
    pub fn all_defaults() -> Vec<RenderModifierEditor> {
        vec![
            RenderModifierEditor::ColorOverLifetime(default()),
            RenderModifierEditor::SetColor(default()),
            RenderModifierEditor::SizeOverLifetime(default()),
            RenderModifierEditor::SetSize(default()),
            RenderModifierEditor::ScreenSpaceSize(default()),
            RenderModifierEditor::ParticleTexture(default()),
            RenderModifierEditor::Flipbook(default()),
            RenderModifierEditor::Round(default()),
            RenderModifierEditor::Orient(default()),
        ]
    }

    /// Builds the bevy_hanabi render modifier. `texture_grid` is the layer
    /// texture's sprite-sheet grid (for Flipbook).
    pub fn produce_boxed(
        &self,
        writer: &EffectWriter,
        texture_grid: Option<(u32, u32)>,
        warnings: &mut Vec<String>,
    ) -> Box<dyn RenderModifier> {
        match self {
            RenderModifierEditor::SizeOverLifetime(m) => Box::new(m.produce()),
            RenderModifierEditor::ColorOverLifetime(m) => Box::new(m.produce()),
            RenderModifierEditor::ParticleTexture(m) => Box::new(m.produce(writer)),
            RenderModifierEditor::Orient(m) => Box::new(m.produce(writer)),
            RenderModifierEditor::SetColor(m) => Box::new(m.produce()),
            RenderModifierEditor::SetSize(m) => Box::new(m.produce()),
            RenderModifierEditor::Flipbook(m) => Box::new(m.produce(texture_grid, warnings)),
            RenderModifierEditor::Round(m) => Box::new(m.produce(writer)),
            RenderModifierEditor::ScreenSpaceSize(m) => Box::new(m.produce()),
        }
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

impl AlphaModeDef {
    pub fn to_hanabi(self, writer: &EffectWriter) -> bevy_hanabi::AlphaMode {
        match self {
            AlphaModeDef::Blend => bevy_hanabi::AlphaMode::Blend,
            AlphaModeDef::Premultiply => bevy_hanabi::AlphaMode::Premultiply,
            AlphaModeDef::Add => bevy_hanabi::AlphaMode::Add,
            AlphaModeDef::Multiply => bevy_hanabi::AlphaMode::Multiply,
            AlphaModeDef::Mask(cutoff) => bevy_hanabi::AlphaMode::Mask(writer.lit(cutoff).expr()),
            AlphaModeDef::Opaque => bevy_hanabi::AlphaMode::Opaque,
        }
    }
}

/// When particle positions follow their velocity (bevy_hanabi's
/// `MotionIntegration`).
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum MotionIntegrationDef {
    /// `position += velocity × dt` after the update modifiers (bevy_hanabi's
    /// default).
    #[default]
    PostUpdate,
    /// The same, before the update modifiers.
    PreUpdate,
    /// No automatic movement: positions only change when a modifier sets
    /// them (expression-driven paths, ribbons). Also silences bevy_hanabi's
    /// "missing VELOCITY" warning on layers that never move.
    None,
}

impl From<MotionIntegrationDef> for MotionIntegration {
    fn from(value: MotionIntegrationDef) -> Self {
        match value {
            MotionIntegrationDef::PostUpdate => MotionIntegration::PostUpdate,
            MotionIntegrationDef::PreUpdate => MotionIntegration::PreUpdate,
            MotionIntegrationDef::None => MotionIntegration::None,
        }
    }
}

/// When bevy_hanabi simulates a layer (its `SimulationCondition`).
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum SimulationConditionDef {
    /// Only while visible (bevy_hanabi's default; cheapest).
    #[default]
    WhenVisible,
    /// Also while hidden or off screen, so the effect is up to date when it
    /// comes back into view.
    Always,
}

impl From<SimulationConditionDef> for SimulationCondition {
    fn from(value: SimulationConditionDef) -> Self {
        match value {
            SimulationConditionDef::WhenVisible => SimulationCondition::WhenVisible,
            SimulationConditionDef::Always => SimulationCondition::Always,
        }
    }
}

fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}

/// The particle effect of a v2 particle layer: everything that becomes one
/// bevy_hanabi [`EffectAsset`].
///
/// Unlike v1 [`EffectEditor`], nothing is hard-coded: texture sampling and
/// orientation are ordinary render modifiers, blending is `alpha_mode`.
/// Settings added in P2b are only written when they differ from bevy_hanabi's
/// defaults.
#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct ParticleEffectDef {
    pub capacity: u32,
    pub spawner: SpawnerDef,
    pub alpha_mode: AlphaModeDef,
    #[serde(skip_serializing_if = "is_default")]
    pub motion_integration: MotionIntegrationDef,
    #[serde(skip_serializing_if = "is_default")]
    pub simulation_condition: SimulationConditionDef,
    /// Seed of the GPU random numbers: the same seed gives the same random
    /// values (reproducible looks); `None` = bevy_hanabi's default (0).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prng_seed: Option<u32>,
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
            motion_integration: MotionIntegrationDef::default(),
            simulation_condition: SimulationConditionDef::default(),
            prng_seed: None,
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
    ///
    /// `params` are the package's parameters: each one an expression reads
    /// (`Param(name)`) becomes a property, declared once.
    pub fn produce(
        &self,
        name: &str,
        simulation_space: SimulationSpace,
        params: &[ParamDef],
        warnings: &mut Vec<String>,
    ) -> EffectAsset {
        self.produce_with_grid(name, simulation_space, params, None, warnings)
    }

    /// [`ParticleEffectDef::produce`] with the sprite-sheet grid
    /// `(rows, columns)` of the layer's texture, used by a Flipbook render
    /// modifier without a grid of its own.
    pub fn produce_with_grid(
        &self,
        name: &str,
        simulation_space: SimulationSpace,
        params: &[ParamDef],
        texture_grid: Option<(u32, u32)>,
        warnings: &mut Vec<String>,
    ) -> EffectAsset {
        let writer = EffectWriter::with_params(params);

        let mut init_modifiers = Vec::new();
        for m in self.init_modifiers.iter() {
            init_modifiers.push((m.kind_name(), m.contexts(), m.produce_boxed(&writer)));
        }
        let mut update_modifiers = Vec::new();
        for m in self.update_modifiers.iter() {
            update_modifiers.push((m.kind_name(), m.contexts(), m.produce_boxed(&writer)));
        }

        let mut uses_texture = false;
        let mut render_warnings = Vec::new();
        let mut render_modifiers: Vec<Box<dyn RenderModifier>> = Vec::new();
        for m in self.render_modifiers.iter() {
            uses_texture |= matches!(m, RenderModifierEditor::ParticleTexture(_));
            render_modifiers.push(m.produce_boxed(&writer, texture_grid, &mut render_warnings));
        }
        let alpha_mode = self.alpha_mode.to_hanabi(&writer);

        let (mut module, writer_warnings) = writer.finish();
        for warning in writer_warnings.into_iter().chain(render_warnings) {
            warnings.push(format!("[{}] {}", name, warning));
        }
        if uses_texture {
            module.add_texture_slot(PARTICLE_TEXTURE_SLOT);
        }

        let mut e = EffectAsset::new(self.capacity, self.spawner.to_settings(), module)
            .with_alpha_mode(alpha_mode)
            .with_simulation_space(simulation_space)
            .with_motion_integration(self.motion_integration.into())
            .with_simulation_condition(self.simulation_condition.into())
            .with_name(name);
        if let Some(seed) = self.prng_seed {
            e.prng_seed = seed;
        }

        for (context, list, label) in [
            (ModifierContext::Init, init_modifiers, "init"),
            (ModifierContext::Update, update_modifiers, "update"),
        ] {
            for (kind, allowed, modifier) in list {
                if allowed.contains(context) && modifier.context().contains(context) {
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

    /// Names of the properties its expressions read (`Property("...")`),
    /// sorted, without duplicates.
    pub fn properties_read(&self) -> Vec<String> {
        self.names_read(|e| match e {
            ExprWriterEditor::Property(name) => Some(name),
            _ => None,
        })
    }

    /// Names of the parameters its expressions read (`Param("...")`),
    /// sorted, without duplicates.
    pub fn params_read(&self) -> Vec<String> {
        self.names_read(|e| match e {
            ExprWriterEditor::Param(name) => Some(name),
            _ => None,
        })
    }

    fn names_read(&self, pick: impl Fn(&ExprWriterEditor) -> Option<&String>) -> Vec<String> {
        let mut names = Vec::new();
        for e in self.exprs() {
            e.visit(&mut |e| {
                if let Some(name) = pick(e) {
                    names.push(name.clone());
                }
            });
        }
        names.sort();
        names.dedup();
        names
    }

    /// Every expression of its init, update and render modifiers.
    pub fn exprs(&self) -> Vec<&ExprWriterEditor> {
        self.init_modifiers
            .iter()
            .chain(self.update_modifiers.iter())
            .flat_map(|m| m.exprs())
            .chain(self.render_modifiers.iter().flat_map(|m| m.exprs()))
            .collect()
    }

    /// Every expression of its init, update and render modifiers, mutable.
    pub fn exprs_mut(&mut self) -> Vec<&mut ExprWriterEditor> {
        self.init_modifiers
            .iter_mut()
            .chain(self.update_modifiers.iter_mut())
            .flat_map(|m| m.exprs_mut())
            .chain(self.render_modifiers.iter_mut().flat_map(|m| m.exprs_mut()))
            .collect()
    }

    /// Seconds from start until the spawner has emitted its last particle,
    /// or `None` when it repeats forever (`cycle_count: 0`). Random values
    /// count with their maximum.
    pub fn emit_duration(&self) -> Option<f32> {
        let s = &self.spawner;
        if s.cycle_count == 0 {
            return None;
        }
        let duration = s.spawn_duration.range()[1].max(0.0);
        // A cycle lasts `period`, or `spawn_duration` when that is longer.
        let cycle = s.period.range()[1].max(duration);
        Some((s.cycle_count - 1) as f32 * cycle + duration)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// `ModifierEditor::contexts` is bevy_hanabi's `Modifier::context()`,
    /// except the documented SetPositionCone3d exception.
    #[test]
    fn contexts_follow_hanabi() {
        for m in ModifierEditor::all_defaults() {
            let writer = EffectWriter::new();
            let hanabi = m.produce_boxed(&writer).context();
            match m {
                ModifierEditor::SetPositionCone3d(_) => {
                    assert_eq!(hanabi, ModifierContext::Init | ModifierContext::Update);
                    assert_eq!(m.contexts(), ModifierContext::Init);
                }
                _ => assert_eq!(m.contexts(), hanabi, "{}", m.kind_name()),
            }
        }
    }
}
