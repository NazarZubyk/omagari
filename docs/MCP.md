# Making effects with Claude (MCP server)

Omagari includes an [MCP](https://modelcontextprotocol.io) server, so Claude Code (or any MCP client) can create, check, tune and export particle effects, surface effects (a shader drawn over your character's model: glowing skins, poison patterns, frost, energy shells), shapes, weapon trails and afterimages for you.

**Claude never sees the effect, and no images are sent to it.** That keeps token use low. Instead:

- Claude writes the project file (format v2: texture assets + particle, surface, shape, trail and afterimage layers) and the server **checks it**: RON syntax, structure (parents, child indices, modifier lists), assets and texture files, expression types, bevy_hanabi's own shader generation, and a package round trip. Mistakes that would otherwise show up as a blank viewport or a GPU error come back as text.
- The server **simulates the effect on the CPU** and describes it in a few lines per layer: spawn pattern, particle counts vs capacity, lifetimes, how far and in which direction particles travel, size in world units, colors and when they are visible, blending, simulation space (with the trail a moving character would leave in Global space), where the layer is attached (target, socket, segment), the size fit, and an alive-over-time timeline. With parameters (values the game sets, like an intensity), it also simulates every number parameter at its min, middle and max and lists how each layer changes. Surface layers are not simulated but described: colour and opacity (at each parameter's min and max), noise (texture, channel, mapping, scroll speed on the body), rim, pulse, flash, dissolve, fill, shells and the thickness they give, fades.
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

   **Exports** go to the game assets folder's `vfx/` when one is configured (the editor's **⚙ SETTINGS** window, stored in `omagari_settings.ron`; or `--game-assets <dir>` / `OMAGARI_GAME_ASSETS`, which the MCP server reads too), else to `exported_effects/`; `export_project`'s `copy_to` copies them anywhere else.

3. To watch effects, open the editor with `start.bat` (or `cargo run --release --features editor`) and leave it open next to Claude. You don't start the MCP server yourself; Claude Code starts it when needed.

   The editor picks up preview requests every quarter second. If it isn't running, a request waits and loads the next time the editor starts.

   Effects play inside a big light gray cube (40 units wide) with a 1-unit grid on every face (darker lines every 5 units) and small axes at the spawn point (X red, Y green, Z blue), so you keep your orientation while rotating and can judge sizes. The toolbar and the **PREVIEW** tab change the scene:

   - **STAGE** sets where the effect plays: **AIR** (middle of the room, nothing nearby; you see everything, including particles that fall), **FLOOR** (the floor is right at the spawn point, like ground in a game; particles below it are hidden), **OBJECT** (a rock about 2 units wide sits on the spawn point, for shields, barriers and auras), **CAPSULE** (a 0.7 × 1.8 m capsule whose centre is the spawn point) and **MODEL** (one of your game's models, e.g. a Mixamo character running with a clip from another file, optionally holding a weapon; see the README's "Model preview"). Claude can also choose the stage, the model and the animation when it asks for a preview. The stage is saved in the project (its `preview` block).
   - **CAMERA**: **ORBIT** is the free camera; **GAME** shows the effect like the game's isometric camera (orthographic, 16 m visible height, following the target). Its values live in `omagari_settings.ron` and are edited in ⚙ SETTINGS.
   - **BG** cycles through a light room, a dark room and black. Glow is easier to judge on dark or black.
   - **Target motion** (PREVIEW tab) moves the effect's target: off, in a circle (radius) or back and forth (distance), at a speed in m/s (3.6 = a running character). `Local` layers stay on the moving target, `Global` layers leave a trail. The setting is saved in the project (its `preview` block); Claude can also set it when it asks for a preview.
   - The **⚓ SOCKETS & FIT** tab edits the project's sockets (named points on the target: a node/bone name of the game's model, or none = the target's origin, plus an offset) and its size fit (none, target scale, or target height for a reference height). A layer's **Attach** setting picks the target, a socket, or a segment between two sockets. On the MODEL stage the sockets show as labelled dots on the model (segments as lines); drag a dot, type the offset or nudge it. The **🌲 HIERARCHY** tab lists the model's nodes, bones, meshes and materials, and turns a node into the preview's attach point, a socket or a segment end.

   While you edit by hand, the preview rebuilds a moment after each change (**Live** in the bottom bar), Ctrl+Z / Ctrl+Y undo and redo, and the layer list and the Problems list show the same checks Claude runs (`check_project`).

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

**Unsaved edits:** a preview replaces what the editor has open. If the open project has unsaved changes, the editor asks first (Save / Don't save / Cancel). The editor's Problems list and the layer badges show the same checks as `check_project`, updated as you edit.

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
| `guide` | The authoring guide ([`CLAUDE_VFX_GUIDE.md`](CLAUDE_VFX_GUIDE.md)), whole or one section: file format, layers and simulation space, attach (sockets, segments, size fit, lifecycle), model preview (models, animations, weapons), parameters, triggers, spawner, modifiers, expressions, attributes, render settings, textures, assets (import, purposes, channels), parent/child layers, surfaces (surface layers: effect shader over the model, shells), shapes (shape layers: spheres, rings, arcs, ... with the same shader; follow modes, one-shots, curves over age), trails (a ribbon between two moving sockets: sword swings, comet tails; emission by start / stop triggers), afterimages (fading copies of the target's meshes), tested recipes (incl. "Status skin", "Energy shell", "Frost from the feet up", "Shield bubble", "Ground ring", "Shockwave", "Slash arc", "Sword trail", "Energy trail on a moving target", "Afterimages"), design tips, game use. |
| `list_projects` | Saved projects, bundled examples, the game assets folder and exported packages. |
| `read_project` | Returns a project's RON (`fireball`, or `examples/fireworks`); v1 projects come back migrated to v2. |
| `write_project` | Checks and saves `saved_effects/<name>.omagari.ron` (v1 input is migrated, everything is saved as v2). Refuses to save a project with errors unless `force` is set. Returns the analysis. |
| `check_project` | Validation only (v1 or v2 input). Also checks textures (missing files, paths outside the game assets folder, sprite-sheet grids, settings that fight the purpose such as Noise with Clamp, 16-bit grayscale PNGs, JPG needing Bevy's `jpeg` feature), sockets, attach (socket names that are not package sockets are listed as NOTES: they must exist as node names in the game's model), segment property use, fit, render modifiers (flipbook grid, texture sampling and `sprite_index`; Orient rotation with AlongVelocity; random values at render time, which bevy_hanabi gives every particle alike; ScreenSpaceSize sizes below 2 pixels), expressions bevy_hanabi 0.19 cannot compile (integer random values, `AlphaCutoff`, `Asin`/`Acos` of an out-of-range constant, `Uniform`/`Normal` of a `Param` without a `Cast`), modifiers in a list they cannot be in, `motion_integration: None` with velocities, and parameters and triggers: unknown names in expressions, `rate`, `visible`, `play` and pulses, a colour parameter where a number is needed, pulses on colour parameters, `OnTrigger` layers whose trigger does not exist or never stops, preview values; parameters and triggers nothing uses are NOTES. With a preview model (`preview.model`), socket nodes, attach names, `attach_to` and the weapon node are checked against the model's nodes (errors), and the preview clip against its file (a missing clip is an error, nodes the model lacks a warning); without one, socket names that are not package sockets stay NOTES. Surface layers: mesh / material targets against the preview model (errors; NOTES without a model), texture slots (missing assets are errors, an empty slot with a noise strength above 0 a warning), parameter reads and curves, a `Blend` overlay so opaque (≥ 0.95, at the defaults or any parameter's min / max) that it hides the model (warning), shell offsets below 0 or above 0.1 m, dissolve without noise, empty target lists and shell lists. Shape layers: sizes of 0 or less and a ring's inner radius at or above its outer (errors), shapes over ~20 m across at their largest scale, an `OnTrigger` shape whose trigger does not exist (error) or that has no `duration` (it would stay after the first trigger), curves over age with keys past the duration (never reached), fades longer than a one-shot, a fill along Y on a flat shape, an opaque `Blend` sphere / capsule / cylinder around the target, socket names like particle layers, and the same material checks as surfaces. Trail layers: `from` / `to` sockets (against the preview model like attach names), `from` = `to` (no width), `length` ≤ 0, unknown emit triggers or `While` parameters (errors); trails over 3 s, more than 16 subdivisions, unsorted or out-of-range `width_over_u` / `fade_over_u` keys, a fill along X / Y / Z instead of U / V (warnings). Afterimage layers: `interval` / `lifetime` ≤ 0 and unknown emit triggers (errors), source meshes against the preview model (errors; NOTES without a model), an interval shorter than a frame, a lifetime shorter than the interval, a `max_ghosts` that cuts copies before they fade, age keys past the lifetime (warnings). Curves over the layer's age (`Age([...])`, any binding) are checked for keys and order. |
| `analyze_project` | Validation plus the CPU simulation and text description. `effect` limits it to one layer, `samples` sets the accuracy. The main report uses the parameters' defaults (an `OnTrigger` layer is simulated as fired once at the start); then each layer is simulated at parameter sets and compared (count, size, colour, reach): by default every number parameter at its min, middle and max, or the sets in `params` (`[{"intensity": 0.2}, {"intensity": 0.9, "tint": [1, 0.2, 0, 1]}]`). Surface and shape layers are described in words instead of simulated: a shape's size, placement, follow mode, play mode and its size / opacity / fill over one play (one-shots) or over its age, and at each parameter set; a trail's sockets, width (their distance when both are offsets from the same node or the target root), length in time and the arc a 10 m/s blade tip draws, emission (which triggers start and stop it), width and opacity along it; an afterimage layer's copies on screen (lifetime / interval, capped). Triggers list which layers they burst, start or stop. |
| `export_project` | Writes the package `<name>.vfx.ron` to `<game assets>/vfx/` (or `exported_effects/`), and with `copy_to` also a copy elsewhere. |
| `import_asset` | Imports a texture file (PNG, JPG; KTX2 copied only) into the game assets folder like the editor: copied into `vfx/textures/` when outside it (name collision → `_2`, identical file → reused), used in place when inside. `purpose` defaults to a guess from the file name; with `project` the asset is added to that saved project. Reports size and layout (16-bit grayscale PNGs and JPG are flagged). |
| `list_assets` | The built-in textures and every texture in the game assets folder, with size, layout and which saved projects use them; and the glTF models (`.glb` / `.gltf`) of the game assets folder with their node counts, height and animation clips. |
| `describe_model` | One model of the game assets folder, read on the CPU (nothing is rendered): its node hierarchy with bones marked and rest positions (the names sockets, `attach_to` and weapon nodes use), mesh entities and materials (surface layer targets), animation clips with their lengths, and the rest-pose bounds and height. |
| `extract_channel` | Writes one channel (`R`, `G`, `B`, `A`, `Luminance`) of a project's texture as an 8-bit grayscale PNG next to it and adds it to the project with the same settings. Needed because particle layers cannot pick a channel. |
| `preview_in_editor` | Asks the running editor to load and play a saved project, optionally in a given stage (`air`, `floor`, `object`, `capsule`, `model`), on a `model` of the game assets folder with an `animation` clip (from `animation_file` when the clip is in another file with the same skeleton), attached to a node (`attach_to`) with the target `root_height` above the feet (checked against the files first), with a `camera` (`orbit`, `game`), with a target `motion` (`{"type": "circle", "radius": 3, "speed": 3.6}`, `{"type": "back_and_forth", "distance": 4, "speed": 3.6}` or `{"type": "none"}`), with parameter values (`params: {"intensity": 0.8}`; they become the project's preview values, as if the game had set them) and a `trigger` to fire once it plays. The request is a small JSON file (`saved_effects/.preview_request`); the old two-line form still works. Returns only whether the editor picked it up. |

## Token use

- The full guide is about 29k tokens. One section is 300–1700 tokens, except the recipes (about 12k). Claude normally reads the workflow section and then the sections it needs.
- An analysis is roughly 350–450 tokens per layer.
- For small changes Claude edits the saved file directly and re-runs `check_project` or `analyze_project` by name, instead of resending the whole project.

## Limits

- The simulation is approximate: it assumes 60 updates per second, uses CPU random numbers, and `ConformToSphereModifier` is simplified. Every update modifier (forces, `RadialAccel`, `TangentAccel`, kill volumes, spawn events, setters running every frame) and every expression operator is simulated; render-only features (flipbooks, rounded corners, rotation, screen-pixel sizes) are described, and SetColor / SetSize enter the colour and size lines. It is reliable for scale, timing, counts and colors, not for exact looks.
- The check runs bevy_hanabi's own shader generation, but it cannot compile the shaders for the GPU (that happens in the editor and the game); the known bevy_hanabi 0.19 compile failures are reported as errors.
- The analysis can't judge taste. You still decide whether it looks good.
- Parameters and triggers: the analysis compares parameter sets, but smoothing and pulses play out over time and are only described; the user sees them in the editor (PARAMETERS sliders and "animate", TRIGGERS FIRE and auto-fire).
- Trails and afterimages depend on how the game moves the sockets and the model, so they are described, not simulated; a trail's metres are the blade speed × its `length`. The user watches the swing in the editor (MODEL stage with a sword clip: the trail / afterimage inspector's Swing preview fires the start / stop triggers at two clip times; PREVIEW → In place keeps the character on the target).
- Shape layers are described from their values too: the analysis gives sizes, timings and opacity over the age, not how a shape overlaps the model or sorts against particles (`sort_bias`); the user judges that in the editor. A particle `rate` over age is simulated at its age-0 value.
- Surface layers are described from their values, not rendered: the check cannot see z-fighting, how a pattern reads on a particular model, or how bloom looks; the user judges those in the editor (MODEL stage with the game's character, GAME camera).
- Claude can only use what the editor supports: nearly all of bevy_hanabi's 3D modifiers, every expression operator and value type, render settings, surface, shape, trail and afterimage layers (one effect shader with noise, rim, pulse, flash, dissolve, fill and mask; light and distortion layers come later), the eight built-in textures (seven particle sprites and a tileable noise) and textures in the game assets folder (`import_asset` brings files in, `list_assets` lists them). Claude never sees a texture; it gets its size, layout and purpose. The simulation assumes a still target; the trail a moving target leaves in Global space is estimated at 3.6 m/s (or the preview motion's speed). Sockets on model nodes are simulated at the target's origin (plus their offset); the editor shows them on the model (MODEL stage), and `describe_model` gives the nodes' rest positions.

## Troubleshooting

- **The `omagari` server fails to start or times out:** build it first (step 1). The first `cargo run` after source changes recompiles; on Windows that fails while the editor (the same `.exe`) is running, so close the editor, run `start.bat`, and restart Claude's session. You can also set `MCP_TIMEOUT=60000` for Claude Code.
- **"no editor picked it up within 3s":** the editor isn't running, or runs from a different folder. Start it with `start.bat`.
- **Textures are missing in the editor:** start it with `start.bat` or `cargo run` from the repository root. Launching `target/release/omagari.exe` directly only finds `assets/` if `BEVY_ASSET_ROOT` points to the repository, which `start.bat` sets.
- **The effect is invisible in the game but fine in the editor:** the game needs a camera with HDR and bloom for colors above 1.0 to glow, and textures other than the built-ins must exist at their path under the game's `assets/` folder (`check_project` warns when they are missing from the configured game assets folder). Legacy `*.hanabi.ron` files need the textures passed to `EffectComplex::spawn` in the editor's old order.
