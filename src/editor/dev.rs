//! Developer command line options, so agents (who cannot see the screen)
//! can check the UI: open a project at startup, pick the camera, tab and
//! asset, import files, and save a screenshot of the window, then quit.
//!
//! ```text
//! omagari --open fire --tab assets --asset smoke --channel a \
//!         --screenshot shot.png --screenshot-after 90 --window-size 1600x900
//! ```

use std::path::PathBuf;

use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use omagari::format::ChannelDef;

use super::actions::Action;
use super::assets::select_asset;
use super::model::ModelPreview;
use super::scene::Backdrop;
use super::session::{EditorSession, RightTab};
use crate::controller::EffectResource;
use crate::mcp::PreviewRequest;

/// Frames rendered before the screenshot unless `--screenshot-after` says
/// otherwise (enough for the preview and the texture thumbnails).
pub const DEFAULT_SCREENSHOT_FRAMES: u32 = 90;
/// Frames after the screenshot request before giving up and quitting.
const SCREENSHOT_TIMEOUT_FRAMES: u32 = 600;

#[derive(Resource, Clone, Debug, PartialEq, Default)]
pub struct DevOptions {
    /// `--open <project>`: a project in `saved_effects/` (with or without
    /// `.omagari.ron`).
    pub open: Option<String>,
    /// `--camera orbit|game`.
    pub camera: Option<String>,
    /// `--tab layer|sockets|preview|parameters|triggers|assets`.
    pub tab: Option<RightTab>,
    /// `--import <file>` (repeatable): imported at startup like a drop.
    pub import: Vec<PathBuf>,
    /// `--asset <id>`: shown in the ASSETS tab.
    pub asset: Option<String>,
    /// `--channel rgb|r|g|b|a`: the asset preview's view (`Some(None)` = RGB).
    pub channel: Option<Option<ChannelDef>>,
    /// `--screenshot <out.png>`: save the window and quit.
    pub screenshot: Option<PathBuf>,
    /// `--screenshot-after <frames>`.
    pub screenshot_after: u32,
    /// `--window-size <w>x<h>` (physical pixels, scale factor 1).
    pub window_size: Option<(u32, u32)>,
    /// `--stage air|floor|object|capsule|model`.
    pub stage: Option<String>,
    /// `--model <glTF path in the game assets folder>` (MODEL stage).
    pub model: Option<String>,
    /// `--animation <clip name>`.
    pub animation: Option<String>,
    /// `--animation-file <glTF path>`: the file the clip comes from.
    pub animation_file: Option<String>,
    /// `--attach-to <node>`: the node the instance is attached to.
    pub attach_to: Option<String>,
    /// `--root-height <metres>`.
    pub root_height: Option<f32>,
    /// `--anim-time <seconds>`: pause the animation at this clip time.
    pub anim_time: Option<f32>,
    /// `--motion none|circle|back_and_forth`: the preview target's motion
    /// (default radius / distance and speed).
    pub motion: Option<Option<omagari::format::PreviewMotion>>,
    /// `--focus`: the orbit camera looks at the preview target.
    pub focus: bool,
    /// `--backdrop light|dark|black`.
    pub backdrop: Option<Backdrop>,
    /// `--orbit <yaw>,<pitch>,<distance>`: the orbit camera's angles
    /// (degrees) and distance (metres) around its focus.
    pub orbit: Option<(f32, f32, f32)>,
    /// `--screenshot-crop <x>,<y>,<w>,<h>`: saves only this part of the
    /// window (physical pixels), for close-ups.
    pub crop: Option<(u32, u32, u32, u32)>,
    /// `--stop-at <frame>`: stops the preview instance (like STOP) at this
    /// frame, to check the fade out.
    pub stop_at: Option<u32>,
    /// `--fire <trigger>`: fires a trigger a few frames before the
    /// screenshot.
    pub fire: Option<String>,
    /// `--fire-before <frames>`: how many frames before the screenshot
    /// `--fire` fires (default 6).
    pub fire_before: Option<u32>,
    /// `--focus-node <name>`: the orbit camera looks at this node of the
    /// preview model (e.g. `mixamorig:Hips`) instead of the target.
    pub focus_node: Option<String>,
}

/// Parses `a,b,c,...` into exactly `n` numbers.
fn numbers<T: std::str::FromStr>(text: &str, n: usize) -> Option<Vec<T>> {
    let values: Vec<T> = text.split(',').map(|v| v.trim().parse().ok()).collect::<Option<_>>()?;
    (values.len() == n).then_some(values)
}

