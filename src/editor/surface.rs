//! Editor widgets for a surface layer (`omagari::format::SurfaceLayerDef`)
//! and the effect material (shared with shape layers).

use bevy::prelude::{Vec2, Vec4};
use bevy_egui::egui;
use omagari::format::*;

use super::binding::{binding_editor_range, binding_editor_with};
use super::effect::ui_for_visible;
use super::helpers::ui_for_f32_ex;
use super::hints;
use super::params_panel::color_editor;
use super::AppContext;
use crate::asset_library::texture_path;
use crate::model_info::ModelInfo;

const WARN: egui::Color32 = egui::Color32::from_rgb(0xFF, 0x88, 0x66);

/// A new surface layer: an additive purple glow with scrolling noise (the
/// built-in noise, added to the assets when missing) and a rim. The name is
/// made unique.
pub fn new_surface_layer(assets: &mut Vec<AssetDef>, existing: &[String]) -> SurfaceLayerDef {
    let noise = builtin_noise_asset(assets);
    let mut name = "surface".to_string();
    let mut n = 2;
    while existing.contains(&name) {
        name = format!("surface{}", n);
        n += 1;
    }
    SurfaceLayerDef {
        name,
        material: EffectMaterialDef {
            blend: EffectBlendDef::Add,
            color: Binding::Const(Vec4::new(0.6, 0.15, 1.0, 0.7)),
            noise_a: Some(NoiseDef {
                texture: TextureSlotDef { asset: noise, channel: ChannelDef::Luminance },
                ..Default::default()
            }),
            rim: Some(RimDef {
                color: Binding::Const(Vec4::new(0.9, 0.5, 1.0, 1.0)),
                ..Default::default()
            }),
            ..Default::default()
        },
        ..Default::default()
    }
}

/// The id of the built-in noise texture in `assets`, added when missing.
pub fn builtin_noise_asset(assets: &mut Vec<AssetDef>) -> String {
    let path = builtin_texture_path(BUILTIN_NOISE_TEXTURE);
    if let Some(a) = assets.iter().find(|a| texture_path(a) == path) {
        return a.id.clone();
    }
    let mut id = "noise".to_string();
    let mut n = 2;
    while assets.iter().any(|a| a.id == id) {
        id = format!("noise{}", n);
        n += 1;
    }
    assets.push(AssetDef {
        id: id.clone(),
        kind: AssetKind::Texture(TextureAssetDef::new(path, TexturePurpose::Noise)),
    });
    id
}

/// The value range of a Float parameter, for curve keys.
pub(crate) fn range_of<'a>(parameters: &'a [ParamDef]) -> impl Fn(&str) -> Option<(f32, f32)> + 'a {
    move |name: &str| {
        parameters
            .iter()
            .find(|p| p.name == name && p.kind() == ParamKind::Float)
            .map(|p| (p.min, p.max))
    }
}

/// One number value: label with a tooltip, then the binding editor.
#[allow(clippy::too_many_arguments)]
pub(crate) fn number_row(
    ui: &mut egui::Ui,
    label: &str,
    hint: &str,
    binding: &mut Binding<f32>,
    app: &AppContext,
    parameters: &[ParamDef],
    unit: &str,
    range: (f32, f32),
    speed: f32,
    id: egui::Id,
) {
    ui.label(label).on_hover_text(hint);
    ui.vertical(|ui| {
        binding_editor_range(ui, binding, &app.params, &range_of(parameters), unit, range, speed, id.with(label));
    });
    ui.end_row();
}

pub(crate) fn color_row(
    ui: &mut egui::Ui,
    label: &str,
    hint: &str,
    binding: &mut Binding<Vec4>,
    app: &AppContext,
    parameters: &[ParamDef],
    id: egui::Id,
) {
    ui.label(label).on_hover_text(hint);
    ui.vertical(|ui| {
        let mut value_ui = |ui: &mut egui::Ui, c: &mut Vec4| {
            ui.horizontal(|ui| {
                color_editor(ui, c);
            });
        };
        binding_editor_with(ui, binding, &app.params, &range_of(parameters), true, &mut value_ui, id.with(label));
    });
    ui.end_row();
}

