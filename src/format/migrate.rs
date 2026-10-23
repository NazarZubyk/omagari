//! Migration of v1 projects ([`OmagariProject`]) to [`ProjectV2`].
//!
//! The migrated project produces the same bevy_hanabi effects as v1
//! `EffectEditor::produce()`: every effect becomes a `Particles` layer with
//! `attach: Target`, `simulation_space: Global` and the formerly hard-coded
//! render settings as explicit values.

use super::modifiers::{
    ImageSampleMappingDef, OrientModeDef, OrientModifierEditor, ParticleTextureModifierEditor,
};
use super::particles::{
    AlphaModeDef, EffectEditor, ModifierEditor, ParticleEffectDef, RenderModifierEditor,
    PARTICLE_TEXTURES,
};
use super::project::*;
use super::OmagariProject;

/// URI prefix of the textures embedded in the library.
pub const BUILTIN_TEXTURES_PREFIX: &str = "omagari://textures/";

/// Path of a built-in texture, e.g. `omagari://textures/cloud.png`.
pub fn builtin_texture_path(file: &str) -> String {
    format!("{}{}", BUILTIN_TEXTURES_PREFIX, file)
}

/// Migrates a v1 project. `warnings` receives a line for everything that does
/// not carry over (modifiers v1 dropped, unknown texture indices).
pub fn migrate_v1(project: OmagariProject, warnings: &mut Vec<String>) -> ProjectV2 {
    let mut v2 = ProjectV2::default();
    for effect in project.effects {
        let texture = effect.texture_index.and_then(|index| {
            let Some(builtin) = PARTICLE_TEXTURES.get(index) else {
                warnings.push(format!(
                    "[{}] texture_index {} is out of range; the layer has no texture",
                    effect.name, index
                ));
                return None;
            };
            let id = builtin
                .filename
                .strip_suffix(".png")
                .unwrap_or(builtin.filename)
                .to_string();
            if v2.asset(&id).is_none() {
                v2.assets.push(AssetDef {
                    id: id.clone(),
                    kind: AssetKind::Texture(TextureAssetDef::new(
                        builtin_texture_path(builtin.filename),
                        TexturePurpose::Sprite,
                    )),
                });
            }
            Some(id)
        });
        let play = if effect.spawner_settings.cycle_count() == 0 {
            PlayDef::Loop
        } else {
            PlayDef::OnStart
        };
        let layer = ParticlesLayerDef {
            name: effect.name.clone(),
            parent: effect.parent.clone(),
            attach: AttachDef::Target,
            simulation_space: SimulationSpaceDef::Global,
            play,
            texture,
            rate: None,
            visible: None,
            effect: migrate_effect(effect, warnings),
        };
        v2.layers.push(LayerDef::Particles(layer));
    }
    v2
}

fn migrate_effect(effect: EffectEditor, warnings: &mut Vec<String>) -> ParticleEffectDef {
    let name = effect.name;
    let mut keep = |list: Vec<ModifierEditor>, init: bool| -> Vec<ModifierEditor> {
        list.into_iter()
            .filter(|m| {
                let accepted = m.v1_accepts(init);
                if !accepted {
                    warnings.push(format!(
                        "[{}] {} in the {} list was ignored by v1 and is dropped",
                        name,
                        m.kind_name(),
                        if init { "init" } else { "update" }
                    ));
                }
                accepted
            })
            .collect()
    };
    let init_modifiers = keep(effect.init_modifiers, true);
    let update_modifiers = keep(effect.update_modifiers, false);

    // v1 ignored explicit texture/orient modifiers and always appended its own.
    let mut render_modifiers: Vec<RenderModifierEditor> = effect
        .render_modifiers
        .into_iter()
        .filter(|m| {
            matches!(
                m,
                RenderModifierEditor::SizeOverLifetime(_) | RenderModifierEditor::ColorOverLifetime(_)
            )
        })
        .collect();
    render_modifiers.push(RenderModifierEditor::ParticleTexture(
        ParticleTextureModifierEditor {
            sample_mapping: ImageSampleMappingDef::ModulateOpacityFromR,
        },
    ));
    render_modifiers.push(RenderModifierEditor::Orient(OrientModifierEditor {
        mode: OrientModeDef::AlongVelocity,
        rotation_expr: None,
    }));

    ParticleEffectDef {
        capacity: effect.capacity,
        spawner: effect.spawner_settings.into(),
        alpha_mode: AlphaModeDef::Blend,
        init_modifiers,
        update_modifiers,
        render_modifiers,
        ..Default::default()
    }
}
