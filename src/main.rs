// OMAGARI - A Bevy-Hanabi 3D particle effects editor for HEXROLL
use bevy::prelude::*;

use bevy::camera::visibility::RenderLayers;
use bevy::post_process::bloom::Bloom;
use bevy::camera::Hdr;
use bevy::mesh::VertexAttributeValues;
use bevy::{camera::Viewport, window::PrimaryWindow};

use bevy_egui::{
    egui::{self, scroll_area::ScrollBarVisibility, LayerId, Layout, Ui, UiBuilder},
    EguiContext, EguiContexts, EguiGlobalSettings, EguiPlugin, EguiPrimaryContextPass,
    PrimaryEguiContext,
};
use bevy_hanabi::prelude::*;
use bevy_panorbit_camera::{PanOrbitCamera, PanOrbitCameraPlugin};

use ron::ser::PrettyConfig;
use std::fs::File;
use std::io::Write;

mod controller;
mod effect;
mod expr;
mod helpers;
mod mcp;
mod modifiers;

use crate::controller::*;
use crate::effect::*;
use crate::expr::*;
use crate::helpers::*;

fn main() {
    // `omagari --mcp` runs the headless MCP server for AI agents instead of the editor.
    if std::env::args().any(|a| a == "--mcp") {
        mcp::run();
        return;
    }

    App::new()
        .insert_resource(ClearColor(Backdrop::default().clear_color()))
        .init_resource::<Backdrop>()
        .init_resource::<Stage>()
        .add_plugins(DefaultPlugins)
        .add_plugins(HanabiPlugin)
        .add_plugins(omagari::OmagariPlugin)
        .add_plugins(PanOrbitCameraPlugin)
        .add_plugins(EguiPlugin::default())
        .add_systems(Startup, setup)
        .add_systems(Update, poll_preview_requests)
        .add_systems(
            Update,
            apply_scene.run_if(resource_changed::<Backdrop>.or_else(resource_changed::<Stage>)),
        )
        .add_systems(Update, draw_room_grid)
        .add_systems(EguiPrimaryContextPass, app_ui)
        .insert_resource(OmagariProject::default())
        .run();
}

fn setup(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut egui_global_settings: ResMut<EguiGlobalSettings>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    egui_global_settings.auto_create_primary_context = false;
    commands.spawn((
        Transform::from_xyz(0.0, 1.5, 5.0),
        Bloom::NATURAL,
        Hdr,
        Camera { ..default() },
        PanOrbitCamera::default(),
    ));

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

    spawn_scene(&mut commands, &mut meshes, &mut materials);

    commands.insert_resource(EffectResource {
        effect_handles: Vec::new(),
        textures: PARTICLE_TEXTURES
            .iter()
            .map(|v| asset_server.load(v.filename))
            .collect(),
        context: AppContext::default(),
    });
}

/// Half the size of the cube room the effects are shown in.
const ROOM_HALF_SIZE: f32 = 20.0;

/// What is drawn behind the effects, cycled from the toolbar.
///
/// The rooms are a cube with a 1-unit grid on every face, which gives a sense of
/// scale and orientation while orbiting the camera.
#[derive(Resource, Default, Clone, Copy, PartialEq)]
enum Backdrop {
    #[default]
    LightRoom,
    DarkRoom,
    Black,
}

impl Backdrop {
    fn next(self) -> Self {
        match self {
            Backdrop::LightRoom => Backdrop::DarkRoom,
            Backdrop::DarkRoom => Backdrop::Black,
            Backdrop::Black => Backdrop::LightRoom,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Backdrop::LightRoom => "◐ BG: LIGHT",
            Backdrop::DarkRoom => "◐ BG: DARK",
            Backdrop::Black => "◐ BG: BLACK",
        }
    }

    fn clear_color(self) -> Color {
        match self {
            Backdrop::LightRoom => Color::srgb(0.62, 0.62, 0.64),
            Backdrop::DarkRoom => Color::srgb(0.08, 0.08, 0.09),
            Backdrop::Black => Color::BLACK,
        }
    }

