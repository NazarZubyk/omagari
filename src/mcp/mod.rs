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

use crate::controller::{Folder, OmagariProject};

pub mod analysis;

/// The guide served by the `guide` tool.
pub const GUIDE: &str = include_str!("../../docs/CLAUDE_VFX_GUIDE.md");

/// File the running editor polls for preview requests.
pub const PREVIEW_REQUEST_FILE: &str = ".preview_request";

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

    let registry = analysis::type_registry();
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
                    .filter_map(|m| handle_message(m, &registry))
                    .collect();
                if replies.is_empty() { None } else { Some(Value::Array(replies)) }
            }
            Ok(message) => handle_message(&message, &registry),
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

fn handle_message(message: &Value, registry: &bevy::prelude::AppTypeRegistry) -> Option<Value> {
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
                "instructions": "Omagari builds bevy_hanabi particle effects as *.omagari.ron projects. \
                    Call `guide` first (section \"workflow\" is enough to start). You cannot see the effect; \
                    use `analyze_project` for a text description and `preview_in_editor` to show it to the user."
            }))
        }
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tool_list() })),
        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            let (text, is_error) = match call_tool(name, &args, registry) {
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
            "description": "Omagari effect authoring guide: workflow, file format, modifiers, expressions, textures, recipes. Read before creating effects.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "section": {
                        "type": "string",
                        "description": "One section to keep the reply short: workflow, format, spawner, modifiers, expressions, attributes, render, textures, hierarchy, recipes, design. Omit for the whole guide."
                    }
                }
            }
        },
        {
            "name": "list_projects",
            "description": "List saved projects, bundled examples and exported files.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "read_project",
            "description": "Return the RON text of a project.",
            "inputSchema": {
                "type": "object",
                "properties": { "name": name_prop },
                "required": ["name"]
            }
        },
        {
            "name": "write_project",
            "description": "Check a project and save it to saved_effects/<name>.omagari.ron. Refuses to save when it has errors (unless force=true). Returns the check result and a short analysis.",
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
            "description": "Fast validation only: parse, structure, expression types, bevy_hanabi shader generation and export round trip. Pass `name` or `ron`.",
            "inputSchema": {
                "type": "object",
                "properties": { "name": name_prop, "ron": ron_prop }
            }
        },
        {
            "name": "analyze_project",
            "description": "Text 'preview' instead of images: checks the project, simulates particles on the CPU and describes each effect (spawn pattern, counts, lifetime, motion extents, size, colors, visibility, timeline). Pass `name` or `ron`.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": name_prop,
                    "ron": ron_prop,
                    "effect": { "type": "string", "description": "Only describe the effect with this name." },
                    "samples": { "type": "integer", "description": "Particles simulated per effect (20-2000, default 300)." }
                }
            }
        },
        {
            "name": "export_project",
            "description": "Export a saved project to exported_effects/<name>.hanabi.ron for use in a game, optionally copying it to another path too.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": name_prop,
                    "copy_to": { "type": "string", "description": "Optional extra destination file or folder, e.g. a game's assets/effects/ folder." }
                },
                "required": ["name"]
            }
        },
        {
            "name": "preview_in_editor",
            "description": "Ask the running Omagari editor to load a saved project and play it, so the USER can watch it. You get no image back, only whether the editor picked up the request.",
            "inputSchema": {
                "type": "object",
                "properties": { "name": name_prop },
                "required": ["name"]
            }
        }
    ])
}

