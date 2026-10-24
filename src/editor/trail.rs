//! Editor widgets for trail and afterimage layers
//! (`omagari::format::{TrailLayerDef, AfterimageLayerDef}`): presets, the
//! sockets of a trail (package sockets and the preview model's nodes),
//! emission, the curves along the trail / over a ghost's life, the shared
//! effect material, the "show samples" debug toggle and the swing preview
//! (animation triggers for the emission's start / stop).

use bevy::prelude::{Vec2, Vec3, Vec4};
use bevy_egui::egui;
use omagari::format::*;

use super::binding::float_param_picker;
use super::effect::ui_for_visible;
use super::helpers::ui_for_f32_ex;
use super::hints;
use super::surface::{builtin_noise_asset, material_parts, material_rows};
use super::AppContext;
use crate::model_info::ModelInfo;

const WARN: egui::Color32 = egui::Color32::from_rgb(0xFF, 0x88, 0x66);

/// Default names of the swing triggers and blade sockets the presets use.
pub const SWING_START: &str = "swing_start";
pub const SWING_END: &str = "swing_end";
pub const BLADE_BASE: &str = "blade_base";
pub const BLADE_TIP: &str = "blade_tip";

/// What the trail / afterimage inspectors edit besides the layer.
pub struct EmitterContext<'a> {
    pub assets: &'a mut Vec<AssetDef>,
    pub parameters: &'a [ParamDef],
    pub triggers: &'a mut Vec<TriggerDef>,
    pub sockets: &'a mut Vec<SocketDef>,
    pub preview: &'a mut Option<PreviewDef>,
    /// The preview model the instance is on (node and mesh pickers).
    pub model: Option<&'a ModelInfo>,
    /// `(clip time, clip length)` of the playing preview animation.
    pub clip: Option<(f32, f32)>,
    /// The "show trail samples" debug toggle (not saved).
    pub show_samples: &'a mut bool,
}

/// Adds a trigger with this name when the project has none.
pub fn ensure_trigger(triggers: &mut Vec<TriggerDef>, name: &str) {
    if !triggers.iter().any(|t| t.name == name) {
        triggers.push(TriggerDef { name: name.to_string(), ..Default::default() });
    }
}

/// The two sockets a new trail uses: the project's first two, or new
/// `blade_base` / `blade_tip` package sockets (offsets from the target root
/// along +Y, to be moved onto the weapon in the SOCKETS tab).
pub fn trail_sockets(sockets: &mut Vec<SocketDef>) -> (String, String) {
    if sockets.len() >= 2 {
        return (sockets[0].name.clone(), sockets[1].name.clone());
    }
    for (name, y) in [(BLADE_BASE, 0.1), (BLADE_TIP, 1.0)] {
        if !sockets.iter().any(|s| s.name == name) {
            sockets.push(SocketDef { name: name.to_string(), node: None, offset: Vec3::new(0.0, y, 0.0) });
        }
    }
    (BLADE_BASE.to_string(), BLADE_TIP.to_string())
}

fn unique(base: &str, existing: &[String]) -> String {
    let mut name = base.to_string();
    let mut n = 2;
    while existing.contains(&name) {
        name = format!("{}{}", base, n);
        n += 1;
    }
    name
}

/// Ready-made trail looks.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TrailPreset {
    SwordTrail,
    EnergyTrail,
}

impl TrailPreset {
    pub const ALL: [TrailPreset; 2] = [TrailPreset::SwordTrail, TrailPreset::EnergyTrail];

    pub fn label(self) -> &'static str {
        match self {
            TrailPreset::SwordTrail => "sword trail",
            TrailPreset::EnergyTrail => "energy trail",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            TrailPreset::SwordTrail => "A white-hot arc behind the blade from swing_start to swing_end, 0.22 s long, thinning and fading toward its end, with streaks running along it",
            TrailPreset::EnergyTrail => "A cyan ribbon that always follows the two sockets for 0.5 s, tapering to a point, with flowing noise (a comet tail on a moving target)",
        }
    }
}

