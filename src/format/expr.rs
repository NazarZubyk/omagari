//! Expression model: a serializable tree of literals, random values,
//! attributes and operators that builds bevy_hanabi expressions.

use std::cell::RefCell;
use std::ops::Deref;
use std::rc::Rc;

use bevy::prelude::*;
use bevy_hanabi::prelude::*;
use serde::Deserialize;
use serde::Serialize;

use super::hanabi_serde;
use super::project::{ParamDef, ParamValue};

/// Properties the runtime writes on a layer's particle entity, readable in
/// expressions with `Property("<name>")`: name, value type and what it holds.
///
/// Positions and directions are in the layer's simulation space: relative to
/// the layer for `Local`, relative to the emitter position (world axes) for
/// `Global`, i.e. the space position modifiers write.
pub const RUNTIME_PROPERTIES: [(&str, RuntimePropertyType, &str); 6] = [
    ("segment_a", RuntimePropertyType::Vec3, "first end (`from`) of a Segment-attached layer"),
    ("segment_b", RuntimePropertyType::Vec3, "second end (`to`) of a Segment-attached layer"),
    (
        "trigger_pos",
        RuntimePropertyType::Vec3,
        "where the last trigger of an OnTrigger layer happened (`trigger_at` position; the layer's origin without one)",
    ),
    (
        "trigger_dir",
        RuntimePropertyType::Vec3,
        "direction of the last trigger of an OnTrigger layer (`trigger_at` direction, unit length; +Y without one)",
    ),
    (
        "trigger_strength",
        RuntimePropertyType::Float,
        "strength of the last trigger of an OnTrigger layer (`trigger_at` strength; 1 without one)",
    ),
    (
        "visibility",
        RuntimePropertyType::Float,
        "the layer's visible-range factor, 0..1 (1 without `visible`); multiply alpha by it to fade the layer",
    ),
];

/// Value type of a [`RUNTIME_PROPERTIES`] entry.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RuntimePropertyType {
    Float,
    Vec3,
}

impl RuntimePropertyType {
    /// Zero of this type.
    pub fn zero(self) -> Value {
        match self {
            RuntimePropertyType::Float => Value::from(0.0f32),
            RuntimePropertyType::Vec3 => Value::from(Vec3::ZERO),
        }
    }
}

/// The runtime property with this name, if any.
pub fn runtime_property(name: &str) -> Option<RuntimePropertyType> {
    RUNTIME_PROPERTIES
        .iter()
        .find(|(n, _, _)| *n == name)
        .map(|(_, t, _)| *t)
}

/// The value a runtime property has until the runtime writes it (also its
/// value without a trigger payload): `trigger_dir` = +Y, `trigger_strength`
/// and `visibility` = 1, everything else zero.
pub fn runtime_property_default(name: &str) -> Option<Value> {
    let t = runtime_property(name)?;
    Some(match name {
        "trigger_dir" => Value::from(Vec3::Y),
        "trigger_strength" | "visibility" => Value::from(1.0f32),
        _ => t.zero(),
    })
}

/// Name of the bevy_hanabi property that holds parameter `name`.
///
/// Parameter names are free text, but property names become WGSL struct
/// fields, so the name is encoded: `param_` + the name with ASCII letters and
/// digits kept, `_` doubled and every other character written as `_<hex>_`
/// (`"hit flash"` → `param_hit_20_flash`). The encoding is one-to-one.
pub fn param_property_name(name: &str) -> String {
    let mut out = String::from("param_");
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if c == '_' {
            out.push_str("__");
        } else {
            out.push_str(&format!("_{:x}_", c as u32));
        }
    }
    out
}

/// The bevy_hanabi [`ExprWriter`] used by `produce()`, plus what the plain
/// writer cannot do: declaring each property once, however many expressions
/// read it, resolving parameters, and collecting warnings. Derefs to the
/// [`ExprWriter`].
pub struct EffectWriter {
    writer: ExprWriter,
    module: Rc<RefCell<Module>>,
    warnings: RefCell<Vec<String>>,
    /// Declared parameters: name and default value (its kind).
    params: Vec<(String, ParamValue)>,
}

impl Default for EffectWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl EffectWriter {
    pub fn new() -> Self {
        let module = Rc::new(RefCell::new(Module::default()));
        EffectWriter {
            writer: ExprWriter::from_module(module.clone()),
            module,
            warnings: RefCell::new(Vec::new()),
            params: Vec::new(),
        }
    }

    /// A writer that resolves `Param(name)` against these parameters.
    pub fn with_params(params: &[ParamDef]) -> Self {
        let mut writer = Self::new();
        writer.params = params.iter().map(|p| (p.name.clone(), p.default)).collect();
        writer
    }

    /// Reads a property, declaring it on first use.
    pub fn property(&self, name: &str, default_value: Value) -> WriterExpr {
        let existing = self.module.borrow().get_property_by_name(name);
        let handle = match existing {
            Some(handle) => handle,
            None => self.writer.add_property(name, default_value),
        };
        self.writer.prop(handle)
    }

    /// Reads a parameter: its property (`f32` for Float, `vec4` for Color),
    /// declared once. Unknown parameters read 0.0, with a warning.
    pub fn param(&self, name: &str) -> WriterExpr {
        match self.params.iter().find(|(n, _)| n == name) {
            Some((_, default)) => self.property(&param_property_name(name), default.to_value()),
            None => {
                self.warn(format!("Param(\"{}\") is not a parameter of this package; it reads 0.0", name));
                self.writer.lit(0.0)
            }
        }
    }

    /// Adds a warning returned by [`EffectWriter::finish`].
    pub fn warn(&self, warning: String) {
        self.warnings.borrow_mut().push(warning);
    }

    /// The finished module and the warnings.
    pub fn finish(self) -> (Module, Vec<String>) {
        let warnings = self.warnings.take();
        (self.writer.finish(), warnings)
    }
}

impl Deref for EffectWriter {
    type Target = ExprWriter;

    fn deref(&self) -> &ExprWriter {
        &self.writer
    }
}

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


/// A value type of hanabi's expression language, as written in project files:
/// the type of a `Rand(..)` value or the target of a `Cast`.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ValueTypeDef {
    #[default]
    F32,
    I32,
    U32,
    Bool,
    Vec2,
    Vec3,
    Vec4,
    IVec2,
    IVec3,
    IVec4,
    UVec2,
    UVec3,
    UVec4,
    BVec2,
    BVec3,
    BVec4,
}

