//! Omagari-owned serialization of the bevy_hanabi values stored in the model.
//!
//! The particle model keeps a few bevy_hanabi types in memory (attributes,
//! spawner settings, small enums), but their text form in project files is
//! defined here, not by bevy_hanabi's own serde impls. A bevy_hanabi upgrade can
//! therefore not silently change the file format: if hanabi renames or removes
//! something, these conversions stop compiling or report a clear error.
//!
//! Every format written here is byte-identical to what bevy_hanabi 0.19 wrote,
//! so v1 projects keep loading and saving unchanged.

use bevy_hanabi::prelude::*;
use serde::{de::Error as _, Deserialize, Deserializer, Serialize, Serializer};

/// A spawn count or duration: a constant or a uniform random range.
///
/// Same text form as bevy_hanabi's `CpuValue<f32>`: `Single(1.0)` or
/// `Uniform((0.5, 2.0))`.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
pub enum CpuValueDef {
    Single(f32),
    Uniform((f32, f32)),
}

impl Default for CpuValueDef {
    fn default() -> Self {
        CpuValueDef::Single(0.0)
    }
}

impl CpuValueDef {
    /// The smallest and largest value this can produce.
    pub fn range(&self) -> [f32; 2] {
        match *self {
            CpuValueDef::Single(v) => [v, v],
            CpuValueDef::Uniform((a, b)) => [a, b],
        }
    }

    fn finite_or(self, fallback: f32) -> Self {
        let fix = |v: f32| if v.is_finite() { v } else { fallback };
        match self {
            CpuValueDef::Single(v) => CpuValueDef::Single(fix(v)),
            CpuValueDef::Uniform((a, b)) => CpuValueDef::Uniform((fix(a), fix(b))),
        }
    }
}

impl From<CpuValueDef> for CpuValue<f32> {
    fn from(value: CpuValueDef) -> Self {
        match value {
            CpuValueDef::Single(v) => CpuValue::Single(v),
            CpuValueDef::Uniform(range) => CpuValue::Uniform(range),
        }
    }
}

impl From<CpuValue<f32>> for CpuValueDef {
    fn from(value: CpuValue<f32>) -> Self {
        match value {
            CpuValue::Single(v) => CpuValueDef::Single(v),
            CpuValue::Uniform(range) => CpuValueDef::Uniform(range),
            // `CpuValue` is non-exhaustive; keep the range of any future kind.
            #[allow(unreachable_patterns)]
            other => {
                let [a, b] = other.range();
                CpuValueDef::Uniform((a, b))
            }
        }
    }
}

/// Particle spawner settings, converted to bevy_hanabi's [`SpawnerSettings`]
/// when an effect is produced.
///
/// Same fields and text form as `SpawnerSettings` in bevy_hanabi 0.19.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
#[serde(default)]
pub struct SpawnerDef {
    /// Particles spawned over `spawn_duration`, every cycle.
    pub count: CpuValueDef,
    /// Seconds over which `count` particles are spawned.
    pub spawn_duration: CpuValueDef,
    /// Seconds between the starts of two cycles.
    pub period: CpuValueDef,
    /// Number of cycles; 0 = forever.
    pub cycle_count: u32,
    pub starts_active: bool,
    pub emit_on_start: bool,
}

impl Default for SpawnerDef {
    /// A steady stream of 500 particles per second, like a new editor effect.
    fn default() -> Self {
        SpawnerSettings::rate(500.0.into()).into()
    }
}

impl SpawnerDef {
    /// Builds bevy_hanabi spawner settings. Never panics: values bevy_hanabi
    /// rejects (infinite periods) are replaced.
    pub fn to_settings(&self) -> SpawnerSettings {
        SpawnerSettings::default()
            .with_count(self.count.into())
            .with_spawn_duration(self.spawn_duration.into())
            .with_period(self.period.finite_or(1.0).into())
            .with_cycle_count(self.cycle_count)
            .with_starts_active(self.starts_active)
            .with_emit_on_start(self.emit_on_start)
    }
}

/// Spawner patterns of bevy_hanabi's `SpawnerSettings` constructors.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SpawnerPreset {
    /// `SpawnerSettings::once`: `count` particles at once, one cycle.
    Once,
    /// `SpawnerSettings::rate`: `count` particles per second, forever.
    Rate,
    /// `SpawnerSettings::burst`: `count` particles at once every `period`
    /// seconds, forever.
    Burst,
}

