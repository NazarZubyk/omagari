//! Editor widgets for the expression model in `omagari::format`.

use bevy::prelude::*;
use bevy_egui::*;
use bevy_hanabi::prelude::*;
use omagari::format::*;

use super::helpers::*;
use super::AppContext;

impl UiProvider for ExprOperatorEditor {
    fn draw_ui(&mut self, app: &mut AppContext, ui: &mut egui::Ui, index: u64) {
        let unique_id = index;
        let info = self.info();
        let label = format!("🖩 {}", info.name);
        match self {
            ExprOperatorEditor::Attr(attr) | ExprOperatorEditor::ParentAttr(attr) => {
                let mut selected_attr: Attribute = attr.clone();
                unique_collapsing(unique_id, &label, ui)
                    .show(ui, |ui| {
                        let selected_text = attr_to_label(selected_attr);

                        let id = ui.make_persistent_id(format!("header"));
                        ui.vertical(|ui| {
                            egui::collapsing_header::CollapsingState::load_with_default_open(
                                ui.ctx(),
                                id,
                                false,
                            )
                            .show_header(ui, |ui| {
                                egui::ComboBox::new(id, "is inherited")
                                    .selected_text(selected_text)
                                    .show_ui(ui, |ui| {
                                        ALL_ATTRS.iter().for_each(|(attr, label)| {
                                            ui.selectable_value(&mut selected_attr, *attr, *label);
                                        });
                                    });
                            })
                            .body(|_| {});
                        });
                    })
                    .header_response
                    .on_hover_text(info.hint);
                *self = if let ExprOperatorEditor::Attr(_) = self {
                    ExprOperatorEditor::Attr(selected_attr)
                } else {
                    ExprOperatorEditor::ParentAttr(selected_attr)
                };
            }
            _ => {
                let names = info.args;
                unique_collapsing(unique_id, &label, ui)
                    .show(ui, |ui| {
                        for (i, arg) in self.args_mut().into_iter().enumerate() {
                            // Named arguments where the order matters.
                            let name = names.get(i).copied().unwrap_or_default();
                            if names.len() >= 3 && !matches!(info.name, "Vec3" | "Vec4") {
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new(name).weak());
                                    arg.draw_ui(app, ui, i as u64 + 1);
                                });
                            } else {
                                arg.draw_ui(app, ui, i as u64 + 1);
                            }
                        }
                    })
                    .header_response
                    .on_hover_text(info.hint);
            }
        }
    }
}

/// The Operator menu: every row of [`OPERATORS`], with its hint.
pub fn operator_menu_ui(ui: &mut egui::Ui) -> Option<Box<ExprOperatorEditor>> {
    for info in OPERATORS.iter() {
        let text = if info.args.is_empty() {
            format!("🖩 {}", info.name)
        } else {
            format!("🖩 {}({})", info.name, info.args.join(", "))
        };
        if ui.button(text).on_hover_text(info.hint).clicked() {
            return Some(Box::new((info.make)()));
        }
    }
    None
}

