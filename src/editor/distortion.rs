//! Editor widgets for a distortion layer (`omagari::format::DistortionLayerDef`):
//! presets, the source (a shape at the attach point, a capsule along a
//! segment, or the target's own meshes), strength, noise, edge, tint, play
//! and fades.

use bevy::prelude::{Vec2, Vec3, Vec4};
use bevy_egui::egui;
use omagari::format::*;

use super::binding::binding_editor_with;
use super::effect::{ui_for_attach, ui_for_play, ui_for_visible};
use super::helpers::ui_for_f32_ex;
use super::hints;
use super::shape::{shape_ui, vec3_fields};
use super::surface::{builtin_noise_asset, color_row, mapping_ui, number_row, range_of, targets_ui, texture_slot_ui};
use super::trail::{ensure_trigger, trail_sockets};
use super::AppContext;
use crate::model_info::ModelInfo;

const WARN: egui::Color32 = egui::Color32::from_rgb(0xFF, 0x88, 0x66);

/// The trigger the "shockwave ripple" preset uses when the project has none.
pub const SLAM: &str = "slam";

/// Ready-made distortions (the inspector's preset buttons).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DistortionPreset {
    BladeHeatHaze,
    ShockwaveRipple,
    AuraShimmer,
}

impl DistortionPreset {
    pub const ALL: [DistortionPreset; 3] =
        [DistortionPreset::BladeHeatHaze, DistortionPreset::ShockwaveRipple, DistortionPreset::AuraShimmer];

    pub fn label(self) -> &'static str {
        match self {
            DistortionPreset::BladeHeatHaze => "heat haze around a blade",
            DistortionPreset::ShockwaveRipple => "shockwave ripple",
            DistortionPreset::AuraShimmer => "aura shimmer",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            DistortionPreset::BladeHeatHaze => "Rising heat haze in a capsule from one socket to the other (the project's first two sockets, or new blade_base / blade_tip sockets); stronger with the first number parameter",
            DistortionPreset::ShockwaveRipple => "A ring of bent air that grows from 0.3 to 3 m and fades over 0.6 s, each time the first trigger fires (adds a \"slam\" trigger when there is none); at the hit point with trigger_at",
            DistortionPreset::AuraShimmer => "The air just around the target's body shimmers (copies of its meshes pushed 4 cm out), strongest along the outline",
        }
    }
}

/// The noise every preset uses: the built-in noise (added to the assets
/// when missing).
fn preset_noise(assets: &mut Vec<AssetDef>, scale: f32, scroll: Vec2) -> DistortionNoiseDef {
    DistortionNoiseDef {
        texture: TextureSlotDef { asset: builtin_noise_asset(assets), channel: ChannelDef::Luminance },
        mapping: NoiseMappingDef::Triplanar,
        scale,
        scroll,
        contrast: 1.0,
        weight: 1.0,
    }
}

