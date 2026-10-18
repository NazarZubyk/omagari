# Omagari VFX guide for AI agents

You build bevy_hanabi particle effects as Omagari project files (`*.omagari.ron`) through the `omagari` MCP server. **You never see the effect.** You work from numbers: `analyze_project` simulates the effect on the CPU and describes it in text, and the user watches it in the editor (`preview_in_editor`) and tells you what to change.

Fetch one section at a time with `guide(section: "...")`: workflow, format, spawner, modifiers, expressions, attributes, render, textures, hierarchy, recipes, design.

## Workflow

1. **Ask before building** (skip what the user already said): what the effect is, one-shot or looping, how long, main colors, and the game's scale (how big is a character in world units, how far is the camera). Default assumption: 1 unit = 1 meter, the effect is seen from about 5 to 10 units away.
2. **Start from something that works.** Copy a recipe (section "recipes") or a bundled example (`read_project("examples/fireworks")`, also `explode`, `greenvapor`, `magicburst`). Writing from scratch is how type errors happen.
3. **Save with `write_project(name, ron)`.** It refuses to save on errors and returns the analysis. Fix every ERROR. Read every WARNING; most are real problems.
4. **Tune with `analyze_project(name)` until the numbers match the intent.** Check: spawn pattern, peak count vs capacity, lifetime, motion extents (is it the right size? does it go up/down/outward?), size in world units, colors and the alpha window. Use `effect: "<name>"` to describe a single effect and save tokens.
5. **For small changes, edit the saved file directly** (`saved_effects/<name>.omagari.ron`) with your file-edit tool, then call `check_project(name)` or `analyze_project(name)`. This is far cheaper than resending the whole RON.
6. **Show it to the user with `preview_in_editor(name, stage)`**, with stage `air`, `floor` or `object` (see section "design"). Ask what they see and what to change, in words ("too fast", "too dim", "should spread wider"). Section "design" maps common feedback to parameters. This replaces any unsaved edits the user had open in the editor, so say so if they were editing.
7. **Export with `export_project(name, copy_to?)`** when the user is happy. `copy_to` can be the game's asset folder. Remind the user how textures are passed (section "textures").

The analysis is an approximation (60 updates/s, CPU random numbers, ConformToSphere is approximate). Trust it for scale, timing, counts and colors, not for exact looks.

## Format

A project is a list of effects. Every effect is spawned at the same point (the origin in the editor); Y is up.

```ron
(
    effects: [
        (
            name: "sparks",              // unique; children refer to parents by name
            parent: None,                // or Some("rocket"); see section "hierarchy"
            capacity: 512,               // max particles alive at once; keep it near the peak count
            spawner_settings: (
                count: Single(200.0),
                spawn_duration: Single(0.0),
                period: Single(1.0),
                cycle_count: 1,
                starts_active: true,
                emit_on_start: true,
            ),
            texture_index: Some(2),      // see section "textures"
            init_modifiers: [ /* run once when a particle is born */ ],
            update_modifiers: [ /* run every frame */ ],
            render_modifiers: [ /* ColorOverLifetime, SizeOverLifetime */ ],
        ),
    ],
)
```

RON rules that trip people up:
- Modifiers are an enum variant wrapping a struct, so they take **double parentheses**: `SetAttribute((attr: "size", attr_expr: Float(0.05)))`.
- Vector literals also take double parentheses: `Vec3((0.0, 1.0, 0.0))`, `Vec4((1.0, 0.5, 0.0, 1.0))`.
- Floats need a decimal point: `Float(1.0)`, not `Float(1)`. `U32(5)` is an integer.
- Attribute names are lowercase strings: `"position"`, `"hdr_color"`.
- Comments (`//`) are allowed in RON, and the editor drops them when it saves the file again.

The editor always adds these to every effect, and you cannot change them: alpha blending, the texture modulating opacity from its red channel, particles oriented along their velocity, and global simulation space.

## Spawner

Only root effects (no parent) use `spawner_settings`. A child effect is spawned by its parent's spawn events, and its own spawner settings are ignored.

A spawner repeats a cycle: spawn `count` particles spread evenly over `spawn_duration` seconds, then rest until `period` has passed. `cycle_count` is the number of cycles, `0` = forever.

| Pattern | count | spawn_duration | period | cycle_count |
| :-- | :-- | :-- | :-- | :-- |
| One burst (explosion, impact) | 200 | 0.0 | 1.0 | 1 |
| Steady stream forever (fire, smoke, aura) | rate × 1 | 1.0 | 1.0 | 0 |
| Emit for 2 s then stop | rate × 2 | 2.0 | 2.0 | 1 |
| Burst every 0.5 s forever (pulses) | 30 | 0.0 | 0.5 | 0 |

- `spawn_duration: 0.0` means everything spawns in the same frame.
- With `cycle_count: 1` the period is ignored.
- Never combine `cycle_count: 0` with `period: 0.0` and `spawn_duration: 0.0`: that bursts `count` particles every frame.
- `count` also accepts `Uniform((min, max))`, but the editor turns it back into `Single(min)` when the user opens the spawner panel. Prefer `Single`.
- `starts_active: false` or `emit_on_start: false` mean game code has to start the effect; leave both `true`.

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

