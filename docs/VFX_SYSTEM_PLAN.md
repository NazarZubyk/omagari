# Omagari VFX system: specification and roadmap

This document is the work order for the agents that extend Omagari and integrate it into the game. It describes **what** to build, **why**, the **file format**, the **runtime API**, the **editor changes**, and **acceptance criteria** per phase. Items marked *verify* are best guesses about Bevy 0.19 / bevy_hanabi 0.19 APIs that the implementing agent must confirm in the crate sources (`~/.cargo/registry/src/*/bevy_*-0.19.1`, `bevy_hanabi-0.19.0`).

Owners used below:
- **Omagari agent**: works in this repository (`C:\bevy\omagari\omagari`): library (runtime plugin + data model), editor, MCP server, docs.
- **Game agent**: works in the game repository; consumes the library. Game phases start only after the user confirms.

---

## 0. Scope and principles

- **Omagari is a separate, reusable project.** No game-specific code or names live in Omagari. The game depends on the `omagari` library and maps its own rules (statuses, XP, attacks) to Omagari parameters and triggers.
- **Scope is driven by the game's needs**, plus **broad bevy_hanabi coverage** (everything except 2D and multi-camera), because which hanabi features the effects will need is not known in advance.
- **Every feature is available three ways: editor UI, project file, MCP.** A person without an agent must be able to do everything by hand; the agent must be able to do everything through MCP. Acceptance criteria of every feature include its UI.
- **The editor previews through the same runtime as the game.** What the editor shows is what the game shows.
- Names used in this document (`poison`, `sword`, `intensity`, `charge`, `hit`, …) are **examples**. The system has no built-in parameter, trigger or effect names.

### Glossary
- **VFX package**: one authored effect: layers + parameters + triggers + assets. One file.
- **Layer**: one visual part of a package: particles, surface, shape, trail, afterimage, light, distortion.
- **Instance**: one playing copy of a package attached to one target entity. A target can have several instances at once.
- **Parameter**: a named value declared by the package author (any name, any number), set by the game at runtime per instance, smoothed by the plugin, usable by any layer value.
- **Binding**: a layer value that is a constant or a curve over a parameter.
- **Trigger**: a named one-shot event (any name, any number) fired by the game, optionally with a position and direction.
- **Socket**: a named point on the target: a node/bone name in the target's hierarchy plus an optional local offset, or just an offset from the target root.
- **Target**: the entity the instance is attached to (a character root, a weapon entity, a prop).

---

## 1. Goals

Every visual effect in the game (status effects, weapon effects, enemy auras, props, hits) is **authored and previewed in Omagari, saved as one file, and played in the game with a few lines of code**.

A package can contain:

