//! Draw order of particle layers (`ParticlesLayerDef::sort_bias`).
//!
//! bevy_hanabi 0.19 draws blended effects in Bevy's `Transparent3d` phase,
//! sorted back to front by the distance of the **emitter** (the layer
//! entity's position) with a depth bias of 0, and exposes no draw-order
//! setting for 3D (`EffectAsset::z_layer_2d` is 2D only). Omagari's own
//! layers (shapes, trails, afterimages, surface overlays) are sorted the
//! same way, by their entity's centre plus their material's `sort_bias`.
//!
//! This module gives particle layers the same bias: layer entities with a
//! non-zero bias carry [`ParticleSortBias`]; the render world copies them
//! every frame and, right after bevy_hanabi queued its draws, writes the
//! bias into those phase items (Bevy adds it to the sort distance, positive
//! = nearer the camera = drawn later, on top). Opaque and mask effects are
//! depth-tested in binned phases; the bias does not apply to them.

use bevy::core_pipeline::core_3d::{Transparent3d, TransparentSortingInfo3d};
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::render::render_phase::ViewSortedRenderPhases;
use bevy::render::sync_world::MainEntity;
use bevy::render::{Extract, ExtractSchedule, Render, RenderApp, RenderSystems};
use bevy_hanabi::EffectSystems;

/// Added to a particle layer's transparent sort distance (m); on the layer
/// entity when the layer's `sort_bias` is not 0.
#[derive(Component, Clone, Copy, PartialEq, Debug)]
pub struct ParticleSortBias(pub f32);

/// The biases of this frame, by main-world entity.
#[derive(Resource, Default)]
struct ExtractedSortBiases(HashMap<MainEntity, f32>);

pub(crate) fn build(app: &mut App) {
    let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
        return;
    };
    render_app
        .init_resource::<ExtractedSortBiases>()
        .add_systems(ExtractSchedule, extract_sort_biases)
        .add_systems(
            Render,
            apply_sort_biases.in_set(RenderSystems::Queue).after(EffectSystems::QueueEffects),
        );
}

fn extract_sort_biases(mut biases: ResMut<ExtractedSortBiases>, layers: Extract<Query<(Entity, &ParticleSortBias)>>) {
    biases.0.clear();
    for (entity, bias) in layers.iter() {
        biases.0.insert(MainEntity::from(entity), bias.0);
    }
}

/// Writes the biases into bevy_hanabi's transparent phase items (Bevy
/// recomputes the sort distance from them in its sort system).
fn apply_sort_biases(biases: Res<ExtractedSortBiases>, mut phases: ResMut<ViewSortedRenderPhases<Transparent3d>>) {
    if biases.0.is_empty() {
        return;
    }
    for phase in phases.values_mut() {
        for (key, item) in phase.items.iter_mut() {
            if let Some(bias) = biases.0.get(&key.1) {
                apply_bias(&mut item.sorting_info, *bias);
            }
        }
    }
}

/// Sets the depth bias of a sorted item (items always on top keep it).
pub fn apply_bias(sorting: &mut TransparentSortingInfo3d, bias: f32) {
    if let TransparentSortingInfo3d::Sorted { depth_bias, .. } = sorting {
        *depth_bias = bias;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::math::Affine3A;
    use bevy::render::render_phase::ViewRangefinder3d;

    /// A positive bias brings an item nearer in the sort (drawn later).
    #[test]
    fn bias_moves_the_sort_distance() {
        let mut info = TransparentSortingInfo3d::Sorted { mesh_center: Vec3::new(0.0, 0.0, -5.0), depth_bias: 0.0 };
        let view = ViewRangefinder3d::from_world_from_view(&Affine3A::IDENTITY);
        let before = info.sort_distance(&view);
        apply_bias(&mut info, 0.5);
        assert!((info.sort_distance(&view) - before - 0.5).abs() < 1e-6);
    }
}
