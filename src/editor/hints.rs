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
pub const PLAY: &str = "When the layer plays. Loop = all the time; OnStart = once at the start. How long it emits is set by the spawner.";
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

pub const ASSET_IMPORT: &str = "Pick PNG or JPG files. Files outside the game assets folder are copied into its vfx/textures/; files already inside it are used where they are.";
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
