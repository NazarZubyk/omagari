//! Showing the checks of the open project (`check_project`): badges in the
//! layers list, lists in the inspector and the Problems window.

use bevy_egui::egui::{self, Color32, RichText};
use omagari::format::ProjectV2;

use super::actions::Action;
use super::session::{EditorSession, RightTab};
use crate::mcp::analysis::{split_subject, Severity};

pub const ACCENT: Color32 = Color32::from_rgb(0x88, 0xAA, 0xFF);
pub const WARNING: Color32 = Color32::from_rgb(0xFF, 0xCC, 0x66);
pub const ERROR: Color32 = Color32::from_rgb(0xFF, 0x77, 0x66);
pub const NOTE: Color32 = Color32::from_rgb(0xA8, 0xA8, 0xA8);

pub fn severity_icon(severity: Severity) -> RichText {
    match severity {
        Severity::Error => RichText::new("⛔").color(ERROR),
        Severity::Warning => RichText::new("⚠").color(WARNING),
        Severity::Note => RichText::new("ℹ").color(NOTE),
    }
}

/// `⛔2 ⚠1`-style badge; nothing when both are 0. Returns the response of
/// the badge for a tooltip.
pub fn badge(ui: &mut egui::Ui, errors: usize, warnings: usize) -> Option<egui::Response> {
    if errors == 0 && warnings == 0 {
        return None;
    }
    let mut text = egui::text::LayoutJob::default();
    let font = egui::TextStyle::Body.resolve(ui.style());
    if errors > 0 {
        text.append(&format!("⛔{} ", errors), 0.0, egui::TextFormat::simple(font.clone(), ERROR));
    }
    if warnings > 0 {
        text.append(&format!("⚠{}", warnings), 0.0, egui::TextFormat::simple(font, WARNING));
    }
    Some(ui.label(text))
}

/// A wrapped list of messages with their icons.
pub fn message_list<'a>(ui: &mut egui::Ui, messages: impl Iterator<Item = (Severity, &'a str)>) -> usize {
    let mut shown = 0;
    for (severity, message) in messages {
        ui.horizontal_wrapped(|ui| {
            ui.label(severity_icon(severity));
            ui.label(message);
        });
        shown += 1;
    }
    shown
}

/// The messages about one subject in a frame, when there are any.
pub fn subject_box(ui: &mut egui::Ui, session: &EditorSession, subject: &str) {
    if session.diagnostics.about(subject).next().is_none() {
        return;
    }
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_width(ui.available_width());
        message_list(ui, session.diagnostics.about(subject));
    });
}

/// Hover text listing the messages about a subject.
pub fn subject_hover(session: &EditorSession, subject: &str) -> String {
    let lines: Vec<String> = session
        .diagnostics
        .about(subject)
        .map(|(severity, message)| {
            let tag = match severity {
                Severity::Error => "Error",
                Severity::Warning => "Warning",
                Severity::Note => "Note",
            };
            format!("{}: {}", tag, message)
        })
        .collect();
    lines.join("\n")
}

/// Every message of the last check. Clicking one about a layer selects it;
/// socket/fit ones open the SOCKETS & FIT tab.
pub fn problems_window(ctx: &egui::Context, project: &ProjectV2, session: &mut EditorSession) {
    let mut open = session.problems_open;
    let mut go: Option<Action> = None;
    let mut tab: Option<RightTab> = None;
    egui::Window::new("Problems")
        .open(&mut open)
        .pivot(egui::Align2::CENTER_BOTTOM)
        .default_pos(ctx.content_rect().center_bottom() - egui::vec2(0.0, 60.0))
        .default_width(520.0)
        .default_height(300.0)
        .resizable(true)
        .collapsible(false)
        .show(ctx, |ui| {
            ui.label("What the checks found (the same checks Claude runs through MCP). Click a line to go there.");
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| {
                let mut any = false;
                for (severity, message) in session.diagnostics.all() {
                    any = true;
                    let (subject, text) = split_subject(message);
                    let response = ui
                        .horizontal_wrapped(|ui| {
                            ui.label(severity_icon(severity));
                            if let Some(subject) = subject {
                                ui.label(RichText::new(subject).strong());
                            }
                            ui.label(text);
                        })
                        .response
                        .interact(egui::Sense::click());
                    if response.clicked() {
                        match subject {
                            Some(s) if s.starts_with("socket ") => tab = Some(RightTab::SocketsFit),
                            Some(s) if s.starts_with("asset ") => tab = Some(RightTab::Assets),
                            Some(s) => {
                                if let Some(i) = project.layers.iter().position(|l| l.name() == s) {
                                    go = Some(Action::SelectLayer(i));
                                }
                            }
                            None if text.contains("fit") => tab = Some(RightTab::SocketsFit),
                            None => {}
                        }
                    }
                }
                if !any {
                    ui.label(RichText::new("✔ No problems found.").color(ACCENT));
                }
            });
        });
    session.problems_open = open;
    session.actions.extend(go);
    if let Some(tab) = tab {
        session.tab = tab;
    }
}
