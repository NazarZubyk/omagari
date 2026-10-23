use bevy::prelude::*;
use bevy_hanabi::prelude::*;
use std::fs::create_dir;
use std::io;
use std::path::{Path, PathBuf};

use omagari::format::{ParamKind, ParamValue, PreviewMotion, ProjectV2, TextureAssetDef};
use omagari::runtime::{apply_texture_settings_to, InMemoryPackageAssets};
use omagari::{TriggerArgs, VfxCommandsExt, VfxControl, VfxPackage};

use crate::editor::model::{random_point_on_bounds, ModelPreview};
use crate::editor::params_panel::preview_value;
use crate::editor::session::EditorSession;
use crate::editor::AppContext;
use crate::settings::{GameAssets, GAME_ASSET_SOURCE};

#[derive(Resource, Default)]
pub struct EffectResource {
    pub context: AppContext,
}

/// The preview's moving root (the game's entity root): the spawn point at
/// the origin (the stage moves the room around it, not the effect), moved by
/// the project's preview motion. Preview instances are attached to it, or
/// on the CAPSULE / MODEL stage to the node `preview.attach_to` names
/// ([`ModelPreview::instance_target`]).
#[derive(Component)]
pub struct PreviewTarget;

/// What the playback buttons, live preview and MCP preview requests ask for.
#[derive(Message, Clone, PartialEq, Debug)]
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
    /// Fire a trigger on the preview instance, without a payload or at a
    /// random point on the target (its meshes' bounds when it has a body,
    /// else near it).
    Fire { name: String, random_point: bool },
}

/// Frames a preview trigger waits for its instance (spawned the same frame)
/// before it is dropped.
const FIRE_WAIT_FRAMES: u32 = 30;

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
    /// The textures of the last build and their settings
    /// ([`sync_preview_texture_settings`]).
    pub textures: Vec<(Handle<Image>, TextureAssetDef)>,
    /// Triggers to fire on the preview instance, with the frames they have
    /// waited for it.
    pub pending_fires: Vec<(String, TriggerArgs, u32)>,
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
    root: Single<Entity, With<PreviewTarget>>,
    globals: Query<&GlobalTransform>,
    model: Option<Res<ModelPreview>>,
    mut state: ResMut<PreviewState>,
    // Both read the packages; only one is used at a time.
    mut packages_or_control: ParamSet<(ResMut<Assets<VfxPackage>>, VfxControl)>,
) {
    let target = model.as_ref().and_then(|m| m.instance_target).filter(|&e| globals.contains(e)).unwrap_or(*root);
    let target_transform = globals.get(target).copied().unwrap_or_default();
    let bounds = model.as_ref().and_then(|m| m.target_bounds);
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
                state.textures = package
                    .particle_layers
                    .iter()
                    .filter_map(|layer| {
                        let id = project.layers.get(layer.layer_index)?.as_particles()?.texture.as_ref()?;
                        Some((layer.texture.clone()?, project.texture(id)?.clone()))
                    })
                    .chain(package.surface_layers.iter().flat_map(|layer| {
                        let slots = layer.def.material.texture_slots();
                        // `textures` holds noise A, noise B, mask; the slots
                        // list only the parts that are on, in that order.
                        let on = [
                            layer.def.material.noise_a.is_some(),
                            layer.def.material.noise_b.is_some(),
                            layer.def.material.mask.is_some(),
                        ];
                        let mut slots = slots.into_iter();
                        let mut pairs = Vec::new();
                        for (handle, on) in layer.textures.iter().zip(on) {
                            let slot = if on { slots.next() } else { None };
                            if let (Some(handle), Some((_, slot))) = (handle, slot) {
                                if let Some(def) = project.texture(&slot.asset) {
                                    pairs.push((handle.clone(), def.clone()));
                                }
                            }
                        }
                        pairs
                    }))
                    .collect();
                // The previous package is dropped with its handle once its
                // instances are gone.
                let handle = packages_or_control.p0().add(package);
                let mut vfx = packages_or_control.p1();
                if matches!(request, PreviewCommand::Restart) {
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
                    let instance = commands.spawn_vfx(&handle, target);
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
            PreviewCommand::Fire { name, random_point } => {
                let args = if *random_point {
                    random_trigger_point(&target_transform, bounds)
                } else {
                    TriggerArgs::default()
                };
                state.pending_fires.push((name.clone(), args, 0));
            }
        }
    }
}

