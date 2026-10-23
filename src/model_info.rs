//! What a glTF model contains, read on the CPU from its JSON with the `gltf`
//! crate (the one Bevy's loader uses): node names and the hierarchy (bones,
//! mesh nodes), meshes with their primitive entity names and materials,
//! animations with durations and animated nodes, and the rest-pose bounds.
//! No Bevy app, window or GPU is needed, and binary buffers are not read
//! (animation lengths and bounds come from the accessors' `min` / `max`,
//! which glTF requires), so the MCP server and the checks can describe
//! models quickly. Shared by the editor (hierarchy browser, animation
//! pickers) and MCP (`describe_model`, `list_assets`, `check_project`).
//!
//! Names follow Bevy 0.19's glTF loader: a node entity is named after the
//! node (`GltfNode<index>` without a name), a mesh primitive entity
//! `<mesh>.<material>` (or `<mesh>` without a material name), and an
//! animation targets nodes by the path of names from its scene root node, so
//! a clip from another file plays on a model whose nodes have the same names
//! (Mixamo clips exported "without skin").

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::SystemTime;

use bevy::math::{Mat4, Vec3};

/// glTF file extensions Omagari previews.
pub const MODEL_EXTENSIONS: [&str; 2] = ["glb", "gltf"];

/// Whether a file name is a glTF model.
pub fn is_model_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| MODEL_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

/// One node of the model's scene.
#[derive(Clone, Debug, PartialEq)]
pub struct ModelNode {
    /// The entity name Bevy gives it (`Name`).
    pub name: String,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    /// A joint of a skin.
    pub bone: bool,
    /// Index into [`ModelInfo::meshes`].
    pub mesh: Option<usize>,
    /// The node's mesh is skinned.
    pub skinned: bool,
    /// Rest-pose transform in model space (glTF axes: +Y up, +Z front).
    pub rest: Mat4,
}

/// One mesh and the entities Bevy spawns for its primitives.
#[derive(Clone, Debug, PartialEq)]
pub struct ModelMesh {
    pub name: String,
    pub primitives: Vec<ModelPrimitive>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ModelPrimitive {
    /// The primitive entity's `Name`: `<mesh>.<material>`.
    pub entity_name: String,
    pub material: Option<String>,
}

/// One animation clip.
#[derive(Clone, Debug, PartialEq)]
pub struct ModelAnimation {
    /// Index in the file (`GltfAssetLabel::Animation(index)`).
    pub index: usize,
    pub name: Option<String>,
    /// Seconds (the largest keyframe time).
    pub duration: f32,
    /// Paths of the animated nodes (names from the scene root node joined
    /// by `/`), what Bevy matches clips with.
    pub targets: Vec<String>,
}

impl ModelAnimation {
    /// The name `preview.animation` uses: the clip's name, or
    /// `Animation<index>`.
    pub fn label(&self) -> String {
        self.name.clone().unwrap_or_else(|| format!("Animation{}", self.index))
    }
}

/// What a glTF model contains (its default scene).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ModelInfo {
    /// The scene Bevy spawns (`GltfAssetLabel::Scene(scene)`).
    pub scene: usize,
    /// Nodes of that scene, parents before children.
    pub nodes: Vec<ModelNode>,
    /// Root nodes of the scene (indices into `nodes`).
    pub roots: Vec<usize>,
    pub meshes: Vec<ModelMesh>,
    pub materials: Vec<String>,
    pub animations: Vec<ModelAnimation>,
    /// Rest-pose bounds `(min, max)` in model space (skinned meshes in their
    /// bind pose).
    pub bounds: Option<(Vec3, Vec3)>,
}

impl ModelInfo {
    /// Reads a `.glb` / `.gltf` file: only its JSON (a `.glb`'s first chunk).
    pub fn load(path: &Path) -> Result<ModelInfo, String> {
        let mut file = std::fs::File::open(path).map_err(|e| format!("{}: {}", path.display(), e))?;
        let mut head = [0u8; 20];
        let read = file.read(&mut head).map_err(|e| format!("{}: {}", path.display(), e))?;
        let json = if read >= 20 && &head[0..4] == b"glTF" {
            let length = u32::from_le_bytes(head[12..16].try_into().unwrap()) as usize;
            if &head[16..20] != b"JSON" {
                return Err(format!("{}: the first GLB chunk is not JSON", path.display()));
            }
            let mut json = vec![0u8; length];
            file.read_exact(&mut json).map_err(|e| format!("{}: {}", path.display(), e))?;
            json
        } else {
            let mut json = head[..read].to_vec();
            file.read_to_end(&mut json).map_err(|e| format!("{}: {}", path.display(), e))?;
            json
        };
        ModelInfo::from_json(&json).map_err(|e| format!("{}: {}", path.display(), e))
    }

