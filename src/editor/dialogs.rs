//! Modal questions: unsaved changes, deleting a layer, the save-as name.

use bevy_egui::egui::{self, Key, RichText};

use super::actions::{project_file_name, Action};
use super::problems::WARNING;
use super::session::{Dialog, EditorSession};
use crate::controller::Folder;

/// What the action would do, for the unsaved-changes question.
fn describe(action: &Action) -> String {
    match action {
        Action::New | Action::NewFromRecipe(_) => "Start a new project".into(),
        Action::Open(name) => format!("Open {}", name),
        Action::Preview(request) => format!("Claude asks to preview {}", request.name),
        Action::Quit => "Quit Omagari".into(),
        _ => "Continue".into(),
    }
}

pub fn dialogs(ctx: &egui::Context, session: &mut EditorSession, filename: Option<&str>) {
    let Some(dialog) = session.dialog.clone() else {
        return;
    };
    let mut close = false;
    let mut actions = Vec::new();
    let modal = egui::Modal::new(egui::Id::new("omagari dialog")).show(ctx, |ui| {
        ui.set_width(380.0);
        match &dialog {
            Dialog::UnsavedChanges { then } => {
                ui.heading("Unsaved changes");
                ui.label(format!(
                    "{}? Changes to {} that are not saved will be lost.",
                    describe(then),
                    filename.filter(|f| !f.is_empty()).unwrap_or("the untitled project")
                ));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("⮋ Save").on_hover_text("Save, then continue").clicked() {
                        actions.push(Action::SaveThen(Box::new(then.clone())));
                        close = true;
                    }
                    if ui.button("Don't save").clicked() {
                        actions.push(Action::Confirmed(Box::new(then.clone())));
                        close = true;
                    }
                    if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
                        close = true;
                    }
                });
            }
            Dialog::DeleteLayer { index, name } => {
                ui.heading("Delete layer");
                ui.label(format!("Delete the layer \"{}\"? Undo (Ctrl+Z) brings it back.", name));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let delete = ui.button(RichText::new("🗑 Delete").color(WARNING)).on_hover_text("Enter");
                    if delete.clicked() || ui.input(|i| i.key_pressed(Key::Enter)) {
                        actions.push(Action::Confirmed(Box::new(Action::DeleteLayer(*index))));
                        close = true;
                    }
                    if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
                        close = true;
                    }
                });
            }
            Dialog::SaveAs { name, then } => {
                ui.heading("Save as");
                ui.label("File name in saved_effects/ (.omagari.ron is added):");
                let mut text = name.clone();
                let response = ui.add(egui::TextEdit::singleline(&mut text).hint_text("fire").desired_width(340.0));
                if ui.memory(|m| m.focused().is_none()) {
                    response.request_focus();
                }
                let file = project_file_name(&text);
                match &file {
                    Some(file) if Folder::SavedEffects.full_file_path(file.clone()).exists() => {
                        ui.colored_label(WARNING, format!("{} exists and will be replaced.", file));
                    }
                    Some(file) => {
                        ui.label(RichText::new(format!("Saves {}", file)).weak());
                    }
                    None => {
                        ui.label(RichText::new("Type a name (no / \\ : * ? \" < > |).").weak());
                    }
                }
                ui.add_space(8.0);
                let enter = response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                ui.horizontal(|ui| {
                    if ui.add_enabled(file.is_some(), egui::Button::new("⮋ Save")).clicked() || (enter && file.is_some()) {
                        // The dialog stays open until the save worked
                        // (then it runs `then`).
                        actions.push(Action::SaveTo(text.clone()));
                    }
                    if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
                        close = true;
                    }
                });
                if text != *name {
                    session.dialog = Some(Dialog::SaveAs { name: text, then: then.clone() });
                }
            }
        }
    });
    if close || modal.should_close() {
        session.dialog = None;
    }
    session.actions.extend(actions);
}