/// Applies a trail preset (the name and sockets are kept; the sword trail
/// adds the `swing_start` / `swing_end` triggers when missing).
pub fn apply_trail_preset(layer: &mut TrailLayerDef, preset: TrailPreset, assets: &mut Vec<AssetDef>, triggers: &mut Vec<TriggerDef>) {
    let noise = builtin_noise_asset(assets);
    let slot = TextureSlotDef { asset: noise, channel: ChannelDef::Luminance };
    let keep = (layer.name.clone(), layer.from.clone(), layer.to.clone(), layer.visible.clone());
    *layer = match preset {
        TrailPreset::SwordTrail => {
            ensure_trigger(triggers, SWING_START);
            ensure_trigger(triggers, SWING_END);
            TrailLayerDef {
                length: 0.22,
                subdivisions: 6,
                min_sample_distance: 0.01,
                emit: EmitDef::Trigger { start: SWING_START.into(), stop: SWING_END.into() },
                width_over_u: vec![(0.0, 1.0), (1.0, 0.6)],
                fade_over_u: vec![(0.0, 1.0), (0.2, 0.85), (1.0, 0.0)],
                material: EffectMaterialDef {
                    blend: EffectBlendDef::Add,
                    color: Binding::Const(Vec4::new(2.4, 2.0, 1.5, 1.0)),
                    emissive: Binding::Const(1.2),
                    noise_a: Some(NoiseDef {
                        texture: slot,
                        mapping: NoiseMappingDef::Uv,
                        scale: Binding::Const(2.0),
                        scroll: Binding::Const(Vec2::new(-2.0, 0.0)),
                        strength: Binding::Const(0.4),
                        contrast: Binding::Const(1.5),
                    }),
                    fill: Some(FillDef {
                        axis: AxisDef::V,
                        from: Binding::Const(0.0),
                        to: Binding::Const(1.0),
                        softness: Binding::Const(0.25),
                    }),
                    ..Default::default()
                },
                ..Default::default()
            }
        }
        TrailPreset::EnergyTrail => TrailLayerDef {
            length: 0.5,
            subdivisions: 4,
            min_sample_distance: 0.02,
            emit: EmitDef::Always,
            width_over_u: vec![(0.0, 1.0), (1.0, 0.1)],
            fade_over_u: vec![(0.0, 0.9), (1.0, 0.0)],
            material: EffectMaterialDef {
                blend: EffectBlendDef::Add,
                color: Binding::Const(Vec4::new(0.4, 1.2, 2.5, 0.9)),
                noise_a: Some(NoiseDef {
                    texture: slot,
                    mapping: NoiseMappingDef::Uv,
                    scale: Binding::Const(1.5),
                    scroll: Binding::Const(Vec2::new(-1.5, 0.0)),
                    strength: Binding::Const(0.6),
                    contrast: Binding::Const(2.0),
                }),
                ..Default::default()
            },
            ..Default::default()
        },
    };
    layer.name = keep.0;
    layer.from = keep.1;
    layer.to = keep.2;
    layer.visible = keep.3;
}

/// A new trail layer (a sword trail) on the project's first two sockets, or
/// on new blade sockets.
pub fn new_trail_layer(
    assets: &mut Vec<AssetDef>,
    triggers: &mut Vec<TriggerDef>,
    sockets: &mut Vec<SocketDef>,
    existing: &[String],
) -> TrailLayerDef {
    let (from, to) = trail_sockets(sockets);
    let mut layer = TrailLayerDef { name: unique("trail", existing), from, to, ..Default::default() };
    apply_trail_preset(&mut layer, TrailPreset::SwordTrail, assets, triggers);
    layer
}

/// Ready-made afterimage looks.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AfterimagePreset {
    /// Blue glowing copies during a swing.
    Afterimages,
    /// Dark see-through copies while the effect runs (a dash).
    ShadowDash,
}

