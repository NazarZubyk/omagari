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

### The editor window

- **Top toolbar**: the open project's name (`• unsaved` when it has changes), **NEW** (empty, or from one of the guide's recipes), **OPEN**, **SAVE**, **SAVE AS…**, **EXPORT**, undo / redo, **STAGE** (air, floor, around a rock, on a capsule, on a model), **CAMERA** (Orbit or Game, see below), the backdrop and **⚙ SETTINGS**.
- **Left: LAYERS**. Click a layer to edit it. ➕ ADD (a ✨ particles layer, a 🎨 surface layer, a 🔷 shape layer, a 〰 trail layer or a 👥 afterimage layer, see "Surface layers", "Shape layers" and "Trail and afterimage layers" below), ⬆ / ⬇ to reorder, DUPLICATE, DELETE (asks first); right-click a layer for the same commands. 👁 hides a layer and **S** solos it in the preview only (hidden layers still spawn their child layers; nothing about this is saved). A `⛔` / `⚠` badge shows the layer's problems.
- **Right: tabs**. **LAYER** is the inspector of the selected layer: its problems, name, capacity, texture, parent, simulation space (**Local**: particles follow the target; **Global**: they stay where they were emitted and trail behind a moving target), attach, play mode, blending, motion (velocity integration), when to simulate, a fixed random seed, the spawner (with **Once / Rate / Burst** presets) and the init / update / render modifiers. The **+** menus are grouped (Position, Velocity, Forces, Kill, Events, Color / Size, Texture / Flipbook, Orientation), and cover nearly every bevy_hanabi 3D modifier (cone emitters, radial and tangent forces, kill volumes, flipbooks, rounded or screen-space particles, render-time colour and size, spinning sprites); the expression **+** menu has every bevy_hanabi operator and value type. **SOCKETS & FIT**: named points on the target (node picker from the preview model, offset fields, nudge buttons; the sockets show as labelled dots in the viewport, segments as lines, and a dot can be dragged) and size fit. **PREVIEW**: stage, camera, backdrop, target motion, and the model stage: model, root height, animation (file and clip), what the instance is attached to, and a weapon (see "Model preview" below). **🌲 HIERARCHY**: the preview model's nodes, bones, meshes and materials as a searchable tree; per node 🎯 attaches the preview instance to it, ⚓+ adds a socket on it, A / B make it the selected layer's segment start / end. **PARAMETERS**: the values the game sets (add, rename — every use is renamed too —, number or colour, default, range, smoothing), each with a live slider or colour picker that drives the preview like the game's `set_param` and an **animate** button (min → max → min). **TRIGGERS**: the events the game fires (add, rename, pulses), with **FIRE**, **at random point** (a payload on the model's or capsule's bounds, near the target without one), **auto-fire every N s** and **fire at clip time** (each loop of the preview animation, e.g. a swing's hit frame). A layer's **Play** can be **OnTrigger** or **While**, its **Rate** a constant or a curve over a parameter, and **Visible** a parameter range. **ASSETS** is the project's texture library (see below). Hover any label for a short explanation with units.
- **Bottom bar**: **RENDER** rebuilds the effect and plays it through the same runtime a game uses; **STOP** stops it the way a game does (no new particles, live ones finish); **RESTART** clears every particle and plays again. **Live** rebuilds the preview a moment after each edit. **⏸** pauses and **Speed** slows the preview down. The problems counter (the same checks as the MCP `check_project`) opens the Problems list; `⚠ n build` lists warnings of the last preview build.
- **Undo / redo** cover every project edit (layers, sockets, fit, the preview block); a drag or a typed word is one step. NEW, OPEN, an MCP preview and closing the window ask before throwing away unsaved changes.
- **Shortcuts**: Ctrl+S save, Ctrl+E export, Ctrl+Z undo, Ctrl+Y / Ctrl+Shift+Z redo, Delete delete layer, Ctrl+D duplicate layer, F focus the camera on the target, Space render. While a text field has focus only Ctrl+S and Ctrl+E work.
- **Cameras**: **Orbit** is the free camera (drag to orbit, right-drag to pan, wheel to zoom). **Game** matches the game's camera: orthographic true isometric (yaw 45°, pitch 35.26°), 16 m visible height, HDR + bloom, following the preview target. Both are render profiles in `omagari_settings.ron` (projection, height or field of view, yaw, pitch, distance, tonemapping, bloom, exposure, background), edited live in ⚙ SETTINGS.

