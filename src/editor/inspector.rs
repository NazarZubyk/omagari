//! The right panel: tabs for the selected layer (inspector), sockets & fit,
//! the preview settings (stage, camera, motion, model, animation, weapon),
//! parameters, triggers, assets and the preview model's hierarchy.

use bevy_egui::egui::{self, RichText};
use omagari::format::{LayerDef, ProjectV2};

use super::actions::Action;
use super::assets::{assets_tab, TexturePreviews};
use super::camera::PreviewCamera;
use super::effect::draw_particles_layer;
use super::light::draw_light_layer;
use super::shape::draw_shape_layer;
use super::surface::draw_surface_layer;
use super::trail::{draw_afterimage_layer, draw_trail_layer, EmitterContext};
use super::model::ModelPreview;
use super::model_panel::{hierarchy_tab, model_section, target_model_info};
use super::package::{motion_panel, sockets_and_fit_panel};
use super::params_panel::{parameters_tab, triggers_tab};
use super::hints;
use super::problems::{message_list, subject_box, ACCENT};
use super::scene::{Backdrop, Stage};
use super::session::{EditorSession, RightTab};
use super::AppContext;
use crate::settings::GameAssets;

const TABS: [(RightTab, &str, &str); 7] = [
    (RightTab::Layer, "🔧 LAYER", "Settings of the selected layer"),
    (RightTab::SocketsFit, "⚓ SOCKETS & FIT", "Named points on the target, and size fit"),
    (RightTab::Preview, "🎬 PREVIEW", "Stage, camera, backdrop and target motion of the preview"),
    (RightTab::Parameters, "📈 PARAMETERS", "Values the game sets while the effect plays (intensity, tint, ...)"),
    (RightTab::Triggers, "⚡ TRIGGERS", "Events the game fires (hit, tick, ...): bursts and pulses"),
    (RightTab::Assets, "🖼 ASSETS", "Textures of the project"),
    (RightTab::Hierarchy, "🌲 HIERARCHY", "Nodes, bones, meshes and materials of the preview model: attach, sockets, segments"),
];

pub fn right_panel(
    ui: &mut egui::Ui,
    project: &mut ProjectV2,
    session: &mut EditorSession,
    app: &mut AppContext,
    backdrop: &mut Backdrop,
    previews: &mut TexturePreviews,
    game_assets: &GameAssets,
    model: &mut ModelPreview,
) {
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        for (tab, label, hint) in TABS {
            ui.selectable_value(&mut session.tab, tab, label).on_hover_text(hint);
        }
    });
    ui.separator();
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| match session.tab {
            RightTab::Layer => {
                let info = target_model_info(project, game_assets);
                let clip = model.clip.as_ref().map(|_| (model.time, model.duration));
                layer_tab(ui, project, session, app, info.as_deref(), clip)
            }
            RightTab::SocketsFit => {
                let info = target_model_info(project, game_assets);
                sockets_and_fit_panel(ui, project, model, info.as_deref());
                let subjects: Vec<String> =
                    project.sockets.iter().map(|s| format!("socket \"{}\"", s.name)).collect();
                for subject in subjects.iter() {
                    subject_box(ui, session, subject);
                }
                let fit_messages = session
                    .diagnostics
                    .all()
                    .filter(|(_, m)| !m.starts_with('[') && m.contains("fit"));
                message_list(ui, fit_messages);
            }
            RightTab::Preview => {
                preview_tab(ui, project, session, backdrop);
                ui.separator();
                model_section(ui, project, model, game_assets);
            }
            RightTab::Parameters => parameters_tab(ui, project, session, app),
            RightTab::Triggers => triggers_tab(ui, project, session, app),
            RightTab::Assets => assets_tab(ui, project, session, previews, game_assets),
            RightTab::Hierarchy => hierarchy_tab(ui, project, session, model, app, game_assets),
        });
}

