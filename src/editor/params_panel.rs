//! The PARAMETERS and TRIGGERS tabs: the package's parameters (name, kind,
//! default, range, smoothing) with a live control driving the preview, and
//! its triggers (pulses) with Fire buttons and auto-fire. Every edit is a
//! `ProjectV2` edit (undo, unsaved marker, live preview); the live values and
//! auto-fire are saved in the project's preview block.

use bevy::math::Vec4;
use bevy_egui::egui::{self, RichText};
use omagari::format::*;

use super::actions::Action;
use super::binding::float_param_picker;
use super::helpers::ui_for_f32_ex;
use super::hints;
use super::problems::{subject_box, ACCENT};
use super::session::{EditorSession, ParamAnimation, PARAM_ANIMATION_SECONDS};
use super::AppContext;
use crate::mcp::analysis::{param_subject, trigger_subject};

const WARN: egui::Color32 = egui::Color32::from_rgb(0xFF, 0x88, 0x66);

/// A name field that commits on Enter / leaving the field: returns the new
/// name then. `key` tells the fields apart (`param:<name>`, `trigger:...`);
/// the text being typed lives in the session.
fn name_field(ui: &mut egui::Ui, current: &str, key: &str, session: &mut EditorSession, hint: &str) -> Option<String> {
    let mut text = match &session.name_draft {
        Some((k, t)) if k == key => t.clone(),
        _ => current.to_string(),
    };
    let response = ui
        .add(egui::TextEdit::singleline(&mut text).desired_width(150.0))
        .on_hover_text(hint);
    if response.changed() {
        session.name_draft = Some((key.to_string(), text.clone()));
    }
    if response.lost_focus() {
        if let Some((k, t)) = session.name_draft.take() {
            if k == key && t.trim() != current {
                return Some(t);
            }
        }
    }
    None
}

/// The preview value of a parameter: the saved preview value, else the
/// default.
pub fn preview_value(project: &ProjectV2, param: &ParamDef) -> ParamValue {
    project
        .preview
        .as_ref()
        .and_then(|p| p.params.iter().find(|(n, v)| *n == param.name && v.kind() == param.kind()))
        .map(|(_, v)| *v)
        .unwrap_or(param.default)
}

/// Stores a preview value (replacing the old one of that name).
fn set_preview_value(project: &mut ProjectV2, name: &str, value: ParamValue) {
    let preview = project.preview.get_or_insert_with(PreviewDef::default);
    match preview.params.iter_mut().find(|(n, _)| n == name) {
        Some(entry) => entry.1 = value,
        None => preview.params.push((name.to_string(), value)),
    }
}

/// A colour: picker (linear RGBA) plus the four numbers (above 1 = HDR glow).
pub fn color_editor(ui: &mut egui::Ui, color: &mut Vec4) -> bool {
    let before = *color;
    let mut rgba = egui::Rgba::from_rgba_unmultiplied(
        color.x.clamp(0.0, 1.0),
        color.y.clamp(0.0, 1.0),
        color.z.clamp(0.0, 1.0),
        color.w.clamp(0.0, 1.0),
    );
    if egui::color_picker::color_edit_button_rgba(ui, &mut rgba, egui::color_picker::Alpha::OnlyBlend).changed() {
        let [r, g, b, a] = rgba.to_rgba_unmultiplied();
        *color = Vec4::new(r, g, b, a);
    }
    for i in 0..4 {
        ui.add(egui::DragValue::new(&mut color[i]).speed(0.01).range(0.0..=100.0).max_decimals(3))
            .on_hover_text(["red", "green", "blue", "opacity"][i]);
    }
    *color != before
}

