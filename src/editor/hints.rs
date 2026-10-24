//! Tooltips of the most-used values, in plain words with units. Kept in one
//! place so they read the same everywhere.

pub const NAME: &str = "The layer's name. Must be unique: child layers find their parent by it.";
pub const CAPACITY: &str =
    "Most particles of this layer alive at the same time. Spawns above it are skipped. Higher costs more GPU memory.";
pub const TEXTURE: &str = "The image drawn on each particle. Needs a ParticleTexture render modifier to show. Import and edit textures in the ASSETS tab.";
pub const PARENT: &str = "A layer whose particles spawn this layer's particles (EmitSpawnEvent on the parent). NONE = spawned by its own spawner.";
pub const SPACE: &str = "Whether particles move along with the target after they are born.";
pub const SPACE_LOCAL: &str = "Local: particles move with the target (auras, shields). Size fit applies.";
pub const SPACE_GLOBAL: &str =
    "Global: particles stay where they were born, so a moving target leaves a trail (smoke, footsteps).";
pub const ATTACH: &str = "Where the layer sits: Target = the target's root; Socket = follows a named point (e.g. a hand bone); Segment = between two sockets (e.g. along a blade).";
pub const PLAY: &str = "When the layer plays. Loop = all the time; OnStart = once at the start; OnTrigger = each time the game fires a trigger (a burst); While = while a parameter is at or above a threshold; Between = from a start trigger until a stop trigger (e.g. sparks only during a swing). How long it emits is set by the spawner.";
pub const RATE: &str = "Particles per second as a steady stream, replacing the spawner's count while the layer plays. A constant, or a curve over a parameter (e.g. more particles at higher intensity). Off = the spawner's own count.";
pub const VISIBLE: &str = "Show the layer only while a parameter is between From and To; outside it the layer fades out over Fade (parameter units). Neighbouring ranges cross-fade, e.g. a weak look below 0.6 and a strong one above. Fades the spawn rate; multiply alpha by Property visibility to fade live particles too.";
pub const BINDING: &str = "Constant: one value. Curve over parameter: the value follows a number parameter: keys are (parameter value → value), straight lines in between, flat past the first and last key. Curve over age: the value changes over the seconds since the layer started (or its trigger played it).";
pub const BINDING_AGE: &str = "The value changes over time: keys are (seconds since the layer started → value). A one-shot or triggered layer starts its age again each time it plays, so a shockwave can grow and fade without code. Flat past the last key.";

// --- parameters and triggers ----------------------------------------------------

pub const PARAMETERS: &str = "Values the game sets while the effect plays, by name (any names, as many as you need), e.g. an intensity 0..1 or a tint colour. Layer values read them: expressions (Param), rate, visible range, play While.";
pub const PARAM_EXPR: &str = "The current value of a parameter the game sets (PARAMETERS tab). Init modifiers read it when a particle is born, update modifiers every frame.";
pub const PARAM_NAME: &str = "The name the game uses: vfx.set_param(instance, \"name\", value). Renaming updates every use in this project (expressions, rate, visible, play, pulses, preview), but not the game's code.";
pub const PARAM_KIND: &str = "Number (with a range) or colour (red, green, blue, opacity; values above 1 glow).";
pub const PARAM_DEFAULT: &str = "The value before the game sets one.";
pub const PARAM_RANGE: &str = "Smallest and largest value; what the game sets is kept inside.";
pub const PARAM_SMOOTHING: &str = "Seconds to get about two thirds of the way to a new value, so changes fade in instead of jumping. 0 = jump at once.";
pub const PARAM_LIVE: &str = "Drives the preview like the game would (smoothed). Saved with the project as a preview value; not part of the exported effect.";
pub const PARAM_ANIMATE: &str = "Moves the preview value from min to max and back over 4 seconds, to see every stage.";
pub const TRIGGERS: &str = "Named one-shot events the game fires (any names), e.g. a damage tick or a hit. Layers with play OnTrigger burst; pulses make a parameter jump and fade back.";
pub const TRIGGER_NAME: &str = "The name the game uses: vfx.trigger(instance, \"name\"). Renaming updates the layers that play on it and the preview's auto-fire.";
pub const PULSES: &str = "When the trigger fires, the parameter jumps to To and goes back to its value over Decay seconds (a flash). Number parameters only.";
pub const FIRE: &str = "Fires the trigger on the preview, like the game's vfx.trigger.";
pub const FIRE_RANDOM: &str = "Fires with a position: a random point on the target's model (its meshes' bounds; near the target without a model), like vfx.trigger_at with a contact point. Layers reading trigger_pos / trigger_dir burst there.";
pub const ANIMATION_TRIGGER: &str = "Fires the trigger each time the preview animation passes this clip time (every loop), e.g. at the hit frame of a swing. Saved with the project as a preview setting.";
pub const AUTO_FIRE: &str = "Fires the trigger on the preview every N seconds. Saved with the project as a preview setting.";
pub const BLEND: &str = "How particles mix with what is behind them. Add for glows and fire, Blend for smoke.";

