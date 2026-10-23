# Omagari VFX guide for AI agents

You build bevy_hanabi particle effects as Omagari projects (`*.omagari.ron`, format v2) through the `omagari` MCP server. **You never see the effect.** You work from numbers: `analyze_project` simulates the effect on the CPU and describes it in text, and the user watches it in the editor (`preview_in_editor`) and tells you what to change.

Fetch one section at a time with `guide(section: "...")`: workflow, format, layers, attach, model, parameters, triggers, spawner, modifiers, expressions, attributes, render, textures, assets, hierarchy, recipes, design, game.

## Workflow

1. **Ask before building** (skip what the user already said): what the effect is, one-shot or looping, how long, main colors, whether it sits on a moving character, and the game's scale (how big is a character in world units, how far is the camera). Default assumption: 1 unit = 1 meter, the effect is seen from about 5 to 10 units away.
2. **Start from something that works.** Copy a recipe (section "recipes") or a bundled example (`read_project("examples/fireworks")`, also `explode`, `greenvapor`, `magicburst`). Writing from scratch is how type errors happen.
3. **Save with `write_project(name, ron)`.** It refuses to save on errors and returns the analysis. Fix every ERROR. Read every WARNING; most are real problems.
4. **Tune with `analyze_project(name)` until the numbers match the intent.** Check: spawn pattern, peak count vs capacity, lifetime, motion extents (is it the right size? does it go up/down/outward?), size in world units, colors and the alpha window, simulation space. Use `effect: "<layer name>"` to describe a single layer and save tokens.
5. **For small changes, edit the saved file directly** (`saved_effects/<name>.omagari.ron`) with your file-edit tool, then call `check_project(name)` or `analyze_project(name)`. This is far cheaper than resending the whole RON.
6. **Show it to the user with `preview_in_editor(name, stage, motion)`**, with stage `air`, `floor`, `object`, `capsule` or `model` (see sections "design" and "model"; effects on characters belong on `model`, e.g. `model: "anim/tpose.glb", animation: "mixamo.com", animation_file: "anim/run.glb"`) and, for effects on moving characters, a `motion` such as `{"type": "circle", "radius": 3.0, "speed": 3.6}`. Ask what they see and what to change, in words ("too fast", "too dim", "should spread wider"). Section "design" maps common feedback to parameters. This replaces any unsaved edits the user had open in the editor, so say so if they were editing.
7. **Export with `export_project(name, copy_to?)`** when the user is happy. It writes `<name>.vfx.ron` into the game assets folder's `vfx/` (or `exported_effects/` when none is configured). Section "game" says how the game plays it.

The analysis is an approximation (60 updates/s, CPU random numbers, ConformToSphere is approximate, the target stands still). Trust it for scale, timing, counts and colors, not for exact looks.

## Format

A project has a list of **assets** (textures, referenced by id) and a list of **layers**. Each `Particles` layer is one bevy_hanabi effect. All layers play at the same point, the target (the origin in the editor); Y is up.

```ron
(
    version: 2,                          // always 2; files without it are old v1 projects
    assets: [
        (id: "spark1", kind: Texture((path: "omagari://textures/spark1.png"))),
    ],
    layers: [
        Particles((
            name: "sparks",              // unique; children refer to parents by name
            parent: None,                // or Some("rocket"); see section "hierarchy"
            simulation_space: Local,     // Local | Global; see section "layers"
            play: OnStart,               // Loop | OnStart | OnTrigger("name") | While("param", 0.5); must match the spawner
            texture: Some("spark1"),     // asset id; see section "textures"
            effect: (
                capacity: 512,           // max particles alive at once; keep it near the peak count
                spawner: (count: Single(200.0), spawn_duration: Single(0.0), period: Single(1.0), cycle_count: 1, starts_active: true, emit_on_start: true),
                alpha_mode: Blend,       // see section "render"
                init_modifiers: [ /* run once when a particle is born */ ],
                update_modifiers: [ /* run every frame */ ],
                render_modifiers: [
                    ParticleTexture((sample_mapping: ModulateOpacityFromR)),
                    Orient((mode: AlongVelocity)),
                ],
            ),
        )),
    ],
)
```

