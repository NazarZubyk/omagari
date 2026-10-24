//! Plays exported Omagari packages in a small Bevy game, without any model
//! file: a capsule "hero" runs in a circle and swings a box "sword".
//!
//! ```text
//! cargo run --release --example game_integration
//! ```
//!
//! The packages are in `assets/vfx/` (exported from the projects of the
//! same names in `saved_effects/`, and `magicburst` from `examples/`):
//!
//! - `status_skin`: a status effect (poison) on the hero: applied, refreshed,
//!   ticking (trigger `tick`), stopped when it runs out.
//! - `shield_bubble`: a progression parameter (`shield`) driven every frame.
//! - `blade_swing`: a weapon trail and afterimages on the sword entity, started
//!   and stopped by `swing_start` / `swing_end`.
//! - `hit_flash`: a light flash and sparks at the tip of the sword
//!   (`trigger_at`).
//! - `shockwave_ripple`: a distortion ripple (`slam`), needs the camera's
//!   screen-space transmission (`omagari::distortion_camera()`).
//! - `magicburst`: a one-shot in the world (`spawn_vfx_at`) that cleans up by
//!   itself and sends `VfxFinished`.
//!
//! Keys: `P` applies (or refreshes) the poison, `Space` fires the slam.
//!
//! See `docs/GAME_INTEGRATION.md`.

use std::f32::consts::{FRAC_PI_2, TAU};

use bevy::camera::ScalingMode;
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;
use bevy_hanabi::prelude::*;
use omagari::{
    OmagariPlugin, TriggerArgs, VfxCommandsExt, VfxControl, VfxFinished, VfxPackage, VfxSeparateModel, VfxSettings,
    VfxSpawn,
};

fn main() {
    App::new()
        // Order: Bevy, then bevy_hanabi, then Omagari.
        .add_plugins((DefaultPlugins, HanabiPlugin, OmagariPlugin))
        // At most 4 Omagari point lights at once (default 8).
        .insert_resource(VfxSettings { max_lights: 4, ..default() })
        .insert_resource(GlobalAmbientLight { brightness: 300.0, ..default() })
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (move_hero, swing_sword, poison_status, shield_from_health, slam, bursts, log_finished),
        )
        .run();
}

/// The packages, loaded once. Paths are relative to the game's `assets/`.
#[derive(Resource)]
struct Packages {
    poison: Handle<VfxPackage>,
    burst: Handle<VfxPackage>,
}

#[derive(Component)]
struct Hero;

/// A status effect on an entity and its VFX instance.
#[derive(Component, Default)]
struct Poisoned {
    /// Seconds left; 0 = not poisoned.
    remaining: f32,
    /// Seconds to the next damage tick.
    next_tick: f32,
    vfx: Option<Entity>,
}

/// The shield effect of the hero.
#[derive(Component)]
struct Shield {
    vfx: Entity,
}

/// The ripple effect of the hero.
#[derive(Component)]
struct Slam {
    vfx: Entity,
    every: Timer,
}

/// The sword: swings every `period` seconds for `duration` seconds.
#[derive(Component)]
struct Sword {
    swing_vfx: Entity,
    hit_vfx: Entity,
    time: f32,
    swinging: bool,
    hit_done: bool,
}

const SWING_PERIOD: f32 = 1.6;
const SWING_DURATION: f32 = 0.3;
const SWING_ARC: f32 = 3.2; // radians swept by the blade
const BLADE_LENGTH: f32 = 1.1;

