//! Preview cameras: the free `Orbit` camera and the fixed isometric `Game`
//! camera, set up from the render profiles of `omagari_settings.ron`
//! ([`RenderSettings`]). A project picks one in `preview.camera`.

use bevy::camera::{Exposure, ScalingMode};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;
use bevy_panorbit_camera::PanOrbitCamera;

use omagari::format::{PreviewCamera as CameraChoice, PreviewDef, ProjectV2};

use super::session::EditorSession;
use crate::controller::PreviewTarget;
use crate::settings::{BloomDef, CameraProfile, ProjectionDef, RenderSettings, TonemappingDef};

/// A camera of the 3D preview, and which preset it is.
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
pub enum PreviewCamera {
    Orbit,
    Game,
}

impl PreviewCamera {
    pub const ALL: [PreviewCamera; 2] = [PreviewCamera::Orbit, PreviewCamera::Game];

    /// The preset the project asks for.
    pub fn of(project: &ProjectV2) -> PreviewCamera {
        match project.preview.as_ref().map(|p| p.camera) {
            Some(CameraChoice::Game) => PreviewCamera::Game,
            _ => PreviewCamera::Orbit,
        }
    }

    /// Stores the preset in the project's preview block.
    pub fn store(self, project: &mut ProjectV2) {
        if PreviewCamera::of(project) != self {
            project.preview.get_or_insert_with(PreviewDef::default).camera = match self {
                PreviewCamera::Orbit => CameraChoice::Orbit,
                PreviewCamera::Game => CameraChoice::Game,
            };
        }
    }

    /// `orbit` / `game` (any case).
    pub fn parse(name: &str) -> Option<PreviewCamera> {
        match name.trim().to_ascii_lowercase().as_str() {
            "orbit" => Some(PreviewCamera::Orbit),
            "game" => Some(PreviewCamera::Game),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            PreviewCamera::Orbit => "ORBIT",
            PreviewCamera::Game => "GAME",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            PreviewCamera::Orbit => "Free camera: drag to orbit, right-drag to pan, wheel to zoom",
            PreviewCamera::Game => "The game's camera: isometric, no perspective, follows the target",
        }
    }

    pub fn profile(self, settings: &RenderSettings) -> &CameraProfile {
        match self {
            PreviewCamera::Orbit => &settings.orbit,
            PreviewCamera::Game => &settings.game,
        }
    }
}

impl CameraProfile {
    pub fn projection_component(&self) -> Projection {
        match self.projection {
            ProjectionDef::Perspective { fov } => Projection::Perspective(PerspectiveProjection {
                fov: fov.clamp(1.0, 170.0).to_radians(),
                ..default()
            }),
            ProjectionDef::Orthographic { height } => Projection::Orthographic(OrthographicProjection {
                scaling_mode: ScalingMode::FixedVertical {
                    viewport_height: height.max(0.01),
                },
                ..OrthographicProjection::default_3d()
            }),
        }
    }

    /// The orbit camera's radius: the distance, or the zoom factor of an
    /// orthographic projection (bevy_panorbit_camera scales it).
    pub fn orbit_radius(&self) -> f32 {
        match self.projection {
            ProjectionDef::Perspective { .. } => self.distance.max(0.05),
            ProjectionDef::Orthographic { .. } => 1.0,
        }
    }

    pub fn bloom_component(&self) -> Option<Bloom> {
        let mut bloom = match self.bloom {
            BloomDef::Off => return None,
            BloomDef::Natural => Bloom::NATURAL,
            BloomDef::OldSchool => Bloom::OLD_SCHOOL,
            BloomDef::ScreenBlur => Bloom::SCREEN_BLUR,
            BloomDef::Anamorphic => Bloom::ANAMORPHIC,
        };
        if let Some(intensity) = self.bloom_intensity {
            bloom.intensity = intensity.max(0.0);
        }
        Some(bloom)
    }

    pub fn tonemapping_component(&self) -> Tonemapping {
        match self.tonemapping {
            TonemappingDef::None => Tonemapping::None,
            TonemappingDef::Reinhard => Tonemapping::Reinhard,
            TonemappingDef::ReinhardLuminance => Tonemapping::ReinhardLuminance,
            TonemappingDef::AcesFitted => Tonemapping::AcesFitted,
            TonemappingDef::AgX => Tonemapping::AgX,
            TonemappingDef::SomewhatBoringDisplayTransform => Tonemapping::SomewhatBoringDisplayTransform,
            TonemappingDef::TonyMcMapface => Tonemapping::TonyMcMapface,
            TonemappingDef::BlenderFilmic => Tonemapping::BlenderFilmic,
        }
    }

    pub fn clear_color(&self) -> ClearColorConfig {
        match self.background {
            Some([r, g, b]) => ClearColorConfig::Custom(Color::srgb(r, g, b)),
            None => ClearColorConfig::Default,
        }
    }
}

