# OMAGARI
Bevy-Hanabi 3D particle effects editor designed for the next version of https://hexroll.app.

![firework](https://raw.githubusercontent.com/hexroll/omagari/refs/heads/master/images/fireworks.gif)

> [!WARNING]
> **This is a personal fork, maintained with AI assistance.**
>
> This fork of [hexroll/omagari](https://github.com/hexroll/omagari) is kept up to date for my own game. Most changes here (Bevy/Hanabi version upgrades, bug fixes, new features) are written with the help of AI coding tools. I review the changes and fix the bugs I hit while using the editor, but:
>
> - **There is no guarantee that everything works.** Features I don't use myself may be broken without me noticing.
> - **The fork will grow extensions** beyond the original editor (extra modifiers, editor tools, workflow changes) as my game needs them. They may change file formats or behaviour without notice.
> - **Compatibility is tied to my game's Bevy version**, not to a release schedule.
>
> You're welcome to use it, but treat it as experimental. For the original, hand-written editor, see the [upstream repository](https://github.com/hexroll/omagari).

## Background
This editor was written as an internal tool for the development of hexroll3. Following requests by the Bevy community, it is now open-sourced as-is.

## Getting Started

Clone this repository, and then

```
cd omagari/
cargo run --release --features editor
```

On Windows you can also double-click `start.bat`, which builds the editor (and the MCP server, see below) and opens it.

- Omagari uses Hanabi's public API only, utilizing a set of serializable proxy editors that together compose a project file that you can save and load.

- Omagari project files should be named `{project_name}.omagari.ron` and live in `saved_effects/`. The editor saves format v2 (a list of texture **assets** and particle **layers**); older v1 projects still load and are converted when you save them.

- The left panel lists the project's **layers**. Each particle layer has a name, a parent layer, a simulation space (**Local**: particles follow the target; **Global**: they stay where they were emitted and trail behind a moving target), a play mode, a texture (the built-in textures, added to the project's assets when picked), the blending mode, the spawner and the init / update / render modifiers (including texture sampling and orientation).

- **RENDER** builds the project and plays it through the same runtime a game uses; **STOP** stops it the way a game does (no new particles, live ones finish).

- **EXPORT** writes a VFX package `{project_name}.vfx.ron`: into the game assets folder's `vfx/` when one is set (see below), else into `exported_effects/`. The editor no longer writes `{project_name}.hanabi.ron` files; existing ones still load in games (legacy).

- All delete ('`X`') buttons require right-click activation for safety.

### Game assets folder

Point Omagari at your game's `assets/` folder so exports land in `<game assets>/vfx/` and project textures (paths like `vfx/textures/smoke.png`) load from the game. Set it in the toolbar's **⚙ GAME ASSETS** menu (stored in `omagari_settings.ron` in the repository root; restart the editor to apply), or per run with `--game-assets <dir>` or the `OMAGARI_GAME_ASSETS` environment variable. The command line beats the environment variable, which beats the settings file. Without a game folder, this repository's `assets/` is used and exports go to `exported_effects/`.

### Creating effects with Claude

Omagari includes an MCP server (`omagari --mcp`) that lets Claude Code create, check, tune and export effects. Claude doesn't get images: the server validates projects and simulates them on the CPU to describe them in text, and you watch the result in the open editor. See [docs/MCP.md](docs/MCP.md) for setup and usage.

### Using exported effects in a game

Add `omagari` as a dependency **without** the `editor` feature, add `OmagariPlugin` after `HanabiPlugin`, and play the exported `*.vfx.ron` package (see "Library API" below).

#### Legacy `*.hanabi.ron` exports

Files exported by earlier versions load as an `EffectComplex` asset:

```rust
app.add_plugins((HanabiPlugin, omagari::OmagariPlugin));

let handle: Handle<omagari::EffectComplex> = asset_server.load("effects/fireworks.hanabi.ron");
```

Since `bevy_hanabi` 0.19, effect assets are serialized through Bevy's type registry, so any custom Hanabi modifier used in an effect must be registered in the app before the file is loaded.

> **Note:** `*.hanabi.ron` files exported by Omagari 0.17 or older can't be loaded by this version, because Hanabi's serialization format changed. Load the `*.omagari.ron` project in the editor and export it again.

### Library API (in progress): VFX packages

The library loads a `*.vfx.ron` package (the editor's export) or a project directly (`*.omagari.ron`, v1 or v2) and plays it attached to any entity. Effects are built from the project at load time, so no Hanabi type registration is needed, and the built-in textures are embedded in the library (`omagari://textures/<file>`).

```rust
app.add_plugins((DefaultPlugins, HanabiPlugin, omagari::OmagariPlugin));

use omagari::{VfxCommandsExt, VfxControl, VfxFinished, VfxPackage};

let package: Handle<VfxPackage> = asset_server.load("vfx/fireworks.omagari.ron");
let instance: Entity = commands.spawn_vfx(&package, target); // child of `target`
let hit: Entity = commands.spawn_vfx_at(&package, Transform::from_translation(point)); // in the world

fn control(mut vfx: VfxControl) {
    vfx.stop(instance);    // stops emitting, despawns when the last particle died
    vfx.restart(instance); // while stopping: plays again on the same instance
    vfx.is_alive(instance);
    vfx.state(instance);   // Starting / Running / Stopping / Finished
}

fn finished(mut done: MessageReader<VfxFinished>) {
    for f in done.read() { /* f.instance, f.target */ }
}
```

- **Lifecycle.** An instance spawns its layers once the package and its textures have loaded. `stop` is graceful; `restart` while stopping plays it again. An instance whose root layers are all one-shot (`play: OnStart` with a finite `cycle_count`) finishes by itself after its last spawn plus its longest particle life, so `spawn_vfx_at` one-shots clean up without code; looping ones run until `stop`. `VfxFinished { instance, target }` is sent once whenever an instance ends: finished, stopped, its target despawned (then `target` no longer exists), or despawned by other code. For `spawn_vfx_at` instances `target` is the instance itself.
- **Attach.** Per layer `attach: Target | Socket("name") | Segment(from: "a", to: "b")`. A socket is a package socket (`sockets: [(name, node: Some("mixamorig:RightHand") | None, offset)]`) or, when no package socket has that name, an entity with that `Name` under the target. The layer follows the node (it becomes its child); until the node exists (a glTF scene still loading) the layer sits at the target's origin, and it re-resolves when the hierarchy changes. Segment layers get the two ends every frame as Hanabi properties `segment_a` / `segment_b`, readable in expressions with `Property("segment_a")`.
- **Size fit.** `fit: None` keeps the authored size whatever the target's scale, `Some(TargetScale)` inherits it, `Some(TargetHeight(1.8))` scales by the height of the target's own meshes / 1.8 m. Fit scales `Local` layers only: Hanabi applies the emitter's full transform to Local particles but only its position to Global ones.
- **Own model.** Mesh searches under a target (fit now, surface layers later) stop at nested glTF scenes (`WorldAssetRoot`, e.g. a weapon in a hand bone) and at entities marked `omagari::VfxSeparateModel`.
- **Hot reload.** When a package asset changes (Bevy's file watcher, `AssetPlugin { watch_for_changes_override: Some(true), .. }` with the `file_watcher` feature), running instances rebuild their layers in place: old particles finish, new layers start, the instance entity and its state stay. `VfxControl::replace_package` does the same with another package (the editor's RENDER uses it).

Texture paths in a package are relative to the game's `assets/` folder (`vfx/textures/smoke.png`); built-ins are `omagari://textures/<file>`. The editor previews packages through this same API. See `docs/VFX_SYSTEM_PLAN.md` for what comes next (parameters, triggers, more layer kinds).

Legacy: `EffectComplex::spawn` returns the spawned entities, and its `despawn_in` timer now stops the spawners and despawns each effect once its last particle has died.

## Compatibility

| `Omagari`    | `bevy_hanabi` | `bevy` | Notes |
| :--          | :--           | :--    | :--   |
| `0.19`       | `0.19`        | `0.19` | This fork, AI-assisted upgrade |
| `0.17`       | `0.17`        | `0.17` | Upstream |
| `0.16`       | `0.16`        | `0.16` | Upstream |

## Contributions

This code could use your care :) so contributions are more than welcome, and [showcasing](https://github.com/hexroll/omagari/discussions/1) your effects for others to learn from is highly encouraged.

## License

Similar to Hanabi, Omagari is dual-licensed under either:

- MIT License ([`LICENSE-MIT`](./LICENSE-MIT) 
- Apache License, Version 2.0 ([`LICENSE-APACHE2`](./LICENSE-APACHE2)

at your option.

`SPDX-License-Identifier: MIT OR Apache-2.0`