fn layer_tab(
    ui: &mut egui::Ui,
    project: &mut ProjectV2,
    session: &mut EditorSession,
    app: &mut AppContext,
    model: Option<&crate::model_info::ModelInfo>,
    clip: Option<(f32, f32)>,
) {
    let Some(index) = session.selected.filter(|&i| i < project.layers.len()) else {
        ui.add_space(8.0);
        ui.label(RichText::new("Select a layer on the left, or ➕ ADD one.").weak());
        return;
    };
    let ProjectV2 { layers, assets, parameters, triggers, sockets, preview, .. } = project;
    let name = layers[index].name().to_string();
    match &mut layers[index] {
        LayerDef::Particles(layer) => {
            ui.horizontal(|ui| {
                ui.label(RichText::new("✨ PARTICLES LAYER").color(ACCENT).strong());
                if !session.view.shows(&name) {
                    ui.label(RichText::new("(hidden in the preview)").weak());
                }
            });
            subject_box(ui, session, &name);
            ui.horizontal(|ui| {
                ui.menu_button("✨ PRESETS", |ui| {
                    for preset in super::particle_presets::ParticlePreset::ALL {
                        if ui.button(preset.label()).on_hover_text(preset.hint()).clicked() {
                            session.actions.push(Action::ApplyParticlePreset { layer: index, preset });
                            ui.close();
                        }
                    }
                })
                .response
                .on_hover_text(hints::PARTICLE_PRESETS);
            });
            ui.add_space(4.0);
            draw_particles_layer(layer, assets, parameters, app, ui, index as u64);
        }
        LayerDef::Surface(layer) => {
            ui.horizontal(|ui| {
                ui.label(RichText::new("🎨 SURFACE LAYER").color(ACCENT).strong()).on_hover_text(hints::SURFACE);
                if !session.view.shows(&name) {
                    ui.label(RichText::new("(hidden in the preview)").weak());
                }
            });
            ui.label(RichText::new(hints::SURFACE).weak().small());
            subject_box(ui, session, &name);
            ui.add_space(4.0);
            draw_surface_layer(layer, assets, parameters, app, ui, index as u64, model);
        }
        LayerDef::Shape(layer) => {
            ui.horizontal(|ui| {
                ui.label(RichText::new("🔷 SHAPE LAYER").color(ACCENT).strong()).on_hover_text(hints::SHAPE);
                if !session.view.shows(&name) {
                    ui.label(RichText::new("(hidden in the preview)").weak());
                }
            });
            ui.label(RichText::new(hints::SHAPE).weak().small());
            subject_box(ui, session, &name);
            ui.add_space(4.0);
            draw_shape_layer(layer, assets, parameters, app, ui, index as u64);
        }
        LayerDef::Trail(layer) => {
            ui.horizontal(|ui| {
                ui.label(RichText::new("〰 TRAIL LAYER").color(ACCENT).strong()).on_hover_text(hints::TRAIL);
                if !session.view.shows(&name) {
                    ui.label(RichText::new("(hidden in the preview)").weak());
                }
            });
            ui.label(RichText::new(hints::TRAIL).weak().small());
            subject_box(ui, session, &name);
            ui.add_space(4.0);
            let mut ctx = EmitterContext {
                assets,
                parameters,
                triggers,
                sockets,
                preview,
                model,
                clip,
                show_samples: &mut session.show_trail_samples,
            };
            draw_trail_layer(layer, &mut ctx, app, ui, index as u64);
        }
        LayerDef::Afterimage(layer) => {
            ui.horizontal(|ui| {
                ui.label(RichText::new("👥 AFTERIMAGE LAYER").color(ACCENT).strong()).on_hover_text(hints::AFTERIMAGE);
                if !session.view.shows(&name) {
                    ui.label(RichText::new("(hidden in the preview)").weak());
                }
            });
            ui.label(RichText::new(hints::AFTERIMAGE).weak().small());
            subject_box(ui, session, &name);
            ui.add_space(4.0);
            let mut ctx = EmitterContext {
                assets,
                parameters,
                triggers,
                sockets,
                preview,
                model,
                clip,
                show_samples: &mut session.show_trail_samples,
            };
            draw_afterimage_layer(layer, &mut ctx, app, ui, index as u64);
        }
        LayerDef::Light(layer) => {
            ui.horizontal(|ui| {
                ui.label(RichText::new("💡 LIGHT LAYER").color(ACCENT).strong()).on_hover_text(hints::LIGHT);
                if !session.view.shows(&name) {
                    ui.label(RichText::new("(hidden in the preview)").weak());
                }
            });
            ui.label(RichText::new(hints::LIGHT).weak().small());
            subject_box(ui, session, &name);
            ui.add_space(4.0);
            draw_light_layer(layer, triggers, parameters, app, &mut session.show_lights, ui, index as u64);
        }
    }
}

fn preview_tab(ui: &mut egui::Ui, project: &mut ProjectV2, session: &mut EditorSession, backdrop: &mut Backdrop) {
    ui.label(RichText::new("Saved with the project (preview block); not part of the exported effect.").weak());
    ui.add_space(4.0);
    egui::Grid::new("preview grid").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
        ui.label("Stage").on_hover_text("Where the effect plays relative to the room");
        ui.horizontal_wrapped(|ui| {
            let stage = Stage::of(project);
            for s in Stage::ALL {
                if ui.radio(s == stage, s.name()).on_hover_text(s.hint()).clicked() {
                    s.store(project);
                }
            }
        });
        ui.end_row();

        ui.label("Camera").on_hover_text("Which camera shows the preview");
        ui.horizontal_wrapped(|ui| {
            let camera = PreviewCamera::of(project);
            for c in PreviewCamera::ALL {
                if ui.radio(c == camera, c.name()).on_hover_text(c.hint()).clicked() {
                    c.store(project);
                }
            }
            if ui.small_button("settings…").on_hover_text("Edit the camera profiles").clicked() {
                session.settings_open = true;
            }
        });
        ui.end_row();

        ui.label("Backdrop").on_hover_text("What is behind the effect (editor setting, not saved)");
        ui.horizontal_wrapped(|ui| {
            for b in Backdrop::ALL {
                if ui.radio(*backdrop == b, b.name()).on_hover_text(b.hint()).clicked() {
                    *backdrop = b;
                }
            }
        });
        ui.end_row();
    });
    ui.separator();
    ui.strong("Target motion")
        .on_hover_text("Moves the preview target so you can see how the effect follows a moving character");
    motion_panel(ui, project);
}


