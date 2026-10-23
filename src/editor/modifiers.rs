//! Editor widgets for the particle modifiers in `omagari::format`.

use bevy::prelude::*;
use bevy_egui::*;
use bevy_hanabi::prelude::*;
use omagari::format::*;

use super::helpers::*;
use super::hints;
use super::AppContext;

/// Name of a modifier in the editor's menus and headers.
pub trait ModifierLabel {
    fn label() -> &'static str;
}

impl ModifierLabel for SetAttributeModifierEditor {
    fn label() -> &'static str {
        "🗠 SetAttributeModifier"
    }
}

impl UiProvider for SetAttributeModifierEditor {
    fn draw_ui(&mut self, app: &mut AppContext, ui: &mut egui::Ui, index: u64) {
        let selected_text = attr_to_label(self.attr);
        let id = ui.make_persistent_id(format!("{} {}", selected_text, index));
        ui.vertical(|ui| {
            egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false)
                .show_header(ui, |ui| {
                    egui::ComboBox::new(id, "modifier")
                        .selected_text(selected_text)
                        .show_ui(ui, |ui| {
                            ALL_ATTRS.iter().for_each(|(attr, label)| {
                                ui.selectable_value(&mut self.attr, *attr, *label)
                                    .on_hover_text(hints::attribute(label));
                            });
                        })
                        .response
                        .on_hover_text(hints::attribute(selected_text));
                })
                .body(|ui| {
                    self.attr_expr.draw_ui(app, ui, 1);
                });
        });
    }
}

impl ModifierLabel for InheritAttributeModifierEditor {
    fn label() -> &'static str {
        "⛓ InheritAttributeModifier"
    }
}

impl UiProvider for InheritAttributeModifierEditor {
    fn draw_ui(&mut self, _app: &mut AppContext, ui: &mut egui::Ui, index: u64) {
        let selected_text = attr_to_label(self.attr);
        let id = ui.make_persistent_id(format!("{} {}", selected_text, index));
        ui.vertical(|ui| {
            egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false)
                .show_header(ui, |ui| {
                    egui::ComboBox::new(id, "is inherited")
                        .selected_text(selected_text)
                        .show_ui(ui, |ui| {
                            ALL_ATTRS.iter().for_each(|(attr, label)| {
                                ui.selectable_value(&mut self.attr, *attr, *label);
                            });
                        });
                })
                .body(|_| {});
        });
    }
}

impl ModifierLabel for SetVelocityCircleModifierEditor {
    fn label() -> &'static str {
        "⏱ SetVelocityCircleModifier"
    }
}

impl UiProvider for SetVelocityCircleModifierEditor {
    fn draw_ui(&mut self, app: &mut AppContext, ui: &mut egui::Ui, index: u64) {
        unique_collapsing(index, Self::label(), ui).show(ui, |ui| {
            unique_collapsing(1, "Origin", ui).show(ui, |ui| {
                self.center_expr.draw_ui(app, ui, 1);
            });
            unique_collapsing(2, "Axis", ui).show(ui, |ui| {
                self.axis_expr.draw_ui(app, ui, 2);
            });
            unique_collapsing(3, "Speed (m/s)", ui).show(ui, |ui| {
                self.speed_expr.draw_ui(app, ui, 3);
            });
        });
    }
}

impl ModifierLabel for SetVelocitySphereModifierEditor {
    fn label() -> &'static str {
        "⏱ SetVelocitySphereModifier"
    }
}

impl UiProvider for SetVelocitySphereModifierEditor {
    fn draw_ui(&mut self, app: &mut AppContext, ui: &mut egui::Ui, index: u64) {
        unique_collapsing(index, Self::label(), ui).show(ui, |ui| {
            unique_collapsing(1, "Origin", ui).show(ui, |ui| {
                self.center_expr.draw_ui(app, ui, 1);
            });
            unique_collapsing(3, "Speed (m/s)", ui).show(ui, |ui| {
                self.speed_expr.draw_ui(app, ui, 3);
            });
        });
    }
}

impl ModifierLabel for SetVelocityTangentModifierEditor {
    fn label() -> &'static str {
        "⏱ SetVelocityTangentModifier"
    }
}

impl UiProvider for SetVelocityTangentModifierEditor {
    fn draw_ui(&mut self, app: &mut AppContext, ui: &mut egui::Ui, index: u64) {
        unique_collapsing(index, Self::label(), ui).show(ui, |ui| {
            unique_collapsing(index, "Origin", ui).show(ui, |ui| {
                self.origin_expr.draw_ui(app, ui, 1);
            });
            unique_collapsing(index, "Axis", ui).show(ui, |ui| {
                self.axis_expr.draw_ui(app, ui, 2);
            });
            unique_collapsing(index, "Speed (m/s)", ui).show(ui, |ui| {
                self.speed_expr.draw_ui(app, ui, 3);
            });
        });
    }
}

impl ModifierLabel for SetPositionCircleModifierEditor {
    fn label() -> &'static str {
        "🕂 SetPositionCircleModifier"
    }
}

