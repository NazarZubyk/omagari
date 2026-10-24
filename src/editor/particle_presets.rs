//! Presets of the particle inspector for the P3b features: ribbons (magic
//! streaks, a swirl, a streak behind the emitter) and mesh particles
//! (debris, glowing shards). They are built from the guide's tested recipes
//! (`mcp::guide_recipes`), so the editor and the guide never drift apart.

use bevy::prelude::*;
use omagari::format::*;

use crate::mcp::guide_recipes;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParticlePreset {
    /// Swirling wisps with a ribbon child each (two layers).
    MagicStreaks,
    /// One ribbon traced around the target.
    RibbonSwirl,
    /// One ribbon behind the moving emitter (a target or socket).
    RibbonFollow,
    /// Solid tumbling cubes, one burst.
    DebrisBurst,
    /// Additive glowing shards, one burst (built-in shard mesh).
    GlowingShards,
}

impl ParticlePreset {
    pub const ALL: [ParticlePreset; 5] = [
        ParticlePreset::MagicStreaks,
        ParticlePreset::RibbonSwirl,
        ParticlePreset::RibbonFollow,
        ParticlePreset::DebrisBurst,
        ParticlePreset::GlowingShards,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ParticlePreset::MagicStreaks => "〰 Ribbon: magic streaks",
            ParticlePreset::RibbonSwirl => "〰 Ribbon: swirl around the target",
            ParticlePreset::RibbonFollow => "〰 Ribbon: streak behind the emitter",
            ParticlePreset::DebrisBurst => "🔶 Mesh: debris burst",
            ParticlePreset::GlowingShards => "🔶 Mesh: glowing shards",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            ParticlePreset::MagicStreaks => "Glowing wisps circle the body and rise, each dragging a tapering ribbon of its last 0.4 s. Turns this layer into the wisps and adds a ribbon layer under it (bevy_hanabi ribbons: particles with the same ribbon id are joined into a strip).",
            ParticlePreset::RibbonSwirl => "One golden ribbon winding around the target: new particles are born on a point that circles the body and stay there; the ribbon joins the last second of them.",
            ParticlePreset::RibbonFollow => "One ribbon behind the emitter: particles stay where they were born (Global space), so a moving target or socket (a hand, a projectile) draws a streak. Nothing shows while the emitter stands still: use the preview motion or a socket on the animated model. For a sword's swing arc use a Trail layer.",
            ParticlePreset::DebrisBurst => "24 solid cubes (6–12 cm, brown to sand) burst up from the feet, tumble along their flight and shrink away; unlit mesh particles, Opaque.",
            ParticlePreset::GlowingShards => "16 pointed shards fly out in all directions, glow cyan (additive) and darken in 0.8 s. Pick another mesh in the Mesh row (a mesh asset from the ASSETS tab).",
        }
    }

    /// The guide recipe the preset comes from.
    fn recipe(self) -> &'static str {
        match self {
            ParticlePreset::MagicStreaks => "Magic streaks",
            ParticlePreset::RibbonSwirl | ParticlePreset::RibbonFollow => "Swirling ribbon",
            ParticlePreset::DebrisBurst => "Debris burst",
            ParticlePreset::GlowingShards => "Glowing shards",
        }
    }

    /// The preset's layers (the first replaces the selected layer).
    pub fn layers(self) -> Result<Vec<ParticlesLayerDef>, String> {
        let recipe = guide_recipes()
            .into_iter()
            .find(|r| r.title.starts_with(self.recipe()))
            .ok_or_else(|| format!("the guide has no recipe \"{}\"", self.recipe()))?;
        let project = ProjectV2::from_ron_str(&recipe.ron).map_err(|e| e.to_string())?;
        let mut layers: Vec<ParticlesLayerDef> = project.particle_layers().map(|(_, l)| l.clone()).collect();
        match self {
            ParticlePreset::RibbonFollow => {
                let layer = &mut layers[0];
                layer.simulation_space = SimulationSpaceDef::Global;
                for m in layer.effect.init_modifiers.iter_mut() {
                    if let ModifierEditor::SetAttribute(s) = m {
                        if s.attr == bevy_hanabi::Attribute::POSITION {
                            s.attr_expr = ExprWriterEditor::Vec3(Vec3::ZERO);
                        }
                        if s.attr == bevy_hanabi::Attribute::LIFETIME {
                            s.attr_expr = ExprWriterEditor::Float(0.5);
                        }
                    }
                }
            }
            ParticlePreset::GlowingShards => {
                // The recipe's mesh asset needs a file; the preset uses the
                // built-in shard (1 m) at 10 cm.
                let layer = &mut layers[0];
                layer.mesh = Some(ParticleMeshDef::Shape(ParticleShapeDef::Shard));
                for m in layer.effect.init_modifiers.iter_mut() {
                    if let ModifierEditor::SetAttribute(s) = m {
                        if s.attr == bevy_hanabi::Attribute::SIZE {
                            s.attr_expr = ExprWriterEditor::Float(0.1);
                        }
                    }
                }
            }
            _ => {}
        }
        Ok(layers)
    }
}

