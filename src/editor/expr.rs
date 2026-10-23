//! Editor widgets for the expression model in `omagari::format`.

use bevy::prelude::*;
use bevy_egui::*;
use bevy_hanabi::prelude::*;
use omagari::format::*;

use super::helpers::*;
use super::AppContext;

fn warn_color() -> egui::Color32 {
    egui::Color32::from_hex("#FF8866").unwrap()
}

/// A picker of value types; `floats_only` limits it to f32 / vec2 / vec3 /
/// vec4 (`Rand`).
fn value_type_picker(ui: &mut egui::Ui, id: egui::Id, value: &mut ValueTypeDef, floats_only: bool) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(value.name())
        .show_ui(ui, |ui| {
            for t in ValueTypeDef::ALL.into_iter().filter(|t| !floats_only || t.is_float()) {
                ui.selectable_value(value, t, t.name());
            }
        });
}

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
            ExprOperatorEditor::Cast(arg, target) => {
                unique_collapsing(unique_id, &label, ui)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("to").weak());
                            let id = ui.make_persistent_id(format!("cast type {}", index));
                            value_type_picker(ui, id, target, false);
                        });
                        arg.draw_ui(app, ui, 1);
                    })
                    .header_response
                    .on_hover_text(info.hint);
            }
            _ => {
                let names = info.args;
                unique_collapsing(unique_id, &label, ui)
                    .show(ui, |ui| {
                        for (i, arg) in self.args_mut().into_iter().enumerate() {
                            // Named arguments where the order matters.
                            let name = names.get(i).copied().unwrap_or_default();
                            if names.len() >= 2 && !matches!(info.name, "Vec2" | "Vec3" | "Vec4")
                                && !matches!(name, "a" | "b")
                            {
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

/// The Operator menu: every row of [`OPERATORS`] in its group, with its hint.
pub fn operator_menu_ui(ui: &mut egui::Ui) -> Option<Box<ExprOperatorEditor>> {
    let mut picked = None;
    for group in OPERATOR_GROUPS {
        ui.menu_button(group, |ui| {
            for info in OPERATORS.iter().filter(|o| o.group == group) {
                let text = if info.args.is_empty() {
                    format!("🖩 {}", info.name)
                } else {
                    format!("🖩 {}({})", info.name, info.args.join(", "))
                };
                if ui.button(text).on_hover_text(info.hint).clicked() {
                    picked = Some(Box::new((info.make)()));
                }
            }
        });
    }
    picked
}

/// The value menu: every row of [`EXPR_VALUES`] in its group.
fn value_menu_ui(ui: &mut egui::Ui) -> Option<ExprWriterEditor> {
    let mut picked = None;
    for group in ["Number", "Vector", "Random", "Time", "Special"] {
        ui.menu_button(group, |ui| {
            for info in EXPR_VALUES.iter().filter(|v| v.group == group && !NOT_OFFERED_VALUES.contains(&v.name)) {
                if ui
                    .button(format!("{} ({})", info.name, info.ty))
                    .on_hover_text(info.hint)
                    .clicked()
                {
                    picked = Some((info.make)());
                }
            }
        });
    }
    picked
}

/// The editable part of a literal or a random value (nothing for built-in
/// values like Time).
fn leaf_value_ui(e: &mut ExprWriterEditor, ui: &mut egui::Ui, index: u64) {
    use ExprWriterEditor as E;
    let int = |ui: &mut egui::Ui, v: &mut i32| {
        ui.add(egui::DragValue::new(v).speed(0.1).range(-100000..=100000));
    };
    let uint = |ui: &mut egui::Ui, v: &mut u32| {
        ui.add(egui::DragValue::new(v).speed(0.1).range(0..=100000));
    };
    match e {
        E::Float(f) => *f = ui_for_f32(ui, *f),
        E::U32(v) => *v = ui_for_u32_ex(ui, *v, 0, 10000, 1),
        E::I32(v) => int(ui, v),
        E::Bool(v) => {
            ui.checkbox(v, "");
        }
        E::Vec2(v) => {
            for k in 0..2 {
                v[k] = ui_for_f32(ui, v[k]);
            }
        }
        E::Vec3(v) => *v = ui_for_vec3(ui, *v),
        E::Vec4(v) => *v = ui_for_vec4(ui, *v),
        E::IVec2(v) => (0..2).for_each(|k| int(ui, &mut v[k])),
        E::IVec3(v) => (0..3).for_each(|k| int(ui, &mut v[k])),
        E::IVec4(v) => (0..4).for_each(|k| int(ui, &mut v[k])),
        E::UVec2(v) => (0..2).for_each(|k| uint(ui, &mut v[k])),
        E::UVec3(v) => (0..3).for_each(|k| uint(ui, &mut v[k])),
        E::UVec4(v) => (0..4).for_each(|k| uint(ui, &mut v[k])),
        E::BVec2(v) => {
            let (mut x, mut y) = (v.x, v.y);
            ui.checkbox(&mut x, "");
            ui.checkbox(&mut y, "");
            *v = BVec2::new(x, y);
        }
        E::BVec3(v) => {
            let (mut x, mut y, mut z) = (v.x, v.y, v.z);
            ui.checkbox(&mut x, "");
            ui.checkbox(&mut y, "");
            ui.checkbox(&mut z, "");
            *v = BVec3::new(x, y, z);
        }
        E::BVec4(v) => {
            let (mut x, mut y, mut z, mut w) = (v.x, v.y, v.z, v.w);
            ui.checkbox(&mut x, "");
            ui.checkbox(&mut y, "");
            ui.checkbox(&mut z, "");
            ui.checkbox(&mut w, "");
            *v = BVec4::new(x, y, z, w);
        }
        E::Rand(t) => {
            let id = ui.make_persistent_id(format!("rand type {}", index));
            value_type_picker(ui, id, t, true);
            if !t.is_float() {
                ui.colored_label(warn_color(), "⚠ float types only")
                    .on_hover_text("bevy_hanabi 0.19's shaders have no integer or bool random function");
            }
        }
        E::RandU32 => {
            ui.colored_label(warn_color(), "⚠ fails on the GPU").on_hover_text(
                "bevy_hanabi 0.19's shaders have no integer random function; use Cast(RandFloat × n, U32)",
            );
        }
        E::IsAlive => {
            ui.label(egui::RichText::new("(update only)").weak());
        }
        E::AlphaCutoff => {
            ui.colored_label(warn_color(), "⚠ fails on the GPU")
                .on_hover_text("bevy_hanabi 0.19 cannot compile AlphaCutoff; use the Mask cutoff number");
        }
        _ => {}
    }
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
                                ui.colored_label(warn_color(), "⚠ unknown")
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
                    ui.menu_button("Value", |ui| {
                        if let Some(value) = value_menu_ui(ui) {
                            *self = value;
                        }
                    });
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
                        if ui
                            .button("Random Point On Segment With Radius")
                            .on_hover_text(super::hints::SEGMENT_RADIUS)
                            .clicked()
                        {
                            *self = segment_point_with_radius(0.05).attr_expr;
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
                        let frames = app.layer_texture_grid.map(|(r, c)| r * c).unwrap_or(16);
                        if ui
                            .button("Sprite Index Over Lifetime")
                            .on_hover_text(format!(
                                "i32(age / lifetime × {}): the flipbook frame from first to last over the particle's life (for sprite_index, in an update SetAttribute)",
                                frames
                            ))
                            .clicked()
                        {
                            *self = sprite_index_over_lifetime(frames).attr_expr;
                        }
                        if ui
                            .button("Spin Over Age")
                            .on_hover_text("age × 3: an angle that grows over the particle's life (Orient rotation, radians)")
                            .clicked()
                        {
                            *self = ExprWriterEditor::Operator(Box::new(ExprOperatorEditor::Multiply(
                                ExprWriterEditor::Age,
                                ExprWriterEditor::Float(3.0),
                            )));
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

            _ => {
                let label = c.leaf_name().unwrap_or_default();
                let control = ui
                    .horizontal(|ui| {
                        let control = ui_tools_for_expr_writer(label, ui);
                        leaf_value_ui(self, ui, index);
                        control
                    })
                    .inner;
                match control {
                    ExprControl::Delete => *self = ExprWriterEditor::Placeholder,
                    ExprControl::Copy => app.expr_clipboard = Some(c),
                    ExprControl::Noop => {}
                }
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