fn vec2_row(
    ui: &mut egui::Ui,
    label: &str,
    hint: &str,
    binding: &mut Binding<Vec2>,
    app: &AppContext,
    parameters: &[ParamDef],
    id: egui::Id,
) {
    ui.label(label).on_hover_text(hint);
    ui.vertical(|ui| {
        let mut value_ui = |ui: &mut egui::Ui, v: &mut Vec2| {
            ui.horizontal(|ui| {
                ui.label("x");
                v.x = ui_for_f32_ex(ui, v.x, -100.0, 100.0, 0.01);
                ui.label("y");
                v.y = ui_for_f32_ex(ui, v.y, -100.0, 100.0, 0.01);
                ui.label("/s");
            });
        };
        binding_editor_with(ui, binding, &app.params, &range_of(parameters), false, &mut value_ui, id.with(label));
    });
    ui.end_row();
}

/// Texture + channel of a slot. Offers the project's textures and the
/// built-in noise (added to the assets when picked).
pub(crate) fn texture_slot_ui(ui: &mut egui::Ui, slot: &mut TextureSlotDef, assets: &mut Vec<AssetDef>, id: egui::Id) {
    ui.horizontal(|ui| {
        let mut add_noise = false;
        egui::ComboBox::from_id_salt(id.with("asset"))
            .selected_text(if slot.asset.is_empty() { "(pick)".to_string() } else { slot.asset.clone() })
            .show_ui(ui, |ui| {
                for asset in assets.iter() {
                    let path = texture_path(asset);
                    ui.selectable_value(&mut slot.asset, asset.id.clone(), &asset.id).on_hover_text(path);
                }
                let noise_path = builtin_texture_path(BUILTIN_NOISE_TEXTURE);
                if !assets.iter().any(|a| texture_path(a) == noise_path) {
                    ui.separator();
                    if ui
                        .selectable_label(false, "Noise (built-in)")
                        .on_hover_text("Omagari's tileable noise; adds it to the project's assets")
                        .clicked()
                    {
                        add_noise = true;
                    }
                }
            })
            .response
            .on_hover_text(hints::TEXTURE_SLOT);
        if add_noise {
            slot.asset = builtin_noise_asset(assets);
        }
        egui::ComboBox::from_id_salt(id.with("channel"))
            .selected_text(slot.channel.name())
            .width(90.0)
            .show_ui(ui, |ui| {
                for channel in ChannelDef::ALL {
                    ui.selectable_value(&mut slot.channel, channel, channel.name());
                }
            })
            .response
            .on_hover_text(hints::TEXTURE_SLOT);
        if !slot.asset.is_empty() && !assets.iter().any(|a| a.id == slot.asset) {
            ui.colored_label(WARN, "⚠").on_hover_text("No asset has this id (ASSETS tab): this part is off");
        }
    });
}

fn mapping_ui(ui: &mut egui::Ui, mapping: &mut NoiseMappingDef) {
    ui.horizontal(|ui| {
        ui.radio_value(mapping, NoiseMappingDef::Triplanar, "Triplanar")
            .on_hover_text("Projected from three sides in the model's rest pose: sticks to the body, no UVs needed");
        ui.radio_value(mapping, NoiseMappingDef::Uv, "UV")
            .on_hover_text("The mesh's own texture coordinates");
    });
}

/// A checkbox header for an optional part; returns whether it is on.
pub(crate) fn optional<T>(ui: &mut egui::Ui, value: &mut Option<T>, label: &str, hint: &str, make: impl FnOnce() -> T) -> bool {
    let mut on = value.is_some();
    if ui.checkbox(&mut on, egui::RichText::new(label).strong()).on_hover_text(hint).changed() {
        *value = on.then(make);
    }
    value.is_some()
}

