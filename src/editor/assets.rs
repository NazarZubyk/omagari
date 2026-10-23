//! The ASSETS tab: the project's textures with thumbnails, a large preview
//! with a per-channel view, their settings, rename / delete / extract
//! channel, adding built-ins, and importing files (native file dialog or
//! drag & drop onto the window, both become [`Action::ImportFiles`]).
//!
//! Previews are decoded on a background thread from the file (the `image`
//! crate, like Bevy's loader) into small RGBA images registered with egui
//! ([`TexturePreviews`]); a changed file is decoded again.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::tasks::{futures::check_ready, AsyncComputeTaskPool, Task};
use bevy::window::FileDragAndDrop;
use bevy_egui::egui::{self, RichText};
use bevy_egui::{EguiTextureHandle, EguiUserTextures};
use image::RgbaImage;
use omagari::format::{
    builtin_texture_path, AssetKind, ChannelDef, ProjectV2, TextureColorSpace, TextureFilter, TexturePurpose,
    TextureWrap, BUILTIN_NOISE_TEXTURE, PARTICLE_TEXTURES,
};

use super::actions::Action;
use super::hints;
use super::problems::{badge, subject_box, ACCENT};
use super::session::{EditorSession, RightTab};
use crate::asset_library::{self as library, DecodedPreview, IMPORT_DIR, TEXTURE_EXTENSIONS};
use crate::settings::GameAssets;

/// Largest side of the decoded preview (the large view).
pub const PREVIEW_SIZE: u32 = 512;
/// Largest side of the list thumbnails.
pub const THUMBNAIL_SIZE: u32 = 96;
/// Largest side of the large view in the details (points).
const LARGE_VIEW_SIZE: f32 = 320.0;
/// Seconds between checks whether previewed files changed on disk.
const FILE_CHECK_INTERVAL: f64 = 1.0;

/// The views of the large preview: RGB, then single channels.
const VIEWS: [(Option<ChannelDef>, &str); 5] = [
    (None, "RGB"),
    (Some(ChannelDef::R), "R"),
    (Some(ChannelDef::G), "G"),
    (Some(ChannelDef::B), "B"),
    (Some(ChannelDef::A), "A"),
];

// --- import dialog -----------------------------------------------------------

/// The native file dialog of IMPORT. It runs on its own thread (rfd's async
/// dialog, blocked on there), so the editor keeps drawing while it is open;
/// [`poll_import_dialog`] turns the picked files into an import.
#[derive(Default)]
pub struct ImportDialog {
    picked: Option<Arc<Mutex<Option<Vec<PathBuf>>>>>,
}

impl ImportDialog {
    pub fn is_open(&self) -> bool {
        self.picked.is_some()
    }

    /// Opens the dialog (once at a time) in `start_dir`.
    pub fn open(&mut self, start_dir: Option<PathBuf>) {
        if self.is_open() {
            return;
        }
        let slot = Arc::new(Mutex::new(None));
        let result = slot.clone();
        std::thread::spawn(move || {
            let mut dialog = rfd::AsyncFileDialog::new()
                .set_title("Import textures")
                .add_filter("Textures (PNG, JPG, KTX2)", &TEXTURE_EXTENSIONS);
            if let Some(dir) = start_dir.filter(|d| d.is_dir()) {
                dialog = dialog.set_directory(dir);
            }
            let files = bevy::tasks::block_on(dialog.pick_files());
            let paths = files
                .map(|files| files.into_iter().map(|f| f.path().to_path_buf()).collect())
                .unwrap_or_default();
            if let Ok(mut slot) = result.lock() {
                *slot = Some(paths);
            }
        });
        self.picked = Some(slot);
    }

    /// The files picked once the dialog closed (empty when cancelled).
    fn take(&mut self) -> Option<Vec<PathBuf>> {
        let picked = self.picked.as_ref()?.lock().ok()?.take()?;
        self.picked = None;
        Some(picked)
    }
}

