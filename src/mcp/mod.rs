//! A Model Context Protocol (MCP) server for Omagari, started with
//! `omagari --mcp`. It speaks JSON-RPC 2.0 over stdin/stdout, one message per
//! line, and lets an agent create, check, simulate and export effects without
//! rendering anything. See `docs/CLAUDE_VFX_GUIDE.md`.
//!
//! Nothing but protocol messages may be written to stdout in this mode.

use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use omagari::format::{
    AssetKind, ChannelDef, ParamValue, PreviewMotion, ProjectV2, TexturePurpose, BUILTIN_TEXTURES_PREFIX,
    DEFAULT_MOTION_SPEED,
};

use crate::asset_library as library;
use crate::controller::Folder;
use crate::settings::GameAssets;

pub mod analysis;

/// The guide served by the `guide` tool.
pub const GUIDE: &str = include_str!("../../docs/CLAUDE_VFX_GUIDE.md");

/// A recipe of the guide's "Recipes" section: a v2 project that passes
/// `check_project` without warnings.
#[derive(Clone, Debug, PartialEq)]
pub struct Recipe {
    /// The `### ` title, e.g. "Spark burst (one-shot impact)".
    pub title: String,
    /// The project text (the first ```ron block under the title).
    pub ron: String,
}

impl Recipe {
    /// A project file name from the title: "Spark burst (one-shot impact)"
    /// gives `spark_burst.omagari.ron`.
    pub fn file_name(&self) -> String {
        let head = self.title.split('(').next().unwrap_or_default();
        let mut slug = String::new();
        for c in head.trim().chars() {
            if c.is_ascii_alphanumeric() {
                slug.push(c.to_ascii_lowercase());
            } else if !slug.ends_with('_') && !slug.is_empty() {
                slug.push('_');
            }
        }
        let slug = slug.trim_end_matches('_');
        format!("{}.omagari.ron", if slug.is_empty() { "recipe" } else { slug })
    }
}

/// The guide's recipes, in guide order (used by the editor's "New from
/// recipe" and by the tests that keep them warning-free).
pub fn guide_recipes() -> Vec<Recipe> {
    let section = GUIDE.split("\n## Recipes").nth(1).unwrap_or_default();
    let section = section.split("\n## ").next().unwrap_or_default();
    section
        .split("\n### ")
        .skip(1)
        .filter_map(|block| {
            let title = block.lines().next()?.trim().to_string();
            let ron = block.split("```ron\n").nth(1)?.split("```").next()?.to_string();
            Some(Recipe { title, ron })
        })
        .collect()
}

/// File the running editor polls for preview requests.
pub const PREVIEW_REQUEST_FILE: &str = ".preview_request";

/// Stages `preview_in_editor` accepts.
pub const PREVIEW_STAGES: [&str; 5] = ["air", "floor", "object", "capsule", "model"];

/// Cameras `preview_in_editor` accepts.
pub const PREVIEW_CAMERAS: [&str; 2] = ["orbit", "game"];

/// What `preview_in_editor` asks the editor for, written to
/// [`PREVIEW_REQUEST_FILE`] as a JSON object
/// `{ "name": "fire.omagari.ron", "stage": "floor", "motion": { "type": "circle", "radius": 3.0, "speed": 3.6 } }`.
/// The older two-line form (file name, then an optional stage) is still read.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct PreviewRequest {
    /// Project file name in `saved_effects/`.
    pub name: String,
    /// `air`, `floor` or `object`; `None` keeps the editor's stage.
    pub stage: Option<String>,
    /// `None` keeps the project's preview motion; `Some(None)` = no motion.
    pub motion: Option<Option<PreviewMotion>>,
    /// `orbit` or `game`; `None` keeps the project's camera.
    pub camera: Option<String>,
    /// Preview parameter values to set (they replace the project's preview
    /// values of those names).
    pub params: Vec<(String, ParamValue)>,
    /// A trigger to fire once the project plays.
    pub trigger: Option<String>,
    /// Preview model (glTF in the game assets folder); `None` keeps the
    /// project's.
    pub model: Option<String>,
    /// Animation clip name; `Some("")` = no animation.
    pub animation: Option<String>,
    /// glTF file of the clip; `Some("")` = the model's file.
    pub animation_file: Option<String>,
    /// Node the instance is attached to; `Some("")` = the model's root.
    pub attach_to: Option<String>,
    /// Height of the target above the model's feet (metres).
    pub root_height: Option<f32>,
}

impl PreviewRequest {
    /// Applies the request's viewing choices to a project's preview block.
    pub fn apply_to(&self, project: &mut ProjectV2) {
        fn preview(p: &mut ProjectV2) -> &mut omagari::format::PreviewDef {
            p.preview.get_or_insert_with(Default::default)
        }
        if let Some(motion) = self.motion {
            preview(project).motion = motion;
        }
        if let Some(model) = &self.model {
            let preview = preview(project);
            preview.model = (!model.is_empty()).then(|| model.clone());
            if preview.model.is_some() {
                preview.stage = omagari::format::PreviewStage::Model;
            }
        }
        if let Some(file) = &self.animation_file {
            preview(project).animation_file = (!file.is_empty()).then(|| file.clone());
        }
        if let Some(clip) = &self.animation {
            preview(project).animation = (!clip.is_empty()).then(|| clip.clone());
        }
        if let Some(node) = &self.attach_to {
            preview(project).attach_to = (!node.is_empty()).then(|| node.clone());
        }
        if let Some(height) = self.root_height {
            preview(project).root_height = height;
        }
        if !self.params.is_empty() {
            let preview = preview(project);
            for (name, value) in self.params.iter() {
                preview.params.retain(|(n, _)| n != name);
                preview.params.push((name.clone(), *value));
            }
        }
    }
}

/// A parameter value from JSON: a number, or `[r, g, b]` / `[r, g, b, a]`
/// (linear colour, like particle colours).
pub fn param_value_from_json(value: &Value) -> Result<ParamValue, String> {
    if let Some(v) = value.as_f64() {
        return Ok(ParamValue::Float(v as f32));
    }
    if let Some(items) = value.as_array() {
        let numbers: Option<Vec<f32>> = items.iter().map(|v| v.as_f64().map(|f| f as f32)).collect();
        if let Some(c) = numbers {
            match c.len() {
                3 => return Ok(ParamValue::Color(bevy::math::Vec4::new(c[0], c[1], c[2], 1.0))),
                4 => return Ok(ParamValue::Color(bevy::math::Vec4::new(c[0], c[1], c[2], c[3]))),
                _ => {}
            }
        }
    }
    Err(format!("{} is not a parameter value: use a number, or [r, g, b] / [r, g, b, a] for a colour", value))
}

pub fn param_value_to_json(value: &ParamValue) -> Value {
    match value {
        ParamValue::Float(v) => json!(v),
        ParamValue::Color(c) => json!([c.x, c.y, c.z, c.w]),
    }
}

/// `{"name": value, ...}` as (name, value) pairs, in the object's order.
pub fn param_set_from_json(value: &Value) -> Result<Vec<(String, ParamValue)>, String> {
    let object = value
        .as_object()
        .ok_or_else(|| format!("{} is not an object of parameter values, e.g. {{\"intensity\": 0.5}}", value))?;
    object
        .iter()
        .map(|(name, v)| Ok((name.clone(), param_value_from_json(v).map_err(|e| format!("\"{}\": {}", name, e))?)))
        .collect()
}

impl PreviewRequest {
    pub fn to_json(&self) -> String {
        let mut object = json!({ "name": self.name });
        if let Some(stage) = &self.stage {
            object["stage"] = json!(stage);
        }
        if let Some(motion) = self.motion {
            object["motion"] = motion_to_json(motion);
        }
        if let Some(camera) = &self.camera {
            object["camera"] = json!(camera);
        }
        if !self.params.is_empty() {
            let params: serde_json::Map<String, Value> =
                self.params.iter().map(|(n, v)| (n.clone(), param_value_to_json(v))).collect();
            object["params"] = Value::Object(params);
        }
        if let Some(trigger) = &self.trigger {
            object["trigger"] = json!(trigger);
        }
        for (key, value) in [
            ("model", &self.model),
            ("animation", &self.animation),
            ("animation_file", &self.animation_file),
            ("attach_to", &self.attach_to),
        ] {
            if let Some(v) = value {
                object[key] = json!(v);
            }
        }
        if let Some(h) = self.root_height {
            object["root_height"] = json!(h);
        }
        object.to_string()
    }

