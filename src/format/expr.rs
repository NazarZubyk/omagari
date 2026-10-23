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
    /// A new operator with `Placeholder` arguments.
    pub make: fn() -> ExprOperatorEditor,
}

fn ph() -> ExprWriterEditor {
    ExprWriterEditor::Placeholder
}

/// Every operator, in menu order.
pub const OPERATORS: [OperatorInfo; 18] = [
    OperatorInfo { name: "Add", args: &["a", "b"], hint: "a + b", make: || ExprOperatorEditor::Add(ph(), ph()) },
    OperatorInfo { name: "Subtract", args: &["a", "b"], hint: "a − b", make: || ExprOperatorEditor::Subtract(ph(), ph()) },
    OperatorInfo {
        name: "Multiply",
        args: &["a", "b"],
        hint: "a × b (a vector times a number scales it)",
        make: || ExprOperatorEditor::Multiply(ph(), ph()),
    },
    OperatorInfo {
        name: "Uniform",
        args: &["low", "high"],
        hint: "A random value between a and b, per particle",
        make: || ExprOperatorEditor::Uniform(ph(), ph()),
    },
    OperatorInfo {
        name: "Mix",
        args: &["a", "b", "t"],
        hint: "Blend from a (t = 0) to b (t = 1); t is often a parameter",
        make: || ExprOperatorEditor::Mix(ph(), ph(), ph()),
    },
    OperatorInfo {
        name: "Clamp",
        args: &["x", "low", "high"],
        hint: "x kept between low and high (x, low, high)",
        make: || ExprOperatorEditor::Clamp(ph(), ph(), ph()),
    },
    OperatorInfo {
        name: "SmoothStep",
        args: &["low", "high", "x"],
        hint: "0 below low, 1 above high, a smooth S-curve in between (low, high, x)",
        make: || ExprOperatorEditor::SmoothStep(ph(), ph(), ph()),
    },
    OperatorInfo { name: "Min", args: &["a", "b"], hint: "The smaller of a and b", make: || ExprOperatorEditor::Min(ph(), ph()) },
    OperatorInfo { name: "Max", args: &["a", "b"], hint: "The larger of a and b", make: || ExprOperatorEditor::Max(ph(), ph()) },
    OperatorInfo { name: "Sin", args: &["x"], hint: "Sine (radians): waves between −1 and 1", make: || ExprOperatorEditor::Sin(ph()) },
    OperatorInfo { name: "Cos", args: &["x"], hint: "Cosine (radians): waves between −1 and 1", make: || ExprOperatorEditor::Cos(ph()) },
    OperatorInfo {
        name: "Distance",
        args: &["a", "b"],
        hint: "Distance between two points",
        make: || ExprOperatorEditor::Distance(ph(), ph()),
    },
    OperatorInfo {
        name: "Vec3",
        args: &["x", "y", "z"],
        hint: "A vector from three numbers (x, y, z)",
        make: || ExprOperatorEditor::Vec3(ph(), ph(), ph()),
    },
    OperatorInfo {
        name: "Vec4",
        args: &["x", "y", "z", "w"],
        hint: "A vector / colour from four numbers (r, g, b, a)",
        make: || ExprOperatorEditor::Vec4(ph(), ph(), ph(), ph()),
    },
    OperatorInfo {
        name: "Normalized",
        args: &["v"],
        hint: "The vector made 1 long (its direction)",
        make: || ExprOperatorEditor::Normalized(ph()),
    },
    OperatorInfo {
        name: "Pack4x8UNorm",
        args: &["rgba"],
        hint: "Packs a vec4 colour into the u32 `color` attribute",
        make: || ExprOperatorEditor::Pack4x8UNorm(ph()),
    },
    OperatorInfo {
        name: "Attr",
        args: &[],
        hint: "Reads an attribute of the particle",
        make: || ExprOperatorEditor::Attr(Attribute::ID),
    },
    OperatorInfo {
        name: "ParentAttr",
        args: &[],
        hint: "Reads an attribute of the parent particle (child layers)",
        make: || ExprOperatorEditor::ParentAttr(Attribute::ID),
    },
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
            O::Sin(a) | O::Cos(a) | O::Pack4x8UNorm(a) | O::Normalized(a) => vec![a],
            O::Uniform(a, b)
            | O::Multiply(a, b)
            | O::Subtract(a, b)
            | O::Add(a, b)
            | O::Distance(a, b)
            | O::Min(a, b)
            | O::Max(a, b) => vec![a, b],
            O::Vec3(a, b, c) | O::Mix(a, b, c) | O::Clamp(a, b, c) | O::SmoothStep(a, b, c) => vec![a, b, c],
            O::Vec4(a, b, c, d) => vec![a, b, c, d],
            O::Attr(_) | O::ParentAttr(_) => vec![],
        }
    }

    /// The sub-expressions, mutable.
    pub fn args_mut(&mut self) -> Vec<&mut ExprWriterEditor> {
        use ExprOperatorEditor as O;
        match self {
            O::Sin(a) | O::Cos(a) | O::Pack4x8UNorm(a) | O::Normalized(a) => vec![a],
            O::Uniform(a, b)
            | O::Multiply(a, b)
            | O::Subtract(a, b)
            | O::Add(a, b)
            | O::Distance(a, b)
            | O::Min(a, b)
            | O::Max(a, b) => vec![a, b],
            O::Vec3(a, b, c) | O::Mix(a, b, c) | O::Clamp(a, b, c) | O::SmoothStep(a, b, c) => vec![a, b, c],
            O::Vec4(a, b, c, d) => vec![a, b, c, d],
            O::Attr(_) | O::ParentAttr(_) => vec![],
        }
    }

    pub fn produce(&self, writer: &EffectWriter) -> WriterExpr {
        use ExprOperatorEditor as O;
        let w = |e: &ExprWriterEditor| e.produce(writer);
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
    /// A runtime property (see [`RUNTIME_PROPERTIES`]), e.g.
    /// `Property("segment_a")`.
    Property(String),
    /// A parameter of the package, e.g. `Param("intensity")`: f32 for Float
    /// parameters, vec4 (linear RGBA) for Color ones. Its current (smoothed)
    /// value per instance; init modifiers read it when a particle is born,
    /// update modifiers every frame.
    Param(String),
}

impl ExprWriterEditor {
    pub fn produce(&self, writer: &EffectWriter) -> WriterExpr {
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
        }
    }
}

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
            ExprOperatorEditor::Min(a, b) => {
                let ((a0, a1), (b0, b1)) = both(a, b)?;
                Some((a0.min(b0), a1.min(b1)))
            }
            ExprOperatorEditor::Max(a, b) => {
                let ((a0, a1), (b0, b1)) = both(a, b)?;
                Some((a0.max(b0), a1.max(b1)))
            }
            ExprOperatorEditor::Clamp(x, low, high) => {
                let (x0, x1) = x.scalar_range()?;
                let ((l0, l1), (h0, h1)) = both(low, high)?;
                Some((x0.max(l0).min(h0), x1.max(l1).min(h1)))
            }
            ExprOperatorEditor::Mix(a, b, t) => {
                let ((a0, a1), (b0, b1)) = both(a, b)?;
                let (t0, t1) = t.scalar_range()?;
                if t0 < 0.0 || t1 > 1.0 {
                    return None;
                }
                Some((a0.min(b0), a1.max(b1)))
            }
            ExprOperatorEditor::SmoothStep(..) => Some((0.0, 1.0)),
            ExprOperatorEditor::Sin(_) | ExprOperatorEditor::Cos(_) => Some((-1.0, 1.0)),
            _ => None,
        }
    }
}