impl ValueTypeDef {
    pub const ALL: [ValueTypeDef; 16] = [
        ValueTypeDef::F32,
        ValueTypeDef::I32,
        ValueTypeDef::U32,
        ValueTypeDef::Bool,
        ValueTypeDef::Vec2,
        ValueTypeDef::Vec3,
        ValueTypeDef::Vec4,
        ValueTypeDef::IVec2,
        ValueTypeDef::IVec3,
        ValueTypeDef::IVec4,
        ValueTypeDef::UVec2,
        ValueTypeDef::UVec3,
        ValueTypeDef::UVec4,
        ValueTypeDef::BVec2,
        ValueTypeDef::BVec3,
        ValueTypeDef::BVec4,
    ];

    /// The WGSL-style name: `f32`, `vec3`, `ivec2`, ...
    pub fn name(self) -> &'static str {
        use ValueTypeDef as T;
        match self {
            T::F32 => "f32",
            T::I32 => "i32",
            T::U32 => "u32",
            T::Bool => "bool",
            T::Vec2 => "vec2",
            T::Vec3 => "vec3",
            T::Vec4 => "vec4",
            T::IVec2 => "ivec2",
            T::IVec3 => "ivec3",
            T::IVec4 => "ivec4",
            T::UVec2 => "uvec2",
            T::UVec3 => "uvec3",
            T::UVec4 => "uvec4",
            T::BVec2 => "bvec2",
            T::BVec3 => "bvec3",
            T::BVec4 => "bvec4",
        }
    }

    /// Element type and component count (1 = scalar).
    pub fn parts(self) -> (ScalarType, u8) {
        use ScalarType as S;
        use ValueTypeDef as T;
        match self {
            T::F32 => (S::Float, 1),
            T::I32 => (S::Int, 1),
            T::U32 => (S::Uint, 1),
            T::Bool => (S::Bool, 1),
            T::Vec2 => (S::Float, 2),
            T::Vec3 => (S::Float, 3),
            T::Vec4 => (S::Float, 4),
            T::IVec2 => (S::Int, 2),
            T::IVec3 => (S::Int, 3),
            T::IVec4 => (S::Int, 4),
            T::UVec2 => (S::Uint, 2),
            T::UVec3 => (S::Uint, 3),
            T::UVec4 => (S::Uint, 4),
            T::BVec2 => (S::Bool, 2),
            T::BVec3 => (S::Bool, 3),
            T::BVec4 => (S::Bool, 4),
        }
    }

    /// Whether the elements are f32.
    pub fn is_float(self) -> bool {
        self.parts().0 == ScalarType::Float
    }

    pub fn to_hanabi(self) -> ValueType {
        match self.parts() {
            (elem, 1) => ValueType::Scalar(elem),
            (elem, n) => ValueType::Vector(VectorType::new(elem, n)),
        }
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
    /// `Mix(a, b, t)` = `a × (1 − t) + b × t`; `t` is the same type as `a`
    /// and `b`, or an f32.
    Mix(ExprWriterEditor, ExprWriterEditor, ExprWriterEditor),
    /// `Clamp(x, low, high)`: `x` kept between `low` and `high` (same types).
    Clamp(ExprWriterEditor, ExprWriterEditor, ExprWriterEditor),
    /// `SmoothStep(low, high, x)`: 0 at `low`, 1 at `high`, smooth in between
    /// (same types, like WGSL `smoothstep`).
    SmoothStep(ExprWriterEditor, ExprWriterEditor, ExprWriterEditor),
    Min(ExprWriterEditor, ExprWriterEditor),
    Max(ExprWriterEditor, ExprWriterEditor),
    // --- P2b: the rest of hanabi's operators -------------------------------
    Divide(ExprWriterEditor, ExprWriterEditor),
    Remainder(ExprWriterEditor, ExprWriterEditor),
    Abs(ExprWriterEditor),
    Sign(ExprWriterEditor),
    Floor(ExprWriterEditor),
    Ceil(ExprWriterEditor),
    Round(ExprWriterEditor),
    Fract(ExprWriterEditor),
    Saturate(ExprWriterEditor),
    Sqrt(ExprWriterEditor),
    InvSqrt(ExprWriterEditor),
    Exp(ExprWriterEditor),
    Exp2(ExprWriterEditor),
    Log(ExprWriterEditor),
    Log2(ExprWriterEditor),
    Tan(ExprWriterEditor),
    Asin(ExprWriterEditor),
    Acos(ExprWriterEditor),
    Atan(ExprWriterEditor),
    /// `Atan2(y, x)`: the angle of the point (x, y), like WGSL `atan2`.
    Atan2(ExprWriterEditor, ExprWriterEditor),
    /// `Step(edge, x)`: 1 when `x ≥ edge`, else 0, like WGSL `step`.
    Step(ExprWriterEditor, ExprWriterEditor),
    /// `Normal(mean, std_dev)`: a normally distributed random value.
    Normal(ExprWriterEditor, ExprWriterEditor),
    Length(ExprWriterEditor),
    Dot(ExprWriterEditor, ExprWriterEditor),
    Cross(ExprWriterEditor, ExprWriterEditor),
    Vec2(ExprWriterEditor, ExprWriterEditor),
    /// `Vec4XyzW(xyz, w)`: a vec4 from a vec3 and a number.
    Vec4XyzW(ExprWriterEditor, ExprWriterEditor),
    X(ExprWriterEditor),
    Y(ExprWriterEditor),
    Z(ExprWriterEditor),
    W(ExprWriterEditor),
    GreaterThan(ExprWriterEditor, ExprWriterEditor),
    GreaterThanOrEqual(ExprWriterEditor, ExprWriterEditor),
    LessThan(ExprWriterEditor, ExprWriterEditor),
    LessThanOrEqual(ExprWriterEditor, ExprWriterEditor),
    All(ExprWriterEditor),
    Any(ExprWriterEditor),
    Pack4x8SNorm(ExprWriterEditor),
    Unpack4x8UNorm(ExprWriterEditor),
    Unpack4x8SNorm(ExprWriterEditor),
    /// `Cast(x, type)`: `x` converted to another type (`i32(x)`,
    /// `vec3<f32>(x)`, ...).
    Cast(ExprWriterEditor, ValueTypeDef),
}

/// One row of [`OPERATORS`]: what the editor menu, the guide and the
/// analysis know about an operator. A new operator is a new enum variant plus
/// one row here and its arms in `name`, `args`, `args_mut` and `produce`.
pub struct OperatorInfo {
    /// Name as written in project files.
    pub name: &'static str,
    /// Names of the sub-expressions, in written order (none for `Attr` /
    /// `ParentAttr`).
    pub args: &'static [&'static str],
    /// Plain words for tooltips.
    pub hint: &'static str,
    /// Menu group (`OPERATOR_GROUPS`).
    pub group: &'static str,
    /// A new operator with `Placeholder` arguments.
    pub make: fn() -> ExprOperatorEditor,
}

