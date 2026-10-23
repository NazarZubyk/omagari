//! Golden tests for the project format in `omagari::format`.
//!
//! `tests/golden/` holds, for every frozen v1 input (`tests/golden/inputs/`:
//! v1 copies of the bundled examples plus two saved projects):
//! - `<name>.hanabi.ron`: the export, built with `produce()`;
//! - `<name>.resaved.omagari.ron`: the project as the editor saves it.
//!
//! They were captured with the editor before the data model moved into the
//! library, so any change in produced effects or in the `*.omagari.ron` format
//! shows up here. Run with `OMAGARI_UPDATE_GOLDEN=1` to rewrite them after an
//! intended change.
//!
//! The bundled examples (`examples/`) are the migrated v2 goldens
//! (`tests/golden/v2/<name>.omagari.ron`) of their v1 inputs.

use std::fs;
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use omagari::format::OmagariProject;
use omagari::{register_missing_hanabi_modifiers, ExportedProject};

const ROOT: &str = env!("CARGO_MANIFEST_DIR");

/// (name, path) of every project with goldens.
fn inputs() -> Vec<(String, PathBuf)> {
    let mut inputs = Vec::new();
    for dir in ["tests/golden/inputs"] {
        for entry in fs::read_dir(Path::new(ROOT).join(dir)).unwrap() {
            let path = entry.unwrap().path();
            let file = path.file_name().unwrap().to_string_lossy().to_string();
            if let Some(name) = file.strip_suffix(".omagari.ron") {
                inputs.push((name.to_string(), path));
            }
        }
    }
    inputs.sort();
    assert!(inputs.len() >= 6, "only {} golden inputs found", inputs.len());
    inputs
}

fn golden_path(file: &str) -> PathBuf {
    Path::new(ROOT).join("tests/golden").join(file)
}

/// Reads a text file; line endings are normalized in case git converted them.
fn read(path: &Path) -> String {
    fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("{}: {}", path.display(), e))
        .replace("\r\n", "\n")
}

fn load(path: &Path) -> OmagariProject {
    OmagariProject::from_ron_str(&read(path)).unwrap_or_else(|e| panic!("{}: {}", path.display(), e))
}

/// Compares `actual` with a golden file, reporting the first differing line.
fn assert_golden(actual: &str, file: &str) {
    let path = golden_path(file);
    if std::env::var_os("OMAGARI_UPDATE_GOLDEN").is_some() {
        fs::write(&path, actual).unwrap();
        return;
    }
    let expected = read(&path);
    if actual != expected {
        let line = actual
            .lines()
            .zip(expected.lines())
            .position(|(a, e)| a != e)
            .unwrap_or_else(|| actual.lines().count().min(expected.lines().count()));
        panic!(
            "{} differs at line {}:\n  actual:   {:?}\n  expected: {:?}",
            path.display(),
            line + 1,
            actual.lines().nth(line),
            expected.lines().nth(line),
        );
    }
}

fn type_registry() -> AppTypeRegistry {
    let registry = AppTypeRegistry::new_with_derived_types();
    bevy_hanabi::register_modifiers(&registry);
    register_missing_hanabi_modifiers(&registry);
    registry
}

/// Every project builds the same bevy_hanabi effects as before, and the
/// export loads back.
#[test]
fn projects_export_like_golden() {
    let app_registry = type_registry();
    let registry = app_registry.read();
    for (name, path) in inputs() {
        let project = load(&path);
        let exported = project.to_exported().to_ron_string(&registry).unwrap();
        assert_golden(&exported, &format!("{}.hanabi.ron", name));

        let reloaded = ExportedProject::from_ron_bytes(exported.as_bytes(), &registry)
            .unwrap_or_else(|e| panic!("{}: {}", path.display(), e));
        assert_eq!(reloaded.effects.len(), project.effects.len(), "{}", path.display());
    }
}