    /// Reads the JSON form, or the older two-line form.
    pub fn parse(text: &str) -> Result<Self, String> {
        let trimmed = text.trim_start();
        if !trimmed.starts_with('{') {
            let mut lines = text.lines();
            let name = lines.next().unwrap_or_default().trim().to_string();
            let stage = lines.next().map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
            return Ok(PreviewRequest { name, stage, ..Default::default() });
        }
        let value: Value = serde_json::from_str(trimmed).map_err(|e| format!("invalid preview request: {}", e))?;
        let name = value
            .get("name")
            .and_then(Value::as_str)
            .ok_or("preview request without \"name\"")?
            .to_string();
        let stage = value.get("stage").and_then(Value::as_str).map(str::to_string);
        let motion = match value.get("motion") {
            None | Some(Value::Null) => None,
            Some(m) => Some(motion_from_json(m)?),
        };
        let camera = value.get("camera").and_then(Value::as_str).map(str::to_string);
        let params = match value.get("params") {
            None | Some(Value::Null) => Vec::new(),
            Some(p) => param_set_from_json(p)?,
        };
        let text = |key: &str| value.get(key).and_then(Value::as_str).map(str::to_string);
        let trigger = text("trigger");
        Ok(PreviewRequest {
            name,
            stage,
            motion,
            camera,
            params,
            trigger,
            model: text("model"),
            animation: text("animation"),
            animation_file: text("animation_file"),
            attach_to: text("attach_to"),
            root_height: value.get("root_height").and_then(Value::as_f64).map(|h| h as f32),
        })
    }
}

/// `{"type": "none"}`, `{"type": "circle", "radius": r, "speed": v}` or
/// `{"type": "back_and_forth", "distance": d, "speed": v}`; missing numbers
/// take defaults (radius 3 m, distance 4 m, speed 3.6 m/s).
pub fn motion_from_json(value: &Value) -> Result<Option<PreviewMotion>, String> {
    let number = |key: &str, default: f32| -> Result<f32, String> {
        match value.get(key) {
            None | Some(Value::Null) => Ok(default),
            Some(v) => v
                .as_f64()
                .map(|x| x as f32)
                .filter(|x| x.is_finite())
                .ok_or_else(|| format!("motion \"{}\" must be a number", key)),
        }
    };
    let kind = match value {
        Value::String(s) => s.as_str(),
        _ => value.get("type").and_then(Value::as_str).unwrap_or("none"),
    };
    let speed = number("speed", DEFAULT_MOTION_SPEED)?;
    match kind {
        "none" => Ok(None),
        "circle" => {
            let radius = number("radius", 3.0)?;
            if radius <= 0.0 {
                return Err("motion radius must be above 0".into());
            }
            Ok(Some(PreviewMotion::Circle { radius, speed }))
        }
        "back_and_forth" => {
            let distance = number("distance", 4.0)?;
            if distance <= 0.0 {
                return Err("motion distance must be above 0".into());
            }
            Ok(Some(PreviewMotion::BackAndForth { distance, speed }))
        }
        other => Err(format!("unknown motion type \"{}\": use none, circle or back_and_forth", other)),
    }
}

pub fn motion_to_json(motion: Option<PreviewMotion>) -> Value {
    match motion {
        None => json!({ "type": "none" }),
        Some(PreviewMotion::Circle { radius, speed }) => json!({ "type": "circle", "radius": radius, "speed": speed }),
        Some(PreviewMotion::BackAndForth { distance, speed }) => {
            json!({ "type": "back_and_forth", "distance": distance, "speed": speed })
        }
    }
}

fn motion_text(motion: Option<PreviewMotion>) -> String {
    match motion {
        None => "standing still".into(),
        Some(PreviewMotion::Circle { radius, speed }) => format!("moving in a {} m circle at {} m/s", radius, speed),
        Some(PreviewMotion::BackAndForth { distance, speed }) => {
            format!("moving back and forth over {} m at {} m/s", distance, speed)
        }
    }
}

const PROTOCOL_VERSION: &str = "2025-06-18";

pub fn run() {
    // Relative folders (saved_effects/, exported_effects/) live next to the
    // project; `--root <dir>` points the server there when started elsewhere.
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--root") {
        if let Some(root) = args.get(i + 1) {
            if let Err(e) = std::env::set_current_dir(root) {
                eprintln!("omagari mcp: cannot enter --root {}: {}", root, e);
            }
        }
    }

    let ctx = Context::new(GameAssets::from_environment());
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let response = match serde_json::from_str::<Value>(&line) {
            Ok(Value::Array(batch)) => {
                let replies: Vec<Value> = batch
                    .iter()
                    .filter_map(|m| handle_message(m, &ctx))
                    .collect();
                if replies.is_empty() { None } else { Some(Value::Array(replies)) }
            }
            Ok(message) => handle_message(&message, &ctx),
            Err(e) => Some(json!({
                "jsonrpc": "2.0",
                "id": null,
                "error": { "code": -32700, "message": format!("parse error: {}", e) }
            })),
        };
        if let Some(response) = response {
            let _ = writeln!(stdout, "{}", response);
            let _ = stdout.flush();
        }
    }
}

/// What the tools need besides their arguments.
pub struct Context {
    game_assets: GameAssets,
}

impl Context {
    pub fn new(game_assets: GameAssets) -> Self {
        Context { game_assets }
    }

    /// Folder relative texture paths are looked up in: the game assets
    /// folder, or this repository's `assets/`.
    fn assets_root(&self) -> PathBuf {
        self.game_assets.assets_root()
    }
}

fn handle_message(message: &Value, ctx: &Context) -> Option<Value> {
    let method = message.get("method").and_then(Value::as_str).unwrap_or("");
    // Notifications have no id and get no reply.
    let id = message.get("id")?.clone();
    let params = message.get("params").cloned().unwrap_or(json!({}));

    let result = match method {
        "initialize" => {
            let version = params
                .get("protocolVersion")
                .and_then(Value::as_str)
                .unwrap_or(PROTOCOL_VERSION);
            Ok(json!({
                "protocolVersion": version,
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "omagari", "version": env!("CARGO_PKG_VERSION") },
                "instructions": "Omagari builds bevy_hanabi particle effects as *.omagari.ron projects (format v2: assets + particle layers). \
                    Call `guide` first (section \"workflow\" is enough to start). You cannot see the effect; \
                    use `analyze_project` for a text description and `preview_in_editor` to show it to the user."
            }))
        }
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tool_list() })),
        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            let (text, is_error) = match call_tool(name, &args, ctx) {
                Ok(text) => (text, false),
                Err(text) => (text, true),
            };
            Ok(json!({
                "content": [{ "type": "text", "text": text }],
                "isError": is_error
            }))
        }
        _ => Err(json!({ "code": -32601, "message": format!("method not found: {}", method) })),
    };

    Some(match result {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err(error) => json!({ "jsonrpc": "2.0", "id": id, "error": error }),
    })
}