pub fn spawn_preview_cameras(mut commands: Commands, settings: Res<RenderSettings>) {
    let orbit = &settings.orbit;
    commands.spawn((
        PreviewCamera::Orbit,
        Camera3d::default(),
        // Hdr + screen-space transmission for distortion layers (Bevy copies
        // the screen only while one draws).
        omagari::distortion_camera(),
        Camera::default(),
        orbit.projection_component(),
        Transform::from_translation(orbit.offset()).looking_at(Vec3::ZERO, Vec3::Y),
        PanOrbitCamera {
            yaw: Some(orbit.yaw.to_radians()),
            pitch: Some(orbit.pitch.to_radians()),
            radius: Some(orbit.orbit_radius()),
            // Pause and slow motion (virtual time) must not freeze the camera.
            use_real_time: true,
            ..default()
        },
    ));
    let game = &settings.game;
    commands.spawn((
        PreviewCamera::Game,
        Camera3d::default(),
        omagari::distortion_camera(),
        Camera {
            is_active: false,
            ..default()
        },
        game.projection_component(),
        Transform::from_translation(game.offset()).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

/// Applies the render profiles when they change, and activates the camera
/// the project asks for.
pub fn apply_render_profiles(
    mut commands: Commands,
    settings: Res<RenderSettings>,
    project: Res<ProjectV2>,
    mut cameras: Query<(Entity, &PreviewCamera, &mut Camera, &mut Projection, Option<&mut PanOrbitCamera>)>,
) {
    let active = PreviewCamera::of(&project);
    for (entity, kind, mut camera, mut projection, orbit) in cameras.iter_mut() {
        let profile = kind.profile(&settings);
        if settings.is_changed() {
            *projection = profile.projection_component();
            camera.clear_color = profile.clear_color();
            let mut entity = commands.entity(entity);
            entity.insert((profile.tonemapping_component(), Exposure { ev100: profile.exposure }));
            match profile.bloom_component() {
                Some(bloom) => entity.insert(bloom),
                None => entity.remove::<Bloom>(),
            };
            if let Some(mut orbit) = orbit {
                // The orbit camera eases to the profile's view.
                orbit.target_yaw = profile.yaw.to_radians();
                orbit.target_pitch = profile.pitch.to_radians();
                orbit.target_radius = profile.orbit_radius();
                orbit.force_update = true;
            }
        }
        let is_active = *kind == active;
        if camera.is_active != is_active {
            camera.is_active = is_active;
        }
    }
}

/// The game camera keeps its offset from the (moving) preview target.
pub fn follow_preview_target(
    settings: Res<RenderSettings>,
    target: Single<&Transform, With<PreviewTarget>>,
    mut cameras: Query<(&PreviewCamera, &mut Transform), Without<PreviewTarget>>,
) {
    for (kind, mut transform) in cameras.iter_mut() {
        if *kind != PreviewCamera::Game {
            continue;
        }
        let focus = target.translation;
        let wanted = Transform::from_translation(focus + settings.game.offset()).looking_at(focus, Vec3::Y);
        if *transform != wanted {
            *transform = wanted;
        }
    }
}

/// F: the orbit camera turns to the preview target at the profile's
/// distance (the game camera always follows it).
pub fn focus_camera(
    mut session: ResMut<EditorSession>,
    settings: Res<RenderSettings>,
    target: Single<&Transform, With<PreviewTarget>>,
    mut orbits: Query<&mut PanOrbitCamera>,
) {
    if !std::mem::take(&mut session.focus_camera) {
        return;
    }
    for mut orbit in orbits.iter_mut() {
        orbit.target_focus = target.translation;
        orbit.target_radius = settings.orbit.orbit_radius();
        orbit.force_update = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_profile_is_orthographic_with_16_m_height() {
        let Projection::Orthographic(ortho) = CameraProfile::game().projection_component() else {
            panic!("not orthographic");
        };
        assert!(matches!(ortho.scaling_mode, ScalingMode::FixedVertical { viewport_height } if viewport_height == 16.0));
        assert!(CameraProfile::game().bloom_component().is_some());
        let mut off = CameraProfile::game();
        off.bloom = BloomDef::Off;
        assert!(off.bloom_component().is_none());
    }

    #[test]
    fn camera_choice_round_trips_through_the_project() {
        let mut project = ProjectV2::default();
        assert_eq!(PreviewCamera::of(&project), PreviewCamera::Orbit);
        PreviewCamera::Orbit.store(&mut project);
        assert!(project.preview.is_none());
        PreviewCamera::Game.store(&mut project);
        assert_eq!(project.preview.as_ref().unwrap().camera, CameraChoice::Game);
        assert_eq!(PreviewCamera::of(&project), PreviewCamera::Game);
    }
}
