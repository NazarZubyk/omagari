//! The preview scene around the effect: a cube room with a grid (the
//! backdrop), the stage (where the room sits relative to the effect) and the
//! rock of the OBJECT stage.

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::math::Affine2;
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use omagari::format::{PreviewDef, PreviewStage, ProjectV2};

use super::camera::PreviewCamera;
use crate::controller::PreviewTarget;

/// Half the size of the cube room the effects are shown in.
const ROOM_HALF_SIZE: f32 = 20.0;

/// What is drawn behind the effects (editor setting, not saved).
///
/// The rooms are a cube with a 1-unit grid on every face, which gives a sense of
/// scale and orientation while orbiting the camera.
#[derive(Resource, Default, Clone, Copy, PartialEq, Debug)]
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
    /// On a 0.7 × 1.8 m capsule standing on the floor (its centre is the
    /// target).
    Capsule,
    /// On the project's preview model (`preview.model`; the capsule without
    /// one), see [`super::model`].
    Model,
}

impl Stage {
    pub const ALL: [Stage; 5] = [Stage::Air, Stage::Floor, Stage::Object, Stage::Capsule, Stage::Model];

    pub fn name(self) -> &'static str {
        match self {
            Stage::Air => "AIR",
            Stage::Floor => "FLOOR",
            Stage::Object => "OBJECT",
            Stage::Capsule => "CAPSULE",
            Stage::Model => "MODEL",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Stage::Air => "The effect floats in the middle of the room",
            Stage::Floor => "The effect sits on the floor",
            Stage::Object => "The effect sits around a rock on the floor (shields, auras)",
            Stage::Capsule => "The effect sits on a 0.7 × 1.8 m capsule standing on the floor (a character's size); its centre is the target",
            Stage::Model => "The effect sits on the preview model (PREVIEW tab: model, animation, weapon); the capsule until a model is set",
        }
    }

    pub fn parse(name: &str) -> Option<Stage> {
        match name.trim().to_ascii_lowercase().as_str() {
            "air" => Some(Stage::Air),
            "floor" => Some(Stage::Floor),
            "object" => Some(Stage::Object),
            "capsule" => Some(Stage::Capsule),
            "model" => Some(Stage::Model),
            _ => None,
        }
    }

    /// The stage of a project.
    pub fn of(project: &ProjectV2) -> Stage {
        match project.preview.as_ref().map(|p| p.stage) {
            Some(PreviewStage::Floor) => Stage::Floor,
            Some(PreviewStage::Object) => Stage::Object,
            Some(PreviewStage::Capsule) => Stage::Capsule,
            Some(PreviewStage::Model) => Stage::Model,
            _ => Stage::Air,
        }
    }

    /// Stores the stage in the project's preview block.
    pub fn store(self, project: &mut ProjectV2) {
        let stage = match self {
            Stage::Air => PreviewStage::Air,
            Stage::Floor => PreviewStage::Floor,
            Stage::Object => PreviewStage::Object,
            Stage::Capsule => PreviewStage::Capsule,
            Stage::Model => PreviewStage::Model,
        };
        if Stage::of(project) != self {
            project.preview.get_or_insert_with(PreviewDef::default).stage = stage;
        }
    }

    /// Height of the room center, given the lowest point of the rock and
    /// how far below the target the floor of a body stage is.
    fn room_center_y(self, rock_bottom: f32, floor: StageFloor) -> f32 {
        match self {
            Stage::Air => 0.0,
            Stage::Floor => ROOM_HALF_SIZE,
            Stage::Object => ROOM_HALF_SIZE + rock_bottom,
            Stage::Capsule | Stage::Model => ROOM_HALF_SIZE - floor.0.unwrap_or(0.0),
        }
    }
}

/// How far below the preview target the floor of the CAPSULE / MODEL stage
/// is (set by [`super::model::sync_preview_models`]).
#[derive(Resource, Default, Clone, Copy, PartialEq, Debug)]
pub struct StageFloor(pub Option<f32>);

/// Keeps the [`Stage`] resource on the project's stage.
pub fn sync_stage(project: Res<ProjectV2>, mut stage: ResMut<Stage>) {
    stage.set_if_neq(Stage::of(&project));
}