impl UiProvider for SetPositionCircleModifierEditor {
    fn draw_ui(&mut self, app: &mut AppContext, ui: &mut egui::Ui, index: u64) {
        unique_collapsing(index, Self::label(), ui).show(ui, |ui| {
            unique_collapsing(index, "Center", ui).show(ui, |ui| {
                self.center_expr.draw_ui(app, ui, 1);
            });
            unique_collapsing(index, "Axis", ui).show(ui, |ui| {
                self.axis_expr.draw_ui(app, ui, 2);
            });
            unique_collapsing(index, "Radius (m)", ui).show(ui, |ui| {
                self.radius_expr.draw_ui(app, ui, 3);
            });
            ui.horizontal(|ui| {
                ui.radio_value(&mut self.dimension, ShapeDimension::Surface, "Surface");
                ui.radio_value(&mut self.dimension, ShapeDimension::Volume, "Volume");
            });
        });
    }
}

impl ModifierLabel for SetPositionSphereModifierEditor {
    fn label() -> &'static str {
        "🕂 SetPositionSphereModifier"
    }
}

impl UiProvider for SetPositionSphereModifierEditor {
    fn draw_ui(&mut self, app: &mut AppContext, ui: &mut egui::Ui, index: u64) {
        unique_collapsing(index, Self::label(), ui).show(ui, |ui| {
            unique_collapsing(index, "Center", ui).show(ui, |ui| {
                self.center_expr.draw_ui(app, ui, 1);
            });
            unique_collapsing(index, "Radius (m)", ui).show(ui, |ui| {
                self.radius_expr.draw_ui(app, ui, 3);
            });
            ui.horizontal(|ui| {
                ui.radio_value(&mut self.dimension, ShapeDimension::Surface, "Surface");
                ui.radio_value(&mut self.dimension, ShapeDimension::Volume, "Volume");
            });
        });
    }
}

impl ModifierLabel for AccelModifierEditor {
    fn label() -> &'static str {
        "🕛 AccelModifier"
    }
}

impl UiProvider for AccelModifierEditor {
    fn draw_ui(&mut self, app: &mut AppContext, ui: &mut egui::Ui, index: u64) {
        unique_collapsing(index, Self::label(), ui)
            .show(ui, |ui| {
                unique_collapsing(index, "Acceleration (m/s²)", ui).show(ui, |ui| {
                    self.accel_expr.draw_ui(app, ui, 1);
                });
            })
            .header_response
            .on_hover_text(hints::ACCEL);
    }
}

impl ModifierLabel for LinearDragModifierEditor {
    fn label() -> &'static str {
        "☄ LinearDragModifier"
    }
}

impl UiProvider for LinearDragModifierEditor {
    fn draw_ui(&mut self, app: &mut AppContext, ui: &mut egui::Ui, index: u64) {
        unique_collapsing(index, Self::label(), ui)
            .show(ui, |ui| {
                unique_collapsing(index, "Drag (per second)", ui).show(ui, |ui| {
                    self.drag_expr.draw_ui(app, ui, 1);
                });
            })
            .header_response
            .on_hover_text(hints::DRAG);
    }
}

impl ModifierLabel for EmitSpawnEventModifierEditor {
    fn label() -> &'static str {
        "⚡ EmitSpawnEvent"
    }
}

impl UiProvider for EmitSpawnEventModifierEditor {
    fn draw_ui(&mut self, app: &mut AppContext, ui: &mut egui::Ui, index: u64) {
        unique_collapsing(index, Self::label(), ui).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("Count:");
                self.count_expr.draw_ui(app, ui, 1);
            });
            ui.horizontal(|ui| {
                ui.label("Child index:");
                self.child_index = ui_for_u32_ex(ui, self.child_index, 0, 16, 1);
            });
            ui.horizontal(|ui| {
                ui.radio_value(&mut self.condition, EventEmitCondition::Always, "Always");
                ui.radio_value(&mut self.condition, EventEmitCondition::OnDie, "OnDie");
            });
        });
    }
}

impl ModifierLabel for ConformToSphereModifierEditor {
    fn label() -> &'static str {
        "📌 ConformToSphereModifier"
    }
}

impl UiProvider for ConformToSphereModifierEditor {
    fn draw_ui(&mut self, app: &mut AppContext, ui: &mut egui::Ui, index: u64) {
        unique_collapsing(index, Self::label(), ui).show(ui, |ui| {
            unique_collapsing(1, "Origin", ui).show(ui, |ui| {
                self.origin_expr.draw_ui(app, ui, 1);
            });
            unique_collapsing(2, "Radius", ui).show(ui, |ui| {
                self.radius_expr.draw_ui(app, ui, 1);
            });
            unique_collapsing(3, "Influence Distance", ui).show(ui, |ui| {
                self.influence_dist_expr.draw_ui(app, ui, 1);
            });
            unique_collapsing(4, "Attraction Acceleration", ui).show(ui, |ui| {
                self.attraction_accel_expr.draw_ui(app, ui, 1);
            });
            unique_collapsing(5, "Max Attraction Speed", ui).show(ui, |ui| {
                self.max_attraction_speed_expr.draw_ui(app, ui, 1);
            });
        });
    }
}

