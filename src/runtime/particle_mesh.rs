//! Mesh particles: the meshes particle layers draw instead of bevy_hanabi's
//! quad, given to the effect entity as bevy_hanabi's [`EffectMesh`]
//! component (bevy_hanabi 0.19's `EffectAsset::mesh` takes only an asset
//! path, which a generated mesh does not have).
//!
//! - Built-in shapes ([`ParticleShapeDef`]) are made once per app
//!   ([`ParticleMeshes`]).
//! - Mesh assets (one primitive of a glTF file) are loaded with the package.
//!   bevy_hanabi's render pipeline needs positions, UVs and normals; a mesh
//!   without UVs gets zero UVs once it is loaded ([`fix_particle_meshes`];
//!   Bevy's glTF loader already adds flat normals when a file has none).

use bevy::platform::collections::{HashMap, HashSet};
use bevy::prelude::*;
use bevy_hanabi::prelude::*;

use super::package::{PreparedMesh, VfxPackage};
use super::shape_mesh::particle_shape_mesh;
use crate::format::ParticleShapeDef;

/// The meshes of the built-in particle shapes, made on first use.
#[derive(Resource, Default)]
pub struct ParticleMeshes {
    meshes: HashMap<ParticleShapeDef, Handle<Mesh>>,
}

impl ParticleMeshes {
    /// The mesh of a built-in particle shape.
    pub fn get(&mut self, shape: ParticleShapeDef, meshes: &mut Assets<Mesh>) -> Handle<Mesh> {
        self.meshes.entry(shape).or_insert_with(|| meshes.add(particle_shape_mesh(shape))).clone()
    }
}

/// The mesh a particle layer draws, as bevy_hanabi's component; `None` for
/// the default quad, or when built-in meshes cannot be made (no
/// `Assets<Mesh>`, e.g. a headless app without meshes).
pub fn effect_mesh(
    prepared: Option<&PreparedMesh>,
    cache: &mut ParticleMeshes,
    meshes: Option<&mut Assets<Mesh>>,
) -> Option<EffectMesh> {
    match prepared? {
        PreparedMesh::Loaded(handle) => Some(EffectMesh(handle.clone())),
        PreparedMesh::Shape(shape) => meshes.map(|meshes| EffectMesh(cache.get(*shape, meshes))),
    }
}

/// Gives loaded particle mesh assets the vertex attributes bevy_hanabi's
/// render pipeline needs: zero UVs when the file has none (without them
/// bevy_hanabi cannot build a vertex layout and draws nothing).
pub(crate) fn fix_particle_meshes(
    packages: Res<Assets<VfxPackage>>,
    meshes: Option<ResMut<Assets<Mesh>>>,
    mut fixed: Local<HashSet<AssetId<Mesh>>>,
) {
    let Some(mut meshes) = meshes else { return };
    for package in packages.iter().map(|(_, p)| p) {
        for handle in package.meshes() {
            let id = handle.id();
            if fixed.contains(&id) {
                continue;
            }
            let Some(mesh) = meshes.get(id) else { continue };
            fixed.insert(id);
            let Ok(false) = mesh.try_contains_attribute(Mesh::ATTRIBUTE_UV_0) else { continue };
            let Ok(positions) = mesh.try_attribute(Mesh::ATTRIBUTE_POSITION) else { continue };
            let count = positions.len();
            if let Some(mut mesh) = meshes.get_mut(id) {
                if mesh.try_insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0f32, 0.0]; count]).is_err() {
                    warn!("a particle mesh has no texture coordinates and cannot be changed (its data is on the GPU only); bevy_hanabi cannot draw it");
                }
            }
        }
    }
}
