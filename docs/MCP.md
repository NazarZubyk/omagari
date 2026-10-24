# Making effects with Claude (MCP server)

Omagari includes an [MCP](https://modelcontextprotocol.io) (Model Context Protocol) server, so Claude Code (or any MCP client) can create, check, tune and export effects for you: particles (also GPU ribbons and mesh particles), surface skins over your character, shapes, weapon trails, afterimages, lights and distortion (heat haze, shockwave ripples).

**Claude never sees the effect, and no images are sent to it.** That keeps token use low. Instead:

- Claude writes the project file and the server **checks it**: RON syntax, fields the format does not know (a typo such as `accel` for `accel_expr` comes back as a warning with its path, e.g. `layers[0].effect.update_modifiers[2]`), structure, assets and texture files, expression types, bevy_hanabi's own shader generation and a package round trip. Mistakes that would otherwise show up as a blank viewport or a GPU error come back as text.
- The server **simulates particle layers on the CPU** and describes every layer in a few lines: spawn pattern, counts, lifetimes, how far and where particles travel, sizes, colours over time, blending, simulation space, attachment, and how each layer changes across its parameters' ranges. Surface, shape, trail, afterimage, light and distortion layers are described from their values.
- **You** watch the real effect in the open editor. Claude asks the editor to load and play a project (`preview_in_editor`), and you tell it what to change ("too fast", "more orange", "spread wider").

```
Claude ──MCP (stdio)──▶ omagari --mcp ──writes──▶ saved_effects/*.omagari.ron ──export──▶ <game assets>/vfx/*.vfx.ron
                              │
                              └─ saved_effects/.preview_request ──▶ running editor loads + plays it ──▶ you watch
```

- [Setup](#setup)
- [Using it](#using-it)
- [What Claude gets instead of a picture](#what-claude-gets-instead-of-a-picture)
- [Tools](#tools)
- [What the checks cover](#what-the-checks-cover)
- [Token use](#token-use)
- [Limits](#limits)
- [Troubleshooting](#troubleshooting)

## Setup

### 1. Build once

The MCP server is the editor binary started with `--mcp`. Build it once so the server starts quickly:

| System | Command |
| :-- | :-- |
| Windows | `start.bat` (builds and opens the editor) |
| Linux / macOS | `./start.sh` (builds and opens the editor) |
| any | `cargo build --release --features editor` (build only) |

### 2. Register the server with Claude Code

**Working in the Omagari repository.** `.mcp.json` already registers the server:

```json
{
  "mcpServers": {
    "omagari": {
      "command": "cargo",
      "args": ["run", "--release", "--quiet", "--features", "editor", "--", "--mcp"]
    }
  }
}
```

Start Claude Code in the repository folder and approve the `omagari` server when asked. In the `claude` CLI, `/mcp` shows whether it is connected. `CLAUDE.md` tells Claude to read the server's guide before making an effect.

**Working in another repository (for example your game).** Register the built binary and point `--root` at your Omagari folder, so projects are still saved to its `saved_effects/` (where the editor finds them):

```text
claude mcp add omagari -- /path/to/omagari/target/release/omagari --mcp --root /path/to/omagari
```

On Windows the binary is `target\release\omagari.exe`. Paths are absolute.

### 3. Tell it where your game is

Exports go to the **game assets folder**'s `vfx/`, and textures and models are imported into it. The server uses the same setting as the editor:

1. `--game-assets <dir>` on the server's command line (add it after `--mcp`),
2. else the `OMAGARI_GAME_ASSETS` environment variable,
3. else `game_assets` in `omagari_settings.ron` in the Omagari folder (set it in the editor's ⚙ SETTINGS),
4. else no game folder: the repository's `assets/` folder is used, and exports go to `exported_effects/`.

`export_project` can also copy a package anywhere with `copy_to`.

### 4. Open the editor to watch

Start the editor (`start.bat`, `./start.sh` or `cargo run --release --features editor`) and leave it open next to Claude. You don't start the MCP server yourself; Claude Code starts it when needed.

The editor picks up preview requests every quarter of a second. If it isn't running, a request waits and loads the next time the editor starts. See [EDITOR.md](EDITOR.md) for the stages (AIR, FLOOR, OBJECT, CAPSULE, MODEL), the cameras and the tabs; while you edit by hand, the editor's problems counter shows the same checks Claude runs.

## Using it

Ask for an effect in plain words, with as much context as you have:

> Make a looping campfire effect for my game. Characters are about 1.8 m tall and the camera shows about 16 m of the scene vertically. Save it as `campfire`.

> Make a one-shot blue magic impact, about 1 m across, under a second long. Export it and copy it to `../my_game/assets/effects/`.

> Load `fireball` and make the sparks fall slower and last longer.

> Make a poison status skin for my character `characters/hero.glb` that flashes on a `tick` trigger, and show it on the model running with `characters/run.glb`.

Claude reads the guide, starts from a recipe or a bundled example, saves the project, reads the analysis, adjusts numbers, and then asks the editor to play it. Tell it what you see. The more specific the feedback, the fewer rounds it takes:

| Instead of | Say |
| :-- | :-- |
| "doesn't look right" | "the sparks fly too far, about twice as far as I want" |
| "make it better" | "it pops in; fade the start" / "it needs a bright flash at the start" |
| "too much" | "half as many particles, and smaller" |

All layers of a project play together on one target, so a layered effect (a flash, sparks, smoke) is one project with several layers.

**Unsaved edits:** a preview replaces what the editor has open. If the open project has unsaved changes, the editor asks first (Save / Don't save / Cancel).

**Your game.** Tell Claude the parameter and trigger names your game will use, or ask it which ones the effect declares. [GAME_INTEGRATION.md](GAME_INTEGRATION.md) shows the game side.

## What Claude gets instead of a picture

The analysis of the spark recipe from the guide:

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

A project `name` is a file in `saved_effects/` (`"fireball"` or `"fireball.omagari.ron"`), or `"examples/fireworks"` for a bundled example (read-only).

| Tool | Arguments | What it does |
| :-- | :-- | :-- |
| `guide` | `section` (optional) | The authoring guide ([CLAUDE_VFX_GUIDE.md](CLAUDE_VFX_GUIDE.md)), whole or one section: `workflow`, `format`, `layers`, `attach`, `model`, `parameters`, `triggers`, `spawner`, `modifiers`, `expressions`, `attributes`, `render`, `textures`, `assets`, `hierarchy`, `surfaces`, `shapes`, `trails`, `afterimages`, `lights`, `distortion`, `ribbons`, `meshes`, `order`, `recipes`, `design`, `game`. |
| `list_projects` | – | Saved projects, bundled examples, the game assets folder and exported packages. |
| `read_project` | `name` | A project's RON; version 1 projects come back migrated to version 2. |
| `write_project` | `name`, `ron`, `force` (optional) | Checks and saves `saved_effects/<name>.omagari.ron` (always version 2). Refuses a project with errors unless `force: true`. Returns the check and a short analysis. |
| `check_project` | `name` or `ron` | Validation only ([what it covers](#what-the-checks-cover)). |
| `analyze_project` | `name` or `ron`; `effect`, `samples`, `params` (optional) | The check plus the CPU simulation and description. `effect` limits it to one layer; `samples` (20–2000, default 300) sets the accuracy; `params` is a list of parameter sets to compare, e.g. `[{"intensity": 0.2}, {"intensity": 0.9, "tint": [1, 0.2, 0, 1]}]` (default: every number parameter at its min, middle and max). |
| `export_project` | `name`, `copy_to` (optional) | Writes the package `<name>.vfx.ron` to `<game assets>/vfx/` (or `exported_effects/`), and with `copy_to` also to another file or folder. |
| `import_asset` | `path`; `purpose`, `id`, `mesh`, `primitive`, `project` (optional) | Imports a texture (PNG, JPG; KTX2 is copied only) or a glTF model into the game assets folder like the editor: copied to `vfx/textures/` (models: `vfx/models/`) when outside it, used in place when inside. `purpose` (`Sprite`, `Noise`, `Mask`, `Gradient`) defaults to a guess from the file name. A model becomes a `Mesh` asset for mesh particles (`mesh` / `primitive` pick which). With `project`, the asset is added to that saved project. |
| `list_assets` | – | The built-in textures and every texture in the game assets folder (size, layout, which projects use them), and the glTF models with their node counts, height, clips and meshes. |
| `describe_model` | `path`; `max_nodes` (optional) | One model of the game assets folder, read on the CPU: node hierarchy (bones marked, rest positions), meshes and materials, mesh primitives (`MeshI/PrimitiveJ`, vertex counts, size), animation clips with lengths, rest-pose bounds and height. |
| `extract_channel` | `project`, `asset`, `channel` | Writes one channel (`R`, `G`, `B`, `A`, `Luminance`) of a project's texture as a grayscale PNG next to it and adds it to the project. Particle layers cannot pick a channel themselves. |
| `preview_in_editor` | `name`; `stage`, `camera`, `motion`, `params`, `trigger`, `model`, `animation`, `animation_file`, `attach_to`, `root_height` (optional) | Asks the running editor to load and play a saved project. `stage`: `air`, `floor`, `object`, `capsule`, `model`. `camera`: `orbit`, `game`. `motion`: `{"type": "circle", "radius": 3, "speed": 3.6}`, `{"type": "back_and_forth", "distance": 4, "speed": 3.6}` or `{"type": "none"}`. `params`: `{"intensity": 0.8}`. `trigger`: fired once it plays. The model arguments pick a model, clip, attach node and root height (checked against the files first). Returns only whether the editor picked it up. |

## What the checks cover

`check_project` (and `write_project`, `analyze_project`, the editor's problems counter) reports **errors** (the effect will not work), **warnings** (it probably does not do what you want) and **notes**.

- **Everything**: RON syntax; unknown fields (warnings with their path); structure (parents, child indices, modifier lists); asset ids; texture files (missing, outside the game assets folder, 16-bit grayscale PNGs, JPG needing Bevy's `jpeg` feature, sprite-sheet grids, settings that fight the purpose such as Noise with Clamp); unknown parameters and triggers in expressions, `rate`, `visible`, `play` and pulses; a colour parameter where a number is needed; `OnTrigger` / `Between` layers whose triggers do not exist; curves (keys and order). Parameters and triggers that nothing uses are notes.
- **Particles**: expressions bevy_hanabi 0.19 cannot compile (integer random values, `AlphaCutoff`, `Asin` / `Acos` of an out-of-range constant, `Uniform` / `Normal` of a `Param` without a `Cast`); modifiers in a list they cannot be in; `motion_integration: None` with velocities; render modifiers (flipbook grid, texture sampling and `sprite_index`, `Orient` rotation with `AlongVelocity`, random values at render time, screen-space sizes below 2 pixels); play mode against the spawner; ribbons and mesh assets.
- **Sockets and the preview model**: with a preview model, socket nodes, attach names, `attach_to`, the weapon node and surface / afterimage targets are checked against the model's nodes and meshes (errors), and the clip against its file. A missing model or animation file is a warning (the editor then shows the capsule). Without a model, socket names that are not package sockets are notes: they must exist in your game's model.
- **Surface**: texture slots, a `Blend` overlay so opaque it hides the model, shell offsets below 0 or above 0.1 m, dissolve without noise, empty target and shell lists.
- **Shape**: sizes of 0 or less, a ring's inner radius at or above its outer, shapes over ~20 m, an `OnTrigger` shape without a `duration`, age keys past the duration, fades longer than a one-shot, a fill along Y on a flat shape, an opaque `Blend` volume around the target.
- **Trail**: `from` = `to`, `length` ≤ 0, unknown emit triggers or parameters (errors); trails over 3 s, more than 16 subdivisions, unsorted or out-of-range width / fade keys, a fill along X / Y / Z instead of U / V (warnings).
- **Afterimage**: `interval` / `lifetime` ≤ 0, unknown emit triggers (errors); an interval shorter than a frame, a lifetime shorter than the interval, a `max_ghosts` that cuts copies before they fade (warnings).
- **Light**: negative intensity, range ≤ 0, a negative radius (errors); a light that never shines, under 200 lm or over 1 000 000 lm, a range over 25 m, a radius larger than the range, odd flicker values, `shadows: true`, an `OnTrigger` light without a duration, more than three lights in one package (warnings).
- **Distortion**: shape sizes ≤ 0, negative strength, unknown noise assets (errors); a strength that is always 0, under 0.001 or above 0.05 of the view height, no noise and no refraction, a noise texture with Clamp, values outside 0..1, a surface offset inside the model or far from it, a flat shape, an `OnTrigger` layer without a duration (warnings); a note when the package's own transparent layers will not be bent.

## Token use

- The full guide is about 35k tokens. One section is 300–1800 tokens, except the recipes (about 15k). Claude normally reads the workflow section and then the sections it needs.
- An analysis is roughly 350–450 tokens per layer.
- For small changes Claude edits the saved file directly and re-runs `check_project` or `analyze_project` by name, instead of resending the whole project.

## Limits

- The simulation is approximate: it assumes 60 updates per second, uses CPU random numbers, and `ConformToSphereModifier` is simplified. Every update modifier and every expression operator is simulated; render-only features (flipbooks, rounded corners, rotation, screen-pixel sizes) are described. It is reliable for scale, timing, counts and colours, not for exact looks.
- The check runs bevy_hanabi's own shader generation, but cannot compile shaders for the GPU (that happens in the editor and the game); the known bevy_hanabi 0.19 compile failures are reported as errors.
- The analysis can't judge taste. You still decide whether it looks good.
- Parameters and triggers: smoothing and pulses play out over time and are only described; you see them in the editor (PARAMETERS sliders and "animate", TRIGGERS FIRE and auto-fire).
- Trails and afterimages depend on how your game moves the sockets and the model, so they are described, not simulated.
- Lights, surfaces, shapes and distortions are described from their values. How bright a light looks depends on the scene; whether a distortion is visible depends on what is behind it (it bends only opaque things); how a skin reads on a particular model, z-fighting and bloom are for you to judge in the editor.
- The simulation assumes a still target; the trail a moving target leaves in `Global` space is estimated at 3.6 m/s (or the preview motion's speed). Sockets on model nodes are simulated at the target's origin plus their offset.
- Claude can only use what the editor supports: nearly all of bevy_hanabi's 3D modifiers, every expression operator and value type, ribbons, mesh particles, the seven layer kinds, the eight built-in textures and textures in your game assets folder. It never sees a texture; it gets its size, layout and purpose.

## Troubleshooting

- **The `omagari` server fails to start or times out:** build it first (step 1). The first `cargo run` after source changes recompiles; on Windows that fails while the editor (the same `.exe`) is running, so close the editor, run `start.bat`, and restart Claude's session. You can also set `MCP_TIMEOUT=60000` for Claude Code.
- **"no editor picked it up within 3s":** the editor isn't running, or runs from a different folder than the server's `--root`. Start it with `start.bat` / `./start.sh` from the Omagari folder.
- **Textures are missing in the editor:** start it with `start.bat`, `./start.sh` or `cargo run` from the repository folder. Launching the binary directly only finds `assets/` when `BEVY_ASSET_ROOT` points to the repository (the start scripts set it).
- **Exports go to the wrong place:** check the game assets folder (step 3); `list_projects` shows the one in effect.
- **The effect is invisible in the game but fine in the editor:** the game camera needs `Hdr` and `Bloom` for colours above 1.0 to glow, and textures other than the built-ins must exist at their path under the game's `assets/` folder. See [GAME_INTEGRATION.md](GAME_INTEGRATION.md#15-limits-and-troubleshooting).
