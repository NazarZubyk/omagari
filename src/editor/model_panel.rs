//! The model stage's UI: the PREVIEW tab's model, animation, target and
//! weapon settings, the HIERARCHY tab (the model's nodes, bones, meshes and
//! materials with search, and actions to use a node), the bottom bar's
//! animation controls and the socket labels drawn over the viewport. Every
//! setting is a `ProjectV2` edit of the preview block (undo, unsaved marker).

use bevy::prelude::*;
use bevy_egui::egui::{self, RichText};
use omagari::format::*;

use super::helpers::ui_for_vec3;
use super::hints;
use super::model::{model_info, ModelPreview, PLAYER_ROOT_HEIGHT};
use super::problems::{ACCENT, ERROR, NOTE, WARNING};
use super::session::EditorSession;
use super::AppContext;
use crate::model_info::ModelInfo;
use crate::settings::GameAssets;

/// A combo box of the game folder's model files; `None` = `none`.
fn model_combo(ui: &mut egui::Ui, id: &str, current: &mut Option<String>, files: &[String], none: &str) -> bool {
    let mut changed = false;
    egui::ComboBox::from_id_salt(id)
        .width(230.0)
        .selected_text(current.clone().unwrap_or_else(|| none.to_string()))
        .show_ui(ui, |ui| {
            if ui.selectable_label(current.is_none(), none).clicked() {
                *current = None;
                changed = true;
            }
            for file in files {
                if ui.selectable_label(current.as_deref() == Some(file.as_str()), file).clicked() {
                    *current = Some(file.clone());
                    changed = true;
                }
            }
        });
    changed
}

/// A node picker over a model's nodes (bones first marked); `None` = `none`.
fn node_combo(ui: &mut egui::Ui, id: &str, current: &mut Option<String>, info: Option<&ModelInfo>, none: &str) -> bool {
    let mut changed = false;
    egui::ComboBox::from_id_salt(id)
        .width(230.0)
        .height(400.0)
        .selected_text(current.clone().unwrap_or_else(|| none.to_string()))
        .show_ui(ui, |ui| {
            if ui.selectable_label(current.is_none(), none).clicked() {
                *current = None;
                changed = true;
            }
            if let Some(info) = info {
                for (i, node) in info.nodes.iter().enumerate() {
                    let label = format!("{}{}", node.name, kind_badge(info, i));
                    if ui.selectable_label(current.as_deref() == Some(node.name.as_str()), label).clicked() {
                        *current = Some(node.name.clone());
                        changed = true;
                    }
                }
            }
        });
    changed
}

fn kind_badge(info: &ModelInfo, index: usize) -> &'static str {
    match info.kind(index) {
        "bone" => "  · bone",
        "mesh" => "  · mesh",
        _ => "",
    }
}

/// The model the instance target is on (body or weapon), described.
pub fn target_model_info(project: &ProjectV2, game_assets: &GameAssets) -> Option<std::sync::Arc<ModelInfo>> {
    let preview = project.preview.as_ref()?;
    let path = match preview.target {
        PreviewTargetModel::Weapon => &preview.weapon.as_ref()?.model,
        PreviewTargetModel::Body => preview.model.as_ref().filter(|_| preview.stage == PreviewStage::Model)?,
    };
    model_info(game_assets, path).ok()
}