pub fn poll_import_dialog(mut session: ResMut<EditorSession>) {
    if !session.import_dialog.is_open() {
        return;
    }
    if let Some(files) = session.import_dialog.take() {
        if !files.is_empty() {
            session.actions.push(Action::ImportFiles(files));
        }
    }
}

/// Files dropped onto the window are imported; while files hover over it,
/// the UI shows a drop hint.
pub fn handle_file_drops(mut drops: MessageReader<FileDragAndDrop>, mut session: ResMut<EditorSession>) {
    let mut files = Vec::new();
    for drop in drops.read() {
        match drop {
            FileDragAndDrop::DroppedFile { path_buf, .. } => {
                files.push(path_buf.clone());
                session.drop_hover = false;
            }
            FileDragAndDrop::HoveredFile { .. } => session.drop_hover = true,
            FileDragAndDrop::HoveredFileCanceled { .. } => session.drop_hover = false,
        }
    }
    if !files.is_empty() {
        session.actions.push(Action::ImportFiles(files));
    }
}

// --- previews ----------------------------------------------------------------

/// When a previewed file was last modified (`None`: built-in or unknown).
type Stamp = Option<SystemTime>;

enum Entry {
    Loading(Task<Result<DecodedPreview, String>>, Stamp),
    Ready(Box<Ready>),
    Failed(String, Stamp),
}

struct Ready {
    preview: DecodedPreview,
    thumbnail: (egui::TextureId, Handle<Image>),
    stamp: Stamp,
}

/// The large view: which texture and channel, and its egui texture.
struct View {
    key: (String, Option<ChannelDef>),
    texture: egui::TextureId,
    handle: Handle<Image>,
    size: egui::Vec2,
}

/// Decoded previews of the textures the ASSETS tab shows.
#[derive(Resource, Default)]
pub struct TexturePreviews {
    entries: HashMap<String, Entry>,
    /// Paths the UI asked for since the last update.
    wanted: HashSet<String>,
    /// The large view the UI asked for since the last update.
    wanted_view: Option<(String, Option<ChannelDef>)>,
    view: Option<View>,
    /// Paths whose files changed (written by an action): decoded again.
    stale: HashSet<String>,
    last_file_check: f64,
}

/// What the UI can show for a texture.
pub enum PreviewState<'a> {
    Loading,
    Failed(&'a str),
    Ready {
        thumbnail: egui::TextureId,
        preview: &'a DecodedPreview,
    },
}

impl TexturePreviews {
    pub fn get(&mut self, path: &str) -> PreviewState<'_> {
        self.wanted.insert(path.to_string());
        match self.entries.get(path) {
            None | Some(Entry::Loading(..)) => PreviewState::Loading,
            Some(Entry::Failed(message, _)) => PreviewState::Failed(message),
            Some(Entry::Ready(ready)) => PreviewState::Ready {
                thumbnail: ready.thumbnail.0,
                preview: &ready.preview,
            },
        }
    }

    /// The large view of a texture in one channel (`None` = RGB), once it
    /// is ready.
    pub fn view(&mut self, path: &str, channel: Option<ChannelDef>) -> Option<(egui::TextureId, egui::Vec2)> {
        let key = (path.to_string(), channel);
        self.wanted.insert(path.to_string());
        self.wanted_view = Some(key.clone());
        self.view.as_ref().filter(|v| v.key == key).map(|v| (v.texture, v.size))
    }

    /// The file at `path` was rewritten: decode it again.
    pub fn invalidate(&mut self, path: &str) {
        self.stale.insert(path.to_string());
    }
}

fn file_stamp(path: &str, root: &std::path::Path) -> Stamp {
    if library::is_builtin(path) {
        return None;
    }
    std::fs::metadata(library::texture_file(path, root)).and_then(|m| m.modified()).ok()
}