pub const SPAWNER: &str = "When and how many particles are born. A cycle spawns Count particles over Duration seconds, then waits until Period seconds have passed.";
pub const COUNT: &str = "Particles born per cycle. ± makes it a random range.";
pub const DURATION: &str = "Seconds to spread the Count particles over. 0 = all at once (a burst).";
pub const PERIOD: &str = "Seconds from the start of one cycle to the start of the next.";
pub const CYCLES: &str = "How many cycles to play. 0 = forever (looping effects); 1 = one burst.";
pub const STARTS_ACTIVE: &str = "Off = the layer waits until game code switches it on.";
pub const EMIT_ON_START: &str = "Spawn the first cycle as soon as the effect starts.";

pub const INIT_LIST: &str = "Values set once when a particle is born: start position, speed, lifetime, size, colour.";
pub const UPDATE_LIST: &str = "Forces applied every frame while a particle lives: gravity, drag, spawning child particles.";
pub const RENDER_LIST: &str = "How particles are drawn: colour and size over their life, texture, which way they face.";

pub const SET_ATTRIBUTE: &str = "Sets one value of each new particle (lifetime, size, velocity, colour, ...).";
pub const ACCEL: &str = "A constant push in m/s². Gravity is (0, -9.8, 0); a gentle rise is (0, 1, 0).";
pub const DRAG: &str = "Slows particles down over time. 0 = no drag; 2 = they lose most of their speed in about a second.";
pub const EMIT_SPAWN_EVENT: &str = "Makes this layer's particles spawn particles of a child layer (always, or when they die).";
pub const SIZE_OVER_LIFETIME: &str = "Particle size in metres from birth (t = 0) to death (t = 1).";
pub const COLOR_OVER_LIFETIME: &str = "Particle colour (red, green, blue, opacity) from birth (t = 0) to death (t = 1). Values above 1 glow with bloom.";
pub const TEXTURE_MAPPING: &str = "Draws the layer's texture on each particle. How the texture and the particle colour combine:";
pub const ORIENT: &str = "Which way the particle squares face. Without it they lie flat and vanish when seen from the side.";
pub const GRADIENT_T: &str = "When in the particle's life: 0 = birth, 1 = death.";

pub const FIT: &str = "Scales the effect to the target it is attached to, so one effect fits small and large characters. Only Local layers scale.";
pub const SOCKETS: &str = "Named points on the target: a node (bone) name of its model plus an offset in metres. Layers attach to them.";

