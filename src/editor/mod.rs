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
pub mod light;
pub mod model;
pub mod model_panel;
pub mod modifiers;
pub mod package;
pub mod params_panel;
pub mod particle_presets;
pub mod playback;
pub mod problems;
pub mod scene;
pub mod session;
pub mod settings_panel;
pub mod shape;
pub mod shortcuts;
pub mod surface;
pub mod toolbar;
pub mod trail;
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
    /// `(rows, columns)` of the selected layer's texture grid, while its
    /// inspector is drawn (Flipbook).
    pub layer_texture_grid: Option<(u32, u32)>,
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
            .init_resource::<model::ModelPreview>()
            .init_resource::<scene::StageFloor>()
            .init_resource::<scene::LitFloor>()
            .init_gizmo_group::<model::SocketGizmos>()
            .add_systems(
                Startup,
                (
                    scene::spawn_scene,
                    spawn_preview_target,
                    camera::spawn_preview_cameras,
                    model::configure_socket_gizmos,
                ),
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
                    scene::sync_lit_floor,
                    move_preview_target,
                    camera::follow_preview_target.after(move_preview_target),
                    camera::focus_camera,
                    camera::apply_render_profiles,
                    scene::draw_room_grid,
                    model::drag_sockets,
                ),
            )
            .add_systems(
                Update,
                (
                    model::sync_preview_models,
                    model::resolve_instance_target,
                    model::drive_model_animation,
                    apply_preview_commands,
                    crate::controller::sync_preview_texture_settings,
                    crate::controller::drive_preview,
                )
                    .chain()
                    .after(move_preview_target)
                    .before(omagari::runtime::VfxSystems),
            )
            .add_systems(
                PostUpdate,
                (
                    model::update_socket_markers.after(bevy::transform::TransformSystems::Propagate),
                    model::keep_model_in_place
                        .after(bevy::app::AnimationSystems)
                        .before(bevy::transform::TransformSystems::Propagate),
                    model::draw_trail_samples.after(omagari::runtime::VfxSystems),
                    light::draw_light_gizmos.after(omagari::runtime::VfxSystems),
                ),
            )
            .add_systems(
                Update,
                scene::apply_scene
                    .after(scene::sync_stage)
                    .run_if(
                        resource_changed::<scene::Backdrop>
                            .or_else(resource_changed::<scene::Stage>)
                            .or_else(resource_changed::<scene::StageFloor>)
                            .or_else(resource_changed::<scene::LitFloor>),
                    ),
            )
            .add_systems(
                EguiPrimaryContextPass,
                (ui::app_ui, actions::perform_actions, session::track_project).chain(),
            );
    }
}
