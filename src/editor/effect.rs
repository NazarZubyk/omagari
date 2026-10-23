//! Editor widgets for a particle layer (`omagari::format::ParticlesLayerDef`)
//! and its particle effect.

use bevy_egui::*;
use omagari::format::*;

use super::binding::{binding_editor, float_param_picker};
use super::helpers::*;
use super::hints;
use super::modifiers::ModifierLabel;
use super::AppContext;

fn ui_for_modifiers_list<T, R>(
    app: &mut AppContext,
    ui: &mut egui::Ui,
    mut modifiers: &mut Vec<T>,
    label: &str,
    id: egui::Id,
    add_contents: impl FnOnce(&mut egui::Ui, &mut Vec<T>) -> R,
) where
    T: UiProvider,
{
    let id = id.with(label);
    let header = format!("{} ({})", label, modifiers.len());
    let response = egui::CollapsingHeader::new(header).id_salt(id).show(ui, |ui| {
        let n_modifiers = modifiers.len();
        for (index, n) in modifiers.iter_mut().enumerate() {
            let swap = ui
                .horizontal(|ui| {
                    if let Some(list_command) = ui_for_list_item(ui, index, n_modifiers) {
                        return Some(list_command);
                    } else {
                        n.draw_ui(app, ui, index as u64);
                    }
                    None
                })
                .inner;
            if let Some(swap) = swap {
                match swap {
                    ListCommand::Remove(i) => {
                        modifiers.remove(i);
                    }
                    ListCommand::Swap((a, b)) => {
                        modifiers.swap(a, b);
                    }
                }
                break;
            }
        }

        ui.menu_button("+", |ui| add_contents(ui, &mut modifiers));
    });
    response.header_response.on_hover_text(match label {
        "Init" => hints::INIT_LIST,
        "Update" => hints::UPDATE_LIST,
        _ => hints::RENDER_LIST,
    });
}

impl UiProvider for ModifierEditor {
    fn draw_ui(&mut self, app: &mut AppContext, ui: &mut egui::Ui, index: u64) {
        match self {
            ModifierEditor::SetPositionCircle(n) => n.draw_ui(app, ui, index),
            ModifierEditor::SetPositionSphere(n) => n.draw_ui(app, ui, index),
            ModifierEditor::SetVelocityCircle(n) => n.draw_ui(app, ui, index),
            ModifierEditor::SetVelocitySphere(n) => n.draw_ui(app, ui, index),
            ModifierEditor::SetVelocityTangent(n) => n.draw_ui(app, ui, index),
            ModifierEditor::SetAttribute(n) => n.draw_ui(app, ui, index),
            ModifierEditor::InheritAttribute(n) => n.draw_ui(app, ui, index),
            ModifierEditor::AccelModifier(n) => n.draw_ui(app, ui, index),
            ModifierEditor::LinearDragModifier(n) => n.draw_ui(app, ui, index),
            ModifierEditor::EmitSpawnEventModifier(n) => n.draw_ui(app, ui, index),
            ModifierEditor::ConformToSphereModifier(n) => n.draw_ui(app, ui, index),
        }
    }
}

impl UiProvider for RenderModifierEditor {
    fn draw_ui(&mut self, app: &mut AppContext, ui: &mut egui::Ui, index: u64) {
        match self {
            RenderModifierEditor::SizeOverLifetime(n) => n.draw_ui(app, ui, index),
            RenderModifierEditor::ColorOverLifetime(n) => n.draw_ui(app, ui, index),
            RenderModifierEditor::ParticleTexture(n) => n.draw_ui(app, ui, index),
            RenderModifierEditor::Orient(n) => n.draw_ui(app, ui, index),
        }
    }
}

