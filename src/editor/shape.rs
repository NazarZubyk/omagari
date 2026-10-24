//! Editor widgets for a shape layer (`omagari::format::ShapeLayerDef`): the
//! shape and its size, placement (attach, follow, offset, rotation), the
//! animated scale and spin, play / duration / fades, the effect material
//! (shared with surface layers) and presets.

use bevy::prelude::{Vec2, Vec3, Vec4};
use bevy_egui::egui;
use omagari::format::*;

use super::binding::binding_editor_with;
use super::effect::{ui_for_attach, ui_for_play, ui_for_visible};
use super::helpers::ui_for_f32_ex;
use super::hints;
use super::surface::{builtin_noise_asset, material_parts, material_rows, number_row, range_of};
use super::AppContext;

const WARN: egui::Color32 = egui::Color32::from_rgb(0xFF, 0x88, 0x66);

/// Ready-made looks for a shape layer (the inspector's preset buttons).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ShapePreset {
    ShieldBubble,
    GroundRing,
    Shockwave,
    SlashArc,
    GlowColumn,
}

impl ShapePreset {
    pub const ALL: [ShapePreset; 5] = [
        ShapePreset::ShieldBubble,
        ShapePreset::GroundRing,
        ShapePreset::Shockwave,
        ShapePreset::SlashArc,
        ShapePreset::GlowColumn,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ShapePreset::ShieldBubble => "shield bubble",
            ShapePreset::GroundRing => "ground ring",
            ShapePreset::Shockwave => "shockwave",
            ShapePreset::SlashArc => "slash arc",
            ShapePreset::GlowColumn => "glow column",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            ShapePreset::ShieldBubble => "A see-through sphere around the target with a glowing edge and slow noise; its opacity follows the first number parameter",
            ShapePreset::GroundRing => "A flat ring at the feet (0.88 m below the target root) that stays flat while the body moves; pulses and turns",
            ShapePreset::Shockwave => "A flat ring that grows from 0.2 to 3 m and fades over 0.6 s, each time the first trigger fires",
            ShapePreset::SlashArc => "A tapered arc in front of the target that sweeps across in 0.15 s and vanishes, each time the first trigger fires",
            ShapePreset::GlowColumn => "An open tube of light from the feet up, fading toward the top, scrolling upward",
        }
    }
}