impl AfterimagePreset {
    pub const ALL: [AfterimagePreset; 2] = [AfterimagePreset::Afterimages, AfterimagePreset::ShadowDash];

    pub fn label(self) -> &'static str {
        match self {
            AfterimagePreset::Afterimages => "afterimages",
            AfterimagePreset::ShadowDash => "shadow dash",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            AfterimagePreset::Afterimages => "Blue glowing copies every 0.04 s from swing_start to swing_end, each fading over 0.25 s with a bright edge (about 6 on screen)",
            AfterimagePreset::ShadowDash => "Dark see-through copies every 0.06 s while the effect runs, fading over 0.3 s (a dash or a fast run)",
        }
    }
}

pub fn apply_afterimage_preset(layer: &mut AfterimageLayerDef, preset: AfterimagePreset, triggers: &mut Vec<TriggerDef>) {
    let keep = (layer.name.clone(), layer.source.clone(), layer.visible.clone());
    *layer = match preset {
        AfterimagePreset::Afterimages => {
            ensure_trigger(triggers, SWING_START);
            ensure_trigger(triggers, SWING_END);
            AfterimageLayerDef {
                interval: 0.04,
                lifetime: 0.25,
                max_ghosts: 8,
                emit: EmitDef::Trigger { start: SWING_START.into(), stop: SWING_END.into() },
                fade_over_life: vec![(0.0, 0.8), (1.0, 0.0)],
                material: EffectMaterialDef {
                    blend: EffectBlendDef::Add,
                    color: Binding::Const(Vec4::new(0.3, 0.6, 1.6, 0.5)),
                    rim: Some(RimDef {
                        color: Binding::Const(Vec4::new(0.6, 1.0, 2.5, 1.0)),
                        power: Binding::Const(2.0),
                        intensity: Binding::Const(1.5),
                    }),
                    ..Default::default()
                },
                ..Default::default()
            }
        }
        AfterimagePreset::ShadowDash => AfterimageLayerDef {
            interval: 0.06,
            lifetime: 0.3,
            max_ghosts: 6,
            emit: EmitDef::Always,
            fade_over_life: vec![(0.0, 0.6), (1.0, 0.0)],
            material: EffectMaterialDef {
                blend: EffectBlendDef::Blend,
                color: Binding::Const(Vec4::new(0.05, 0.02, 0.12, 0.6)),
                rim: Some(RimDef {
                    color: Binding::Const(Vec4::new(0.5, 0.2, 1.0, 0.6)),
                    power: Binding::Const(2.5),
                    intensity: Binding::Const(1.0),
                }),
                ..Default::default()
            },
            ..Default::default()
        },
    };
    layer.name = keep.0;
    layer.source = keep.1;
    layer.visible = keep.2;
}

pub fn new_afterimage_layer(triggers: &mut Vec<TriggerDef>, existing: &[String]) -> AfterimageLayerDef {
    let mut layer = AfterimageLayerDef { name: unique("afterimages", existing), ..Default::default() };
    apply_afterimage_preset(&mut layer, AfterimagePreset::Afterimages, triggers);
    layer
}