fn parse_tab(name: &str) -> Option<RightTab> {
    match name.trim().to_ascii_lowercase().as_str() {
        "layer" | "layers" => Some(RightTab::Layer),
        "sockets" | "fit" | "sockets_fit" => Some(RightTab::SocketsFit),
        "preview" => Some(RightTab::Preview),
        "parameters" | "params" => Some(RightTab::Parameters),
        "triggers" => Some(RightTab::Triggers),
        "assets" => Some(RightTab::Assets),
        "hierarchy" | "model" => Some(RightTab::Hierarchy),
        _ => None,
    }
}

impl DevOptions {
    /// Reads the options from the command line arguments (the program name
    /// first); other arguments are ignored.
    pub fn parse(args: &[String]) -> Result<Self, String> {
        let mut options = DevOptions {
            screenshot_after: DEFAULT_SCREENSHOT_FRAMES,
            ..default()
        };
        let mut i = 1;
        while i < args.len() {
            let flag = args[i].as_str();
            let value = || args.get(i + 1).cloned().ok_or_else(|| format!("{} needs a value", flag));
            match flag {
                "--open" => options.open = Some(value()?),
                "--camera" => {
                    let camera = value()?;
                    if super::camera::PreviewCamera::parse(&camera).is_none() {
                        return Err(format!("--camera {}: use orbit or game", camera));
                    }
                    options.camera = Some(camera);
                }
                "--tab" => {
                    let tab = value()?;
                    options.tab = Some(parse_tab(&tab).ok_or_else(|| {
                        format!("--tab {}: use layer, sockets, preview, parameters, triggers, assets or hierarchy", tab)
                    })?);
                }
                "--import" => options.import.push(PathBuf::from(value()?)),
                "--stage" => {
                    let stage = value()?;
                    if super::scene::Stage::parse(&stage).is_none() {
                        return Err(format!("--stage {}: use air, floor, object, capsule or model", stage));
                    }
                    options.stage = Some(stage);
                }
                "--model" => options.model = Some(value()?),
                "--motion" => {
                    let motion = value()?;
                    options.motion = Some(
                        crate::mcp::motion_from_json(&serde_json::Value::String(motion.clone()))
                            .map_err(|e| format!("--motion {}: {}", motion, e))?,
                    );
                }
                "--focus" => {
                    options.focus = true;
                    i += 1;
                    continue;
                }
                "--backdrop" => {
                    let name = value()?;
                    options.backdrop = Some(
                        Backdrop::ALL
                            .into_iter()
                            .find(|b| b.name().eq_ignore_ascii_case(name.trim()))
                            .ok_or_else(|| format!("--backdrop {}: use light, dark or black", name))?,
                    );
                }
                "--animation" => options.animation = Some(value()?),
                "--animation-file" => options.animation_file = Some(value()?),
                "--attach-to" => options.attach_to = Some(value()?),
                "--root-height" | "--anim-time" => {
                    let text = value()?;
                    let number: f32 = text.parse().map_err(|_| format!("{} {}: not a number", flag, text))?;
                    if flag == "--root-height" {
                        options.root_height = Some(number);
                    } else {
                        options.anim_time = Some(number);
                    }
                }
                "--asset" => options.asset = Some(value()?),
                "--channel" => {
                    let channel = value()?;
                    options.channel = Some(if channel.eq_ignore_ascii_case("rgb") {
                        None
                    } else {
                        Some(
                            ChannelDef::parse(&channel)
                                .filter(|c| *c != ChannelDef::Luminance)
                                .ok_or_else(|| format!("--channel {}: use rgb, r, g, b or a", channel))?,
                        )
                    });
                }
                "--screenshot" => options.screenshot = Some(PathBuf::from(value()?)),
                "--orbit" => {
                    let text = value()?;
                    let v = numbers::<f32>(&text, 3)
                        .ok_or_else(|| format!("--orbit {}: use <yaw>,<pitch>,<distance>, e.g. 30,10,2.5", text))?;
                    options.orbit = Some((v[0], v[1], v[2]));
                }
                "--screenshot-crop" => {
                    let text = value()?;
                    let v = numbers::<u32>(&text, 4)
                        .ok_or_else(|| format!("--screenshot-crop {}: use <x>,<y>,<w>,<h> in pixels", text))?;
                    options.crop = Some((v[0], v[1], v[2], v[3]));
                }
                "--stop-at" => {
                    let text = value()?;
                    options.stop_at = Some(text.parse().map_err(|_| format!("--stop-at {}: not a frame number", text))?);
                }
                "--fire" => options.fire = Some(value()?),
                "--fire-before" => {
                    let text = value()?;
                    options.fire_before = Some(text.parse().map_err(|_| format!("--fire-before {}: not a frame count", text))?);
                }
                "--focus-node" => options.focus_node = Some(value()?),
                "--screenshot-after" => {
                    let frames = value()?;
                    options.screenshot_after =
                        frames.parse().map_err(|_| format!("--screenshot-after {}: not a frame count", frames))?;
                }
                "--window-size" => {
                    let size = value()?;
                    let parsed = size
                        .split_once(['x', 'X'])
                        .and_then(|(w, h)| Some((w.trim().parse().ok()?, h.trim().parse().ok()?)));
                    options.window_size = Some(parsed.ok_or_else(|| format!("--window-size {}: use e.g. 1600x900", size))?);
                }
                _ => {
                    i += 1;
                    continue;
                }
            }
            i += 2;
        }
        Ok(options)
    }

