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

/// The built-in textures: file name and PNG bytes. Same files and order as
/// [`crate::format::PARTICLE_TEXTURES`].
pub const BUILTIN_TEXTURES: [(&str, &[u8]); 7] = [
    ("cloud.png", include_bytes!("../../assets/cloud.png")),
    ("cloud2.png", include_bytes!("../../assets/cloud2.png")),
    ("spark1.png", include_bytes!("../../assets/spark1.png")),
    ("spark2.png", include_bytes!("../../assets/spark2.png")),
    ("spark3.png", include_bytes!("../../assets/spark3.png")),
    ("glow1.png", include_bytes!("../../assets/glow1.png")),
    ("splat1.png", include_bytes!("../../assets/splat1.png")),
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

/// Turns a package asset path into a path the `AssetServer` can load:
/// `omagari://textures/cloud.png` becomes
/// `embedded://omagari/textures/cloud.png`; other paths (relative to the
/// game's `assets/` folder) are returned unchanged.
pub fn resolve_asset_path(path: &str) -> String {
    match path.strip_prefix(BUILTIN_TEXTURES_PREFIX) {
        Some(file) => format!("embedded://{}/{}", EMBEDDED_TEXTURES_DIR, file),
        None => path.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::{builtin_texture_path, PARTICLE_TEXTURES};

    #[test]
    fn builtin_textures_match_editor_list() {
        let files: Vec<&str> = BUILTIN_TEXTURES.iter().map(|(f, _)| *f).collect();
        let editor: Vec<&str> = PARTICLE_TEXTURES.iter().map(|t| t.filename).collect();
        assert_eq!(files, editor);
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
}