fn tool_list() -> Value {
    let name_prop = json!({
        "type": "string",
        "description": "Project name, e.g. \"fireball\" or \"fireball.omagari.ron\" (in saved_effects/), or \"examples/fireworks\" for a bundled example."
    });
    let ron_prop = json!({
        "type": "string",
        "description": "Full project RON text, used instead of `name` to check unsaved content."
    });
    json!([
        {
            "name": "guide",
            "description": "Omagari effect authoring guide: workflow, file format (v2), layers, attach (sockets, segments, fit, lifecycle), model preview (models, animations, weapons), parameters and triggers (values and events the game sends), modifiers, expressions, textures, recipes, game use. Read before creating effects.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "section": {
                        "type": "string",
                        "description": "One section to keep the reply short: workflow, format, layers, attach, model, parameters, triggers, spawner, modifiers, expressions, attributes, render, textures, assets, hierarchy, recipes, design, game. Omit for the whole guide."
                    }
                }
            }
        },
        {
            "name": "list_projects",
            "description": "List saved projects, bundled examples, the game assets folder and exported packages.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "read_project",
            "description": "Return the RON text of a project. v1 projects are returned migrated to format v2.",
            "inputSchema": {
                "type": "object",
                "properties": { "name": name_prop },
                "required": ["name"]
            }
        },
        {
            "name": "write_project",
            "description": "Check a project (v2; v1 input is migrated) and save it to saved_effects/<name>.omagari.ron in format v2. Refuses to save when it has errors (unless force=true). Returns the check result and a short analysis.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "Project name, e.g. \"fireball\"." },
                    "ron": { "type": "string", "description": "Full project RON text." },
                    "force": { "type": "boolean", "description": "Save even with errors. Default false." }
                },
                "required": ["name", "ron"]
            }
        },
        {
            "name": "check_project",
            "description": "Fast validation only: parse (v1 or v2), structure, assets and texture files, expression types, bevy_hanabi shader generation and package round trip. Pass `name` or `ron`.",
            "inputSchema": {
                "type": "object",
                "properties": { "name": name_prop, "ron": ron_prop }
            }
        },
        {
            "name": "analyze_project",
            "description": "Text 'preview' instead of images: checks the project, simulates particles on the CPU and describes each layer (spawn pattern, counts, lifetime, motion extents, size, colors, visibility, blending, simulation space and trail, timeline). Pass `name` or `ron`.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": name_prop,
                    "ron": ron_prop,
                    "effect": { "type": "string", "description": "Only describe the layer with this name." },
                    "samples": { "type": "integer", "description": "Particles simulated per layer (20-2000, default 300)." },
                    "params": {
                        "type": "array",
                        "items": { "type": "object" },
                        "description": "Parameter sets to simulate and compare, each an object {\"name\": value} (number, or [r, g, b, a] linear colour); missing parameters keep their default. Default: every number parameter at its min, middle and max. The main report always uses the defaults."
                    }
                }
            }
        },
        {
            "name": "export_project",
            "description": "Export a saved project as a VFX package <name>.vfx.ron for a game: to the game assets folder's vfx/ when one is configured, else exported_effects/. Optionally copies it to another path too.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": name_prop,
                    "copy_to": { "type": "string", "description": "Optional extra destination file or folder, e.g. a game's assets/vfx/ folder." }
                },
                "required": ["name"]
            }
        },
        {
            "name": "import_asset",
            "description": "Import a texture file (PNG or JPG; KTX2 is copied but not inspected) into the game assets folder like the editor does: files outside it are copied to vfx/textures/ (name taken by another file -> numeric suffix, identical file -> reused), files inside it are used in place. Returns the asset entry to put in a project's `assets`; with `project`, adds it to that saved project. Guide section \"assets\".",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "The file to import (absolute, or relative to the Omagari folder)." },
                    "purpose": { "type": "string", "enum": ["Sprite", "Noise", "Mask", "Gradient"], "description": "What the texture is for; picks wrap and colour-space defaults. Default: guessed from the file name (noise/perlin/voronoi/cellular -> Noise, mask -> Mask, gradient/ramp -> Gradient, else Sprite)." },
                    "id": { "type": "string", "description": "Asset id for the new entry. Default: the file name without extension." },
                    "project": { "type": "string", "description": "A saved project (in saved_effects/) to add the asset to. The file is re-written in canonical form (comments are lost)." }
                },
                "required": ["path"]
            }
        },
        {
            "name": "list_assets",
            "description": "List the textures you can use (the 7 built-ins and the texture files in the game assets folder, with size, layout (RGBA / grayscale, bits) and which saved projects use them) and the glTF models of the game assets folder (preview models) with their size, height and animation clips.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "extract_channel",
            "description": "Write one channel (R, G, B, A or Luminance) of a project's texture asset as an 8-bit grayscale PNG next to it (<name>_<channel>.png; built-ins go to vfx/textures/) and add it to the saved project as a new asset with the same settings. Particle layers cannot pick a channel themselves, so use this for G/B/A of channel-packed textures.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": { "type": "string", "description": "A saved project (in saved_effects/)." },
                    "asset": { "type": "string", "description": "Id of the texture asset." },
                    "channel": { "type": "string", "enum": ["R", "G", "B", "A", "Luminance"] }
                },
                "required": ["project", "asset", "channel"]
            }
        },
        {
            "name": "preview_in_editor",
            "description": "Ask the running Omagari editor to load a saved project and play it, so the USER can watch it. You get no image back, only whether the editor picked up the request.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": name_prop,
                    "stage": {
                        "type": "string",
                        "enum": PREVIEW_STAGES,
                        "description": "Scene to play it in: \"air\" (middle of an empty room), \"floor\" (floor at y = 0), \"object\" (a ~2-unit rock centered on the spawn point, floor under it), \"capsule\" (a 0.7 x 1.8 m capsule, its centre is the target, floor 0.9 below), \"model\" (the project's preview model, or `model`; the capsule without one). Omit to keep the project's stage."
                    },
                    "camera": {
                        "type": "string",
                        "enum": PREVIEW_CAMERAS,
                        "description": "\"orbit\" (free perspective camera) or \"game\" (the game's isometric orthographic camera, 16 m visible height). Omit to keep the project's camera."
                    },
                    "motion": {
                        "type": "object",
                        "description": "Move the target while it plays, to judge Local vs Global layers and trails: {\"type\": \"circle\", \"radius\": 3.0, \"speed\": 3.6}, {\"type\": \"back_and_forth\", \"distance\": 4.0, \"speed\": 3.6} or {\"type\": \"none\"} (metres, m/s; speed defaults to 3.6, a running character). Omit to keep the project's preview motion.",
                        "properties": {
                            "type": { "type": "string", "enum": ["none", "circle", "back_and_forth"] },
                            "radius": { "type": "number" },
                            "distance": { "type": "number" },
                            "speed": { "type": "number" }
                        }
                    },
                    "params": {
                        "type": "object",
                        "description": "Parameter values to play it with, {\"name\": value} (number, or [r, g, b, a] linear colour), like the game's set_param. They become the project's preview values in the editor."
                    },
                    "trigger": {
                        "type": "string",
                        "description": "A trigger of the project to fire once it plays, like the game's trigger."
                    },
                    "model": {
                        "type": "string",
                        "description": "A glTF model (.glb/.gltf) of the game assets folder to play it on (sets the model stage), e.g. \"anim/tpose.glb\". list_assets lists the models, describe_model their nodes and clips."
                    },
                    "animation": {
                        "type": "string",
                        "description": "Animation clip to play on the model (its name, e.g. Mixamo's \"mixamo.com\"); \"\" = none."
                    },
                    "animation_file": {
                        "type": "string",
                        "description": "Another glTF file the clip comes from, with the same skeleton (node names), e.g. \"anim/run.glb\"; \"\" = the model's file."
                    },
                    "attach_to": {
                        "type": "string",
                        "description": "Node of the model the instance is attached to (the game's target entity); \"\" = the model's root."
                    },
                    "root_height": {
                        "type": "number",
                        "description": "Height of the target (the game's entity root) above the model's feet, metres (0.9 for a player root 0.9 m above the feet)."
                    }
                },
                "required": ["name"]
            }
        },
        {
            "name": "describe_model",
            "description": "Describe a glTF model (.glb/.gltf) of the game assets folder without rendering it: node hierarchy (names for sockets, attach_to and weapon nodes; bones marked, rest positions), meshes with their entity names and materials (surface layer targets), animation clips with lengths, and the rest-pose bounds / height. Guide section \"model\".",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "The model, relative to the game assets folder (e.g. \"anim/tpose.glb\") or absolute." },
                    "max_nodes": { "type": "integer", "description": "Nodes listed in the hierarchy (default 300)." }
                },
                "required": ["path"]
            }
        }
    ])
}