impl UiProvider for ParticleTextureModifierEditor {
    fn draw_ui(&mut self, _app: &mut AppContext, ui: &mut egui::Ui, index: u64) {
        ui.horizontal(|ui| {
            ui.label(ParticleTextureModifierEditor::label()).on_hover_text(hints::TEXTURE_MAPPING);
            let id = ui.make_persistent_id(format!("sample mapping {}", index));
            egui::ComboBox::from_id_salt(id)
                .selected_text(sample_mapping_label(self.sample_mapping))
                .show_ui(ui, |ui| {
                    for mapping in [
                        ImageSampleMappingDef::ModulateOpacityFromR,
                        ImageSampleMappingDef::Modulate,
                        ImageSampleMappingDef::ModulateRGB,
                    ] {
                        ui.selectable_value(&mut self.sample_mapping, mapping, sample_mapping_label(mapping))
                            .on_hover_text(sample_mapping_hint(mapping));
                    }
                })
                .response
                .on_hover_text(sample_mapping_hint(self.sample_mapping));
        });
    }
}

fn sample_mapping_label(mapping: ImageSampleMappingDef) -> &'static str {
    match mapping {
        ImageSampleMappingDef::Modulate => "Modulate",
        ImageSampleMappingDef::ModulateRGB => "ModulateRGB",
        ImageSampleMappingDef::ModulateOpacityFromR => "ModulateOpacityFromR",
    }
}

fn sample_mapping_hint(mapping: ImageSampleMappingDef) -> &'static str {
    match mapping {
        ImageSampleMappingDef::Modulate => {
            "Particle colour × texture colour, including the texture's transparency (colour sprites)"
        }
        ImageSampleMappingDef::ModulateRGB => "Particle colour × texture colour; the texture's transparency is ignored",
        ImageSampleMappingDef::ModulateOpacityFromR => {
            "The texture's brightness (red channel) becomes the opacity: grey sprites tinted by the particle colour"
        }
    }
}

impl UiProvider for OrientModifierEditor {
    fn draw_ui(&mut self, _app: &mut AppContext, ui: &mut egui::Ui, index: u64) {
        ui.horizontal(|ui| {
            ui.label(OrientModifierEditor::label()).on_hover_text(hints::ORIENT);
            let id = ui.make_persistent_id(format!("orient mode {}", index));
            egui::ComboBox::from_id_salt(id)
                .selected_text(orient_mode_label(self.mode))
                .show_ui(ui, |ui| {
                    for mode in [
                        OrientModeDef::AlongVelocity,
                        OrientModeDef::ParallelCameraDepthPlane,
                        OrientModeDef::FaceCameraPosition,
                    ] {
                        ui.selectable_value(&mut self.mode, mode, orient_mode_label(mode))
                            .on_hover_text(orient_mode_hint(mode));
                    }
                })
                .response
                .on_hover_text(orient_mode_hint(self.mode));
        });
    }
}

fn orient_mode_label(mode: OrientModeDef) -> &'static str {
    match mode {
        OrientModeDef::AlongVelocity => "AlongVelocity",
        OrientModeDef::ParallelCameraDepthPlane => "ParallelCameraDepthPlane",
        OrientModeDef::FaceCameraPosition => "FaceCameraPosition",
    }
}

fn orient_mode_hint(mode: OrientModeDef) -> &'static str {
    match mode {
        OrientModeDef::AlongVelocity => {
            "Points along the direction of travel (sparks, streaks); particles that don't move may be invisible"
        }
        OrientModeDef::ParallelCameraDepthPlane => "Always flat to the screen (billboards): smoke, glows, most sprites",
        OrientModeDef::FaceCameraPosition => "Turned towards the camera's position; like flat-to-screen, differs at the screen edges",
    }
}

/// A spawner count or duration: one value, or a random range (`±` switches).
fn ui_for_cpu_value(ui: &mut egui::Ui, value: &mut CpuValueDef, max: f32, speed: f32) {
    match value {
        CpuValueDef::Single(v) => {
            *v = ui_for_f32_ex(ui, *v, 0.0, max, speed);
            let v = *v;
            if ui.small_button("±").on_hover_text("Random range").clicked() {
                *value = CpuValueDef::Uniform((v, v));
            }
        }
        CpuValueDef::Uniform((a, b)) => {
            *a = ui_for_f32_ex(ui, *a, 0.0, max, speed);
            ui.label("to");
            *b = ui_for_f32_ex(ui, *b, 0.0, max, speed);
            let a = *a;
            if ui.small_button("=").on_hover_text("Single value").clicked() {
                *value = CpuValueDef::Single(a);
            }
        }
    }
}