/// Plain words for a particle attribute, with its unit.
pub fn attribute(name: &str) -> &'static str {
    match name {
        "Position" => "Where the particle starts, in metres from the layer's origin.",
        "Velocity" => "Starting speed and direction in m/s. (0, 2, 0) = 2 metres per second upwards.",
        "Lifetime" => "Seconds the particle lives. Without it particles never die.",
        "Age" => "Seconds the particle has already lived; usually 0 at birth.",
        "Size" => "Particle size in metres (both sides of the square).",
        "Size2" => "Particle width and height in metres.",
        "Size3" => "Particle size in metres along X, Y and Z.",
        "Color" => "Packed colour (one number); prefer HDR Color, or ColorOverLifetime.",
        "HDR Color" => "Colour as (red, green, blue, opacity); values above 1 glow with bloom.",
        "Alpha" => "Opacity: 0 = invisible, 1 = solid.",
        "Axis X" | "Axis Y" | "Axis Z" => "Orientation axis of the particle square.",
        "Sprite Index" => "Frame of a flipbook texture.",
        "Prev" | "Next" | "Ribbon ID" => "Ribbon / trail links between particles.",
        "ID" | "Particle Counter" => "A number that tells particles apart.",
        _ => "A free per-particle value for your own expressions.",
    }
}

// --- assets ------------------------------------------------------------------

pub const ASSET_IMPORT: &str = "Pick PNG or JPG textures, or GLB / glTF models (a mesh of the model becomes a mesh asset for mesh particles). Files outside the game assets folder are copied into its vfx/textures/ (models: vfx/models/, .glb only); files already inside it are used where they are.";
pub const MESH_ASSET_ID: &str = "The name particle layers use to pick this mesh (Mesh: Asset). Renaming updates every layer that uses it.";
pub const MESH_ASSET_FILE: &str = "The glTF model the mesh comes from, relative to the game's assets/ folder. The game loads the mesh from it, so the file must ship with the game.";
pub const MESH_ASSET_MESH: &str = "Which mesh of the file the particles draw (a mesh with several materials has one part per material). It needs texture coordinates (UVs) for bevy_hanabi to draw it; particles are not lit, so they show one colour per particle.";
pub const ASSET_ID: &str = "The name layers use to pick this texture. Renaming updates every layer that uses it.";
pub const ASSET_PATH: &str = "Where the file is, relative to the game's assets/ folder (omagari://textures/… = built into Omagari, nothing to copy).";
pub const ASSET_PURPOSE: &str = "What the texture is for. It picks the defaults of the settings below.";
pub const ASSET_WRAP: &str = "What is drawn past the texture's edge: Repeat tiles it (scrolling noise), Clamp stretches the edge pixels (sprites).";
pub const ASSET_COLOR_SPACE: &str = "Colour = the pixels are colours (sprites, gradients). Data = the pixels are numbers (noise, masks) and are used unchanged.";
pub const ASSET_FILTER: &str = "Smooth blends neighbouring pixels; Pixelated shows hard pixel edges (pixel art).";
pub const ASSET_GRID: &str = "A sprite sheet: the image holds rows × columns frames of an animation (flipbooks).";
pub const ASSET_EXTRACT: &str = "Writes one channel as a grayscale PNG next to the texture and adds it as a new asset. Particle layers can only read a texture's colour or its red channel, so a channel-packed texture needs this for G, B or A.";
pub const ASSET_VIEW: &str = "Show the colour, or one channel as gray (white = 255).";

pub fn purpose_hint(purpose: omagari::format::TexturePurpose) -> &'static str {
    use omagari::format::TexturePurpose as P;
    match purpose {
        P::Sprite => "A picture drawn on particles: smoke, sparks, glows. Clamp, colour.",
        P::Noise => "A tiling pattern that scrolls or distorts (perlin, voronoi). Repeat, data.",
        P::Mask => "Black-and-white shape that hides or shows parts. Clamp, data.",
        P::Gradient => "A colour ramp looked up from left to right. Clamp, colour.",
    }
}

// --- P2b: hanabi coverage ----------------------------------------------------------