    /// Like [`ModelInfo::load`], cached by path, size and modification time
    /// (the editor's checks run after every edit).
    pub fn cached(path: &Path) -> Result<Arc<ModelInfo>, String> {
        type Cache = HashMap<PathBuf, (SystemTime, u64, Arc<ModelInfo>)>;
        static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
        let meta = std::fs::metadata(path).map_err(|e| format!("{}: {}", path.display(), e))?;
        let stamp = (meta.modified().unwrap_or(SystemTime::UNIX_EPOCH), meta.len());
        let cache = CACHE.get_or_init(Default::default);
        if let Some((time, len, info)) = cache.lock().unwrap().get(path) {
            if (*time, *len) == stamp {
                return Ok(info.clone());
            }
        }
        let info = Arc::new(ModelInfo::load(path)?);
        cache.lock().unwrap().insert(path.to_path_buf(), (stamp.0, stamp.1, info.clone()));
        Ok(info)
    }

    /// Parses glTF JSON.
    pub fn from_json(json: &[u8]) -> Result<ModelInfo, String> {
        let root = gltf::json::Root::from_slice(json).map_err(|e| format!("not glTF: {}", e))?;
        let document = gltf::Document::from_json(root).map_err(|e| format!("invalid glTF: {}", e))?;
        Ok(ModelInfo::from_document(&document))
    }

    fn from_document(document: &gltf::Document) -> ModelInfo {
        let mut info = ModelInfo::default();
        info.materials = document.materials().map(|m| material_name(&m)).collect();
        info.meshes = document
            .meshes()
            .map(|mesh| ModelMesh {
                name: mesh.name().map(str::to_string).unwrap_or_else(|| format!("Mesh{}", mesh.index())),
                primitives: mesh
                    .primitives()
                    .map(|p| {
                        let material = p.material().name().map(str::to_string);
                        let mesh_name = mesh.name().unwrap_or("Mesh");
                        ModelPrimitive {
                            entity_name: match &material {
                                Some(m) => format!("{}.{}", mesh_name, m),
                                None => mesh_name.to_string(),
                            },
                            material,
                        }
                    })
                    .collect(),
            })
            .collect();
        let joints: Vec<usize> = document.skins().flat_map(|s| s.joints().map(|j| j.index()).collect::<Vec<_>>()).collect();

        let Some(scene) = document.default_scene().or_else(|| document.scenes().next()) else {
            return info;
        };
        info.scene = scene.index();
        // glTF node index -> index in `nodes`.
        let mut index_of: HashMap<usize, usize> = HashMap::new();
        let mut stack: Vec<(gltf::Node, Option<usize>, Mat4)> =
            scene.nodes().collect::<Vec<_>>().into_iter().rev().map(|n| (n, None, Mat4::IDENTITY)).collect();
        while let Some((node, parent, parent_rest)) = stack.pop() {
            if index_of.contains_key(&node.index()) {
                continue; // a node used twice (invalid glTF): keep the first
            }
            let rest = parent_rest * Mat4::from_cols_array_2d(&node.transform().matrix());
            let i = info.nodes.len();
            index_of.insert(node.index(), i);
            info.nodes.push(ModelNode {
                name: node.name().map(str::to_string).unwrap_or_else(|| format!("GltfNode{}", node.index())),
                parent,
                children: Vec::new(),
                bone: joints.contains(&node.index()),
                mesh: node.mesh().map(|m| m.index()),
                skinned: node.skin().is_some(),
                rest,
            });
            match parent {
                Some(p) => info.nodes[p].children.push(i),
                None => info.roots.push(i),
            }
            for child in node.children().collect::<Vec<_>>().into_iter().rev() {
                stack.push((child, Some(i), rest));
            }
        }

        // Bounds: skinned meshes are in model space in their bind pose (the
        // joints' inverse bind matrices undo the hierarchy), rigid ones are
        // placed by their node.
        let mut bounds: Option<(Vec3, Vec3)> = None;
        for node in scene_nodes(&scene) {
            let Some(mesh) = node.mesh() else { continue };
            let Some(&i) = index_of.get(&node.index()) else { continue };
            let transform = if node.skin().is_some() { Mat4::IDENTITY } else { info.nodes[i].rest };
            for primitive in mesh.primitives() {
                let b = primitive.bounding_box();
                let (min, max) = (Vec3::from(b.min), Vec3::from(b.max));
                for c in 0..8 {
                    let corner = Vec3::new(
                        if c & 1 == 0 { min.x } else { max.x },
                        if c & 2 == 0 { min.y } else { max.y },
                        if c & 4 == 0 { min.z } else { max.z },
                    );
                    let p = transform.transform_point3(corner);
                    bounds = Some(match bounds {
                        None => (p, p),
                        Some((lo, hi)) => (lo.min(p), hi.max(p)),
                    });
                }
            }
        }
        info.bounds = bounds;

        info.animations = document
            .animations()
            .map(|animation| {
                let duration = animation
                    .samplers()
                    .filter_map(|s| s.input().max())
                    .filter_map(|v| v.as_array().and_then(|a| a.first()).and_then(|x| x.as_f64()))
                    .fold(0.0f64, f64::max) as f32;
                let mut targets: Vec<String> = animation
                    .channels()
                    .filter_map(|c| index_of.get(&c.target().node().index()).map(|&i| info.path(i)))
                    .collect();
                targets.sort();
                targets.dedup();
                ModelAnimation {
                    index: animation.index(),
                    name: animation.name().map(str::to_string),
                    duration,
                    targets,
                }
            })
            .collect();
        info
    }

