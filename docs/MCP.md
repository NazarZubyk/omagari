# Making effects with Claude (MCP server)

Omagari includes an [MCP](https://modelcontextprotocol.io) server, so Claude Code (or any MCP client) can create, check, tune and export particle effects for you.

**Claude never sees the effect, and no images are sent to it.** That keeps token use low. Instead:

- Claude writes the project file (format v2: texture assets + particle layers) and the server **checks it**: RON syntax, structure (parents, child indices, modifier lists), assets and texture files, expression types, bevy_hanabi's own shader generation, and a package round trip. Mistakes that would otherwise show up as a blank viewport or a GPU error come back as text.
- The server **simulates the effect on the CPU** and describes it in a few lines per layer: spawn pattern, particle counts vs capacity, lifetimes, how far and in which direction particles travel, size in world units, colors and when they are visible, blending, simulation space (with the trail a moving character would leave in Global space), and an alive-over-time timeline.
- **You** watch the real effect in the editor. Claude asks the running editor to load and play a project (`preview_in_editor`), and you describe what to change ("too fast", "more orange", "spread wider").

```
Claude ──MCP (stdio)──▶ omagari --mcp ──writes──▶ saved_effects/*.omagari.ron ──export──▶ <game assets>/vfx/*.vfx.ron
                              │
                              └─ saved_effects/.preview_request ──▶ running editor loads + plays it ──▶ you watch
```

## Setup

1. Build once, so the server starts quickly. Double-click `start.bat` in the repository folder: it builds everything and opens the editor. From a terminal:

   ```bash
   start.bat
   ```

   or build without opening the editor:

   ```bash
   cargo build --release --features editor
   ```

2. **Working in this repository:** `.mcp.json` already registers the server as `omagari`. Start Claude Code in the repository and approve the `omagari` server when asked. In the `claude` CLI, `/mcp` shows whether it is connected.

   **Working in another repository (for example your game):** register the built binary and point `--root` at this repository, so projects still go to `saved_effects/` here:

   ```bash
   claude mcp add omagari -- C:/bevy/omagari/omagari/target/release/omagari.exe --mcp --root C:/bevy/omagari/omagari
   ```

   **Exports** go to the game assets folder's `vfx/` when one is configured (the editor's **⚙ GAME ASSETS** menu, stored in `omagari_settings.ron`; or `--game-assets <dir>` / `OMAGARI_GAME_ASSETS`, which the MCP server reads too), else to `exported_effects/`; `export_project`'s `copy_to` copies them anywhere else.

3. To watch effects, open the editor with `start.bat` (or `cargo run --release --features editor`) and leave it open next to Claude. You don't start the MCP server yourself; Claude Code starts it when needed.

   The editor picks up preview requests every quarter second. If it isn't running, a request waits and loads the next time the editor starts.

   Effects play inside a big light gray cube (40 units wide) with a 1-unit grid on every face (darker lines every 5 units) and small axes at the spawn point (X red, Y green, Z blue), so you keep your orientation while rotating and can judge sizes. Two toolbar buttons change the scene:

   - **STAGE** sets where the effect plays: **AIR** (middle of the room, nothing nearby; you see everything, including particles that fall), **FLOOR** (the floor is right at the spawn point, like ground in a game; particles below it are hidden) and **OBJECT** (a rock about 2 units wide sits on the spawn point, for shields, barriers and auras). Claude can also choose the stage when it asks for a preview.
   - **BG** cycles through a light room, a dark room and black. Glow is easier to judge on dark or black.

## Using it

Ask for an effect in plain words, with as much context as you have:

> Make a looping campfire effect for my game. Characters are about 1.8 units tall and the camera is about 8 units away. Save it as `campfire`.

> Make a one-shot blue magic impact, about 1 unit across, under a second long. Export it to `../my_game/assets/effects/`.

> Load `fireball` and make the sparks fall slower and last longer.

Claude reads the guide, starts from a recipe or a bundled example, saves the project, reads the analysis, adjusts numbers, and then asks the editor to play it. Tell it what you see. The more specific the feedback, the fewer rounds it takes:

| Instead of | Say |
| :-- | :-- |
| "doesn't look right" | "the sparks fly too far, about twice as far as I want" |
| "make it better" | "it pops in; fade the start" / "it needs a bright flash at the start" |
| "too much" | "half as many particles, and smaller" |

Every layer in a project plays at the same point, so layered effects (a flash, sparks, smoke) go into one project as separate root layers.