impl SpawnerDef {
    /// This spawner switched to a preset pattern, keeping its count (also a
    /// random range), the period of a burst, and `starts_active` /
    /// `emit_on_start`.
    pub fn with_preset(&self, preset: SpawnerPreset) -> SpawnerDef {
        let count = self.count.into();
        let settings = match preset {
            SpawnerPreset::Once => SpawnerSettings::once(count),
            SpawnerPreset::Rate => SpawnerSettings::rate(count),
            SpawnerPreset::Burst => {
                let period = if self.period.range()[0] > 0.0 { self.period } else { CpuValueDef::Single(1.0) };
                SpawnerSettings::burst(count, period.finite_or(1.0).into())
            }
        };
        SpawnerDef {
            starts_active: self.starts_active,
            emit_on_start: self.emit_on_start,
            ..settings.into()
        }
    }

    /// The preset this spawner matches, if any.
    pub fn preset(&self) -> Option<SpawnerPreset> {
        [SpawnerPreset::Once, SpawnerPreset::Rate, SpawnerPreset::Burst]
            .into_iter()
            .find(|p| self.with_preset(*p) == *self)
    }
}

impl From<SpawnerSettings> for SpawnerDef {
    fn from(settings: SpawnerSettings) -> Self {
        SpawnerDef {
            count: settings.count().into(),
            spawn_duration: settings.spawn_duration().into(),
            period: settings.period().into(),
            cycle_count: settings.cycle_count(),
            starts_active: settings.starts_active(),
            emit_on_start: settings.emits_on_start(),
        }
    }
}

/// `#[serde(with)]` for a [`SpawnerSettings`] field, written as a [`SpawnerDef`].
pub mod spawner_settings {
    use super::*;

    pub fn serialize<S: Serializer>(value: &SpawnerSettings, s: S) -> Result<S::Ok, S::Error> {
        SpawnerDef::from(*value).serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<SpawnerSettings, D::Error> {
        Ok(SpawnerDef::deserialize(d)?.to_settings())
    }
}

/// `#[serde(with)]` for an [`Attribute`] field: the attribute's stable name
/// (`"position"`, `"u32_0"`, ...).
pub mod attribute {
    use super::*;

    pub fn serialize<S: Serializer>(value: &Attribute, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(value.name())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Attribute, D::Error> {
        let name = String::deserialize(d)?;
        attribute_from_name(&name)
            .ok_or_else(|| D::Error::custom(format!("unknown particle attribute \"{}\"", name)))
    }
}

/// Looks up an attribute by the name used in project files.
pub fn attribute_from_name(name: &str) -> Option<Attribute> {
    Attribute::from_name(name)
}

#[derive(Serialize, Deserialize)]
#[serde(rename = "ShapeDimension")]
enum ShapeDimensionDef {
    Surface,
    Volume,
}

/// `#[serde(with)]` for a [`ShapeDimension`] field: `Surface` or `Volume`.
pub mod shape_dimension {
    use super::*;

    pub fn serialize<S: Serializer>(value: &ShapeDimension, s: S) -> Result<S::Ok, S::Error> {
        match value {
            ShapeDimension::Surface => ShapeDimensionDef::Surface,
            ShapeDimension::Volume => ShapeDimensionDef::Volume,
        }
        .serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<ShapeDimension, D::Error> {
        Ok(match ShapeDimensionDef::deserialize(d)? {
            ShapeDimensionDef::Surface => ShapeDimension::Surface,
            ShapeDimensionDef::Volume => ShapeDimension::Volume,
        })
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename = "EventEmitCondition")]
enum EventEmitConditionDef {
    Always,
    OnDie,
}

/// `#[serde(with)]` for an [`EventEmitCondition`] field: `Always` or `OnDie`.
pub mod event_emit_condition {
    use super::*;

    pub fn serialize<S: Serializer>(value: &EventEmitCondition, s: S) -> Result<S::Ok, S::Error> {
        match value {
            EventEmitCondition::Always => EventEmitConditionDef::Always,
            EventEmitCondition::OnDie => EventEmitConditionDef::OnDie,
        }
        .serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<EventEmitCondition, D::Error> {
        Ok(match EventEmitConditionDef::deserialize(d)? {
            EventEmitConditionDef::Always => EventEmitCondition::Always,
            EventEmitConditionDef::OnDie => EventEmitCondition::OnDie,
        })
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename = "ColorBlendMode")]
enum ColorBlendModeDef {
    Overwrite,
    Add,
    Modulate,
}

/// `#[serde(with)]` for an `Option<ColorBlendMode>` field: `Some(Modulate)`, ...
pub mod option_color_blend_mode {
    use super::*;

