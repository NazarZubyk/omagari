//! The packages of the `game_integration` example (`assets/vfx/*.vfx.ron`)
//! are exports of projects in `saved_effects/` or `examples/`: re-export
//! them after changing those projects (editor EXPORT, or the MCP
//! `export_project` with `copy_to: "assets/vfx/"`).

use std::fs;
use std::path::Path;

use omagari::format::ProjectV2;

const ROOT: &str = env!("CARGO_MANIFEST_DIR");

#[test]
fn example_packages_are_exports_of_their_projects() {
    let root = Path::new(ROOT);
    let mut checked = 0;
    for entry in fs::read_dir(root.join("assets/vfx")).unwrap() {
        let path = entry.unwrap().path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()).and_then(|n| n.strip_suffix(".vfx.ron")) else {
            continue;
        };
        let project_file = format!("{}.omagari.ron", name);
        let source = ["saved_effects", "examples"]
            .iter()
            .map(|dir| root.join(dir).join(&project_file))
            .find(|p| p.is_file())
            .unwrap_or_else(|| panic!("no project {} in saved_effects/ or examples/", project_file));
        let project = ProjectV2::from_ron_str(&fs::read_to_string(&source).unwrap()).unwrap();
        let expected = project.to_package().to_ron_string().unwrap();
        let actual = fs::read_to_string(&path).unwrap().replace("\r\n", "\n");
        assert!(
            actual == expected,
            "{} is not the export of {}; export it again",
            path.display(),
            source.display()
        );
        checked += 1;
    }
    assert!(checked >= 6, "only {} packages found", checked);
}
