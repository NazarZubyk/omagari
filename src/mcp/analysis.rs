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
use std::path::Path;

use bevy::prelude::*;
use bevy_hanabi::prelude::*;
use rand::{rngs::StdRng, Rng, SeedableRng};

use omagari::format::*;
use omagari::runtime::{socket_lookup, BUILTIN_TEXTURES};

use crate::asset_library as library;

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
    /// ivec2..ivec4: component count, components.
    IV(u8, [i32; 4]),
    UV(u8, [u32; 4]),
    BV(u8, [bool; 4]),
}

/// Element type of a value.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Elem {
    F,
    I,
    U,
    B,
}

fn elem_of(scalar: ScalarType) -> Elem {
    match scalar {
        ScalarType::Int => Elem::I,
        ScalarType::Uint => Elem::U,
        ScalarType::Bool => Elem::B,
        // `ScalarType` is non-exhaustive; every other kind is a float.
        _ => Elem::F,
    }
}

/// A value as `n` lanes of one element type (`n` = 0 for a scalar), for
/// component-wise operators.
#[derive(Clone, Copy, Debug)]
struct Lanes {
    elem: Elem,
    n: usize,
    f: [f32; 4],
    i: [i32; 4],
    u: [u32; 4],
    b: [bool; 4],
}

impl Lanes {
    fn new(elem: Elem, n: usize) -> Lanes {
        Lanes { elem, n, f: [0.0; 4], i: [0; 4], u: [0; 4], b: [false; 4] }
    }

    /// Number of components (1 for a scalar).
    fn count(&self) -> usize {
        self.n.max(1)
    }

    fn to_val(self) -> Val {
        match (self.elem, self.n) {
            (Elem::F, 0) => Val::F(self.f[0]),
            (Elem::I, 0) => Val::I(self.i[0]),
            (Elem::U, 0) => Val::U(self.u[0]),
            (Elem::B, 0) => Val::B(self.b[0]),
            (Elem::F, 2) => Val::V2(Vec2::new(self.f[0], self.f[1])),
            (Elem::F, 3) => Val::V3(Vec3::new(self.f[0], self.f[1], self.f[2])),
            (Elem::F, _) => Val::V4(Vec4::from_array(self.f)),
            (Elem::I, n) => Val::IV(n as u8, self.i),
            (Elem::U, n) => Val::UV(n as u8, self.u),
            (Elem::B, n) => Val::BV(n as u8, self.b),
        }
    }
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
            Val::IV(n, _) => ["ivec2", "ivec3", "ivec4"][(*n as usize).clamp(2, 4) - 2],
            Val::UV(n, _) => ["uvec2", "uvec3", "uvec4"][(*n as usize).clamp(2, 4) - 2],
            Val::BV(n, _) => ["bvec2", "bvec3", "bvec4"][(*n as usize).clamp(2, 4) - 2],
        }
    }

    fn same_type(&self, other: &Val) -> bool {
        self.type_name() == other.type_name()
    }

    fn lanes(&self) -> Lanes {
        let mut l = match self {
            Val::F(_) | Val::V2(_) | Val::V3(_) | Val::V4(_) => Lanes::new(Elem::F, 0),
            Val::I(_) | Val::IV(..) => Lanes::new(Elem::I, 0),
            Val::U(_) | Val::UV(..) => Lanes::new(Elem::U, 0),
            Val::B(_) | Val::BV(..) => Lanes::new(Elem::B, 0),
        };
        match *self {
            Val::F(x) => l.f[0] = x,
            Val::I(x) => l.i[0] = x,
            Val::U(x) => l.u[0] = x,
            Val::B(x) => l.b[0] = x,
            Val::V2(v) => {
                l.n = 2;
                l.f[..2].copy_from_slice(&v.to_array());
            }
            Val::V3(v) => {
                l.n = 3;
                l.f[..3].copy_from_slice(&v.to_array());
            }
            Val::V4(v) => {
                l.n = 4;
                l.f = v.to_array();
            }
            Val::IV(n, c) => {
                l.n = n as usize;
                l.i = c;
            }
            Val::UV(n, c) => {
                l.n = n as usize;
                l.u = c;
            }
            Val::BV(n, c) => {
                l.n = n as usize;
                l.b = c;
            }
        }
        l
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
        let (elem, n) = match value_type {
            ValueType::Scalar(s) => (s, 0),
            ValueType::Vector(v) => (v.elem_type(), v.count() as usize),
            _ => (ScalarType::Float, 0),
        };
        let elem = elem_of(elem);
        Lanes::new(elem, n).to_val()
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

/// Which shader an expression runs in.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Ctx {
    Init,
    Update,
    Render,
}

struct Env<'a> {
    rng: &'a mut StdRng,
    attrs: &'a Attrs,
    parent: Option<&'a Attrs>,
    time: f32,
    ctx: Ctx,
    /// The particle's alive flag (update modifiers).
    is_alive: bool,
}

fn pack4x8unorm(v: Vec4) -> u32 {
    let b = |x: f32| ((x.clamp(0.0, 1.0) * 255.0).round() as u32) & 0xFF;
    b(v.x) | (b(v.y) << 8) | (b(v.z) << 16) | (b(v.w) << 24)
}

fn unpack4x8unorm(u: u32) -> Vec4 {
    let b = |s: u32| ((u >> s) & 0xFF) as f32 / 255.0;
    Vec4::new(b(0), b(8), b(16), b(24))
}

fn pack4x8snorm(v: Vec4) -> u32 {
    let b = |x: f32| (((x.clamp(-1.0, 1.0) * 127.0).round() as i32) as u32) & 0xFF;
    b(v.x) | (b(v.y) << 8) | (b(v.z) << 16) | (b(v.w) << 24)
}

fn unpack4x8snorm(u: u32) -> Vec4 {
    let b = |s: u32| ((((u >> s) & 0xFF) as u8 as i8) as f32 / 127.0).max(-1.0);
    Vec4::new(b(0), b(8), b(16), b(24))
}

fn types_text(args: &[Val]) -> String {
    args.iter().map(Val::type_name).collect::<Vec<_>>().join(", ")
}

/// Why an integer or bool `Rand` cannot be used.
fn int_rand_error(what: &str) -> String {
    format!(
        "{} is not available: bevy_hanabi 0.19's shaders only have float random functions, so the shader fails on the GPU; use e.g. Operator(Cast(Operator(Multiply(RandFloat, Float(10.0))), U32))",
        what
    )
}

fn eval(e: &ExprWriterEditor, env: &mut Env) -> Result<Val, String> {
    use ExprWriterEditor as E;
    let rand_f = |env: &mut Env| env.rng.random::<f32>();
    Ok(match e {
        E::Placeholder => Val::F(0.0),
        E::RandU32 => return Err(int_rand_error("RandU32")),
        E::RandFloat => Val::F(rand_f(env)),
        E::RandVec3 => Val::V3(Vec3::new(rand_f(env), rand_f(env), rand_f(env))),
        E::Rand(t) => {
            if !t.is_float() {
                return Err(int_rand_error(&format!("Rand({:?})", t)));
            }
            let n = t.parts().1 as usize;
            let mut c = [0.0; 4];
            for x in c.iter_mut().take(n) {
                *x = rand_f(env);
            }
            Val::from_floats(n, c)
        }
        E::Float(x) => Val::F(*x),
        E::U32(x) => Val::U(*x),
        E::I32(x) => Val::I(*x),
        E::Bool(x) => Val::B(*x),
        E::Vec2(v) => Val::V2(*v),
        E::Vec3(v) => Val::V3(*v),
        E::Vec4(v) => Val::V4(*v),
        E::IVec2(v) => Val::IV(2, [v.x, v.y, 0, 0]),
        E::IVec3(v) => Val::IV(3, [v.x, v.y, v.z, 0]),
        E::IVec4(v) => Val::IV(4, v.to_array()),
        E::UVec2(v) => Val::UV(2, [v.x, v.y, 0, 0]),
        E::UVec3(v) => Val::UV(3, [v.x, v.y, v.z, 0]),
        E::UVec4(v) => Val::UV(4, v.to_array()),
        E::BVec2(v) => Val::BV(2, [v.x, v.y, false, false]),
        E::BVec3(v) => Val::BV(3, [v.x, v.y, v.z, false]),
        E::BVec4(v) => Val::BV(4, [v.x, v.y, v.z, v.w]),
        // The simulation clock: simulation, virtual and real time agree.
        E::Time | E::VirtualTime | E::RealTime => Val::F(env.time),
        E::DeltaTime | E::VirtualDeltaTime | E::RealDeltaTime => Val::F(DT),
        // bevy_hanabi 0.19 writes `sim_params.alpha_cutoff`, which does not
        // exist: the shader fails to compile wherever it is used.
        E::AlphaCutoff => {
            return Err(
                "AlphaCutoff does not work in bevy_hanabi 0.19 (its shader reads sim_params.alpha_cutoff, which does not exist, so the layer fails to compile on the GPU); use the Mask cutoff number instead".into(),
            )
        }
        E::IsAlive => match env.ctx {
            Ctx::Update => Val::B(env.is_alive),
            _ => return Err("IsAlive exists only in update modifiers".into()),
        },
        E::Age => env.attrs.get(Attribute::AGE),
        E::Operator(op) => eval_op(op, env)?,
        // The simulation substitutes real values first (`resolve_for_analysis`).
        E::Property(name) => match runtime_property(name) {
            Some(RuntimePropertyType::Float) => Val::F(0.0),
            Some(RuntimePropertyType::Vec3) => Val::V3(Vec3::ZERO),
            None => return Err(format!("Property(\"{}\") is not a runtime property", name)),
        },
        // Declared parameters are substituted by `resolve_for_analysis`;
        // what is left is unknown.
        E::Param(name) => return Err(format!("Param(\"{}\") is not a parameter of this package", name)),
    })
}

/// Applies `f` to each component of same-typed float values (`args[i]` may
/// be an f32 when `scalar_ok[i]`); `name` is used in the error.
fn componentwise(name: &str, args: &[Val], scalar_ok: &[bool], f: impl Fn(&[f32]) -> f32) -> Result<Val, String> {
    let floats: Vec<Option<(usize, [f32; 4])>> = args.iter().map(|a| a.floats()).collect();
    let n = floats.iter().flatten().map(|(n, _)| *n).max().unwrap_or(1);
    let ok = floats.iter().zip(scalar_ok).all(|(v, scalar)| match v {
        Some((k, _)) => *k == n || (*k == 1 && *scalar),
        None => false,
    });
    if !ok {
        return Err(format!(
            "{} needs values of the same float type (f32, vec2, vec3 or vec4), got ({})",
            name,
            types_text(args)
        ));
    }
    let mut out = [0.0; 4];
    for (i, slot) in out.iter_mut().enumerate().take(n) {
        let xs: Vec<f32> = floats
            .iter()
            .map(|v| {
                let (k, c) = v.unwrap();
                if k == 1 { c[0] } else { c[i] }
            })
            .collect();
        *slot = f(&xs);
    }
    Ok(Val::from_floats(n, out))
}

/// A float-only operator applied per component.
fn map_float(name: &str, a: Val, f: impl Fn(f32) -> f32) -> Result<Val, String> {
    a.map_floats(f)
        .ok_or_else(|| format!("{} needs a float value (f32, vec2, vec3 or vec4), got {}", name, a.type_name()))
}

/// A numeric operator applied per component (floats, i32, u32 and their
/// vectors).
fn map_num(
    name: &str,
    a: Val,
    ff: impl Fn(f32) -> f32,
    fi: impl Fn(i32) -> i32,
    fu: impl Fn(u32) -> u32,
) -> Result<Val, String> {
    let mut l = a.lanes();
    for k in 0..l.count() {
        match l.elem {
            Elem::F => l.f[k] = ff(l.f[k]),
            Elem::I => l.i[k] = fi(l.i[k]),
            Elem::U => l.u[k] = fu(l.u[k]),
            Elem::B => return Err(format!("{} needs a number, got {}", name, a.type_name())),
        }
    }
    Ok(l.to_val())
}

/// A numeric operator on two values of the same type, per component; with
/// `broadcast` a vector may be combined with a scalar of its element type
/// (WGSL `+ - * / %`).
fn zip_num(
    name: &str,
    a: Val,
    b: Val,
    broadcast: bool,
    ff: impl Fn(f32, f32) -> f32,
    fi: impl Fn(i32, i32) -> i32,
    fu: impl Fn(u32, u32) -> u32,
) -> Result<Val, String> {
    let (la, lb) = (a.lanes(), b.lanes());
    let shape_ok = la.n == lb.n || (broadcast && (la.n == 0 || lb.n == 0));
    if la.elem != lb.elem || la.elem == Elem::B || !shape_ok {
        return Err(format!(
            "{} needs two numbers of the same type{}, got ({})",
            name,
            if broadcast { " (or a vector and a number of its element type)" } else { "" },
            types_text(&[a, b])
        ));
    }
    let mut out = Lanes::new(la.elem, la.n.max(lb.n));
    for k in 0..out.count() {
        let (ka, kb) = (if la.n == 0 { 0 } else { k }, if lb.n == 0 { 0 } else { k });
        match la.elem {
            Elem::F => out.f[k] = ff(la.f[ka], lb.f[kb]),
            Elem::I => out.i[k] = fi(la.i[ka], lb.i[kb]),
            Elem::U => out.u[k] = fu(la.u[ka], lb.u[kb]),
            Elem::B => unreachable!(),
        }
    }
    Ok(out.to_val())
}

/// A comparison of two numbers of the same type: bool, or a bool vector.
fn compare(name: &str, a: Val, b: Val, f: impl Fn(std::cmp::Ordering) -> bool) -> Result<Val, String> {
    let (la, lb) = (a.lanes(), b.lanes());
    if la.elem != lb.elem || la.elem == Elem::B || la.n != lb.n {
        return Err(format!("{} needs two numbers of the same type, got ({})", name, types_text(&[a, b])));
    }
    let mut out = Lanes::new(Elem::B, la.n);
    for k in 0..la.count() {
        let ord = match la.elem {
            Elem::F => la.f[k].partial_cmp(&lb.f[k]),
            Elem::I => Some(la.i[k].cmp(&lb.i[k])),
            Elem::U => Some(la.u[k].cmp(&lb.u[k])),
            Elem::B => unreachable!(),
        };
        out.b[k] = ord.is_some_and(&f);
    }
    Ok(out.to_val())
}

/// Builds a vector from scalars of one type (`Vec2`, `Vec3`, `Vec4`).
fn build_vector(name: &str, parts: &[Val]) -> Result<Val, String> {
    let first = parts[0].lanes();
    if parts.iter().any(|p| p.lanes().n != 0 || p.lanes().elem != first.elem) {
        return Err(format!(
            "{} operator needs {} numbers of the same type (f32, i32, u32 or bool), got ({})",
            name,
            parts.len(),
            types_text(parts)
        ));
    }
    let mut out = Lanes::new(first.elem, parts.len());
    for (k, p) in parts.iter().enumerate() {
        let l = p.lanes();
        out.f[k] = l.f[0];
        out.i[k] = l.i[0];
        out.u[k] = l.u[0];
        out.b[k] = l.b[0];
    }
    Ok(out.to_val())
}

/// `Cast(x, type)`, like WGSL's conversions: a scalar to any scalar, a
/// scalar or a vector of the same size to a vector.
fn cast(a: Val, target: ValueTypeDef) -> Result<Val, String> {
    let (elem, n) = target.parts();
    let n = if n == 1 { 0 } else { n as usize };
    let l = a.lanes();
    if (n == 0 && l.n != 0) || (n != 0 && l.n != 0 && l.n != n) {
        return Err(format!("Cast cannot convert {} to {}", a.type_name(), target.name()));
    }
    let elem = elem_of(elem);
    let mut out = Lanes::new(elem, n);
    for k in 0..out.count() {
        let src = if l.n == 0 { 0 } else { k };
        let (f, i, u, b) = (l.f[src], l.i[src], l.u[src], l.b[src]);
        match (l.elem, elem) {
            (Elem::F, Elem::F) => out.f[k] = f,
            (Elem::F, Elem::I) => out.i[k] = f as i32,
            (Elem::F, Elem::U) => out.u[k] = f as u32,
            (Elem::F, Elem::B) => out.b[k] = f != 0.0,
            (Elem::I, Elem::F) => out.f[k] = i as f32,
            (Elem::I, Elem::I) => out.i[k] = i,
            (Elem::I, Elem::U) => out.u[k] = i as u32,
            (Elem::I, Elem::B) => out.b[k] = i != 0,
            (Elem::U, Elem::F) => out.f[k] = u as f32,
            (Elem::U, Elem::I) => out.i[k] = u as i32,
            (Elem::U, Elem::U) => out.u[k] = u,
            (Elem::U, Elem::B) => out.b[k] = u != 0,
            (Elem::B, Elem::F) => out.f[k] = if b { 1.0 } else { 0.0 },
            (Elem::B, Elem::I) => out.i[k] = b as i32,
            (Elem::B, Elem::U) => out.u[k] = b as u32,
            (Elem::B, Elem::B) => out.b[k] = b,
        }
    }
    Ok(out.to_val())
}

/// Whether bevy_hanabi knows an expression's type while writing the shader
/// (literals, random and built-in values, attributes, casts). `Uniform` and
/// `Normal` need it of both operands.
fn has_known_type(e: &ExprWriterEditor) -> bool {
    match e {
        ExprWriterEditor::Param(_) | ExprWriterEditor::Property(_) => false,
        ExprWriterEditor::Operator(op) => {
            matches!(**op, ExprOperatorEditor::Attr(_) | ExprOperatorEditor::ParentAttr(_) | ExprOperatorEditor::Cast(..))
        }
        _ => true,
    }
}

/// Whether an expression is a compile-time constant (literals and
/// operators on them), which the GPU shader compiler evaluates itself.
fn is_constant(e: &ExprWriterEditor) -> bool {
    let mut constant = true;
    e.visit(&mut |e| {
        constant &= match e {
            ExprWriterEditor::Operator(op) => !matches!(
                **op,
                ExprOperatorEditor::Attr(_) | ExprOperatorEditor::ParentAttr(_) | ExprOperatorEditor::Uniform(..) | ExprOperatorEditor::Normal(..)
            ),
            ExprWriterEditor::Placeholder
            | ExprWriterEditor::Float(_)
            | ExprWriterEditor::U32(_)
            | ExprWriterEditor::I32(_)
            | ExprWriterEditor::Bool(_)
            | ExprWriterEditor::Vec2(_)
            | ExprWriterEditor::Vec3(_)
            | ExprWriterEditor::Vec4(_)
            | ExprWriterEditor::IVec2(_)
            | ExprWriterEditor::IVec3(_)
            | ExprWriterEditor::IVec4(_)
            | ExprWriterEditor::UVec2(_)
            | ExprWriterEditor::UVec3(_)
            | ExprWriterEditor::UVec4(_)
            | ExprWriterEditor::BVec2(_)
            | ExprWriterEditor::BVec3(_)
            | ExprWriterEditor::BVec4(_) => true,
            _ => false,
        }
    });
    constant
}

fn smoothstep(low: f32, high: f32, x: f32) -> f32 {
    let t = if high != low { ((x - low) / (high - low)).clamp(0.0, 1.0) } else if x < low { 0.0 } else { 1.0 };
    t * t * (3.0 - 2.0 * t)
}

/// A normally distributed value (Box-Muller, like bevy_hanabi's
/// `rand_normal`).
fn normal_sample(rng: &mut StdRng, mean: f32, std_dev: f32) -> f32 {
    let u: f32 = rng.random::<f32>().max(f32::MIN_POSITIVE);
    let v: f32 = rng.random();
    mean + std_dev * (-2.0 * u.ln()).sqrt() * (std::f32::consts::TAU * v).cos()
}

