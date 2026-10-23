//! Editor settings: `omagari_settings.ron` in the repository root (the
//! working directory), plus command line and environment overrides.
//!
//! **Game assets folder**: the game's `assets/` folder. Project asset paths
//! (`vfx/textures/x.png`) are relative to it; the editor loads them through an
//! asset source named [`GAME_ASSET_SOURCE`], and EXPORT writes packages to its
//! `vfx/` folder. Precedence: `--game-assets <dir>` > `OMAGARI_GAME_ASSETS` >
//! the settings file. Without one, the repository's `assets/` folder serves as
//! the game folder and exports go to `exported_effects/`.
//!
//! **Render profiles**: how the preview camera looks for each camera preset
//! (`Orbit`, `Game`), see [`RenderSettings`]. Applied live from the settings
//! window; a project picks its preset in `preview.camera`.

use std::path::{Path, PathBuf};

use bevy::asset::io::{AssetSourceBuilder, AssetSourceId};
use bevy::asset::AssetApp;
use bevy::prelude::*;
use ron::ser::PrettyConfig;
use serde::de::{self, Deserializer, IgnoredAny, MapAccess, Visitor};
use serde::{Deserialize, Serialize};

/// File the settings are stored in, relative to the working directory.
pub const SETTINGS_FILE: &str = "omagari_settings.ron";
/// Command line flag for the game assets folder.
pub const GAME_ASSETS_FLAG: &str = "--game-assets";
/// Environment variable for the game assets folder.
pub const GAME_ASSETS_ENV: &str = "OMAGARI_GAME_ASSETS";
/// Name of the asset source serving the game assets folder in the editor.
pub const GAME_ASSET_SOURCE: &str = "game";
/// Folder of the game assets folder that exported packages go to.
pub const GAME_VFX_DIR: &str = "vfx";

/// Contents of `omagari_settings.ron`. Missing fields take their defaults, so
/// older and newer files load.
#[derive(Serialize, Deserialize, Clone, Default, PartialEq, Debug)]
#[serde(default)]
pub struct OmagariSettings {
    /// The game's `assets/` folder.
    pub game_assets: Option<String>,
    /// Camera presets of the preview.
    pub render: RenderSettings,
}

impl OmagariSettings {
    /// Reads the settings file; a missing or broken file gives the defaults
    /// (a broken one is reported on stderr).
    pub fn load(path: &Path) -> Self {
        let Ok(text) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        ron::from_str(&text).unwrap_or_else(|e| {
            eprintln!("omagari: {}: {}; using default settings", path.display(), e);
            Self::default()
        })
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let text = ron::ser::to_string_pretty(self, PrettyConfig::new().new_line("\n".to_string()))
            .map_err(std::io::Error::other)?;
        std::fs::write(path, text)
    }
}

/// The render profiles of the two camera presets.
///
/// In the file, a profile may list only some values; the others take that
/// preset's defaults (not the other preset's), so `game: (height: 20.0)`-style
/// partial edits keep the isometric angles.
#[derive(Resource, Serialize, Clone, PartialEq, Debug)]
pub struct RenderSettings {
    /// Free camera: drag to orbit, wheel to zoom. Yaw/pitch/distance are the
    /// starting view (and the view F returns to).
    pub orbit: CameraProfile,
    /// The game's camera: fixed angles, follows the preview target.
    pub game: CameraProfile,
}

impl Default for RenderSettings {
    fn default() -> Self {
        RenderSettings {
            orbit: CameraProfile::orbit(),
            game: CameraProfile::game(),
        }
    }
}

/// How the preview camera looks and where it sits relative to the target.
#[derive(Serialize, Clone, PartialEq, Debug)]
pub struct CameraProfile {
    pub projection: ProjectionDef,
    /// Degrees around the vertical axis: 0 = the camera is on +Z looking
    /// towards -Z, 45 = on the +X/+Z diagonal.
    pub yaw: f32,
    /// Degrees above the horizon.
    pub pitch: f32,
    /// Metres from the target.
    pub distance: f32,
    pub tonemapping: TonemappingDef,
    pub bloom: BloomDef,
    /// Overrides the bloom preset's intensity; `None` = the preset's.
    pub bloom_intensity: Option<f32>,
    /// Camera exposure (EV100); higher = darker lit surfaces.
    pub exposure: f32,
    /// Clear colour (sRGB 0..1); `None` = the backdrop's colour.
    pub background: Option<[f32; 3]>,
}

impl CameraProfile {
    /// Bevy's exposure default (calibrated to Blender).
    pub const DEFAULT_EXPOSURE: f32 = 9.7;

