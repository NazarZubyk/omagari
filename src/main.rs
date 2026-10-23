// OMAGARI - A Bevy-Hanabi 3D particle effects editor for HEXROLL
use std::path::Path;

use bevy::camera::visibility::RenderLayers;
use bevy::camera::Hdr;
use bevy::prelude::*;
use bevy::window::WindowResolution;
use bevy_egui::{EguiGlobalSettings, EguiPlugin, PrimaryEguiContext};
use bevy_hanabi::prelude::*;
use bevy_panorbit_camera::PanOrbitCameraPlugin;

use omagari::format::ProjectV2;

mod asset_library;
mod controller;
mod editor;
mod mcp;
mod model_info;
mod settings;

use crate::controller::*;
use crate::editor::dev::{self, DevOptions};
use crate::editor::scene::Backdrop;
use crate::editor::session::window_title;
use crate::editor::EditorPlugin;
use crate::settings::{GameAssets, OmagariSettings, SETTINGS_FILE};

fn main() {
    // `omagari --mcp` runs the headless MCP server for AI agents instead of the editor.
    if std::env::args().any(|a| a == "--mcp") {
        mcp::run();
        return;
    }

    let args: Vec<String> = std::env::args().collect();
    let dev_options = match DevOptions::parse(&args) {
        Ok(options) => options,
        Err(e) => {
            eprintln!("omagari: {}", e);
            std::process::exit(2);
        }
    };
    let game_assets = GameAssets::from_environment();
    let settings = OmagariSettings::load(Path::new(SETTINGS_FILE));
    let mut window = Window {
        title: window_title(None, false),
        ..default()
    };
    if let Some((width, height)) = dev_options.window_size {
        window.resolution = WindowResolution::new(width, height).with_scale_factor_override(1.0);
    }
    let mut app = App::new();
    // Asset sources must exist before `AssetPlugin` (in `DefaultPlugins`).
    game_assets.register_source(&mut app);
    app.insert_resource(game_assets)
        .insert_resource(settings.render)
        .insert_resource(ClearColor(Backdrop::default().clear_color()))
        .insert_resource(ProjectV2::default())
        .insert_resource(dev_options)
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(window),
            // Closing asks about unsaved changes first (`handle_close_requests`).
            close_when_requested: false,
            ..default()
        }))
        .add_plugins(HanabiPlugin)
        .add_plugins(omagari::OmagariPlugin)
        .add_plugins(PanOrbitCameraPlugin)
        .add_plugins(EguiPlugin::default())
        .add_message::<PreviewCommand>()
        .init_resource::<PreviewState>()
        .init_resource::<EffectResource>()
        .add_plugins(EditorPlugin)
        .add_systems(Startup, (setup, dev::queue_startup_actions))
        .add_systems(Update, dev::run_dev_options)
        .run();
}

/// The egui camera, drawn over the 3D preview cameras
/// (`editor::camera::spawn_preview_cameras`).
fn setup(mut commands: Commands, mut egui_global_settings: ResMut<EguiGlobalSettings>) {
    egui_global_settings.auto_create_primary_context = false;
    commands.spawn((
        PrimaryEguiContext,
        Camera2d,
        RenderLayers::none(),
        Hdr,
        Camera {
            order: 1,
            ..default()
        },
    ));
}

