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

// --- P2b modifiers -------------------------------------------------------------

/// A named expression in a collapsible section, with a tooltip.
fn expr_section(app: &mut AppContext, ui: &mut egui::Ui, index: u64, label: &str, hint: &str, expr: &mut ExprWriterEditor) {
    unique_collapsing(index, label, ui)
        .show(ui, |ui| expr.draw_ui(app, ui, index))
        .header_response
        .on_hover_text(hint);
}

impl ModifierLabel for SetPositionCone3dModifierEditor {
    fn label() -> &'static str {
        "🕂 SetPositionCone3d"
    }
}

impl UiProvider for SetPositionCone3dModifierEditor {
    fn draw_ui(&mut self, app: &mut AppContext, ui: &mut egui::Ui, index: u64) {
        unique_collapsing(index, Self::label(), ui)
            .show(ui, |ui| {
                expr_section(app, ui, 1, "Height (m)", hints::CONE_HEIGHT, &mut self.height_expr);
                expr_section(app, ui, 2, "Base radius (m)", hints::CONE_BASE, &mut self.base_radius_expr);
                expr_section(app, ui, 3, "Top radius (m)", hints::CONE_TOP, &mut self.top_radius_expr);
                ui.horizontal(|ui| {
                    ui.radio_value(&mut self.dimension, ShapeDimension::Volume, "Volume");
                    ui.radio_value(&mut self.dimension, ShapeDimension::Surface, "Surface")
                        .on_hover_text("bevy_hanabi always fills the cone's volume; Surface behaves like Volume");
                });
            })
            .header_response
            .on_hover_text(hints::CONE);
    }
}

impl ModifierLabel for RadialAccelModifierEditor {
    fn label() -> &'static str {
        "🕛 RadialAccel"
    }
}

impl UiProvider for RadialAccelModifierEditor {
    fn draw_ui(&mut self, app: &mut AppContext, ui: &mut egui::Ui, index: u64) {
        unique_collapsing(index, Self::label(), ui)
            .show(ui, |ui| {
                expr_section(app, ui, 1, "Origin", "The point particles are pushed from or pulled to (m)", &mut self.origin_expr);
                expr_section(app, ui, 2, "Acceleration (m/s²)", hints::RADIAL_ACCEL_VALUE, &mut self.accel_expr);
            })
            .header_response
            .on_hover_text(hints::RADIAL_ACCEL);
    }
}

impl ModifierLabel for TangentAccelModifierEditor {
    fn label() -> &'static str {
        "🕛 TangentAccel"
    }
}

impl UiProvider for TangentAccelModifierEditor {
    fn draw_ui(&mut self, app: &mut AppContext, ui: &mut egui::Ui, index: u64) {
        unique_collapsing(index, Self::label(), ui)
            .show(ui, |ui| {
                expr_section(app, ui, 1, "Origin", "A point on the swirl's axis (m)", &mut self.origin_expr);
                expr_section(app, ui, 2, "Axis", "The direction the swirl turns around, e.g. (0, 1, 0) = around up", &mut self.axis_expr);
                expr_section(app, ui, 3, "Acceleration (m/s²)", hints::TANGENT_ACCEL_VALUE, &mut self.accel_expr);
            })
            .header_response
            .on_hover_text(hints::TANGENT_ACCEL);
    }
}

impl ModifierLabel for KillSphereModifierEditor {
    fn label() -> &'static str {
        "💀 KillSphere"
    }
}

impl UiProvider for KillSphereModifierEditor {
    fn draw_ui(&mut self, app: &mut AppContext, ui: &mut egui::Ui, index: u64) {
        unique_collapsing(index, Self::label(), ui)
            .show(ui, |ui| {
                expr_section(app, ui, 1, "Center", "Center of the sphere (m)", &mut self.center_expr);
                expr_section(app, ui, 2, "Radius (m)", "Radius of the sphere", &mut self.radius_expr);
                ui.checkbox(&mut self.kill_inside, "kill inside").on_hover_text(hints::KILL_INSIDE);
            })
            .header_response
            .on_hover_text(hints::KILL_SPHERE);
    }
}

impl ModifierLabel for KillAabbModifierEditor {
    fn label() -> &'static str {
        "💀 KillAabb"
    }
}

impl UiProvider for KillAabbModifierEditor {
    fn draw_ui(&mut self, app: &mut AppContext, ui: &mut egui::Ui, index: u64) {
        unique_collapsing(index, Self::label(), ui)
            .show(ui, |ui| {
                expr_section(app, ui, 1, "Center", "Center of the box (m)", &mut self.center_expr);
                expr_section(app, ui, 2, "Half size (m)", "Half the box's width, height and depth (vec3)", &mut self.half_size_expr);
                ui.checkbox(&mut self.kill_inside, "kill inside").on_hover_text(hints::KILL_INSIDE);
            })
            .header_response
            .on_hover_text(hints::KILL_AABB);
    }
}

/// A render-time constant or random range of vectors (`±` switches).
fn ui_for_cpu_vec<T: Copy>(ui: &mut egui::Ui, value: &mut CpuVecDef<T>, edit: impl Fn(&mut egui::Ui, T) -> T) {
    match value {
        CpuVecDef::Single(v) => {
            *v = edit(ui, *v);
            let v = *v;
            if ui.small_button("±").on_hover_text("Random range (the same value for every particle at render time)").clicked() {
                *value = CpuVecDef::Uniform((v, v));
            }
        }
        CpuVecDef::Uniform((a, b)) => {
            ui.vertical(|ui| {
                *a = edit(ui, *a);
                ui.label("to");
                *b = edit(ui, *b);
            });
            let a = *a;
            if ui.small_button("=").on_hover_text("Single value").clicked() {
                *value = CpuVecDef::Single(a);
            }
        }
    }
}

