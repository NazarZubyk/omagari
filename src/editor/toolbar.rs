//! The top toolbar: project (new, open, save, save as, export), undo/redo,
//! stage, camera, backdrop and settings.

use bevy_egui::egui::{self, RichText};
use omagari::format::ProjectV2;

use super::actions::Action;
use super::camera::PreviewCamera;
use super::helpers::ui_text;
use super::problems::{ACCENT, WARNING};
use super::scene::{Backdrop, Stage};
use super::session::EditorSession;
use super::shortcuts::Shortcut;
use super::AppContext;
use crate::controller::{projects_list, validate_project_filename};
use crate::mcp;
use crate::settings::GameAssets;

pub fn toolbar(
    ui: &mut egui::Ui,
    project: &mut ProjectV2,
    session: &mut EditorSession,
    app: &AppContext,
    backdrop: &mut Backdrop,
    game_assets: &GameAssets,
) {
    ui.add_space(2.0);
    ui.horizontal_wrapped(|ui| {
        ui.colored_label(ACCENT, "OMAGARI🔥");
        let name = app.filename.as_deref().filter(|f| !f.is_empty()).unwrap_or("untitled");
        let dirty = session.is_dirty();
        ui.label(RichText::new(name).strong()).on_hover_text("The open project (in saved_effects/)");
        if dirty {
            ui.colored_label(WARNING, "• unsaved").on_hover_text("This project has changes that are not saved (SAVE, Ctrl+S)");
        }
        ui.separator();

        ui.menu_button("🌌 NEW", |ui| {
            if ui.button("Empty project").clicked() {
                session.actions.push(Action::New);
                ui.close();
            }
            ui.separator();
            ui.label(RichText::new("From a recipe:").weak())
                .on_hover_text("Ready-made effects from the guide; they pass every check");
            for (index, recipe) in mcp::guide_recipes().iter().enumerate() {
                if ui.button(ui_text(&recipe.title)).clicked() {
                    session.actions.push(Action::NewFromRecipe(index));
                    ui.close();
                }
            }
        })
        .response
        .on_hover_text("Start a new project: empty, or from a recipe");

        ui.menu_button("⮉ OPEN", |ui| {
            let files = projects_list();
            if files.is_empty() {
                ui.label("No *.omagari.ron files in saved_effects/");
            }
            egui::ScrollArea::vertical().max_height(400.0).show(ui, |ui| {
                for f in files {
                    let current = app.filename.as_deref() == Some(f.as_str());
                    if ui.selectable_label(current, &f).clicked() {
                        session.actions.push(Action::Open(f.clone()));
                        ui.close();
                    }
                }
            });
        })
        .response
        .on_hover_text("Open a project from saved_effects/");

        if ui
            .button("⮋ SAVE")
            .on_hover_text(format!("Save to saved_effects/ ({})", Shortcut::Save.label()))
            .clicked()
        {
            session.actions.push(Action::Save);
        }
        if ui.button("SAVE AS…").on_hover_text("Save under a new name").clicked() {
            session.actions.push(Action::SaveAs);
        }
        let named = app.filename.as_deref().is_some_and(validate_project_filename);
        if ui
            .add_enabled(named, egui::Button::new("🖭 EXPORT"))
            .on_hover_text(format!(
                "Write <name>.vfx.ron (the file a game loads) to {} ({})",
                game_assets.export_dir().display(),
                Shortcut::Export.label()
            ))
            .on_disabled_hover_text("Save the project first: the export is named after it")
            .clicked()
        {
            session.actions.push(Action::Export);
        }
        ui.separator();

        let can_undo = session.history.can_undo(&session.text);
        let can_redo = session.history.can_redo(&session.text);
        if ui
            .add_enabled(can_undo, egui::Button::new("↺"))
            .on_hover_text(format!(
                "Undo ({}); {} step(s) back",
                Shortcut::Undo.label(),
                session.history.undo_len()
            ))
            .clicked()
        {
            session.actions.push(Action::Undo);
        }
        if ui
            .add_enabled(can_redo, egui::Button::new("↻"))
            .on_hover_text(format!("Redo ({})", Shortcut::Redo.label()))
            .clicked()
        {
            session.actions.push(Action::Redo);
        }
        ui.separator();

        let stage = Stage::of(project);
        ui.menu_button(format!("▣ STAGE: {}", stage.name()), |ui| {
            for s in Stage::ALL {
                if ui.radio(s == stage, s.name()).on_hover_text(s.hint()).clicked() {
                    s.store(project);
                    ui.close();
                }
            }
        })
        .response
        .on_hover_text("Where the effect plays: in the air, on the floor, or around a rock (saved with the project)");

        let camera = PreviewCamera::of(project);
        ui.menu_button(format!("🎥 CAMERA: {}", camera.name()), |ui| {
            for c in PreviewCamera::ALL {
                if ui.radio(c == camera, c.name()).on_hover_text(c.hint()).clicked() {
                    c.store(project);
                    ui.close();
                }
            }
            ui.separator();
            if ui.button("Focus on the target (F)").clicked() {
                session.actions.push(Action::FocusCamera);
                ui.close();
            }
            if ui.button("Camera settings…").clicked() {
                session.settings_open = true;
                ui.close();
            }
        })
        .response
        .on_hover_text("Orbit = free camera; Game = the game's isometric camera (saved with the project)");

        if ui
            .button(format!("◐ BG: {}", backdrop.name()))
            .on_hover_text(format!("{} (click for the next backdrop)", backdrop.hint()))
            .clicked()
        {
            *backdrop = backdrop.next();
        }
        if ui
            .selectable_label(session.settings_open, "⚙ SETTINGS")
            .on_hover_text("Game assets folder and camera profiles")
            .clicked()
        {
            session.settings_open = !session.settings_open;
        }
    });
    ui.add_space(2.0);
}