fn ph() -> ExprWriterEditor {
    ExprWriterEditor::Placeholder
}

/// Groups of the operator menu, in menu order.
pub const OPERATOR_GROUPS: [&str; 9] =
    ["Arithmetic", "Blend", "Random", "Rounding", "Math", "Vectors", "Compare", "Convert", "Attributes"];

macro_rules! op {
    ($name:literal, $group:literal, [$($arg:literal),*], $hint:literal, $make:expr) => {
        OperatorInfo { name: $name, args: &[$($arg),*], hint: $hint, group: $group, make: $make }
    };
}

/// Every operator, in menu order.
pub const OPERATORS: [OperatorInfo; 59] = [
    op!("Add", "Arithmetic", ["a", "b"], "a + b (a vector and a number: the number is added to every component)", || ExprOperatorEditor::Add(ph(), ph())),
    op!("Subtract", "Arithmetic", ["a", "b"], "a − b", || ExprOperatorEditor::Subtract(ph(), ph())),
    op!("Multiply", "Arithmetic", ["a", "b"], "a × b (a vector times a number scales it)", || ExprOperatorEditor::Multiply(ph(), ph())),
    op!("Divide", "Arithmetic", ["a", "b"], "a ÷ b (a vector divided by a number shrinks it)", || ExprOperatorEditor::Divide(ph(), ph())),
    op!("Remainder", "Arithmetic", ["a", "b"], "What is left of a after taking out whole b's (5 % 3 = 2): repeats a value", || ExprOperatorEditor::Remainder(ph(), ph())),
    op!("Min", "Arithmetic", ["a", "b"], "The smaller of a and b", || ExprOperatorEditor::Min(ph(), ph())),
    op!("Max", "Arithmetic", ["a", "b"], "The larger of a and b", || ExprOperatorEditor::Max(ph(), ph())),
    op!("Abs", "Arithmetic", ["x"], "x without its sign (−2 → 2)", || ExprOperatorEditor::Abs(ph())),
    op!("Sign", "Arithmetic", ["x"], "−1, 0 or 1: whether x is negative, zero or positive", || ExprOperatorEditor::Sign(ph())),
    op!("Mix", "Blend", ["a", "b", "t"], "Blend from a (t = 0) to b (t = 1); t is often a parameter", || ExprOperatorEditor::Mix(ph(), ph(), ph())),
    op!("Clamp", "Blend", ["x", "low", "high"], "x kept between low and high (x, low, high)", || ExprOperatorEditor::Clamp(ph(), ph(), ph())),
    op!("Saturate", "Blend", ["x"], "x kept between 0 and 1", || ExprOperatorEditor::Saturate(ph())),
    op!("SmoothStep", "Blend", ["low", "high", "x"], "0 below low, 1 above high, a smooth S-curve in between (low, high, x)", || ExprOperatorEditor::SmoothStep(ph(), ph(), ph())),
    op!("Step", "Blend", ["edge", "x"], "1 when x is at or above edge, else 0: switches a value on", || ExprOperatorEditor::Step(ph(), ph())),
    op!("Uniform", "Random", ["low", "high"], "A random value between low and high, per particle (floats)", || ExprOperatorEditor::Uniform(ph(), ph())),
    op!("Normal", "Random", ["mean", "std_dev"], "A random value around mean; about two thirds fall within ± std_dev (floats)", || ExprOperatorEditor::Normal(ph(), ph())),
    op!("Floor", "Rounding", ["x"], "Rounds down to a whole number (2.7 → 2)", || ExprOperatorEditor::Floor(ph())),
    op!("Ceil", "Rounding", ["x"], "Rounds up to a whole number (2.1 → 3)", || ExprOperatorEditor::Ceil(ph())),
    op!("Round", "Rounding", ["x"], "Rounds to the nearest whole number (halves to even)", || ExprOperatorEditor::Round(ph())),
    op!("Fract", "Rounding", ["x"], "The part after the decimal point (2.7 → 0.7): a 0..1 saw-tooth", || ExprOperatorEditor::Fract(ph())),
    op!("Sin", "Math", ["x"], "Sine (radians): waves between −1 and 1", || ExprOperatorEditor::Sin(ph())),
    op!("Cos", "Math", ["x"], "Cosine (radians): waves between −1 and 1", || ExprOperatorEditor::Cos(ph())),
    op!("Tan", "Math", ["x"], "Tangent (radians)", || ExprOperatorEditor::Tan(ph())),
    op!("Asin", "Math", ["x"], "Inverse sine: the angle (radians) whose sine is x", || ExprOperatorEditor::Asin(ph())),
    op!("Acos", "Math", ["x"], "Inverse cosine: the angle (radians) whose cosine is x", || ExprOperatorEditor::Acos(ph())),
    op!("Atan", "Math", ["x"], "Inverse tangent: the angle (radians) whose tangent is x", || ExprOperatorEditor::Atan(ph())),
    op!("Atan2", "Math", ["y", "x"], "The angle (radians) of the point (x, y) around the origin", || ExprOperatorEditor::Atan2(ph(), ph())),
    op!("Sqrt", "Math", ["x"], "Square root", || ExprOperatorEditor::Sqrt(ph())),
    op!("InvSqrt", "Math", ["x"], "1 ÷ square root of x", || ExprOperatorEditor::InvSqrt(ph())),
    op!("Exp", "Math", ["x"], "e to the power x (fast growth)", || ExprOperatorEditor::Exp(ph())),
    op!("Exp2", "Math", ["x"], "2 to the power x", || ExprOperatorEditor::Exp2(ph())),
    op!("Log", "Math", ["x"], "Natural logarithm (slow growth)", || ExprOperatorEditor::Log(ph())),
    op!("Log2", "Math", ["x"], "Base-2 logarithm", || ExprOperatorEditor::Log2(ph())),
    op!("Vec2", "Vectors", ["x", "y"], "A vec2 from two numbers", || ExprOperatorEditor::Vec2(ph(), ph())),
    op!("Vec3", "Vectors", ["x", "y", "z"], "A vector from three numbers (x, y, z)", || ExprOperatorEditor::Vec3(ph(), ph(), ph())),
    op!("Vec4", "Vectors", ["x", "y", "z", "w"], "A vector / colour from four numbers (r, g, b, a)", || ExprOperatorEditor::Vec4(ph(), ph(), ph(), ph())),
    op!("Vec4XyzW", "Vectors", ["xyz", "w"], "A vec4 from a vec3 and a number (a colour and its opacity)", || ExprOperatorEditor::Vec4XyzW(ph(), ph())),
    op!("X", "Vectors", ["v"], "The first component of a vector", || ExprOperatorEditor::X(ph())),
    op!("Y", "Vectors", ["v"], "The second component of a vector", || ExprOperatorEditor::Y(ph())),
    op!("Z", "Vectors", ["v"], "The third component of a vector (vec3 or vec4)", || ExprOperatorEditor::Z(ph())),
    op!("W", "Vectors", ["v"], "The fourth component of a vec4 (a colour's opacity)", || ExprOperatorEditor::W(ph())),
    op!("Length", "Vectors", ["v"], "How long a vector is (a speed from a velocity)", || ExprOperatorEditor::Length(ph())),
    op!("Distance", "Vectors", ["a", "b"], "Distance between two points", || ExprOperatorEditor::Distance(ph(), ph())),
    op!("Normalized", "Vectors", ["v"], "The vector made 1 long (its direction)", || ExprOperatorEditor::Normalized(ph())),
    op!("Dot", "Vectors", ["a", "b"], "Dot product: how much two directions agree (1 same, 0 square, −1 opposite, for unit vectors)", || ExprOperatorEditor::Dot(ph(), ph())),
    op!("Cross", "Vectors", ["a", "b"], "Cross product of two vec3: a direction square to both", || ExprOperatorEditor::Cross(ph(), ph())),
    op!("GreaterThan", "Compare", ["a", "b"], "true when a > b (per component for vectors)", || ExprOperatorEditor::GreaterThan(ph(), ph())),
    op!("GreaterThanOrEqual", "Compare", ["a", "b"], "true when a ≥ b (per component for vectors)", || ExprOperatorEditor::GreaterThanOrEqual(ph(), ph())),
    op!("LessThan", "Compare", ["a", "b"], "true when a < b (per component for vectors)", || ExprOperatorEditor::LessThan(ph(), ph())),
    op!("LessThanOrEqual", "Compare", ["a", "b"], "true when a ≤ b (per component for vectors)", || ExprOperatorEditor::LessThanOrEqual(ph(), ph())),
    op!("All", "Compare", ["v"], "true when every component of a bool vector is true", || ExprOperatorEditor::All(ph())),
    op!("Any", "Compare", ["v"], "true when any component of a bool vector is true", || ExprOperatorEditor::Any(ph())),
    op!("Cast", "Convert", ["x"], "Converts x to another type: f32 → i32 (sprite_index), bool → f32 (0 or 1), f32 → vec3 (all three the same), ...", || ExprOperatorEditor::Cast(ph(), ValueTypeDef::F32)),
    op!("Pack4x8UNorm", "Convert", ["rgba"], "Packs a vec4 colour (0..1) into the u32 `color` attribute", || ExprOperatorEditor::Pack4x8UNorm(ph())),
    op!("Pack4x8SNorm", "Convert", ["v"], "Packs a vec4 with values −1..1 into a u32", || ExprOperatorEditor::Pack4x8SNorm(ph())),
    op!("Unpack4x8UNorm", "Convert", ["u"], "Unpacks a u32 colour (like `color`) into a vec4 (0..1)", || ExprOperatorEditor::Unpack4x8UNorm(ph())),
    op!("Unpack4x8SNorm", "Convert", ["u"], "Unpacks a u32 into a vec4 with values −1..1", || ExprOperatorEditor::Unpack4x8SNorm(ph())),
    op!("Attr", "Attributes", [], "Reads an attribute of the particle", || ExprOperatorEditor::Attr(Attribute::ID)),
    op!("ParentAttr", "Attributes", [], "Reads an attribute of the parent particle (child layers, init modifiers)", || ExprOperatorEditor::ParentAttr(Attribute::ID)),
];