fn setup(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Camera: HDR + bloom make colours above 1 glow; distortion layers need
    // screen-space transmission. `distortion_camera()` adds `Hdr` and the
    // transmission settings, so do not add `Hdr` again in the same bundle.
    // An orthographic isometric camera like the editor's GAME preset (change
    // it to your own camera).
    commands.spawn((
        Camera3d::default(),
        omagari::distortion_camera(),
        Projection::from(OrthographicProjection {
            scaling_mode: ScalingMode::FixedVertical { viewport_height: 12.0 },
            ..OrthographicProjection::default_3d()
        }),
        Bloom::NATURAL,
        Transform::from_xyz(20.0, 20.0, 20.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        DirectionalLight { illuminance: 4000.0, shadow_maps_enabled: true, ..default() },
        Transform::from_xyz(4.0, 10.0, 2.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // A tiled floor: a distortion bends what is behind it, so a plain
    // single-coloured floor would show no ripple.
    let tile = meshes.add(Cuboid::new(1.0, 0.1, 1.0));
    let dark = materials.add(Color::srgb(0.25, 0.27, 0.3));
    let light = materials.add(Color::srgb(0.45, 0.47, 0.5));
    for x in -8..8 {
        for z in -8..8 {
            let material = if (x + z) % 2 == 0 { dark.clone() } else { light.clone() };
            commands.spawn((
                Mesh3d(tile.clone()),
                MeshMaterial3d(material),
                Transform::from_xyz(x as f32 + 0.5, -0.05, z as f32 + 0.5),
            ));
        }
    }

    let packages = Packages {
        poison: asset_server.load("vfx/status_skin.vfx.ron"),
        burst: asset_server.load("vfx/magicburst.vfx.ron"),
    };
    let shield: Handle<VfxPackage> = asset_server.load("vfx/shield_bubble.vfx.ron");
    let ripple: Handle<VfxPackage> = asset_server.load("vfx/shockwave_ripple.vfx.ron");
    let swing: Handle<VfxPackage> = asset_server.load("vfx/blade_swing.vfx.ron");
    let hit: Handle<VfxPackage> = asset_server.load("vfx/hit_flash.vfx.ron");

    // The hero: a 1.8 m capsule whose origin is its centre (0.9 m above the
    // feet). Surface layers (the poison skin) draw over its own meshes.
    let hero = commands
        .spawn((
            Name::new("hero"),
            Hero,
            Mesh3d(meshes.add(Capsule3d::new(0.35, 1.1))),
            MeshMaterial3d(materials.add(Color::srgb(0.75, 0.7, 0.65))),
            Transform::from_xyz(3.0, 0.9, 0.0),
            Poisoned::default(),
        ))
        .id();

    // Effects that stay on the hero. The instance is a child of its target.
    let shield_vfx = commands.spawn_vfx_with(&shield, hero, VfxSpawn { params: &[("shield", 1.0.into())] });
    let slam_vfx = commands.spawn_vfx(&ripple, hero);
    let hit_vfx = commands.spawn_vfx(&hit, hero);
    commands.entity(hero).insert((
        Shield { vfx: shield_vfx },
        Slam { vfx: slam_vfx, every: Timer::from_seconds(3.0, TimerMode::Repeating) },
    ));

    // The sword: a pivot in the hero's hand with the blade along its +Y.
    // It is a separate model: `VfxSeparateModel` keeps the hero's surface
    // layers off it. Its own package (trail + afterimages) uses sockets
    // that are offsets from the sword's root (`blade_base`, `blade_tip`).
    let sword = commands
        .spawn((
            Name::new("sword"),
            VfxSeparateModel,
            Transform::from_xyz(0.0, 0.2, 0.0),
            Visibility::default(),
            ChildOf(hero),
        ))
        .id();
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(0.06, BLADE_LENGTH - 0.1, 0.02))),
        MeshMaterial3d(materials.add(Color::srgb(0.8, 0.85, 0.9))),
        Transform::from_xyz(0.0, 0.1 + (BLADE_LENGTH - 0.1) * 0.5, 0.0),
        ChildOf(sword),
    ));
    let swing_vfx = commands.spawn_vfx(&swing, sword);
    commands.entity(sword).insert(Sword { swing_vfx, hit_vfx, time: 0.0, swinging: false, hit_done: false });

    commands.insert_resource(packages);
}

/// The hero runs in a circle, facing where it goes.
fn move_hero(time: Res<Time>, mut hero: Single<&mut Transform, With<Hero>>) {
    let angle = time.elapsed_secs() * 0.6;
    let position = Vec3::new(3.0 * angle.cos(), 0.9, 3.0 * angle.sin());
    let forward = Vec3::new(-angle.sin(), 0.0, angle.cos());
    hero.translation = position;
    hero.look_to(forward, Vec3::Y);
}