/// A socket name with a picker of the package's sockets and the preview
/// model's nodes (bones marked).
fn socket_picker(ui: &mut egui::Ui, name: &mut String, sockets: &[SocketDef], model: Option<&ModelInfo>, id: egui::Id) {
    ui.add(egui::TextEdit::singleline(name).desired_width(120.0))
        .on_hover_text("A socket of this package (⚓ SOCKETS & FIT tab), or the name of a node (bone) in the target's model");
    ui.menu_button("⏷", |ui| {
        ui.label(egui::RichText::new("package sockets").weak());
        if sockets.is_empty() {
            ui.label(egui::RichText::new("(none: add one in ⚓ SOCKETS & FIT or 🌲 HIERARCHY)").weak());
        }
        for s in sockets {
            let text = match &s.node {
                Some(node) => format!("⚓ {}  ({} + offset)", s.name, node),
                None => format!("⚓ {}  (target root + offset)", s.name),
            };
            if ui.button(text).clicked() {
                *name = s.name.clone();
                ui.close();
            }
        }
        if let Some(model) = model {
            ui.separator();
            ui.menu_button("nodes of the preview model", |ui| {
                egui::ScrollArea::vertical().id_salt(id.with("nodes")).max_height(320.0).show(ui, |ui| {
                    for node in model.nodes.iter() {
                        let label = if node.bone { format!("🦴 {}", node.name) } else { node.name.clone() };
                        if ui.button(label).clicked() {
                            *name = node.name.clone();
                            ui.close();
                        }
                    }
                });
            });
        }
    });
    let known = sockets.iter().any(|s| &s.name == name) || model.is_some_and(|m| m.nodes.iter().any(|n| &n.name == name));
    if name.is_empty() {
        ui.colored_label(WARN, "⚠").on_hover_text("Pick a socket");
    } else if !known && model.is_some() {
        ui.colored_label(WARN, "⚠").on_hover_text("Neither a package socket nor a node of the preview model");
    }
}

/// Emission: Always / Trigger(start, stop) / While(param, threshold).
fn emit_ui(ui: &mut egui::Ui, emit: &mut EmitDef, triggers: &[TriggerDef], app: &AppContext, parameters: &[ParamDef], id: egui::Id) {
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            let kind = emit.kind_name();
            if ui.radio(kind == "Always", "Always").on_hover_text("While the effect runs").clicked() && kind != "Always" {
                *emit = EmitDef::Always;
            }
            if ui
                .radio(kind == "Trigger", "Trigger")
                .on_hover_text("From a start trigger to a stop trigger (e.g. swing_start / swing_end fired by the game's attack)")
                .clicked()
                && kind != "Trigger"
            {
                let start = triggers.iter().find(|t| t.name == SWING_START).or(triggers.first()).map(|t| t.name.clone());
                let stop = triggers.iter().find(|t| t.name == SWING_END).or(triggers.get(1)).map(|t| t.name.clone());
                *emit = EmitDef::Trigger {
                    start: start.unwrap_or_else(|| SWING_START.into()),
                    stop: stop.unwrap_or_else(|| SWING_END.into()),
                };
            }
            if ui.radio(kind == "While", "While").on_hover_text("While a number parameter is at or above a threshold").clicked() && kind != "While" {
                let param = parameters.iter().find(|p| p.kind() == ParamKind::Float);
                *emit = EmitDef::While(param.map(|p| p.name.clone()).unwrap_or_default(), param.map(|p| p.middle()).unwrap_or(0.5));
            }
        })
        .response
        .on_hover_text(hints::EMIT);
        match emit {
            EmitDef::Always => {}
            EmitDef::Trigger { start, stop } => {
                for (label, name) in [("start", start), ("stop", stop)] {
                    ui.horizontal(|ui| {
                        ui.label(label);
                        egui::ComboBox::from_id_salt(id.with(label))
                            .selected_text(if name.is_empty() { "(pick)".to_string() } else { name.clone() })
                            .show_ui(ui, |ui| {
                                if triggers.is_empty() {
                                    ui.label(egui::RichText::new("No triggers: add one in the TRIGGERS tab").weak());
                                }
                                for t in triggers.iter() {
                                    ui.selectable_value(name, t.name.clone(), &t.name);
                                }
                            });
                        if !triggers.iter().any(|t| &t.name == name) {
                            ui.colored_label(WARN, "⚠").on_hover_text("No trigger has this name (TRIGGERS tab)");
                        }
                    });
                }
            }
            EmitDef::While(param, threshold) => {
                ui.horizontal(|ui| {
                    float_param_picker(ui, param, &app.params, id.with("while"));
                    ui.label("≥");
                    *threshold = ui_for_f32_ex(ui, *threshold, -1.0e6, 1.0e6, 0.01);
                });
            }
        }
    });
}