/// Applies a preset to a layer: shape, placement, animation, play and
/// material (the name, attach and visible range are kept). `trigger` and
/// `param` are the project's first trigger and number parameter, if any.
pub fn apply_shape_preset(
    layer: &mut ShapeLayerDef,
    preset: ShapePreset,
    assets: &mut Vec<AssetDef>,
    trigger: Option<&str>,
    param: Option<&ParamDef>,
) {
    let noise = builtin_noise_asset(assets);
    let slot = TextureSlotDef { asset: noise, channel: ChannelDef::Luminance };
    let keep = (layer.name.clone(), layer.attach.clone(), layer.visible.clone());
    let on_trigger = PlayDef::OnTrigger(trigger.unwrap_or_default().to_string());
    *layer = match preset {
        ShapePreset::ShieldBubble => ShapeLayerDef {
            shape: ShapeDef::Sphere { radius: 1.1 },
            follow: FollowDef::Position,
            rotation_speed: Binding::Const(12.0),
            fade: FadeDef { fade_in: 0.3, fade_out: 0.4 },
            material: EffectMaterialDef {
                blend: EffectBlendDef::Add,
                color: Binding::Const(Vec4::new(0.3, 0.7, 1.6, 0.25)),
                opacity: match param {
                    Some(p) => Binding::Param(p.name.clone(), vec![(p.min, 0.0), (p.max, 1.0)]),
                    None => Binding::Const(1.0),
                },
                noise_a: Some(NoiseDef {
                    texture: slot,
                    mapping: NoiseMappingDef::Triplanar,
                    scale: Binding::Const(2.0),
                    scroll: Binding::Const(Vec2::new(0.05, 0.15)),
                    strength: Binding::Const(0.6),
                    contrast: Binding::Const(1.5),
                }),
                rim: Some(RimDef {
                    color: Binding::Const(Vec4::new(0.5, 1.2, 2.5, 0.8)),
                    power: Binding::Const(2.5),
                    intensity: match param {
                        Some(p) => Binding::Param(p.name.clone(), vec![(p.min, 0.0), (p.max, 2.0)]),
                        None => Binding::Const(2.0),
                    },
                }),
                pulse: Some(EffectPulseDef { frequency: Binding::Const(0.7), amount: Binding::Const(0.2) }),
                ..Default::default()
            },
            ..Default::default()
        },
        ShapePreset::GroundRing => ShapeLayerDef {
            shape: ShapeDef::Ring { inner: 0.55, outer: 0.75 },
            follow: FollowDef::PositionYaw,
            transform: ShapeTransformDef { offset: Vec3::new(0.0, -0.88, 0.0), rotation_deg: Vec3::ZERO },
            scale: Binding::Age(vec![(0.0, Vec3::ONE), (0.6, Vec3::new(1.08, 1.0, 1.08)), (1.2, Vec3::ONE)]),
            rotation_speed: Binding::Const(30.0),
            duration: Some(1.2),
            fade: FadeDef { fade_in: 0.3, fade_out: 0.4 },
            material: EffectMaterialDef {
                blend: EffectBlendDef::Add,
                color: Binding::Const(Vec4::new(1.6, 0.7, 0.2, 0.8)),
                noise_a: Some(NoiseDef {
                    texture: slot,
                    mapping: NoiseMappingDef::Uv,
                    scale: Binding::Const(3.0),
                    scroll: Binding::Const(Vec2::new(0.1, 0.0)),
                    strength: Binding::Const(0.5),
                    contrast: Binding::Const(1.5),
                }),
                pulse: Some(EffectPulseDef { frequency: Binding::Const(0.83), amount: Binding::Const(0.35) }),
                fill: Some(FillDef {
                    axis: AxisDef::V,
                    from: Binding::Const(0.3),
                    to: Binding::Const(0.7),
                    softness: Binding::Const(0.4),
                }),
                ..Default::default()
            },
            ..Default::default()
        },
        ShapePreset::Shockwave => ShapeLayerDef {
            shape: ShapeDef::Ring { inner: 0.8, outer: 1.0 },
            follow: FollowDef::PositionYaw,
            transform: ShapeTransformDef { offset: Vec3::new(0.0, -0.86, 0.0), rotation_deg: Vec3::ZERO },
            scale: Binding::Age(vec![(0.0, Vec3::new(0.2, 1.0, 0.2)), (0.6, Vec3::new(3.0, 1.0, 3.0))]),
            play: on_trigger,
            duration: Some(0.6),
            fade: FadeDef { fade_in: 0.0, fade_out: 0.3 },
            material: EffectMaterialDef {
                blend: EffectBlendDef::Add,
                color: Binding::Const(Vec4::new(0.6, 1.2, 2.5, 1.0)),
                emissive: Binding::Const(1.5),
                fill: Some(FillDef {
                    axis: AxisDef::V,
                    from: Binding::Const(0.3),
                    to: Binding::Const(0.7),
                    softness: Binding::Const(0.4),
                }),
                ..Default::default()
            },
            ..Default::default()
        },
        ShapePreset::SlashArc => ShapeLayerDef {
            shape: ShapeDef::Arc { radius: 1.0, width: 0.35, angle_deg: 150.0, taper: 1.0 },
            follow: FollowDef::Full,
            transform: ShapeTransformDef { offset: Vec3::new(0.0, 0.2, 0.0), rotation_deg: Vec3::new(0.0, 0.0, -25.0) },
            play: on_trigger,
            duration: Some(0.35),
            fade: FadeDef { fade_in: 0.0, fade_out: 0.1 },
            material: EffectMaterialDef {
                blend: EffectBlendDef::Add,
                color: Binding::Const(Vec4::new(2.5, 2.2, 1.6, 1.0)),
                emissive: Binding::Const(1.5),
                noise_a: Some(NoiseDef {
                    texture: slot,
                    mapping: NoiseMappingDef::Uv,
                    scale: Binding::Const(2.0),
                    scroll: Binding::Const(Vec2::new(-3.0, 0.0)),
                    strength: Binding::Const(0.5),
                    contrast: Binding::Const(2.0),
                }),
                fill: Some(FillDef {
                    axis: AxisDef::U,
                    from: Binding::Age(vec![(0.1, 0.0), (0.35, 1.0)]),
                    to: Binding::Age(vec![(0.0, 0.0), (0.15, 1.0)]),
                    softness: Binding::Const(0.25),
                }),
                ..Default::default()
            },
            ..Default::default()
        },
        ShapePreset::GlowColumn => ShapeLayerDef {
            shape: ShapeDef::Cylinder { radius: 0.6, height: 2.0, open_ends: true },
            follow: FollowDef::PositionYaw,
            transform: ShapeTransformDef { offset: Vec3::new(0.0, 0.1, 0.0), rotation_deg: Vec3::ZERO },
            rotation_speed: Binding::Const(20.0),
            fade: FadeDef { fade_in: 0.3, fade_out: 0.5 },
            material: EffectMaterialDef {
                blend: EffectBlendDef::Add,
                color: Binding::Const(Vec4::new(0.4, 1.0, 2.0, 0.6)),
                noise_a: Some(NoiseDef {
                    texture: slot,
                    mapping: NoiseMappingDef::Uv,
                    scale: Binding::Const(3.0),
                    scroll: Binding::Const(Vec2::new(0.0, -0.8)),
                    strength: Binding::Const(0.7),
                    contrast: Binding::Const(2.0),
                }),
                fill: Some(FillDef {
                    axis: AxisDef::V,
                    from: Binding::Const(0.0),
                    to: Binding::Const(0.6),
                    softness: Binding::Const(0.6),
                }),
                ..Default::default()
            },
            ..Default::default()
        },
    };
    layer.name = keep.0;
    layer.attach = keep.1;
    layer.visible = keep.2;
}

