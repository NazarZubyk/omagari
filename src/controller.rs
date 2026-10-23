use bevy::prelude::*;
use bevy_hanabi::prelude::*;
use std::fs::create_dir;
use std::io;
use std::path::{Path, PathBuf};

use omagari::format::{PreviewMotion, ProjectV2};
use omagari::runtime::InMemoryPackageAssets;
use omagari::{VfxCommandsExt, VfxControl, VfxPackage};

use crate::editor::session::EditorSession;
use crate::editor::AppContext;
use crate::settings::{GameAssets, GAME_ASSET_SOURCE};

#[derive(Resource, Default)]
pub struct EffectResource {
    pub context: AppContext,
}

/// The entity preview instances are attached to: the spawn point at the
/// origin (the stage moves the room around it, not the effect), moved by the
/// project's preview motion.
#[derive(Component)]
pub struct PreviewTarget;

/// What the playback buttons, live preview and MCP preview requests ask for.
#[derive(Message, Clone, Copy, PartialEq, Eq, Debug)]
pub enum PreviewCommand {
    /// Rebuild the package from the open project and play it again: the
    /// running preview instance gets the new package (old particles finish,
    /// new layers start), or a new instance is spawned.
    Render,
    /// Stop the instance gracefully (no new particles, live ones finish).
    Stop,
    /// Remove the preview instances at once (no lingering particles) and
    /// play the project from the start in a new instance.
    Restart,
}

/// The preview package and its instances. Only these instances are touched
/// by the preview, so other `ParticleEffect`s in the world are left alone.
#[derive(Resource, Default)]
pub struct PreviewState {
    /// The last package built from the open project.
    pub package: Option<Handle<VfxPackage>>,
    /// Preview instances that may still be alive (a stopped one lingers
    /// until its particles died).
    pub instances: Vec<Entity>,
    /// Warnings from the last build.
    pub warnings: Vec<String>,
}

pub fn spawn_preview_target(mut commands: Commands) {
    commands.spawn((
        Name::new("preview target"),
        PreviewTarget,
        Transform::default(),
        Visibility::default(),
    ));
}

/// Applies RENDER / STOP / RESTART: RENDER builds a [`VfxPackage`] from the
/// in-memory project (hidden layers masked out, see
/// [`crate::editor::layer_view::LayerView`]) and spawns it through the
/// runtime on the preview target, exactly like a game would; STOP stops the
/// running instances gracefully.
pub fn apply_preview_commands(
    mut commands: Commands,
    mut requests: MessageReader<PreviewCommand>,
    project: Res<ProjectV2>,
    session: Res<EditorSession>,
    mut effects: ResMut<Assets<EffectAsset>>,
    asset_server: Res<AssetServer>,
    target: Single<Entity, With<PreviewTarget>>,
    mut state: ResMut<PreviewState>,
    // Both read the packages; only one is used at a time.
    mut packages_or_control: ParamSet<(ResMut<Assets<VfxPackage>>, VfxControl)>,
) {
    for request in requests.read() {
        match request {
            PreviewCommand::Render | PreviewCommand::Restart => {
                let mut warnings = Vec::new();
                let package = VfxPackage::build(
                    session.view.apply(&project),
                    &mut InMemoryPackageAssets::new(&mut effects, &asset_server)
                        .with_source(Some(GAME_ASSET_SOURCE)),
                    &mut warnings,
                );
                for warning in warnings.iter() {
                    warn!("preview: {}", warning);
                }
                state.warnings = warnings;
                // The previous package is dropped with its handle once its
                // instances are gone.
                let handle = packages_or_control.p0().add(package);
                let mut vfx = packages_or_control.p1();
                if *request == PreviewCommand::Restart {
                    for instance in state.instances.drain(..) {
                        commands.entity(instance).try_despawn();
                    }
                }
                state.instances.retain(|&i| vfx.is_alive(i));
                // Same path as hot reload: the instance keeps running with the
                // new package.
                let replaced = state
                    .instances
                    .last()
                    .is_some_and(|&i| vfx.replace_package(i, handle.clone()));
                if !replaced {
                    let instance = commands.spawn_vfx(&handle, *target);
                    state.instances.push(instance);
                }
                state.package = Some(handle);
            }
            PreviewCommand::Stop => {
                let mut vfx = packages_or_control.p1();
                let instances = std::mem::take(&mut state.instances);
                state.instances = instances
                    .into_iter()
                    .filter(|&instance| vfx.stop(instance))
                    .collect();
            }
        }
    }
}