fn ui_for_spawner(ui: &mut egui::Ui, spawner: &mut SpawnerDef) {
    egui::Grid::new("spawner grid").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
        ui.label("Count").on_hover_text(hints::COUNT);
        ui.horizontal(|ui| ui_for_cpu_value(ui, &mut spawner.count, 10000.0, 1.0))
            .response
            .on_hover_text(hints::COUNT);
        ui.end_row();
        ui.label("Duration").on_hover_text(hints::DURATION);
        ui.horizontal(|ui| {
            ui_for_cpu_value(ui, &mut spawner.spawn_duration, 10000.0, 0.01);
            ui.label("s");
        })
        .response
        .on_hover_text(hints::DURATION);
        ui.end_row();
        ui.label("Period").on_hover_text(hints::PERIOD);
        ui.horizontal(|ui| {
            ui_for_cpu_value(ui, &mut spawner.period, 10000.0, 0.01);
            ui.label("s");
        })
        .response
        .on_hover_text(hints::PERIOD);
        ui.end_row();
        ui.label("Cycles").on_hover_text(hints::CYCLES);
        ui.horizontal(|ui| {
            spawner.cycle_count = ui_for_u32_ex(ui, spawner.cycle_count, 0, 10000, 1);
            if spawner.cycle_count == 0 {
                ui.label("(forever)");
            }
        })
        .response
        .on_hover_text(hints::CYCLES);
        ui.end_row();
    });
    ui.horizontal(|ui| {
        ui.checkbox(&mut spawner.starts_active, "Starts active").on_hover_text(hints::STARTS_ACTIVE);
        ui.checkbox(&mut spawner.emit_on_start, "Emit on start").on_hover_text(hints::EMIT_ON_START);
    });
}

/// Asset id a built-in texture gets when picked (same as v1 migration).
fn builtin_texture_id(file: &str) -> &str {
    file.strip_suffix(".png").unwrap_or(file)
}

fn texture_path(asset: &AssetDef) -> &str {
    match &asset.kind {
        AssetKind::Texture(t) => &t.path,
    }
}

/// Texture picker: the project's texture assets plus the built-ins; picking a
/// built-in that is not in `assets` yet adds it.
fn ui_for_texture(ui: &mut egui::Ui, texture: &mut Option<String>, assets: &mut Vec<AssetDef>, index: u64) {
    let id = ui.make_persistent_id(format!("layer texture {}", index));
    let selected = texture.clone().unwrap_or_else(|| "NONE".to_string());
    let mut picked: Option<Option<String>> = None;
    let mut add: Option<AssetDef> = None;
    egui::ComboBox::from_id_salt(id)
        .selected_text(selected)
        .show_ui(ui, |ui| {
            if ui.selectable_label(texture.is_none(), "NONE").clicked() {
                picked = Some(None);
            }
            // The project's own (non built-in) textures.
            for asset in assets.iter() {
                let path = texture_path(asset);
                if path.starts_with(BUILTIN_TEXTURES_PREFIX) {
                    continue;
                }
                let current = texture.as_deref() == Some(asset.id.as_str());
                if ui.selectable_label(current, &asset.id).on_hover_text(path).clicked() {
                    picked = Some(Some(asset.id.clone()));
                }
            }
            ui.separator();
            for builtin in PARTICLE_TEXTURES.iter() {
                let path = builtin_texture_path(builtin.filename);
                // A project may already have the built-in under another id.
                let existing = assets.iter().find(|a| texture_path(a) == path);
                let asset_id = existing
                    .map(|a| a.id.clone())
                    .unwrap_or_else(|| builtin_texture_id(builtin.filename).to_string());
                let current = texture.as_deref() == Some(asset_id.as_str());
                if ui
                    .selectable_label(current, format!("{} ({})", builtin.ui_label, asset_id))
                    .on_hover_text(&path)
                    .clicked()
                {
                    if existing.is_none() && !assets.iter().any(|a| a.id == asset_id) {
                        add = Some(AssetDef {
                            id: asset_id.clone(),
                            kind: AssetKind::Texture(TextureAssetDef::new(path, TexturePurpose::Sprite)),
                        });
                    }
                    picked = Some(Some(asset_id));
                }
            }
        });
    if let Some(asset) = add {
        assets.push(asset);
    }
    if let Some(picked) = picked {
        *texture = picked;
    }
    if let Some(id) = texture.as_ref() {
        if !assets.iter().any(|a| &a.id == id) {
            ui.colored_label(egui::Color32::from_hex("#FF8866").unwrap(), "⚠ not in assets")
                .on_hover_text("This texture id has no entry in the project's assets; the layer renders untextured.");
        }
    }
}