fn eval_op(op: &ExprOperatorEditor, env: &mut Env) -> Result<Val, String> {
    use ExprOperatorEditor as O;
    let name = op.name();
    // The operands, evaluated in written order.
    let mut args = Vec::new();
    for arg in op.args() {
        args.push(eval(arg, env)?);
    }
    let a = args.first().copied().unwrap_or(Val::F(0.0));
    let b = args.get(1).copied().unwrap_or(Val::F(0.0));
    let float_vector = |v: Val| v.floats().filter(|(n, _)| *n > 1);
    Ok(match op {
        O::Add(..) => zip_num(name, a, b, true, |x, y| x + y, i32::wrapping_add, u32::wrapping_add)?,
        O::Subtract(..) => zip_num(name, a, b, true, |x, y| x - y, i32::wrapping_sub, u32::wrapping_sub)?,
        O::Multiply(..) => zip_num(name, a, b, true, |x, y| x * y, i32::wrapping_mul, u32::wrapping_mul)?,
        // WGSL: integer division by 0 (and i32::MIN / -1) gives the dividend.
        O::Divide(..) => zip_num(
            name,
            a,
            b,
            true,
            |x, y| x / y,
            |x, y| if y == 0 || (x == i32::MIN && y == -1) { x } else { x / y },
            |x, y| if y == 0 { x } else { x / y },
        )?,
        O::Remainder(..) => zip_num(
            name,
            a,
            b,
            true,
            |x, y| x % y,
            |x, y| if y == 0 || (x == i32::MIN && y == -1) { 0 } else { x % y },
            |x, y| if y == 0 { 0 } else { x % y },
        )?,
        O::Min(..) => zip_num(name, a, b, false, f32::min, i32::min, u32::min)?,
        O::Max(..) => zip_num(name, a, b, false, f32::max, i32::max, u32::max)?,
        O::Abs(_) => map_num(name, a, f32::abs, i32::wrapping_abs, |x| x)?,
        O::Sign(_) => {
            if matches!(a.lanes().elem, Elem::U) {
                return Err(format!("Sign needs a float or i32 value, got {}", a.type_name()));
            }
            let sign = |x: f32| if x > 0.0 { 1.0 } else if x < 0.0 { -1.0 } else { 0.0 };
            map_num(name, a, sign, i32::signum, |x| x)?
        }
        O::Floor(_) => map_float(name, a, f32::floor)?,
        O::Ceil(_) => map_float(name, a, f32::ceil)?,
        O::Round(_) => map_float(name, a, f32::round_ties_even)?,
        O::Fract(_) => map_float(name, a, |x| x - x.floor())?,
        O::Saturate(_) => map_float(name, a, |x| x.clamp(0.0, 1.0))?,
        O::Sqrt(_) => map_float(name, a, f32::sqrt)?,
        O::InvSqrt(_) => map_float(name, a, |x| 1.0 / x.sqrt())?,
        O::Exp(_) => map_float(name, a, f32::exp)?,
        O::Exp2(_) => map_float(name, a, f32::exp2)?,
        O::Log(_) => map_float(name, a, f32::ln)?,
        O::Log2(_) => map_float(name, a, f32::log2)?,
        O::Sin(_) => map_float(name, a, f32::sin)?,
        O::Cos(_) => map_float(name, a, f32::cos)?,
        O::Tan(_) => map_float(name, a, f32::tan)?,
        O::Asin(arg) | O::Acos(arg) => {
            // The GPU shader compiler evaluates constant arguments and
            // rejects ones outside [-1, 1].
            if is_constant(arg) && a.floats().is_some_and(|(n, c)| c[..n].iter().any(|x| x.abs() > 1.0)) {
                return Err(format!(
                    "{} of the constant {:?} is outside -1..1: the GPU shader compiler rejects it",
                    name,
                    a.floats().map(|(n, c)| c[..n].to_vec()).unwrap_or_default()
                ));
            }
            map_float(name, a, if matches!(op, O::Asin(_)) { f32::asin } else { f32::acos })?
        }
        O::Atan(_) => map_float(name, a, f32::atan)?,
        O::Atan2(..) => componentwise(name, &args, &[false, false], |x| x[0].atan2(x[1]))?,
        O::Step(..) => componentwise(name, &args, &[false, false], |x| if x[1] >= x[0] { 1.0 } else { 0.0 })?,
        O::Uniform(ea, eb) | O::Normal(ea, eb) => {
            let uniform = matches!(op, O::Uniform(..));
            match (a.floats(), b.floats()) {
                (Some((na, ca)), Some((nb, cb))) if na == nb => {
                    if !has_known_type(ea) || !has_known_type(eb) {
                        return Err(format!(
                            "{} needs operands whose type bevy_hanabi knows while writing the shader (numbers, random values, attributes); wrap a Param, Property or calculation in Operator(Cast(..., {}))",
                            name,
                            ["F32", "Vec2", "Vec3", "Vec4"][na - 1]
                        ));
                    }
                    let mut out = [0.0; 4];
                    for i in 0..na {
                        out[i] = if uniform {
                            let t: f32 = env.rng.random();
                            ca[i] + t * (cb[i] - ca[i])
                        } else {
                            normal_sample(env.rng, ca[i], cb[i])
                        };
                    }
                    Val::from_floats(na, out)
                }
                _ => {
                    return Err(format!(
                        "{} needs two values of the same float type (f32, vec2, vec3 or vec4), got {} and {}",
                        name,
                        a.type_name(),
                        b.type_name()
                    ))
                }
            }
        }
        O::Distance(..) => match (a.floats(), b.floats()) {
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
        },
        O::Length(_) => match a.floats() {
            Some((n, c)) => Val::F((0..n).map(|i| c[i] * c[i]).sum::<f32>().sqrt()),
            None => return Err(format!("Length needs a float value, got {}", a.type_name())),
        },
        O::Normalized(_) => match float_vector(a) {
            Some((n, c)) => {
                let len = (0..n).map(|i| c[i] * c[i]).sum::<f32>().sqrt();
                let inv = if len > 0.0 { 1.0 / len } else { f32::NAN };
                a.map_floats(|x| x * inv).unwrap()
            }
            None => return Err(format!("Normalized needs a vector, got {}", a.type_name())),
        },
        O::Dot(..) => {
            let (la, lb) = (a.lanes(), b.lanes());
            if la.n == 0 || la.n != lb.n || la.elem != lb.elem || la.elem == Elem::B {
                return Err(format!("Dot needs two number vectors of the same type, got ({})", types_text(&args)));
            }
            let n = la.count();
            match la.elem {
                Elem::F => Val::F((0..n).map(|k| la.f[k] * lb.f[k]).sum()),
                Elem::I => Val::I((0..n).fold(0i32, |s, k| s.wrapping_add(la.i[k].wrapping_mul(lb.i[k])))),
                _ => Val::U((0..n).fold(0u32, |s, k| s.wrapping_add(la.u[k].wrapping_mul(lb.u[k])))),
            }
        }
        O::Cross(..) => match (a, b) {
            (Val::V3(x), Val::V3(y)) => Val::V3(x.cross(y)),
            _ => return Err(format!("Cross needs two vec3, got ({})", types_text(&args))),
        },
        O::Vec2(..) | O::Vec3(..) | O::Vec4(..) => build_vector(name, &args)?,
        O::Vec4XyzW(..) => {
            let (la, lb) = (a.lanes(), b.lanes());
            if la.n != 3 || lb.n != 0 || la.elem != lb.elem {
                return Err(format!(
                    "Vec4XyzW needs a 3-component vector and a number of its element type, got ({})",
                    types_text(&args)
                ));
            }
            let mut out = la;
            out.n = 4;
            out.f[3] = lb.f[0];
            out.i[3] = lb.i[0];
            out.u[3] = lb.u[0];
            out.b[3] = lb.b[0];
            out.to_val()
        }
        O::X(_) | O::Y(_) | O::Z(_) | O::W(_) => {
            let index = match op {
                O::X(_) => 0,
                O::Y(_) => 1,
                O::Z(_) => 2,
                _ => 3,
            };
            let l = a.lanes();
            if l.n <= index {
                return Err(format!("{} needs a vector with at least {} components, got {}", name, index + 1, a.type_name()));
            }
            let mut out = Lanes::new(l.elem, 0);
            out.f[0] = l.f[index];
            out.i[0] = l.i[index];
            out.u[0] = l.u[index];
            out.b[0] = l.b[index];
            out.to_val()
        }
        O::GreaterThan(..) => compare(name, a, b, |o| o.is_gt())?,
        O::GreaterThanOrEqual(..) => compare(name, a, b, |o| o.is_ge())?,
        O::LessThan(..) => compare(name, a, b, |o| o.is_lt())?,
        O::LessThanOrEqual(..) => compare(name, a, b, |o| o.is_le())?,
        O::All(_) | O::Any(_) => {
            let l = a.lanes();
            if l.elem != Elem::B {
                return Err(format!("{} needs a bool or a bool vector, got {}", name, a.type_name()));
            }
            let lanes = &l.b[..l.count()];
            Val::B(if matches!(op, O::All(_)) { lanes.iter().all(|b| *b) } else { lanes.iter().any(|b| *b) })
        }
        O::Mix(..) => componentwise(name, &args, &[false, false, true], |x| x[0] + (x[1] - x[0]) * x[2])?,
        O::Clamp(..) => match args.as_slice() {
            [Val::I(x), Val::I(lo), Val::I(hi)] => Val::I((*x).max(*lo).min(*hi)),
            [Val::U(x), Val::U(lo), Val::U(hi)] => Val::U((*x).max(*lo).min(*hi)),
            _ => componentwise(name, &args, &[false, false, false], |x| x[0].max(x[1]).min(x[2]))?,
        },
        O::SmoothStep(..) => componentwise(name, &args, &[false, false, false], |x| smoothstep(x[0], x[1], x[2]))?,
        O::Pack4x8UNorm(_) | O::Pack4x8SNorm(_) => match a {
            Val::V4(v) => Val::U(if matches!(op, O::Pack4x8UNorm(_)) { pack4x8unorm(v) } else { pack4x8snorm(v) }),
            other => return Err(format!("{} needs a vec4, got {}", name, other.type_name())),
        },
        O::Unpack4x8UNorm(_) | O::Unpack4x8SNorm(_) => match a {
            Val::U(u) => Val::V4(if matches!(op, O::Unpack4x8UNorm(_)) { unpack4x8unorm(u) } else { unpack4x8snorm(u) }),
            other => return Err(format!("{} needs a u32, got {}", name, other.type_name())),
        },
        O::Cast(_, target) => cast(a, *target)?,
        O::Attr(attr) => env.attrs.get(*attr),
        O::ParentAttr(attr) => match (env.parent, env.ctx) {
            (Some(parent), Ctx::Init) => parent.get(*attr),
            (Some(_), _) => {
                return Err(format!(
                    "ParentAttr(\"{}\") works only in init modifiers (the parent particle is read when a child is born)",
                    attr.name()
                ))
            }
            (None, _) => {
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
    } else if attr == Attribute::SPRITE_INDEX && matches!(got, Val::F(_) | Val::U(_)) {
        " (sprite_index is an i32: wrap the value in Operator(Cast(..., I32)))"
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
    m.kind_name()
}

/// Applies an init modifier to a newborn particle.
fn apply_init(
    m: &ModifierEditor,
    p: &mut Attrs,
    parent: Option<&Attrs>,
    rng: &mut StdRng,
    time: f32,
) -> Result<(), String> {
    apply_setter(m, p, parent, rng, time, Ctx::Init, true)
}

/// Sets attributes the way a setter modifier (SetAttribute, positions,
/// velocities, InheritAttribute) does in the shader `ctx`.
fn apply_setter(
    m: &ModifierEditor,
    p: &mut Attrs,
    parent: Option<&Attrs>,
    rng: &mut StdRng,
    time: f32,
    ctx: Ctx,
    is_alive: bool,
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
                    ctx,
                    is_alive,
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
        ModifierEditor::SetPositionCone3d(m) => {
            let height = want_f32(ev!(&m.height_expr), "SetPositionCone3d height")?;
            let base = want_f32(ev!(&m.base_radius_expr), "SetPositionCone3d base_radius")?;
            let top = want_f32(ev!(&m.top_radius_expr), "SetPositionCone3d top_radius")?;
            // bevy_hanabi's distribution (the emitter is not rotated here).
            let alpha_h = rng.random::<f32>().powf(1.0 / 3.0);
            let r = (base + (top - base) * alpha_h) * rng.random::<f32>().sqrt();
            let theta = rng.random::<f32>() * std::f32::consts::TAU;
            p.set(
                Attribute::POSITION,
                Val::V3(Vec3::new(r * theta.cos(), height * alpha_h, r * theta.sin())),
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
        // Update-only modifiers are reported (and ignored) in the init list.
        _ => {}
    }
    Ok(())
}

/// Spawn events a particle emitted during one update.
struct Emission {
    child_index: u32,
    count: u32,
}

/// Applies an update modifier for one simulation step. `alive` is the
/// particle's alive flag at this point of the update (kill modifiers clear
/// it; `EmitSpawnEvent(OnDie)` and `IsAlive` read it).
fn apply_update(
    m: &ModifierEditor,
    p: &mut Attrs,
    parent: Option<&Attrs>,
    rng: &mut StdRng,
    time: f32,
    was_alive: bool,
    alive: &mut bool,
    emissions: &mut Vec<Emission>,
) -> Result<(), String> {
    let is_alive = *alive;
    macro_rules! ev {
        ($e:expr) => {
            eval(
                $e,
                &mut Env {
                    rng: &mut *rng,
                    attrs: &*p,
                    parent,
                    time,
                    ctx: Ctx::Update,
                    is_alive,
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
        ModifierEditor::RadialAccel(m) => {
            let origin = want_vec3(ev!(&m.origin_expr), "RadialAccel origin")?;
            let accel = want_f32(ev!(&m.accel_expr), "RadialAccel accel")?;
            let radial = safe_normalize(p.vec3(Attribute::POSITION) - origin);
            let v = p.vec3(Attribute::VELOCITY) + radial * accel * DT;
            p.set(Attribute::VELOCITY, Val::V3(v));
        }
        ModifierEditor::TangentAccel(m) => {
            let origin = want_vec3(ev!(&m.origin_expr), "TangentAccel origin")?;
            let axis = want_vec3(ev!(&m.axis_expr), "TangentAccel axis")?;
            let accel = want_f32(ev!(&m.accel_expr), "TangentAccel accel")?;
            let radial = safe_normalize(p.vec3(Attribute::POSITION) - origin);
            let tangent = safe_normalize(axis.cross(radial));
            let v = p.vec3(Attribute::VELOCITY) + tangent * accel * DT;
            p.set(Attribute::VELOCITY, Val::V3(v));
        }
        ModifierEditor::LinearDragModifier(m) => {
            let drag = want_f32(ev!(&m.drag_expr), "LinearDragModifier drag")?;
            let v = p.vec3(Attribute::VELOCITY) * (1.0 - drag * DT).max(0.0);
            p.set(Attribute::VELOCITY, Val::V3(v));
        }
        ModifierEditor::KillSphere(m) => {
            let center = want_vec3(ev!(&m.center_expr), "KillSphere center")?;
            let radius = want_f32(ev!(&m.radius_expr), "KillSphere radius")?;
            let d2 = (p.vec3(Attribute::POSITION) - center).length_squared();
            let r2 = radius * radius;
            if (m.kill_inside && d2 < r2) || (!m.kill_inside && d2 > r2) {
                *alive = false;
            }
        }
        ModifierEditor::KillAabb(m) => {
            let center = want_vec3(ev!(&m.center_expr), "KillAabb center")?;
            let half = want_vec3(ev!(&m.half_size_expr), "KillAabb half_size")?;
            let d = (p.vec3(Attribute::POSITION) - center).abs();
            let kill = if m.kill_inside { d.cmplt(half).all() } else { d.cmpgt(half).any() };
            if kill {
                *alive = false;
            }
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
        // Setters allowed in both lists run every frame here.
        m if m.contexts().contains(ModifierContext::Update) => {
            apply_setter(m, p, parent, rng, time, Ctx::Update, is_alive)?
        }
        // Init-only modifiers are reported (and ignored) in the update list.
        _ => {}
    }
    Ok(())
}

/// Type checks a render-time expression (Round roundness, Orient rotation):
/// it must give an f32.
fn check_render_expr(e: &ExprWriterEditor, what: &str, attrs: &Attrs) -> Result<(), String> {
    let mut rng = StdRng::seed_from_u64(0);
    let mut env = Env { rng: &mut rng, attrs, parent: None, time: 0.0, ctx: Ctx::Render, is_alive: true };
    want_f32(eval(e, &mut env)?, what).map(|_| ())
}

// ---------------------------------------------------------------------------
// Checks
// ---------------------------------------------------------------------------

/// How bad a diagnostic is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Severity {
    Error,
    Warning,
    Note,
}

/// Splits `[subject] message` into the subject and the message; messages
/// about the whole project have no subject.
pub fn split_subject(message: &str) -> (Option<&str>, &str) {
    if let Some(rest) = message.strip_prefix('[') {
        if let Some(end) = rest.find("] ") {
            return (Some(&rest[..end]), &rest[end + 2..]);
        }
    }
    (None, message)
}

#[derive(Default)]
pub struct Diagnostics {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    /// Things worth knowing that are not problems (e.g. a socket name that
    /// must exist as a node in the game's model).
    pub notes: Vec<String>,
}

impl Diagnostics {
    pub(crate) fn error(&mut self, effect: &str, msg: impl AsRef<str>) {
        self.errors.push(format!("[{}] {}", effect, msg.as_ref()));
    }
    pub(crate) fn warn(&mut self, effect: &str, msg: impl AsRef<str>) {
        self.warnings.push(format!("[{}] {}", effect, msg.as_ref()));
    }
    pub(crate) fn note(&mut self, effect: &str, msg: impl AsRef<str>) {
        self.notes.push(format!("[{}] {}", effect, msg.as_ref()));
    }

    /// Every message with its severity: errors, then warnings, then notes.
    pub fn all(&self) -> impl Iterator<Item = (Severity, &str)> {
        let errors = self.errors.iter().map(|m| (Severity::Error, m.as_str()));
        let warnings = self.warnings.iter().map(|m| (Severity::Warning, m.as_str()));
        let notes = self.notes.iter().map(|m| (Severity::Note, m.as_str()));
        errors.chain(warnings).chain(notes)
    }

    /// The messages about one subject (a layer name, `asset "id"`,
    /// `socket "name"`), without their `[subject] ` prefix.
    pub fn about<'a>(&'a self, subject: &'a str) -> impl Iterator<Item = (Severity, &'a str)> + 'a {
        self.all().filter_map(move |(severity, message)| match split_subject(message) {
            (Some(s), rest) if s == subject => Some((severity, rest)),
            _ => None,
        })
    }

    /// (errors, warnings) about one subject.
    pub fn count_about(&self, subject: &str) -> (usize, usize) {
        self.about(subject).fold((0, 0), |(e, w), (severity, _)| match severity {
            Severity::Error => (e + 1, w),
            Severity::Warning => (e, w + 1),
            Severity::Note => (e, w),
        })
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
        if !self.notes.is_empty() {
            let _ = writeln!(out, "NOTES ({}):", self.notes.len());
            for n in &self.notes {
                let _ = writeln!(out, "- {}", n);
            }
        }
        out
    }
}

fn expr_has_placeholder(e: &ExprWriterEditor) -> bool {
    let mut found = false;
    e.visit(&mut |e| found |= matches!(e, ExprWriterEditor::Placeholder));
    found
}

fn modifier_exprs(m: &ModifierEditor) -> Vec<&ExprWriterEditor> {
    m.exprs()
}

/// Where the analysis puts a socket: its offset from the target root (a
/// node's position is unknown without the model, so nodes count as the
/// target root).
fn socket_position(project: &ProjectV2, name: &str) -> Vec3 {
    socket_lookup(project, name).1
}

/// A copy of the project whose `Property(...)` reads are replaced by the
/// values the runtime would write for a still target: segment ends at their
/// socket positions, the trigger payload of a trigger without one (layer
/// origin, +Y, strength 1), visibility 1.
pub fn resolve_properties(project: &ProjectV2) -> ProjectV2 {
    resolve_for_analysis(project, &[])
}

/// Value of each parameter for the analysis: `values` where given (and of
/// the right kind), else the default; Float values clamped.
pub fn param_values(project: &ProjectV2, values: &[(String, ParamValue)]) -> Vec<(String, ParamValue)> {
    project
        .parameters
        .iter()
        .map(|p| {
            let value = values
                .iter()
                .find(|(n, v)| *n == p.name && v.kind() == p.kind())
                .map(|(_, v)| *v)
                .unwrap_or(p.default);
            (p.name.clone(), p.clamp(value))
        })
        .collect()
}

/// A copy of the project as the runtime plays it with these parameter
/// values (missing ones at their default), for the CPU simulation:
/// `Param(..)` and `Property(..)` reads become literals, `rate` × `visible`
/// becomes the spawner's count (a steady stream per second when `rate` is
/// set), a `While` layer below its threshold spawns nothing, and an
/// `OnTrigger` layer plays as if its trigger fired once at the start.
/// Reads of unknown parameters are left in place (the type check reports
/// them).
pub fn resolve_for_analysis(project: &ProjectV2, values: &[(String, ParamValue)]) -> ProjectV2 {
    let values = param_values(project, values);
    let float = |name: &str| values.iter().find(|(n, _)| n == name).and_then(|(_, v)| v.as_float());
    let mut resolved = project.clone();
    for layer in resolved.layers.iter_mut() {
        let Some(layer) = layer.as_particles_mut() else {
            continue;
        };
        let (a, b) = match &layer.attach {
            AttachDef::Segment { from, to } => (socket_position(project, from), socket_position(project, to)),
            _ => (Vec3::ZERO, Vec3::ZERO),
        };
        let visibility = match &layer.visible {
            Some(visible) => float(&visible.param).map(|v| visible.factor(v)).unwrap_or(1.0),
            None => 1.0,
        };
        for e in layer.effect.exprs_mut() {
            e.visit_mut(&mut |e| match e {
                ExprWriterEditor::Property(name) => match name.as_str() {
                    "segment_a" => *e = ExprWriterEditor::Vec3(a),
                    "segment_b" => *e = ExprWriterEditor::Vec3(b),
                    "trigger_pos" => *e = ExprWriterEditor::Vec3(Vec3::ZERO),
                    "trigger_dir" => *e = ExprWriterEditor::Vec3(Vec3::Y),
                    "trigger_strength" => *e = ExprWriterEditor::Float(1.0),
                    "visibility" => *e = ExprWriterEditor::Float(visibility),
                    _ => {}
                },
                ExprWriterEditor::Param(name) => match values.iter().find(|(n, _)| n == name) {
                    Some((_, ParamValue::Float(v))) => *e = ExprWriterEditor::Float(*v),
                    Some((_, ParamValue::Color(c))) => *e = ExprWriterEditor::Vec4(*c),
                    None => {}
                },
                _ => {}
            });
        }
        let spawner = &mut layer.effect.spawner;
        match layer.rate.as_ref().and_then(|r| r.evaluate(|n| float(n))) {
            Some(rate) => {
                spawner.count = CpuValueDef::Single(rate.max(0.0) * visibility);
                spawner.spawn_duration = CpuValueDef::Single(1.0);
                spawner.period = CpuValueDef::Single(1.0);
            }
            None if visibility < 1.0 => {
                let [lo, hi] = spawner.count.range();
                spawner.count = if lo == hi {
                    CpuValueDef::Single(lo * visibility)
                } else {
                    CpuValueDef::Uniform((lo * visibility, hi * visibility))
                };
            }
            None => {}
        }
        if let PlayDef::While(param, threshold) = &layer.play {
            if !float(param).is_some_and(|v| v >= *threshold) {
                spawner.count = CpuValueDef::Single(0.0);
            }
        }
    }
    resolved
}

/// How a layer is attached, in words.
pub(crate) fn attach_text(project: &ProjectV2, attach: &AttachDef) -> String {
    let socket = |name: &str| match project.socket(name) {
        Some(s) => match &s.node {
            Some(node) => format!("socket \"{}\" (node \"{}\" + offset {})", name, node, fmt_vec3(s.offset)),
            None => format!("socket \"{}\" (target root + offset {})", name, fmt_vec3(s.offset)),
        },
        None => format!("node \"{}\" of the target's model (target root until found)", name),
    };
    match attach {
        AttachDef::Target => "target root".into(),
        AttachDef::Socket(name) => format!("{}; the layer follows it", socket(name)),
        AttachDef::Segment { from, to } => format!(
            "segment from {} to {}; segment_a / segment_b hold the ends (simulated at {} and {})",
            socket(from),
            socket(to),
            fmt_vec3(socket_position(project, from)),
            fmt_vec3(socket_position(project, to))
        ),
    }
}

/// The project's size fit, in words.
fn fit_text(fit: Option<FitDef>) -> String {
    match fit {
        None => "none — authored size, whatever the target's scale".into(),
        Some(FitDef::TargetScale) => "TargetScale — scales with the target's scale (Local layers only)".into(),
        Some(FitDef::TargetHeight(h)) => format!(
            "TargetHeight({}) — scaled by the target's mesh height / {} m (Local layers only)",
            h, h
        ),
    }
}

fn emit_modifiers<'a>(layer: &'a ParticlesLayerDef) -> impl Iterator<Item = &'a EmitSpawnEventModifierEditor> {
    layer.effect.update_modifiers.iter().filter_map(|m| match m {
        ModifierEditor::EmitSpawnEventModifier(m) => Some(m),
        _ => None,
    })
}

/// The project's particle layers, in layer order.
fn particle_layers(project: &ProjectV2) -> Vec<&ParticlesLayerDef> {
    project.particle_layers().map(|(_, l)| l).collect()
}

/// Indices of the layers whose `parent` names `layers[index]`, in list order.
/// This order is the `child_index` used by `EmitSpawnEventModifier`.
fn children_of(layers: &[&ParticlesLayerDef], index: usize) -> Vec<usize> {
    let name = &layers[index].name;
    layers
        .iter()
        .enumerate()
        .filter(|(i, l)| *i != index && l.parent.as_deref() == Some(name.as_str()))
        .map(|(i, _)| i)
        .collect()
}

/// Index of the parent layer, if it exists and is listed before the child.
fn parent_index(layers: &[&ParticlesLayerDef], index: usize) -> Option<usize> {
    let parent = layers[index].parent.as_deref()?;
    layers[..index].iter().position(|l| l.name == parent)
}

/// The root of a layer's parent chain (the layer itself when it has none).
fn root_index(layers: &[&ParticlesLayerDef], mut index: usize) -> usize {
    for _ in 0..layers.len() {
        match parent_index(layers, index) {
            Some(p) => index = p,
            None => break,
        }
    }
    index
}

fn has_texture_sampling(effect: &ParticleEffectDef) -> bool {
    effect
        .render_modifiers
        .iter()
        .any(|m| matches!(m, RenderModifierEditor::ParticleTexture(_)))
}

/// Whether particles are oriented along their velocity (then motionless ones
/// have no orientation and may render invisible).
fn orients_along_velocity(effect: &ParticleEffectDef) -> bool {
    effect.render_modifiers.iter().any(|m| {
        matches!(m, RenderModifierEditor::Orient(o) if o.mode == OrientModeDef::AlongVelocity)
    })
}

/// Speed of the target used for the trail estimate of `Global` layers: the
/// preview motion's speed, else a running character (3.6 m/s).
pub fn target_speed(project: &ProjectV2) -> f32 {
    match project.preview.as_ref().and_then(|p| p.motion) {
        Some(PreviewMotion::Circle { speed, .. } | PreviewMotion::BackAndForth { speed, .. }) => speed,
        None => DEFAULT_TARGET_SPEED,
    }
}

/// Lifetime from which a looping `Global` layer gets a trail warning.
const TRAIL_WARN_LIFETIME: f32 = 1.0;
/// Speed of a moving target for trail estimates, in m/s.
const DEFAULT_TARGET_SPEED: f32 = 3.6;

/// The package text written by export (`*.vfx.ron`).
pub fn package_string(project: &ProjectV2) -> Result<String, String> {
    project
        .to_package()
        .to_ron_string()
        .map_err(|e| format!("export failed: {}", e))
}

fn texture_label(project: &ProjectV2, layer: &ParticlesLayerDef) -> String {
    match &layer.texture {
        None => "none".to_string(),
        Some(id) => match project.texture(id) {
            Some(t) => format!("\"{}\" ({})", id, t.path),
            None => format!("\"{}\" (missing)", id),
        },
    }
}

/// Attach checks of one layer: socket names (against the preview model's
/// nodes when there is one), segment property use.
fn check_attach(
    d: &mut Diagnostics,
    project: &ProjectV2,
    layers: &[&ParticlesLayerDef],
    index: usize,
    model: Option<&PreviewModelNodes>,
) {
    let layer = layers[index];
    let name = layer.name.as_str();
    for socket in layer.attach.socket_names() {
        if socket.is_empty() {
            d.error(name, "attach names an empty socket");
        } else if project.socket(socket).is_some() {
        } else if let Some(model) = model {
            if !model.names.iter().any(|n| n == socket) {
                d.error(
                    name,
                    format!(
                        "\"{}\" is neither a package socket nor a node {}; the layer would sit at the target root",
                        socket, model.place
                    ),
                );
            }
        } else {
            let has_model = project.preview.as_ref().is_some_and(|p| p.model.is_some());
            d.note(
                name,
                format!(
                    "\"{}\" is not a package socket, so it must be a node name in the target's model{}; until it is found the layer sits at the target root",
                    socket,
                    if has_model { "" } else { " (no preview model to check against)" }
                ),
            );
        }
    }
    let reads = layer.effect.properties_read();
    for property in reads.iter() {
        if runtime_property(property).is_none() {
            let known: Vec<&str> = RUNTIME_PROPERTIES.iter().map(|(n, _, _)| *n).collect();
            d.error(name, format!("Property(\"{}\") is not a runtime property ({})", property, known.join(", ")));
        }
    }
    let reads_segment = reads.iter().any(|p| p == "segment_a" || p == "segment_b");
    match &layer.attach {
        AttachDef::Segment { from, to } => {
            if from == to {
                d.warn(name, format!("segment from \"{}\" to itself has no length", from));
            }
            if !reads_segment {
                d.warn(
                    name,
                    "attach is Segment, but no expression reads Property(\"segment_a\") / Property(\"segment_b\"), so particles ignore the segment; e.g. position = segment_a + (segment_b - segment_a) × RandFloat",
                );
            }
        }
        _ if reads_segment => d.warn(
            name,
            "reads segment_a / segment_b, but only Segment-attached layers get them (they stay (0, 0, 0))",
        ),
        _ => {}
    }
    if let Some(pi) = parent_index(layers, index) {
        if layers[pi].attach != layer.attach {
            d.warn(
                name,
                format!(
                    "attach differs from the parent \"{}\"; child particles spawn at parent particles, so give the child the parent's attach",
                    layers[pi].name
                ),
            );
        }
    }
}

/// Subject of the messages about a parameter.
pub fn param_subject(name: &str) -> String {
    format!("parameter \"{}\"", name)
}

/// Subject of the messages about a trigger.
pub fn trigger_subject(name: &str) -> String {
    format!("trigger \"{}\"", name)
}

/// Declared parameters and triggers: names, ranges, pulses, use, preview
/// values.
fn check_params_and_triggers(d: &mut Diagnostics, project: &ProjectV2) {
    let mut properties: Vec<(String, &str)> = Vec::new();
    for (i, p) in project.parameters.iter().enumerate() {
        let label = param_subject(&p.name);
        if p.name.trim().is_empty() {
            d.errors.push("a parameter has an empty name".into());
            continue;
        }
        if project.parameters[..i].iter().any(|q| q.name == p.name) {
            d.error(&label, "duplicate parameter name; the game and the layers find parameters by name");
        }
        let property = param_property_name(&p.name);
        if let Some((_, other)) = properties.iter().find(|(prop, other)| *prop == property && **other != p.name) {
            d.error(&label, format!("same hanabi property name as parameter \"{}\"", other));
        }
        properties.push((property, &p.name));
        if p.smoothing < 0.0 || !p.smoothing.is_finite() {
            d.error(&label, format!("smoothing {} must be 0 (immediate) or more seconds", p.smoothing));
        }
        if let ParamValue::Float(default) = p.default {
            if !(p.min <= p.max) {
                d.error(&label, format!("min {} is above max {}", p.min, p.max));
            } else if default < p.min || default > p.max {
                d.warn(
                    &label,
                    format!("default {} is outside [{}, {}]; it is clamped to the range", default, p.min, p.max),
                );
            }
        }
        if project.param_users(&p.name).is_empty() {
            d.note(&label, "not used by any layer or pulse; the game can set it, but nothing changes");
        }
    }
    for (i, t) in project.triggers.iter().enumerate() {
        let label = trigger_subject(&t.name);
        if t.name.trim().is_empty() {
            d.errors.push("a trigger has an empty name".into());
            continue;
        }
        if project.triggers[..i].iter().any(|u| u.name == t.name) {
            d.error(&label, "duplicate trigger name; the game and the layers find triggers by name");
        }
        for pulse in t.pulses.iter() {
            match project.param(&pulse.param).map(|p| p.kind()) {
                None => d.error(&label, format!("pulse on \"{}\", which is not a parameter", pulse.param)),
                Some(ParamKind::Color) => d.error(
                    &label,
                    format!("pulse on \"{}\", a Color parameter; pulses work on Float parameters only", pulse.param),
                ),
                Some(ParamKind::Float) => {}
            }
            if pulse.decay < 0.0 || !pulse.decay.is_finite() {
                d.error(&label, format!("pulse decay {} must be 0 or more seconds", pulse.decay));
            }
        }
        if t.pulses.is_empty() && project.trigger_users(&t.name).is_empty() {
            d.note(&label, "no layer plays on it and it has no pulses; firing it does nothing");
        }
    }
    if let Some(preview) = &project.preview {
        for (name, value) in preview.params.iter() {
            match project.param(name) {
                None => d.warnings.push(format!("preview value for \"{}\", which is not a parameter", name)),
                Some(p) if p.kind() != value.kind() => d.warnings.push(format!(
                    "preview value for \"{}\" is a {}, but the parameter is a {}",
                    name,
                    value.kind().name(),
                    p.kind().name()
                )),
                _ => {}
            }
        }
        for (name, every) in preview.auto_triggers.iter() {
            if project.trigger(name).is_none() {
                d.warnings.push(format!("preview auto-fires \"{}\", which is not a trigger", name));
            }
            if !(*every > 0.0) {
                d.warnings.push(format!("preview auto-fires \"{}\" every {} s; use more than 0", name, every));
            }
        }
        for (name, at) in preview.animation_triggers.iter() {
            if project.trigger(name).is_none() {
                d.warnings.push(format!("preview fires \"{}\" at clip time {} s, which is not a trigger", name, at));
            }
            if !(*at >= 0.0) {
                d.warnings.push(format!("preview fires \"{}\" at clip time {} s; use 0 or more", name, at));
            }
        }
    }
}

/// A layer's parameter and trigger references: expressions, `rate`,
/// `visible`, `play`.
fn check_layer_bindings(d: &mut Diagnostics, project: &ProjectV2, layer: &ParticlesLayerDef) {
    let name = layer.name.as_str();
    for param in layer.effect.params_read() {
        if project.param(&param).is_none() {
            d.error(name, format!("Param(\"{}\") is not a parameter of this package", param));
        }
    }
    let float_param = |d: &mut Diagnostics, what: &str, param: &str| match project.param(param).map(|p| p.kind()) {
        None => d.error(name, format!("{} reads \"{}\", which is not a parameter of this package", what, param)),
        Some(ParamKind::Color) => d.error(name, format!("{} reads \"{}\", a Color parameter; it needs a Float one", what, param)),
        Some(ParamKind::Float) => {}
    };
    match &layer.rate {
        Some(Binding::Const(rate)) if *rate < 0.0 => d.error(name, format!("rate {} is below 0", rate)),
        Some(binding @ (Binding::Param(..) | Binding::Age(_))) => {
            let keys = binding.keys().unwrap_or_default();
            let over = match binding.param() {
                Some(param) => {
                    float_param(d, "rate", param);
                    "parameter value"
                }
                None => "age",
            };
            if keys.is_empty() {
                d.error(name, "rate curve has no keys");
            }
            if keys.windows(2).any(|w| w[1].0 < w[0].0) {
                d.error(name, format!("rate curve keys must be sorted by {}", over));
            }
            if keys.iter().any(|(_, v)| *v < 0.0) {
                d.error(name, "rate curve has a value below 0");
            }
            if binding.reads_age() && keys.iter().any(|(t, _)| *t < 0.0) {
                d.warn(name, "rate curve over age has keys below 0 s; the age starts at 0");
            }
        }
        _ => {}
    }
    if let Some(visible) = &layer.visible {
        float_param(d, "visible", &visible.param);
        if visible.from > visible.to {
            d.error(name, format!("visible from {} is above to {}", visible.from, visible.to));
        }
        if visible.fade < 0.0 {
            d.error(name, format!("visible fade {} must be 0 or more", visible.fade));
        }
    }
    match &layer.play {
        PlayDef::OnTrigger(trigger) => {
            if project.trigger(trigger).is_none() {
                d.error(name, format!("plays on trigger \"{}\", which is not a trigger of this package", trigger));
            }
        }
        PlayDef::While(param, _) => float_param(d, "play While", param),
        PlayDef::Loop | PlayDef::OnStart => {}
    }
    let reads = layer.effect.properties_read();
    let reads_trigger = reads.iter().any(|p| p.starts_with("trigger_"));
    if reads_trigger && layer.play.trigger().is_none() {
        d.warn(
            name,
            "reads trigger_pos / trigger_dir / trigger_strength, but only layers with play OnTrigger get them (they stay at the layer origin, +Y, 1)",
        );
    }
    if reads.iter().any(|p| p == "visibility") && layer.visible.is_none() {
        d.warn(name, "reads Property(\"visibility\"), but the layer has no visible range, so it stays 1");
    }
}

/// Texture settings that do not fit the purpose, sprite-sheet grids, and
/// files Bevy loads in a form particles cannot sample.
fn check_texture_settings(d: &mut Diagnostics, label: &str, t: &TextureAssetDef, assets_root: &Path) {
    use TexturePurpose as P;
    match (t.purpose, t.wrap) {
        (P::Noise, Some(TextureWrap::Clamp)) => d.warn(
            label,
            "Noise with wrap Clamp: scrolled or tiled noise shows stretched edges; use Repeat (the Noise default)",
        ),
        (P::Gradient, Some(TextureWrap::Repeat)) => {
            d.warn(label, "Gradient with wrap Repeat: the ends of the ramp bleed into each other; use Clamp")
        }
        _ => {}
    }
    if matches!(t.purpose, P::Noise | P::Mask) && t.color_space == Some(TextureColorSpace::Srgb) {
        d.warn(
            label,
            format!(
                "{} with colour space Srgb: data textures should be Linear, or their values are bent (the {} default)",
                t.purpose.name(),
                t.purpose.name()
            ),
        );
    }
    let info = library::texture_info_at(&t.path, assets_root).ok();
    if let Some((rows, cols)) = t.grid {
        if rows == 0 || cols == 0 {
            d.error(label, format!("grid ({}, {}) needs at least 1 row and 1 column", rows, cols));
        } else {
            if t.purpose != P::Sprite {
                d.warn(label, format!("grid is for sprite sheets (flipbooks), but the purpose is {}", t.purpose.name()));
            }
            if let Some(info) = &info {
                if info.width % cols != 0 || info.height % rows != 0 {
                    d.warn(
                        label,
                        format!(
                            "grid {} rows × {} columns does not divide the {}×{} image evenly; frames will be cut off",
                            rows, cols, info.width, info.height
                        ),
                    );
                }
            }
        }
    }
    if let Some(info) = &info {
        if info.loads_as_integer_texture() {
            d.warn(
                label,
                format!(
                    "{} is a 16-bit grayscale PNG: Bevy loads it as an integer texture that cannot be sampled; save it as 8-bit (extract channel Luminance writes one)",
                    t.path
                ),
            );
        }
    }
}

/// Whether an expression reads a random value.
fn expr_is_random(e: &ExprWriterEditor) -> bool {
    let mut random = false;
    e.visit(&mut |e| {
        random |= match e {
            ExprWriterEditor::RandFloat | ExprWriterEditor::RandVec3 | ExprWriterEditor::RandU32 | ExprWriterEditor::Rand(_) => true,
            ExprWriterEditor::Operator(op) => matches!(**op, ExprOperatorEditor::Uniform(..) | ExprOperatorEditor::Normal(..)),
            _ => false,
        }
    });
    random
}

/// The largest particle size (metres, or pixels with ScreenSpaceSize) the
/// render modifiers before `before` (an index into the render list) leave,
/// when it is known without simulating.
fn size_before(effect: &ParticleEffectDef, before: usize) -> Option<f32> {
    let mut size = None;
    for m in effect.init_modifiers.iter() {
        if let ModifierEditor::SetAttribute(s) = m {
            if s.attr == Attribute::SIZE {
                size = s.attr_expr.scalar_range().map(|(_, hi)| hi);
            }
        }
    }
    for m in effect.render_modifiers.iter().take(before) {
        match m {
            RenderModifierEditor::SizeOverLifetime(s) => {
                size = s.gradient.g.iter().map(|(_, v)| v.max_element()).reduce(f32::max);
            }
            RenderModifierEditor::SetSize(s) => {
                size = Some(match s.size {
                    CpuVecDef::Single(v) => v.max_element(),
                    CpuVecDef::Uniform((a, b)) => a.max(b).max_element(),
                });
            }
            _ => {}
        }
    }
    Some(size.unwrap_or(1.0))
}

/// Render modifiers: the flipbook's grid and frames, settings bevy_hanabi
/// ignores, random values at render time, pixel sizes.
fn check_render_modifiers(d: &mut Diagnostics, project: &ProjectV2, layer: &ParticlesLayerDef) {
    let name = layer.name.as_str();
    let effect = &layer.effect;
    let writes = |attr: Attribute| {
        effect.init_modifiers.iter().chain(effect.update_modifiers.iter()).any(|m| match m {
            ModifierEditor::SetAttribute(s) => s.attr == attr,
            ModifierEditor::InheritAttribute(s) => s.attr == attr,
            _ => false,
        })
    };
    for m in effect.init_modifiers.iter() {
        if let ModifierEditor::SetPositionCone3d(c) = m {
            if c.dimension == ShapeDimension::Surface {
                d.warn(name, "SetPositionCone3d always fills the cone's volume (bevy_hanabi ignores dimension Surface); use Volume");
            }
        }
    }
    for (index, m) in effect.render_modifiers.iter().enumerate() {
        match m {
            RenderModifierEditor::Flipbook(f) => {
                match f.grid_for(project.texture_grid(layer)) {
                    None => d.warn(
                        name,
                        "Flipbook has no grid: set the texture asset's grid (rows × columns, ASSETS tab) or the modifier's own; until then the whole texture is one frame",
                    ),
                    Some((rows, cols)) if rows == 0 || cols == 0 => {
                        d.error(name, format!("Flipbook grid ({}, {}) needs at least 1 row and 1 column", rows, cols))
                    }
                    Some(_) => {}
                }
                if !has_texture_sampling(effect) {
                    d.warn(name, "Flipbook picks frames of the layer's texture, but no ParticleTexture render modifier draws it");
                }
                if !writes(Attribute::SPRITE_INDEX) {
                    d.warn(
                        name,
                        "Flipbook draws frame sprite_index, but nothing sets it, so every particle shows frame 0; add an update SetAttribute(\"sprite_index\") that steps through the frames (editor: Update + \"Flipbook frames over lifetime\"; guide section \"render\")",
                    );
                }
            }
            RenderModifierEditor::Orient(o) if o.rotation_expr.is_some() && o.mode == OrientModeDef::AlongVelocity => d.warn(
                name,
                "Orient rotation is ignored with AlongVelocity (bevy_hanabi rotates ParallelCameraDepthPlane and FaceCameraPosition only)",
            ),
            RenderModifierEditor::SetColor(c) if matches!(c.color, CpuVecDef::Uniform(_)) => d.warn(
                name,
                "SetColor with a random range: bevy_hanabi's render shader has no per-particle random seed, so every particle gets the same colour; set hdr_color in an init modifier for per-particle colours",
            ),
            RenderModifierEditor::SetSize(s) if matches!(s.size, CpuVecDef::Uniform(_)) => d.warn(
                name,
                "SetSize with a random range: bevy_hanabi's render shader has no per-particle random seed, so every particle gets the same size; set size in an init modifier for per-particle sizes",
            ),
            RenderModifierEditor::ScreenSpaceSize(_) => {
                if let Some(size) = size_before(effect, index) {
                    if size < 2.0 {
                        d.warn(
                            name,
                            format!(
                                "ScreenSpaceSize reads the particle size as screen pixels, but the size before it is about {}: the particles are smaller than 2 pixels (sizes like 8.0 to 32.0 work)",
                                fmt_num(size)
                            ),
                        );
                    }
                }
            }
            _ => {}
        }
        if m.exprs().into_iter().any(expr_is_random) {
            d.warn(
                name,
                format!(
                    "{} reads a random value, but at render time bevy_hanabi gives every particle the same one; set a random attribute at birth (e.g. f32_0) and read it with Attr",
                    m.kind_name()
                ),
            );
        }
    }
}

/// The nodes sockets can resolve to: those below the preview instance's
/// target in the preview model(s).
pub struct PreviewModelNodes {
    pub names: Vec<String>,
    /// "of the preview model <file>" / "below <node> in ...", for messages.
    pub place: String,
}

/// Checks the preview block's model settings against the files in the game
/// assets folder (model, weapon, animation clip and its skeleton, animation
/// triggers, attach_to) and returns the node names sockets can use, when a
/// model is there to check against.
pub fn check_preview_model(d: &mut Diagnostics, project: &ProjectV2, assets_root: &Path) -> Option<PreviewModelNodes> {
    use crate::model_info::ModelInfo;
    let preview = project.preview.as_ref()?;
    let load = |d: &mut Diagnostics, what: &str, path: &str| -> Option<std::sync::Arc<ModelInfo>> {
        let file = assets_root.join(path);
        if !file.is_file() {
            d.warn(
                "preview",
                format!(
                    "{} {} is not in the game assets folder ({}); the editor shows the capsule and sockets are not checked against it",
                    what,
                    path,
                    assets_root.display()
                ),
            );
            return None;
        }
        match ModelInfo::cached(&file) {
            Ok(info) => Some(info),
            Err(e) => {
                d.error("preview", format!("{} {}", what, e));
                None
            }
        }
    };
    let body = preview.model.as_deref().and_then(|m| load(d, "model", m));
    let weapon_def = preview.weapon.as_ref().filter(|w| !w.model.is_empty());
    let weapon = weapon_def.and_then(|w| load(d, "weapon", &w.model));
    if let (Some(w), Some(body)) = (weapon_def, &body) {
        if let Some(node) = w.node.as_deref().filter(|n| body.node(n).is_none()) {
            d.error(
                "preview",
                format!("weapon node \"{}\" is not a node of the model {}", node, preview.model.as_deref().unwrap_or_default()),
            );
        }
    }
    if preview.weapon.as_ref().is_some_and(|w| w.model.is_empty()) {
        d.warn("preview", "the weapon has no model file");
    }

    // The animation clip and whether it fits the model's skeleton.
    if let Some(clip_name) = &preview.animation {
        match preview.animation_source() {
            None => d.warn("preview", format!("animation \"{}\" without a model or animation_file", clip_name)),
            Some(source) => {
                let info = if Some(source) == preview.model.as_deref() { body.clone() } else { load(d, "animation file", source) };
                if let Some(info) = info {
                    match info.animation(clip_name) {
                        None => {
                            let known: Vec<String> = info.animations.iter().map(|a| format!("\"{}\"", a.label())).collect();
                            d.error(
                                "preview",
                                format!(
                                    "{} has no animation \"{}\" ({})",
                                    source,
                                    clip_name,
                                    if known.is_empty() { "it has none".into() } else { format!("it has {}", known.join(", ")) }
                                ),
                            );
                        }
                        Some(clip) => {
                            if let Some(body) = &body {
                                let matching = body.matching_targets(clip);
                                if matching < clip.targets.len() {
                                    d.warn(
                                        "preview",
                                        format!(
                                            "{} of the {} nodes animation \"{}\" ({}) moves are not in the model {} (Bevy matches them by their name path from the scene root): they stay still",
                                            clip.targets.len() - matching,
                                            clip.targets.len(),
                                            clip_name,
                                            source,
                                            preview.model.as_deref().unwrap_or_default()
                                        ),
                                    );
                                }
                            }
                            for (name, at) in preview.animation_triggers.iter() {
                                if *at > clip.duration + 1e-4 {
                                    d.warn(
                                        "preview",
                                        format!(
                                            "\"{}\" fires at clip time {} s, after the end of the {:.2} s clip: it never fires",
                                            name, at, clip.duration
                                        ),
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    } else if !preview.animation_triggers.is_empty() {
        d.warn("preview", "animation triggers are set, but no animation plays");
    }

    // The nodes below the instance target.
    let (info, file) = match preview.target {
        PreviewTargetModel::Weapon => match (&weapon, weapon_def) {
            (Some(info), Some(w)) => (info.clone(), w.model.clone()),
            _ => {
                if weapon_def.is_none() {
                    d.error("preview", "target is Weapon, but there is no weapon");
                }
                return None;
            }
        },
        PreviewTargetModel::Body => (body.clone()?, preview.model.clone().unwrap_or_default()),
    };
    let (mut names, place): (Vec<String>, String) = match &preview.attach_to {
        None => (info.nodes.iter().map(|n| n.name.clone()).collect(), format!("of the preview model {}", file)),
        Some(node) => match info.node(node) {
            None => {
                d.error(
                    "preview",
                    format!("attach_to \"{}\" is not a node of the preview model {} (describe_model lists them)", node, file),
                );
                return None;
            }
            Some(at) => (
                (0..info.nodes.len())
                    .filter(|&i| i != at && info.is_under(i, at))
                    .map(|i| info.nodes[i].name.clone())
                    .collect(),
                format!("below \"{}\" in the preview model {}", node, file),
            ),
        },
    };
    // A weapon held below the body target is part of its hierarchy too.
    if let (PreviewTargetModel::Body, Some(w), Some(winfo)) = (preview.target, weapon_def, &weapon) {
        let held = match (&w.node, &preview.attach_to) {
            (_, None) => true,
            (Some(n), Some(_)) => names.contains(n),
            (None, Some(_)) => false,
        };
        if held {
            names.extend(winfo.nodes.iter().map(|n| n.name.clone()));
        }
    }
    Some(PreviewModelNodes { names, place })
}

/// Runs every static check on a project: structure, assets, expression types,
/// the bevy_hanabi shader generation and a package round trip. Relative
/// texture paths are looked up in `assets_root` (the game assets folder).
pub fn check_project(project: &ProjectV2, assets_root: &Path) -> Diagnostics {
    let mut d = Diagnostics::default();
    let layers = particle_layers(project);
    let resolved = resolve_for_analysis(project, &[]);
    let resolved_layers = particle_layers(&resolved);

    if project.layers.is_empty() {
        d.errors.push("project has no layers".into());
        return d;
    }

    for (i, asset) in project.assets.iter().enumerate() {
        let label = format!("asset \"{}\"", asset.id);
        if asset.id.is_empty() {
            d.errors.push("an asset has an empty id".into());
        }
        if project.assets[..i].iter().any(|a| a.id == asset.id) {
            d.error(&label, "duplicate asset id; layers find assets by id");
        }
        let AssetKind::Texture(t) = &asset.kind;
        if let Some(file) = t.path.strip_prefix(BUILTIN_TEXTURES_PREFIX) {
            if !BUILTIN_TEXTURES.iter().any(|(f, _)| *f == file) {
                let files: Vec<&str> = BUILTIN_TEXTURES.iter().map(|(f, _)| *f).collect();
                d.error(&label, format!("{} is not a built-in texture ({})", t.path, files.join(", ")));
            }
        } else if t.path.is_empty() {
            d.error(&label, "texture path is empty");
        } else {
            match library::path_problem(&t.path) {
                Err(e) => d.error(&label, e),
                Ok(Some(w)) => d.warn(&label, w),
                Ok(None) => {}
            }
            let ext = library::extension(&t.path);
            if !library::TEXTURE_EXTENSIONS.contains(&ext.as_str()) {
                d.warn(
                    &label,
                    format!("{}: .{} is not a texture type Omagari imports (PNG, JPG, KTX2); Bevy may not load it", t.path, ext),
                );
            } else if ext == "jpg" || ext == "jpeg" {
                d.note(&label, "JPG: the game needs Bevy's `jpeg` feature to load it (PNG works by default)");
            }
            if !assets_root.join(&t.path).is_file() {
                d.warn(
                    &label,
                    format!("{} is not in the game assets folder ({}); it will not load", t.path, assets_root.display()),
                );
            }
        }
        check_texture_settings(&mut d, &label, t, assets_root);
        if project.asset_users(&asset.id).is_empty() {
            d.warn(&label, "not used by any layer");
        }
    }

    check_params_and_triggers(&mut d, project);
    let model_nodes = check_preview_model(&mut d, project, assets_root);

    for (i, socket) in project.sockets.iter().enumerate() {
        let label = format!("socket \"{}\"", socket.name);
        if socket.name.is_empty() {
            d.errors.push("a socket has an empty name".into());
        }
        if project.sockets[..i].iter().any(|s| s.name == socket.name) {
            d.error(&label, "duplicate socket name; layers find sockets by name");
        }
        if socket.node.as_deref() == Some("") {
            d.error(&label, "node is an empty name; use None for the target root");
        } else if let (Some(node), Some(model)) = (&socket.node, &model_nodes) {
            if !model.names.iter().any(|n| n == node) {
                d.error(&label, format!("node \"{}\" is not a node {}; the socket would sit at the target root", node, model.place));
            }
        }
        let used = layers.iter().any(|l| l.attach.socket_names().contains(&socket.name.as_str()))
            || project.shape_layers().any(|(_, l)| l.attach.socket_names().contains(&socket.name.as_str()));
        if !used {
            d.warn(&label, "not used by any layer");
        }
    }

    match project.fit {
        Some(FitDef::TargetHeight(h)) if !(h > 0.0) => {
            d.errors.push(format!("fit TargetHeight({}) needs a reference height above 0 (metres)", h));
        }
        Some(_) => {
            for layer in layers.iter().filter(|l| l.simulation_space == SimulationSpaceDef::Global) {
                d.warn(
                    &layer.name,
                    "fit does not scale Global layers (bevy_hanabi applies only the emitter position to them); use Local or no fit",
                );
            }
        }
        None => {}
    }

    for (i, layer) in layers.iter().enumerate() {
        let name = layer.name.as_str();
        let effect = &layer.effect;

        if layers[..i].iter().any(|l| l.name == layer.name) {
            d.error(name, "duplicate layer name; names must be unique (parents are found by name)");
        }
        if effect.capacity == 0 {
            d.error(name, "capacity is 0, nothing can spawn");
        }

        let samples_texture = has_texture_sampling(effect);
        match &layer.texture {
            Some(id) => {
                if project.texture(id).is_none() {
                    d.error(name, format!("texture \"{}\" is not in the assets list", id));
                } else if !samples_texture {
                    d.warn(name, format!("texture \"{}\" is set, but no ParticleTexture render modifier samples it", id));
                }
            }
            None if samples_texture => {
                d.warn(name, "ParticleTexture samples the layer's texture, but the layer has none")
            }
            None => {}
        }

        check_attach(&mut d, project, &layers, i, model_nodes.as_ref());
        check_layer_bindings(&mut d, project, layer);

        if let Some(parent) = &layer.parent {
            if parent == name {
                d.error(name, "layer is its own parent");
            } else if let Some(pi) = parent_index(&layers, i) {
                let child_index = children_of(&layers, pi).iter().position(|c| *c == i).unwrap() as u32;
                if !emit_modifiers(layers[pi]).any(|m| m.child_index == child_index) {
                    d.error(
                        name,
                        format!(
                            "parent \"{}\" has no EmitSpawnEventModifier with child_index {}, so this layer never spawns",
                            parent, child_index
                        ),
                    );
                }
            } else if layers.iter().any(|l| &l.name == parent) {
                d.error(name, format!("parent \"{}\" must be listed before this layer", parent));
            } else {
                d.error(name, format!("parent \"{}\" is not a particle layer of this project", parent));
            }
        } else {
            let s = &effect.spawner;
            if s.count.range()[1] <= 0.0 {
                d.warn(name, "spawner count is 0, nothing spawns");
            }
            if !s.starts_active && !layer.play.is_driven() {
                d.warn(name, "spawner starts_active is false; game code must activate it");
            }
            if s.cycle_count == 0 && s.period.range()[1] <= 0.0 && s.spawn_duration.range()[1] <= 0.0 {
                d.warn(name, "spawner repeats forever with period 0 and duration 0: it bursts `count` particles every frame");
            }
            match &layer.play {
                PlayDef::Loop if s.cycle_count != 0 => d.warn(
                    name,
                    format!("play is Loop, but the spawner stops after {} cycle(s); use cycle_count 0 or play OnStart", s.cycle_count),
                ),
                PlayDef::OnStart if s.cycle_count == 0 => {
                    d.warn(name, "play is OnStart, but the spawner repeats forever (cycle_count 0); use play Loop")
                }
                PlayDef::OnTrigger(trigger) if s.cycle_count == 0 => d.warn(
                    name,
                    format!(
                        "play is OnTrigger(\"{}\"), but the spawner repeats forever (cycle_count 0): after the first trigger it never stops; use a finite cycle_count (cycle_count 1 = one burst per trigger)",
                        trigger
                    ),
                ),
                _ => {}
            }
            if layer.rate.is_some() && s.cycle_count != 0 {
                d.warn(
                    name,
                    format!(
                        "rate is a steady stream per second, but the spawner runs {} cycle(s) only; rate applies only while they run (use cycle_count 0, or bind nothing and use the spawner's count for bursts)",
                        s.cycle_count
                    ),
                );
            }
        }
        if layer.parent.is_some() {
            if layer.play.is_driven() {
                d.warn(name, "play OnTrigger / While is ignored on a child layer: its particles are spawned by the parent's particles; give the parent the play mode");
            }
            if layer.rate.is_some() {
                d.warn(name, "rate is ignored on a child layer: its particles are spawned by the parent's particles");
            }
        }

        let n_children = children_of(&layers, i).len() as u32;
        for m in emit_modifiers(layer) {
            if m.child_index >= n_children {
                d.error(
                    name,
                    format!(
                        "EmitSpawnEventModifier child_index {} but this layer has {} child layer(s)",
                        m.child_index, n_children
                    ),
                );
            }
        }

        for m in effect.init_modifiers.iter() {
            if !m.contexts().contains(ModifierContext::Init) {
                d.error(name, format!("{} is an update modifier but is listed in init_modifiers; it is ignored", modifier_name(m)));
            }
        }
        for m in effect.update_modifiers.iter() {
            if !m.contexts().contains(ModifierContext::Update) {
                d.error(name, format!("{} is an init modifier but is listed in update_modifiers; it is ignored", modifier_name(m)));
            }
        }
        for m in effect.init_modifiers.iter().chain(effect.update_modifiers.iter()) {
            if modifier_exprs(m).into_iter().any(expr_has_placeholder) {
                d.warn(name, format!("{} has an empty Placeholder expression (evaluates to f32 0.0)", modifier_name(m)));
            }
        }
        for m in effect.render_modifiers.iter() {
            if m.exprs().into_iter().any(expr_has_placeholder) {
                d.warn(name, format!("{} has an empty Placeholder expression (evaluates to f32 0.0)", m.kind_name()));
            }
        }

        let errors_before = d.errors.len();

        // Type check with one dry-run particle, parameters at their defaults.
        // Expressions have no branches, so one evaluation visits every node.
        let mut rng = StdRng::seed_from_u64(0);
        let parent_attrs = layer.parent.as_ref().map(|_| Attrs::default());
        let mut p = Attrs::default();
        let dry = &resolved_layers[i].effect;
        for m in dry.init_modifiers.iter().filter(|m| m.contexts().contains(ModifierContext::Init)) {
            if let Err(e) = apply_init(m, &mut p, parent_attrs.as_ref(), &mut rng, 0.0) {
                d.error(name, format!("init {}: {}", modifier_name(m), e));
            }
        }
        let mut emissions = Vec::new();
        let mut alive = true;
        for m in dry.update_modifiers.iter().filter(|m| m.contexts().contains(ModifierContext::Update)) {
            if let Err(e) = apply_update(m, &mut p, parent_attrs.as_ref(), &mut rng, 0.0, true, &mut alive, &mut emissions) {
                d.error(name, format!("update {}: {}", modifier_name(m), e));
            }
        }
        for m in dry.render_modifiers.iter() {
            let result = match m {
                RenderModifierEditor::Round(r) => check_render_expr(&r.roundness_expr, "Round roundness", &p),
                RenderModifierEditor::Orient(o) => match &o.rotation_expr {
                    Some(rotation) => check_render_expr(rotation, "Orient rotation", &p),
                    None => Ok(()),
                },
                _ => Ok(()),
            };
            if let Err(e) = result {
                d.error(name, format!("render {}: {}", m.kind_name(), e));
            }
        }

        // Let bevy_hanabi generate the shaders, which catches missing attributes
        // and expression errors that would otherwise only show up at runtime.
        // Skipped when the checks above already failed, to avoid duplicates.
        // Wrong-list modifiers were reported above, so produce()'s own
        // warnings are not repeated.
        let asset = layer.produce_in(project, &mut Vec::new());
        if d.errors.len() == errors_before {
            let parent_layout =
                parent_index(&layers, i).map(|pi| layers[pi].produce_in(project, &mut Vec::new()).particle_layout());
            if let Err(e) = EffectShaderSources::generate(&asset, parent_layout.as_ref(), n_children) {
                d.error(name, format!("bevy_hanabi shader generation failed: {}", e));
            }
        }
        check_render_modifiers(&mut d, project, layer);
        let sets_velocity = effect.init_modifiers.iter().chain(effect.update_modifiers.iter()).any(|m| match m {
            ModifierEditor::SetAttribute(m) => m.attr == Attribute::VELOCITY,
            ModifierEditor::InheritAttribute(m) => m.attr == Attribute::VELOCITY,
            ModifierEditor::SetVelocityCircle(_)
            | ModifierEditor::SetVelocitySphere(_)
            | ModifierEditor::SetVelocityTangent(_) => true,
            _ => false,
        });
        let accelerates = effect.update_modifiers.iter().any(|m| {
            matches!(
                m,
                ModifierEditor::AccelModifier(_)
                    | ModifierEditor::ConformToSphereModifier(_)
                    | ModifierEditor::RadialAccel(_)
                    | ModifierEditor::TangentAccel(_)
            )
        });
        if !effect
            .render_modifiers
            .iter()
            .any(|m| matches!(m, RenderModifierEditor::Orient(_)))
        {
            d.warn(name, "no Orient render modifier: particles are flat quads in the XY plane and vanish when seen from the side; add Orient((mode: ParallelCameraDepthPlane)) or AlongVelocity");
        }
        if !sets_velocity && !accelerates && orients_along_velocity(effect) {
            d.warn(name, "nothing sets a velocity, but Orient(AlongVelocity) orients particles along it, so motionless particles may render invisible (use another orient mode)");
        }
        if effect.motion_integration == MotionIntegrationDef::None && (sets_velocity || accelerates) {
            let moves_by_position = effect.update_modifiers.iter().any(|m| {
                matches!(m, ModifierEditor::SetAttribute(s) if s.attr == Attribute::POSITION)
                    || matches!(m, ModifierEditor::SetPositionCircle(_) | ModifierEditor::SetPositionSphere(_))
            });
            if !moves_by_position {
                d.warn(
                    name,
                    "motion_integration is None, so velocities do not move particles (they stay where they are born); use PostUpdate, or set the position in an update modifier",
                );
            }
        }
        let layout = asset.particle_layout();
        if !layout.contains(Attribute::AGE) {
            d.warn(name, "particles never die: nothing uses the age attribute (set age to 0.0 in init, or add ColorOverLifetime/SizeOverLifetime)");
        }

        if layer.simulation_space == SimulationSpaceDef::Global {
            let root = layers[root_index(&layers, i)];
            let lifetime = effect.max_lifetime();
            if root.effect.spawner.cycle_count == 0 && lifetime.is_some_and(|l| l >= TRAIL_WARN_LIFETIME) {
                let lifetime = lifetime.unwrap();
                let speed = target_speed(project);
                d.warn(
                    name,
                    format!(
                        "Global simulation space, looping, particles live up to {}s: on a target moving at {} m/s they leave a trail ~{} m long; use Local to keep them on the target",
                        fmt_num(lifetime),
                        fmt_num(speed),
                        fmt_num(speed * lifetime)
                    ),
                );
            }
        }
    }

    super::surface_check::check_surface_layers(&mut d, project, assets_root);
    super::shape_check::check_shape_layers(&mut d, project, model_nodes.as_ref());

    match package_string(project) {
        Ok(text) => match ProjectV2::from_ron_str_with_warnings(&text) {
            Ok((reloaded, _)) => {
                if reloaded.to_ron_string().ok().as_deref() != Some(text.as_str()) {
                    d.errors.push("exported package does not load back unchanged".into());
                }
            }
            Err(e) => d.errors.push(format!("exported package does not load back: {}", e)),
        },
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
    effect: &ParticleEffectDef,
    mortal: bool,
    parent: Option<&Attrs>,
    birth: f32,
    max_time: f32,
    rng: &mut StdRng,
) -> Result<Sample, String> {
    let mut p = Attrs::default();
    for m in effect.init_modifiers.iter().filter(|m| m.contexts().contains(ModifierContext::Init)) {
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
    let integrate = |p: &mut Attrs| {
        let pos = p.vec3(Attribute::POSITION) + p.vec3(Attribute::VELOCITY) * DT;
        p.set(Attribute::POSITION, Val::V3(pos));
    };
    while step < steps {
        // As bevy_hanabi: alive while age < lifetime (re-read, an update
        // modifier may change it), and kill modifiers clear the flag.
        let was_alive = !mortal || age < p.f32(Attribute::LIFETIME);
        age += DT;
        p.set(Attribute::AGE, Val::F(age));
        let mut is_alive = !mortal || age < p.f32(Attribute::LIFETIME);
        let time = birth + age;

        emissions.clear();
        if effect.motion_integration == MotionIntegrationDef::PreUpdate {
            integrate(&mut p);
        }
        for m in effect.update_modifiers.iter().filter(|m| m.contexts().contains(ModifierContext::Update)) {
            apply_update(m, &mut p, parent, rng, time, was_alive, &mut is_alive, &mut emissions)?;
        }
        if mortal {
            is_alive &= age < p.f32(Attribute::LIFETIME);
        }
        if effect.motion_integration == MotionIntegrationDef::PostUpdate {
            integrate(&mut p);
        }
        let vel = p.vec3(Attribute::VELOCITY);
        let pos = p.vec3(Attribute::POSITION);
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
fn max_lifetime(layer: &ParticlesLayerDef, mortal: bool, rng: &mut StdRng) -> f32 {
    if !mortal {
        return MAX_PARTICLE_TIME;
    }
    let parent = layer.parent.as_ref().map(|_| Attrs::default());
    let mut max: f32 = 0.0;
    for _ in 0..64 {
        let mut p = Attrs::default();
        for m in layer.effect.init_modifiers.iter() {
            if apply_init(m, &mut p, parent.as_ref(), rng, 0.0).is_err() {
                return 1.0;
            }
        }
        max = max.max(p.f32(Attribute::LIFETIME));
    }
    max.clamp(DT, MAX_PARTICLE_TIME)
}

fn simulate_project(project: &ProjectV2, n_samples: usize, seed: u64) -> (Vec<EffectSim>, f32) {
    let effects = &particle_layers(project)[..];
    let mut rng = StdRng::seed_from_u64(seed);

    let mortal: Vec<bool> = effects
        .iter()
        .map(|e| {
            let layout = e.produce_in(project, &mut Vec::new()).particle_layout();
            layout.contains(Attribute::AGE) && layout.contains(Attribute::LIFETIME)
        })
        .collect();

    // Longest chain of lifetimes (a child lives on after its parent particle).
    let mut chain_life = vec![0.0f32; effects.len()];
    for i in 0..effects.len() {
        let own = max_lifetime(effects[i], mortal[i], &mut rng);
        chain_life[i] = own + parent_index(effects, i).map(|pi| chain_life[pi]).unwrap_or(0.0);
    }
    let longest_chain = chain_life.iter().cloned().fold(0.0, f32::max);

    // The timeline holds the longest emission plus the longest particle chain.
    // Spawners that repeat forever are shown long enough to reach a steady state.
    let mut emission_end: f32 = 0.0;
    for e in effects.iter().filter(|e| e.parent.is_none()) {
        let s = &e.effect.spawner;
        let duration = s.spawn_duration.range()[1].max(0.0);
        let period = s.period.range()[1].max(duration);
        let emission = match s.cycle_count {
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
                let (births, text, forever) = spawner_schedule(&effect.effect.spawner.to_settings(), horizon);
                sim.spawn_text = text;
                sim.forever = forever;
                for b in weighted_picks(&births, n_samples, &mut rng) {
                    let t = (b as f32 + rng.random::<f32>()) * DT;
                    sim.samples.push(simulate_particle(&effect.effect, mortal, None, t, particle_time(mortal, t), &mut rng)?);
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
                for m in emit_modifiers(effects[pi]).filter(|m| m.child_index == child_index) {
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
                    sim.samples.push(simulate_particle(&effect.effect, mortal, Some(&state), t, particle_time(mortal, t), &mut rng)?);
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
    // Positions are simulated relative to each layer; report them relative to
    // the target (a socket layer sits at its socket).
    for (sim, effect) in sims.iter_mut().zip(effects) {
        let origin = match &effect.attach {
            AttachDef::Socket(name) => socket_position(project, name),
            _ => Vec3::ZERO,
        };
        if origin == Vec3::ZERO {
            continue;
        }
        for sample in sim.samples.iter_mut() {
            sample.birth_pos += origin;
            sample.end_pos += origin;
            for p in sample.path.iter_mut() {
                *p += origin;
            }
        }
    }
    (sims, horizon)
}

// ---------------------------------------------------------------------------
// Report
// ---------------------------------------------------------------------------

pub(crate) fn fmt_num(x: f32) -> String {
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
pub(crate) fn color_name(c: Vec3) -> String {
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

/// `value` combined with `base` like bevy_hanabi's colour render modifiers:
/// `blend` on the `mask` channels, the others unchanged.
fn blend_color(base: Vec4, value: Vec4, blend: ColorBlendMode, mask: ColorBlendMask) -> Vec4 {
    let mut out = base;
    for (i, bit) in [ColorBlendMask::R, ColorBlendMask::G, ColorBlendMask::B, ColorBlendMask::A].into_iter().enumerate() {
        if mask.contains(bit) {
            out[i] = match blend {
                ColorBlendMode::Overwrite => value[i],
                ColorBlendMode::Modulate => base[i] * value[i],
                ColorBlendMode::Add => base[i] + value[i],
            };
        }
    }
    out
}

/// The colour a particle born with `base` is drawn with at life fraction
/// `t`: the colour render modifiers in list order (SetColor ranges at their
/// middle).
fn render_color(effect: &ParticleEffectDef, base: Vec4, t: f32) -> Vec4 {
    let mut color = base;
    for m in effect.render_modifiers.iter() {
        match m {
            RenderModifierEditor::ColorOverLifetime(c) => {
                color = blend_color(color, gradient_eval4(&c.gradient.g, t), c.blend.unwrap_or_default(), c.mask.unwrap_or_default());
            }
            RenderModifierEditor::SetColor(c) => {
                let value = match c.color {
                    CpuVecDef::Single(v) => v,
                    CpuVecDef::Uniform((a, b)) => (a + b) * 0.5,
                };
                color = blend_color(color, value, c.blend, c.mask);
            }
            _ => {}
        }
    }
    color
}

fn describe_color(effect: &ParticleEffectDef, samples: &[Sample], out: &mut String) {
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

    let final_at = |t: f32| -> Vec4 { render_color(effect, base, t) };
    let set_color = effect.render_modifiers.iter().find_map(|m| match m {
        RenderModifierEditor::SetColor(c) => Some(c),
        _ => None,
    });
    if let Some(c) = set_color {
        let value = match c.color {
            CpuVecDef::Single(v) => v,
            CpuVecDef::Uniform((a, b)) => (a + b) * 0.5,
        };
        let _ = writeln!(
            out,
            "  SetColor (render): {:?} by ({}, {}, {}, {}) on {}",
            c.blend,
            fmt_num(value.x),
            fmt_num(value.y),
            fmt_num(value.z),
            fmt_num(value.w),
            mask_text(c.mask)
        );
    }

    match color_mod {
        None => {
            let drawn = final_at(0.5);
            let _ = writeln!(
                out,
                "  color: {}{} alpha {} for the whole life (no ColorOverLifetime)",
                color_name(drawn.truncate()),
                if varied { " (varies per particle)" } else { "" },
                fmt_num(drawn.w)
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

fn describe_size(effect: &ParticleEffectDef, samples: &[Sample], out: &mut String) {
    let unit = if effect.render_modifiers.iter().any(|m| matches!(m, RenderModifierEditor::ScreenSpaceSize(_))) {
        "screen pixels (ScreenSpaceSize: the same size on screen at any distance)"
    } else {
        "world units"
    };
    // The last size render modifier wins.
    let last_size = effect.render_modifiers.iter().rev().find(|m| {
        matches!(m, RenderModifierEditor::SizeOverLifetime(_) | RenderModifierEditor::SetSize(_))
    });
    if let Some(RenderModifierEditor::SetSize(s)) = last_size {
        let text = match s.size {
            CpuVecDef::Single(v) => fmt_vec3(v),
            CpuVecDef::Uniform((a, b)) => format!("{}–{} (one value for every particle)", fmt_vec3(a), fmt_vec3(b)),
        };
        let _ = writeln!(out, "  size: SetSize (render) {} {}", text, unit);
        return;
    }
    let size_mod = match last_size {
        Some(RenderModifierEditor::SizeOverLifetime(s)) => Some(s),
        _ => None,
    };
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
            let _ = writeln!(out, "  size: SizeOverLifetime ({}) {}", unit, stops.join(" → "));
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
                let _ = writeln!(out, "  size: {} {}", range_text(&sizes, ""), unit);
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

/// Blending, texture sampling, orientation and the other render modifiers
/// and asset settings of a layer, in words.
fn render_text(project: &ProjectV2, layer: &ParticlesLayerDef) -> String {
    let effect = &layer.effect;
    let blend = match effect.alpha_mode {
        AlphaModeDef::Blend => "Blend".to_string(),
        AlphaModeDef::Premultiply => "Premultiply".to_string(),
        AlphaModeDef::Add => "Add (additive glow)".to_string(),
        AlphaModeDef::Multiply => "Multiply".to_string(),
        AlphaModeDef::Mask(c) => format!("Mask (cutoff {})", fmt_num(c)),
        AlphaModeDef::Opaque => "Opaque".to_string(),
    };
    let sampling = effect.render_modifiers.iter().find_map(|m| match m {
        RenderModifierEditor::ParticleTexture(t) => Some(format!("{:?}", t.sample_mapping)),
        _ => None,
    });
    let orient = effect.render_modifiers.iter().find_map(|m| match m {
        RenderModifierEditor::Orient(o) => Some(match &o.rotation_expr {
            Some(_) => format!("{:?} with a rotation expression (spinning sprites)", o.mode),
            None => format!("{:?}", o.mode),
        }),
        _ => None,
    });
    let mut text = format!(
        "render: blend {}; texture sampling {}; orient {}",
        blend,
        sampling.unwrap_or_else(|| "none (untextured quads)".into()),
        orient.unwrap_or_else(|| "default (no Orient modifier)".into())
    );
    for m in effect.render_modifiers.iter() {
        match m {
            RenderModifierEditor::Flipbook(f) => match f.grid_for(project.texture_grid(layer)) {
                Some((rows, cols)) => {
                    let _ = write!(text, "; flipbook {} × {} = {} frames, frame = sprite_index", rows, cols, rows * cols);
                }
                None => text.push_str("; flipbook without a grid (one frame)"),
            },
            RenderModifierEditor::Round(r) => match r.roundness_expr.scalar_range() {
                Some((lo, hi)) if lo == hi => {
                    let _ = write!(text, "; rounded corners {} (1 = circles)", fmt_num(lo));
                }
                _ => text.push_str("; rounded corners from an expression"),
            },
            RenderModifierEditor::ScreenSpaceSize(_) => text.push_str("; size in screen pixels"),
            _ => {}
        }
    }
    match effect.motion_integration {
        MotionIntegrationDef::PostUpdate => {}
        MotionIntegrationDef::PreUpdate => text.push_str("; motion integrated before the update modifiers"),
        MotionIntegrationDef::None => text.push_str("; no motion integration (velocity does not move particles)"),
    }
    if effect.simulation_condition == SimulationConditionDef::Always {
        text.push_str("; simulated even when not visible");
    }
    if let Some(seed) = effect.prng_seed {
        let _ = write!(text, "; random seed {}", seed);
    }
    text
}

/// Simulation space, with the trail a moving target leaves in `Global`.
fn space_text(project: &ProjectV2, layer: &ParticlesLayerDef, layers: &[&ParticlesLayerDef], index: usize) -> String {
    match layer.simulation_space {
        SimulationSpaceDef::Local => "Local — particles move with the target".into(),
        SimulationSpaceDef::Global => {
            let speed = target_speed(project);
            let looping = layers[root_index(layers, index)].effect.spawner.cycle_count == 0;
            let trail = match layer.effect.max_lifetime() {
                Some(l) => format!(
                    "on a target moving at {} m/s particles trail ~{} m behind (lifetime up to {}s)",
                    fmt_num(speed),
                    fmt_num(speed * l),
                    fmt_num(l)
                ),
                None => "on a moving target particles trail behind".into(),
            };
            format!(
                "Global — particles stay where they were emitted; {}{}",
                trail,
                if looping { ", for as long as it loops" } else { "" }
            )
        }
    }
}

/// Simulates a project and describes every layer in a compact text form.
pub fn analyze_project(
    project: &ProjectV2,
    assets_root: &Path,
    n_samples: usize,
    only_effect: Option<&str>,
) -> String {
    analyze_project_with_sets(project, assets_root, n_samples, only_effect, None)
}

/// Parameter values for one simulation: (name, value); missing parameters
/// keep their default.
pub type ParamSet = Vec<(String, ParamValue)>;

/// The parameter sets analysed when none are given: every Float parameter
/// at its min, middle and max together (others at their default), with a
/// label each. Empty without Float parameters.
pub fn default_param_sets(project: &ProjectV2) -> Vec<(String, ParamSet)> {
    let floats: Vec<&ParamDef> = project.parameters.iter().filter(|p| p.kind() == ParamKind::Float).collect();
    if floats.is_empty() {
        return Vec::new();
    }
    [("min", 0.0f32), ("middle", 0.5), ("max", 1.0)]
        .iter()
        .map(|(label, f)| {
            let set: ParamSet = floats
                .iter()
                .map(|p| (p.name.clone(), ParamValue::Float(p.min + (p.max - p.min) * f)))
                .collect();
            (format!("number parameters at {}", label), set)
        })
        .collect()
}

/// A parameter value in words: `0.5`, `(1, 0.2, 0, 1)`.
fn param_value_text(value: &ParamValue) -> String {
    match value {
        ParamValue::Float(v) => fmt_num(*v),
        ParamValue::Color(c) => format!("({}, {}, {}, {}) {}", fmt_num(c.x), fmt_num(c.y), fmt_num(c.z), fmt_num(c.w), color_name(c.truncate())),
    }
}

/// A set as `a=0.5, tint=(...)`.
fn param_set_text(set: &[(String, ParamValue)]) -> String {
    set.iter()
        .map(|(n, v)| format!("{}={}", n, param_value_text(v)))
        .collect::<Vec<_>>()
        .join(", ")
}

/// How a layer depends on parameters and triggers, in words (empty when it
/// does not).
fn bindings_text(project: &ProjectV2, layer: &ParticlesLayerDef) -> Vec<String> {
    let mut lines = Vec::new();
    match &layer.play {
        PlayDef::OnTrigger(t) => lines.push(format!(
            "play: OnTrigger(\"{}\") — bursts each time the game fires \"{}\" (simulated as fired once at t = 0), not at the start; a restart does not replay it",
            t, t
        )),
        PlayDef::While(p, threshold) => lines.push(format!(
            "play: While(\"{}\", {}) — emits while {} ≥ {}",
            p,
            fmt_num(*threshold),
            p,
            fmt_num(*threshold)
        )),
        _ => {}
    }
    match &layer.rate {
        Some(Binding::Const(rate)) => lines.push(format!("rate: steady {}/s (replaces the spawner's count)", fmt_num(*rate))),
        Some(Binding::Param(p, keys)) => {
            let keys: Vec<String> = keys.iter().map(|(x, v)| format!("{}→{}/s", fmt_num(*x), fmt_num(*v))).collect();
            lines.push(format!("rate: curve over \"{}\": {} (steady stream, replaces the spawner's count)", p, keys.join(", ")));
        }
        Some(Binding::Age(keys)) => {
            let keys: Vec<String> = keys.iter().map(|(x, v)| format!("{} s→{}/s", fmt_num(*x), fmt_num(*v))).collect();
            lines.push(format!(
                "rate: curve over the layer's age (seconds since it started, or since its trigger / While switched it on): {} (steady stream; the simulation uses its value at age 0)",
                keys.join(", ")
            ));
        }
        None => {}
    }
    if let Some(v) = &layer.visible {
        lines.push(format!(
            "visible: only while {} is in [{}, {}], fading out over {} outside; multiplies the spawn rate{}",
            v.param,
            fmt_num(v.from),
            fmt_num(v.to),
            fmt_num(v.fade),
            if layer.effect.properties_read().iter().any(|p| p == "visibility") {
                " and alpha (reads Property(\"visibility\"))"
            } else {
                "; live particles do not fade (read Property(\"visibility\") for that)"
            }
        ));
    }
    let read = layer.effect.params_read();
    if !read.is_empty() {
        let read: Vec<String> = read
            .iter()
            .map(|n| match project.param(n) {
                Some(p) => format!("{} ({})", n, p.kind().name()),
                None => format!("{} (unknown)", n),
            })
            .collect();
        lines.push(format!("expressions read parameters: {}", read.join(", ")));
    }
    let payload: Vec<String> = layer.effect.properties_read().into_iter().filter(|p| p.starts_with("trigger_")).collect();
    if !payload.is_empty() {
        lines.push(format!(
            "reads the trigger payload ({}): trigger_at puts the burst at the contact point / along its direction",
            payload.join(", ")
        ));
    }
    lines
}

/// Colour of a layer's particles at half their life: the mean birth colour
/// through ColorOverLifetime.
fn mid_life_color(effect: &ParticleEffectDef, samples: &[Sample]) -> Vec4 {
    let base = if samples.is_empty() {
        Vec4::ONE
    } else {
        samples.iter().map(|s| base_color(&s.init)).sum::<Vec4>() / samples.len() as f32
    };
    render_color(effect, base, 0.5)
}

/// Channels of a colour mask in words: `RGBA`, `RGB`, `A`, ...
fn mask_text(mask: ColorBlendMask) -> String {
    let names = ["R", "G", "B", "A"];
    let bits = [ColorBlendMask::R, ColorBlendMask::G, ColorBlendMask::B, ColorBlendMask::A];
    bits.iter().zip(names).filter(|(b, _)| mask.contains(**b)).map(|(_, n)| n).collect()
}

/// One line about a simulated layer: count, size, colour, reach.
fn layer_summary(effect: &ParticleEffectDef, sim: &EffectSim) -> String {
    if let Some(e) = &sim.error {
        return format!("simulation failed: {}", e);
    }
    if sim.samples.is_empty() {
        return "nothing spawns".into();
    }
    let count = if sim.forever {
        let tail = &sim.alive[sim.alive.len() * 3 / 4..];
        format!("~{} alive", (tail.iter().sum::<f32>() / tail.len().max(1) as f32).round())
    } else {
        format!("~{} in total", sim.births.iter().sum::<f32>().round())
    };
    let sizes: Vec<f32> = sim
        .samples
        .iter()
        .filter(|s| s.init.0.iter().any(|(a, _)| *a == Attribute::SIZE))
        .map(|s| s.init.f32(Attribute::SIZE))
        .collect();
    let size = if !sizes.is_empty() {
        format!("size {}", range_text(&sizes, ""))
    } else if effect.render_modifiers.iter().any(|m| matches!(m, RenderModifierEditor::SizeOverLifetime(_))) {
        "size from SizeOverLifetime".into()
    } else if effect.render_modifiers.iter().any(|m| matches!(m, RenderModifierEditor::SetSize(_))) {
        "size from SetSize".into()
    } else {
        "size 1 (default)".into()
    };
    let c = mid_life_color(effect, &sim.samples);
    let reach = sim
        .samples
        .iter()
        .flat_map(|s| s.path.iter().chain(std::iter::once(&s.end_pos)))
        .filter(|p| p.is_finite())
        .map(|p| p.length())
        .fold(0.0f32, f32::max);
    format!(
        "{}; {}; colour at mid-life {} a{}; reaches {} m",
        count,
        size,
        color_name(c.truncate()),
        fmt_num(c.w),
        fmt_num(reach)
    )
}

/// The project's parameters and triggers, and the layers simulated at each
/// parameter set side by side.
fn parameters_report(project: &ProjectV2, n_samples: usize, sets: &[(String, ParamSet)], only_effect: Option<&str>, out: &mut String) {
    if project.parameters.is_empty() && project.triggers.is_empty() && sets.is_empty() {
        return;
    }
    let _ = writeln!(out, "\n## Parameters and triggers");
    for p in project.parameters.iter() {
        let range = match p.kind() {
            ParamKind::Float => format!(", range [{}, {}]", fmt_num(p.min), fmt_num(p.max)),
            ParamKind::Color => String::new(),
        };
        let users = project.param_users(&p.name);
        let _ = writeln!(
            out,
            "  parameter \"{}\": {}, default {}{}, smoothing {}s; used by {}",
            p.name,
            p.kind().name(),
            param_value_text(&p.default),
            range,
            fmt_num(p.smoothing),
            if users.is_empty() { "nothing".to_string() } else { users.join(", ") }
        );
    }
    for t in project.triggers.iter() {
        let pulses: Vec<String> = t
            .pulses
            .iter()
            .map(|p| format!("{} jumps to {} and returns over {}s", p.param, fmt_num(p.to), fmt_num(p.decay)))
            .collect();
        let layers = project.trigger_users(&t.name);
        let _ = writeln!(
            out,
            "  trigger \"{}\": {}{}",
            t.name,
            if layers.is_empty() { "no layer bursts".to_string() } else { format!("bursts {}", layers.join(", ")) },
            if pulses.is_empty() { String::new() } else { format!("; pulses: {}", pulses.join("; ")) }
        );
    }
    if sets.is_empty() {
        return;
    }

    let _ = writeln!(out, "\n## Parameter sets");
    let effects = particle_layers(project);
    let mut rows: Vec<Vec<String>> = vec![Vec::new(); effects.len()];
    for (label, set) in sets.iter() {
        let values = param_values(project, set);
        let _ = writeln!(out, "  set {}: {}", label, param_set_text(&values));
        let resolved = resolve_for_analysis(project, set);
        let (sims, _) = simulate_project(&resolved, n_samples.clamp(20, 2000), 42);
        for (i, (effect, sim)) in particle_layers(&resolved).iter().zip(sims.iter()).enumerate() {
            rows[i].push(layer_summary(&effect.effect, sim));
        }
    }
    super::surface_check::surface_sets_report(project, sets, only_effect, out);
    super::shape_check::shape_sets_report(project, sets, only_effect, out);
    for (effect, rows) in effects.iter().zip(rows.iter()) {
        if only_effect.is_some_and(|o| o != effect.name) {
            continue;
        }
        if rows.windows(2).all(|w| w[0] == w[1]) {
            let _ = writeln!(out, "  [{}] same in every set: {}", effect.name, rows.first().cloned().unwrap_or_default());
        } else {
            let _ = writeln!(out, "  [{}] changes with the parameters:", effect.name);
            for ((label, _), row) in sets.iter().zip(rows.iter()) {
                let _ = writeln!(out, "    {}: {}", label, row);
            }
        }
    }
}

/// [`analyze_project`] at chosen parameter sets: the main report uses the
/// defaults, then every layer is simulated at each set and compared. `None`
/// = [`default_param_sets`].
pub fn analyze_project_with_sets(
    project: &ProjectV2,
    assets_root: &Path,
    n_samples: usize,
    only_effect: Option<&str>,
    sets: Option<&[(String, ParamSet)]>,
) -> String {
    let diagnostics = check_project(project, assets_root);
    let mut out = diagnostics.render();
    if !diagnostics.errors.is_empty() {
        out.push_str("\n(simulation still runs, but results for broken effects are unreliable)\n");
    }

    let _ = writeln!(out, "fit: {}", fit_text(project.fit));
    let (sims, horizon) = simulate_project(&resolve_properties(project), n_samples.clamp(20, 2000), 42);
    let truncated = horizon >= MAX_TIMELINE - DT;
    let effects = particle_layers(project);
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

        let role = match &effect.parent {
            Some(p) => format!("child of \"{}\"", p),
            None => "root".into(),
        };
        let _ = writeln!(
            out,
            "\n## [{}] \"{}\" — {}, texture {}, capacity {}",
            i,
            effect.name,
            role,
            texture_label(project, effect),
            effect.effect.capacity
        );
        let _ = writeln!(out, "  spawn: {}", sim.spawn_text);
        let _ = writeln!(out, "  {}", render_text(project, effect));
        let _ = writeln!(out, "  space: {}", space_text(project, effect, &effects, i));
        let _ = writeln!(out, "  attach: {}", attach_text(project, &effect.attach));
        for line in bindings_text(project, effect) {
            let _ = writeln!(out, "  {}", line);
        }
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
        let capacity_note = if peak > effect.effect.capacity as f32 {
            format!(" — EXCEEDS capacity {}, spawning will be capped", effect.effect.capacity)
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
        // How much each editor stage hides: the FLOOR stage's floor is at y = 0, and
        // the OBJECT stage's rock is roughly an ellipsoid 2 units wide, 1.6 tall.
        let path_len = samples.iter().map(|s| s.path.len()).sum::<usize>().max(1);
        let below_floor = ys.iter().filter(|y| **y < -0.01).count() * 100 / ys.len().max(1);
        let inside_rock = samples
            .iter()
            .flat_map(|s| s.path.iter())
            .filter(|p| Vec3::new(p.x, p.y / 0.8, p.z).length() < 0.85)
            .count()
            * 100
            / path_len;
        let mut hidden = Vec::new();
        if below_floor >= 10 {
            hidden.push(format!("FLOOR hides {}% (below y = 0)", below_floor));
        }
        if inside_rock >= 10 {
            hidden.push(format!("OBJECT hides {}% (inside the rock)", inside_rock));
        }
        if !hidden.is_empty() {
            let _ = writeln!(out, "  editor stages: {}; AIR shows all", hidden.join(", "));
        }

        describe_size(&effect.effect, samples, &mut out);
        describe_color(&effect.effect, samples, &mut out);

        if samples.iter().any(|s| s.nan) {
            let _ = writeln!(out, "  WARNING: some positions become NaN (e.g. Normalized of a zero vector)");
        }
        let zero_v = samples.iter().filter(|s| s.zero_velocity).count();
        if zero_v > 0 && orients_along_velocity(&effect.effect) {
            let _ = writeln!(
                out,
                "  WARNING: {}% of particles have zero velocity at some point; Orient(AlongVelocity) orients particles along it, so they may render invisible then",
                (zero_v * 100 / samples.len())
            );
        }
        let _ = writeln!(out, "  alive over time: {}", timeline_text(&sim.alive, horizon));
    }

    for (index, layer) in project.surface_layers() {
        if only_effect.is_none_or(|o| o == layer.name) {
            super::surface_check::describe_surface_layer(project, index, layer, &mut out);
        }
    }
    for (index, layer) in project.shape_layers() {
        if only_effect.is_none_or(|o| o == layer.name) {
            super::shape_check::describe_shape_layer(project, index, layer, &mut out);
        }
    }

    if only_effect.is_none() && effects.is_empty() {
        let _ = writeln!(out, "\n## Whole project\n  no particle layers");
        if let Some(text) = super::shape_check::shapes_lifecycle_text(project) {
            let _ = writeln!(out, "  {}", text);
        }
    } else if only_effect.is_none() {
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
    let default_sets;
    let sets = match sets {
        Some(sets) => sets,
        None => {
            default_sets = default_param_sets(project);
            &default_sets
        }
    };
    parameters_report(project, n_samples, sets, only_effect, &mut out);
    let _ = writeln!(
        out,
        "\n(CPU approximation, {} samples per layer, 60 updates/s. Spawned at the origin of a still target, Y is up.)",
        n_samples.clamp(20, 2000)
    );
    out
}


#[cfg(test)]
mod tests {
    use super::*;

    fn assets_root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("assets")
    }

    fn parse(text: &str) -> ProjectV2 {
        ProjectV2::from_ron_str(text).unwrap_or_else(|e| panic!("{}\n{}", e, text))
    }

    /// A one-layer v2 project around the given init/update/render modifiers.
    fn layer_project(extra_layer_fields: &str, effect_fields: &str) -> ProjectV2 {
        parse(&format!(
            r#"(
                version: 2,
                assets: [(id: "spark1", kind: Texture((path: "omagari://textures/spark1.png")))],
                layers: [Particles((
                    name: "a",
                    texture: Some("spark1"),
                    {}
                    effect: (
                        capacity: 10,
                        spawner: (count: Single(1.0), spawn_duration: Single(0.0), period: Single(1.0), cycle_count: 1, starts_active: true, emit_on_start: true),
                        {}
                    ),
                ))],
            )"#,
            extra_layer_fields, effect_fields
        ))
    }

    /// A game assets folder for the recipes: the flipbook recipe's sprite
    /// sheet (a generated 4 × 4 grid of growing dots) as a test fixture;
    /// built-in textures need no files.
    fn recipe_assets_root() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("omagari_recipe_assets_{}", std::process::id()));
        let sheet = dir.join("vfx/textures/flipbook_4x4.png");
        if !sheet.is_file() {
            std::fs::create_dir_all(sheet.parent().unwrap()).unwrap();
            let image = image::RgbaImage::from_fn(256, 256, |x, y| {
                let frame = (y / 64) * 4 + x / 64;
                let (u, v) = ((x % 64) as f32 - 32.0, (y % 64) as f32 - 32.0);
                let inside = (u * u + v * v).sqrt() < 4.0 + frame as f32 * 1.6;
                image::Rgba([255, (frame * 16) as u8, 40, if inside { 255 } else { 0 }])
            });
            image.save(&sheet).unwrap();
        }
        dir
    }

    /// Every recipe in the agent guide is a v2 project and passes the checks
    /// it tells agents to run, with no warning at all.
    #[test]
    fn guide_recipes_pass_checks() {
        let mut checked = 0;
        let assets_root = recipe_assets_root();
        for recipe in crate::mcp::guide_recipes() {
            let text = recipe.ron.as_str();
            let (project, migration) = ProjectV2::from_ron_str_with_warnings(text)
                .unwrap_or_else(|e| panic!("{}\n{}", e, text));
            assert!(text.contains("version: 2"), "recipes must be v2:\n{}", text);
            assert!(migration.is_empty(), "{:?}\n{}", migration, text);
            let d = check_project(&project, &assets_root);
            assert!(
                d.errors.is_empty() && d.warnings.is_empty() && d.notes.is_empty(),
                "{}\n{}",
                d.render(),
                text
            );
            let mut produce_warnings = Vec::new();
            for (_, layer) in project.particle_layers() {
                layer.produce_in(&project, &mut produce_warnings);
            }
            assert!(produce_warnings.is_empty(), "{:?}", produce_warnings);
            let report = analyze_project(&project, &assets_root, 50, None);
            assert!(report.contains("## Whole project"), "{}", report);
            checked += 1;
        }
        assert!(checked >= 22, "only {} recipes found", checked);
    }

    fn has(list: &[String], needle: &str) -> bool {
        list.iter().any(|m| m.contains(needle))
    }

    /// With a preview model, socket nodes, attach names, `attach_to`, the
    /// weapon node and the clip are checked against the model's files.
    #[test]
    fn sockets_and_preview_are_checked_against_the_model() {
        let dir = crate::asset_library::tests::temp_dir("check_model");
        crate::model_info::tests::write_fixture(&dir.join("hero.gltf"), Some(("Wave", 1.5)));
        crate::model_info::tests::write_fixture(&dir.join("swing.gltf"), Some(("Swing", 0.5)));
        let effect = |init: &str| format!("init_modifiers: [SetAttribute((attr: \"age\", attr_expr: Float(0.0))), {}],", init);
        let mut p = layer_project(r#"play: OnStart, attach: Segment(from: "hand", to: "Spine"),"#, &effect(SEGMENT_POSITION));
        p.sockets = vec![SocketDef { name: "hand".into(), node: Some("RightHand".into()), offset: Vec3::Y }];
        // Without a model: node names are notes.
        let d = check_project(&p, &dir);
        assert!(d.errors.is_empty() && has(&d.notes, "\"Spine\" is not a package socket"), "{}", d.render());

        p.preview = Some(PreviewDef { stage: PreviewStage::Model, model: Some("hero.gltf".into()), ..Default::default() });
        let d = check_project(&p, &dir);
        assert!(d.errors.is_empty() && d.notes.is_empty() && !has(&d.warnings, "preview"), "{}", d.render());

        // Nodes the model does not have are errors.
        p.sockets[0].node = Some("LeftHand".into());
        p.layers[0].as_particles_mut().unwrap().attach = AttachDef::Segment { from: "hand".into(), to: "Tail".into() };
        let d = check_project(&p, &dir);
        assert!(has(&d.errors, "[socket \"hand\"] node \"LeftHand\" is not a node of the preview model hero.gltf"), "{}", d.render());
        assert!(has(&d.errors, "\"Tail\" is neither a package socket nor a node of the preview model hero.gltf"), "{}", d.render());
        assert!(!has(&d.notes, "Tail"), "{}", d.render());

        // attach_to: sockets must be below it.
        p.sockets[0].node = Some("RightHand".into());
        p.layers[0].as_particles_mut().unwrap().attach = AttachDef::Segment { from: "hand".into(), to: "Hips".into() };
        p.preview.as_mut().unwrap().attach_to = Some("Spine".into());
        let d = check_project(&p, &dir);
        assert!(has(&d.errors, "\"Hips\" is neither a package socket nor a node below \"Spine\""), "{}", d.render());
        assert!(!has(&d.errors, "socket \"hand\""), "RightHand is below Spine: {}", d.render());
        p.preview.as_mut().unwrap().attach_to = Some("Neck".into());
        assert!(has(&check_project(&p, &dir).errors, "attach_to \"Neck\" is not a node of the preview model"));
        p.preview.as_mut().unwrap().attach_to = None;

        // Clips: by name in the animation file, triggers within the clip.
        let preview = p.preview.as_mut().unwrap();
        preview.animation = Some("Swing".into());
        preview.animation_file = Some("swing.gltf".into());
        preview.animation_triggers = vec![("hit".into(), 0.8)];
        let d = check_project(&p, &dir);
        assert!(!has(&d.errors, "animation"), "{}", d.render());
        assert!(has(&d.warnings, "\"hit\" fires at clip time 0.8 s, after the end of the 0.50 s clip"), "{}", d.render());
        assert!(has(&d.warnings, "preview fires \"hit\" at clip time 0.8 s, which is not a trigger"), "{}", d.render());
        p.preview.as_mut().unwrap().animation = Some("Run".into());
        assert!(has(&check_project(&p, &dir).errors, "swing.gltf has no animation \"Run\" (it has \"Swing\")"));

        // A weapon held by a node the body lacks; a missing model file.
        let preview = p.preview.as_mut().unwrap();
        preview.animation = None;
        preview.animation_triggers.clear();
        preview.weapon = Some(PreviewWeaponDef { model: "swing.gltf".into(), node: Some("Claw".into()), ..Default::default() });
        assert!(has(&check_project(&p, &dir).errors, "weapon node \"Claw\" is not a node of the model hero.gltf"));
        let preview = p.preview.as_mut().unwrap();
        preview.weapon = None;
        preview.model = Some("villain.glb".into());
        let d = check_project(&p, &dir);
        assert!(has(&d.warnings, "model villain.glb is not in the game assets folder"), "{}", d.render());
        assert!(d.errors.is_empty() && has(&d.notes, "\"Hips\" is not a package socket"), "no model, notes again: {}", d.render());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The editor shows each layer's messages next to the layer.
    #[test]
    fn diagnostics_group_by_subject() {
        assert_eq!(split_subject("[a] capacity is 0"), (Some("a"), "capacity is 0"));
        assert_eq!(split_subject("[socket \"s\"] not used"), (Some("socket \"s\""), "not used"));
        assert_eq!(split_subject("project has no layers"), (None, "project has no layers"));

        let mut project = layer_project("", "init_modifiers: [], update_modifiers: [], render_modifiers: []");
        project.layers[0].as_particles_mut().unwrap().effect.capacity = 0;
        let d = check_project(&project, &assets_root());
        let about_a: Vec<_> = d.about("a").collect();
        assert!(about_a.contains(&(Severity::Error, "capacity is 0, nothing can spawn")), "{:?}", about_a);
        let (errors, warnings) = d.count_about("a");
        assert!(errors >= 1 && warnings >= 1, "{}", d.render());
        assert_eq!(d.count_about("b"), (0, 0));
        assert_eq!(d.all().count(), d.errors.len() + d.warnings.len() + d.notes.len());
    }

    const SEGMENT_POSITION: &str = r#"SetAttribute((attr: "position", attr_expr: Operator(Add(Property("segment_a"), Operator(Multiply(Operator(Subtract(Property("segment_b"), Property("segment_a"))), RandFloat))))))"#;

    #[test]
    fn attach_sockets_and_fit_are_checked() {
        let effect = |init: &str| format!("init_modifiers: [SetAttribute((attr: \"age\", attr_expr: Float(0.0))), {}],", init);
        let mut p = layer_project(
            r#"play: OnStart, attach: Segment(from: "base", to: "tip"),"#,
            &effect(SEGMENT_POSITION),
        );
        p.sockets = vec![
            SocketDef { name: "base".into(), node: None, offset: Vec3::ZERO },
            SocketDef { name: "tip".into(), node: None, offset: Vec3::new(0.0, 1.0, 0.0) },
        ];
        let d = check_project(&p, &assets_root());
        assert!(d.errors.is_empty() && d.notes.is_empty(), "{}", d.render());
        assert!(!has(&d.warnings, "segment"), "{}", d.render());

        // The simulation puts particles on the segment, 0 to 1 m up.
        let report = analyze_project(&p, &assets_root(), 100, None);
        assert!(report.contains("attach: segment from socket \"base\""), "{}", report);
        assert!(report.contains("fit: none"), "{}", report);
        let (sims, _) = simulate_project(&resolve_properties(&p), 100, 1);
        for sample in sims[0].samples.iter() {
            let y = sample.birth_pos.y;
            assert!((0.0..=1.0).contains(&y), "spawned at y {}", y);
        }

        // Unknown and missing sockets, an unused one, a duplicate.
        p.sockets.push(SocketDef { name: "tip".into(), node: Some(String::new()), offset: Vec3::ZERO });
        p.sockets.push(SocketDef { name: "spare".into(), node: None, offset: Vec3::ZERO });
        p.layers[0].as_particles_mut().unwrap().attach = AttachDef::Segment { from: "base".into(), to: "Hand".into() };
        let d = check_project(&p, &assets_root());
        assert!(has(&d.errors, "duplicate socket name"), "{}", d.render());
        assert!(has(&d.errors, "node is an empty name"), "{}", d.render());
        assert!(has(&d.warnings, "socket \"spare\"] not used"), "{}", d.render());
        assert!(has(&d.notes, "\"Hand\" is not a package socket"), "{}", d.render());
        assert!(!has(&d.warnings, "Hand"), "node names are only a note: {}", d.render());

        // Segment ends without a Segment, a Segment without them.
        let p = layer_project(r#"play: OnStart, attach: Socket("Head"),"#, &effect(SEGMENT_POSITION));
        let d = check_project(&p, &assets_root());
        assert!(has(&d.warnings, "only Segment-attached layers get them"), "{}", d.render());
        let p = layer_project(r#"play: OnStart, attach: Segment(from: "a", to: "a"),"#, &effect(""));
        let d = check_project(&p, &assets_root());
        assert!(has(&d.warnings, "no expression reads Property"), "{}", d.render());
        assert!(has(&d.warnings, "has no length"), "{}", d.render());

        // Unknown properties are errors.
        let p = layer_project(
            "play: OnStart,",
            &effect(r#"SetAttribute((attr: "position", attr_expr: Property("contact_point")))"#),
        );
        let d = check_project(&p, &assets_root());
        assert!(has(&d.errors, "Property(\"contact_point\") is not a runtime property"), "{}", d.render());

        // Fit: a reference height is needed; Global layers are not scaled.
        let mut p = layer_project("play: OnStart, simulation_space: Global,", &effect(""));
        p.fit = Some(FitDef::TargetHeight(0.0));
        assert!(has(&check_project(&p, &assets_root()).errors, "reference height above 0"));
        p.fit = Some(FitDef::TargetHeight(1.8));
        let d = check_project(&p, &assets_root());
        assert!(has(&d.warnings, "fit does not scale Global layers"), "{}", d.render());
        assert!(analyze_project(&p, &assets_root(), 20, None).contains("fit: TargetHeight(1.8)"));

        // A socket layer's particles are reported where the socket is.
        let mut p = layer_project(
            r#"play: OnStart, attach: Socket("chest"),"#,
            &effect(r#"SetPositionSphere((center_expr: Vec3((0.0, 0.0, 0.0)), radius_expr: Float(0.1), dimension: Volume))"#),
        );
        p.sockets = vec![SocketDef { name: "chest".into(), node: None, offset: Vec3::new(0.0, 1.0, 0.0) }];
        let (sims, _) = simulate_project(&resolve_properties(&p), 50, 1);
        assert!(sims[0].samples.iter().all(|s| (s.birth_pos.y - 1.0).abs() <= 0.1 + 1e-4));
        p.layers[0].as_particles_mut().unwrap().simulation_space = SimulationSpaceDef::Local;
        assert!(!has(&check_project(&p, &assets_root()).warnings, "fit"));
    }

    /// The format example in the guide parses as v2.
    #[test]
    fn guide_format_example_parses() {
        let format = crate::mcp::GUIDE.split("## Format").nth(1).unwrap();
        let text = format.split("```ron\n").nth(1).unwrap().split("```").next().unwrap();
        let (project, warnings) = ProjectV2::from_ron_str_with_warnings(text).unwrap_or_else(|e| panic!("{}\n{}", e, text));
        assert!(warnings.is_empty(), "{:?}", warnings);
        assert_eq!(project.layers.len(), 1);
    }

    #[test]
    fn examples_analyze() {
        let examples = concat!(env!("CARGO_MANIFEST_DIR"), "/examples");
        for entry in std::fs::read_dir(examples).unwrap() {
            let path = entry.unwrap().path();
            if path.to_string_lossy().ends_with(".omagari.ron") {
                let project = parse(&std::fs::read_to_string(&path).unwrap());
                let d = check_project(&project, &assets_root());
                assert!(d.errors.is_empty(), "{}: {}", path.display(), d.render());
                let report = analyze_project(&project, &assets_root(), 50, None);
                assert!(report.contains("## Whole project"), "{}", report);
            }
        }
    }

    /// v1 text is migrated and checked like v2.
    #[test]
    fn v1_projects_are_checked_after_migration() {
        let project = parse(
            r#"(effects: [
                (name: "a", parent: None, capacity: 10,
                 spawner_settings: (count: Single(1.0), spawn_duration: Single(0.0), period: Single(1.0), cycle_count: 1, starts_active: true, emit_on_start: true),
                 texture_index: Some(2),
                 init_modifiers: [SetAttribute((attr: "lifetime", attr_expr: Float(1.0))), SetAttribute((attr: "velocity", attr_expr: Vec3((0.0, 1.0, 0.0))))],
                 update_modifiers: [], render_modifiers: []),
            ])"#,
        );
        assert_eq!(project.version, 2);
        let d = check_project(&project, &assets_root());
        assert!(d.errors.is_empty(), "{}", d.render());
        let report = analyze_project(&project, &assets_root(), 20, None);
        assert!(report.contains("texture \"spark1\" (omagari://textures/spark1.png)"), "{}", report);
        assert!(report.contains("space: Global"), "{}", report);
        assert!(report.contains("orient AlongVelocity"), "{}", report);
    }

    #[test]
    fn reports_type_and_structure_errors() {
        let project = parse(
            r#"(version: 2, layers: [
                Particles((name: "a", effect: (capacity: 10,
                    spawner: (count: Single(1.0), spawn_duration: Single(0.0), period: Single(1.0), cycle_count: 1),
                    init_modifiers: [SetAttribute((attr: "size", attr_expr: Vec3((1.0, 1.0, 1.0))))],
                    update_modifiers: [InheritAttribute((attr: "age"))],
                    render_modifiers: []))),
                Particles((name: "b", parent: Some("a"), effect: (capacity: 10,
                    init_modifiers: [SetAttribute((attr: "position", attr_expr: Float(1.0)))],
                    render_modifiers: []))),
                Particles((name: "c", parent: Some("nope"), effect: (render_modifiers: []))),
            ])"#,
        );
        let errors = check_project(&project, &assets_root()).errors.join("\n");
        assert!(errors.contains("[a] init SetAttribute: SetAttribute(\"size\") needs f32, got vec3"), "{}", errors);
        assert!(errors.contains("[a] InheritAttribute is an init modifier but is listed in update_modifiers"), "{}", errors);
        assert!(!errors.contains("[a] bevy_hanabi"), "{}", errors);
        assert!(errors.contains("[b] parent \"a\" has no EmitSpawnEventModifier with child_index 0"), "{}", errors);
        assert!(errors.contains("[b] init SetAttribute: SetAttribute(\"position\") needs vec3, got f32"), "{}", errors);
        assert!(errors.contains("[c] parent \"nope\" is not a particle layer"), "{}", errors);
    }

    #[test]
    fn textures_are_checked_by_asset_id() {
        let project = parse(
            r#"(version: 2,
                assets: [
                    (id: "bad", kind: Texture((path: "omagari://textures/nope.png"))),
                    (id: "file", kind: Texture((path: "vfx/textures/missing.png"))),
                    (id: "spare", kind: Texture((path: "omagari://textures/glow1.png"))),
                ],
                layers: [
                    Particles((name: "a", texture: Some("ghost"), effect: (render_modifiers: [ParticleTexture(())]))),
                    Particles((name: "b", texture: Some("bad"), effect: (render_modifiers: [ParticleTexture(())]))),
                    Particles((name: "c", texture: Some("file"), effect: (render_modifiers: []))),
                    Particles((name: "d", effect: (render_modifiers: [ParticleTexture(())]))),
                ])"#,
        );
        let d = check_project(&project, &assets_root());
        let all = d.render();
        assert!(all.contains("[a] texture \"ghost\" is not in the assets list"), "{}", all);
        assert!(all.contains("[asset \"bad\"] omagari://textures/nope.png is not a built-in texture"), "{}", all);
        assert!(all.contains("[asset \"file\"] vfx/textures/missing.png is not in the game assets folder"), "{}", all);
        assert!(all.contains("[asset \"spare\"] not used by any layer"), "{}", all);
        assert!(all.contains("[c] texture \"file\" is set, but no ParticleTexture render modifier samples it"), "{}", all);
        assert!(all.contains("[d] ParticleTexture samples the layer's texture, but the layer has none"), "{}", all);
    }

    #[test]
    fn relative_textures_are_found_in_the_game_folder() {
        let project = parse(
            r#"(version: 2,
                assets: [(id: "glow", kind: Texture((path: "glow1.png")))],
                layers: [Particles((name: "a", texture: Some("glow"), effect: (render_modifiers: [ParticleTexture(())])))])"#,
        );
        let d = check_project(&project, &assets_root());
        assert!(!d.render().contains("game assets folder"), "{}", d.render());
    }

    /// Settings that fight the purpose, bad grids, paths outside the game
    /// folder, JPG, 16-bit grayscale PNGs.
    #[test]
    fn texture_settings_and_paths_are_checked() {
        let dir = std::env::temp_dir().join(format!("omagari_texture_checks_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        image::RgbaImage::new(10, 8).save(dir.join("sheet.png")).unwrap();
        image::ImageBuffer::<image::Luma<u16>, Vec<u16>>::new(4, 4).save(dir.join("height16.png")).unwrap();
        image::RgbImage::new(4, 4).save(dir.join("photo.jpg")).unwrap();
        let layer = |name: &str, id: &str| {
            format!(r#"Particles((name: "{}", texture: Some("{}"), effect: (render_modifiers: [ParticleTexture(())]))),"#, name, id)
        };
        let ids = ["noise", "ramp", "mask", "sheet", "zero", "height", "photo", "abs", "up", "odd"];
        let layers: String = ids.iter().map(|id| layer(&format!("l_{}", id), id)).collect();
        let project = parse(&format!(
            r#"(version: 2,
                assets: [
                    (id: "noise", kind: Texture((path: "sheet.png", purpose: Noise, wrap: Some(Clamp)))),
                    (id: "ramp", kind: Texture((path: "sheet.png", purpose: Gradient, wrap: Some(Repeat)))),
                    (id: "mask", kind: Texture((path: "sheet.png", purpose: Mask, color_space: Some(Srgb)))),
                    (id: "sheet", kind: Texture((path: "sheet.png", grid: Some((2, 3))))),
                    (id: "zero", kind: Texture((path: "sheet.png", grid: Some((0, 2))))),
                    (id: "height", kind: Texture((path: "height16.png", purpose: Noise))),
                    (id: "photo", kind: Texture((path: "photo.jpg"))),
                    (id: "abs", kind: Texture((path: "C:/textures/x.png"))),
                    (id: "up", kind: Texture((path: "../x.png"))),
                    (id: "odd", kind: Texture((path: "sheet.psd"))),
                ],
                layers: [{}])"#,
            layers
        ));
        let d = check_project(&project, &dir);
        let all = d.render();
        let about = |id: &str| -> String {
            d.about(&format!("asset \"{}\"", id)).map(|(_, m)| m.to_string()).collect::<Vec<_>>().join("\n")
        };
        assert!(about("noise").contains("Noise with wrap Clamp"), "{}", all);
        assert!(about("ramp").contains("Gradient with wrap Repeat"), "{}", all);
        assert!(about("mask").contains("Mask with colour space Srgb"), "{}", all);
        assert!(about("sheet").contains("does not divide the 10×8 image evenly"), "{}", all);
        assert!(d.errors.iter().any(|e| e.contains("[asset \"zero\"] grid (0, 2)")), "{}", all);
        assert!(about("height").contains("16-bit grayscale PNG"), "{}", all);
        assert!(d.notes.iter().any(|n| n.contains("[asset \"photo\"] JPG")), "{}", all);
        assert!(d.errors.iter().any(|e| e.contains("[asset \"abs\"] C:/textures/x.png is not relative")), "{}", all);
        assert!(about("up").contains("points outside the game assets folder"), "{}", all);
        assert!(about("odd").contains(".psd is not a texture type"), "{}", all);

        // Defaults from the purpose raise nothing.
        let fine = parse(
            r#"(version: 2,
                assets: [(id: "n", kind: Texture((path: "sheet.png", purpose: Noise))), (id: "s", kind: Texture((path: "sheet.png", grid: Some((2, 5)))))],
                layers: [
                    Particles((name: "a", texture: Some("n"), effect: (render_modifiers: [ParticleTexture(())]))),
                    Particles((name: "b", texture: Some("s"), effect: (render_modifiers: [ParticleTexture(())]))),
                ])"#,
        );
        let d = check_project(&fine, &dir);
        assert!(d.about("asset \"n\"").next().is_none() && d.about("asset \"s\"").next().is_none(), "{}", d.render());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn motionless_particles_are_reported_only_when_oriented_along_velocity() {
        let init = r#"init_modifiers: [SetAttribute((attr: "lifetime", attr_expr: Float(1.0)))],"#;
        let project = layer_project(
            "",
            &format!("{} render_modifiers: [ParticleTexture(()), Orient((mode: AlongVelocity))],", init),
        );
        let d = check_project(&project, &assets_root());
        assert!(d.errors.is_empty(), "{}", d.render());
        assert!(d.warnings.iter().any(|w| w.contains("nothing sets a velocity")), "{}", d.render());
        assert!(d.warnings.iter().any(|w| w.contains("never die")), "{}", d.render());
        let report = analyze_project(&project, &assets_root(), 20, None);
        assert!(report.contains("zero velocity"), "{}", report);

        let billboard = layer_project(
            "",
            &format!("{} render_modifiers: [ParticleTexture(()), Orient((mode: ParallelCameraDepthPlane))],", init),
        );
        let d = check_project(&billboard, &assets_root());
        assert!(!d.warnings.iter().any(|w| w.contains("nothing sets a velocity")), "{}", d.render());
        let report = analyze_project(&billboard, &assets_root(), 20, None);
        assert!(!report.contains("zero velocity"), "{}", report);
    }

    #[test]
    fn global_looping_layers_warn_about_trails() {
        let effect = r#"init_modifiers: [
                SetAttribute((attr: "position", attr_expr: Vec3((0.0, 0.0, 0.0)))),
                SetAttribute((attr: "age", attr_expr: Float(0.0))),
                SetAttribute((attr: "lifetime", attr_expr: Float(2.0))),
                SetAttribute((attr: "velocity", attr_expr: Vec3((0.0, 1.0, 0.0)))),
            ],
            render_modifiers: [ParticleTexture(())],"#;
        let looping = |space: &str| {
            let mut p = layer_project(&format!("simulation_space: {}, play: Loop,", space), effect);
            let l = p.layers[0].as_particles_mut().unwrap();
            l.effect.spawner.cycle_count = 0;
            p
        };

        let global = looping("Global");
        let d = check_project(&global, &assets_root());
        let trail = d.warnings.iter().find(|w| w.contains("Global simulation space")).cloned();
        assert!(trail.as_deref().is_some_and(|w| w.contains("3.60 m/s") && w.contains("~7.20 m")), "{}", d.render());
        let report = analyze_project(&global, &assets_root(), 20, None);
        assert!(report.contains("space: Global"), "{}", report);
        assert!(report.contains("trail ~7.20 m"), "{}", report);

        let local = looping("Local");
        let d = check_project(&local, &assets_root());
        assert!(!d.render().contains("Global simulation space"), "{}", d.render());
        assert!(analyze_project(&local, &assets_root(), 20, None).contains("space: Local"));

        // The preview motion's speed is used when set.
        let mut moving = looping("Global");
        moving.preview = Some(PreviewDef {
            motion: Some(PreviewMotion::Circle { radius: 3.0, speed: 7.0 }),
            ..default()
        });
        let d = check_project(&moving, &assets_root());
        assert!(d.render().contains("~14 m"), "{}", d.render());

        // One-shot layers leave no lasting trail.
        let once = layer_project("simulation_space: Global, play: OnStart,", effect);
        assert!(!check_project(&once, &assets_root()).render().contains("Global simulation space"));
    }

    #[test]
    fn play_mode_must_match_the_spawner() {
        let effect = r#"init_modifiers: [SetAttribute((attr: "position", attr_expr: Vec3((0.0, 0.0, 0.0)))), SetAttribute((attr: "age", attr_expr: Float(0.0))), SetAttribute((attr: "velocity", attr_expr: Vec3((0.0, 1.0, 0.0))))], render_modifiers: [ParticleTexture(())],"#;
        let project = layer_project("play: Loop,", effect);
        let d = check_project(&project, &assets_root());
        assert!(d.render().contains("play is Loop, but the spawner stops after 1 cycle(s)"), "{}", d.render());
    }

    #[test]
    fn package_round_trip_is_checked() {
        let project = layer_project(
            "play: OnStart,",
            r#"init_modifiers: [SetAttribute((attr: "position", attr_expr: Vec3((0.0, 0.0, 0.0)))), SetAttribute((attr: "age", attr_expr: Float(0.0))), SetAttribute((attr: "velocity", attr_expr: Vec3((0.0, 1.0, 0.0))))], render_modifiers: [ParticleTexture(()), Orient((mode: AlongVelocity))],"#,
        );
        let d = check_project(&project, &assets_root());
        assert!(d.errors.is_empty() && d.warnings.is_empty(), "{}", d.render());
        let text = package_string(&project).unwrap();
        assert!(text.contains("version: 2") && !text.contains("preview"), "{}", text);
    }

    /// A project with a layer and the given top-level blocks.
    fn param_project(blocks: &str, layer_fields: &str, init: &str) -> ProjectV2 {
        parse(&format!(
            r#"(
                version: 2,
                {}
                assets: [(id: "spark1", kind: Texture((path: "omagari://textures/spark1.png")))],
                layers: [Particles((
                    name: "a",
                    texture: Some("spark1"),
                    {}
                    effect: (
                        capacity: 64,
                        spawner: (count: Single(10.0), spawn_duration: Single(1.0), period: Single(1.0), cycle_count: 0),
                        init_modifiers: [SetAttribute((attr: "age", attr_expr: Float(0.0))), SetAttribute((attr: "lifetime", attr_expr: Float(0.5))), {}],
                        render_modifiers: [ParticleTexture(()), Orient((mode: ParallelCameraDepthPlane))],
                    ),
                ))],
            )"#,
            blocks, layer_fields, init
        ))
    }

    #[test]
    fn parameter_and_trigger_references_are_checked() {
        let blocks = r#"
            parameters: [
                (name: "power", default: Float(0.5), min: 0.0, max: 1.0),
                (name: "tint", default: Color((1.0, 0.0, 0.0, 1.0))),
                (name: "spare", default: Float(2.0), min: 1.0, max: 0.0),
                (name: "power", default: Float(0.0)),
            ],
            triggers: [
                (name: "tick", pulses: [(param: "tint", to: 1.0, decay: 0.2), (param: "gone", to: 1.0, decay: -1.0)]),
                (name: "idle"),
            ],
            preview: Some((params: [("ghost", Float(1.0)), ("tint", Float(1.0))], auto_triggers: [("nope", 0.0)])),
        "#;
        let p = param_project(
            blocks,
            r#"play: OnTrigger("boom"), rate: Some(Param("tint", [(0.0, 1.0)])), visible: Some((param: "missing", from: 1.0, to: 0.0, fade: -1.0)),"#,
            r#"SetAttribute((attr: "size", attr_expr: Param("tint"))), SetAttribute((attr: "position", attr_expr: Operator(Mix(Vec3((0.0, 0.0, 0.0)), Vec3((1.0, 1.0, 1.0)), Param("nope")))))"#,
        );
        let d = check_project(&p, &assets_root());
        for needle in [
            "duplicate parameter name",
            "min 1 is above max 0",
            "pulse on \"tint\", a Color parameter",
            "pulse on \"gone\", which is not a parameter",
            "pulse decay -1",
            "Param(\"nope\") is not a parameter of this package",
            "rate reads \"tint\", a Color parameter",
            "visible reads \"missing\", which is not a parameter",
            "visible from 1 is above to 0",
            "visible fade -1",
            "plays on trigger \"boom\", which is not a trigger",
            "SetAttribute(\"size\") needs f32, got vec4",
        ] {
            assert!(has(&d.errors, needle), "{}\n{}", needle, d.render());
        }
        for needle in [
            "preview value for \"ghost\", which is not a parameter",
            "preview value for \"tint\" is a Float, but the parameter is a Color",
            "preview auto-fires \"nope\"",
            "after the first trigger it never stops",
        ] {
            assert!(has(&d.warnings, needle), "{}\n{}", needle, d.render());
        }
        assert!(has(&d.notes, "[trigger \"idle\"] no layer plays on it"), "{}", d.render());
        assert!(has(&d.notes, "[parameter \"spare\"] not used"), "{}", d.render());
        assert!(d.about("parameter \"spare\"").next().is_some(), "messages are grouped by parameter");

        // Payload and visibility reads without the matching layer settings.
        let p = param_project(
            r#"parameters: [(name: "power", default: Float(0.5))],"#,
            "",
            r#"SetAttribute((attr: "position", attr_expr: Property("trigger_pos"))), SetAttribute((attr: "alpha", attr_expr: Property("visibility")))"#,
        );
        let d = check_project(&p, &assets_root());
        assert!(d.errors.is_empty(), "{}", d.render());
        assert!(has(&d.warnings, "only layers with play OnTrigger get them"), "{}", d.render());
        assert!(has(&d.warnings, "has no visible range"), "{}", d.render());
        assert!(has(&d.notes, "[parameter \"power\"] not used"), "{}", d.render());
    }

    fn env_eval(text: &str, ctx: Ctx) -> Result<Val, String> {
        let mut rng = StdRng::seed_from_u64(1);
        let mut attrs = Attrs::default();
        attrs.set(Attribute::AGE, Val::F(0.5));
        attrs.set(Attribute::LIFETIME, Val::F(2.0));
        let mut env = Env { rng: &mut rng, attrs: &attrs, parent: None, time: 3.0, ctx, is_alive: true };
        let expr = ron::from_str::<ExprWriterEditor>(text).unwrap_or_else(|e| panic!("{}: {}", text, e));
        eval(&expr, &mut env)
    }

    /// Every operator of the table is simulated: some argument types make
    /// it evaluate (none is left "not simulated").
    #[test]
    fn every_operator_is_simulated() {
        let samples = [
            ExprWriterEditor::Float(0.5),
            ExprWriterEditor::Vec3(Vec3::new(0.5, 0.25, 1.0)),
            ExprWriterEditor::Vec4(Vec4::new(0.5, 0.25, 1.0, 0.75)),
            ExprWriterEditor::U32(7),
            ExprWriterEditor::I32(-3),
            ExprWriterEditor::BVec3(BVec3::new(true, false, true)),
            ExprWriterEditor::Bool(true),
        ];
        let mut rng = StdRng::seed_from_u64(1);
        let attrs = Attrs::default();
        for row in OPERATORS.iter() {
            let mut ok = false;
            for a in samples.iter() {
                for b in samples.iter() {
                    let mut op = (row.make)();
                    for (i, arg) in op.args_mut().into_iter().enumerate() {
                        *arg = if i == 0 { a.clone() } else { b.clone() };
                    }
                    let mut env = Env { rng: &mut rng, attrs: &attrs, parent: Some(&attrs), time: 0.0, ctx: Ctx::Init, is_alive: true };
                    ok |= eval_op(&op, &mut env).is_ok();
                }
            }
            assert!(ok, "{} is not simulated for any argument types", row.name);
        }
        for value in EXPR_VALUES.iter() {
            let expected_err = matches!(value.name, "IsAlive" | "AlphaCutoff");
            let mut env = Env { rng: &mut rng, attrs: &attrs, parent: None, time: 0.0, ctx: Ctx::Init, is_alive: true };
            assert_eq!(eval(&(value.make)(), &mut env).is_err(), expected_err, "{}", value.name);
        }
    }

    #[test]
    fn p2b_operators_follow_wgsl() {
        let v = |text: &str| env_eval(text, Ctx::Init);
        assert_eq!(v("Operator(Divide(Float(3.0), Float(2.0)))"), Ok(Val::F(1.5)));
        assert_eq!(v("Operator(Divide(I32(7), I32(0)))"), Ok(Val::I(7)), "WGSL: x / 0 = x");
        assert_eq!(v("Operator(Remainder(U32(7), U32(3)))"), Ok(Val::U(1)));
        assert_eq!(v("Operator(Remainder(Float(-5.5), Float(2.0)))"), Ok(Val::F(-1.5)));
        assert_eq!(v("Operator(Multiply(Vec3((1.0, 2.0, 3.0)), Float(2.0)))"), Ok(Val::V3(Vec3::new(2.0, 4.0, 6.0))));
        assert_eq!(v("Operator(Add(IVec3((1, 2, 3)), I32(1)))"), Ok(Val::IV(3, [2, 3, 4, 0])));
        assert!(v("Operator(Add(Float(1.0), U32(1)))").unwrap_err().contains("same type"));
        assert_eq!(v("Operator(Abs(I32(-4)))"), Ok(Val::I(4)));
        assert_eq!(v("Operator(Sign(Float(0.0)))"), Ok(Val::F(0.0)));
        assert_eq!(v("Operator(Round(Float(2.5)))"), Ok(Val::F(2.0)), "ties to even");
        assert_eq!(v("Operator(Fract(Float(-0.25)))"), Ok(Val::F(0.75)));
        assert_eq!(v("Operator(Saturate(Float(1.5)))"), Ok(Val::F(1.0)));
        assert_eq!(v("Operator(Step(Float(0.5), Float(0.7)))"), Ok(Val::F(1.0)), "Step(edge, x)");
        assert_eq!(v("Operator(Step(Float(0.5), Float(0.2)))"), Ok(Val::F(0.0)));
        assert_eq!(v("Operator(Length(Vec3((3.0, 4.0, 0.0))))"), Ok(Val::F(5.0)));
        assert_eq!(v("Operator(Dot(Vec3((1.0, 2.0, 3.0)), Vec3((1.0, 1.0, 1.0))))"), Ok(Val::F(6.0)));
        assert_eq!(v("Operator(Cross(Vec3((1.0, 0.0, 0.0)), Vec3((0.0, 1.0, 0.0))))"), Ok(Val::V3(Vec3::Z)));
        assert_eq!(v("Operator(Vec2(Float(1.0), Float(2.0)))"), Ok(Val::V2(Vec2::new(1.0, 2.0))));
        assert_eq!(v("Operator(Vec3(U32(1), U32(2), U32(3)))"), Ok(Val::UV(3, [1, 2, 3, 0])));
        assert_eq!(v("Operator(Vec4XyzW(Vec3((1.0, 2.0, 3.0)), Float(4.0)))"), Ok(Val::V4(Vec4::new(1.0, 2.0, 3.0, 4.0))));
        assert_eq!(v("Operator(Z(Vec4((1.0, 2.0, 3.0, 4.0))))"), Ok(Val::F(3.0)));
        assert!(v("Operator(X(Float(1.0)))").unwrap_err().contains("vector"));
        assert_eq!(v("Operator(LessThan(Vec3((1.0, 5.0, 0.0)), Vec3((2.0, 2.0, 2.0))))"), Ok(Val::BV(3, [true, false, true, false])));
        assert_eq!(v("Operator(Any(Operator(GreaterThan(Vec2((1.0, 5.0)), Vec2((2.0, 2.0))))))"), Ok(Val::B(true)));
        assert_eq!(v("Operator(All(BVec2((true, false))))"), Ok(Val::B(false)));
        assert_eq!(v("Operator(Cast(Float(2.9), I32))"), Ok(Val::I(2)));
        assert_eq!(v("Operator(Cast(Bool(true), F32))"), Ok(Val::F(1.0)));
        assert_eq!(v("Operator(Cast(Float(2.0), Vec3))"), Ok(Val::V3(Vec3::splat(2.0))));
        assert!(v("Operator(Cast(Vec3((1.0, 1.0, 1.0)), F32))").is_err());
        assert_eq!(v("Operator(Unpack4x8UNorm(Operator(Pack4x8UNorm(Vec4((1.0, 0.0, 1.0, 0.0))))))"), Ok(Val::V4(Vec4::new(1.0, 0.0, 1.0, 0.0))));
        assert_eq!(v("Operator(Unpack4x8SNorm(Operator(Pack4x8SNorm(Vec4((-1.0, 0.0, 1.0, 0.0))))))"), Ok(Val::V4(Vec4::new(-1.0, 0.0, 1.0, 0.0))));
        assert_eq!(v("Operator(Atan2(Float(1.0), Float(0.0)))"), Ok(Val::F(std::f32::consts::FRAC_PI_2)));
        assert_eq!(v("Operator(Clamp(I32(9), I32(0), I32(5)))"), Ok(Val::I(5)));
        // The flipbook prebuilt: frame = i32(age / lifetime × frames).
        let frame = ExprWriterEditor::Operator(Box::new(ExprOperatorEditor::Cast(ExprWriterEditor::Float(0.0), ValueTypeDef::I32)));
        let _ = frame;
        let prebuilt = ron::to_string(&sprite_index_over_lifetime(8).attr_expr).unwrap();
        assert_eq!(v(&prebuilt), Ok(Val::I(2)), "age 0.5 of 2 s, 8 frames");
        // Built-in values.
        assert_eq!(v("DeltaTime"), Ok(Val::F(DT)));
        assert_eq!(v("RealTime"), Ok(Val::F(3.0)));
        let Ok(Val::V2(r)) = v("Rand(Vec2)") else { panic!() };
        assert!(r.cmpge(Vec2::ZERO).all() && r.cmplt(Vec2::ONE).all());
        let Ok(Val::F(n)) = v("Operator(Normal(Float(10.0), Float(0.0)))") else { panic!() };
        assert_eq!(n, 10.0);
        // Not available in shaders, or not here.
        assert!(v("RandU32").unwrap_err().contains("only have float random functions"));
        assert!(v("Rand(U32)").unwrap_err().contains("only have float random functions"));
        assert!(v("IsAlive").unwrap_err().contains("update modifiers"));
        assert_eq!(env_eval("IsAlive", Ctx::Update), Ok(Val::B(true)));
        assert!(v("AlphaCutoff").unwrap_err().contains("does not work in bevy_hanabi 0.19"));
        assert!(v("Operator(Asin(Float(1.5)))").unwrap_err().contains("outside -1..1"));
        assert!(v("Operator(Asin(Operator(Multiply(RandFloat, Float(1.5)))))").is_ok(), "not a constant");
        assert!(v("Operator(Uniform(Float(0.0), Param(\"x\")))").is_err());
        assert!(v("Operator(Uniform(Float(0.0), Operator(Add(Float(1.0), Float(1.0)))))").unwrap_err().contains("Cast"));
        assert!(v("Operator(Uniform(Float(0.0), Operator(Cast(Operator(Add(Float(1.0), Float(1.0))), F32))))").is_ok());
    }

    /// The P2b modifiers are checked and simulated: cone positions, kill
    /// volumes, radial / tangent forces, flipbooks, render settings.
    #[test]
    fn p2b_modifiers_are_checked_and_simulated() {
        let init = r#"SetAttribute((attr: "age", attr_expr: Float(0.0))), SetAttribute((attr: "lifetime", attr_expr: Float(1.0))), SetAttribute((attr: "size", attr_expr: Float(0.05)))"#;
        let effect = |init_extra: &str, update: &str, render: &str| {
            format!(
                "init_modifiers: [{}, {}], update_modifiers: [{}], render_modifiers: [ParticleTexture(()), Orient((mode: ParallelCameraDepthPlane)), {}],",
                init, init_extra, update, render
            )
        };
        // A cone fountain: every birth inside the cone (height 1, radius 0.1 → 0.5).
        let cone = layer_project(
            "play: OnStart,",
            &effect(r#"SetPositionCone3d((height_expr: Float(1.0), base_radius_expr: Float(0.1), top_radius_expr: Float(0.5), dimension: Volume))"#, "", ""),
        );
        let d = check_project(&cone, &assets_root());
        assert!(d.errors.is_empty() && d.warnings.is_empty(), "{}", d.render());
        let (sims, _) = simulate_project(&resolve_properties(&cone), 200, 3);
        for s in sims[0].samples.iter() {
            let p = s.birth_pos;
            let r = Vec2::new(p.x, p.z).length();
            assert!((0.0..=1.0).contains(&p.y) && r <= 0.1 + 0.4 * p.y + 1e-4, "{:?}", p);
        }
        // Kill sphere: particles flying outwards at 2 m/s die at 0.5 m (0.25 s).
        let kill = layer_project(
            "play: OnStart,",
            &effect(
                r#"SetPositionSphere((center_expr: Vec3((0.0, 0.0, 0.0)), radius_expr: Float(0.01), dimension: Surface)), SetVelocitySphere((center_expr: Vec3((0.0, 0.0, 0.0)), speed_expr: Float(2.0)))"#,
                r#"KillSphere((center_expr: Vec3((0.0, 0.0, 0.0)), radius_expr: Float(0.5), kill_inside: false))"#,
                "",
            ),
        );
        let (sims, _) = simulate_project(&resolve_properties(&kill), 50, 3);
        for s in sims[0].samples.iter() {
            let death = s.death.expect("dies");
            assert!((0.2..0.3).contains(&death), "died at {}", death);
        }
        // A box kills too.
        let aabb = layer_project(
            "play: OnStart,",
            &effect(
                r#"SetAttribute((attr: "velocity", attr_expr: Vec3((0.0, 1.0, 0.0))))"#,
                r#"KillAabb((center_expr: Vec3((0.0, 0.0, 0.0)), half_size_expr: Vec3((1.0, 0.5, 1.0)), kill_inside: false))"#,
                "",
            ),
        );
        let (sims, _) = simulate_project(&resolve_properties(&aabb), 20, 3);
        assert!(sims[0].samples.iter().all(|s| s.death.is_some_and(|d| d < 0.6)));
        // Swirl: tangent + inward pull keeps particles near the ring.
        let swirl = layer_project(
            "play: OnStart,",
            &effect(
                r#"SetPositionCircle((center_expr: Vec3((0.0, 0.0, 0.0)), axis_expr: Vec3((0.0, 1.0, 0.0)), radius_expr: Float(1.0), dimension: Surface)), SetVelocityTangent((origin_expr: Vec3((0.0, 0.0, 0.0)), axis_expr: Vec3((0.0, 1.0, 0.0)), speed_expr: Float(1.0)))"#,
                r#"TangentAccel((origin_expr: Vec3((0.0, 0.0, 0.0)), axis_expr: Vec3((0.0, 1.0, 0.0)), accel_expr: Float(0.5))), RadialAccel((origin_expr: Vec3((0.0, 0.0, 0.0)), accel_expr: Float(-1.0)))"#,
                "",
            ),
        );
        let d = check_project(&swirl, &assets_root());
        assert!(d.errors.is_empty() && d.warnings.is_empty(), "{}", d.render());
        let (sims, _) = simulate_project(&resolve_properties(&swirl), 50, 3);
        for s in sims[0].samples.iter() {
            assert!(s.path.iter().all(|p| (0.5..1.6).contains(&p.length())), "left the ring: {:?}", s.path);
        }
        // Type errors in the new modifiers.
        let bad = layer_project(
            "play: OnStart,",
            &effect(
                "",
                r#"KillSphere((center_expr: Float(0.0), radius_expr: Float(1.0))), RadialAccel((origin_expr: Vec3((0.0, 0.0, 0.0)), accel_expr: Vec3((1.0, 0.0, 0.0))))"#,
                r#"Round((roundness_expr: Vec3((1.0, 1.0, 1.0))))"#,
            ),
        );
        let d = check_project(&bad, &assets_root());
        assert!(has(&d.errors, "KillSphere center must be vec3"), "{}", d.render());
        assert!(has(&d.errors, "RadialAccel accel must be f32"), "{}", d.render());
        assert!(has(&d.errors, "render Round: Round roundness must be f32"), "{}", d.render());
        // Wrong lists, a cone in update, integer random values, IsAlive in init.
        let wrong = layer_project(
            "play: OnStart,",
            &effect(
                r#"KillAabb(()), SetAttribute((attr: "u32_0", attr_expr: RandU32)), SetAttribute((attr: "f32_0", attr_expr: Operator(Cast(IsAlive, F32))))"#,
                r#"SetPositionCone3d(())"#,
                "",
            ),
        );
        let d = check_project(&wrong, &assets_root());
        assert!(has(&d.errors, "KillAabb is an update modifier but is listed in init_modifiers"), "{}", d.render());
        assert!(has(&d.errors, "SetPositionCone3d is an init modifier but is listed in update_modifiers"), "{}", d.render());
        assert!(has(&d.errors, "RandU32 is not available"), "{}", d.render());
        assert!(has(&d.errors, "IsAlive exists only in update modifiers"), "{}", d.render());
        // SetAttribute may run every frame (update list) in v2.
        let every_frame = layer_project(
            "play: OnStart,",
            &effect(r#"SetAttribute((attr: "position", attr_expr: Vec3((0.0, 0.0, 0.0))))"#, r#"SetAttribute((attr: "f32_0", attr_expr: Age))"#, ""),
        );
        let d = check_project(&every_frame, &assets_root());
        assert!(d.errors.is_empty(), "{}", d.render());

        // Flipbook: grid, frames, texture, analysis text.
        let mut flip = layer_project(
            "play: OnStart,",
            &effect("", "", "Flipbook(())"),
        );
        let d = check_project(&flip, &assets_root());
        assert!(has(&d.warnings, "Flipbook has no grid"), "{}", d.render());
        assert!(has(&d.warnings, "nothing sets it"), "{}", d.render());
        let AssetKind::Texture(t) = &mut flip.assets[0].kind;
        t.grid = Some((2, 4));
        flip.layers[0].as_particles_mut().unwrap().effect.update_modifiers.push(ModifierEditor::SetAttribute(sprite_index_over_lifetime(8)));
        let d = check_project(&flip, &assets_root());
        assert!(!has(&d.warnings, "Flipbook"), "{}", d.render());
        let report = analyze_project(&flip, &assets_root(), 20, None);
        assert!(report.contains("flipbook 2 × 4 = 8 frames"), "{}", report);

        // Render-time settings and their warnings.
        let render = layer_project(
            "play: OnStart,",
            &effect(
                r#"SetAttribute((attr: "velocity", attr_expr: Vec3((0.0, 1.0, 0.0))))"#,
                "",
                r#"SetColor((color: Uniform(((1.0, 0.0, 0.0, 1.0), (0.0, 0.0, 1.0, 1.0))), blend: Overwrite, mask: (15))), SetSize((size: Single((0.2, 0.2, 0.2)))), ScreenSpaceSize(()), Round((roundness_expr: RandFloat)), Orient((mode: AlongVelocity, rotation_expr: Some(Age)))"#,
            ),
        );
        let mut render = render;
        {
            let effect = &mut render.layers[0].as_particles_mut().unwrap().effect;
            effect.motion_integration = MotionIntegrationDef::None;
            effect.prng_seed = Some(5);
            effect.simulation_condition = SimulationConditionDef::Always;
        }
        let d = check_project(&render, &assets_root());
        for needle in [
            "SetColor with a random range",
            "ScreenSpaceSize reads the particle size as screen pixels",
            "Round reads a random value",
            "Orient rotation is ignored with AlongVelocity",
            "motion_integration is None",
        ] {
            assert!(has(&d.warnings, needle), "{}\n{}", needle, d.render());
        }
        let report = analyze_project(&render, &assets_root(), 20, None);
        for needle in [
            "SetColor (render): Overwrite",
            "size: SetSize (render) (0.20, 0.20, 0.20) screen pixels",
            "no motion integration",
            "random seed 5",
            "simulated even when not visible",
            "size in screen pixels",
        ] {
            assert!(report.contains(needle), "{}\n{}", needle, report);
        }
        // With no motion integration the particles stay where they are born.
        let (sims, _) = simulate_project(&resolve_properties(&render), 20, 3);
        assert!(sims[0].samples.iter().all(|s| (s.end_pos - s.birth_pos).length() < 1e-5));
    }

    #[test]
    fn new_operators_evaluate_and_type_check() {
        let mut rng = StdRng::seed_from_u64(1);
        let attrs = Attrs::default();
        let mut env = Env { rng: &mut rng, attrs: &attrs, parent: None, time: 0.0, ctx: Ctx::Init, is_alive: true };
        let mut value = |text: &str| eval(&ron::from_str::<ExprWriterEditor>(text).unwrap(), &mut env);
        assert_eq!(value("Operator(Mix(Float(2.0), Float(4.0), Float(0.25)))"), Ok(Val::F(2.5)));
        assert_eq!(
            value("Operator(Mix(Vec3((0.0, 0.0, 0.0)), Vec3((2.0, 4.0, 6.0)), Float(0.5)))"),
            Ok(Val::V3(Vec3::new(1.0, 2.0, 3.0))),
            "a vector mix with an f32 fraction"
        );
        assert_eq!(value("Operator(Clamp(Float(3.0), Float(0.0), Float(1.0)))"), Ok(Val::F(1.0)));
        assert_eq!(value("Operator(SmoothStep(Float(0.0), Float(1.0), Float(0.5)))"), Ok(Val::F(0.5)));
        assert_eq!(value("Operator(SmoothStep(Float(0.0), Float(2.0), Float(3.0)))"), Ok(Val::F(1.0)));
        assert_eq!(value("Operator(Min(Float(3.0), Float(1.0)))"), Ok(Val::F(1.0)));
        assert_eq!(value("Operator(Max(U32(3), U32(7)))"), Ok(Val::U(7)));
        assert!(value("Operator(Clamp(Vec3((0.0, 0.0, 0.0)), Float(0.0), Float(1.0)))").unwrap_err().contains("same float type"));
        assert!(value("Param(\"x\")").unwrap_err().contains("not a parameter"));
        assert_eq!(ExprWriterEditor::Operator(Box::new(ExprOperatorEditor::Max(ExprWriterEditor::Float(1.0), ExprWriterEditor::RandFloat))).scalar_range(), Some((1.0, 1.0)));
        // Every operator has a table row.
        for info in OPERATORS.iter() {
            assert_eq!((info.make)().name(), info.name);
            assert_eq!((info.make)().args().len(), info.args.len(), "{}", info.name);
        }
    }

    /// The default parameter sets (min, middle, max) show how a bound rate,
    /// a parameter-driven size and a While layer change.
    #[test]
    fn analysis_compares_parameter_sets() {
        let p = parse(
            r#"(
                version: 2,
                parameters: [
                    (name: "power", default: Float(0.0), min: 0.0, max: 1.0, smoothing: 0.3),
                    (name: "tint", default: Color((0.2, 1.0, 0.2, 1.0))),
                ],
                assets: [(id: "glow1", kind: Texture((path: "omagari://textures/glow1.png")))],
                layers: [
                    Particles((
                        name: "aura",
                        texture: Some("glow1"),
                        rate: Some(Param("power", [(0.0, 5.0), (1.0, 50.0)])),
                        effect: (
                            capacity: 256,
                            spawner: (count: Single(10.0), spawn_duration: Single(1.0), period: Single(1.0), cycle_count: 0),
                            init_modifiers: [
                                SetAttribute((attr: "position", attr_expr: Vec3((0.0, 0.0, 0.0)))),
                                SetAttribute((attr: "age", attr_expr: Float(0.0))),
                                SetAttribute((attr: "lifetime", attr_expr: Float(1.0))),
                                SetAttribute((attr: "size", attr_expr: Operator(Mix(Float(0.1), Float(0.3), Param("power"))))),
                                SetAttribute((attr: "hdr_color", attr_expr: Param("tint"))),
                            ],
                            render_modifiers: [ParticleTexture(()), Orient((mode: ParallelCameraDepthPlane))],
                        ),
                    )),
                    Particles((
                        name: "sparks",
                        texture: Some("glow1"),
                        play: While("power", 0.75),
                        effect: (
                            capacity: 64,
                            spawner: (count: Single(20.0), spawn_duration: Single(1.0), period: Single(1.0), cycle_count: 0),
                            init_modifiers: [SetAttribute((attr: "position", attr_expr: Vec3((0.0, 0.0, 0.0)))), SetAttribute((attr: "age", attr_expr: Float(0.0))), SetAttribute((attr: "lifetime", attr_expr: Float(0.5)))],
                            render_modifiers: [ParticleTexture(()), Orient((mode: ParallelCameraDepthPlane))],
                        ),
                    )),
                ],
            )"#,
        );
        let d = check_project(&p, &assets_root());
        assert!(d.errors.is_empty() && d.warnings.is_empty() && d.notes.is_empty(), "{}", d.render());
        let report = analyze_project(&p, &assets_root(), 100, None);
        assert!(report.contains("rate: curve over \"power\""), "{}", report);
        assert!(report.contains("play: While(\"power\", 0.75)"), "{}", report);
        assert!(report.contains("parameter \"power\": Float, default 0"), "{}", report);
        assert!(report.contains("set number parameters at max: power=1, tint="), "{}", report);
        assert!(report.contains("[aura] changes with the parameters:"), "{}", report);
        assert!(report.contains("number parameters at min: ~5 alive; size 0.1"), "{}", report);
        let at_max = report.split("number parameters at max: ~").nth(1).unwrap_or_default();
        let alive: f32 = at_max.split(' ').next().unwrap_or_default().parse().unwrap_or(0.0);
        assert!((45.0..=55.0).contains(&alive) && at_max.contains("size 0.30 (fixed)"), "{}", report);
        assert!(report.contains("colour at mid-life green"), "{}", report);
        assert!(report.contains("number parameters at min: nothing spawns"), "{}", report);

        // Chosen sets; colours too.
        let red: ParamSet = vec![("tint".into(), ParamValue::Color(Vec4::new(1.0, 0.0, 0.0, 1.0)))];
        let sets = vec![("red".to_string(), red), ("default".to_string(), Vec::new())];
        let report = analyze_project_with_sets(&p, &assets_root(), 50, Some("aura"), Some(&sets));
        assert!(report.contains("red: ~5 alive; size 0.1") && report.contains("colour at mid-life red"), "{}", report);
        assert!(!report.contains("[sparks]"), "only the asked layer: {}", report);
    }
}
