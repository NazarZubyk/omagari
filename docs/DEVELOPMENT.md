# Working on Omagari

Notes for people changing Omagari itself. Users of the editor and the library start at [GETTING_STARTED.md](GETTING_STARTED.md).

## Layout

| Path | What it is |
| :-- | :-- |
| `src/lib.rs`, `src/runtime/` | The library: `OmagariPlugin`, VFX packages, instances, every layer kind's runtime, legacy `EffectComplex`. |
| `src/format/` | The data model (serde only, no egui): project format v2 (`ProjectV2`), v1 (`OmagariProject`) and its migration, `produce()` (bevy_hanabi effects from layers). |
| `src/editor/` | The egui editor (the `omagari` binary, `--features editor`). |
| `src/mcp/` | The MCP server (`omagari --mcp`): tools, checks, the CPU simulation. |
| `docs/CLAUDE_VFX_GUIDE.md` | The authoring guide, compiled into the MCP server; its recipes are tested. |
| `saved_effects/` | Demo projects. |
| `examples/` | Bundled particle examples (`*.omagari.ron`) and the cargo example `game_integration.rs`. |
| `assets/` | The built-in textures (embedded into the library) and the example's packages (`assets/vfx/`). |
| `tests/` | Integration tests and golden files. |

## Build and test

```text
cargo build --release --features editor         # the editor and MCP server
cargo build --release                           # the library alone, as games build it
cargo build --release --examples                # the game_integration example
cargo test --release --features editor          # every test
```

- If the editor or the MCP server is running from `target/release`, build into another folder (`CARGO_TARGET_DIR=target/dev`) on Windows, where a running `.exe` cannot be replaced.
- `tests/format_golden.rs` compares exports and re-saved projects with `tests/golden/`. After an intended format or `produce()` change, rerun it with `OMAGARI_UPDATE_GOLDEN=1` and review the diff. The bundled `examples/*.omagari.ron` must equal `tests/golden/v2/`.
- `tests/example_packages.rs` checks that `assets/vfx/*.vfx.ron` are exports of their projects in `saved_effects/` or `examples/`. After changing one of those projects, export it again (editor EXPORT and copy, or the MCP `export_project` with `copy_to: "assets/vfx/"`).
- `docs/CLAUDE_VFX_GUIDE.md` is compiled into the MCP server and its recipes are tested: keep it in sync with `src/mcp/analysis.rs`.
- In MCP mode nothing but JSON-RPC may go to stdout; log to stderr.

## Screenshots without looking at the screen

For agents (and scripts) checking the UI: the editor takes developer flags, then saves the window and quits:

```text
omagari --open shield_bubble --tab layer --camera game --stage capsule \
        --window-size 1600x900 --screenshot shot.png --screenshot-after 90
```

| Flag | Effect |
| :-- | :-- |
| `--open <project>` | opens `saved_effects/<project>.omagari.ron` at startup |
| `--camera orbit\|game` | the preview camera of the opened project |
| `--tab layer\|sockets\|preview\|parameters\|triggers\|assets\|hierarchy` | the right panel's tab |
| `--asset <id>` | shows that texture in the ASSETS tab |
| `--channel rgb\|r\|g\|b\|a` | the texture preview's channel view |
| `--import <file>` | imports a file at startup like a drop (repeatable) |
| `--screenshot <out.png>` | saves the window after `--screenshot-after <frames>` (default 90) and quits |
| `--window-size <w>x<h>` | window size in pixels (scale factor 1) |
| `--stage air\|floor\|object\|capsule\|model` | the stage of the opened project |
| `--model <glTF>` | the preview model (path in the game assets folder; sets the MODEL stage) |
| `--animation <clip>` / `--animation-file <glTF>` | the clip and the file it comes from |
| `--attach-to <node>` / `--root-height <m>` | where the instance is attached, the target's height above the feet |
| `--anim-time <s>` | pauses the animation at this clip time |
| `--anim-start <s>` | jumps the animation to this clip time and lets it play |
| `--freeze-at <s>` | when the playing animation passes this clip time, pauses the whole preview (animation, trails, particles) and takes the screenshot 8 frames later, e.g. a sword trail mid-swing |
| `--no-in-place` | keeps the clip's root motion (preview `in_place` off) |
| `--motion none\|circle\|back_and_forth` | the target motion |
| `--focus` | the orbit camera looks at the target |
| `--backdrop light\|dark\|black` | the backdrop |
| `--orbit <yaw>,<pitch>,<distance>` | the orbit camera's angles (degrees) and distance (m) |
| `--focus-node <name>` | the orbit camera looks at a node of the model (e.g. `mixamorig:Hips`) |
| `--screenshot-crop <x>,<y>,<w>,<h>` | saves only this part of the window (pixels) |
| `--stop-at <frame>` | stops the preview instance (STOP) at this frame, e.g. to check a fade out |
| `--fire <trigger>` | fires a trigger six frames before the screenshot (`--fire-before <frames>` changes it, e.g. 18 ≈ 0.3 s at 60 fps) |

Combine them with `--game-assets <temp dir>` so imports don't land in a real game folder. A model check, with a character of your game assets folder: `omagari --open model_demo --camera orbit --motion none --focus --anim-time 0.3 --tab sockets --screenshot run.png`.

The screenshots in `docs/images/` were taken this way on the CAPSULE and AIR stages, without third-party models.

## Roadmap

[VFX_SYSTEM_PLAN.md](VFX_SYSTEM_PLAN.md) is the design document and work order of the VFX system.
