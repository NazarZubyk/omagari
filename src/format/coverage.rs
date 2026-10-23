//! Which bevy_hanabi features the format exposes, and which it skips (and
//! why). The coverage test (`tests/hanabi_coverage.rs`) checks this against
//! bevy_hanabi itself, so a bevy_hanabi upgrade that adds a modifier or an
//! operator fails the test until it is mapped or listed as skipped.
//!
//! - Operators: the `match`es below are exhaustive (no `_` arm), so a new
//!   variant of bevy_hanabi's operator enums stops this file from compiling.
//! - Modifiers: bevy_hanabi's type registry after its `register_modifiers()`
//!   (plus [`crate::register_missing_hanabi_modifiers`], which adds the one
//!   it forgets, `EmitSpawnEventModifier`) lists every modifier type; each
//!   must be produced by a default Omagari modifier or be in
//!   [`SKIPPED_MODIFIERS`].

use bevy_hanabi::graph::expr::TernaryOperator;
use bevy_hanabi::prelude::*;

/// The Omagari operator (row of [`super::OPERATORS`]) for a bevy_hanabi
/// unary operator.
pub fn unary_operator(op: UnaryOperator) -> &'static str {
    use UnaryOperator as U;
    match op {
        U::Abs => "Abs",
        U::Acos => "Acos",
        U::Asin => "Asin",
        U::Atan => "Atan",
        U::All => "All",
        U::Any => "Any",
        U::Ceil => "Ceil",
        U::Cos => "Cos",
        U::Exp => "Exp",
        U::Exp2 => "Exp2",
        U::Floor => "Floor",
        U::Fract => "Fract",
        U::InvSqrt => "InvSqrt",
        U::Length => "Length",
        U::Log => "Log",
        U::Log2 => "Log2",
        U::Normalize => "Normalized",
        U::Pack4x8snorm => "Pack4x8SNorm",
        U::Pack4x8unorm => "Pack4x8UNorm",
        U::Round => "Round",
        U::Saturate => "Saturate",
        U::Sign => "Sign",
        U::Sin => "Sin",
        U::Sqrt => "Sqrt",
        U::Tan => "Tan",
        U::Unpack4x8snorm => "Unpack4x8SNorm",
        U::Unpack4x8unorm => "Unpack4x8UNorm",
        U::W => "W",
        U::X => "X",
        U::Y => "Y",
        U::Z => "Z",
    }
}

/// Every bevy_hanabi unary operator.
pub const UNARY_OPERATORS: [UnaryOperator; 31] = {
    use UnaryOperator as U;
    [
        U::Abs, U::Acos, U::Asin, U::Atan, U::All, U::Any, U::Ceil, U::Cos, U::Exp, U::Exp2, U::Floor,
        U::Fract, U::InvSqrt, U::Length, U::Log, U::Log2, U::Normalize, U::Pack4x8snorm, U::Pack4x8unorm,
        U::Round, U::Saturate, U::Sign, U::Sin, U::Sqrt, U::Tan, U::Unpack4x8snorm, U::Unpack4x8unorm, U::W,
        U::X, U::Y, U::Z,
    ]
};

/// The Omagari operator for a bevy_hanabi binary operator.
pub fn binary_operator(op: BinaryOperator) -> &'static str {
    use BinaryOperator as B;
    match op {
        B::Add => "Add",
        B::Atan2 => "Atan2",
        B::Cross => "Cross",
        B::Distance => "Distance",
        B::Div => "Divide",
        B::Dot => "Dot",
        B::GreaterThan => "GreaterThan",
        B::GreaterThanOrEqual => "GreaterThanOrEqual",
        B::LessThan => "LessThan",
        B::LessThanOrEqual => "LessThanOrEqual",
        B::Max => "Max",
        B::Min => "Min",
        B::Mul => "Multiply",
        B::Remainder => "Remainder",
        B::Step => "Step",
        B::Sub => "Subtract",
        B::UniformRand => "Uniform",
        B::NormalRand => "Normal",
        B::Vec2 => "Vec2",
        B::Vec4XyzW => "Vec4XyzW",
    }
}

/// Every bevy_hanabi binary operator.
pub const BINARY_OPERATORS: [BinaryOperator; 20] = {
    use BinaryOperator as B;
    [
        B::Add, B::Atan2, B::Cross, B::Distance, B::Div, B::Dot, B::GreaterThan, B::GreaterThanOrEqual,
        B::LessThan, B::LessThanOrEqual, B::Max, B::Min, B::Mul, B::Remainder, B::Step, B::Sub,
        B::UniformRand, B::NormalRand, B::Vec2, B::Vec4XyzW,
    ]
};

/// The Omagari operator for a bevy_hanabi ternary operator.
pub fn ternary_operator(op: TernaryOperator) -> &'static str {
    use TernaryOperator as T;
    match op {
        T::Mix => "Mix",
        T::Clamp => "Clamp",
        T::SmoothStep => "SmoothStep",
        T::Vec3 => "Vec3",
    }
}

/// Every bevy_hanabi ternary operator.
pub const TERNARY_OPERATORS: [TernaryOperator; 4] =
    [TernaryOperator::Mix, TernaryOperator::Clamp, TernaryOperator::SmoothStep, TernaryOperator::Vec3];