pub const CONE: &str = "Particles start inside a cone standing on the layer's origin and opening upwards (+Y), turning with the emitter: fountains, thrusters, torches.";
pub const CONE_HEIGHT: &str = "How tall the cone is, in metres (from its base at the origin up).";
pub const CONE_BASE: &str = "Radius of the cone at the bottom (the origin), in metres. Small = a narrow nozzle.";
pub const CONE_TOP: &str = "Radius at the top, in metres. 0 = a pointed cone; larger than the base = a cone opening upwards.";
pub const RADIAL_ACCEL: &str = "Pushes particles away from a point (positive) or pulls them towards it (negative): implosions, black holes, bursts.";
pub const RADIAL_ACCEL_VALUE: &str = "m/s² away from the origin; negative pulls particles in.";
pub const TANGENT_ACCEL: &str = "Swirls particles around an axis: auras, tornados, vortices. Combine with a pull (RadialAccel, negative) to keep them circling.";
pub const TANGENT_ACCEL_VALUE: &str = "m/s² around the axis; negative swirls the other way.";
pub const KILL_SPHERE: &str = "Removes particles that leave a sphere (or enter it, with kill inside): keep a swarm in a ball, or clear a hole.";
pub const KILL_AABB: &str = "Removes particles that leave a box (or enter it, with kill inside): keep rain inside a room, stop particles at the floor.";
pub const KILL_INSIDE: &str = "On: particles inside the volume die. Off: particles outside it die.";
pub const SET_COLOR: &str = "Sets the colour when particles are drawn, combined with their colour by the blend mode. A random range gives every particle the same colour (bevy_hanabi limit): use hdr_color at birth for per-particle colours.";
pub const SET_SIZE: &str = "Sets the size in metres when particles are drawn (replaces the size attribute; a later SizeOverLifetime replaces it again).";
pub const FLIPBOOK: &str = "Plays a sprite sheet: each particle draws frame sprite_index of the texture (frames left to right, then top to bottom). Needs ParticleTexture, a grid and something that sets sprite_index (Update + › Flipbook frames over lifetime).";
pub const FLIPBOOK_FRAMES: &str = "An update SetAttribute that steps sprite_index from the first frame at birth to the last at death, using the texture's grid (rows × columns frames).";
pub const ROUND: &str = "Rounds the corners of particle squares: 0 = square, 1 = circle. A number or an expression (render time).";
pub const SCREEN_SPACE_SIZE: &str = "The particle size is in screen pixels instead of metres, so particles keep their size on screen at any distance (dots, UI-like sparkles). Put it after the size modifiers; sizes like 8 to 32.";
pub const ORIENT_ROTATION: &str = "Spins the particle square around the view direction, in radians: a number, or e.g. Age × 3 for spinning sprites. Works with ParallelCameraDepthPlane and FaceCameraPosition.";
pub const MOTION: &str = "When particles move by their velocity. After update (default) / before update; None = only modifiers that set the position move them (expression-driven paths, ribbons).";
pub const SIM_CONDITION: &str = "When visible (default): the layer is simulated only while visible, the cheapest. Always: also off screen, so it is up to date when it comes back into view.";
pub const PRNG_SEED: &str = "Seed of the GPU random numbers. Off = the default seed (0). The same seed gives the same random spread every time; change it to get another variation.";
pub const PRESET_ONCE: &str = "One burst: Count particles at once, one cycle (impacts, explosions). Keeps the count (and its random range).";
pub const PRESET_RATE: &str = "A steady stream: Count particles per second, forever (fire, smoke, auras). Keeps the count (and its random range).";
pub const PRESET_BURST: &str = "Repeated bursts: Count particles at once every Period seconds, forever (pulses). Keeps the count and the period.";

// --- model preview -------------------------------------------------------------