Order matters: set the position before a velocity modifier that reads it.

**Update modifiers** (`update_modifiers`, run every frame, then `position += velocity * dt`):

| Modifier | Fields | Effect |
| :-- | :-- | :-- |
| `AccelModifier((accel_expr: vec3))` | — | Adds acceleration, e.g. gravity `Vec3((0.0, -9.8, 0.0))`. A random expression here gives jitter. |
| `LinearDragModifier((drag_expr: f32))` | 0 = none, 1–5 typical | Slows particles down; velocity × (1 − drag·dt) each frame. |
| `ConformToSphereModifier((origin_expr: vec3, radius_expr: f32, influence_dist_expr: f32, attraction_accel_expr: f32, max_attraction_speed_expr: f32))` | — | Pulls particles within `influence_dist` toward a sphere surface. |
| `EmitSpawnEventModifier((condition: OnDie, count_expr: U32(100), child_index: 0))` | `Always` or `OnDie` | Spawns particles in a child effect. `Always` emits `count` **every frame** per alive particle (60 fps → ×60 per second). See section "hierarchy". |

Putting an init modifier in `update_modifiers` (or the reverse) is an error: the editor silently drops it.

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
| `"position"` | vec3 | (0,0,0) | Required by every effect; set it (a position modifier or SetAttribute). |
| `"velocity"` | vec3 | (0,0,0) | Particles with zero velocity have no orientation and may be invisible. |
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

**`ColorOverLifetime((gradient: (g: [(t, (r, g, b, a)), ...]), blend: Some(Overwrite), mask: Some((15))))`**
- `t` goes from 0.0 (birth) to 1.0 (death); keys in increasing order.
- RGB above 1.0 is HDR and glows with bloom (the editor camera has bloom; the game needs bloom on its camera too). 2–6 reads as bright glowing; 10+ blows out to white.
- `a` is opacity. Fade in from 0 and out to 0 to avoid particles popping.
- `blend`: `Overwrite` replaces the color, `Modulate` multiplies the particle's `color`/`hdr_color`, `Add` adds to it. Use `Modulate` with a white gradient to keep per-particle colors (from SetAttribute or a parent) and only fade alpha.
- `mask`: `Some((15))` = RGBA, `Some((7))` = RGB only (alpha untouched).

**`SizeOverLifetime((gradient: (g: [(t, (x, y, z)), ...])))`** sets the particle size in world units over its life (replaces `size`). Use equal x, y, z.

## Textures

`texture_index` picks the particle sprite. The texture's red channel becomes opacity, so the shape comes from the texture and the color from the color attributes.

| Index | File | Use for |
| :-- | :-- | :-- |
| 0 | cloud.png | soft round puff: smoke, dust, fire body |
| 1 | cloud2.png | another cloud shape: smoke, vapor, mist |
| 2 | spark1.png | spark / star: sparks, glitter |
| 3 | spark2.png | spark variant |
| 4 | spark3.png | spark variant |
| 5 | glow1.png | soft glowing dot: embers, magic, fireworks, light orbs |
| 6 | splat1.png | splat: blood, liquid, impacts |

In a game, `EffectComplex::spawn(&mut commands, &textures, position, despawn_in)` takes the textures as a list in exactly this order, because the exported file only stores the index. The game must load these images (copy them from `assets/`) into a `Vec<Handle<Image>>` in this order.

## Hierarchy

A child effect (with `parent: Some("name")`) spawns particles at its parent's particles: trails behind a rocket, an explosion when a rocket dies.

1. The parent must be **listed before** the child.
2. The parent gets an `EmitSpawnEventModifier` in `update_modifiers`. `child_index` is the child's position among that parent's children, counted in list order from 0.
3. The child needs `InheritAttribute((attr: "position"))` in `init_modifiers`, otherwise its particles appear at the origin.
4. To pass data, the parent stores it in an attribute and the child reads it with `ParentAttr` or copies it with `InheritAttribute`. For example, a random color per rocket: the parent sets `"u32_0"` to a packed random color, and the child sets `"color"` to `Operator(ParentAttr("u32_0"))`.
5. Child `capacity` must hold everything all parents emit: `OnDie` count × parent particles, or `Always` count × 60 × parent particles × child lifetime.

`read_project("examples/fireworks")` is a complete rocket → trail sparkles + explosion example.

## Recipes

These pass `check_project`. Copy one, then adjust the numbers.

### Spark burst (one-shot impact)

```ron
(
    effects: [
        (
            name: "sparks",
            parent: None,
            capacity: 256,
            spawner_settings: (count: Single(150.0), spawn_duration: Single(0.0), period: Single(1.0), cycle_count: 1, starts_active: true, emit_on_start: true),
            texture_index: Some(2),
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
            ],
        ),
    ],
)
```

### Fire (looping)

```ron
(
    effects: [
        (
            name: "fire",
            parent: None,
            capacity: 512,
            spawner_settings: (count: Single(300.0), spawn_duration: Single(1.0), period: Single(1.0), cycle_count: 0, starts_active: true, emit_on_start: true),
            texture_index: Some(0),
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
            ],
        ),
    ],
)
```

