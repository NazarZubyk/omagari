//! The settings window: the game assets folder (applies after a restart) and
//! the camera profiles (apply at once), both stored in `omagari_settings.ron`.

use std::path::Path;

use bevy_egui::egui::{self, RichText};

use super::helpers::ui_for_f32_ex;
use super::problems::{ERROR, WARNING};
use super::AppContext;
use crate::settings::{
    self, BloomDef, CameraProfile, GameAssets, OmagariSettings, ProjectionDef, RenderSettings, SettingOrigin,
    TonemappingDef, SETTINGS_FILE,
};

/// Shows the window while `open`. Camera profile edits go to `render`.
pub fn settings_window(
    ctx: &egui::Context,
    open: &mut bool,
    render: &mut RenderSettings,
    game_assets: &GameAssets,
    app: &mut AppContext,
) {
    let mut edited = render.clone();
    egui::Window::new("⚙ Settings")
        .open(open)
        .pivot(egui::Align2::CENTER_TOP)
        .default_pos(ctx.content_rect().center_top() + egui::vec2(0.0, 50.0))
        .default_width(460.0)
        .resizable(true)
        .collapsible(false)
        .show(ctx, |ui| {
            egui::ScrollArea::vertical().max_height(ctx.content_rect().height() * 0.75).show(ui, |ui| {
                egui::CollapsingHeader::new(RichText::new("Game assets folder").strong())
                    .default_open(true)
                    .show(ui, |ui| game_assets_section(ui, game_assets, app));
                egui::CollapsingHeader::new(RichText::new("Camera: ORBIT").strong())
                    .default_open(false)
                    .show(ui, |ui| {
                        ui.label(RichText::new("The free camera. Yaw, pitch and distance are where it starts and where F returns it.").weak());
                        profile_editor(ui, "orbit", &mut edited.orbit, CameraProfile::orbit());
                    });
                egui::CollapsingHeader::new(RichText::new("Camera: GAME").strong())
                    .default_open(true)
                    .show(ui, |ui| {
                        ui.label(RichText::new("Should match the game's camera. It follows the preview target.").weak());
                        profile_editor(ui, "game", &mut edited.game, CameraProfile::game());
                    });
                ui.separator();
                ui.horizontal(|ui| {
                    if ui
                        .button("⮋ Save camera settings")
                        .on_hover_text(format!("Write the camera profiles to {}", SETTINGS_FILE))
                        .clicked()
                    {
                        let path = Path::new(SETTINGS_FILE);
                        let mut file = OmagariSettings::load(path);
                        file.render = edited.clone();
                        app.status = Some(match file.save(path) {
                            Ok(()) => {
                                app.saved_render = Some(edited.clone());
                                format!("Saved the camera profiles to {}", SETTINGS_FILE)
                            }
                            Err(e) => format!("Cannot write {}: {}", SETTINGS_FILE, e),
                        });
                    }
                    let stored = app
                        .saved_render
                        .get_or_insert_with(|| OmagariSettings::load(Path::new(SETTINGS_FILE)).render);
                    if *stored != edited {
                        ui.colored_label(WARNING, "● not saved").on_hover_text(
                            "The cameras already use these values; save to keep them after a restart",
                        );
                    }
                });
            });
        });
    if edited != *render {
        *render = edited;
    }
}

