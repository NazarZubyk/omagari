//! Editor widgets for package-level settings: sockets, size fit, and the
//! preview motion.

use bevy::prelude::*;
use bevy_egui::*;
use omagari::format::*;

use super::helpers::*;

/// The preview motion of the open project (saved in its `preview` block).
pub fn motion_menu(ui: &mut egui::Ui, project: &mut ProjectV2) {
    ui.set_min_width(300.0);
    let current = project.preview.as_ref().and_then(|p| p.motion);
    let speed = current.map(|m| m.speed()).unwrap_or(DEFAULT_MOTION_SPEED);
    let mut motion = current;
    ui.horizontal(|ui| {
        if ui.radio(motion.is_none(), "Off").on_hover_text("The target stands still").clicked() {
            motion = None;
        }
        let circle = matches!(motion, Some(PreviewMotion::Circle { .. }));
        if ui.radio(circle, "Circle").on_hover_text("Runs in a circle around the origin").clicked() && !circle {
            motion = Some(PreviewMotion::Circle { radius: 3.0, speed });
        }
        let back = matches!(motion, Some(PreviewMotion::BackAndForth { .. }));
        if ui.radio(back, "Back & forth").on_hover_text("Runs back and forth along X").clicked() && !back {
            motion = Some(PreviewMotion::BackAndForth { distance: 4.0, speed });
        }
    });
    match &mut motion {
        None => {}
        Some(PreviewMotion::Circle { radius, speed }) => {
            ui.horizontal(|ui| {
                ui.label("Radius:");
                *radius = ui_for_f32_ex(ui, *radius, 0.1, 50.0, 0.05);
                ui.label("m");
            });
            speed_row(ui, speed);
        }
        Some(PreviewMotion::BackAndForth { distance, speed }) => {
            ui.horizontal(|ui| {
                ui.label("Distance:");
                *distance = ui_for_f32_ex(ui, *distance, 0.1, 50.0, 0.05);
                ui.label("m");
            });
            speed_row(ui, speed);
        }
    }
    ui.label("Local layers move with the target; Global layers leave a trail.");
    if motion != current {
        project.preview.get_or_insert_with(PreviewDef::default).motion = motion;
    }
}

fn speed_row(ui: &mut egui::Ui, speed: &mut f32) {
    ui.horizontal(|ui| {
        ui.label("Speed:");
        *speed = ui_for_f32_ex(ui, *speed, 0.0, 50.0, 0.05);
        ui.label("m/s");
        if ui.small_button("run").on_hover_text("3.6 m/s, a running character").clicked() {
            *speed = DEFAULT_MOTION_SPEED;
        }
    });
}

/// The package's sockets (named points on the target) and its size fit.
pub fn sockets_and_fit_menu(ui: &mut egui::Ui, project: &mut ProjectV2) {
    ui.set_min_width(460.0);
    ui.label("Fit (scales the instance to its target; Local layers only):");
    ui.horizontal(|ui| {
        let fit = project.fit;
        if ui.radio(fit.is_none(), "None").on_hover_text("Authored size, whatever the target's scale").clicked() {
            project.fit = None;
        }
        if ui
            .radio(fit == Some(FitDef::TargetScale), "Target scale")
            .on_hover_text("Scales with the target entity's scale")
            .clicked()
        {
            project.fit = Some(FitDef::TargetScale);
        }
        let height = matches!(fit, Some(FitDef::TargetHeight(_)));
        if ui
            .radio(height, "Target height")
            .on_hover_text("Scaled by the height of the target's meshes / the reference height the effect was made for")
            .clicked()
            && !height
        {
            project.fit = Some(FitDef::TargetHeight(1.8));
        }
        if let Some(FitDef::TargetHeight(reference)) = &mut project.fit {
            ui.label("made for");
            *reference = ui_for_f32_ex(ui, *reference, 0.01, 100.0, 0.01);
            ui.label("m");
        }
    });
    ui.separator();
    ui.label("Sockets (attach layers with Socket / Segment):")
        .on_hover_text("A node name of the target's model (or none = the target root) plus an offset in metres");
    let mut remove = None;
    for (index, socket) in project.sockets.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            if ui.small_button("❌").on_hover_text("Remove").clicked() {
                remove = Some(index);
            }
            egui::TextEdit::singleline(&mut socket.name)
                .hint_text("name")
                .desired_width(90.0)
                .show(ui);
            let mut node = socket.node.clone().unwrap_or_default();
            ui.add(
                egui::TextEdit::singleline(&mut node)
                    .hint_text("node (empty = target root)")
                    .desired_width(150.0),
            )
            .on_hover_text("Name of an entity under the target, e.g. a bone; empty = the target root");
            socket.node = (!node.is_empty()).then_some(node);
            socket.offset = ui_for_vec3(ui, socket.offset);
        });
    }
    if let Some(index) = remove {
        project.sockets.remove(index);
    }
    if ui.button("+ socket").clicked() {
        let mut name = "socket".to_string();
        let mut n = 2;
        while project.socket(&name).is_some() {
            name = format!("socket{}", n);
            n += 1;
        }
        project.sockets.push(SocketDef {
            name,
            node: None,
            offset: Vec3::ZERO,
        });
    }
    ui.label("The editor's preview target has no model: sockets with a node sit at the target root.");
}
