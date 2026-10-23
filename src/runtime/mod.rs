//! The VFX runtime: [`VfxPackage`] assets, instances attached to entities,
//! and built-in textures. Added by [`crate::OmagariPlugin`].

mod builtin;
mod instance;
mod package;

pub use builtin::{resolve_asset_path, resolve_asset_path_in, BUILTIN_TEXTURES};
pub use instance::*;
pub use package::*;

use bevy::prelude::*;

/// Systems that start, stop and clean up instances (in `Update`).
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct VfxSystems;

pub(crate) fn build(app: &mut App) {
    builtin::register_builtin_textures(app);
    app.init_asset::<VfxPackage>()
        .init_asset_loader::<VfxPackageLoader>()
        .add_message::<VfxFinished>()
        .add_systems(
            Update,
            (
                instance::start_instances,
                instance::stop_layer_spawners,
                instance::finish_instances,
            )
                .chain()
                .in_set(VfxSystems),
        );
}