    /// Index of the first node with this name.
    pub fn node(&self, name: &str) -> Option<usize> {
        self.nodes.iter().position(|n| n.name == name)
    }

    /// The node's path: names from its scene root node, joined by `/`.
    pub fn path(&self, index: usize) -> String {
        let mut names = Vec::new();
        let mut at = Some(index);
        while let Some(i) = at {
            names.push(self.nodes[i].name.as_str());
            at = self.nodes[i].parent;
        }
        names.reverse();
        names.join("/")
    }

    /// Whether `node` is `ancestor` or below it.
    pub fn is_under(&self, node: usize, ancestor: usize) -> bool {
        let mut at = Some(node);
        while let Some(i) = at {
            if i == ancestor {
                return true;
            }
            at = self.nodes[i].parent;
        }
        false
    }

    /// Height of the rest-pose bounds (metres).
    pub fn height(&self) -> Option<f32> {
        self.bounds.map(|(lo, hi)| hi.y - lo.y)
    }

    /// Rest-pose position of a node in model space.
    pub fn rest_position(&self, index: usize) -> Vec3 {
        self.nodes[index].rest.transform_point3(Vec3::ZERO)
    }

    /// The clip `preview.animation` names: by name, else `Animation<index>`.
    pub fn animation(&self, name: &str) -> Option<&ModelAnimation> {
        self.animations.iter().find(|a| a.name.as_deref() == Some(name)).or_else(|| {
            let index: usize = name.strip_prefix("Animation")?.parse().ok()?;
            self.animations.iter().find(|a| a.index == index && a.name.is_none())
        })
    }

    /// How many of a clip's animated nodes this model has (matched by path,
    /// like Bevy).
    pub fn matching_targets(&self, clip: &ModelAnimation) -> usize {
        let paths: Vec<String> = (0..self.nodes.len()).map(|i| self.path(i)).collect();
        clip.targets.iter().filter(|t| paths.contains(t)).count()
    }

