//! Editor widgets for values bound to parameters: the binding editor
//! (constant / curve over a parameter, with a key list and a small plot) and
//! the parameter picker. Used for a particle layer's `rate`; later layer
//! kinds use the same widget for their bindable values.

use bevy_egui::egui;
use omagari::format::{Binding, ParamKind};

use super::helpers::ui_for_f32_ex;
use super::hints;

const WARN: egui::Color32 = egui::Color32::from_rgb(0xFF, 0x88, 0x66);

/// A picker of the project's Float parameters. Returns whether the name
/// changed. Unknown names are shown with a warning.
pub fn float_param_picker(ui: &mut egui::Ui, name: &mut String, params: &[(String, ParamKind)], id: egui::Id) -> bool {
    let before = name.clone();
    egui::ComboBox::from_id_salt(id)
        .selected_text(if name.is_empty() { "(pick)".to_string() } else { name.clone() })
        .show_ui(ui, |ui| {
            let mut any = false;
            for (param, kind) in params {
                if *kind == ParamKind::Float {
                    any = true;
                    ui.selectable_value(name, param.clone(), param);
                }
            }
            if !any {
                ui.label(egui::RichText::new("No number parameters: add one in the PARAMETERS tab").weak());
            }
        });
    match params.iter().find(|(p, _)| p == name) {
        None if !name.is_empty() => {
            ui.colored_label(WARN, "⚠").on_hover_text("No parameter has this name (PARAMETERS tab)");
        }
        Some((_, ParamKind::Color)) => {
            ui.colored_label(WARN, "⚠").on_hover_text("This is a colour parameter; a number parameter is needed here");
        }
        _ => {}
    }
    *name != before
}

/// The first Float parameter's name, or empty.
fn first_float(params: &[(String, ParamKind)]) -> String {
    params
        .iter()
        .find(|(_, k)| *k == ParamKind::Float)
        .map(|(n, _)| n.clone())
        .unwrap_or_default()
}

/// Edits a `Binding<f32>`: "constant" or "curve over" a Float parameter, with
/// its keys (parameter value → value) and a small plot. `range` is the
/// parameter's (min, max) when known, for new keys and the plot; `unit`
/// follows values (e.g. "/s"); `max` caps values.
pub fn binding_editor(
    ui: &mut egui::Ui,
    binding: &mut Binding<f32>,
    params: &[(String, ParamKind)],
    param_range: impl Fn(&str) -> Option<(f32, f32)>,
    unit: &str,
    max: f32,
    id: egui::Id,
) {
    ui.horizontal(|ui| {
        let constant = matches!(binding, Binding::Const(_));
        if ui.radio(constant, "constant").on_hover_text(hints::BINDING).clicked() && !constant {
            let value = match binding {
                Binding::Param(_, keys) => keys.first().map(|k| k.1).unwrap_or(0.0),
                Binding::Const(v) => *v,
            };
            *binding = Binding::Const(value);
        }
        if ui.radio(!constant, "curve over parameter").on_hover_text(hints::BINDING).clicked() && constant {
            let value = match binding {
                Binding::Const(v) => *v,
                Binding::Param(..) => 0.0,
            };
            let name = first_float(params);
            let (lo, hi) = param_range(&name).unwrap_or((0.0, 1.0));
            *binding = Binding::Param(name, vec![(lo, value), (hi, value)]);
        }
    });
    match binding {
        Binding::Const(value) => {
            ui.horizontal(|ui| {
                *value = ui_for_f32_ex(ui, *value, 0.0, max, 0.1);
                ui.label(unit);
            });
        }
        Binding::Param(name, keys) => {
            ui.horizontal(|ui| {
                ui.label("parameter");
                float_param_picker(ui, name, params, id.with("param"));
            });
            let range = param_range(name);
            keys_editor(ui, keys, range, unit, max, id.with("keys"));
            curve_plot(ui, keys, range);
        }
    }
}

/// Rows of (parameter value → value), sorted by parameter value; add and
/// remove buttons.
fn keys_editor(ui: &mut egui::Ui, keys: &mut Vec<(f32, f32)>, range: Option<(f32, f32)>, unit: &str, max: f32, id: egui::Id) {
    let mut remove = None;
    egui::Grid::new(id).num_columns(4).spacing([6.0, 2.0]).show(ui, |ui| {
        ui.label(egui::RichText::new("at").weak());
        ui.label("");
        ui.label(egui::RichText::new("value").weak());
        ui.end_row();
        for (i, (x, v)) in keys.iter_mut().enumerate() {
            *x = ui_for_f32_ex(ui, *x, -1.0e6, 1.0e6, 0.01);
            ui.label("➡");
            ui.horizontal(|ui| {
                *v = ui_for_f32_ex(ui, *v, 0.0, max, 0.1);
                ui.label(unit);
            });
            if ui.small_button("❌").on_hover_text("Remove this key").clicked() {
                remove = Some(i);
            }
            ui.end_row();
        }
    });
    if let Some(i) = remove {
        keys.remove(i);
    }
    if ui.small_button("+ key").on_hover_text("Adds a key after the last one").clicked() {
        let (lo, hi) = range.unwrap_or((0.0, 1.0));
        let next = match keys.last() {
            Some((x, v)) => ((x + (hi - lo) * 0.25).min(hi.max(*x + 0.1)), *v),
            None => (lo, 0.0),
        };
        keys.push(next);
    }
    // Keys must stay sorted for evaluation; sort only when needed so a key
    // being dragged keeps its row.
    if keys.windows(2).any(|w| w[1].0 < w[0].0) && !ui.ctx().egui_is_using_pointer() {
        keys.sort_by(|a, b| a.0.total_cmp(&b.0));
    }
}

/// A small line plot of the curve over the parameter range.
fn curve_plot(ui: &mut egui::Ui, keys: &[(f32, f32)], range: Option<(f32, f32)>) {
    if keys.is_empty() {
        ui.colored_label(WARN, "⚠ no keys: the spawner's own count applies");
        return;
    }
    let binding = Binding::Param(String::new(), keys.to_vec());
    let (x0, x1) = range.unwrap_or((keys[0].0, keys[keys.len() - 1].0.max(keys[0].0 + 1.0)));
    let (x0, x1) = if x1 > x0 { (x0, x1) } else { (x0, x0 + 1.0) };
    let samples: Vec<(f32, f32)> = (0..=48)
        .map(|i| {
            let x = x0 + (x1 - x0) * i as f32 / 48.0;
            (x, binding.evaluate(|_| Some(x)).unwrap_or(0.0))
        })
        .collect();
    let top = samples.iter().map(|s| s.1).fold(0.0f32, f32::max).max(1e-3);
    let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width().min(260.0), 48.0), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    let visuals = ui.visuals();
    painter.rect_filled(rect, 2.0, visuals.extreme_bg_color);
    let to_screen = |(x, y): (f32, f32)| {
        egui::pos2(
            rect.left() + (x - x0) / (x1 - x0) * rect.width(),
            rect.bottom() - y / top * (rect.height() - 4.0) - 2.0,
        )
    };
    let points: Vec<egui::Pos2> = samples.iter().copied().map(to_screen).collect();
    painter.add(egui::Shape::line(points, egui::Stroke::new(1.5, visuals.selection.bg_fill)));
    for key in keys {
        if key.0 >= x0 && key.0 <= x1 {
            painter.circle_filled(to_screen(*key), 2.5, visuals.strong_text_color());
        }
    }
    response.on_hover_text(format!("parameter {} … {}; value 0 … {}", x0, x1, top));
}