fn play_label(play: &PlayDef) -> &'static str {
    match play {
        PlayDef::Loop => "Loop",
        PlayDef::OnStart => "OnStart",
        PlayDef::OnTrigger(_) => "OnTrigger",
        PlayDef::While(..) => "While",
    }
}

/// The play mode: combo, plus the trigger or the parameter and threshold.
fn ui_for_play(ui: &mut egui::Ui, play: &mut PlayDef, app: &AppContext, parameters: &[ParamDef], id: egui::Id) {
    egui::ComboBox::from_id_salt(id.with("play"))
        .selected_text(play_label(play))
        .show_ui(ui, |ui| {
            ui.selectable_value(play, PlayDef::Loop, "Loop")
                .on_hover_text("Plays for as long as the effect runs");
            ui.selectable_value(play, PlayDef::OnStart, "OnStart")
                .on_hover_text("Plays once when the effect starts");
            let on_trigger = matches!(play, PlayDef::OnTrigger(_));
            if ui
                .selectable_label(on_trigger, "OnTrigger")
                .on_hover_text("Bursts each time the game fires a trigger (TRIGGERS tab); not when the effect starts")
                .clicked()
                && !on_trigger
            {
                *play = PlayDef::OnTrigger(app.trigger_names.first().cloned().unwrap_or_default());
            }
            let while_ = matches!(play, PlayDef::While(..));
            if ui
                .selectable_label(while_, "While")
                .on_hover_text("Emits while a number parameter is at or above a threshold")
                .clicked()
                && !while_
            {
                let param = parameters.iter().find(|p| p.kind() == ParamKind::Float);
                *play = PlayDef::While(
                    param.map(|p| p.name.clone()).unwrap_or_default(),
                    param.map(|p| p.middle()).unwrap_or(0.5),
                );
            }
        })
        .response
        .on_hover_text(hints::PLAY);
    match play {
        PlayDef::OnTrigger(trigger) => {
            egui::ComboBox::from_id_salt(id.with("trigger"))
                .selected_text(if trigger.is_empty() { "(pick)".to_string() } else { trigger.clone() })
                .show_ui(ui, |ui| {
                    if app.trigger_names.is_empty() {
                        ui.label(egui::RichText::new("No triggers: add one in the TRIGGERS tab").weak());
                    }
                    for name in app.trigger_names.iter() {
                        ui.selectable_value(trigger, name.clone(), name);
                    }
                })
                .response
                .on_hover_text("The trigger that makes this layer burst");
            if !app.trigger_names.contains(trigger) {
                ui.colored_label(egui::Color32::from_hex("#FF8866").unwrap(), "⚠")
                    .on_hover_text("No trigger has this name (TRIGGERS tab)");
            }
        }
        PlayDef::While(param, threshold) => {
            float_param_picker(ui, param, &app.params, id.with("while"));
            ui.label("≥").on_hover_text("Emits while the parameter is at or above this value");
            *threshold = ui_for_f32_ex(ui, *threshold, -1.0e6, 1.0e6, 0.01);
        }
        PlayDef::Loop | PlayDef::OnStart => {}
    }
}