/// The PREVIEW tab's model section.
pub fn model_section(ui: &mut egui::Ui, project: &mut ProjectV2, model: &mut ModelPreview, game_assets: &GameAssets) {
    let stage = project.preview.as_ref().map(|p| p.stage).unwrap_or_default();
    ui.horizontal(|ui| {
        ui.strong("Model").on_hover_text(hints::MODEL);
        if ui.small_button("↻").on_hover_text("Look for model files in the game assets folder again").clicked() {
            model.files = None;
        }
    });
    if !matches!(stage, PreviewStage::Capsule | PreviewStage::Model) {
        ui.label(RichText::new("Pick the CAPSULE or MODEL stage to preview the effect on a character.").weak());
    }
    let files = model.model_files(game_assets).to_vec();
    let mut preview = project.preview.clone().unwrap_or_default();
    egui::Grid::new("model grid").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
        ui.label("Model file").on_hover_text(hints::MODEL);
        ui.horizontal(|ui| {
            if model_combo(ui, "model file", &mut preview.model, &files, "none (capsule)") && preview.model.is_some() {
                preview.stage = PreviewStage::Model;
            }
        });
        ui.end_row();

        ui.label("Root height").on_hover_text(hints::ROOT_HEIGHT);
        ui.horizontal(|ui| {
            ui.add(egui::DragValue::new(&mut preview.root_height).speed(0.01).range(-10.0..=10.0).suffix(" m"))
                .on_hover_text(hints::ROOT_HEIGHT);
            if ui.small_button(format!("{} (player root)", PLAYER_ROOT_HEIGHT)).on_hover_text(hints::ROOT_HEIGHT).clicked() {
                preview.root_height = PLAYER_ROOT_HEIGHT;
            }
            if ui.small_button("0 (feet)").clicked() {
                preview.root_height = 0.0;
            }
        });
        ui.end_row();

        // Animation: a clip of the model's file or of another file.
        let body_info = preview.model.as_deref().and_then(|m| model_info(game_assets, m).ok());
        ui.label("Animation file").on_hover_text(hints::ANIMATION_FILE);
        ui.horizontal(|ui| {
            let mut file = preview.animation_file.clone();
            if model_combo(ui, "animation file", &mut file, &files, "the model's file") {
                preview.animation_file = file;
                preview.animation = None;
            }
        });
        ui.end_row();

        ui.label("Clip").on_hover_text(hints::ANIMATION_CLIP);
        let source_info = preview.animation_source().and_then(|f| model_info(game_assets, f).ok());
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("animation clip")
                .width(230.0)
                .selected_text(preview.animation.clone().unwrap_or_else(|| "none (rest pose)".into()))
                .show_ui(ui, |ui| {
                    if ui.selectable_label(preview.animation.is_none(), "none (rest pose)").clicked() {
                        preview.animation = None;
                    }
                    for clip in source_info.iter().flat_map(|i| i.animations.iter()) {
                        let label = clip.label();
                        let text = format!("{} ({:.2} s)", label, clip.duration);
                        if ui.selectable_label(preview.animation.as_deref() == Some(label.as_str()), text).clicked() {
                            preview.animation = Some(label);
                        }
                    }
                });
        });
        ui.end_row();
        if let (Some(body), Some(source), Some(clip)) =
            (&body_info, &source_info, preview.animation.as_deref().and_then(|c| source_info.as_ref()?.animation(c).cloned()))
        {
            if !std::sync::Arc::ptr_eq(body, source) {
                let matching = body.matching_targets(&clip);
                if matching < clip.targets.len() {
                    ui.label("");
                    ui.colored_label(
                        WARNING,
                        format!(
                            "{} of the clip's {} animated nodes are not in the model (names differ): they do not move",
                            clip.targets.len() - matching,
                            clip.targets.len()
                        ),
                    );
                    ui.end_row();
                }
            }
        }

        ui.label("Speed").on_hover_text("Playback speed of the clip (1 = as authored); saved with the preview");
        ui.add(egui::DragValue::new(&mut preview.animation_speed).speed(0.01).range(-5.0..=5.0).suffix("×"));
        ui.end_row();

        // Target.
        ui.label("Instance on").on_hover_text(hints::ATTACH_TO);
        ui.horizontal(|ui| {
            ui.radio_value(&mut preview.target, PreviewTargetModel::Body, "body")
                .on_hover_text("The character (the model or the capsule)");
            ui.add_enabled_ui(preview.weapon.is_some(), |ui| {
                ui.radio_value(&mut preview.target, PreviewTargetModel::Weapon, "weapon")
                    .on_hover_text("The weapon model: a separate model, body effects do not cover it");
            });
        });
        ui.end_row();
        ui.label("Attach to").on_hover_text(hints::ATTACH_TO);
        let target_info = match preview.target {
            PreviewTargetModel::Weapon => preview.weapon.as_ref().and_then(|w| model_info(game_assets, &w.model).ok()),
            PreviewTargetModel::Body => body_info.clone(),
        };
        node_combo(ui, "attach to", &mut preview.attach_to, target_info.as_deref(), "the model's root (target)");
        ui.end_row();
    });

    // Weapon.
    ui.add_space(4.0);
    let mut has_weapon = preview.weapon.is_some();
    if ui.checkbox(&mut has_weapon, RichText::new("Weapon").strong()).on_hover_text(hints::WEAPON).changed() {
        preview.weapon = has_weapon.then(|| PreviewWeaponDef {
            model: files.iter().find(|f| Some(*f) != preview.model.as_ref()).cloned().unwrap_or_default(),
            node: Some("mixamorig:RightHand".to_string()).filter(|n| {
                preview.model.as_deref().and_then(|m| model_info(game_assets, m).ok()).is_some_and(|i| i.node(n).is_some())
            }),
            ..default()
        });
        if !has_weapon {
            preview.target = PreviewTargetModel::Body;
        }
    }
    let body_info = preview.model.as_deref().and_then(|m| model_info(game_assets, m).ok());
    if let Some(weapon) = &mut preview.weapon {
        egui::Grid::new("weapon grid").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
            ui.label("File");
            let mut file = Some(weapon.model.clone()).filter(|m| !m.is_empty());
            if model_combo(ui, "weapon file", &mut file, &files, "none") {
                weapon.model = file.unwrap_or_default();
            }
            ui.end_row();
            ui.label("Held by").on_hover_text("The body node the weapon is attached to (a hand bone)");
            node_combo(ui, "weapon node", &mut weapon.node, body_info.as_deref(), "the target root");
            ui.end_row();
            ui.label("Offset (m)").on_hover_text("Metres from the node, in its axes (its scale is ignored)");
            weapon.offset = ui_for_vec3(ui, weapon.offset);
            ui.end_row();
            ui.label("Rotation (°)").on_hover_text("Degrees around the node's X, then Y, then Z");
            weapon.rotation = ui_for_vec3(ui, weapon.rotation);
            ui.end_row();
            ui.label("Scale");
            ui.add(egui::DragValue::new(&mut weapon.scale).speed(0.01).range(0.001..=100.0));
            ui.end_row();
        });
    }

    for problem in model.problems.iter() {
        ui.colored_label(ERROR, format!("⚠ {}", problem));
    }
    if let Some(info) = body_info.filter(|_| preview.shows_model()) {
        ui.label(RichText::new(format!("Model: {}", info.summary())).color(NOTE));
    }
    if Some(&preview) != project.preview.as_ref() && (project.preview.is_some() || preview != PreviewDef::default()) {
        project.preview = Some(preview);
    }
}