fn profile_editor(ui: &mut egui::Ui, id: &str, profile: &mut CameraProfile, defaults: CameraProfile) {
    egui::Grid::new(("profile", id)).num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
        ui.label("Projection");
        ui.horizontal(|ui| {
            let perspective = matches!(profile.projection, ProjectionDef::Perspective { .. });
            if ui.radio(perspective, "Perspective").on_hover_text("Far things look smaller").clicked() && !perspective {
                profile.projection = ProjectionDef::Perspective { fov: 45.0 };
            }
            if ui
                .radio(!perspective, "Orthographic")
                .on_hover_text("No perspective: sizes don't change with distance (isometric games)")
                .clicked()
                && perspective
            {
                profile.projection = ProjectionDef::Orthographic { height: 16.0 };
            }
        });
        ui.end_row();
        match &mut profile.projection {
            ProjectionDef::Perspective { fov } => {
                ui.label("Field of view").on_hover_text("Vertical view angle in degrees");
                ui.horizontal(|ui| {
                    *fov = ui_for_f32_ex(ui, *fov, 5.0, 150.0, 0.2);
                    ui.label("°");
                });
            }
            ProjectionDef::Orthographic { height } => {
                ui.label("Visible height").on_hover_text("How many metres fit from the bottom to the top of the view");
                ui.horizontal(|ui| {
                    *height = ui_for_f32_ex(ui, *height, 0.5, 500.0, 0.05);
                    ui.label("m");
                });
            }
        }
        ui.end_row();
        ui.label("Yaw").on_hover_text("Turn around the vertical axis: 0° = looking from +Z, 45° = from the +X +Z diagonal");
        ui.horizontal(|ui| {
            profile.yaw = ui_for_f32_ex(ui, profile.yaw, -360.0, 360.0, 0.25);
            ui.label("°");
        });
        ui.end_row();
        ui.label("Pitch").on_hover_text("Angle above the horizon: 35.26° = true isometric");
        ui.horizontal(|ui| {
            profile.pitch = ui_for_f32_ex(ui, profile.pitch, -89.0, 89.0, 0.25);
            ui.label("°");
        });
        ui.end_row();
        ui.label("Distance").on_hover_text("Metres from the target to the camera");
        ui.horizontal(|ui| {
            profile.distance = ui_for_f32_ex(ui, profile.distance, 0.1, 500.0, 0.05);
            ui.label("m");
        });
        ui.end_row();
        ui.label("Tonemapping").on_hover_text("How bright HDR colours are squeezed into the screen's range");
        egui::ComboBox::from_id_salt(("tonemapping", id))
            .selected_text(format!("{:?}", profile.tonemapping))
            .show_ui(ui, |ui| {
                for t in TonemappingDef::ALL {
                    ui.selectable_value(&mut profile.tonemapping, t, format!("{:?}", t));
                }
            });
        ui.end_row();
        ui.label("Bloom").on_hover_text("Glow around bright (HDR, above 1) colours");
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt(("bloom", id))
                .selected_text(format!("{:?}", profile.bloom))
                .show_ui(ui, |ui| {
                    for b in BloomDef::ALL {
                        ui.selectable_value(&mut profile.bloom, b, format!("{:?}", b));
                    }
                });
            if profile.bloom != BloomDef::Off {
                let mut custom = profile.bloom_intensity.is_some();
                if ui.checkbox(&mut custom, "intensity").on_hover_text("Override the preset's strength").changed() {
                    profile.bloom_intensity = custom.then_some(0.15);
                }
                if let Some(intensity) = &mut profile.bloom_intensity {
                    *intensity = ui_for_f32_ex(ui, *intensity, 0.0, 1.0, 0.005);
                }
            }
        });
        ui.end_row();
        ui.label("Exposure").on_hover_text("EV100; higher = darker lit surfaces (the rock). 9.7 = Bevy's default");
        profile.exposure = ui_for_f32_ex(ui, profile.exposure, -5.0, 20.0, 0.05);
        ui.end_row();
        ui.label("Background").on_hover_text("Off = the backdrop's colour");
        ui.horizontal(|ui| {
            let mut custom = profile.background.is_some();
            if ui.checkbox(&mut custom, "custom").changed() {
                profile.background = custom.then_some([0.1, 0.1, 0.12]);
            }
            if let Some(color) = &mut profile.background {
                ui.color_edit_button_rgb(color);
            }
        });
        ui.end_row();
    });
    if *profile != defaults && ui.button("Reset to defaults").clicked() {
        *profile = defaults;
    }
}

/// The game assets folder in effect, and a field to change the one stored in
/// `omagari_settings.ron` (used after a restart).
fn game_assets_section(ui: &mut egui::Ui, game_assets: &GameAssets, app: &mut AppContext) {
    match &game_assets.configured {
        Some((folder, origin)) => {
            ui.label(format!("In use: {}", folder.display()));
            ui.label(RichText::new(format!("(set by {})", origin.describe())).weak());
            if !folder.is_dir() {
                ui.colored_label(ERROR, "⚠ this folder does not exist");
            }
        }
        None => {
            ui.label("Not set: project textures load from this repository's assets/ folder.");
        }
    }
    ui.label(format!("EXPORT writes to: {}", game_assets.export_dir().display()));
    ui.separator();

    let input = app.game_assets_input.get_or_insert_with(|| {
        OmagariSettings::load(Path::new(SETTINGS_FILE)).game_assets.unwrap_or_default()
    });
    ui.label(format!("Folder stored in {} (your game's assets/ folder):", SETTINGS_FILE));
    ui.horizontal(|ui| {
        egui::TextEdit::singleline(input)
            .hint_text("C:/path/to/game/assets")
            .desired_width(300.0)
            .show(ui);
        if ui.button("Apply").clicked() {
            let path = Path::new(SETTINGS_FILE);
            let mut settings = OmagariSettings::load(path);
            let value = input.trim().to_string();
            settings.game_assets = (!value.is_empty()).then_some(value);
            app.status = Some(match settings.save(path) {
                Ok(()) => format!("Saved {}. Restart the editor to use the new game assets folder.", SETTINGS_FILE),
                Err(e) => format!("Cannot write {}: {}", SETTINGS_FILE, e),
            });
        }
    });
    let trimmed = input.trim();
    if !trimmed.is_empty() && !Path::new(trimmed).is_dir() {
        ui.colored_label(ERROR, "⚠ this folder does not exist");
    }
    ui.label(RichText::new("Empty = not set. Changes apply after a restart.").weak());
    if matches!(
        game_assets.configured,
        Some((_, SettingOrigin::CommandLine | SettingOrigin::Environment))
    ) {
        ui.label(format!(
            "Note: {} / {} override the file.",
            settings::GAME_ASSETS_FLAG,
            settings::GAME_ASSETS_ENV
        ));
    }
}