/// A curve over 0..1 as a key list `(x, value)`.
fn unit_curve_ui(ui: &mut egui::Ui, keys: &mut Vec<(f32, f32)>, x_label: &str, max: f32, empty: &str, id: egui::Id) {
    ui.vertical(|ui| {
        if keys.is_empty() {
            ui.label(egui::RichText::new(empty).weak());
        }
        let mut remove = None;
        for (i, (x, v)) in keys.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.label(x_label);
                *x = ui_for_f32_ex(ui, *x, 0.0, 1.0, 0.01);
                ui.label("➡");
                *v = ui_for_f32_ex(ui, *v, 0.0, max, 0.01);
                if ui.small_button("❌").on_hover_text("Remove this key").clicked() {
                    remove = Some(i);
                }
            });
        }
        if let Some(i) = remove {
            keys.remove(i);
        }
        ui.horizontal(|ui| {
            if ui.small_button("+ key").clicked() {
                let x = keys.last().map(|k| (k.0 + 0.25).min(1.0)).unwrap_or(0.0);
                let v = keys.last().map(|k| k.1).unwrap_or(1.0);
                keys.push((x, v));
            }
            if keys.windows(2).any(|w| w[1].0 < w[0].0) {
                ui.colored_label(WARN, "⚠ keys must be sorted");
                if ui.small_button("sort").clicked() {
                    keys.sort_by(|a, b| a.0.total_cmp(&b.0));
                }
            }
        });
        let _ = id;
    });
}

/// "Preview swing": fire the emission's start and stop triggers at two clip
/// times of the preview animation (writes `preview.animation_triggers`).
fn swing_preview_ui(ui: &mut egui::Ui, emit: &EmitDef, ctx: &mut EmitterContext, id: egui::Id) {
    let EmitDef::Trigger { start, stop } = emit else { return };
    ui.label("Swing preview").on_hover_text(hints::SWING_PREVIEW);
    ui.vertical(|ui| {
        let preview = ctx.preview.get_or_insert_with(PreviewDef::default);
        let Some((now, length)) = ctx.clip.filter(|(_, d)| *d > 0.0) else {
            ui.label(egui::RichText::new("Pick an animation (PREVIEW tab, MODEL stage) to fire start / stop at clip times; or fire them in the TRIGGERS tab.").weak());
            return;
        };
        for name in [start, stop] {
            ui.horizontal(|ui| {
                let at = preview.animation_triggers.iter().position(|(n, _)| n == name);
                let mut on = at.is_some();
                if ui.checkbox(&mut on, format!("fire \"{}\" at", name)).on_hover_text(hints::SWING_PREVIEW).changed() {
                    if on {
                        preview.animation_triggers.push((name.clone(), now));
                    } else if let Some(i) = at {
                        preview.animation_triggers.remove(i);
                    }
                }
                if let Some(i) = preview.animation_triggers.iter().position(|(n, _)| n == name) {
                    let t = &mut preview.animation_triggers[i].1;
                    *t = ui_for_f32_ex(ui, *t, 0.0, length, 0.01);
                    ui.label("s");
                    if ui.small_button("⏱ now").on_hover_text("The animation's current time").clicked() {
                        *t = (now * 1000.0).round() / 1000.0;
                    }
                }
            });
        }
        ui.label(egui::RichText::new(format!("clip at {:.2} / {:.2} s (pause and scrub in the bottom bar)", now, length)).weak());
        let _ = id;
    });
    ui.end_row();
}

