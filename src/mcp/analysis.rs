//! Headless checks and a CPU approximation of an Omagari project.
//!
//! This lets an agent reason about an effect without rendering it: the project
//! is type checked, built and exported like the editor would, and a few hundred
//! particles per effect are simulated on the CPU to produce a short text summary.
//!
//! The simulation is an approximation of what bevy_hanabi does on the GPU. It
//! follows the same modifier order and integration, but assumes 60 updates per
//! second and samples random values on the CPU.

use std::fmt::Write;

use bevy::prelude::*;
use bevy_hanabi::prelude::*;
use rand::{rngs::StdRng, Rng, SeedableRng};

use omagari::{register_missing_hanabi_modifiers, ExportedEffect, ExportedProject};

use crate::controller::OmagariProject;
use crate::effect::*;
use crate::expr::*;
use crate::modifiers::*;

const DT: f32 = 1.0 / 60.0;
/// Longest time a single particle is simulated.
const MAX_PARTICLE_TIME: f32 = 30.0;
/// Longest time the project timeline covers.
const MAX_TIMELINE: f32 = 30.0;
/// Always-emitted spawn events are grouped into chunks of this many updates.
const EVENT_CHUNK: u32 = 6;
/// Half of the vertical extent the editor camera shows at its default distance
/// (camera at (0, 1.5, 5), 45 degree vertical field of view).
const DEFAULT_VIEW_HALF_HEIGHT: f32 = 2.1;

// ---------------------------------------------------------------------------
// Values and expressions
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Val {
    F(f32),
    I(i32),
    U(u32),
    B(bool),
    V2(Vec2),
    V3(Vec3),
    V4(Vec4),
}

impl Val {
    fn type_name(&self) -> &'static str {
        match self {
            Val::F(_) => "f32",
            Val::I(_) => "i32",
            Val::U(_) => "u32",
            Val::B(_) => "bool",
            Val::V2(_) => "vec2",
            Val::V3(_) => "vec3",
            Val::V4(_) => "vec4",
        }
    }

    fn same_type(&self, other: &Val) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }

    /// Float scalar or vector as (component count, components).
    fn floats(&self) -> Option<(usize, [f32; 4])> {
        match *self {
            Val::F(x) => Some((1, [x, 0.0, 0.0, 0.0])),
            Val::V2(v) => Some((2, [v.x, v.y, 0.0, 0.0])),
            Val::V3(v) => Some((3, [v.x, v.y, v.z, 0.0])),
            Val::V4(v) => Some((4, v.to_array())),
            _ => None,
        }
    }

    fn from_floats(n: usize, c: [f32; 4]) -> Val {
        match n {
            1 => Val::F(c[0]),
            2 => Val::V2(Vec2::new(c[0], c[1])),
            3 => Val::V3(Vec3::new(c[0], c[1], c[2])),
            _ => Val::V4(Vec4::from_array(c)),
        }
    }

    fn map_floats(&self, f: impl Fn(f32) -> f32) -> Option<Val> {
        let (n, mut c) = self.floats()?;
        for x in c.iter_mut().take(n) {
            *x = f(*x);
        }
        Some(Val::from_floats(n, c))
    }

    fn zero_of(value_type: ValueType) -> Val {
        match value_type {
            ValueType::Scalar(ScalarType::Float) => Val::F(0.0),
            ValueType::Scalar(ScalarType::Int) => Val::I(0),
            ValueType::Scalar(ScalarType::Uint) => Val::U(0),
            ValueType::Scalar(ScalarType::Bool) => Val::B(false),
            ValueType::Vector(v) => match v.count() {
                2 => Val::V2(Vec2::ZERO),
                3 => Val::V3(Vec3::ZERO),
                _ => Val::V4(Vec4::ZERO),
            },
            _ => Val::F(0.0),
        }
    }
}

/// Default value of an attribute that was never written, as in bevy_hanabi.
fn attr_default(attr: Attribute) -> Val {
    if attr == Attribute::LIFETIME || attr == Attribute::SIZE || attr == Attribute::ALPHA {
        Val::F(1.0)
    } else if attr == Attribute::COLOR || attr == Attribute::PREV || attr == Attribute::NEXT {
        Val::U(u32::MAX)
    } else if attr == Attribute::HDR_COLOR {
        Val::V4(Vec4::ONE)
    } else if attr == Attribute::SIZE2 {
        Val::V2(Vec2::ONE)
    } else if attr == Attribute::SIZE3 {
        Val::V3(Vec3::ONE)
    } else if attr == Attribute::AXIS_X {
        Val::V3(Vec3::X)
    } else if attr == Attribute::AXIS_Y {
        Val::V3(Vec3::Y)
    } else if attr == Attribute::AXIS_Z {
        Val::V3(Vec3::Z)
    } else {
        Val::zero_of(attr.value_type())
    }
}

/// Small attribute map; particles only ever hold a handful of attributes.
#[derive(Clone, Default)]
struct Attrs(Vec<(Attribute, Val)>);

impl Attrs {
    fn get(&self, attr: Attribute) -> Val {
        self.0
            .iter()
            .find(|(a, _)| *a == attr)
            .map(|(_, v)| *v)
            .unwrap_or_else(|| attr_default(attr))
    }

    fn set(&mut self, attr: Attribute, value: Val) {
        if let Some(slot) = self.0.iter_mut().find(|(a, _)| *a == attr) {
            slot.1 = value;
        } else {
            self.0.push((attr, value));
        }
    }

    fn f32(&self, attr: Attribute) -> f32 {
        match self.get(attr) {
            Val::F(x) => x,
            _ => 0.0,
        }
    }

    fn vec3(&self, attr: Attribute) -> Vec3 {
        match self.get(attr) {
            Val::V3(v) => v,
            _ => Vec3::ZERO,
        }
    }
}

struct Env<'a> {
    rng: &'a mut StdRng,
    attrs: &'a Attrs,
    parent: Option<&'a Attrs>,
    time: f32,
}

#[derive(Clone, Copy)]
enum Arith {
    Add,
    Sub,
    Mul,
}

fn arith(op: Arith, a: Val, b: Val) -> Result<Val, String> {
    let name = match op {
        Arith::Add => "Add",
        Arith::Sub => "Subtract",
        Arith::Mul => "Multiply",
    };
    let f = |x: f32, y: f32| match op {
        Arith::Add => x + y,
        Arith::Sub => x - y,
        Arith::Mul => x * y,
    };
    match (a, b) {
        (Val::U(x), Val::U(y)) => {
            return Ok(Val::U(match op {
                Arith::Add => x.wrapping_add(y),
                Arith::Sub => x.wrapping_sub(y),
                Arith::Mul => x.wrapping_mul(y),
            }))
        }
        (Val::I(x), Val::I(y)) => {
            return Ok(Val::I(match op {
                Arith::Add => x.wrapping_add(y),
                Arith::Sub => x.wrapping_sub(y),
                Arith::Mul => x.wrapping_mul(y),
            }))
        }
        _ => {}
    }
    match (a.floats(), b.floats()) {
        (Some((na, ca)), Some((nb, cb))) if na == nb || na == 1 || nb == 1 => {
            let n = na.max(nb);
            let mut out = [0.0; 4];
            for i in 0..n {
                let x = if na == 1 { ca[0] } else { ca[i] };
                let y = if nb == 1 { cb[0] } else { cb[i] };
                out[i] = f(x, y);
            }
            Ok(Val::from_floats(n, out))
        }
        _ => Err(format!(
            "{} cannot combine {} and {} (only same types, or a vector with an f32)",
            name,
            a.type_name(),
            b.type_name()
        )),
    }
}

fn pack4x8unorm(v: Vec4) -> u32 {
    let b = |x: f32| ((x.clamp(0.0, 1.0) * 255.0).round() as u32) & 0xFF;
    b(v.x) | (b(v.y) << 8) | (b(v.z) << 16) | (b(v.w) << 24)
}

fn unpack4x8unorm(u: u32) -> Vec4 {
    let b = |s: u32| ((u >> s) & 0xFF) as f32 / 255.0;
    Vec4::new(b(0), b(8), b(16), b(24))
}

fn eval(e: &ExprWriterEditor, env: &mut Env) -> Result<Val, String> {
    Ok(match e {
        ExprWriterEditor::Placeholder => Val::F(0.0),
        ExprWriterEditor::RandU32 => Val::U(env.rng.random()),
        ExprWriterEditor::RandFloat => Val::F(env.rng.random()),
        ExprWriterEditor::RandVec3 => Val::V3(Vec3::new(
            env.rng.random(),
            env.rng.random(),
            env.rng.random(),
        )),
        ExprWriterEditor::Float(x) => Val::F(*x),
        ExprWriterEditor::U32(x) => Val::U(*x),
        ExprWriterEditor::Vec3(v) => Val::V3(*v),
        ExprWriterEditor::Vec4(v) => Val::V4(*v),
        ExprWriterEditor::Time => Val::F(env.time),
        ExprWriterEditor::Age => env.attrs.get(Attribute::AGE),
        ExprWriterEditor::Operator(op) => eval_op(op, env)?,
    })
}

