//! Editor settings: `omagari_settings.ron` in the repository root (the
//! working directory), plus command line and environment overrides.
//!
//! **Game assets folder**: the game's `assets/` folder. Project asset paths
//! (`vfx/textures/x.png`) are relative to it; the editor loads them through an
//! asset source named [`GAME_ASSET_SOURCE`], and EXPORT writes packages to its
//! `vfx/` folder. Precedence: `--game-assets <dir>` > `OMAGARI_GAME_ASSETS` >
//! the settings file. Without one, the repository's `assets/` folder serves as
//! the game folder and exports go to `exported_effects/`.

use std::path::{Path, PathBuf};

use bevy::asset::io::{AssetSourceBuilder, AssetSourceId};
use bevy::asset::AssetApp;
use bevy::prelude::*;
use ron::ser::PrettyConfig;
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
}