/// The Omagari value (row of [`super::EXPR_VALUES`]) for a bevy_hanabi
/// built-in operator. `Rand` of a float type is `Rand(type)` (also
/// `RandFloat` / `RandVec3`); integer and bool types have no shader function
/// in bevy_hanabi 0.19, so `check_project` reports them.
pub fn builtin_operator(op: BuiltInOperator) -> &'static str {
    use BuiltInOperator as O;
    match op {
        O::Time => "Time",
        O::DeltaTime => "DeltaTime",
        O::VirtualTime => "VirtualTime",
        O::VirtualDeltaTime => "VirtualDeltaTime",
        O::RealTime => "RealTime",
        O::RealDeltaTime => "RealDeltaTime",
        O::Rand(_) => "Rand",
        O::AlphaCutoff => "AlphaCutoff",
        O::IsAlive => "IsAlive",
    }
}

/// Every bevy_hanabi built-in operator (`Rand` once).
pub const BUILTIN_OPERATORS: [BuiltInOperator; 9] = {
    use BuiltInOperator as O;
    [
        O::Time,
        O::DeltaTime,
        O::VirtualTime,
        O::VirtualDeltaTime,
        O::RealTime,
        O::RealDeltaTime,
        O::Rand(ValueType::Scalar(ScalarType::Float)),
        O::AlphaCutoff,
        O::IsAlive,
    ]
};

/// bevy_hanabi modifier types (short type name) Omagari does not expose,
/// with the reason. Empty: every registered modifier is exposed.
pub const SKIPPED_MODIFIERS: [(&str, &str); 0] = [];

/// Other bevy_hanabi features (3D) that are not exposed yet, with the
/// reason or the phase that adds them.
pub const SKIPPED_FEATURES: [(&str, &str); 9] = [
    ("Expr::TextureSample", "sampling a texture inside an expression needs extra texture slots; particle layers have one (P3b/P4)"),
    ("matrix literals (mat2..mat4)", "no modifier input takes a matrix and no operator needs one for 3D particles"),
    ("AlphaMode::Mask with an expression cutoff", "the cutoff is a number (f32); an expression adds nothing a render modifier cannot do"),
    ("EffectAsset::mesh (mesh particles)", "needs mesh assets in the asset library (P3b)"),
    ("ribbons (Attribute::RIBBON_ID / PREV / NEXT linking)", "the attributes can be set, but the ribbon preset and editor support come in P3b"),
    ("draw ordering between effects (examples/ordering.rs)", "P3b"),
    ("EffectAsset::z_layer_2d, 2D rendering", "Omagari is 3D only"),
    ("render layers / multi-camera", "not needed by the game (one camera)"),
    ("SizeOverLifetimeModifier::screen_space_size", "bevy_hanabi 0.19 ignores the field; use the ScreenSpaceSize render modifier"),
];

// --- settings and small enums: exhaustive conversions from bevy_hanabi --------

use super::{
    ImageSampleMappingDef, MotionIntegrationDef, OrientModeDef, SimulationConditionDef,
};

/// Omagari's motion integration for bevy_hanabi's (exhaustive).
pub fn motion_integration(m: MotionIntegration) -> MotionIntegrationDef {
    match m {
        MotionIntegration::None => MotionIntegrationDef::None,
        MotionIntegration::PreUpdate => MotionIntegrationDef::PreUpdate,
        MotionIntegration::PostUpdate => MotionIntegrationDef::PostUpdate,
    }
}

/// Omagari's simulation condition for bevy_hanabi's (exhaustive).
pub fn simulation_condition(c: SimulationCondition) -> SimulationConditionDef {
    match c {
        SimulationCondition::WhenVisible => SimulationConditionDef::WhenVisible,
        SimulationCondition::Always => SimulationConditionDef::Always,
    }
}

/// Omagari's orient mode for bevy_hanabi's (exhaustive).
pub fn orient_mode(m: OrientMode) -> OrientModeDef {
    match m {
        OrientMode::ParallelCameraDepthPlane => OrientModeDef::ParallelCameraDepthPlane,
        OrientMode::FaceCameraPosition => OrientModeDef::FaceCameraPosition,
        OrientMode::AlongVelocity => OrientModeDef::AlongVelocity,
    }
}

/// Omagari's texture sampling for bevy_hanabi's (exhaustive).
pub fn image_sample_mapping(m: ImageSampleMapping) -> ImageSampleMappingDef {
    match m {
        ImageSampleMapping::Modulate => ImageSampleMappingDef::Modulate,
        ImageSampleMapping::ModulateRGB => ImageSampleMappingDef::ModulateRGB,
        ImageSampleMapping::ModulateOpacityFromR => ImageSampleMappingDef::ModulateOpacityFromR,
    }
}

/// bevy_hanabi's alpha modes Omagari writes (`AlphaMode` is non-exhaustive,
/// so a new one cannot be caught at compile time; the coverage test lists
/// these six and checks each converts).
pub const ALPHA_MODES: [&str; 6] = ["Blend", "Premultiply", "Add", "Multiply", "Mask", "Opaque"];
