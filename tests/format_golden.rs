//! Golden tests for the project format in `omagari::format`.
//!
//! `tests/golden/` holds, for every bundled example (`examples/`) and every
//! frozen input (`tests/golden/inputs/`):
//! - `<name>.hanabi.ron`: the export, built with `produce()`;
//! - `<name>.resaved.omagari.ron`: the project as the editor saves it.
//!
//! They were captured with the editor before the data model moved into the
//! library, so any change in produced effects or in the `*.omagari.ron` format
//! shows up here. Run with `OMAGARI_UPDATE_GOLDEN=1` to rewrite them after an
//! intended change.

use std::fs;
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use omagari::format::OmagariProject;
use omagari::{register_missing_hanabi_modifiers, ExportedProject};

const ROOT: &str = env!("CARGO_MANIFEST_DIR");

/// (name, path) of every project with goldens.
fn inputs() -> Vec<(String, PathBuf)> {
    let mut inputs = Vec::new();
    for dir in ["examples", "tests/golden/inputs"] {
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