### Model preview (CAPSULE and MODEL stages)

- **CAPSULE** puts a 0.7 × 1.8 m capsule on the target (its centre is the target, like a character's collider and root); **MODEL** shows a glTF model of the game assets folder (PREVIEW tab → Model file, a `.glb` / `.gltf`), standing on the floor `root height` metres below the target (0.9 for a game whose player root is 0.9 m above the feet) and facing the direction of travel when the target moves. Without a model (or with a missing file) MODEL shows the capsule.
- **Animation**: a clip of the model's file or of another file with the same skeleton (Mixamo clips are separate files, all named `mixamo.com`): pick the animation file and the clip. Clips play on nodes with the same names, no retargeting. The bottom bar has the animation's pause, loop, time slider (drag to scrub) and speed.
- **Attach to**: the preview instance is spawned on the model's root (the moving target) or on a node of the body or of the weapon, like the game's `spawn_vfx(&package, entity)`; sockets are found below it.
- **In place** (on by default): removes the clip's horizontal root motion (the skeleton root, e.g. Mixamo's hips, walking away from the model's origin) after the animation, so the character stays on the target while you tune effects; the vertical bob stays. Preview only; the game handles root motion itself.
- **Weapon**: a second model held by a node of the body (e.g. `mixamorig:RightHand`) with an offset (metres), rotation (degrees) and scale. It is a separate model: effects on the body (fit height, surface layers) do not cover it. Pick **Instance on: weapon** to preview a weapon effect.
- The checks know the model: socket nodes, attach names, attach-to and the weapon node it does not have are errors, and so is a clip the animation file does not have.
- Everything is saved in the project's `preview` block (`model`, `root_height`, `animation`, `animation_file`, `animation_speed`, `animation_triggers`, `attach_to`, `target`, `weapon`, `in_place`; new fields are written only when set) and is not part of the exported effect. The MCP's `describe_model` lists a model's nodes and clips; `preview_in_editor` takes `stage: "model"`, `model`, `animation`, `animation_file`, `attach_to`.
- Licensing: models stay in the game's folder; do not commit them to this repository.

### Surface layers (effect shader over the model)

A **surface layer** draws Omagari's effect shader over the target's own model: a purple poison skin with crawling noise, an energy field with thickness, frost creeping up from the feet. The model and its materials are never changed: the runtime adds copies of the model's meshes (the same mesh and skin, so they follow every animation frame) and draws the effect on them; a weapon held in the hand is a separate model and is not covered.

- **Covers**: all meshes, some meshes, or the meshes with some materials (names picked from the preview model, as in the HIERARCHY tab).
- **Shells**: each shell is one copy pushed outward along the surface by an offset in metres. One shell at 0 = a coloured skin; two or three at 1–4 cm = a soft glow with volume around the body (presets "skin" and "thick glow").
- **Material**: blend (Blend covers by opacity, Add adds light), colour (HDR), opacity, emissive; up to two scrolling **noise** textures (a texture and its channel R / G / B / A / Luminance, **Triplanar** mapping in the model's rest pose so the pattern sticks to the moving body, or the mesh's UVs; scale, scroll, strength, contrast); a **rim** (edge glow); a **pulse**; a **flash** for trigger pulses; **dissolve** with a glowing edge; **fill** (a band from the feet up, e.g. driven by a parameter; or along the mesh's U / V); a **mask** texture. Every value is a constant, a curve over a number parameter, a curve over the layer's **age** (seconds since it started), or (colours) a colour parameter.
- **Fade** in when the effect starts and out when the game stops it (the stop waits for it); **Visible** shows it only in a parameter range.
- Preview on the CAPSULE stage, or on MODEL with your character. `omagari://textures/noise.png` is a built-in tileable noise (➕ BUILT-IN in the ASSETS tab).

### Shape layers (simple 3D shapes with the effect shader)

A **shape layer** draws a sphere, capsule, cylinder (or open tube), flat ring, torus, cone, quad (flat or facing the camera) or flat slash **arc** with the same effect material as surface layers, attached to the target but not bent by its skeleton: a shield bubble, a ground ring, an expanding shockwave, a slash, a column of light. The inspector has preset buttons for those five.

- **Shape** and its size in metres; **Attach** (target, socket, or the middle of a segment) and **Follow**: **Full** moves, turns and tilts with the attach point, **Position** only moves, **PositionYaw** moves and turns with the heading but never tilts (a ground ring stays flat while the character runs). **Offset** (metres) and **Rotation** (degrees).
- **Scale** (X, Y, Z) and **Spin** (degrees per second around an axis): constants, curves over a parameter, or curves over the layer's **age** (a shockwave that grows).
- **Play**: Loop, OnStart, OnTrigger (each trigger plays it again from age 0; `trigger_at` plays it at the hit point) or While a parameter is high. **Duration** makes OnStart / OnTrigger shapes one-shots (fade in and out inside it, then gone) and repeats the age of Loop / While shapes. **Fade**, **Visible**, **Draw order** (sort bias in metres against particles and skin effects at a similar distance), then the material.
- Demo projects: `saved_effects/shield_bubble`, `ground_ring`, `shockwave`, `slash_arc` (Y Bot running; the shockwave and the slash fire on their own every 1.5 / 1 s, or with ⚡ FIRE).

### Trail and afterimage layers (weapon swings)

A **trail layer** draws a glowing ribbon between two moving sockets — a blade's base and tip — that stays in the world behind them and fades: the arc of a sword swing, a comet tail behind a runner. An **afterimage layer** leaves fading copies of the target's meshes along their path (sword shadows, dash echoes); skinned characters are copied in their current pose.

- **Trail**: **From** / **To** sockets (package sockets or nodes of the preview model; in the 🌲 HIERARCHY tab A / B set them), **Length** in seconds (a swing: 0.15–0.4 s), **Smoothing** (Catmull-Rom segments between samples so fast swings stay round), **Min distance** between samples, **Width along** and **Fade along** the trail (u = 0 at the blade, 1 at the end), then the effect material (u / v: `fill` along U cuts along the trail, V softens its edges, UV noise scrolls along it). **show samples** draws the samples in the viewport.
- **Afterimage**: **Copies** of the whole target model or some meshes, **Interval**, **Lifetime**, **Max copies** (copies on screen ≈ lifetime / interval), **Fade over life**, then the material.
- **Emit**: Always, between a start and a stop trigger (`swing_start` / `swing_end`, any names), or While a parameter is high. When emission stops, the trail ages out and the copies fade; nothing is drawn between swings. **Swing preview** fires the start / stop triggers at two clip times of the preview animation (e.g. the slash of `StableSwordOutwardSlash.glb`), saved as animation triggers.
- Presets: "sword trail", "energy trail", "afterimages", "shadow dash". A new trail uses the project's first two sockets, or adds `blade_base` / `blade_tip` sockets (offsets from the target root) to move onto the weapon.
- Demo: `saved_effects/sword_trail` (Y Bot swinging `anim/StableSwordOutwardSlash.glb`, a virtual blade on `mixamorig:RightHand`, trail + body afterimages; set the game assets folder to the game's `assets/`).

### Textures (ASSETS tab)

- **Import** with **📥 IMPORT…** (a file dialog) or by **dropping PNG / JPG files onto the window**. A file outside the game assets folder is copied into `<game assets>/vfx/textures/` (a name already taken by a different file gets `_2`, `_3`, …; the same file again is reused); a file already inside the game assets folder is used where it is. **➕ BUILT-IN** adds one of the 7 textures built into Omagari.
- Each texture has an **id** (what layers pick), a **purpose** (Sprite, Noise, Mask, Gradient; guessed from the file name: noise / perlin / voronoi / cellular, mask, gradient / ramp) and settings that default from it: **wrap** (Repeat tiles, Clamp stretches the edge), **colour space** (colour or data), **filter** (smooth or pixelated) and a **sprite sheet** grid (rows × columns).
- Click a texture for a large preview with **RGB / R / G / B / A** buttons (one channel shown as gray), its size and layout, the layers that use it, **RENAME** (updates those layers), **DELETE** (only while no layer uses it) and **EXTRACT CHANNEL**: writes one channel (R, G, B, A or luminance) as a grayscale PNG next to the file (`<name>_g.png`) and adds it as a new texture. Particle layers can only read a texture's colour or its red channel, so a texture with different masks packed into R, G, B and A needs this for G, B and A.
- Missing files, paths outside the game assets folder and settings that fight the purpose (e.g. Noise with Clamp) show as warnings on the texture. All of it is part of the project: undo, the unsaved marker and the live preview cover it.
- Particle layers read a texture's colour or red channel; **surface layers** pick the channel per texture slot (R, G, B, A or Luminance), so one packed texture can feed several slots.
- Formats: PNG everywhere; JPG needs Bevy's `jpeg` feature in the game (the editor has it); KTX2 files can be referenced but have no preview.

- **EXPORT** writes a VFX package `{project_name}.vfx.ron`: into the game assets folder's `vfx/` when one is set (see below), else into `exported_effects/`. The editor no longer writes `{project_name}.hanabi.ron` files; existing ones still load in games (legacy).

- Delete ('`X`') buttons inside the modifier lists require right-click activation for safety.

### Game assets folder

Point Omagari at your game's `assets/` folder so exports land in `<game assets>/vfx/` and project textures (paths like `vfx/textures/smoke.png`) load from the game. Set it in **⚙ SETTINGS** (stored in `omagari_settings.ron` in the repository root; restart the editor to apply), or per run with `--game-assets <dir>` or the `OMAGARI_GAME_ASSETS` environment variable. The command line beats the environment variable, which beats the settings file. Without a game folder, this repository's `assets/` is used and exports go to `exported_effects/`.

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

Parameters and triggers are how the game talks to an effect. The package declares them under any names (the plugin knows none); the game maps its own rules to them:

```rust
use omagari::{ParamValue, TriggerArgs, VfxSpawn};

// Start with values (not smoothed from the defaults).
let aura = commands.spawn_vfx_with(&package, enemy, VfxSpawn { params: &[("tint", Color::srgb(0.2, 0.9, 1.0).into())] });

// A game rule drives a parameter every frame: here a status's stack count.
fn status_intensity(statuses: Query<(&Poisoned, &PoisonVfx)>, mut vfx: VfxControl) {
    for (poison, fx) in statuses.iter() {
        vfx.set_param(fx.instance, "intensity", poison.stacks as f32 / 5.0); // smoothed, clamped to the range
    }
}

// Game events fire triggers.
fn on_damage(mut ticks: MessageReader<PoisonTick>, mut vfx: VfxControl) {
    for tick in ticks.read() {
        vfx.trigger(tick.vfx, "tick"); // OnTrigger layers burst, pulses flash
    }
}
fn on_hit(mut hits: MessageReader<WeaponHit>, mut vfx: VfxControl) {
    for hit in hits.read() {
        vfx.trigger_at(hit.vfx, "hit", TriggerArgs::at(hit.point).with_direction(hit.normal).with_strength(hit.force));
    }
}

let now: Option<ParamValue> = vfx.param(instance, "intensity"); // current (smoothed) value
vfx.set_param_now(instance, "intensity", 1.0);                  // no smoothing
```

- **Parameters** (`parameters: [(name, default: Float(..) | Color(..), min, max, smoothing)]`): per instance a target (what the game set, clamped to `[min, max]` for numbers) and a current value that follows it, `current += (target - current) × (1 - e^(-dt / smoothing))` (colours per channel; `smoothing: 0` jumps). Layers read the current value in expressions (`Param("intensity")`, uploaded into the effect's Hanabi properties only when it changes), in `rate` (particles per second as a curve over a parameter), `visible` (a parameter range the layer shows in, fading at its edges) and `play: While(param, threshold)`. Values set before the package has loaded apply when it has; they survive hot reload and `replace_package`.
- **Triggers** (`triggers: [(name, pulses: [(param, to, decay)])]`): layers with `play: OnTrigger(name)` burst (the spawner restarts; such layers do not play at the start or on `restart`), and pulses make a number parameter jump to `to` and return over `decay` seconds. `trigger_at` writes the position, direction and strength into the properties `trigger_pos` / `trigger_dir` / `trigger_strength` of the bursting layers (in their simulation space) first; without them the layer's origin and +Y are used. A trigger fired while the instance is still loading waits for it. Packages with `OnTrigger` layers do not finish by themselves; stop them.
- Unknown parameter or trigger names and values of the wrong kind (a number for a colour) are ignored with one warning per package and name; nothing panics.

- **Lifecycle.** An instance spawns its layers once the package and its textures have loaded. `stop` is graceful; `restart` while stopping plays it again. An instance whose root layers are all one-shot (`play: OnStart` with a finite `cycle_count`) finishes by itself after its last spawn plus its longest particle life, so `spawn_vfx_at` one-shots clean up without code; looping ones run until `stop`. `VfxFinished { instance, target }` is sent once whenever an instance ends: finished, stopped, its target despawned (then `target` no longer exists), or despawned by other code. For `spawn_vfx_at` instances `target` is the instance itself.
- **Attach.** Per layer `attach: Target | Socket("name") | Segment(from: "a", to: "b")`. A socket is a package socket (`sockets: [(name, node: Some("mixamorig:RightHand") | None, offset)]`) or, when no package socket has that name, an entity with that `Name` under the target. The layer follows the node (it becomes its child); until the node exists (a glTF scene still loading) the layer sits at the target's origin, and it re-resolves when the hierarchy changes. Segment layers get the two ends every frame as Hanabi properties `segment_a` / `segment_b`, readable in expressions with `Property("segment_a")`.
- **Size fit.** `fit: None` keeps the authored size whatever the target's scale, `Some(TargetScale)` inherits it, `Some(TargetHeight(1.8))` scales by the height of the target's own meshes / 1.8 m. Fit scales `Local` layers only: Hanabi applies the emitter's full transform to Local particles but only its position to Global ones.
- **Surface layers.** For every mesh of the target's own model that matches the layer's targets, one overlay entity per shell is spawned as a child of the mesh (same `Mesh3d`, same `SkinnedMesh`, so it animates with the body) with the library's `OmagariEffectMaterial` (`MaterialPlugin` registered by `OmagariPlugin`; the WGSL is embedded, the game adds no shader files). The model and its materials are not changed; overlays follow meshes that appear later (a glTF scene still loading) or change, and go with the layer. One material per instance, layer and shell; its uniforms follow the parameters, pulses and fades. Overlays draw a couple of millimetres closer to the camera along the view ray (no z-fighting with the model, the same pixels) and cast no shadows. `VfxInstance::surface_layers()` lists the layer entities.
- **Shape layers.** One entity per shape layer, a child of the instance, with a generated mesh (one per distinct shape, shared) and the `OmagariEffectMaterial`. It is placed after transform propagation each frame from its attach point and follow mode (so it never lags a frame or inherits a tilt it should not), scaled by the size fit. `play: OnTrigger` shapes play on the trigger (at the payload position when `trigger_at` gives one); a `duration` makes `OnStart` / `OnTrigger` shapes one-shots; a package whose layers are all one-shots (particles and `OnStart` shapes) finishes by itself. `VfxInstance::shape_layers()` lists the entities.
- **Trail and afterimage layers.** A trail is one entity per layer (a child of the instance) with a CPU-built mesh in world space (`GlobalTransform` = identity): every frame, after transform propagation, both sockets are sampled; samples older than `length` are dropped (the end cut exactly at `length`), smoothed with centripetal Catmull-Rom curves and written as a two-sided ribbon (u = age / length, v = from → to, the fade along it in the vertex colours); at most 200 samples / 256 points. An afterimage layer copies the target's own meshes every `interval` as world entities with their own material; a skinned mesh's copy gets frozen joints (entities holding copies of the joints' `GlobalTransform`s) and a `SkinnedMesh` pointing at them. Both emit `Always`, `While(param, threshold)` or `Trigger(start, stop)`; `stop` ends emission and the instance waits for the trail to age out / the copies to fade. `VfxInstance::{trail_layers, afterimage_layers}` list the entities.
- **Age.** Every binding can be a curve over the layer's age, `Age([(0.0, 0.2), (0.6, 3.0)])`: the seconds since the layer started (since its trigger played it, or a `While` parameter switched it on); `restart` starts it over.
- **Own model.** Mesh searches under a target (fit, surface layers) stop at nested glTF scenes (`WorldAssetRoot`, e.g. a weapon in a hand bone) and at entities marked `omagari::VfxSeparateModel`. The editor's weapon preview is such a nested scene.
- **Hot reload.** When a package asset changes (Bevy's file watcher, `AssetPlugin { watch_for_changes_override: Some(true), .. }` with the `file_watcher` feature), running instances rebuild their layers in place: old particles finish, new layers start, the instance entity and its state stay. `VfxControl::replace_package` does the same with another package (the editor's RENDER uses it).

Texture paths in a package are relative to the game's `assets/` folder (`vfx/textures/smoke.png`); built-ins are `omagari://textures/<file>`. Each texture loads with its settings (wrap, colour space, filter) as loader settings, so the game passes nothing; a hot reload that changes them applies them to the already loaded image. PNG works with Bevy's default features; for JPG textures enable Bevy's `jpeg` feature (`bevy = { version = "0.19", features = ["jpeg"] }`). The editor previews packages through this same API (its PARAMETERS sliders call `set_param`, its FIRE buttons `trigger` / `trigger_at`). See `docs/VFX_SYSTEM_PLAN.md` for what comes next (ribbons and mesh particles, light and distortion layers).

Weapons are separate models with their own effects. Spawn the weapon's package on the weapon entity and drive its swing from the attack state (the trigger names are the package's):

```rust
fn swing_vfx(attacks: Query<(&Attack, &WeaponFx), Changed<Attack>>, mut vfx: VfxControl) {
    for (attack, fx) in attacks.iter() {
        match attack.phase {
            AttackPhase::Swing => { vfx.trigger(fx.instance, "swing_start"); } // trail and afterimages start
            AttackPhase::Recover => { vfx.trigger(fx.instance, "swing_end"); } // they age out / fade
            _ => {}
        }
    }
}
```

Legacy: `EffectComplex::spawn` returns the spawned entities, and its `despawn_in` timer now stops the spawners and despawns each effect once its last particle has died.

## Development

- Build: `cargo build --release --features editor`; test: `cargo test --release --features editor`.
- **Screenshots without looking at the screen** (for agents checking the UI): the editor takes developer flags, then saves the window and quits:

  ```
  omagari --open fire --tab assets --asset smoke --channel a --camera game \
          --window-size 1600x900 --screenshot shot.png --screenshot-after 90
  ```

  | Flag | Effect |
  | :-- | :-- |
  | `--open <project>` | opens `saved_effects/<project>.omagari.ron` at startup |
  | `--camera orbit\|game` | the preview camera of the opened project |
  | `--tab layer\|sockets\|preview\|parameters\|triggers\|assets\|hierarchy` | the right panel's tab |
  | `--asset <id>` | shows that texture in the ASSETS tab |
  | `--channel rgb\|r\|g\|b\|a` | the texture preview's channel view |
  | `--import <file>` | imports a file at startup like a drop (repeatable) |
  | `--screenshot <out.png>` | saves the window after `--screenshot-after <frames>` (default 90) and quits |
  | `--window-size <w>x<h>` | window size in pixels (scale factor 1) |
  | `--stage air\|floor\|object\|capsule\|model` | the stage of the opened project |
  | `--model <glTF>` | the preview model (path in the game assets folder; sets the MODEL stage) |
  | `--animation <clip>` / `--animation-file <glTF>` | the clip and the file it comes from |
  | `--attach-to <node>` / `--root-height <m>` | where the instance is attached, the target's height above the feet |
  | `--anim-time <s>` | pauses the animation at this clip time |
  | `--anim-start <s>` | jumps the animation to this clip time and lets it play |
  | `--freeze-at <s>` | when the playing animation passes this clip time, pauses the whole preview (animation, trails, particles) and takes the screenshot 8 frames later, e.g. a sword trail mid-swing |
  | `--no-in-place` | keeps the clip's root motion (preview `in_place` off) |
  | `--motion none\|circle\|back_and_forth` | the target motion |
  | `--focus` | the orbit camera looks at the target |
  | `--backdrop light\|dark\|black` | the backdrop |
  | `--orbit <yaw>,<pitch>,<distance>` | the orbit camera's angles (degrees) and distance (m) |
  | `--focus-node <name>` | the orbit camera looks at a node of the model (e.g. `mixamorig:Hips`) |
  | `--screenshot-crop <x>,<y>,<w>,<h>` | saves only this part of the window (pixels) |
  | `--stop-at <frame>` | stops the preview instance (STOP) at this frame, e.g. to check a fade out |
  | `--fire <trigger>` | fires a trigger six frames before the screenshot (`--fire-before <frames>` changes it, e.g. 18 ≈ 0.3 s at 60 fps) |

  Combine with `--game-assets <temp dir>` so imports don't land in the real game folder. A model check: `omagari --game-assets <game>/assets --open model_demo --camera orbit --motion none --focus --anim-time 0.3 --tab sockets --screenshot run.png` (the demo uses `anim/tpose.glb` + `anim/run.glb`).

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