/// Applies a preset to a layer (the name and visible range are kept). The
/// blade haze uses the first two sockets (or adds `blade_base` /
/// `blade_tip`), the ripple the first trigger (or adds `slam`); `param` is
/// the project's first number parameter.
pub fn apply_distortion_preset(
    layer: &mut DistortionLayerDef,
    preset: DistortionPreset,
    assets: &mut Vec<AssetDef>,
    sockets: &mut Vec<SocketDef>,
    triggers: &mut Vec<TriggerDef>,
    param: Option<&ParamDef>,
) {
    let keep = (layer.name.clone(), layer.visible.clone());
    *layer = match preset {
        DistortionPreset::BladeHeatHaze => {
            let (from, to) = trail_sockets(sockets);
            DistortionLayerDef {
                attach: AttachDef::Segment { from, to },
                source: DistortionSourceDef::Shape {
                    shape: ShapeDef::Capsule { radius: 0.12, length: 1.0 },
                    follow: FollowDef::Full,
                    transform: ShapeTransformDef::default(),
                },
                strength: match param {
                    Some(p) => Binding::Param(p.name.clone(), vec![(p.min, 0.0), (p.max, 0.01)]),
                    None => Binding::Const(0.008),
                },
                noise: preset_noise(assets, 1.2, Vec2::new(0.0, 0.7)),
                noise_b: Some(DistortionNoiseDef { weight: 0.35, ..preset_noise(assets, 3.0, Vec2::new(0.15, 1.1)) }),
                edge: DistortionEdgeDef::Soft,
                edge_softness: 0.7,
                tint: Binding::Const(Vec4::new(1.08, 1.0, 0.92, 0.35)),
                fade: FadeDef { fade_in: 0.2, fade_out: 0.4 },
                ..Default::default()
            }
        }
        DistortionPreset::ShockwaveRipple => {
            let trigger = match triggers.first() {
                Some(t) => t.name.clone(),
                None => {
                    ensure_trigger(triggers, SLAM);
                    SLAM.to_string()
                }
            };
            DistortionLayerDef {
                source: DistortionSourceDef::Shape {
                    shape: ShapeDef::Sphere { radius: 1.0 },
                    follow: FollowDef::Position,
                    transform: ShapeTransformDef::default(),
                },
                scale: Binding::Age(vec![(0.0, Vec3::splat(0.3)), (0.6, Vec3::splat(3.0))]),
                strength: Binding::Age(vec![(0.0, 0.03), (0.6, 0.0)]),
                noise: preset_noise(assets, 1.5, Vec2::ZERO),
                edge: DistortionEdgeDef::Rim,
                edge_softness: 0.35,
                refraction: 0.75,
                play: PlayDef::OnTrigger(trigger),
                duration: Some(0.6),
                fade: FadeDef { fade_in: 0.0, fade_out: 0.1 },
                ..Default::default()
            }
        }
        DistortionPreset::AuraShimmer => DistortionLayerDef {
            source: DistortionSourceDef::Surface { targets: MeshTargetsDef::AllMeshes, offset: 0.04 },
            strength: Binding::Const(0.004),
            noise: preset_noise(assets, 2.5, Vec2::new(0.0, 0.6)),
            edge: DistortionEdgeDef::Rim,
            edge_softness: 0.4,
            fade: FadeDef { fade_in: 0.3, fade_out: 0.4 },
            ..Default::default()
        },
    };
    layer.name = keep.0;
    layer.visible = keep.1;
}

/// A new distortion layer (heat haze around a blade), with a unique name.
pub fn new_distortion_layer(
    assets: &mut Vec<AssetDef>,
    sockets: &mut Vec<SocketDef>,
    triggers: &mut Vec<TriggerDef>,
    existing: &[String],
    param: Option<&ParamDef>,
) -> DistortionLayerDef {
    let mut name = "distortion".to_string();
    let mut n = 2;
    while existing.contains(&name) {
        name = format!("distortion{}", n);
        n += 1;
    }
    let mut layer = DistortionLayerDef { name, ..Default::default() };
    apply_distortion_preset(&mut layer, DistortionPreset::BladeHeatHaze, assets, sockets, triggers, param);
    layer
}

fn noise_ui(ui: &mut egui::Ui, noise: &mut DistortionNoiseDef, assets: &mut Vec<AssetDef>, id: egui::Id) {
    ui.vertical(|ui| {
        texture_slot_ui(ui, &mut noise.texture, assets, id.with("texture"));
        mapping_ui(ui, &mut noise.mapping);
        ui.horizontal(|ui| {
            ui.label("scale").on_hover_text(hints::DISTORTION_NOISE_SCALE);
            noise.scale = ui_for_f32_ex(ui, noise.scale, 0.0, 100.0, 0.02);
            ui.label("scroll").on_hover_text(hints::DISTORTION_NOISE_SCROLL);
            noise.scroll.x = ui_for_f32_ex(ui, noise.scroll.x, -100.0, 100.0, 0.01);
            noise.scroll.y = ui_for_f32_ex(ui, noise.scroll.y, -100.0, 100.0, 0.01);
            ui.label("/s");
        });
        ui.horizontal(|ui| {
            ui.label("contrast").on_hover_text(hints::DISTORTION_NOISE_CONTRAST);
            noise.contrast = ui_for_f32_ex(ui, noise.contrast, 0.0, 10.0, 0.01);
            ui.label("weight").on_hover_text(hints::DISTORTION_NOISE_WEIGHT);
            noise.weight = ui_for_f32_ex(ui, noise.weight, 0.0, 1.0, 0.01);
        });
        if !noise.has_texture() {
            ui.colored_label(WARN, "⚠ no texture: this noise is off (pick the built-in noise)");
        }
    });
}