pub fn parameters_tab(ui: &mut egui::Ui, project: &mut ProjectV2, session: &mut EditorSession, app: &mut AppContext) {
    ui.label(hints::PARAMETERS);
    ui.label(
        RichText::new("The game sets them with vfx.set_param(instance, \"name\", value). The sliders drive the preview the same way.")
            .weak(),
    );
    ui.add_space(4.0);
    if ui.button("➕ ADD PARAMETER").on_hover_text("A new number parameter, 0..1").clicked() {
        let name = project.unique_param_name("param");
        project.parameters.push(ParamDef {
            name,
            default: ParamValue::Float(0.0),
            min: 0.0,
            max: 1.0,
            smoothing: 0.2,
        });
    }
    ui.separator();
    if project.parameters.is_empty() {
        ui.label(RichText::new("This project declares no parameters.").weak());
        return;
    }

    let mut remove = None;
    for index in 0..project.parameters.len() {
        let param = project.parameters[index].clone();
        let id = ui.make_persistent_id(("param", index));
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new("📈").color(ACCENT));
                if let Some(to) = name_field(ui, &param.name, &format!("param:{}", param.name), session, hints::PARAM_NAME) {
                    match project.rename_param(&param.name, &to) {
                        Ok(()) => {
                            app.status = Some(format!("Renamed parameter \"{}\" to \"{}\" and its uses", param.name, to.trim()));
                            if let Some(a) = session.param_animation.as_mut().filter(|a| a.name == param.name) {
                                a.name = to.trim().to_string();
                            }
                        }
                        Err(e) => app.status = Some(format!("Cannot rename \"{}\": {}", param.name, e)),
                    }
                }
                egui::ComboBox::from_id_salt(id.with("kind"))
                    .selected_text(match param.kind() {
                        ParamKind::Float => "number",
                        ParamKind::Color => "colour",
                    })
                    .show_ui(ui, |ui| {
                        let p = &mut project.parameters[index];
                        if ui.selectable_label(p.kind() == ParamKind::Float, "number").clicked() && p.kind() != ParamKind::Float {
                            p.default = ParamValue::Float(p.min.max(0.0).min(p.max));
                        }
                        if ui.selectable_label(p.kind() == ParamKind::Color, "colour").clicked() && p.kind() != ParamKind::Color {
                            p.default = ParamValue::Color(Vec4::ONE);
                        }
                    })
                    .response
                    .on_hover_text(hints::PARAM_KIND);
                if ui.small_button("❌").on_hover_text("Remove the parameter (its uses stay and are reported; Ctrl+Z brings it back)").clicked() {
                    remove = Some(index);
                }
            });
            let name = project.parameters[index].name.clone();

            egui::Grid::new(id.with("grid")).num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
                let p = &mut project.parameters[index];
                ui.label("Default").on_hover_text(hints::PARAM_DEFAULT);
                ui.horizontal(|ui| match &mut p.default {
                    ParamValue::Float(v) => {
                        *v = ui_for_f32_ex(ui, *v, -1.0e6, 1.0e6, 0.01);
                    }
                    ParamValue::Color(c) => {
                        color_editor(ui, c);
                    }
                });
                ui.end_row();
                if p.kind() == ParamKind::Float {
                    ui.label("Range").on_hover_text(hints::PARAM_RANGE);
                    ui.horizontal(|ui| {
                        p.min = ui_for_f32_ex(ui, p.min, -1.0e6, 1.0e6, 0.01);
                        ui.label("to");
                        p.max = ui_for_f32_ex(ui, p.max, -1.0e6, 1.0e6, 0.01);
                    });
                    ui.end_row();
                }
                ui.label("Smoothing").on_hover_text(hints::PARAM_SMOOTHING);
                ui.horizontal(|ui| {
                    p.smoothing = ui_for_f32_ex(ui, p.smoothing, 0.0, 60.0, 0.01);
                    ui.label("s");
                });
                ui.end_row();

                // Live control of the preview.
                let p = p.clone();
                ui.label(RichText::new("Preview").color(ACCENT)).on_hover_text(hints::PARAM_LIVE);
                ui.horizontal(|ui| {
                    let mut value = preview_value(project, &p);
                    let changed = match &mut value {
                        ParamValue::Float(v) => {
                            let (lo, hi) = (p.min.min(p.max), p.max.max(p.min));
                            ui.add(egui::Slider::new(v, lo..=hi).clamping(egui::SliderClamping::Always))
                                .on_hover_text(hints::PARAM_LIVE)
                                .changed()
                        }
                        ParamValue::Color(c) => color_editor(ui, c),
                    };
                    if changed {
                        set_preview_value(project, &name, value);
                    }
                    if p.kind() == ParamKind::Float {
                        let animating = session.param_animation.as_ref().is_some_and(|a| a.name == name);
                        if animating {
                            let elapsed = session.param_animation.as_ref().map(|a| a.elapsed).unwrap_or(0.0);
                            if ui
                                .button(format!("⏹ {:.1}s", PARAM_ANIMATION_SECONDS - elapsed))
                                .on_hover_text("Stop the animation")
                                .clicked()
                            {
                                session.param_animation = None;
                            }
                        } else if ui.button("↔ animate").on_hover_text(hints::PARAM_ANIMATE).clicked() {
                            session.param_animation = Some(ParamAnimation { name: name.clone(), elapsed: 0.0 });
                        }
                    }
                });
                ui.end_row();
            });

            let users = project.param_users(&name);
            ui.label(
                RichText::new(if users.is_empty() {
                    "Not used yet: read it in an expression (Param), a layer's rate or visible range, a While layer or a pulse.".to_string()
                } else {
                    format!("Used by {}", users.join(", "))
                })
                .weak(),
            );
            subject_box(ui, session, &param_subject(&name));
        });
    }
    if let Some(index) = remove {
        let removed = project.parameters.remove(index);
        if let Some(preview) = &mut project.preview {
            preview.params.retain(|(n, _)| *n != removed.name);
        }
        app.status = Some(format!("Removed parameter \"{}\" (Ctrl+Z brings it back)", removed.name));
    }
}