/// Registers pixels as an egui texture; `nearest` = hard pixels when the
/// image is shown enlarged.
fn egui_image(
    pixels: &RgbaImage,
    nearest: bool,
    images: &mut Assets<Image>,
    textures: &mut EguiUserTextures,
) -> (egui::TextureId, Handle<Image>) {
    let mut image = Image::new(
        Extent3d { width: pixels.width(), height: pixels.height(), depth_or_array_layers: 1 },
        TextureDimension::D2,
        pixels.as_raw().clone(),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    if nearest {
        image.sampler = ImageSampler::nearest();
    }
    let handle = images.add(image);
    let id = textures.add_image(EguiTextureHandle::Strong(handle.clone()));
    (id, handle)
}

fn free(textures: &mut EguiUserTextures, handle: &Handle<Image>) {
    textures.remove_image(handle);
}

/// Starts decoding the previews the UI asked for, collects finished ones,
/// re-decodes changed files and builds the large view.
pub fn update_texture_previews(
    mut previews: ResMut<TexturePreviews>,
    mut images: ResMut<Assets<Image>>,
    mut textures: ResMut<EguiUserTextures>,
    game_assets: Res<GameAssets>,
    time: Res<Time<Real>>,
) {
    let previews = &mut *previews;
    let root = game_assets.assets_root();
    let now = time.elapsed_secs_f64();
    let check_files = now - previews.last_file_check >= FILE_CHECK_INTERVAL;
    if check_files {
        previews.last_file_check = now;
    }

    // Changed files: forget the entry, so it is decoded again below.
    let mut changed: Vec<String> = previews.stale.drain().collect();
    if check_files {
        for (path, entry) in previews.entries.iter() {
            let stamp = match entry {
                Entry::Loading(_, s) | Entry::Failed(_, s) => *s,
                Entry::Ready(r) => r.stamp,
            };
            if !matches!(entry, Entry::Loading(..)) && file_stamp(path, &root) != stamp {
                changed.push(path.clone());
            }
        }
    }
    for path in changed {
        if let Some(Entry::Ready(ready)) = previews.entries.remove(&path) {
            free(&mut textures, &ready.thumbnail.1);
        }
        if previews.view.as_ref().is_some_and(|v| v.key.0 == path) {
            if let Some(view) = previews.view.take() {
                free(&mut textures, &view.handle);
            }
        }
    }

    for path in previews.wanted.drain() {
        if previews.entries.contains_key(&path) {
            continue;
        }
        let stamp = file_stamp(&path, &root);
        let (task_path, task_root) = (path.clone(), root.clone());
        let task = AsyncComputeTaskPool::get().spawn(async move {
            let bytes = library::texture_bytes(&task_path, &task_root)?;
            library::decode_preview(&bytes, PREVIEW_SIZE)
        });
        previews.entries.insert(path, Entry::Loading(task, stamp));
    }

    for entry in previews.entries.values_mut() {
        let Entry::Loading(task, stamp) = entry else { continue };
        let Some(result) = check_ready(task) else { continue };
        let stamp = *stamp;
        *entry = match result {
            Ok(preview) => {
                let small = image::imageops::thumbnail(
                    &preview.pixels,
                    preview.pixels.width().min(THUMBNAIL_SIZE),
                    preview.pixels.height().min(THUMBNAIL_SIZE),
                );
                let thumbnail = egui_image(&small, false, &mut images, &mut textures);
                Entry::Ready(Box::new(Ready { preview, thumbnail, stamp }))
            }
            Err(message) => Entry::Failed(message, stamp),
        };
    }

    // The large view, built from the decoded preview.
    if let Some(key) = previews.wanted_view.take() {
        if previews.view.as_ref().is_none_or(|v| v.key != key) {
            if let Some(Entry::Ready(ready)) = previews.entries.get(&key.0) {
                let pixels = library::view_pixels(&ready.preview.pixels, key.1);
                let size = egui::vec2(pixels.width() as f32, pixels.height() as f32);
                // Small textures are shown enlarged: keep their pixels visible.
                let nearest = pixels.width().max(pixels.height()) < LARGE_VIEW_SIZE as u32;
                let (texture, handle) = egui_image(&pixels, nearest, &mut images, &mut textures);
                if let Some(old) = previews.view.replace(View { key, texture, handle, size }) {
                    free(&mut textures, &old.handle);
                }
            }
        }
    }
}

// --- the tab -----------------------------------------------------------------

/// A thumbnail of `size` points (keeps the aspect ratio), or a placeholder.
fn thumbnail(ui: &mut egui::Ui, previews: &mut TexturePreviews, path: &str, size: f32) {
    match previews.get(path) {
        PreviewState::Ready { thumbnail, preview } => {
            let (w, h) = (preview.pixels.width() as f32, preview.pixels.height() as f32);
            let scale = size / w.max(h);
            let image = egui::Image::new((thumbnail, egui::vec2(w * scale, h * scale)));
            let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
            ui.painter().rect_filled(rect, 2.0, egui::Color32::from_gray(24));
            let inner = egui::Rect::from_center_size(rect.center(), egui::vec2(w * scale, h * scale));
            image.paint_at(ui, inner);
        }
        PreviewState::Loading => {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
            ui.painter().rect_filled(rect, 2.0, egui::Color32::from_gray(40));
        }
        PreviewState::Failed(message) => {
            let message = message.to_string();
            let (rect, response) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
            ui.painter().rect_filled(rect, 2.0, egui::Color32::from_gray(40));
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "?",
                egui::FontId::proportional(size * 0.5),
                egui::Color32::GRAY,
            );
            response.on_hover_text(format!("No preview: {}", message));
        }
    }
}

