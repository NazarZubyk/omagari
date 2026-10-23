# Omagari VFX guide for AI agents

You build bevy_hanabi particle effects as Omagari projects (`*.omagari.ron`, format v2) through the `omagari` MCP server. **You never see the effect.** You work from numbers: `analyze_project` simulates the effect on the CPU and describes it in text, and the user watches it in the editor (`preview_in_editor`) and tells you what to change.

Fetch one section at a time with `guide(section: "...")`: workflow, format, layers, attach, spawner, modifiers, expressions, attributes, render, textures, hierarchy, recipes, design, game.

## Workflow

1. **Ask before building** (skip what the user already said): what the effect is, one-shot or looping, how long, main colors, whether it sits on a moving character, and the game's scale (how big is a character in world units, how far is the camera). Default assumption: 1 unit = 1 meter, the effect is seen from about 5 to 10 units away.
2. **Start from something that works.** Copy a recipe (section "recipes") or a bundled example (`read_project("examples/fireworks")`, also `explode`, `greenvapor`, `magicburst`). Writing from scratch is how type errors happen.
3. **Save with `write_project(name, ron)`.** It refuses to save on errors and returns the analysis. Fix every ERROR. Read every WARNING; most are real problems.
4. **Tune with `analyze_project(name)` until the numbers match the intent.** Check: spawn pattern, peak count vs capacity, lifetime, motion extents (is it the right size? does it go up/down/outward?), size in world units, colors and the alpha window, simulation space. Use `effect: "<layer name>"` to describe a single layer and save tokens.
5. **For small changes, edit the saved file directly** (`saved_effects/<name>.omagari.ron`) with your file-edit tool, then call `check_project(name)` or `analyze_project(name)`. This is far cheaper than resending the whole RON.
6. **Show it to the user with `preview_in_editor(name, stage, motion)`**, with stage `air`, `floor` or `object` (see section "design") and, for effects on moving characters, a `motion` such as `{"type": "circle", "radius": 3.0, "speed": 3.6}`. Ask what they see and what to change, in words ("too fast", "too dim", "should spread wider"). Section "design" maps common feedback to parameters. This replaces any unsaved edits the user had open in the editor, so say so if they were editing.
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
            play: OnStart,               // Loop | OnStart; must match the spawner
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

Projects can also hold `sockets` and `fit` (section "attach"), and `parameters` and `triggers`, which the runtime doesn't use yet, so leave those out (the check warns).

## Layers

| Field | Values | Meaning |
| :-- | :-- | :-- |
| `name` | string | Unique. Used by `parent` and `analyze_project(effect: ...)`. |
| `parent` | `None` / `Some("name")` | Spawned by the parent's spawn events (section "hierarchy"). |
| `attach` | `Target` / `Socket("name")` / `Segment(from: "a", to: "b")` | Where the layer sits: the target's origin (default), a socket it follows, or a line between two sockets. Section "attach". |
| `simulation_space` | `Local` / `Global` | See below. |
| `play` | `Loop` / `OnStart` | `Loop` = the spawner repeats (`cycle_count: 0`); `OnStart` = one-shot (`cycle_count` ≥ 1). The check warns when they disagree. `OnTrigger`/`While` come later. |
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
- `Segment(from: "a", to: "b")`: a line between two sockets, e.g. a blade's base and tip. The layer sits at the target's origin and every frame the runtime writes the two ends into `Property("segment_a")` and `Property("segment_b")` (vec3, in the layer's simulation space). Read them in init expressions to spawn along the line, e.g. position = `Operator(Add(Property("segment_a"), Operator(Multiply(Operator(Subtract(Property("segment_b"), Property("segment_a"))), RandFloat))))` (recipe "Blade edge sparks"). A Segment layer that reads neither gets a warning, and so does a layer that reads them without being a Segment.

```ron
sockets: [
    (name: "blade_base", node: Some("Sword"), offset: (0.0, 0.1, 0.0)),
    (name: "blade_tip", node: Some("Sword"), offset: (0.0, 1.0, 0.0)),
    (name: "chest", node: None, offset: (0.0, 1.2, 0.0)),
],
```

Socket names and nodes come from the game's model: ask the user (bone names like `mixamorig:RightHand`, or the weapon entity's name). `check_project` lists socket names that are not package sockets as NOTES (they must exist as nodes in the model); the editor's preview target has no model, so the user sees nodes at the target's origin. Package sockets with `node: None` work everywhere. Give a child layer the same `attach` as its parent (the check warns otherwise).

**Size fit** (`fit`, per project):
- `None` (default): the effect keeps its authored size, even on a scaled target.
- `Some(TargetScale)`: scales with the target entity's scale.
- `Some(TargetHeight(1.8))`: author the effect for a 1.8 m character; on a target whose model is 4 m tall (its meshes, measured once they are loaded) everything is scaled × 4 / 1.8. Socket offsets scale too.

Fit scales `Local` layers only: bevy_hanabi gives `Global` layers the emitter's position, not its scale, so the check warns about `Global` layers in a fitted project.

**Lifecycle** (the game controls it, section "game"): an instance plays once its package has loaded and runs until the game stops it; stopping is smooth (no new particles, live ones finish), and restarting while stopping plays it again on the same instance. An instance whose root layers are all one-shot (`play: OnStart` with `cycle_count` ≥ 1) finishes by itself after its last spawn plus its longest particle life, so one-shot effects clean up without code; looping ones run until stopped.

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