/// The bottom bar's animation controls, while a clip plays.
pub fn animation_controls(ui: &mut egui::Ui, project: &mut ProjectV2, model: &mut ModelPreview) {
    let Some(clip) = model.clip.as_ref() else { return };
    let name = clip.key.1.clone();
    ui.separator();
    ui.label(RichText::new(format!("🏃 {}", name)).color(ACCENT))
        .on_hover_text(format!("Animation \"{}\" from {}", name, clip.key.0));
    if ui
        .selectable_label(model.paused, if model.paused { "⏵" } else { "⏸" })
        .on_hover_text("Play / pause the animation only (the effect keeps playing)")
        .clicked()
    {
        model.paused = !model.paused;
    }
    ui.checkbox(&mut model.looping, "loop").on_hover_text("Play the clip again and again; off = stop at its end");
    let mut time = model.time;
    let slider = ui
        .add(egui::Slider::new(&mut time, 0.0..=model.duration.max(0.001)).show_value(false))
        .on_hover_text("The clip time: drag to scrub (pauses the animation)");
    // Only a user's drag or click scrubs.
    if slider.changed() && (slider.dragged() || slider.clicked()) {
        model.seek = Some(time);
        model.paused = true;
    }
    ui.label(format!("{:.2} / {:.2} s", model.time, model.duration));
    if let Some(preview) = &mut project.preview {
        ui.add(egui::DragValue::new(&mut preview.animation_speed).speed(0.01).range(-5.0..=5.0).suffix("×"))
            .on_hover_text("Clip speed (saved with the preview)");
    }
}