impl ExprOperatorEditor {
    /// The operator's name as written in project files.
    pub fn name(&self) -> &'static str {
        use ExprOperatorEditor as O;
        match self {
            O::Uniform(..) => "Uniform",
            O::Multiply(..) => "Multiply",
            O::Subtract(..) => "Subtract",
            O::Add(..) => "Add",
            O::Sin(..) => "Sin",
            O::Cos(..) => "Cos",
            O::Distance(..) => "Distance",
            O::Vec3(..) => "Vec3",
            O::Vec4(..) => "Vec4",
            O::Pack4x8UNorm(..) => "Pack4x8UNorm",
            O::Attr(..) => "Attr",
            O::ParentAttr(..) => "ParentAttr",
            O::Normalized(..) => "Normalized",
            O::Mix(..) => "Mix",
            O::Clamp(..) => "Clamp",
            O::SmoothStep(..) => "SmoothStep",
            O::Min(..) => "Min",
            O::Max(..) => "Max",
            O::Divide(..) => "Divide",
            O::Remainder(..) => "Remainder",
            O::Abs(..) => "Abs",
            O::Sign(..) => "Sign",
            O::Floor(..) => "Floor",
            O::Ceil(..) => "Ceil",
            O::Round(..) => "Round",
            O::Fract(..) => "Fract",
            O::Saturate(..) => "Saturate",
            O::Sqrt(..) => "Sqrt",
            O::InvSqrt(..) => "InvSqrt",
            O::Exp(..) => "Exp",
            O::Exp2(..) => "Exp2",
            O::Log(..) => "Log",
            O::Log2(..) => "Log2",
            O::Tan(..) => "Tan",
            O::Asin(..) => "Asin",
            O::Acos(..) => "Acos",
            O::Atan(..) => "Atan",
            O::Atan2(..) => "Atan2",
            O::Step(..) => "Step",
            O::Normal(..) => "Normal",
            O::Length(..) => "Length",
            O::Dot(..) => "Dot",
            O::Cross(..) => "Cross",
            O::Vec2(..) => "Vec2",
            O::Vec4XyzW(..) => "Vec4XyzW",
            O::X(..) => "X",
            O::Y(..) => "Y",
            O::Z(..) => "Z",
            O::W(..) => "W",
            O::GreaterThan(..) => "GreaterThan",
            O::GreaterThanOrEqual(..) => "GreaterThanOrEqual",
            O::LessThan(..) => "LessThan",
            O::LessThanOrEqual(..) => "LessThanOrEqual",
            O::All(..) => "All",
            O::Any(..) => "Any",
            O::Pack4x8SNorm(..) => "Pack4x8SNorm",
            O::Unpack4x8UNorm(..) => "Unpack4x8UNorm",
            O::Unpack4x8SNorm(..) => "Unpack4x8SNorm",
            O::Cast(..) => "Cast",
        }
    }

    /// The operator's row in [`OPERATORS`].
    pub fn info(&self) -> &'static OperatorInfo {
        let name = self.name();
        OPERATORS.iter().find(|o| o.name == name).expect("every operator has a row")
    }

    /// The sub-expressions, in written order.
    pub fn args(&self) -> Vec<&ExprWriterEditor> {
        use ExprOperatorEditor as O;
        match self {
            O::Sin(a)
            | O::Cos(a)
            | O::Pack4x8UNorm(a)
            | O::Normalized(a)
            | O::Abs(a)
            | O::Sign(a)
            | O::Floor(a)
            | O::Ceil(a)
            | O::Round(a)
            | O::Fract(a)
            | O::Saturate(a)
            | O::Sqrt(a)
            | O::InvSqrt(a)
            | O::Exp(a)
            | O::Exp2(a)
            | O::Log(a)
            | O::Log2(a)
            | O::Tan(a)
            | O::Asin(a)
            | O::Acos(a)
            | O::Atan(a)
            | O::Length(a)
            | O::X(a)
            | O::Y(a)
            | O::Z(a)
            | O::W(a)
            | O::All(a)
            | O::Any(a)
            | O::Pack4x8SNorm(a)
            | O::Unpack4x8UNorm(a)
            | O::Unpack4x8SNorm(a)
            | O::Cast(a, _) => vec![a],
            O::Uniform(a, b)
            | O::Multiply(a, b)
            | O::Subtract(a, b)
            | O::Add(a, b)
            | O::Distance(a, b)
            | O::Min(a, b)
            | O::Max(a, b)
            | O::Divide(a, b)
            | O::Remainder(a, b)
            | O::Atan2(a, b)
            | O::Step(a, b)
            | O::Normal(a, b)
            | O::Dot(a, b)
            | O::Cross(a, b)
            | O::Vec2(a, b)
            | O::Vec4XyzW(a, b)
            | O::GreaterThan(a, b)
            | O::GreaterThanOrEqual(a, b)
            | O::LessThan(a, b)
            | O::LessThanOrEqual(a, b) => vec![a, b],
            O::Vec3(a, b, c) | O::Mix(a, b, c) | O::Clamp(a, b, c) | O::SmoothStep(a, b, c) => vec![a, b, c],
            O::Vec4(a, b, c, d) => vec![a, b, c, d],
            O::Attr(_) | O::ParentAttr(_) => vec![],
        }
    }

    /// The sub-expressions, mutable.
    pub fn args_mut(&mut self) -> Vec<&mut ExprWriterEditor> {
        use ExprOperatorEditor as O;
        match self {
            O::Sin(a)
            | O::Cos(a)
            | O::Pack4x8UNorm(a)
            | O::Normalized(a)
            | O::Abs(a)
            | O::Sign(a)
            | O::Floor(a)
            | O::Ceil(a)
            | O::Round(a)
            | O::Fract(a)
            | O::Saturate(a)
            | O::Sqrt(a)
            | O::InvSqrt(a)
            | O::Exp(a)
            | O::Exp2(a)
            | O::Log(a)
            | O::Log2(a)
            | O::Tan(a)
            | O::Asin(a)
            | O::Acos(a)
            | O::Atan(a)
            | O::Length(a)
            | O::X(a)
            | O::Y(a)
            | O::Z(a)
            | O::W(a)
            | O::All(a)
            | O::Any(a)
            | O::Pack4x8SNorm(a)
            | O::Unpack4x8UNorm(a)
            | O::Unpack4x8SNorm(a)
            | O::Cast(a, _) => vec![a],
            O::Uniform(a, b)
            | O::Multiply(a, b)
            | O::Subtract(a, b)
            | O::Add(a, b)
            | O::Distance(a, b)
            | O::Min(a, b)
            | O::Max(a, b)
            | O::Divide(a, b)
            | O::Remainder(a, b)
            | O::Atan2(a, b)
            | O::Step(a, b)
            | O::Normal(a, b)
            | O::Dot(a, b)
            | O::Cross(a, b)
            | O::Vec2(a, b)
            | O::Vec4XyzW(a, b)
            | O::GreaterThan(a, b)
            | O::GreaterThanOrEqual(a, b)
            | O::LessThan(a, b)
            | O::LessThanOrEqual(a, b) => vec![a, b],
            O::Vec3(a, b, c) | O::Mix(a, b, c) | O::Clamp(a, b, c) | O::SmoothStep(a, b, c) => vec![a, b, c],
            O::Vec4(a, b, c, d) => vec![a, b, c, d],
            O::Attr(_) | O::ParentAttr(_) => vec![],
        }
    }

    pub fn produce(&self, writer: &EffectWriter) -> WriterExpr {
        use ExprOperatorEditor as O;
        let w = |e: &ExprWriterEditor| e.produce(writer);
        let unary = |a: &ExprWriterEditor, op: UnaryOperator| {
            let expr = w(a).expr();
            writer.push(Expr::Unary { op, expr })
        };
        let binary = |a: &ExprWriterEditor, b: &ExprWriterEditor, op: BinaryOperator| {
            let left = w(a).expr();
            let right = w(b).expr();
            writer.push(Expr::Binary { op, left, right })
        };
        match self {
            O::Uniform(a, b) => w(a).uniform(w(b)),
            O::Multiply(a, b) => w(a).mul(w(b)),
            O::Subtract(a, b) => w(a).sub(w(b)),
            O::Add(a, b) => w(a).add(w(b)),
            O::Distance(a, b) => w(a).distance(w(b)),
            O::Sin(a) => w(a).sin(),
            O::Cos(a) => w(a).cos(),
            O::Vec3(a, b, c) => w(a).vec3(w(b), w(c)),
            O::Vec4(a, b, c, d) => w(a).vec3(w(b), w(c)).vec4_xyz_w(w(d)),
            O::Pack4x8UNorm(a) => w(a).pack4x8unorm(),
            O::Attr(attr) => writer.attr(*attr),
            O::ParentAttr(attr) => writer.parent_attr(*attr),
            O::Normalized(a) => w(a).normalized(),
            O::Mix(a, b, t) => w(a).mix(w(b), w(t)),
            O::Clamp(x, low, high) => w(x).clamp(w(low), w(high)),
            O::SmoothStep(low, high, x) => w(x).smoothstep(w(low), w(high)),
            O::Min(a, b) => w(a).min(w(b)),
            O::Max(a, b) => w(a).max(w(b)),
            // The written order is kept: arguments are produced left to
            // right, then the operator.
            O::Divide(a, b) => binary(a, b, BinaryOperator::Div),
            O::Remainder(a, b) => binary(a, b, BinaryOperator::Remainder),
            O::Atan2(y, x) => binary(y, x, BinaryOperator::Atan2),
            O::Step(edge, x) => binary(edge, x, BinaryOperator::Step),
            O::Normal(mean, std_dev) => binary(mean, std_dev, BinaryOperator::NormalRand),
            O::Dot(a, b) => binary(a, b, BinaryOperator::Dot),
            O::Cross(a, b) => binary(a, b, BinaryOperator::Cross),
            O::Vec2(x, y) => binary(x, y, BinaryOperator::Vec2),
            O::Vec4XyzW(xyz, w_) => binary(xyz, w_, BinaryOperator::Vec4XyzW),
            O::GreaterThan(a, b) => binary(a, b, BinaryOperator::GreaterThan),
            O::GreaterThanOrEqual(a, b) => binary(a, b, BinaryOperator::GreaterThanOrEqual),
            O::LessThan(a, b) => binary(a, b, BinaryOperator::LessThan),
            O::LessThanOrEqual(a, b) => binary(a, b, BinaryOperator::LessThanOrEqual),
            O::Abs(a) => unary(a, UnaryOperator::Abs),
            O::Sign(a) => unary(a, UnaryOperator::Sign),
            O::Floor(a) => unary(a, UnaryOperator::Floor),
            O::Ceil(a) => unary(a, UnaryOperator::Ceil),
            O::Round(a) => unary(a, UnaryOperator::Round),
            O::Fract(a) => unary(a, UnaryOperator::Fract),
            O::Saturate(a) => unary(a, UnaryOperator::Saturate),
            O::Sqrt(a) => unary(a, UnaryOperator::Sqrt),
            O::InvSqrt(a) => unary(a, UnaryOperator::InvSqrt),
            O::Exp(a) => unary(a, UnaryOperator::Exp),
            O::Exp2(a) => unary(a, UnaryOperator::Exp2),
            O::Log(a) => unary(a, UnaryOperator::Log),
            O::Log2(a) => unary(a, UnaryOperator::Log2),
            O::Tan(a) => unary(a, UnaryOperator::Tan),
            O::Asin(a) => unary(a, UnaryOperator::Asin),
            O::Acos(a) => unary(a, UnaryOperator::Acos),
            O::Atan(a) => unary(a, UnaryOperator::Atan),
            O::Length(a) => unary(a, UnaryOperator::Length),
            O::X(a) => unary(a, UnaryOperator::X),
            O::Y(a) => unary(a, UnaryOperator::Y),
            O::Z(a) => unary(a, UnaryOperator::Z),
            O::W(a) => unary(a, UnaryOperator::W),
            O::All(a) => unary(a, UnaryOperator::All),
            O::Any(a) => unary(a, UnaryOperator::Any),
            O::Pack4x8SNorm(a) => unary(a, UnaryOperator::Pack4x8snorm),
            O::Unpack4x8UNorm(a) => unary(a, UnaryOperator::Unpack4x8unorm),
            O::Unpack4x8SNorm(a) => unary(a, UnaryOperator::Unpack4x8snorm),
            O::Cast(a, t) => w(a).cast(t.to_hanabi()),
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub enum ExprWriterEditor {
    Placeholder,
    /// A random u32. hanabi 0.19's shaders have no integer random function,
    /// so this fails on the GPU (`check_project` reports it); kept loadable.
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
    /// A runtime property (see [`RUNTIME_PROPERTIES`]), e.g.
    /// `Property("segment_a")`.
    Property(String),
    /// A parameter of the package, e.g. `Param("intensity")`: f32 for Float
    /// parameters, vec4 (linear RGBA) for Color ones. Its current (smoothed)
    /// value per instance; init modifiers read it when a particle is born,
    /// update modifiers every frame.
    Param(String),
    // --- P2b: every literal type and built-in value -------------------------
    I32(i32),
    Bool(bool),
    Vec2(Vec2),
    IVec2(IVec2),
    IVec3(IVec3),
    IVec4(IVec4),
    UVec2(UVec2),
    UVec3(UVec3),
    UVec4(UVec4),
    BVec2(BVec2),
    BVec3(BVec3),
    BVec4(BVec4),
    /// A random value of a type, each component in [0, 1) (float types only:
    /// hanabi 0.19 has no integer or bool random function in its shaders).
    Rand(ValueTypeDef),
    /// Seconds since the last simulation step.
    DeltaTime,
    /// Bevy's virtual clock (pausable, scaled); `Time` is the effect
    /// simulation clock.
    VirtualTime,
    VirtualDeltaTime,
    /// Real (wall-clock) seconds since startup.
    RealTime,
    RealDeltaTime,
    /// The cutoff of `alpha_mode: Mask`. bevy_hanabi 0.19 writes it as
    /// `sim_params.alpha_cutoff`, which does not exist, so any shader using it
    /// fails to compile; `check_project` reports it.
    AlphaCutoff,
    /// Whether the particle is still alive (update modifiers only).
    IsAlive,
}

impl ExprWriterEditor {
    pub fn produce(&self, writer: &EffectWriter) -> WriterExpr {
        let builtin = |op: BuiltInOperator| writer.push(Expr::BuiltIn(BuiltInExpr::new(op)));
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
            ExprWriterEditor::Property(name) => match runtime_property_default(name) {
                Some(default) => writer.property(name, default),
                None => {
                    writer.warn(format!(
                        "Property(\"{}\") is not a runtime property ({}); it reads 0.0",
                        name,
                        RUNTIME_PROPERTIES.map(|(n, _, _)| n).join(", ")
                    ));
                    writer.lit(0.0)
                }
            },
            ExprWriterEditor::Param(name) => writer.param(name),
            ExprWriterEditor::I32(v) => writer.lit(*v),
            ExprWriterEditor::Bool(v) => writer.lit(*v),
            ExprWriterEditor::Vec2(v) => writer.lit(*v),
            ExprWriterEditor::IVec2(v) => writer.lit(Value::Vector((*v).into())),
            ExprWriterEditor::IVec3(v) => writer.lit(Value::Vector((*v).into())),
            ExprWriterEditor::IVec4(v) => writer.lit(Value::Vector((*v).into())),
            ExprWriterEditor::UVec2(v) => writer.lit(Value::Vector((*v).into())),
            ExprWriterEditor::UVec3(v) => writer.lit(Value::Vector((*v).into())),
            ExprWriterEditor::UVec4(v) => writer.lit(Value::Vector((*v).into())),
            ExprWriterEditor::BVec2(v) => writer.lit(Value::Vector((*v).into())),
            ExprWriterEditor::BVec3(v) => writer.lit(Value::Vector((*v).into())),
            ExprWriterEditor::BVec4(v) => writer.lit(Value::Vector((*v).into())),
            ExprWriterEditor::Rand(t) => writer.rand(t.to_hanabi()),
            ExprWriterEditor::DeltaTime => builtin(BuiltInOperator::DeltaTime),
            ExprWriterEditor::VirtualTime => builtin(BuiltInOperator::VirtualTime),
            ExprWriterEditor::VirtualDeltaTime => builtin(BuiltInOperator::VirtualDeltaTime),
            ExprWriterEditor::RealTime => builtin(BuiltInOperator::RealTime),
            ExprWriterEditor::RealDeltaTime => builtin(BuiltInOperator::RealDeltaTime),
            ExprWriterEditor::AlphaCutoff => builtin(BuiltInOperator::AlphaCutoff),
            ExprWriterEditor::IsAlive => builtin(BuiltInOperator::IsAlive),
        }
    }

    /// The name of a value that is not an operator, as written in project
    /// files (`"Float"`, `"Time"`, `"Rand"`, ...); `None` for operators.
    pub fn leaf_name(&self) -> Option<&'static str> {
        use ExprWriterEditor as E;
        Some(match self {
            E::Operator(_) => return None,
            E::Placeholder => "Placeholder",
            E::RandU32 => "RandU32",
            E::RandFloat => "RandFloat",
            E::RandVec3 => "RandVec3",
            E::Float(_) => "Float",
            E::U32(_) => "U32",
            E::Vec3(_) => "Vec3",
            E::Vec4(_) => "Vec4",
            E::Time => "Time",
            E::Age => "Age",
            E::Property(_) => "Property",
            E::Param(_) => "Param",
            E::I32(_) => "I32",
            E::Bool(_) => "Bool",
            E::Vec2(_) => "Vec2",
            E::IVec2(_) => "IVec2",
            E::IVec3(_) => "IVec3",
            E::IVec4(_) => "IVec4",
            E::UVec2(_) => "UVec2",
            E::UVec3(_) => "UVec3",
            E::UVec4(_) => "UVec4",
            E::BVec2(_) => "BVec2",
            E::BVec3(_) => "BVec3",
            E::BVec4(_) => "BVec4",
            E::Rand(_) => "Rand",
            E::DeltaTime => "DeltaTime",
            E::VirtualTime => "VirtualTime",
            E::VirtualDeltaTime => "VirtualDeltaTime",
            E::RealTime => "RealTime",
            E::RealDeltaTime => "RealDeltaTime",
            E::AlphaCutoff => "AlphaCutoff",
            E::IsAlive => "IsAlive",
        })
    }
}