/// A combo box for a setting whose `None` means "the purpose's default".
fn option_combo<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    id: egui::Id,
    value: &mut Option<T>,
    default: T,
    options: &[(T, &str, &str)],
) -> egui::Response {
    let name = |v: T| options.iter().find(|o| o.0 == v).map(|o| o.1).unwrap_or("?");
    let selected = match *value {
        None => format!("{} (default)", name(default)),
        Some(v) => name(v).to_string(),
    };
    egui::ComboBox::from_id_salt(id)
        .selected_text(selected)
        .show_ui(ui, |ui| {
            if ui
                .selectable_label(value.is_none(), format!("{} (default)", name(default)))
                .on_hover_text("What the purpose suggests")
                .clicked()
            {
                *value = None;
            }
            for (option, label, hint) in options {
                if ui.selectable_label(*value == Some(*option), *label).on_hover_text(*hint).clicked() {
                    *value = Some(*option);
                }
            }
        })
        .response
}

pub fn assets_tab(
    ui: &mut egui::Ui,
    project: &mut ProjectV2,
    session: &mut EditorSession,
    previews: &mut TexturePreviews,
    game_assets: &GameAssets,
) {
    ui.label("The textures of this project. Layers pick one by its id.");
    ui.horizontal_wrapped(|ui| {
        let busy = session.import_dialog.is_open();
        if ui
            .add_enabled(!busy, egui::Button::new("📥 IMPORT…"))
            .on_hover_text(hints::ASSET_IMPORT)
            .clicked()
        {
            session.actions.push(Action::OpenImportDialog);
        }
        ui.menu_button("➕ BUILT-IN", |ui| {
            let labels = PARTICLE_TEXTURES
                .iter()
                .map(|t| (t.filename, t.ui_label))
                .chain(std::iter::once((BUILTIN_NOISE_TEXTURE, "Noise")));
            for (filename, ui_label) in labels {
                let path = builtin_texture_path(filename);
                let present = project.assets.iter().any(|a| library::texture_path(a) == path);
                let button = egui::Button::new(format!("{} ({})", ui_label, filename));
                let response = ui.add_enabled(!present, button).on_hover_text(if present {
                    "Already in this project"
                } else {
                    "Built into Omagari: the game needs no file"
                });
                if response.clicked() {
                    session.actions.push(Action::AddBuiltinTexture(filename.to_string()));
                    ui.close();
                }
            }
        })
        .response
        .on_hover_text("Add one of the 7 textures built into Omagari");
        if busy {
            ui.label(RichText::new("file dialog open…").weak());
        }
    });
    ui.label(RichText::new("…or drop PNG / JPG files onto the window.").weak());
    ui.label(
        RichText::new(format!(
            "Game assets folder: {} (imports outside it are copied to {}/)",
            game_assets.assets_root().display(),
            IMPORT_DIR
        ))
        .weak()
        .small(),
    );
    ui.separator();

    if project.assets.is_empty() {
        ui.label(RichText::new("No assets yet: import a texture, add a built-in, or pick one on a layer.").weak());
    }
    if session.selected_asset.as_ref().is_some_and(|id| project.asset(id).is_none()) {
        session.selected_asset = None;
    }

    let rows: Vec<(String, String, TexturePurpose)> = project
        .assets
        .iter()
        .map(|a| {
            let AssetKind::Texture(t) = &a.kind;
            (a.id.clone(), t.path.clone(), t.purpose)
        })
        .collect();
    for (id, path, purpose) in rows.iter() {
        let selected = session.selected_asset.as_deref() == Some(id.as_str());
        let users = project.asset_users(id);
        let subject = format!("asset \"{}\"", id);
        let frame = egui::Frame::NONE
            .inner_margin(egui::Margin::same(3))
            .corner_radius(3.0)
            .fill(if selected { ui.visuals().selection.bg_fill.gamma_multiply(0.35) } else { egui::Color32::TRANSPARENT });
        let response = frame
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    thumbnail(ui, previews, path, 44.0);
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(id).strong());
                            let (errors, warnings) = session.diagnostics.count_about(&subject);
                            if let Some(b) = badge(ui, errors, warnings) {
                                b.on_hover_text(super::problems::subject_hover(session, &subject));
                            }
                        });
                        ui.label(RichText::new(path).small().weak());
                        let used = match users.len() {
                            0 => "not used".to_string(),
                            1 => format!("used by \"{}\"", users[0]),
                            n => format!("used by {} layers", n),
                        };
                        ui.label(RichText::new(format!("{} · {}", purpose.name(), used)).small());
                    });
                });
            })
            .response
            .interact(egui::Sense::click())
            .on_hover_text("Click to see and edit this texture");
        if response.clicked() {
            session.actions.push(Action::SelectAsset(id.clone()));
        }
    }

    if let Some(id) = session.selected_asset.clone() {
        ui.separator();
        asset_details(ui, project, session, previews, &id);
    }
}

