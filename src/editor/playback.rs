//! The bottom bar: play (RENDER), stop, restart, live preview, playback
//! speed, the problems of the project and the last status message.

use bevy::prelude::*;
use bevy_egui::egui::{self, RichText};

use super::actions::Action;
use super::problems::{badge, ERROR, NOTE, WARNING};
use super::session::EditorSession;
use super::shortcuts::Shortcut;
use super::AppContext;
use crate::controller::PreviewState;

/// Playback speeds offered next to the slider.
const SPEEDS: [f32; 4] = [0.1, 0.25, 0.5, 1.0];

pub fn playback_bar(
    ui: &mut egui::Ui,
    session: &mut EditorSession,
    preview_state: &PreviewState,
    time: &mut Time<Virtual>,
    app: &AppContext,
) {
    ui.add_space(3.0);
    ui.horizontal_wrapped(|ui| {
        if ui
            .button("⏵ RENDER")
            .on_hover_text(format!(
                "Rebuild the effect from the project and play it ({}). Live preview does this after every edit.",
                Shortcut::Render.label()
            ))
            .clicked()
        {
            session.actions.push(Action::Render);
        }
        if ui
            .button("⏹ STOP")
            .on_hover_text("Stop emitting, the way a game stops an effect: live particles finish their life")
            .clicked()
        {
            session.actions.push(Action::Stop);
        }
        if ui
            .button("⏮ RESTART")
            .on_hover_text("Clear every particle and play the effect again from the start")
            .clicked()
        {
            session.actions.push(Action::Restart);
        }
        ui.checkbox(&mut session.live_preview, "Live")
            .on_hover_text("Live preview: rebuild the effect a moment after each edit. Off = only RENDER rebuilds.");
        ui.separator();

        let paused = time.is_paused();
        if ui
            .selectable_label(paused, "⏸")
            .on_hover_text("Pause / resume the preview (particles and target motion)")
            .clicked()
        {
            if paused {
                time.unpause();
            } else {
                time.pause();
            }
        }
        let mut speed = time.relative_speed();
        ui.label("Speed").on_hover_text("Playback speed of the preview: 1 = real time, 0.25 = four times slower");
        let slider = ui.add(
            egui::Slider::new(&mut speed, 0.05..=2.0)
                .logarithmic(true)
                .max_decimals(2)
                .suffix("×"),
        );
        for s in SPEEDS {
            if ui.small_button(format!("{}×", s)).clicked() {
                speed = s;
            }
        }
        if (slider.changed() || speed != time.relative_speed()) && speed > 0.0 {
            time.set_relative_speed(speed);
        }
        ui.separator();

        let d = &session.diagnostics;
        let (errors, warnings) = (d.errors.len(), d.warnings.len());
        let problems = match badge(ui, errors, warnings) {
            Some(response) => response,
            None => ui.label(RichText::new("✔ no problems").color(NOTE)),
        };
        let problems = problems.interact(egui::Sense::click()).on_hover_text(
            "Checks of the project (the ones Claude runs through MCP), updated as you edit. Click for the list.",
        );
        if problems.clicked() {
            session.problems_open = !session.problems_open;
        }
        if !preview_state.warnings.is_empty() {
            ui.colored_label(WARNING, format!("⚠ {} build", preview_state.warnings.len()))
                .on_hover_text(format!(
                    "Warnings from the last preview build:\n{}",
                    preview_state.warnings.join("\n")
                ));
        }
        if let Some(status) = &app.status {
            ui.separator();
            let failed = status.contains("failed") || status.contains("Cannot") || status.contains("not a usable");
            let text = RichText::new(status);
            ui.label(if failed { text.color(ERROR) } else { text });
        }
    });
    ui.add_space(3.0);
}
