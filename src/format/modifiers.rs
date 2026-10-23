//! Particle modifiers of the effect model and how each one becomes a
//! bevy_hanabi modifier.

use bevy::prelude::*;
use bevy_hanabi::prelude::*;
use serde::Deserialize;
use serde::Serialize;

use super::expr::*;
use super::hanabi_serde;

pub trait ModifierProducer<T>
where
    T: bevy_hanabi::Modifier,
{
    fn produce(&self, writer: &EffectWriter) -> T;
}

pub trait RenderModifierProducer<T>
where
    T: bevy_hanabi::Modifier,
{
    type Output;
    fn produce(&self) -> T;
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct SetAttributeModifierEditor {
    #[serde(with = "hanabi_serde::attribute")]
    pub attr: Attribute,
    pub attr_expr: ExprWriterEditor,
}

impl ModifierProducer<SetAttributeModifier> for SetAttributeModifierEditor {
    fn produce(&self, writer: &EffectWriter) -> SetAttributeModifier {
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

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct InheritAttributeModifierEditor {
    #[serde(with = "hanabi_serde::attribute")]
    pub attr: Attribute,
}

impl ModifierProducer<InheritAttributeModifier> for InheritAttributeModifierEditor {
    fn produce(&self, _writer: &EffectWriter) -> InheritAttributeModifier {
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

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct SetVelocityCircleModifierEditor {
    pub center_expr: ExprWriterEditor,
    pub axis_expr: ExprWriterEditor,
    pub speed_expr: ExprWriterEditor,
}

impl ModifierProducer<SetVelocityCircleModifier> for SetVelocityCircleModifierEditor {
    fn produce(&self, writer: &EffectWriter) -> SetVelocityCircleModifier {
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

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct SetVelocitySphereModifierEditor {
    pub center_expr: ExprWriterEditor,
    pub speed_expr: ExprWriterEditor,
}

impl ModifierProducer<SetVelocitySphereModifier> for SetVelocitySphereModifierEditor {
    fn produce(&self, writer: &EffectWriter) -> SetVelocitySphereModifier {
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

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct SetVelocityTangentModifierEditor {
    pub origin_expr: ExprWriterEditor,
    pub axis_expr: ExprWriterEditor,
    pub speed_expr: ExprWriterEditor,
}

impl ModifierProducer<SetVelocityTangentModifier> for SetVelocityTangentModifierEditor {
    fn produce(&self, writer: &EffectWriter) -> SetVelocityTangentModifier {
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

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct SetPositionCircleModifierEditor {
    pub center_expr: ExprWriterEditor,
    pub axis_expr: ExprWriterEditor,
    pub radius_expr: ExprWriterEditor,
    #[serde(with = "hanabi_serde::shape_dimension")]
    pub dimension: ShapeDimension,
}

impl ModifierProducer<SetPositionCircleModifier> for SetPositionCircleModifierEditor {
    fn produce(&self, writer: &EffectWriter) -> SetPositionCircleModifier {
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

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct SetPositionSphereModifierEditor {
    pub center_expr: ExprWriterEditor,
    pub radius_expr: ExprWriterEditor,
    #[serde(with = "hanabi_serde::shape_dimension")]
    pub dimension: ShapeDimension,
}

impl ModifierProducer<SetPositionSphereModifier> for SetPositionSphereModifierEditor {
    fn produce(&self, writer: &EffectWriter) -> SetPositionSphereModifier {
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

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct AccelModifierEditor {
    pub accel_expr: ExprWriterEditor,
}

impl ModifierProducer<AccelModifier> for AccelModifierEditor {
    fn produce(&self, writer: &EffectWriter) -> AccelModifier {
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

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct LinearDragModifierEditor {
    pub drag_expr: ExprWriterEditor,
}

impl ModifierProducer<LinearDragModifier> for LinearDragModifierEditor {
    fn produce(&self, writer: &EffectWriter) -> LinearDragModifier {
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

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct EmitSpawnEventModifierEditor {
    #[serde(with = "hanabi_serde::event_emit_condition")]
    pub condition: EventEmitCondition,
    pub count_expr: ExprWriterEditor,
    pub child_index: u32,
}

impl ModifierProducer<EmitSpawnEventModifier> for EmitSpawnEventModifierEditor {
    fn produce(&self, writer: &EffectWriter) -> EmitSpawnEventModifier {
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

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct ConformToSphereModifierEditor {
    pub origin_expr: ExprWriterEditor,
    pub radius_expr: ExprWriterEditor,
    pub influence_dist_expr: ExprWriterEditor,
    pub attraction_accel_expr: ExprWriterEditor,
    pub max_attraction_speed_expr: ExprWriterEditor,
}

impl ModifierProducer<ConformToSphereModifier> for ConformToSphereModifierEditor {
    fn produce(&self, writer: &EffectWriter) -> ConformToSphereModifier {
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

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
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

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct ColorOverLifetimeModifierEditor {
    pub gradient: GradientVec4Editor,
    #[serde(default, with = "hanabi_serde::option_color_blend_mode")]
    pub blend: Option<ColorBlendMode>,
    #[serde(default, with = "hanabi_serde::option_color_blend_mask")]
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

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
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

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
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

/// How a particle texture is combined with the particle colour (bevy_hanabi's
/// `ImageSampleMapping`).
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ImageSampleMappingDef {
    /// `color = color * texture`.
    Modulate,
    /// `color.rgb = color.rgb * texture.rgb`, alpha unchanged.
    ModulateRGB,
    /// `color.a = color.a * texture.r`: the texture's red channel is opacity.
    #[default]
    ModulateOpacityFromR,
}

impl From<ImageSampleMappingDef> for ImageSampleMapping {
    fn from(value: ImageSampleMappingDef) -> Self {
        match value {
            ImageSampleMappingDef::Modulate => ImageSampleMapping::Modulate,
            ImageSampleMappingDef::ModulateRGB => ImageSampleMapping::ModulateRGB,
            ImageSampleMappingDef::ModulateOpacityFromR => ImageSampleMapping::ModulateOpacityFromR,
        }
    }
}

/// Samples the layer's texture (texture slot `"color"`).
///
/// v1 projects had this hard-coded with `ModulateOpacityFromR`; migration adds
/// it explicitly.
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct ParticleTextureModifierEditor {
    pub sample_mapping: ImageSampleMappingDef,
}

impl ParticleTextureModifierEditor {
    /// The texture slot expression must be written before the module is
    /// finished, hence the writer.
    pub fn produce(&self, writer: &EffectWriter) -> ParticleTextureModifier {
        ParticleTextureModifier {
            texture_slot: writer.lit(0u32).expr(),
            sample_mapping: self.sample_mapping.into(),
        }
    }
}

/// How particle quads are oriented (bevy_hanabi's `OrientMode`).
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum OrientModeDef {
    /// Parallel to the camera's depth plane.
    ParallelCameraDepthPlane,
    /// Facing the camera position.
    FaceCameraPosition,
    /// Stretched along the particle velocity.
    #[default]
    AlongVelocity,
}

impl From<OrientModeDef> for OrientMode {
    fn from(value: OrientModeDef) -> Self {
        match value {
            OrientModeDef::ParallelCameraDepthPlane => OrientMode::ParallelCameraDepthPlane,
            OrientModeDef::FaceCameraPosition => OrientMode::FaceCameraPosition,
            OrientModeDef::AlongVelocity => OrientMode::AlongVelocity,
        }
    }
}

/// Orients particles. v1 projects had `AlongVelocity` hard-coded; migration
/// adds it explicitly.
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct OrientModifierEditor {
    pub mode: OrientModeDef,
    /// Spin of the particle square around the view direction, in radians
    /// (f32, render time; e.g. `Age × speed`). bevy_hanabi uses it with
    /// `ParallelCameraDepthPlane` and `FaceCameraPosition` only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rotation_expr: Option<ExprWriterEditor>,
}

impl OrientModifierEditor {
    /// The rotation expression must be written before the module is
    /// finished, hence the writer.
    pub fn produce(&self, writer: &EffectWriter) -> OrientModifier {
        let modifier = OrientModifier::new(self.mode.into());
        match &self.rotation_expr {
            Some(rotation) => modifier.with_rotation(rotation.produce(writer).expr()),
            None => modifier,
        }
    }
}

// --- P2b modifiers ---------------------------------------------------------------

/// Random position in a truncated cone along the layer's +Y axis, with its
/// base circle at the layer origin (bevy_hanabi's `SetPositionCone3dModifier`;
/// it always fills the volume, `dimension` is kept for its file format). The
/// cone turns with the emitter. Init only.
#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct SetPositionCone3dModifierEditor {
    pub height_expr: ExprWriterEditor,
    pub base_radius_expr: ExprWriterEditor,
    pub top_radius_expr: ExprWriterEditor,
    #[serde(with = "hanabi_serde::shape_dimension")]
    pub dimension: ShapeDimension,
}

impl ModifierProducer<SetPositionCone3dModifier> for SetPositionCone3dModifierEditor {
    fn produce(&self, writer: &EffectWriter) -> SetPositionCone3dModifier {
        SetPositionCone3dModifier {
            height: self.height_expr.produce(writer).expr(),
            base_radius: self.base_radius_expr.produce(writer).expr(),
            top_radius: self.top_radius_expr.produce(writer).expr(),
            dimension: self.dimension,
        }
    }
}

impl Default for SetPositionCone3dModifierEditor {
    fn default() -> Self {
        Self {
            height_expr: ExprWriterEditor::Float(0.5),
            base_radius_expr: ExprWriterEditor::Float(0.05),
            top_radius_expr: ExprWriterEditor::Float(0.3),
            dimension: ShapeDimension::Volume,
        }
    }
}

/// Pushes particles away from (positive) or pulls them towards (negative)
/// a point, in m/s² (bevy_hanabi's `RadialAccelModifier`). Update only.
#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct RadialAccelModifierEditor {
    pub origin_expr: ExprWriterEditor,
    pub accel_expr: ExprWriterEditor,
}

impl ModifierProducer<RadialAccelModifier> for RadialAccelModifierEditor {
    fn produce(&self, writer: &EffectWriter) -> RadialAccelModifier {
        let origin = self.origin_expr.produce(writer).expr();
        let accel = self.accel_expr.produce(writer).expr();
        RadialAccelModifier::new(origin, accel)
    }
}

impl Default for RadialAccelModifierEditor {
    fn default() -> Self {
        Self {
            origin_expr: ExprWriterEditor::Vec3(Vec3::ZERO),
            accel_expr: ExprWriterEditor::Float(-2.0),
        }
    }
}

/// Swirls particles around an axis through a point, in m/s² (bevy_hanabi's
/// `TangentAccelModifier`: direction = axis × (position − origin)). Update
/// only.
#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct TangentAccelModifierEditor {
    pub origin_expr: ExprWriterEditor,
    pub axis_expr: ExprWriterEditor,
    pub accel_expr: ExprWriterEditor,
}

impl ModifierProducer<TangentAccelModifier> for TangentAccelModifierEditor {
    fn produce(&self, writer: &EffectWriter) -> TangentAccelModifier {
        let origin = self.origin_expr.produce(writer).expr();
        let axis = self.axis_expr.produce(writer).expr();
        let accel = self.accel_expr.produce(writer).expr();
        TangentAccelModifier::new(origin, axis, accel)
    }
}

impl Default for TangentAccelModifierEditor {
    fn default() -> Self {
        Self {
            origin_expr: ExprWriterEditor::Vec3(Vec3::ZERO),
            axis_expr: ExprWriterEditor::Vec3(Vec3::Y),
            accel_expr: ExprWriterEditor::Float(3.0),
        }
    }
}

/// Kills particles outside (or inside, `kill_inside`) a sphere (bevy_hanabi's
/// `KillSphereModifier`, which takes the squared radius: produced as
/// `radius × radius`). Update only.
#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct KillSphereModifierEditor {
    pub center_expr: ExprWriterEditor,
    pub radius_expr: ExprWriterEditor,
    pub kill_inside: bool,
}

impl ModifierProducer<KillSphereModifier> for KillSphereModifierEditor {
    fn produce(&self, writer: &EffectWriter) -> KillSphereModifier {
        let center = self.center_expr.produce(writer).expr();
        let radius = self.radius_expr.produce(writer);
        let sqr_radius = radius.clone().mul(radius).expr();
        KillSphereModifier::new(center, sqr_radius).with_kill_inside(self.kill_inside)
    }
}

impl Default for KillSphereModifierEditor {
    fn default() -> Self {
        Self {
            center_expr: ExprWriterEditor::Vec3(Vec3::ZERO),
            radius_expr: ExprWriterEditor::Float(2.0),
            kill_inside: false,
        }
    }
}

/// Kills particles outside (or inside, `kill_inside`) a box given by its
/// center and half its size (bevy_hanabi's `KillAabbModifier`). Update only.
#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct KillAabbModifierEditor {
    pub center_expr: ExprWriterEditor,
    pub half_size_expr: ExprWriterEditor,
    pub kill_inside: bool,
}

impl ModifierProducer<KillAabbModifier> for KillAabbModifierEditor {
    fn produce(&self, writer: &EffectWriter) -> KillAabbModifier {
        let center = self.center_expr.produce(writer).expr();
        let half_size = self.half_size_expr.produce(writer).expr();
        KillAabbModifier::new(center, half_size).with_kill_inside(self.kill_inside)
    }
}

impl Default for KillAabbModifierEditor {
    fn default() -> Self {
        Self {
            center_expr: ExprWriterEditor::Vec3(Vec3::ZERO),
            half_size_expr: ExprWriterEditor::Vec3(Vec3::ONE),
            kill_inside: false,
        }
    }
}

/// Sets the colour when particles are drawn (bevy_hanabi's
/// `SetColorModifier`): a constant (or a random range, which bevy_hanabi's
/// render shader samples with one seed: the same value for every particle).
/// Combined with the particle colour by `blend` on the `mask` channels.
#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct SetColorModifierEditor {
    pub color: hanabi_serde::CpuVecDef<Vec4>,
    #[serde(with = "hanabi_serde::color_blend_mode")]
    pub blend: ColorBlendMode,
    #[serde(with = "hanabi_serde::color_blend_mask")]
    pub mask: ColorBlendMask,
}

impl SetColorModifierEditor {
    pub fn produce(&self) -> SetColorModifier {
        SetColorModifier {
            color: self.color.into(),
            blend: self.blend,
            mask: self.mask,
        }
    }
}

impl Default for SetColorModifierEditor {
    fn default() -> Self {
        Self {
            color: hanabi_serde::CpuVecDef::Single(Vec4::ONE),
            blend: ColorBlendMode::Modulate,
            mask: ColorBlendMask::RGBA,
        }
    }
}

/// Sets the size in metres when particles are drawn (bevy_hanabi's
/// `SetSizeModifier`), replacing the `size` attribute (like
/// SizeOverLifetime, which replaces it too; the later one wins).
#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct SetSizeModifierEditor {
    pub size: hanabi_serde::CpuVecDef<Vec3>,
}

impl SetSizeModifierEditor {
    pub fn produce(&self) -> SetSizeModifier {
        SetSizeModifier { size: self.size.into() }
    }
}

impl Default for SetSizeModifierEditor {
    fn default() -> Self {
        Self {
            size: hanabi_serde::CpuVecDef::Single(Vec3::splat(0.1)),
        }
    }
}

/// Plays a sprite sheet: draws frame `sprite_index` (an i32 attribute; frames
/// count left to right, then top to bottom) of the layer's texture
/// (bevy_hanabi's `FlipbookModifier`). `grid` = `(rows, columns)`; `None` =
/// the `grid` of the layer's texture asset. Animate `sprite_index` with an
/// update `SetAttribute` (see [`sprite_index_over_lifetime`]).
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct FlipbookModifierEditor {
    pub grid: Option<(u32, u32)>,
}

impl FlipbookModifierEditor {
    /// The grid it uses: its own, else the layer texture's `(rows, columns)`.
    pub fn grid_for(&self, texture_grid: Option<(u32, u32)>) -> Option<(u32, u32)> {
        self.grid.or(texture_grid)
    }

    /// Without a usable grid the whole texture is one frame (with a warning).
    pub fn produce(&self, texture_grid: Option<(u32, u32)>, warnings: &mut Vec<String>) -> FlipbookModifier {
        let (rows, cols) = match self.grid_for(texture_grid) {
            Some((rows, cols)) if rows > 0 && cols > 0 => (rows, cols),
            Some(grid) => {
                warnings.push(format!("Flipbook grid {:?} needs at least 1 row and 1 column; it uses 1 × 1", grid));
                (1, 1)
            }
            None => {
                warnings.push(
                    "Flipbook has no grid: set the texture asset's grid (rows × columns) or the modifier's; it uses 1 × 1"
                        .into(),
                );
                (1, 1)
            }
        };
        // bevy_hanabi's grid is (columns, rows).
        FlipbookModifier { sprite_grid_size: UVec2::new(cols, rows) }
    }
}

/// An update `SetAttribute` that steps `sprite_index` from frame 0 at birth
/// to frame `frames − 1` at death: `i32(age / lifetime × frames)`. The
/// prebuilt "animate sprite index over lifetime" of the editor and guide.
pub fn sprite_index_over_lifetime(frames: u32) -> SetAttributeModifierEditor {
    let op = |o: ExprOperatorEditor| ExprWriterEditor::Operator(Box::new(o));
    SetAttributeModifierEditor {
        attr: Attribute::SPRITE_INDEX,
        attr_expr: op(ExprOperatorEditor::Cast(
            op(ExprOperatorEditor::Multiply(
                op(ExprOperatorEditor::Divide(
                    ExprWriterEditor::Age,
                    op(ExprOperatorEditor::Attr(Attribute::LIFETIME)),
                )),
                ExprWriterEditor::Float(frames.max(1) as f32),
            )),
            ValueTypeDef::I32,
        )),
    }
}

/// The prebuilt init `SetAttribute` "random point on segment with radius":
/// `position = Mix(segment_a, segment_b, rand) + a random offset of up to
/// `radius` metres square to the segment` (a capsule-shaped volume along the
/// segment, e.g. sparks along a blade). For `Segment`-attached layers, which
/// get `segment_a` / `segment_b` from the runtime.
///
/// The offset is `normalize(cross(b − a, random vector) + tiny) × radius ×
/// √rand`: square to the segment, spread over the disc; the tiny bias keeps a
/// zero-length segment (sockets not resolved yet) finite.
pub fn segment_point_with_radius(radius: f32) -> SetAttributeModifierEditor {
    let op = |o: ExprOperatorEditor| ExprWriterEditor::Operator(Box::new(o));
    let a = || ExprWriterEditor::Property("segment_a".into());
    let b = || ExprWriterEditor::Property("segment_b".into());
    let random_vector = op(ExprOperatorEditor::Subtract(
        op(ExprOperatorEditor::Multiply(ExprWriterEditor::RandVec3, ExprWriterEditor::Float(2.0))),
        ExprWriterEditor::Float(1.0),
    ));
    let across = op(ExprOperatorEditor::Normalized(op(ExprOperatorEditor::Add(
        op(ExprOperatorEditor::Cross(op(ExprOperatorEditor::Subtract(b(), a())), random_vector)),
        ExprWriterEditor::Vec3(Vec3::new(1e-6, 0.0, 0.0)),
    ))));
    let distance = op(ExprOperatorEditor::Multiply(
        ExprWriterEditor::Float(radius.max(0.0)),
        op(ExprOperatorEditor::Sqrt(ExprWriterEditor::RandFloat)),
    ));
    SetAttributeModifierEditor {
        attr: Attribute::POSITION,
        attr_expr: op(ExprOperatorEditor::Add(
            op(ExprOperatorEditor::Mix(a(), b(), ExprWriterEditor::RandFloat)),
            op(ExprOperatorEditor::Multiply(across, distance)),
        )),
    }
}

/// Rounds the corners of particle squares (bevy_hanabi's `RoundModifier`):
/// 0 = square, 1 = circle / ellipse (f32, render time).
#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct RoundModifierEditor {
    pub roundness_expr: ExprWriterEditor,
}

impl RoundModifierEditor {
    pub fn produce(&self, writer: &EffectWriter) -> RoundModifier {
        RoundModifier { roundness: self.roundness_expr.produce(writer).expr() }
    }
}

impl Default for RoundModifierEditor {
    fn default() -> Self {
        Self { roundness_expr: ExprWriterEditor::Float(1.0) }
    }
}

/// Particle size in screen pixels instead of metres: the size (attribute,
/// SetSize or SizeOverLifetime before it) is read as pixels, so particles
/// keep their on-screen size at any distance (bevy_hanabi's
/// `ScreenSpaceSizeModifier`).
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct ScreenSpaceSizeModifierEditor {}

impl ScreenSpaceSizeModifierEditor {
    pub fn produce(&self) -> ScreenSpaceSizeModifier {
        ScreenSpaceSizeModifier
    }
}