fn asset_details(
    ui: &mut egui::Ui,
    project: &mut ProjectV2,
    session: &mut EditorSession,
    previews: &mut TexturePreviews,
    id: &str,
) {
    let users = project.asset_users(id);
    let user_layers: Vec<(String, usize)> = users
        .iter()
        .filter_map(|u| project.layers.iter().position(|l| l.name() == u).map(|i| (u.clone(), i)))
        .collect();
    let Some(asset) = project.assets.iter_mut().find(|a| a.id == id) else {
        return;
    };
    let AssetKind::Texture(texture) = &mut asset.kind;
    let path = texture.path.clone();

    ui.label(RichText::new(format!("🖼 {}", id)).color(ACCENT).strong());
    subject_box(ui, session, &format!("asset \"{}\"", id));

    // Large preview with the channel view.
    ui.horizontal(|ui| {
        ui.label("View").on_hover_text(hints::ASSET_VIEW);
        for (view, label) in VIEWS {
            if ui
                .selectable_label(session.asset_view == view, label)
                .on_hover_text(match view {
                    None => "The colour (transparency ignored)".to_string(),
                    Some(c) => format!("The {} channel as gray", c.name()),
                })
                .clicked()
            {
                session.asset_view = view;
            }
        }
    });
    let info = match previews.get(&path) {
        PreviewState::Ready { preview, .. } => Some((
            preview.info.describe(),
            preview.uses_alpha,
            preview.grayscale,
        )),
        PreviewState::Failed(message) => {
            ui.label(RichText::new(format!("No preview: {}", message)).weak());
            None
        }
        PreviewState::Loading => None,
    };
    match previews.view(&path, session.asset_view) {
        Some((texture_id, size)) => {
            let side = ui.available_width().min(LARGE_VIEW_SIZE);
            let scale = side / size.x.max(size.y);
            ui.add(egui::Image::new((texture_id, size * scale)));
        }
        None if info.is_some() || matches!(previews.get(&path), PreviewState::Loading) => {
            ui.spinner();
        }
        None => {}
    }
    if let Some((describe, uses_alpha, grayscale)) = info {
        let mut notes = vec![describe];
        if grayscale {
            notes.push("R = G = B (grayscale)".into());
        }
        notes.push(if uses_alpha { "uses transparency".into() } else { "fully opaque".into() });
        ui.label(RichText::new(notes.join(" · ")).small());
    }
    ui.add_space(4.0);

    let grid_id = ui.make_persistent_id(("asset settings", id));
    let mut rename: Option<String> = None;
    egui::Grid::new(grid_id).num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
        ui.label("Id").on_hover_text(hints::ASSET_ID);
        ui.horizontal(|ui| {
            let draft = match &mut session.rename_draft {
                Some((asset, text)) if asset == id => text,
                slot => {
                    *slot = Some((id.to_string(), id.to_string()));
                    &mut slot.as_mut().unwrap().1
                }
            };
            let response = ui
                .add(egui::TextEdit::singleline(draft).desired_width(140.0))
                .on_hover_text(hints::ASSET_ID);
            let enter = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let changed = draft.trim() != id && !draft.trim().is_empty();
            if (ui.add_enabled(changed, egui::Button::new("RENAME")).on_hover_text(hints::ASSET_ID).clicked() || enter)
                && changed
            {
                rename = Some(draft.trim().to_string());
            }
        });
        ui.end_row();

        ui.label("Path").on_hover_text(hints::ASSET_PATH);
        ui.add(egui::Label::new(RichText::new(&path).small()).wrap()).on_hover_text(hints::ASSET_PATH);
        ui.end_row();

        ui.label("Purpose").on_hover_text(hints::ASSET_PURPOSE);
        egui::ComboBox::from_id_salt(grid_id.with("purpose"))
            .selected_text(texture.purpose.name())
            .show_ui(ui, |ui| {
                for purpose in TexturePurpose::ALL {
                    ui.selectable_value(&mut texture.purpose, purpose, purpose.name())
                        .on_hover_text(hints::purpose_hint(purpose));
                }
            })
            .response
            .on_hover_text(hints::purpose_hint(texture.purpose));
        ui.end_row();

        let defaults = omagari::format::TextureAssetDef { path: String::new(), purpose: texture.purpose, ..default() };
        ui.label("Wrap").on_hover_text(hints::ASSET_WRAP);
        option_combo(
            ui,
            grid_id.with("wrap"),
            &mut texture.wrap,
            defaults.wrap(),
            &[
                (TextureWrap::Repeat, "Repeat", "Tiles: what leaves one edge comes back at the other"),
                (TextureWrap::Clamp, "Clamp", "The edge pixels continue outwards"),
            ],
        )
        .on_hover_text(hints::ASSET_WRAP);
        ui.end_row();

        ui.label("Colour space").on_hover_text(hints::ASSET_COLOR_SPACE);
        option_combo(
            ui,
            grid_id.with("color space"),
            &mut texture.color_space,
            defaults.color_space(),
            &[
                (TextureColorSpace::Srgb, "Colour (sRGB)", "Pixels are colours as painted"),
                (TextureColorSpace::Linear, "Data (linear)", "Pixels are numbers, used unchanged"),
            ],
        )
        .on_hover_text(hints::ASSET_COLOR_SPACE);
        ui.end_row();

        ui.label("Filter").on_hover_text(hints::ASSET_FILTER);
        option_combo(
            ui,
            grid_id.with("filter"),
            &mut texture.filter,
            defaults.filter(),
            &[
                (TextureFilter::Linear, "Smooth", "Blends neighbouring pixels"),
                (TextureFilter::Nearest, "Pixelated", "Hard pixel edges"),
            ],
        )
        .on_hover_text(hints::ASSET_FILTER);
        ui.end_row();

        ui.label("Sprite sheet").on_hover_text(hints::ASSET_GRID);
        ui.horizontal(|ui| {
            let mut on = texture.grid.is_some();
            if ui.checkbox(&mut on, "").on_hover_text(hints::ASSET_GRID).changed() {
                texture.grid = on.then_some((2, 2));
            }
            if let Some((rows, cols)) = &mut texture.grid {
                ui.add(egui::DragValue::new(rows).range(1..=256)).on_hover_text("Rows of frames");
                ui.label("rows ×");
                ui.add(egui::DragValue::new(cols).range(1..=256)).on_hover_text("Columns of frames");
                ui.label("columns");
            } else {
                ui.label(RichText::new("single image").weak());
            }
        });
        ui.end_row();

        ui.label("Used by").on_hover_text("The layers that draw this texture");
        ui.horizontal_wrapped(|ui| {
            if users.is_empty() {
                ui.label(RichText::new("no layer").weak());
            }
            for (user, index) in user_layers.iter() {
                if ui.small_button(user).on_hover_text("Open this layer").clicked() {
                    session.actions.push(Action::SelectLayer(*index));
                }
            }
        });
        ui.end_row();
    });

    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        let decodable = library::is_builtin(&path) || library::DECODABLE_EXTENSIONS.contains(&library::extension(&path).as_str());
        ui.add_enabled_ui(decodable, |ui| {
            ui.menu_button("🎨 EXTRACT CHANNEL", |ui| {
                for channel in ChannelDef::ALL {
                    if ui
                        .button(format!("{} → {}", channel.name(), library::extracted_path(&path, channel)))
                        .clicked()
                    {
                        session.actions.push(Action::ExtractChannel { asset: id.to_string(), channel });
                        ui.close();
                    }
                }
            })
            .response
            .on_hover_text(hints::ASSET_EXTRACT)
            .on_disabled_hover_text("Channels can be extracted from PNG and JPG files");
        });
        let response = ui
            .add_enabled(users.is_empty(), egui::Button::new("🗑 DELETE"))
            .on_hover_text("Remove this texture from the project (the file stays on disk)");
        let response = if users.is_empty() {
            response
        } else {
            response.on_disabled_hover_text(format!(
                "Used by {}; pick another texture on those layers first",
                users.iter().map(|u| format!("\"{}\"", u)).collect::<Vec<_>>().join(", ")
            ))
        };
        if response.clicked() {
            session.actions.push(Action::DeleteAsset(id.to_string()));
        }
    });

    if let Some(to) = rename {
        session.actions.push(Action::RenameAsset { from: id.to_string(), to });
    }
}

/// Shows the ASSETS tab with an asset selected.
pub fn select_asset(session: &mut EditorSession, id: &str) {
    session.selected_asset = Some(id.to_string());
    session.rename_draft = None;
    session.tab = RightTab::Assets;
}

/// The "drop to import" hint over the window while files hover over it.
pub fn drop_overlay(ctx: &egui::Context, session: &EditorSession) {
    if !session.drop_hover {
        return;
    }
    let rect = ctx.content_rect();
    egui::Area::new(egui::Id::new("drop overlay"))
        .order(egui::Order::Foreground)
        .fixed_pos(rect.min)
        .interactable(false)
        .show(ctx, |ui| {
            ui.painter().rect_filled(rect, 0.0, egui::Color32::from_black_alpha(150));
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "Drop to import into the project's assets",
                egui::FontId::proportional(28.0),
                egui::Color32::WHITE,
            );
        });
}
