//! Particle modifiers of the effect model and how each one becomes a
//! bevy_hanabi modifier.

use bevy::prelude::*;
use bevy_hanabi::prelude::*;
use serde::Deserialize;
use serde::Serialize;

use super::expr::*;

pub trait ModifierProducer<T>
where
    T: bevy_hanabi::Modifier,
{
    fn produce(&self, writer: &bevy_hanabi::ExprWriter) -> T;
}

pub trait RenderModifierProducer<T>
where
    T: bevy_hanabi::Modifier,
{
    type Output;
    fn produce(&self) -> T;
}

#[derive(Serialize, Deserialize)]
pub struct SetAttributeModifierEditor {
    pub attr: Attribute,
    pub attr_expr: ExprWriterEditor,
}

impl ModifierProducer<SetAttributeModifier> for SetAttributeModifierEditor {
    fn produce(&self, writer: &ExprWriter) -> SetAttributeModifier {
        SetAttributeModifier {
            attribute: self.attr,
            value: self.attr_expr.produce(writer).expr(),
        }
    }
}

impl Default for SetAttributeModifierEditor {
    fn default() -> Self {
        Self {
            attr: Attribute::ID,
            attr_expr: ExprWriterEditor::Float(0.0),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct InheritAttributeModifierEditor {
    pub attr: Attribute,
}

impl ModifierProducer<InheritAttributeModifier> for InheritAttributeModifierEditor {
    fn produce(&self, _writer: &ExprWriter) -> InheritAttributeModifier {
        InheritAttributeModifier {
            attribute: self.attr,
        }
    }
}

impl Default for InheritAttributeModifierEditor {
    fn default() -> Self {
        Self {
            attr: Attribute::ID,
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct SetVelocityCircleModifierEditor {
    pub center_expr: ExprWriterEditor,
    pub axis_expr: ExprWriterEditor,
    pub speed_expr: ExprWriterEditor,
}

impl ModifierProducer<SetVelocityCircleModifier> for SetVelocityCircleModifierEditor {
    fn produce(&self, writer: &ExprWriter) -> SetVelocityCircleModifier {
        SetVelocityCircleModifier {
            center: self.center_expr.produce(writer).expr(),
            axis: self.axis_expr.produce(writer).expr(),
            speed: self.speed_expr.produce(writer).expr(),
        }
    }
}

impl Default for SetVelocityCircleModifierEditor {
    fn default() -> Self {
        Self {
            center_expr: ExprWriterEditor::Vec3(Vec3::ZERO),
            axis_expr: ExprWriterEditor::Vec3(Vec3::Y),
            speed_expr: ExprWriterEditor::Float(0.5),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct SetVelocitySphereModifierEditor {
    pub center_expr: ExprWriterEditor,
    pub speed_expr: ExprWriterEditor,
}

impl ModifierProducer<SetVelocitySphereModifier> for SetVelocitySphereModifierEditor {
    fn produce(&self, writer: &ExprWriter) -> SetVelocitySphereModifier {
        SetVelocitySphereModifier {
            center: self.center_expr.produce(writer).expr(),
            speed: self.speed_expr.produce(writer).expr(),
        }
    }
}

impl Default for SetVelocitySphereModifierEditor {
    fn default() -> Self {
        Self {
            center_expr: ExprWriterEditor::Vec3(Vec3::ZERO),
            speed_expr: ExprWriterEditor::Float(0.5),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct SetVelocityTangentModifierEditor {
    pub origin_expr: ExprWriterEditor,
    pub axis_expr: ExprWriterEditor,
    pub speed_expr: ExprWriterEditor,
}

impl ModifierProducer<SetVelocityTangentModifier> for SetVelocityTangentModifierEditor {
    fn produce(&self, writer: &ExprWriter) -> SetVelocityTangentModifier {
        SetVelocityTangentModifier {
            origin: self.origin_expr.produce(writer).expr(),
            axis: self.axis_expr.produce(writer).expr(),
            speed: self.speed_expr.produce(writer).expr(),
        }
    }
}

impl Default for SetVelocityTangentModifierEditor {
    fn default() -> Self {
        Self {
            origin_expr: ExprWriterEditor::Vec3(Vec3::ZERO),
            axis_expr: ExprWriterEditor::Vec3(Vec3::Y),
            speed_expr: ExprWriterEditor::Operator(Box::new(ExprOperatorEditor::Uniform(
                ExprWriterEditor::Float(0.2),
                ExprWriterEditor::Float(1.0),
            ))),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct SetPositionCircleModifierEditor {
    pub center_expr: ExprWriterEditor,
    pub axis_expr: ExprWriterEditor,
    pub radius_expr: ExprWriterEditor,
    pub dimension: ShapeDimension,
}

impl ModifierProducer<SetPositionCircleModifier> for SetPositionCircleModifierEditor {
    fn produce(&self, writer: &ExprWriter) -> SetPositionCircleModifier {
        SetPositionCircleModifier {
            center: self.center_expr.produce(writer).expr(),
            axis: self.axis_expr.produce(writer).expr(),
            radius: self.radius_expr.produce(writer).expr(),
            dimension: self.dimension,
        }
    }
}

impl Default for SetPositionCircleModifierEditor {
    fn default() -> Self {
        Self {
            center_expr: ExprWriterEditor::Vec3(Vec3::ZERO),
            axis_expr: ExprWriterEditor::Vec3(Vec3::Y),
            radius_expr: ExprWriterEditor::Float(0.2),
            dimension: ShapeDimension::Surface,
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct SetPositionSphereModifierEditor {
    pub center_expr: ExprWriterEditor,
    pub radius_expr: ExprWriterEditor,
    pub dimension: ShapeDimension,
}

impl ModifierProducer<SetPositionSphereModifier> for SetPositionSphereModifierEditor {
    fn produce(&self, writer: &ExprWriter) -> SetPositionSphereModifier {
        SetPositionSphereModifier {
            center: self.center_expr.produce(writer).expr(),
            radius: self.radius_expr.produce(writer).expr(),
            dimension: self.dimension,
        }
    }
}

impl Default for SetPositionSphereModifierEditor {
    fn default() -> Self {
        Self {
            center_expr: ExprWriterEditor::Vec3(Vec3::ZERO),
            radius_expr: ExprWriterEditor::Float(0.2),
            dimension: ShapeDimension::Surface,
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct AccelModifierEditor {
    pub accel_expr: ExprWriterEditor,
}

impl ModifierProducer<AccelModifier> for AccelModifierEditor {
    fn produce(&self, writer: &ExprWriter) -> AccelModifier {
        AccelModifier::new(self.accel_expr.produce(writer).expr())
    }
}

impl Default for AccelModifierEditor {
    fn default() -> Self {
        Self {
            accel_expr: ExprWriterEditor::Operator(Box::new(ExprOperatorEditor::Subtract(
                ExprWriterEditor::Operator(Box::new(ExprOperatorEditor::Multiply(
                    ExprWriterEditor::RandVec3,
                    ExprWriterEditor::Vec3(Vec3::splat(2.0)),
                ))),
                ExprWriterEditor::Vec3(Vec3::ONE),
            ))),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct LinearDragModifierEditor {
    pub drag_expr: ExprWriterEditor,
}

impl ModifierProducer<LinearDragModifier> for LinearDragModifierEditor {
    fn produce(&self, writer: &ExprWriter) -> LinearDragModifier {
        LinearDragModifier {
            drag: self.drag_expr.produce(writer).expr(),
        }
    }
}

impl Default for LinearDragModifierEditor {
    fn default() -> Self {
        Self {
            drag_expr: ExprWriterEditor::Placeholder,
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct EmitSpawnEventModifierEditor {
    pub condition: EventEmitCondition,
    pub count_expr: ExprWriterEditor,
    pub child_index: u32,
}

impl ModifierProducer<EmitSpawnEventModifier> for EmitSpawnEventModifierEditor {
    fn produce(&self, writer: &ExprWriter) -> EmitSpawnEventModifier {
        EmitSpawnEventModifier {
            condition: self.condition,
            count: self.count_expr.produce(writer).expr(),
            child_index: self.child_index,
        }
    }
}

impl Default for EmitSpawnEventModifierEditor {
    fn default() -> Self {
        Self {
            condition: EventEmitCondition::OnDie,
            count_expr: ExprWriterEditor::U32(0),
            child_index: 0,
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct ConformToSphereModifierEditor {
    pub origin_expr: ExprWriterEditor,
    pub radius_expr: ExprWriterEditor,
    pub influence_dist_expr: ExprWriterEditor,
    pub attraction_accel_expr: ExprWriterEditor,
    pub max_attraction_speed_expr: ExprWriterEditor,
}

impl ModifierProducer<ConformToSphereModifier> for ConformToSphereModifierEditor {
    fn produce(&self, writer: &ExprWriter) -> ConformToSphereModifier {
        ConformToSphereModifier {
            origin: self.origin_expr.produce(writer).expr(),
            radius: self.radius_expr.produce(writer).expr(),
            influence_dist: self.influence_dist_expr.produce(writer).expr(),
            attraction_accel: self.attraction_accel_expr.produce(writer).expr(),
            max_attraction_speed: self.max_attraction_speed_expr.produce(writer).expr(),
            shell_half_thickness: None,
            sticky_factor: None,
        }
    }
}

impl Default for ConformToSphereModifierEditor {
    fn default() -> Self {
        Self {
            origin_expr: ExprWriterEditor::Vec3(Vec3::ZERO),
            radius_expr: ExprWriterEditor::Float(1.0),
            influence_dist_expr: ExprWriterEditor::Float(10.0),
            attraction_accel_expr: ExprWriterEditor::Float(2.0),
            max_attraction_speed_expr: ExprWriterEditor::Float(2.0),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct SizeOverLifetimeModifierEditor {
    pub gradient: GradientVec3Editor,
}

impl RenderModifierProducer<SizeOverLifetimeModifier> for SizeOverLifetimeModifierEditor {
    type Output = SizeOverLifetimeModifier;

    fn produce(&self) -> SizeOverLifetimeModifier {
        SizeOverLifetimeModifier {
            gradient: self.gradient.produce(),
            screen_space_size: false,
        }
    }
}

impl Default for SizeOverLifetimeModifierEditor {
    fn default() -> Self {
        Self {
            gradient: GradientVec3Editor {
                g: vec![(0.3, Vec3::splat(0.1)), (1.0, Vec3::splat(1.0))],
            },
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct ColorOverLifetimeModifierEditor {
    pub gradient: GradientVec4Editor,
    pub blend: Option<ColorBlendMode>,
    pub mask: Option<ColorBlendMask>,
}

impl RenderModifierProducer<ColorOverLifetimeModifier> for ColorOverLifetimeModifierEditor {
    type Output = ColorOverLifetimeModifier;

    fn produce(&self) -> Self::Output {
        ColorOverLifetimeModifier {
            gradient: self.gradient.produce(),
            blend: self.blend.unwrap_or(ColorBlendMode::default()),
            mask: self.mask.unwrap_or(ColorBlendMask::default()),
        }
    }
}

impl Default for ColorOverLifetimeModifierEditor {
    fn default() -> Self {
        Self {
            gradient: GradientVec4Editor {
                g: vec![
                    (0.0, Vec4::new(0.0, 4.0, 4.0, 0.0)),
                    (0.1, Vec4::new(0.0, 4.0, 4.0, 1.0)),
                    (0.3, Vec4::new(4.0, 4.0, 0.0, 1.0)),
                    (0.6, Vec4::new(4.0, 0.0, 0.0, 0.0)),
                    (1.0, Vec4::new(4.0, 0.0, 0.0, 0.0)),
                ],
            },
            blend: Some(ColorBlendMode::default()),
            mask: Some(ColorBlendMask::default()),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct GradientVec3Editor {
    pub g: Vec<(f32, Vec3)>,
}

impl GradientVec3Editor {
    pub fn produce(&self) -> bevy_hanabi::Gradient<Vec3> {
        let mut gradient = bevy_hanabi::Gradient::<Vec3>::new();
        for (t, v) in self.g.iter() {
            gradient.add_key(*t, *v);
        }
        gradient
    }
}

#[derive(Serialize, Deserialize)]
pub struct GradientVec4Editor {
    pub g: Vec<(f32, Vec4)>,
}

impl GradientVec4Editor {
    pub fn produce(&self) -> bevy_hanabi::Gradient<Vec4> {
        let mut gradient = bevy_hanabi::Gradient::<Vec4>::new();
        for (t, v) in self.g.iter() {
            gradient.add_key(*t, *v);
        }
        gradient
    }
}
