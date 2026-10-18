# Making effects with Claude (MCP server)

Omagari includes an [MCP](https://modelcontextprotocol.io) server, so Claude Code (or any MCP client) can create, check, tune and export particle effects for you.

**Claude never sees the effect, and no images are sent to it.** That keeps token use low. Instead:

- Claude writes the project file and the server **checks it**: RON syntax, structure (parents, child indices, modifier lists), expression types, bevy_hanabi's own shader generation, and an export round trip. Mistakes that would otherwise show up as a blank viewport or a GPU error come back as text.
- The server **simulates the effect on the CPU** and describes it in a few lines per effect: spawn pattern, particle counts vs capacity, lifetimes, how far and in which direction particles travel, size in world units, colors and when they are visible, and an alive-over-time timeline.
- **You** watch the real effect in the editor. Claude asks the running editor to load and play a project (`preview_in_editor`), and you describe what to change ("too fast", "more orange", "spread wider").

```
Claude ──MCP (stdio)──▶ omagari --mcp ──writes──▶ saved_effects/*.omagari.ron ──▶ exported_effects/*.hanabi.ron
                              │
                              └─ saved_effects/.preview_request ──▶ running editor loads + plays it ──▶ you watch
```

## Setup

1. Build once, so the server starts quickly:

   ```bash
   cargo build --release --features editor
   ```

2. **Working in this repository:** `.mcp.json` already registers the server as `omagari`. Start Claude Code in the repository and approve the `omagari` server when asked. In the `claude` CLI, `/mcp` shows whether it is connected.

   **Working in another repository (for example your game):** register the built binary and point `--root` at this repository, so projects still go to `saved_effects/` and `exported_effects/` here:

   ```bash
   claude mcp add omagari -- C:/bevy/omagari/omagari/target/release/omagari.exe --mcp --root C:/bevy/omagari/omagari
   ```

   Then ask Claude to copy exports into your game, using `export_project`'s `copy_to` argument.

3. To watch effects, start the editor next to Claude and leave it open:

   ```bash
   cargo run --release --features editor
   ```

   The editor picks up preview requests every quarter second. If it isn't running, a request waits and loads the next time the editor starts.

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

Every effect in a project is spawned at the same point, so layered effects (a flash, sparks, smoke) go into one project as separate root effects.

**Unsaved edits:** a preview replaces what the editor has open. Save your own work in the editor before asking Claude to preview.

## What Claude gets instead of a picture

This is the analysis of the spark recipe from the guide:

```
OK: no errors.

## [0] "sparks" — root, texture Spark1, capacity 256
  spawn: burst of 150 at t=0, once
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
| `guide` | The authoring guide ([`CLAUDE_VFX_GUIDE.md`](CLAUDE_VFX_GUIDE.md)), whole or one section: file format, spawner, modifiers, expressions, attributes, textures, parent/child effects, tested recipes, design tips. |
| `list_projects` | Saved projects, bundled examples and exported files. |
| `read_project` | Returns a project's RON (`fireball`, or `examples/fireworks`). |
| `write_project` | Checks and saves `saved_effects/<name>.omagari.ron`. Refuses to save a project with errors unless `force` is set. Returns the analysis. |
| `check_project` | Validation only. |
| `analyze_project` | Validation plus the CPU simulation and text description. `effect` limits it to one effect, `samples` sets the accuracy. |
| `export_project` | Writes `exported_effects/<name>.hanabi.ron`, and with `copy_to` also a copy elsewhere. |
| `preview_in_editor` | Asks the running editor to load and play a saved project. Returns only whether the editor picked it up. |

## Token use

- The full guide is about 7k tokens. One section is 250–700 tokens, except the recipes (about 2.5k). Claude normally reads the workflow section and then the sections it needs.
- An analysis is roughly 300–400 tokens per effect.
- For small changes Claude edits the saved file directly and re-runs `check_project` or `analyze_project` by name, instead of resending the whole project.

## Limits

- The simulation is approximate: it assumes 60 updates per second, uses CPU random numbers, and `ConformToSphereModifier` is simplified. It is reliable for scale, timing, counts and colors, not for exact looks.
- The analysis can't judge taste. You still decide whether it looks good.
- Claude can only use what the editor supports: the modifiers, expressions and seven textures listed in the guide. The editor also always uses alpha blending, opacity from the texture's red channel, particles oriented along their velocity, and global simulation space.

## Troubleshooting

- **The `omagari` server fails to start or times out:** build it first (step 1). The first `cargo run` after source changes recompiles; on Windows that fails while the editor (the same `.exe`) is running, so close the editor, build, and restart both. You can also set `MCP_TIMEOUT=60000` for Claude Code.
- **"no editor picked it up within 3s":** the editor isn't running, or runs from a different folder. Start it from this repository with `cargo run --release --features editor`.
- **Textures are missing in the editor:** start it with `cargo run` from the repository root, not by launching `target/release/omagari.exe` directly, so Bevy finds `assets/`.
- **The effect is invisible in the game but fine in the editor:** the game must pass the particle textures to `EffectComplex::spawn` in the editor's order (see the guide's "Textures" section), and needs a camera with HDR and bloom for colors above 1.0 to glow.