/// Weapon pattern: the attack state fires `swing_start` / `swing_end` (the
/// trail and afterimages run in between) and `trigger_at("hit")` at the
/// contact point.
fn swing_sword(
    time: Res<Time>,
    mut swords: Query<(&mut Sword, &mut Transform, &GlobalTransform)>,
    mut vfx: VfxControl,
) {
    for (mut sword, mut transform, global) in swords.iter_mut() {
        sword.time = (sword.time + time.delta_secs()) % SWING_PERIOD;
        let t = sword.time / SWING_DURATION;
        let swinging = t < 1.0;
        if swinging && !sword.swinging {
            vfx.trigger(sword.swing_vfx, "swing_start");
            sword.hit_done = false;
        }
        if !swinging && sword.swinging {
            vfx.trigger(sword.swing_vfx, "swing_end");
        }
        sword.swinging = swinging;

        // The blade lies flat (its +Y turned to the hero's side) and sweeps
        // around the hero during the swing.
        let progress = if swinging { t } else { 1.0 };
        let angle = -SWING_ARC * 0.5 + SWING_ARC * progress;
        transform.rotation = Quat::from_rotation_y(angle) * Quat::from_rotation_z(-FRAC_PI_2);

        // The hit: half-way through the swing, at the blade's tip.
        if swinging && t >= 0.5 && !sword.hit_done {
            sword.hit_done = true;
            let tip = global.transform_point(Vec3::Y * BLADE_LENGTH);
            let outward = (tip - global.translation()).normalize_or_zero();
            vfx.trigger_at(sword.hit_vfx, "hit", TriggerArgs::at(tip).with_direction(outward).with_strength(1.0));
        }
    }
}

/// Status pattern: apply = spawn the instance, or `restart` it when it is
/// still fading out; each damage tick fires `tick`; expiry = `stop` (it fades
/// out, then the instance despawns and `VfxFinished` is sent).
fn poison_status(
    mut commands: Commands,
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    packages: Res<Packages>,
    mut statuses: Query<(Entity, &mut Poisoned)>,
    mut vfx: VfxControl,
    mut every: Local<f32>,
) {
    // Apply every 7 s (and on P), for 4 s.
    *every -= time.delta_secs();
    let apply = keys.just_pressed(KeyCode::KeyP) || *every <= 0.0;
    if *every <= 0.0 {
        *every = 7.0;
    }
    for (target, mut status) in statuses.iter_mut() {
        if apply {
            status.remaining = 4.0;
            status.next_tick = 1.0;
            match status.vfx {
                // Still alive (running or fading out): play it again.
                Some(instance) if vfx.is_alive(instance) => {
                    vfx.restart(instance);
                }
                _ => status.vfx = Some(commands.spawn_vfx(&packages.poison, target)),
            }
        }
        let Some(instance) = status.vfx else { continue };
        if status.remaining <= 0.0 {
            continue;
        }
        status.remaining -= time.delta_secs();
        status.next_tick -= time.delta_secs();
        if status.next_tick <= 0.0 {
            status.next_tick += 1.0;
            vfx.trigger(instance, "tick"); // the skin flashes (a pulse of `flash`)
        }
        if status.remaining <= 0.0 {
            vfx.stop(instance);
        }
    }
}

/// Progression pattern: a game value mapped to a package parameter every
/// frame. `set_param` clamps to the parameter's range and smooths.
fn shield_from_health(time: Res<Time>, shields: Query<&Shield>, mut vfx: VfxControl) {
    let health = 0.5 + 0.5 * (time.elapsed_secs() * TAU / 8.0).sin();
    for shield in shields.iter() {
        vfx.set_param(shield.vfx, "shield", health);
    }
}

/// A trigger without a payload: the ripple plays at the target.
fn slam(time: Res<Time>, keys: Res<ButtonInput<KeyCode>>, mut slams: Query<&mut Slam>, mut vfx: VfxControl) {
    for mut slam in slams.iter_mut() {
        if slam.every.tick(time.delta()).just_finished() || keys.just_pressed(KeyCode::Space) {
            vfx.trigger(slam.vfx, "slam");
        }
    }
}

/// One-shots in the world: every 2.5 s a burst somewhere on the floor. Its
/// only layer plays once, so the instance finishes by itself.
fn bursts(mut commands: Commands, time: Res<Time>, packages: Res<Packages>, mut next: Local<f32>, mut count: Local<u32>) {
    *next -= time.delta_secs();
    if *next > 0.0 {
        return;
    }
    *next = 2.5;
    *count += 1;
    let angle = *count as f32 * 2.4;
    let point = Vec3::new(5.5 * angle.cos(), 0.0, 5.5 * angle.sin());
    commands.spawn_vfx_at(&packages.burst, Transform::from_translation(point));
}

/// Every instance ends with one `VfxFinished`: stopped, finished by itself,
/// or its target despawned.
fn log_finished(mut finished: MessageReader<VfxFinished>) {
    for f in finished.read() {
        info!("vfx {} finished (target {})", f.instance, f.target);
    }
}