/// The visible range of a layer: off, or (parameter, from, to, fade).
fn ui_for_visible(ui: &mut egui::Ui, visible: &mut Option<VisibleRangeDef>, app: &AppContext, parameters: &[ParamDef], id: egui::Id) {
    ui.vertical(|ui| {
        let mut on = visible.is_some();
        if ui.checkbox(&mut on, "only in a parameter range").on_hover_text(hints::VISIBLE).changed() {
            *visible = on.then(|| {
                let param = parameters.iter().find(|p| p.kind() == ParamKind::Float);
                VisibleRangeDef {
                    param: param.map(|p| p.name.clone()).unwrap_or_default(),
                    from: param.map(|p| p.min).unwrap_or(0.0),
                    to: param.map(|p| p.max).unwrap_or(1.0),
                    fade: 0.1,
                }
            });
        }
        if let Some(v) = visible {
            ui.horizontal(|ui| {
                float_param_picker(ui, &mut v.param, &app.params, id.with("visible"));
                ui.label("from");
                v.from = ui_for_f32_ex(ui, v.from, -1.0e6, 1.0e6, 0.01);
                ui.label("to");
                v.to = ui_for_f32_ex(ui, v.to, -1.0e6, 1.0e6, 0.01);
            });
            ui.horizontal(|ui| {
                ui.label("fade").on_hover_text("How far past From / To (in parameter units) the layer fades out");
                v.fade = ui_for_f32_ex(ui, v.fade, 0.0, 1.0e6, 0.01);
            });
        }
    });
}

/// The spawn rate: off (the spawner's count) or a binding, in particles per
/// second.
fn ui_for_rate(ui: &mut egui::Ui, rate: &mut Option<Binding<f32>>, app: &AppContext, parameters: &[ParamDef], id: egui::Id) {
    ui.vertical(|ui| {
        let mut on = rate.is_some();
        if ui.checkbox(&mut on, "particles per second").on_hover_text(hints::RATE).changed() {
            *rate = on.then(|| Binding::Const(20.0));
        }
        if let Some(binding) = rate {
            let range = |name: &str| {
                parameters
                    .iter()
                    .find(|p| p.name == name && p.kind() == ParamKind::Float)
                    .map(|p| (p.min, p.max))
            };
            binding_editor(ui, binding, &app.params, range, "/s", 100000.0, id.with("rate"));
        }
    });
}

fn attach_label(attach: &AttachDef) -> &'static str {
    match attach {
        AttachDef::Target => "Target",
        AttachDef::Socket(_) => "Socket",
        AttachDef::Segment { .. } => "Segment",
    }
}

/// A socket name field with a menu of the package's sockets.
fn ui_for_socket_name(ui: &mut egui::Ui, name: &mut String, sockets: &[String], hint: &str) {
    ui.add(egui::TextEdit::singleline(name).hint_text(hint).desired_width(90.0)).on_hover_text(
        "A socket of this package (⚓ SOCKETS & FIT tab), or the name of a node (bone) in the target's model",
    );
    if !sockets.is_empty() {
        ui.menu_button("⏷", |ui| {
            for socket in sockets {
                if ui.button(socket).clicked() {
                    *name = socket.clone();
                    ui.close();
                }
            }
        });
    }
}

fn ui_for_attach(ui: &mut egui::Ui, attach: &mut AttachDef, sockets: &[String], id: egui::Id) {
    let first = sockets.first().cloned().unwrap_or_default();
    egui::ComboBox::from_id_salt(id.with("attach"))
        .selected_text(attach_label(attach))
        .show_ui(ui, |ui| {
            if ui.selectable_label(*attach == AttachDef::Target, "Target").on_hover_text("The target's root").clicked() {
                *attach = AttachDef::Target;
            }
            let socket = matches!(attach, AttachDef::Socket(_));
            if ui.selectable_label(socket, "Socket").on_hover_text("Follows a socket").clicked() && !socket {
                *attach = AttachDef::Socket(first.clone());
            }
            let segment = matches!(attach, AttachDef::Segment { .. });
            if ui
                .selectable_label(segment, "Segment")
                .on_hover_text("Between two sockets; read Property segment_a / segment_b in expressions")
                .clicked()
                && !segment
            {
                *attach = AttachDef::Segment {
                    from: first.clone(),
                    to: sockets.get(1).cloned().unwrap_or_default(),
                };
            }
        });
    match attach {
        AttachDef::Target => {}
        AttachDef::Socket(name) => ui_for_socket_name(ui, name, sockets, "socket"),
        AttachDef::Segment { from, to } => {
            ui_for_socket_name(ui, from, sockets, "from");
            ui.label("➡");
            ui_for_socket_name(ui, to, sockets, "to");
        }
    }
}