    /// The editor's free camera: perspective, a little above the target.
    pub fn orbit() -> Self {
        CameraProfile {
            projection: ProjectionDef::Perspective { fov: 45.0 },
            yaw: 0.0,
            pitch: (1.5f32 / 5.0).atan().to_degrees(),
            distance: (1.5f32 * 1.5 + 5.0 * 5.0).sqrt(),
            tonemapping: TonemappingDef::TonyMcMapface,
            bloom: BloomDef::Natural,
            bloom_intensity: None,
            exposure: Self::DEFAULT_EXPOSURE,
            background: None,
        }
    }

    /// The game camera: orthographic true isometric (camera offset along
    /// (1, 1, 1): yaw 45°, pitch atan(1/√2) ≈ 35.26°), 16 m visible height,
    /// HDR + bloom.
    pub fn game() -> Self {
        CameraProfile {
            projection: ProjectionDef::Orthographic { height: 16.0 },
            yaw: 45.0,
            pitch: (1.0f32 / 2.0f32.sqrt()).atan().to_degrees(),
            distance: 20.0 * 3.0f32.sqrt(),
            tonemapping: TonemappingDef::TonyMcMapface,
            bloom: BloomDef::Natural,
            bloom_intensity: None,
            exposure: Self::DEFAULT_EXPOSURE,
            background: None,
        }
    }

    /// Unit vector from the target towards the camera.
    pub fn direction(&self) -> Vec3 {
        let (yaw, pitch) = (self.yaw.to_radians(), self.pitch.to_radians());
        Vec3::new(pitch.cos() * yaw.sin(), pitch.sin(), pitch.cos() * yaw.cos())
    }

    /// Camera offset from the target.
    pub fn offset(&self) -> Vec3 {
        self.direction() * self.distance
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
pub enum ProjectionDef {
    /// Vertical field of view in degrees.
    Perspective { fov: f32 },
    /// Visible height in metres; no perspective.
    Orthographic { height: f32 },
}

/// Bevy's tonemapping operators (`Tonemapping`).
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum TonemappingDef {
    None,
    Reinhard,
    ReinhardLuminance,
    AcesFitted,
    AgX,
    SomewhatBoringDisplayTransform,
    TonyMcMapface,
    BlenderFilmic,
}

impl TonemappingDef {
    pub const ALL: [TonemappingDef; 8] = [
        TonemappingDef::None,
        TonemappingDef::Reinhard,
        TonemappingDef::ReinhardLuminance,
        TonemappingDef::AcesFitted,
        TonemappingDef::AgX,
        TonemappingDef::SomewhatBoringDisplayTransform,
        TonemappingDef::TonyMcMapface,
        TonemappingDef::BlenderFilmic,
    ];
}

/// Bevy's bloom presets (`Bloom::NATURAL`, ...).
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum BloomDef {
    Off,
    Natural,
    OldSchool,
    ScreenBlur,
    Anamorphic,
}

impl BloomDef {
    pub const ALL: [BloomDef; 5] =
        [BloomDef::Off, BloomDef::Natural, BloomDef::OldSchool, BloomDef::ScreenBlur, BloomDef::Anamorphic];
}

#[derive(Deserialize)]
#[serde(field_identifier, rename_all = "snake_case")]
enum ProfileField {
    Projection,
    Yaw,
    Pitch,
    Distance,
    Tonemapping,
    Bloom,
    BloomIntensity,
    Exposure,
    Background,
    #[serde(other)]
    Unknown,
}

const PROFILE_FIELDS: &[&str] = &[
    "projection",
    "yaw",
    "pitch",
    "distance",
    "tonemapping",
    "bloom",
    "bloom_intensity",
    "exposure",
    "background",
];

/// Reads a profile over the given defaults: missing values keep them,
/// unknown ones are ignored.
struct ProfileOver(CameraProfile);

impl<'de> de::DeserializeSeed<'de> for ProfileOver {
    type Value = CameraProfile;
    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<CameraProfile, D::Error> {
        deserializer.deserialize_struct("CameraProfile", PROFILE_FIELDS, self)
    }
}

impl<'de> Visitor<'de> for ProfileOver {
    type Value = CameraProfile;
    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("a camera profile")
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<CameraProfile, A::Error> {
        let mut p = self.0;
        while let Some(field) = map.next_key::<ProfileField>()? {
            match field {
                ProfileField::Projection => p.projection = map.next_value()?,
                ProfileField::Yaw => p.yaw = map.next_value()?,
                ProfileField::Pitch => p.pitch = map.next_value()?,
                ProfileField::Distance => p.distance = map.next_value()?,
                ProfileField::Tonemapping => p.tonemapping = map.next_value()?,
                ProfileField::Bloom => p.bloom = map.next_value()?,
                ProfileField::BloomIntensity => p.bloom_intensity = map.next_value()?,
                ProfileField::Exposure => p.exposure = map.next_value()?,
                ProfileField::Background => p.background = map.next_value()?,
                ProfileField::Unknown => {
                    map.next_value::<IgnoredAny>()?;
                }
            }
        }
        Ok(p)
    }
}

#[derive(Deserialize)]
#[serde(field_identifier, rename_all = "snake_case")]
enum RenderField {
    Orbit,
    Game,
    #[serde(other)]
    Unknown,
}

struct RenderVisitor;

impl<'de> Visitor<'de> for RenderVisitor {
    type Value = RenderSettings;
    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("render settings")
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<RenderSettings, A::Error> {
        let mut settings = RenderSettings::default();
        while let Some(field) = map.next_key::<RenderField>()? {
            match field {
                RenderField::Orbit => settings.orbit = map.next_value_seed(ProfileOver(CameraProfile::orbit()))?,
                RenderField::Game => settings.game = map.next_value_seed(ProfileOver(CameraProfile::game()))?,
                RenderField::Unknown => {
                    map.next_value::<IgnoredAny>()?;
                }
            }
        }
        Ok(settings)
    }
}

impl<'de> Deserialize<'de> for RenderSettings {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_struct("RenderSettings", &["orbit", "game"], RenderVisitor)
    }
}

/// Where the game assets folder setting came from.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SettingOrigin {
    CommandLine,
    Environment,
    SettingsFile,
}