| Capability | Example |
|---|---|
| Particle layers (bevy_hanabi, nearly all 3D features, see F9) | mist, bubbles, drips, sparks, additive glow, flipbooks, ribbons, mesh particles |
| Surface layers (shader drawn over a model's meshes, with optional volume shells) | purple scrolling noise + glowing rim on a poisoned body; glowing energy skin with thickness |
| Shape layers (simple 3D shapes with the same shader) | shield bubble, glowing column, ground ring, shockwave, stylized slash arc |
| Trail layers (ribbon between two moving points) | sword swing trail |
| Afterimage layers (fading ghost copies of a model) | sword "shadows" left along a swing |
| Light layers | weapon glow lighting the floor |
| Distortion layers | heat haze |
| Parameters (free names and count; number or colour) | `intensity` 0..1 set from game progress; `tint` colour per enemy |
| Triggers (free names and count, optional position/direction) | `tick` on damage, `hit` at the contact point, `swing_start` / `swing_end` |
| Attachment | whole target, socket (bone/node + offset), segment between two sockets |
| Size fit | effect authored for a 1.8 m body fits a 4 m enemy |
| Lifecycle | start, keep running while refreshed, stop smoothly (fade out), auto-cleanup |

Hard requirements from the user:
1. Everything is **visible, importable and editable in Omagari**, by hand and through MCP. What the editor shows must be what the game shows.
2. Omagari **imports textures** now (required) through a generic asset library that later takes more asset kinds (models, meshes, animations).
3. The runtime is a **Bevy plugin** usable on any entity (player, enemies, props, weapons).
4. Effects react to **game rules** only through parameters and triggers; the plugin knows nothing about the rules.

### Non-goals
- No gameplay logic in the plugin.
- Omagari never changes the game's models or their materials. Effects are drawn **on top of / around** models.
- No node-graph shader editor: one well-parameterised effect shader (+ distortion shader).
- No texture generator and no channel-packing tool (possible future work: a "pack 4 grayscale images into RGBA" tool).
- No real volumetric rendering (raymarched fog). Volume comes from shells, shapes and particles.

---

## 2. Current state (facts, read from the code)

### Editor project format `*.omagari.ron`
- `OmagariProject { effects: Vec<EffectEditor> }`: `src/controller.rs:18-21`. No other fields, no `#[serde(default)]` anywhere in the repo.
- `EffectEditor { name, parent, capacity, spawner_settings, texture_index, init_modifiers, update_modifiers, render_modifiers }`: `src/effect.rs:154-164`.
- The data model (`src/effect.rs`, `src/modifiers.rs`, `src/expr.rs`) is part of the **editor binary** and is mixed with egui UI code (`draw_ui`, `UiProvider`). The library cannot use it.
- `EffectEditor::produce()` builds a hanabi `EffectAsset` (`src/effect.rs:369-447`). It always adds: one texture slot `"color"`, `AlphaMode::Blend`, `ParticleTextureModifier`, `OrientModifier::AlongVelocity`. Simulation space is never set (hanabi default **Global**).
- Expressions: `ExprWriterEditor` / `ExprOperatorEditor` in `src/expr.rs` (literals, Rand*, Time, Age, Add/Sub/Mul, Uniform, Sin, Cos, Distance, Normalized, Vec3/Vec4, Pack4x8UNorm, Attr, ParentAttr). No property expression, no Mix/Clamp/Smoothstep.
- Save: toolbar SAVE `src/main.rs:549-563`. Load: `load_project` `src/controller.rs:112-120`. Export: `export_effects_to_files` `src/controller.rs:75-95` (copy in `src/mcp/analysis.rs:753-766`).
- `src/complex.rs` is dead code (not a module).

### Export format `*.hanabi.ron` and library (`src/lib.rs`)
- `ExportedProject { effects: Vec<ExportedEffect> }`, `ExportedEffect { name, parent, texture_index, effect_asset }`; the hanabi `EffectAsset` is serialized through **Bevy reflection** with hand-written (de)serializers (`lib.rs:164-391`). Consequences: depends on hanabi's internal type paths, every modifier must be reflection-registered in the game (`register_missing_hanabi_modifiers`, `lib.rs:40`), unknown fields are errors, old files cannot be migrated.
- `OmagariPlugin` (`lib.rs:21-32`) registers the `EffectComplex` asset + loader and `despawn_effects_on_timer`.
- `EffectComplex::spawn(commands, textures, pos, despawn_in)` (`lib.rs:65-103`): fixed world position, returns nothing, textures passed by the game in the editor's fixed order (`PARTICLE_TEXTURES`, `src/effect.rs:470-499`), abrupt despawn (TODO `lib.rs:153`).

### Editor scene and UI (`src/main.rs`)
- `Stage` = Air / Floor / Object (rock), `Backdrop` = light / dark / black (`main.rs:108-219`).
- Camera: perspective `PanOrbitCamera`, `Hdr` + `Bloom::NATURAL` (`main.rs:62-99`). The game uses an orthographic isometric camera.
- Preview entities are respawned only by RENDER or by `poll_preview_requests` (`src/controller.rs:30-64`). **Editing does not live-update. No undo/redo.**
- UI: egui top toolbar + left effects panel (`app_ui`, `main.rs:439-635`), widgets in `src/helpers.rs`.
- No glTF, animation, custom materials or WGSL yet.

### MCP server (`src/mcp/`)
- `mod.rs`: JSON-RPC over stdio, tool list `mod.rs:113-213`, dispatch `mod.rs:215-322`; `preview_in_editor` writes `saved_effects/.preview_request`.
- `analysis.rs`: `check_project` (`:770-924`), CPU simulation `simulate_project` (`:1189`), `analyze_project` (`:1625`), tests `:1857-1940` (guide recipes must pass with zero warnings).
- `docs/CLAUDE_VFX_GUIDE.md` is compiled in (`include_str!`, `mod.rs:20`); keep it in sync with `analysis.rs`.

---

## 3. Target architecture

### 3.1 One data model, one runtime, three front ends
```
                 ┌──────────────────── omagari (library crate) ─────────────────────┐
                 │ format/   data model (serde only, no egui): ProjectV2, layers,     │
                 │           particle effect model (former EffectEditor/modifiers/   │
                 │           expr), produce() -> hanabi EffectAsset, migration       │
 game ──uses──▶  │ runtime/  OmagariPlugin: VfxPackage asset + loader, instances,    │ ◀──uses── editor (bin, feature "editor")
                 │           params, triggers, particles, surface, shape, trail,     │            ├─ egui UI (draw_ui impls live here)
                 │           afterimage, light, distortion                           │            ├─ asset library + import
                 │ shaders/  embedded WGSL                                           │            ├─ model/animation preview
                 │ legacy    EffectComplex (*.hanabi.ron), kept loadable             │            └─ MCP server (--mcp)
                 └───────────────────────────────────────────────────────────────────┘
```
- **The game loads the authoring data directly.** The package file has the same schema as the project file; the runtime calls `produce()` at load time to build hanabi `EffectAsset`s. No hanabi reflection serialization, no per-modifier registration in the game, migrations are ours, hot reload works.
- The editor previews by spawning instances through the runtime API.

### 3.2 Module layout
```
src/lib.rs                  // OmagariPlugin, re-exports, legacy EffectComplex
src/format/
  mod.rs                    // ProjectV2, LayerDef, ParamDef, TriggerDef, Binding, AssetDef, AttachDef
  particles.rs              // particle effect model (moved from effect.rs/modifiers.rs/expr.rs) + produce()
  migrate.rs                // v1 OmagariProject -> ProjectV2
src/runtime/
  mod.rs                    // runtime plugin parts, system sets
  package.rs                // VfxPackage asset + loader (loads textures as dependencies), hot reload
  instance.rs               // instance components, spawn API, lifecycle
  attach.rs                 // socket/segment resolution, size fit
  params.rs                 // smoothing, binding evaluation, hanabi property upload
  triggers.rs               // trigger events, one-shot layers, pulses, payload
  particles.rs              // particle layer -> hanabi entities
  surface.rs                // overlay meshes + OmagariEffectMaterial
  shape.rs                  // shape layer
  trail.rs                  // trail layer (CPU ribbon mesh)
  afterimage.rs             // afterimage layer
  light.rs                  // light layer
  distortion.rs             // distortion layer
src/shaders/omagari_effect.wgsl       // vertex (rest-pose position, inflate) + fragment
src/shaders/omagari_distortion.wgsl
src/editor/                           // editor-only: UI, panels, undo, import, preview stage
src/mcp/                              // unchanged location
```
Rules:
- The library never depends on egui or editor-only crates (`editor` feature gating as today).
- UI code for a data type lives in `src/editor/` (e.g. `impl DrawUi for SetPositionSphereDef`), never in `src/format/`.
- Shaders embedded with `embedded_asset!` (*verify* 0.19 path) so the game adds only the plugin.
- MCP mode: nothing but JSON-RPC on stdout.

---

## 4. Data model and files

### 4.1 Files
| File | Written by | Read by | Contents |
|---|---|---|---|
| `saved_effects/<name>.omagari.ron` | editor, MCP | editor, MCP | Project v2: package data + `preview` block (editor-only) |
| `<game assets>/vfx/<name>.vfx.ron` | editor EXPORT, MCP `export_project` | game runtime, editor | same schema without `preview`, `format_version` set |
| `*.hanabi.ron` | legacy | legacy loader | kept loadable, no longer written |

`ProjectV2` and the package share one Rust type; export = clear `preview`, write. The runtime ignores `preview` if present, so the game can also load a `.omagari.ron` directly during development.

### 4.2 Asset references and the game assets folder
- Assets are referenced **by path relative to the game's `assets/` folder** (`"vfx/textures/noise_perlin.png"`). Built-ins use `"omagari://textures/cloud.png"` (embedded in the library).
- Editor setting **Game assets folder** (persisted in `omagari_settings.ron` in the repo root; also `--game-assets <dir>` and `OMAGARI_GAME_ASSETS`). The editor registers an asset source `game` for it (*verify* `register_asset_source` 0.19 API); the game loads the same paths from its default source.
- Without a configured game folder, the editor uses `assets/` of this repo as the game folder (so the editor still works for other users).

### 4.3 Asset library (F3)
The project has an `assets` list. Each entry: `id` (name used by layers), `kind`, `path`, kind-specific settings. Kinds now: `Texture`. Later: `Model`, `Mesh`, `Animation` (added as new kinds, same panel, same import flow).

Texture settings:
- `purpose`: `Sprite` | `Noise` | `Mask` | `Gradient`. Chooses defaults for the settings below.
- `wrap`: `Repeat` | `Clamp` (noise defaults to Repeat).
- `color_space`: `Srgb` (colour) | `Linear` (data: noise, masks). Purpose picks the default.
- `filter`: `Linear` | `Nearest`.
- `grid`: `Option<(rows, cols)>` for sprite sheets (flipbooks).

Using textures:
- Every texture slot in Omagari's own shaders (surface, shape, trail, afterimage, distortion) is `(asset: id, channel: R|G|B|A|Luminance)`. One-layer internet textures use `Luminance`; channel-packed textures can be used once per channel.
- Particle layers use hanabi's texture sampling (`ImageSampleMapping`: `Modulate`, `ModulateRGB`, `ModulateOpacityFromR`, *verify*), which cannot pick G/B/A. Import therefore offers **"Extract channel to a separate texture"**, writing a grayscale PNG.

Import:
- Editor: **Import** button (native file dialog, `rfd` crate, editor feature only) and **drag & drop** onto the window (Bevy `FileDragAndDrop`). MCP: `import_asset(path, purpose?)`.
- Files outside the game folder are **copied** into `<game assets>/vfx/textures/` (name collision → suffix). Files inside are referenced in place.
- Formats: PNG, JPG; KTX2 if the Bevy feature is enabled.
- **Assets panel**: thumbnails, per-channel view (R/G/B/A buttons), settings, rename, delete (blocked while used, shows users), warnings for missing files.

### 4.4 Project v2 sketch (`*.omagari.ron`)
```ron
(
    version: 2,
    parameters: [
        // any names, any number. kind: Float | Color. smoothing: seconds to ~63% of a new target.
        (name: "intensity", kind: Float, default: 0.0, min: 0.0, max: 1.0, smoothing: 0.4),
        (name: "tint", kind: Color, default: (0.65, 0.08, 0.95, 1.0), smoothing: 0.2),
        (name: "flash", kind: Float, default: 0.0, min: 0.0, max: 1.0, smoothing: 0.0),
    ],
    triggers: [
        (name: "tick", pulses: [(param: "flash", to: 1.0, decay: 0.35)]),
        (name: "hit"),                              // may carry position/direction from the game
    ],
    assets: [
        (id: "mist",  kind: Texture((path: "omagari://textures/cloud2.png", purpose: Sprite))),
        (id: "noise", kind: Texture((path: "vfx/textures/noise_perlin.png", purpose: Noise))),
    ],
    fit: None,                                      // None | TargetScale | TargetHeight(1.8) (F1)
    layers: [
        Particles((
            name: "mist",
            attach: Target,                         // Target | Socket("name") | Segment(from: "a", to: "b")
            simulation_space: Local,                // Local | Global
            play: Loop,                             // Loop | OnStart | OnTrigger("hit") | While(param, threshold)
            rate: Param("intensity", [(0.0, 10.0), (1.0, 30.0)]),
            visible: None,                          // Some((param: "intensity", from: 0.6, to: 1.0, fade: 0.1))
            effect: ( /* particle effect model: capacity, spawner, texture, init/update/render modifiers */ ),
        )),
        Surface((
            name: "skin",
            targets: AllMeshes,                     // AllMeshes | Meshes([..]) | Materials([..]); own model only
            material: ( /* EffectMaterialDef, see F4 */ ),
            shells: [(offset: 0.0), (offset: 0.02, opacity: 0.5)],   // volume
            fade: (in: 0.2, out: 0.4),
        )),
        Shape(( name: "shield", attach: Target, shape: Sphere(radius: 1.0), material: ( /* … */ ), fade: (in: 0.1, out: 0.3) )),
        Trail(( name: "slash", from: "blade_base", to: "blade_tip", length: 0.2, emit: Trigger(start: "swing_start", stop: "swing_end"), material: ( /* … */ ) )),
        Afterimage(( name: "ghosts", interval: 0.04, lifetime: 0.25, emit: Trigger(start: "swing_start", stop: "swing_end"), material: ( /* … */ ) )),
        Light(( name: "glow", attach: Socket("blade_mid"), color: Param("tint", ..), intensity: Param("intensity", [(0.3, 0.0), (1.0, 8000.0)]), range: Const(3.0), flicker: 0.1 )),
        Distortion(( name: "haze", attach: Segment(from: "blade_base", to: "blade_tip"), shape: Capsule(radius: 0.12), strength: Param("intensity", [(0.8, 0.0), (1.0, 0.015)]), noise: (asset: "noise", channel: Luminance) )),
    ],
    sockets: [                                      // package-level named points (used when the model has no such node)
        (name: "blade_base", node: None, offset: (0.0, 0.05, 0.0)),
        (name: "blade_tip",  node: None, offset: (0.0, 0.95, 0.0)),
    ],
    preview: (                                      // editor-only
        stage: Model,                               // Air | Floor | Object | Capsule | Model
        camera: Game,                               // Orbit | Game (render profile from settings)
        model: Some("models/ybot.glb"),
        animation: Some("Run"), animation_speed: 1.0,
        attach_to: None,
        params: [("intensity", Float(0.5))],
        motion: Some(Circle(radius: 3.0, speed: 3.6)),
        auto_triggers: [("tick", 1.0)],             // fire every N seconds
    ),
)
```
Notes:
- `Binding<T>` = `Const(T)` | `Param(name, keys: [(param_value, T)])`, piecewise linear. `T` is `f32`, colour or vec3. A `Color` parameter can be bound directly (`Param("tint", Identity)` *exact syntax to define*).
- Every field has `#[serde(default)]`; v1 files and partial files load.
- Unknown fields are tolerated with a log warning (newer editor, older game).

### 4.5 v1 → v2 migration
- `version` missing ⇒ v1. Each `EffectEditor` becomes `Particles(...)` with `attach: Target`, `simulation_space: Global` (keeps the old look), `play: Loop` or `OnStart` (from `cycle_count`), `texture_index` → a built-in texture asset entry. The formerly hard-coded render settings (texture sampling, orient along velocity, alpha blend) become explicit render modifiers / settings with the same values.
- Test: every file in `examples/`, `saved_effects/` and every guide recipe loads, migrates, produces the same `EffectAsset` as before (compare produced modifiers), and round-trips v2.

---

## 5. Feature specifications

Each feature: design, runtime API, editor UI, MCP, tests, AC. "AC" = acceptance criteria.

### F1. Instances: attach, follow, local space, fit, smooth stop, hot reload

**Design**
- `VfxPackage` asset + loader (`.vfx.ron` and `.omagari.ron`). Loads referenced textures as dependencies. Builds hanabi `EffectAsset`s once per (package, particle layer) and stores their handles; instances share them (per-instance differences come from properties, F2).
- An instance is an entity, child of the target (`ChildOf(target)`), with `VfxInstance { package, state, params, … }`. Layer entities are children of the instance.
- Attach (`attach.rs`): `Target`; `Socket(name)`: package socket → descendant node with `Name == node` (+ offset), or offset from target root if `node` is `None`; `Segment(from, to)`: two sockets resolved each frame, positions written into hanabi properties `segment_a`/`segment_b` (layer local space). Resolution retries every frame until the glTF scene exists, then caches; re-resolves when the hierarchy changes.
- **Own model only**: a surface layer covers meshes under the target but stops at descendants that are another model root (entities with `SceneRoot`, or marked `VfxSeparateModel`). A weapon parented to a hand bone is therefore not covered by a body effect. To cover both, the game spawns the package on both.
- **Size fit** (`fit`): `None`; `TargetScale` (inherit target scale); `TargetHeight(reference)`: measure the target's mesh AABB height once loaded, scale the instance by `height / reference`. *Verify* hanabi Local space applies the emitter's scale; if not, upload `scale` as a property and multiply sizes/positions/velocities in produce().
- `simulation_space` per particle layer (`EffectAsset::with_simulation_space`, `bevy_hanabi-0.19.0/src/asset.rs:443`).
- Lifecycle: `Starting → Running → Stopping → Finished`. Stop: `EffectSpawner.active = false` on particle layers (`spawn.rs:694`), fade the other layers over `fade.out`, despawn after `max(particle lifetime, fades, trail length)`, send `VfxFinished { instance, target }`. `restart()` while Stopping → Running.
- **Hot reload**: when a package asset changes (Bevy file watcher), running instances rebuild their layers and keep their parameter values. Lets the user tune in the editor and see it in the running game.
- Legacy: `EffectComplex` keeps working; `spawn` returns `Vec<Entity>`; abrupt despawn replaced by graceful stop.

**Runtime API**
```rust
app.add_plugins((HanabiPlugin, OmagariPlugin));

let pkg: Handle<VfxPackage> = asset_server.load("vfx/poison.vfx.ron");
let inst: Entity = commands.spawn_vfx(&pkg, target);
let inst = commands.spawn_vfx_with(&pkg, target, VfxSpawn { params: &[("intensity", 0.3.into())], ..default() });
let inst = commands.spawn_vfx_at(&pkg, Transform::from_xyz(..));        // one-shot in the world, auto-cleanup

fn system(mut vfx: VfxControl) {
    vfx.set_param(inst, "intensity", 0.8);          // smoothed
    vfx.set_param_now(inst, "tint", Color::srgb(0.2, 0.9, 1.0));
    vfx.trigger(inst, "tick");
    vfx.trigger_at(inst, "hit", TriggerArgs { position: Some(p), direction: Some(d), strength: 1.0 });
    vfx.stop(inst);  vfx.restart(inst);  vfx.is_alive(inst);
}
// message VfxFinished { instance, target }
```
Unknown parameter/trigger names: warn once per (package, name), never panic.

**Editor**: preview uses this API. Preview options: motion (circle / back-and-forth at a set speed), auto triggers.

**MCP**: `analyze_project` reports simulation space and warns for `Global` + looping + long lifetime (trail length estimate = speed × lifetime; speed from `preview.motion` or 3.6 m/s).

**Tests** (MinimalPlugins, no render): spawn → stop → `VfxFinished` once after the expected time; restart during Stopping; target despawn cleans up; socket resolution on a hand-built hierarchy; own-model-only mesh collection; fit scaling math.

**AC**: a looping Local package on a moving entity stays around it with no trail; `stop` fades, nothing pops, `VfxFinished` once; several instances on one target are independent; editing the package file updates the running instance; old `.hanabi.ron` files still load.

### F2. Parameters and triggers

**Parameters**
- Declared per package: name (free), kind (`Float` | `Color`), default, range (Float), smoothing. Any number per package. The plugin has no built-in names.
- Per instance: `current`, `target`. Each frame `current += (target - current) * (1 - exp(-dt / smoothing))`; `smoothing = 0` = immediate; clamp Float to `[min, max]`. Colors smooth per channel.
- **Particle expressions** (`format/particles.rs` expression model):
  - `Param(name)` → hanabi property (`ExprWriter::add_property` / `prop`, `bevy_hanabi-0.19.0/src/graph/expr.rs:2438,2505`), declared once per effect asset for every parameter it uses. Float → f32 property, Color → vec4.
  - New operators `Mix`, `Clamp`, `SmoothStep`, `Min`, `Max` (full table in F9).
  - Runtime uploads `current` values each frame with `EffectProperties::set` (`properties.rs:319`) on each particle entity of the instance (only when changed).
  - Init-time values read the property at spawn, update-time values every frame.
  - Parameter-driven colour: init `hdr_color` from `Mix(a, b, Param)` or the `Color` param directly, plus `ColorOverLifetime` with Modulate-style white keys for the alpha shape; or `SetColorModifier` (render) from an expression.
- **Spawn rate**: `rate: Binding<f32>` on CPU → `EffectSpawner.settings.set_count` (`spawn.rs:483`). Rate 0 ⇒ layer off.
- **Visibility ranges**: per layer `visible: (param, from, to, fade)` multiplies alpha / rate / intensity → tier looks that crossfade.
- Surface / shape / trail / afterimage / light / distortion values: `Binding<T>` evaluated on CPU each frame into material uniforms / components.

**Triggers**
- Declared per package: free names, any number. Effects of a trigger:
  - layers with `play: OnTrigger(name)` burst (`EffectSpawner::reset()` + active, *verify* `spawn.rs:814`);
  - parameter pulses: `current` of a parameter jumps to `to`, then returns to its target over `decay` seconds;
  - trail/afterimage `emit: Trigger(start, stop)` start/stop emitting.
- **Payload** (`trigger_at`): optional world `position`, `direction`, `strength`. Written into hanabi properties `trigger_pos` / `trigger_dir` / `trigger_strength` (layer space) before the burst, so a burst layer can spawn at the contact point and fly along the direction. Without payload: the layer's attach point and +Y.

**Editor UI**: **Parameters** panel: add/remove/rename parameters, kind, range, smoothing; a slider (Float) or colour picker (Color) per parameter driving the preview; "animate min→max→min" button. **Triggers** panel: add/remove, pulses, a Fire button per trigger, "auto-fire every N s", "fire at random point near target" for payload preview. Binding editor on every bindable value: toggle "constant / curve over parameter" + key list.

**MCP**: `preview_in_editor(name, stage, camera, params: {...}, trigger, motion)`; `analyze_project(params: [...sets...])` simulates at several parameter sets (default: every Float param at min, middle, max) and reports differences. The CPU evaluator learns `Param` and the new operators.

**Tests**: smoothing math; binding evaluation; Color params; property declaration in produced assets; shader generation with properties (`EffectShaderSources::generate` check); trigger pulses; payload property upload.

**AC**: dragging a parameter slider in the editor changes the effect smoothly; the game's `set_param` gives the same result; a trigger produces a visible pulse/burst; `trigger_at` spawns the burst at the given point.

### F3. Asset library and texture import
See 4.3. **AC**: a PNG imported by button, by drag & drop, and by MCP is copied into the game folder, appears in the Assets panel with thumbnail and channel view, is used by a particle layer and by a surface layer (with a chosen channel), and the exported package loads it in the game without the game passing a texture list; "Extract channel" produces a usable grayscale texture.

### F4. Surface layers (effect shader over the model, with volume)

**Approach: overlay copies.** For every matching mesh entity of the target's own model, spawn a sibling entity with the same `Mesh3d`, the same `SkinnedMesh` (shares joints → animates identically), and `MeshMaterial3d<OmagariEffectMaterial>`. Original materials untouched; layers stack; removal = despawn overlays. When meshes under the target appear or change (scene finished loading, equipment changed), overlays are updated.

**Shells (volume)**: a surface layer has 1..N shells; each shell is an overlay pushed outward along the normals by `offset` metres, with its own opacity/noise offset. One shell at 0 = coloured skin; 2–3 shells at 1–4 cm = a thick soft aura. Small offset also prevents z-fighting.

**Own vertex shader (required in phase 1).** Bevy's default mesh vertex shader passes only the animated (skinned) world position to the fragment shader. Patterns based on it slide over a moving body. Our vertex shader:
- does the normal skinning/morph path (reuse `bevy_pbr::skinning` / `mesh_functions`, *verify* imports in `bevy_pbr-0.19.1/src/render/mesh.wgsl`);
- passes the **rest-pose (pre-skinning) local position and normal** to the fragment shader, so triplanar noise sticks to the body;
- applies `offset` along the skinned normal (shells / inflate), scaled by fit.
Shader code is kept small and isolated in `src/shaders/`; it will need updates on Bevy upgrades (accepted by the user).

**Material `OmagariEffectMaterial`** (shared by surface, shape, trail, afterimage): `#[derive(Asset, AsBindGroup, TypePath, Clone)]`, one uniform block + up to 2 noise textures + 1 mask texture, each with a channel selector. `impl Material`: own `vertex_shader()` + `fragment_shader()`, `alpha_mode()` Blend or Add, `depth_bias()` small positive (tune for the orthographic camera), shadows and prepass off. One material asset per instance per layer (per shell), uniforms updated from bindings each frame.

Shader features (all bindable to parameters, each can be zero):
- colour (RGBA, HDR), blend Alpha | Add;
- 1–2 scrolling noise textures, mapping `Uv(scale)` | `Triplanar(scale)` (rest-pose space);
- rim / fresnel (colour, power, intensity);
- emissive multiplier (bloom);
- pulse (frequency, amount) + `flash` from trigger pulses;
- dissolve (threshold vs noise, glowing edge width) for appear/disappear;
- mask: object-space height gradient with `fill` (e.g. frost rising from the feet, energy filling a blade) and/or mask texture;
- fade in/out and layer visibility.

**Editor**: Surface layer panel with all values + bindings, mesh target picker (list of the preview model's meshes/materials), shell list; preview on the Model stage or Capsule fallback.
**MCP**: `check_project` validates targets against the preview model; `analyze_project` describes the layer in words (colour at parameter min/max, rim, scroll speed, pulse rate, opacity, blend, shells) and warns about opaque overlays hiding the model.
**Tests**: uniform packing (size/alignment), binding → uniform mapping, mesh matching / own-model rule, WGSL validation with naga in a unit test if feasible (*verify* `naga_oil` composition of Bevy imports), otherwise an editor smoke test.
**AC**: an animated character shows a scrolling surface pattern that sticks to the body (does not slide) and a glowing rim, identical in editor and game; two surface layers stack; a 3-shell layer looks thick; stop fades it out.

### F4b. Shape layers
- A built-in shape mesh with `OmagariEffectMaterial`: `Sphere(radius)`, `Capsule(radius, length)`, `Cylinder(radius, height, open_ends)`, `Ring(inner, outer)` (flat), `Torus(major, minor)`, `Cone(radius, height)`, `Quad(size)`, `Arc(radius, width, angle)` (stylized slash). Later: mesh asset from the library.
- Attach like other layers; follow mode `Full | Position | PositionYaw` (a ground ring stays flat).
- Animated transform: `scale`, `rotation_speed` as bindings (incl. over instance age for one-shots: `age` input, see below); `duration` for one-shot shapes (shockwave grows and fades).
- Bindings get one extra input besides parameters: **`Age`** (seconds since the layer started), so one-shot shapes can animate without code.
- **AC**: shield bubble around a character, ground ring under it that stays flat while it moves, a one-shot expanding shockwave, an arc slash played by a trigger.

### F5. Model, animation and socket preview (editor)
- Stage **Model**: loads `preview.model` (`.glb/.gltf` from the game folder; later via the asset library) with `SceneRoot(GltfAssetLabel::Scene(0))`. Stage **Capsule** (0.7 × 1.8, origin at the centre) when no model is set.
- Animations: `Gltf::named_animations` (*verify*) → `AnimationGraph` with one clip, `AnimationPlayer` with loop, speed, pause, scrub. Clips may come from another `.glb` with the same skeleton (Mixamo animations are separate files in the user's `animation/` folder; *verify* no retargeting needed when bone names match).
- Hierarchy browser: tree of nodes/bones/meshes/materials; pick a node as `preview.attach_to`, as a socket node, or as segment endpoints.
- **Socket editor**: package sockets (name + optional node + offset) placed with a gizmo in the viewport, so weapon points (base, tip) need no change to the weapon file.
- Weapon preview: a second model attached to a chosen bone of the first (e.g. sword in the right hand), each can be a target.
- Segment emitter (particles): `SetPositionSegment(a, b, radius)` = `Mix(segment_a, segment_b, rand) + random disc offset`.
- Preview motion (F1) while the animation plays.
- MCP: `preview_in_editor(..., stage: "model")`; `list_assets`; `describe_model(path)` → node/bone/mesh/material and animation names.
- Licensing: do not commit Mixamo models to this repo; reference them from the game folder.
- **AC**: a character runs in the editor with a package attached; a weapon package follows a swing animation; segment particles spawn along the blade.

### F5b. Trail layer (weapon swing trail)
- Two points (sockets `from`, `to`, e.g. blade base and tip) sampled every frame; samples older than `length` seconds are dropped.
- **Smoothing**: Catmull-Rom subdivision between samples (configurable `subdivisions`, default 4) so fast swings stay round.
- CPU-built ribbon mesh (two vertices per sample: base, tip), updated each frame (`Mesh` asset in `RENDER_WORLD | MAIN_WORLD` usage *verify*). UV: u = normalized age (0 new → 1 old), v = base → tip.
- Material `OmagariEffectMaterial` with `age` fade (opacity/colour curve over u), width falloff, additive glow, scrolling noise along u.
- Emission control: `emit: Always | Trigger(start, stop) | While(param, threshold)`. When emission stops, the remaining trail fades out over `length`.
- Optional sparks along the trail: a particle layer with `Segment` attach and `play: While(..)`.
- Built in world space (trails must stay in the world).
- **AC**: during a swing animation in the editor, a smooth glowing arc follows the blade and fades within `length`; nothing is drawn between swings; same in the game.

### F5c. Afterimage layer
- While emitting, every `interval` seconds snapshot the target model's meshes (rigid meshes: current global transform; skinned meshes: *verify* cost of copying joint matrices, or restrict v1 to rigid meshes like weapons) and spawn a ghost copy with `OmagariEffectMaterial`, fading over `lifetime`.
- Same `emit` control as trails. Cap on simultaneous ghosts.
- **AC**: a swinging sword leaves 4–6 fading ghost copies along its path.

### F6. Distortion layers (heat haze)
- Transmission pass: a material reads `view_transmission_texture` (`bevy_pbr-0.19.1/src/render/mesh_view_bindings.wgsl:102`) when `Material::reads_view_transmission_texture()` is true (`material.rs:186`).
- `OmagariDistortionMaterial`: samples it at `screen_uv + noise_offset * strength`, soft edge falloff, optional tint; drawn on a shape volume (capsule along a segment, sphere) or an overlay of a mesh.
- Camera: transmission enabled (*verify* component/field in `bevy_core_pipeline-0.19.1`), `Hdr`. **Spike first with the game's orthographic camera.** Fallback: "shimmer" particle recipe.
- Limitation: transparent hanabi particles are not distorted.
- **AC**: at full strength the air around the attach point visibly wobbles in editor and game; no cost when strength is 0.

### F7. Light layers
- `PointLight` child at the attach point; `color`, `intensity`, `range` bindings; optional flicker; fades with the instance; shadows off by default; global cap on Omagari lights (oldest/weakest dropped).
- **AC**: light grows with its parameter; fades out on stop.

### F8. MCP and guide (every phase)
- MCP tools accept v2 (and v1, migrated). `write_project` validates v2.
- New/changed tools: `import_asset`, `list_assets`, `extract_channel`, `describe_model`, `preview_in_editor` (+ `stage`, `camera`, `params`, `trigger`, `motion`), `analyze_project` (+ parameter sets).
- Preview request file becomes a small JSON object (name, stage, camera, params, trigger, motion); the old two-line form is still accepted.
- Guide `docs/CLAUDE_VFX_GUIDE.md`: sections `parameters`, `triggers`, `assets`, `attach`, `surfaces`, `shapes`, `trails`, `afterimages`, `lights`, `distortion`; recipes (tested, zero warnings): status aura (particles + surface), slow-style cue, intensity progression 0→1, shield bubble, ground ring, slash trail, afterimages, hit burst at contact point.
- Human docs: `README.md` (game integration with the new API), `docs/MCP.md`.

### F9. bevy_hanabi coverage (all 3D features)

Inventory from `bevy_hanabi-0.19.0/src` (`modifier/*.rs`, `asset.rs`, `graph/expr.rs`) and `examples/`. "Now" = exposed today. Every row except "skip" is implemented; order is P2b first, then P3b for items needing the model preview.

**Modifiers**

| hanabi | Kind | Now | Phase | Notes / use |
|---|---|---|---|---|
| `SetAttributeModifier`, `InheritAttributeModifier` | init | yes | – | |
| `SetPositionSphereModifier`, `SetPositionCircleModifier` | init | yes | – | |
| `SetPositionCone3dModifier` | init | no | P2b | cones: fountains, thrusts |
| `SetVelocitySphere/Circle/TangentModifier` | init | yes | – | |
| `AccelModifier`, `LinearDragModifier`, `ConformToSphereModifier` | update | yes | – | |
| `RadialAccelModifier` | update | no | P2b | attract/repel: vortex, implosion |
| `TangentAccelModifier` | update | no | P2b | swirl: auras, tornado |
| `KillSphereModifier`, `KillAabbModifier` | update | no | P2b | kill inside/outside a volume |
| `EmitSpawnEventModifier` | update | yes | – | |
| `ParticleTextureModifier` | render | fixed | P2b | expose `ImageSampleMapping` |
| `SetColorModifier`, `SetSizeModifier` | render | no | P2b | render-time colour/size from expressions (parameter-driven) |
| `ColorOverLifetimeModifier`, `SizeOverLifetimeModifier` | render | yes | – | + gradient options if any |
| `OrientModifier` | render | fixed | P2b | all `OrientMode`s + rotation expression (*verify* field) |
| `FlipbookModifier` | render | no | P2b | sprite sheets; uses texture `grid` (F3) + `sprite_index` |
| `RoundModifier` | render | no | P2b | rounded quads |
| `ScreenSpaceSizeModifier` | render | no | P2b | constant pixel size |
| Ribbons (`Attribute::RIBBON_ID`, `prev`/`next`, `examples/ribbon.rs`) | init + render | no | P3b | magic streaks; needs `MotionIntegration::None`; editor preset |
| Mesh particles (`EffectAsset::mesh`, `asset.rs:335,643`) | asset | no | P3b | debris, shards, leaves; mesh from the asset library |
| Any other modifier found in the 0.19 sources | | | P2b/P3b | add, or list as skipped with a reason |

**Effect asset settings**

| Setting | Now | Phase | Notes |
|---|---|---|---|
| `simulation_space` | fixed Global | P1 | per layer |
| `alpha_mode` (`Blend`, `Premultiply`, `Add`, `Multiply`, mask *verify*) | fixed Blend | P2b | Add = additive glow |
| `motion_integration` | default | P2b | `None` for ribbons / expression-positioned particles |
| `simulation_condition` | default | P2b | `Always` for off-screen simulation |
| `prng_seed` | default | P2b | reproducible previews/analysis |
| spawner `once` / `rate` / `burst`, `Uniform` counts | partial | P2b | presets + keep ranges |
| draw ordering between effects (`examples/ordering.rs`) | no | P3b | stable layering |
| `z_layer_2d`, 2D rendering, render layers / multi-camera | – | skip | 2D / not needed |

**Expressions**: replace the hand-picked operator list with a **table-driven** model over hanabi's `BuiltInOperator`, `UnaryOperator`, `BinaryOperator`, `TernaryOperator` (+ attributes, properties, literals of every `Value` type). New operator = one table row with name, arity, type rule, CPU evaluator.

**Coverage test**: a unit test lists hanabi's modifiers (reflection registry after `HanabiPlugin`'s `register_modifiers()`) and operator enums and fails if Omagari has no mapping and the item is not in an explicit "skipped" list. Updating hanabi then surfaces new features as a failing test.

**Rules**
- One pattern per modifier: data struct in `src/format/particles.rs`, UI in `src/editor/`, produce arm. Formerly hard-coded render settings become normal removable modifiers with defaults reproducing the old look.
- Round-trip test per modifier: build a project using it, save, load through the runtime loader, produce, compare.
- MCP analysis: the CPU simulator simulates every update modifier and operator, or reports the layer as "not simulated" explicitly (never silently wrong). Render-only features are described, not simulated.
- Guide: every feature gets a line; tested recipes use additive blending, a flipbook, TangentAccel/RadialAccel, a cone, a ribbon.

**AC**: every non-skipped item can be added, edited, saved, loaded in the game and previewed in the editor; `check_project`/`analyze_project` handle it; coverage test passes.

### F10. Editor UX
Follow the current egui style; use common app practice:
- **Layout**: top toolbar (project, save/export, stage, camera, backdrop); left: **Layers** list (add/duplicate/delete/reorder, visibility toggle per layer); right: **Inspector** for the selected layer; tabs/panels for **Parameters**, **Triggers**, **Assets**, **Sockets**; bottom or overlay: playback (play/stop/restart instance, time scale, model animation controls).
- **Live preview**: edits rebuild the preview instance automatically (debounced ~150 ms); RENDER button stays as a manual refresh.
- **Undo/redo**: snapshot-based (serialize project on each committed edit; Ctrl+Z / Ctrl+Y), at least 100 steps.
- **Tooltips** in plain words on every value; units shown (m, s, m/s).
- **Shortcuts**: Ctrl+S save, Ctrl+E export, Ctrl+Z/Y, Delete, Ctrl+D duplicate, F focus camera.
- **Dirty state**: unsaved marker, confirm before closing/loading with unsaved changes.
- **Errors**: validation from `check_project` shown in the UI next to the field, not only in the MCP output.
- **New from recipe**: create a package from a guide recipe.
- **Camera presets** (render profile in `omagari_settings.ron`): `Orbit` (current) and `Game` (projection, orthographic height, yaw/pitch, tonemapping, bloom, exposure, background). Defaults for `Game` match the game camera (orthographic, true isometric 45° / 35.26°, ~16 units visible height, `Hdr`, bloom).

---

## 6. Game integration (game agent, after user confirmation)

Facts provided by the user about the game (game repo, likely `C:\bevy\justForFun`, *confirm with the user*):
- Player capsule radius 0.35, height 1.5 (`player/mod.rs:56-59`); visible model Mixamo Y Bot ≈ 1.8 m; root 0.9 above the feet (`player_rig.rs:19`); metres, Y up.
- Camera: orthographic true-isometric (45° yaw, ≈35.26° pitch), offset (20,20,20), ≈16 world units visible vertically (`world/camera.rs:31-34`). Effects must read at ~1/10 screen height per character.
- Statuses (example usage): Poison 4 s, damage every 1 s, re-contact resets the timer, poison zone (2.5 m, 8 s) re-applies every 0.5 s; Slow half speed 3 s, own cyan cue; Ignite orange, rises. Placeholder poison colour `srgb(0.65, 0.08, 0.95)` (`combat_periodic_status/visual.rs:20`). Enemies can be affected.
- Movement: walk 7.2 m/s, dodge 16 m/s.
- Weapons are separate models with their own effects.
- The game uses bevy_hanabi 0.19 and Bevy 0.19.1.

Integration pattern (game-side code, not in Omagari):
- Add `OmagariPlugin` + `HanabiPlugin`; camera `Hdr` + `Bloom` (+ transmission if distortion is used).
- Assets: `assets/vfx/*.vfx.ron`, `assets/vfx/textures/*`; Omagari's game assets folder points at the game's `assets/`.
- Status pattern: store the instance per (target, status); on apply: spawn, or `restart` if Stopping; on damage tick: `trigger("tick")`; on expire: `stop`. Same code for player and enemies.
- Progression pattern: the game maps its progress to whatever parameters the package declares (`set_param`), smoothing animates changes.
- Weapon pattern: package on the weapon entity; `trigger("swing_start")` / `trigger("swing_end")` from the attack state for trails/afterimages; `trigger_at("hit", contact)` on contact.

---

## 7. Roadmap

Each phase lands as its own commit series on branch `vfx-system`, with `cargo build --release --features editor` and `cargo test --release --features editor` passing, and a demo package in `saved_effects/` or `examples/`. Guide/MCP/docs are updated in every phase (F8).

| Phase | Content | Depends on | Done when |
|---|---|---|---|
| **P0a Data model split** | move the particle data model out of the editor into `src/format/` (serde, no egui); UI impls into `src/editor/`; `produce()` in the library; delete `src/complex.rs`. **No behaviour change.** | – | build + tests pass; editor and MCP behave as before; produced assets identical for all examples |
| **P0b Format v2 + runtime slice** | ProjectV2 (`#[serde(default)]`), v1 migration, package loader (`.vfx.ron` / `.omagari.ron`), `game` asset source + settings, minimal instance (particles only, attach Target, Local space, stop, `VfxFinished`), editor saves v2 and previews through the runtime | P0a | AC of 4.5; a package spawned by a test app follows a moving entity and stops |
| **P1 Instances complete** | rest of F1 (sockets, segments, fit, restart, hot reload, `spawn_vfx_at`, legacy `EffectComplex` returns entities + graceful stop), preview motion | P0b | AC of F1 |
| **P1b Assets + UX base** | F3 asset library + import (dialog, drag & drop, MCP, channel view, extract channel); F10 layout, live preview, undo/redo, dirty state, camera presets | P0b | AC of F3; F10 basics usable |
| **P2 Parameters & triggers** | F2 complete (runtime, expressions, panels, MCP, guide) | P1 | AC of F2 |
| **P2b hanabi coverage** | F9 rows marked P2b, expression table, coverage test | P2 | AC of F9 for P2b rows |
| **P3 Model preview** | F5 (glTF, animations, hierarchy, sockets editor, weapon preview, segment emitter, `describe_model`) | P1 (can run parallel to P2/P2b) | AC of F5 |
| **P3b hanabi coverage 2** | ribbons, mesh particles, draw ordering | P2b, P3 | ribbon recipe works on the animated model |
| **P4 Effect shader** | F4 surface layers (own vertex shader, shells) + F4b shape layers | P2, P3 | AC of F4 and F4b |
| **P5 Trails + afterimages** | F5b, F5c | P4 | AC of F5b, F5c |
| **P6 Lights** | F7 | P4 | AC of F7 |
| **P7 Distortion** | F6 (spike with ortho camera first) | P4 | AC of F6 |
| **G1..G3 Game** | integrate after P2, P4, P5 (user confirms repo and scope first) | | 6.x patterns working in the game |

### Verification per phase
- `cargo build --release --features editor` and `cargo test --release --features editor`.
- MCP: `check_project`, `analyze_project` on the phase demo.
- Visual checks are done by the user in the editor (agents cannot see the screen); each phase lists what to look at.

---

## 8. Risks and open questions

**Risks**
- **Bevy upgrades** break custom materials/WGSL (F4–F7). Accepted; shaders kept small and isolated, smoke test per phase.
- **Own vertex shader** must follow Bevy's mesh/skinning shader closely; verify early in P4 with a skinned Mixamo model.
- **hanabi limits**: gradients cannot use properties (use init colours/SetColor), spawn count is CPU-side, particles can't pick texture channels (extract channel), no distortion in particles.
- **Transmission + orthographic camera** (P7): spike first, shimmer fallback.
- **Overlay sorting / z-fighting** with the orthographic camera: depth bias + shell offset.
- **Trail mesh updates** every frame: keep vertex counts small (≤ 200 per trail).
- **glTF names** (sockets, targets): confirm Bevy 0.19 naming in P3.
- **Performance**: one draw per matched mesh per shell per surface layer per instance; measure with many enemies.

**Open questions (ask the user when the phase starts)**
1. Game repo path and when to start game integration (G phases).
2. Afterimages on skinned (animated) meshes, or rigid meshes (weapons) only?
3. Should statuses fade out on death or stop instantly? (game side)