/// The HIERARCHY tab: the nodes of the body or weapon model as a tree (or a
/// filtered list while searching), with actions per node, and the meshes
/// and materials.
pub fn hierarchy_tab(
    ui: &mut egui::Ui,
    project: &mut ProjectV2,
    session: &mut EditorSession,
    model: &mut ModelPreview,
    app: &mut AppContext,
    game_assets: &GameAssets,
) {
    let preview = project.preview.clone().unwrap_or_default();
    let weapon = preview.weapon.as_ref().map(|w| w.model.clone()).filter(|m| !m.is_empty());
    ui.horizontal(|ui| {
        ui.label("Model:");
        ui.radio_value(&mut session.hierarchy_weapon, false, preview.model.as_deref().unwrap_or("none"));
        ui.add_enabled_ui(weapon.is_some(), |ui| {
            ui.radio_value(&mut session.hierarchy_weapon, true, format!("weapon: {}", weapon.as_deref().unwrap_or("none")));
        });
    });
    if weapon.is_none() {
        session.hierarchy_weapon = false;
    }
    let path = if session.hierarchy_weapon { weapon.clone() } else { preview.model.clone() };
    let Some(path) = path else {
        ui.add_space(8.0);
        ui.label(RichText::new("No model: pick one in the PREVIEW tab (MODEL stage).").weak());
        return;
    };
    let info = match model_info(game_assets, &path) {
        Ok(info) => info,
        Err(e) => {
            ui.colored_label(ERROR, e);
            return;
        }
    };
    ui.label(RichText::new(info.summary()).color(NOTE));
    ui.horizontal(|ui| {
        ui.label("🔍");
        ui.add(egui::TextEdit::singleline(&mut session.hierarchy_search).hint_text("search names").desired_width(200.0));
        if ui.small_button("✖").clicked() {
            session.hierarchy_search.clear();
        }
    });
    ui.label(RichText::new(hints::HIERARCHY_ACTIONS).weak().small());
    ui.separator();

    let selected_layer = session.selected.filter(|&i| project.layers.get(i).and_then(|l| l.as_particles()).is_some());
    let mut action: Option<(NodeAction, String)> = None;
    let search = session.hierarchy_search.trim().to_lowercase();
    let mut row = |ui: &mut egui::Ui, i: usize, action: &mut Option<(NodeAction, String)>| {
        let node = &info.nodes[i];
        ui.horizontal(|ui| {
            let attached = preview.attach_to.as_deref() == Some(node.name.as_str())
                && (preview.target == PreviewTargetModel::Weapon) == session.hierarchy_weapon;
            let text = RichText::new(format!("{}{}", node.name, kind_badge(&info, i)));
            ui.label(if attached { text.color(ACCENT).strong() } else { text })
                .on_hover_text(format!("{} — {}", info.kind(i), info.path(i)));
            if ui.small_button("🎯").on_hover_text(hints::NODE_TARGET).clicked() {
                *action = Some((NodeAction::Target, node.name.clone()));
            }
            if ui.small_button("⚓+").on_hover_text(hints::NODE_SOCKET).clicked() {
                *action = Some((NodeAction::Socket, node.name.clone()));
            }
            ui.add_enabled_ui(selected_layer.is_some(), |ui| {
                if ui.small_button("A").on_hover_text(hints::NODE_SEGMENT).clicked() {
                    *action = Some((NodeAction::SegmentFrom, node.name.clone()));
                }
                if ui.small_button("B").on_hover_text(hints::NODE_SEGMENT).clicked() {
                    *action = Some((NodeAction::SegmentTo, node.name.clone()));
                }
            });
        });
    };
    if search.is_empty() {
        fn tree(
            ui: &mut egui::Ui,
            info: &ModelInfo,
            i: usize,
            depth: usize,
            row: &mut dyn FnMut(&mut egui::Ui, usize, &mut Option<(NodeAction, String)>),
            action: &mut Option<(NodeAction, String)>,
        ) {
            let node = &info.nodes[i];
            if node.children.is_empty() {
                row(ui, i, action);
                return;
            }
            let id = ui.make_persistent_id(("hierarchy", i));
            egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, depth < 3)
                .show_header(ui, |ui| row(ui, i, action))
                .body(|ui| {
                    for &c in node.children.iter() {
                        tree(ui, info, c, depth + 1, row, action);
                    }
                });
        }
        for &root in info.roots.iter() {
            tree(ui, &info, root, 0, &mut row, &mut action);
        }
    } else {
        let mut shown = 0;
        for i in 0..info.nodes.len() {
            if info.nodes[i].name.to_lowercase().contains(&search) {
                row(ui, i, &mut action);
                shown += 1;
            }
        }
        if shown == 0 {
            ui.label(RichText::new("No node matches.").weak());
        }
    }

    ui.separator();
    ui.strong("Meshes and materials").on_hover_text(hints::MESHES);
    for mesh in info.meshes.iter() {
        let matches = |s: &str| search.is_empty() || s.to_lowercase().contains(&search);
        for p in mesh.primitives.iter().filter(|p| matches(&p.entity_name)) {
            ui.label(format!(
                "▣ {}   (mesh {}, material {})",
                p.entity_name,
                mesh.name,
                p.material.as_deref().unwrap_or("none")
            ));
        }
    }

    if let Some((what, node)) = action {
        apply_node_action(project, session, model, app, what, &node, session.hierarchy_weapon);
    }
}

