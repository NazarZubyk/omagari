//! The VFX runtime: [`VfxPackage`] assets, instances attached to entities
//! (sockets, segments, size fit), parameters and triggers, and built-in
//! textures. Added by [`crate::OmagariPlugin`].

mod attach;
mod builtin;
mod instance;
mod package;
mod params;
mod render_fix;
mod triggers;

pub use attach::*;
pub use builtin::{resolve_asset_path, resolve_asset_path_in, BUILTIN_TEXTURES};
pub use instance::*;
pub use package::*;
pub use params::{evaluate_binding, smooth, smooth_value, ParamError, ParamSlot, ParamState, VfxWarnOnce};
pub use triggers::TriggerArgs;

use bevy::prelude::*;
use bevy::transform::TransformSystems;
use bevy_hanabi::EffectSystems;

/// Systems that start, place, drive, stop and clean up instances (in
/// `Update`), write segment positions (in `PostUpdate`, after transform
/// propagation) and scale spawn counts (in `PostUpdate`, after bevy_hanabi
/// ticks the spawners).
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct VfxSystems;

pub(crate) fn build(app: &mut App) {
    builtin::register_builtin_textures(app);
    render_fix::build(app);
    app.init_asset::<VfxPackage>()
        .init_asset_loader::<VfxPackageLoader>()
        .init_resource::<VfxWarnOnce>()
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
                params::update_params,
                triggers::apply_triggers,
                triggers::drive_layer_spawners,
                params::upload_layer_values,
                instance::finish_instances,
            )
                .chain()
                .in_set(VfxSystems),
        )
        .add_systems(
            PostUpdate,
            (
                attach::write_segment_properties.after(TransformSystems::Propagate),
                triggers::scale_spawn_counts.after(EffectSystems::TickSpawners),
            )
                .in_set(VfxSystems),
        );
}
