//! Checks of the P3b particle features (plan F9): mesh assets and mesh
//! particles, bevy_hanabi ribbons, and the draw-order bias of particle
//! layers. The descriptions (`analyze_project`) are in `analysis.rs`, which
//! has the simulated particles.

use std::path::Path;

use bevy::prelude::*;
use bevy_hanabi::prelude::*;
use omagari::format::*;
use omagari::runtime::particle_shape_vertex_count;

use super::analysis::Diagnostics;
use crate::asset_library as library;
use crate::model_info::{ModelInfo, MODEL_EXTENSIONS};

/// Mesh vertices × capacity above which a mesh-particle layer gets a
/// warning (every particle draws the whole mesh).
pub const MESH_VERTEX_BUDGET: usize = 1_000_000;
/// Vertices above which one particle mesh is heavy.
pub const HEAVY_PARTICLE_MESH: usize = 2_000;
/// Largest sensible `sort_bias` (metres).
pub const MAX_SORT_BIAS: f32 = 50.0;

/// Checks a `Mesh` asset: the file, the mesh in it, and whether bevy_hanabi
/// can draw it.
pub fn check_mesh_asset(d: &mut Diagnostics, label: &str, m: &MeshAssetDef, assets_root: &Path) {
    if m.path.is_empty() {
        d.error(label, "mesh path is empty");
        return;
    }
    match library::path_problem(&m.path) {
        Err(e) => {
            d.error(label, e);
            return;
        }
        Ok(Some(w)) => d.warn(label, w),
        Ok(None) => {}
    }
    let ext = library::extension(&m.path);
    if !MODEL_EXTENSIONS.contains(&ext.as_str()) {
        d.error(label, format!("{}: mesh assets come from glTF files (.glb / .gltf)", m.path));
        return;
    }
    let file = assets_root.join(&m.path);
    if !file.is_file() {
        d.warn(label, format!("{} is not in the game assets folder ({}); the mesh will not load", m.path, assets_root.display()));
        return;
    }
    let info = match ModelInfo::cached(&file) {
        Ok(info) => info,
        Err(e) => {
            d.warn(label, format!("cannot read {}: {}", m.path, e));
            return;
        }
    };
    let Some((mesh, primitive)) = info.primitive(m.mesh as usize, m.primitive as usize) else {
        let list: Vec<String> =
            info.primitives().map(|(mi, pi, mesh, _)| format!("Mesh{}/Primitive{} \"{}\"", mi, pi, mesh.name)).collect();
        d.error(
            label,
            format!(
                "{} has no {} (it has {}); the layer would draw nothing",
                m.path,
                m.label(),
                if list.is_empty() { "no meshes".to_string() } else { list.join(", ") }
            ),
        );
        return;
    };
    if let Some(name) = &m.name {
        if *name != mesh.name {
            d.warn(
                label,
                format!(
                    "{} is \"{}\" in the file, but \"{}\" was picked: the file changed? pick the mesh again",
                    m.label(),
                    mesh.name,
                    name
                ),
            );
        }
    }
    if !primitive.triangles {
        d.error(label, format!("{} is not made of triangles; bevy_hanabi draws triangle meshes only", m.label()));
    }
    if !primitive.has_uvs {
        d.warn(
            label,
            format!(
                "{} has no texture coordinates (UVs); bevy_hanabi needs them, so Omagari adds zero UVs (a texture shows one texel)",
                m.label()
            ),
        );
    }
    if primitive.skinned {
        d.note(label, format!("{} is skinned; particles draw it in its rest pose", m.label()));
    }
    if primitive.vertices > HEAVY_PARTICLE_MESH {
        d.warn(
            label,
            format!(
                "{} has {} vertices; every particle draws all of them, so keep particle meshes small (≤ {})",
                m.label(),
                primitive.vertices,
                HEAVY_PARTICLE_MESH
            ),
        );
    }
}

/// The vertex count and the size (largest side, m, at particle size 1) of
/// the mesh a layer draws, when it is known.
pub fn particle_mesh_info(project: &ProjectV2, layer: &ParticlesLayerDef, assets_root: &Path) -> Option<(usize, f32)> {
    match layer.mesh.as_ref()? {
        ParticleMeshDef::Shape(shape) => Some((particle_shape_vertex_count(*shape), 1.0)),
        ParticleMeshDef::Asset(id) => {
            let m = project.mesh(id)?;
            let info = ModelInfo::cached(&assets_root.join(&m.path)).ok()?;
            let (_, p) = info.primitive(m.mesh as usize, m.primitive as usize)?;
            Some((p.vertices, p.size()))
        }
    }
}

