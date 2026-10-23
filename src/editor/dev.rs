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
}

fn parse_tab(name: &str) -> Option<RightTab> {
    match name.trim().to_ascii_lowercase().as_str() {
        "layer" | "layers" => Some(RightTab::Layer),
        "sockets" | "fit" | "sockets_fit" => Some(RightTab::SocketsFit),
        "preview" => Some(RightTab::Preview),
        "parameters" | "params" => Some(RightTab::Parameters),
        "triggers" => Some(RightTab::Triggers),
        "assets" => Some(RightTab::Assets),
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
                        format!("--tab {}: use layer, sockets, preview, parameters, triggers or assets", tab)
                    })?);
                }
                "--import" => options.import.push(PathBuf::from(value()?)),
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
    mut frame: Local<u32>,
    mut exit: MessageWriter<AppExit>,
) {
    *frame += 1;
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
    }
    let Some(path) = options.screenshot.clone() else { return };
    let at = options.screenshot_after.max(4);
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
                    .and_then(|image| image.to_rgb8().save(&path).map_err(|e| e.to_string()));
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
        assert!(DevOptions::parse(&args("--window-size big")).is_err());
    }
}