    /// Floor, ceiling, X walls and Z walls colors; slightly different shades so
    /// the faces read without lighting.
    fn face_colors(self) -> Option<[Color; 4]> {
        match self {
            Backdrop::LightRoom => Some([
                Color::srgb(0.70, 0.70, 0.72),
                Color::srgb(0.66, 0.66, 0.68),
                Color::srgb(0.62, 0.62, 0.64),
                Color::srgb(0.57, 0.57, 0.59),
            ]),
            Backdrop::DarkRoom => Some([
                Color::srgb(0.16, 0.16, 0.17),
                Color::srgb(0.09, 0.09, 0.10),
                Color::srgb(0.12, 0.12, 0.13),
                Color::srgb(0.10, 0.10, 0.11),
            ]),
            Backdrop::Black => None,
        }
    }

    /// Minor and major (every 5 units) grid line colors.
    fn grid_colors(self) -> Option<(Color, Color)> {
        match self {
            Backdrop::LightRoom => Some((Color::srgb(0.50, 0.50, 0.52), Color::srgb(0.30, 0.30, 0.32))),
            Backdrop::DarkRoom => Some((Color::srgb(0.20, 0.20, 0.22), Color::srgb(0.34, 0.34, 0.37))),
            Backdrop::Black => None,
        }
    }
}

/// Where the effects play relative to the room, cycled from the toolbar.
///
/// Effects always spawn at the origin; the room moves around them.
#[derive(Resource, Default, Clone, Copy, PartialEq)]
enum Stage {
    /// In the middle of the room, nothing nearby.
    #[default]
    Air,
    /// On the floor.
    Floor,
    /// Around a rock resting on the floor, for shields and auras.
    Object,
}

impl Stage {
    fn next(self) -> Self {
        match self {
            Stage::Air => Stage::Floor,
            Stage::Floor => Stage::Object,
            Stage::Object => Stage::Air,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Stage::Air => "▣ STAGE: AIR",
            Stage::Floor => "▣ STAGE: FLOOR",
            Stage::Object => "▣ STAGE: OBJECT",
        }
    }

    fn parse(name: &str) -> Option<Stage> {
        match name.trim().to_ascii_lowercase().as_str() {
            "air" => Some(Stage::Air),
            "floor" => Some(Stage::Floor),
            "object" => Some(Stage::Object),
            _ => None,
        }
    }

    /// Height of the room center, given the lowest point of the rock.
    fn room_center_y(self, rock_bottom: f32) -> f32 {
        match self {
            Stage::Air => 0.0,
            Stage::Floor => ROOM_HALF_SIZE,
            Stage::Object => ROOM_HALF_SIZE + rock_bottom,
        }
    }
}

/// The lowest point of the rock mesh, where the floor goes in the OBJECT stage.
#[derive(Resource)]
struct RockBottom(f32);

#[derive(Component)]
struct Room;

/// One face of the room: the index of its color in [`Backdrop::face_colors`].
#[derive(Component)]
struct RoomFace {
    shade: usize,
}

#[derive(Component)]
struct Rock;

/// The six inward-facing sides of the room cube: (normal, shade).
const ROOM_FACES: [(Vec3, usize); 6] = [
    (Vec3::Y, 0),
    (Vec3::NEG_Y, 1),
    (Vec3::X, 2),
    (Vec3::NEG_X, 2),
    (Vec3::Z, 3),
    (Vec3::NEG_Z, 3),
];