fn has_orient(effect: &ParticleEffectDef, mode: Option<OrientModeDef>) -> bool {
    effect.render_modifiers.iter().any(|m| match m {
        RenderModifierEditor::Orient(o) => mode.is_none_or(|mode| o.mode == mode),
        _ => false,
    })
}

/// Whether anything gives the particles a velocity (set or accelerated).
fn moves(effect: &ParticleEffectDef) -> bool {
    effect.init_modifiers.iter().chain(effect.update_modifiers.iter()).any(|m| match m {
        ModifierEditor::SetAttribute(s) => s.attr == Attribute::VELOCITY,
        ModifierEditor::InheritAttribute(i) => i.attr == Attribute::VELOCITY,
        ModifierEditor::SetVelocityCircle(_)
        | ModifierEditor::SetVelocitySphere(_)
        | ModifierEditor::SetVelocityTangent(_)
        | ModifierEditor::AccelModifier(_)
        | ModifierEditor::RadialAccel(_)
        | ModifierEditor::TangentAccel(_)
        | ModifierEditor::ConformToSphereModifier(_) => true,
        _ => false,
    })
}

fn is_random(e: &ExprWriterEditor) -> bool {
    let mut random = false;
    e.visit(&mut |e| {
        random |= match e {
            ExprWriterEditor::RandFloat | ExprWriterEditor::RandVec3 | ExprWriterEditor::RandU32 | ExprWriterEditor::Rand(_) => true,
            ExprWriterEditor::Operator(op) => matches!(**op, ExprOperatorEditor::Uniform(..) | ExprOperatorEditor::Normal(..)),
            _ => false,
        }
    });
    random
}

/// Attributes of the particle (`Attr`) and of the parent (`ParentAttr`) an
/// expression reads.
fn attributes_read(e: &ExprWriterEditor) -> (Vec<Attribute>, Vec<Attribute>) {
    let (mut own, mut parent) = (Vec::new(), Vec::new());
    e.visit(&mut |e| {
        if let ExprWriterEditor::Operator(op) = e {
            match **op {
                ExprOperatorEditor::Attr(a) => own.push(a),
                ExprOperatorEditor::ParentAttr(a) => parent.push(a),
                _ => {}
            }
        }
    });
    (own, parent)
}

