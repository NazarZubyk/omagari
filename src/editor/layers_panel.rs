//! The left panel: the project's layers. Select, add, duplicate, delete,
//! reorder, and hide / solo layers in the preview.

use bevy_egui::egui::{self, RichText};
use omagari::format::{LayerDef, ProjectV2};

use super::actions::Action;
use super::problems::{badge, subject_hover, ACCENT, WARNING};
use super::session::EditorSession;
use super::shortcuts::Shortcut;

/// How deep a layer sits under parents (0 = root), for the indent.
fn depth(project: &ProjectV2, index: usize) -> usize {
    let mut depth = 0;
    let mut current = index;
    while let Some(parent) = project.layers[current].as_particles().and_then(|p| p.parent.as_deref()) {
        match project.layers.iter().position(|l| l.name() == parent) {
            Some(p) if depth < project.layers.len() => {
                current = p;
                depth += 1;
            }
            _ => break,
        }
    }
    depth
}

pub fn layers_panel(ui: &mut egui::Ui, project: &ProjectV2, session: &mut EditorSession) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.strong("LAYERS");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.menu_button("➕ ADD", |ui| {
                if ui
                    .button("✨ Particles layer")
                    .on_hover_text("A new layer of bevy_hanabi particles")
                    .clicked()
                {
                    session.actions.push(Action::AddParticlesLayer);
                    ui.close();
                }
                if ui
                    .button("🎨 Surface layer")
                    .on_hover_text("Omagari's effect shader over the target's model (glowing skin, scrolling noise, rim, shells with volume)")
                    .clicked()
                {
                    session.actions.push(Action::AddSurfaceLayer);
                    ui.close();
                }
                if ui
                    .button("🔷 Shape layer")
                    .on_hover_text("A simple 3D shape with the effect shader: shield bubble, ground ring, shockwave, slash arc, glow column (presets in the inspector)")
                    .clicked()
                {
                    session.actions.push(Action::AddShapeLayer);
                    ui.close();
                }
                ui.add_enabled(false, egui::Button::new("Trail, afterimage, light… (later phases)"));
            })
            .response
            .on_hover_text("Add a layer");
        });
    });
    ui.separator();

    let n = project.layers.len();
    if n == 0 {
        ui.label(RichText::new("No layers yet. ➕ ADD one, or start from a recipe (🌌 NEW).").weak());
    }
    egui::ScrollArea::vertical()
        .auto_shrink([false, true])
        .max_height((ui.available_height() - 40.0).max(80.0))
        .show(ui, |ui| {
            for (index, layer) in project.layers.iter().enumerate() {
                layer_row(ui, project, session, index, layer);
            }
        });

    ui.separator();
    let selected = session.selected.filter(|&i| i < n);
    ui.horizontal_wrapped(|ui| {
        let enabled = selected.is_some();
        let index = selected.unwrap_or(0);
        if ui
            .add_enabled(enabled && index > 0, egui::Button::new("⬆"))
            .on_hover_text("Move the selected layer up (child layers must stay below their parent)")
            .clicked()
        {
            session.actions.push(Action::MoveLayer { from: index, to: index - 1 });
        }
        if ui
            .add_enabled(enabled && index + 1 < n, egui::Button::new("⬇"))
            .on_hover_text("Move the selected layer down")
            .clicked()
        {
            session.actions.push(Action::MoveLayer { from: index, to: index + 1 });
        }
        if ui
            .add_enabled(enabled, egui::Button::new("🗐 DUPLICATE"))
            .on_hover_text(format!("Copy the selected layer ({})", Shortcut::DuplicateLayer.label()))
            .clicked()
        {
            session.actions.push(Action::DuplicateLayer(index));
        }
        if ui
            .add_enabled(enabled, egui::Button::new("🗑 DELETE"))
            .on_hover_text(format!("Delete the selected layer ({}); asks first", Shortcut::DeleteLayer.label()))
            .clicked()
        {
            session.actions.push(Action::DeleteLayer(index));
        }
    });
    if session.view.is_filtering() {
        ui.colored_label(WARNING, "Some layers are hidden in the preview")
            .on_hover_text("👁 / S only change the preview; the saved and exported effect has every layer");
    }
}