fn call_tool(name: &str, args: &Value, ctx: &Context) -> Result<String, String> {
    let str_arg = |key: &str| args.get(key).and_then(Value::as_str);
    let assets_root = ctx.assets_root();
    match name {
        "guide" => Ok(guide_section(str_arg("section"))),
        "list_projects" => Ok(list_projects(ctx)),
        "read_project" => {
            let path = resolve_project(str_arg("name").ok_or("missing `name`")?)?;
            let text = read(&path)?;
            let (project, notes) = parse_project(&text)?;
            if notes.migrated {
                // Agents copy what they read; give them the current format.
                let v2 = project.to_ron_string().map_err(|e| e.to_string())?;
                Ok(format!(
                    "// {} is a v1 project; shown migrated to format v2 (write_project saves v2).\n{}{}",
                    path.display(),
                    notes.comment_lines(),
                    v2
                ))
            } else {
                Ok(text)
            }
        }
        "write_project" => {
            let name = str_arg("name").ok_or("missing `name`")?;
            let ron_text = str_arg("ron").ok_or("missing `ron`")?;
            let force = args.get("force").and_then(Value::as_bool).unwrap_or(false);
            let file_name = project_file_name(name)?;
            let (project, notes) = parse_project(ron_text)?;
            let diagnostics = analysis::check_project(&project, &assets_root);
            if !diagnostics.errors.is_empty() && !force {
                return Err(format!("NOT SAVED.\n{}{}", notes.text(), diagnostics.render()));
            }
            // v1 input is stored migrated; v2 input is stored as written
            // (keeps the author's comments and layout).
            let text = if notes.migrated {
                project.to_ron_string().map_err(|e| e.to_string())?
            } else {
                ron_text.to_string()
            };
            let path = Folder::SavedEffects.full_file_path(file_name.clone());
            fs::write(&path, text).map_err(|e| format!("{}: {}", path.display(), e))?;
            Ok(format!(
                "Saved {}{}.\n{}{}",
                path.display(),
                if notes.migrated { " (migrated from v1 to format v2)" } else { "" },
                notes.text(),
                analysis::analyze_project(&project, &assets_root, 200, None)
            ))
        }
        "check_project" => {
            let (project, notes) = load_from_args(args)?;
            let diagnostics = analysis::check_project(&project, &assets_root);
            let text = format!("{}{}", notes.text(), diagnostics.render());
            if diagnostics.errors.is_empty() { Ok(text) } else { Err(text) }
        }
        "analyze_project" => {
            let (project, notes) = load_from_args(args)?;
            let samples = args.get("samples").and_then(Value::as_u64).unwrap_or(300) as usize;
            let sets = match args.get("params") {
                None | Some(Value::Null) => None,
                Some(Value::Array(items)) => {
                    let mut sets = Vec::new();
                    for item in items {
                        let set = param_set_from_json(item)?;
                        for (name, value) in set.iter() {
                            match project.param(name) {
                                None => return Err(format!("params: \"{}\" is not a parameter of this project", name)),
                                Some(p) if p.kind() != value.kind() => {
                                    return Err(format!("params: \"{}\" is a {} parameter", name, p.kind().name()))
                                }
                                _ => {}
                            }
                        }
                        let label = format!("{}", sets.len() + 1);
                        sets.push((label, set));
                    }
                    Some(sets)
                }
                Some(other) => return Err(format!("params must be an array of objects, got {}", other)),
            };
            Ok(format!(
                "{}{}",
                notes.text(),
                analysis::analyze_project_with_sets(&project, &assets_root, samples, str_arg("effect"), sets.as_deref())
            ))
        }
        "export_project" => {
            let path = resolve_project(str_arg("name").ok_or("missing `name`")?)?;
            let (project, _) = parse_project(&read(&path)?)?;
            let diagnostics = analysis::check_project(&project, &assets_root);
            if !diagnostics.errors.is_empty() {
                return Err(format!("NOT EXPORTED.\n{}", diagnostics.render()));
            }
            let base = path
                .file_name()
                .and_then(|f| f.to_str())
                .and_then(|f| f.split('.').next())
                .unwrap_or("effect")
                .to_string();
            let out = crate::controller::export_package(&project, &base, &ctx.game_assets.export_dir())
                .map_err(|e| format!("export failed: {}", e))?;
            let file_name = out.file_name().unwrap().to_string_lossy().to_string();
            let mut msg = format!("Exported {}", out.display());
            if let Some(copy_to) = str_arg("copy_to") {
                let mut dest = PathBuf::from(copy_to);
                if dest.is_dir() || copy_to.ends_with('/') || copy_to.ends_with('\\') {
                    fs::create_dir_all(&dest).map_err(|e| format!("{}: {}", dest.display(), e))?;
                    dest = dest.join(&file_name);
                }
                fs::copy(&out, &dest).map_err(|e| format!("{}: {}", dest.display(), e))?;
                msg += &format!(" and copied it to {}", dest.display());
            }
            msg += &format!(".\n{}", texture_list(&project));
            msg += "\nThe game loads it with `asset_server.load(\"vfx/<name>.vfx.ron\")` and plays it with `commands.spawn_vfx(&handle, target)` (guide section \"game\").";
            Ok(msg)
        }
        "preview_in_editor" => {
            let path = resolve_project(str_arg("name").ok_or("missing `name`")?)?;
            let saved = Path::new(Folder::SavedEffects.to_path());
            if path.parent().map(|p| p.canonicalize().ok()) != Some(saved.canonicalize().ok()) {
                return Err(format!(
                    "{} is not in saved_effects/. Save it with write_project first.",
                    path.display()
                ));
            }
            let file_name = path.file_name().unwrap().to_string_lossy().to_string();
            let stage = match str_arg("stage") {
                None => None,
                Some(s) if PREVIEW_STAGES.contains(&s) => Some(s.to_string()),
                Some(other) => return Err(format!("unknown stage \"{}\": use air, floor, object, capsule or model", other)),
            };
            let motion = match args.get("motion") {
                None | Some(Value::Null) => None,
                Some(m) => Some(motion_from_json(m)?),
            };
            let camera = match str_arg("camera") {
                None => None,
                Some(c) if PREVIEW_CAMERAS.contains(&c) => Some(c.to_string()),
                Some(other) => return Err(format!("unknown camera \"{}\": use orbit or game", other)),
            };
            let (project, _) = parse_project(&read(&path)?)?;
            let params = match args.get("params") {
                None | Some(Value::Null) => Vec::new(),
                Some(p) => param_set_from_json(p)?,
            };
            for (name, value) in params.iter() {
                match project.param(name) {
                    None => return Err(format!("params: \"{}\" is not a parameter of {}", name, file_name)),
                    Some(p) if p.kind() != value.kind() => {
                        return Err(format!("params: \"{}\" is a {} parameter", name, p.kind().name()))
                    }
                    _ => {}
                }
            }
            let trigger = str_arg("trigger").map(str::to_string);
            if let Some(t) = &trigger {
                if project.trigger(t).is_none() {
                    let known: Vec<&str> = project.triggers.iter().map(|t| t.name.as_str()).collect();
                    return Err(format!("\"{}\" is not a trigger of {} (triggers: {})", t, file_name, known.join(", ")));
                }
            }
            let model_args = preview_model_args(args, &project, ctx)?;
            let preview = PreviewRequest {
                name: file_name.clone(),
                stage: stage.clone(),
                motion,
                camera: camera.clone(),
                params: params.clone(),
                trigger: trigger.clone(),
                model: model_args.model.clone(),
                animation: model_args.animation.clone(),
                animation_file: model_args.animation_file.clone(),
                attach_to: model_args.attach_to.clone(),
                root_height: model_args.root_height,
            };
            let request = Folder::SavedEffects.full_file_path(PREVIEW_REQUEST_FILE.to_string());
            fs::write(&request, preview.to_json()).map_err(|e| format!("{}: {}", request.display(), e))?;
            // The editor deletes the request once it has loaded the project.
            let start = Instant::now();
            while start.elapsed() < Duration::from_secs(3) {
                if !request.exists() {
                    let in_stage = stage.map(|s| format!(" in the {} stage", s)).unwrap_or_default();
                    let moving = motion.map(|m| format!(", the target {}", motion_text(m))).unwrap_or_default();
                    let seen = camera.map(|c| format!(", seen through the {} camera", c)).unwrap_or_default();
                    let with = if params.is_empty() {
                        String::new()
                    } else {
                        let values: Vec<String> = params.iter().map(|(n, v)| format!("{} = {}", n, param_value_to_json(v))).collect();
                        format!(", with {}", values.join(", "))
                    };
                    let fired = trigger.map(|t| format!(", and fired \"{}\"", t)).unwrap_or_default();
                    return Ok(format!(
                        "The editor loaded {} and is playing it for the user{}{}{}{}{}{}.",
                        file_name,
                        in_stage,
                        model_args.describe(),
                        moving,
                        seen,
                        with,
                        fired
                    ));
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Ok(format!(
                "Request written, but no editor picked it up within 3s. Ask the user to start the editor \
                 (`cargo run --release --features editor`); it will load {} on startup.",
                file_name
            ))
        }
        "import_asset" => import_asset(args, ctx),
        "describe_model" => {
            let path = str_arg("path").ok_or("missing `path`")?.trim().replace('\\', "/");
            let file = if Path::new(&path).is_absolute() { PathBuf::from(&path) } else { assets_root.join(&path) };
            if !file.is_file() {
                let found: Vec<String> =
                    crate::model_info::scan_models(&assets_root, 50).into_iter().map(|(p, _)| p).collect();
                return Err(format!(
                    "{} not found (game assets folder {}; models: {})",
                    path,
                    assets_root.display(),
                    if found.is_empty() { "none".to_string() } else { found.join(", ") }
                ));
            }
            let info = crate::model_info::ModelInfo::cached(&file)?;
            let max = args.get("max_nodes").and_then(Value::as_u64).unwrap_or(300) as usize;
            let relative = library::relative_game_path(&file, &assets_root).unwrap_or(path.clone());
            Ok(format!(
                "{}: {}Use it as `preview: Some((stage: Model, model: Some(\"{}\"), ..))`; sockets name nodes with `node: Some(\"...\")`.",
                relative,
                info.describe(max),
                relative
            ))
        }
        "list_assets" => Ok(list_assets(ctx)),
        "extract_channel" => {
            let project_name = str_arg("project").ok_or("missing `project`")?;
            let id = str_arg("asset").ok_or("missing `asset`")?;
            let channel_name = str_arg("channel").ok_or("missing `channel`")?;
            let channel = ChannelDef::parse(channel_name)
                .ok_or_else(|| format!("unknown channel \"{}\": use R, G, B, A or Luminance", channel_name))?;
            let path = resolve_saved_project(project_name)?;
            let (mut project, _) = parse_project(&read(&path)?)?;
            let (new_id, new_path) = library::extract_channel_asset(&mut project, id, channel, &assets_root)?;
            save_project_file(&path, &project)?;
            let info = library::texture_info_at(&new_path, &assets_root).map(|i| i.describe()).unwrap_or_default();
            Ok(format!(
                "Wrote {} ({}) from the {} channel of \"{}\" and added it to {} as asset \"{}\" (same purpose and settings). \
                 Use it on a particle layer with `texture: Some(\"{}\")` and `ParticleTexture((sample_mapping: ModulateOpacityFromR))`.",
                new_path,
                info,
                channel.name(),
                id,
                path.display(),
                new_id,
                new_id
            ))
        }
        _ => Err(format!("unknown tool: {}", name)),
    }
}

/// The model arguments of `preview_in_editor`, checked against the game
/// assets folder.
#[derive(Default)]
struct PreviewModelArgs {
    model: Option<String>,
    animation: Option<String>,
    animation_file: Option<String>,
    attach_to: Option<String>,
    root_height: Option<f32>,
}

impl PreviewModelArgs {
    fn describe(&self) -> String {
        let mut out = String::new();
        if let Some(m) = &self.model {
            out += &format!(" on the model {}", m);
        }
        if let Some(a) = self.animation.as_ref().filter(|a| !a.is_empty()) {
            out += &format!(
                " playing \"{}\"{}",
                a,
                self.animation_file.as_ref().filter(|f| !f.is_empty()).map(|f| format!(" from {}", f)).unwrap_or_default()
            );
        }
        if let Some(n) = self.attach_to.as_ref().filter(|n| !n.is_empty()) {
            out += &format!(", attached to \"{}\"", n);
        }
        out
    }
}

fn preview_model_args(args: &Value, project: &ProjectV2, ctx: &Context) -> Result<PreviewModelArgs, String> {
    let text = |key: &str| args.get(key).and_then(Value::as_str).map(|s| s.trim().replace('\\', "/"));
    let root = ctx.assets_root();
    let describe = |path: &str| -> Result<std::sync::Arc<crate::model_info::ModelInfo>, String> {
        let file = root.join(path);
        if !file.is_file() {
            let found: Vec<String> = crate::model_info::scan_models(&root, 30).into_iter().map(|(p, _)| p).collect();
            return Err(format!(
                "{} is not a model in the game assets folder {} (models: {})",
                path,
                root.display(),
                if found.is_empty() { "none".to_string() } else { found.join(", ") }
            ));
        }
        crate::model_info::ModelInfo::cached(&file)
    };
    let out = PreviewModelArgs {
        model: text("model").filter(|m| !m.is_empty()),
        animation: text("animation"),
        animation_file: text("animation_file"),
        attach_to: text("attach_to"),
        root_height: args.get("root_height").and_then(Value::as_f64).map(|h| h as f32),
    };
    let preview = project.preview.clone().unwrap_or_default();
    let model = out.model.clone().or(preview.model.clone());
    let body = match &model {
        Some(m) => Some(describe(m)?),
        None => None,
    };
    if let Some(clip) = out.animation.as_ref().filter(|a| !a.is_empty()) {
        let file = out.animation_file.clone().filter(|f| !f.is_empty()).or(preview.animation_file.clone()).or(model.clone());
        let file = file.ok_or("animation needs a model (or animation_file)")?;
        let info = describe(&file)?;
        if info.animation(clip).is_none() {
            let known: Vec<String> = info.animations.iter().map(|a| format!("\"{}\"", a.label())).collect();
            return Err(format!("{} has no animation \"{}\" (it has: {})", file, clip, if known.is_empty() { "none".into() } else { known.join(", ") }));
        }
    }
    if let (Some(node), Some(body)) = (out.attach_to.as_ref().filter(|n| !n.is_empty()), &body) {
        if preview.target == omagari::format::PreviewTargetModel::Body && body.node(node).is_none() {
            return Err(format!("attach_to: {} has no node \"{}\" (describe_model lists its nodes)", model.unwrap_or_default(), node));
        }
    }
    Ok(out)
}

/// A project in `saved_effects/` (tools that change projects refuse the
/// bundled examples).
fn resolve_saved_project(name: &str) -> Result<PathBuf, String> {
    if name.trim().replace('\\', "/").starts_with("examples/") {
        return Err("bundled examples are read-only; save a copy with write_project first".into());
    }
    resolve_project(name)
}

/// Writes a project changed by a tool (canonical v2 text).
fn save_project_file(path: &Path, project: &ProjectV2) -> Result<(), String> {
    let text = project.to_ron_string().map_err(|e| e.to_string())?;
    fs::write(path, text).map_err(|e| format!("{}: {}", path.display(), e))
}

fn parse_purpose(text: &str) -> Result<TexturePurpose, String> {
    TexturePurpose::ALL
        .into_iter()
        .find(|p| p.name().eq_ignore_ascii_case(text.trim()))
        .ok_or_else(|| format!("unknown purpose \"{}\": use Sprite, Noise, Mask or Gradient", text))
}

/// The asset entry as RON, ready to paste into a project.
fn asset_entry_ron(project: &ProjectV2, id: &str) -> String {
    project
        .asset(id)
        .and_then(|a| ron::to_string(a).ok())
        .unwrap_or_default()
}

fn import_asset(args: &Value, ctx: &Context) -> Result<String, String> {
    let str_arg = |key: &str| args.get(key).and_then(Value::as_str);
    let source = PathBuf::from(str_arg("path").ok_or("missing `path`")?);
    let root = ctx.assets_root();
    let file_name = source.file_name().unwrap_or_default().to_string_lossy().to_string();
    let purpose = match str_arg("purpose") {
        Some(p) => parse_purpose(p)?,
        None => library::guess_purpose(&file_name),
    };
    let project_path = str_arg("project").map(resolve_saved_project).transpose()?;
    let imported = library::import_file(&source, &root)?;

    let mut out = format!("{}: {} -> {}.\n", source.display(), imported.kind.describe(), imported.path);
    match library::texture_info_at(&imported.path, &root) {
        Ok(info) => {
            out += &format!("Texture: {}.\n", info.describe());
            if info.loads_as_integer_texture() {
                out += "WARNING: 16-bit grayscale PNGs load in Bevy as integer textures that cannot be sampled; \
                        extract_channel(..., \"Luminance\") writes an 8-bit copy.\n";
            }
        }
        Err(_) => out += "Texture: not inspected (only PNG and JPG are read).\n",
    }
    if matches!(library::extension(&imported.path).as_str(), "jpg" | "jpeg") {
        out += "Note: the game needs Bevy's `jpeg` feature to load JPG files (PNG works by default).\n";
    }

    let mut project = match &project_path {
        Some(path) => parse_project(&read(path)?)?.0,
        None => ProjectV2::default(),
    };
    let (id, added) = library::add_texture_asset(&mut project, &imported.path, purpose, str_arg("id"))?;
    let entry = asset_entry_ron(&project, &id);
    match &project_path {
        Some(path) => {
            if added {
                save_project_file(path, &project)?;
                out += &format!("Added to {} as asset \"{}\": {}\n", path.display(), id, entry);
            } else {
                out += &format!("{} already has this file as asset \"{}\": {}\n", path.display(), id, entry);
            }
            out += &format!("Use it on a layer with `texture: Some(\"{}\")`.", id);
        }
        None => {
            out += &format!(
                "Asset entry for a project's `assets` list:\n{}\nUse it on a layer with `texture: Some(\"{}\")`.",
                entry, id
            );
        }
    }
    Ok(out)
}

fn list_assets(ctx: &Context) -> String {
    let root = ctx.assets_root();
    // Which saved projects use which texture path.
    let mut users: Vec<(String, String)> = Vec::new();
    for file in list_files(Path::new(Folder::SavedEffects.to_path()), ".omagari.ron") {
        let path = Path::new(Folder::SavedEffects.to_path()).join(&file);
        let Ok(text) = fs::read_to_string(&path) else { continue };
        let Ok((project, _)) = ProjectV2::from_ron_str_with_warnings(&text) else { continue };
        for asset in project.assets.iter() {
            let AssetKind::Texture(t) = &asset.kind;
            users.push((t.path.clone(), file.trim_end_matches(".omagari.ron").to_string()));
        }
    }
    let used_by = |path: &str| {
        let mut names: Vec<&str> = users.iter().filter(|(p, _)| p == path).map(|(_, n)| n.as_str()).collect();
        names.dedup();
        if names.is_empty() { "not used by a saved project".to_string() } else { format!("used by {}", names.join(", ")) }
    };
    let describe = |path: &str| {
        library::texture_info_at(path, &root).map(|i| i.describe()).unwrap_or_else(|_| "not inspected".into())
    };

    let mut out = String::from("Built-in textures (embedded in the library; id = file name without .png):\n");
    for (file, _) in omagari::runtime::BUILTIN_TEXTURES.iter() {
        let path = format!("{}{}", BUILTIN_TEXTURES_PREFIX, file);
        out += &format!("- {} - {}; {}\n", path, describe(&path), used_by(&path));
    }
    let found = library::scan_textures(&root, 500);
    out += &format!("\nTextures in the game assets folder {} ({}):\n", root.display(), found.len());
    if found.is_empty() {
        out += "- none. Import files with import_asset.\n";
    }
    for (path, bytes) in found.iter() {
        out += &format!("- {} - {}, {} KB; {}\n", path, describe(path), bytes.div_ceil(1024), used_by(path));
    }
    if found.len() >= 500 {
        out += "(list cut at 500 files)\n";
    }
    let models = crate::model_info::scan_models(&root, 200);
    out += &format!(
        "\nModels in the game assets folder ({}; preview models: preview_in_editor(model: ..), describe_model(path) for nodes):\n",
        models.len()
    );
    if models.is_empty() {
        out += "- none.\n";
    }
    for (path, bytes) in models.iter() {
        let info = crate::model_info::ModelInfo::cached(&root.join(path))
            .map(|i| i.summary())
            .unwrap_or_else(|e| format!("not readable: {}", e));
        out += &format!("- {} - {} KB; {}\n", path, bytes.div_ceil(1024), info);
    }
    out
}

fn guide_section(section: Option<&str>) -> String {
    let Some(section) = section.map(|s| s.trim().to_lowercase()).filter(|s| !s.is_empty() && s != "all") else {
        return GUIDE.to_string();
    };
    // Sections are the `## ` headings; match on the first word of the title.
    let mut out = String::new();
    let mut keep = false;
    for line in GUIDE.lines() {
        if let Some(title) = line.strip_prefix("## ") {
            keep = title.to_lowercase().split_whitespace().next() == Some(section.as_str());
        }
        if keep {
            out.push_str(line);
            out.push('\n');
        }
    }
    if out.is_empty() {
        let titles: Vec<&str> = GUIDE.lines().filter_map(|l| l.strip_prefix("## ")).collect();
        format!("No section \"{}\". Sections: {}", section, titles.join(" | "))
    } else {
        out
    }
}

fn list_files(dir: &Path, suffix: &str) -> Vec<String> {
    let mut files: Vec<String> = fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().to_string())
                .filter(|f| f.ends_with(suffix))
                .collect()
        })
        .unwrap_or_default();
    files.sort();
    files
}