### Smoke (looping, slow and soft)

```ron
(
    effects: [
        (
            name: "smoke",
            parent: None,
            capacity: 256,
            spawner_settings: (count: Single(30.0), spawn_duration: Single(1.0), period: Single(1.0), cycle_count: 0, starts_active: true, emit_on_start: true),
            texture_index: Some(1),
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
            ],
        ),
    ],
)
```

### Magic orbit (looping swirl on a sphere)

```ron
(
    effects: [
        (
            name: "orbit",
            parent: None,
            capacity: 512,
            spawner_settings: (count: Single(120.0), spawn_duration: Single(1.0), period: Single(1.0), cycle_count: 0, starts_active: true, emit_on_start: true),
            texture_index: Some(5),
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
                    gradient: (g: [(0.0, (1.0, 2.0, 6.0, 0.0)), (0.2, (2.0, 1.0, 6.0, 1.0)), (1.0, (3.0, 0.5, 4.0, 0.0))]),
                    blend: Some(Overwrite),
                    mask: Some((15)),
                )),
            ],
        ),
    ],
)
```

### Barrier around an object (looping; preview with stage "object")

```ron
(
    effects: [
        (
            name: "barrier",
            parent: None,
            capacity: 1024,
            spawner_settings: (count: Single(500.0), spawn_duration: Single(1.0), period: Single(1.0), cycle_count: 0, starts_active: true, emit_on_start: true),
            texture_index: Some(5),
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
            ],
        ),
    ],
)
```

The radius (1.4) is just outside the rock; the lower part of the bubble is under the floor, like a dome. For a body in the game, set the radius to the character's size.

### Explosion with debris (parent → child on death)

```ron
(
    effects: [
        (
            name: "debris",
            parent: None,
            capacity: 64,
            spawner_settings: (count: Single(24.0), spawn_duration: Single(0.0), period: Single(1.0), cycle_count: 1, starts_active: true, emit_on_start: true),
            texture_index: Some(5),
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
            ],
        ),
        (
            name: "pop",
            parent: Some("debris"),
            capacity: 512,
            spawner_settings: (count: Single(1.0), spawn_duration: Single(0.0), period: Single(0.0), cycle_count: 1, starts_active: true, emit_on_start: true),
            texture_index: Some(2),
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
            ],
        ),
    ],
)
```

## Design

**Scale.** Everything is in world units and seconds. The editor camera starts 5 units from the origin and shows about ±2 units; `analyze_project` says when an effect is bigger. Size the effect to the user's game, not to the editor view.

**The editor scene.** Effects play inside a big light gray cube (40 units wide) with grid lines every 1 unit (darker every 5) and small axes at the spawn point (X red, Y green, Z blue). You can refer the user to the grid ("each spark should travel about 2 squares"). The toolbar has a STAGE button, and `preview_in_editor` takes a `stage`, for where the effect plays:
- `air`: the spawn point is in the middle of the room, nothing nearby. Shows everything, including particles that fall.
- `floor`: the floor is at y = 0, under the spawn point, like ground in a game. Particles below y = 0 are hidden; `analyze_project` warns about that.
- `object`: a rock about 2 units wide (radius ~1, 1.6 tall) is centered on the spawn point and rests on the floor. Use it for shields, barriers, auras and hit effects on a body; particles inside the rock are hidden.

Pick the stage that matches how the game uses the effect. The BG button switches between a light room, a dark room and black; HDR glow stands out less on the light room, so suggest dark or black when judging glow.

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
| HDR intensity | 1 = plain, 2–6 = glowing, 10+ = blown out |
| Smoke alpha | 0.2–0.5; additive-looking glow comes from HDR colors, not alpha |

**How distance comes out:** a particle launched at speed v with drag d travels roughly v / d before stopping (without drag: v × lifetime). Gravity pulls it 4.9 × t² down after t seconds.

**Make it read well:**
- Fade alpha in at the start (t ≈ 0.05–0.15) and out at the end, so particles don't pop.
- Vary lifetime, speed and size with `Uniform` (±20–50%); identical particles look artificial.
- Layer simple effects (a bright core burst, sparks, smoke) instead of one complex one. Each layer is a separate root effect in the same project.
- Keep `capacity` slightly above the reported peak count.

**Translate user feedback:**

| User says | Change |
| :-- | :-- |
| too fast / too slow | speed, lifetime, drag |
| too big / small, spreads too much / too little | position radius, speed, size, drag |
| too dense / too sparse | spawn count or rate (adjust capacity) |
| too dim / not glowing / too bright | RGB intensity in ColorOverLifetime (HDR > 1 glows) |
| pops in / disappears abruptly | alpha 0 at t = 0 and t = 1 in the gradient |
| looks like squares / dots | a different `texture_index`, or larger size |
| lasts too long / too short | lifetime, spawn_duration, cycle_count |
| nothing appears | check alpha window, size, capacity, `starts_active`, and that the camera sees it |
| falls / should rise | sign of the Y acceleration |