fn source_ui(ui: &mut egui::Ui, layer: &mut DistortionLayerDef, model: Option<&ModelInfo>, id: egui::Id) {
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            let is_shape = matches!(layer.source, DistortionSourceDef::Shape { .. });
            if ui.radio(is_shape, "shape").on_hover_text(hints::DISTORTION_SOURCE_SHAPE).clicked() && !is_shape {
                layer.source = DistortionSourceDef::default();
            }
            if ui.radio(!is_shape, "surface").on_hover_text(hints::DISTORTION_SOURCE_SURFACE).clicked() && is_shape {
                layer.source = DistortionSourceDef::Surface { targets: MeshTargetsDef::AllMeshes, offset: 0.04 };
            }
        });
        let segment = matches!(layer.attach, AttachDef::Segment { .. });
        match &mut layer.source {
            DistortionSourceDef::Shape { shape, .. } => {
                shape_ui(ui, shape, id.with("shape"));
                if segment && matches!(shape, ShapeDef::Capsule { .. }) {
                    ui.label(egui::RichText::new("the capsule runs from one socket to the other (its length is ignored)").weak());
                }
            }
            DistortionSourceDef::Surface { targets, offset } => {
                targets_ui(ui, targets, model, id.with("targets"));
                ui.horizontal(|ui| {
                    ui.label("offset").on_hover_text(hints::DISTORTION_OFFSET);
                    *offset = ui_for_f32_ex(ui, *offset, -1.0, 1.0, 0.001);
                    ui.label("m");
                });
            }
        }
    });
}