pub fn triggers_tab(ui: &mut egui::Ui, project: &mut ProjectV2, session: &mut EditorSession, app: &mut AppContext) {
    ui.label(hints::TRIGGERS);
    ui.label(
        RichText::new("The game fires them with vfx.trigger(instance, \"name\") or vfx.trigger_at(instance, \"name\", TriggerArgs::at(point)).")
            .weak(),
    );
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        if ui.button("➕ ADD TRIGGER").on_hover_text("A new trigger").clicked() {
            let name = project.unique_trigger_name("trigger");
            project.triggers.push(TriggerDef { name, pulses: Vec::new() });
        }
        ui.checkbox(&mut session.auto_fire_random, "auto-fire at random points")
            .on_hover_text("Auto-fire passes a random point near the target as the trigger position (editor setting, not saved)");
    });
    ui.separator();
    if project.triggers.is_empty() {
        ui.label(RichText::new("This project declares no triggers.").weak());
        return;
    }
    let params: Vec<(String, ParamKind)> = project.parameters.iter().map(|p| (p.name.clone(), p.kind())).collect();

    let mut remove = None;
    for index in 0..project.triggers.len() {
        let name = project.triggers[index].name.clone();
        let id = ui.make_persistent_id(("trigger", index));
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("⚡").color(ACCENT));
                if let Some(to) = name_field(ui, &name, &format!("trigger:{}", name), session, hints::TRIGGER_NAME) {
                    match project.rename_trigger(&name, &to) {
                        Ok(()) => app.status = Some(format!("Renamed trigger \"{}\" to \"{}\" and its uses", name, to.trim())),
                        Err(e) => app.status = Some(format!("Cannot rename \"{}\": {}", name, e)),
                    }
                }
                if ui.button("⚡ FIRE").on_hover_text(hints::FIRE).clicked() {
                    session.actions.push(Action::FireTrigger { name: name.clone(), random_point: false });
                }
                if ui.button("🎯 at random point").on_hover_text(hints::FIRE_RANDOM).clicked() {
                    session.actions.push(Action::FireTrigger { name: name.clone(), random_point: true });
                }
                if ui.small_button("❌").on_hover_text("Remove the trigger (layers playing on it are reported; Ctrl+Z brings it back)").clicked() {
                    remove = Some(index);
                }
            });

            // Auto-fire (preview block).
            ui.horizontal(|ui| {
                let current = project
                    .preview
                    .as_ref()
                    .and_then(|p| p.auto_triggers.iter().find(|(n, _)| *n == name).map(|(_, every)| *every));
                let mut on = current.is_some();
                let mut every = current.unwrap_or(1.0);
                let toggled = ui.checkbox(&mut on, "auto-fire every").on_hover_text(hints::AUTO_FIRE).changed();
                let edited = ui
                    .add_enabled(on, egui::DragValue::new(&mut every).speed(0.05).range(0.05..=600.0).suffix(" s"))
                    .on_hover_text(hints::AUTO_FIRE)
                    .changed();
                if toggled || edited {
                    let preview = project.preview.get_or_insert_with(PreviewDef::default);
                    preview.auto_triggers.retain(|(n, _)| *n != name);
                    if on {
                        preview.auto_triggers.push((name.clone(), every));
                    }
                }
            });

            // Fire at a clip time of the preview animation (preview block).
            ui.horizontal(|ui| {
                let current = project
                    .preview
                    .as_ref()
                    .and_then(|p| p.animation_triggers.iter().find(|(n, _)| *n == name).map(|(_, at)| *at));
                let mut on = current.is_some();
                let mut at = current.unwrap_or(0.3);
                let toggled = ui.checkbox(&mut on, "fire at clip time").on_hover_text(hints::ANIMATION_TRIGGER).changed();
                let edited = ui
                    .add_enabled(on, egui::DragValue::new(&mut at).speed(0.01).range(0.0..=600.0).suffix(" s"))
                    .on_hover_text(hints::ANIMATION_TRIGGER)
                    .changed();
                if toggled || edited {
                    let preview = project.preview.get_or_insert_with(PreviewDef::default);
                    preview.animation_triggers.retain(|(n, _)| *n != name);
                    if on {
                        preview.animation_triggers.push((name.clone(), at));
                    }
                }
            });

            // Pulses.
            ui.label(RichText::new("Pulses").strong()).on_hover_text(hints::PULSES);
            let mut remove_pulse = None;
            if project.triggers[index].pulses.is_empty() {
                ui.label(RichText::new("none").weak());
            } else {
                egui::Grid::new(id.with("pulses")).num_columns(4).spacing([6.0, 4.0]).show(ui, |ui| {
                for (i, pulse) in project.triggers[index].pulses.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        float_param_picker(ui, &mut pulse.param, &params, id.with(("pulse", i)));
                    });
                    ui.horizontal(|ui| {
                        ui.label("to").on_hover_text("The value the parameter jumps to");
                        pulse.to = ui_for_f32_ex(ui, pulse.to, -1.0e6, 1.0e6, 0.01);
                    });
                    ui.horizontal(|ui| {
                        ui.label("decay").on_hover_text("Seconds to go back to the parameter's value");
                        pulse.decay = ui_for_f32_ex(ui, pulse.decay, 0.0, 60.0, 0.01);
                        ui.label("s");
                    });
                    if ui.small_button("❌").on_hover_text("Remove this pulse").clicked() {
                        remove_pulse = Some(i);
                    }
                    ui.end_row();
                }
                });
            }
            if let Some(i) = remove_pulse {
                project.triggers[index].pulses.remove(i);
            }
            if ui.small_button("+ pulse").on_hover_text(hints::PULSES).clicked() {
                let param = params.iter().find(|(_, k)| *k == ParamKind::Float).map(|(n, _)| n.clone());
                let (param, to) = match param.as_deref().and_then(|n| project.param(n)) {
                    Some(p) => (p.name.clone(), p.max),
                    None => (String::new(), 1.0),
                };
                project.triggers[index].pulses.push(PulseDef { param, to, decay: 0.3 });
            }
            if params.iter().all(|(_, k)| *k != ParamKind::Float) && !project.triggers[index].pulses.is_empty() {
                ui.colored_label(WARN, "Pulses need a number parameter (PARAMETERS tab).");
            }

            let layers = project.trigger_users(&name);
            ui.label(
                RichText::new(if layers.is_empty() {
                    "No layer plays on it (set a layer's Play to OnTrigger).".to_string()
                } else {
                    format!("Bursts {}", layers.iter().map(|l| format!("\"{}\"", l)).collect::<Vec<_>>().join(", "))
                })
                .weak(),
            );
            subject_box(ui, session, &trigger_subject(&name));
        });
    }
    if let Some(index) = remove {
        let removed = project.triggers.remove(index);
        if let Some(preview) = &mut project.preview {
            preview.auto_triggers.retain(|(n, _)| *n != removed.name);
            preview.animation_triggers.retain(|(n, _)| *n != removed.name);
        }
        app.status = Some(format!("Removed trigger \"{}\" (Ctrl+Z brings it back)", removed.name));
    }
}