/// One row of [`EXPR_VALUES`]: a value that is not an operator (literal,
/// random value, built-in time).
pub struct ValueInfo {
    pub name: &'static str,
    /// Value type in words.
    pub ty: &'static str,
    pub hint: &'static str,
    /// Menu group: "Number", "Vector", "Random", "Time", "Special".
    pub group: &'static str,
    pub make: fn() -> ExprWriterEditor,
}

macro_rules! value {
    ($name:literal, $group:literal, $ty:literal, $hint:literal, $make:expr) => {
        ValueInfo { name: $name, ty: $ty, hint: $hint, group: $group, make: $make }
    };
}

/// The values of the "+" menu besides operators, parameters and properties,
/// in menu order. `RandU32` is not listed and `AlphaCutoff` is not offered
/// ([`NOT_OFFERED_VALUES`]): both fail on the GPU with bevy_hanabi 0.19.
pub const EXPR_VALUES: [ValueInfo; 29] = [
    value!("Float", "Number", "f32", "A number with a decimal point", || ExprWriterEditor::Float(0.0)),
    value!("I32", "Number", "i32", "A whole number that can be negative (sprite_index is an i32)", || ExprWriterEditor::I32(0)),
    value!("U32", "Number", "u32", "A whole number, 0 or more (counts, packed colours)", || ExprWriterEditor::U32(0)),
    value!("Bool", "Number", "bool", "true or false", || ExprWriterEditor::Bool(false)),
    value!("Vec2", "Vector", "vec2", "Two numbers (size2)", || ExprWriterEditor::Vec2(Vec2::ZERO)),
    value!("Vec3", "Vector", "vec3", "Three numbers: a position, a direction, a velocity", || ExprWriterEditor::Vec3(Vec3::ZERO)),
    value!("Vec4", "Vector", "vec4", "Four numbers: a colour (red, green, blue, opacity)", || ExprWriterEditor::Vec4(Vec4::ZERO)),
    value!("IVec2", "Vector", "ivec2", "Two whole numbers", || ExprWriterEditor::IVec2(IVec2::ZERO)),
    value!("IVec3", "Vector", "ivec3", "Three whole numbers", || ExprWriterEditor::IVec3(IVec3::ZERO)),
    value!("IVec4", "Vector", "ivec4", "Four whole numbers", || ExprWriterEditor::IVec4(IVec4::ZERO)),
    value!("UVec2", "Vector", "uvec2", "Two whole numbers, 0 or more", || ExprWriterEditor::UVec2(UVec2::ZERO)),
    value!("UVec3", "Vector", "uvec3", "Three whole numbers, 0 or more", || ExprWriterEditor::UVec3(UVec3::ZERO)),
    value!("UVec4", "Vector", "uvec4", "Four whole numbers, 0 or more", || ExprWriterEditor::UVec4(UVec4::ZERO)),
    value!("BVec2", "Vector", "bvec2", "Two true/false values", || ExprWriterEditor::BVec2(BVec2::FALSE)),
    value!("BVec3", "Vector", "bvec3", "Three true/false values", || ExprWriterEditor::BVec3(BVec3::FALSE)),
    value!("BVec4", "Vector", "bvec4", "Four true/false values", || ExprWriterEditor::BVec4(BVec4::FALSE)),
    value!("RandFloat", "Random", "f32", "A random number in [0, 1), per particle", || ExprWriterEditor::RandFloat),
    value!("RandVec3", "Random", "vec3", "Three random numbers in [0, 1) (not centred: subtract 0.5 for −0.5..0.5)", || ExprWriterEditor::RandVec3),
    value!("Rand", "Random", "f32 / vec2 / vec3 / vec4", "Random numbers of a float type, each in [0, 1)", || ExprWriterEditor::Rand(ValueTypeDef::Vec2)),
    value!("Age", "Time", "f32", "Seconds the particle has lived", || ExprWriterEditor::Age),
    value!("Time", "Time", "f32", "Seconds of effect simulation time", || ExprWriterEditor::Time),
    value!("DeltaTime", "Time", "f32", "Seconds since the last simulation step (about 1/60)", || ExprWriterEditor::DeltaTime),
    value!("VirtualTime", "Time", "f32", "Seconds of the game's virtual clock (pausable, can run slower or faster)", || ExprWriterEditor::VirtualTime),
    value!("VirtualDeltaTime", "Time", "f32", "Virtual seconds since the last step", || ExprWriterEditor::VirtualDeltaTime),
    value!("RealTime", "Time", "f32", "Real (wall-clock) seconds since the game started; keeps running while paused", || ExprWriterEditor::RealTime),
    value!("RealDeltaTime", "Time", "f32", "Real seconds since the last step", || ExprWriterEditor::RealDeltaTime),
    value!("IsAlive", "Special", "bool", "Whether the particle is still alive (update modifiers only)", || ExprWriterEditor::IsAlive),
    value!("AlphaCutoff", "Special", "f32", "The Mask blend cutoff. Broken in bevy_hanabi 0.19 (the shader fails to compile); not offered in the editor", || ExprWriterEditor::AlphaCutoff),
    value!("Placeholder", "Special", "f32", "An empty slot (reads 0.0)", || ExprWriterEditor::Placeholder),
];

