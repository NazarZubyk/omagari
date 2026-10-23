//! Workaround for a bevy_hanabi 0.19 crash with opaque particles.
//!
//! Effects with `AlphaMode::Opaque` / `Mask` are drawn in Bevy's binned 3D
//! phases (`Opaque3d`, `AlphaMask3d`). Since Bevy 0.16 those phases are
//! retained across frames and an item must be removed explicitly when it
//! stops being drawn. bevy_hanabi 0.19 re-adds its effects every frame with a
//! fresh render-world draw entity but never removes an effect that went away,
//! so after such an effect is despawned (stopped, finished, replaced) the
//! phase still holds its old draw entity and hanabi's draw function panics
//! (`effect_draw_batches.get(..).unwrap()`, "NotSpawned").
//!
//! [`sweep_dead_effect_draws`] runs right after hanabi queued this frame's
//! effects and removes the non-mesh items whose draw entity no longer exists.

use bevy::core_pipeline::core_3d::{AlphaMask3d, Opaque3d};
use bevy::ecs::entity::Entities;
use bevy::prelude::*;
use bevy::render::render_phase::{BinnedPhaseItem, ViewBinnedRenderPhases};
use bevy::render::sync_world::MainEntity;
use bevy::render::{Render, RenderApp, RenderSystems};
use bevy_hanabi::EffectSystems;

pub(crate) fn build(app: &mut App) {
    let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
        return;
    };
    render_app.add_systems(
        Render,
        (sweep_dead_effect_draws::<Opaque3d>, sweep_dead_effect_draws::<AlphaMask3d>)
            .in_set(RenderSystems::Queue)
            .after(EffectSystems::QueueEffects),
    );
}

/// Removes binned non-mesh items whose render-world entity is gone.
fn sweep_dead_effect_draws<P: BinnedPhaseItem>(mut phases: ResMut<ViewBinnedRenderPhases<P>>, entities: &Entities) {
    for phase in phases.values_mut() {
        let dead: Vec<MainEntity> = phase
            .non_mesh_items
            .values()
            .flat_map(|bin| bin.entities.iter())
            .filter(|(_, entity)| !entities.contains(**entity))
            .map(|(main_entity, _)| *main_entity)
            .collect();
        for main_entity in dead {
            phase.remove(main_entity);
        }
    }
}