pub fn draw_trail_layer(layer: &mut TrailLayerDef, ctx: &mut EmitterContext, app: &mut AppContext, ui: &mut egui::Ui, index: u64) {
    let id = ui.make_persistent_id(format!("trail layer {}", index));
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new("Presets:").weak());
        for preset in TrailPreset::ALL {
            if ui.small_button(preset.label()).on_hover_text(preset.hint()).clicked() {
                apply_trail_preset(layer, preset, ctx.assets, ctx.triggers);
            }
        }
    });
    ui.add_space(4.0);
    let parameters = ctx.parameters;
    egui::Grid::new(id.with("grid")).num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
        ui.label("Name").on_hover_text(hints::NAME);
        ui.add(egui::TextEdit::singleline(&mut layer.name).desired_width(180.0)).on_hover_text(hints::NAME);
        ui.end_row();

        ui.label("From").on_hover_text(hints::TRAIL_SOCKETS);
        ui.horizontal(|ui| socket_picker(ui, &mut layer.from, ctx.sockets, ctx.model, id.with("from")));
        ui.end_row();
        ui.label("To").on_hover_text(hints::TRAIL_SOCKETS);
        ui.horizontal(|ui| {
            socket_picker(ui, &mut layer.to, ctx.sockets, ctx.model, id.with("to"));
            if ui.small_button("swap").on_hover_text("Swap From and To").clicked() {
                std::mem::swap(&mut layer.from, &mut layer.to);
            }
        });
        ui.end_row();

        ui.label("Length").on_hover_text(hints::TRAIL_LENGTH);
        ui.horizontal(|ui| {
            layer.length = ui_for_f32_ex(ui, layer.length, 0.0, 10.0, 0.005);
            ui.label("s");
        });
        ui.end_row();

        ui.label("Smoothing").on_hover_text(hints::TRAIL_SUBDIVISIONS);
        ui.horizontal(|ui| {
            let mut s = layer.subdivisions as f32;
            s = ui_for_f32_ex(ui, s, 1.0, 16.0, 0.1);
            layer.subdivisions = s.round().clamp(1.0, 16.0) as u32;
            ui.label("segments between samples");
        });
        ui.end_row();

        ui.label("Min distance").on_hover_text(hints::TRAIL_MIN_DISTANCE);
        ui.horizontal(|ui| {
            layer.min_sample_distance = ui_for_f32_ex(ui, layer.min_sample_distance, 0.0, 10.0, 0.001);
            ui.label("m");
        });
        ui.end_row();

        ui.label("Emit").on_hover_text(hints::EMIT);
        emit_ui(ui, &mut layer.emit, ctx.triggers, app, parameters, id.with("emit"));
        ui.end_row();

        swing_preview_ui(ui, &layer.emit, ctx, id.with("swing"));

        ui.label("Width along").on_hover_text(hints::TRAIL_WIDTH);
        unit_curve_ui(ui, &mut layer.width_over_u, "u", 4.0, "full width (from → to) everywhere", id.with("width"));
        ui.end_row();

        ui.label("Fade along").on_hover_text(hints::TRAIL_FADE_U);
        unit_curve_ui(ui, &mut layer.fade_over_u, "u", 1.0, "no fade along the trail", id.with("fade u"));
        ui.end_row();

        ui.label("Fade").on_hover_text(hints::EMITTER_FADE);
        ui.horizontal(|ui| {
            ui.label("in");
            layer.fade.fade_in = ui_for_f32_ex(ui, layer.fade.fade_in, 0.0, 60.0, 0.01);
            ui.label("s   out");
            layer.fade.fade_out = ui_for_f32_ex(ui, layer.fade.fade_out, 0.0, 60.0, 0.01);
            ui.label("s");
        });
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

        ui.label("Debug").on_hover_text("Draws the trail's samples (a line from From to To per sample) over the viewport; not saved");
        ui.checkbox(ctx.show_samples, "show samples");
        ui.end_row();

        material_rows(ui, &mut layer.material, app, parameters, id);
    });
    if layer.from == layer.to && !layer.from.is_empty() {
        ui.colored_label(WARN, "⚠ From and To are the same socket: the trail has no width");
    }
    ui.label(egui::RichText::new("u runs 0 (newest) → 1 (oldest) along the trail, v 0 (From) → 1 (To): fill along U or V, UV noise scrolls along it; Age curves read the seconds since emission started.").weak().small());
    material_parts(&mut layer.material, ctx.assets, parameters, app, ui, id);
}