fn eval_op(op: &ExprOperatorEditor, env: &mut Env) -> Result<Val, String> {
    use ExprOperatorEditor as O;
    Ok(match op {
        O::Add(a, b) => arith(Arith::Add, eval(a, env)?, eval(b, env)?)?,
        O::Subtract(a, b) => arith(Arith::Sub, eval(a, env)?, eval(b, env)?)?,
        O::Multiply(a, b) => arith(Arith::Mul, eval(a, env)?, eval(b, env)?)?,
        O::Uniform(a, b) => {
            let (a, b) = (eval(a, env)?, eval(b, env)?);
            match (a.floats(), b.floats()) {
                (Some((na, ca)), Some((nb, cb))) if na == nb => {
                    let mut out = [0.0; 4];
                    for i in 0..na {
                        let t: f32 = env.rng.random();
                        out[i] = ca[i] + t * (cb[i] - ca[i]);
                    }
                    Val::from_floats(na, out)
                }
                _ => {
                    return Err(format!(
                        "Uniform needs two values of the same float type (f32, vec2, vec3 or vec4), got {} and {}",
                        a.type_name(),
                        b.type_name()
                    ))
                }
            }
        }
        O::Sin(a) => {
            let a = eval(a, env)?;
            a.map_floats(f32::sin)
                .ok_or_else(|| format!("Sin needs a float value, got {}", a.type_name()))?
        }
        O::Cos(a) => {
            let a = eval(a, env)?;
            a.map_floats(f32::cos)
                .ok_or_else(|| format!("Cos needs a float value, got {}", a.type_name()))?
        }
        O::Distance(a, b) => {
            let (a, b) = (eval(a, env)?, eval(b, env)?);
            match (a.floats(), b.floats()) {
                (Some((na, ca)), Some((nb, cb))) if na == nb => {
                    let d2: f32 = (0..na).map(|i| (ca[i] - cb[i]).powi(2)).sum();
                    Val::F(d2.sqrt())
                }
                _ => {
                    return Err(format!(
                        "Distance needs two values of the same float type, got {} and {}",
                        a.type_name(),
                        b.type_name()
                    ))
                }
            }
        }
        O::Vec3(x, y, z) => {
            let c = [eval(x, env)?, eval(y, env)?, eval(z, env)?];
            match c {
                [Val::F(x), Val::F(y), Val::F(z)] => Val::V3(Vec3::new(x, y, z)),
                _ => {
                    return Err(format!(
                        "Vec3 operator needs three f32 values, got ({}, {}, {})",
                        c[0].type_name(),
                        c[1].type_name(),
                        c[2].type_name()
                    ))
                }
            }
        }
        O::Vec4(x, y, z, w) => {
            let c = [eval(x, env)?, eval(y, env)?, eval(z, env)?, eval(w, env)?];
            match c {
                [Val::F(x), Val::F(y), Val::F(z), Val::F(w)] => Val::V4(Vec4::new(x, y, z, w)),
                _ => {
                    return Err(format!(
                        "Vec4 operator needs four f32 values, got ({}, {}, {}, {})",
                        c[0].type_name(),
                        c[1].type_name(),
                        c[2].type_name(),
                        c[3].type_name()
                    ))
                }
            }
        }
        O::Pack4x8UNorm(a) => match eval(a, env)? {
            Val::V4(v) => Val::U(pack4x8unorm(v)),
            other => {
                return Err(format!(
                    "Pack4x8UNorm needs a vec4, got {}",
                    other.type_name()
                ))
            }
        },
        O::Normalized(a) => {
            let a = eval(a, env)?;
            match a.floats() {
                Some((n, c)) if n > 1 => {
                    let len = (0..n).map(|i| c[i] * c[i]).sum::<f32>().sqrt();
                    let inv = if len > 0.0 { 1.0 / len } else { f32::NAN };
                    a.map_floats(|x| x * inv).unwrap()
                }
                _ => return Err(format!("Normalized needs a vector, got {}", a.type_name())),
            }
        }
        O::Attr(attr) => env.attrs.get(*attr),
        O::ParentAttr(attr) => match env.parent {
            Some(parent) => parent.get(*attr),
            None => {
                return Err(format!(
                    "ParentAttr(\"{}\") is used, but this effect has no parent",
                    attr.name()
                ))
            }
        },
    })
}

fn want_f32(v: Val, what: &str) -> Result<f32, String> {
    match v {
        Val::F(x) => Ok(x),
        other => Err(format!("{} must be f32, got {}", what, other.type_name())),
    }
}

fn want_vec3(v: Val, what: &str) -> Result<Vec3, String> {
    match v {
        Val::V3(x) => Ok(x),
        other => Err(format!("{} must be vec3, got {}", what, other.type_name())),
    }
}

fn want_u32(v: Val, what: &str) -> Result<u32, String> {
    match v {
        Val::U(x) => Ok(x),
        other => Err(format!("{} must be u32, got {}", what, other.type_name())),
    }
}

fn attr_type_hint(attr: Attribute, got: Val) -> &'static str {
    if attr == Attribute::COLOR && matches!(got, Val::V4(_)) {
        " (color is a packed u32: wrap the vec4 in Pack4x8UNorm, or set hdr_color instead)"
    } else if attr == Attribute::HDR_COLOR && matches!(got, Val::U(_)) {
        " (hdr_color is a vec4; set color for a packed u32)"
    } else if attr == Attribute::SIZE && matches!(got, Val::V3(_)) {
        " (size is a single f32; use size3 for a per-axis vec3)"
    } else {
        ""
    }
}

fn random_unit(rng: &mut StdRng) -> Vec3 {
    loop {
        let v = Vec3::new(
            rng.random::<f32>() * 2.0 - 1.0,
            rng.random::<f32>() * 2.0 - 1.0,
            rng.random::<f32>() * 2.0 - 1.0,
        );
        let len2 = v.length_squared();
        if len2 > 1e-4 && len2 <= 1.0 {
            return v / len2.sqrt();
        }
    }
}

fn safe_normalize(v: Vec3) -> Vec3 {
    let len = v.length();
    if len > 0.0 { v / len } else { Vec3::NAN }
}

// ---------------------------------------------------------------------------
// Modifiers
// ---------------------------------------------------------------------------