**Unsaved edits:** a preview replaces what the editor has open. Save your own work in the editor before asking Claude to preview.

## What Claude gets instead of a picture

This is the analysis of the spark recipe from the guide:

```
OK: no errors.

## [0] "sparks" — root, texture "spark1" (omagari://textures/spark1.png), capacity 256
  spawn: burst of 150 at t=0, once
  render: blend Blend; texture sampling ModulateOpacityFromR; orient AlongVelocity
  space: Local — particles move with the target
  count: ~150 particles in total, peak ~150 alive
  lifetime: 0.40–0.90s (mean 0.66)
  birth: around (0.000, -0.001, 0.003) within 0.052; initial speed 2.02–5.99 (mean 4.04), directions spread all around
  motion: 90% of positions in x[-1.14, 1.02] y[-2.52, 0.32] z[-1.00, 1.07]; farthest 4.60 from origin; ...
  size: 0.040 (fixed) world units
  color: ColorOverLifetime Overwrite: 0% orange×6 a1 → 40% orange×5 a1 → 100% red×2 a0
  visible: alpha > 0.05 from 0% to 96% of life; peak intensity 6, HDR >1 so it blooms
  alive over time: 0s:150 0.11s:150 0.22s:150 0.33s:150 0.44s:139 0.55s:109 0.66s:79 0.77s:43 0.88s:6
```

## Tools

| Tool | What it does |
| :-- | :-- |
| `guide` | The authoring guide ([`CLAUDE_VFX_GUIDE.md`](CLAUDE_VFX_GUIDE.md)), whole or one section: file format, layers and simulation space, spawner, modifiers, expressions, attributes, render settings, textures, parent/child layers, tested recipes, design tips, game use. |
| `list_projects` | Saved projects, bundled examples, the game assets folder and exported packages. |
| `read_project` | Returns a project's RON (`fireball`, or `examples/fireworks`); v1 projects come back migrated to v2. |
| `write_project` | Checks and saves `saved_effects/<name>.omagari.ron` (v1 input is migrated, everything is saved as v2). Refuses to save a project with errors unless `force` is set. Returns the analysis. |
| `check_project` | Validation only (v1 or v2 input). |
| `analyze_project` | Validation plus the CPU simulation and text description. `effect` limits it to one layer, `samples` sets the accuracy. |
| `export_project` | Writes the package `<name>.vfx.ron` to `<game assets>/vfx/` (or `exported_effects/`), and with `copy_to` also a copy elsewhere. |
| `preview_in_editor` | Asks the running editor to load and play a saved project, optionally in a given stage (`air`, `floor`, `object`). Returns only whether the editor picked it up. |

## Token use

- The full guide is about 9k tokens. One section is 250–900 tokens, except the recipes (about 3.5k). Claude normally reads the workflow section and then the sections it needs.
- An analysis is roughly 350–450 tokens per layer.
- For small changes Claude edits the saved file directly and re-runs `check_project` or `analyze_project` by name, instead of resending the whole project.

## Limits

- The simulation is approximate: it assumes 60 updates per second, uses CPU random numbers, and `ConformToSphereModifier` is simplified. It is reliable for scale, timing, counts and colors, not for exact looks.
- The analysis can't judge taste. You still decide whether it looks good.
- Claude can only use what the editor supports: the modifiers, expressions, render settings and the seven built-in textures listed in the guide (other textures can be referenced by path under the game assets folder, but importing them comes later). The simulation assumes a still target; the trail a moving target leaves in Global space is estimated at 3.6 m/s.

## Troubleshooting

- **The `omagari` server fails to start or times out:** build it first (step 1). The first `cargo run` after source changes recompiles; on Windows that fails while the editor (the same `.exe`) is running, so close the editor, run `start.bat`, and restart Claude's session. You can also set `MCP_TIMEOUT=60000` for Claude Code.
- **"no editor picked it up within 3s":** the editor isn't running, or runs from a different folder. Start it with `start.bat`.
- **Textures are missing in the editor:** start it with `start.bat` or `cargo run` from the repository root. Launching `target/release/omagari.exe` directly only finds `assets/` if `BEVY_ASSET_ROOT` points to the repository, which `start.bat` sets.
- **The effect is invisible in the game but fine in the editor:** the game needs a camera with HDR and bloom for colors above 1.0 to glow, and textures other than the built-ins must exist at their path under the game's `assets/` folder (`check_project` warns when they are missing from the configured game assets folder). Legacy `*.hanabi.ron` files need the textures passed to `EffectComplex::spawn` in the editor's old order.