fn layer_row(ui: &mut egui::Ui, project: &ProjectV2, session: &mut EditorSession, index: usize, layer: &LayerDef) {
    let name = layer.name().to_string();
    let selected = session.selected == Some(index);
    ui.horizontal(|ui| {
        let shows = session.view.shows(&name);
        let eye = RichText::new("👁");
        let eye = if session.view.is_hidden(&name) { eye.weak().strikethrough() } else { eye };
        if ui
            .selectable_label(!session.view.is_hidden(&name), eye)
            .on_hover_text("Show / hide this layer in the preview (not saved)")
            .clicked()
        {
            session.view.toggle_hidden(&name);
            session.view_changed = true;
        }
        let solo = session.view.is_solo(&name);
        let solo_text = if solo { RichText::new("S").color(ACCENT).strong() } else { RichText::new("S").weak() };
        if ui
            .selectable_label(solo, solo_text)
            .on_hover_text("Solo: preview only the soloed layers (not saved)")
            .clicked()
        {
            session.view.toggle_solo(&name);
            session.view_changed = true;
        }
        ui.add_space(depth(project, index) as f32 * 14.0);
        let icon = match layer {
            LayerDef::Particles(p) if p.parent.is_some() => "·✨",
            LayerDef::Particles(_) => "✨",
            LayerDef::Surface(_) => "🎨",
            LayerDef::Shape(_) => "🔷",
        };
        let mut text = RichText::new(format!("{} {}", icon, name));
        if !shows {
            text = text.weak();
        }
        let response = ui.selectable_label(selected, text).on_hover_text(match layer {
            LayerDef::Particles(p) => match &p.parent {
                Some(parent) => format!("Particles layer, spawned by \"{}\". Right-click for more.", parent),
                None => "Particles layer. Right-click for more.".to_string(),
            },
            LayerDef::Surface(s) => format!(
                "Surface layer: effect shader over the target's meshes, {} shell(s). Right-click for more.",
                s.shells.len()
            ),
            LayerDef::Shape(s) => format!("Shape layer: {}. Right-click for more.", s.shape.describe()),
        });
        if response.clicked() {
            session.actions.push(Action::SelectLayer(index));
        }
        response.context_menu(|ui| {
            if ui.button("🗐 Duplicate").clicked() {
                session.actions.push(Action::DuplicateLayer(index));
                ui.close();
            }
            if ui.add_enabled(index > 0, egui::Button::new("⬆ Move up")).clicked() {
                session.actions.push(Action::MoveLayer { from: index, to: index - 1 });
                ui.close();
            }
            if ui.add_enabled(index + 1 < project.layers.len(), egui::Button::new("⬇ Move down")).clicked() {
                session.actions.push(Action::MoveLayer { from: index, to: index + 1 });
                ui.close();
            }
            ui.separator();
            if ui.button("🗑 Delete…").clicked() {
                session.actions.push(Action::DeleteLayer(index));
                ui.close();
            }
        });
        let (errors, warnings) = session.diagnostics.count_about(&name);
        if let Some(badge) = badge(ui, errors, warnings) {
            badge.on_hover_text(subject_hover(session, &name));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use omagari::format::ParticlesLayerDef;

    #[test]
    fn depth_follows_parents_and_survives_cycles() {
        let layer = |name: &str, parent: Option<&str>| {
            LayerDef::Particles(ParticlesLayerDef {
                name: name.into(),
                parent: parent.map(str::to_string),
                ..Default::default()
            })
        };
        let mut project = ProjectV2::default();
        project.layers = vec![layer("a", None), layer("b", Some("a")), layer("c", Some("b")), layer("x", Some("x"))];
        assert_eq!(depth(&project, 0), 0);
        assert_eq!(depth(&project, 1), 1);
        assert_eq!(depth(&project, 2), 2);
        assert!(depth(&project, 3) <= project.layers.len(), "a layer that is its own parent ends");
    }
}
