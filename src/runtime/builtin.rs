//! Built-in textures embedded in the library, so games need not copy them.
//!
//! Packages reference them as `omagari://textures/<file>`. They are stored in
//! Bevy's `embedded` asset source (`embedded://omagari/textures/<file>`):
//! a separate `omagari` source would have to be registered before
//! `AssetPlugin`, i.e. before `DefaultPlugins`, which `OmagariPlugin` cannot
//! require. [`resolve_asset_path`] maps the package path to the loadable one.

use std::path::{Path, PathBuf};

use bevy::asset::io::embedded::EmbeddedAssetRegistry;
use bevy::prelude::*;

use crate::format::BUILTIN_TEXTURES_PREFIX;

/// The built-in textures: file name and PNG bytes. The particle textures
/// (same files and order as [`crate::format::PARTICLE_TEXTURES`]), then the
/// tileable noise of the effect shader
/// ([`crate::format::BUILTIN_NOISE_TEXTURE`]).
pub const BUILTIN_TEXTURES: [(&str, &[u8]); 8] = [
    ("cloud.png", include_bytes!("../../assets/cloud.png")),
    ("cloud2.png", include_bytes!("../../assets/cloud2.png")),
    ("spark1.png", include_bytes!("../../assets/spark1.png")),
    ("spark2.png", include_bytes!("../../assets/spark2.png")),
    ("spark3.png", include_bytes!("../../assets/spark3.png")),
    ("glow1.png", include_bytes!("../../assets/glow1.png")),
    ("splat1.png", include_bytes!("../../assets/splat1.png")),
    ("noise.png", include_bytes!("../../assets/noise.png")),
];

/// Folder of the built-in textures in the `embedded` asset source.
const EMBEDDED_TEXTURES_DIR: &str = "omagari/textures";

/// Adds the built-in textures to the `embedded` asset source. Needs
/// `AssetPlugin`; does nothing without it.
pub(crate) fn register_builtin_textures(app: &mut App) {
    let Some(registry) = app.world().get_resource::<EmbeddedAssetRegistry>() else {
        return;
    };
    for (file, bytes) in BUILTIN_TEXTURES {
        let full_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets").join(file);
        let asset_path = format!("{}/{}", EMBEDDED_TEXTURES_DIR, file);
        registry.insert_asset(full_path, Path::new(&asset_path), bytes);
    }
}

/// Turns a package asset path into a path the `AssetServer` can load from
/// the default asset source (what games do): `omagari://textures/cloud.png`
/// becomes `embedded://omagari/textures/cloud.png`; other paths (relative to
/// the game's `assets/` folder) are returned unchanged.
pub fn resolve_asset_path(path: &str) -> String {
    resolve_asset_path_in(path, None)
}

/// Like [`resolve_asset_path`], but paths relative to the game's `assets/`
/// folder are loaded from the asset source `source` (`Some("game")` gives
/// `game://vfx/textures/x.png`) instead of the default source. The editor
/// uses this, because its default source is its own `assets/` folder.
/// Built-in textures always come from the `embedded` source.
pub fn resolve_asset_path_in(path: &str, source: Option<&str>) -> String {
    if let Some(file) = path.strip_prefix(BUILTIN_TEXTURES_PREFIX) {
        return format!("embedded://{}/{}", EMBEDDED_TEXTURES_DIR, file);
    }
    match source {
        Some(source) if !path.contains("://") => format!("{}://{}", source, path),
        _ => path.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::{builtin_texture_path, BUILTIN_NOISE_TEXTURE, PARTICLE_TEXTURES};

    #[test]
    fn builtin_textures_match_editor_list() {
        let files: Vec<&str> = BUILTIN_TEXTURES.iter().map(|(f, _)| *f).collect();
        let editor: Vec<&str> = PARTICLE_TEXTURES.iter().map(|t| t.filename).collect();
        assert_eq!(files[..editor.len()], editor[..], "the v1 particle textures first, in order");
        assert_eq!(files[editor.len()..], [BUILTIN_NOISE_TEXTURE]);
        for (_, bytes) in BUILTIN_TEXTURES {
            assert!(bytes.starts_with(b"\x89PNG"));
        }
    }

    #[test]
    fn builtin_paths_resolve_to_embedded_source() {
        assert_eq!(
            resolve_asset_path(&builtin_texture_path("cloud.png")),
            "embedded://omagari/textures/cloud.png"
        );
        assert_eq!(resolve_asset_path("vfx/textures/noise.png"), "vfx/textures/noise.png");
    }

    #[test]
    fn paths_resolve_in_a_named_source() {
        assert_eq!(
            resolve_asset_path_in("vfx/textures/noise.png", Some("game")),
            "game://vfx/textures/noise.png"
        );
        assert_eq!(
            resolve_asset_path_in(&builtin_texture_path("glow1.png"), Some("game")),
            "embedded://omagari/textures/glow1.png"
        );
        assert_eq!(
            resolve_asset_path_in("other://x.png", Some("game")),
            "other://x.png",
            "explicit sources are kept"
        );
    }
}