/// A new shape layer (a shield bubble), with a unique name.
pub fn new_shape_layer(
    assets: &mut Vec<AssetDef>,
    existing: &[String],
    trigger: Option<&str>,
    param: Option<&ParamDef>,
) -> ShapeLayerDef {
    let mut name = "shape".to_string();
    let mut n = 2;
    while existing.contains(&name) {
        name = format!("shape{}", n);
        n += 1;
    }
    let mut layer = ShapeLayerDef { name, ..Default::default() };
    apply_shape_preset(&mut layer, ShapePreset::ShieldBubble, assets, trigger, param);
    layer
}

fn dim(ui: &mut egui::Ui, label: &str, value: &mut f32, max: f32, unit: &str) {
    ui.label(label);
    *value = ui_for_f32_ex(ui, *value, 0.0, max, 0.01);
    ui.label(unit);
}

/// The shape kind and its dimensions.
pub(crate) fn shape_ui(ui: &mut egui::Ui, shape: &mut ShapeDef, id: egui::Id) {
    ui.vertical(|ui| {
        egui::ComboBox::from_id_salt(id.with("kind"))
            .selected_text(shape.kind_name())
            .show_ui(ui, |ui| {
                for kind in ShapeDef::all_defaults() {
                    let selected = std::mem::discriminant(&kind) == std::mem::discriminant(shape);
                    if ui.selectable_label(selected, kind.kind_name()).clicked() && !selected {
                        *shape = kind;
                    }
                }
            })
            .response
            .on_hover_text(hints::SHAPE_KIND);
        ui.horizontal_wrapped(|ui| match shape {
            ShapeDef::Sphere { radius } => dim(ui, "radius", radius, 100.0, "m"),
            ShapeDef::Capsule { radius, length } => {
                dim(ui, "radius", radius, 100.0, "m");
                dim(ui, "length", length, 100.0, "m");
            }
            ShapeDef::Cylinder { radius, height, open_ends } => {
                dim(ui, "radius", radius, 100.0, "m");
                dim(ui, "height", height, 100.0, "m");
                ui.checkbox(open_ends, "open ends").on_hover_text("A tube without caps, drawn inside and out");
            }
            ShapeDef::Ring { inner, outer } => {
                dim(ui, "inner", inner, 100.0, "m");
                dim(ui, "outer", outer, 100.0, "m");
            }
            ShapeDef::Torus { major, minor } => {
                dim(ui, "ring radius", major, 100.0, "m");
                dim(ui, "tube radius", minor, 100.0, "m");
            }
            ShapeDef::Cone { radius, height } => {
                dim(ui, "base radius", radius, 100.0, "m");
                dim(ui, "height", height, 100.0, "m");
            }
            ShapeDef::Quad { size, facing } => {
                dim(ui, "x", &mut size.x, 100.0, "m");
                dim(ui, "z", &mut size.y, 100.0, "m");
                let mut camera = *facing == QuadFacingDef::Camera;
                if ui
                    .checkbox(&mut camera, "faces the camera")
                    .on_hover_text("A sprite that always faces the camera (rotation ignored)")
                    .changed()
                {
                    *facing = if camera { QuadFacingDef::Camera } else { QuadFacingDef::Fixed };
                }
            }
            ShapeDef::Arc { radius, width, angle_deg, taper } => {
                dim(ui, "radius", radius, 100.0, "m");
                dim(ui, "width", width, 100.0, "m");
                dim(ui, "angle", angle_deg, 360.0, "°");
                ui.label("taper").on_hover_text("0 = even width, 1 = pointed ends");
                *taper = ui_for_f32_ex(ui, *taper, 0.0, 1.0, 0.01);
            }
        });
        match shape {
            ShapeDef::Capsule { .. } => {
                ui.label(egui::RichText::new("length = between the centres of the round ends").weak());
            }
            ShapeDef::Arc { .. } => {
                ui.label(egui::RichText::new("flat, in front (-Z); U runs along it from the left end").weak());
            }
            _ => {}
        }
        if shape.dimensions().iter().any(|(_, v)| *v <= 0.0) {
            ui.colored_label(WARN, "⚠ a size is 0 or less: nothing is drawn");
        }
    });
}