impl UiProvider for ExprWriterEditor {
    fn draw_ui(&mut self, app: &mut AppContext, ui: &mut egui::Ui, index: u64) {
        let c = self.clone();
        match self {
            ExprWriterEditor::Operator(o) => {
                if let Some(v) = ui
                    .horizontal(|ui| match ui_tools_for_expr_writer("", ui) {
                        ExprControl::Delete => Some(ExprWriterEditor::Placeholder),
                        ExprControl::Noop => {
                            o.draw_ui(app, ui, index);
                            None
                        }
                        ExprControl::Copy => {
                            app.expr_clipboard = Some(c);
                            None
                        }
                    })
                    .inner
                {
                    *self = v;
                }
            }
            ExprWriterEditor::Age => {
                if let Some(v) = ui
                    .horizontal(|ui| match ui_tools_for_expr_writer("Age", ui) {
                        ExprControl::Delete => Some(ExprWriterEditor::Placeholder),
                        ExprControl::Noop => None,
                        ExprControl::Copy => {
                            app.expr_clipboard = Some(c);
                            None
                        }
                    })
                    .inner
                {
                    *self = v;
                }
            }
            ExprWriterEditor::RandU32 => {
                if let Some(v) = ui
                    .horizontal(|ui| match ui_tools_for_expr_writer("Rand U32", ui) {
                        ExprControl::Delete => Some(ExprWriterEditor::Placeholder),
                        ExprControl::Noop => None,
                        ExprControl::Copy => {
                            app.expr_clipboard = Some(c);
                            None
                        }
                    })
                    .inner
                {
                    *self = v;
                }
            }
            ExprWriterEditor::RandFloat => {
                if let Some(v) = ui
                    .horizontal(|ui| match ui_tools_for_expr_writer("Rand Float", ui) {
                        ExprControl::Delete => Some(ExprWriterEditor::Placeholder),
                        ExprControl::Noop => None,
                        ExprControl::Copy => {
                            app.expr_clipboard = Some(c);
                            None
                        }
                    })
                    .inner
                {
                    *self = v;
                }
            }
            ExprWriterEditor::RandVec3 => {
                if let Some(v) = ui
                    .horizontal(|ui| match ui_tools_for_expr_writer("Rand Vec3", ui) {
                        ExprControl::Delete => Some(ExprWriterEditor::Placeholder),
                        ExprControl::Noop => None,
                        ExprControl::Copy => {
                            app.expr_clipboard = Some(c);
                            None
                        }
                    })
                    .inner
                {
                    *self = v;
                }
            }
            ExprWriterEditor::Float(f) => {
                *self = ui
                    .horizontal(|ui| match ui_tools_for_expr_writer("Float", ui) {
                        ExprControl::Delete => ExprWriterEditor::Placeholder,
                        ExprControl::Noop => ExprWriterEditor::Float(ui_for_f32(ui, *f)),
                        ExprControl::Copy => {
                            app.expr_clipboard = Some(c);
                            ExprWriterEditor::Float(ui_for_f32(ui, *f))
                        }
                    })
                    .inner;
            }
            ExprWriterEditor::U32(f) => {
                *self = ui
                    .horizontal(|ui| match ui_tools_for_expr_writer("U32", ui) {
                        ExprControl::Delete => ExprWriterEditor::Placeholder,
                        ExprControl::Noop => {
                            ExprWriterEditor::U32(ui_for_u32_ex(ui, *f, 0, 10000, 1))
                        }
                        ExprControl::Copy => {
                            app.expr_clipboard = Some(c);
                            ExprWriterEditor::U32(ui_for_u32_ex(ui, *f, 0, 10000, 1))
                        }
                    })
                    .inner;
            }
            ExprWriterEditor::Vec3(result) => {
                *self = ui
                    .horizontal(|ui| match ui_tools_for_expr_writer("Vec3", ui) {
                        ExprControl::Delete => ExprWriterEditor::Placeholder,
                        ExprControl::Noop => ExprWriterEditor::Vec3(ui_for_vec3(ui, *result)),
                        ExprControl::Copy => {
                            app.expr_clipboard = Some(c);
                            ExprWriterEditor::Vec3(ui_for_vec3(ui, *result))
                        }
                    })
                    .inner;
            }
            ExprWriterEditor::Vec4(result) => {
                *self = ui
                    .horizontal(|ui| match ui_tools_for_expr_writer("Vec4", ui) {
                        ExprControl::Delete => ExprWriterEditor::Placeholder,
                        ExprControl::Noop => ExprWriterEditor::Vec4(ui_for_vec4(ui, *result)),
                        ExprControl::Copy => {
                            app.expr_clipboard = Some(c);
                            ExprWriterEditor::Vec4(ui_for_vec4(ui, *result))
                        }
                    })
                    .inner;
            }
            ExprWriterEditor::Time => {
                *self = ui
                    .horizontal(|ui| match ui_tools_for_expr_writer("Time", ui) {
                        ExprControl::Delete => ExprWriterEditor::Placeholder,
                        ExprControl::Noop => ExprWriterEditor::Time,
                        ExprControl::Copy => {
                            app.expr_clipboard = Some(c);
                            ExprWriterEditor::Time
                        }
                    })
                    .inner;
            }

            ExprWriterEditor::Property(name) => {
                let mut name = name.clone();
                let replace = ui
                    .horizontal(|ui| {
                        let control = ui_tools_for_expr_writer("Property", ui);
                        let id = ui.make_persistent_id(format!("property {}", index));
                        egui::ComboBox::from_id_salt(id).selected_text(name.clone()).show_ui(ui, |ui| {
                            for (property, _, hint) in RUNTIME_PROPERTIES {
                                ui.selectable_value(&mut name, property.to_string(), property)
                                    .on_hover_text(hint);
                            }
                        });
                        match control {
                            ExprControl::Delete => Some(ExprWriterEditor::Placeholder),
                            ExprControl::Noop => None,
                            ExprControl::Copy => {
                                app.expr_clipboard = Some(c);
                                None
                            }
                        }
                    })
                    .inner;
                *self = replace.unwrap_or(ExprWriterEditor::Property(name));
            }

            ExprWriterEditor::Param(name) => {
                let mut name = name.clone();
                let replace = ui
                    .horizontal(|ui| {
                        let control = ui_tools_for_expr_writer("Param", ui);
                        let id = ui.make_persistent_id(format!("param {}", index));
                        let known = app.params.iter().find(|(n, _)| *n == name).map(|(_, k)| *k);
                        egui::ComboBox::from_id_salt(id)
                            .selected_text(name.clone())
                            .show_ui(ui, |ui| {
                                for (param, kind) in app.params.iter() {
                                    ui.selectable_value(&mut name, param.clone(), param)
                                        .on_hover_text(param_kind_hint(*kind));
                                }
                            })
                            .response
                            .on_hover_text(super::hints::PARAM_EXPR);
                        match known {
                            Some(kind) => {
                                ui.label(egui::RichText::new(param_kind_label(kind)).weak());
                            }
                            None => {
                                ui.colored_label(egui::Color32::from_hex("#FF8866").unwrap(), "⚠ unknown")
                                    .on_hover_text("No parameter has this name (PARAMETERS tab); it reads 0.0");
                            }
                        }
                        match control {
                            ExprControl::Delete => Some(ExprWriterEditor::Placeholder),
                            ExprControl::Noop => None,
                            ExprControl::Copy => {
                                app.expr_clipboard = Some(c);
                                None
                            }
                        }
                    })
                    .inner;
                *self = replace.unwrap_or(ExprWriterEditor::Param(name));
            }

            ExprWriterEditor::Placeholder => {
                ui.menu_button("+", |ui| {
                    ui.menu_button("Operator", |ui| {
                        if let Some(op) = operator_menu_ui(ui) {
                            *self = ExprWriterEditor::Operator(op);
                        }
                    });
                    if ui.button("Age").clicked() {
                        *self = ExprWriterEditor::Age;
                    }
                    if ui.button("RandFloat").clicked() {
                        *self = ExprWriterEditor::RandFloat;
                    }
                    if ui.button("RandU32").clicked() {
                        *self = ExprWriterEditor::RandU32;
                    }
                    if ui.button("RandVec3").clicked() {
                        *self = ExprWriterEditor::RandVec3;
                    }
                    if ui.button("Time").clicked() {
                        *self = ExprWriterEditor::Time;
                    }
                    if ui.button("Float").clicked() {
                        *self = ExprWriterEditor::Float(0.0);
                    }
                    if ui.button("U32").clicked() {
                        *self = ExprWriterEditor::U32(0);
                    }
                    if ui.button("Vec3").clicked() {
                        *self = ExprWriterEditor::Vec3(Vec3::ZERO);
                    }
                    if ui.button("Vec4").clicked() {
                        *self = ExprWriterEditor::Vec4(Vec4::ZERO);
                    }
                    ui.menu_button("Parameter", |ui| {
                        if app.params.is_empty() {
                            ui.label(egui::RichText::new("No parameters yet: add them in the PARAMETERS tab").weak());
                        }
                        for (param, kind) in app.params.iter() {
                            if ui
                                .button(format!("{} ({})", param, param_kind_label(*kind)))
                                .on_hover_text(param_kind_hint(*kind))
                                .clicked()
                            {
                                *self = ExprWriterEditor::Param(param.clone());
                            }
                        }
                    })
                    .response
                    .on_hover_text(super::hints::PARAM_EXPR);
                    ui.menu_button("Property", |ui| {
                        for (property, _, hint) in RUNTIME_PROPERTIES {
                            if ui.button(property).on_hover_text(hint).clicked() {
                                *self = ExprWriterEditor::Property(property.to_string());
                            }
                        }
                    });
                    ui.separator();
                    ui.menu_button("Prebuilt", |ui| {
                        if ui
                            .button("Random Point On Segment")
                            .on_hover_text("segment_a + (segment_b - segment_a) × random: for Segment-attached layers")
                            .clicked()
                        {
                            *self = segment_point_expr();
                        }
                        if let Some((param, _)) = app.params.iter().find(|(_, k)| *k == ParamKind::Float) {
                            if ui
                                .button("Mix By Parameter")
                                .on_hover_text(format!(
                                    "Mix(a, b, {}): from a at 0 to b at 1 of a Float parameter; fill in a and b",
                                    param
                                ))
                                .clicked()
                            {
                                *self = ExprWriterEditor::Operator(Box::new(ExprOperatorEditor::Mix(
                                    ExprWriterEditor::Placeholder,
                                    ExprWriterEditor::Placeholder,
                                    ExprWriterEditor::Param(param.clone()),
                                )));
                            }
                        }
                        if ui.button("Random Normalized Vector").clicked() {
                            *self = ExprWriterEditor::Operator(Box::new(
                                ExprOperatorEditor::Normalized(ExprWriterEditor::Operator(
                                    Box::new(ExprOperatorEditor::Subtract(
                                        ExprWriterEditor::Operator(Box::new(
                                            ExprOperatorEditor::Multiply(
                                                ExprWriterEditor::RandVec3,
                                                ExprWriterEditor::Float(2.0),
                                            ),
                                        )),
                                        ExprWriterEditor::Float(1.0),
                                    )),
                                )),
                            ))
                        }
                    });
                    if app.expr_clipboard.is_some() {
                        ui.separator();
                        if ui.button("From clipboard").clicked() {
                            *self = app.expr_clipboard.clone().unwrap().clone();
                        }
                    }
                });
            }
        }
    }
}

fn param_kind_label(kind: ParamKind) -> &'static str {
    match kind {
        ParamKind::Float => "number",
        ParamKind::Color => "colour",
    }
}

fn param_kind_hint(kind: ParamKind) -> &'static str {
    match kind {
        ParamKind::Float => "A number (f32): use it where a number goes, or as the t of Mix",
        ParamKind::Color => "A colour (vec4: red, green, blue, opacity): use it for hdr_color, or Mix two colours",
    }
}

/// `segment_a + (segment_b - segment_a) * RandFloat`: a random point on the
/// layer's segment.
fn segment_point_expr() -> ExprWriterEditor {
    let a = || ExprWriterEditor::Property("segment_a".into());
    let b = ExprWriterEditor::Property("segment_b".into());
    ExprWriterEditor::Operator(Box::new(ExprOperatorEditor::Add(
        a(),
        ExprWriterEditor::Operator(Box::new(ExprOperatorEditor::Multiply(
            ExprWriterEditor::Operator(Box::new(ExprOperatorEditor::Subtract(b, a()))),
            ExprWriterEditor::RandFloat,
        ))),
    )))
}