impl SettingOrigin {
    pub fn describe(self) -> String {
        match self {
            SettingOrigin::CommandLine => format!("{} flag", GAME_ASSETS_FLAG),
            SettingOrigin::Environment => format!("{} variable", GAME_ASSETS_ENV),
            SettingOrigin::SettingsFile => SETTINGS_FILE.to_string(),
        }
    }
}

/// The game assets folder in effect for this run (fixed at startup, because
/// asset sources are registered before the asset plugin).
#[derive(Resource, Clone, Default, PartialEq, Debug)]
pub struct GameAssets {
    /// The configured folder (absolute) and where the setting came from;
    /// `None` = not configured.
    pub configured: Option<(PathBuf, SettingOrigin)>,
}

impl GameAssets {
    /// Applies the precedence: command line > environment > settings file.
    /// Empty values count as unset. Relative folders are made absolute
    /// against the working directory.
    pub fn resolve(cli: Option<String>, env: Option<String>, file: &OmagariSettings) -> Self {
        let candidates = [
            (cli, SettingOrigin::CommandLine),
            (env, SettingOrigin::Environment),
            (file.game_assets.clone(), SettingOrigin::SettingsFile),
        ];
        let configured = candidates.into_iter().find_map(|(value, origin)| {
            let value = value?.trim().to_string();
            if value.is_empty() {
                return None;
            }
            let path = PathBuf::from(&value);
            Some((std::path::absolute(&path).unwrap_or(path), origin))
        });
        GameAssets { configured }
    }

    /// Reads the command line, the environment and the settings file in the
    /// working directory.
    pub fn from_environment() -> Self {
        let args: Vec<String> = std::env::args().collect();
        let cli = args
            .iter()
            .position(|a| a == GAME_ASSETS_FLAG)
            .and_then(|i| args.get(i + 1).cloned());
        let env = std::env::var(GAME_ASSETS_ENV).ok();
        Self::resolve(cli, env, &OmagariSettings::load(Path::new(SETTINGS_FILE)))
    }

    pub fn folder(&self) -> Option<&Path> {
        self.configured.as_ref().map(|(p, _)| p.as_path())
    }