/// What a hierarchy row's button does with its node.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum NodeAction {
    /// Use as `preview.attach_to`.
    Target,
    /// Create a package socket on it.
    Socket,
    /// Segment start / end of the selected layer.
    SegmentFrom,
    SegmentTo,
}

/// Applies a hierarchy action to the project.
pub fn apply_node_action(
    project: &mut ProjectV2,
    session: &mut EditorSession,
    model: &mut ModelPreview,
    app: &mut AppContext,
    what: NodeAction,
    node: &str,
    on_weapon: bool,
) {
    match what {
        NodeAction::Target => {
            let preview = project.preview.get_or_insert_with(PreviewDef::default);
            preview.attach_to = Some(node.to_string());
            preview.target = if on_weapon { PreviewTargetModel::Weapon } else { PreviewTargetModel::Body };
            app.status = Some(format!("The preview instance is attached to \"{}\"", node));
        }
        NodeAction::Socket => {
            let name = new_socket_for(project, node);
            app.status = Some(format!("Added socket \"{}\" on node \"{}\" (SOCKETS & FIT tab)", name, node));
            model.selected_socket = Some(name);
        }
        NodeAction::SegmentFrom | NodeAction::SegmentTo => {
            let Some(index) = session.selected else { return };
            let socket = project
                .sockets
                .iter()
                .find(|s| s.node.as_deref() == Some(node))
                .map(|s| s.name.clone())
                .unwrap_or_else(|| node.to_string());
            let Some(layer) = project.layers.get_mut(index).and_then(|l| l.as_particles_mut()) else { return };
            let (from, to) = match &layer.attach {
                AttachDef::Segment { from, to } => (from.clone(), to.clone()),
                _ => (socket.clone(), socket.clone()),
            };
            layer.attach = if what == NodeAction::SegmentFrom {
                AttachDef::Segment { from: socket.clone(), to }
            } else {
                AttachDef::Segment { from, to: socket.clone() }
            };
            app.status = Some(format!(
                "Layer \"{}\": segment {} is \"{}\"",
                layer.name,
                if what == NodeAction::SegmentFrom { "start" } else { "end" },
                socket
            ));
        }
    }
}