RON rules that trip people up:
- Enum variants wrapping a struct take **double parentheses**: `Particles((...))`, `SetAttribute((attr: "size", attr_expr: Float(0.05)))`, `Texture((path: "..."))`.
- Vector literals also take double parentheses: `Vec3((0.0, 1.0, 0.0))`, `Vec4((1.0, 0.5, 0.0, 1.0))`.
- Floats need a decimal point: `Float(1.0)`, not `Float(1)`. `U32(5)` is an integer.
- Attribute names are lowercase strings: `"position"`, `"hdr_color"`.
- Missing fields take defaults, but write the ones shown above: defaults are not always what you want (a layer's default is `Local`, `Loop`, no texture).
- Comments (`//`) are allowed in RON; `write_project` keeps them, the editor drops them when the user saves.

Nothing is hard-coded any more: texture sampling and orientation are render modifiers, blending is `alpha_mode`. A v1 project (`effects: [...]`) still loads and is migrated (Global space, the old fixed render settings made explicit); always write v2.

Projects can also hold `sockets` and `fit` (section "attach"), and `parameters` and `triggers` (sections "parameters" and "triggers"): values and events the game sends, under names you choose.

## Layers

| Field | Values | Meaning |
| :-- | :-- | :-- |
| `name` | string | Unique. Used by `parent` and `analyze_project(effect: ...)`. |
| `parent` | `None` / `Some("name")` | Spawned by the parent's spawn events (section "hierarchy"). |
| `attach` | `Target` / `Socket("name")` / `Segment(from: "a", to: "b")` | Where the layer sits: the target's origin (default), a socket it follows, or a line between two sockets. Section "attach". |
| `simulation_space` | `Local` / `Global` | See below. |
| `play` | `Loop` / `OnStart` / `OnTrigger("name")` / `While("param", threshold)` | `Loop` = the spawner repeats (`cycle_count: 0`); `OnStart` = one-shot (`cycle_count` ≥ 1); `OnTrigger` = the spawner plays each time the game fires the trigger (usually a burst, `cycle_count: 1`), not at the start; `While` = emits while a number parameter is ≥ threshold. The check warns when play and spawner disagree. |
| `rate` | `None` / `Some(Const(20.0))` / `Some(Param("intensity", [(0.0, 5.0), (1.0, 40.0)]))` | Particles per second as a steady stream, replacing the spawner's `count` while it plays; a constant or a curve over a parameter (section "parameters"). Use it with a looping spawner. |
| `visible` | `None` / `Some((param: "intensity", from: 0.6, to: 1.0, fade: 0.1))` | The layer spawns only while the parameter is in `[from, to]`, fading out over `fade` outside (section "parameters"). |
| `texture` | `None` / `Some("asset id")` | The texture the `ParticleTexture` render modifier samples. |
| `effect` | struct | Capacity, spawner, blending, modifiers. |

**Simulation space** matters as soon as the target moves (a running character, a swung weapon):
- `Local`: particles move with the target. Use it for anything that should stay on the character: auras, shields, orbiting sparks, status effects. It is the default.
- `Global`: particles stay where they were emitted, so a moving target leaves a trail behind. Use it for smoke trails, footstep dust, embers left behind. A looping `Global` layer with long-lived particles leaves a long trail: `analyze_project` estimates it as target speed (3.6 m/s, or the preview motion's speed) × lifetime and warns from 1 s of lifetime.

`analyze_project` assumes a still target, where both spaces look the same. In the editor, the PREVIEW tab's target motion (or `preview_in_editor(..., motion)`) moves the target so the user sees the difference.

## Attach

**Where a layer sits** (`attach`, per layer):
- `Target` (default): the origin of the entity the game spawns the effect on.
- `Socket("name")`: the layer follows a socket. A **package socket** is an entry of the project's `sockets` list: `(name: "head", node: Some("mixamorig:Head"), offset: (0.0, 0.1, 0.0))` is the node with that `Name` in the target's hierarchy (a bone or part of the game's model) plus an offset in metres in the node's space; `node: None` is an offset from the target's origin. A socket name that is not in `sockets` is used as a node name directly. Until the node exists (the model is still loading), and if it never does, the layer sits at the target's origin (+ the offset); the game logs a warning after 3 s.
- `Segment(from: "a", to: "b")`: a line between two sockets, e.g. a blade's base and tip. The layer sits at the target's origin and every frame the runtime writes the two ends into `Property("segment_a")` and `Property("segment_b")` (vec3, in the layer's simulation space). Read them in init expressions to spawn along the line, e.g. position = `Operator(Add(Property("segment_a"), Operator(Multiply(Operator(Subtract(Property("segment_b"), Property("segment_a"))), RandFloat))))` (recipe "Blade edge sparks"), or within a radius around it (a capsule along the blade) with the editor's preset "Random point on segment with radius": `Mix(segment_a, segment_b, RandFloat) + Normalized(Cross(segment_b − segment_a, RandVec3 × 2 − 1) + Vec3((0.000001, 0.0, 0.0))) × radius × Sqrt(RandFloat)` (recipe "Blade sparks with a radius"). A Segment layer that reads neither gets a warning, and so does a layer that reads them without being a Segment.

```ron
sockets: [
    (name: "blade_base", node: Some("Sword"), offset: (0.0, 0.1, 0.0)),
    (name: "blade_tip", node: Some("Sword"), offset: (0.0, 1.0, 0.0)),
    (name: "chest", node: None, offset: (0.0, 1.2, 0.0)),
],
```

Socket names and nodes come from the game's model: `describe_model("anim/tpose.glb")` lists a model's node names (bones like `mixamorig:RightHand`), or ask the user (e.g. the weapon entity's name). With a preview model in the project (section "model"), `check_project` checks every socket node and attach name against the nodes below the preview target in that model and reports the missing ones as ERRORS; without one, names that are not package sockets are NOTES (they must exist as nodes in the game's model) and the editor shows such sockets at the target's origin. Package sockets with `node: None` work everywhere. In the editor the sockets show as dots with labels on the model (segments as lines); the user drags them or types offsets, so a weapon's base and tip need no change to the weapon file. Give a child layer the same `attach` as its parent (the check warns otherwise).

**Size fit** (`fit`, per project):
- `None` (default): the effect keeps its authored size, even on a scaled target.
- `Some(TargetScale)`: scales with the target entity's scale.
- `Some(TargetHeight(1.8))`: author the effect for a 1.8 m character; on a target whose model is 4 m tall (its meshes, measured once they are loaded) everything is scaled × 4 / 1.8. Socket offsets scale too.

Fit scales `Local` layers only: bevy_hanabi gives `Global` layers the emitter's position, not its scale, so the check warns about `Global` layers in a fitted project.

**Lifecycle** (the game controls it, section "game"): an instance plays once its package has loaded and runs until the game stops it; stopping is smooth (no new particles, live ones finish), and restarting while stopping plays it again on the same instance. An instance whose root layers are all one-shot (`play: OnStart` with `cycle_count` ≥ 1) finishes by itself after its last spawn plus its longest particle life, so one-shot effects clean up without code; looping ones run until stopped.

## Model

The editor previews effects on the game's own models: the project's `preview` block (editor only, not exported) names a glTF model of the game assets folder, an animation clip, where the instance is attached and an optional weapon. `list_assets` lists the models (`.glb` / `.gltf`) with their height and clips; `describe_model(path)` gives a model's node hierarchy (bones marked, rest positions), mesh entities and materials, clips with lengths and the rest-pose bounds.

```ron
preview: Some((
    stage: Model,                         // Air | Floor | Object | Capsule | Model
    camera: Game,
    model: Some("anim/tpose.glb"),        // glTF in the game assets folder
    root_height: 0.9,                     // the target (game entity root) is 0.9 m above the model's feet
    animation: Some("mixamo.com"),        // clip name in the file
    animation_file: Some("anim/run.glb"), // clip from another file with the same skeleton (None = the model's file)
    animation_speed: 1.0,
    animation_triggers: [("swing", 0.3)], // fire "swing" at 0.3 s of every loop
    attach_to: None,                      // node the instance is attached to (None = the model's root)
    target: Body,                         // Body | Weapon: which model attach_to and the instance use
    weapon: Some((model: "weapons/sword.glb", node: Some("mixamorig:RightHand"), offset: (0.0, 0.0, 0.0), rotation: (0.0, 0.0, 0.0), scale: 1.0)),
    motion: Some(Circle(radius: 3.0, speed: 3.6)),
)),
```

- **Stages.** `Capsule`: a 0.7 × 1.8 m capsule whose centre is the target (a character's collider; floor 0.9 below). `Model`: the model, `root_height` metres below the target and turned to face the direction of travel (glTF +Z = forward); the capsule when no model is set or the file is missing. With `motion`, the model runs along the path.
- **Animations.** Mixamo clips are separate files with the same skeleton, all named `"mixamo.com"`: pick the file with `animation_file` and the clip by name. They play by node names (no retargeting); the check warns when the clip's nodes are not in the model. The user pauses, loops, scrubs and changes the speed in the editor's bottom bar.
- **Attach.** The instance is spawned on the target root, or on `attach_to` (a node of the body, or of the weapon with `target: Weapon`), like the game's `spawn_vfx(&package, entity)`. Sockets are found below it.
- **Weapon.** A second model held by a node of the body, with a local offset (metres, ignoring the bone's scale) and rotation (degrees). It is a separate model: the own-model rule keeps body effects (fit height, later surface layers) off it. Put a weapon effect on it with `target: Weapon`, or give its sockets offsets from the hand bone without a weapon file.
- **Checks.** With a model, `check_project` reports socket nodes, attach names, `attach_to` and the weapon node that the model does not have as errors, a clip the file does not have as an error, and a missing model file as a warning (the editor then shows the capsule).
- `preview_in_editor(name, stage: "model", model: "anim/tpose.glb", animation: "mixamo.com", animation_file: "anim/run.glb", attach_to: "", root_height: 0.9)` shows it. In the editor's TRIGGERS tab, "at random point" fires at a point on the model's mesh bounds and "fire at clip time" sets `animation_triggers`.

## Parameters

**Parameters are values the game sets while the effect plays**, declared by you in the project under any names, as many as the effect needs: an `intensity` 0..1 that grows with a status, a `tint` colour per enemy, a `charge` level. The game calls `vfx.set_param(instance, "intensity", 0.8)`; the plugin knows no names, so ask the user what the game will send and name parameters after that.

```ron
parameters: [
    // kind = the variant of `default`; min/max clamp what the game sets (numbers only);
    // smoothing = seconds to move ~63% towards a new value (0 = jump).
    (name: "intensity", default: Float(0.0), min: 0.0, max: 1.0, smoothing: 0.4),
    (name: "tint", default: Color((0.65, 0.08, 0.95, 1.0)), smoothing: 0.2),   // linear RGBA, may go above 1 (glow)
],
```

Where a parameter is used:
- **Expressions**: `Param("intensity")` is f32 for a number parameter, vec4 for a colour. Init modifiers read it when a particle is born, update modifiers every frame. Typical: `Operator(Mix(a, b, Param("intensity")))` blends from `a` at 0 to `b` at 1 (sizes, speeds, colours). A colour parameter goes straight into `hdr_color`: `SetAttribute((attr: "hdr_color", attr_expr: Param("tint")))`. ColorOverLifetime gradients cannot read parameters: set the colour with `hdr_color` and shape the alpha with a white gradient and `blend: Some(Modulate)`.
- **`rate`** (layer): particles per second, `Const(20.0)` or a curve `Param("intensity", [(0.0, 5.0), (1.0, 40.0)])`: keys (parameter value, rate) sorted by parameter value, straight lines between, flat outside. It replaces the spawner's count while the spawner plays; use a looping spawner (`cycle_count: 0`). Rate 0 = the layer spawns nothing.
- **`visible`** (layer): `Some((param: "intensity", from: 0.6, to: 1.0, fade: 0.1))` spawns only while the parameter is in the range, fading the spawn rate to 0 over `fade` parameter units outside it. Two layers with neighbouring ranges cross-fade (a weak look and a strong look). The factor (0..1) is also `Property("visibility")`; multiply a colour by it to fade new particles' alpha too.
- **`play: While("intensity", 0.8)`** (layer): emits while the parameter is ≥ 0.8 (an extra layer at full power).
- **Pulses** of triggers (section "triggers").

Number parameters used by `rate`, `visible`, `While` and pulses must be `Float`. `check_project` reports unknown names, kind mismatches (a colour where a number is needed), unsorted or empty curves, and notes parameters nothing uses. `analyze_project` simulates the defaults, then every number parameter at its min, middle and max (or the sets you pass as `params: [{"intensity": 0.2}, {"intensity": 0.9}]`) and lists, per layer, how counts, sizes, colours and reach change. `preview_in_editor(name, params: {"intensity": 0.8})` shows the user a value; in the editor's PARAMETERS tab they drag a slider (or press "animate" for min → max → min), which drives the preview the way the game's `set_param` would.

## Triggers

**Triggers are named one-shot events the game fires**: `vfx.trigger(instance, "tick")` on a damage tick, `vfx.trigger_at(instance, "hit", TriggerArgs::at(contact_point).with_direction(normal))` on a hit. Any names, any number; declare them in `triggers`:

```ron
triggers: [
    (name: "tick", pulses: [(param: "flash", to: 1.0, decay: 0.35)]),
    (name: "hit"),
],
```

A trigger does two things:
- **Layers with `play: OnTrigger("hit")` burst**: their spawner restarts from its first cycle. Give them a one-shot spawner (`cycle_count: 1`, `spawn_duration: 0.0` for an instant burst); a looping spawner would never stop after the first trigger (the check warns). They do not play when the effect starts, and restarting the effect does not replay them. A trigger fired while the effect is still loading waits for it.
- **Pulses**: the parameter jumps to `to` and returns to its current value linearly over `decay` seconds, on number parameters only. Use a dedicated `flash` parameter (default 0, smoothing 0) and read it where the flash should show (size, colour, rate).

**Payload.** `trigger_at` carries a world position, a direction and a strength (default: none, none, 1). Before an OnTrigger layer bursts, they are written into `Property("trigger_pos")` (vec3), `Property("trigger_dir")` (vec3, unit length) and `Property("trigger_strength")` (f32), in the layer's simulation space; without a position the layer's own origin is used, without a direction +Y. Spawn at the contact point with `SetAttribute((attr: "position", attr_expr: Property("trigger_pos")))` and fly along the direction with `Operator(Multiply(Property("trigger_dir"), ...))`. Use `simulation_space: Global` so the burst stays where it hit. Recipe "Hit burst at the contact point".

`check_project` reports OnTrigger layers whose trigger does not exist, pulses on unknown or colour parameters, and notes triggers that do nothing. `analyze_project` simulates an OnTrigger layer as if its trigger fired once at t = 0. `preview_in_editor(name, trigger: "hit")` fires a trigger for the user; the editor's TRIGGERS tab has a FIRE button, "at random point" (a payload on the preview model's or capsule's bounds, near the target without one), auto-fire every N seconds (saved in the project's preview block as `auto_triggers: [("tick", 1.0)]`) and fire at a clip time of the preview animation (`animation_triggers: [("swing", 0.3)]`, section "model").

## Spawner

Only root layers (no parent) use `spawner`. A child layer is spawned by its parent's spawn events, and its own spawner is ignored.

A spawner repeats a cycle: spawn `count` particles spread evenly over `spawn_duration` seconds, then rest until `period` has passed. `cycle_count` is the number of cycles, `0` = forever.

| Pattern | count | spawn_duration | period | cycle_count | play |
| :-- | :-- | :-- | :-- | :-- | :-- |
| One burst (explosion, impact) | 200 | 0.0 | 1.0 | 1 | OnStart |
| Steady stream forever (fire, smoke, aura) | rate × 1 | 1.0 | 1.0 | 0 | Loop |
| Emit for 2 s then stop | rate × 2 | 2.0 | 2.0 | 1 | OnStart |
| Burst every 0.5 s forever (pulses) | 30 | 0.0 | 0.5 | 0 | Loop |

- `spawn_duration: 0.0` means everything spawns in the same frame.
- With `cycle_count: 1` the period is ignored.
- Never combine `cycle_count: 0` with `period: 0.0` and `spawn_duration: 0.0`: that bursts `count` particles every frame.
- `count`, `spawn_duration` and `period` also accept `Uniform((min, max))`; the analysis uses the mean count.
- The editor's spawner **presets** are bevy_hanabi's constructors and keep the count (also a `Uniform` range): **Once** = `(count, spawn_duration: Single(0.0), period: Single(0.0), cycle_count: 1)`, **Rate** = `(count, spawn_duration: Single(1.0), period: Single(1.0), cycle_count: 0)` (count per second), **Burst** = `(count, spawn_duration: Single(0.0), period, cycle_count: 0)` (count every period).
- `starts_active: false` or `emit_on_start: false` mean game code has to start the layer; leave both `true`.
- When the game stops an effect, spawners stop and live particles finish their life, so a looping layer ends smoothly.

## Modifiers

All expressions (`*_expr`) are expression trees (section "expressions"). Types must match exactly: `f32`, `u32`, `vec3`.

**Init modifiers** (`init_modifiers`, run once at birth, in order):

| Modifier | Fields | Effect |
| :-- | :-- | :-- |
| `SetAttribute((attr: "<name>", attr_expr: E))` | E has the attribute's type | Sets any attribute (section "attributes"). |
| `InheritAttribute((attr: "<name>"))` | — | Copies an attribute from the parent particle that spawned this one. Children only. |
| `SetPositionSphere((center_expr: vec3, radius_expr: f32, dimension: Surface))` | `Surface` or `Volume` | Random position on/in a sphere. |
| `SetPositionCircle((center_expr: vec3, axis_expr: vec3, radius_expr: f32, dimension: Surface))` | axis = circle normal | Random position on a ring (`Surface`) or disc (`Volume`). |
| `SetVelocitySphere((center_expr: vec3, speed_expr: f32))` | — | Velocity pointing away from `center` (needs a position set first, otherwise every particle sits on the center and the direction is undefined). |
| `SetVelocityCircle((center_expr: vec3, axis_expr: vec3, speed_expr: f32))` | — | Velocity pointing away from the axis, in the circle's plane. |
| `SetVelocityTangent((origin_expr: vec3, axis_expr: vec3, speed_expr: f32))` | — | Velocity tangent around the axis (swirls). Needs a position off the axis. |
| `SetPositionCone3d((height_expr: f32, base_radius_expr: f32, top_radius_expr: f32, dimension: Volume))` | base circle at the layer origin, opening up +Y | Random position inside a truncated cone (fountains, thrusters, torches); top radius 0 = a point. It always fills the volume (`Surface` is ignored, the check warns) and turns with the emitter. **Init only.** Give the velocity from the position, e.g. `velocity = Normalized(Attr("position")) × speed` for a spray. |

Order matters: set the position before a velocity modifier that reads it. Positions are relative to the target in `Local` space (and to the spawn point in `Global`).

**Update modifiers** (`update_modifiers`, run every frame, then `position += velocity * dt`):

| Modifier | Fields | Effect |
| :-- | :-- | :-- |
| `AccelModifier((accel_expr: vec3))` | — | Adds acceleration, e.g. gravity `Vec3((0.0, -9.8, 0.0))`. A random expression here gives jitter. |
| `LinearDragModifier((drag_expr: f32))` | 0 = none, 1–5 typical | Slows particles down; velocity × (1 − drag·dt) each frame. |
| `ConformToSphereModifier((origin_expr: vec3, radius_expr: f32, influence_dist_expr: f32, attraction_accel_expr: f32, max_attraction_speed_expr: f32))` | — | Pulls particles within `influence_dist` toward a sphere surface. |
| `EmitSpawnEventModifier((condition: OnDie, count_expr: U32(100), child_index: 0))` | `Always` or `OnDie` | Spawns particles in a child layer. `Always` emits `count` **every frame** per alive particle (60 fps → ×60 per second). See section "hierarchy". |
| `RadialAccel((origin_expr: vec3, accel_expr: f32))` | m/s², + pushes away, − pulls in | Acceleration along the line from `origin` to the particle: implosions, black holes, keeping a swirl together. |
| `TangentAccel((origin_expr: vec3, axis_expr: vec3, accel_expr: f32))` | m/s², − turns the other way | Acceleration around the axis through `origin` (direction = axis × (position − origin)): swirls, tornados, auras. Alone it spirals outwards; add a `RadialAccel` pull to keep particles circling. |
| `KillSphere((center_expr: vec3, radius_expr: f32, kill_inside: false))` | `kill_inside: true` kills inside | Removes particles that leave (or enter) a sphere. |
| `KillAabb((center_expr: vec3, half_size_expr: vec3, kill_inside: false))` | half the box size per axis | Removes particles that leave (or enter) a box (a floor: center (0, −50, 0), half size (50, 50, 50) with `kill_inside: true`). |

`SetAttribute`, `SetPositionSphere/Circle` and `SetVelocity*` may also be in `update_modifiers`: they then run **every frame** (an animated `sprite_index`, a position on a path with `motion_integration: None`). Putting an init-only modifier (`InheritAttribute`, `SetPositionCone3d`) in `update_modifiers`, or an update-only one (forces, kill, spawn events) in `init_modifiers`, is an error: it is ignored.

## Expressions

| Expression | Type | Meaning |
| :-- | :-- | :-- |
| `Float(1.5)` | f32 | literal |
| `U32(5)` | u32 | literal |
| `Vec3((x, y, z))` | vec3 | literal |
| `Vec4((x, y, z, w))` | vec4 | literal |
| `RandFloat` | f32 | random in [0, 1) |
| `RandVec3` | vec3 | each component random in [0, 1) (not centered!) |
| `I32(-2)`, `Bool(true)` | i32, bool | literals |
| `Vec2((x, y))`, `IVec3((1, 2, 3))`, `UVec2((1, 2))`, `BVec3((true, false, true))` | vec2, ivec2..4, uvec2..4, bvec2..4 | vector literals of every type |
| `Rand(Vec2)` | the type | random in [0, 1) per component; **float types only** (`F32`, `Vec2`, `Vec3`, `Vec4`) |
| `RandU32` | — | **an error**: bevy_hanabi 0.19's shaders have no integer random function; use `Operator(Cast(Operator(Multiply(RandFloat, Float(10.0))), U32))` |
| `Time` | f32 | effect time in seconds |
| `DeltaTime` | f32 | seconds since the last simulation step |
| `VirtualTime`, `VirtualDeltaTime`, `RealTime`, `RealDeltaTime` | f32 | the game's virtual clock (pausable) and the real clock |
| `IsAlive` | bool | whether the particle is alive; **update modifiers only** |
| `AlphaCutoff` | — | **an error**: broken in bevy_hanabi 0.19 (the shader fails to compile) |
| `Age` | f32 | the particle's age (same as `Operator(Attr("age"))`) |
| `Placeholder` | f32 | empty slot, evaluates to 0.0; never leave one |
| `Operator(Add(a, b))`, `Operator(Subtract(a, b))`, `Operator(Multiply(a, b))` | same as inputs | same types, or a vector with an f32 (`vec3 * f32`) |
| `Operator(Uniform(a, b))` | type of a | random between a and b, per component; a and b must have the same float type |
| `Operator(Sin(a))`, `Operator(Cos(a))` | type of a | float or float vector |
| `Operator(Distance(a, b))` | f32 | distance between two vectors |
| `Operator(Normalized(a))` | type of a | unit vector (zero vector → NaN) |
| `Operator(Vec3(x, y, z))` | vec3 | builds a vec3 from three f32 expressions |
| `Operator(Vec4(x, y, z, w))` | vec4 | builds a vec4 from four f32 expressions |
| `Operator(Pack4x8UNorm(v))` | u32 | packs a vec4 in [0, 1] into a u32 color |
| `Operator(Mix(a, b, t))` | type of a | `a × (1 − t) + b × t`; a and b the same type, t the same type or an f32 (blend colours with a vec4 a, b and an f32 t) |
| `Operator(Clamp(x, low, high))` | type of x | x kept in [low, high]; all the same type |
| `Operator(SmoothStep(low, high, x))` | type of x | 0 below low, 1 above high, a smooth S-curve between; all the same type |
| `Operator(Min(a, b))`, `Operator(Max(a, b))` | type of a | the smaller / larger; same types |
| `Operator(Divide(a, b))`, `Operator(Remainder(a, b))` | type of a | `a / b`, `a % b` (like Add: same types, or a vector and a number); integer division by 0 gives `a` |
| `Operator(Abs(x))`, `Sign`, `Floor`, `Ceil`, `Round`, `Fract`, `Saturate` | type of x | per component; `Round` rounds halves to even, `Fract` = x − floor(x), `Saturate` = clamp to [0, 1] |
| `Operator(Sqrt(x))`, `InvSqrt`, `Exp`, `Exp2`, `Log`, `Log2`, `Tan`, `Asin`, `Acos`, `Atan` | type of x | float math per component; `Asin`/`Acos` of a constant outside −1..1 does not compile (the check reports it) |
| `Operator(Atan2(y, x))` | type of y | the angle of the point (x, y) |
| `Operator(Step(edge, x))` | type of x | 1 where x ≥ edge, else 0 |
| `Operator(Normal(mean, std_dev))` | type of mean | normally distributed random value (floats) |
| `Operator(Length(v))`, `Operator(Dot(a, b))` | f32 | length; dot product (also integer vectors → i32 / u32) |
| `Operator(Cross(a, b))` | vec3 | cross product of two vec3 |
| `Operator(Vec2(x, y))`, `Operator(Vec4XyzW(xyz, w))` | vec2, vec4 | build vectors; `Vec2`/`Vec3`/`Vec4` also from i32, u32 or bool numbers (→ ivec, uvec, bvec) |
| `Operator(X(v))`, `Y`, `Z`, `W` | element type | one component of a vector (`Z` needs 3, `W` 4 components) |
| `Operator(GreaterThan(a, b))`, `GreaterThanOrEqual`, `LessThan`, `LessThanOrEqual` | bool / bvecN | comparison per component; same number types |
| `Operator(All(v))`, `Operator(Any(v))` | bool | all / any component of a bool vector is true |
| `Operator(Cast(x, I32))` | the type | conversion: a number to any number type (`F32`, `I32`, `U32`, `Bool`; f32 → i32 drops the fraction), a number or a same-size vector to a vector type (`Vec3`, `IVec2`, ...) |
| `Operator(Pack4x8SNorm(v))`, `Operator(Unpack4x8UNorm(u))`, `Operator(Unpack4x8SNorm(u))` | u32 / vec4 | (un)pack 4 values in −1..1 or 0..1 into a u32 |
| `Param("intensity")` | f32 / vec4 | a parameter of the project (section "parameters"): f32 for `Float`, vec4 (linear RGBA) for `Color` |
| `Property("segment_a")`, `Property("segment_b")` | vec3 | ends of a `Segment`-attached layer (section "attach"); the analysis puts them at the sockets' offsets |
| `Property("trigger_pos")`, `Property("trigger_dir")` | vec3 | position and direction of the last trigger of an `OnTrigger` layer (section "triggers"); the analysis uses the layer origin and +Y |
| `Property("trigger_strength")` | f32 | strength of the last trigger (1 without a payload) |
| `Property("visibility")` | f32 | the layer's `visible` factor, 0..1 (1 without `visible`) |
| `Operator(Attr("<name>"))` | attribute type | this particle's attribute |
| `Operator(ParentAttr("<name>"))` | attribute type | the parent particle's attribute (children only) |

`Uniform` and `Normal` need operands whose type bevy_hanabi knows while writing the shader: numbers, random values, attributes, `Cast`. Wrap a `Param`, `Property` or a calculation: `Operator(Uniform(Float(0.1), Operator(Cast(Param("power"), F32))))` (the check reports it otherwise).

Useful building blocks:
- Random value in a range: `Operator(Uniform(Float(0.8), Float(1.2)))`
- Random direction (unit vector):
  `Operator(Normalized(Operator(Subtract(Operator(Multiply(RandVec3, Float(2.0))), Float(1.0)))))`
- Random direction × random speed:
  `Operator(Multiply(<random direction>, Operator(Uniform(Float(2.0), Float(4.0)))))`
- Mostly-up cone: `Operator(Vec3(Operator(Uniform(Float(-0.3), Float(0.3))), Operator(Uniform(Float(2.0), Float(3.0))), Operator(Uniform(Float(-0.3), Float(0.3)))))`
- Random packed color per particle: `Operator(Pack4x8UNorm(Operator(Vec4(RandFloat, RandFloat, RandFloat, Float(1.0)))))`
- Grows with a parameter: `Operator(Mix(Float(0.05), Float(0.15), Param("intensity")))`
- Colour from dim to bright with a parameter: `Operator(Mix(Vec4((0.6, 0.1, 0.9, 1.0)), Vec4((3.0, 0.6, 5.0, 1.0)), Param("intensity")))` into `hdr_color`
- Flipbook frame over the lifetime (update list, 16 frames): `SetAttribute((attr: "sprite_index", attr_expr: Operator(Cast(Operator(Multiply(Operator(Divide(Age, Operator(Attr("lifetime")))), Float(16.0))), I32))))`
- Spinning sprites (Orient rotation): `Operator(Multiply(Age, Float(3.0)))` radians

## Attributes

| Attribute | Type | Default | Notes |
| :-- | :-- | :-- | :-- |
| `"position"` | vec3 | (0,0,0) | Required by every layer; set it (a position modifier or SetAttribute). |
| `"velocity"` | vec3 | (0,0,0) | With `Orient(AlongVelocity)`, particles with zero velocity have no orientation and may be invisible. |
| `"age"` | f32 | 0.0 | **Set `age` to `Float(0.0)`** (or use a render modifier) or particles never die. |
| `"lifetime"` | f32 | 1.0 | Seconds. Particle dies when age ≥ lifetime. |
| `"size"` | f32 | 1.0 | World units. 1.0 is big: set it explicitly or use SizeOverLifetime. |
| `"size3"` | vec3 | (1,1,1) | Per-axis size. |
| `"color"` | u32 | white | Packed RGBA 0–1; build with `Pack4x8UNorm`. |
| `"hdr_color"` | vec4 | (1,1,1,1) | Unpacked color, may go above 1 (glow). |
| `"alpha"` | f32 | 1.0 | |
| `"f32_0"`…`"f32_3"`, `"f32x2_0"`…, `"f32x3_0"`…, `"f32x4_0"`…, `"u32_0"`…`"u32_3"` | various | 0 | Free slots, e.g. to pass data to children with `ParentAttr`. |

| `"sprite_index"` | i32 | 0 | The frame a `Flipbook` draws; set it with a `Cast(..., I32)` (section "render"). |

Others exist (`"id"`, `"particle_counter"`, `"axis_x"`, `"axis_y"`, `"axis_z"`, `"prev"`, `"next"`, `"ribbon_id"`) but are rarely useful here (ribbons come later).

## Render

`render_modifiers` run when particles are drawn; `effect.alpha_mode` sets blending.

**`ColorOverLifetime((gradient: (g: [(t, (r, g, b, a)), ...]), blend: Some(Overwrite), mask: Some((15))))`**
- `t` goes from 0.0 (birth) to 1.0 (death); keys in increasing order.
- RGB above 1.0 is HDR and glows with bloom (the editor camera has bloom; the game needs bloom on its camera too). 2–6 reads as bright glowing; 10+ blows out to white.
- `a` is opacity. Fade in from 0 and out to 0 to avoid particles popping.
- `blend`: `Overwrite` replaces the color, `Modulate` multiplies the particle's `color`/`hdr_color`, `Add` adds to it. Use `Modulate` with a white gradient to keep per-particle colors (from SetAttribute or a parent) and only fade alpha.
- `mask`: `Some((15))` = RGBA, `Some((7))` = RGB only (alpha untouched).

**`SizeOverLifetime((gradient: (g: [(t, (x, y, z)), ...])))`** sets the particle size in world units over its life (replaces `size`). Use equal x, y, z.

**`ParticleTexture((sample_mapping: ModulateOpacityFromR))`** samples the layer's `texture`. Without it particles are plain squares. `sample_mapping`: `ModulateOpacityFromR` (the texture's red channel is opacity, color comes from the color attributes; right for the built-in grayscale sprites), `Modulate` (color × texture RGBA, for colored textures), `ModulateRGB` (RGB only, alpha untouched).

**`Orient((mode: AlongVelocity))`**: `AlongVelocity` (the sprite's X axis follows the velocity: sparks, streaks; motionless particles may vanish), `ParallelCameraDepthPlane` (flat to the screen: puffs, glows), `FaceCameraPosition` (faces the camera). Always add one: without it, particles are flat quads in the effect's XY plane and vanish when seen from the side (the check warns).

`Orient` also takes **`rotation_expr: Some(E)`** (f32 radians, evaluated when drawing): spinning sprites with `Operator(Multiply(Age, Float(3.0)))`. It works with `ParallelCameraDepthPlane` and `FaceCameraPosition` (ignored with `AlongVelocity`, the check warns).

**`SetColor((color: Single((r, g, b, a)), blend: Modulate, mask: (15)))`** sets the colour when drawing, combined with the particle's colour like ColorOverLifetime (`Overwrite`, `Add`, `Modulate`; `mask` `(15)` RGBA, `(7)` RGB, `(8)` alpha only). `Uniform((a, b))` gives every particle the **same** value (bevy_hanabi's render shader has no per-particle seed; the check warns); for per-particle colours set `hdr_color` at birth.

**`SetSize((size: Single((x, y, z))))`** sets the size in metres when drawing; like SizeOverLifetime it replaces the `size` attribute, and the later one in the list wins. `Uniform` has the same limit as SetColor.

**`ScreenSpaceSize(())`**: the size (attribute, SetSize or SizeOverLifetime, placed before it) is read as **screen pixels**, so particles keep their size on screen at any distance (sparkles, dots). Use sizes like 8–32; the check warns below 2 pixels.

**`Round((roundness_expr: Float(1.0)))`** rounds the corners of particle squares: 0 = square, 1 = circle (f32, evaluated when drawing). Untextured round dots: `Round` without `ParticleTexture`.

**`Flipbook(())`** plays a sprite sheet: each particle draws frame `sprite_index` (an i32 attribute; frames count left to right, then top to bottom) of the layer's texture. The grid comes from the texture asset's `grid: Some((rows, cols))` (or the modifier's own `Flipbook((grid: Some((4, 4))))`). Animate the frame with an **update** `SetAttribute` on `"sprite_index"` (section "expressions": "Flipbook frame over the lifetime"; the editor's Update + menu has it). Use `ParticleTexture((sample_mapping: Modulate))` for colour sheets. The check warns about a missing grid, texture sampling, or `sprite_index`. Recipe "Flipbook".

Gradients (ColorOverLifetime, SizeOverLifetime) are linear between keys; bevy_hanabi 0.19 has no other gradient options, and SizeOverLifetime's `screen_space_size` does nothing (use ScreenSpaceSize).

**`alpha_mode`**: `Blend` (normal transparency), `Add` (additive: overlapping particles get brighter, good for glow, fire, magic; black adds nothing), `Premultiply`, `Multiply` (darkens), `Mask(0.5)` (opaque cut-out at that alpha), `Opaque`.

Effect settings (written only when not the default):
- `motion_integration`: `PostUpdate` (default: `position += velocity × dt` after the update modifiers), `PreUpdate` (before them), `None` (velocity does not move particles; only modifiers that set `position` do, e.g. an update `SetAttribute` on a path; the check warns when velocities are set but nothing moves them). `None` also silences bevy_hanabi's "missing VELOCITY" log for layers that never move.
- `simulation_condition`: `WhenVisible` (default) or `Always` (keeps simulating off screen, so it is up to date when it comes back into view; costs more).
- `prng_seed: Some(7)`: a fixed seed for the GPU random numbers (the same random spread every time; another seed gives another variation). Default: bevy_hanabi's seed 0.

## Textures

Textures are **assets**: an entry in the project's `assets` list with an id, used by a layer's `texture: Some("<id>")`. Every asset should be used by a layer (the check warns otherwise).

```ron
assets: [
    (id: "glow1", kind: Texture((path: "omagari://textures/glow1.png"))),
    (id: "smoke", kind: Texture((path: "vfx/textures/smoke.png", purpose: Sprite))),
],
```

Built-in textures (embedded in the omagari library; the game needs no files). Use the file name without `.png` as the id:

| Path | Use for |
| :-- | :-- |
| `omagari://textures/cloud.png` | soft round puff: smoke, dust, fire body |
| `omagari://textures/cloud2.png` | another cloud shape: smoke, vapor, mist |
| `omagari://textures/spark1.png` | spark / star: sparks, glitter |
| `omagari://textures/spark2.png` | spark variant |
| `omagari://textures/spark3.png` | spark variant |
| `omagari://textures/glow1.png` | soft glowing dot: embers, magic, fireworks, light orbs |
| `omagari://textures/splat1.png` | splat: blood, liquid, impacts |

They are grayscale; use `ParticleTexture((sample_mapping: ModulateOpacityFromR))` so the shape comes from the texture and the color from the color attributes.

Other textures use a path relative to the game's `assets/` folder (`vfx/textures/smoke.png`). The file must exist there (`check_project` warns when it doesn't); the editor and the check look it up in the configured game assets folder (or this repository's `assets/`). `purpose` (`Sprite`, `Noise`, `Mask`, `Gradient`) picks wrap and color-space defaults; sprites need nothing else. Bring new files in with `import_asset` (section "assets").

## Assets

**Import** a file with `import_asset(path, purpose?, id?, project?)`. A file outside the game assets folder is copied to `<game assets>/vfx/textures/` (a name taken by a different file gets `_2`, `_3`, ...; an identical file is reused); a file already inside it is used where it is. The reply has the asset entry; with `project` it is added to that saved project. `list_assets` lists the built-ins and every texture file in the game assets folder with size, layout and which saved projects use it.

Formats: **PNG** (works everywhere), **JPG** (the game needs Bevy's `jpeg` feature; the check adds a note), KTX2 (copied, not inspected). Avoid 16-bit grayscale PNGs: Bevy loads them as integer textures that cannot be sampled (the check warns; `extract_channel` with `Luminance` writes an 8-bit copy).

**Purpose** (guessed from the file name when you don't pass one: noise / perlin / voronoi / cellular → Noise, mask → Mask, gradient / ramp → Gradient, else Sprite) sets the defaults of the other settings; leave them out unless you need something else:

| purpose | wrap | color_space | use |
| :-- | :-- | :-- | :-- |
| `Sprite` | Clamp | Srgb | pictures on particles: smoke, sparks |
| `Noise` | Repeat | Linear | tiling patterns that scroll (later: surface layers) |
| `Mask` | Clamp | Linear | black/white shapes |
| `Gradient` | Clamp | Srgb | colour ramps |

`filter: Some(Nearest)` gives hard pixels (pixel art), default `Linear`. `grid: Some((rows, cols))` marks a sprite sheet for `Flipbook` (section "render"); it must divide the image evenly. `check_project` warns about settings that fight the purpose (Noise + Clamp, Gradient + Repeat, Noise/Mask + Srgb).

```ron
(id: "noise", kind: Texture((path: "vfx/textures/perlin_noise.png", purpose: Noise))),
(id: "pixel", kind: Texture((path: "vfx/textures/ember.png", filter: Some(Nearest)))),
```

**Channels.** Particle layers sample the whole texture: `Modulate` (RGBA), `ModulateRGB` or `ModulateOpacityFromR` (red only). They **cannot pick G, B or A**. For a channel-packed texture (different masks in R, G, B, A), call `extract_channel(project, asset, channel)` with `R`, `G`, `B`, `A` or `Luminance`: it writes `<name>_<channel>.png` (8-bit grayscale) next to the file (built-ins: into `vfx/textures/`) and adds it to the project as `<asset>_<channel>` with the same settings. Use it with `ParticleTexture((sample_mapping: ModulateOpacityFromR))`. Omagari's own shader layers (later phases) pick a channel per texture slot: `(asset: "pack", channel: G)`, default `Luminance`.

The user sees the same in the editor's ASSETS tab: thumbnails, an R / G / B / A view, the settings, rename (updates the layers), delete (refused while a layer uses the asset), extract channel, add built-in, and import by button or by dropping files on the window.

## Hierarchy

A child layer (with `parent: Some("name")`) spawns particles at its parent's particles: trails behind a rocket, an explosion when a rocket dies.

1. The parent must be **listed before** the child.
2. The parent gets an `EmitSpawnEventModifier` in `update_modifiers`. `child_index` is the child's position among that parent's children, counted in list order from 0.
3. The child needs `InheritAttribute((attr: "position"))` in `init_modifiers`, otherwise its particles appear at the target's origin.
4. To pass data, the parent stores it in an attribute and the child reads it with `ParentAttr` or copies it with `InheritAttribute`. For example, a random color per rocket: the parent sets `"u32_0"` to a packed random color, and the child sets `"color"` to `Operator(ParentAttr("u32_0"))`.
5. Child `capacity` must hold everything all parents emit: `OnDie` count × parent particles, or `Always` count × 60 × parent particles × child lifetime.
6. Give parent and child the same `simulation_space`.

`read_project("examples/fireworks")` is a complete rocket → trail sparkles + explosion example.

## Recipes

These pass `check_project` without warnings. Copy one, then adjust the numbers.

### Spark burst (one-shot impact)

```ron
(
    version: 2,
    assets: [(id: "spark1", kind: Texture((path: "omagari://textures/spark1.png")))],
    layers: [
        Particles((
            name: "sparks",
            simulation_space: Local,
            play: OnStart,
            texture: Some("spark1"),
            effect: (
                capacity: 256,
                spawner: (count: Single(150.0), spawn_duration: Single(0.0), period: Single(1.0), cycle_count: 1, starts_active: true, emit_on_start: true),
                alpha_mode: Blend,
                init_modifiers: [
                    SetAttribute((attr: "age", attr_expr: Float(0.0))),
                    SetAttribute((attr: "lifetime", attr_expr: Operator(Uniform(Float(0.4), Float(0.9))))),
                    SetAttribute((attr: "size", attr_expr: Float(0.04))),
                    SetPositionSphere((center_expr: Vec3((0.0, 0.0, 0.0)), radius_expr: Float(0.05), dimension: Volume)),
                    SetVelocitySphere((center_expr: Vec3((0.0, 0.0, 0.0)), speed_expr: Operator(Uniform(Float(2.0), Float(6.0))))),
                ],
                update_modifiers: [
                    AccelModifier((accel_expr: Vec3((0.0, -9.8, 0.0)))),
                    LinearDragModifier((drag_expr: Float(2.0))),
                ],
                render_modifiers: [
                    ColorOverLifetime((
                        gradient: (g: [(0.0, (6.0, 5.0, 3.0, 1.0)), (0.4, (5.0, 2.0, 0.4, 1.0)), (1.0, (2.0, 0.3, 0.0, 0.0))]),
                        blend: Some(Overwrite),
                        mask: Some((15)),
                    )),
                    ParticleTexture((sample_mapping: ModulateOpacityFromR)),
                    Orient((mode: AlongVelocity)),
                ],
            ),
        )),
    ],
)
```

### Fire (looping)

```ron
(
    version: 2,
    assets: [(id: "cloud", kind: Texture((path: "omagari://textures/cloud.png")))],
    layers: [
        Particles((
            name: "fire",
            simulation_space: Local,
            play: Loop,
            texture: Some("cloud"),
            effect: (
                capacity: 512,
                spawner: (count: Single(300.0), spawn_duration: Single(1.0), period: Single(1.0), cycle_count: 0, starts_active: true, emit_on_start: true),
                alpha_mode: Blend,
                init_modifiers: [
                    SetAttribute((attr: "age", attr_expr: Float(0.0))),
                    SetAttribute((attr: "lifetime", attr_expr: Operator(Uniform(Float(0.6), Float(1.1))))),
                    SetPositionCircle((center_expr: Vec3((0.0, 0.0, 0.0)), axis_expr: Vec3((0.0, 1.0, 0.0)), radius_expr: Float(0.25), dimension: Volume)),
                    SetAttribute((attr: "velocity", attr_expr: Operator(Vec3(Operator(Uniform(Float(-0.15), Float(0.15))), Operator(Uniform(Float(0.8), Float(1.6))), Operator(Uniform(Float(-0.15), Float(0.15))))))),
                ],
                update_modifiers: [
                    AccelModifier((accel_expr: Vec3((0.0, 1.0, 0.0)))),
                    LinearDragModifier((drag_expr: Float(1.0))),
                ],
                render_modifiers: [
                    SizeOverLifetime((gradient: (g: [(0.0, (0.35, 0.35, 0.35)), (1.0, (0.08, 0.08, 0.08))]))),
                    ColorOverLifetime((
                        gradient: (g: [(0.0, (4.0, 2.5, 0.8, 0.0)), (0.15, (4.0, 1.8, 0.4, 0.8)), (0.6, (2.0, 0.4, 0.05, 0.5)), (1.0, (0.4, 0.05, 0.0, 0.0))]),
                        blend: Some(Overwrite),
                        mask: Some((15)),
                    )),
                    ParticleTexture((sample_mapping: ModulateOpacityFromR)),
                    Orient((mode: AlongVelocity)),
                ],
            ),
        )),
    ],
)
```

### Smoke (looping, slow and soft)

```ron
(
    version: 2,
    assets: [(id: "cloud2", kind: Texture((path: "omagari://textures/cloud2.png")))],
    layers: [
        Particles((
            name: "smoke",
            simulation_space: Local,
            play: Loop,
            texture: Some("cloud2"),
            effect: (
                capacity: 256,
                spawner: (count: Single(30.0), spawn_duration: Single(1.0), period: Single(1.0), cycle_count: 0, starts_active: true, emit_on_start: true),
                alpha_mode: Blend,
                init_modifiers: [
                    SetAttribute((attr: "age", attr_expr: Float(0.0))),
                    SetAttribute((attr: "lifetime", attr_expr: Operator(Uniform(Float(3.0), Float(5.0))))),
                    SetPositionSphere((center_expr: Vec3((0.0, 0.2, 0.0)), radius_expr: Float(0.2), dimension: Volume)),
                    SetAttribute((attr: "velocity", attr_expr: Operator(Vec3(Operator(Uniform(Float(-0.1), Float(0.1))), Operator(Uniform(Float(0.4), Float(0.7))), Operator(Uniform(Float(-0.1), Float(0.1))))))),
                ],
                update_modifiers: [
                    AccelModifier((accel_expr: Operator(Subtract(Operator(Multiply(RandVec3, Float(0.4))), Vec3((0.2, 0.2, 0.2)))))),
                    LinearDragModifier((drag_expr: Float(0.5))),
                ],
                render_modifiers: [
                    SizeOverLifetime((gradient: (g: [(0.0, (0.3, 0.3, 0.3)), (1.0, (1.6, 1.6, 1.6))]))),
                    ColorOverLifetime((
                        gradient: (g: [(0.0, (0.5, 0.5, 0.5, 0.0)), (0.2, (0.45, 0.45, 0.45, 0.35)), (1.0, (0.3, 0.3, 0.3, 0.0))]),
                        blend: Some(Overwrite),
                        mask: Some((15)),
                    )),
                    ParticleTexture((sample_mapping: ModulateOpacityFromR)),
                    Orient((mode: AlongVelocity)),
                ],
            ),
        )),
    ],
)
```

For smoke left behind a moving character, switch to `simulation_space: Global` (expect a trail warning: lifetime 5 s × 3.6 m/s ≈ 18 m; shorten the lifetime if that is too long).

### Magic orbit (looping swirl on a sphere, additive)

```ron
(
    version: 2,
    assets: [(id: "glow1", kind: Texture((path: "omagari://textures/glow1.png")))],
    layers: [
        Particles((
            name: "orbit",
            simulation_space: Local,
            play: Loop,
            texture: Some("glow1"),
            effect: (
                capacity: 512,
                spawner: (count: Single(120.0), spawn_duration: Single(1.0), period: Single(1.0), cycle_count: 0, starts_active: true, emit_on_start: true),
                alpha_mode: Add,
                init_modifiers: [
                    SetAttribute((attr: "age", attr_expr: Float(0.0))),
                    SetAttribute((attr: "lifetime", attr_expr: Operator(Uniform(Float(1.5), Float(2.5))))),
                    SetAttribute((attr: "size", attr_expr: Float(0.06))),
                    SetPositionCircle((center_expr: Vec3((0.0, 1.0, 0.0)), axis_expr: Vec3((0.0, 1.0, 0.0)), radius_expr: Float(0.8), dimension: Surface)),
                    SetVelocityTangent((origin_expr: Vec3((0.0, 1.0, 0.0)), axis_expr: Vec3((0.0, 1.0, 0.0)), speed_expr: Operator(Uniform(Float(1.5), Float(2.5))))),
                ],
                update_modifiers: [
                    ConformToSphereModifier((origin_expr: Vec3((0.0, 1.0, 0.0)), radius_expr: Float(0.8), influence_dist_expr: Float(3.0), attraction_accel_expr: Float(8.0), max_attraction_speed_expr: Float(3.0))),
                ],
                render_modifiers: [
                    ColorOverLifetime((
                        gradient: (g: [(0.0, (0.5, 1.0, 3.0, 0.0)), (0.2, (1.0, 0.5, 3.0, 1.0)), (1.0, (1.5, 0.25, 2.0, 0.0))]),
                        blend: Some(Overwrite),
                        mask: Some((15)),
                    )),
                    ParticleTexture((sample_mapping: ModulateOpacityFromR)),
                    Orient((mode: ParallelCameraDepthPlane)),
                ],
            ),
        )),
    ],
)
```

Additive blending adds up where particles overlap, so the colors here are half of what `Blend` would need.

### Cone fountain (looping, additive)

```ron
(
    version: 2,
    assets: [(id: "glow1", kind: Texture((path: "omagari://textures/glow1.png")))],
    layers: [
        Particles((
            name: "fountain",
            simulation_space: Local,
            play: Loop,
            texture: Some("glow1"),
            effect: (
                capacity: 512,
                spawner: (count: Single(150.0), spawn_duration: Single(1.0), period: Single(1.0), cycle_count: 0, starts_active: true, emit_on_start: true),
                alpha_mode: Add,
                init_modifiers: [
                    SetAttribute((attr: "age", attr_expr: Float(0.0))),
                    SetAttribute((attr: "lifetime", attr_expr: Operator(Uniform(Float(1.0), Float(1.4))))),
                    SetAttribute((attr: "size", attr_expr: Float(0.05))),
                    SetPositionCone3d((height_expr: Float(0.2), base_radius_expr: Float(0.03), top_radius_expr: Float(0.12), dimension: Volume)),
                    SetAttribute((attr: "velocity", attr_expr: Operator(Multiply(Operator(Normalized(Operator(Add(Operator(Attr("position")), Vec3((0.0, 0.05, 0.0)))))), Operator(Uniform(Float(3.5), Float(4.5))))))),
                ],
                update_modifiers: [
                    AccelModifier((accel_expr: Vec3((0.0, -9.8, 0.0)))),
                    KillAabb((center_expr: Vec3((0.0, -50.0, 0.0)), half_size_expr: Vec3((50.0, 50.0, 50.0)), kill_inside: true)),
                ],
                render_modifiers: [
                    ColorOverLifetime((
                        gradient: (g: [(0.0, (0.4, 1.2, 3.0, 0.0)), (0.1, (0.4, 1.2, 3.0, 1.0)), (1.0, (0.1, 0.3, 1.5, 0.0))]),
                        blend: Some(Overwrite),
                        mask: Some((15)),
                    )),
                    ParticleTexture((sample_mapping: ModulateOpacityFromR)),
                    Orient((mode: AlongVelocity)),
                ],
            ),
        )),
    ],
)
```

The cone sprays droplets up and outwards (its velocity points from the cone's apex below the origin through each droplet); gravity brings them back, and the `KillAabb` (a box below y = 0, `kill_inside`) removes them at the floor. Narrow the top radius for a jet, widen it for a shower.

### Swirl aura (looping, TangentAccel + RadialAccel, additive)

```ron
(
    version: 2,
    assets: [(id: "glow1", kind: Texture((path: "omagari://textures/glow1.png")))],
    layers: [
        Particles((
            name: "swirl",
            simulation_space: Local,
            play: Loop,
            texture: Some("glow1"),
            effect: (
                capacity: 512,
                spawner: (count: Single(100.0), spawn_duration: Single(1.0), period: Single(1.0), cycle_count: 0, starts_active: true, emit_on_start: true),
                alpha_mode: Add,
                init_modifiers: [
                    SetAttribute((attr: "age", attr_expr: Float(0.0))),
                    SetAttribute((attr: "lifetime", attr_expr: Operator(Uniform(Float(1.5), Float(2.5))))),
                    SetAttribute((attr: "size", attr_expr: Operator(Uniform(Float(0.03), Float(0.06))))),
                    SetPositionCircle((center_expr: Vec3((0.0, 0.1, 0.0)), axis_expr: Vec3((0.0, 1.0, 0.0)), radius_expr: Float(0.6), dimension: Volume)),
                    SetVelocityTangent((origin_expr: Vec3((0.0, 0.0, 0.0)), axis_expr: Vec3((0.0, 1.0, 0.0)), speed_expr: Float(1.2))),
                ],
                update_modifiers: [
                    TangentAccel((origin_expr: Vec3((0.0, 0.0, 0.0)), axis_expr: Vec3((0.0, 1.0, 0.0)), accel_expr: Float(1.5))),
                    RadialAccel((origin_expr: Vec3((0.0, 0.0, 0.0)), accel_expr: Float(-2.5))),
                    AccelModifier((accel_expr: Vec3((0.0, 0.6, 0.0)))),
                    LinearDragModifier((drag_expr: Float(0.8))),
                ],
                render_modifiers: [
                    ColorOverLifetime((
                        gradient: (g: [(0.0, (1.0, 0.3, 2.5, 0.0)), (0.2, (1.0, 0.3, 2.5, 1.0)), (1.0, (0.3, 0.1, 1.5, 0.0))]),
                        blend: Some(Overwrite),
                        mask: Some((15)),
                    )),
                    ParticleTexture((sample_mapping: ModulateOpacityFromR)),
                    Orient((mode: ParallelCameraDepthPlane, rotation_expr: Some(Operator(Multiply(Age, Float(2.0)))))),
                ],
            ),
        )),
    ],
)
```

`TangentAccel` keeps particles turning around the body's vertical axis, the `RadialAccel` pull (negative) keeps them from flying outwards, and a light upward acceleration makes the swirl rise. More tangent acceleration = faster spin; a stronger pull = a tighter ring.

### Flipbook (sprite sheet animated over the lifetime)

```ron
(
    version: 2,
    assets: [(id: "flame_sheet", kind: Texture((path: "vfx/textures/flipbook_4x4.png", grid: Some((4, 4)))))],
    layers: [
        Particles((
            name: "flames",
            simulation_space: Local,
            play: Loop,
            texture: Some("flame_sheet"),
            effect: (
                capacity: 128,
                spawner: (count: Single(12.0), spawn_duration: Single(1.0), period: Single(1.0), cycle_count: 0, starts_active: true, emit_on_start: true),
                alpha_mode: Blend,
                init_modifiers: [
                    SetAttribute((attr: "age", attr_expr: Float(0.0))),
                    SetAttribute((attr: "lifetime", attr_expr: Float(1.0))),
                    SetAttribute((attr: "size", attr_expr: Float(0.5))),
                    SetPositionCircle((center_expr: Vec3((0.0, 0.0, 0.0)), axis_expr: Vec3((0.0, 1.0, 0.0)), radius_expr: Float(0.2), dimension: Volume)),
                    SetAttribute((attr: "velocity", attr_expr: Vec3((0.0, 0.8, 0.0)))),
                ],
                update_modifiers: [
                    SetAttribute((attr: "sprite_index", attr_expr: Operator(Cast(Operator(Multiply(Operator(Divide(Age, Operator(Attr("lifetime")))), Float(16.0))), I32)))),
                ],
                render_modifiers: [
                    ParticleTexture((sample_mapping: Modulate)),
                    Flipbook(()),
                    Orient((mode: ParallelCameraDepthPlane)),
                ],
            ),
        )),
    ],
)
```

Replace `vfx/textures/flipbook_4x4.png` with the user's sprite sheet (bring it in with `import_asset`, then set `grid: Some((rows, cols))` on the asset) and the `16.0` with rows × columns. Each particle plays the whole sheet once over its life. For a looping animation independent of the lifetime use `Operator(Cast(Operator(Remainder(Operator(Multiply(Age, Float(12.0))), Float(16.0))), I32))` (12 frames per second).

### Barrier around an object (looping; preview with stage "object")

```ron
(
    version: 2,
    assets: [(id: "glow1", kind: Texture((path: "omagari://textures/glow1.png")))],
    layers: [
        Particles((
            name: "barrier",
            simulation_space: Local,
            play: Loop,
            texture: Some("glow1"),
            effect: (
                capacity: 1024,
                spawner: (count: Single(500.0), spawn_duration: Single(1.0), period: Single(1.0), cycle_count: 0, starts_active: true, emit_on_start: true),
                alpha_mode: Blend,
                init_modifiers: [
                    SetAttribute((attr: "age", attr_expr: Float(0.0))),
                    SetAttribute((attr: "lifetime", attr_expr: Operator(Uniform(Float(0.8), Float(1.4))))),
                    SetAttribute((attr: "size", attr_expr: Float(0.05))),
                    SetPositionSphere((center_expr: Vec3((0.0, 0.0, 0.0)), radius_expr: Float(1.4), dimension: Surface)),
                    SetVelocityTangent((origin_expr: Vec3((0.0, 0.0, 0.0)), axis_expr: Vec3((0.0, 1.0, 0.0)), speed_expr: Operator(Uniform(Float(0.3), Float(0.8))))),
                ],
                update_modifiers: [
                    ConformToSphereModifier((origin_expr: Vec3((0.0, 0.0, 0.0)), radius_expr: Float(1.4), influence_dist_expr: Float(3.0), attraction_accel_expr: Float(6.0), max_attraction_speed_expr: Float(1.0))),
                ],
                render_modifiers: [
                    ColorOverLifetime((
                        gradient: (g: [(0.0, (0.5, 2.0, 4.0, 0.0)), (0.2, (0.5, 2.5, 5.0, 0.8)), (1.0, (0.3, 1.0, 3.0, 0.0))]),
                        blend: Some(Overwrite),
                        mask: Some((15)),
                    )),
                    ParticleTexture((sample_mapping: ModulateOpacityFromR)),
                    Orient((mode: AlongVelocity)),
                ],
            ),
        )),
    ],
)
```

The radius (1.4) is just outside the rock; the lower part of the bubble is under the floor, like a dome. For a body in the game, set the radius to the character's size; `Local` keeps the barrier on a moving character.

### Explosion with debris (parent → child on death)

```ron
(
    version: 2,
    assets: [
        (id: "glow1", kind: Texture((path: "omagari://textures/glow1.png"))),
        (id: "spark1", kind: Texture((path: "omagari://textures/spark1.png"))),
    ],
    layers: [
        Particles((
            name: "debris",
            simulation_space: Local,
            play: OnStart,
            texture: Some("glow1"),
            effect: (
                capacity: 64,
                spawner: (count: Single(24.0), spawn_duration: Single(0.0), period: Single(1.0), cycle_count: 1, starts_active: true, emit_on_start: true),
                alpha_mode: Blend,
                init_modifiers: [
                    SetAttribute((attr: "age", attr_expr: Float(0.0))),
                    SetAttribute((attr: "lifetime", attr_expr: Operator(Uniform(Float(0.5), Float(1.0))))),
                    SetAttribute((attr: "size", attr_expr: Float(0.08))),
                    SetPositionSphere((center_expr: Vec3((0.0, 0.0, 0.0)), radius_expr: Float(0.1), dimension: Volume)),
                    SetVelocitySphere((center_expr: Vec3((0.0, -0.3, 0.0)), speed_expr: Operator(Uniform(Float(4.0), Float(7.0))))),
                ],
                update_modifiers: [
                    AccelModifier((accel_expr: Vec3((0.0, -9.8, 0.0)))),
                    EmitSpawnEventModifier((condition: OnDie, count_expr: U32(20), child_index: 0)),
                ],
                render_modifiers: [
                    ColorOverLifetime((
                        gradient: (g: [(0.0, (6.0, 3.0, 1.0, 1.0)), (1.0, (3.0, 0.8, 0.1, 1.0))]),
                        blend: Some(Overwrite),
                        mask: Some((15)),
                    )),
                    ParticleTexture((sample_mapping: ModulateOpacityFromR)),
                    Orient((mode: AlongVelocity)),
                ],
            ),
        )),
        Particles((
            name: "pop",
            parent: Some("debris"),
            simulation_space: Local,
            play: OnStart,
            texture: Some("spark1"),
            effect: (
                capacity: 512,
                spawner: (count: Single(1.0), spawn_duration: Single(0.0), period: Single(0.0), cycle_count: 1, starts_active: true, emit_on_start: true),
                alpha_mode: Blend,
                init_modifiers: [
                    InheritAttribute((attr: "position")),
                    SetAttribute((attr: "age", attr_expr: Float(0.0))),
                    SetAttribute((attr: "lifetime", attr_expr: Operator(Uniform(Float(0.2), Float(0.4))))),
                    SetAttribute((attr: "size", attr_expr: Float(0.03))),
                    SetAttribute((attr: "velocity", attr_expr: Operator(Multiply(Operator(Normalized(Operator(Subtract(Operator(Multiply(RandVec3, Float(2.0))), Float(1.0))))), Operator(Uniform(Float(1.0), Float(2.5))))))),
                ],
                update_modifiers: [
                    LinearDragModifier((drag_expr: Float(3.0))),
                ],
                render_modifiers: [
                    ColorOverLifetime((
                        gradient: (g: [(0.0, (5.0, 4.0, 2.0, 1.0)), (1.0, (2.0, 0.5, 0.1, 0.0))]),
                        blend: Some(Overwrite),
                        mask: Some((15)),
                    )),
                    ParticleTexture((sample_mapping: ModulateOpacityFromR)),
                    Orient((mode: AlongVelocity)),
                ],
            ),
        )),
    ],
)
```

### Blade edge sparks (segment between two sockets, looping)

```ron
(
    version: 2,
    assets: [(id: "spark1", kind: Texture((path: "omagari://textures/spark1.png")))],
    sockets: [
        (name: "blade_base", node: None, offset: (0.0, 0.2, 0.0)),
        (name: "blade_tip", node: None, offset: (0.0, 1.2, 0.0)),
    ],
    layers: [
        Particles((
            name: "edge_sparks",
            attach: Segment(from: "blade_base", to: "blade_tip"),
            simulation_space: Global,
            play: Loop,
            texture: Some("spark1"),
            effect: (
                capacity: 256,
                spawner: (count: Single(80.0), spawn_duration: Single(1.0), period: Single(1.0), cycle_count: 0, starts_active: true, emit_on_start: true),
                alpha_mode: Add,
                init_modifiers: [
                    SetAttribute((attr: "age", attr_expr: Float(0.0))),
                    SetAttribute((attr: "lifetime", attr_expr: Operator(Uniform(Float(0.2), Float(0.5))))),
                    SetAttribute((attr: "size", attr_expr: Float(0.03))),
                    SetAttribute((attr: "position", attr_expr: Operator(Add(Property("segment_a"), Operator(Multiply(Operator(Subtract(Property("segment_b"), Property("segment_a"))), RandFloat)))))),
                    SetAttribute((attr: "velocity", attr_expr: Operator(Multiply(Operator(Subtract(Operator(Multiply(RandVec3, Float(2.0))), Float(1.0))), Float(0.6))))),
                ],
                update_modifiers: [
                    AccelModifier((accel_expr: Vec3((0.0, -2.0, 0.0)))),
                ],
                render_modifiers: [
                    ColorOverLifetime((
                        gradient: (g: [(0.0, (2.0, 3.0, 5.0, 0.0)), (0.1, (2.0, 3.0, 5.0, 1.0)), (1.0, (0.5, 1.0, 3.0, 0.0))]),
                        blend: Some(Overwrite),
                        mask: Some((15)),
                    )),
                    ParticleTexture((sample_mapping: ModulateOpacityFromR)),
                    Orient((mode: AlongVelocity)),
                ],
            ),
        )),
    ],
)
```

In the game, give the sockets the weapon's node (`node: Some("Sword")`) or put the package on the weapon entity and keep `node: None` with offsets along the blade. `Global` leaves the sparks behind during a swing; `Local` keeps them on the blade.

### Blade sparks with a radius (segment preset, swing burst, capsule preview)

```ron
(
    version: 2,
    triggers: [(name: "swing")],
    assets: [(id: "spark1", kind: Texture((path: "omagari://textures/spark1.png")))],
    sockets: [
        (name: "blade_base", node: None, offset: (0.3, 0.0, 0.0)),
        (name: "blade_tip", node: None, offset: (0.3, 0.9, 0.0)),
    ],
    layers: [
        Particles((
            name: "blade_glow",
            attach: Segment(from: "blade_base", to: "blade_tip"),
            simulation_space: Local,
            play: Loop,
            texture: Some("spark1"),
            effect: (
                capacity: 128,
                spawner: (count: Single(60.0), spawn_duration: Single(1.0), period: Single(1.0), cycle_count: 0, starts_active: true, emit_on_start: true),
                alpha_mode: Add,
                init_modifiers: [
                    SetAttribute((attr: "age", attr_expr: Float(0.0))),
                    SetAttribute((attr: "lifetime", attr_expr: Operator(Uniform(Float(0.3), Float(0.6))))),
                    SetAttribute((attr: "size", attr_expr: Float(0.04))),
                    SetAttribute((attr: "position", attr_expr: Operator(Add(Operator(Mix(Property("segment_a"), Property("segment_b"), RandFloat)), Operator(Multiply(Operator(Normalized(Operator(Add(Operator(Cross(Operator(Subtract(Property("segment_b"), Property("segment_a"))), Operator(Subtract(Operator(Multiply(RandVec3, Float(2.0))), Float(1.0))))), Vec3((0.000001, 0.0, 0.0)))))), Operator(Multiply(Float(0.04), Operator(Sqrt(RandFloat)))))))))),
                    SetAttribute((attr: "velocity", attr_expr: Vec3((0.0, 0.3, 0.0)))),
                ],
                render_modifiers: [
                    ColorOverLifetime((
                        gradient: (g: [(0.0, (1.0, 2.0, 4.0, 0.0)), (0.2, (1.0, 2.0, 4.0, 1.0)), (1.0, (0.3, 0.6, 2.0, 0.0))]),
                        blend: Some(Overwrite),
                        mask: Some((15)),
                    )),
                    ParticleTexture((sample_mapping: ModulateOpacityFromR)),
                    Orient((mode: ParallelCameraDepthPlane)),
                ],
            ),
        )),
        Particles((
            name: "swing_burst",
            attach: Segment(from: "blade_base", to: "blade_tip"),
            simulation_space: Global,
            play: OnTrigger("swing"),
            texture: Some("spark1"),
            effect: (
                capacity: 128,
                spawner: (count: Single(60.0), spawn_duration: Single(0.0), period: Single(1.0), cycle_count: 1, starts_active: true, emit_on_start: true),
                alpha_mode: Add,
                init_modifiers: [
                    SetAttribute((attr: "age", attr_expr: Float(0.0))),
                    SetAttribute((attr: "lifetime", attr_expr: Operator(Uniform(Float(0.3), Float(0.6))))),
                    SetAttribute((attr: "size", attr_expr: Float(0.05))),
                    SetAttribute((attr: "position", attr_expr: Operator(Add(Operator(Mix(Property("segment_a"), Property("segment_b"), RandFloat)), Operator(Multiply(Operator(Normalized(Operator(Add(Operator(Cross(Operator(Subtract(Property("segment_b"), Property("segment_a"))), Operator(Subtract(Operator(Multiply(RandVec3, Float(2.0))), Float(1.0))))), Vec3((0.000001, 0.0, 0.0)))))), Operator(Multiply(Float(0.05), Operator(Sqrt(RandFloat)))))))))),
                    SetAttribute((attr: "velocity", attr_expr: Operator(Multiply(Operator(Subtract(Operator(Multiply(RandVec3, Float(2.0))), Float(1.0))), Float(2.5))))),
                ],
                update_modifiers: [
                    LinearDragModifier((drag_expr: Float(3.0))),
                ],
                render_modifiers: [
                    ColorOverLifetime((
                        gradient: (g: [(0.0, (6.0, 3.0, 1.0, 1.0)), (1.0, (2.0, 0.5, 0.1, 0.0))]),
                        blend: Some(Overwrite),
                        mask: Some((15)),
                    )),
                    ParticleTexture((sample_mapping: ModulateOpacityFromR)),
                    Orient((mode: AlongVelocity)),
                ],
            ),
        )),
    ],
    preview: Some((stage: Capsule, camera: Game, auto_triggers: [("swing", 1.0)])),
)
```

The glow fills a 4 cm capsule around the blade (the editor's Position preset "Random point on segment with radius" writes this expression); `swing` bursts sparks off the whole blade. On a model, give the sockets a hand bone (`node: Some("mixamorig:RightHand")`, offsets along the hand's Y axis) or the weapon's nodes, and use `animation_triggers: [("swing", 0.3)]` in the preview to fire it at the hit frame of a swing clip (section "model").

### Intensity progression (one parameter drives rate, colour and size)

```ron
(
    version: 2,
    parameters: [
        (name: "intensity", default: Float(0.0), min: 0.0, max: 1.0, smoothing: 0.4),
    ],
    assets: [(id: "glow1", kind: Texture((path: "omagari://textures/glow1.png")))],
    layers: [
        Particles((
            name: "embers",
            simulation_space: Local,
            play: Loop,
            texture: Some("glow1"),
            rate: Some(Param("intensity", [(0.0, 4.0), (1.0, 40.0)])),
            effect: (
                capacity: 128,
                spawner: (count: Single(10.0), spawn_duration: Single(1.0), period: Single(1.0), cycle_count: 0, starts_active: true, emit_on_start: true),
                alpha_mode: Add,
                init_modifiers: [
                    SetPositionCircle((center_expr: Vec3((0.0, 0.1, 0.0)), axis_expr: Vec3((0.0, 1.0, 0.0)), radius_expr: Float(0.4), dimension: Volume)),
                    SetAttribute((attr: "age", attr_expr: Float(0.0))),
                    SetAttribute((attr: "lifetime", attr_expr: Operator(Uniform(Float(0.7), Float(1.1))))),
                    SetAttribute((attr: "size", attr_expr: Operator(Mix(Float(0.05), Float(0.14), Param("intensity"))))),
                    SetAttribute((attr: "velocity", attr_expr: Operator(Vec3(Float(0.0), Operator(Mix(Float(0.5), Float(1.6), Param("intensity"))), Float(0.0))))),
                    SetAttribute((attr: "hdr_color", attr_expr: Operator(Mix(Vec4((1.5, 0.6, 0.2, 1.0)), Vec4((4.0, 2.0, 0.5, 1.0)), Param("intensity"))))),
                ],
                update_modifiers: [
                    LinearDragModifier((drag_expr: Float(0.8))),
                ],
                render_modifiers: [
                    ColorOverLifetime((
                        gradient: (g: [(0.0, (1.0, 1.0, 1.0, 0.0)), (0.2, (1.0, 1.0, 1.0, 1.0)), (1.0, (1.0, 1.0, 1.0, 0.0))]),
                        blend: Some(Modulate),
                        mask: Some((15)),
                    )),
                    ParticleTexture((sample_mapping: ModulateOpacityFromR)),
                    Orient((mode: ParallelCameraDepthPlane)),
                ],
            ),
        )),
        Particles((
            name: "flare",
            simulation_space: Local,
            play: Loop,
            texture: Some("glow1"),
            visible: Some((param: "intensity", from: 0.7, to: 1.0, fade: 0.15)),
            effect: (
                capacity: 64,
                spawner: (count: Single(12.0), spawn_duration: Single(1.0), period: Single(1.0), cycle_count: 0, starts_active: true, emit_on_start: true),
                alpha_mode: Add,
                init_modifiers: [
                    SetPositionSphere((center_expr: Vec3((0.0, 0.8, 0.0)), radius_expr: Float(0.3), dimension: Volume)),
                    SetAttribute((attr: "age", attr_expr: Float(0.0))),
                    SetAttribute((attr: "lifetime", attr_expr: Operator(Uniform(Float(0.4), Float(0.7))))),
                    SetAttribute((attr: "size", attr_expr: Float(0.3))),
                    SetAttribute((attr: "velocity", attr_expr: Vec3((0.0, 0.3, 0.0)))),
                    SetAttribute((attr: "hdr_color", attr_expr: Operator(Multiply(Vec4((5.0, 3.0, 1.0, 1.0)), Property("visibility"))))),
                ],
                update_modifiers: [],
                render_modifiers: [
                    ColorOverLifetime((
                        gradient: (g: [(0.0, (1.0, 1.0, 1.0, 0.0)), (0.3, (1.0, 1.0, 1.0, 0.8)), (1.0, (1.0, 1.0, 1.0, 0.0))]),
                        blend: Some(Modulate),
                        mask: Some((15)),
                    )),
                    ParticleTexture((sample_mapping: ModulateOpacityFromR)),
                    Orient((mode: ParallelCameraDepthPlane)),
                ],
            ),
        )),
    ],
    preview: Some((params: [("intensity", Float(0.5))])),
)
```

At 0 a few dim, small, slow embers; at 1 ten times as many, bigger, faster and glowing; above 0.7 a bright flare fades in. The game maps its own progress to it: `vfx.set_param(instance, "intensity", progress)`, smoothing makes jumps fade in over ~0.4 s. `analyze_project` lists the embers at min / middle / max.

### Damage tick pulse (trigger flash on a looping aura)

```ron
(
    version: 2,
    parameters: [
        (name: "flash", default: Float(0.0), min: 0.0, max: 1.0, smoothing: 0.0),
    ],
    triggers: [
        (name: "tick", pulses: [(param: "flash", to: 1.0, decay: 0.35)]),
    ],
    assets: [(id: "cloud2", kind: Texture((path: "omagari://textures/cloud2.png")))],
    layers: [
        Particles((
            name: "mist",
            simulation_space: Local,
            play: Loop,
            texture: Some("cloud2"),
            rate: Some(Param("flash", [(0.0, 15.0), (1.0, 60.0)])),
            effect: (
                capacity: 128,
                spawner: (count: Single(15.0), spawn_duration: Single(1.0), period: Single(1.0), cycle_count: 0, starts_active: true, emit_on_start: true),
                alpha_mode: Add,
                init_modifiers: [
                    SetPositionSphere((center_expr: Vec3((0.0, 0.9, 0.0)), radius_expr: Float(0.45), dimension: Volume)),
                    SetAttribute((attr: "age", attr_expr: Float(0.0))),
                    SetAttribute((attr: "lifetime", attr_expr: Operator(Uniform(Float(0.6), Float(1.0))))),
                    SetAttribute((attr: "size", attr_expr: Operator(Mix(Float(0.25), Float(0.45), Param("flash"))))),
                    SetAttribute((attr: "velocity", attr_expr: Operator(Vec3(Float(0.0), Float(0.3), Float(0.0))))),
                    SetAttribute((attr: "hdr_color", attr_expr: Operator(Mix(Vec4((0.5, 0.06, 0.8, 0.5)), Vec4((3.0, 0.6, 4.0, 0.9)), Param("flash"))))),
                ],
                update_modifiers: [],
                render_modifiers: [
                    ColorOverLifetime((
                        gradient: (g: [(0.0, (1.0, 1.0, 1.0, 0.0)), (0.3, (1.0, 1.0, 1.0, 1.0)), (1.0, (1.0, 1.0, 1.0, 0.0))]),
                        blend: Some(Modulate),
                        mask: Some((15)),
                    )),
                    ParticleTexture((sample_mapping: ModulateOpacityFromR)),
                    Orient((mode: ParallelCameraDepthPlane)),
                ],
            ),
        )),
    ],
    preview: Some((auto_triggers: [("tick", 1.0)])),
)
```

A soft purple mist; on every `vfx.trigger(instance, "tick")` (one damage tick) `flash` jumps to 1 and falls back over 0.35 s, so new puffs are briefly brighter, bigger and four times as many. The editor auto-fires `tick` every second.

### Hit burst at the contact point (trigger_at, one-shot per trigger)

```ron
(
    version: 2,
    triggers: [(name: "hit")],
    assets: [(id: "spark1", kind: Texture((path: "omagari://textures/spark1.png")))],
    layers: [
        Particles((
            name: "hit_sparks",
            simulation_space: Global,
            play: OnTrigger("hit"),
            texture: Some("spark1"),
            effect: (
                capacity: 256,
                spawner: (count: Single(40.0), spawn_duration: Single(0.0), period: Single(1.0), cycle_count: 1, starts_active: true, emit_on_start: true),
                alpha_mode: Add,
                init_modifiers: [
                    SetAttribute((attr: "position", attr_expr: Property("trigger_pos"))),
                    SetAttribute((attr: "age", attr_expr: Float(0.0))),
                    SetAttribute((attr: "lifetime", attr_expr: Operator(Uniform(Float(0.2), Float(0.45))))),
                    SetAttribute((attr: "size", attr_expr: Float(0.04))),
                    SetAttribute((attr: "velocity", attr_expr: Operator(Multiply(Operator(Normalized(Operator(Add(Property("trigger_dir"), Operator(Multiply(Operator(Subtract(Operator(Multiply(RandVec3, Float(2.0))), Float(1.0))), Float(0.8))))))), Operator(Multiply(Operator(Uniform(Float(2.0), Float(5.0))), Property("trigger_strength"))))))),
                ],
                update_modifiers: [
                    LinearDragModifier((drag_expr: Float(3.0))),
                    AccelModifier((accel_expr: Vec3((0.0, -6.0, 0.0)))),
                ],
                render_modifiers: [
                    ColorOverLifetime((
                        gradient: (g: [(0.0, (6.0, 4.0, 1.5, 1.0)), (1.0, (2.0, 0.5, 0.1, 0.0))]),
                        blend: Some(Overwrite),
                        mask: Some((15)),
                    )),
                    ParticleTexture((sample_mapping: ModulateOpacityFromR)),
                    Orient((mode: AlongVelocity)),
                ],
            ),
        )),
    ],
    preview: Some((auto_triggers: [("hit", 0.8)])),
)
```

Put the package on the weapon or the struck character and call `vfx.trigger_at(instance, "hit", TriggerArgs::at(contact).with_direction(surface_normal).with_strength(1.0))` on contact: 40 sparks fly out from the contact point along the normal (strength scales their speed) and stay in the world (`Global`). Nothing plays until the first hit. In the editor, TRIGGERS → "at random point" (or auto-fire at random points) shows the payload.

### Status aura on a body (socket + fit, looping)

```ron
(
    version: 2,
    assets: [(id: "glow1", kind: Texture((path: "omagari://textures/glow1.png")))],
    fit: Some(TargetHeight(1.8)),
    sockets: [(name: "chest", node: None, offset: (0.0, 1.0, 0.0))],
    layers: [
        Particles((
            name: "aura",
            attach: Socket("chest"),
            simulation_space: Local,
            play: Loop,
            texture: Some("glow1"),
            effect: (
                capacity: 256,
                spawner: (count: Single(60.0), spawn_duration: Single(1.0), period: Single(1.0), cycle_count: 0, starts_active: true, emit_on_start: true),
                alpha_mode: Add,
                init_modifiers: [
                    SetAttribute((attr: "age", attr_expr: Float(0.0))),
                    SetAttribute((attr: "lifetime", attr_expr: Operator(Uniform(Float(1.0), Float(1.6))))),
                    SetAttribute((attr: "size", attr_expr: Float(0.08))),
                    SetPositionCircle((center_expr: Vec3((0.0, -0.9, 0.0)), axis_expr: Vec3((0.0, 1.0, 0.0)), radius_expr: Float(0.45), dimension: Volume)),
                    SetAttribute((attr: "velocity", attr_expr: Operator(Vec3(Float(0.0), Operator(Uniform(Float(0.6), Float(1.2))), Float(0.0))))),
                ],
                update_modifiers: [
                    LinearDragModifier((drag_expr: Float(0.5))),
                ],
                render_modifiers: [
                    ColorOverLifetime((
                        gradient: (g: [(0.0, (0.6, 0.1, 1.0, 0.0)), (0.2, (1.2, 0.2, 2.0, 0.8)), (1.0, (0.4, 0.05, 0.8, 0.0))]),
                        blend: Some(Overwrite),
                        mask: Some((15)),
                    )),
                    ParticleTexture((sample_mapping: ModulateOpacityFromR)),
                    Orient((mode: ParallelCameraDepthPlane)),
                ],
            ),
        )),
    ],
)
```

Made for a 1.8 m character: the socket sits 1 m up, particles start at the feet (0.9 m below the socket) and rise ~1.2 m. On a 4 m enemy the whole aura is scaled × 2.2.

## Design

**Scale.** Everything is in world units and seconds. The editor camera starts 5 units from the origin and shows about ±2 units; `analyze_project` says when an effect is bigger. Size the effect to the user's game, not to the editor view.

**The editor scene.** The editor plays the project through the same runtime as the game, on a still target at the spawn point. Effects play inside a big light gray cube (40 units wide) with grid lines every 1 unit (darker every 5) and small axes at the spawn point (X red, Y green, Z blue). You can refer the user to the grid ("each spark should travel about 2 squares"). The toolbar has a STAGE button, and `preview_in_editor` takes a `stage`, for where the effect plays:
- `air`: the spawn point is in the middle of the room, nothing nearby. Shows everything, including particles that fall.
- `floor`: the floor is at y = 0, under the spawn point, like ground in a game. Particles below y = 0 are hidden; `analyze_project` warns about that.
- `object`: a rock about 2 units wide (radius ~1, 1.6 tall) is centered on the spawn point and rests on the floor. Use it for shields, barriers, auras and hit effects on a body; particles inside the rock are hidden.
- `capsule`: a 0.7 × 1.8 m capsule (a character) whose centre is the spawn point, standing on the floor 0.9 below.
- `model`: the game's model from the project's preview block (section "model"), e.g. a Mixamo character running with a clip of another file; the capsule without one.

Pick the stage that matches how the game uses the effect. The PREVIEW tab's target motion (or `preview_in_editor(..., motion: {"type": "circle", "radius": 3.0, "speed": 3.6})`, also `back_and_forth` with `distance`, or `none`) moves the target, so the user sees whether layers stay on a running character (`Local`) or trail behind (`Global`); the setting is saved in the project's `preview` block. The BG button (or the PREVIEW tab) switches between a light room, a dark room and black; HDR glow stands out less on the light room, so suggest dark or black when judging glow. STOP stops the effect the way the game does: no new particles, live ones finish. To judge how big an effect reads in the game, ask the user to switch CAMERA to GAME (the game's orthographic isometric view, 16 m visible height).

**Typical values** (1 unit = 1 m):

| Thing | Value |
| :-- | :-- |
| Gravity | `Vec3((0.0, -9.8, 0.0))`; lighter (−2 to −5) for floaty sparks |
| Spark size / speed / life | 0.02–0.06 / 2–8 / 0.3–1.0 s |
| Fire puff size / rise speed / life | 0.1–0.5 / 0.5–2 / 0.5–1.2 s |
| Smoke size / rise speed / life | 0.3–2 growing / 0.2–0.8 / 2–6 s |
| Ember / glow dot size | 0.03–0.1 |
| Drag | 0.5 (floaty) to 5 (stops quickly) |
| Counts | impacts 50–300, loops 20–500/s; past a few thousand alive, ask first |
| HDR intensity | 1 = plain, 2–6 = glowing, 10+ = blown out (halve it with `alpha_mode: Add`) |
| Smoke alpha | 0.2–0.5 |
| Character run speed (trail estimate) | 3.6 m/s |

**How distance comes out:** a particle launched at speed v with drag d travels roughly v / d before stopping (without drag: v × lifetime). Gravity pulls it 4.9 × t² down after t seconds.

**Make it read well:**
- Fade alpha in at the start (t ≈ 0.05–0.15) and out at the end, so particles don't pop.
- Vary lifetime, speed and size with `Uniform` (±20–50%); identical particles look artificial.
- Layer simple effects (a bright core burst, sparks, smoke) instead of one complex one. Each is a separate root layer in the same project.
- Keep `capacity` slightly above the reported peak count.
- Glow, fire and magic often look better with `alpha_mode: Add`; smoke and dust need `Blend`.

**Translate user feedback:**

| User says | Change |
| :-- | :-- |
| too fast / too slow | speed, lifetime, drag |
| too big / small, spreads too much / too little | position radius, speed, size, drag |
| too dense / too sparse | spawn count or rate (adjust capacity) |
| too dim / not glowing / too bright | RGB intensity in ColorOverLifetime (HDR > 1 glows), or `alpha_mode: Add` |
| pops in / disappears abruptly | alpha 0 at t = 0 and t = 1 in the gradient |
| looks like squares / dots | add `ParticleTexture`, pick another texture, or a larger size |
| streaks look wrong / sprites spin | another `Orient` mode (`ParallelCameraDepthPlane` for puffs) |
| lasts too long / too short | lifetime, spawn_duration, cycle_count |
| nothing appears | check alpha window, size, capacity, `starts_active`, and that the camera sees it |
| falls / should rise | sign of the Y acceleration |
| leaves a trail / should stay on the character | `simulation_space` (`Global` trails, `Local` follows) |
| should grow / change with something in the game | a parameter (section "parameters") read with `Mix(..., Param("name"))`, a `rate` curve or a `visible` range |
| should react to an event (hit, tick, swing) | a trigger: an `OnTrigger` burst layer or a pulse (section "triggers") |

## Game

`export_project` writes `<name>.vfx.ron` (the project without the editor's `preview` block) to `<game assets>/vfx/`, or to `exported_effects/` when no game assets folder is configured (then copy it with `copy_to`). In the game, with `HanabiPlugin` and `omagari::OmagariPlugin` added:

```rust
let package: Handle<VfxPackage> = asset_server.load("vfx/fireball.vfx.ron");
let instance = commands.spawn_vfx(&package, target);                // follows `target`
let hit = commands.spawn_vfx_at(&package, Transform::from_translation(p)); // in the world, no target
vfx.stop(instance);    // VfxControl: smooth stop
vfx.restart(instance); // while stopping: plays again (re-applied status)
// message VfxFinished { instance, target }: sent once when an instance ends

// Parameters and triggers: the names are the ones the project declares.
let tinted = commands.spawn_vfx_with(&package, target, VfxSpawn { params: &[("intensity", 0.3.into())] });
vfx.set_param(instance, "intensity", progress);              // smoothed, clamped to the range
vfx.set_param_now(instance, "tint", Color::srgb(0.2, 0.9, 1.0)); // no smoothing
vfx.trigger(instance, "tick");                                // OnTrigger layers burst, pulses run
vfx.trigger_at(instance, "hit", TriggerArgs::at(contact).with_direction(normal));
```

Tell the user which parameter and trigger names (and value ranges) the game must use; the game maps its own rules to them. Unknown names are ignored with one warning. Parameter values survive hot reloads.

One-shot packages (every root layer `OnStart` with a finite `cycle_count`) finish and clean up by themselves; looping ones run until `stop`. Despawning the target removes its instances (and sends `VfxFinished`). Sockets with a `node` need that node name in the target's hierarchy (section "attach"). Editing the package file while the game runs updates running instances (hot reload, when the game enables Bevy's file watcher).

Built-in textures need nothing; other textures must exist at their path under the game's `assets/` folder (`import_asset` puts them there). JPG textures need Bevy's `jpeg` feature in the game. The game camera needs `Hdr` and `Bloom` for colors above 1.0 to glow. The game can also load `*.omagari.ron` projects directly.
