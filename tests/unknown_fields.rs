//! Unknown fields in project files are reported (with the path to the
//! field), and no bundled file has one: goldens (v1 inputs, v2), examples,
//! the demo projects in `saved_effects/`. The guide's recipes are checked by
//! `guide_recipes_pass_checks` (MCP), which rejects any parse warning.

use std::path::Path;

use omagari::format::ProjectV2;

fn project_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            project_files(&path, out);
        } else if path.to_string_lossy().ends_with(".omagari.ron") || path.to_string_lossy().ends_with(".vfx.ron") {
            out.push(path);
        }
    }
}

#[test]
fn bundled_projects_have_no_unknown_fields() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    for dir in ["tests/golden", "examples", "saved_effects"] {
        project_files(&root.join(dir), &mut files);
    }
    // The user's own scratch project (not part of the repository).
    files.retain(|f| !(f.ends_with("saved_effects/poison.omagari.ron") || f.ends_with("saved_effects\\poison.omagari.ron")));
    assert!(files.len() >= 20, "{:?}", files);
    for file in files {
        let text = std::fs::read_to_string(&file).unwrap();
        let (_, report) = ProjectV2::from_ron_str_with_report(&text).unwrap_or_else(|e| panic!("{}: {}", file.display(), e));
        assert!(report.unknown_fields.is_empty(), "{}: {:?}", file.display(), report.unknown_fields);
    }
}

#[test]
fn typos_are_reported_with_their_path() {
    // The typo that went unnoticed in a recipe: `accel` instead of
    // `accel_expr`.
    let text = r#"(
        version: 2,
        assets: [(id: "glow", kind: Texture((path: "omagari://textures/glow1.png", purpos: Sprite)))],
        layers: [
            Particles((
                name: "sparks",
                effect: (
                    capacity: 64,
                    update_modifiers: [AccelModifier((accel: Vec3((0.0, -9.8, 0.0))))],
                    render_modifiers: [Orient((mode: FaceCameraPosition, rotaton_expr: None))],
                ),
            )),
            Shape((name: "ring", shape: Ring(inner: 0.5, outer: 1.0), colour: (1.0, 1.0, 1.0, 1.0))),
        ],
        paramters: [],
    )"#;
    let (project, report) = ProjectV2::from_ron_str_with_report(text).unwrap();
    assert_eq!(project.layers.len(), 2, "the file still loads");
    assert!(report.notes.is_empty(), "{:?}", report.notes);
    let expected = [
        "unknown field \"purpos\" at assets[0].kind (asset \"glow\")",
        "unknown field \"accel\" at layers[0].effect.update_modifiers[0] (particles layer \"sparks\", AccelModifier)",
        "unknown field \"rotaton_expr\" at layers[0].effect.render_modifiers[0] (particles layer \"sparks\", Orient)",
        "unknown field \"colour\" at layers[1] (shape layer \"ring\")",
        "unknown field \"paramters\" at the top level",
    ];
    assert_eq!(report.unknown_fields.len(), expected.len(), "{:#?}", report.unknown_fields);
    for (got, want) in report.unknown_fields.iter().zip(expected) {
        assert!(got.starts_with(want), "{}\nwanted {}", got, want);
    }
    // The warnings list has them too (what the runtime loader logs).
    let (_, warnings) = ProjectV2::from_ron_str_with_warnings(text).unwrap();
    assert_eq!(warnings.len(), expected.len());

    // v1 files too.
    let v1 = r#"(effects: [(name: "a", capacity: 10, textur_index: Some(0))])"#;
    let (_, report) = ProjectV2::from_ron_str_with_report(v1).unwrap();
    assert_eq!(report.unknown_fields.len(), 1, "{:?}", report.unknown_fields);
    assert!(report.unknown_fields[0].starts_with("unknown field \"textur_index\" at effects[0] (effect \"a\")"), "{:?}", report.unknown_fields);
}