pub const MODEL: &str = "A glTF model (.glb / .gltf) of the game assets folder to preview the effect on (MODEL stage). It is the game's model: sockets, attach_to and the checks use its node names. Without one the MODEL stage shows the capsule.";
pub const ROOT_HEIGHT: &str = "How high the game's entity root (the effect's target) is above the model's feet, in metres: the model is placed this much below the target. E.g. 0.9 for a player whose root is 0.9 m above the feet; 0 = the target is the model's origin.";
pub const ANIMATION_FILE: &str = "The glTF file the animation clip comes from: the model's own file, or another file with the same skeleton (Mixamo clips are separate files). Clips play on nodes with the same names, no retargeting.";
pub const ANIMATION_CLIP: &str = "The clip to play (its name in the file; Mixamo clips are all called \"mixamo.com\"). Pause, loop and scrub in the bottom bar.";
pub const ATTACH_TO: &str = "Where the preview instance is attached, like the game's spawn_vfx(&package, target): the model's root (the moving target), or a node of the body or of the weapon (pick it here or with 🎯 in the HIERARCHY tab). Sockets are looked up under it.";
pub const WEAPON: &str = "A second model held by a node of the first (e.g. a sword in mixamorig:RightHand), with an offset and rotation. It is a separate model: effects on the body do not cover it. Pick \"weapon\" under Instance on to preview a weapon effect.";
pub const HIERARCHY_ACTIONS: &str = "🎯 attach the preview instance to the node · ⚓+ new socket on the node · A / B segment start / end of the selected layer";
pub const NODE_TARGET: &str = "Attach the preview instance to this node (preview.attach_to): the game's target entity";
pub const NODE_SOCKET: &str = "Add a package socket on this node (offset 0); move it in the SOCKETS & FIT tab or by dragging its dot in the viewport";
pub const NODE_SEGMENT: &str = "Use this node as the segment start (A) or end (B) of the selected layer (its socket on the node, else the node name)";
pub const MESHES: &str = "Mesh entities (mesh.material) and materials of the model: what surface layers will target (own model only).";
pub const SOCKET_DRAG: &str = "Drag a socket's dot in the viewport to move it in the camera plane; or type the offset, or nudge it. Gizmos: cyan = socket, yellow = selected or segment, red = node not found.";
pub const NUDGE: &str = "Moves the selected socket by the step along the node's axis";
pub const SEGMENT_RADIUS: &str = "Particles start at a random point along the layer's segment (attach Segment), up to a radius (here 5 cm, edit the last number) around it: a capsule along a blade. A SetAttribute position with Mix(segment_a, segment_b, rand) + a random offset square to the segment.";

// --- surface layers -------------------------------------------------------------