fn list_projects(ctx: &Context) -> String {
    let cwd = std::env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    let export_dir = ctx.game_assets.export_dir();
    let game = match &ctx.game_assets.configured {
        Some((folder, origin)) => format!("{} (from {})", folder.display(), origin.describe()),
        None => "not set (textures resolve in assets/, exports go to exported_effects/)".to_string(),
    };
    let exported = Path::new(Folder::ExportedEffects.to_path());
    format!(
        "root: {}\ngame assets folder: {}\nsaved_effects/: {}\nexamples/: {}\nexports in {}: {}\nlegacy exports (*.hanabi.ron) in exported_effects/: {}\n",
        cwd,
        game,
        list_files(Path::new(Folder::SavedEffects.to_path()), ".omagari.ron").join(", "),
        list_files(Path::new("examples"), ".omagari.ron").join(", "),
        export_dir.display(),
        list_files(&export_dir, ".vfx.ron").join(", "),
        list_files(exported, ".hanabi.ron").join(", "),
    )
}

/// `fireball`, `fireball.omagari.ron` → `fireball.omagari.ron`.
fn project_file_name(name: &str) -> Result<String, String> {
    let base = name.trim().trim_end_matches(".omagari.ron");
    if base.is_empty() || base.contains(['/', '\\', '.']) || base.contains("..") {
        return Err(format!(
            "invalid project name \"{}\": use a plain name like \"fireball\"",
            name
        ));
    }
    Ok(format!("{}.omagari.ron", base))
}