/// Every project saves exactly as before, and load → save → load → save gives
/// the same text.
#[test]
fn projects_round_trip() {
    for (name, path) in inputs() {
        let saved = load(&path).to_ron_string().unwrap();
        assert_golden(&saved, &format!("{}.resaved.omagari.ron", name));

        let reloaded = OmagariProject::from_ron_str(&saved)
            .unwrap_or_else(|e| panic!("{} after saving: {}", path.display(), e));
        assert_eq!(reloaded.to_ron_string().unwrap(), saved, "{}", path.display());
    }
}

/// No golden file is left without an input.
#[test]
fn goldens_have_inputs() {
    let names: Vec<String> = inputs().into_iter().map(|(name, _)| name).collect();
    for entry in fs::read_dir(golden_path("")).unwrap() {
        let file = entry.unwrap().file_name().to_string_lossy().to_string();
        let name = file
            .strip_suffix(".hanabi.ron")
            .or_else(|| file.strip_suffix(".resaved.omagari.ron"));
        if let Some(name) = name {
            assert!(names.iter().any(|n| n == name), "tests/golden/{} has no input project", file);
        }
    }
}

// --- project format v2 ---------------------------------------------------------

use omagari::format::{builtin_texture_path, ProjectV2, PARTICLE_TEXTURES};
use omagari::ExportedEffect;

fn load_v2(path: &Path) -> (ProjectV2, Vec<String>) {
    ProjectV2::from_ron_str_with_warnings(&read(path))
        .unwrap_or_else(|e| panic!("{}: {}", path.display(), e))
}

/// Builds the `*.hanabi.ron` export of a v2 project whose textures are all
/// built-ins, so it can be compared with the v1 goldens.
fn export_v2(project: &ProjectV2) -> ExportedProject {
    let mut exported = ExportedProject::default();
    for (_, layer) in project.particle_layers() {
        let texture_index = layer.texture.as_ref().map(|id| {
            let path = &project.texture(id).expect("texture asset").path;
            PARTICLE_TEXTURES
                .iter()
                .position(|t| builtin_texture_path(t.filename) == *path)
                .unwrap_or_else(|| panic!("{} is not a built-in texture", path))
        });
        let mut warnings = Vec::new();
        exported.effects.push(ExportedEffect {
            name: layer.name.clone(),
            parent: layer.parent.clone(),
            texture_index,
            effect_asset: layer.produce(&project.parameters, &mut warnings),
        });
        assert!(warnings.is_empty(), "{}: {:?}", layer.name, warnings);
    }
    exported
}

/// v1 → migrate → produce builds exactly the same bevy_hanabi effects as the
/// v1 path (the export text equals the v1 golden byte for byte).
#[test]
fn migrated_projects_export_like_v1() {
    let app_registry = type_registry();
    let registry = app_registry.read();
    for (name, path) in inputs() {
        let (project, warnings) = load_v2(&path);
        assert!(warnings.is_empty(), "{}: {:?}", path.display(), warnings);
        assert_eq!(project.version, 2);
        assert_eq!(project.layers.len(), load(&path).effects.len());
        let exported = export_v2(&project).to_ron_string(&registry).unwrap();
        assert_eq!(
            exported,
            load(&path).to_exported().to_ron_string(&registry).unwrap(),
            "{}",
            path.display()
        );
        assert_golden(&exported, &format!("{}.hanabi.ron", name));
    }
}

/// Migrated projects save as in `tests/golden/v2/`, and load → save → load →
/// save gives the same text.
#[test]
fn migrated_projects_round_trip() {
    for (name, path) in inputs() {
        let saved = load_v2(&path).0.to_ron_string().unwrap();
        assert_golden(&saved, &format!("v2/{}.omagari.ron", name));

        let reloaded = ProjectV2::from_ron_str(&saved)
            .unwrap_or_else(|e| panic!("{} after saving: {}", path.display(), e));
        assert_eq!(reloaded.to_ron_string().unwrap(), saved, "{}", path.display());
    }
}