/// Applies a preset to the particle layer at `index`: it becomes the
/// preset's first layer (keeping its name, attach and visible range); the
/// preset's other layers are inserted after it with unique names (their
/// parents follow). Returns the number of layers added.
pub fn apply_particle_preset(project: &mut ProjectV2, index: usize, preset: ParticlePreset) -> Result<usize, String> {
    let mut layers = preset.layers()?;
    let Some(LayerDef::Particles(current)) = project.layers.get(index) else {
        return Err("not a particle layer".into());
    };
    let (name, attach, visible) = (current.name.clone(), current.attach.clone(), current.visible.clone());
    let first_name = layers[0].name.clone();
    let mut renames = vec![(first_name, name.clone())];
    let mut taken: Vec<String> = project.layers.iter().map(|l| l.name().to_string()).collect();
    for layer in layers.iter_mut().skip(1) {
        let wanted = format!("{}_{}", name, layer.name);
        let mut unique = wanted.clone();
        let mut n = 2;
        while taken.contains(&unique) {
            unique = format!("{}{}", wanted, n);
            n += 1;
        }
        taken.push(unique.clone());
        renames.push((layer.name.clone(), unique.clone()));
        layer.name = unique;
        layer.attach = attach.clone();
    }
    for layer in layers.iter_mut() {
        if let Some(parent) = &layer.parent {
            if let Some((_, to)) = renames.iter().find(|(from, _)| from == parent) {
                layer.parent = Some(to.clone());
            }
        }
    }
    layers[0].name = name;
    layers[0].attach = attach;
    layers[0].visible = visible;
    let added = layers.len() - 1;
    let mut iter = layers.into_iter();
    project.layers[index] = LayerDef::Particles(iter.next().unwrap());
    for (k, layer) in iter.enumerate() {
        project.layers.insert(index + 1 + k, LayerDef::Particles(layer));
    }
    Ok(added)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_hanabi::{EffectShaderSources, LayoutFlags};

    /// Every preset applies to a layer of a project, keeps its name, and
    /// builds and generates its shaders (ribbons with bevy_hanabi's ribbon
    /// flag); the checks report no problem with it.
    #[test]
    fn presets_apply_and_generate_shaders() {
        for preset in ParticlePreset::ALL {
            let mut project = ProjectV2::default();
            project.layers.push(LayerDef::Particles(ParticlesLayerDef { name: "fx".into(), ..default() }));
            let added = apply_particle_preset(&mut project, 0, preset).unwrap();
            assert_eq!(project.layers.len(), 1 + added, "{:?}", preset);
            assert_eq!(project.layers[0].name(), "fx");
            let layers: Vec<&ParticlesLayerDef> = project.particle_layers().map(|(_, l)| l).collect();
            for (i, layer) in layers.iter().enumerate() {
                let mut warnings = Vec::new();
                let effect = layer.produce_in(&project, &mut warnings);
                assert!(warnings.is_empty(), "{:?}: {:?}", preset, warnings);
                let parent = layer.parent.as_ref().map(|p| {
                    let pi = layers.iter().position(|l| &l.name == p).unwrap();
                    layers[pi].produce_in(&project, &mut Vec::new()).particle_layout()
                });
                let children = layers.iter().filter(|l| l.parent.as_deref() == Some(layer.name.as_str())).count() as u32;
                let sources = EffectShaderSources::generate(&effect, parent.as_ref(), children)
                    .unwrap_or_else(|e| panic!("{:?} layer {}: {}", preset, i, e));
                assert_eq!(sources.layout_flags.contains(LayoutFlags::RIBBONS), layer.effect.is_ribbon(), "{:?} {}", preset, layer.name);
            }
            let d = crate::mcp::analysis::check_project(&project, std::path::Path::new("assets"));
            assert!(d.errors.is_empty() && d.warnings.is_empty(), "{:?}\n{}", preset, d.render());
        }
        // Magic streaks: the ribbon child follows the renamed parent.
        let mut project = ProjectV2::default();
        project.layers.push(LayerDef::Particles(ParticlesLayerDef { name: "fx".into(), ..default() }));
        apply_particle_preset(&mut project, 0, ParticlePreset::MagicStreaks).unwrap();
        let child = project.layers[1].as_particles().unwrap();
        assert_eq!((child.name.as_str(), child.parent.as_deref()), ("fx_streaks", Some("fx")));
        assert!(child.effect.is_ribbon());
    }
}
