//! Editor widgets for a particle layer (`omagari::format::ParticlesLayerDef`)
//! and its particle effect.

use bevy_egui::*;
use omagari::format::*;

use super::helpers::*;
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
    unique_collapsing(id.value(), label, ui).show(ui, |ui| {
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
            ui.label(ParticleTextureModifierEditor::label())
                .on_hover_text("Samples the layer's texture. Particles can't pick the G/B/A channels.");
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
        ImageSampleMappingDef::Modulate => "color × texture (RGBA)",
        ImageSampleMappingDef::ModulateRGB => "color.rgb × texture.rgb, alpha unchanged",
        ImageSampleMappingDef::ModulateOpacityFromR => "the texture's red channel is opacity (grayscale sprites)",
    }
}

impl UiProvider for OrientModifierEditor {
    fn draw_ui(&mut self, _app: &mut AppContext, ui: &mut egui::Ui, index: u64) {
        ui.horizontal(|ui| {
            ui.label(OrientModifierEditor::label());
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
        OrientModeDef::AlongVelocity => "stretched along the velocity; motionless particles may be invisible",
        OrientModeDef::ParallelCameraDepthPlane => "flat to the screen (billboards)",
        OrientModeDef::FaceCameraPosition => "facing the camera position",
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
    ui.horizontal(|ui| {
        ui.label("Count:")
            .on_hover_text("Particles spawned over the spawn duration, every cycle");
        ui_for_cpu_value(ui, &mut spawner.count, 10000.0, 1.0);
    });
    ui.horizontal(|ui| {
        ui.label("Duration:")
            .on_hover_text("Seconds over which Count particles are spawned; 0 = all at once");
        ui_for_cpu_value(ui, &mut spawner.spawn_duration, 10000.0, 0.01);
        ui.label("s");
    });
    ui.horizontal(|ui| {
        ui.label("Period:").on_hover_text("Seconds between the starts of two cycles");
        ui_for_cpu_value(ui, &mut spawner.period, 10000.0, 0.01);
        ui.label("s");
    });
    ui.horizontal(|ui| {
        ui.label("Cycles:").on_hover_text("Number of cycles; 0 = forever");
        spawner.cycle_count = ui_for_u32_ex(ui, spawner.cycle_count, 0, 10000, 1);
    });
    ui.horizontal(|ui| {
        ui.checkbox(&mut spawner.starts_active, "Starts active")
            .on_hover_text("Off: game code must activate the spawner");
        ui.checkbox(&mut spawner.emit_on_start, "Emit on start");
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

fn play_label(play: &PlayDef) -> String {
    match play {
        PlayDef::Loop => "Loop".to_string(),
        PlayDef::OnStart => "OnStart".to_string(),
        PlayDef::OnTrigger(t) => format!("OnTrigger(\"{}\") (later)", t),
        PlayDef::While(p, v) => format!("While(\"{}\", {}) (later)", p, v),
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
        AlphaModeDef::Blend => "regular alpha blending",
        AlphaModeDef::Premultiply => "premultiplied alpha",
        AlphaModeDef::Add => "additive: glows, gets brighter where particles overlap",
        AlphaModeDef::Multiply => "darkens what is behind",
        AlphaModeDef::Mask(_) => "opaque cut-out: alpha below the cutoff is discarded",
        AlphaModeDef::Opaque => "opaque quads",
    }
}

/// The settings of a particle layer (name, parent, space, play, texture,
/// alpha mode), its spawner and its modifier lists.
pub fn draw_particles_layer(
    layer: &mut ParticlesLayerDef,
    assets: &mut Vec<AssetDef>,
    app: &mut AppContext,
    ui: &mut egui::Ui,
    index: u64,
) {
    let id = ui.make_persistent_id(format!("layer {}", index));
    ui.vertical(|ui| {
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false)
            .show_header(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.set_max_width(260.0);
                    ui.label("✨ Particles:");
                    ui.text_edit_singleline(&mut layer.name);
                });
            })
            .body(|ui| {
                ui.horizontal(|ui| {
                    ui.label("Capacity:").on_hover_text("Most particles alive at once");
                    layer.effect.capacity = ui_for_u32_ex(ui, layer.effect.capacity, 0, 16384, 1);
                    ui.label("Texture:");
                    ui_for_texture(ui, &mut layer.texture, assets, index);
                });
                ui.horizontal(|ui| {
                    ui.label("Parent layer:").on_hover_text(
                        "Particles of this layer are spawned by the parent's EmitSpawnEvent modifiers",
                    );
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
                    });
                });
                ui.horizontal(|ui| {
                    ui.label("Space:");
                    ui.radio_value(&mut layer.simulation_space, SimulationSpaceDef::Local, "Local")
                        .on_hover_text("Particles move with the target");
                    ui.radio_value(&mut layer.simulation_space, SimulationSpaceDef::Global, "Global")
                        .on_hover_text("Particles stay where they were emitted: a moving target leaves a trail");
                });
                ui.horizontal(|ui| {
                    ui.label("Play:").on_hover_text(
                        "When the layer plays. How long it emits is set by the spawner (Cycles: 0 = loop).",
                    );
                    egui::ComboBox::from_id_salt(id.with("play"))
                        .selected_text(play_label(&layer.play))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut layer.play, PlayDef::Loop, "Loop")
                                .on_hover_text("For as long as the instance runs");
                            ui.selectable_value(&mut layer.play, PlayDef::OnStart, "OnStart")
                                .on_hover_text("Once when the instance starts");
                            ui.add_enabled(false, egui::Button::selectable(false, "OnTrigger (later)"));
                            ui.add_enabled(false, egui::Button::selectable(false, "While (later)"));
                        });
                    let cycles = layer.effect.spawner.cycle_count;
                    let mismatch = match layer.play {
                        PlayDef::Loop => cycles != 0,
                        PlayDef::OnStart => cycles == 0,
                        _ => false,
                    };
                    if mismatch {
                        ui.colored_label(egui::Color32::from_hex("#FFCC66").unwrap(), "⚠").on_hover_text(
                            "Play doesn't match the spawner: Loop usually has Cycles 0, OnStart a finite number.",
                        );
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Blend:");
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
                        ui.label("cutoff:");
                        *cutoff = ui_for_f32_ex(ui, *cutoff, 0.0, 1.0, 0.01);
                    }
                });

                unique_collapsing(1, "Spawner", ui).show(ui, |ui| {
                    ui_for_spawner(ui, &mut layer.effect.spawner);
                });

                let effect = &mut layer.effect;
                ui_for_modifiers_list(app, ui, &mut effect.init_modifiers, "Init", id, |ui, list| {
                    if ui.button(SetAttributeModifierEditor::label()).clicked() {
                        list.push(ModifierEditor::SetAttribute(SetAttributeModifierEditor::default()));
                    }
                    if ui.button(SetPositionCircleModifierEditor::label()).clicked() {
                        list.push(ModifierEditor::SetPositionCircle(
                            SetPositionCircleModifierEditor::default(),
                        ));
                    }
                    if ui.button(SetPositionSphereModifierEditor::label()).clicked() {
                        list.push(ModifierEditor::SetPositionSphere(
                            SetPositionSphereModifierEditor::default(),
                        ));
                    }
                    if ui.button(SetVelocityCircleModifierEditor::label()).clicked() {
                        list.push(ModifierEditor::SetVelocityCircle(
                            SetVelocityCircleModifierEditor::default(),
                        ));
                    }
                    if ui.button(SetVelocitySphereModifierEditor::label()).clicked() {
                        list.push(ModifierEditor::SetVelocitySphere(
                            SetVelocitySphereModifierEditor::default(),
                        ));
                    }
                    if ui.button(SetVelocityTangentModifierEditor::label()).clicked() {
                        list.push(ModifierEditor::SetVelocityTangent(
                            SetVelocityTangentModifierEditor::default(),
                        ));
                    }
                    if ui.button(InheritAttributeModifierEditor::label()).clicked() {
                        list.push(ModifierEditor::InheritAttribute(
                            InheritAttributeModifierEditor::default(),
                        ));
                    }
                });
                ui_for_modifiers_list(app, ui, &mut effect.update_modifiers, "Update", id, |ui, list| {
                    if ui.button(AccelModifierEditor::label()).clicked() {
                        list.push(ModifierEditor::AccelModifier(AccelModifierEditor::default()));
                    }
                    if ui.button(LinearDragModifierEditor::label()).clicked() {
                        list.push(ModifierEditor::LinearDragModifier(LinearDragModifierEditor::default()));
                    }
                    if ui.button(EmitSpawnEventModifierEditor::label()).clicked() {
                        list.push(ModifierEditor::EmitSpawnEventModifier(
                            EmitSpawnEventModifierEditor::default(),
                        ));
                    }
                    if ui.button(ConformToSphereModifierEditor::label()).clicked() {
                        list.push(ModifierEditor::ConformToSphereModifier(
                            ConformToSphereModifierEditor::default(),
                        ));
                    }
                });
                ui_for_modifiers_list(app, ui, &mut effect.render_modifiers, "Render", id, |ui, list| {
                    if ui.button(SizeOverLifetimeModifierEditor::label()).clicked() {
                        list.push(RenderModifierEditor::SizeOverLifetime(
                            SizeOverLifetimeModifierEditor::default(),
                        ));
                    }
                    if ui.button(ColorOverLifetimeModifierEditor::label()).clicked() {
                        list.push(RenderModifierEditor::ColorOverLifetime(
                            ColorOverLifetimeModifierEditor::default(),
                        ));
                    }
                    if ui.button(ParticleTextureModifierEditor::label()).clicked() {
                        list.push(RenderModifierEditor::ParticleTexture(
                            ParticleTextureModifierEditor::default(),
                        ));
                    }
                    if ui.button(OrientModifierEditor::label()).clicked() {
                        list.push(RenderModifierEditor::Orient(OrientModifierEditor::default()));
                    }
                });
            });
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