Order matters: set the position before a velocity modifier that reads it. Positions are relative to the target in `Local` space (and to the spawn point in `Global`).

**Update modifiers** (`update_modifiers`, run every frame, then `position += velocity * dt`):

| Modifier | Fields | Effect |
| :-- | :-- | :-- |
| `AccelModifier((accel_expr: vec3))` | — | Adds acceleration, e.g. gravity `Vec3((0.0, -9.8, 0.0))`. A random expression here gives jitter. |
| `LinearDragModifier((drag_expr: f32))` | 0 = none, 1–5 typical | Slows particles down; velocity × (1 − drag·dt) each frame. |
| `ConformToSphereModifier((origin_expr: vec3, radius_expr: f32, influence_dist_expr: f32, attraction_accel_expr: f32, max_attraction_speed_expr: f32))` | — | Pulls particles within `influence_dist` toward a sphere surface. |
| `EmitSpawnEventModifier((condition: OnDie, count_expr: U32(100), child_index: 0))` | `Always` or `OnDie` | Spawns particles in a child layer. `Always` emits `count` **every frame** per alive particle (60 fps → ×60 per second). See section "hierarchy". |

Putting an init modifier in `update_modifiers` (or the reverse) is an error: it is ignored.

## Expressions

| Expression | Type | Meaning |
| :-- | :-- | :-- |
| `Float(1.5)` | f32 | literal |
| `U32(5)` | u32 | literal |
| `Vec3((x, y, z))` | vec3 | literal |
| `Vec4((x, y, z, w))` | vec4 | literal |
| `RandFloat` | f32 | random in [0, 1) |
| `RandVec3` | vec3 | each component random in [0, 1) (not centered!) |
| `RandU32` | u32 | random integer |
| `Time` | f32 | effect time in seconds |
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
| `Property("segment_a")`, `Property("segment_b")` | vec3 | ends of a `Segment`-attached layer (section "attach"); the analysis puts them at the sockets' offsets |
| `Operator(Attr("<name>"))` | attribute type | this particle's attribute |
| `Operator(ParentAttr("<name>"))` | attribute type | the parent particle's attribute (children only) |

Useful building blocks:
- Random value in a range: `Operator(Uniform(Float(0.8), Float(1.2)))`
- Random direction (unit vector):
  `Operator(Normalized(Operator(Subtract(Operator(Multiply(RandVec3, Float(2.0))), Float(1.0)))))`
- Random direction × random speed:
  `Operator(Multiply(<random direction>, Operator(Uniform(Float(2.0), Float(4.0)))))`
- Mostly-up cone: `Operator(Vec3(Operator(Uniform(Float(-0.3), Float(0.3))), Operator(Uniform(Float(2.0), Float(3.0))), Operator(Uniform(Float(-0.3), Float(0.3)))))`
- Random packed color per particle: `Operator(Pack4x8UNorm(Operator(Vec4(RandFloat, RandFloat, RandFloat, Float(1.0)))))`

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

Others exist (`"id"`, `"particle_counter"`, `"axis_x"`, `"axis_y"`, `"axis_z"`, `"sprite_index"`, `"prev"`, `"next"`, `"ribbon_id"`) but are rarely useful here.

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

**`alpha_mode`**: `Blend` (normal transparency), `Add` (additive: overlapping particles get brighter, good for glow, fire, magic; black adds nothing), `Premultiply`, `Multiply` (darkens), `Mask(0.5)` (opaque cut-out at that alpha), `Opaque`.

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

Other textures use a path relative to the game's `assets/` folder (`vfx/textures/smoke.png`). The file must exist there (`check_project` warns when it doesn't); the editor and the check look it up in the configured game assets folder (or this repository's `assets/`). `purpose` (`Sprite`, `Noise`, `Mask`, `Gradient`) picks wrap and color-space defaults; sprites need nothing else.

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

## Game

`export_project` writes `<name>.vfx.ron` (the project without the editor's `preview` block) to `<game assets>/vfx/`, or to `exported_effects/` when no game assets folder is configured (then copy it with `copy_to`). In the game, with `HanabiPlugin` and `omagari::OmagariPlugin` added:

```rust
let package: Handle<VfxPackage> = asset_server.load("vfx/fireball.vfx.ron");
let instance = commands.spawn_vfx(&package, target);                // follows `target`
let hit = commands.spawn_vfx_at(&package, Transform::from_translation(p)); // in the world, no target
vfx.stop(instance);    // VfxControl: smooth stop
vfx.restart(instance); // while stopping: plays again (re-applied status)
// message VfxFinished { instance, target }: sent once when an instance ends
```

One-shot packages (every root layer `OnStart` with a finite `cycle_count`) finish and clean up by themselves; looping ones run until `stop`. Despawning the target removes its instances (and sends `VfxFinished`). Sockets with a `node` need that node name in the target's hierarchy (section "attach"). Editing the package file while the game runs updates running instances (hot reload, when the game enables Bevy's file watcher).

Built-in textures need nothing; other textures must exist at their path under the game's `assets/` folder. The game camera needs `Hdr` and `Bloom` for colors above 1.0 to glow. The game can also load `*.omagari.ron` projects directly.
