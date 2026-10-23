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

use omagari::format::{AssetKind, PreviewMotion, ProjectV2, BUILTIN_TEXTURES_PREFIX, DEFAULT_MOTION_SPEED};

use crate::controller::Folder;
use crate::settings::GameAssets;

pub mod analysis;

/// The guide served by the `guide` tool.
pub const GUIDE: &str = include_str!("../../docs/CLAUDE_VFX_GUIDE.md");

/// File the running editor polls for preview requests.
pub const PREVIEW_REQUEST_FILE: &str = ".preview_request";

/// Stages `preview_in_editor` accepts.
pub const PREVIEW_STAGES: [&str; 3] = ["air", "floor", "object"];

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
        object.to_string()
    }

    /// Reads the JSON form, or the older two-line form.
    pub fn parse(text: &str) -> Result<Self, String> {
        let trimmed = text.trim_start();
        if !trimmed.starts_with('{') {
            let mut lines = text.lines();
            let name = lines.next().unwrap_or_default().trim().to_string();
            let stage = lines.next().map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
            return Ok(PreviewRequest { name, stage, motion: None });
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
        Ok(PreviewRequest { name, stage, motion })
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
        self.game_assets
            .folder()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("assets"))
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
            "description": "Omagari effect authoring guide: workflow, file format (v2), layers, attach (sockets, segments, fit, lifecycle), modifiers, expressions, textures, recipes, game use. Read before creating effects.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "section": {
                        "type": "string",
                        "description": "One section to keep the reply short: workflow, format, layers, attach, spawner, modifiers, expressions, attributes, render, textures, hierarchy, recipes, design, game. Omit for the whole guide."
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
                    "samples": { "type": "integer", "description": "Particles simulated per layer (20-2000, default 300)." }
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
            "name": "preview_in_editor",
            "description": "Ask the running Omagari editor to load a saved project and play it, so the USER can watch it. You get no image back, only whether the editor picked up the request.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": name_prop,
                    "stage": {
                        "type": "string",
                        "enum": PREVIEW_STAGES,
                        "description": "Scene to play it in: \"air\" (middle of an empty room), \"floor\" (floor at y = 0), \"object\" (a ~2-unit rock centered on the spawn point, floor under it). Omit to keep the user's current choice."
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
                    }
                },
                "required": ["name"]
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
            Ok(format!(
                "{}{}",
                notes.text(),
                analysis::analyze_project(&project, &assets_root, samples, str_arg("effect"))
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
                Some(other) => return Err(format!("unknown stage \"{}\": use air, floor or object", other)),
            };
            let motion = match args.get("motion") {
                None | Some(Value::Null) => None,
                Some(m) => Some(motion_from_json(m)?),
            };
            let preview = PreviewRequest {
                name: file_name.clone(),
                stage: stage.clone(),
                motion,
            };
            let request = Folder::SavedEffects.full_file_path(PREVIEW_REQUEST_FILE.to_string());
            fs::write(&request, preview.to_json()).map_err(|e| format!("{}: {}", request.display(), e))?;
            // The editor deletes the request once it has loaded the project.
            let start = Instant::now();
            while start.elapsed() < Duration::from_secs(3) {
                if !request.exists() {
                    let in_stage = stage.map(|s| format!(" in the {} stage", s)).unwrap_or_default();
                    let moving = motion.map(|m| format!(", the target {}", motion_text(m))).unwrap_or_default();
                    return Ok(format!(
                        "The editor loaded {} and is playing it for the user{}{}.",
                        file_name, in_stage, moving
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
        _ => Err(format!("unknown tool: {}", name)),
    }
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
        let recipes = GUIDE.split("## Recipes").nth(1).unwrap();
        recipes.split("```ron\n").nth(1).unwrap().split("```").next().unwrap().to_string()
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

    #[test]
    fn preview_requests_are_json_and_old_files_still_read() {
        let request = PreviewRequest {
            name: "fire.omagari.ron".into(),
            stage: Some("floor".into()),
            motion: Some(Some(PreviewMotion::Circle { radius: 2.0, speed: 3.6 })),
        };
        let text = request.to_json();
        assert!(text.starts_with('{'), "{}", text);
        assert_eq!(PreviewRequest::parse(&text).unwrap(), request);

        let still = PreviewRequest { motion: Some(None), stage: None, ..request.clone() };
        assert_eq!(PreviewRequest::parse(&still.to_json()).unwrap(), still);

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

    #[test]
    fn guide_sections_are_listed() {
        for section in ["workflow", "format", "layers", "attach", "textures", "recipes", "game"] {
            assert!(!guide_section(Some(section)).starts_with("No section"), "{}", section);
        }
    }
}