/// Moves the preview target along the project's preview motion, facing the
/// direction of travel; back to the origin without motion.
pub fn move_preview_target(
    time: Res<Time>,
    project: Res<ProjectV2>,
    mut clock: Local<f32>,
    mut last: Local<Option<PreviewMotion>>,
    mut target: Single<&mut Transform, With<PreviewTarget>>,
) {
    let motion = project.preview.as_ref().and_then(|p| p.motion);
    if motion != *last {
        // A new motion starts from its beginning.
        *clock = 0.0;
        *last = motion;
    }
    let wanted = match motion {
        Some(motion) => {
            *clock += time.delta_secs();
            let (position, direction) = motion.sample(*clock);
            Transform::from_translation(position).looking_to(direction, Vec3::Y)
        }
        None => Transform::default(),
    };
    if **target != wanted {
        **target = wanted;
    }
}

/// Writes the package export of a project: `<dest_dir>/<base>.vfx.ron`, where
/// `base` is the project file name without its extensions. Returns the path.
pub fn export_package(project: &ProjectV2, project_filename: &str, dest_dir: &Path) -> io::Result<PathBuf> {
    let base = project_filename.split('.').next().unwrap_or_default();
    if base.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "the project has no file name"));
    }
    let text = project
        .to_package()
        .to_ron_string()
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    std::fs::create_dir_all(dest_dir)?;
    let path = dest_dir.join(format!("{}.vfx.ron", base));
    std::fs::write(&path, text)?;
    Ok(path)
}

/// EXPORT: the package goes to the game assets folder's `vfx/` when one is
/// configured, else to `exported_effects/`.
pub fn export_effects_to_files(
    filename: &str,
    project: &ProjectV2,
    game_assets: &GameAssets,
) -> io::Result<PathBuf> {
    export_package(project, filename, &game_assets.export_dir())
}

pub fn projects_list() -> Vec<String> {
    let mut files = Vec::new();
    Folder::SavedEffects.make_folder();
    let saved_effects_path = String::from(Folder::SavedEffects.to_path());
    let entries = std::fs::read_dir(saved_effects_path).unwrap();
    for entry in entries {
        let entry = entry.unwrap();
        let filename = entry.file_name();
        if filename.to_string_lossy().ends_with(".omagari.ron") {
            files.push(filename.to_string_lossy().into_owned());
        }
    }
    files.sort();
    files
}

