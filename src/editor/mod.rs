//! Editor-only code: the editor window (panels, undo, live preview, camera
//! presets) and the egui widgets for the data model in `omagari::format`.

pub mod actions;
pub mod assets;
pub mod binding;
pub mod camera;
pub mod dialogs;
pub mod effect;
pub mod expr;
pub mod dev;
pub mod helpers;
pub mod hints;
pub mod history;
pub mod inspector;
pub mod layer_view;
pub mod layers_panel;
pub mod modifiers;
pub mod package;
pub mod params_panel;
pub mod playback;
pub mod problems;
pub mod scene;
pub mod session;
pub mod settings_panel;
pub mod shortcuts;
pub mod toolbar;
pub mod ui;

use bevy::prelude::*;
use bevy_egui::EguiPrimaryContextPass;
use omagari::format::ExprWriterEditor;

use crate::controller::{apply_preview_commands, move_preview_target, spawn_preview_target};
use crate::settings::RenderSettings;

/// Editor state shared by the widgets.
#[derive(Default)]
pub struct AppContext {
    pub expr_clipboard: Option<ExprWriterEditor>,
    /// Names of the open project's layers (parent pickers).
    pub layer_names: Vec<String>,
    /// Names of the open project's sockets (attach pickers).
    pub socket_names: Vec<String>,
    /// The open project's parameters (expression and binding pickers).
    pub params: Vec<(String, omagari::format::ParamKind)>,
    /// Names of the open project's triggers (play pickers).
    pub trigger_names: Vec<String>,
    /// File name of the open project in `saved_effects/`; `None` = untitled.
    pub filename: Option<String>,
    /// File name SAVE AS offers for an untitled project (from its recipe).
    pub suggested_name: Option<String>,
    /// Result of the last command (save, export, ...), shown in the bottom bar.
    pub status: Option<String>,
    /// Text of the game assets folder field in the settings window.
    pub game_assets_input: Option<String>,
    /// The camera profiles as stored in the settings file (for the
    /// "not saved" marker).
    pub saved_render: Option<RenderSettings>,
}

/// The editor's systems: scene, cameras, UI, actions, tracking, preview.
pub struct EditorPlugin;

impl Plugin for EditorPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<scene::Backdrop>()
            .init_resource::<scene::Stage>()
            .init_resource::<session::EditorSession>()
            .init_resource::<assets::TexturePreviews>()
            .add_systems(
                Startup,
                (scene::spawn_scene, spawn_preview_target, camera::spawn_preview_cameras),
            )
            .add_systems(
                Update,
                (
                    actions::poll_preview_requests,
                    actions::handle_close_requests,
                    assets::handle_file_drops,
                    assets::poll_import_dialog,
                    assets::update_texture_previews,
                    scene::sync_stage,
                    move_preview_target,
                    camera::follow_preview_target.after(move_preview_target),
                    camera::focus_camera,
                    camera::apply_render_profiles,
                    scene::draw_room_grid,
                ),
            )
            .add_systems(
                Update,
                (
                    apply_preview_commands,
                    crate::controller::sync_preview_texture_settings,
                    crate::controller::drive_preview,
                )
                    .chain()
                    .before(omagari::runtime::VfxSystems),
            )
            .add_systems(
                Update,
                scene::apply_scene
                    .after(scene::sync_stage)
                    .run_if(resource_changed::<scene::Backdrop>.or_else(resource_changed::<scene::Stage>)),
            )
            .add_systems(
                EguiPrimaryContextPass,
                (ui::app_ui, actions::perform_actions, session::track_project).chain(),
            );
    }
}
