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

- Omagari project files should be named `{project_name}.omagari.ron`.

- Exporting an Omagari project will generate a custom `ron` file built from a set of serialized `EffectAssets` along with some additional metadata.

- Exported files will be named `{project_name}.hanabi.ron`.

- All delete ('`X`') buttons require right-click activation for safety.

### Creating effects with Claude

Omagari includes an MCP server (`omagari --mcp`) that lets Claude Code create, check, tune and export effects. Claude doesn't get images: the server validates projects and simulates them on the CPU to describe them in text, and you watch the result in the open editor. See [docs/MCP.md](docs/MCP.md) for setup and usage.

### Using exported effects in a game

Add `omagari` as a dependency **without** the `editor` feature, add `OmagariPlugin` after `HanabiPlugin`, and load the exported file as an `EffectComplex` asset:

```rust
app.add_plugins((HanabiPlugin, omagari::OmagariPlugin));

let handle: Handle<omagari::EffectComplex> = asset_server.load("effects/fireworks.hanabi.ron");
```

Since `bevy_hanabi` 0.19, effect assets are serialized through Bevy's type registry, so any custom Hanabi modifier used in an effect must be registered in the app before the file is loaded.

> **Note:** `*.hanabi.ron` files exported by Omagari 0.17 or older can't be loaded by this version, because Hanabi's serialization format changed. Load the `*.omagari.ron` project in the editor and export it again.

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