fn call_tool(name: &str, args: &Value, registry: &bevy::prelude::AppTypeRegistry) -> Result<String, String> {
    let str_arg = |key: &str| args.get(key).and_then(Value::as_str);
    match name {
        "guide" => Ok(guide_section(str_arg("section"))),
        "list_projects" => Ok(list_projects()),
        "read_project" => {
            let path = resolve_project(str_arg("name").ok_or("missing `name`")?)?;
            fs::read_to_string(&path).map_err(|e| format!("{}: {}", path.display(), e))
        }
        "write_project" => {
            let name = str_arg("name").ok_or("missing `name`")?;
            let ron_text = str_arg("ron").ok_or("missing `ron`")?;
            let force = args.get("force").and_then(Value::as_bool).unwrap_or(false);
            let file_name = project_file_name(name)?;
            let project = parse_project(ron_text)?;
            let diagnostics = analysis::check_project(&project, registry);
            if !diagnostics.errors.is_empty() && !force {
                return Err(format!("NOT SAVED.\n{}", diagnostics.render()));
            }
            let path = Folder::SavedEffects.full_file_path(file_name.clone());
            fs::write(&path, ron_text).map_err(|e| format!("{}: {}", path.display(), e))?;
            Ok(format!(
                "Saved {}.\n{}",
                path.display(),
                analysis::analyze_project(&project, registry, 200, None)
            ))
        }
        "check_project" => {
            let (project, _) = load_from_args(args)?;
            let diagnostics = analysis::check_project(&project, registry);
            let text = diagnostics.render();
            if diagnostics.errors.is_empty() { Ok(text) } else { Err(text) }
        }
        "analyze_project" => {
            let (project, _) = load_from_args(args)?;
            let samples = args.get("samples").and_then(Value::as_u64).unwrap_or(300) as usize;
            Ok(analysis::analyze_project(&project, registry, samples, str_arg("effect")))
        }
        "export_project" => {
            let path = resolve_project(str_arg("name").ok_or("missing `name`")?)?;
            let project = parse_project(&read(&path)?)?;
            let diagnostics = analysis::check_project(&project, registry);
            if !diagnostics.errors.is_empty() {
                return Err(format!("NOT EXPORTED.\n{}", diagnostics.render()));
            }
            let text = analysis::export_string(&project, registry)?;
            let base = path
                .file_name()
                .and_then(|f| f.to_str())
                .and_then(|f| f.split('.').next())
                .unwrap_or("effect")
                .to_string();
            let file_name = format!("{}.hanabi.ron", base);
            let out = Folder::ExportedEffects.full_file_path(file_name.clone());
            fs::write(&out, &text).map_err(|e| format!("{}: {}", out.display(), e))?;
            let mut msg = format!("Exported {}", out.display());
            if let Some(copy_to) = str_arg("copy_to") {
                let mut dest = PathBuf::from(copy_to);
                if dest.is_dir() || copy_to.ends_with('/') || copy_to.ends_with('\\') {
                    fs::create_dir_all(&dest).map_err(|e| format!("{}: {}", dest.display(), e))?;
                    dest = dest.join(&file_name);
                }
                fs::write(&dest, &text).map_err(|e| format!("{}: {}", dest.display(), e))?;
                msg += &format!(" and copied it to {}", dest.display());
            }
            msg += &format!(
                ".\nTexture indices used: {}. The game must pass textures in the same order as the editor's PARTICLE_TEXTURES (see guide section \"textures\").",
                texture_list(&project)
            );
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
            let request = Folder::SavedEffects.full_file_path(PREVIEW_REQUEST_FILE.to_string());
            fs::write(&request, &file_name).map_err(|e| format!("{}: {}", request.display(), e))?;
            // The editor deletes the request once it has loaded the project.
            let start = Instant::now();
            while start.elapsed() < Duration::from_secs(3) {
                if !request.exists() {
                    return Ok(format!("The editor loaded {} and is playing it for the user.", file_name));
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

fn list_files(dir: &str, suffix: &str) -> Vec<String> {
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

fn list_projects() -> String {
    let cwd = std::env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    format!(
        "root: {}\nsaved_effects/: {}\nexamples/: {}\nexported_effects/: {}\n",
        cwd,
        list_files(Folder::SavedEffects.to_path(), ".omagari.ron").join(", "),
        list_files("examples", ".omagari.ron").join(", "),
        list_files(Folder::ExportedEffects.to_path(), ".hanabi.ron").join(", "),
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
        Err(format!("{} not found. Saved: {}", path.display(), list_files(Folder::SavedEffects.to_path(), ".omagari.ron").join(", ")))
    }
}

fn read(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|e| format!("{}: {}", path.display(), e))
}

fn parse_project(text: &str) -> Result<OmagariProject, String> {
    ron::from_str::<OmagariProject>(text).map_err(|e| format!("RON parse error: {}", e))
}

fn load_from_args(args: &Value) -> Result<(OmagariProject, String), String> {
    if let Some(text) = args.get("ron").and_then(Value::as_str) {
        return Ok((parse_project(text)?, text.to_string()));
    }
    let name = args
        .get("name")
        .and_then(Value::as_str)
        .ok_or("pass `name` or `ron`")?;
    let text = read(&resolve_project(name)?)?;
    Ok((parse_project(&text)?, text))
}

fn texture_list(project: &OmagariProject) -> String {
    let mut used: Vec<String> = project
        .effects
        .iter()
        .filter_map(|e| e.texture_index())
        .map(|t| {
            let file = crate::effect::PARTICLE_TEXTURES
                .get(t)
                .map(|t| t.filename)
                .unwrap_or("?");
            format!("{}={}", t, file)
        })
        .collect();
    used.sort();
    used.dedup();
    used.join(", ")
}