/// A random trigger payload: on the bounds of the target's meshes when it has
/// any (a model or the capsule), else near the target.
pub fn random_trigger_point(target: &GlobalTransform, bounds: Option<(Vec3, Vec3)>) -> TriggerArgs {
    let (a, b, c) = (rand::random(), rand::random(), rand::random());
    match bounds {
        Some((min, max)) => random_point_on_bounds(min, max, a, b, c),
        None => random_point_near(target, a, b, c),
    }
}

/// A trigger payload at a point near the target: within 1 m around it,
/// 0.3–1.5 m up, pointing away from it (a hit on a body). `a`, `b`, `c` are
/// random numbers in 0..1.
pub fn random_point_near(target: &GlobalTransform, a: f32, b: f32, c: f32) -> TriggerArgs {
    let offset = Vec3::new(a * 2.0 - 1.0, 0.3 + b * 1.2, c * 2.0 - 1.0);
    let outward = Vec3::new(offset.x, 0.0, offset.z).normalize_or(Vec3::X);
    TriggerArgs::at(target.translation() + offset).with_direction(outward)
}

/// Gives the preview's textures the settings the project asks for: Bevy
/// keeps a file's image with the settings of its first load, so a texture
/// whose wrap, colour space or filter was just edited would keep the old
/// ones until the editor restarts.
pub fn sync_preview_texture_settings(state: Res<PreviewState>, mut images: ResMut<Assets<Image>>) {
    for (handle, texture) in state.textures.iter() {
        apply_texture_settings_to(&mut images, handle, texture);
    }
}

/// What [`drive_preview`] last sent to the preview instance.
#[derive(Default)]
pub struct PreviewDrive {
    instance: Option<Entity>,
    values: Vec<(String, ParamValue)>,
    /// Seconds since each auto-fired trigger last fired.
    auto: Vec<(String, f32)>,
}