/// Whether the room's floor is lit (shaded by lights) instead of a flat
/// unlit colour: on while the project has light layers, so their light shows
/// on the floor. The lit floor's colour is scaled so the editor's sun gives
/// about the same look as the unlit one ([`lit_floor_factor`]).
#[derive(Resource, Default, Clone, Copy, PartialEq, Eq, Debug)]
pub struct LitFloor(pub bool);

/// Keeps [`LitFloor`] on "the project has a light layer".
pub fn sync_lit_floor(project: Res<ProjectV2>, mut lit: ResMut<LitFloor>) {
    if project.is_changed() {
        lit.set_if_neq(LitFloor(project.light_layers().next().is_some()));
    }
}

/// Whether the room's grid is painted into its faces (opaque textures)
/// instead of drawn as lines over them: on while the project has a
/// distortion layer, because distortion bends only opaque things (the
/// line grid is drawn after it and would stay sharp on top).
#[derive(Resource, Default, Clone, Copy, PartialEq, Eq, Debug)]
pub struct PaintedGrid(pub bool);

/// Keeps [`PaintedGrid`] on "the project has a distortion layer".
pub fn sync_painted_grid(project: Res<ProjectV2>, mut painted: ResMut<PaintedGrid>) {
    if project.is_changed() {
        painted.set_if_neq(PaintedGrid(project.distortion_layers().next().is_some()));
    }
}

/// Pixels per metre of the painted grid.
const GRID_CELL_PX: usize = 64;

/// A 5 × 5 m tile of the room's grid as an sRGB texture with mipmaps: the
/// face colour, a minor line every metre and a major line on its edge (so
/// tiles repeat into the 5 m grid the line gizmos draw).
fn grid_tile(face: Color, minor: Color, major: Color) -> Image {
    let n = GRID_CELL_PX * 5;
    let px = |c: Color| {
        let s = c.to_srgba();
        [(s.red * 255.0).round() as u8, (s.green * 255.0).round() as u8, (s.blue * 255.0).round() as u8, 255u8]
    };
    let (f, mi, ma) = (px(face), px(minor), px(major));
    let on_line = |v: usize, width: usize, period: usize| v % period < width || v % period >= period - width;
    let mut level = Vec::with_capacity(n * n * 4);
    for y in 0..n {
        for x in 0..n {
            let c = if on_line(x, 3, n) || on_line(y, 3, n) {
                ma
            } else if on_line(x, 1, GRID_CELL_PX) || on_line(y, 1, GRID_CELL_PX) {
                mi
            } else {
                f
            };
            level.extend_from_slice(&c);
        }
    }
    // Mipmaps (2 × 2 box filter), so the far walls do not shimmer.
    let mut data = level.clone();
    let mut size = n;
    let mut levels = 1;
    while size > 1 {
        let half = (size / 2).max(1);
        let mut next = vec![0u8; half * half * 4];
        for y in 0..half {
            for x in 0..half {
                for c in 0..4 {
                    let at = |xx: usize, yy: usize| level[((yy.min(size - 1)) * size + xx.min(size - 1)) * 4 + c] as u32;
                    let sum = at(2 * x, 2 * y) + at(2 * x + 1, 2 * y) + at(2 * x, 2 * y + 1) + at(2 * x + 1, 2 * y + 1);
                    next[(y * half + x) * 4 + c] = (sum / 4) as u8;
                }
            }
        }
        data.extend_from_slice(&next);
        level = next;
        size = half;
        levels += 1;
    }
    let mut image = Image::new(
        Extent3d { width: n as u32, height: n as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        Vec::new(),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.data = Some(data);
    image.texture_descriptor.mip_level_count = levels;
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        anisotropy_clamp: 8,
        ..default()
    });
    image
}

/// The editor's sun: illuminance (lux) and where it shines from.
const SUN_ILLUMINANCE: f32 = 6000.0;
const SUN_POSITION: Vec3 = Vec3::new(3.0, 6.0, 4.0);