    pub fn serialize<S: Serializer>(value: &Option<ColorBlendMode>, s: S) -> Result<S::Ok, S::Error> {
        value
            .map(|mode| match mode {
                ColorBlendMode::Overwrite => ColorBlendModeDef::Overwrite,
                ColorBlendMode::Add => ColorBlendModeDef::Add,
                ColorBlendMode::Modulate => ColorBlendModeDef::Modulate,
            })
            .serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<ColorBlendMode>, D::Error> {
        Ok(Option::<ColorBlendModeDef>::deserialize(d)?.map(|mode| match mode {
            ColorBlendModeDef::Overwrite => ColorBlendMode::Overwrite,
            ColorBlendModeDef::Add => ColorBlendMode::Add,
            ColorBlendModeDef::Modulate => ColorBlendMode::Modulate,
        }))
    }
}

/// Written as a newtype holding the RGBA bit mask, e.g. `(15)`.
#[derive(Serialize, Deserialize)]
#[serde(rename = "ColorBlendMask")]
struct ColorBlendMaskDef(u8);

/// `#[serde(with)]` for an `Option<ColorBlendMask>` field: `Some((15))`, ...
pub mod option_color_blend_mask {
    use super::*;

    pub fn serialize<S: Serializer>(value: &Option<ColorBlendMask>, s: S) -> Result<S::Ok, S::Error> {
        value.map(|mask| ColorBlendMaskDef(mask.bits())).serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<ColorBlendMask>, D::Error> {
        Ok(Option::<ColorBlendMaskDef>::deserialize(d)?
            .map(|mask| ColorBlendMask::from_bits_retain(mask.0)))
    }
}

/// `#[serde(with)]` for a [`ColorBlendMode`] field: `Overwrite`, `Add` or
/// `Modulate`.
pub mod color_blend_mode {
    use super::*;

    pub fn serialize<S: Serializer>(value: &ColorBlendMode, s: S) -> Result<S::Ok, S::Error> {
        match value {
            ColorBlendMode::Overwrite => ColorBlendModeDef::Overwrite,
            ColorBlendMode::Add => ColorBlendModeDef::Add,
            ColorBlendMode::Modulate => ColorBlendModeDef::Modulate,
        }
        .serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<ColorBlendMode, D::Error> {
        Ok(match ColorBlendModeDef::deserialize(d)? {
            ColorBlendModeDef::Overwrite => ColorBlendMode::Overwrite,
            ColorBlendModeDef::Add => ColorBlendMode::Add,
            ColorBlendModeDef::Modulate => ColorBlendMode::Modulate,
        })
    }
}

/// `#[serde(with)]` for a [`ColorBlendMask`] field: the RGBA bit mask as a
/// newtype, e.g. `(15)`.
pub mod color_blend_mask {
    use super::*;

    pub fn serialize<S: Serializer>(value: &ColorBlendMask, s: S) -> Result<S::Ok, S::Error> {
        ColorBlendMaskDef(value.bits()).serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<ColorBlendMask, D::Error> {
        Ok(ColorBlendMask::from_bits_retain(ColorBlendMaskDef::deserialize(d)?.0))
    }
}

/// A render-time constant or uniform random range of vectors (bevy_hanabi's
/// `CpuValue<Vec3>` / `CpuValue<Vec4>` in `SetSizeModifier` /
/// `SetColorModifier`): `Single((1.0, 1.0, 1.0))` or `Uniform(((..), (..)))`.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
pub enum CpuVecDef<T> {
    Single(T),
    Uniform((T, T)),
}

impl<T: Copy + bevy::reflect::FromReflect> From<CpuVecDef<T>> for CpuValue<T> {
    fn from(value: CpuVecDef<T>) -> Self {
        match value {
            CpuVecDef::Single(v) => CpuValue::Single(v),
            CpuVecDef::Uniform(range) => CpuValue::Uniform(range),
        }
    }
}