pub const SURFACE: &str = "A surface layer draws Omagari's effect shader over the target's own meshes (copies of them, the model itself is not changed). It moves with the animation; shells give it volume.";
pub const SURFACE_TARGETS: &str = "Which meshes of the target's model it covers: all, some meshes by name, or the meshes using some materials (names from the HIERARCHY tab or the MCP's describe_model). A weapon held in the hand is a separate model and never covered.";
pub const SURFACE_SHELLS: &str = "Each shell is one copy of the meshes, pushed outward along the surface by Offset metres. One shell at 0 = a coloured skin; two or three at 1–4 cm = a thick, soft glow around the body.";
pub const SHELL_OFFSET: &str = "Metres the shell sits outside the surface (scaled with the size fit). 0 = on the skin.";
pub const SHELL_OPACITY: &str = "Opacity of this shell (× the material's). Outer shells are usually fainter.";
pub const SHELL_NOISE_OFFSET: &str = "Shifts the noise pattern of this shell, so stacked shells do not repeat it exactly.";
pub const SURFACE_FADE: &str = "Seconds the layer takes to appear when the effect starts (In) and to disappear when the game stops it (Out). Stopping waits for Out.";
pub const SURFACE_BLEND: &str = "Blend: covers the model by its opacity (colour skins, frost). Add: adds light, never darkens (glows, energy).";
pub const SURFACE_COLOR: &str = "Colour of the layer (red, green, blue, opacity; values above 1 glow with bloom). Constant, a curve over a number parameter, or a colour parameter the game sets.";
pub const SURFACE_OPACITY: &str = "Overall opacity, 0 = invisible, 1 = as the colour says.";
pub const SURFACE_EMISSIVE: &str = "Brightness multiplier. Above 1 the layer glows (bloom with the GAME camera).";
pub const NOISE: &str = "A noise texture that breaks the colour and opacity into a moving pattern. Triplanar sticks to the body (needs no UVs); UV follows the mesh's own texture layout.";
pub const NOISE_SCALE: &str = "Repeats of the texture per metre (Triplanar) or per UV square (UV). Higher = finer pattern.";
pub const NOISE_SCROLL: &str = "How fast the pattern moves, in texture squares per second (x, y). Triplanar: y moves it along the body.";
pub const NOISE_CONTRAST: &str = "Sharpens the pattern: 1 = the texture as it is (soft clouds), 2–4 = crisp patches and veins.";
pub const NOISE_STRENGTH: &str = "0 = no effect, 1 = the texture fully shapes the colour and opacity.";
pub const TEXTURE_SLOT: &str = "The texture and which of its values the shader reads: R, G, B, A (one channel of a packed texture) or Luminance (brightness, right for grayscale and colour images).";
pub const RIM: &str = "A glow on the edges of the body as seen from the camera (fresnel): the classic outline of auras and energy skins.";
pub const RIM_POWER: &str = "Higher = thinner rim (1 = soft and wide, 5 = a thin outline).";
pub const RIM_INTENSITY: &str = "Brightness of the rim; above 1 it blooms.";
pub const PULSE: &str = "A steady breathing: the opacity goes down by Amount and back, Frequency times per second.";
pub const FLASH: &str = "A flash: 0 = nothing, 1 = three times as bright and half-way to opaque. Bind it to a parameter that a trigger pulses (TRIGGERS tab) for a hit or damage-tick flash.";
pub const DISSOLVE: &str = "The surface breaks up where the noise (A, else B) is below Threshold, with a glowing edge. 0 = whole, 1 = gone: bind it to a parameter to make the layer appear or burn away.";
pub const FILL: &str = "Shows the layer only on a band of the model along an axis of its rest pose: 0 = the low end (feet for Y), 1 = the high end (head). From 0 and To bound to a parameter = frost rising from the feet. U / V use the mesh's texture coordinates instead (0..1): along a slash arc or around a ring, so To over age sweeps it.";

// --- shape layers ------------------------------------------------------------------