/// [`EXPR_VALUES`] rows the editor menu leaves out because bevy_hanabi 0.19
/// cannot compile them (kept in the table for coverage and loading).
pub const NOT_OFFERED_VALUES: [&str; 1] = ["AlphaCutoff"];

impl Default for ExprWriterEditor {
    fn default() -> Self {
        ExprWriterEditor::Placeholder
    }
}

impl ExprWriterEditor {
    /// Calls `f` on this expression and every sub-expression.
    pub fn visit<'a>(&'a self, f: &mut impl FnMut(&'a ExprWriterEditor)) {
        f(self);
        if let ExprWriterEditor::Operator(op) = self {
            for arg in op.args() {
                arg.visit(f);
            }
        }
    }

    /// Calls `f` on this expression and every sub-expression, parents
    /// first; `f` may replace an expression (its new children are visited).
    pub fn visit_mut(&mut self, f: &mut impl FnMut(&mut ExprWriterEditor)) {
        f(self);
        if let ExprWriterEditor::Operator(op) = self {
            for arg in op.args_mut() {
                arg.visit_mut(f);
            }
        }
    }

    /// Smallest and largest value of a scalar expression, when they are known
    /// without simulating (literals, random values and arithmetic on them).
    pub fn scalar_range(&self) -> Option<(f32, f32)> {
        match self {
            ExprWriterEditor::Placeholder => Some((0.0, 0.0)),
            ExprWriterEditor::Float(v) => Some((*v, *v)),
            ExprWriterEditor::U32(v) => Some((*v as f32, *v as f32)),
            ExprWriterEditor::I32(v) => Some((*v as f32, *v as f32)),
            ExprWriterEditor::RandFloat | ExprWriterEditor::Rand(ValueTypeDef::F32) => Some((0.0, 1.0)),
            ExprWriterEditor::Operator(op) => op.scalar_range(),
            _ => None,
        }
    }
}

