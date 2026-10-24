# File format reference

Omagari stores an effect as a **project**, `saved_effects/<name>.omagari.ron`, and exports it as a **package**, `<name>.vfx.ron`, for games. Both are the same format (version 2) in [RON](https://github.com/ron-rs/ron) (Rusty Object Notation, a text format close to Rust syntax). A package is the project without its editor-only `preview` block. Games load either.

This page lists every field with its default. It is derived from the serde types in [`src/format/`](../src/format/). For the bevy_hanabi parts of particle layers (spawner, modifiers, expressions) the authoring guide [CLAUDE_VFX_GUIDE.md](CLAUDE_VFX_GUIDE.md) has the details and tested recipes.

- [General rules](#general-rules)
- [Top level](#top-level)
- [Parameters](#parameters)
- [Triggers](#triggers)
- [Bindings: `Const`, `Param`, `Age`](#bindings-const-param-age)
- [Assets](#assets)
- [Sockets and attach](#sockets-and-attach)
- [Fit](#fit)
- [Play modes](#play-modes)
- [Shared parts: `visible`, `fade`, `material`](#shared-parts-visible-fade-material)
- [Layers](#layers): [Particles](#particles), [Surface](#surface), [Shape](#shape), [Trail](#trail), [Afterimage](#afterimage), [Light](#light), [Distortion](#distortion)
- [Preview block (editor only)](#preview-block-editor-only)
- [Version 1 projects](#version-1-projects)
- [Unknown fields and newer files](#unknown-fields-and-newer-files)

## General rules

- **Every field has a default.** Leave out what you don't need; the defaults are listed below.
- **Enum variants that wrap a struct take double parentheses**: `Particles((name: "sparks"))`, `Texture((path: "..."))`. Variants with named fields take single ones: `Segment(from: "a", to: "b")`, `Sphere(radius: 1.0)`.
- **Vectors** are tuples: `(0.0, 1.0, 0.0)`. Colours are **linear** RGBA `(r, g, b, a)`; values above 1 glow (HDR).
- **Floats need a decimal point**: `1.0`, not `1`.
- **Comments** (`//`) are allowed. The editor drops them when it saves.
- **Units**: metres, seconds, degrees (where a field says `_deg` or "degrees"), lumens for lights.

## Top level

```ron
(
    version: 2,
    parameters: [ /* ParamDef */ ],
    triggers: [ /* TriggerDef */ ],
    assets: [ /* AssetDef */ ],
    fit: None,
    layers: [ /* LayerDef */ ],
    sockets: [ /* SocketDef */ ],
    preview: Some(( /* editor only */ )),
)
```

| Field | Type | Default | Meaning |
| :-- | :-- | :-- | :-- |
| `version` | number | `2` | Format version. A file without `version` but with `effects` is a [version 1](#version-1-projects) project. |
| `parameters` | list | `[]` | Values the game sets ([Parameters](#parameters)). |
| `triggers` | list | `[]` | Events the game fires ([Triggers](#triggers)). |
| `assets` | list | `[]` | Textures and meshes, referenced by id ([Assets](#assets)). |
| `fit` | `None` / `Some(...)` | `None` | How instances scale with their target ([Fit](#fit)). |
| `layers` | list | `[]` | The visual parts ([Layers](#layers)). |
| `sockets` | list | `[]` | Named points on the target ([Sockets](#sockets-and-attach)). |
| `preview` | `None` / `Some(...)` | `None` | Editor-only view settings; not written into packages ([Preview](#preview-block-editor-only)). |

## Parameters

A parameter is a named value the game sets per instance (`set_param`). Any name, any number of them.

```ron
parameters: [
    (name: "intensity", default: Float(0.0), min: 0.0, max: 1.0, smoothing: 0.4),
    (name: "tint", default: Color((0.65, 0.08, 0.95, 1.0)), smoothing: 0.2),
],
```

| Field | Default | Meaning |
| :-- | :-- | :-- |
| `name` | `""` | The name layers and the game use. |
| `default` | `Float(0.0)` | `Float(x)` or `Color((r, g, b, a))`. Its kind is the parameter's kind. |
| `min`, `max` | `0.0`, `1.0` | Range of a `Float` parameter; the game's values are clamped to it. |
| `smoothing` | `0.0` | Seconds to reach about 63% of a new value. `0` = jumps at once. |

Layers read the current (smoothed) value through [bindings](#bindings-const-param-age), particle expressions (`Param("intensity")`), a particle layer's `rate`, `visible` ranges and `play: While(...)`.

## Triggers

A trigger is a named event the game fires (`trigger`, `trigger_at`).

```ron
triggers: [
    (name: "tick", pulses: [(param: "flash", to: 1.0, decay: 0.35)]),
    (name: "hit"),
],
```

| Field | Default | Meaning |
| :-- | :-- | :-- |
| `name` | `""` | The name layers and the game use. |
| `pulses` | `[]` | On the trigger, the `Float` parameter `param` jumps to `to` and returns to its value over `decay` seconds. |

Layers react through `play: OnTrigger("hit")` and `play: Between(start: "a", stop: "b")`, trails and afterimages through `emit: Trigger(start: "a", stop: "b")`. With `trigger_at`, particle layers can read the properties `trigger_pos`, `trigger_dir` and `trigger_strength` in expressions (`Property("trigger_pos")`).

## Bindings: `Const`, `Param`, `Age`

Most layer values (everything typed "binding" below) can be one of:

```ron
Const(20.0)                                // a constant
Param("intensity", [(0.0, 10.0), (1.0, 30.0)]) // a curve over a parameter: (parameter value, layer value) keys
Age([(0.0, 0.2), (0.6, 3.0)])              // a curve over the layer's age: (seconds, layer value) keys
```

- Keys are sorted by their first value; between keys the value is interpolated linearly; outside them the nearest key's value is used.
- The **age** is the seconds since the layer started: since the instance started, since its trigger last played it, or since a `While` parameter switched it on.
- A colour binding can read a colour parameter directly: `Param("tint", [])`.
- The value type depends on the field: a number, a vector `(x, y, z)`, a 2D vector `(x, y)` or a colour `(r, g, b, a)`.

## Assets

```ron
assets: [
    (id: "spark1", kind: Texture((path: "omagari://textures/spark1.png"))),
    (id: "smoke", kind: Texture((path: "vfx/textures/smoke.png", purpose: Sprite, grid: Some((4, 4))))),
    (id: "noise", kind: Texture((path: "omagari://textures/noise.png", purpose: Noise))),
    (id: "rock", kind: Mesh((path: "vfx/models/debris.glb", mesh: 0, primitive: 0, name: Some("Rock")))),
],
```

| Field | Default | Meaning |
| :-- | :-- | :-- |
| `id` | `""` | What layers refer to. |
| `kind` | `Texture(...)` | `Texture((...))` or `Mesh((...))`. |

**Texture** (`TextureAssetDef`):

| Field | Default | Meaning |
| :-- | :-- | :-- |
| `path` | `""` | Relative to the game's `assets/` folder, or a built-in: `omagari://textures/<file>` with `cloud.png`, `cloud2.png`, `spark1.png`, `spark2.png`, `spark3.png`, `glow1.png`, `splat1.png`, `noise.png`. |
| `purpose` | `Sprite` | `Sprite`, `Noise`, `Mask` or `Gradient`. Picks the defaults of the next three fields. |
| `wrap` | `None` | `Some(Repeat)` / `Some(Clamp)`. `None` = `Repeat` for `Noise`, else `Clamp`. |
| `color_space` | `None` | `Some(Srgb)` (colour) / `Some(Linear)` (data). `None` = `Srgb` for `Sprite` and `Gradient`, `Linear` for `Noise` and `Mask`. |
| `filter` | `None` | `Some(Linear)` (smooth) / `Some(Nearest)` (pixelated). `None` = `Linear`. |
| `grid` | `None` | `Some((rows, columns))` of a sprite sheet. |

**Mesh** (`MeshAssetDef`), one mesh of a glTF file for mesh particles:

| Field | Default | Meaning |
| :-- | :-- | :-- |
| `path` | `""` | The `.glb` / `.gltf`, relative to the game's `assets/` folder. |
| `mesh`, `primitive` | `0`, `0` | Which mesh and primitive; the game loads `<path>#Mesh<mesh>/Primitive<primitive>`. |
| `name` | `None` | The mesh's name in the file (informational). |

**Texture slots.** Shader layers (surface, shape, trail, afterimage, distortion) refer to a texture with a slot that also picks the channel:

```ron
(asset: "noise_pack", channel: G)   // R, G, B, A or Luminance (default)
```

Particle layers cannot pick a channel: their `texture` is an asset id, sampled as colour or red channel by the `ParticleTexture` render modifier.

## Sockets and attach

```ron
sockets: [
    (name: "hand", node: Some("mixamorig:RightHand"), offset: (0.0, 0.1, 0.0)),
    (name: "blade_base", node: None, offset: (0.0, 0.15, 0.0)),
    (name: "blade_tip", node: None, offset: (0.0, 1.1, 0.0)),
],
```

| Field | Default | Meaning |
| :-- | :-- | :-- |
| `name` | `""` | What layers attach to. |
| `node` | `None` | An entity with this `Name` below the target (a bone). `None` = the target's origin. |
| `offset` | `(0, 0, 0)` | Metres, in the node's space (scaled like the instance, see [Fit](#fit)). |

Until the node exists (a model still loading, or a rig without that bone) the socket is at the target's origin.

**`attach`** (on most layers) is where the layer sits:

| Value | Meaning |
| :-- | :-- |
| `Target` (default) | The target's origin. |
| `Socket("hand")` | A socket; the layer follows it. A name that no package socket has is looked up as a node name directly. |
| `Segment(from: "blade_base", to: "blade_tip")` | The line between two sockets. Particle layers get both ends as the properties `segment_a` / `segment_b` (in expressions, `Property("segment_a")`); lights sit at its middle; a distortion capsule spans it. |

## Fit

| Value | Meaning |
| :-- | :-- |
| `None` (default) | The authored size, whatever the target's scale. |
| `Some(TargetScale)` | Inherit the target's scale. |
| `Some(TargetHeight(1.8))` | Scale by the height of the target's own meshes ÷ 1.8 m. |

Fit scales `Local` particle layers (bevy_hanabi applies only the position to `Global` ones), socket offsets, shapes and lights.

## Play modes

`play` (particles, shapes, lights, distortions):

| Value | Plays |
| :-- | :-- |
| `Loop` (default) | For as long as the instance runs. |
| `OnStart` | Once when the instance starts. |
| `OnTrigger("hit")` | Each time the trigger fires; not at the start or on `restart`. |
| `While("charge", 0.5)` | While the `Float` parameter is ≥ the threshold. |
| `Between(start: "swing_start", stop: "swing_end")` | From the start trigger until the stop trigger (held emission). The same trigger for both toggles it. |

For particle layers the play mode should match the spawner: `Loop` and `Between` with a repeating spawner (`cycle_count: 0`), `OnStart` and `OnTrigger` with a finite one (`cycle_count: 1` for a burst). The checks warn otherwise.

`emit` (trails and afterimages): `Always` (default), `Trigger(start: "a", stop: "b")`, or `While("param", threshold)`.

`duration` (shapes, lights, distortions): `None` (default) or `Some(seconds)`. It makes `OnStart` / `OnTrigger` layers one-shots that fade in and out inside it and are gone after it; for `Loop` / `While` / `Between` the age starts over every `duration` seconds.

An instance **finishes by itself** when every root particle layer is `OnStart` with a finite `cycle_count`, every shape, light and distortion is `OnStart` with a `duration`, and there are no surface, trail or afterimage layers. Otherwise it runs until the game stops it.

## Shared parts: `visible`, `fade`, `material`

**`visible`**: `None` (default) or a parameter range the layer shows in:

```ron
visible: Some((param: "intensity", from: 0.6, to: 1.0, fade: 0.1)),
```

Outside `[from, to]` the layer fades out over `fade` (in parameter units). Particle expressions can read the factor as `Property("visibility")`.

**`fade`** (surface, shape, trail, afterimage, light, distortion): `(in: 0.2, out: 0.3)` by default (trails and afterimages: `(in: 0.0, out: 0.0)`). In when the layer starts, out when the game stops the instance (the stop waits for it). One-shots fade inside their `duration`.

**`sort_bias`** (particles, shape, trail, afterimage, distortion): `0.0` by default. Metres added to the layer's distance in the transparent draw order: positive = drawn later (on top of other see-through things at a similar distance).

**`material`** (`EffectMaterialDef`): Omagari's effect shader, used by surface, shape, trail and afterimage layers. Every value is a [binding](#bindings-const-param-age).

| Field | Default | Meaning |
| :-- | :-- | :-- |
| `blend` | `Blend` | `Blend` covers by opacity; `Add` adds light (never darkens). |
| `color` | `Const((0.6, 0.1, 1.0, 0.5))` | Linear RGBA; alpha × `opacity` is the coverage. |
| `opacity` | `Const(1.0)` | |
| `emissive` | `Const(1.0)` | Brightness multiplier; above 1 blooms. |
| `noise_a`, `noise_b` | `None` | Scrolling noise: `Some((texture: (asset: "noise"), mapping: Triplanar, scale: Const(3.0), scroll: Const((0.0, 0.2)), strength: Const(0.8), contrast: Const(1.0)))` (these are the defaults). `mapping`: `Triplanar` (metres, in the model's rest pose; no UVs needed) or `Uv`. `scale`: repeats per metre or per UV square. `scroll`: squares per second. `strength`: 0..1. `contrast`: 1 = as is, 2–4 = crisp patches. |
| `rim` | `None` | Edge glow: `Some((color: Const((1.0, 1.0, 1.0, 1.0)), power: Const(2.5), intensity: Const(1.0)))`. Higher `power` = thinner rim. |
| `pulse` | `None` | Breathing: `Some((frequency: Const(1.0), amount: Const(0.5)))` (pulses per second; 0..1). |
| `flash` | `None` | `Some(binding)`: 0 = nothing, 1 = three times as bright and half-way to opaque. Bind it to a parameter that a trigger pulses. |
| `dissolve` | `None` | `Some((threshold: Const(0.0), edge_width: Const(0.05), edge_color: Const((4.0, 2.0, 0.5, 1.0))))`: 0 = whole, 1 = gone; needs a noise. |
| `fill` | `None` | A band: `Some((axis: Y, from: Const(0.0), to: Const(0.5), softness: Const(0.1)))`. `axis`: `X`, `Y`, `Z`, `NegX`, `NegY`, `NegZ` (fractions of the model) or `U`, `V` (the mesh's texture coordinates: along / across a ring, arc or trail). |
| `mask` | `None` | `Some((texture: (asset: "mask"), mapping: Uv, scale: Const(1.0), strength: Const(1.0)))`. |

## Layers

`layers` is a list of these variants. Every layer has a `name` (unique in the project).

### Particles

One bevy_hanabi GPU particle effect.

```ron
Particles((
    name: "sparks",
    parent: None,                 // or Some("rocket"): spawned by that layer's particles
    attach: Target,
    simulation_space: Local,      // Local: move with the target; Global: stay where born
    play: OnStart,
    texture: Some("spark1"),      // asset id
    mesh: None,                   // or Some(Shape(Cube)) / Some(Asset("rock"))
    rate: None,                   // or Some(Param("intensity", [(0.0, 5.0), (1.0, 40.0)])): particles per second
    visible: None,
    effect: (
        capacity: 256,
        spawner: (count: Single(150.0), spawn_duration: Single(0.0), period: Single(1.0), cycle_count: 1, starts_active: true, emit_on_start: true),
        alpha_mode: Add,
        init_modifiers: [
            SetAttribute((attr: "lifetime", attr_expr: Operator(Uniform(Float(0.4), Float(0.9))))),
            SetAttribute((attr: "size", attr_expr: Float(0.04))),
            SetVelocitySphere((center_expr: Vec3((0.0, 0.0, 0.0)), speed_expr: Operator(Uniform(Float(2.0), Float(6.0))))),
        ],
        update_modifiers: [AccelModifier((accel_expr: Vec3((0.0, -9.8, 0.0))))],
        render_modifiers: [
            ColorOverLifetime((gradient: (g: [(0.0, (6.0, 5.0, 3.0, 1.0)), (1.0, (2.0, 0.3, 0.0, 0.0))]), blend: Some(Overwrite), mask: Some((15)))),
            ParticleTexture((sample_mapping: ModulateOpacityFromR)),
            Orient((mode: AlongVelocity)),
        ],
    ),
)),
```

| Field | Default | Meaning |
| :-- | :-- | :-- |
| `name` | `"particles"` | |
| `parent` | `None` | The parent layer's name. The parent's `EmitSpawnEventModifier` spawns this layer's particles; it can read the parent's attributes. |
| `attach` | `Target` | [Attach](#sockets-and-attach). |
| `simulation_space` | `Local` | `Local` or `Global`. |
| `play` | `Loop` | [Play modes](#play-modes). |
| `texture` | `None` | Asset id of the texture the `ParticleTexture` render modifier samples. |
| `mesh` | `None` | `None` = bevy_hanabi's quad (1 m at size 1). `Some(Shape(...))` with `Cube`, `Sphere`, `Cone`, `Cylinder`, `Shard`, `Torus`, `Quad` (built-in, 1 m across at size 1), or `Some(Asset("id"))` (a `Mesh` asset). Mesh particles are drawn **unlit**, one flat colour per particle. |
| `rate` | `None` | A binding: particles per second, replacing the spawner's count while it plays. |
| `visible` | `None` | [`visible`](#shared-parts-visible-fade-material). |
| `sort_bias` | `0.0` | Draw order (blended modes only). |
| `effect` | see below | The bevy_hanabi effect. |

`effect` (`ParticleEffectDef`):

| Field | Default | Meaning |
| :-- | :-- | :-- |
| `capacity` | `16384` | Most particles alive at once. Keep it near the peak count. |
| `spawner` | 500 per second, forever | `(count, spawn_duration, period, cycle_count, starts_active, emit_on_start)` as in bevy_hanabi's `SpawnerSettings`. Values are `Single(x)` or `Uniform((a, b))`. `count` particles over `spawn_duration` seconds, a cycle every `period` seconds, `cycle_count` cycles (0 = forever). |
| `alpha_mode` | `Blend` | `Blend`, `Premultiply`, `Add`, `Multiply`, `Mask(cutoff)`, `Opaque`. |
| `motion_integration` | `PostUpdate` | `PostUpdate`, `PreUpdate`, or `None` (particles only move when a modifier sets their position, for ribbons). |
| `simulation_condition` | `WhenVisible` | `WhenVisible` or `Always`. |
| `prng_seed` | `None` | `Some(n)`: the same random values every time. |
| `init_modifiers` | `[]` | Run once when a particle is born. |
| `update_modifiers` | `[]` | Run every frame. |
| `render_modifiers` | `[ParticleTexture, Orient]` | How particles are drawn. |

Init and update modifiers: `SetAttribute`, `InheritAttribute`, `SetPositionCircle`, `SetPositionSphere`, `SetPositionCone3d`, `SetVelocityCircle`, `SetVelocitySphere`, `SetVelocityTangent`, `AccelModifier`, `LinearDragModifier`, `RadialAccel`, `TangentAccel`, `ConformToSphereModifier`, `KillSphere`, `KillAabb`, `EmitSpawnEventModifier`. Render modifiers: `ColorOverLifetime`, `SizeOverLifetime`, `SetColor`, `SetSize`, `ParticleTexture`, `Orient`, `Flipbook`, `Round`, `ScreenSpaceSize`.

Modifier values are **expressions**: literals (`Float(1.0)`, `Vec3((0.0, 1.0, 0.0))`, `Vec4(...)`, `U32(5)`, `Bool(true)`, ...), random values (`RandFloat`, `RandVec3`, `Rand(...)`), built-ins (`Time`, `DeltaTime`, ...), `Property("name")`, `Param("name")` and `Operator(...)` with every bevy_hanabi operator. Note: the expression `Age` is the particle's age attribute, not the layer age of [bindings](#bindings-const-param-age). See the guide's sections "spawner", "modifiers", "expressions", "attributes", "render", "ribbons", "meshes" and "hierarchy". bevy_hanabi 0.19 limits that the checks report: integer random values (`RandU32`, `Rand(U32)`) and `AlphaCutoff` do not compile; `Asin` / `Acos` of a constant outside −1..1 are invalid; random values in render modifiers are the same for every particle.

### Surface

Omagari's effect shader over the target's own meshes (copies of them that follow the animation; the model is not changed).

```ron
Surface((
    name: "poison_skin",
    targets: AllMeshes,
    material: (
        blend: Blend,
        color: Const((0.55, 0.08, 0.95, 0.7)),
        emissive: Const(1.5),
        noise_a: Some((texture: (asset: "noise"), mapping: Triplanar, scale: Const(3.0), scroll: Const((0.0, 0.25)), strength: Const(0.85), contrast: Const(2.5))),
        rim: Some((color: Const((1.0, 0.45, 1.0, 0.8)), power: Const(2.5), intensity: Const(1.5))),
        flash: Some(Param("flash", [(0.0, 0.0), (1.0, 1.0)])),
    ),
    shells: [(offset: 0.0)],
    fade: (in: 0.3, out: 0.6),
)),
```

| Field | Default | Meaning |
| :-- | :-- | :-- |
| `name` | `"surface"` | |
| `targets` | `AllMeshes` | `AllMeshes`, `Meshes(["Body"])` (mesh or mesh entity names), or `Materials(["Skin"])`. Weapons and other separate models are never covered. |
| `material` | see [material](#shared-parts-visible-fade-material) | |
| `shells` | `[(offset: 0.0)]` | One copy per shell: `offset` (metres outward along the normals), `opacity` (1.0), `noise_offset` (0.0, shifts the noise per shell). An empty list draws nothing. |
| `fade` | `(in: 0.2, out: 0.3)` | |
| `visible` | `None` | |

### Shape

A simple 3D shape with the effect material, attached to the target but not bent by its skeleton.

```ron
Shape((
    name: "shockwave",
    attach: Target,
    follow: PositionYaw,
    shape: Ring(inner: 0.8, outer: 1.0),
    transform: (offset: (0.0, -0.86, 0.0)),        // at the feet of a target 0.9 m above them
    scale: Age([(0.0, (0.2, 1.0, 0.2)), (0.6, (3.0, 1.0, 3.0))]), // grows over 0.6 s
    play: OnTrigger("slam"),
    duration: Some(0.6),
    fade: (in: 0.0, out: 0.3),
    material: (blend: Add, color: Const((0.6, 1.2, 2.5, 1.0)), emissive: Const(1.5)),
)),
```

| Field | Default | Meaning |
| :-- | :-- | :-- |
| `name` | `"shape"` | |
| `attach` | `Target` | |
| `follow` | `Full` | `Full` (moves, turns, tilts), `Position` (only moves), `PositionYaw` (moves and turns around the vertical, never tilts). |
| `shape` | `Sphere(radius: 1.0)` | See below. |
| `transform` | `(offset: (0, 0, 0), rotation_deg: (0, 0, 0))` | Metres; degrees about X, then Y, then Z. |
| `material` | see [material](#shared-parts-visible-fade-material) | |
| `scale` | `Const((1.0, 1.0, 1.0))` | Binding, on top of the shape's size. |
| `rotation_speed` | `Const(0.0)` | Binding, degrees per second around `rotation_axis`. |
| `rotation_axis` | `(0.0, 1.0, 0.0)` | In the shape's own space. |
| `play`, `duration` | `Loop`, `None` | [Play modes](#play-modes). |
| `fade`, `visible`, `sort_bias` | `(in: 0.2, out: 0.3)`, `None`, `0.0` | |

Shapes (sizes in metres; defaults in brackets):

| Shape | Fields |
| :-- | :-- |
| `Sphere` | `radius` [1.0] |
| `Capsule` (along Y) | `radius` [0.5], `length` [1.0] between the centres of the end caps |
| `Cylinder` (along Y) | `radius` [0.5], `height` [1.0], `open_ends` [false] (a tube) |
| `Ring` (flat in XZ) | `inner` [0.8], `outer` [1.0] |
| `Torus` (around Y) | `major` [1.0], `minor` [0.1] |
| `Cone` (along Y, tip up) | `radius` [0.5], `height` [1.0] |
| `Quad` | `size` [(1.0, 1.0)] (X, Z), `facing` [`Fixed`] (flat in XZ) or `Camera` |
| `Arc` (flat slash in XZ, centred on −Z) | `radius` [1.0], `width` [0.3], `angle_deg` [120.0], `taper` [0.0] (1 = pointed ends) |

### Trail

A ribbon between two moving sockets that stays in the world and fades: a sword swing's arc.

```ron
Trail((
    name: "slash",
    from: "blade_base",
    to: "blade_tip",
    length: 0.22,
    subdivisions: 6,
    emit: Trigger(start: "swing_start", stop: "swing_end"),
    width_over_u: [(0.0, 1.0), (1.0, 0.6)],
    fade_over_u: [(0.0, 1.0), (0.2, 0.85), (1.0, 0.0)],
    material: (blend: Add, color: Const((2.4, 2.0, 1.5, 1.0)), fill: Some((axis: V, from: Const(0.0), to: Const(1.0), softness: Const(0.25)))),
)),
```

| Field | Default | Meaning |
| :-- | :-- | :-- |
| `name` | `"trail"` | |
| `from`, `to` | `"blade_base"`, `"blade_tip"` | Sockets (package sockets or node names) of the two edges (`v` = 0 and 1). |
| `length` | `0.25` | Seconds a sample stays: the trail's length in time. |
| `subdivisions` | `4` | Curve segments between samples (1 = straight lines). |
| `min_sample_distance` | `0.01` | Metres an end must move before a new sample. |
| `emit` | `Always` | `Always`, `Trigger(start, stop)`, `While(param, threshold)`. |
| `width_over_u` | `[]` | `(u, fraction of the from → to width)`, `u` = 0 newest, 1 oldest. Empty = full width. |
| `fade_over_u` | `[(0.0, 1.0), (1.0, 0.0)]` | `(u, opacity factor)`. |
| `material` | `Add`, colour `(2.0, 1.6, 1.1, 1.0)` | `Age` curves read the seconds since emission last started. |
| `fade` | `(in: 0.0, out: 0.0)` | |
| `visible`, `sort_bias` | `None`, `0.0` | |

### Afterimage

Fading copies of the target's meshes left along their path; skinned meshes are copied in their current pose.

```ron
Afterimage((
    name: "shadows",
    source: TargetModel,
    interval: 0.04,
    lifetime: 0.25,
    max_ghosts: 8,
    emit: Trigger(start: "swing_start", stop: "swing_end"),
    fade_over_life: [(0.0, 0.6), (1.0, 0.0)],
    material: (blend: Add, color: Const((0.3, 0.6, 1.6, 0.35))),
)),
```

| Field | Default | Meaning |
| :-- | :-- | :-- |
| `name` | `"afterimages"` | |
| `source` | `TargetModel` | `TargetModel` (every own mesh) or `Meshes(["Blade"])`. |
| `interval` | `0.05` | Seconds between copies while emitting. |
| `lifetime` | `0.3` | Seconds each copy stays. |
| `max_ghosts` | `8` | Most copies at once; the oldest goes first. |
| `emit` | `Always` | Like trails. |
| `fade_over_life` | `[(0.0, 1.0), (1.0, 0.0)]` | `(fraction of the lifetime, opacity factor)`. |
| `material` | `Add`, colour `(0.4, 0.8, 2.0, 0.6)` | `Age` curves read the copy's age. |
| `fade`, `visible`, `sort_bias` | `(in: 0.0, out: 0.0)`, `None`, `0.0` | |

### Light

A Bevy `PointLight` at the attach point.

```ron
Light((
    name: "flash",
    offset: (0.0, 0.2, -0.6),
    play: OnTrigger("hit"),
    duration: Some(0.25),
    fade: (in: 0.0, out: 0.05),
    color: Const((1.0, 0.85, 0.6, 1.0)),
    intensity: Age([(0.0, 150000.0), (0.05, 120000.0), (0.25, 0.0)]),
    range: Const(5.0),
)),
```

| Field | Default | Meaning |
| :-- | :-- | :-- |
| `name` | `"light"` | |
| `attach` | `Target` | A segment puts it at the middle. |
| `offset` | `(0, 0, 0)` | Metres from the attach point. |
| `color` | `Const((1.0, 0.7, 0.4, 1.0))` | Binding, linear RGB (alpha unused). Keep the largest channel at 1; brightness comes from `intensity`. |
| `intensity` | `Const(20000.0)` | Binding, lumens (a 60 W bulb ≈ 800 lm). |
| `range` | `Const(4.0)` | Binding, metres. |
| `radius` | `0.05` | Size of the light source (softer highlights). |
| `flicker` | `(amount: 0.0, speed: 6.0)` | `amount` 0..1 (fraction of the intensity), `speed` changes per second. |
| `play`, `duration` | `Loop`, `None` | An `OnTrigger` light fired with `trigger_at` shines at the trigger's position. |
| `fade`, `visible` | `(in: 0.2, out: 0.3)`, `None` | |
| `shadows` | `false` | Costly: six shadow maps per light. |

### Distortion

Bends what is behind a volume: heat haze, ripples. Needs screen-space transmission on the game camera; bends only opaque things.

```ron
Distortion((
    name: "ripple",
    source: Shape(shape: Sphere(radius: 1.0), follow: Position),
    scale: Age([(0.0, (0.3, 0.3, 0.3)), (0.6, (3.0, 3.0, 3.0))]),
    strength: Age([(0.0, 0.03), (0.6, 0.0)]),
    noise: (texture: (asset: "noise"), scale: 1.5, scroll: (0.0, 0.0)),
    edge: Rim,
    refraction: 0.75,
    edge_softness: 0.35,
    play: OnTrigger("slam"),
    duration: Some(0.6),
    fade: (in: 0.0, out: 0.1),
)),
```

| Field | Default | Meaning |
| :-- | :-- | :-- |
| `name` | `"distortion"` | |
| `attach` | `Target` | With `Segment`, a `Capsule` source spans the two sockets. Not used by a `Surface` source. |
| `source` | `Shape(shape: Sphere(radius: 0.5), follow: Full, transform: ...)` | `Shape(shape, follow, transform)` like a shape layer, or `Surface(targets: AllMeshes, offset: 0.03)` (copies of the target's meshes pushed `offset` metres out). |
| `scale` | `Const((1.0, 1.0, 1.0))` | Binding, scale of a shape source. |
| `strength` | `Const(0.008)` | Binding, the largest shift as a fraction of the view height (0.005–0.012 haze, 0.02–0.03 strong ripple). 0 draws nothing. |
| `noise` | `(texture: (asset: ""), mapping: Triplanar, scale: 1.5, scroll: (0.0, 0.6), contrast: 1.0, weight: 1.0)` | Plain numbers, not bindings. |
| `noise_b` | `None` | A second noise. |
| `edge` | `Soft` | `Soft` (strongest in the middle) or `Rim` (a band inside the outline). |
| `refraction` | `0.0` | 0..1: how much the shift follows the surface like a lens. |
| `edge_softness` | `0.6` | 0..1. |
| `tint` | `Const((1.0, 1.0, 1.0, 0.0))` | Binding, rgb multiplied into what is behind, alpha = amount. |
| `play`, `duration`, `fade`, `visible`, `sort_bias` | `Loop`, `None`, `(in: 0.2, out: 0.3)`, `None`, `0.0` | |

## Preview block (editor only)

The editor's view settings. Not written into packages; games ignore it.

```ron
preview: Some((
    stage: Model,                         // Air | Floor | Object | Capsule | Model
    camera: Game,                         // Orbit | Game
    model: Some("characters/hero.glb"),   // glTF in the game assets folder
    root_height: 0.9,                     // the target is 0.9 m above the model's feet
    animation: Some("mixamo.com"),        // clip name
    animation_file: Some("characters/run.glb"), // clip from another file (None = the model's)
    animation_speed: 1.0,
    animation_triggers: [("swing", 0.3)], // fire "swing" at 0.3 s of every loop
    attach_to: None,                      // node the instance is attached to
    target: Body,                         // Body | Weapon
    weapon: Some((model: "weapons/sword.glb", node: Some("mixamorig:RightHand"), offset: (0.0, 0.0, 0.0), rotation: (0.0, 0.0, 0.0), scale: 1.0)),
    in_place: true,                       // remove the clip's horizontal root motion
    motion: Some(Circle(radius: 3.0, speed: 3.6)), // or BackAndForth(distance: 4.0, speed: 3.6)
    params: [("intensity", Float(0.8))],  // preview parameter values
    auto_triggers: [("hit", 1.0)],        // fire "hit" every second
)),
```

Defaults: `stage: Air`, `camera: Orbit`, `animation_speed: 1.0`, `in_place: true`, everything else empty / `None` / 0. The model fields are written only when they are set.

## Version 1 projects

Projects from older Omagari versions have no `version` and a list of `effects`. They still load everywhere (editor, MCP, games) and are **migrated** to version 2: every effect becomes a `Particles` layer with `attach: Target` and `simulation_space: Global`, its texture index becomes a built-in texture asset, and the render settings that version 1 hard-coded (`Blend`, `ParticleTexture(ModulateOpacityFromR)`, `Orient(AlongVelocity)`) become explicit. The editor saves the migrated version 2.

## Unknown fields and newer files

- A field the format does not know (a typo such as `accel` for `accel_expr`, or a field of a newer editor) is **ignored** with a warning naming its path, for example `layers[0].effect.update_modifiers[2]`. The editor's status line, the MCP checks and the game's log show it.
- A `version` above 2 loads with a note; unknown values are ignored.
- An unknown **variant** (a layer kind, shape or modifier this version does not have) is a parse error: the file does not load.
