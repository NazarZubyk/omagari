#!/bin/sh
# Builds Omagari and opens the editor (Linux, macOS; start.bat on Windows).
#
# The same build is the MCP server: Claude Code starts it on its own
# (`omagari --mcp`, see .mcp.json), so you only need to open the editor
# here to watch the effects Claude makes.
#
# Extra arguments go to the editor, e.g. `./start.sh --game-assets ~/my_game/assets`.

set -e
cd "$(dirname "$0")"

echo "Building Omagari (editor + MCP server)..."
cargo build --release --features editor

# Running the binary directly (instead of `cargo run`) keeps cargo free for
# the MCP server; BEVY_ASSET_ROOT lets Bevy find the assets/ folder.
echo "Starting the editor..."
BEVY_ASSET_ROOT="$(pwd)" exec "${CARGO_TARGET_DIR:-target}/release/omagari" "$@"