impl ExprOperatorEditor {
    /// See [`ExprWriterEditor::scalar_range`].
    pub fn scalar_range(&self) -> Option<(f32, f32)> {
        use ExprOperatorEditor as O;
        let both = |a: &ExprWriterEditor, b: &ExprWriterEditor| Some((a.scalar_range()?, b.scalar_range()?));
        // A function that never decreases maps a range to a range.
        let monotonic = |a: &ExprWriterEditor, f: fn(f32) -> f32| a.scalar_range().map(|(lo, hi)| (f(lo), f(hi)));
        match self {
            O::Uniform(a, b) => {
                let ((a0, a1), (b0, b1)) = both(a, b)?;
                Some((a0.min(b0), a1.max(b1)))
            }
            O::Add(a, b) => {
                let ((a0, a1), (b0, b1)) = both(a, b)?;
                Some((a0 + b0, a1 + b1))
            }
            O::Subtract(a, b) => {
                let ((a0, a1), (b0, b1)) = both(a, b)?;
                Some((a0 - b1, a1 - b0))
            }
            O::Multiply(a, b) => {
                let ((a0, a1), (b0, b1)) = both(a, b)?;
                let products = [a0 * b0, a0 * b1, a1 * b0, a1 * b1];
                Some((
                    products.iter().copied().fold(f32::INFINITY, f32::min),
                    products.iter().copied().fold(f32::NEG_INFINITY, f32::max),
                ))
            }
            O::Divide(a, b) => {
                let ((a0, a1), (b0, b1)) = both(a, b)?;
                // Only when the divisor keeps one sign.
                if b0 <= 0.0 && b1 >= 0.0 {
                    return None;
                }
                let quotients = [a0 / b0, a0 / b1, a1 / b0, a1 / b1];
                Some((
                    quotients.iter().copied().fold(f32::INFINITY, f32::min),
                    quotients.iter().copied().fold(f32::NEG_INFINITY, f32::max),
                ))
            }
            O::Min(a, b) => {
                let ((a0, a1), (b0, b1)) = both(a, b)?;
                Some((a0.min(b0), a1.min(b1)))
            }
            O::Max(a, b) => {
                let ((a0, a1), (b0, b1)) = both(a, b)?;
                Some((a0.max(b0), a1.max(b1)))
            }
            O::Clamp(x, low, high) => {
                let (x0, x1) = x.scalar_range()?;
                let ((l0, l1), (h0, h1)) = both(low, high)?;
                Some((x0.max(l0).min(h0), x1.max(l1).min(h1)))
            }
            O::Mix(a, b, t) => {
                let ((a0, a1), (b0, b1)) = both(a, b)?;
                let (t0, t1) = t.scalar_range()?;
                if t0 < 0.0 || t1 > 1.0 {
                    return None;
                }
                Some((a0.min(b0), a1.max(b1)))
            }
            O::Abs(a) => {
                let (lo, hi) = a.scalar_range()?;
                let low = if lo <= 0.0 && hi >= 0.0 { 0.0 } else { lo.abs().min(hi.abs()) };
                Some((low, lo.abs().max(hi.abs())))
            }
            O::Floor(a) => monotonic(a, f32::floor),
            O::Ceil(a) => monotonic(a, f32::ceil),
            O::Round(a) => monotonic(a, f32::round_ties_even),
            O::Saturate(a) => monotonic(a, |x| x.clamp(0.0, 1.0)),
            O::Exp(a) => monotonic(a, f32::exp),
            O::Exp2(a) => monotonic(a, f32::exp2),
            O::Sqrt(a) => a.scalar_range().filter(|(lo, _)| *lo >= 0.0).map(|(lo, hi)| (lo.sqrt(), hi.sqrt())),
            O::Cast(a, ValueTypeDef::F32) => a.scalar_range(),
            O::Fract(_) => Some((0.0, 1.0)),
            O::Step(..) => Some((0.0, 1.0)),
            O::SmoothStep(..) => Some((0.0, 1.0)),
            O::Sin(_) | O::Cos(_) => Some((-1.0, 1.0)),
            O::Sign(_) => Some((-1.0, 1.0)),
            _ => None,
        }
    }
}