/// Loads a project from `saved_effects/`; v1 files are migrated to v2.
pub fn load_project(filename: &str) -> Result<ProjectV2, io::Error> {
    let file_path = Folder::SavedEffects.full_file_path(String::from(filename));
    let ron_string = std::fs::read_to_string(file_path)?;
    ProjectV2::from_ron_str(&ron_string).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// Saves a project (always v2) to `saved_effects/`.
pub fn save_project(filename: &str, project: &ProjectV2) -> Result<PathBuf, io::Error> {
    let text = project
        .to_ron_string()
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let file_path = Folder::SavedEffects.full_file_path(String::from(filename));
    std::fs::write(&file_path, text)?;
    Ok(file_path)
}

pub fn validate_project_filename(filename: &str) -> bool {
    regex::Regex::new(r".*\.omagari\.ron")
        .unwrap()
        .captures(&filename)
        .is_some()
}


type FolderPath = &'static str;
pub enum Folder{
    SavedEffects,
    ExportedEffects,
}

impl Folder{
    pub fn make_folder(&self) -> FolderPath{
        let folder_path = self.to_path();
        let _ = create_dir(String::from(folder_path));
        folder_path
    }

    pub fn full_file_path(&self, file_name: String) -> PathBuf{
        let folder_path = self.make_folder();
        PathBuf::from(String::from(folder_path)).join(file_name)
    }

    pub fn to_path(&self) -> FolderPath {
        match self{
            Folder::SavedEffects => "saved_effects/",
            Folder::ExportedEffects => "exported_effects/",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::asset::AssetPlugin;
    use bevy::image::{CompressedImageFormats, ImageLoader};
    use omagari::format::{
        builtin_texture_path, AssetDef, AssetKind, LayerDef, ParticlesLayerDef, PreviewDef,
        TextureAssetDef, TexturePurpose,
    };
    use std::time::Duration;

    fn sample_project() -> ProjectV2 {
        let mut project = ProjectV2::default();
        project.assets.push(AssetDef {
            id: "glow1".into(),
            kind: AssetKind::Texture(TextureAssetDef::new(builtin_texture_path("glow1.png"), TexturePurpose::Sprite)),
        });
        project.layers.push(LayerDef::Particles(ParticlesLayerDef {
            name: "dots".into(),
            texture: Some("glow1".into()),
            ..default()
        }));
        project.preview = Some(PreviewDef::default());
        project
    }

    /// EXPORT writes `<name>.vfx.ron` without the preview block, and the
    /// runtime's package loader reads it back.
    #[test]
    fn export_writes_a_package_the_runtime_loads() {
        let dir = std::env::temp_dir().join(format!("omagari_export_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = export_package(&sample_project(), "sparkle.omagari.ron", &dir.join("vfx")).unwrap();
        assert_eq!(path, dir.join("vfx").join("sparkle.vfx.ron"));
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("version: 2"), "{}", text);
        assert!(!text.contains("preview"), "{}", text);

        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            AssetPlugin {
                file_path: dir.to_string_lossy().to_string(),
                ..default()
            },
            bevy::image::ImagePlugin::default(),
        ))
        .register_asset_loader(ImageLoader::new(CompressedImageFormats::empty()))
        .init_asset::<EffectAsset>()
        .add_plugins(omagari::OmagariPlugin);
        let package: Handle<VfxPackage> = app.world().resource::<AssetServer>().load("vfx/sparkle.vfx.ron");
        for _ in 0..500 {
            app.update();
            let server = app.world().resource::<AssetServer>();
            if server.is_loaded_with_dependencies(&package) || server.load_state(&package).is_failed() {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let server = app.world().resource::<AssetServer>();
        assert!(
            server.is_loaded_with_dependencies(&package),
            "{:?}",
            server.recursive_dependency_load_state(&package)
        );
        let packages = app.world().resource::<Assets<VfxPackage>>();
        let package = packages.get(&package).unwrap();
        assert_eq!(package.particle_layers.len(), 1);
        assert_eq!(package.particle_layers[0].name, "dots");
        assert!(package.project.preview.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The preview target follows the project's preview motion, facing where
    /// it goes, and returns to the origin without one.
    #[test]
    fn preview_target_follows_the_motion() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(0.25)))
            .add_systems(Startup, spawn_preview_target)
            .add_systems(Update, move_preview_target);
        let mut project = ProjectV2::default();
        project.preview = Some(PreviewDef {
            motion: Some(PreviewMotion::BackAndForth { distance: 4.0, speed: 2.0 }),
            ..default()
        });
        app.insert_resource(project);
        for _ in 0..4 {
            app.update();
        }
        let mut targets = app.world_mut().query_filtered::<&Transform, With<PreviewTarget>>();
        let transform = *targets.single(app.world()).unwrap();
        assert!(transform.translation.x > 0.4 && transform.translation.x <= 2.0, "{}", transform.translation);
        assert!((transform.forward().as_vec3() - Vec3::X).length() < 1e-4, "faces +X");

        app.world_mut().resource_mut::<ProjectV2>().preview = None;
        app.update();
        let transform = *targets.single(app.world()).unwrap();
        assert_eq!(transform, Transform::default());
    }

    #[test]
    fn export_needs_a_file_name() {
        let dir = std::env::temp_dir();
        assert!(export_package(&sample_project(), "", &dir).is_err());
    }
}
