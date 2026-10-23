# Omagari

Bevy-Hanabi 3D particle effects editor (Bevy 0.19, bevy_hanabi 0.19). The editor is the `omagari` binary (`--features editor`); the library (`src/lib.rs`) loads exported `*.hanabi.ron` files in games.

## Creating effects

Use the `omagari` MCP server (`omagari --mcp`, configured in `.mcp.json`). Call its `guide` tool with `section: "workflow"` before making an effect. You cannot see effects: describe them with `analyze_project` and let the user watch with `preview_in_editor`. Human docs: `docs/MCP.md`.

## Development

- Build: `cargo build --release --features editor`
- Test: `cargo test --release --features editor`
- `docs/CLAUDE_VFX_GUIDE.md` is compiled into the MCP server and its recipes are tested, so keep it in sync with `src/mcp/analysis.rs`.
- In MCP mode nothing but JSON-RPC may go to stdout; log to stderr.
- The effect data model and `produce()` live in the library (`src/format/`, serde only, no egui); their egui widgets live in the editor (`src/editor/`).
- `tests/format_golden.rs` compares exports and re-saved projects with `tests/golden/`; after an intended format/produce change, rerun it with `OMAGARI_UPDATE_GOLDEN=1` and review the diff.
