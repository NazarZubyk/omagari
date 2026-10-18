@echo off
rem Builds Omagari and opens the editor.
rem
rem The same build is the MCP server: Claude Code starts it on its own
rem (`omagari --mcp`, see .mcp.json), so you only need to open the editor
rem here to watch the effects Claude makes.

cd /d "%~dp0"

echo Building Omagari (editor + MCP server)...
cargo build --release --features editor
if errorlevel 1 (
    echo.
    echo Build failed. If the editor is still open, close it and run start.bat again.
    pause
    exit /b 1
)

rem Launching the .exe directly (instead of `cargo run`) keeps cargo free for
rem the MCP server; BEVY_ASSET_ROOT lets Bevy find the assets/ folder.
set "BEVY_ASSET_ROOT=%~dp0."
echo Starting the editor...
start "Omagari" "%~dp0target\release\omagari.exe"