pub const SHAPE: &str = "A shape layer draws a simple 3D shape (sphere, ring, arc, ...) with the effect shader, attached to the target but not bent by its skeleton: shield bubbles, ground rings, shockwaves, slash arcs, glow columns.";
pub const SHAPE_KIND: &str = "The shape and its size in metres (before the scale below and the size fit). Flat shapes (ring, quad, arc) lie flat, facing up; turn them with Rotation.";
pub const SHAPE_FOLLOW: &str = "How it follows its attach point. Full = moves, turns and tilts with it; Position = moves only, keeps its own orientation; PositionYaw = moves and turns to face the same way but never tilts (a ground ring stays flat while the body leans).";
pub const SHAPE_OFFSET: &str = "Metres from the attach point (scaled with the size fit): in the attach point's space (Full), in world axes (Position), or turned with the target's heading (PositionYaw). The target root of a character is often 0.9 m above its feet: a ground ring needs about -0.88 in Y.";
pub const SHAPE_ROTATION: &str = "Degrees about X, then Y, then Z. A flat ring or arc at 0 lies flat; 90 about X stands it up facing forward.";
pub const SHAPE_SCALE: &str = "Scale (X, Y, Z) on top of the shape's size. Make it a curve over age for a shockwave that grows, or over a parameter for a shield that swells.";
pub const SHAPE_SPIN: &str = "Turning speed in degrees per second around the axis (in the shape's own space; Y = the shape's up). Can be a curve.";
pub const SHAPE_PLAY: &str = "When the shape shows. Loop = all the time; OnStart = once at the start; OnTrigger = each time the game fires the trigger (the age starts over; with a payload position it plays there); While = while a parameter is at or above a threshold; Between = from a start trigger until a stop trigger.";
pub const SHAPE_DURATION: &str = "Seconds an OnStart / OnTrigger shape plays (it fades in and out inside this time and is gone after it). For Loop / While shapes the age starts over every Duration seconds, so age curves repeat. Off = plays until the effect stops.";
pub const SHAPE_FADE: &str = "One-shots fade in over In after they start and out over Out before they end. Other shapes fade in when they start and out when they stop (While: when the parameter drops). Every shape fades out over Out when the game stops the effect.";
pub const PARTICLE_PRESETS: &str = "Ready-made looks from the guide's tested recipes: ribbons (GPU particle streaks: magic streaks, a swirl, a streak behind the emitter) and mesh particles (debris, shards). The layer keeps its name and attach; a preset may add a second layer under it.";
pub const PARTICLE_MESH: &str = "What each particle draws: the default flat quad, a built-in 3D shape (1 m across at size 1, so the particle size is its size in metres), or a mesh asset (a mesh of a glTF model imported in the ASSETS tab). Mesh particles are unlit: one flat colour per particle (vary the colour per particle); use Opaque for solid debris or Add for glowing shards.";
pub const PARTICLE_SORT: &str = "Draw order among see-through things at a similar distance (metres added to the emitter's distance from the camera): positive = drawn later, over other effects; negative = earlier, under them. The whole layer moves at once (particles inside it are not re-sorted). No effect with Opaque / Mask blending.";
pub const SHAPE_SORT: &str ="Draw order among see-through things at a similar distance (metres added to its distance): positive = drawn later, on top of particles and skin effects; negative = earlier.";
pub const MASK: &str = "A mask texture: where it is dark the layer is transparent.";

// --- trail and afterimage layers ---------------------------------------------------

pub const IN_PLACE: &str = "Removes the clip's horizontal root motion (the skeleton root, e.g. Mixamo's hips, walking away from the model's origin) after the animation, so the model stays on the target while effects are tuned; the vertical bob stays. Preview only (saved in the preview block); the game handles root motion itself.";
pub const TRAIL: &str = "A trail layer draws a glowing ribbon between two moving sockets (a blade's base and tip) that stays in the world behind them and fades: a sword swing's arc, a comet tail.";
pub const AFTERIMAGE: &str = "An afterimage layer leaves fading copies of the target's meshes along their path while it emits (sword shadows along a swing, dash echoes). Skinned meshes are copied in their current pose.";
pub const TRAIL_SOCKETS: &str = "The two points the ribbon runs between, sampled every frame: package sockets (⚓ SOCKETS & FIT, e.g. offsets from the hand bone) or node names of the target's model. From = the v = 0 edge (blade base), To = the v = 1 edge (tip).";
pub const TRAIL_LENGTH: &str = "Seconds a sample stays: how long the trail is in time (a sword swing: 0.15–0.3 s). When emission stops the rest ages out over this time.";
pub const TRAIL_SUBDIVISIONS: &str = "Curve segments (Catmull-Rom) between two samples, so a fast swing stays round instead of showing corners. 1 = straight lines; 4–8 for fast weapons.";
pub const TRAIL_MIN_DISTANCE: &str = "A new sample is kept once an end moved this far (metres); the newest point always follows the sockets. Larger = fewer samples on slow motion.";
pub const TRAIL_WIDTH: &str = "Width along the trail as (u, fraction of the From → To distance): u = 0 at the newest end, 1 at the oldest; it shrinks toward To. Empty = full width.";
pub const TRAIL_FADE_U: &str = "Opacity along the trail as (u, factor): u = 0 at the newest end, 1 at the oldest. (1 → 0) fades the tail out. Empty = no fade.";
pub const EMITTER_FADE: &str = "Lifecycle fade: in when the effect starts, out when the game stops it (the trail also ages out, copies fade over their lifetime).";
pub const EMIT: &str = "When it emits. Always = while the effect runs; Trigger = from the start trigger to the stop trigger (the game fires e.g. swing_start / swing_end from its attack state; the same trigger twice = toggle); While = while a number parameter is at or above a threshold. When emission stops what is there ages out.";
pub const SWING_PREVIEW: &str = "Fires the start / stop triggers at these times of the preview animation (each loop), e.g. at the start and end of the swing in the clip (saved in the preview block as animation triggers).";
pub const GHOST_SOURCE: &str = "Which meshes of the target's own model are copied (a weapon in the hand is a separate model: put the package on the weapon for weapon copies).";
pub const GHOST_INTERVAL: &str = "Seconds between two copies while emitting (at most one per frame).";
pub const GHOST_LIFETIME: &str = "Seconds each copy stays, fading over its life.";
pub const GHOST_MAX: &str = "Most copies at once; the oldest goes first. Copies on screen ≈ lifetime / interval.";
pub const GHOST_FADE: &str = "Opacity of a copy over its life as (fraction of the lifetime, factor). (0.8 → 0) starts at 80% and fades out. Empty = full until it goes.";

