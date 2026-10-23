//! The right panel: tabs for the selected layer (inspector), sockets & fit,
//! the preview settings, parameters, triggers and assets.

use bevy_egui::egui::{self, RichText};
use omagari::format::{LayerDef, ParamValue, ProjectV2};

use super::assets::{assets_tab, TexturePreviews};
use super::camera::PreviewCamera;
use super::effect::draw_particles_layer;
use super::package::{motion_panel, sockets_and_fit_panel};
use super::problems::{message_list, subject_box, ACCENT};
use super::scene::{Backdrop, Stage};
use super::session::{EditorSession, RightTab};
use super::AppContext;
use crate::settings::GameAssets;

const TABS: [(RightTab, &str, &str); 6] = [
    (RightTab::Layer, "🔧 LAYER", "Settings of the selected layer"),
    (RightTab::SocketsFit, "⚓ SOCKETS & FIT", "Named points on the target, and size fit"),
    (RightTab::Preview, "🎬 PREVIEW", "Stage, camera, backdrop and target motion of the preview"),
    (RightTab::Parameters, "PARAMETERS", "Values the game sets at runtime (P2)"),
    (RightTab::Triggers, "⚡ TRIGGERS", "Events the game fires (P2)"),
    (RightTab::Assets, "🖼 ASSETS", "Textures of the project"),
];

pub fn right_panel(
    ui: &mut egui::Ui,
    project: &mut ProjectV2,
    session: &mut EditorSession,
    app: &mut AppContext,
    backdrop: &mut Backdrop,
    previews: &mut TexturePreviews,
    game_assets: &GameAssets,
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
            RightTab::Layer => layer_tab(ui, project, session, app),
            RightTab::SocketsFit => {
                sockets_and_fit_panel(ui, project);
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
            RightTab::Preview => preview_tab(ui, project, session, backdrop),
            RightTab::Parameters => parameters_tab(ui, project),
            RightTab::Triggers => triggers_tab(ui, project),
            RightTab::Assets => assets_tab(ui, project, session, previews, game_assets),
        });
}

fn layer_tab(ui: &mut egui::Ui, project: &mut ProjectV2, session: &mut EditorSession, app: &mut AppContext) {
    let Some(index) = session.selected.filter(|&i| i < project.layers.len()) else {
        ui.add_space(8.0);
        ui.label(RichText::new("Select a layer on the left, or ➕ ADD one.").weak());
        return;
    };
    let ProjectV2 { layers, assets, .. } = project;
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
            ui.add_space(4.0);
            draw_particles_layer(layer, assets, app, ui, index as u64);
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
    ui.separator();
    ui.label(RichText::new("Model, animation and socket preview come in P3.").weak());
}

fn parameters_tab(ui: &mut egui::Ui, project: &ProjectV2) {
    ui.label("Parameters are values the game sets while the effect plays (e.g. intensity 0..1, a tint colour).");
    ui.label(RichText::new("Editing them and binding layer values to them comes in P2.").weak());
    ui.separator();
    if project.parameters.is_empty() {
        ui.label(RichText::new("This project declares no parameters.").weak());
    }
    egui::Grid::new("parameters").striped(true).show(ui, |ui| {
        for p in project.parameters.iter() {
            ui.label(&p.name);
            ui.label(match &p.default {
                ParamValue::Float(v) => format!("number, default {}", v),
                ParamValue::Color(c) => format!("colour, default {:?}", c),
            });
            ui.end_row();
        }
    });
}

fn triggers_tab(ui: &mut egui::Ui, project: &ProjectV2) {
    ui.label("Triggers are named one-shot events the game fires (e.g. hit, tick, swing_start).");
    ui.label(RichText::new("Editing them and playing layers on a trigger comes in P2.").weak());
    ui.separator();
    if project.triggers.is_empty() {
        ui.label(RichText::new("This project declares no triggers.").weak());
    }
    for t in project.triggers.iter() {
        ui.label(&t.name);
    }
}