fn modifier_name(m: &ModifierEditor) -> &'static str {
    match m {
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

fn is_init_modifier(m: &ModifierEditor) -> bool {
    matches!(
        m,
        ModifierEditor::SetAttribute(_)
            | ModifierEditor::InheritAttribute(_)
            | ModifierEditor::SetPositionCircle(_)
            | ModifierEditor::SetPositionSphere(_)
            | ModifierEditor::SetVelocityCircle(_)
            | ModifierEditor::SetVelocitySphere(_)
            | ModifierEditor::SetVelocityTangent(_)
    )
}

/// Applies an init modifier to a newborn particle.
fn apply_init(
    m: &ModifierEditor,
    p: &mut Attrs,
    parent: Option<&Attrs>,
    rng: &mut StdRng,
    time: f32,
) -> Result<(), String> {
    macro_rules! ev {
        ($e:expr) => {
            eval(
                $e,
                &mut Env {
                    rng: &mut *rng,
                    attrs: &*p,
                    parent,
                    time,
                },
            )?
        };
    }
    match m {
        ModifierEditor::SetAttribute(m) => {
            let v = ev!(&m.attr_expr);
            let expected = attr_default(m.attr);
            if !v.same_type(&expected) {
                return Err(format!(
                    "SetAttribute(\"{}\") needs {}, got {}{}",
                    m.attr.name(),
                    expected.type_name(),
                    v.type_name(),
                    attr_type_hint(m.attr, v)
                ));
            }
            p.set(m.attr, v);
        }
        ModifierEditor::InheritAttribute(m) => match parent {
            Some(parent) => p.set(m.attr, parent.get(m.attr)),
            None => {
                return Err(format!(
                    "InheritAttribute(\"{}\") is used, but this effect has no parent",
                    m.attr.name()
                ))
            }
        },
        ModifierEditor::SetPositionCircle(m) => {
            let center = want_vec3(ev!(&m.center_expr), "SetPositionCircle center")?;
            let axis = want_vec3(ev!(&m.axis_expr), "SetPositionCircle axis")?;
            let radius = want_f32(ev!(&m.radius_expr), "SetPositionCircle radius")?;
            let (u, v) = safe_normalize(axis).any_orthonormal_pair();
            let angle = rng.random::<f32>() * std::f32::consts::TAU;
            let dist = match m.dimension {
                ShapeDimension::Surface => radius,
                ShapeDimension::Volume => radius * rng.random::<f32>().sqrt(),
            };
            p.set(
                Attribute::POSITION,
                Val::V3(center + (u * angle.cos() + v * angle.sin()) * dist),
            );
        }
        ModifierEditor::SetPositionSphere(m) => {
            let center = want_vec3(ev!(&m.center_expr), "SetPositionSphere center")?;
            let radius = want_f32(ev!(&m.radius_expr), "SetPositionSphere radius")?;
            let dist = match m.dimension {
                ShapeDimension::Surface => radius,
                ShapeDimension::Volume => radius * rng.random::<f32>().cbrt(),
            };
            p.set(
                Attribute::POSITION,
                Val::V3(center + random_unit(rng) * dist),
            );
        }
        ModifierEditor::SetVelocityCircle(m) => {
            let center = want_vec3(ev!(&m.center_expr), "SetVelocityCircle center")?;
            let axis = want_vec3(ev!(&m.axis_expr), "SetVelocityCircle axis")?;
            let speed = want_f32(ev!(&m.speed_expr), "SetVelocityCircle speed")?;
            let axis = safe_normalize(axis);
            let delta = p.vec3(Attribute::POSITION) - center;
            let radial = safe_normalize(delta - axis * delta.dot(axis));
            p.set(Attribute::VELOCITY, Val::V3(radial * speed));
        }
        ModifierEditor::SetVelocitySphere(m) => {
            let center = want_vec3(ev!(&m.center_expr), "SetVelocitySphere center")?;
            let speed = want_f32(ev!(&m.speed_expr), "SetVelocitySphere speed")?;
            let dir = safe_normalize(p.vec3(Attribute::POSITION) - center);
            p.set(Attribute::VELOCITY, Val::V3(dir * speed));
        }
        ModifierEditor::SetVelocityTangent(m) => {
            let origin = want_vec3(ev!(&m.origin_expr), "SetVelocityTangent origin")?;
            let axis = want_vec3(ev!(&m.axis_expr), "SetVelocityTangent axis")?;
            let speed = want_f32(ev!(&m.speed_expr), "SetVelocityTangent speed")?;
            let dir = safe_normalize(axis.cross(p.vec3(Attribute::POSITION) - origin));
            p.set(Attribute::VELOCITY, Val::V3(dir * speed));
        }
        // Update modifiers are dropped by the editor when listed in `init_modifiers`.
        _ => {}
    }
    Ok(())
}

/// Spawn events a particle emitted during one update.
struct Emission {
    child_index: u32,
    count: u32,
}

/// Applies an update modifier for one simulation step.
fn apply_update(
    m: &ModifierEditor,
    p: &mut Attrs,
    parent: Option<&Attrs>,
    rng: &mut StdRng,
    time: f32,
    was_alive: bool,
    is_alive: bool,
    emissions: &mut Vec<Emission>,
) -> Result<(), String> {
    macro_rules! ev {
        ($e:expr) => {
            eval(
                $e,
                &mut Env {
                    rng: &mut *rng,
                    attrs: &*p,
                    parent,
                    time,
                },
            )?
        };
    }
    match m {
        ModifierEditor::AccelModifier(m) => {
            let accel = want_vec3(ev!(&m.accel_expr), "AccelModifier accel")?;
            let v = p.vec3(Attribute::VELOCITY) + accel * DT;
            p.set(Attribute::VELOCITY, Val::V3(v));
        }
        ModifierEditor::LinearDragModifier(m) => {
            let drag = want_f32(ev!(&m.drag_expr), "LinearDragModifier drag")?;
            let v = p.vec3(Attribute::VELOCITY) * (1.0 - drag * DT).max(0.0);
            p.set(Attribute::VELOCITY, Val::V3(v));
        }
        ModifierEditor::EmitSpawnEventModifier(m) => {
            let count = want_u32(ev!(&m.count_expr), "EmitSpawnEvent count")?;
            let fire = match m.condition {
                EventEmitCondition::Always => is_alive,
                EventEmitCondition::OnDie => was_alive && !is_alive,
            };
            if fire && count > 0 {
                emissions.push(Emission {
                    child_index: m.child_index,
                    count,
                });
            }
        }
        ModifierEditor::ConformToSphereModifier(m) => {
            let origin = want_vec3(ev!(&m.origin_expr), "ConformToSphere origin")?;
            let radius = want_f32(ev!(&m.radius_expr), "ConformToSphere radius")?;
            let influence = want_f32(ev!(&m.influence_dist_expr), "ConformToSphere influence_dist")?;
            let accel = want_f32(ev!(&m.attraction_accel_expr), "ConformToSphere attraction_accel")?;
            let max_speed = want_f32(
                ev!(&m.max_attraction_speed_expr),
                "ConformToSphere max_attraction_speed",
            )?;
            let delta = origin - p.vec3(Attribute::POSITION);
            let dist = delta.length();
            if dist > 0.0 && dist < influence {
                let to_center = delta / dist;
                let toward_surface = if dist > radius { to_center } else { -to_center };
                let mut v = p.vec3(Attribute::VELOCITY) + toward_surface * accel * DT;
                if v.length() > max_speed {
                    v = v.normalize() * max_speed;
                }
                p.set(Attribute::VELOCITY, Val::V3(v));
            }
        }
        // Init modifiers are dropped by the editor when listed in `update_modifiers`.
        _ => {}
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Checks
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct Diagnostics {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

impl Diagnostics {
    fn error(&mut self, effect: &str, msg: impl AsRef<str>) {
        self.errors.push(format!("[{}] {}", effect, msg.as_ref()));
    }
    fn warn(&mut self, effect: &str, msg: impl AsRef<str>) {
        self.warnings.push(format!("[{}] {}", effect, msg.as_ref()));
    }

    pub fn render(&self) -> String {
        let mut out = String::new();
        if self.errors.is_empty() {
            out.push_str("OK: no errors.\n");
        } else {
            let _ = writeln!(out, "ERRORS ({}):", self.errors.len());
            for e in &self.errors {
                let _ = writeln!(out, "- {}", e);
            }
        }
        if !self.warnings.is_empty() {
            let _ = writeln!(out, "WARNINGS ({}):", self.warnings.len());
            for w in &self.warnings {
                let _ = writeln!(out, "- {}", w);
            }
        }
        out
    }
}

fn expr_has_placeholder(e: &ExprWriterEditor) -> bool {
    use ExprOperatorEditor as O;
    match e {
        ExprWriterEditor::Placeholder => true,
        ExprWriterEditor::Operator(op) => match op.as_ref() {
            O::Uniform(a, b) | O::Multiply(a, b) | O::Subtract(a, b) | O::Add(a, b) | O::Distance(a, b) => {
                expr_has_placeholder(a) || expr_has_placeholder(b)
            }
            O::Sin(a) | O::Cos(a) | O::Pack4x8UNorm(a) | O::Normalized(a) => expr_has_placeholder(a),
            O::Vec3(a, b, c) => [a, b, c].iter().any(|x| expr_has_placeholder(x)),
            O::Vec4(a, b, c, d) => [a, b, c, d].iter().any(|x| expr_has_placeholder(x)),
            O::Attr(_) | O::ParentAttr(_) => false,
        },
        _ => false,
    }
}

fn modifier_exprs(m: &ModifierEditor) -> Vec<&ExprWriterEditor> {
    match m {
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
    }
}

fn emit_modifiers(effect: &EffectEditor) -> impl Iterator<Item = &EmitSpawnEventModifierEditor> {
    effect.update_modifiers.iter().filter_map(|m| match m {
        ModifierEditor::EmitSpawnEventModifier(m) => Some(m),
        _ => None,
    })
}

/// Indices of the effects whose `parent` names `effects[index]`, in list order.
/// This order is the `child_index` used by `EmitSpawnEventModifier`.
fn children_of(effects: &[EffectEditor], index: usize) -> Vec<usize> {
    let name = &effects[index].name;
    effects
        .iter()
        .enumerate()
        .filter(|(i, e)| *i != index && e.parent.as_deref() == Some(name.as_str()))
        .map(|(i, _)| i)
        .collect()
}

/// Index of the parent effect, if it exists and is listed before the child.
fn parent_index(effects: &[EffectEditor], index: usize) -> Option<usize> {
    let parent = effects[index].parent.as_deref()?;
    effects[..index].iter().position(|e| e.name == parent)
}

/// Builds the type registry the editor uses for exporting.
pub fn type_registry() -> AppTypeRegistry {
    let registry = AppTypeRegistry::new_with_derived_types();
    bevy_hanabi::register_modifiers(&registry);
    register_missing_hanabi_modifiers(&registry);
    registry
}

/// Builds the `*.hanabi.ron` export of a project.
pub fn export_string(project: &OmagariProject, registry: &AppTypeRegistry) -> Result<String, String> {
    let mut to_export = ExportedProject::default();
    for effect in project.effects.iter() {
        to_export.effects.push(ExportedEffect {
            name: effect.name().to_string(),
            parent: effect.parent(),
            texture_index: effect.texture_index(),
            effect_asset: effect.produce(),
        });
    }
    to_export
        .to_ron_string(&registry.read())
        .map_err(|e| format!("export failed: {}", e))
}

/// Runs every static check on a project: structure, expression types, the
/// bevy_hanabi shader generation and an export round trip.
pub fn check_project(project: &OmagariProject, registry: &AppTypeRegistry) -> Diagnostics {
    let mut d = Diagnostics::default();
    let effects = &project.effects;

    if effects.is_empty() {
        d.errors.push("project has no effects".into());
        return d;
    }

    for (i, effect) in effects.iter().enumerate() {
        let name = effect.name.as_str();

        if effects[..i].iter().any(|e| e.name == effect.name) {
            d.error(name, "duplicate effect name; names must be unique (parents are found by name)");
        }
        if effect.capacity == 0 {
            d.error(name, "capacity is 0, nothing can spawn");
        }
        match effect.texture_index {
            Some(t) if t >= PARTICLE_TEXTURES.len() => d.error(
                name,
                format!("texture_index {} is out of range (0..={})", t, PARTICLE_TEXTURES.len() - 1),
            ),
            None => d.warn(name, "texture_index is None; the editor previews it with texture 0 but exported games spawn it untextured"),
            _ => {}
        }

        if let Some(parent) = &effect.parent {
            if parent == name {
                d.error(name, "effect is its own parent");
            } else if parent_index(effects, i).is_none() {
                if effects.iter().any(|e| &e.name == parent) {
                    d.error(name, format!("parent \"{}\" must be listed before this effect", parent));
                } else {
                    d.error(name, format!("parent \"{}\" does not exist", parent));
                }
            } else {
                let pi = parent_index(effects, i).unwrap();
                let child_index = children_of(effects, pi).iter().position(|c| *c == i).unwrap() as u32;
                if !emit_modifiers(&effects[pi]).any(|m| m.child_index == child_index) {
                    d.error(
                        name,
                        format!(
                            "parent \"{}\" has no EmitSpawnEventModifier with child_index {}, so this effect never spawns",
                            parent, child_index
                        ),
                    );
                }
            }
        } else {
            let range = effect.spawner_settings.count().range();
            if range[1] <= 0.0 {
                d.warn(name, "spawner count is 0, nothing spawns");
            }
            if !effect.spawner_settings.starts_active() {
                d.warn(name, "spawner starts_active is false; game code must activate it");
            }
            if effect.spawner_settings.cycle_count() == 0
                && effect.spawner_settings.period().range()[1] <= 0.0
                && effect.spawner_settings.spawn_duration().range()[1] <= 0.0
            {
                d.warn(name, "spawner repeats forever with period 0 and duration 0: it bursts `count` particles every frame");
            }
        }

        let n_children = children_of(effects, i).len() as u32;
        for m in emit_modifiers(effect) {
            if m.child_index >= n_children {
                d.error(
                    name,
                    format!(
                        "EmitSpawnEventModifier child_index {} but this effect has {} child effect(s)",
                        m.child_index, n_children
                    ),
                );
            }
        }

        for m in effect.init_modifiers.iter() {
            if !is_init_modifier(m) {
                d.error(name, format!("{} is an update modifier but is listed in init_modifiers; the editor drops it", modifier_name(m)));
            }
        }
        for m in effect.update_modifiers.iter() {
            if is_init_modifier(m) {
                d.error(name, format!("{} is an init modifier but is listed in update_modifiers; the editor drops it", modifier_name(m)));
            }
        }
        for m in effect.init_modifiers.iter().chain(effect.update_modifiers.iter()) {
            if modifier_exprs(m).into_iter().any(expr_has_placeholder) {
                d.warn(name, format!("{} has an empty Placeholder expression (evaluates to f32 0.0)", modifier_name(m)));
            }
        }

        let errors_before = d.errors.len();

        // Type check with one dry-run particle. Expressions have no branches, so
        // one evaluation visits every node.
        let mut rng = StdRng::seed_from_u64(0);
        let parent_attrs = effect.parent.as_ref().map(|_| Attrs::default());
        let mut p = Attrs::default();
        for m in effect.init_modifiers.iter() {
            if let Err(e) = apply_init(m, &mut p, parent_attrs.as_ref(), &mut rng, 0.0) {
                d.error(name, format!("init {}: {}", modifier_name(m), e));
            }
        }
        let mut emissions = Vec::new();
        for m in effect.update_modifiers.iter() {
            if let Err(e) = apply_update(m, &mut p, parent_attrs.as_ref(), &mut rng, 0.0, true, true, &mut emissions) {
                d.error(name, format!("update {}: {}", modifier_name(m), e));
            }
        }

        // Let bevy_hanabi generate the shaders, which catches missing attributes
        // and expression errors that would otherwise only show up at runtime.
        // Skipped when the checks above already failed, to avoid duplicates.
        let asset = effect.produce();
        if d.errors.len() == errors_before {
            let parent_layout = parent_index(effects, i).map(|pi| effects[pi].produce().particle_layout());
            if let Err(e) = EffectShaderSources::generate(&asset, parent_layout.as_ref(), n_children) {
                d.error(name, format!("bevy_hanabi shader generation failed: {}", e));
            }
        }
        let sets_velocity = effect.init_modifiers.iter().any(|m| match m {
            ModifierEditor::SetAttribute(m) => m.attr == Attribute::VELOCITY,
            ModifierEditor::InheritAttribute(m) => m.attr == Attribute::VELOCITY,
            ModifierEditor::SetVelocityCircle(_)
            | ModifierEditor::SetVelocitySphere(_)
            | ModifierEditor::SetVelocityTangent(_) => true,
            _ => false,
        });
        let accelerates = effect
            .update_modifiers
            .iter()
            .any(|m| matches!(m, ModifierEditor::AccelModifier(_) | ModifierEditor::ConformToSphereModifier(_)));
        if !sets_velocity && !accelerates {
            d.warn(name, "nothing sets a velocity; the editor orients particles along velocity, so motionless particles may render invisible");
        }
        let layout = asset.particle_layout();
        if !layout.contains(Attribute::AGE) {
            d.warn(name, "particles never die: nothing uses the age attribute (set age to 0.0 in init, or add ColorOverLifetime/SizeOverLifetime)");
        }
    }

    match export_string(project, registry) {
        Ok(s) => {
            if let Err(e) = ExportedProject::from_ron_bytes(s.as_bytes(), &registry.read()) {
                d.errors.push(format!("exported file does not load back: {}", e));
            }
        }
        Err(e) => d.errors.push(e),
    }

    d
}

// ---------------------------------------------------------------------------
// Simulation
// ---------------------------------------------------------------------------

/// What happened to one sampled particle.
struct Sample {
    birth: f32,
    death: Option<f32>,
    lifetime_attr: f32,
    init_speed: f32,
    birth_pos: Vec3,
    end_pos: Vec3,
    /// Positions every few updates, for extents.
    path: Vec<Vec3>,
    /// Spawn events: (time since birth, child_index, count, particle state).
    events: Vec<(f32, u32, u32, Attrs)>,
    init: Attrs,
    zero_velocity: bool,
    nan: bool,
}

struct EffectSim {
    samples: Vec<Sample>,
    /// Expected particle births per timeline bin.
    births: Vec<f32>,
    /// Expected alive particles per timeline bin.
    alive: Vec<f32>,
    forever: bool,
    spawn_text: String,
    error: Option<String>,
}

fn bins(horizon: f32) -> usize {
    (horizon / DT).ceil() as usize + 1
}

/// Expected births per bin for a CPU spawner, plus a description and whether it
/// repeats forever.
fn spawner_schedule(settings: &SpawnerSettings, horizon: f32) -> (Vec<f32>, String, bool) {
    let count = {
        let r = settings.count().range();
        (r[0] + r[1]) * 0.5
    };
    let duration = settings.spawn_duration().range()[0].max(0.0);
    let period = settings.period().range()[0].max(0.0);
    let cycles = settings.cycle_count();
    let forever = cycles == 0;
    let cycle_len = if cycles == 1 { duration } else { period.max(duration) };

    let mut births = vec![0.0; bins(horizon)];
    let mut start = 0.0;
    let mut k = 0;
    while (forever || k < cycles) && start <= horizon {
        if duration <= 0.0 {
            let b = (start / DT) as usize;
            if b < births.len() {
                births[b] += count;
            }
        } else {
            let rate = count / duration;
            let mut t = start;
            while t < start + duration && t <= horizon {
                let b = (t / DT) as usize;
                if b < births.len() {
                    births[b] += rate * DT.min(start + duration - t);
                }
                t += DT;
            }
        }
        k += 1;
        if cycle_len <= 0.0 {
            if forever {
                // Bursts every frame.
                start += DT;
            } else {
                break;
            }
        } else {
            start += cycle_len;
        }
    }

    let repeat = match cycles {
        0 => "repeats forever".to_string(),
        1 => "once".to_string(),
        n => format!("{} cycles", n),
    };
    let text = if duration <= 0.0 {
        if cycles == 1 {
            format!("burst of {} at t=0, once", fmt_num(count))
        } else {
            format!("burst of {} every {}s, {}", fmt_num(count), fmt_num(cycle_len), repeat)
        }
    } else if cycles != 1 && period <= duration {
        format!("steady stream {}/s, {}", fmt_num(count / duration), repeat)
    } else if cycles == 1 {
        format!("{} particles over {}s ({}/s), once", fmt_num(count), fmt_num(duration), fmt_num(count / duration))
    } else {
        format!(
            "{} particles over {}s ({}/s), then rest; cycle {}s, {}",
            fmt_num(count),
            fmt_num(duration),
            fmt_num(count / duration),
            fmt_num(cycle_len),
            repeat
        )
    };
    (births, text, forever)
}

fn simulate_particle(
    effect: &EffectEditor,
    mortal: bool,
    parent: Option<&Attrs>,
    birth: f32,
    max_time: f32,
    rng: &mut StdRng,
) -> Result<Sample, String> {
    let mut p = Attrs::default();
    for m in effect.init_modifiers.iter() {
        apply_init(m, &mut p, parent, rng, birth)?;
    }
    let init = p.clone();
    let lifetime_attr = p.f32(Attribute::LIFETIME);
    let birth_pos = p.vec3(Attribute::POSITION);
    let init_speed = p.vec3(Attribute::VELOCITY).length();

    let mut sample = Sample {
        birth,
        death: None,
        lifetime_attr,
        init_speed,
        birth_pos,
        end_pos: birth_pos,
        path: vec![birth_pos],
        events: Vec::new(),
        init,
        zero_velocity: false,
        nan: false,
    };

    let mut age = p.f32(Attribute::AGE);
    let mut emissions = Vec::new();
    let mut pending_always: Vec<(u32, u32)> = Vec::new();
    let mut step: u32 = 0;
    let steps = (max_time / DT).ceil() as u32;
    while step < steps {
        let was_alive = !mortal || age < lifetime_attr;
        age += DT;
        p.set(Attribute::AGE, Val::F(age));
        let is_alive = !mortal || age < lifetime_attr;
        let time = birth + age;

        emissions.clear();
        for m in effect.update_modifiers.iter() {
            apply_update(m, &mut p, parent, rng, time, was_alive, is_alive, &mut emissions)?;
        }
        let vel = p.vec3(Attribute::VELOCITY);
        let pos = p.vec3(Attribute::POSITION) + vel * DT;
        p.set(Attribute::POSITION, Val::V3(pos));
        if vel.length_squared() < 1e-12 {
            sample.zero_velocity = true;
        }
        if !pos.is_finite() {
            sample.nan = true;
        }

        for e in emissions.iter() {
            let on_die = !is_alive;
            if on_die {
                sample.events.push((age, e.child_index, e.count, p.clone()));
            } else {
                // Group per-frame events to keep memory bounded.
                match pending_always.iter_mut().find(|(c, _)| *c == e.child_index) {
                    Some(slot) => slot.1 += e.count,
                    None => pending_always.push((e.child_index, e.count)),
                }
            }
        }
        if step % EVENT_CHUNK == EVENT_CHUNK - 1 || !is_alive {
            for (child_index, count) in pending_always.drain(..) {
                sample.events.push((age, child_index, count, p.clone()));
            }
        }

        if step % 5 == 0 {
            sample.path.push(pos);
        }
        sample.end_pos = pos;
        step += 1;
        if !is_alive {
            sample.death = Some(age);
            break;
        }
    }
    Ok(sample)
}

/// Picks `n` indices from `weights` proportionally to their weight.
fn weighted_picks(weights: &[f32], n: usize, rng: &mut StdRng) -> Vec<usize> {
    let total: f32 = weights.iter().sum();
    if total <= 0.0 {
        return Vec::new();
    }
    let mut cumulative = Vec::with_capacity(weights.len());
    let mut acc = 0.0;
    for w in weights {
        acc += w;
        cumulative.push(acc);
    }
    (0..n)
        .map(|_| {
            let r = rng.random::<f32>() * total;
            cumulative.partition_point(|c| *c < r).min(weights.len() - 1)
        })
        .collect()
}

/// Expected alive count per bin: births convolved with the sampled survival.
fn alive_from_births(births: &[f32], samples: &[Sample]) -> Vec<f32> {
    let n_bins = births.len();
    let mut alive = vec![0.0; n_bins];
    if samples.is_empty() {
        return alive;
    }
    // survival[k] = fraction of particles still alive k bins after birth.
    let mut lives: Vec<usize> = samples
        .iter()
        .map(|s| s.death.map(|d| (d / DT).round() as usize).unwrap_or(usize::MAX))
        .collect();
    lives.sort_unstable();
    let survival: Vec<f32> = (0..n_bins)
        .map(|k| (samples.len() - lives.partition_point(|l| *l <= k)) as f32 / samples.len() as f32)
        .collect();
    let max_lag = survival.iter().rposition(|s| *s > 0.0).map(|k| k + 1).unwrap_or(0);
    for (b, count) in births.iter().enumerate().filter(|(_, c)| **c > 0.0) {
        for k in 0..max_lag.min(n_bins - b) {
            alive[b + k] += count * survival[k];
        }
    }
    alive
}

/// Longest lifetime of an effect's particles, from a few dry runs of its init
/// modifiers. Immortal particles count as `MAX_PARTICLE_TIME`.
fn max_lifetime(effect: &EffectEditor, mortal: bool, rng: &mut StdRng) -> f32 {
    if !mortal {
        return MAX_PARTICLE_TIME;
    }
    let parent = effect.parent.as_ref().map(|_| Attrs::default());
    let mut max: f32 = 0.0;
    for _ in 0..64 {
        let mut p = Attrs::default();
        for m in effect.init_modifiers.iter() {
            if apply_init(m, &mut p, parent.as_ref(), rng, 0.0).is_err() {
                return 1.0;
            }
        }
        max = max.max(p.f32(Attribute::LIFETIME));
    }
    max.clamp(DT, MAX_PARTICLE_TIME)
}

fn simulate_project(project: &OmagariProject, n_samples: usize, seed: u64) -> (Vec<EffectSim>, f32) {
    let effects = &project.effects;
    let mut rng = StdRng::seed_from_u64(seed);

    let mortal: Vec<bool> = effects
        .iter()
        .map(|e| {
            let layout = e.produce().particle_layout();
            layout.contains(Attribute::AGE) && layout.contains(Attribute::LIFETIME)
        })
        .collect();

    // Longest chain of lifetimes (a child lives on after its parent particle).
    let mut chain_life = vec![0.0f32; effects.len()];
    for i in 0..effects.len() {
        let own = max_lifetime(&effects[i], mortal[i], &mut rng);
        chain_life[i] = own + parent_index(effects, i).map(|pi| chain_life[pi]).unwrap_or(0.0);
    }
    let longest_chain = chain_life.iter().cloned().fold(0.0, f32::max);

    // The timeline holds the longest emission plus the longest particle chain.
    // Spawners that repeat forever are shown long enough to reach a steady state.
    let mut emission_end: f32 = 0.0;
    for e in effects.iter().filter(|e| e.parent.is_none()) {
        let s = &e.spawner_settings;
        let duration = s.spawn_duration().range()[1].max(0.0);
        let period = s.period().range()[1].max(duration);
        let emission = match s.cycle_count() {
            0 => (period * 2.0).max(longest_chain * 2.0).max(2.0),
            1 => duration,
            n => period * (n - 1) as f32 + duration,
        };
        emission_end = emission_end.max(emission);
    }
    let horizon = (emission_end + longest_chain).clamp(DT, MAX_TIMELINE);
    let n_bins = bins(horizon);

    // Mortal particles are followed until they die, so motion stats cover the
    // whole life; immortal ones only until the end of the timeline.
    let particle_time = |mortal: bool, birth: f32| {
        if mortal { MAX_PARTICLE_TIME } else { (horizon - birth).clamp(DT, MAX_PARTICLE_TIME) }
    };

    let mut sims: Vec<EffectSim> = Vec::with_capacity(effects.len());
    for (i, effect) in effects.iter().enumerate() {
        let mortal = mortal[i];
        let mut sim = EffectSim {
            samples: Vec::new(),
            births: vec![0.0; n_bins],
            alive: vec![0.0; n_bins],
            forever: false,
            spawn_text: String::new(),
            error: None,
        };

        // Birth candidates: (time, parent state, weight).
        let result: Result<(), String> = (|| {
            if effect.parent.is_none() {
                let (births, text, forever) = spawner_schedule(&effect.spawner_settings, horizon);
                sim.spawn_text = text;
                sim.forever = forever;
                for b in weighted_picks(&births, n_samples, &mut rng) {
                    let t = (b as f32 + rng.random::<f32>()) * DT;
                    sim.samples.push(simulate_particle(effect, mortal, None, t, particle_time(mortal, t), &mut rng)?);
                }
                sim.alive = alive_from_births(&births, &sim.samples);
                sim.births = births;
            } else {
                let Some(pi) = parent_index(effects, i) else {
                    sim.spawn_text = "never spawns (parent missing)".into();
                    return Ok(());
                };
                let child_index = children_of(effects, pi).iter().position(|c| *c == i).unwrap() as u32;
                let parent = &sims[pi];
                let parent_total: f32 = parent.births.iter().sum();
                let parent_weight = if parent.samples.is_empty() {
                    0.0
                } else {
                    parent_total / parent.samples.len() as f32
                };

                // Every spawn event of every sampled parent particle, as a candidate.
                let mut candidates: Vec<(f32, &Attrs)> = Vec::new();
                let mut weights: Vec<f32> = Vec::new();
                let mut births = vec![0.0; n_bins];
                for ps in parent.samples.iter() {
                    for (t_rel, ci, count, state) in ps.events.iter() {
                        if *ci != child_index {
                            continue;
                        }
                        let t = ps.birth + t_rel;
                        let b = (t / DT) as usize;
                        if b < n_bins {
                            births[b] += *count as f32 * parent_weight;
                        }
                        candidates.push((t, state));
                        weights.push(*count as f32);
                    }
                }
                let mut always = 0u32;
                let mut on_die = 0u32;
                for m in emit_modifiers(&effects[pi]).filter(|m| m.child_index == child_index) {
                    let count = match &m.count_expr {
                        ExprWriterEditor::U32(c) => *c,
                        _ => 0,
                    };
                    match m.condition {
                        EventEmitCondition::Always => always += count,
                        EventEmitCondition::OnDie => on_die += count,
                    }
                }
                let mut parts = Vec::new();
                if always > 0 {
                    parts.push(format!("{} per frame (~{}/s) per alive parent particle", always, always * 60));
                }
                if on_die > 0 {
                    parts.push(format!("{} when each parent particle dies", on_die));
                }
                if parts.is_empty() {
                    parts.push("spawn events with a computed count".into());
                }
                sim.spawn_text = format!(
                    "child of \"{}\" (child_index {}): {}; own spawner settings are ignored",
                    effects[pi].name,
                    child_index,
                    parts.join(" + ")
                );
                sim.forever = parent.forever;

                for c in weighted_picks(&weights, n_samples, &mut rng) {
                    let (t, state) = candidates[c];
                    let state = state.clone();
                    sim.samples.push(simulate_particle(effect, mortal, Some(&state), t, particle_time(mortal, t), &mut rng)?);
                }
                sim.alive = alive_from_births(&births, &sim.samples);
                sim.births = births;
            }
            Ok(())
        })();
        if let Err(e) = result {
            sim.error = Some(e);
        }
        sims.push(sim);
    }
    (sims, horizon)
}

// ---------------------------------------------------------------------------
// Report
// ---------------------------------------------------------------------------

fn fmt_num(x: f32) -> String {
    if !x.is_finite() {
        return format!("{}", x);
    }
    let a = x.abs();
    let s = if a >= 100.0 || a == a.round() && a < 1e6 {
        format!("{:.0}", x)
    } else if a >= 10.0 {
        format!("{:.1}", x)
    } else if a >= 0.1 {
        format!("{:.2}", x)
    } else {
        format!("{:.3}", x)
    };
    // Tiny negatives round to "-0.000"; drop the sign.
    if s.starts_with('-') && s[1..].chars().all(|c| c == '0' || c == '.') {
        s[1..].to_string()
    } else {
        s
    }
}

fn fmt_vec3(v: Vec3) -> String {
    format!("({}, {}, {})", fmt_num(v.x), fmt_num(v.y), fmt_num(v.z))
}

fn range_text(values: &[f32], unit: &str) -> String {
    if values.is_empty() {
        return "n/a".into();
    }
    let min = values.iter().cloned().fold(f32::INFINITY, f32::min);
    let max = values.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let mean = values.iter().sum::<f32>() / values.len() as f32;
    if (max - min).abs() <= 1e-4 * max.abs().max(1.0) {
        format!("{}{} (fixed)", fmt_num(mean), unit)
    } else {
        format!("{}–{}{} (mean {})", fmt_num(min), fmt_num(max), unit, fmt_num(mean))
    }
}

fn percentile(sorted: &[f32], p: f32) -> f32 {
    if sorted.is_empty() {
        return 0.0;
    }
    let i = ((sorted.len() - 1) as f32 * p).round() as usize;
    sorted[i]
}

/// Human name for a linear RGB color, plus its intensity (max channel).
fn color_name(c: Vec3) -> String {
    let max = c.max_element();
    if max <= 1e-3 {
        return "black".into();
    }
    let n = c / max;
    let min = n.min_element();
    let base = if min > 0.8 {
        "white".to_string()
    } else {
        let (r, g, b) = (n.x, n.y, n.z);
        let delta = 1.0 - min;
        let hue = if r >= g && r >= b {
            60.0 * (((g - b) / delta).rem_euclid(6.0))
        } else if g >= b {
            60.0 * ((b - r) / delta + 2.0)
        } else {
            60.0 * ((r - g) / delta + 4.0)
        };
        let name = match hue as u32 {
            0..=14 | 345..=360 => "red",
            15..=39 => "orange",
            40..=69 => "yellow",
            70..=159 => "green",
            160..=199 => "cyan",
            200..=259 => "blue",
            260..=289 => "purple",
            _ => "magenta",
        };
        if min > 0.5 { format!("pale {}", name) } else { name.to_string() }
    };
    if (max - 1.0).abs() < 0.05 {
        base
    } else {
        format!("{}×{}", base, fmt_num(max))
    }
}

fn gradient_eval4(keys: &[(f32, Vec4)], t: f32) -> Vec4 {
    let mut keys: Vec<(f32, Vec4)> = keys.to_vec();
    keys.sort_by(|a, b| a.0.total_cmp(&b.0));
    match keys.as_slice() {
        [] => Vec4::ONE,
        [only] => only.1,
        _ => {
            if t <= keys[0].0 {
                return keys[0].1;
            }
            for w in keys.windows(2) {
                if t <= w[1].0 {
                    let span = (w[1].0 - w[0].0).max(1e-6);
                    return w[0].1.lerp(w[1].1, (t - w[0].0) / span);
                }
            }
            keys.last().unwrap().1
        }
    }
}

fn base_color(init: &Attrs) -> Vec4 {
    if init.0.iter().any(|(a, _)| *a == Attribute::HDR_COLOR) {
        match init.get(Attribute::HDR_COLOR) {
            Val::V4(v) => v,
            _ => Vec4::ONE,
        }
    } else {
        match init.get(Attribute::COLOR) {
            Val::U(u) => unpack4x8unorm(u),
            _ => Vec4::ONE,
        }
    }
}

fn describe_color(effect: &EffectEditor, samples: &[Sample], out: &mut String) {
    let color_mod = effect.render_modifiers.iter().find_map(|m| match m {
        RenderModifierEditor::ColorOverLifetime(c) => Some(c),
        _ => None,
    });
    // Average base color over samples (set by init `color`/`hdr_color`).
    let base = if samples.is_empty() {
        Vec4::ONE
    } else {
        samples.iter().map(|s| base_color(&s.init)).sum::<Vec4>() / samples.len() as f32
    };
    let varied = samples.len() > 1 && {
        let first = base_color(&samples[0].init);
        samples.iter().any(|s| (base_color(&s.init) - first).length() > 0.05)
    };

    let final_at = |t: f32| -> Vec4 {
        match color_mod {
            None => base,
            Some(c) => {
                let g = gradient_eval4(&c.gradient.g, t);
                let blend = c.blend.unwrap_or_default();
                let mask = c.mask.unwrap_or_default();
                let mixed = match blend {
                    ColorBlendMode::Overwrite => g,
                    ColorBlendMode::Modulate => base * g,
                    ColorBlendMode::Add => base + g,
                };
                if mask.contains(ColorBlendMask::A) { mixed } else { mixed.truncate().extend(base.w) }
            }
        }
    };

    match color_mod {
        None => {
            let _ = writeln!(
                out,
                "  color: {}{} alpha {} for the whole life (no ColorOverLifetime)",
                color_name(base.truncate()),
                if varied { " (varies per particle)" } else { "" },
                fmt_num(base.w)
            );
        }
        Some(c) => {
            let blend = c.blend.unwrap_or_default();
            let mut keys: Vec<f32> = c.gradient.g.iter().map(|k| k.0).collect();
            keys.sort_by(|a, b| a.total_cmp(b));
            keys.dedup();
            // With per-particle base colors an average is meaningless, so show
            // the gradient itself.
            let per_particle = varied && blend != ColorBlendMode::Overwrite;
            let stops: Vec<String> = keys
                .iter()
                .map(|t| {
                    let v = if per_particle { gradient_eval4(&c.gradient.g, *t) } else { final_at(*t) };
                    format!("{}% {} a{}", (t * 100.0).round(), color_name(v.truncate()), fmt_num(v.w))
                })
                .collect();
            let _ = writeln!(
                out,
                "  color: ColorOverLifetime {:?}{}: {}",
                blend,
                if per_particle { " with per-particle base colors (they vary), gradient" } else { "" },
                stops.join(" → ")
            );
        }
    }

    // Visibility window and brightness.
    let mut first = None;
    let mut last = None;
    let mut peak: f32 = 0.0;
    for i in 0..=100 {
        let t = i as f32 / 100.0;
        let v = final_at(t);
        peak = peak.max(v.truncate().max_element());
        if v.w > 0.05 {
            first.get_or_insert(t);
            last = Some(t);
        }
    }
    let glow = if peak > 1.0 { ", HDR >1 so it blooms" } else { "" };
    match (first, last) {
        (Some(a), Some(b)) => {
            let _ = writeln!(
                out,
                "  visible: alpha > 0.05 from {}% to {}% of life; peak intensity {}{}",
                (a * 100.0).round(),
                (b * 100.0).round(),
                fmt_num(peak),
                glow
            );
        }
        _ => {
            let _ = writeln!(out, "  visible: NEVER (alpha stays ≤ 0.05) — particles are invisible");
        }
    }
}

fn describe_size(effect: &EffectEditor, samples: &[Sample], out: &mut String) {
    let size_mod = effect.render_modifiers.iter().find_map(|m| match m {
        RenderModifierEditor::SizeOverLifetime(s) => Some(s),
        _ => None,
    });
    match size_mod {
        Some(s) => {
            let mut keys = s.gradient.g.clone();
            keys.sort_by(|a, b| a.0.total_cmp(&b.0));
            let stops: Vec<String> = keys
                .iter()
                .map(|(t, v)| {
                    let size = if (v.x - v.y).abs() < 1e-4 && (v.x - v.z).abs() < 1e-4 {
                        fmt_num(v.x)
                    } else {
                        fmt_vec3(*v)
                    };
                    format!("{}% {}", (t * 100.0).round(), size)
                })
                .collect();
            let _ = writeln!(out, "  size: SizeOverLifetime (world units) {}", stops.join(" → "));
        }
        None => {
            let set = effect.init_modifiers.iter().any(|m| {
                matches!(m, ModifierEditor::SetAttribute(m) if m.attr == Attribute::SIZE || m.attr == Attribute::SIZE2 || m.attr == Attribute::SIZE3)
            });
            if set {
                let sizes: Vec<f32> = samples
                    .iter()
                    .map(|s| match (s.init.get(Attribute::SIZE), s.init.0.iter().any(|(a, _)| *a == Attribute::SIZE)) {
                        (Val::F(x), true) => x,
                        _ => match s.init.get(Attribute::SIZE3) {
                            Val::V3(v) => v.max_element(),
                            _ => 1.0,
                        },
                    })
                    .collect();
                let _ = writeln!(out, "  size: {} world units", range_text(&sizes, ""));
            } else {
                let _ = writeln!(out, "  size: default 1.0 world unit (nothing sets size; that is large)");
            }
        }
    }
}

fn timeline_text(alive: &[f32], horizon: f32) -> String {
    let points = 8;
    let end = alive
        .iter()
        .rposition(|a| *a >= 0.5)
        .map(|b| (b as f32 * DT).min(horizon))
        .unwrap_or(0.0);
    let span = if end > 0.0 { end } else { horizon };
    (0..=points)
        .map(|k| {
            let t = span * k as f32 / points as f32;
            let b = ((t / DT) as usize).min(alive.len().saturating_sub(1));
            format!("{}s:{}", fmt_num(t), alive.get(b).map(|a| a.round() as i64).unwrap_or(0))
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Simulates a project and describes every effect in a compact text form.
pub fn analyze_project(
    project: &OmagariProject,
    registry: &AppTypeRegistry,
    n_samples: usize,
    only_effect: Option<&str>,
) -> String {
    let diagnostics = check_project(project, registry);
    let mut out = diagnostics.render();
    if !diagnostics.errors.is_empty() {
        out.push_str("\n(simulation still runs, but results for broken effects are unreliable)\n");
    }

    let (sims, horizon) = simulate_project(project, n_samples.clamp(20, 2000), 42);
    let truncated = horizon >= MAX_TIMELINE - DT;
    let effects = &project.effects;
    let mut total_alive = vec![0.0f32; bins(horizon)];
    let mut overall_extent: f32 = 0.0;
    let mut any_forever = false;

    for (i, (effect, sim)) in effects.iter().zip(sims.iter()).enumerate() {
        for (a, b) in total_alive.iter_mut().zip(sim.alive.iter()) {
            *a += b;
        }
        any_forever |= sim.forever;
        if let Some(only) = only_effect {
            if effect.name != only {
                continue;
            }
        }

        let texture = effect
            .texture_index
            .and_then(|t| PARTICLE_TEXTURES.get(t))
            .map(|t| t.ui_label)
            .unwrap_or("none");
        let role = match &effect.parent {
            Some(p) => format!("child of \"{}\"", p),
            None => "root".into(),
        };
        let _ = writeln!(
            out,
            "\n## [{}] \"{}\" — {}, texture {}, capacity {}",
            i, effect.name, role, texture, effect.capacity
        );
        let _ = writeln!(out, "  spawn: {}", sim.spawn_text);
        if let Some(e) = &sim.error {
            let _ = writeln!(out, "  simulation failed: {}", e);
            continue;
        }
        if sim.samples.is_empty() {
            let _ = writeln!(out, "  no particles spawn in the simulated {}s", fmt_num(horizon));
            continue;
        }
        let samples = &sim.samples;

        let total_births: f32 = sim.births.iter().sum();
        let peak = sim.alive.iter().cloned().fold(0.0, f32::max);
        let capacity_note = if peak > effect.capacity as f32 {
            format!(" — EXCEEDS capacity {}, spawning will be capped", effect.capacity)
        } else {
            String::new()
        };
        if sim.forever {
            let tail = &sim.alive[sim.alive.len() * 3 / 4..];
            let steady = tail.iter().sum::<f32>() / tail.len().max(1) as f32;
            let _ = writeln!(
                out,
                "  count: ~{} alive at steady state, peak ~{}{}",
                steady.round(),
                peak.round(),
                capacity_note
            );
        } else {
            let _ = writeln!(
                out,
                "  count: ~{} particles in total{}, peak ~{} alive{}",
                total_births.round(),
                if truncated { format!(" in the first {}s", fmt_num(horizon)) } else { String::new() },
                peak.round(),
                capacity_note
            );
        }

        let lifetimes: Vec<f32> = samples.iter().map(|s| s.lifetime_attr).collect();
        let dies = samples.iter().any(|s| s.death.is_some());
        let _ = writeln!(
            out,
            "  lifetime: {}{}",
            range_text(&lifetimes, "s"),
            if dies { "" } else { " — but particles NEVER die (no age attribute)" }
        );

        let birth_center = samples.iter().map(|s| s.birth_pos).sum::<Vec3>() / samples.len() as f32;
        let birth_spread = samples
            .iter()
            .map(|s| (s.birth_pos - birth_center).length())
            .fold(0.0, f32::max);
        let speeds: Vec<f32> = samples.iter().map(|s| s.init_speed).collect();
        let mean_dir = samples
            .iter()
            .map(|s| s.init.vec3(Attribute::VELOCITY))
            .sum::<Vec3>()
            / samples.len() as f32;
        let mean_speed = speeds.iter().sum::<f32>() / speeds.len() as f32;
        let direction = if mean_speed <= 1e-6 {
            "no initial velocity".to_string()
        } else if mean_dir.length() < 0.25 * mean_speed {
            "directions spread all around".to_string()
        } else {
            format!("mostly toward {}", fmt_vec3(mean_dir.normalize()))
        };
        let _ = writeln!(
            out,
            "  birth: around {} within {}; initial speed {}, {}",
            fmt_vec3(birth_center),
            fmt_num(birth_spread),
            range_text(&speeds, ""),
            direction
        );

        let mut xs = Vec::new();
        let mut ys = Vec::new();
        let mut zs = Vec::new();
        let mut max_dist: f32 = 0.0;
        for s in samples {
            for p in s.path.iter().chain(std::iter::once(&s.end_pos)) {
                if p.is_finite() {
                    xs.push(p.x);
                    ys.push(p.y);
                    zs.push(p.z);
                    max_dist = max_dist.max(p.length());
                }
            }
        }
        for v in [&mut xs, &mut ys, &mut zs] {
            v.sort_by(|a, b| a.total_cmp(b));
        }
        let ext = |v: &[f32]| format!("[{}, {}]", fmt_num(percentile(v, 0.05)), fmt_num(percentile(v, 0.95)));
        let end_mean = samples.iter().map(|s| s.end_pos).filter(|p| p.is_finite()).sum::<Vec3>()
            / samples.len() as f32;
        let travel: Vec<f32> = samples.iter().map(|s| (s.end_pos - s.birth_pos).length()).collect();
        let _ = writeln!(
            out,
            "  motion: 90% of positions in x{} y{} z{}; farthest {} from origin; travels {}; ends around {}",
            ext(&xs),
            ext(&ys),
            ext(&zs),
            fmt_num(max_dist),
            range_text(&travel, ""),
            fmt_vec3(end_mean)
        );
        overall_extent = overall_extent.max(max_dist);

        describe_size(effect, samples, &mut out);
        describe_color(effect, samples, &mut out);

        if samples.iter().any(|s| s.nan) {
            let _ = writeln!(out, "  WARNING: some positions become NaN (e.g. Normalized of a zero vector)");
        }
        let zero_v = samples.iter().filter(|s| s.zero_velocity).count();
        if zero_v > 0 {
            let _ = writeln!(
                out,
                "  WARNING: {}% of particles have zero velocity at some point; the editor orients particles along velocity, so they may render invisible then",
                (zero_v * 100 / samples.len())
            );
        }
        let _ = writeln!(out, "  alive over time: {}", timeline_text(&sim.alive, horizon));
    }

    if only_effect.is_none() {
        let _ = writeln!(out, "\n## Whole project");
        let peak = total_alive.iter().cloned().fold(0.0, f32::max);
        if any_forever {
            let _ = writeln!(out, "  loops forever; peak ~{} particles alive", peak.round());
        } else if truncated && total_alive.last().is_some_and(|a| *a >= 0.5) {
            let _ = writeln!(
                out,
                "  one-shot, but still running after the simulated {}s; peak ~{} particles alive",
                fmt_num(horizon),
                peak.round()
            );
        } else {
            let end = total_alive
                .iter()
                .rposition(|a| *a >= 0.5)
                .map(|b| b as f32 * DT)
                .unwrap_or(0.0);
            let _ = writeln!(
                out,
                "  one-shot: last particle dies at ~{}s; peak ~{} particles alive",
                fmt_num(end),
                peak.round()
            );
        }
        let _ = writeln!(out, "  alive over time: {}", timeline_text(&total_alive, horizon));
        let fits = if overall_extent <= DEFAULT_VIEW_HALF_HEIGHT {
            "fits the default editor view"
        } else {
            "bigger than the default editor view (±2 units around the origin); the user must zoom out"
        };
        let _ = writeln!(out, "  farthest particle {} units from the origin: {}", fmt_num(overall_extent), fits);
    }
    let _ = writeln!(
        out,
        "\n(CPU approximation, {} samples per effect, 60 updates/s. Spawned at the origin, Y is up.)",
        n_samples.clamp(20, 2000)
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> OmagariProject {
        ron::from_str(text).unwrap_or_else(|e| panic!("{}\n{}", e, text))
    }

    /// Every recipe in the agent guide must pass the checks it tells agents to run.
    #[test]
    fn guide_recipes_pass_checks() {
        let registry = type_registry();
        let recipes = crate::mcp::GUIDE.split("## Recipes").nth(1).unwrap();
        let recipes = recipes.split("\n## ").next().unwrap();
        let mut checked = 0;
        for block in recipes.split("```ron\n").skip(1) {
            let text = block.split("```").next().unwrap();
            let project = parse(text);
            let d = check_project(&project, &registry);
            assert!(d.errors.is_empty() && d.warnings.is_empty(), "{}\n{}", d.render(), text);
            analyze_project(&project, &registry, 50, None);
            checked += 1;
        }
        assert!(checked >= 5, "only {} recipes found", checked);
    }

    #[test]
    fn examples_analyze() {
        let registry = type_registry();
        let examples = concat!(env!("CARGO_MANIFEST_DIR"), "/examples");
        for entry in std::fs::read_dir(examples).unwrap() {
            let path = entry.unwrap().path();
            if path.to_string_lossy().ends_with(".omagari.ron") {
                let project = parse(&std::fs::read_to_string(&path).unwrap());
                let report = analyze_project(&project, &registry, 50, None);
                assert!(report.contains("## Whole project"), "{}", report);
            }
        }
    }

    #[test]
    fn reports_type_and_structure_errors() {
        let registry = type_registry();
        let project = parse(
            r#"(effects: [
                (name: "a", parent: None, capacity: 10,
                 spawner_settings: (count: Single(1.0), spawn_duration: Single(0.0), period: Single(1.0), cycle_count: 1, starts_active: true, emit_on_start: true),
                 texture_index: Some(0),
                 init_modifiers: [SetAttribute((attr: "size", attr_expr: Vec3((1.0, 1.0, 1.0))))],
                 update_modifiers: [], render_modifiers: []),
                (name: "b", parent: Some("a"), capacity: 10,
                 spawner_settings: (count: Single(1.0), spawn_duration: Single(0.0), period: Single(1.0), cycle_count: 1, starts_active: true, emit_on_start: true),
                 texture_index: Some(0),
                 init_modifiers: [SetAttribute((attr: "position", attr_expr: Float(1.0)))],
                 update_modifiers: [], render_modifiers: []),
            ])"#,
        );
        let errors = check_project(&project, &registry).errors.join("\n");
        assert!(errors.contains("[a] init SetAttribute: SetAttribute(\"size\") needs f32, got vec3"), "{}", errors);
        assert!(errors.contains("[a] bevy_hanabi") == false, "{}", errors);
        assert!(errors.contains("[b] parent \"a\" has no EmitSpawnEventModifier with child_index 0"), "{}", errors);
        assert!(errors.contains("[b] init SetAttribute: SetAttribute(\"position\") needs vec3, got f32"), "{}", errors);
    }

    #[test]
    fn motionless_particles_are_reported() {
        let registry = type_registry();
        let project = parse(
            r#"(effects: [
                (name: "a", parent: None, capacity: 10,
                 spawner_settings: (count: Single(1.0), spawn_duration: Single(0.0), period: Single(1.0), cycle_count: 1, starts_active: true, emit_on_start: true),
                 texture_index: Some(0),
                 init_modifiers: [SetAttribute((attr: "lifetime", attr_expr: Float(1.0)))],
                 update_modifiers: [], render_modifiers: []),
            ])"#,
        );
        let d = check_project(&project, &registry);
        assert!(d.errors.is_empty(), "{}", d.render());
        assert!(d.warnings.iter().any(|w| w.contains("nothing sets a velocity")), "{}", d.render());
        assert!(d.warnings.iter().any(|w| w.contains("never die")), "{}", d.render());
        let report = analyze_project(&project, &registry, 20, None);
        assert!(report.contains("zero velocity"), "{}", report);
    }
}