/// Names of the preview model's meshes (mesh names, then mesh entity names)
/// and materials, for the target picker.
fn model_names(model: Option<&ModelInfo>) -> (Vec<String>, Vec<String>) {
    let Some(info) = model else { return (Vec::new(), Vec::new()) };
    let mut meshes: Vec<String> = info.meshes.iter().map(|m| m.name.clone()).collect();
    for m in info.meshes.iter() {
        for p in m.primitives.iter() {
            if !meshes.contains(&p.entity_name) {
                meshes.push(p.entity_name.clone());
            }
        }
    }
    (meshes, info.materials.clone())
}

fn targets_ui(ui: &mut egui::Ui, targets: &mut MeshTargetsDef, model: Option<&ModelInfo>, id: egui::Id) {
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            let current = match targets {
                MeshTargetsDef::AllMeshes => 0,
                MeshTargetsDef::Meshes(_) => 1,
                MeshTargetsDef::Materials(_) => 2,
            };
            if ui.radio(current == 0, "all meshes").clicked() && current != 0 {
                *targets = MeshTargetsDef::AllMeshes;
            }
            if ui.radio(current == 1, "meshes").clicked() && current != 1 {
                *targets = MeshTargetsDef::Meshes(Vec::new());
            }
            if ui.radio(current == 2, "materials").clicked() && current != 2 {
                *targets = MeshTargetsDef::Materials(Vec::new());
            }
        })
        .response
        .on_hover_text(hints::SURFACE_TARGETS);
        let (meshes, materials) = model_names(model);
        let (names, offer) = match targets {
            MeshTargetsDef::AllMeshes => return,
            MeshTargetsDef::Meshes(n) => (n, meshes),
            MeshTargetsDef::Materials(n) => (n, materials),
        };
        let mut remove = None;
        for (i, name) in names.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(format!("▣ {}", name));
                if model.is_some() && !offer.contains(name) {
                    ui.colored_label(WARN, "⚠").on_hover_text("The preview model has no mesh or material with this name");
                }
                if ui.small_button("❌").on_hover_text("Remove").clicked() {
                    remove = Some(i);
                }
            });
        }
        if let Some(i) = remove {
            names.remove(i);
        }
        ui.horizontal(|ui| {
            if offer.is_empty() {
                let key = id.with("new target");
                let mut text: String = ui.data_mut(|d| d.get_temp::<String>(key).unwrap_or_default());
                let response = ui.add(egui::TextEdit::singleline(&mut text).hint_text("name").desired_width(160.0));
                if ui.small_button("+ add").clicked() || (response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
                    let name = text.trim().to_string();
                    if !name.is_empty() && !names.contains(&name) {
                        names.push(name);
                    }
                    text.clear();
                }
                ui.data_mut(|d| d.insert_temp(key, text));
                ui.label(egui::RichText::new("(no preview model: type names)").weak());
            } else {
                let mut picked = None;
                egui::ComboBox::from_id_salt(id.with("add target")).selected_text("+ add").show_ui(ui, |ui| {
                    for name in offer.iter().filter(|n| !names.contains(n)) {
                        if ui.selectable_label(false, name).clicked() {
                            picked = Some(name.clone());
                        }
                    }
                });
                names.extend(picked);
            }
        });
        if names.is_empty() {
            ui.colored_label(WARN, "⚠ empty list: the layer covers nothing");
        }
    });
}

