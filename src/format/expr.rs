//! Expression model: a serializable tree of literals, random values,
//! attributes and operators that builds bevy_hanabi expressions.

use bevy::prelude::*;
use bevy_hanabi::prelude::*;
use serde::Deserialize;
use serde::Serialize;

use super::hanabi_serde;

pub const ALL_ATTRS: [(Attribute, &str); 39] = [
    (Attribute::ID, "ID"),
    (Attribute::PARTICLE_COUNTER, "Particle Counter"),
    (Attribute::POSITION, "Position"),
    (Attribute::VELOCITY, "Velocity"),
    (Attribute::AGE, "Age"),
    (Attribute::LIFETIME, "Lifetime"),
    (Attribute::COLOR, "Color"),
    (Attribute::HDR_COLOR, "HDR Color"),
    (Attribute::ALPHA, "Alpha"),
    (Attribute::SIZE, "Size"),
    (Attribute::SIZE2, "Size2"),
    (Attribute::SIZE3, "Size3"),
    (Attribute::PREV, "Prev"),
    (Attribute::NEXT, "Next"),
    (Attribute::AXIS_X, "Axis X"),
    (Attribute::AXIS_Y, "Axis Y"),
    (Attribute::AXIS_Z, "Axis Z"),
    (Attribute::SPRITE_INDEX, "Sprite Index"),
    (Attribute::F32_0, "F32_0"),
    (Attribute::F32_1, "F32_1"),
    (Attribute::F32_2, "F32_2"),
    (Attribute::F32_3, "F32_3"),
    (Attribute::F32X2_0, "F32X2_0"),
    (Attribute::F32X2_1, "F32X2_1"),
    (Attribute::F32X2_2, "F32X2_2"),
    (Attribute::F32X2_3, "F32X2_3"),
    (Attribute::F32X3_0, "F32X3_0"),
    (Attribute::F32X3_1, "F32X3_1"),
    (Attribute::F32X3_2, "F32X3_2"),
    (Attribute::F32X3_3, "F32X3_3"),
    (Attribute::F32X4_0, "F32X4_0"),
    (Attribute::F32X4_1, "F32X4_1"),
    (Attribute::F32X4_2, "F32X4_2"),
    (Attribute::F32X4_3, "F32X4_3"),
    (Attribute::U32_0, "U32_0"),
    (Attribute::U32_1, "U32_1"),
    (Attribute::U32_2, "U32_2"),
    (Attribute::U32_3, "U32_3"),
    (Attribute::RIBBON_ID, "Ribbon ID"),
];