fn alpha_mode_label(mode: AlphaModeDef) -> &'static str {
    match mode {
        AlphaModeDef::Blend => "Blend",
        AlphaModeDef::Premultiply => "Premultiply",
        AlphaModeDef::Add => "Add",
        AlphaModeDef::Multiply => "Multiply",
        AlphaModeDef::Mask(_) => "Mask",
        AlphaModeDef::Opaque => "Opaque",
    }
}

fn alpha_mode_hint(mode: AlphaModeDef) -> &'static str {
    match mode {
        AlphaModeDef::Blend => "Normal transparency: smoke, dust, mist",
        AlphaModeDef::Premultiply => "Transparency for textures with premultiplied alpha",
        AlphaModeDef::Add => "Adds light: glows, fire, sparks; gets brighter where particles overlap",
        AlphaModeDef::Multiply => "Darkens what is behind (shadows, soot)",
        AlphaModeDef::Mask(_) => "Solid cut-out: pixels below the cutoff opacity are not drawn",
        AlphaModeDef::Opaque => "Solid squares, no transparency",
    }
}

/// The inspector of a particle layer: its settings (name, capacity,
/// texture, parent, space, attach, play, blend), its spawner and its
/// modifier lists.
pub fn draw_particles_layer(
    layer: &mut ParticlesLayerDef,
    assets: &mut Vec<AssetDef>,
    parameters: &[ParamDef],
    app: &mut AppContext,
    ui: &mut egui::Ui,
    index: u64,
) {
    let id = ui.make_persistent_id(format!("layer {}", index));
    egui::Grid::new(id.with("grid")).num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
        ui.label("Name").on_hover_text(hints::NAME);
        ui.add(egui::TextEdit::singleline(&mut layer.name).desired_width(180.0))
            .on_hover_text(hints::NAME);
        ui.end_row();

        ui.label("Capacity").on_hover_text(hints::CAPACITY);
        ui.horizontal(|ui| {
            layer.effect.capacity = ui_for_u32_ex(ui, layer.effect.capacity, 0, 16384, 1);
            ui.label("particles");
        })
        .response
        .on_hover_text(hints::CAPACITY);
        ui.end_row();

        ui.label("Texture").on_hover_text(hints::TEXTURE);
        ui.horizontal(|ui| ui_for_texture(ui, &mut layer.texture, assets, index));
        ui.end_row();

        ui.label("Parent layer").on_hover_text(hints::PARENT);
        let parent = layer.parent.clone().unwrap_or_else(|| "NONE".to_string());
        ui.menu_button(parent, |ui| {
            if ui.button("NONE").clicked() {
                layer.parent = None;
                ui.close();
            }
            for name in app.layer_names.iter() {
                if *name != layer.name && ui.button(name).clicked() {
                    layer.parent = Some(name.clone());
                    ui.close();
                }
            }
        })
        .response
        .on_hover_text(hints::PARENT);
        ui.end_row();

        ui.label("Space").on_hover_text(hints::SPACE);
        ui.horizontal(|ui| {
            ui.radio_value(&mut layer.simulation_space, SimulationSpaceDef::Local, "Local")
                .on_hover_text(hints::SPACE_LOCAL);
            ui.radio_value(&mut layer.simulation_space, SimulationSpaceDef::Global, "Global")
                .on_hover_text(hints::SPACE_GLOBAL);
        });
        ui.end_row();

        ui.label("Attach").on_hover_text(hints::ATTACH);
        ui.horizontal(|ui| ui_for_attach(ui, &mut layer.attach, &app.socket_names, id));
        ui.end_row();

        ui.label("Play").on_hover_text(hints::PLAY);
        ui.horizontal_wrapped(|ui| {
            ui_for_play(ui, &mut layer.play, app, parameters, id);
            let cycles = layer.effect.spawner.cycle_count;
            let mismatch = match layer.play {
                PlayDef::Loop => cycles != 0,
                PlayDef::OnStart | PlayDef::OnTrigger(_) => cycles == 0,
                PlayDef::While(..) => false,
            };
            if mismatch {
                ui.colored_label(egui::Color32::from_hex("#FFCC66").unwrap(), "⚠").on_hover_text(
                    "Play doesn't match the spawner: Loop usually has Cycles 0, OnStart and OnTrigger a finite number (1 = one burst).",
                );
            }
        });
        ui.end_row();

        ui.label("Rate").on_hover_text(hints::RATE);
        ui_for_rate(ui, &mut layer.rate, app, parameters, id);
        ui.end_row();

        ui.label("Visible").on_hover_text(hints::VISIBLE);
        ui_for_visible(ui, &mut layer.visible, app, parameters, id);
        ui.end_row();

        ui.label("Blend").on_hover_text(hints::BLEND);
        ui.horizontal(|ui| {
            let current = layer.effect.alpha_mode;
            egui::ComboBox::from_id_salt(id.with("alpha"))
                .selected_text(alpha_mode_label(current))
                .show_ui(ui, |ui| {
                    let cutoff = match current {
                        AlphaModeDef::Mask(c) => c,
                        _ => 0.5,
                    };
                    for mode in [
                        AlphaModeDef::Blend,
                        AlphaModeDef::Add,
                        AlphaModeDef::Premultiply,
                        AlphaModeDef::Multiply,
                        AlphaModeDef::Mask(cutoff),
                        AlphaModeDef::Opaque,
                    ] {
                        let selected = alpha_mode_label(current) == alpha_mode_label(mode);
                        if ui
                            .selectable_label(selected, alpha_mode_label(mode))
                            .on_hover_text(alpha_mode_hint(mode))
                            .clicked()
                        {
                            layer.effect.alpha_mode = mode;
                        }
                    }
                })
                .response
                .on_hover_text(alpha_mode_hint(current));
            if let AlphaModeDef::Mask(cutoff) = &mut layer.effect.alpha_mode {
                ui.label("cutoff:").on_hover_text("Pixels less opaque than this (0..1) are not drawn");
                *cutoff = ui_for_f32_ex(ui, *cutoff, 0.0, 1.0, 0.01);
            }
        });
        ui.end_row();
    });

    ui.add_space(4.0);
    egui::CollapsingHeader::new("Spawner")
        .id_salt(id.with("spawner"))
        .default_open(true)
        .show(ui, |ui| ui_for_spawner(ui, &mut layer.effect.spawner))
        .header_response
        .on_hover_text(hints::SPAWNER);

    let effect = &mut layer.effect;
    ui_for_modifiers_list(app, ui, &mut effect.init_modifiers, "Init", id, |ui, list| {
        if ui.button(SetAttributeModifierEditor::label()).on_hover_text(hints::SET_ATTRIBUTE).clicked() {
            list.push(ModifierEditor::SetAttribute(SetAttributeModifierEditor::default()));
        }
        if ui
            .button(SetPositionCircleModifierEditor::label())
            .on_hover_text("Particles start on (or inside) a circle")
            .clicked()
        {
            list.push(ModifierEditor::SetPositionCircle(SetPositionCircleModifierEditor::default()));
        }
        if ui
            .button(SetPositionSphereModifierEditor::label())
            .on_hover_text("Particles start on (or inside) a sphere")
            .clicked()
        {
            list.push(ModifierEditor::SetPositionSphere(SetPositionSphereModifierEditor::default()));
        }
        if ui
            .button(SetVelocityCircleModifierEditor::label())
            .on_hover_text("Particles fly outwards in a flat circle (speed in m/s)")
            .clicked()
        {
            list.push(ModifierEditor::SetVelocityCircle(SetVelocityCircleModifierEditor::default()));
        }
        if ui
            .button(SetVelocitySphereModifierEditor::label())
            .on_hover_text("Particles fly outwards in all directions (speed in m/s)")
            .clicked()
        {
            list.push(ModifierEditor::SetVelocitySphere(SetVelocitySphereModifierEditor::default()));
        }
        if ui
            .button(SetVelocityTangentModifierEditor::label())
            .on_hover_text("Particles spin around an axis (speed in m/s)")
            .clicked()
        {
            list.push(ModifierEditor::SetVelocityTangent(SetVelocityTangentModifierEditor::default()));
        }
        if ui
            .button(InheritAttributeModifierEditor::label())
            .on_hover_text("Copies a value from the parent particle (child layers only)")
            .clicked()
        {
            list.push(ModifierEditor::InheritAttribute(InheritAttributeModifierEditor::default()));
        }
    });
    ui_for_modifiers_list(app, ui, &mut effect.update_modifiers, "Update", id, |ui, list| {
        if ui.button(AccelModifierEditor::label()).on_hover_text(hints::ACCEL).clicked() {
            list.push(ModifierEditor::AccelModifier(AccelModifierEditor::default()));
        }
        if ui.button(LinearDragModifierEditor::label()).on_hover_text(hints::DRAG).clicked() {
            list.push(ModifierEditor::LinearDragModifier(LinearDragModifierEditor::default()));
        }
        if ui.button(EmitSpawnEventModifierEditor::label()).on_hover_text(hints::EMIT_SPAWN_EVENT).clicked() {
            list.push(ModifierEditor::EmitSpawnEventModifier(EmitSpawnEventModifierEditor::default()));
        }
        if ui
            .button(ConformToSphereModifierEditor::label())
            .on_hover_text("Pulls particles onto the surface of a sphere")
            .clicked()
        {
            list.push(ModifierEditor::ConformToSphereModifier(ConformToSphereModifierEditor::default()));
        }
    });
    ui_for_modifiers_list(app, ui, &mut effect.render_modifiers, "Render", id, |ui, list| {
        if ui
            .button(SizeOverLifetimeModifierEditor::label())
            .on_hover_text(hints::SIZE_OVER_LIFETIME)
            .clicked()
        {
            list.push(RenderModifierEditor::SizeOverLifetime(SizeOverLifetimeModifierEditor::default()));
        }
        if ui
            .button(ColorOverLifetimeModifierEditor::label())
            .on_hover_text(hints::COLOR_OVER_LIFETIME)
            .clicked()
        {
            list.push(RenderModifierEditor::ColorOverLifetime(ColorOverLifetimeModifierEditor::default()));
        }
        if ui
            .button(ParticleTextureModifierEditor::label())
            .on_hover_text(hints::TEXTURE_MAPPING)
            .clicked()
        {
            list.push(RenderModifierEditor::ParticleTexture(ParticleTextureModifierEditor::default()));
        }
        if ui.button(OrientModifierEditor::label()).on_hover_text(hints::ORIENT).clicked() {
            list.push(RenderModifierEditor::Orient(OrientModifierEditor::default()));
        }
    });
}