fn shells_ui(ui: &mut egui::Ui, shells: &mut Vec<ShellDef>, id: egui::Id) {
    ui.vertical(|ui| {
        let mut remove = None;
        egui::Grid::new(id.with("shells")).num_columns(5).spacing([6.0, 2.0]).show(ui, |ui| {
            ui.label("");
            ui.label(egui::RichText::new("offset (m)").weak()).on_hover_text(hints::SHELL_OFFSET);
            ui.label(egui::RichText::new("opacity").weak()).on_hover_text(hints::SHELL_OPACITY);
            ui.label(egui::RichText::new("noise shift").weak()).on_hover_text(hints::SHELL_NOISE_OFFSET);
            ui.end_row();
            for (i, shell) in shells.iter_mut().enumerate() {
                ui.label(format!("{}", i + 1));
                shell.offset = ui_for_f32_ex(ui, shell.offset, -0.05, 0.5, 0.001);
                shell.opacity = ui_for_f32_ex(ui, shell.opacity, 0.0, 1.0, 0.01);
                shell.noise_offset = ui_for_f32_ex(ui, shell.noise_offset, -10.0, 10.0, 0.01);
                if ui.small_button("❌").on_hover_text("Remove this shell").clicked() {
                    remove = Some(i);
                }
                ui.end_row();
            }
        });
        if let Some(i) = remove {
            shells.remove(i);
        }
        ui.horizontal(|ui| {
            if ui.small_button("+ shell").on_hover_text("Adds a shell 1.5 cm outside the last one, fainter").clicked() {
                let last = shells.last().copied().unwrap_or(ShellDef { offset: -0.015, ..Default::default() });
                shells.push(ShellDef {
                    offset: last.offset + 0.015,
                    opacity: (last.opacity * 0.6).max(0.05),
                    noise_offset: last.noise_offset + 0.37,
                });
            }
            if ui.small_button("skin").on_hover_text("One shell on the skin").clicked() {
                *shells = vec![ShellDef::default()];
            }
            if ui.small_button("thick glow").on_hover_text("Three shells at 0, 1.5 and 3 cm").clicked() {
                *shells = vec![
                    ShellDef::default(),
                    ShellDef { offset: 0.015, opacity: 0.6, noise_offset: 0.37 },
                    ShellDef { offset: 0.03, opacity: 0.35, noise_offset: 0.74 },
                ];
            }
        });
        if shells.is_empty() {
            ui.colored_label(WARN, "⚠ no shells: nothing is drawn");
        }
    });
}

pub fn draw_surface_layer(
    layer: &mut SurfaceLayerDef,
    assets: &mut Vec<AssetDef>,
    parameters: &[ParamDef],
    app: &mut AppContext,
    ui: &mut egui::Ui,
    index: u64,
    model: Option<&ModelInfo>,
) {
    let id = ui.make_persistent_id(format!("surface layer {}", index));
    egui::Grid::new(id.with("grid")).num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
        ui.label("Name").on_hover_text(hints::NAME);
        ui.add(egui::TextEdit::singleline(&mut layer.name).desired_width(180.0))
            .on_hover_text(hints::NAME);
        ui.end_row();

        ui.label("Covers").on_hover_text(hints::SURFACE_TARGETS);
        targets_ui(ui, &mut layer.targets, model, id.with("targets"));
        ui.end_row();

        ui.label("Shells").on_hover_text(hints::SURFACE_SHELLS);
        shells_ui(ui, &mut layer.shells, id);
        ui.end_row();

        ui.label("Fade").on_hover_text(hints::SURFACE_FADE);
        ui.horizontal(|ui| {
            ui.label("in");
            layer.fade.fade_in = ui_for_f32_ex(ui, layer.fade.fade_in, 0.0, 60.0, 0.01);
            ui.label("s   out");
            layer.fade.fade_out = ui_for_f32_ex(ui, layer.fade.fade_out, 0.0, 60.0, 0.01);
            ui.label("s");
        })
        .response
        .on_hover_text(hints::SURFACE_FADE);
        ui.end_row();

        ui.label("Visible").on_hover_text(hints::VISIBLE);
        ui_for_visible(ui, &mut layer.visible, app, parameters, id);
        ui.end_row();

        material_rows(ui, &mut layer.material, app, parameters, id);
    });
    material_parts(&mut layer.material, assets, parameters, app, ui, id);
}