pub fn attr_to_label(attr: Attribute) -> &'static str {
    if let Some(result) = ALL_ATTRS
        .iter()
        .filter(|(attr_opt, _)| attr == *attr_opt)
        .next()
    {
        result.1
    } else {
        "None"
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub enum ExprOperatorEditor {
    Uniform(ExprWriterEditor, ExprWriterEditor),
    Multiply(ExprWriterEditor, ExprWriterEditor),
    Subtract(ExprWriterEditor, ExprWriterEditor),
    Add(ExprWriterEditor, ExprWriterEditor),
    Sin(ExprWriterEditor),
    Cos(ExprWriterEditor),
    Distance(ExprWriterEditor, ExprWriterEditor),
    Vec3(ExprWriterEditor, ExprWriterEditor, ExprWriterEditor),
    Vec4(
        ExprWriterEditor,
        ExprWriterEditor,
        ExprWriterEditor,
        ExprWriterEditor,
    ),
    Pack4x8UNorm(ExprWriterEditor),
    Attr(#[serde(with = "hanabi_serde::attribute")] Attribute),
    ParentAttr(#[serde(with = "hanabi_serde::attribute")] Attribute),
    Normalized(ExprWriterEditor),
}

impl ExprOperatorEditor {
    pub fn produce(&self, writer: &ExprWriter) -> WriterExpr {
        match self {
            ExprOperatorEditor::Uniform(lit1, lit2) => {
                lit1.produce(writer).uniform(lit2.produce(writer))
            }
            ExprOperatorEditor::Multiply(lit1, lit2) => {
                lit1.produce(writer).mul(lit2.produce(writer))
            }
            ExprOperatorEditor::Subtract(lit1, lit2) => {
                lit1.produce(writer).sub(lit2.produce(writer))
            }
            ExprOperatorEditor::Add(lit1, lit2) => lit1.produce(writer).add(lit2.produce(writer)),
            ExprOperatorEditor::Distance(lit1, lit2) => {
                lit1.produce(writer).distance(lit2.produce(writer))
            }
            ExprOperatorEditor::Sin(lit) => lit.produce(writer).sin(),
            ExprOperatorEditor::Cos(lit) => lit.produce(writer).cos(),
            ExprOperatorEditor::Vec3(lit1, lit2, lit3) => lit1
                .produce(writer)
                .vec3(lit2.produce(writer), lit3.produce(writer)),
            ExprOperatorEditor::Vec4(lit1, lit2, lit3, lit4) => lit1
                .produce(writer)
                .vec3(lit2.produce(writer), lit3.produce(writer))
                .vec4_xyz_w(lit4.produce(writer)),
            ExprOperatorEditor::Pack4x8UNorm(lit) => lit.produce(writer).pack4x8unorm(),
            ExprOperatorEditor::Attr(attr) => writer.attr(*attr),
            ExprOperatorEditor::ParentAttr(attr) => writer.parent_attr(*attr),
            ExprOperatorEditor::Normalized(lit) => lit.produce(writer).normalized(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub enum ExprWriterEditor {
    Placeholder,
    RandU32,
    RandFloat,
    RandVec3,
    Operator(Box<ExprOperatorEditor>),
    Float(f32),
    U32(u32),
    Vec3(Vec3),
    Vec4(Vec4),
    Time,
    Age,
}

impl ExprWriterEditor {
    pub fn produce(&self, writer: &ExprWriter) -> WriterExpr {
        match self {
            ExprWriterEditor::Operator(o) => o.produce(writer),
            ExprWriterEditor::RandU32 => writer.rand(ValueType::Scalar(ScalarType::Uint)),
            ExprWriterEditor::RandFloat => writer.rand(ValueType::Scalar(ScalarType::Float)),
            ExprWriterEditor::RandVec3 => writer.rand(ValueType::Vector(VectorType::VEC3F)),
            ExprWriterEditor::Float(f) => writer.lit(*f),
            ExprWriterEditor::U32(f) => writer.lit(*f),
            ExprWriterEditor::Vec3(v) => writer.lit(*v),
            ExprWriterEditor::Vec4(v) => writer.lit(*v),
            ExprWriterEditor::Time => writer.time(),
            ExprWriterEditor::Placeholder => writer.lit(0.0),
            ExprWriterEditor::Age => writer.attr(Attribute::AGE),
        }
    }
}

impl Default for ExprWriterEditor {
    fn default() -> Self {
        ExprWriterEditor::Placeholder
    }
}

impl ExprWriterEditor {
    /// Smallest and largest value of a scalar expression, when they are known
    /// without simulating (literals, random values and arithmetic on them).
    pub fn scalar_range(&self) -> Option<(f32, f32)> {
        match self {
            ExprWriterEditor::Placeholder => Some((0.0, 0.0)),
            ExprWriterEditor::Float(v) => Some((*v, *v)),
            ExprWriterEditor::U32(v) => Some((*v as f32, *v as f32)),
            ExprWriterEditor::RandFloat => Some((0.0, 1.0)),
            ExprWriterEditor::Operator(op) => op.scalar_range(),
            _ => None,
        }
    }
}

impl ExprOperatorEditor {
    /// See [`ExprWriterEditor::scalar_range`].
    pub fn scalar_range(&self) -> Option<(f32, f32)> {
        let both = |a: &ExprWriterEditor, b: &ExprWriterEditor| Some((a.scalar_range()?, b.scalar_range()?));
        match self {
            ExprOperatorEditor::Uniform(a, b) => {
                let ((a0, a1), (b0, b1)) = both(a, b)?;
                Some((a0.min(b0), a1.max(b1)))
            }
            ExprOperatorEditor::Add(a, b) => {
                let ((a0, a1), (b0, b1)) = both(a, b)?;
                Some((a0 + b0, a1 + b1))
            }
            ExprOperatorEditor::Subtract(a, b) => {
                let ((a0, a1), (b0, b1)) = both(a, b)?;
                Some((a0 - b1, a1 - b0))
            }
            ExprOperatorEditor::Multiply(a, b) => {
                let ((a0, a1), (b0, b1)) = both(a, b)?;
                let products = [a0 * b0, a0 * b1, a1 * b0, a1 * b1];
                Some((
                    products.iter().copied().fold(f32::INFINITY, f32::min),
                    products.iter().copied().fold(f32::NEG_INFINITY, f32::max),
                ))
            }
            ExprOperatorEditor::Sin(_) | ExprOperatorEditor::Cos(_) => Some((-1.0, 1.0)),
            _ => None,
        }
    }
}
