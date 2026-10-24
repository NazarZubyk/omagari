# Using Omagari effects in your game

This page shows how to play effects made in the Omagari editor in your own Bevy 0.19 game. The `omagari` crate (the library, without the `editor` feature) loads the editor's exported **packages** (`*.vfx.ron`) and plays them on any entity.

A complete, runnable program is in [`examples/game_integration.rs`](../examples/game_integration.rs). It needs no model files: a capsule "hero" runs in a circle, swings a box "sword", gets poisoned, has a shield, makes ripples and hit flashes:

```text
cargo run --release --example game_integration
```

Its packages are in [`assets/vfx/`](../assets/vfx/), exported from the projects of the same names in `saved_effects/` (and `magicburst` from `examples/`).

- [Words used on this page](#words-used-on-this-page)
- [1. Add the dependency](#1-add-the-dependency)
- [2. Add the plugins](#2-add-the-plugins)
- [3. Set up the camera](#3-set-up-the-camera)
- [4. Put the packages in your assets folder](#4-put-the-packages-in-your-assets-folder)
- [5. Load and play a package](#5-load-and-play-a-package)
- [6. Control a running effect: `VfxControl`](#6-control-a-running-effect-vfxcontrol)
- [7. Know when an effect has ended: `VfxFinished`](#7-know-when-an-effect-has-ended-vfxfinished)
- [8. Patterns](#8-patterns)
- [9. Sockets on your own models](#9-sockets-on-your-own-models)
- [10. Size fit](#10-size-fit)
- [11. Lights: the global cap](#11-lights-the-global-cap)
- [12. Textures and meshes](#12-textures-and-meshes)
- [13. Hot reload](#13-hot-reload)
- [14. Legacy `*.hanabi.ron` files](#14-legacy-hanabiron-files)
- [15. Limits and troubleshooting](#15-limits-and-troubleshooting)

## Words used on this page

- **Package**: one effect, as one file (`<name>.vfx.ron`). The editor's EXPORT writes it. It holds layers, parameters, triggers, sockets and the list of textures and meshes it uses.
- **Layer**: one visual part of a package: particles, a surface skin over a model, a 3D shape, a trail, afterimages, a point light or a distortion.
- **Instance**: one playing copy of a package, attached to one entity (its **target**). A target can have several instances.
- **Parameter**: a named number or colour that your game sets on an instance (for example `intensity` from 0 to 1). The package author picks the names.
- **Trigger**: a named event that your game fires on an instance (for example `hit`). Layers can burst, start or stop on it.
- **Socket**: a named point on the target, such as a hand bone or the tip of a blade.
- **HDR** (high dynamic range) and **bloom**: colours brighter than 1.0 only glow if the camera renders in HDR and has bloom.

## 1. Add the dependency

Omagari is not on crates.io. Add it from git (or from a local path) **without** the `editor` feature:

```toml
[dependencies]
bevy = "0.19.1"
bevy_hanabi = "0.19"
omagari = { git = "https://github.com/NazarZubyk/omagari", branch = "master" }
# or, with a local clone:
# omagari = { path = "../omagari" }
```

`bevy_hanabi` must be the same version Omagari uses (0.19), or Bevy sees two different `HanabiPlugin`s. Omagari's library has no other heavy dependencies (no egui, no file dialogs).

## 2. Add the plugins

Add Bevy's default plugins first, then `HanabiPlugin`, then `OmagariPlugin`:

```rust
use bevy::prelude::*;
use bevy_hanabi::prelude::*;

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, HanabiPlugin, omagari::OmagariPlugin))
        .run();
}
```

`OmagariPlugin` registers the package loader (`*.vfx.ron` and `*.omagari.ron`), the built-in textures, the effect and distortion materials (their shaders are embedded, you add no shader files) and the systems that run instances.

## 3. Set up the camera

Effects use HDR colours (values above 1.0) for glow. The camera needs `Hdr` and `Bloom` for them to glow; without them bright colours are just clipped to white.

**Distortion layers** (heat haze, shockwave ripples) also need Bevy's screen-space transmission: a copy of the screen that the distortion bends. Every `Camera3d` in Bevy 0.19 already has `ScreenSpaceTransmission { steps: 1, .. }`; keep `steps` above 0. `omagari::distortion_camera()` adds both `Hdr` and the transmission settings:

```rust
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;

fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        Camera3d::default(),
        omagari::distortion_camera(), // Hdr + ScreenSpaceTransmission
        Bloom::NATURAL,
        Transform::from_xyz(0.0, 8.0, 12.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}
```

> **Gotcha:** `distortion_camera()` already contains `Hdr`. Do not add `Hdr` again in the same bundle: Bevy panics on a bundle with the same component twice. If you don't use distortion, add `bevy::camera::Hdr` yourself instead.

Orthographic and perspective cameras both work. An isometric camera like the editor's default GAME preset:

```rust
use bevy::camera::ScalingMode;
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;

fn spawn_isometric_camera(mut commands: Commands) {
    commands.spawn((
        Camera3d::default(),
        omagari::distortion_camera(),
        Projection::from(OrthographicProjection {
            scaling_mode: ScalingMode::FixedVertical { viewport_height: 16.0 },
            ..OrthographicProjection::default_3d()
        }),
        Bloom::NATURAL,
        Transform::from_xyz(20.0, 20.0, 20.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}
```

Set the editor's GAME camera to the same values (⚙ SETTINGS → Game), so what you judge in the editor is what the game shows. See [EDITOR.md](EDITOR.md#cameras).

## 4. Put the packages in your assets folder

The editor's EXPORT writes `<name>.vfx.ron` into `<game assets>/vfx/` when you have told it where your game's `assets/` folder is (see [GETTING_STARTED.md](GETTING_STARTED.md#3-point-omagari-at-your-game)). Without that setting it writes to `exported_effects/` in the Omagari folder; copy the file into your game's `assets/` yourself.

- **Built-in textures** (`omagari://textures/...`) are inside the library. Nothing to copy.
- **Your own textures** are paths relative to your `assets/` folder, for example `vfx/textures/smoke.png`. When you import a file in the editor with the game folder set, it is copied there for you.
- **Mesh assets** (meshes of a `.glb` for mesh particles) are also paths in your `assets/` folder.

A game can also load the editor's project file (`*.omagari.ron`) directly; the editor-only `preview` block is ignored.

## 5. Load and play a package

Load a package like any Bevy asset and keep the handle. Spawning an instance before the package has loaded is fine: its layers appear once the package and its textures are ready.

```rust
use bevy::prelude::*;
use omagari::{VfxCommandsExt, VfxPackage, VfxSpawn};

#[derive(Resource)]
struct Effects {
    aura: Handle<VfxPackage>,
    explosion: Handle<VfxPackage>,
}

fn load_effects(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.insert_resource(Effects {
        aura: asset_server.load("vfx/aura.vfx.ron"),
        explosion: asset_server.load("vfx/explosion.vfx.ron"),
    });
}

#[derive(Component)]
struct Player;

fn play(mut commands: Commands, effects: Res<Effects>, player: Single<Entity, Added<Player>>) {
    // On an entity: the instance is a child of `player` and follows it.
    let _aura: Entity = commands.spawn_vfx(&effects.aura, *player);

    // With starting parameter values (set at once, not smoothed).
    let _tinted: Entity = commands.spawn_vfx_with(
        &effects.aura,
        *player,
        VfxSpawn { params: &[("intensity", 0.3.into()), ("tint", Color::srgb(0.2, 0.9, 1.0).into())] },
    );

    // In the world, with no target: a one-shot explosion.
    let _boom: Entity = commands.spawn_vfx_at(&effects.explosion, Transform::from_xyz(2.0, 0.0, -1.0));
}
```

| Function | What it does |
| :-- | :-- |
| `spawn_vfx(&package, target)` | Spawns an instance as a child of `target`. Returns the instance entity. |
| `spawn_vfx_with(&package, target, VfxSpawn { params })` | The same, with starting parameter values. |
| `spawn_vfx_at(&package, transform)` | Spawns an instance in the world, with no target. |
| `spawn_vfx_at_with(&package, transform, VfxSpawn { params })` | The same, with starting parameter values. |

**How long an instance lives.** A package whose layers all play once (particles with `play: OnStart` and a finite cycle count; shapes, lights and distortions with `play: OnStart` and a duration) **finishes by itself**: after its last particle has died, the instance is despawned. That is why `spawn_vfx_at` one-shots need no cleanup code. Every other package (looping layers, trigger-driven layers, surfaces, trails, afterimages) runs until you **stop** it, or until its target is despawned.

Keep the returned `Entity` when you want to control the instance later. Despawning the instance entity yourself also works (it ends at once, without a fade).

## 6. Control a running effect: `VfxControl`

`VfxControl` is a system parameter. Every method takes the instance entity.

```rust
use bevy::prelude::*;
use omagari::{ParamValue, TriggerArgs, VfxControl, VfxState};

#[derive(Component)]
struct MyEffect(Entity);

fn control(effects: Query<&MyEffect>, mut vfx: VfxControl) {
    for &MyEffect(instance) in effects.iter() {
        // Parameters: the names are the ones the package declares.
        vfx.set_param(instance, "intensity", 0.8); // smoothed, clamped to the parameter's range
        vfx.set_param_now(instance, "intensity", 0.8); // no smoothing
        vfx.set_param(instance, "tint", Color::srgb(1.0, 0.4, 0.1)); // a colour parameter
        let _now: Option<ParamValue> = vfx.param(instance, "intensity"); // current (smoothed) value

        // Triggers.
        vfx.trigger(instance, "tick");
        vfx.trigger_at(
            instance,
            "hit",
            TriggerArgs::at(Vec3::new(0.0, 1.0, 0.5)).with_direction(Vec3::Z).with_strength(2.0),
        );

        // Lifecycle.
        let _alive: bool = vfx.is_alive(instance);
        let _state: Option<VfxState> = vfx.state(instance); // Starting / Running / Stopping / Finished
        if vfx.state(instance) == Some(VfxState::Running) {
            vfx.stop(instance); // graceful: no new particles, the live ones finish
        }
        vfx.restart(instance); // while stopping: play again on the same instance
    }
}
```

| Method | Notes |
| :-- | :-- |
| `set_param(instance, name, value)` | The value is a target; the current value follows it over the parameter's `smoothing` time. Numbers are clamped to the parameter's `min..max`. Values set before the package has loaded apply once it has. |
| `set_param_now(instance, name, value)` | Jumps without smoothing. |
| `param(instance, name)` | The current value, or `None` while the package is still loading. |
| `trigger(instance, name)` | Fires a trigger: `OnTrigger` layers burst, `Between` layers start or stop, pulses run. Fired while the instance is still loading, it waits; fired while it is stopping, it is ignored. |
| `trigger_at(instance, name, TriggerArgs)` | The same with a world position, a direction and a strength. Particle layers read them as the properties `trigger_pos`, `trigger_dir`, `trigger_strength`; `OnTrigger` shapes, lights and distortions play at the position. |
| `stop(instance)` | Stops gracefully: emission ends, layers fade out, then the instance is despawned and `VfxFinished` is sent. |
| `restart(instance)` | While stopping: plays again on the same instance. Does nothing to a running instance (returns `true`); returns `false` when it has finished. |
| `replace_package(instance, handle)` | Swaps the package and rebuilds the layers in place (old particles finish). |
| `is_alive(instance)`, `state(instance)` | Whether it exists and has not finished; its `VfxState`. |

A wrong name or a value of the wrong kind (a number for a colour parameter) is ignored with one warning per package and name. Nothing panics.

`Color` values become linear RGBA (`ParamValue::Color`), numbers become `ParamValue::Float`.

## 7. Know when an effect has ended: `VfxFinished`

Every instance sends one `VfxFinished` message when it ends: it was stopped and its particles died, it finished by itself, its target was despawned, or other code despawned it.

```rust
use bevy::prelude::*;
use omagari::VfxFinished;

fn on_finished(mut finished: MessageReader<VfxFinished>) {
    for f in finished.read() {
        // For spawn_vfx_at instances, `target` is the instance itself.
        info!("effect {} on {} has ended", f.instance, f.target);
    }
}
```

## 8. Patterns

The package only declares names; your game decides what they mean. These patterns are all in the example program.

### A status effect: apply, refresh, tick, expire

A poison status that lasts 4 s and deals damage every second. Keep the instance per target and status. On apply: spawn it, or `restart` it if it is still fading out. On each damage tick: fire `tick`. When it runs out: `stop`.

```rust
use bevy::prelude::*;
use omagari::{VfxCommandsExt, VfxControl, VfxPackage};

#[derive(Resource)]
struct StatusEffects {
    poison: Handle<VfxPackage>,
}

#[derive(Component, Default)]
struct Poisoned {
    remaining: f32,
    next_tick: f32,
    vfx: Option<Entity>,
}

/// Call this when the poison hits `target` (again).
fn apply_poison(commands: &mut Commands, vfx: &mut VfxControl, effects: &StatusEffects, target: Entity, status: &mut Poisoned) {
    status.remaining = 4.0;
    status.next_tick = 1.0;
    match status.vfx {
        // Still playing or fading out: refresh it.
        Some(instance) if vfx.is_alive(instance) => {
            vfx.restart(instance);
        }
        _ => status.vfx = Some(commands.spawn_vfx(&effects.poison, target)),
    }
}

fn update_poison(time: Res<Time>, mut statuses: Query<&mut Poisoned>, mut vfx: VfxControl) {
    for mut status in statuses.iter_mut() {
        let Some(instance) = status.vfx else { continue };
        if status.remaining <= 0.0 {
            continue;
        }
        status.remaining -= time.delta_secs();
        status.next_tick -= time.delta_secs();
        if status.next_tick <= 0.0 {
            status.next_tick += 1.0;
            vfx.trigger(instance, "tick"); // e.g. the skin flashes
        }
        if status.remaining <= 0.0 {
            vfx.stop(instance); // fades out, then VfxFinished
        }
    }
}
```

The same code works for the player and for enemies: the package goes on whatever entity you pass as the target.

### A progression parameter

Map any game value to a parameter every frame. Smoothing (set per parameter in the editor) animates the changes, so you can set it every frame or only when the value changes.

```rust
use bevy::prelude::*;
use omagari::VfxControl;

#[derive(Component)]
struct Charge {
    level: f32, // 0..1
    vfx: Entity,
}

fn charge_glow(charges: Query<&Charge, Changed<Charge>>, mut vfx: VfxControl) {
    for charge in charges.iter() {
        vfx.set_param(charge.vfx, "charge", charge.level);
    }
}
```

### A weapon swing trail

Weapons are separate models with their own effects. Spawn the weapon's package on the **weapon entity**, and fire its triggers from your attack state: trails and afterimages run between `swing_start` and `swing_end`, then fade out.

```rust
use bevy::prelude::*;
use omagari::VfxControl;

#[derive(Clone, Copy, PartialEq)]
enum AttackPhase {
    Idle,
    Swing,
    Recover,
}

#[derive(Component)]
struct Attack {
    phase: AttackPhase,
}

#[derive(Component)]
struct WeaponFx {
    instance: Entity,
}

fn swing_vfx(attacks: Query<(&Attack, &WeaponFx), Changed<Attack>>, mut vfx: VfxControl) {
    for (attack, fx) in attacks.iter() {
        match attack.phase {
            AttackPhase::Swing => {
                vfx.trigger(fx.instance, "swing_start");
            }
            AttackPhase::Recover => {
                vfx.trigger(fx.instance, "swing_end");
            }
            AttackPhase::Idle => {}
        }
    }
}
```

The trail is drawn between two sockets of the package (for example `blade_base` and `blade_tip`). In the example they are plain offsets from the sword entity's origin, so the sword needs no named bones.

### A hit burst

Two ways, depending on the package:

- **A one-shot package** (it plays once and ends): spawn it in the world at the hit point with `spawn_vfx_at`. It cleans up by itself.
- **A package with an `OnTrigger("hit")` layer**, kept on the character or weapon: fire it with `trigger_at` at the contact point. Particles read the point as `trigger_pos`; light flashes and shapes play there.

```rust
use bevy::prelude::*;
use omagari::{TriggerArgs, VfxCommandsExt, VfxControl, VfxPackage};

#[derive(Message)]
struct WeaponHit {
    point: Vec3,
    normal: Vec3,
    force: f32,
    weapon_vfx: Entity,
}

#[derive(Resource)]
struct HitEffects {
    sparks: Handle<VfxPackage>,
}

fn hit_vfx(mut commands: Commands, mut hits: MessageReader<WeaponHit>, effects: Res<HitEffects>, mut vfx: VfxControl) {
    for hit in hits.read() {
        // A one-shot in the world.
        commands.spawn_vfx_at(&effects.sparks, Transform::from_translation(hit.point));
        // Or a trigger on an instance that is already there.
        vfx.trigger_at(hit.weapon_vfx, "hit", TriggerArgs::at(hit.point).with_direction(hit.normal).with_strength(hit.force));
    }
}
```

(Register the message with `app.add_message::<WeaponHit>()`.)

## 9. Sockets on your own models

A **socket** is a named point on the target. In the package it is either:

- an **offset from the target's origin** (`node: None`), which works on any entity; or
- a **node** (an entity with that `Name` somewhere below the target, for example the bone `mixamorig:RightHand` of a Mixamo character) plus an offset in the node's space.

glTF scenes give every node a `Name`, so a socket on a bone works as soon as the character's scene has spawned under the target. Until the node exists, the layer sits at the target's origin; after 3 s without it, a warning is logged and the layer keeps trying. So a package made for a rig with other bone names still plays, just at the target's origin: rename the socket's node in the editor (SOCKETS & FIT tab) to your bone's name.

A layer attached to a socket name that no package socket has uses the entity with that `Name` directly.

**Separate models.** Surface layers, afterimages and the height fit look at the target's **own** meshes: every mesh below the target, except nested glTF scenes (a weapon scene spawned inside a hand bone) and entities marked `VfxSeparateModel`. Mark a weapon that you build from plain meshes, so a body effect does not cover it:

```rust
use bevy::prelude::*;
use omagari::VfxSeparateModel;

/// A sword made of plain meshes, held by `hand`.
fn spawn_sword(commands: &mut Commands, hand: Entity, blade: Handle<Mesh>, steel: Handle<StandardMaterial>) -> Entity {
    let sword = commands.spawn((Name::new("sword"), VfxSeparateModel, Transform::default(), Visibility::default(), ChildOf(hand))).id();
    commands.spawn((Mesh3d(blade), MeshMaterial3d(steel), Transform::from_xyz(0.0, 0.5, 0.0), ChildOf(sword)));
    sword
}
```

## 10. Size fit

A package's `fit` decides how instances scale with their target:

- `None` (default): the authored size, whatever the target's scale.
- `TargetScale`: the target's scale.
- `TargetHeight(1.8)`: scaled by the height of the target's own meshes ÷ 1.8 m, so an aura made for a 1.8 m character also fits a 4 m ogre. The height is measured once the meshes exist (it waits up to 3 s, then uses the target's scale).

Fit scales `Local` particle layers (bevy_hanabi applies only the position to `Global` ones), shapes, lights (range by the fit, intensity by fit²) and socket offsets.

## 11. Lights: the global cap

Light layers are Bevy `PointLight`s. Each one costs shading time, so at most `VfxSettings::max_lights` Omagari lights shine at once (default 8, all instances together). The strongest win; a light pushed out fades over `light_cap_fade` seconds instead of popping.

```rust
use bevy::prelude::*;

fn configure(app: &mut App) {
    app.insert_resource(omagari::VfxSettings { max_lights: 4, ..default() });
}
```

You can insert or change the resource before or after adding the plugin, or at any time. Lights only light lit materials (`StandardMaterial` and the like); `shadows: true` on a light layer renders six shadow maps per light.

## 12. Textures and meshes

- Each texture loads with the settings the package gives it (wrap, colour space, filter); your game passes nothing.
- **PNG** works with Bevy's default features. **JPG** needs Bevy's `jpeg` feature in your game: `bevy = { version = "0.19.1", features = ["jpeg"] }`. **KTX2** works with Bevy's `ktx2` feature (part of the default `3d` feature).
- **Mesh particles** with a mesh asset load `<file>.glb#Mesh<i>/Primitive<j>` through Bevy's glTF loader (part of `DefaultPlugins`). Ship the `.glb` with your game.
- A texture or mesh that does not exist logs Bevy's asset error; the layer plays without it.

## 13. Hot reload

With Bevy's file watcher on, editing a package file while the game runs rebuilds the running instances in place: old particles finish, new layers start, the instance entity, its parameter values and its state stay. Enable Bevy's `file_watcher` feature and turn watching on:

```rust
use bevy::prelude::*;

fn hot_reload_plugins() -> impl PluginGroup {
    DefaultPlugins.set(AssetPlugin { watch_for_changes_override: Some(true), ..default() })
}
```

Re-export from the editor (Ctrl+E) and the game updates.

## 14. Legacy `*.hanabi.ron` files

Files exported by older Omagari versions (`*.hanabi.ron`) still load as an `EffectComplex` asset:

```rust
use bevy::prelude::*;

fn legacy(asset_server: Res<AssetServer>) {
    let _handle: Handle<omagari::EffectComplex> = asset_server.load("effects/fireworks.hanabi.ron");
}
```

`EffectComplex::spawn(&mut commands, &textures, position, despawn_in)` spawns them and returns the entities; its textures are passed in the editor's old order. Files exported by Omagari 0.17 or older cannot be loaded (bevy_hanabi's format changed): open the project in the editor and export a package instead. New code should use packages.

## 15. Limits and troubleshooting

- **The effect is invisible or flat.** Check the camera (`Hdr` + `Bloom`), that the package path is right (Bevy logs a failed load), and that your own textures exist under `assets/`.
- **A distortion shows nothing.** It bends only **opaque** things behind it (floors, walls, models). On a flat, single-coloured background there is nothing to see. Transparent particles and Omagari's see-through layers are drawn after it and are not bent. A warning is logged once when a distortion plays but no active 3D camera has transmission (`steps: 0`).
- **Mesh particles look flat.** bevy_hanabi draws them unlit, one flat colour per particle.
- **A socket effect sits at the character's feet or centre.** The socket's node name is not in your model. Fix the name in the editor or in the package.
- **A surface skin also covers the weapon.** Mark the weapon `VfxSeparateModel` (or spawn it as its own glTF scene).
- **Too many lights.** Lower `VfxSettings::max_lights` or the number of light layers.
- **A package made with a newer editor** may have fields this library does not know: they are ignored, with one warning per load naming each field.