/// The material's base values as grid rows: blend, colour, opacity,
/// emissive.
pub(crate) fn material_rows(
    ui: &mut egui::Ui,
    m: &mut EffectMaterialDef,
    app: &AppContext,
    parameters: &[ParamDef],
    id: egui::Id,
) {
    ui.label("Blend").on_hover_text(hints::SURFACE_BLEND);
    ui.horizontal(|ui| {
        ui.radio_value(&mut m.blend, EffectBlendDef::Blend, "Blend").on_hover_text(hints::SURFACE_BLEND);
        ui.radio_value(&mut m.blend, EffectBlendDef::Add, "Add").on_hover_text(hints::SURFACE_BLEND);
    });
    ui.end_row();

    color_row(ui, "Colour", hints::SURFACE_COLOR, &mut m.color, app, parameters, id);
    number_row(ui, "Opacity", hints::SURFACE_OPACITY, &mut m.opacity, app, parameters, "", (0.0, 1.0), 0.01, id);
    number_row(ui, "Emissive", hints::SURFACE_EMISSIVE, &mut m.emissive, app, parameters, "×", (0.0, 100.0), 0.05, id);
}

/// The material's optional parts, each behind a checkbox: noise A / B, rim,
/// pulse, flash, dissolve, fill, mask.
pub(crate) fn material_parts(
    m: &mut EffectMaterialDef,
    assets: &mut Vec<AssetDef>,
    parameters: &[ParamDef],
    app: &mut AppContext,
    ui: &mut egui::Ui,
    id: egui::Id,
) {
    ui.separator();
    for (label, noise) in [("Noise A", &mut m.noise_a), ("Noise B", &mut m.noise_b)] {
        if optional(ui, noise, label, hints::NOISE, || NoiseDef {
            texture: TextureSlotDef { asset: builtin_noise_asset(assets), channel: ChannelDef::Luminance },
            ..Default::default()
        }) {
            let n = noise.as_mut().unwrap();
            let nid = id.with(label);
            egui::Grid::new(nid.with("grid")).num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                ui.label("Texture").on_hover_text(hints::TEXTURE_SLOT);
                texture_slot_ui(ui, &mut n.texture, assets, nid);
                ui.end_row();
                ui.label("Mapping").on_hover_text(hints::NOISE);
                mapping_ui(ui, &mut n.mapping);
                ui.end_row();
                let unit = if n.mapping == NoiseMappingDef::Triplanar { "/m" } else { "/UV" };
                number_row(ui, "Scale", hints::NOISE_SCALE, &mut n.scale, app, parameters, unit, (0.0, 1000.0), 0.05, nid);
                vec2_row(ui, "Scroll", hints::NOISE_SCROLL, &mut n.scroll, app, parameters, nid);
                number_row(ui, "Strength", hints::NOISE_STRENGTH, &mut n.strength, app, parameters, "", (0.0, 1.0), 0.01, nid);
                number_row(ui, "Contrast", hints::NOISE_CONTRAST, &mut n.contrast, app, parameters, "×", (0.0, 20.0), 0.05, nid);
            });
        }
    }
    ui.separator();
    if optional(ui, &mut m.rim, "Rim (edge glow)", hints::RIM, RimDef::default) {
        let r = m.rim.as_mut().unwrap();
        let rid = id.with("rim");
        egui::Grid::new(rid.with("grid")).num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
            color_row(ui, "Colour", hints::RIM, &mut r.color, app, parameters, rid);
            number_row(ui, "Power", hints::RIM_POWER, &mut r.power, app, parameters, "", (0.1, 16.0), 0.05, rid);
            number_row(ui, "Intensity", hints::RIM_INTENSITY, &mut r.intensity, app, parameters, "", (0.0, 100.0), 0.05, rid);
        });
    }
    ui.separator();
    if optional(ui, &mut m.pulse, "Pulse", hints::PULSE, EffectPulseDef::default) {
        let p = m.pulse.as_mut().unwrap();
        let pid = id.with("pulse");
        egui::Grid::new(pid.with("grid")).num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
            number_row(ui, "Frequency", hints::PULSE, &mut p.frequency, app, parameters, "/s", (0.0, 60.0), 0.05, pid);
            number_row(ui, "Amount", hints::PULSE, &mut p.amount, app, parameters, "", (0.0, 1.0), 0.01, pid);
        });
    }
    ui.separator();
    if optional(ui, &mut m.flash, "Flash", hints::FLASH, || {
        let param = parameters.iter().find(|p| p.kind() == ParamKind::Float);
        match param {
            Some(p) => Binding::Param(p.name.clone(), vec![(p.min, 0.0), (p.max, 1.0)]),
            None => Binding::Const(0.0),
        }
    }) {
        let f = m.flash.as_mut().unwrap();
        egui::Grid::new(id.with("flash grid")).num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
            number_row(ui, "Flash", hints::FLASH, f, app, parameters, "", (0.0, 1.0), 0.01, id.with("flash"));
        });
    }
    ui.separator();
    if optional(ui, &mut m.dissolve, "Dissolve", hints::DISSOLVE, DissolveDef::default) {
        let d = m.dissolve.as_mut().unwrap();
        let did = id.with("dissolve");
        egui::Grid::new(did.with("grid")).num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
            number_row(ui, "Threshold", hints::DISSOLVE, &mut d.threshold, app, parameters, "", (0.0, 1.0), 0.01, did);
            number_row(ui, "Edge width", hints::DISSOLVE, &mut d.edge_width, app, parameters, "", (0.0, 1.0), 0.005, did);
            color_row(ui, "Edge colour", hints::DISSOLVE, &mut d.edge_color, app, parameters, did);
        });
        if m.noise_a.is_none() && m.noise_b.is_none() {
            ui.colored_label(WARN, "⚠ dissolve needs noise A or B");
        }
    }
    ui.separator();
    if optional(ui, &mut m.fill, "Fill (band along an axis)", hints::FILL, FillDef::default) {
        let f = m.fill.as_mut().unwrap();
        let fid = id.with("fill");
        egui::Grid::new(fid.with("grid")).num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
            ui.label("Axis").on_hover_text(hints::FILL);
            egui::ComboBox::from_id_salt(fid.with("axis")).selected_text(f.axis.name()).show_ui(ui, |ui| {
                for axis in AxisDef::ALL {
                    ui.selectable_value(&mut f.axis, axis, axis.name());
                }
            });
            ui.end_row();
            number_row(ui, "From", hints::FILL, &mut f.from, app, parameters, "", (0.0, 1.0), 0.01, fid);
            number_row(ui, "To", hints::FILL, &mut f.to, app, parameters, "", (0.0, 1.0), 0.01, fid);
            number_row(ui, "Softness", hints::FILL, &mut f.softness, app, parameters, "", (0.0, 1.0), 0.005, fid);
        });
    }
    ui.separator();
    if optional(ui, &mut m.mask, "Mask texture", hints::MASK, || MaskDef {
        texture: TextureSlotDef { asset: builtin_noise_asset(assets), channel: ChannelDef::Luminance },
        ..Default::default()
    }) {
        let k = m.mask.as_mut().unwrap();
        let kid = id.with("mask");
        egui::Grid::new(kid.with("grid")).num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
            ui.label("Texture").on_hover_text(hints::TEXTURE_SLOT);
            texture_slot_ui(ui, &mut k.texture, assets, kid);
            ui.end_row();
            ui.label("Mapping").on_hover_text(hints::MASK);
            mapping_ui(ui, &mut k.mapping);
            ui.end_row();
            number_row(ui, "Scale", hints::NOISE_SCALE, &mut k.scale, app, parameters, "", (0.0, 1000.0), 0.05, kid);
            number_row(ui, "Strength", hints::MASK, &mut k.strength, app, parameters, "", (0.0, 1.0), 0.01, kid);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_surface_layers_use_one_noise_asset() {
        let mut assets = Vec::new();
        let a = new_surface_layer(&mut assets, &[]);
        let b = new_surface_layer(&mut assets, &[a.name.clone()]);
        assert_eq!((a.name.as_str(), b.name.as_str()), ("surface", "surface2"));
        assert_eq!(assets.len(), 1);
        assert_eq!(texture_path(&assets[0]), "omagari://textures/noise.png");
        let t = assets[0].kind.as_texture().unwrap();
        assert_eq!(t.purpose, TexturePurpose::Noise);
        assert_eq!(a.material.noise_a.as_ref().unwrap().texture.asset, "noise");
    }
}