fn mesh_names(model: Option<&ModelInfo>) -> Vec<String> {
    let mut names = Vec::new();
    if let Some(model) = model {
        for mesh in model.meshes.iter() {
            names.push(mesh.name.clone());
            for p in mesh.primitives.iter() {
                if !names.contains(&p.entity_name) {
                    names.push(p.entity_name.clone());
                }
            }
        }
    }
    names
}

fn source_ui(ui: &mut egui::Ui, source: &mut GhostSourceDef, model: Option<&ModelInfo>, id: egui::Id) {
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            let all = *source == GhostSourceDef::TargetModel;
            if ui.radio(all, "the whole target model").clicked() && !all {
                *source = GhostSourceDef::TargetModel;
            }
            if ui.radio(!all, "some meshes").clicked() && all {
                *source = GhostSourceDef::Meshes(Vec::new());
            }
        })
        .response
        .on_hover_text(hints::GHOST_SOURCE);
        let GhostSourceDef::Meshes(names) = source else { return };
        let offer = mesh_names(model);
        let mut remove = None;
        for (i, name) in names.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(format!("▣ {}", name));
                if model.is_some() && !offer.contains(name) {
                    ui.colored_label(WARN, "⚠").on_hover_text("The preview model has no mesh with this name");
                }
                if ui.small_button("❌").clicked() {
                    remove = Some(i);
                }
            });
        }
        if let Some(i) = remove {
            names.remove(i);
        }
        ui.horizontal(|ui| {
            let key = id.with("new mesh");
            let mut text: String = ui.data_mut(|d| d.get_temp::<String>(key).unwrap_or_default());
            let response = ui.add(egui::TextEdit::singleline(&mut text).hint_text("mesh name").desired_width(140.0));
            if ui.small_button("+ add").clicked() || (response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
                let name = text.trim().to_string();
                if !name.is_empty() && !names.contains(&name) {
                    names.push(name);
                }
                text.clear();
            }
            ui.data_mut(|d| d.insert_temp(key, text));
            if !offer.is_empty() {
                let mut picked = None;
                ui.menu_button("⏷", |ui| {
                    for name in offer.iter().filter(|n| !names.contains(n)) {
                        if ui.button(name).clicked() {
                            picked = Some(name.clone());
                            ui.close();
                        }
                    }
                });
                names.extend(picked);
            }
        });
    });
}