    /// Folder relative texture paths are looked up in (checks): the game
    /// assets folder, or this repository's `assets/`.
    pub fn assets_root(&self) -> PathBuf {
        self.folder().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("assets"))
    }

    /// Folder exported packages are written to: `<game assets>/vfx/` when
    /// configured, else `exported_effects/`.
    pub fn export_dir(&self) -> PathBuf {
        match self.folder() {
            Some(folder) => folder.join(GAME_VFX_DIR),
            None => PathBuf::from(crate::controller::Folder::ExportedEffects.to_path()),
        }
    }

    /// The asset source builder for [`GAME_ASSET_SOURCE`]: the configured
    /// folder, or `assets/` (the same folder as the default source).
    fn source_builder(&self) -> AssetSourceBuilder {
        match self.folder() {
            Some(folder) => AssetSourceBuilder::platform_default(&folder.to_string_lossy(), None),
            None => AssetSourceBuilder::platform_default("assets", None),
        }
    }

    /// Registers the `game` asset source. Must run before `AssetPlugin`
    /// (`DefaultPlugins`).
    pub fn register_source(&self, app: &mut App) {
        app.register_asset_source(
            AssetSourceId::from(GAME_ASSET_SOURCE),
            self.source_builder(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(value: Option<&str>) -> OmagariSettings {
        OmagariSettings {
            game_assets: value.map(str::to_string),
            ..Default::default()
        }
    }

    fn resolved(cli: Option<&str>, env: Option<&str>, f: Option<&str>) -> Option<(PathBuf, SettingOrigin)> {
        GameAssets::resolve(cli.map(str::to_string), env.map(str::to_string), &file(f)).configured
    }

    #[test]
    fn command_line_beats_environment_beats_file() {
        let abs = |p: &str| std::path::absolute(p).unwrap();
        assert_eq!(
            resolved(Some("cli"), Some("env"), Some("file")),
            Some((abs("cli"), SettingOrigin::CommandLine))
        );
        assert_eq!(
            resolved(None, Some("env"), Some("file")),
            Some((abs("env"), SettingOrigin::Environment))
        );
        assert_eq!(
            resolved(None, None, Some("file")),
            Some((abs("file"), SettingOrigin::SettingsFile))
        );
        assert_eq!(resolved(None, None, None), None);
    }

    #[test]
    fn empty_values_count_as_unset() {
        let abs = |p: &str| std::path::absolute(p).unwrap();
        assert_eq!(
            resolved(Some(""), Some("  "), Some("file")),
            Some((abs("file"), SettingOrigin::SettingsFile))
        );
        assert_eq!(resolved(None, None, Some("")), None);
    }

    #[test]
    fn export_dir_follows_the_game_folder() {
        let none = GameAssets::default();
        assert_eq!(none.export_dir(), PathBuf::from("exported_effects/"));
        let game = GameAssets::resolve(Some("C:/game/assets".into()), None, &file(None));
        assert_eq!(game.export_dir(), PathBuf::from("C:/game/assets").join("vfx"));
    }

    #[test]
    fn settings_file_round_trips_and_tolerates_unknown_fields() {
        let dir = std::env::temp_dir().join(format!("omagari_settings_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(SETTINGS_FILE);
        assert_eq!(OmagariSettings::load(&path), OmagariSettings::default(), "missing file");
        let settings = file(Some("../game/assets"));
        settings.save(&path).unwrap();
        assert_eq!(OmagariSettings::load(&path), settings);
        std::fs::write(&path, "(future_setting: 3, game_assets: Some(\"x\"))").unwrap();
        assert_eq!(OmagariSettings::load(&path), file(Some("x")));
        std::fs::write(&path, "()").unwrap();
        assert_eq!(OmagariSettings::load(&path), OmagariSettings::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn game_profile_is_true_isometric() {
        let game = CameraProfile::game();
        assert!((game.direction() - Vec3::ONE.normalize()).length() < 1e-5, "{}", game.direction());
        assert!((game.offset() - Vec3::splat(20.0)).length() < 1e-3, "{}", game.offset());
        assert!((game.pitch - 35.264).abs() < 1e-2);
        assert_eq!(game.projection, ProjectionDef::Orthographic { height: 16.0 });
        assert_ne!(game.bloom, BloomDef::Off);
        // The orbit profile starts where the editor camera always started.
        let orbit = CameraProfile::orbit();
        assert!((orbit.offset() - Vec3::new(0.0, 1.5, 5.0)).length() < 1e-4, "{}", orbit.offset());
    }

    /// Missing render values take the defaults of their own preset; unknown
    /// ones are ignored; files without `render` keep working.
    #[test]
    fn render_profiles_fill_in_their_own_defaults() {
        let settings: OmagariSettings = ron::from_str("(game_assets: None)").unwrap();
        assert_eq!(settings.render, RenderSettings::default());

        let settings: OmagariSettings =
            ron::from_str("(render: (game: (projection: Orthographic(height: 20.0), future: 1), orbit: (yaw: 30.0)))")
                .unwrap();
        let game = &settings.render.game;
        assert_eq!(game.projection, ProjectionDef::Orthographic { height: 20.0 });
        assert_eq!(game.yaw, 45.0, "game default, not orbit's");
        assert_eq!(game.distance, CameraProfile::game().distance);
        assert_eq!(settings.render.orbit.yaw, 30.0);
        assert_eq!(settings.render.orbit.pitch, CameraProfile::orbit().pitch);

        let settings: OmagariSettings = ron::from_str("(render: ())").unwrap();
        assert_eq!(settings.render, RenderSettings::default());
    }

    #[test]
    fn render_profiles_round_trip() {
        let mut settings = OmagariSettings::default();
        settings.render.game.background = Some([0.1, 0.2, 0.3]);
        settings.render.game.bloom = BloomDef::OldSchool;
        settings.render.game.bloom_intensity = Some(0.3);
        settings.render.orbit.tonemapping = TonemappingDef::AgX;
        let text = ron::ser::to_string_pretty(&settings, PrettyConfig::new()).unwrap();
        let back: OmagariSettings = ron::from_str(&text).unwrap();
        assert_eq!(back, settings, "{}", text);
    }
}