/// Mesh particles, ribbons and the draw-order bias of one particle layer.
pub fn check_particle_layer(d: &mut Diagnostics, project: &ProjectV2, layer: &ParticlesLayerDef, assets_root: &Path) {
    let name = layer.name.as_str();
    let effect = &layer.effect;
    let opaque = matches!(effect.alpha_mode, AlphaModeDef::Opaque | AlphaModeDef::Mask(_));

    // --- mesh particles ------------------------------------------------------
    match &layer.mesh {
        Some(ParticleMeshDef::Asset(id)) => match project.asset(id).map(|a| &a.kind) {
            None => d.error(name, format!("mesh \"{}\" is not in the assets list; the layer draws quads", id)),
            Some(AssetKind::Texture(_)) => d.error(name, format!("mesh \"{}\" is a texture asset, not a mesh", id)),
            Some(AssetKind::Mesh(m)) => {
                let samples_texture = effect.render_modifiers.iter().any(|r| matches!(r, RenderModifierEditor::ParticleTexture(_)));
                let no_uvs = ModelInfo::cached(&assets_root.join(&m.path))
                    .ok()
                    .and_then(|info| info.primitive(m.mesh as usize, m.primitive as usize).map(|(_, p)| !p.has_uvs))
                    .unwrap_or(false);
                if samples_texture && no_uvs {
                    d.warn(name, format!("ParticleTexture samples the texture, but mesh \"{}\" has no UVs (it shows one texel)", id));
                }
            }
        },
        Some(ParticleMeshDef::Shape(_)) | None => {}
    }
    if layer.mesh.is_some() {
        if let Some((vertices, _)) = particle_mesh_info(project, layer, assets_root) {
            let total = vertices * effect.capacity as usize;
            if total > MESH_VERTEX_BUDGET {
                d.warn(
                    name,
                    format!(
                        "every particle draws the whole mesh: {} vertices × capacity {} = {} vertices per frame; lower the capacity or use a smaller mesh",
                        vertices, effect.capacity, total
                    ),
                );
            }
        }
        let flat = matches!(layer.mesh, Some(ParticleMeshDef::Shape(ParticleShapeDef::Quad)));
        if !flat && matches!(effect.alpha_mode, AlphaModeDef::Blend | AlphaModeDef::Premultiply | AlphaModeDef::Multiply) {
            d.warn(
                name,
                "a see-through blend draws a mesh's far side over its near side in no order (bevy_hanabi does not sort triangles); use alpha_mode Opaque (solid debris), Mask, or Add (glowing shards)",
            );
        }
    }

    // --- ribbons -------------------------------------------------------------
    if let Some(id) = effect.ribbon_id() {
        if layer.mesh.is_some() {
            d.warn(name, "ribbons are drawn as strips between the particles; the mesh is stretched along them (remove the mesh)");
        }
        if effect.motion_integration != MotionIntegrationDef::None && moves(effect) {
            d.warn(
                name,
                "ribbon particles move by their velocity, so the ribbon bends and stretches as they fly; for a streak that stays where it was drawn, use motion_integration: None (and move the emitter or set positions)",
            );
        }
        let lifetime_random = effect.init_modifiers.iter().any(|m| {
            matches!(m, ModifierEditor::SetAttribute(s) if s.attr == Attribute::LIFETIME && is_random(&s.attr_expr))
        });
        if lifetime_random {
            d.warn(
                name,
                "ribbon particles have different lifetimes: they die in the middle of the ribbon, which then cuts across; give every particle of a ribbon the same lifetime",
            );
        }
        if has_orient(effect, Some(OrientModeDef::AlongVelocity)) {
            d.warn(
                name,
                "Orient AlongVelocity on a ribbon turns its width with the velocity (and breaks without one); use Orient((mode: FaceCameraPosition)) so the ribbon faces the camera",
            );
        }
        if let Some(expr) = id {
            let (own, parent) = attributes_read(expr);
            let unique = own.iter().any(|a| *a == Attribute::PARTICLE_COUNTER || *a == Attribute::ID);
            if is_random(expr) || unique {
                d.warn(
                    name,
                    "every particle gets its own ribbon_id, so no two particles join and nothing is drawn; give the particles of one ribbon the same id (a constant, or ParentAttr of a value the parent sets once)",
                );
            }
            if let Some(parent_layer) = layer.parent.as_ref().and_then(|p| project.particle_layers().find(|(_, l)| &l.name == p)) {
                let parent_effect = &parent_layer.1.effect;
                for a in parent {
                    let set = parent_effect
                        .init_modifiers
                        .iter()
                        .any(|m| matches!(m, ModifierEditor::SetAttribute(s) if s.attr == a));
                    if a == Attribute::ID {
                        d.warn(
                            name,
                            "the ribbon id is the parent's id, which bevy_hanabi reuses when a parent dies: ribbons of old and new parents join; store a unique value in the parent instead (SetAttribute u32_0 = Attr(particle_counter))",
                        );
                    } else if !set {
                        d.warn(
                            name,
                            format!(
                                "the ribbon id reads the parent's {}, which the parent's init never sets: every child gets the same id and one ribbon zig-zags between all parents; set it in \"{}\" (e.g. SetAttribute u32_0 = Attr(particle_counter))",
                                a.name(),
                                parent_layer.1.name
                            ),
                        );
                    }
                }
            }
        }
    }

    // --- draw order ----------------------------------------------------------
    if layer.sort_bias != 0.0 {
        if opaque {
            d.warn(
                name,
                "sort_bias has no effect on Opaque / Mask layers: they are depth-tested, not sorted (use it on Blend / Add layers)",
            );
        }
        if !layer.sort_bias.is_finite() || layer.sort_bias.abs() > MAX_SORT_BIAS {
            d.warn(
                name,
                format!(
                    "sort_bias {} m moves the layer past everything in the scene; a few tenths to a few metres are enough (±{} m at most)",
                    layer.sort_bias, MAX_SORT_BIAS
                ),
            );
        }
    }
}

/// Whether the "no Orient" warning applies: mesh particles keep the mesh's
/// own orientation (that is fine without Orient).
pub fn wants_orient(layer: &ParticlesLayerDef) -> bool {
    layer.mesh.is_none()
}