pub fn draw_afterimage_layer(layer: &mut AfterimageLayerDef, ctx: &mut EmitterContext, app: &mut AppContext, ui: &mut egui::Ui, index: u64) {
    let id = ui.make_persistent_id(format!("afterimage layer {}", index));
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new("Presets:").weak());
        for preset in AfterimagePreset::ALL {
            if ui.small_button(preset.label()).on_hover_text(preset.hint()).clicked() {
                apply_afterimage_preset(layer, preset, ctx.triggers);
            }
        }
    });
    ui.add_space(4.0);
    let parameters = ctx.parameters;
    egui::Grid::new(id.with("grid")).num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
        ui.label("Name").on_hover_text(hints::NAME);
        ui.add(egui::TextEdit::singleline(&mut layer.name).desired_width(180.0)).on_hover_text(hints::NAME);
        ui.end_row();

        ui.label("Copies").on_hover_text(hints::GHOST_SOURCE);
        source_ui(ui, &mut layer.source, ctx.model, id.with("source"));
        ui.end_row();

        ui.label("Interval").on_hover_text(hints::GHOST_INTERVAL);
        ui.horizontal(|ui| {
            layer.interval = ui_for_f32_ex(ui, layer.interval, 0.0, 10.0, 0.002);
            ui.label("s");
        });
        ui.end_row();

        ui.label("Lifetime").on_hover_text(hints::GHOST_LIFETIME);
        ui.horizontal(|ui| {
            layer.lifetime = ui_for_f32_ex(ui, layer.lifetime, 0.0, 10.0, 0.005);
            ui.label("s");
        });
        ui.end_row();

        ui.label("Max copies").on_hover_text(hints::GHOST_MAX);
        ui.horizontal(|ui| {
            let mut m = layer.max_ghosts as f32;
            m = ui_for_f32_ex(ui, m, 1.0, MAX_GHOSTS_LIMIT as f32, 0.1);
            layer.max_ghosts = m.round().clamp(1.0, MAX_GHOSTS_LIMIT as f32) as u32;
            ui.label(egui::RichText::new(format!("~{} on screen while emitting", layer.ghosts_on_screen())).weak());
        });
        ui.end_row();

        ui.label("Emit").on_hover_text(hints::EMIT);
        emit_ui(ui, &mut layer.emit, ctx.triggers, app, parameters, id.with("emit"));
        ui.end_row();

        swing_preview_ui(ui, &layer.emit, ctx, id.with("swing"));

        ui.label("Fade over life").on_hover_text(hints::GHOST_FADE);
        unit_curve_ui(ui, &mut layer.fade_over_life, "life", 1.0, "full until it goes", id.with("fade life"));
        ui.end_row();

        ui.label("Fade").on_hover_text(hints::EMITTER_FADE);
        ui.horizontal(|ui| {
            ui.label("in");
            layer.fade.fade_in = ui_for_f32_ex(ui, layer.fade.fade_in, 0.0, 60.0, 0.01);
            ui.label("s   out");
            layer.fade.fade_out = ui_for_f32_ex(ui, layer.fade.fade_out, 0.0, 60.0, 0.01);
            ui.label("s");
        });
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
    ui.label(egui::RichText::new("Age curves read each copy's age in seconds. Skinned meshes are copied in their current pose (frozen joints). A weapon in the hand is a separate model: put the package on the weapon for weapon copies.").weak().small());
    material_parts(&mut layer.material, ctx.assets, parameters, app, ui, id);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_layers_and_presets_are_valid() {
        let (mut assets, mut triggers, mut sockets) = (Vec::new(), Vec::new(), Vec::new());
        let trail = new_trail_layer(&mut assets, &mut triggers, &mut sockets, &["trail".to_string()]);
        assert_eq!(trail.name, "trail2");
        assert_eq!((trail.from.as_str(), trail.to.as_str()), (BLADE_BASE, BLADE_TIP));
        assert_eq!(sockets.len(), 2, "blade sockets added");
        assert_eq!(triggers.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), vec![SWING_START, SWING_END]);
        assert_eq!(assets.len(), 1, "the built-in noise");
        let mut layer = trail.clone();
        for preset in TrailPreset::ALL {
            apply_trail_preset(&mut layer, preset, &mut assets, &mut triggers);
            assert_eq!((layer.name.as_str(), layer.from.as_str()), ("trail2", BLADE_BASE), "{:?} keeps the name and sockets", preset);
            assert!(layer.length > 0.0 && layer.subdivisions >= 1);
            assert!(layer.fade_over_u.windows(2).all(|w| w[0].0 <= w[1].0));
        }
        assert_eq!((assets.len(), triggers.len()), (1, 2));
        // Existing sockets are reused.
        let mut sockets = vec![
            SocketDef { name: "base".into(), ..Default::default() },
            SocketDef { name: "tip".into(), ..Default::default() },
        ];
        let trail = new_trail_layer(&mut assets, &mut triggers, &mut sockets, &[]);
        assert_eq!((trail.from.as_str(), trail.to.as_str(), sockets.len()), ("base", "tip", 2));

        let mut ghosts = new_afterimage_layer(&mut triggers, &[]);
        assert_eq!(ghosts.name, "afterimages");
        for preset in AfterimagePreset::ALL {
            apply_afterimage_preset(&mut ghosts, preset, &mut triggers);
            assert!(ghosts.interval > 0.0 && ghosts.lifetime > ghosts.interval && ghosts.ghosts_on_screen() >= 4, "{:?}", preset);
        }
    }
}