pub(crate) fn vec3_fields(ui: &mut egui::Ui, v: &mut Vec3, min: f32, max: f32, speed: f32, unit: &str) {
    ui.horizontal(|ui| {
        for (label, i) in [("x", 0), ("y", 1), ("z", 2)] {
            ui.label(label);
            v[i] = ui_for_f32_ex(ui, v[i], min, max, speed);
        }
        ui.label(unit);
    });
}

pub fn draw_shape_layer(
    layer: &mut ShapeLayerDef,
    assets: &mut Vec<AssetDef>,
    parameters: &[ParamDef],
    app: &mut AppContext,
    ui: &mut egui::Ui,
    index: u64,
) {
    let id = ui.make_persistent_id(format!("shape layer {}", index));
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new("Presets:").weak());
        for preset in ShapePreset::ALL {
            if ui.small_button(preset.label()).on_hover_text(preset.hint()).clicked() {
                let trigger = app.trigger_names.first().cloned();
                let param = parameters.iter().find(|p| p.kind() == ParamKind::Float);
                apply_shape_preset(layer, preset, assets, trigger.as_deref(), param);
            }
        }
    });
    ui.add_space(4.0);
    egui::Grid::new(id.with("grid")).num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
        ui.label("Name").on_hover_text(hints::NAME);
        ui.add(egui::TextEdit::singleline(&mut layer.name).desired_width(180.0))
            .on_hover_text(hints::NAME);
        ui.end_row();

        ui.label("Shape").on_hover_text(hints::SHAPE_KIND);
        shape_ui(ui, &mut layer.shape, id);
        ui.end_row();

        ui.label("Attach").on_hover_text(hints::ATTACH);
        ui.horizontal(|ui| ui_for_attach(ui, &mut layer.attach, &app.socket_names, id));
        ui.end_row();

        ui.label("Follow").on_hover_text(hints::SHAPE_FOLLOW);
        ui.horizontal(|ui| {
            for follow in FollowDef::ALL {
                ui.radio_value(&mut layer.follow, follow, follow.name()).on_hover_text(hints::SHAPE_FOLLOW);
            }
        });
        ui.end_row();

        ui.label("Offset").on_hover_text(hints::SHAPE_OFFSET);
        vec3_fields(ui, &mut layer.transform.offset, -1000.0, 1000.0, 0.01, "m");
        ui.end_row();

        ui.label("Rotation").on_hover_text(hints::SHAPE_ROTATION);
        vec3_fields(ui, &mut layer.transform.rotation_deg, -360.0, 360.0, 0.5, "°");
        ui.end_row();

        ui.label("Scale").on_hover_text(hints::SHAPE_SCALE);
        ui.vertical(|ui| {
            let mut value_ui = |ui: &mut egui::Ui, v: &mut Vec3| {
                ui.horizontal(|ui| {
                    vec3_fields(ui, v, 0.0, 1000.0, 0.01, "×");
                    if ui.small_button("=x").on_hover_text("Uniform: Y and Z take X").clicked() {
                        *v = Vec3::splat(v.x);
                    }
                });
            };
            binding_editor_with(ui, &mut layer.scale, &app.params, &range_of(parameters), false, &mut value_ui, id.with("scale"));
        });
        ui.end_row();

        number_row(ui, "Spin", hints::SHAPE_SPIN, &mut layer.rotation_speed, app, parameters, "°/s", (-100000.0, 100000.0), 1.0, id);
        ui.label("Spin axis").on_hover_text(hints::SHAPE_SPIN);
        vec3_fields(ui, &mut layer.rotation_axis, -1.0, 1.0, 0.01, "");
        ui.end_row();

        ui.label("Play").on_hover_text(hints::SHAPE_PLAY);
        ui.horizontal_wrapped(|ui| ui_for_play(ui, &mut layer.play, app, parameters, id));
        ui.end_row();

        ui.label("Duration").on_hover_text(hints::SHAPE_DURATION);
        ui.horizontal(|ui| {
            let mut on = layer.duration.is_some();
            if ui.checkbox(&mut on, "").on_hover_text(hints::SHAPE_DURATION).changed() {
                layer.duration = on.then_some(1.0);
            }
            if let Some(d) = &mut layer.duration {
                *d = ui_for_f32_ex(ui, *d, 0.0, 3600.0, 0.01);
                ui.label("s");
                ui.label(
                    egui::RichText::new(if layer.is_one_shot() { "one-shot" } else { "the age repeats" }).weak(),
                );
            } else if matches!(layer.play, PlayDef::OnStart | PlayDef::OnTrigger(_)) {
                ui.label(egui::RichText::new("off: plays until the effect stops").weak());
            }
        });
        ui.end_row();

        ui.label("Fade").on_hover_text(hints::SHAPE_FADE);
        ui.horizontal(|ui| {
            ui.label("in");
            layer.fade.fade_in = ui_for_f32_ex(ui, layer.fade.fade_in, 0.0, 60.0, 0.01);
            ui.label("s   out");
            layer.fade.fade_out = ui_for_f32_ex(ui, layer.fade.fade_out, 0.0, 60.0, 0.01);
            ui.label("s");
        })
        .response
        .on_hover_text(hints::SHAPE_FADE);
        ui.end_row();

        ui.label("Visible").on_hover_text(hints::VISIBLE);
        ui_for_visible(ui, &mut layer.visible, app, parameters, id);
        ui.end_row();

        ui.label("Draw order").on_hover_text(hints::SHAPE_SORT);
        ui.horizontal(|ui| {
            layer.sort_bias = ui_for_f32_ex(ui, layer.sort_bias, -100.0, 100.0, 0.01);
            ui.label("m");
        });
        ui.end_row();

        material_rows(ui, &mut layer.material, app, parameters, id);
    });
    if layer.shape.is_flat() {
        if let Some(fill) = &layer.material.fill {
            if !fill.axis.is_uv() && !matches!(fill.axis, AxisDef::X | AxisDef::NegX | AxisDef::Z | AxisDef::NegZ) {
                ui.colored_label(WARN, "⚠ the shape is flat: fill along Y has no length; use U, V, X or Z");
            }
        }
    }
    material_parts(&mut layer.material, assets, parameters, app, ui, id);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_make_valid_layers() {
        let mut assets = Vec::new();
        let param = ParamDef { name: "power".into(), ..Default::default() };
        let mut layer = new_shape_layer(&mut assets, &["shape".to_string()], Some("hit"), Some(&param));
        assert_eq!(layer.name, "shape2");
        assert_eq!(layer.material.opacity.param(), Some("power"));
        assert_eq!(assets.len(), 1, "the built-in noise");
        for preset in ShapePreset::ALL {
            apply_shape_preset(&mut layer, preset, &mut assets, Some("hit"), None);
            assert_eq!(layer.name, "shape2", "{:?} keeps the name", preset);
            assert!(layer.shape.dimensions().iter().all(|(_, v)| *v > 0.0), "{:?}", preset);
            if let PlayDef::OnTrigger(t) = &layer.play {
                assert_eq!(t, "hit");
                assert!(layer.is_one_shot(), "{:?}", preset);
            }
        }
        assert_eq!(assets.len(), 1);
    }
}