    /// Kind word of a node: `bone`, `mesh` or `node`.
    pub fn kind(&self, index: usize) -> &'static str {
        let node = &self.nodes[index];
        if node.bone {
            "bone"
        } else if node.mesh.is_some() {
            "mesh"
        } else {
            "node"
        }
    }

    /// Whether a mesh entity name, mesh name or material name exists
    /// (surface layer targets).
    #[allow(dead_code)] // surface layer targets (P4)
    pub fn has_mesh_or_material(&self, name: &str) -> bool {
        self.materials.iter().any(|m| m == name)
            || self.meshes.iter().any(|m| m.name == name || m.primitives.iter().any(|p| p.entity_name == name))
    }

    /// A one-line summary.
    pub fn summary(&self) -> String {
        let bones = self.nodes.iter().filter(|n| n.bone).count();
        let mesh_nodes = self.nodes.iter().filter(|n| n.mesh.is_some()).count();
        let mut out = format!(
            "{} nodes ({} bones, {} mesh nodes), {} meshes, {} materials",
            self.nodes.len(),
            bones,
            mesh_nodes,
            self.meshes.len(),
            self.materials.len()
        );
        if let Some(h) = self.height() {
            out += &format!(", height {:.2} m", h);
        }
        if !self.animations.is_empty() {
            let clips: Vec<String> =
                self.animations.iter().map(|a| format!("\"{}\" {:.2} s", a.label(), a.duration)).collect();
            out += &format!(", animations: {}", clips.join(", "));
        } else {
            out += ", no animations";
        }
        out
    }

    /// The `describe_model` text: summary, animations, meshes, materials and
    /// the node tree (cut after `max_nodes` nodes).
    pub fn describe(&self, max_nodes: usize) -> String {
        let mut out = format!("{}.\n", self.summary());
        // No "-0.00".
        let r = |v: f32| if v.abs() < 0.005 { 0.0 } else { v };
        if let Some((lo, hi)) = self.bounds {
            out += &format!(
                "Rest-pose bounds (model space, Y up, the model faces +Z): x[{:.2}, {:.2}] y[{:.2}, {:.2}] z[{:.2}, {:.2}].\n",
                r(lo.x),
                r(hi.x),
                r(lo.y),
                r(hi.y),
                r(lo.z),
                r(hi.z)
            );
        }
        if !self.animations.is_empty() {
            out += "Animations (preview.animation = the name; clips from other files play on nodes with the same names):\n";
            for a in self.animations.iter() {
                out += &format!("- \"{}\": {:.3} s, {} animated nodes\n", a.label(), a.duration, a.targets.len());
            }
        }
        if !self.meshes.is_empty() {
            out += "Meshes (entity name = mesh.material; surface layer targets):\n";
            for m in self.meshes.iter() {
                let skinned = self.nodes.iter().any(|n| n.mesh.is_some_and(|i| self.meshes.get(i) == Some(m)) && n.skinned);
                let prims: Vec<String> = m.primitives.iter().map(|p| format!("\"{}\"", p.entity_name)).collect();
                out += &format!("- {}{}: {}\n", m.name, if skinned { " (skinned)" } else { "" }, prims.join(", "));
            }
        }
        if !self.materials.is_empty() {
            out += &format!("Materials: {}\n", self.materials.join(", "));
        }
        out += "Nodes (names for sockets, attach_to and weapon nodes; [bone] = skin joint, [mesh] = has a mesh); rest position in metres:\n";
        let mut shown = 0;
        let mut stack: Vec<(usize, usize)> = self.roots.iter().rev().map(|&r| (r, 0)).collect();
        while let Some((i, depth)) = stack.pop() {
            if shown >= max_nodes {
                out += &format!("... ({} more nodes)\n", self.nodes.len() - shown);
                break;
            }
            shown += 1;
            let node = &self.nodes[i];
            let kind = match self.kind(i) {
                "node" => String::new(),
                k => format!(" [{}]", k),
            };
            let p = self.rest_position(i);
            out += &format!("{}{}{} ({:.2}, {:.2}, {:.2})\n", "  ".repeat(depth), node.name, kind, r(p.x), r(p.y), r(p.z));
            for &c in node.children.iter().rev() {
                stack.push((c, depth + 1));
            }
        }
        out
    }
}

fn material_name(material: &gltf::Material) -> String {
    material
        .name()
        .map(str::to_string)
        .unwrap_or_else(|| match material.index() {
            Some(i) => format!("Material{}", i),
            None => "default".into(),
        })
}

fn scene_nodes<'a>(scene: &gltf::Scene<'a>) -> Vec<gltf::Node<'a>> {
    let mut out = Vec::new();
    let mut stack: Vec<gltf::Node<'a>> = scene.nodes().collect();
    while let Some(node) = stack.pop() {
        stack.extend(node.children());
        out.push(node);
    }
    out
}