fn spawn_scene(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    let h = ROOM_HALF_SIZE;
    // Faces are one-sided and point inward, so from outside the room only the
    // far faces show and never block the view.
    commands
        .spawn((Room, Transform::default(), Visibility::default()))
        .with_children(|room| {
            for (normal, shade) in ROOM_FACES {
                room.spawn((
                    RoomFace { shade },
                    Mesh3d(meshes.add(Plane3d::new(normal, Vec2::splat(h)))),
                    MeshMaterial3d(materials.add(StandardMaterial {
                        unlit: true,
                        ..default()
                    })),
                    Transform::from_translation(-normal * h),
                ));
            }
        });

    let (rock, rock_bottom) = rock_mesh();
    commands.insert_resource(RockBottom(rock_bottom));
    commands.spawn((
        Rock,
        Mesh3d(meshes.add(rock)),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.42, 0.39, 0.36),
            perceptual_roughness: 0.95,
            ..default()
        })),
        Transform::default(),
        Visibility::Hidden,
    ));
    // Only the rock is lit; the room and the particles are unlit.
    commands.spawn((
        DirectionalLight {
            illuminance: 6000.0,
            ..default()
        },
        Transform::from_xyz(3.0, 6.0, 4.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

/// A lumpy, faceted boulder about 2 units wide, centered on the origin.
/// Returns the mesh and its lowest point.
fn rock_mesh() -> (Mesh, f32) {
    let mut mesh = Sphere::new(1.0).mesh().ico(4).unwrap();
    let mut bottom = 0.0f32;
    if let Some(VertexAttributeValues::Float32x3(positions)) =
        mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION)
    {
        for p in positions.iter_mut() {
            let d = Vec3::from(*p).normalize();
            let bumps = 0.14 * (3.1 * d.x + 1.7).sin() * (2.3 * d.y + 0.4).sin() * (2.9 * d.z + 2.1).sin()
                + 0.07 * (7.3 * d.x).sin() * (6.1 * d.y + 1.0).sin() * (5.7 * d.z + 0.3).sin()
                + 0.03 * (13.0 * d.x + 2.0).sin() * (11.0 * d.z).sin();
            let mut v = d * (1.0 + bumps);
            v.y *= 0.8;
            // Flatten the base so the rock sits on the floor.
            if v.y < -0.55 {
                v.y = -0.55 + (v.y + 0.55) * 0.3;
            }
            bottom = bottom.min(v.y);
            *p = v.to_array();
        }
    }
    mesh.duplicate_vertices();
    mesh.compute_flat_normals();
    (mesh, bottom)
}

fn apply_scene(
    backdrop: Res<Backdrop>,
    stage: Res<Stage>,
    rock_bottom: Res<RockBottom>,
    mut clear_color: ResMut<ClearColor>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut room: Single<(&mut Transform, &mut Visibility), With<Room>>,
    mut rock: Single<&mut Visibility, (With<Rock>, Without<Room>)>,
    faces: Query<(&RoomFace, &MeshMaterial3d<StandardMaterial>)>,
) {
    clear_color.0 = backdrop.clear_color();
    let (room_transform, room_visibility) = &mut *room;
    room_transform.translation.y = stage.room_center_y(rock_bottom.0);
    match backdrop.face_colors() {
        Some(colors) => {
            **room_visibility = Visibility::Inherited;
            for (face, material) in faces.iter() {
                if let Some(mut material) = materials.get_mut(&material.0) {
                    material.base_color = colors[face.shade];
                }
            }
        }
        None => **room_visibility = Visibility::Hidden,
    }
    **rock = if *stage == Stage::Object {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
}

fn draw_room_grid(
    mut gizmos: Gizmos,
    backdrop: Res<Backdrop>,
    stage: Res<Stage>,
    rock_bottom: Res<RockBottom>,
    camera: Single<&Transform, With<PanOrbitCamera>>,
) {
    // Small axes at the spawn point: X red, Y green, Z blue.
    gizmos.axes(Transform::from_xyz(0.0, 0.004, 0.0), 0.5);

    let Some((minor, major)) = backdrop.grid_colors() else {
        return;
    };
    let h = ROOM_HALF_SIZE;
    let center = Vec3::Y * stage.room_center_y(rock_bottom.0);
    let cells = (2.0 * h) as u32;
    for (normal, _) in ROOM_FACES {
        let face_center = center - normal * h;
        // Like the faces themselves, only draw grids seen from the inside.
        if (camera.translation - face_center).dot(normal) <= 0.0 {
            continue;
        }
        // Grids lie in the XY plane by default; lift them a little off the face.
        let plane = Isometry3d::new(
            face_center + normal * 0.003,
            Quat::from_rotation_arc(Vec3::Z, normal),
        );
        gizmos.grid(plane, UVec2::splat(cells), Vec2::ONE, minor);
        gizmos.grid(plane, UVec2::splat(cells / 5), Vec2::splat(5.0), major);
    }
}

/// Loads and plays a project when the MCP server asks for a preview.
///
/// The server writes the project file name to `saved_effects/.preview_request`,
/// optionally followed by a stage name on the next line, and waits for the
/// editor to delete it.
fn poll_preview_requests(
    mut commands: Commands,
    mut since_last_poll: Local<f32>,
    time: Res<Time>,
    mut project: ResMut<OmagariProject>,
    mut res: ResMut<EffectResource>,
    mut stage: ResMut<Stage>,
    effects: ResMut<Assets<EffectAsset>>,
    curr: Query<Entity, With<ParticleEffect>>,
) {
    *since_last_poll += time.delta_secs();
    if *since_last_poll < 0.25 {
        return;
    }
    *since_last_poll = 0.0;

    let request = Folder::SavedEffects.full_file_path(mcp::PREVIEW_REQUEST_FILE.to_string());
    let Ok(content) = std::fs::read_to_string(&request) else {
        return;
    };
    let _ = std::fs::remove_file(&request);
    let mut lines = content.lines();
    let filename = lines.next().unwrap_or_default().trim().to_string();
    if let Some(requested) = lines.next().and_then(Stage::parse) {
        *stage = requested;
    }
    match load_project(&filename) {
        Ok(loaded) => {
            *project = loaded;
            res.context.filename = Some(filename);
            spawn_particle_effects(
                &mut commands,
                &mut res,
                std::rc::Rc::new(std::cell::RefCell::new(&mut *project)),
                effects,
                curr,
            );
        }
        Err(e) => warn!("preview request for {}: {}", filename, e),
    }
}

#[derive(Default)]
struct AppContext {
    expr_clipboard: Option<ExprWriterEditor>,
    visible_effects: Vec<String>,
    filename: Option<String>,
}

fn app_ui(
    mut commands: Commands,
    mut contexts: EguiContexts,
    mut camera: Single<&mut Camera, Without<EguiContext>>,
    window: Single<&mut Window, With<PrimaryWindow>>,
    project: ResMut<OmagariProject>,
    mut res: ResMut<EffectResource>,
    effects: ResMut<Assets<EffectAsset>>,
    curr: Query<Entity, With<ParticleEffect>>,
    type_registry: Res<AppTypeRegistry>,
    mut backdrop: ResMut<Backdrop>,
    mut stage: ResMut<Stage>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    // egui 0.36 panels are shown inside a root `Ui` instead of directly on the context.
    let mut viewport_ui = Ui::new(
        ctx.clone(),
        "viewport".into(),
        UiBuilder::new()
            .layer_id(LayerId::background())
            .max_rect(ctx.viewport_rect()),
    );

    res.context.visible_effects = project
        .effects
        .iter()
        .map(|e| e.name().to_string())
        .collect();

    let project = std::rc::Rc::new(std::cell::RefCell::new(project.into_inner()));

    let mut filename = res
        .context
        .filename
        .as_ref()
        .unwrap_or(&"".to_string())
        .clone();

    egui::Panel::top("Toolbar")
        .resizable(false)
        .show(&mut viewport_ui, |ui| {
            ui.vertical(|ui| {
                ui.add_space(2.0);
                ui.horizontal(|ui| {
                    if ui.button("⏵ RENDER").clicked() {
                        spawn_particle_effects(
                            &mut commands,
                            &mut res,
                            project.clone(),
                            effects,
                            curr,
                        );
                    }

                    if ui.button("⏹ STOP").clicked() {
                        despawn_all_particle_effects(
                            &curr,
                            &mut commands,
                        );
                    }

                    if ui.button("🖭 EXPORT").clicked() {
                        export_effects_to_files(
                            &filename,
                            project.clone(),
                            &type_registry.read(),
                        );
                    }

                    if ui.button(backdrop.label()).clicked() {
                        *backdrop = backdrop.next();
                    }
                    if ui.button(stage.label()).clicked() {
                        *stage = stage.next();
                    }

                    ui.add_space(10.0);
                    ui.separator();
                    ui.add_space(10.0);

                    ui.label("filename:");

                    let filename_okay = validate_project_filename(&filename);

                    let mut filename_textedit =
                        egui::TextEdit::singleline(&mut filename).hint_text("filename.omagari.ron");
                    if !filename_okay {
                        filename_textedit = filename_textedit
                            .background_color(egui::Color32::from_hex("#220000").unwrap())
                    }

                    filename_textedit.show(ui);
                    res.context.filename = Some(filename.clone());

                    if ui.button("🌌 NEW").clicked() {
                        res.context.filename = None;
                        commands.insert_resource(OmagariProject::default());
                    }

                    ui.menu_button("⮉ LOAD", |ui| {
                        for f in projects_list() {
                            if ui.button(f.clone()).clicked() {
                                if let Ok(project) = load_project(&f) {
                                    commands.insert_resource(project);
                                    res.context.filename = Some(f.clone());
                                    ui.close();
                                }
                            }
                        }
                    });
                    if ui
                        .add_enabled(filename_okay, egui::Button::new("⮋ SAVE"))
                        .clicked()
                    {
                        let ron_string = ron::ser::to_string_pretty(
                            *project.clone().borrow(),
                            PrettyConfig::new().new_line("\n".to_string()),
                        )
                        .unwrap();

                        let file_path = Folder::SavedEffects.full_file_path(filename);
                        if let Ok(mut file) = File::create(file_path) {
                            file.write_all(ron_string.as_bytes()).unwrap();
                        }
                    }

                    ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.colored_label(egui::Color32::from_hex("#88AAFF").unwrap(), "OMAGARI🔥");
                    });
                });
            });
            ui.allocate_rect(ui.available_rect_before_wrap(), egui::Sense::hover());
        })
        .response
        .rect
        .height();

    let left = egui::Panel::left("EffectsPanel")
        .resizable(true)
        .show(&mut viewport_ui, |ui| {
            egui::ScrollArea::vertical()
                .scroll_bar_visibility(ScrollBarVisibility::VisibleWhenNeeded)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            let effects = &mut project.borrow_mut().effects;
                            let n_effects = effects.len();
                            for (index, effect) in effects.iter_mut().enumerate() {
                                let swap = ui
                                    .horizontal(|ui| {
                                        if let Some(list_command) =
                                            ui_for_list_item(ui, index, n_effects)
                                        {
                                            return Some(list_command);
                                        } else {
                                            effect.draw_ui(&mut res.context, ui, index as u64);
                                        }
                                        None
                                    })
                                    .inner;
                                if let Some(swap) = swap {
                                    match swap {
                                        ListCommand::Remove(i) => {
                                            effects.remove(i);
                                        }
                                        ListCommand::Swap((a, b)) => {
                                            effects.swap(a, b);
                                        }
                                    }
                                    break;
                                }
                            }
                            if ui.button("+").clicked() {
                                effects.push(EffectEditor::default());
                            }
                        });
                        ui.allocate_rect(ui.available_rect_before_wrap(), egui::Sense::hover());
                    })
                });

            ui.allocate_rect(ui.available_rect_before_wrap(), egui::Sense::hover());
        })
        .response
        .rect
        .width();

    let pos = UVec2::new(left as u32, 0);
    let size =
        UVec2::new(window.physical_width(), window.physical_height()) - UVec2::new(left as u32, 0);
    camera.viewport = Some(Viewport {
        physical_position: pos,
        physical_size: size,
        ..default()
    });

    Ok(())
}
