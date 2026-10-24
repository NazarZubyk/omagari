//! The VFX runtime: [`VfxPackage`] assets, instances attached to entities
//! (sockets, segments, size fit), parameters and triggers, surface, shape,
//! trail and afterimage layers with Omagari's effect material, light layers
//! (point lights under a global cap, [`VfxSettings`]), and built-in
//! textures. Added by [`crate::OmagariPlugin`].

mod afterimage;
mod attach;
mod builtin;
mod effect_material;
mod instance;
mod light;
mod package;
mod params;
mod render_fix;
mod shape;
mod shape_mesh;
mod surface;
mod trail;
mod triggers;

pub use afterimage::*;
pub use attach::*;
pub use builtin::{resolve_asset_path, resolve_asset_path_in, BUILTIN_TEXTURES};
pub use effect_material::*;
pub use instance::*;
pub use light::*;
pub use package::*;
pub use shape::*;
pub use shape_mesh::*;
pub use surface::*;
pub use trail::*;
pub use params::{
    evaluate_binding, evaluate_binding_at, smooth, smooth_value, ParamError, ParamSlot, ParamState, VfxWarnOnce,
};
pub use triggers::TriggerArgs;

use bevy::camera::visibility::VisibilitySystems;
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
    effect_material::build(app);
    render_fix::build(app);
    app.init_asset::<VfxPackage>()
        .init_asset_loader::<VfxPackageLoader>()
        .init_resource::<VfxWarnOnce>()
        .init_resource::<ShapeMeshCache>()
        .init_resource::<VfxSettings>()
        .add_message::<VfxFinished>()
        .add_observer(instance::on_instance_removed)
        .add_observer(instance::on_layer_removed)
        .add_observer(surface::on_surface_layer_removed)
        .add_observer(afterimage::on_afterimage_layer_removed)
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
                surface::sync_surface_overlays,
                surface::update_surface_materials,
                shape::update_shapes,
                trail::update_trails,
                afterimage::update_afterimages,
                light::update_lights,
                light::apply_light_cap,
                instance::finish_instances,
            )
                .chain()
                .in_set(VfxSystems),
        )
        .add_systems(
            PostUpdate,
            (
                attach::write_segment_properties.after(TransformSystems::Propagate),
                shape::place_shapes
                    .after(TransformSystems::Propagate)
                    .before(VisibilitySystems::CheckVisibility),
                trail::build_trails
                    .after(TransformSystems::Propagate)
                    .before(VisibilitySystems::CheckVisibility),
                afterimage::snapshot_afterimages
                    .after(TransformSystems::Propagate)
                    .before(VisibilitySystems::CheckVisibility),
                light::place_lights
                    .after(TransformSystems::Propagate)
                    .before(VisibilitySystems::CheckVisibility),
                triggers::scale_spawn_counts.after(EffectSystems::TickSpawners),
            )
                .in_set(VfxSystems),
        );
}
