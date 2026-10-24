# Getting started

This page takes you from a fresh clone to an effect playing in your own game.

1. [Install](#1-install)
2. [Build and run the editor](#2-build-and-run-the-editor)
3. [Point Omagari at your game](#3-point-omagari-at-your-game)
4. [Open the demos](#4-open-the-demos)
5. [Make your first effect](#5-make-your-first-effect)
6. [Export it and play it in your game](#6-export-it-and-play-it-in-your-game)
7. [Next steps](#7-next-steps)

## 1. Install

- **Rust**: a recent stable toolchain from [rustup.rs](https://rustup.rs) (`rustup update` if you already have one). Omagari uses the 2024 edition.
- **A GPU** that runs Bevy 0.19 (Vulkan, Metal or DX12).
- **Windows**: nothing else.
- **Linux**: Bevy's system libraries (ALSA, udev, X11 / Wayland development packages); see Bevy's [Linux dependencies](https://github.com/bevyengine/bevy/blob/main/docs/linux_dependencies.md). The file dialog of the ASSETS tab uses the XDG desktop portal.
- **macOS**: Xcode's command line tools.

Omagari is developed and tested on Windows. Linux and macOS should work like any Bevy app, but they are not tested.

## 2. Build and run the editor

Clone the repository and start the editor **from the repository folder** (the editor reads `saved_effects/` and `assets/` from the folder it runs in):

| System | Command |
| :-- | :-- |
| Windows | double-click `start.bat`, or run it in a terminal |
| Linux / macOS | `./start.sh` |
| any | `cargo run --release --features editor` |

`start.bat` / `start.sh` build the editor (which is also the MCP server for Claude, see [MCP.md](MCP.md)) and start it. Extra arguments of `start.sh` go to the editor, for example `./start.sh --game-assets ~/my_game/assets`.

The first build takes a few minutes. The editor is the `editor` feature of the crate; without it, `cargo build` builds only the library that games use.

When the window opens you see the empty preview room: a light grey cube 40 m wide with a 1 m grid, the LAYERS list on the left, the inspector tabs on the right and the playback bar at the bottom. [EDITOR.md](EDITOR.md) is a tour of all of it.

![The editor with a surface layer on the capsule stage](images/editor_overview.png)

## 3. Point Omagari at your game

The **game assets folder** is your game's `assets/` folder. When it is set:

- EXPORT writes packages to `<game assets>/vfx/`;
- textures you import are copied to `<game assets>/vfx/textures/` (models to `vfx/models/`), and project paths such as `vfx/textures/smoke.png` load from there;
- the MODEL stage and the HIERARCHY tab can show your game's characters (see [MODELS.md](MODELS.md)).

Set it in **⚙ SETTINGS** (top right): type the folder, press **Apply**, then restart the editor. It is stored in `omagari_settings.ron` in the repository folder (not committed). You can also set it per run:

```text
omagari --game-assets /path/to/my_game/assets
OMAGARI_GAME_ASSETS=/path/to/my_game/assets
```

The command line beats the environment variable, which beats the settings file.

**No game yet?** Leave it unset. Then this repository's `assets/` folder serves as the game folder (it holds the built-in textures) and EXPORT writes to `exported_effects/`.

While you are in ⚙ SETTINGS, set the **Game** camera to match your game's camera (projection, visible height or field of view, angles). The default is an orthographic isometric camera showing 16 m from the bottom to the top of the view. See [EDITOR.md](EDITOR.md#cameras).

## 4. Open the demos

**OPEN** lists the projects in `saved_effects/`. The demos there show every layer kind. Most were made for a game with a humanoid (Mixamo) character: without that model the MODEL stage shows a capsule instead and says so in the status line, and sockets on the character's bones sit at the capsule's centre. They still play; see [MODELS.md](MODELS.md#the-demos-and-their-models) for which demos expect which bones, and how to use your own character.

Good first picks that need no model:

| Demo | What it shows |
| :-- | :-- |
| `blade_swing` | A trail and afterimages between two sockets, on a target running in a circle. |
| `status_skin` | A surface layer: a poison skin with scrolling noise that flashes on `tick`. |
| `shield_bubble` | A shape layer whose size and opacity follow the parameter `shield`. |
| `shockwave` | A ring that grows and fades on the trigger `slam` (auto-fired every 1.5 s). |
| `hit_flash` | A light flash and sparks on the trigger `hit` (auto-fired every second). |
| `shockwave_ripple` | A distortion ripple on `slam`. |
| `attach_demo` | Particle layers on a socket and on a segment between two sockets. |
| `hanabi_coverage_demo` | Many bevy_hanabi modifiers in one project. |

Use **STAGE → CAPSULE** or **AIR** if a demo looks odd without its model.

**Bundled examples.** `examples/` holds four particle projects from the original Omagari (`fireworks`, `explode`, `greenvapor`, `magicburst`). OPEN only lists `saved_effects/`, so copy one there to open it (for example `examples/fireworks.omagari.ron` → `saved_effects/fireworks.omagari.ron`).

**Recipes.** **NEW → From a recipe** starts a new, unsaved project from one of the tested recipes of the authoring guide: sparks, fire, smoke, auras, shields, trails, lights, distortion and more.

## 5. Make your first effect

A burst of sparks that your game fires when something is hit.

1. **NEW → From a recipe → "Spark burst (one-shot impact)"**. A project with one particle layer, `sparks`, opens and plays. Press **Space** (or **RENDER** in the bottom bar) to play it again.
2. **Set the scene.** **STAGE → FLOOR** puts the floor at the spawn point, like ground in a game. **BG** switches to a dark room, where glow is easier to judge.
3. **Tune it.** In the **LAYER** tab:
   - **Spawner → Count**: how many sparks (try 300).
   - **Init → `lifetime`**: how long each spark lives, in seconds.
   - **Init → SetVelocitySphere → speed**: how fast they fly out.
   - **Render → ColorOverLifetime**: the colour from birth to death. Values above 1 glow (HDR); the last key's alpha 0 fades them out.

   Every value has a tooltip with its unit. **Live** (bottom bar) rebuilds the preview a moment after each change; **Ctrl+Z** undoes.
4. **Save it.** **SAVE AS…** → `my_sparks`. The project is now `saved_effects/my_sparks.omagari.ron`.
5. **Let the game fire it.** A one-shot that plays at the start is fine for `spawn_vfx_at`. To fire it on an instance that stays on a character instead:
   - **TRIGGERS** tab → **➕ ADD TRIGGER**, name it `hit`.
   - **LAYER** tab → **Play** → **OnTrigger**, trigger `hit`.
   - Back in **TRIGGERS**, **⚡ FIRE** plays it; **auto-fire every N s** repeats it while you tune.
6. **Check it.** The counter in the bottom bar shows errors and warnings (the same checks Claude's MCP server runs). Click it for the list.

From here, add more layers with **➕ ADD** in the LAYERS list (a flash of light, a shockwave ring, smoke), or start from a demo.

## 6. Export it and play it in your game

**EXPORT** (Ctrl+E) writes `my_sparks.vfx.ron` to `<game assets>/vfx/` (or `exported_effects/` without a game folder). In your game (Bevy 0.19):

```rust
use bevy::prelude::*;
use bevy_hanabi::prelude::*;
use omagari::{VfxCommandsExt, VfxPackage};

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, HanabiPlugin, omagari::OmagariPlugin))
        .add_systems(Startup, setup)
        .run();
}

fn setup(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.spawn((Camera3d::default(), omagari::distortion_camera(), bevy::post_process::bloom::Bloom::NATURAL, Transform::from_xyz(0.0, 3.0, 6.0).looking_at(Vec3::ZERO, Vec3::Y)));
    let sparks: Handle<VfxPackage> = asset_server.load("vfx/my_sparks.vfx.ron");
    commands.spawn_vfx_at(&sparks, Transform::default()); // plays once, then cleans up
}
```

with `omagari = { git = "https://github.com/NazarZubyk/omagari", branch = "master" }` and `bevy_hanabi = "0.19"` in your `Cargo.toml`. If you made it an `OnTrigger("hit")` layer, keep an instance on a character (`spawn_vfx`) and call `vfx.trigger_at(instance, "hit", TriggerArgs::at(point))`.

[GAME_INTEGRATION.md](GAME_INTEGRATION.md) explains all of it; `cargo run --release --example game_integration` runs a complete example.

## 7. Next steps

- [EDITOR.md](EDITOR.md): every tab, the stages and cameras, and each layer kind.
- [MODELS.md](MODELS.md): previewing on your own characters, animations and weapons.
- [FORMAT.md](FORMAT.md): the project and package file format.
- [MCP.md](MCP.md): let Claude make and tune effects for you.
- [CLAUDE_VFX_GUIDE.md](CLAUDE_VFX_GUIDE.md): the authoring guide written for Claude; it is also the most detailed reference for modifiers, expressions and recipes.