/// Drives the preview instance like a game would: the PARAMETERS tab's
/// values (`preview.params`, or the "animate" in progress) through
/// `set_param` (smoothed; a new instance gets them at once), the triggers
/// fired by the TRIGGERS tab and MCP, and the preview's auto-fire.
pub fn drive_preview(
    time: Res<Time>,
    project: Res<ProjectV2>,
    mut session: ResMut<EditorSession>,
    mut state: ResMut<PreviewState>,
    target: Single<&GlobalTransform, With<PreviewTarget>>,
    model: Option<Res<ModelPreview>>,
    mut vfx: VfxControl,
    mut drive: Local<PreviewDrive>,
) {
    let dt = time.delta_secs();
    let Some(&instance) = state.instances.last() else {
        drive.instance = None;
        state.pending_fires.clear();
        return;
    };

    // Parameters.
    if let Some(animation) = &mut session.param_animation {
        animation.elapsed += dt;
    }
    if session.param_animation.as_ref().is_some_and(|a| a.finished()) {
        session.param_animation = None;
    }
    let wanted: Vec<(String, ParamValue)> = project
        .parameters
        .iter()
        .map(|p| {
            let value = match &session.param_animation {
                Some(a) if a.name == p.name && p.kind() == ParamKind::Float => {
                    ParamValue::Float(p.min + (p.max - p.min) * a.fraction())
                }
                _ => preview_value(&project, p),
            };
            (p.name.clone(), value)
        })
        .collect();
    let fresh = drive.instance != Some(instance);
    for (name, value) in wanted.iter() {
        if fresh {
            vfx.set_param_now(instance, name, *value);
        } else if !drive.values.iter().any(|(n, v)| n == name && v == value) {
            vfx.set_param(instance, name, *value);
        }
    }
    drive.instance = Some(instance);
    drive.values = wanted;

    // Triggers from the UI and MCP; they wait for an instance spawned this
    // frame.
    let fires = std::mem::take(&mut state.pending_fires);
    for (name, args, waited) in fires {
        if !vfx.trigger_at(instance, &name, args) && waited < FIRE_WAIT_FRAMES {
            state.pending_fires.push((name, args, waited + 1));
        }
    }

    // Auto-fire.
    let auto_triggers = project.preview.as_ref().map(|p| p.auto_triggers.clone()).unwrap_or_default();
    drive.auto.retain(|(name, _)| auto_triggers.iter().any(|(n, _)| n == name));
    for (name, every) in auto_triggers.iter().filter(|(_, every)| *every > 0.0) {
        let index = match drive.auto.iter().position(|(n, _)| n == name) {
            Some(i) => i,
            None => {
                drive.auto.push((name.clone(), 0.0));
                drive.auto.len() - 1
            }
        };
        drive.auto[index].1 += dt;
        if drive.auto[index].1 >= *every {
            drive.auto[index].1 = drive.auto[index].1.rem_euclid(*every);
            let args = if session.auto_fire_random {
                random_trigger_point(*target, model.as_ref().and_then(|m| m.target_bounds))
            } else {
                TriggerArgs::default()
            };
            vfx.trigger_at(instance, name, args);
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

    #[test]
    fn random_trigger_points_are_near_the_target_and_point_outward() {
        let target = GlobalTransform::from_xyz(10.0, 0.0, 0.0);
        for (a, b, c) in [(0.0, 0.0, 0.0), (1.0, 1.0, 1.0), (0.5, 0.5, 0.9)] {
            let args = random_point_near(&target, a, b, c);
            let offset = args.position.unwrap() - target.translation();
            assert!(offset.x.abs() <= 1.0 && offset.z.abs() <= 1.0 && offset.y >= 0.3 && offset.y <= 1.5, "{}", offset);
            let dir = args.direction.unwrap();
            assert!((dir.length() - 1.0).abs() < 1e-5 && dir.y == 0.0, "{}", dir);
            assert!(dir.dot(Vec3::new(offset.x, 0.0, offset.z)) >= 0.0, "away from the target");
        }
    }

    /// The PARAMETERS tab's values and the TRIGGERS tab's FIRE reach the
    /// preview instance through the runtime API, like a game's calls.
    #[test]
    fn preview_values_and_fired_triggers_reach_the_instance() {
        use crate::editor::session::ParamAnimation;
        use omagari::format::ParamValue;

        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default(), bevy::image::ImagePlugin::default()))
            .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(0.1)))
            .init_asset::<EffectAsset>()
            .add_plugins(omagari::OmagariPlugin)
            .init_resource::<EditorSession>()
            .init_resource::<PreviewState>()
            .add_systems(Startup, spawn_preview_target)
            .add_systems(Update, drive_preview.before(omagari::runtime::VfxSystems));
        let project = ProjectV2::from_ron_str(
            r#"(
                parameters: [(name: "power", default: Float(0.0), min: 0.0, max: 2.0, smoothing: 0.0)],
                triggers: [(name: "hit", pulses: [(param: "power", to: 2.0, decay: 1.0)])],
                layers: [Particles((name: "a"))],
                preview: Some((params: [("power", Float(0.5))])),
            )"#,
        )
        .unwrap();
        let package = app.world_mut().resource_scope(|world, mut effects: Mut<Assets<EffectAsset>>| {
            let server = world.resource::<AssetServer>();
            VfxPackage::build(project.clone(), &mut (&mut *effects, server), &mut Vec::new())
        });
        app.insert_resource(project);
        let handle = app.world_mut().resource_mut::<Assets<VfxPackage>>().add(package);
        app.update();
        let target = app.world_mut().query_filtered::<Entity, With<PreviewTarget>>().single(app.world()).unwrap();
        let instance = app.world_mut().commands().spawn_vfx(&handle, target);
        app.world_mut().flush();
        app.world_mut().resource_mut::<PreviewState>().instances.push(instance);
        let power = |app: &App| {
            app.world().get::<omagari::VfxInstance>(instance).unwrap().params().get("power").and_then(|v| v.as_float())
        };

        app.update();
        app.update();
        assert_eq!(power(&app), Some(0.5), "the saved preview value");

        app.world_mut().resource_mut::<EditorSession>().param_animation =
            Some(ParamAnimation { name: "power".into(), elapsed: 1.0 });
        app.update();
        // 1.1 s of 4: 55% of the way up to max 2.
        assert!((power(&app).unwrap() - 1.1).abs() < 1e-4, "{:?}", power(&app));

        app.world_mut().resource_mut::<EditorSession>().param_animation = None;
        app.world_mut().resource_mut::<PreviewState>().pending_fires.push(("hit".into(), TriggerArgs::default(), 0));
        app.update();
        app.update();
        let pulsed = power(&app).unwrap();
        assert!(pulsed > 1.5, "the trigger's pulse: {}", pulsed);
        assert!(app.world().resource::<PreviewState>().pending_fires.is_empty());

        let mut project = app.world_mut().resource_mut::<ProjectV2>();
        project.preview.as_mut().unwrap().params[0].1 = ParamValue::Float(0.25);
        for _ in 0..12 {
            app.update();
        }
        assert_eq!(power(&app), Some(0.25), "a new slider value, after the pulse");
    }
}