impl ModifierLabel for SizeOverLifetimeModifierEditor {
    fn label() -> &'static str {
        "📏 SizeOverLifetime"
    }
}

impl UiProvider for SizeOverLifetimeModifierEditor {
    fn draw_ui(&mut self, app: &mut AppContext, ui: &mut egui::Ui, index: u64) {
        unique_collapsing(index, Self::label(), ui)
            .show(ui, |ui| {
                unique_collapsing(1, "Gradient (size in m)", ui).show(ui, |ui| self.gradient.draw_ui(app, ui, 1));
            })
            .header_response
            .on_hover_text(hints::SIZE_OVER_LIFETIME);
    }
}

impl ModifierLabel for ColorOverLifetimeModifierEditor {
    fn label() -> &'static str {
        "🎨 ColorOverLifetime"
    }
}

impl UiProvider for ColorOverLifetimeModifierEditor {
    fn draw_ui(&mut self, app: &mut AppContext, ui: &mut egui::Ui, index: u64) {
        let response = unique_collapsing(index, Self::label(), ui).show(ui, |ui| {
            unique_collapsing(1, "Gradient (r, g, b, opacity)", ui).show(ui, |ui| self.gradient.draw_ui(app, ui, 0));
            unique_collapsing(2, "Blend", ui).show(ui, |ui| {
                let mut blend = self.blend.unwrap_or(ColorBlendMode::default());
                ui.horizontal(|ui| {
                    ui.radio_value(&mut blend, ColorBlendMode::Add, "Add")
                        .on_hover_text("Adds the gradient to the colour set at birth");
                    ui.radio_value(&mut blend, ColorBlendMode::Modulate, "Modulate")
                        .on_hover_text("Multiplies the colour set at birth by the gradient");
                    ui.radio_value(&mut blend, ColorBlendMode::Overwrite, "Overwrite")
                        .on_hover_text("The gradient replaces the colour set at birth");
                });
                self.blend = Some(blend);
            });
            unique_collapsing(3, "Mask", ui).show(ui, |ui| {
                let mut mask = self.mask.unwrap_or(ColorBlendMask::default());
                ui.horizontal(|ui| {
                    ui.radio_value(&mut mask, ColorBlendMask::RGB, "RGB")
                        .on_hover_text("Changes the colour only, not the opacity");
                    ui.radio_value(&mut mask, ColorBlendMask::RGBA, "RGBA")
                        .on_hover_text("Changes colour and opacity");
                });
                self.mask = Some(mask);
            });
        });
        response.header_response.on_hover_text(hints::COLOR_OVER_LIFETIME);
    }
}

impl UiProvider for GradientVec3Editor {
    fn draw_ui(&mut self, _app: &mut AppContext, ui: &mut egui::Ui, _index: u64) {
        let n_keys = self.g.len();
        for (index, (t, v)) in self.g.iter_mut().enumerate() {
            if let Some(list_command) = ui
                .horizontal(|ui| {
                    if let Some(list_command) = ui_for_list_item(ui, index, n_keys) {
                        return Some(list_command);
                    } else {
                        ui.label("t:").on_hover_text(hints::GRADIENT_T);
                        *t = ui_for_f32(ui, *t);
                        ui.label("x y z:");
                        *v = ui_for_vec3(ui, *v);
                        None
                    }
                })
                .inner
            {
                list_command.apply(&mut self.g);
                break;
            }
        }
        if ui.button("+").clicked() {
            self.g.push((0.0, Vec3::ZERO));
        }
    }
}

impl UiProvider for GradientVec4Editor {
    fn draw_ui(&mut self, _app: &mut AppContext, ui: &mut egui::Ui, _index: u64) {
        let n_keys = self.g.len();
        for (index, (t, v)) in self.g.iter_mut().enumerate() {
            if let Some(list_command) = ui
                .horizontal(|ui| {
                    if let Some(list_command) = ui_for_list_item(ui, index, n_keys) {
                        return Some(list_command);
                    } else {
                        ui.label("t:").on_hover_text(hints::GRADIENT_T);
                        *t = ui_for_f32(ui, *t);
                        ui.label("r g b a:");
                        *v = ui_for_vec4(ui, *v);
                    }
                    None
                })
                .inner
            {
                list_command.apply(&mut self.g);
                break;
            }
        }
        if ui.button("+").clicked() {
            self.g.push((0.0, Vec4::ZERO));
        }
    }
}

impl ModifierLabel for ParticleTextureModifierEditor {
    fn label() -> &'static str {
        "🖼 ParticleTexture"
    }
}

impl ModifierLabel for OrientModifierEditor {
    fn label() -> &'static str {
        "🧭 Orient"
    }
}