/// Adds a package socket on `node` (offset 0) named after it (the part
/// after a `prefix:`, unique) and returns its name.
pub fn new_socket_for(project: &mut ProjectV2, node: &str) -> String {
    let base = node.rsplit(':').next().unwrap_or(node).trim();
    let base = if base.is_empty() { "socket" } else { base };
    let mut name = base.to_string();
    let mut n = 2;
    while project.socket(&name).is_some() {
        name = format!("{}{}", base, n);
        n += 1;
    }
    project.sockets.push(SocketDef { name: name.clone(), node: Some(node.to_string()), offset: Vec3::ZERO });
    name
}

/// Socket names over their dots in the viewport.
pub fn paint_socket_labels(painter: &egui::Painter, model: &ModelPreview) {
    if !model.show_gizmos {
        return;
    }
    for m in model.markers.iter() {
        let Some(screen) = m.screen else { continue };
        let pos = egui::pos2(screen.x + 7.0, screen.y - 7.0);
        if !painter.clip_rect().contains(pos) {
            continue;
        }
        let selected = model.selected_socket.as_deref() == Some(m.name.as_str());
        let color = if m.pending {
            ERROR
        } else if selected {
            egui::Color32::from_rgb(0xFF, 0xE6, 0x33)
        } else {
            egui::Color32::from_rgb(0x9E, 0xE8, 0xFF)
        };
        let text = if m.pending { format!("{} (node not found)", m.name) } else { m.name.clone() };
        let galley = painter.layout_no_wrap(text, egui::FontId::proportional(12.0), color);
        let rect = egui::Align2::LEFT_BOTTOM.anchor_size(pos, galley.size()).expand(2.0);
        painter.rect_filled(rect, 3.0, egui::Color32::from_black_alpha(150));
        painter.galley(rect.min + egui::vec2(2.0, 2.0), galley, color);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hierarchy_actions_edit_the_project() {
        let mut project = ProjectV2::default();
        project.layers.push(LayerDef::Particles(ParticlesLayerDef::default()));
        let mut session = EditorSession::default();
        session.selected = Some(0);
        let mut model = ModelPreview::default();
        let mut app = AppContext::default();
        let mut act = |project: &mut ProjectV2, what, node: &str, weapon| {
            apply_node_action(project, &mut session, &mut model, &mut app, what, node, weapon)
        };

        act(&mut project, NodeAction::Socket, "mixamorig:RightHand", false);
        act(&mut project, NodeAction::Socket, "mixamorig:RightHand", false);
        let names: Vec<&str> = project.sockets.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["RightHand", "RightHand2"]);
        assert_eq!(project.sockets[0].node.as_deref(), Some("mixamorig:RightHand"));

        act(&mut project, NodeAction::Target, "Grip", true);
        let preview = project.preview.as_ref().unwrap();
        assert_eq!((preview.attach_to.as_deref(), preview.target), (Some("Grip"), PreviewTargetModel::Weapon));

        // Segment ends: the package socket on the node, else the node name.
        act(&mut project, NodeAction::SegmentFrom, "mixamorig:RightHand", false);
        act(&mut project, NodeAction::SegmentTo, "Tip", false);
        assert_eq!(
            project.layers[0].as_particles().unwrap().attach,
            AttachDef::Segment { from: "RightHand".into(), to: "Tip".into() }
        );
    }
}
