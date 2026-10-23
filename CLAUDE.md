# Omagari

Bevy-Hanabi 3D particle effects editor (Bevy 0.19, bevy_hanabi 0.19). The editor is the `omagari` binary (`--features editor`); the library loads VFX packages (`*.vfx.ron` / `*.omagari.ron`, `src/runtime/`) and legacy exported `*.hanabi.ron` files (`src/lib.rs`) in games.

## Creating effects

Use the `omagari` MCP server (`omagari --mcp`, configured in `.mcp.json`). Call its `guide` tool with `section: "workflow"` before making an effect. You cannot see effects: describe them with `analyze_project` and let the user watch with `preview_in_editor`. Human docs: `docs/MCP.md`.

## Development

- Build: `cargo build --release --features editor`
- Test: `cargo test --release --features editor`
- `docs/CLAUDE_VFX_GUIDE.md` is compiled into the MCP server and its recipes are tested, so keep it in sync with `src/mcp/analysis.rs`.
- In MCP mode nothing but JSON-RPC may go to stdout; log to stderr.
- The effect data model and `produce()` live in the library (`src/format/`, serde only, no egui; v1 `OmagariProject` and v2 `ProjectV2` + migration); their egui widgets live in the editor (`src/editor/`).
- `tests/format_golden.rs` compares exports and re-saved projects with `tests/golden/`; after an intended format/produce change, rerun it with `OMAGARI_UPDATE_GOLDEN=1` and review the diff. v1 golden inputs live in `tests/golden/inputs/`; `examples/` must equal `tests/golden/v2/`.
- The game assets folder comes from `--game-assets <dir>` > `OMAGARI_GAME_ASSETS` > `omagari_settings.ron` (repo root, git-ignored) > the repo's `assets/`. It is registered as the `game` asset source at startup, so changes need a restart.
- The VFX roadmap is `docs/VFX_SYSTEM_PLAN.md`.