// --- light layers ------------------------------------------------------------------

pub const LIGHT: &str = "A light layer is a point light at the attach point: it lights the floor, the model and the scene around it (a weapon glow, a flash on a hit, a torch flicker). The game shows at most a few Omagari lights at once (the weakest are switched off), so keep one to three per effect.";
pub const LIGHT_ATTACH: &str = "Where the light is: Target = the target's root; Socket = follows a named point (e.g. the hand bone or the blade's middle); Segment = the middle between two sockets.";
pub const LIGHT_OFFSET: &str = "Metres from the attach point, in its space (scaled with the size fit).";
pub const LIGHT_COLOR: &str = "The light's colour. Keep the brightest channel at about 1: how bright it is comes from Intensity. Can follow a colour parameter (tint) or a curve.";
pub const LIGHT_INTENSITY: &str = "How bright, in lumens (Bevy's unit): a 60 W bulb ≈ 800 lm. Effect lights seen from a game camera: a soft glow 3 000–20 000, a strong glow 20 000–60 000, a hit flash 50 000–200 000. Make it a curve over a parameter (a glow that grows) or over age (a flash that fades).";
pub const LIGHT_RANGE: &str = "Metres: nothing farther is lit; the light fades smoothly to 0 there. 2–6 m for a character effect. Larger ranges cost more (more pixels to shade).";
pub const LIGHT_RADIUS: &str = "Size of the glowing source (metres): softer highlights on shiny surfaces. It does not change how far it lights.";
pub const LIGHT_FLICKER: &str = "Random brightness changes, different for every copy of the effect: Amount = how much (0.2 = between 80% and 120%), Speed = about how many changes per second (a torch 4–8, an electric crackle 15–25).";
pub const LIGHT_PLAY: &str = "When it shines. Loop = all the time; OnStart = once at the start; OnTrigger = each time the game fires the trigger (a flash; with a payload position it shines there; give it a Duration); While = while a parameter is at or above a threshold; Between = from a start trigger until a stop trigger (light only during a swing).";
pub const LIGHT_DURATION: &str = "Seconds an OnStart / OnTrigger light shines (a flash: it fades in and out inside this time). For the other modes the age starts over every Duration seconds. Off = shines until the effect stops.";
pub const LIGHT_FADE: &str = "Seconds to fade in when it starts and out when it stops (and when the game stops the effect; the effect waits for it). A flash fades inside its duration.";
pub const LIGHT_SHADOWS: &str = "Cast shadows. Costly (six shadow maps per light, every frame): leave off for effect lights unless the shadow matters.";
pub const LIGHT_GIZMO: &str = "Draws the lights of the preview in the viewport: a star at the light and a circle at its range (the selected light layer brighter).";