/// A hand-written v2 project with partial structs loads with defaults, saves
/// as its golden, round-trips and produces without warnings except the
/// unsupported attach.
#[test]
fn v2_project_round_trips() {
    let path = golden_path("v2/inputs/showcase.omagari.ron");
    let (project, warnings) = load_v2(&path);
    assert!(warnings.is_empty(), "{:?}", warnings);
    let saved = project.to_ron_string().unwrap();
    assert_golden(&saved, "v2/showcase.omagari.ron");
    let reloaded = ProjectV2::from_ron_str(&saved).unwrap();
    assert_eq!(reloaded.to_ron_string().unwrap(), saved);

    let package = project.to_package();
    assert!(package.preview.is_none());
    assert!(project.preview.is_some());
    assert!(!package.to_ron_string().unwrap().contains("preview"));

    for (_, layer) in project.particle_layers() {
        let mut warnings = Vec::new();
        layer.produce(&project.parameters, &mut warnings);
        assert!(warnings.is_empty(), "{}: {:?}", layer.name, warnings);
    }
}

/// Partial files load; unknown struct fields are ignored.
#[test]
fn v2_partial_and_unknown_fields_load() {
    let text = r#"(
        version: 2,
        added_in_a_newer_editor: 3,
        layers: [Particles((name: "a", glow: true, effect: (capacity: 8)))],
    )"#;
    let (project, warnings) = ProjectV2::from_ron_str_with_warnings(text).unwrap();
    assert!(warnings.is_empty(), "{:?}", warnings);
    let (_, layer) = project.particle_layers().next().unwrap();
    assert_eq!(layer.name, "a");
    assert_eq!(layer.effect.capacity, 8);
    assert_eq!(layer.effect.render_modifiers.len(), 2, "default texture + orient");

    let empty = ProjectV2::from_ron_str("()").unwrap();
    assert_eq!(empty.version, 2);
    assert!(empty.layers.is_empty());
}

/// Modifiers in a list that doesn't support them are reported, not dropped
/// silently.
#[test]
fn v2_wrong_list_modifier_warns() {
    let text = r#"(layers: [Particles((name: "a", effect: (
        init_modifiers: [AccelModifier((accel_expr: Vec3((0.0, -1.0, 0.0))))],
        update_modifiers: [InheritAttribute((attr: "position"))],
    )))])"#;
    let project = ProjectV2::from_ron_str(text).unwrap();
    let (_, layer) = project.particle_layers().next().unwrap();
    let mut warnings = Vec::new();
    let effect = layer.produce(&project.parameters, &mut warnings);
    assert_eq!(warnings.len(), 2, "{:?}", warnings);
    assert!(warnings[0].contains("AccelModifier cannot be used in the init list"));
    assert!(warnings[1].contains("InheritAttribute cannot be used in the update list"));
    assert_eq!(effect.init_modifiers().count(), 0);
    assert_eq!(effect.update_modifiers().count(), 0);
}

/// The bundled examples are v2 and identical to the migrated goldens of their
/// v1 inputs, so they produce the same effects as the v1 examples did.
#[test]
fn examples_are_migrated_goldens() {
    let mut checked = 0;
    for entry in fs::read_dir(Path::new(ROOT).join("examples")).unwrap() {
        let path = entry.unwrap().path();
        let file = path.file_name().unwrap().to_string_lossy().to_string();
        let Some(name) = file.strip_suffix(".omagari.ron") else {
            continue;
        };
        let text = read(&path);
        assert!(text.contains("version: 2"), "{} is not a v2 project", path.display());
        let (project, warnings) = load_v2(&path);
        assert!(warnings.is_empty(), "{}: {:?}", path.display(), warnings);
        assert_eq!(project.to_ron_string().unwrap(), text, "{} is not saved as the editor saves", path.display());
        assert_eq!(text, read(&golden_path(&format!("v2/{}.omagari.ron", name))), "{}", path.display());
        assert!(
            golden_path("inputs").join(&file).exists(),
            "{} has no v1 input in tests/golden/inputs/",
            path.display()
        );
        checked += 1;
    }
    assert!(checked >= 4, "only {} examples found", checked);
}
