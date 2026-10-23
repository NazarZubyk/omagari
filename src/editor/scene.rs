//! The preview scene around the effect: a cube room with a grid (the
//! backdrop), the stage (where the room sits relative to the effect) and the
//! rock of the OBJECT stage.

use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;

use omagari::format::{PreviewDef, PreviewStage, ProjectV2};

use super::camera::PreviewCamera;
use crate::controller::PreviewTarget;

/// Half the size of the cube room the effects are shown in.
const ROOM_HALF_SIZE: f32 = 20.0;

/// What is drawn behind the effects (editor setting, not saved).
///
/// The rooms are a cube with a 1-unit grid on every face, which gives a sense of
/// scale and orientation while orbiting the camera.
#[derive(Resource, Default, Clone, Copy, PartialEq)]
pub enum Backdrop {
    #[default]
    LightRoom,
    DarkRoom,
    Black,
}

impl Backdrop {
    pub const ALL: [Backdrop; 3] = [Backdrop::LightRoom, Backdrop::DarkRoom, Backdrop::Black];

    pub fn next(self) -> Self {
        match self {
            Backdrop::LightRoom => Backdrop::DarkRoom,
            Backdrop::DarkRoom => Backdrop::Black,
            Backdrop::Black => Backdrop::LightRoom,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Backdrop::LightRoom => "LIGHT",
            Backdrop::DarkRoom => "DARK",
            Backdrop::Black => "BLACK",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Backdrop::LightRoom => "Light grey room with a 1 m grid",
            Backdrop::DarkRoom => "Dark room with a 1 m grid: good for glows",
            Backdrop::Black => "Plain black, no room",
        }
    }

    pub fn clear_color(self) -> Color {
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

/// Where the effects play relative to the room. Follows the open project's
/// `preview.stage` ([`sync_stage`]).
///
/// Effects always spawn at the origin; the room moves around them.
#[derive(Resource, Default, Clone, Copy, PartialEq, Debug)]
pub enum Stage {
    /// In the middle of the room, nothing nearby.
    #[default]
    Air,
    /// On the floor.
    Floor,
    /// Around a rock resting on the floor, for shields and auras.
    Object,
}

impl Stage {
    pub const ALL: [Stage; 3] = [Stage::Air, Stage::Floor, Stage::Object];

    pub fn name(self) -> &'static str {
        match self {
            Stage::Air => "AIR",
            Stage::Floor => "FLOOR",
            Stage::Object => "OBJECT",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Stage::Air => "The effect floats in the middle of the room",
            Stage::Floor => "The effect sits on the floor",
            Stage::Object => "The effect sits around a rock on the floor (shields, auras)",
        }
    }

    pub fn parse(name: &str) -> Option<Stage> {
        match name.trim().to_ascii_lowercase().as_str() {
            "air" => Some(Stage::Air),
            "floor" => Some(Stage::Floor),
            "object" => Some(Stage::Object),
            _ => None,
        }
    }

    /// The stage of a project; stages the editor can't show yet (capsule,
    /// model) fall back to AIR.
    pub fn of(project: &ProjectV2) -> Stage {
        match project.preview.as_ref().map(|p| p.stage) {
            Some(PreviewStage::Floor) => Stage::Floor,
            Some(PreviewStage::Object) => Stage::Object,
            _ => Stage::Air,
        }
    }

    /// Stores the stage in the project's preview block.
    pub fn store(self, project: &mut ProjectV2) {
        let stage = match self {
            Stage::Air => PreviewStage::Air,
            Stage::Floor => PreviewStage::Floor,
            Stage::Object => PreviewStage::Object,
        };
        if Stage::of(project) != self {
            project.preview.get_or_insert_with(PreviewDef::default).stage = stage;
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

/// Keeps the [`Stage`] resource on the project's stage.
pub fn sync_stage(project: Res<ProjectV2>, mut stage: ResMut<Stage>) {
    stage.set_if_neq(Stage::of(&project));
}

/// The lowest point of the rock mesh, where the floor goes in the OBJECT stage.
#[derive(Resource)]
pub struct RockBottom(f32);

#[derive(Component)]
pub struct Room;

/// One face of the room: the index of its color in [`Backdrop::face_colors`].
#[derive(Component)]
pub struct RoomFace {
    shade: usize,
}

#[derive(Component)]
pub struct Rock;

/// The six inward-facing sides of the room cube: (normal, shade).
const ROOM_FACES: [(Vec3, usize); 6] = [
    (Vec3::Y, 0),
    (Vec3::NEG_Y, 1),
    (Vec3::X, 2),
    (Vec3::NEG_X, 2),
    (Vec3::Z, 3),
    (Vec3::NEG_Z, 3),
];

pub fn spawn_scene(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
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
    if let Some(VertexAttributeValues::Float32x3(positions)) = mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION) {
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

pub fn apply_scene(
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

pub fn draw_room_grid(
    mut gizmos: Gizmos,
    backdrop: Res<Backdrop>,
    stage: Res<Stage>,
    rock_bottom: Res<RockBottom>,
    cameras: Query<(&Camera, &Transform, &Projection), With<PreviewCamera>>,
    target: Single<&Transform, (With<PreviewTarget>, Without<PreviewCamera>)>,
) {
    // Small axes at the preview target (the spawn point, moved by the preview
    // motion): X red, Y green, Z blue.
    gizmos.axes(**target * Transform::from_xyz(0.0, 0.004, 0.0), 0.5);

    let Some((minor, major)) = backdrop.grid_colors() else {
        return;
    };
    let Some((_, camera, projection)) = cameras.iter().find(|(c, _, _)| c.is_active) else {
        return;
    };
    let orthographic = matches!(projection, Projection::Orthographic(_));
    let h = ROOM_HALF_SIZE;
    let center = Vec3::Y * stage.room_center_y(rock_bottom.0);
    let cells = (2.0 * h) as u32;
    for (normal, _) in ROOM_FACES {
        let face_center = center - normal * h;
        // Like the faces themselves, only draw grids seen from the inside
        // (with an orthographic camera, every ray has the view direction).
        let towards_camera = if orthographic {
            -camera.forward().as_vec3()
        } else {
            camera.translation - face_center
        };
        if towards_camera.dot(normal) <= 0.0 {
            continue;
        }
        // Grids lie in the XY plane by default; lift them a little off the face.
        let plane = Isometry3d::new(face_center + normal * 0.003, Quat::from_rotation_arc(Vec3::Z, normal));
        gizmos.grid(plane, UVec2::splat(cells), Vec2::ONE, minor);
        gizmos.grid(plane, UVec2::splat(cells / 5), Vec2::splat(5.0), major);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage_round_trips_through_the_project() {
        let mut project = ProjectV2::default();
        assert_eq!(Stage::of(&project), Stage::Air);
        Stage::Air.store(&mut project);
        assert!(project.preview.is_none(), "storing the default adds no preview block");
        Stage::Object.store(&mut project);
        assert_eq!(project.preview.as_ref().unwrap().stage, PreviewStage::Object);
        assert_eq!(Stage::of(&project), Stage::Object);
        project.preview.as_mut().unwrap().stage = PreviewStage::Model;
        assert_eq!(Stage::of(&project), Stage::Air, "not shown yet");
        assert_eq!(Stage::parse(" Floor "), Some(Stage::Floor));
    }
}
