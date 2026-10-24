# Omagari VFX guide for AI agents

You build bevy_hanabi particle effects as Omagari projects (`*.omagari.ron`, format v2) through the `omagari` MCP server. **You never see the effect.** You work from numbers: `analyze_project` simulates the effect on the CPU and describes it in text, and the user watches it in the editor (`preview_in_editor`) and tells you what to change.

Fetch one section at a time with `guide(section: "...")`: workflow, format, layers, attach, model, parameters, triggers, spawner, modifiers, expressions, attributes, render, textures, assets, hierarchy, surfaces, shapes, trails, afterimages, lights, distortion, ribbons, meshes, order, recipes, design, game.

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

A project has a list of **assets** (textures, and meshes for mesh particles, referenced by id) and a list of **layers**. Each `Particles` layer is one bevy_hanabi effect; a `Surface` layer draws Omagari's effect shader over the target's model (section "surfaces"); a `Shape` layer draws a simple 3D shape (sphere, ring, arc, ...) with the same shader (section "shapes"); a `Trail` layer draws a ribbon between two moving sockets, e.g. a sword swing's arc (section "trails"); an `Afterimage` layer leaves fading copies of the target's meshes (section "afterimages"); a `Light` layer is a point light that lights the floor and the model around it (section "lights"); a `Distortion` layer bends what is behind it, e.g. heat haze or a shockwave ripple (section "distortion"). All layers play at the same point, the target (the origin in the editor); Y is up.

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
            play: OnStart,               // Loop | OnStart | OnTrigger("name") | While("param", 0.5) | Between(start: "a", stop: "b"); must match the spawner
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
| `play` | `Loop` / `OnStart` / `OnTrigger("name")` / `While("param", threshold)` / `Between(start: "a", stop: "b")` | `Loop` = the spawner repeats (`cycle_count: 0`); `OnStart` = one-shot (`cycle_count` ≥ 1); `OnTrigger` = the spawner plays each time the game fires the trigger (usually a burst, `cycle_count: 1`), not at the start; `While` = emits while a number parameter is ≥ threshold; `Between` = emits from the moment the game fires `start` until it fires `stop` (held emission, e.g. sparks along a blade during a swing; a looping spawner, `cycle_count: 0`). The check warns when play and spawner disagree. |
| `rate` | `None` / `Some(Const(20.0))` / `Some(Param("intensity", [(0.0, 5.0), (1.0, 40.0)]))` | Particles per second as a steady stream, replacing the spawner's `count` while it plays; a constant or a curve over a parameter (section "parameters"). Use it with a looping spawner. |
| `visible` | `None` / `Some((param: "intensity", from: 0.6, to: 1.0, fade: 0.1))` | The layer spawns only while the parameter is in `[from, to]`, fading out over `fade` outside (section "parameters"). |
| `texture` | `None` / `Some("asset id")` | The texture the `ParticleTexture` render modifier samples. |
| `mesh` | `None` / `Some(Shape(Cube))` / `Some(Asset("rock"))` | What each particle draws: the default 1 m quad, a built-in 3D shape or a mesh asset (section "meshes"). |
| `sort_bias` | `0.0` (default, not written) / metres | Draw order among see-through things at a similar distance: positive = drawn later, on top (section "order"). |
| `effect` | struct | Capacity, spawner, blending, modifiers. |