fn resolve_project(name: &str) -> Result<PathBuf, String> {
    let name = name.trim().replace('\\', "/");
    if let Some(example) = name.strip_prefix("examples/") {
        let path = Path::new("examples").join(project_file_name(example)?);
        return if path.exists() { Ok(path) } else { Err(format!("{} not found", path.display())) };
    }
    let name = name.strip_prefix(Folder::SavedEffects.to_path()).unwrap_or(&name);
    let path = Path::new(Folder::SavedEffects.to_path()).join(project_file_name(name)?);
    if path.exists() {
        Ok(path)
    } else {
        Err(format!(
            "{} not found. Saved: {}",
            path.display(),
            list_files(Path::new(Folder::SavedEffects.to_path()), ".omagari.ron").join(", ")
        ))
    }
}

fn read(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|e| format!("{}: {}", path.display(), e))
}

/// How a project text was read: migrated from v1, and the migration notes.
#[derive(Default)]
struct ParseNotes {
    migrated: bool,
    warnings: Vec<String>,
}

impl ParseNotes {
    /// Lines to put before a tool's output.
    fn text(&self) -> String {
        let mut out = String::new();
        if self.migrated {
            out.push_str("Input is a v1 project; it was migrated to format v2.\n");
        }
        for w in self.warnings.iter() {
            out.push_str(&format!("Note: {}\n", w));
        }
        out
    }