/// The base colour factor of the lit floor: under the sun (Lambert: E × cos
/// θ / π, × Bevy's default exposure 1 / (1.2 × 2^9.7)) it then looks like the
/// unlit floor.
pub fn lit_floor_factor() -> f32 {
    let exposure = 1.0 / (1.2 * 2f32.powf(9.7));
    let sun = SUN_ILLUMINANCE * SUN_POSITION.normalize().y / std::f32::consts::PI * exposure;
    1.0 / sun.max(1e-3)
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
    // Only the rock (and the floor while the project has light layers) is
    // lit; the room and the particles are unlit.
    commands.spawn((
        DirectionalLight {
            illuminance: SUN_ILLUMINANCE,
            ..default()
        },
        Transform::from_translation(SUN_POSITION).looking_at(Vec3::ZERO, Vec3::Y),
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

#[allow(clippy::too_many_arguments)]
pub fn apply_scene(
    backdrop: Res<Backdrop>,
    stage: Res<Stage>,
    floor: Res<StageFloor>,
    lit_floor: Res<LitFloor>,
    painted: Res<PaintedGrid>,
    rock_bottom: Res<RockBottom>,
    mut clear_color: ResMut<ClearColor>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut room: Single<(&mut Transform, &mut Visibility), With<Room>>,
    mut rock: Single<&mut Visibility, (With<Rock>, Without<Room>)>,
    faces: Query<(&RoomFace, &MeshMaterial3d<StandardMaterial>)>,
) {
    clear_color.0 = backdrop.clear_color();
    let (room_transform, room_visibility) = &mut *room;
    room_transform.translation.y = stage.room_center_y(rock_bottom.0, *floor);
    match backdrop.face_colors() {
        Some(colors) => {
            **room_visibility = Visibility::Inherited;
            for (face, material) in faces.iter() {
                if let Some(mut material) = materials.get_mut(&material.0) {
                    // Shade 0 is the floor.
                    let lit = face.shade == 0 && lit_floor.0;
                    material.unlit = !lit;
                    material.perceptual_roughness = 0.9;
                    // With a painted grid the texture carries the colour.
                    let grid = backdrop.grid_colors().filter(|_| painted.0);
                    let c = if grid.is_some() { LinearRgba::WHITE } else { colors[face.shade].to_linear() };
                    material.base_color = if lit {
                        let k = lit_floor_factor();
                        Color::linear_rgb(c.red * k, c.green * k, c.blue * k)
                    } else {
                        Color::from(c)
                    };
                    match grid {
                        Some((minor, major)) => {
                            material.base_color_texture = Some(images.add(grid_tile(colors[face.shade], minor, major)));
                            material.uv_transform = Affine2::from_scale(Vec2::splat(2.0 * ROOM_HALF_SIZE / 5.0));
                        }
                        None => {
                            material.base_color_texture = None;
                            material.uv_transform = Affine2::IDENTITY;
                        }
                    }
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

#[allow(clippy::too_many_arguments)]
pub fn draw_room_grid(
    mut gizmos: Gizmos,
    backdrop: Res<Backdrop>,
    stage: Res<Stage>,
    floor: Res<StageFloor>,
    rock_bottom: Res<RockBottom>,
    painted: Res<PaintedGrid>,
    cameras: Query<(&Camera, &Transform, &Projection), With<PreviewCamera>>,
    target: Single<&Transform, (With<PreviewTarget>, Without<PreviewCamera>)>,
) {
    // Small axes at the preview target (the spawn point, moved by the preview
    // motion): X red, Y green, Z blue.
    gizmos.axes(**target * Transform::from_xyz(0.0, 0.004, 0.0), 0.5);

    let Some((minor, major)) = backdrop.grid_colors() else {
        return;
    };
    // Painted into the faces instead (distortion layers bend it).
    if painted.0 {
        return;
    }
    let Some((_, camera, projection)) = cameras.iter().find(|(c, _, _)| c.is_active) else {
        return;
    };
    let orthographic = matches!(projection, Projection::Orthographic(_));
    let h = ROOM_HALF_SIZE;
    let center = Vec3::Y * stage.room_center_y(rock_bottom.0, *floor);
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
        assert_eq!(Stage::of(&project), Stage::Model);
        Stage::Capsule.store(&mut project);
        assert_eq!(project.preview.as_ref().unwrap().stage, PreviewStage::Capsule);
        assert_eq!(Stage::parse(" Floor "), Some(Stage::Floor));
        assert_eq!(Stage::parse("MODEL"), Some(Stage::Model));
        // The floor of a body stage is under the body's feet.
        assert_eq!(Stage::Capsule.room_center_y(-0.5, StageFloor(Some(0.9))), ROOM_HALF_SIZE - 0.9);
    }
}