    /// The project file name `--open` names.
    fn open_file_name(&self) -> Option<String> {
        let name = self.open.as_ref()?.trim().replace('\\', "/");
        let name = name.rsplit('/').next().unwrap_or_default();
        Some(if name.ends_with(".omagari.ron") { name.to_string() } else { format!("{}.omagari.ron", name) })
    }
}

/// Queues the startup actions: open (with the camera), then the imports.
pub fn queue_startup_actions(options: Res<DevOptions>, mut session: ResMut<EditorSession>) {
    if let Some(name) = options.open_file_name() {
        session.actions.push(Action::Preview(PreviewRequest {
            name,
            camera: options.camera.clone(),
            stage: options.stage.clone(),
            model: options.model.clone(),
            animation: options.animation.clone(),
            animation_file: options.animation_file.clone(),
            attach_to: options.attach_to.clone(),
            root_height: options.root_height,
            motion: options.motion,
            ..default()
        }));
    }
    if !options.import.is_empty() {
        session.actions.push(Action::ImportFiles(options.import.clone()));
    }}

/// Applies the view options once the startup actions ran, and takes the
/// screenshot after the requested number of frames.
pub fn run_dev_options(
    mut commands: Commands,
    options: Res<DevOptions>,
    mut session: ResMut<EditorSession>,
    mut res: ResMut<EffectResource>,
    mut model: ResMut<ModelPreview>,
    mut backdrop: ResMut<Backdrop>,
    mut orbits: Query<&mut bevy_panorbit_camera::PanOrbitCamera>,
    nodes: Query<(&Name, &GlobalTransform)>,
    mut frame: Local<u32>,
    mut exit: MessageWriter<AppExit>,
) {
    *frame += 1;
    let at = options.screenshot_after.max(4);
    if let Some((yaw, pitch, distance)) = options.orbit {
        if *frame == 5 || *frame + 1 == at {
            for mut orbit in orbits.iter_mut() {
                orbit.target_yaw = yaw.to_radians();
                orbit.target_pitch = pitch.to_radians();
                orbit.target_radius = distance;
                orbit.yaw = Some(orbit.target_yaw);
                orbit.pitch = Some(orbit.target_pitch);
                orbit.radius = Some(distance);
                orbit.force_update = true;
            }
        }
    }
    if let Some(node) = &options.focus_node {
        if *frame + 1 == at || *frame + 3 == at {
            if let Some((_, global)) = nodes.iter().find(|(n, _)| n.as_str() == node) {
                for mut orbit in orbits.iter_mut() {
                    orbit.target_focus = global.translation();
                    orbit.focus = global.translation();
                    orbit.force_update = true;
                }
            }
        }
    }
    if options.stop_at == Some(*frame) {
        session.actions.push(Action::Stop);
    }
    if let Some(trigger) = &options.fire {
        if *frame + options.fire_before.unwrap_or(6) == at {
            session.actions.push(Action::FireTrigger { name: trigger.clone(), random_point: false });
        }
    }
    if *frame == 3 {
        let opened = options.open_file_name().filter(|name| res.context.filename.as_ref() == Some(name));
        if let Some(name) = opened.filter(|_| options.import.is_empty()) {
            res.context.status = Some(format!("Opened {} (--open)", name));
        }
        if let Some(id) = &options.asset {
            select_asset(&mut session, id);
        }
        if let Some(tab) = options.tab {
            session.tab = tab;
        }
        if let Some(channel) = options.channel {
            session.asset_view = channel;
        }
        if let Some(b) = options.backdrop {
            *backdrop = b;
        }
        if let Some(time) = options.anim_time {
            model.seek = Some(time);
            model.paused = true;
        }
    }
    if options.focus && (*frame == 3 || *frame + 2 == options.screenshot_after.max(4)) {
        session.focus_camera = true;
    }
    let Some(path) = options.screenshot.clone() else { return };
    let crop = options.crop;
    if *frame == at {
        info!("dev: taking a screenshot to {}", path.display());
        commands
            .spawn(Screenshot::primary_window())
            .observe(move |captured: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
                let result = captured
                    .image
                    .clone()
                    .try_into_dynamic()
                    .map_err(|e| e.to_string())
                    .and_then(|image| {
                        let image = match crop {
                            Some((x, y, w, h)) => image.crop_imm(x, y, w, h),
                            None => image,
                        };
                        image.to_rgb8().save(&path).map_err(|e| e.to_string())
                    });
                match result {
                    Ok(()) => eprintln!("omagari: screenshot saved to {}", path.display()),
                    Err(e) => eprintln!("omagari: cannot save the screenshot {}: {}", path.display(), e),
                }
                exit.write(AppExit::Success);
            });
    } else if *frame == at + SCREENSHOT_TIMEOUT_FRAMES {
        eprintln!("omagari: the screenshot was not captured; quitting");
        exit.write(AppExit::error());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(text: &str) -> Vec<String> {
        std::iter::once("omagari").chain(text.split_whitespace()).map(str::to_string).collect()
    }

    #[test]
    fn dev_flags_parse() {
        let o = DevOptions::parse(&args(
            "--game-assets x --open fire --camera game --tab assets --asset smoke --channel a --import a.png --import b.jpg --screenshot out.png --screenshot-after 120 --window-size 1600x900",
        ))
        .unwrap();
        assert_eq!(o.open_file_name().as_deref(), Some("fire.omagari.ron"));
        assert_eq!(o.camera.as_deref(), Some("game"));
        assert_eq!(o.tab, Some(RightTab::Assets));
        assert_eq!(o.asset.as_deref(), Some("smoke"));
        assert_eq!(o.channel, Some(Some(ChannelDef::A)));
        assert_eq!(o.import, vec![PathBuf::from("a.png"), PathBuf::from("b.jpg")]);
        assert_eq!(o.screenshot, Some(PathBuf::from("out.png")));
        assert_eq!(o.screenshot_after, 120);
        assert_eq!(o.window_size, Some((1600, 900)));

        let o = DevOptions::parse(&args("--open saved_effects/fire.omagari.ron --channel rgb")).unwrap();
        assert_eq!(o.open_file_name().as_deref(), Some("fire.omagari.ron"));
        assert_eq!(o.channel, Some(None));
        assert_eq!(o.screenshot_after, DEFAULT_SCREENSHOT_FRAMES);
        assert_eq!(DevOptions::parse(&args("")).unwrap().open, None);

        assert!(DevOptions::parse(&args("--camera top")).is_err());
        assert!(DevOptions::parse(&args("--tab nope")).is_err());
        assert!(DevOptions::parse(&args("--screenshot")).is_err());
        let o = DevOptions::parse(&args(
            "--stage model --model anim/tpose.glb --animation mixamo.com --animation-file anim/run.glb --attach-to Hips --root-height 0.9 --anim-time 0.25 --tab hierarchy",
        ))
        .unwrap();
        assert_eq!(o.stage.as_deref(), Some("model"));
        assert_eq!(o.model.as_deref(), Some("anim/tpose.glb"));
        assert_eq!((o.animation.as_deref(), o.animation_file.as_deref()), (Some("mixamo.com"), Some("anim/run.glb")));
        assert_eq!((o.attach_to.as_deref(), o.root_height, o.anim_time), (Some("Hips"), Some(0.9), Some(0.25)));
        assert_eq!(o.tab, Some(RightTab::Hierarchy));
        assert!(DevOptions::parse(&args("--stage moon")).is_err());
        let o = DevOptions::parse(&args("--focus --motion none --open x --backdrop Dark")).unwrap();
        assert_eq!(o.backdrop, Some(Backdrop::DarkRoom));
        assert!(o.focus);
        assert_eq!(o.motion, Some(None));
        assert_eq!(o.open.as_deref(), Some("x"));
        assert!(DevOptions::parse(&args("--motion fly")).is_err());
        assert!(DevOptions::parse(&args("--anim-time soon")).is_err());
        assert!(DevOptions::parse(&args("--window-size big")).is_err());
        let o = DevOptions::parse(&args("--fire slam --fire-before 20")).unwrap();
        assert_eq!((o.fire.as_deref(), o.fire_before), (Some("slam"), Some(20)));
        assert!(DevOptions::parse(&args("--fire-before soon")).is_err());
    }
}