/// Blend and mask radio buttons of the colour render modifiers.
fn ui_for_color_blend(ui: &mut egui::Ui, blend: &mut ColorBlendMode, mask: &mut ColorBlendMask) {
    ui.horizontal(|ui| {
        ui.radio_value(blend, ColorBlendMode::Modulate, "Modulate")
            .on_hover_text("Multiplies the particle's colour");
        ui.radio_value(blend, ColorBlendMode::Add, "Add").on_hover_text("Adds to the particle's colour");
        ui.radio_value(blend, ColorBlendMode::Overwrite, "Overwrite")
            .on_hover_text("Replaces the particle's colour");
    });
    ui.horizontal(|ui| {
        ui.radio_value(mask, ColorBlendMask::RGBA, "RGBA").on_hover_text("Changes colour and opacity");
        ui.radio_value(mask, ColorBlendMask::RGB, "RGB").on_hover_text("Changes the colour only, not the opacity");
        ui.radio_value(mask, ColorBlendMask::A, "A").on_hover_text("Changes the opacity only");
    });
}

impl ModifierLabel for SetColorModifierEditor {
    fn label() -> &'static str {
        "🎨 SetColor"
    }
}

impl UiProvider for SetColorModifierEditor {
    fn draw_ui(&mut self, _app: &mut AppContext, ui: &mut egui::Ui, index: u64) {
        unique_collapsing(index, Self::label(), ui)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("r g b a:");
                    ui_for_cpu_vec(ui, &mut self.color, ui_for_vec4);
                });
                ui_for_color_blend(ui, &mut self.blend, &mut self.mask);
            })
            .header_response
            .on_hover_text(hints::SET_COLOR);
    }
}

impl ModifierLabel for SetSizeModifierEditor {
    fn label() -> &'static str {
        "📏 SetSize"
    }
}

impl UiProvider for SetSizeModifierEditor {
    fn draw_ui(&mut self, _app: &mut AppContext, ui: &mut egui::Ui, index: u64) {
        unique_collapsing(index, Self::label(), ui)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("x y z (m):");
                    ui_for_cpu_vec(ui, &mut self.size, ui_for_vec3);
                });
            })
            .header_response
            .on_hover_text(hints::SET_SIZE);
    }
}

impl ModifierLabel for FlipbookModifierEditor {
    fn label() -> &'static str {
        "🎞 Flipbook"
    }
}

impl UiProvider for FlipbookModifierEditor {
    fn draw_ui(&mut self, app: &mut AppContext, ui: &mut egui::Ui, index: u64) {
        unique_collapsing(index, Self::label(), ui)
            .show(ui, |ui| {
                let mut from_texture = self.grid.is_none();
                ui.horizontal(|ui| {
                    if ui
                        .checkbox(&mut from_texture, "grid of the texture")
                        .on_hover_text("Use the rows × columns set on the texture asset (ASSETS tab)")
                        .changed()
                    {
                        self.grid = if from_texture { None } else { Some(app.layer_texture_grid.unwrap_or((4, 4))) };
                    }
                    if from_texture {
                        match app.layer_texture_grid {
                            Some((rows, cols)) => {
                                ui.label(format!("{} rows × {} columns", rows, cols));
                            }
                            None => {
                                ui.colored_label(egui::Color32::from_hex("#FFCC66").unwrap(), "⚠ the texture has no grid")
                                    .on_hover_text("Set the grid in the ASSETS tab, or untick and set it here");
                            }
                        }
                    }
                });
                if let Some((rows, cols)) = &mut self.grid {
                    ui.horizontal(|ui| {
                        ui.label("rows");
                        *rows = ui_for_u32_ex(ui, *rows, 1, 64, 1);
                        ui.label("columns");
                        *cols = ui_for_u32_ex(ui, *cols, 1, 64, 1);
                    });
                }
            })
            .header_response
            .on_hover_text(hints::FLIPBOOK);
    }
}

impl ModifierLabel for RoundModifierEditor {
    fn label() -> &'static str {
        "⏺ Round"
    }
}

impl UiProvider for RoundModifierEditor {
    fn draw_ui(&mut self, app: &mut AppContext, ui: &mut egui::Ui, index: u64) {
        unique_collapsing(index, Self::label(), ui)
            .show(ui, |ui| {
                expr_section(app, ui, 1, "Roundness (0..1)", hints::ROUND, &mut self.roundness_expr);
            })
            .header_response
            .on_hover_text(hints::ROUND);
    }
}

impl ModifierLabel for ScreenSpaceSizeModifierEditor {
    fn label() -> &'static str {
        "🖵 ScreenSpaceSize"
    }
}

impl UiProvider for ScreenSpaceSizeModifierEditor {
    fn draw_ui(&mut self, _app: &mut AppContext, ui: &mut egui::Ui, _index: u64) {
        ui.label(Self::label()).on_hover_text(hints::SCREEN_SPACE_SIZE);
    }
}
