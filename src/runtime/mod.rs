//! The VFX runtime: [`VfxPackage`] assets, instances attached to entities
//! (sockets, segments, size fit), and built-in textures. Added by
//! [`crate::OmagariPlugin`].

mod attach;
mod builtin;
mod instance;
mod package;

pub use attach::*;
pub use builtin::{resolve_asset_path, resolve_asset_path_in, BUILTIN_TEXTURES};
pub use instance::*;
pub use package::*;

use bevy::prelude::*;
use bevy::transform::TransformSystems;

/// Systems that start, place, stop and clean up instances (in `Update`), and
/// write segment positions (in `PostUpdate`, after transform propagation).
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct VfxSystems;

pub(crate) fn build(app: &mut App) {
    builtin::register_builtin_textures(app);
    app.init_asset::<VfxPackage>()
        .init_asset_loader::<VfxPackageLoader>()
        .add_message::<VfxFinished>()
        .add_observer(instance::on_instance_removed)
        .add_observer(instance::on_layer_removed)
        .add_systems(
            Update,
            (
                instance::reload_instances,
                instance::start_instances,
                attach::resolve_sockets,
                attach::apply_fit,
                instance::stop_layer_spawners,
                instance::finish_instances,
            )
                .chain()
                .in_set(VfxSystems),
        )
        .add_systems(
            PostUpdate,
            attach::write_segment_properties
                .after(TransformSystems::Propagate)
                .in_set(VfxSystems),
        );
}