The table is for `Particles` layers. `Surface` layers (a shader over the target's model: glowing skins, shells with volume) have their own fields: section "surfaces"; so do `Shape` layers (shield bubbles, ground rings, shockwaves, slash arcs): section "shapes"; `Trail` layers (sword trails, comet tails): section "trails"; `Afterimage` layers (sword shadows, dash echoes): section "afterimages"; `Light` layers (weapon glows, hit flashes, torches): section "lights"; `Distortion` layers (heat haze, ripples, aura shimmer): section "distortion".

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
- **Weapon.** A second model held by a node of the body, with a local offset (metres, ignoring the bone's scale) and rotation (degrees). It is a separate model: the own-model rule keeps body effects (fit height, surface layers) off it. Put a weapon effect on it with `target: Weapon`, or give its sockets offsets from the hand bone without a weapon file.
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
- **Curves over age**: any binding (`rate`, every surface and shape value) can instead be `Age([(0.0, 40.0), (2.0, 0.0)])`: keys (seconds, value) over the layer's **age** — the seconds since the layer started, since its trigger last played it (`OnTrigger`), or since a `While` parameter switched it on; a `restart` of the effect starts it over. Use it for one-shots that grow and fade without code (section "shapes"). `analyze_project` simulates a particle `rate` over age at its age-0 value and describes shapes over their age.
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
- **Layers with `play: Between(start: "swing_start", stop: "swing_end")` are held on** from the start trigger until the stop trigger: a particle layer emits (its spawner restarts at each start; give it a looping spawner, `cycle_count: 0`), a shape shows, a light shines. The same name twice (`Between(start: "t", stop: "t")`) toggles on and off. This is the trail's `emit: Trigger(start, stop)` for every other layer: "sparks along the blade during a swing" needs no parameter (recipe "Sword sparks during a swing"). A stop of the effect lets go; a restart waits for the next start. A `Between` particle layer gets the start trigger's payload (`trigger_pos` / `trigger_dir` / `trigger_strength`).

**Payload.** `trigger_at` carries a world position, a direction and a strength (default: none, none, 1). Before an OnTrigger layer bursts, they are written into `Property("trigger_pos")` (vec3), `Property("trigger_dir")` (vec3, unit length) and `Property("trigger_strength")` (f32), in the layer's simulation space; without a position the layer's own origin is used, without a direction +Y. Spawn at the contact point with `SetAttribute((attr: "position", attr_expr: Property("trigger_pos")))` and fly along the direction with `Operator(Multiply(Property("trigger_dir"), ...))`. Use `simulation_space: Global` so the burst stays where it hit. Recipe "Hit burst at the contact point".

`check_project` reports OnTrigger / Between layers whose triggers do not exist, pulses on unknown or colour parameters, and notes triggers that do nothing. `analyze_project` simulates an OnTrigger layer as if its trigger fired once at t = 0, and a Between layer as held on from t = 0; its "Parameters and triggers" section says which layers each trigger bursts, starts, stops or toggles. `preview_in_editor(name, trigger: "hit")` fires a trigger for the user; the editor's TRIGGERS tab has a FIRE button, "at random point" (a payload on the preview model's or capsule's bounds, near the target without one), auto-fire every N seconds (saved in the project's preview block as `auto_triggers: [("tick", 1.0)]`) and fire at a clip time of the preview animation (`animation_triggers: [("swing", 0.3)]`, section "model").

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
| `"ribbon_id"` | u32 | 0 | Particles with the same id are drawn as one ribbon (section "ribbons"). |
| `"particle_counter"` | u32 | unique | A different value for every particle ever spawned (read with `Attr`): a unique id for a parent's ribbon. |
| `"axis_x"`, `"axis_y"`, `"axis_z"` | vec3 | X, Y, Z | The particle's own axes when drawn (set by Orient); mesh particles turn with them. |

Others exist (`"id"` = the particle's slot, reused when it dies; `"prev"`, `"next"` = old ribbon links, unused by bevy_hanabi 0.19) but are not useful here.

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
| `omagari://textures/noise.png` | tileable cloudy noise (purpose `Noise`) for surface layers (section "surfaces") |

They are grayscale; use `ParticleTexture((sample_mapping: ModulateOpacityFromR))` so the shape comes from the texture and the color from the color attributes.

Other textures use a path relative to the game's `assets/` folder (`vfx/textures/smoke.png`). The file must exist there (`check_project` warns when it doesn't); the editor and the check look it up in the configured game assets folder (or this repository's `assets/`). `purpose` (`Sprite`, `Noise`, `Mask`, `Gradient`) picks wrap and color-space defaults; sprites need nothing else. Bring new files in with `import_asset` (section "assets").

## Assets

**Import** a file with `import_asset(path, purpose?, id?, project?)` (a texture; a glTF model becomes a `Mesh` asset, section "meshes"). A file outside the game assets folder is copied to `<game assets>/vfx/textures/` (a name taken by a different file gets `_2`, `_3`, ...; an identical file is reused); a file already inside it is used where it is. The reply has the asset entry; with `project` it is added to that saved project. `list_assets` lists the built-ins and every texture file in the game assets folder with size, layout and which saved projects use it.

Formats: **PNG** (works everywhere), **JPG** (the game needs Bevy's `jpeg` feature; the check adds a note), KTX2 (copied, not inspected). Avoid 16-bit grayscale PNGs: Bevy loads them as integer textures that cannot be sampled (the check warns; `extract_channel` with `Luminance` writes an 8-bit copy).

**Purpose** (guessed from the file name when you don't pass one: noise / perlin / voronoi / cellular → Noise, mask → Mask, gradient / ramp → Gradient, else Sprite) sets the defaults of the other settings; leave them out unless you need something else:

| purpose | wrap | color_space | use |
| :-- | :-- | :-- | :-- |
| `Sprite` | Clamp | Srgb | pictures on particles: smoke, sparks |
| `Noise` | Repeat | Linear | tiling patterns that scroll (surface layers) |
| `Mask` | Clamp | Linear | black/white shapes |
| `Gradient` | Clamp | Srgb | colour ramps |

`filter: Some(Nearest)` gives hard pixels (pixel art), default `Linear`. `grid: Some((rows, cols))` marks a sprite sheet for `Flipbook` (section "render"); it must divide the image evenly. `check_project` warns about settings that fight the purpose (Noise + Clamp, Gradient + Repeat, Noise/Mask + Srgb).

```ron
(id: "noise", kind: Texture((path: "vfx/textures/perlin_noise.png", purpose: Noise))),
(id: "pixel", kind: Texture((path: "vfx/textures/ember.png", filter: Some(Nearest)))),
```

**Channels.** Particle layers sample the whole texture: `Modulate` (RGBA), `ModulateRGB` or `ModulateOpacityFromR` (red only). They **cannot pick G, B or A**. For a channel-packed texture (different masks in R, G, B, A), call `extract_channel(project, asset, channel)` with `R`, `G`, `B`, `A` or `Luminance`: it writes `<name>_<channel>.png` (8-bit grayscale) next to the file (built-ins: into `vfx/textures/`) and adds it to the project as `<asset>_<channel>` with the same settings. Use it with `ParticleTexture((sample_mapping: ModulateOpacityFromR))`. Surface layers (Omagari's own shader) pick a channel per texture slot: `(asset: "pack", channel: G)`, default `Luminance`, so they need no extraction.

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

## Surfaces

A **surface layer** draws Omagari's effect shader **over the target's own model**: a glowing skin, scrolling poison veins, frost creeping up the legs, an energy shell with thickness. The model and its materials are never changed: the runtime adds copies of the model's meshes (the same mesh and skin, so they move with every animation frame) and draws the effect on them. A weapon held in the hand is a separate model and is not covered (put another instance on the weapon). Surface layers have no particles: `analyze_project` describes them in words instead of simulating them.

```ron
Surface((
    name: "skin",                       // unique among all layers
    targets: AllMeshes,                 // AllMeshes | Meshes(["Beta_Surface"]) | Materials(["Beta_HighLimbsGeoSG3"])
    material: (
        blend: Add,                     // Blend (covers by opacity) | Add (adds light, never darkens)
        color: Const((0.6, 0.1, 1.0, 0.6)), // linear RGBA; above 1 glows with bloom
        opacity: Const(1.0),            // × the colour's alpha
        emissive: Const(1.5),           // brightness multiplier
        noise_a: Some((texture: (asset: "noise"), mapping: Triplanar, scale: Const(3.0), scroll: Const((0.0, 0.25)), strength: Const(0.8))),
        noise_b: None,                  // a second noise, multiplied in
        rim: Some((color: Const((1.0, 0.5, 1.0, 1.0)), power: Const(2.5), intensity: Const(1.5))),
        pulse: Some((frequency: Const(0.8), amount: Const(0.3))),
        flash: Some(Param("flash", [(0.0, 0.0), (1.0, 1.0)])),
        dissolve: None,                 // Some((threshold, edge_width, edge_color))
        fill: None,                     // Some((axis: Y, from, to, softness))
        mask: None,                     // Some((texture, mapping, scale, strength))
    ),
    shells: [(offset: 0.0), (offset: 0.02, opacity: 0.5, noise_offset: 0.37)],
    fade: (in: 0.3, out: 0.6),          // seconds; the game's stop waits for `out`
    visible: None,                      // a parameter range, like particle layers
))
```

- **Every value is a binding** (section "parameters"): `Const(x)` or a curve over a number parameter `Param("power", [(0.0, a), (1.0, b)])`. A colour can also be a Color parameter itself: `color: Param("tint", [])`. Scroll is a pair: `Const((x, y))`, curve keys `(0.0, (0.0, 0.1))`.
- **targets**: names from `describe_model` (a mesh entity is `<mesh>.<material>`, e.g. `Beta_Surface.Beta_HighLimbsGeoSG3`; `Meshes` takes a mesh name or a mesh entity name, `Materials` a material name). With a preview model the check reports names the model does not have as errors; without one they are notes.
- **shells** give volume: each shell is a copy pushed `offset` metres outward along the surface (× the size fit). One shell at 0 = a coloured skin; 2–3 shells at 1–4 cm = a thick, soft glow around the body (use `Add` and fainter outer shells, `noise_offset` so their patterns differ). Negative offsets are hidden inside the model and offsets above 0.1 m float away (both warn). An empty list draws nothing.
- **noise** (`noise_a`, `noise_b`): a texture slot `(asset: "id", channel: R | G | B | A | Luminance)` (default Luminance; a channel-packed texture can feed several slots). `mapping: Triplanar` projects it from three sides in the model's **rest pose**, so the pattern **sticks to the body** while it runs (`scale` = repeats per metre); `Uv` uses the mesh's own UVs (`scale` = repeats per UV square). `scroll` moves it (squares per second; on Triplanar `y` runs along the body, so `(0.0, 0.3)` with scale 3 crawls ~0.1 m/s). `strength` 0..1 shapes colour and opacity. The built-in `omagari://textures/noise.png` (purpose `Noise`) is a tileable cloudy noise; `import_asset` brings others.
- **rim**: a fresnel glow on the edges seen from the camera (`power` 1 = wide and soft, 5 = thin outline; `intensity` above 1 blooms). The classic aura outline.
- **pulse**: the opacity breathes between `1 − amount` and 1, `frequency` times per second.
- **flash**: 0 = nothing, 1 = three times as bright and half-way to opaque. Bind it to a parameter that a trigger pulses: `triggers: [(name: "tick", pulses: [(param: "flash", to: 1.0, decay: 0.35)])]` flashes the skin on every damage tick.
- **dissolve**: the surface breaks up where noise A (else B) is below `threshold` (0 = whole, 1 = gone) with a glowing edge `edge_width` wide in `edge_color`. Bind the threshold to a parameter to burn the layer in or away. Needs a noise (warns otherwise).
- **fill**: shows the layer only on a band of the model along an `axis` of its rest pose (`Y` = up for glTF models): 0 = the low end (feet), 1 = the high end (head), `softness` the soft edge. `from: Const(0.0), to: Param("frost", [(0.0, 0.0), (1.0, 1.0)])` = frost rising from the feet as the game raises `frost`.
- **mask**: a texture whose value multiplies the opacity (same slot and mapping as noise).
- **blend**: `Blend` for colour skins and frost; `Add` for glows and energy. A `Blend` layer that covers with opacity ≥ 0.95 hides the model under it (the check warns): keep the colour's alpha around 0.4–0.8 or add noise.
- **fade** in when the instance starts, out when the game stops it; a `restart` during the fade out fades back in. Surface layers loop: a package with one does not finish by itself.

The editor's 🎨 Surface layer (➕ ADD) has every value with tooltips, a texture picker with the channel, a target picker from the preview model and a shell list (presets "skin" and "thick glow"). Preview surface layers on the `capsule` stage, or on `model` with the game's character (`preview_in_editor(name, stage: "model", model: "anim/tpose.glb", animation: "mixamo.com", animation_file: "anim/run.glb", root_height: 0.9)`). Recipes "Status skin", "Energy shell" and "Frost from the feet up" show the common uses.

Every material value can also be a curve over the layer's age (`Age([...])`, section "parameters"); a surface layer's age runs from the effect's start.

## Shapes

A **shape layer** draws a simple 3D shape with Omagari's effect shader (the same material as surface layers: colour, noise, rim, pulse, flash, dissolve, fill, mask): a shield bubble, a ground ring, an expanding shockwave, a slash arc, a column of light. It is attached to the target like a particle layer but not bent by the skeleton. Shapes have no particles: `analyze_project` describes their size, opacity and look over their age and at each parameter set.

```ron
Shape((
    name: "shockwave",                  // unique among all layers
    attach: Target,                     // Target | Socket("name") | Segment(from: "a", to: "b") (the middle, Y along the segment)
    follow: PositionYaw,                // Full | Position | PositionYaw
    shape: Ring(inner: 0.8, outer: 1.0),// see the table; metres at scale 1
    transform: (offset: (0.0, -0.86, 0.0), rotation_deg: (0.0, 0.0, 0.0)),
    scale: Age([(0.0, (0.2, 1.0, 0.2)), (0.6, (3.0, 1.0, 3.0))]),   // (x, y, z) × the shape's size
    rotation_speed: Const(0.0),         // degrees per second around rotation_axis
    rotation_axis: (0.0, 1.0, 0.0),     // in the shape's own space
    play: OnTrigger("slam"),            // Loop | OnStart | OnTrigger("name") | While("param", threshold)
    duration: Some(0.6),                // seconds: a one-shot for OnStart / OnTrigger; the age repeats for Loop / While
    fade: (in: 0.0, out: 0.3),
    visible: None,                      // a parameter range, like other layers
    sort_bias: 0.0,                     // metres added to its draw-order distance (+ = drawn later, on top)
    material: ( /* EffectMaterialDef, section "surfaces" */ ),
))
```

| Shape | Fields (metres, defaults) | Mesh and UVs |
| :-- | :-- | :-- |
| `Sphere(radius: 1.0)` | radius | u around, v bottom → top |
| `Capsule(radius: 0.5, length: 1.0)` | `length` = between the centres of the round ends (total height = length + 2 × radius), along Y | u around, v bottom → top |
| `Cylinder(radius: 0.5, height: 1.0, open_ends: false)` | along Y; `open_ends: true` = a tube without caps, drawn inside and out | side: u around, v bottom → top |
| `Ring(inner: 0.8, outer: 1.0)` | flat in XZ (facing up), drawn from both sides | u around, v inner → outer edge |
| `Torus(major: 1.0, minor: 0.1)` | ring radius, tube radius, around Y | u around the ring, v around the tube |
| `Cone(radius: 0.5, height: 1.0)` | base at the bottom, tip up, along Y | side: u around, v base → tip |
| `Quad(size: (1.0, 1.0), facing: Fixed)` | (X, Z) metres, flat in XZ, both sides; `facing: Camera` = always faces the camera (a glow sprite; rotation ignored) | u along X, v along Z |
| `Arc(radius: 1.0, width: 0.3, angle_deg: 120.0, taper: 0.0)` | a flat band in XZ **in front (-Z)**, `radius` to its middle, `angle_deg` degrees wide, `taper` 0 = even width, 1 = pointed ends; both sides | u along the arc from the left end (-X side) to the right, v inner → outer edge |

Every shape is centred on its attach point (the cylinder and capsule from -height/2 to +height/2). The game's character root is often 0.9 m above the feet (capsule stage: the capsule's centre): a ground ring needs `offset: (0.0, -0.88, 0.0)` (2 cm above the floor, so it does not flicker with it).

- **follow**: `Full` moves, turns and tilts with the attach point (a shape on a hand bone turns with the hand); `Position` only moves (world axes; a shield sphere); `PositionYaw` moves and turns to face where the target faces but never tilts: a **ground ring stays flat** while the body leans and animates, even on a bone like `mixamorig:Hips`. The offset is in the attach point's space (`Full`), in world axes (`Position`), or turned with the heading (`PositionYaw`); it scales with the size fit.
- **scale** and **rotation_speed** are bindings: `Const`, a curve over a parameter (a shield that swells with `charge`), or over the age. A spin is integrated over time, so changing its speed never jumps.
- **play**: `Loop` while the effect runs; `OnStart` once at the start; `OnTrigger("name")` each time the game fires the trigger (the age starts over; a trigger during the play restarts it; with `trigger_at` the shape plays at the payload position); `While("param", t)` while the parameter is ≥ t (the age starts over each time it switches on). A trigger shape needs the trigger declared in `triggers`.
- **duration**: with `OnStart` / `OnTrigger` the shape is a **one-shot**: it plays `duration` seconds, fading in over `fade.in` after it starts and out over `fade.out` before its end, then it is gone (an `OnTrigger` shape waits for the next trigger). Without a duration an `OnTrigger` shape stays after its first trigger until the effect stops (warning). With `Loop` / `While` the age starts over every `duration` seconds, so age curves repeat (a pulsing ring). Every shape fades out over `fade.out` when the game stops the effect.
- **Lifecycle**: a package whose layers are all one-shots (particles `OnStart` with `cycle_count` ≥ 1, shapes `OnStart` with a duration) finishes by itself; anything else runs until stopped.
- **Material**: as in section "surfaces". On shapes, `Uv` mapping sticks to the shape (it grows with it), `Triplanar` is in the shape's own space in metres (it turns with the shape; a growing shape grows through the pattern). `fill` takes `axis: U` or `V` (the mesh's UVs, 0..1) besides X / Y / Z: `fill: Some((axis: U, to: Age([(0.0, 0.0), (0.15, 1.0)])))` sweeps a slash arc across in 0.15 s; `axis: V, from: 0.3, to: 0.7, softness: 0.4` gives a ring soft inner and outer edges; on a flat shape a fill along Y has no length (warning). `Add` for light (glows, energy); `Blend` for smoke-like or solid looks — an opaque `Blend` sphere around the target hides the character (warning).
- **Draw order**: see-through things are drawn back to front by their centre; a shape around the target and particles inside it can swap order as the camera turns. `sort_bias` (metres) pushes the shape later (positive, on top) or earlier.
- **Checks**: sizes ≤ 0 and an inner ring radius ≥ the outer are errors; shapes over ~20 m across (size × scale) warn; `OnTrigger` with an unknown trigger is an error; age curves with keys past the duration (never reached), fades longer than the one-shot, and a trigger shape without a duration warn; socket names are checked like other layers.

The editor's 🔷 Shape layer (➕ ADD) has every value with tooltips and preset buttons ("shield bubble", "ground ring", "shockwave", "slash arc", "glow column"). Preview on the `capsule` stage, or on `model` with the game's character running (`motion`); fire triggers in the TRIGGERS tab (or `preview_in_editor(..., trigger: "slam")`). Recipes "Shield bubble", "Ground ring", "Shockwave" and "Slash arc".

## Trails

A **trail layer** draws a ribbon between two moving points — a blade's base and tip — that stays in the world behind them and fades: the glowing arc of a sword swing, a comet tail behind a running character. Every frame both sockets are sampled (after the animation); samples older than `length` seconds are dropped (the end is cut exactly at `length`, so the tail slides away instead of popping); the points are smoothed with Catmull-Rom curves (`subdivisions` segments between two samples, so a fast swing stays round) and drawn as a two-sided ribbon with Omagari's effect material (section "surfaces"). Trails are not simulated: `analyze_project` describes their time length, width, fade, emission and look.

```ron
Trail((
    name: "slash",                       // unique among all layers
    from: "blade_base",                  // socket (package socket or node name): the v = 0 edge
    to: "blade_tip",                     // the v = 1 edge
    length: 0.22,                        // seconds a sample stays (a swing: 0.15–0.4)
    subdivisions: 6,                     // curve segments between samples (1 = straight; 4–8 for fast swings)
    min_sample_distance: 0.01,           // metres an end moves before a new sample (the newest point always follows)
    emit: Trigger(start: "swing_start", stop: "swing_end"),   // Always | Trigger(start: "a", stop: "b") | While("param", threshold)
    width_over_u: [(0.0, 1.0), (1.0, 0.6)],   // (u, fraction of the from → to width), shrinking toward `to`; [] = full
    fade_over_u: [(0.0, 1.0), (1.0, 0.0)],    // (u, opacity factor); [] = no fade along it
    fade: (in: 0.0, out: 0.0),           // lifecycle fade (the trail also ages out after a stop)
    visible: None,
    sort_bias: 0.0,
    material: ( /* EffectMaterialDef, section "surfaces" */ ),
))
```

- **UVs**: `u` = the point's age / `length` (0 at the blade, 1 at the oldest end), `v` = 0 at `from` → 1 at `to`. So `fill: Some((axis: U, ...))` cuts along the trail, `axis: V` softens its edges (`from: 0, to: 1, softness: 0.25`), and `Uv` noise with a scroll in x (`scroll: (-2.0, 0.0)`) streams streaks along it. `Triplanar` noise is in world space (the trail moves through a still pattern). A fill along X / Y / Z warns (use U or V).
- **emit**: `Always` while the effect runs; `Trigger(start, stop)` from the start trigger to the stop trigger — the game fires them from its attack state (`vfx.trigger(weapon_fx, "swing_start")` when the swing begins, `"swing_end"` when it ends; the same trigger twice toggles); `While("param", t)` while a number parameter is ≥ t. When emission stops, the rest ages out over `length`; nothing is drawn between swings. `Age` curves in the material read the seconds since emission last started. A stop of the effect ends emission; the instance waits for the trail to age out.
- **Sockets**: on the weapon (a separate model with its own effects): package sockets with offsets from the weapon's root (`node: None`) or a weapon node, so the base and tip need no change to the weapon file; or on the hand bone of the character (`node: Some("mixamorig:RightHand")`) when there is no weapon model. In the editor they show as dots joined by a line and can be dragged; `width` = their distance.
- **Size**: the trail is in world space; socket offsets scale with the size fit. Its length in metres is the blade's speed × `length` (a tip at ~10 m/s and 0.22 s ≈ a 2.2 m arc).
- **Cost**: at most 200 samples and 256 smoothed points (two vertices each, both sides); the subdivisions drop when a long trail needs more.
- **Checks**: unknown sockets (with a preview model: errors), `from` = `to` (no width), `length` ≤ 0 and unknown emit triggers / parameters are errors; very long trails, more than 16 subdivisions, unsorted or out-of-range curve keys, a fill along an axis warn.

The editor's 〰 Trail layer (➕ ADD) has presets "sword trail" (Trigger swing_start / swing_end; adds the triggers and two blade sockets when missing) and "energy trail" (Always), socket pickers with the preview model's nodes, a **show samples** debug toggle, and **Swing preview**: fire the start / stop triggers at two clip times of the preview animation (`preview.animation_triggers`), e.g. the start and end of the slash in a sword clip. On the MODEL stage, PREVIEW → **In place** (default on) removes the clip's horizontal root motion so the character stays on the target. Recipes "Sword trail" and "Energy trail on a moving target".

## Afterimages

An **afterimage layer** leaves fading copies ("ghosts") of the target's own meshes along their path: sword shadows along a swing, echoes of a dashing character. While it emits, every `interval` seconds it copies the meshes where they are — skinned meshes in their current pose (the copy gets frozen joints) — and each copy fades over `lifetime` with Omagari's effect material.

```ron
Afterimage((
    name: "ghosts",
    source: TargetModel,                 // TargetModel | Meshes(["Sword", "Blade.Metal"]) (mesh or mesh entity names)
    interval: 0.04,                      // seconds between copies (at most one per frame)
    lifetime: 0.25,                      // seconds each copy stays
    max_ghosts: 8,                       // most copies at once; the oldest goes first
    emit: Trigger(start: "swing_start", stop: "swing_end"),   // as for trails
    fade_over_life: [(0.0, 0.8), (1.0, 0.0)],   // (fraction of the lifetime, opacity factor)
    fade: (in: 0.0, out: 0.0),
    visible: None,
    sort_bias: 0.0,
    material: ( /* EffectMaterialDef; Age curves read the copy's age in seconds */ ),
))
```

- **Copies on screen** ≈ `lifetime / interval` (capped by `max_ghosts`; the check warns when the cap cuts copies before they fade). 4–6 copies read as a swing's shadows.
- **What is copied**: the meshes of the instance target's own model (the own-model rule, as for surface layers): a sword is copied when the package is on the sword; a weapon held in the hand is not part of the body. `Meshes([...])` picks some; with a preview model the names are checked. Copies are separate entities in the world: they stay where they were left. The model and its materials are never changed.
- **Look**: `blend: Add` with a rim gives bright energy copies; `Blend` with a dark colour gives shadow copies. Patterns stick to the copy (triplanar in its rest pose).
- **Cost**: one draw per mesh per copy; a skinned copy also keeps a frozen copy of each joint (Mixamo: ~65 small entities per copy).
- **Checks**: `interval` / `lifetime` ≤ 0 and unknown emit triggers / parameters are errors; an interval shorter than a frame, a lifetime shorter than the interval, a cap that cuts copies, an empty mesh list and age keys past the lifetime warn.

The editor's 👥 Afterimage layer (➕ ADD) has presets "afterimages" (blue copies during a swing) and "shadow dash" (dark copies while the effect runs), the same Swing preview as trails, and the mesh picker. Recipe "Afterimages".

## Lights

A **light layer** is a point light (Bevy's `PointLight`) at the attach point: it lights the floor, the character and everything around it — a weapon glow that lights the floor more as a parameter rises, a flash of light on a hit, light only during a swing, a flickering torch. Lights are not simulated: `analyze_project` describes their brightness in lumens and in lux at 1 m at every parameter's min / middle / max and over their age.

```ron
Light((
    name: "glow",                        // unique among all layers
    attach: Socket("blade_mid"),         // Target | Socket("name") | Segment(from: "a", to: "b") (the middle)
    offset: (0.0, 0.0, 0.0),             // metres in the attach point's space (× the size fit)
    color: Const((0.45, 0.7, 1.0, 1.0)), // linear RGB(A), alpha unused; Param("tint", []) reads a Color parameter
    intensity: Param("charge", [(0.0, 0.0), (1.0, 40000.0)]),   // lumens: Const, a curve over a parameter, or Age([...])
    range: Const(4.0),                   // metres: nothing farther is lit (it fades smoothly to 0 there)
    radius: 0.05,                        // metres: size of the glowing source (softer highlights only)
    flicker: (amount: 0.1, speed: 6.0),  // smooth random ±10% of the intensity, ~6 changes per second, per copy
    play: Loop,                          // Loop | OnStart | OnTrigger("name") | While("param", t) | Between(start: "a", stop: "b")
    duration: None,                      // seconds: a one-shot for OnStart / OnTrigger (a flash)
    fade: (in: 0.2, out: 0.4),
    visible: None,                       // a parameter range, like other layers
    shadows: false,                      // costly; leave off
))
```

**Units.** `intensity` is Bevy's `PointLight` intensity: luminous power in **lumens** (a 60 W bulb ≈ 800 lm). The light at distance d is about `lumens / (4π d²)` lux; the editor's sun puts ~4600 lux on the floor, so a light shows on a lit floor from ~500 lux (≈ 6 000 lm at 1 m) and dominates it from ~5 000 lux.

| Light | Intensity (lm) | Range (m) |
| :-- | :-- | :-- |
| soft glow (a charged weapon, an aura) | 3 000–20 000 | 2–4 |
| strong glow (full charge, a fire spell) | 20 000–60 000 | 3–6 |
| hit flash (0.1–0.3 s) | 50 000–200 000 | 4–8 |
| torch / campfire | 10 000–30 000, flicker 0.2–0.4 at 5–8 /s | 4–8 |

- **attach**: the target root, a socket (it follows the bone; e.g. the blade's middle) or the middle of a segment. `offset` is in the attach point's space. An `OnTrigger` light fired with `trigger_at` shines at the payload position (a flash at the contact point).
- **play** works like a shape's (section "shapes"): `OnTrigger` + `duration` is a one-shot **flash** — each trigger plays it from age 0 (`intensity: Age([(0.0, 150000.0), (0.25, 0.0)])` fades it out), a trigger during it starts it over; without a duration it stays on after the first trigger (warning). `Between(start, stop)` shines from one trigger to the other (light only during a swing); `While("param", t)` while a parameter is high; `Loop` while the effect runs. `OnStart` + `duration` is a one-shot: a package whose layers are all one-shots finishes by itself.
- **Bindings**: `color`, `intensity` and `range` are constants, curves over a number parameter (a glow that grows with `charge`), or curves over the age; a colour may follow a Color parameter directly (`Param("tint", [])`). Keep the colour's brightest channel at about 1: brightness comes from `intensity`.
- **flicker**: smooth noise on the intensity between `1 − amount` and `1 + amount`, about `speed` changes per second, different for each copy of the effect (a seed per instance).
- **fade**: in when it starts, out when it stops; when the game stops the effect it fades out over `fade.out` (the effect waits). A flash fades inside its duration.
- **Size fit**: range and radius scale with the fit, the intensity with fit² (a scaled-up copy lights its surroundings the same way).
- **Global cap**: the game shows at most `VfxSettings::max_lights` Omagari lights at once (default 8, every effect together); the weakest (then the oldest) go dark — a light pushed out while it shines fades out over `light_cap_fade` (0.25 s) — so keep one to three lights per effect (the check warns above three).
- **shadows**: `true` renders six shadow maps for the light every frame (per copy of the effect): a warning; effect lights rarely need them.
- **Checks**: intensity below 0, range ≤ 0, a negative radius, unknown triggers and parameters (a Color parameter only for `color`) are errors; a light that never shines, a peak under 200 lm (too dim to see) or over 1 000 000 lm (blinding), ranges over 25 m (costly), a radius larger than the range, flicker outside 0..1 or with speed 0 or above 40 /s, `shadows: true`, an `OnTrigger` light without a duration, age keys past the duration and more than three lights in a package warn.

The editor's 💡 Light layer (➕ ADD) has presets "weapon glow" (blue, growing with the first number parameter), "hit flash" (warm, 0.25 s on the first trigger) and "torch flicker" (orange, flickering), every value with tooltips and units, and draws the lights in the viewport (a star at the light, a circle at its range; the selected layer's range sphere). While a project has a light layer, the editor's floor is lit (FLOOR, CAPSULE and MODEL stages; the dark room shows light best), and the model is lit as in the game. Recipes "Weapon glow growing with a parameter" and "Hit flash light".

## Distortion

A **distortion layer** bends what is behind it: heat haze around a hot blade, the ripple of a shockwave, the air shimmering around a body. It draws a volume that shows the picture behind it moved by scrolling noise (and, with `refraction`, by its own shape like a lens). Distortion is not simulated: `analyze_project` describes the volume, how far the picture moves (in pixels of a 1080 px tall view) at every parameter's min / middle / max and over its age.

```ron
Distortion((
    name: "haze",                                   // unique among all layers
    attach: Segment(from: "blade_base", to: "blade_tip"),   // Target | Socket("name") | Segment(from: "a", to: "b")
    source: Shape(shape: Capsule(radius: 0.12)),    // Shape(shape, follow, transform) | Surface(targets, offset)
    scale: Const((1.0, 1.0, 1.0)),                  // shape sources: Const, a curve over a parameter, or Age([...])
    strength: Param("heat", [(0.0, 0.0), (1.0, 0.01)]),     // largest shift, FRACTION OF THE VIEW HEIGHT
    noise: (texture: (asset: "noise"), mapping: Triplanar, scale: 1.2, scroll: (0.0, 0.7), contrast: 1.0, weight: 1.0),
    noise_b: Some((texture: (asset: "noise"), scale: 3.0, scroll: (0.15, 1.1), weight: 0.35)),  // optional second noise
    edge: Soft,                                     // Soft (strongest in the middle) | Rim (a band inside the outline)
    edge_softness: 0.7,                             // 0..1
    refraction: 0.0,                                // 0..1: share of the shift along the surface's screen direction (a lens)
    tint: Const((1.08, 1.0, 0.92, 0.35)),           // rgb multiplied into what is behind, alpha = amount; Param("c", []) reads a Color parameter
    play: Loop,                                     // Loop | OnStart | OnTrigger("name") | While("param", t) | Between(start: "a", stop: "b")
    duration: None,                                 // seconds: a one-shot for OnStart / OnTrigger (a ripple)
    fade: (in: 0.2, out: 0.4),
    visible: None,
    sort_bias: 0.0,                                 // only where two distortion volumes overlap
))
```

**What bends.** Distortion uses Bevy's screen-space transmission: a copy of the screen taken after the **opaque** things are drawn. So it bends the floor, walls, the character's and weapon's models (and opaque particles); **transparent particles and Omagari's see-through layers (surfaces, shapes, trails, afterimages) are drawn after it and are not bent** — they stay sharp, also where they are behind the distortion (the check notes it when the package has such layers). Particles cannot be distortion sources: put a shape around them. On a flat, single-coloured background nothing visibly bends; in the editor the room's grid is painted into the walls while a project has a distortion layer, and the MODEL / CAPSULE / FLOOR stages show it best.

**Units.** `strength` is the largest shift as a fraction of the view's height, the same on the game's orthographic camera and on the orbit camera:

| Look | strength | on a 1080 px tall view |
| :-- | :-- | :-- |
| faint heat shimmer | 0.003–0.006 | 3–6 px |
| heat haze (a hot blade, a fire) | 0.006–0.012 | 6–13 px |
| strong ripple (a shockwave, at its start) | 0.02–0.03 | 20–30 px |
| tearing (avoid) | above 0.05 | over 50 px |

The noise moves the picture by about a third of the strength on average; edges fade (`edge`). Smooth, low-scale noise (1–3 per metre) reads as heat; fine noise (5+) as crinkled glass.

- **source `Shape`**: any shape of the shapes section (`Sphere`, `Capsule`, `Cylinder`, …) placed like a shape layer (`follow`, `transform: (offset, rotation_deg)`, `scale`). With `attach: Segment(...)` a **`Capsule` spans the two sockets** (its `length` is ignored, its radius kept): heat along a blade. Other shapes sit at the segment's middle. Flat shapes (ring, quad, arc) vanish edge-on (warning); use a sphere or capsule.
- **source `Surface`**: `Surface(targets: AllMeshes, offset: 0.04)` — copies of the target's own meshes (skinned with the body, like a surface layer's shells) pushed `offset` metres out: the air around the body shimmers. `attach` is not used. Use `edge: Rim` so the shimmer sits along the outline.
- **edge**: `Soft` = strongest where the surface faces the camera (the middle of a sphere, along a capsule's axis), nothing at the outline: no visible edge. `Rim` = a band peaking `edge_softness` in from the outline, fading toward the outline and the middle: a ring on a sphere.
- **refraction**: 0 = shimmer from the noise only (haze); 0.6–1 = the shift follows the surface's direction on screen like a lens: a clean bulging ring for a shockwave (`Sphere` + `edge: Rim`). Works without a noise texture.
- **noise**: a texture slot (asset + channel) like the surface noise (section "surfaces"); `scale` repeats per metre (Triplanar, in the shape's space or the body's rest pose) or per UV square; `scroll` squares per second (`(0, 0.7)` rises like heat). Use a Noise texture (Repeat wrap); the built-in `omagari://textures/noise.png` is right. Without a noise texture and without refraction nothing moves (warning).
- **play** works like a shape's: `OnTrigger("slam")` + `duration` is a one-shot ripple (with `trigger_at` at the hit point), `Between` from one trigger to another, `While` while a parameter is high, `Loop` while the effect runs. `scale` and `strength` as curves over `Age` make a ripple grow and fade.
- **Cost**: Bevy copies the screen once per frame and camera while any distortion (or other transmissive material) is drawn, and draws the volume once. A layer whose strength × fade × visible is 0 is hidden: **no draw, no copy, no cost** — bind `strength` to a parameter and set it to 0 when the effect is calm.
- **Camera**: the game's camera needs Bevy's screen-space transmission: every `Camera3d` has `ScreenSpaceTransmission { steps: 1, .. }` by default (keep `steps` above 0); `omagari::distortion_camera()` adds it explicitly with `Hdr`. Orthographic and perspective cameras both work.
- **Checks**: dimensions ≤ 0, a negative strength, unknown triggers, parameters and noise assets are errors; a strength that is always 0, under 0.001 (invisible) or above 0.05 (tears), no noise and no refraction, a noise texture with Clamp wrap, a noise B without a texture, noise scale ≤ 0, weights / softness / refraction outside 0..1, a surface offset inside the model or far from it, a flat shape, an `OnTrigger` layer without a duration and age keys past the duration warn; transparent layers in the same package (not bent) are a note.

The editor's 🌊 Distortion layer (➕ ADD) has presets "heat haze around a blade" (a capsule between the first two sockets, stronger with the first number parameter), "shockwave ripple" (a sphere lens growing from 0.3 to 3 m over 0.6 s on the first trigger) and "aura shimmer" (the air around the body), every value with tooltips. Recipes "Blade heat haze" and "Shockwave ripple".

## Ribbons

A **ribbon** joins a particle layer's particles into a strip: bevy_hanabi draws a quad from each particle to the one before it **with the same `ribbon_id`**, in age order (the newest particle is the head, the oldest the tail). Use it for magic streaks behind flying particles, a ribbon swirling around a body, a comet tail on a socket. It is a particle layer like any other plus one init attribute; `analyze_project` describes it (number of ribbons, particles per ribbon, length).

- **`ribbon_id`** (u32, init): all particles of one ribbon share it. `U32(0)` = one ribbon per emitter (a streak behind the target or a socket, a traced path). Under a parent layer, `Operator(ParentAttr("u32_0"))` gives every parent particle its own ribbon, with the parent storing a unique value: `SetAttribute((attr: "u32_0", attr_expr: Operator(Attr("particle_counter"))))` (not the parent's `id`, which bevy_hanabi reuses when a parent dies). A random or per-particle id (`Rand`, `Attr("particle_counter")` on the ribbon itself) joins nothing: the check warns. The old `prev` / `next` attributes do nothing in bevy_hanabi 0.19 (soft-deprecated): don't set them.
- **`age`** must be set in init (`SetAttribute((attr: "age", attr_expr: Float(0.0)))`): bevy_hanabi sorts ribbons by it and refuses to build the shaders without it (the check reports it as an error).
- **Constant lifetime**: particles that die early cut the ribbon across (the check warns about a random lifetime).
- **`motion_integration: None`** for streaks that stay where they were drawn: the particles do not move; the ribbon is the path of the emitter (a moving target or socket) or of the parent particles (children inherit the parent's position each frame). Moving ribbon particles bend and stretch the ribbon (the check warns).
- **`Orient((mode: FaceCameraPosition))`**: the ribbon's width faces the camera. Without Orient it lies in the XY plane; `AlongVelocity` breaks without a velocity (both warned).
- **Width** = the particle size (`SizeOverLifetime` from the head's width to 0 tapers the tail); the length of each piece is the distance between neighbouring particles. Colour and alpha come from the usual render modifiers (fade the tail with `ColorOverLifetime`).
- **Density**: one particle per frame per ribbon (a rate of 60/s, or `EmitSpawnEventModifier((condition: Always, count_expr: U32(1), child_index: 0))` on the parent) is smooth; more particles per frame stack on one point (wasted).
- **Space**: `Global` leaves the ribbon behind a moving target (a trail); `Local` keeps it on the target (a swirl around the body).
- Textures: a `ParticleTexture` repeats once per piece (between two particles); untextured ribbons with an `Add` blend glow best.

Ribbons are **GPU particle streaks**. For a weapon's swing arc between two sockets (blade base and tip), use a **Trail layer** (section "trails"): it is a real two-edged ribbon of the blade's sweep with Omagari's effect shader, independent of particles. The editor's particle inspector has presets "Ribbon: magic streaks" (adds a ribbon child to a layer of swirling wisps), "Ribbon: swirl" and "Ribbon: streak behind the emitter" (needs a moving target or socket). Recipes "Magic streaks" and "Swirling ribbon".

## Meshes

**Mesh particles**: a particle layer's `mesh` makes every particle draw a 3D mesh instead of bevy_hanabi's 1 m quad: debris, shards, pebbles, leaves, rings.

```ron
mesh: Some(Shape(Cube)),     // built-in: Cube, Sphere, Cone, Cylinder, Shard, Torus, Quad
mesh: Some(Asset("rock")),   // a Mesh asset of the project (below)
```

- **Built-in shapes** are low-poly (12 segments; a cube and a shard 24 vertices), centred and **1 m across**, so the particle `size` is the size in metres (`size3` stretches per axis: `Vec3((0.2, 0.05, 0.1))` = a flat chip). Shard = a pointed octahedron 0.5 × 1 × 0.3 m along Y.
- **Mesh assets** are one mesh of a glTF file of the game assets folder: `(id: "rock", kind: Mesh((path: "vfx/models/debris.glb", mesh: 0, primitive: 0, name: Some("Rock"))))`. `import_asset("C:/art/debris.glb", project: "x")` copies a `.glb` into `vfx/models/` and adds its first usable mesh (pass `mesh` / `primitive` for another; `describe_model` and `list_assets` list every model's meshes as `MeshI/PrimitiveJ` with vertex counts and size). The game loads `path#MeshI/PrimitiveJ` (Bevy's glTF label), so the file ships with the game. The mesh needs texture coordinates (Omagari adds zero UVs to a mesh without, with a warning); skinned meshes draw in their rest pose. The size of the mesh in the file times the particle `size` is the size on screen (`analyze_project` says both).
- **Unlit**: bevy_hanabi draws mesh particles in one flat colour per particle (no shading). Give each particle its own colour (`hdr_color` from a `Mix` of two colours with `RandFloat`) so a burst reads as separate pieces; silhouettes change as they turn.
- **Blending**: use `alpha_mode: Opaque` (solid debris; written to the depth buffer) or `Mask`, or `Add` for glowing shards. A see-through `Blend` draws a mesh's far side over its near side (bevy_hanabi does not sort triangles): the check warns.
- **Turning**: `Orient((mode: AlongVelocity))` points the mesh's X axis along its flight (shards tumble as gravity bends their path); without Orient a mesh keeps the layer's axes (fine for meshes, unlike quads); the axis attributes (`axis_x`, `axis_y`, `axis_z`) set any orientation.
- **Cost**: every particle draws the whole mesh: vertices × capacity per frame (the check warns above 1 000 000, and about meshes over 2 000 vertices). Shrinking pieces before they die: an update `SetAttribute` of `size` from a stored size (`f32_0`) times `Saturate((lifetime − age) × 6)` (recipe "Debris burst"); `Opaque` particles cannot fade.
- In the **game camera** (16 m tall view) pieces under 5 cm vanish: use sizes of 6–15 cm.

The editor's particle inspector has a **Mesh** row (built-in shapes and the project's mesh assets) and presets "Mesh: debris burst" and "Mesh: glowing shards"; the ASSETS tab imports models (button or drag & drop) and picks the mesh of the file. Recipes "Debris burst" and "Glowing shards from a mesh asset".

## Order

**Draw order between effects.** Blended layers (`Blend`, `Add`, `Premultiply`, `Multiply`) are drawn back to front, each sorted as one piece by the distance of its centre from the camera: a particle layer by its **emitter** (the layer's attach point, not where its particles are), shape layers by the shape's centre, trails and afterimages by their entity, surface overlays by each mesh. Opaque and Mask layers are depth-tested and need no order.

- **`sort_bias`** (metres, default 0, every particle / shape / trail / afterimage layer) is added to that distance: positive = drawn later, on top of transparent things up to that far in front; negative = earlier, below. Use a few tenths of a metre to a few metres: sparks over a shield bubble at the same point (`sort_bias: 0.5` on the sparks), smoke under a glow (`sort_bias: -0.5` on the smoke). It is per layer, not per particle: particles inside one layer are not sorted against each other.
- When two layers overlap in space they cannot both be in front everywhere; additive layers (`Add`) look the same in any order, so prefer `Add` for glows and keep the bias for `Blend` layers (smoke, dust, shields).
- `sort_bias` has no effect on `Opaque` / `Mask` layers, and over ±50 m it moves the layer past everything (both warned). bevy_hanabi 0.19 has no draw-order setting of its own in 3D (its `z_layer_2d` is 2D only); Omagari sets the bias on bevy_hanabi's draws.
- The editor: **Draw order** row in the particle, shape and trail inspectors.

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

### Status skin (surface: scrolling noise, rim, flash on tick)

```ron
(
    version: 2,
    parameters: [
        (name: "flash", default: Float(0.0), min: 0.0, max: 1.0, smoothing: 0.0),
    ],
    triggers: [
        (name: "tick", pulses: [(param: "flash", to: 1.0, decay: 0.35)]),
    ],
    assets: [(id: "noise", kind: Texture((path: "omagari://textures/noise.png", purpose: Noise)))],
    layers: [
        Surface((
            name: "poison_skin",
            targets: AllMeshes,
            material: (
                blend: Blend,
                color: Const((0.55, 0.08, 0.95, 0.7)),
                opacity: Const(1.0),
                emissive: Const(1.5),
                noise_a: Some((texture: (asset: "noise"), mapping: Triplanar, scale: Const(3.0), scroll: Const((0.0, 0.25)), strength: Const(0.85), contrast: Const(2.5))),
                rim: Some((color: Const((1.0, 0.45, 1.0, 0.8)), power: Const(2.5), intensity: Const(1.5))),
                pulse: Some((frequency: Const(0.8), amount: Const(0.25))),
                flash: Some(Param("flash", [(0.0, 0.0), (1.0, 1.0)])),
            ),
            shells: [(offset: 0.0)],
            fade: (in: 0.3, out: 0.6),
        )),
    ],
    preview: Some((stage: Capsule, auto_triggers: [("tick", 1.0)])),
)
```

A purple skin over the whole body: cloudy noise crawling up the body (it sticks to the limbs while the character runs), a glowing pink rim, a slow breathing, and a bright flash on every `vfx.trigger(instance, "tick")` (a damage tick). The model shows through (opacity ~0.4 on average). Preview on `capsule`, or on `model` with the game's character.

### Energy shell (surface: three additive shells with volume)

```ron
(
    version: 2,
    assets: [(id: "noise", kind: Texture((path: "omagari://textures/noise.png", purpose: Noise)))],
    layers: [
        Surface((
            name: "energy",
            targets: AllMeshes,
            material: (
                blend: Add,
                color: Const((0.2, 0.8, 2.0, 0.5)),
                opacity: Const(1.0),
                emissive: Const(1.5),
                noise_a: Some((texture: (asset: "noise"), mapping: Triplanar, scale: Const(4.0), scroll: Const((0.1, 0.6)), strength: Const(0.9), contrast: Const(2.0))),
                noise_b: Some((texture: (asset: "noise"), mapping: Triplanar, scale: Const(1.5), scroll: Const((0.0, -0.3)), strength: Const(0.5))),
                rim: Some((color: Const((0.5, 1.5, 3.0, 0.6)), power: Const(1.5), intensity: Const(1.5))),
                pulse: Some((frequency: Const(1.2), amount: Const(0.3))),
            ),
            shells: [
                (offset: 0.004, opacity: 0.7),
                (offset: 0.016, opacity: 0.4, noise_offset: 0.37),
                (offset: 0.03, opacity: 0.2, noise_offset: 0.74),
            ],
            fade: (in: 0.25, out: 0.5),
        )),
    ],
    preview: Some((stage: Capsule)),
)
```

A cyan energy field with thickness: three copies of the body 0.5, 2 and 3.5 cm outside the skin, each fainter, with two noises moving against each other and a bright rim, so the silhouette glows ~3.5 cm wide. Additive: it brightens the model and never hides it. Looks best with bloom (GAME camera, dark backdrop).

### Frost from the feet up (surface: fill driven by a parameter)

```ron
(
    version: 2,
    parameters: [
        (name: "frost", default: Float(0.0), min: 0.0, max: 1.0, smoothing: 0.5),
    ],
    assets: [(id: "noise", kind: Texture((path: "omagari://textures/noise.png", purpose: Noise)))],
    layers: [
        Surface((
            name: "frost",
            targets: AllMeshes,
            material: (
                blend: Blend,
                color: Const((0.7, 0.9, 1.0, 0.75)),
                opacity: Const(1.0),
                emissive: Const(1.2),
                noise_a: Some((texture: (asset: "noise"), mapping: Triplanar, scale: Const(6.0), scroll: Const((0.0, 0.0)), strength: Const(0.6))),
                rim: Some((color: Const((0.8, 0.95, 1.0, 0.6)), power: Const(3.0), intensity: Const(1.2))),
                fill: Some((axis: Y, from: Const(0.0), to: Param("frost", [(0.0, 0.0), (1.0, 1.0)]), softness: Const(0.08))),
            ),
            shells: [(offset: 0.003)],
            fade: (in: 0.2, out: 0.8),
        )),
    ],
    preview: Some((stage: Capsule, params: [("frost", Float(0.5))])),
)
```

Pale blue frost covering the model from the feet up to `frost` × its height (0 = none, 0.5 = up to the hips, 1 = all), with a soft edge and a still, crystalline noise. The game raises `frost` with `vfx.set_param(instance, "frost", value)`; smoothing 0.5 s makes it creep. The PARAMETERS tab's slider (or "animate") shows it rising.

### Shield bubble (shape: sphere, opacity from a parameter)

```ron
(
    version: 2,
    parameters: [
        (name: "shield", default: Float(1.0), min: 0.0, max: 1.0, smoothing: 0.3),
    ],
    assets: [(id: "noise", kind: Texture((path: "omagari://textures/noise.png", purpose: Noise)))],
    layers: [
        Shape((
            name: "bubble",
            attach: Target,
            follow: Position,
            shape: Sphere(radius: 1.1),
            scale: Param("shield", [(0.0, (0.9, 0.9, 0.9)), (1.0, (1.0, 1.0, 1.0))]),
            rotation_speed: Const(12.0),
            play: Loop,
            fade: (in: 0.3, out: 0.4),
            material: (
                blend: Add,
                color: Const((0.3, 0.7, 1.6, 0.25)),
                opacity: Param("shield", [(0.0, 0.0), (1.0, 1.0)]),
                emissive: Const(1.0),
                noise_a: Some((texture: (asset: "noise"), mapping: Triplanar, scale: Const(2.0), scroll: Const((0.05, 0.15)), strength: Const(0.6), contrast: Const(1.5))),
                rim: Some((color: Const((0.5, 1.2, 2.5, 0.8)), power: Const(2.5), intensity: Param("shield", [(0.0, 0.0), (1.0, 2.0)]))),
                pulse: Some((frequency: Const(0.7), amount: Const(0.2))),
            ),
        )),
    ],
    preview: Some((stage: Capsule, params: [("shield", Float(1.0))])),
)
```

A faint blue sphere 2.2 m across around the character, with a bright edge where it curves away from the camera, slow drifting noise, a gentle breathing and a slow turn. The game sets `shield` 0..1 (`vfx.set_param(instance, "shield", hp_fraction)`): at 0 the bubble is gone (opacity and rim 0) and 10% smaller, so it shrinks as it fades; smoothing 0.3 s eases the change. Additive, so the character stays visible inside.

### Ground ring (shape: flat ring at the feet, stays flat, pulses)

```ron
(
    version: 2,
    assets: [(id: "noise", kind: Texture((path: "omagari://textures/noise.png", purpose: Noise)))],
    layers: [
        Shape((
            name: "ring",
            attach: Target,
            follow: PositionYaw,
            shape: Ring(inner: 0.55, outer: 0.75),
            transform: (offset: (0.0, -0.88, 0.0)),
            scale: Age([(0.0, (1.0, 1.0, 1.0)), (0.6, (1.08, 1.0, 1.08)), (1.2, (1.0, 1.0, 1.0))]),
            rotation_speed: Const(30.0),
            play: Loop,
            duration: Some(1.2),
            fade: (in: 0.3, out: 0.4),
            material: (
                blend: Add,
                color: Const((1.6, 0.7, 0.2, 0.8)),
                noise_a: Some((texture: (asset: "noise"), mapping: Uv, scale: Const(3.0), scroll: Const((0.1, 0.0)), strength: Const(0.5), contrast: Const(1.5))),
                pulse: Some((frequency: Const(0.83), amount: Const(0.35))),
                fill: Some((axis: V, from: Const(0.3), to: Const(0.7), softness: Const(0.4))),
            ),
        )),
    ],
    preview: Some((stage: Capsule, motion: Some(Circle(radius: 3.0, speed: 3.6)))),
)
```

An orange ring on the ground around the feet (0.55–0.75 m from the centre, 2 cm above the floor), with soft inner and outer edges, turning 30°/s and breathing every 1.2 s (it grows 8% and brightens; the age repeats every `duration`). `PositionYaw` keeps it level and turned with the character while it runs and leans. A status marker or an aura's base.

### Shockwave (shape: ring growing over its age, played by a trigger)

```ron
(
    version: 2,
    triggers: [(name: "slam")],
    layers: [
        Shape((
            name: "shockwave",
            attach: Target,
            follow: PositionYaw,
            shape: Ring(inner: 0.8, outer: 1.0),
            transform: (offset: (0.0, -0.86, 0.0)),
            scale: Age([(0.0, (0.2, 1.0, 0.2)), (0.6, (3.0, 1.0, 3.0))]),
            play: OnTrigger("slam"),
            duration: Some(0.6),
            fade: (in: 0.0, out: 0.3),
            material: (
                blend: Add,
                color: Const((0.6, 1.2, 2.5, 1.0)),
                emissive: Const(1.5),
                fill: Some((axis: V, from: Const(0.3), to: Const(0.7), softness: Const(0.4))),
            ),
        )),
    ],
    preview: Some((stage: Capsule, auto_triggers: [("slam", 1.5)])),
)
```

Each `vfx.trigger(instance, "slam")` sends a bright cyan ring along the ground from 0.4 m to 6 m across in 0.6 s, fading out over its second half; a new trigger during the wave starts it over. With `trigger_at(instance, "slam", TriggerArgs::at(point))` it starts at that point. Nothing shows between triggers.

### Slash arc (shape: arc swept by fill over its age, played by a trigger)

```ron
(
    version: 2,
    triggers: [(name: "swing")],
    assets: [(id: "noise", kind: Texture((path: "omagari://textures/noise.png", purpose: Noise)))],
    layers: [
        Shape((
            name: "slash",
            attach: Target,
            follow: Full,
            shape: Arc(radius: 1.0, width: 0.35, angle_deg: 150.0, taper: 1.0),
            transform: (offset: (0.0, 0.2, 0.0), rotation_deg: (0.0, 0.0, -25.0)),
            play: OnTrigger("swing"),
            duration: Some(0.35),
            fade: (in: 0.0, out: 0.1),
            material: (
                blend: Add,
                color: Const((2.5, 2.2, 1.6, 1.0)),
                emissive: Const(1.5),
                noise_a: Some((texture: (asset: "noise"), mapping: Uv, scale: Const(2.0), scroll: Const((-3.0, 0.0)), strength: Const(0.5), contrast: Const(2.0))),
                fill: Some((axis: U, from: Age([(0.1, 0.0), (0.35, 1.0)]), to: Age([(0.0, 0.0), (0.15, 1.0)]), softness: Const(0.25))),
            ),
        )),
    ],
    preview: Some((stage: Capsule, auto_triggers: [("swing", 1.0)])),
)
```

A white-hot crescent 1 m in front of the chest, tilted 25°, pointed at both ends: on each `swing` trigger it sweeps across from the left end in 0.15 s (fill `to` over the age), its tail follows from 0.1 s (fill `from`) and it is gone at 0.35 s, with streaky noise running along it. Mirror it with `rotation_deg: (0.0, 180.0, 25.0)` for a swing the other way. For an arc that follows the real blade through the animation, use a trail layer (section "trails", recipe "Sword trail").

### Sword trail (trail between two blade sockets, started and stopped by triggers)

```ron
(
    version: 2,
    triggers: [(name: "swing_start"), (name: "swing_end")],
    assets: [(id: "noise", kind: Texture((path: "omagari://textures/noise.png", purpose: Noise)))],
    sockets: [
        (name: "blade_base", node: None, offset: (0.3, 0.2, -0.3)),
        (name: "blade_tip", node: None, offset: (0.3, 1.1, -0.6)),
    ],
    layers: [
        Trail((
            name: "slash",
            from: "blade_base",
            to: "blade_tip",
            length: 0.22,
            subdivisions: 6,
            min_sample_distance: 0.01,
            emit: Trigger(start: "swing_start", stop: "swing_end"),
            width_over_u: [(0.0, 1.0), (1.0, 0.6)],
            fade_over_u: [(0.0, 1.0), (0.2, 0.85), (1.0, 0.0)],
            material: (
                blend: Add,
                color: Const((2.4, 2.0, 1.5, 1.0)),
                emissive: Const(1.2),
                noise_a: Some((texture: (asset: "noise"), mapping: Uv, scale: Const(2.0), scroll: Const((-2.0, 0.0)), strength: Const(0.4), contrast: Const(1.5))),
                fill: Some((axis: V, from: Const(0.0), to: Const(1.0), softness: Const(0.25))),
            ),
        )),
    ],
    preview: Some((stage: Capsule, motion: Some(Circle(radius: 3.0, speed: 3.6)))),
)
```

A white-hot arc behind the blade from `swing_start` to `swing_end`: 0.22 s long (≈ 2 m at a 10 m/s tip), 0.95 m wide at the blade and 60% at its end, fading out along it, soft at both edges, with streaks running toward the tail. Put the package on the weapon entity and move the two sockets onto the blade (offsets from the weapon's root, or a weapon node); in the game fire `swing_start` / `swing_end` from the attack state. In the editor fire them in the TRIGGERS tab while the capsule runs, or on the MODEL stage with a sword clip set the Swing preview's clip times (the sockets then go on `mixamorig:RightHand` or the weapon).

### Energy trail on a moving target (trail, always on)

```ron
(
    version: 2,
    assets: [(id: "noise", kind: Texture((path: "omagari://textures/noise.png", purpose: Noise)))],
    sockets: [
        (name: "tail_low", node: None, offset: (0.0, -0.35, 0.1)),
        (name: "tail_high", node: None, offset: (0.0, 0.35, 0.1)),
    ],
    layers: [
        Trail((
            name: "comet",
            from: "tail_low",
            to: "tail_high",
            length: 0.5,
            subdivisions: 4,
            min_sample_distance: 0.02,
            emit: Always,
            width_over_u: [(0.0, 1.0), (1.0, 0.1)],
            fade_over_u: [(0.0, 0.9), (1.0, 0.0)],
            material: (
                blend: Add,
                color: Const((0.4, 1.2, 2.5, 0.9)),
                noise_a: Some((texture: (asset: "noise"), mapping: Uv, scale: Const(1.5), scroll: Const((-1.5, 0.0)), strength: Const(0.6), contrast: Const(2.0))),
            ),
        )),
    ],
    preview: Some((stage: Capsule, motion: Some(Circle(radius: 3.0, speed: 3.6)))),
)
```

A cyan ribbon 0.7 m tall behind the character's middle that always follows it for 0.5 s (≈ 1.8 m at a run), tapering to a point and fading, with flowing noise: a speed or power-up streak. On a still target there is no trail (it has no length); it grows with the speed.

### Sword sparks during a swing (segment + held emission between two triggers)

```ron
(
    version: 2,
    triggers: [(name: "swing_start"), (name: "swing_end")],
    assets: [(id: "spark1", kind: Texture((path: "omagari://textures/spark1.png")))],
    sockets: [
        (name: "blade_base", node: None, offset: (0.3, 0.2, -0.3)),
        (name: "blade_tip", node: None, offset: (0.3, 1.1, -0.6)),
    ],
    layers: [
        Particles((
            name: "swing_sparks",
            attach: Segment(from: "blade_base", to: "blade_tip"),
            simulation_space: Global,
            play: Between(start: "swing_start", stop: "swing_end"),
            texture: Some("spark1"),
            effect: (
                capacity: 128,
                spawner: (count: Single(150.0), spawn_duration: Single(1.0), period: Single(1.0), cycle_count: 0, starts_active: true, emit_on_start: true),
                alpha_mode: Add,
                init_modifiers: [
                    SetAttribute((attr: "age", attr_expr: Float(0.0))),
                    SetAttribute((attr: "lifetime", attr_expr: Operator(Uniform(Float(0.2), Float(0.45))))),
                    SetAttribute((attr: "size", attr_expr: Float(0.05))),
                    SetAttribute((attr: "position", attr_expr: Operator(Mix(Property("segment_a"), Property("segment_b"), RandFloat)))),
                    SetAttribute((attr: "velocity", attr_expr: Operator(Multiply(Operator(Subtract(Operator(Multiply(RandVec3, Float(2.0))), Float(1.0))), Float(1.5))))),
                ],
                update_modifiers: [
                    AccelModifier((accel_expr: Vec3((0.0, -6.0, 0.0)))),
                    LinearDragModifier((drag_expr: Float(2.0))),
                ],
                render_modifiers: [
                    ColorOverLifetime((
                        gradient: (g: [(0.0, (6.0, 3.5, 1.2, 1.0)), (1.0, (2.0, 0.6, 0.1, 0.0))]),
                        blend: Some(Overwrite),
                        mask: Some((15)),
                    )),
                    ParticleTexture((sample_mapping: ModulateOpacityFromR)),
                    Orient((mode: AlongVelocity)),
                ],
            ),
        )),
    ],
    preview: Some((stage: Capsule, camera: Game)),
)
```

Hot sparks shed along the whole blade (a random point between its base and tip each) from the moment the game fires `swing_start` until it fires `swing_end`, ~150 per second, falling and fading over 0.2–0.45 s; nothing between swings and no parameter to drive. `Global`: the sparks stay where the blade left them, so a fast swing leaves a streak of them. In the editor fire the two triggers in the TRIGGERS tab, or on the MODEL stage with a sword clip set `animation_triggers: [("swing_start", 0.78), ("swing_end", 1.08)]` (a trail layer's Swing preview does it) and move the sockets onto the hand bone or the weapon. Add a trail with `emit: Trigger(start: "swing_start", stop: "swing_end")` for the arc and a light with `play: Between(start: "swing_start", stop: "swing_end")` for a glow during the swing.

### Weapon glow growing with a parameter (light)

```ron
(
    version: 2,
    parameters: [(name: "charge", default: Float(0.0), min: 0.0, max: 1.0, smoothing: 0.3)],
    sockets: [(name: "blade_mid", node: None, offset: (0.3, 0.6, -0.4))],
    layers: [
        Light((
            name: "glow",
            attach: Socket("blade_mid"),
            color: Const((0.45, 0.7, 1.0, 1.0)),
            intensity: Param("charge", [(0.0, 0.0), (0.2, 4000.0), (1.0, 40000.0)]),
            range: Param("charge", [(0.0, 2.0), (1.0, 4.5)]),
            radius: 0.05,
            flicker: (amount: 0.08, speed: 5.0),
            fade: (in: 0.2, out: 0.4),
        )),
    ],
    preview: Some((stage: Capsule, params: [("charge", Float(0.8))])),
)
```

A cool blue light at the blade that is dark at `charge` 0, a faint glow at 0.2 (4 000 lm) and lights the floor and the character strongly at 1 (40 000 lm ≈ 3 200 lux at 1 m, reaching 4.5 m), with a faint shimmer; the game sets `charge` (smoothed over 0.3 s). Put the package on the weapon and the socket on the blade (a weapon node or an offset from the hand bone). Pair it with a surface or shape glow on the blade itself; the light is what makes the floor glow. Preview on the dark room (BG) to judge it.

### Hit flash light (light, one-shot per trigger)

```ron
(
    version: 2,
    triggers: [(name: "hit")],
    layers: [
        Light((
            name: "flash",
            play: OnTrigger("hit"),
            duration: Some(0.25),
            fade: (in: 0.0, out: 0.05),
            color: Const((1.0, 0.85, 0.6, 1.0)),
            intensity: Age([(0.0, 150000.0), (0.05, 120000.0), (0.25, 0.0)]),
            range: Const(5.0),
            radius: 0.1,
        )),
    ],
    preview: Some((stage: Capsule, auto_triggers: [("hit", 1.0)])),
)
```

A warm white flash of 150 000 lm that drops quickly and is gone after 0.25 s, each time the game fires `hit` — at the contact point with `vfx.trigger_at(instance, "hit", TriggerArgs::at(contact))`, else at the target. Add it to a hit burst package (recipe "Hit burst at the contact point") so the sparks light up the floor and the target.

### Blade heat haze (distortion: capsule between two sockets, strength from a parameter)

```ron
(
    version: 2,
    parameters: [(name: "heat", default: Float(0.0), min: 0.0, max: 1.0, smoothing: 0.2)],
    assets: [(id: "noise", kind: Texture((path: "omagari://textures/noise.png", purpose: Noise)))],
    sockets: [
        (name: "blade_base", node: None, offset: (0.35, 0.1, -0.3)),
        (name: "blade_tip", node: None, offset: (0.35, 1.0, -0.3)),
    ],
    layers: [
        Distortion((
            name: "haze",
            attach: Segment(from: "blade_base", to: "blade_tip"),
            source: Shape(shape: Capsule(radius: 0.12)),
            strength: Param("heat", [(0.0, 0.0), (1.0, 0.01)]),
            noise: (texture: (asset: "noise"), scale: 1.2, scroll: (0.0, 0.7)),
            noise_b: Some((texture: (asset: "noise"), scale: 3.0, scroll: (0.15, 1.1), weight: 0.35)),
            edge_softness: 0.7,
            tint: Const((1.08, 1.0, 0.92, 0.35)),
            fade: (in: 0.2, out: 0.4),
        )),
    ],
    preview: Some((stage: Capsule, params: [("heat", Float(1.0))])),
)
```

Rising heat haze in a 24 cm thick capsule from the blade's base to its tip: nothing at `heat` 0 (the layer is not drawn and costs nothing), a shimmer of up to ~11 px on a 1080 px view at 1, slightly warm. The game sets `heat` (smoothed over 0.2 s), e.g. from a weapon's charge or a fire enchantment. Put the package on the weapon (or the hand) and move the sockets onto the blade (a weapon node, or offsets from the hand bone: on the Y Bot `mixamorig:RightHand` (0.08, 0.06, 0.02) → (0.95, 0.06, 0.02)). It bends the floor, walls and models behind the blade, not particles; add a glow (surface / shape / light) for the blade itself.

### Shockwave ripple (distortion: a sphere lens growing over its age, played by a trigger)

```ron
(
    version: 2,
    triggers: [(name: "slam")],
    assets: [(id: "noise", kind: Texture((path: "omagari://textures/noise.png", purpose: Noise)))],
    layers: [
        Distortion((
            name: "ripple",
            source: Shape(shape: Sphere(radius: 1.0), follow: Position),
            scale: Age([(0.0, (0.3, 0.3, 0.3)), (0.6, (3.0, 3.0, 3.0))]),
            strength: Age([(0.0, 0.03), (0.6, 0.0)]),
            noise: (texture: (asset: "noise"), scale: 1.5, scroll: (0.0, 0.0)),
            edge: Rim,
            edge_softness: 0.35,
            refraction: 0.75,
            play: OnTrigger("slam"),
            duration: Some(0.6),
            fade: (in: 0.0, out: 0.1),
        )),
    ],
    preview: Some((stage: Capsule, auto_triggers: [("slam", 1.5)])),
)
```

Each time the game fires `slam` a ring of bent air grows from 0.6 m to 6 m across in 0.6 s, bulging like a lens at the start (~32 px on a 1080 px view) and flattening out as it grows; at the hit point with `vfx.trigger_at(instance, "slam", TriggerArgs::at(point))`. Pair it with the "Shockwave" shape recipe (a glowing ring on the ground) and a hit flash light for a full impact.

### Afterimages (fading copies of the target's meshes)

```ron
(
    version: 2,
    layers: [
        Afterimage((
            name: "echoes",
            source: TargetModel,
            interval: 0.06,
            lifetime: 0.3,
            max_ghosts: 6,
            emit: Always,
            fade_over_life: [(0.0, 0.7), (1.0, 0.0)],
            material: (
                blend: Add,
                color: Const((0.3, 0.6, 1.6, 0.5)),
                rim: Some((color: Const((0.6, 1.0, 2.5, 1.0)), power: Const(2.0), intensity: Const(1.5))),
            ),
        )),
    ],
    preview: Some((stage: Capsule, motion: Some(Circle(radius: 3.0, speed: 3.6)))),
)
```

Blue glowing copies of the target's meshes every 0.06 s, each fading from 70% to nothing over 0.3 s: 5 echoes on screen behind a moving character (here the capsule; in the game the model the package is on, skinned characters in their pose). For a sword's shadows put it on the weapon with `emit: Trigger(start: "swing_start", stop: "swing_end")`, `interval: 0.04`, `lifetime: 0.25`.

### Magic streaks (ribbons following swirling particles)

```ron
(
    version: 2,
    layers: [
        Particles((
            name: "wisps",
            simulation_space: Local,
            play: Loop,
            effect: (
                capacity: 32,
                spawner: (count: Single(8.0), spawn_duration: Single(1.0), period: Single(1.0), cycle_count: 0, starts_active: true, emit_on_start: true),
                alpha_mode: Add,
                init_modifiers: [
                    SetAttribute((attr: "age", attr_expr: Float(0.0))),
                    SetAttribute((attr: "lifetime", attr_expr: Float(1.6))),
                    SetAttribute((attr: "size", attr_expr: Float(0.07))),
                    SetAttribute((attr: "u32_0", attr_expr: Operator(Attr("particle_counter")))),
                    SetPositionCircle((center_expr: Vec3((0.0, -0.6, 0.0)), axis_expr: Vec3((0.0, 1.0, 0.0)), radius_expr: Float(0.5), dimension: Surface)),
                    SetVelocityTangent((origin_expr: Vec3((0.0, 0.0, 0.0)), axis_expr: Vec3((0.0, 1.0, 0.0)), speed_expr: Float(2.0))),
                ],
                update_modifiers: [
                    TangentAccel((origin_expr: Vec3((0.0, 0.0, 0.0)), axis_expr: Vec3((0.0, 1.0, 0.0)), accel_expr: Float(0.5))),
                    RadialAccel((origin_expr: Vec3((0.0, 0.0, 0.0)), accel_expr: Float(-8.0))),
                    AccelModifier((accel_expr: Vec3((0.0, 0.9, 0.0)))),
                    EmitSpawnEventModifier((condition: Always, count_expr: U32(1), child_index: 0)),
                ],
                render_modifiers: [
                    ColorOverLifetime((
                        gradient: (g: [(0.0, (2.0, 4.0, 6.0, 0.0)), (0.15, (2.0, 4.0, 6.0, 1.0)), (1.0, (1.5, 0.6, 5.0, 0.0))]),
                        blend: Some(Overwrite),
                        mask: Some((15)),
                    )),
                    Round((roundness_expr: Float(1.0))),
                    Orient((mode: FaceCameraPosition)),
                ],
            ),
        )),
        Particles((
            name: "streaks",
            parent: Some("wisps"),
            simulation_space: Local,
            play: Loop,
            effect: (
                capacity: 512,
                spawner: (count: Single(1.0), spawn_duration: Single(0.0), period: Single(0.0), cycle_count: 1, starts_active: true, emit_on_start: true),
                alpha_mode: Add,
                motion_integration: None,
                init_modifiers: [
                    InheritAttribute((attr: "position")),
                    SetAttribute((attr: "ribbon_id", attr_expr: Operator(ParentAttr("u32_0")))),
                    SetAttribute((attr: "age", attr_expr: Float(0.0))),
                    SetAttribute((attr: "lifetime", attr_expr: Float(0.4))),
                ],
                update_modifiers: [],
                render_modifiers: [
                    ColorOverLifetime((
                        gradient: (g: [(0.0, (1.5, 3.0, 6.0, 1.0)), (1.0, (1.5, 0.4, 4.0, 0.0))]),
                        blend: Some(Overwrite),
                        mask: Some((15)),
                    )),
                    SizeOverLifetime((gradient: (g: [(0.0, (0.07, 0.07, 0.07)), (1.0, (0.0, 0.0, 0.0))]))),
                    Orient((mode: FaceCameraPosition)),
                ],
            ),
        )),
    ],
    preview: Some((stage: Capsule)),
)
```

Eight glowing wisps a second start at the feet, circle the body (TangentAccel + RadialAccel) and rise; each drags a tapering ribbon of the last 0.4 s of its path. The ribbon id is the wisp's own `u32_0` (set from `particle_counter`), so every wisp has its own ribbon. A longer child `lifetime` = longer streaks; a wider first SizeOverLifetime key = thicker streaks (keep ≥ 0.05 m for the game camera).

### Swirling ribbon (one ribbon traced around the target)

```ron
(
    version: 2,
    layers: [
        Particles((
            name: "ribbon",
            simulation_space: Local,
            play: Loop,
            effect: (
                capacity: 128,
                spawner: (count: Single(60.0), spawn_duration: Single(1.0), period: Single(1.0), cycle_count: 0, starts_active: true, emit_on_start: true),
                alpha_mode: Add,
                motion_integration: None,
                init_modifiers: [
                    SetAttribute((attr: "position", attr_expr: Operator(Vec3(
                        Operator(Multiply(Operator(Cos(Operator(Multiply(Time, Float(4.0))))), Float(0.6))),
                        Operator(Multiply(Operator(Sin(Operator(Multiply(Time, Float(1.3))))), Float(0.5))),
                        Operator(Multiply(Operator(Sin(Operator(Multiply(Time, Float(4.0))))), Float(0.6))),
                    )))),
                    SetAttribute((attr: "age", attr_expr: Float(0.0))),
                    SetAttribute((attr: "lifetime", attr_expr: Float(1.0))),
                    SetAttribute((attr: "ribbon_id", attr_expr: U32(0))),
                ],
                update_modifiers: [],
                render_modifiers: [
                    ColorOverLifetime((
                        gradient: (g: [(0.0, (6.0, 3.5, 1.0, 1.0)), (1.0, (3.0, 0.6, 0.2, 0.0))]),
                        blend: Some(Overwrite),
                        mask: Some((15)),
                    )),
                    SizeOverLifetime((gradient: (g: [(0.0, (0.1, 0.1, 0.1)), (1.0, (0.0, 0.0, 0.0))]))),
                    Orient((mode: FaceCameraPosition)),
                ],
            ),
        )),
    ],
    preview: Some((stage: Capsule)),
)
```

Sixty particles a second are born on a moving point (the init position is a function of `Time`: around the target at 0.6 m, going up and down by 0.5 m) and stay there (`motion_integration: None`); one ribbon (`ribbon_id` 0) joins the last second of them into a golden band that winds around the body and fades at its tail. Faster `Time` factors = a tighter spiral. For a streak that follows a moving hand instead, attach the layer to a socket, set `position` to `Vec3((0.0, 0.0, 0.0))` and use `simulation_space: Global`.

### Debris burst (mesh particles: tumbling cubes, one-shot)

```ron
(
    version: 2,
    layers: [
        Particles((
            name: "debris",
            simulation_space: Local,
            play: OnStart,
            mesh: Some(Shape(Cube)),
            effect: (
                capacity: 32,
                spawner: (count: Single(24.0), spawn_duration: Single(0.0), period: Single(1.0), cycle_count: 1, starts_active: true, emit_on_start: true),
                alpha_mode: Opaque,
                init_modifiers: [
                    SetAttribute((attr: "age", attr_expr: Float(0.0))),
                    SetAttribute((attr: "lifetime", attr_expr: Operator(Uniform(Float(0.8), Float(1.1))))),
                    SetAttribute((attr: "f32_0", attr_expr: Operator(Uniform(Float(0.06), Float(0.12))))),
                    SetAttribute((attr: "size", attr_expr: Operator(Attr("f32_0")))),
                    SetAttribute((attr: "hdr_color", attr_expr: Operator(Mix(Vec4((0.16, 0.11, 0.07, 1.0)), Vec4((0.55, 0.45, 0.33, 1.0)), RandFloat)))),
                    SetPositionSphere((center_expr: Vec3((0.0, -0.8, 0.0)), radius_expr: Float(0.1), dimension: Volume)),
                    SetAttribute((attr: "velocity", attr_expr: Operator(Multiply(Operator(Normalized(Operator(Add(Operator(Subtract(Operator(Multiply(RandVec3, Float(2.0))), Float(1.0))), Vec3((0.0, 1.5, 0.0)))))), Operator(Uniform(Float(2.5), Float(4.5))))))),
                ],
                update_modifiers: [
                    AccelModifier((accel_expr: Vec3((0.0, -9.8, 0.0)))),
                    LinearDragModifier((drag_expr: Float(0.6))),
                    KillAabb((center_expr: Vec3((0.0, -6.0, 0.0)), half_size_expr: Vec3((20.0, 5.0, 20.0)), kill_inside: true)),
                    SetAttribute((attr: "size", attr_expr: Operator(Multiply(Operator(Attr("f32_0")), Operator(Saturate(Operator(Multiply(Operator(Subtract(Operator(Attr("lifetime")), Age)), Float(6.0))))))))),
                ],
                render_modifiers: [
                    Orient((mode: AlongVelocity)),
                ],
            ),
        )),
    ],
    preview: Some((stage: Capsule)),
)
```

Twenty-four solid cubes (6–12 cm, a brown to sand colour each) burst up and out from near the feet, tumble as gravity bends their flight (AlongVelocity turns each cube with its path) and shrink away in their last 0.17 s; the KillAabb below y = −1 (the floor is 0.9 m below a character's root) removes pieces that fall through the floor. `mesh: Some(Shape(Shard))` gives pointed shards, `Sphere` pebbles; `play: OnTrigger("hit")` with `trigger_at` bursts them at a contact point.

### Glowing shards from a mesh asset (mesh particles from the asset library)

```ron
(
    version: 2,
    assets: [(id: "rock", kind: Mesh((path: "vfx/models/debris.glb", mesh: 0, primitive: 0, name: Some("Rock"))))],
    layers: [
        Particles((
            name: "shards",
            simulation_space: Local,
            play: OnStart,
            mesh: Some(Asset("rock")),
            effect: (
                capacity: 32,
                spawner: (count: Single(16.0), spawn_duration: Single(0.0), period: Single(1.0), cycle_count: 1, starts_active: true, emit_on_start: true),
                alpha_mode: Add,
                init_modifiers: [
                    SetAttribute((attr: "age", attr_expr: Float(0.0))),
                    SetAttribute((attr: "lifetime", attr_expr: Float(0.8))),
                    SetAttribute((attr: "size", attr_expr: Float(0.25))),
                    SetPositionSphere((center_expr: Vec3((0.0, 0.0, 0.0)), radius_expr: Float(0.15), dimension: Volume)),
                    SetVelocitySphere((center_expr: Vec3((0.0, 0.0, 0.0)), speed_expr: Operator(Uniform(Float(2.0), Float(3.5))))),
                ],
                update_modifiers: [
                    LinearDragModifier((drag_expr: Float(2.0))),
                ],
                render_modifiers: [
                    ColorOverLifetime((
                        gradient: (g: [(0.0, (0.8, 2.5, 5.0, 1.0)), (1.0, (0.0, 0.0, 0.0, 1.0))]),
                        blend: Some(Overwrite),
                        mask: Some((15)),
                    )),
                    Orient((mode: AlongVelocity)),
                ],
            ),
        )),
    ],
    preview: Some((stage: Capsule)),
)
```

Sixteen copies of the mesh "Rock" of `vfx/models/debris.glb` (0.4 m in the file × size 0.25 = 10 cm) fly out in all directions, glow cyan (additive) and darken to nothing in 0.8 s. Bring your own model with `import_asset("path/to/model.glb", project: "...")` and change the asset; the check reports a missing file, a mesh index the file does not have and a mesh without UVs.

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
| the body itself should glow / change colour / look poisoned or frozen | a surface layer (section "surfaces"): colour, noise, rim |
| the glow on the body is too thin / flat | more shells with larger offsets (1–4 cm), `blend: Add`, a stronger rim |
| the pattern on the body is too busy / too coarse | noise `scale` (repeats per metre) and `strength`; slower `scroll` |
| the model disappears under the effect | lower the surface colour's alpha / opacity, add noise, or use `blend: Add` |
| a ring / bubble / column around the character, a shockwave, a slash | a shape layer (section "shapes"); presets in the editor |
| the ground ring tilts or bobs with the body | `follow: PositionYaw`, attach to the target root (not a bone), offset about −0.88 m in Y |
| the shockwave is too fast / too big | its `duration` and the last key of its `scale: Age([...])` |
| the slash sweeps too slowly / lingers | the age keys of `fill.to` / `fill.from` and `duration` |
| a glowing arc should follow the sword | a trail layer (section "trails") between blade base and tip sockets, emitted by swing triggers |
| the trail is too long / too short | `length` (seconds); its metres are the blade speed × length |
| the trail is angular on fast swings | more `subdivisions` (6–8), smaller `min_sample_distance` |
| the trail ends abruptly | `fade_over_u` down to 0 at u = 1, `width_over_u` thinning toward the end |
| the trail shows between swings | `emit: Trigger(start, stop)` and fire the stop trigger when the swing ends |
| sword shadows / echoes along the swing | an afterimage layer (section "afterimages"); 4–6 copies: `lifetime / interval` |
| the copies pop out instead of fading | raise `max_ghosts` to lifetime / interval, fade_over_life to 0 at 1 |
| sparks / a glow / light only during a swing | `play: Between(start: "swing_start", stop: "swing_end")` (particles, shapes, lights) |
| the weapon should light the floor / the character | a light layer (section "lights") on a blade socket; intensity as a curve over the parameter |
| a hit should flash | a light layer `play: OnTrigger("hit")`, `duration: Some(0.25)`, intensity `Age([...])` down to 0 |
| the light is too weak / too strong on the floor | `intensity` (lumens; lux at 1 m ≈ lm / 12.6); the editor's sun is ~4600 lux |
| the light reaches too far / cuts off hard | `range` (metres); a larger range fades out more gently |
| the light should flicker like fire | `flicker: (amount: 0.3, speed: 7.0)` |
| heat haze around a blade / a fire | a distortion layer (section "distortion"): `Capsule` on a `Segment`, `strength` 0.006–0.012 over a parameter |
| a shockwave should bend the air | a distortion `Sphere` with `edge: Rim`, `refraction: 0.75`, `scale` and `strength` over `Age`, `OnTrigger` + `duration` |
| the haze is invisible | something opaque and detailed must be behind it (it bends only opaque things); raise `strength`; lower noise `scale` for broader waves |
| the haze looks like crinkled glass | lower noise `scale` (1–3 per metre), `contrast` 1, a weaker `noise_b` |
| particles behind the haze are not bent | expected: distortion bends only opaque things; transparent layers draw over it |

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

Weapons are separate models with their own effects: spawn the weapon's package on the weapon entity and fire its swing triggers from the attack state — `vfx.trigger(weapon_fx, "swing_start")` when the swing begins and `vfx.trigger(weapon_fx, "swing_end")` when it ends (any names the package declares). Trails sample the blade sockets every frame and stay in the world; afterimages copy the weapon's meshes (put an afterimage package on the character for body copies). A stop ends emission and waits for the trail to age out / the copies to fade.

Light layers are Bevy `PointLight`s placed every frame at their attach point; the game shows at most `VfxSettings::max_lights` of them at once (default 8, all effects together; the weakest go dark): `app.insert_resource(omagari::VfxSettings { max_lights: 4, ..default() })`. They light the game's own lit materials; unlit materials do not react.

Surface layers draw on the meshes under the instance's target that belong to its own model (a weapon's glTF scene in the hand is skipped; mark other separate parts with `VfxSeparateModel`); spawn the package on the character's root entity. Shape layers need nothing from the game (the meshes are generated); a trigger's `trigger_at` position places an `OnTrigger` shape. The effect shader is embedded in the library: the game adds no shader files.

Built-in textures need nothing; other textures must exist at their path under the game's `assets/` folder (`import_asset` puts them there). JPG textures need Bevy's `jpeg` feature in the game. The game camera needs `Hdr` and `Bloom` for colors above 1.0 to glow. The game can also load `*.omagari.ron` projects directly.

Distortion layers need Bevy's screen-space transmission on the game camera: every `Camera3d` has `ScreenSpaceTransmission { steps: 1, .. }` by default (do not set `steps: 0`); `commands.spawn((Camera3d::default(), omagari::distortion_camera(), ...))` adds it explicitly with `Hdr` (do not add `Hdr` twice in one bundle). They bend only opaque things (the game's floor, walls, models); a distortion whose strength is 0 costs nothing.