/// The distortion layer inspector.
#[allow(clippy::too_many_arguments)]
pub fn draw_distortion_layer(
    layer: &mut DistortionLayerDef,
    assets: &mut Vec<AssetDef>,
    sockets: &mut Vec<SocketDef>,
    triggers: &mut Vec<TriggerDef>,
    parameters: &[ParamDef],
    stage: PreviewStage,
    app: &mut AppContext,
    ui: &mut egui::Ui,
    index: u64,
    model: Option<&ModelInfo>,
) {
    let id = ui.make_persistent_id(format!("distortion layer {}", index));
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new("Presets:").weak());
        for preset in DistortionPreset::ALL {
            if ui.small_button(preset.label()).on_hover_text(preset.hint()).clicked() {
                let param = parameters.iter().find(|p| p.kind() == ParamKind::Float);
                apply_distortion_preset(layer, preset, assets, sockets, triggers, param);
            }
        }
    });
    if stage == PreviewStage::Air {
        ui.colored_label(WARN, "⚠ AIR stage: only the room's walls are behind the effect; FLOOR, CAPSULE or MODEL shows the bending better")
            .on_hover_text(hints::DISTORTION_BEHIND);
    }
    ui.add_space(4.0);
    egui::Grid::new(id.with("grid")).num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
        ui.label("Name").on_hover_text(hints::NAME);
        ui.add(egui::TextEdit::singleline(&mut layer.name).desired_width(180.0)).on_hover_text(hints::NAME);
        ui.end_row();

        ui.label("Source").on_hover_text(hints::DISTORTION_SOURCE);
        source_ui(ui, layer, model, id);
        ui.end_row();

        let shape_source = matches!(layer.source, DistortionSourceDef::Shape { .. });
        if shape_source {
            ui.label("Attach").on_hover_text(hints::DISTORTION_ATTACH);
            ui.horizontal(|ui| ui_for_attach(ui, &mut layer.attach, &app.socket_names, id));
            ui.end_row();
        }
        let segment_capsule = layer.is_segment_capsule();
        if let DistortionSourceDef::Shape { follow, transform, .. } = &mut layer.source
            && !segment_capsule
        {
            ui.label("Follow").on_hover_text(hints::SHAPE_FOLLOW);
            ui.horizontal(|ui| {
                for f in FollowDef::ALL {
                    ui.radio_value(follow, f, f.name()).on_hover_text(hints::SHAPE_FOLLOW);
                }
            });
            ui.end_row();

            ui.label("Offset").on_hover_text(hints::SHAPE_OFFSET);
            vec3_fields(ui, &mut transform.offset, -1000.0, 1000.0, 0.01, "m");
            ui.end_row();

            ui.label("Rotation").on_hover_text(hints::SHAPE_ROTATION);
            vec3_fields(ui, &mut transform.rotation_deg, -360.0, 360.0, 0.5, "°");
            ui.end_row();
        }
        if shape_source {
            ui.label("Scale").on_hover_text(hints::DISTORTION_SCALE);
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
        }

        number_row(ui, "Strength", hints::DISTORTION_STRENGTH, &mut layer.strength, app, parameters, "of the view", (0.0, 0.2), 0.0002, id);

        ui.label("Noise").on_hover_text(hints::DISTORTION_NOISE);
        noise_ui(ui, &mut layer.noise, assets, id.with("noise a"));
        ui.end_row();

        ui.label("Noise B").on_hover_text(hints::DISTORTION_NOISE_B);
        ui.vertical(|ui| {
            let mut on = layer.noise_b.is_some();
            if ui.checkbox(&mut on, "second noise").on_hover_text(hints::DISTORTION_NOISE_B).changed() {
                layer.noise_b = on.then(|| DistortionNoiseDef {
                    texture: layer.noise.texture.clone(),
                    scale: layer.noise.scale * 2.5,
                    weight: 0.5,
                    ..Default::default()
                });
            }
            if let Some(noise) = &mut layer.noise_b {
                noise_ui(ui, noise, assets, id.with("noise b"));
            }
        });
        ui.end_row();

        ui.label("Edge").on_hover_text(hints::DISTORTION_EDGE);
        ui.horizontal(|ui| {
            for edge in DistortionEdgeDef::ALL {
                ui.radio_value(&mut layer.edge, edge, edge.name()).on_hover_text(hints::DISTORTION_EDGE);
            }
            ui.label("softness").on_hover_text(hints::DISTORTION_EDGE);
            layer.edge_softness = ui_for_f32_ex(ui, layer.edge_softness, 0.0, 1.0, 0.01);
        });
        ui.end_row();

        ui.label("Refraction").on_hover_text(hints::DISTORTION_REFRACTION);
        ui.horizontal(|ui| {
            layer.refraction = ui_for_f32_ex(ui, layer.refraction, 0.0, 1.0, 0.01);
            ui.label(egui::RichText::new(if layer.refraction > 0.0 { "lens" } else { "shimmer only" }).weak());
        });
        ui.end_row();

        color_row(ui, "Tint", hints::DISTORTION_TINT, &mut layer.tint, app, parameters, id);

        ui.label("Play").on_hover_text(hints::DISTORTION_PLAY);
        ui.horizontal_wrapped(|ui| ui_for_play(ui, &mut layer.play, app, parameters, id));
        ui.end_row();

        ui.label("Duration").on_hover_text(hints::SHAPE_DURATION);
        ui.horizontal(|ui| {
            let mut on = layer.duration.is_some();
            if ui.checkbox(&mut on, "").on_hover_text(hints::SHAPE_DURATION).changed() {
                layer.duration = on.then_some(0.6);
            }
            if let Some(d) = &mut layer.duration {
                *d = ui_for_f32_ex(ui, *d, 0.0, 3600.0, 0.01);
                ui.label("s");
                ui.label(egui::RichText::new(if layer.is_one_shot() { "one-shot" } else { "the age repeats" }).weak());
            } else if matches!(layer.play, PlayDef::OnTrigger(_)) {
                ui.colored_label(WARN, "⚠ off: after the first trigger it stays on until the effect stops");
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

        ui.label("Draw order").on_hover_text(hints::DISTORTION_SORT);
        ui.horizontal(|ui| {
            layer.sort_bias = ui_for_f32_ex(ui, layer.sort_bias, -100.0, 100.0, 0.01);
            ui.label("m");
        });
        ui.end_row();
    });
    ui.add_space(4.0);
    ui.label(egui::RichText::new(hints::DISTORTION_LIMITS).weak().small());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_make_valid_layers() {
        let mut assets = Vec::new();
        let mut sockets = Vec::new();
        let mut triggers = Vec::new();
        let param = ParamDef { name: "heat".into(), ..Default::default() };
        let mut layer = new_distortion_layer(&mut assets, &mut sockets, &mut triggers, &["distortion".to_string()], Some(&param));
        assert_eq!(layer.name, "distortion2");
        assert!(layer.is_segment_capsule());
        assert_eq!(layer.strength.param(), Some("heat"));
        assert_eq!(sockets.len(), 2, "blade sockets added");
        assert_eq!(assets.len(), 1, "the built-in noise");
        for preset in DistortionPreset::ALL {
            apply_distortion_preset(&mut layer, preset, &mut assets, &mut sockets, &mut triggers, None);
            assert_eq!(layer.name, "distortion2", "{:?} keeps the name", preset);
            assert!(layer.noise.has_texture(), "{:?}", preset);
            if let Some(shape) = layer.source.shape() {
                assert!(shape.dimensions().iter().all(|(_, v)| *v > 0.0), "{:?}", preset);
            }
            if let PlayDef::OnTrigger(t) = &layer.play {
                assert_eq!(t, SLAM);
                assert!(layer.is_one_shot());
            }
        }
        assert_eq!(triggers.len(), 1, "slam added once");
        assert_eq!(assets.len(), 1);
        assert_eq!(sockets.len(), 2);
    }
}