/// A new particle layer, textured with the first built-in (cloud) like a new
/// v1 effect; adds the texture asset when missing. The name is made unique.
pub fn new_particles_layer(assets: &mut Vec<AssetDef>, existing: &[String]) -> ParticlesLayerDef {
    let file = PARTICLE_TEXTURES[0].filename;
    let path = builtin_texture_path(file);
    let texture = match assets.iter().find(|a| texture_path(a) == path) {
        Some(a) => a.id.clone(),
        None => {
            let id = builtin_texture_id(file).to_string();
            assets.push(AssetDef {
                id: id.clone(),
                kind: AssetKind::Texture(TextureAssetDef::new(path, TexturePurpose::Sprite)),
            });
            id
        }
    };
    let mut name = "particles".to_string();
    let mut n = 2;
    while existing.contains(&name) {
        name = format!("particles{}", n);
        n += 1;
    }
    ParticlesLayerDef {
        name,
        texture: Some(texture),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_layers_get_unique_names_and_one_cloud_asset() {
        let mut assets = Vec::new();
        let a = new_particles_layer(&mut assets, &[]);
        let b = new_particles_layer(&mut assets, &[a.name.clone()]);
        assert_eq!(a.name, "particles");
        assert_eq!(b.name, "particles2");
        assert_eq!(assets.len(), 1);
        assert_eq!(texture_path(&assets[0]), "omagari://textures/cloud.png");
        assert_eq!(a.texture.as_deref(), Some("cloud"));
        assert_eq!(a.simulation_space, SimulationSpaceDef::Local);
    }
}