/// Every glTF model in the game assets folder (relative paths with `/`,
/// sizes in bytes), sorted, at most `limit`.
pub fn scan_models(game_root: &Path, limit: usize) -> Vec<(String, u64)> {
    crate::asset_library::scan_files(game_root, limit, is_model_file)
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use bevy::math::Quat;

    /// Writes a small glTF test model (`.gltf` + `.bin`):
    /// `Armature` > `Hips` [bone] > `Spine` [bone] > `RightHand` [bone] >
    /// `Prop` [mesh "Prop", material "Metal", 0.2 m cube at 0.1 m], and a
    /// skinned mesh node `Body` [mesh "Body", material "Skin", 0.6 × 1.8 m]
    /// under `Armature`. With `clip`, it has an animation of that name and
    /// `seconds` length turning `RightHand` around Z by 90° (the same
    /// skeleton in several files, like Mixamo clips).
    pub fn write_fixture(path: &Path, clip: Option<(&str, f32)>) {
        let mut b = Buffers::default();
        // A box from `min` to `max` as one triangle list (12 triangles).
        let cube = |min: Vec3, max: Vec3| -> Vec<f32> {
            let c = |i: usize| {
                Vec3::new(
                    if i & 1 == 0 { min.x } else { max.x },
                    if i & 2 == 0 { min.y } else { max.y },
                    if i & 4 == 0 { min.z } else { max.z },
                )
            };
            let faces = [[0, 1, 3, 2], [4, 6, 7, 5], [0, 4, 5, 1], [2, 3, 7, 6], [0, 2, 6, 4], [1, 5, 7, 3]];
            let mut out = Vec::new();
            for f in faces {
                for i in [f[0], f[1], f[2], f[0], f[2], f[3]] {
                    out.extend_from_slice(&c(i).to_array());
                }
            }
            out
        };
        let (pmin, pmax) = (Vec3::splat(-0.1), Vec3::splat(0.1));
        let prop = b.floats(&cube(pmin, pmax), "VEC3", Some((pmin.to_array().to_vec(), pmax.to_array().to_vec())));
        let (bmin, bmax) = (Vec3::new(-0.3, 0.0, -0.15), Vec3::new(0.3, 1.8, 0.15));
        let body_positions = cube(bmin, bmax);
        let vertices = body_positions.len() / 3;
        let body = b.floats(&body_positions, "VEC3", Some((bmin.to_array().to_vec(), bmax.to_array().to_vec())));
        // Every body vertex follows `Hips` (joint 0).
        let joints = b.bytes(&vec![0u8; vertices * 4], vertices);
        let weights: Vec<f32> = std::iter::repeat_n([1.0f32, 0.0, 0.0, 0.0], vertices).flatten().collect();
        let weights = b.floats(&weights, "VEC4", None);
        // Joint rest transforms (model space): Hips at 0.9, Spine 0.4 above,
        // RightHand 0.4 to the side and 0.3 up.
        let hips = Mat4::from_translation(Vec3::new(0.0, 0.9, 0.0));
        let spine = hips * Mat4::from_translation(Vec3::new(0.0, 0.4, 0.0));
        let hand = spine * Mat4::from_translation(Vec3::new(-0.4, 0.3, 0.0));
        let ibm: Vec<f32> = [hips, spine, hand].iter().flat_map(|m| m.inverse().to_cols_array()).collect();
        let ibm = b.floats(&ibm, "MAT4", None);
        let mut animations = Vec::new();
        if let Some((name, seconds)) = clip {
            let times = b.floats(&[0.0, seconds], "SCALAR", Some((vec![0.0], vec![seconds])));
            let q = Quat::from_rotation_z(std::f32::consts::FRAC_PI_2);
            let rotations = b.floats(&[0.0, 0.0, 0.0, 1.0, q.x, q.y, q.z, q.w], "VEC4", None);
            animations.push(serde_json::json!({
                "name": name,
                "samplers": [{ "input": times, "output": rotations, "interpolation": "LINEAR" }],
                "channels": [{ "sampler": 0, "target": { "node": 3, "path": "rotation" } }]
            }));
        }
        let stem = path.file_stem().unwrap().to_string_lossy().to_string();
        let bin = format!("{}.bin", stem);
        let json = serde_json::json!({
            "asset": { "version": "2.0" },
            "scene": 0,
            "scenes": [{ "name": "Scene", "nodes": [0] }],
            "nodes": [
                { "name": "Armature", "children": [1, 5] },
                { "name": "Hips", "translation": [0.0, 0.9, 0.0], "children": [2] },
                { "name": "Spine", "translation": [0.0, 0.4, 0.0], "children": [3] },
                { "name": "RightHand", "translation": [-0.4, 0.3, 0.0], "children": [4] },
                { "name": "Prop", "mesh": 0, "translation": [0.0, 0.1, 0.0] },
                { "name": "Body", "mesh": 1, "skin": 0 }
            ],
            "meshes": [
                { "name": "Prop", "primitives": [{ "attributes": { "POSITION": prop }, "material": 0 }] },
                { "name": "Body", "primitives": [{ "attributes": { "POSITION": body, "JOINTS_0": joints, "WEIGHTS_0": weights }, "material": 1 }] }
            ],
            "materials": [{ "name": "Metal" }, { "name": "Skin" }],
            "skins": [{ "name": "Armature", "joints": [1, 2, 3], "inverseBindMatrices": ibm }],
            "animations": animations,
            "buffers": [{ "byteLength": b.data.len(), "uri": bin }],
            "bufferViews": b.views,
            "accessors": b.accessors
        });
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path.with_file_name(&bin), &b.data).unwrap();
        std::fs::write(path, serde_json::to_string_pretty(&json).unwrap()).unwrap();
    }

    /// The buffer, views and accessors of a fixture.
    #[derive(Default)]
    struct Buffers {
        data: Vec<u8>,
        views: Vec<serde_json::Value>,
        accessors: Vec<serde_json::Value>,
    }

    impl Buffers {
        fn view(&mut self, bytes: &[u8]) -> usize {
            while self.data.len() % 4 != 0 {
                self.data.push(0);
            }
            self.views.push(serde_json::json!({ "buffer": 0, "byteOffset": self.data.len(), "byteLength": bytes.len() }));
            self.data.extend_from_slice(bytes);
            self.views.len() - 1
        }

        fn floats(&mut self, data: &[f32], ty: &str, min_max: Option<(Vec<f32>, Vec<f32>)>) -> usize {
            let bytes: Vec<u8> = data.iter().flat_map(|v| v.to_le_bytes()).collect();
            let view = self.view(&bytes);
            let components = match ty {
                "SCALAR" => 1,
                "VEC3" => 3,
                "VEC4" => 4,
                "MAT4" => 16,
                _ => unreachable!(),
            };
            let mut accessor = serde_json::json!({
                "bufferView": view, "componentType": 5126, "count": data.len() / components, "type": ty
            });
            if let Some((min, max)) = min_max {
                accessor["min"] = serde_json::json!(min);
                accessor["max"] = serde_json::json!(max);
            }
            self.accessors.push(accessor);
            self.accessors.len() - 1
        }

        /// `count` VEC4 of unsigned bytes (joint indices).
        fn bytes(&mut self, data: &[u8], count: usize) -> usize {
            let view = self.view(data);
            self.accessors
                .push(serde_json::json!({ "bufferView": view, "componentType": 5121, "count": count, "type": "VEC4" }));
            self.accessors.len() - 1
        }
    }

    /// Wraps a `.gltf` file's JSON into a minimal `.glb` (JSON chunk only).
    fn to_glb(json: &[u8]) -> Vec<u8> {
        let mut chunk = json.to_vec();
        while chunk.len() % 4 != 0 {
            chunk.push(b' ');
        }
        let mut out = Vec::new();
        out.extend_from_slice(b"glTF");
        out.extend_from_slice(&2u32.to_le_bytes());
        out.extend_from_slice(&((12 + 8 + chunk.len()) as u32).to_le_bytes());
        out.extend_from_slice(&(chunk.len() as u32).to_le_bytes());
        out.extend_from_slice(b"JSON");
        out.extend_from_slice(&chunk);
        out
    }

    #[test]
    fn hierarchy_meshes_animations_and_bounds() {
        let dir = crate::asset_library::tests::temp_dir("model_info");
        let path = dir.join("hero.gltf");
        write_fixture(&path, Some(("Wave", 1.5)));
        let info = ModelInfo::load(&path).unwrap();

        let names: Vec<&str> = info.nodes.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(names, ["Armature", "Hips", "Spine", "RightHand", "Prop", "Body"]);
        assert_eq!(info.roots, vec![0]);
        let hand = info.node("RightHand").unwrap();
        assert_eq!(info.path(hand), "Armature/Hips/Spine/RightHand");
        assert_eq!(info.kind(hand), "bone");
        assert_eq!(info.kind(info.node("Prop").unwrap()), "mesh");
        assert_eq!(info.kind(0), "node");
        assert!(info.nodes[info.node("Body").unwrap()].skinned);
        assert!(info.is_under(info.node("Prop").unwrap(), info.node("Hips").unwrap()));
        assert!(!info.is_under(info.node("Body").unwrap(), info.node("Hips").unwrap()));
        assert!((info.rest_position(hand) - Vec3::new(-0.4, 1.6, 0.0)).length() < 1e-5);

        assert_eq!(info.meshes[0].primitives[0].entity_name, "Prop.Metal");
        assert_eq!(info.meshes[1].primitives[0].entity_name, "Body.Skin");
        assert_eq!(info.materials, ["Metal", "Skin"]);
        assert!(info.has_mesh_or_material("Body.Skin") && info.has_mesh_or_material("Metal") && info.has_mesh_or_material("Prop"));
        assert!(!info.has_mesh_or_material("Cape"));

        // The skinned body (bind pose) and the prop at the hand (y 1.6 + 0.1 ± 0.1).
        let (lo, hi) = info.bounds.unwrap();
        assert!((lo - Vec3::new(-0.5, 0.0, -0.15)).length() < 1e-5, "{}", lo);
        assert!((hi - Vec3::new(0.3, 1.8, 0.15)).length() < 1e-5, "{}", hi);
        assert!((info.height().unwrap() - 1.8).abs() < 1e-5);

        assert_eq!(info.animations.len(), 1);
        let wave = info.animation("Wave").unwrap();
        assert_eq!((wave.index, wave.duration), (0, 1.5));
        assert_eq!(wave.targets, ["Armature/Hips/Spine/RightHand"]);
        assert_eq!(info.matching_targets(wave), 1);
        assert!(info.animation("Run").is_none());

        // The same JSON in a .glb, read through the cache.
        let glb = dir.join("hero.glb");
        std::fs::write(&glb, to_glb(&std::fs::read(&path).unwrap())).unwrap();
        let cached = ModelInfo::cached(&glb).unwrap();
        assert_eq!(*cached, info);
        assert!(Arc::ptr_eq(&cached, &ModelInfo::cached(&glb).unwrap()));

        let text = info.describe(100);
        assert!(text.contains("6 nodes (3 bones, 2 mesh nodes), 2 meshes, 2 materials, height 1.80 m"), "{}", text);
        assert!(text.contains("- \"Wave\": 1.500 s, 1 animated nodes"), "{}", text);
        assert!(text.contains("- Body (skinned): \"Body.Skin\""), "{}", text);
        assert!(text.contains("      RightHand [bone] (-0.40, 1.60, 0.00)"), "{}", text);
        assert!(info.describe(2).contains("... (4 more nodes)"));

        assert!(ModelInfo::load(&dir.join("missing.glb")).is_err());
        std::fs::write(dir.join("bad.gltf"), "{ nope").unwrap();
        assert!(ModelInfo::load(&dir.join("bad.gltf")).unwrap_err().contains("not glTF"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unnamed_clips_and_model_files() {
        let json = br#"{"asset":{"version":"2.0"},"nodes":[{}],"scenes":[{"nodes":[0]}],
            "animations":[],"accessors":[]}"#;
        let info = ModelInfo::from_json(json).unwrap();
        assert_eq!(info.nodes[0].name, "GltfNode0", "Bevy's name for an unnamed node");
        assert!(info.bounds.is_none() && info.animations.is_empty());
        let clip = ModelAnimation { index: 2, name: None, duration: 1.0, targets: Vec::new() };
        assert_eq!(clip.label(), "Animation2");
        assert!(is_model_file(Path::new("a/B.GLB")) && is_model_file(Path::new("x.gltf")));
        assert!(!is_model_file(Path::new("x.png")));
    }
}
