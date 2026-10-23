//! The editor window: toolbar on top, layers on the left, the inspector tabs
//! on the right, playback at the bottom, the 3D preview in the middle.

use bevy::camera::Viewport;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::egui::{self, LayerId, Ui, UiBuilder};
use bevy_egui::EguiContexts;
use bevy_panorbit_camera::PanOrbitCamera;
use omagari::format::ProjectV2;

use super::assets::{drop_overlay, TexturePreviews};
use super::camera::PreviewCamera;
use super::history::Activity;
use super::scene::Backdrop;
use super::session::EditorSession;
use super::{dialogs, inspector, layers_panel, playback, problems, settings_panel, shortcuts, toolbar};
use crate::controller::{EffectResource, PreviewState};
use crate::settings::{GameAssets, RenderSettings};

#[derive(SystemParam)]
pub struct EditorUi<'w, 's> {
    contexts: EguiContexts<'w, 's>,
    project: ResMut<'w, ProjectV2>,
    res: ResMut<'w, EffectResource>,
    session: ResMut<'w, EditorSession>,
    preview_state: Res<'w, PreviewState>,
    game_assets: Res<'w, GameAssets>,
    previews: ResMut<'w, TexturePreviews>,
    backdrop: ResMut<'w, Backdrop>,
    render_settings: ResMut<'w, RenderSettings>,
    virtual_time: ResMut<'w, Time<Virtual>>,
    cameras: Query<'w, 's, (&'static mut Camera, Option<&'static mut PanOrbitCamera>), With<PreviewCamera>>,
    window: Single<'w, 's, &'static Window, With<PrimaryWindow>>,
}

pub fn app_ui(params: EditorUi) -> Result {
    let EditorUi {
        mut contexts,
        mut project,
        mut res,
        mut session,
        preview_state,
        game_assets,
        mut previews,
        mut backdrop,
        mut render_settings,
        mut virtual_time,
        mut cameras,
        window,
    } = params;
    let ctx = contexts.ctx_mut()?.clone();
    let project = &mut *project;
    let session = &mut *session;
    let app = &mut res.context;

    let typing = ctx.text_edit_focused();
    session.activity = if ctx.egui_is_using_pointer() {
        Activity::Dragging
    } else if typing {
        Activity::Typing
    } else {
        Activity::Idle
    };
    if session.dialog.is_none() {
        let shortcuts = shortcuts::read_shortcuts(&ctx, typing, session.selected);
        session.actions.extend(shortcuts);
    }

    app.layer_names = project.layers.iter().map(|l| l.name().to_string()).collect();
    app.socket_names = project.sockets.iter().map(|s| s.name.clone()).collect();
    app.params = project.parameters.iter().map(|p| (p.name.clone(), p.kind())).collect();
    app.trigger_names = project.triggers.iter().map(|t| t.name.clone()).collect();

    // Edited on a copy: writing the resource every frame would re-run the
    // systems that react to its changes.
    let mut scene_backdrop = *backdrop;

    // egui 0.36 panels are shown inside a root `Ui` instead of directly on the context.
    let mut root = Ui::new(
        ctx.clone(),
        "viewport".into(),
        UiBuilder::new().layer_id(LayerId::background()).max_rect(ctx.viewport_rect()),
    );
    egui::Panel::top("Toolbar").resizable(false).show(&mut root, |ui| {
        toolbar::toolbar(ui, project, session, app, &mut scene_backdrop, &game_assets);
    });
    egui::Panel::bottom("Playback").resizable(false).show(&mut root, |ui| {
        playback::playback_bar(ui, session, &preview_state, &mut virtual_time, app);
    });
    egui::Panel::left("LayersPanel")
        .resizable(true)
        .default_size(240.0)
        .size_range(170.0..=480.0)
        .show(&mut root, |ui| {
            layers_panel::layers_panel(ui, project, session);
        });
    egui::Panel::right("Inspector")
        .resizable(true)
        .default_size(430.0)
        .size_range(300.0..=900.0)
        .show(&mut root, |ui| {
            inspector::right_panel(ui, project, session, app, &mut scene_backdrop, &mut previews, &game_assets);
        });
    let central = root.available_rect_before_wrap();

    dialogs::dialogs(&ctx, session, app.filename.as_deref());
    if session.settings_open {
        let mut open = true;
        let mut render = render_settings.clone();
        settings_panel::settings_window(&ctx, &mut open, &mut render, &game_assets, app);
        session.settings_open = open;
        if render != *render_settings {
            *render_settings = render;
        }
    }
    if session.problems_open {
        problems::problems_window(&ctx, project, session);
    }
    drop_overlay(&ctx, session);

    if scene_backdrop != *backdrop {
        *backdrop = scene_backdrop;
    }

    // The 3D view fills what the panels leave.
    let viewport = central_viewport(
        central,
        ctx.pixels_per_point(),
        UVec2::new(window.physical_width(), window.physical_height()),
    );
    let pointer_on_ui = ctx.egui_wants_pointer_input() || session.dialog.is_some();
    for (mut camera, orbit) in cameras.iter_mut() {
        let changed = match (&camera.viewport, &viewport) {
            (Some(a), Some(b)) => a.physical_position != b.physical_position || a.physical_size != b.physical_size,
            (None, None) => false,
            _ => true,
        };
        if changed {
            camera.viewport = viewport.clone();
        }
        if let Some(mut orbit) = orbit {
            let enabled = camera.is_active && !pointer_on_ui;
            if orbit.enabled != enabled {
                orbit.enabled = enabled;
            }
        }
    }
    Ok(())
}

/// The camera viewport for the central area (egui points), in physical
/// pixels and inside the window: a viewport larger than the render target is
/// a wgpu validation error (the window may have shrunk since egui laid out
/// this frame).
pub fn central_viewport(central: egui::Rect, pixels_per_point: f32, window: UVec2) -> Option<Viewport> {
    let min = (central.min.to_vec2() * pixels_per_point).round();
    let max = (central.max.to_vec2() * pixels_per_point).round();
    let x0 = (min.x.max(0.0) as u32).min(window.x);
    let y0 = (min.y.max(0.0) as u32).min(window.y);
    let x1 = (max.x.max(0.0) as u32).min(window.x);
    let y1 = (max.y.max(0.0) as u32).min(window.y);
    (x1 > x0 && y1 > y0).then(|| Viewport {
        physical_position: UVec2::new(x0, y0),
        physical_size: UVec2::new(x1 - x0, y1 - y0),
        ..default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viewport_stays_inside_the_window() {
        let central = egui::Rect::from_min_max(egui::pos2(240.0, 30.0), egui::pos2(850.0, 688.0));
        let v = central_viewport(central, 1.0, UVec2::new(1280, 720)).unwrap();
        assert_eq!((v.physical_position, v.physical_size), (UVec2::new(240, 30), UVec2::new(610, 658)));
        let v = central_viewport(central, 1.0, UVec2::new(1024, 600)).unwrap();
        assert_eq!(v.physical_size, UVec2::new(610, 570), "window shrank");
        let v = central_viewport(central, 1.5, UVec2::new(1920, 1080)).unwrap();
        assert_eq!((v.physical_position, v.physical_size), (UVec2::new(360, 45), UVec2::new(915, 987)));
        assert!(central_viewport(central, 1.0, UVec2::new(200, 20)).is_none(), "nothing left");
    }
}