    fn comment_lines(&self) -> String {
        self.warnings.iter().map(|w| format!("// note: {}\n", w)).collect()
    }
}

/// Parses v1 or v2 project text; v1 is migrated to v2.
fn parse_project(text: &str) -> Result<(ProjectV2, ParseNotes), String> {
    let (project, warnings) =
        ProjectV2::from_ron_str_with_warnings(text).map_err(|e| format!("RON parse error: {}", e))?;
    let migrated = is_v1(text);
    Ok((project, ParseNotes { migrated, warnings }))
}

/// v1 projects are a plain `effects` list without `version`.
fn is_v1(text: &str) -> bool {
    omagari::format::OmagariProject::from_ron_str(text).is_ok()
}

fn load_from_args(args: &Value) -> Result<(ProjectV2, ParseNotes), String> {
    if let Some(text) = args.get("ron").and_then(Value::as_str) {
        return parse_project(text);
    }
    let name = args
        .get("name")
        .and_then(Value::as_str)
        .ok_or("pass `name` or `ron`")?;
    parse_project(&read(&resolve_project(name)?)?)
}

/// The textures an exported package needs, and where they come from.
fn texture_list(project: &ProjectV2) -> String {
    let mut builtin = Vec::new();
    let mut game = Vec::new();
    for asset in project.assets.iter() {
        let AssetKind::Texture(t) = &asset.kind;
        if t.path.starts_with(BUILTIN_TEXTURES_PREFIX) {
            builtin.push(format!("{}={}", asset.id, t.path));
        } else {
            game.push(format!("{}={}", asset.id, t.path));
        }
    }
    let mut out = String::new();
    if !builtin.is_empty() {
        out += &format!("Built-in textures (embedded in the omagari library, nothing to copy): {}.", builtin.join(", "));
    }
    if !game.is_empty() {
        if !out.is_empty() {
            out.push(' ');
        }
        out += &format!("Game textures (must exist under the game's assets/ folder): {}.", game.join(", "));
    }
    if out.is_empty() {
        out = "No textures.".into();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> Context {
        Context::new(GameAssets::default())
    }

    /// The first recipe of the guide (a v2 project).
    fn recipe() -> String {
        guide_recipes().remove(0).ron
    }

    #[test]
    fn recipes_have_titles_and_file_names() {
        let recipes = guide_recipes();
        assert!(recipes.len() >= 6, "{}", recipes.len());
        assert_eq!(recipes[0].title, "Spark burst (one-shot impact)");
        assert_eq!(recipes[0].file_name(), "spark_burst.omagari.ron");
        for recipe in &recipes {
            assert!(recipe.ron.contains("version: 2"), "{}", recipe.title);
            assert!(crate::controller::validate_project_filename(&recipe.file_name()), "{}", recipe.file_name());
        }
    }

    /// `analyze_project(params: [...])` simulates the given sets; names and
    /// kinds are checked against the project.
    #[test]
    fn analyze_takes_parameter_sets() {
        let ron = guide_recipes()
            .into_iter()
            .find(|r| r.title.starts_with("Intensity progression"))
            .expect("recipe")
            .ron;
        let args = json!({ "ron": ron, "samples": 50, "params": [{ "intensity": 0.1 }, { "intensity": 0.95 }] });
        let report = call_tool("analyze_project", &args, &ctx()).unwrap();
        assert!(report.contains("set 1: intensity=0.10") && report.contains("set 2: intensity=0.95"), "{}", report);
        assert!(report.contains("[embers] changes with the parameters"), "{}", report);
        assert!(report.contains("[flare] changes with the parameters"), "the flare tier appears: {}", report);

        let bad = json!({ "ron": args["ron"], "params": [{ "power": 1.0 }] });
        assert!(call_tool("analyze_project", &bad, &ctx()).unwrap_err().contains("\"power\" is not a parameter"));
        let bad = json!({ "ron": args["ron"], "params": [{ "intensity": [1.0, 0.0, 0.0] }] });
        assert!(call_tool("analyze_project", &bad, &ctx()).unwrap_err().contains("is a Float parameter"));
    }

    #[test]
    fn check_and_analyze_accept_v2_text() {
        let args = json!({ "ron": recipe() });
        let check = call_tool("check_project", &args, &ctx()).unwrap();
        assert!(check.starts_with("OK: no errors."), "{}", check);
        assert!(!check.contains("WARNINGS"), "{}", check);
        let report = call_tool("analyze_project", &args, &ctx()).unwrap();
        assert!(report.contains("space: Local"), "{}", report);
        assert!(report.contains("render: blend Blend; texture sampling ModulateOpacityFromR; orient AlongVelocity"), "{}", report);
    }

    #[test]
    fn check_reports_v1_input_as_migrated() {
        let v1 = r#"(effects: [(name: "a", capacity: 10, texture_index: Some(5),
            spawner_settings: (count: Single(5.0), spawn_duration: Single(0.0), period: Single(1.0), cycle_count: 1, starts_active: true, emit_on_start: true),
            init_modifiers: [SetAttribute((attr: "age", attr_expr: Float(0.0))), SetAttribute((attr: "velocity", attr_expr: Vec3((0.0, 1.0, 0.0))))],
            update_modifiers: [], render_modifiers: [])])"#;
        let check = call_tool("check_project", &json!({ "ron": v1 }), &ctx()).unwrap();
        assert!(check.starts_with("Input is a v1 project; it was migrated to format v2."), "{}", check);
        assert!(is_v1(v1));
        assert!(!is_v1(&recipe()));
    }

    #[test]
    fn check_errors_are_tool_errors() {
        let broken = r#"(version: 2, layers: [Particles((name: "a", texture: Some("missing")))])"#;
        let err = call_tool("check_project", &json!({ "ron": broken }), &ctx()).unwrap_err();
        assert!(err.contains("texture \"missing\" is not in the assets list"), "{}", err);
    }

    /// `describe_model`, the models of `list_assets` and the model arguments
    /// of `preview_in_editor`, on generated glTF files in a temporary game
    /// assets folder.
    #[test]
    fn models_are_described_listed_and_checked() {
        let dir = crate::asset_library::tests::temp_dir("mcp_models");
        crate::model_info::tests::write_fixture(&dir.join("chars/hero.gltf"), Some(("Wave", 1.5)));
        crate::model_info::tests::write_fixture(&dir.join("anim/swing.gltf"), Some(("Swing", 0.5)));
        let ctx = Context::new(GameAssets::resolve(Some(dir.to_string_lossy().to_string()), None, &Default::default()));

        let text = call_tool("describe_model", &json!({ "path": "chars/hero.gltf" }), &ctx).unwrap();
        assert!(text.starts_with("chars/hero.gltf: 6 nodes (3 bones, 2 mesh nodes)"), "{}", text);
        assert!(text.contains("      RightHand [bone] (-0.40, 1.60, 0.00)"), "{}", text);
        assert!(text.contains("- \"Wave\": 1.500 s"), "{}", text);
        assert!(text.contains("model: Some(\"chars/hero.gltf\")"), "{}", text);
        let absolute = dir.join("anim/swing.gltf").to_string_lossy().to_string();
        assert!(call_tool("describe_model", &json!({ "path": absolute }), &ctx).unwrap().contains("\"Swing\""));
        let missing = call_tool("describe_model", &json!({ "path": "nope.glb" }), &ctx).unwrap_err();
        assert!(missing.contains("anim/swing.gltf, chars/hero.gltf"), "{}", missing);

        let list = call_tool("list_assets", &json!({}), &ctx).unwrap();
        assert!(list.contains("Models in the game assets folder (2"), "{}", list);
        assert!(list.contains("- chars/hero.gltf - ") && list.contains("animations: \"Wave\" 1.50 s"), "{}", list);

        let project = ProjectV2::default();
        let args = json!({ "model": "chars/hero.gltf", "animation": "Swing", "animation_file": "anim/swing.gltf", "attach_to": "Spine" });
        let ok = preview_model_args(&args, &project, &ctx).unwrap();
        assert_eq!(ok.describe(), " on the model chars/hero.gltf playing \"Swing\" from anim/swing.gltf, attached to \"Spine\"");
        let err = |args: Value| preview_model_args(&args, &project, &ctx).err().unwrap_or_default();
        assert!(err(json!({ "model": "chars/hero.gltf", "animation": "Run" })).contains("has no animation \"Run\" (it has: \"Wave\")"));
        assert!(err(json!({ "model": "chars/hero.gltf", "attach_to": "Tail" })).contains("has no node \"Tail\""));
        assert!(err(json!({ "model": "chars/villain.glb" })).contains("is not a model in the game assets folder"));
        assert!(err(json!({ "animation": "Wave" })).contains("animation needs a model"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn preview_requests_are_json_and_old_files_still_read() {
        let request = PreviewRequest {
            name: "fire.omagari.ron".into(),
            stage: Some("floor".into()),
            motion: Some(Some(PreviewMotion::Circle { radius: 2.0, speed: 3.6 })),
            camera: Some("game".into()),
            params: vec![
                ("intensity".into(), ParamValue::Float(0.75)),
                ("tint".into(), ParamValue::Color(bevy::math::Vec4::new(1.0, 0.5, 0.0, 1.0))),
            ],
            trigger: Some("hit".into()),
            model: Some("anim/tpose.glb".into()),
            animation: Some("mixamo.com".into()),
            animation_file: Some("anim/run.glb".into()),
            attach_to: Some("".into()),
            root_height: Some(0.9),
        };
        let text = request.to_json();
        assert!(text.starts_with('{'), "{}", text);
        assert_eq!(PreviewRequest::parse(&text).unwrap(), request);

        let still = PreviewRequest {
            motion: Some(None),
            stage: None,
            camera: None,
            params: Vec::new(),
            trigger: None,
            model: None,
            animation: Some("".into()),
            animation_file: None,
            attach_to: None,
            root_height: None,
            ..request.clone()
        };
        assert_eq!(PreviewRequest::parse(&still.to_json()).unwrap(), still);

        // Applied to a project: the model sets the model stage, "" clears.
        let mut project = ProjectV2::default();
        project.preview = Some(omagari::format::PreviewDef { attach_to: Some("Hips".into()), ..Default::default() });
        request.apply_to(&mut project);
        let preview = project.preview.clone().unwrap();
        assert_eq!(preview.stage, omagari::format::PreviewStage::Model);
        assert_eq!(preview.model.as_deref(), Some("anim/tpose.glb"));
        assert_eq!(preview.animation_source(), Some("anim/run.glb"));
        assert_eq!((preview.attach_to, preview.root_height), (None, 0.9));
        assert_eq!(preview.params.len(), 2);
        still.apply_to(&mut project);
        assert_eq!(project.preview.as_ref().unwrap().animation, None);
        assert_eq!(
            param_value_from_json(&json!([0.1, 0.2, 0.3])).unwrap(),
            ParamValue::Color(bevy::math::Vec4::new(0.1, 0.2, 0.3, 1.0))
        );
        assert!(param_value_from_json(&json!("red")).is_err());
        assert!(param_set_from_json(&json!([1.0])).is_err());

        let old = PreviewRequest::parse("fire.omagari.ron\nobject").unwrap();
        assert_eq!(old.name, "fire.omagari.ron");
        assert_eq!(old.stage.as_deref(), Some("object"));
        assert_eq!(old.motion, None);
        assert_eq!(PreviewRequest::parse("fire.omagari.ron\n").unwrap().stage, None);

        assert_eq!(
            motion_from_json(&json!({ "type": "back_and_forth" })).unwrap(),
            Some(PreviewMotion::BackAndForth { distance: 4.0, speed: 3.6 })
        );
        assert!(motion_from_json(&json!({ "type": "zigzag" })).is_err());
        assert!(motion_from_json(&json!({ "type": "circle", "radius": 0.0 })).is_err());
    }

    /// Removes a test project from `saved_effects/` even when the test fails.
    struct TempProject(PathBuf);

    impl Drop for TempProject {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }

    /// import_asset (with and without a project), extract_channel and
    /// list_assets against a temporary game assets folder.
    #[test]
    fn asset_tools_import_extract_and_list() {
        let dir = crate::asset_library::tests::temp_dir("mcp_assets");
        let game = dir.join("game");
        fs::create_dir_all(&game).unwrap();
        crate::asset_library::tests::write_test_png(&dir.join("pack_mask.png"));
        let ctx = Context::new(GameAssets::resolve(Some(game.to_string_lossy().to_string()), None, &Default::default()));

        let source = dir.join("pack_mask.png").to_string_lossy().to_string();
        let out = call_tool("import_asset", &json!({ "path": source }), &ctx).unwrap();
        assert!(out.contains("copied into the game assets folder -> vfx/textures/pack_mask.png"), "{}", out);
        assert!(out.contains("4×2 PNG, RGBA 8-bit"), "{}", out);
        assert!(out.contains(r#"(id:"pack_mask",kind:Texture((path:"vfx/textures/pack_mask.png",purpose:Mask"#), "{}", out);
        assert!(game.join("vfx/textures/pack_mask.png").is_file());
        assert!(call_tool("import_asset", &json!({ "path": source, "purpose": "Shiny" }), &ctx).is_err());

        // Into a saved project.
        let name = format!("mcp_asset_test_{}", std::process::id());
        let file = Folder::SavedEffects.full_file_path(format!("{}.omagari.ron", name));
        let _guard = TempProject(file.clone());
        fs::write(&file, recipe()).unwrap();
        let out = call_tool("import_asset", &json!({ "path": source, "project": name, "id": "pack" }), &ctx).unwrap();
        assert!(out.contains("identical file was already in the game assets folder"), "{}", out);
        assert!(out.contains("as asset \"pack\""), "{}", out);
        let project = ProjectV2::from_ron_str(&fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(project.texture("pack").unwrap().purpose, TexturePurpose::Mask);

        let out = call_tool("extract_channel", &json!({ "project": name, "asset": "pack", "channel": "G" }), &ctx).unwrap();
        assert!(out.contains("vfx/textures/pack_mask_g.png (4×2 PNG, grayscale 8-bit)"), "{}", out);
        assert!(out.contains("asset \"pack_g\""), "{}", out);
        let project = ProjectV2::from_ron_str(&fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(project.texture("pack_g").unwrap().path, "vfx/textures/pack_mask_g.png");
        assert!(call_tool("extract_channel", &json!({ "project": name, "asset": "pack", "channel": "X" }), &ctx).is_err());
        assert!(call_tool("extract_channel", &json!({ "project": "examples/fireworks", "asset": "a", "channel": "R" }), &ctx)
            .unwrap_err()
            .contains("read-only"));

        let list = call_tool("list_assets", &json!({}), &ctx).unwrap();
        assert!(list.contains("omagari://textures/cloud.png - 333×333 PNG, RGBA 8-bit"), "{}", list);
        assert!(list.contains(&format!("- vfx/textures/pack_mask.png - 4×2 PNG, RGBA 8-bit, 1 KB; used by {}", name)), "{}", list);
        assert!(list.contains("vfx/textures/pack_mask_g.png"), "{}", list);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn guide_sections_are_listed() {
        for section in ["workflow", "format", "layers", "attach", "model", "textures", "assets", "recipes", "game"] {
            assert!(!guide_section(Some(section)).starts_with("No section"), "{}", section);
        }
    }
}
