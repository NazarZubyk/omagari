//! The file side of the asset library, shared by the editor and the MCP
//! server: importing textures and glTF models (mesh assets) into the game
//! assets folder, guessing their
//! purpose, extracting a channel to a grayscale PNG, and reading texture
//! files (game folder or built-in) and their headers.
//!
//! Import rules: a file inside the game assets folder is referenced in place
//! (path relative to the folder); a file outside is copied into
//! `<game assets>/vfx/textures/` (models: `vfx/models/`, `.glb` only). A
//! name already taken by a different file gets a numeric suffix
//! (`smoke_2.png`); an identical file is reused.

use std::fs;
use std::io::Cursor;
use std::path::{Component, Path, PathBuf};

use image::{DynamicImage, GrayImage, ImageDecoder, ImageFormat, ImageReader, Luma, RgbaImage};
use omagari::format::{
    AssetDef, AssetKind, ChannelDef, MeshAssetDef, ProjectV2, TextureAssetDef, TexturePurpose, BUILTIN_TEXTURES_PREFIX,
};
use omagari::runtime::BUILTIN_TEXTURES;

/// Folder of the game assets folder imported textures are copied to.
pub const IMPORT_DIR: &str = "vfx/textures";
/// Folder of the game assets folder imported models (mesh assets) are
/// copied to.
pub const MODEL_IMPORT_DIR: &str = "vfx/models";

/// Texture file types the editor imports. PNG loads in every game; JPG needs
/// Bevy's `jpeg` feature in the game; KTX2 is a Bevy default (3D) but has no
/// preview or channel extraction here.
pub const TEXTURE_EXTENSIONS: [&str; 4] = ["png", "jpg", "jpeg", "ktx2"];

/// File types this module can decode (previews, channel extraction).
pub const DECODABLE_EXTENSIONS: [&str; 3] = ["png", "jpg", "jpeg"];

/// Lower-case extension of a path or file name.
pub fn extension(path: &str) -> String {
    Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
}

/// Whether the editor imports files of this type.
pub fn is_texture_file(path: &Path) -> bool {
    TEXTURE_EXTENSIONS.contains(&extension(&path.to_string_lossy()).as_str())
}

/// The purpose a texture probably has, from words in its file name: noise /
/// perlin / voronoi / cellular → Noise, mask → Mask, gradient / ramp →
/// Gradient, anything else → Sprite.
pub fn guess_purpose(file_name: &str) -> TexturePurpose {
    let name = Path::new(file_name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(file_name)
        .to_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| name.contains(w));
    if has(&["noise", "perlin", "voronoi", "cellular", "worley", "simplex"]) {
        TexturePurpose::Noise
    } else if has(&["mask"]) {
        TexturePurpose::Mask
    } else if has(&["gradient", "ramp"]) {
        TexturePurpose::Gradient
    } else {
        TexturePurpose::Sprite
    }
}

/// How [`import_file`] placed a file.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ImportKind {
    /// Copied into `<game assets>/vfx/textures/`.
    Copied,
    /// An identical file was already there; nothing was written.
    Reused,
    /// The file is inside the game assets folder and is used where it is.
    InPlace,
}

impl ImportKind {
    pub fn describe(self) -> &'static str {
        match self {
            ImportKind::Copied => "copied into the game assets folder",
            ImportKind::Reused => "an identical file was already in the game assets folder",
            ImportKind::InPlace => "already in the game assets folder, used in place",
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Imported {
    /// Path relative to the game assets folder, with `/`
    /// (`vfx/textures/smoke.png`): what the asset entry stores.
    pub path: String,
    pub kind: ImportKind,
}

/// Path of `file` relative to `root` with `/` separators, when `file` is
/// inside `root` (both are canonicalized, so they must exist).
pub fn relative_game_path(file: &Path, root: &Path) -> Option<String> {
    let file = fs::canonicalize(file).ok()?;
    let root = fs::canonicalize(root).ok()?;
    let relative = file.strip_prefix(&root).ok()?;
    let parts: Vec<String> = relative
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .collect();
    (!parts.is_empty()).then(|| parts.join("/"))
}

/// A file name that is safe in a Bevy asset path (`#` starts a label).
fn clean_file_name(name: &str) -> String {
    name.chars().map(|c| if c == '#' { '_' } else { c }).collect()
}

/// Imports a texture or glTF model file into the game assets folder
/// `game_root`, see the module docs. Models outside the folder are copied to
/// `vfx/models/`; a `.gltf` (JSON with separate buffer and texture files)
/// outside it is refused, since its other files would not come along.
pub fn import_file(source: &Path, game_root: &Path) -> Result<Imported, String> {
    if !source.is_file() {
        return Err(format!("{} is not a file", source.display()));
    }
    let model = crate::model_info::is_model_file(source);
    if !is_texture_file(source) && !model {
        return Err(format!(
            "{}: not a texture or model Omagari imports (use {}, glb, gltf)",
            source.display(),
            TEXTURE_EXTENSIONS.join(", ")
        ));
    }
    if !game_root.is_dir() {
        return Err(format!("the game assets folder {} does not exist", game_root.display()));
    }
    if let Some(path) = relative_game_path(source, game_root) {
        return Ok(Imported { path, kind: ImportKind::InPlace });
    }
    if model && extension(&source.to_string_lossy()) == "gltf" {
        return Err(format!(
            "{}: a .gltf file uses separate buffer and texture files; put it into the game assets folder ({}) or export a .glb",
            source.display(),
            game_root.display()
        ));
    }

    let bytes = fs::read(source).map_err(|e| format!("{}: {}", source.display(), e))?;
    let import_dir = if model { MODEL_IMPORT_DIR } else { IMPORT_DIR };
    let dir = game_root.join(import_dir);
    fs::create_dir_all(&dir).map_err(|e| format!("{}: {}", dir.display(), e))?;
    let name = clean_file_name(&source.file_name().unwrap_or_default().to_string_lossy());
    let stem = Path::new(&name).file_stem().unwrap_or_default().to_string_lossy().to_string();
    let ext = extension(&name);
    for n in 1.. {
        let file = if n == 1 { format!("{}.{}", stem, ext) } else { format!("{}_{}.{}", stem, n, ext) };
        let target = dir.join(&file);
        let path = format!("{}/{}", import_dir, file);
        if !target.exists() {
            fs::write(&target, &bytes).map_err(|e| format!("{}: {}", target.display(), e))?;
            return Ok(Imported { path, kind: ImportKind::Copied });
        }
        if fs::read(&target).ok().as_deref() == Some(bytes.as_slice()) {
            return Ok(Imported { path, kind: ImportKind::Reused });
        }
    }
    unreachable!()
}

/// Adds a texture asset for `path` to the project, or finds the asset that
/// already has this path. `id` = the wanted id (must be free), else one is
/// made from the file name. Returns the id and whether an asset was added.
pub fn add_texture_asset(
    project: &mut ProjectV2,
    path: &str,
    purpose: TexturePurpose,
    id: Option<&str>,
) -> Result<(String, bool), String> {
    if let Some(existing) = project.assets.iter().find(|a| a.kind.as_texture().is_some_and(|t| t.path == path)) {
        return Ok((existing.id.clone(), false));
    }
    let id = new_asset_id(project, path, id)?;
    project.assets.push(AssetDef {
        id: id.clone(),
        kind: AssetKind::Texture(TextureAssetDef::new(path, purpose)),
    });
    Ok((id, true))
}

/// The id for a new asset: `wanted` (must be free), else one made from the
/// file name.
fn new_asset_id(project: &ProjectV2, path: &str, wanted: Option<&str>) -> Result<String, String> {
    match wanted.map(str::trim).filter(|i| !i.is_empty()) {
        Some(id) if project.asset(id).is_some() => Err(format!("an asset \"{}\" already exists", id)),
        Some(id) => Ok(id.to_string()),
        None => {
            let stem = Path::new(path).file_stem().unwrap_or_default().to_string_lossy().to_string();
            Ok(project.unique_asset_id(&stem))
        }
    }
}

/// Adds a mesh asset for one primitive of the glTF file `path` (relative to
/// the game assets folder), or finds the asset that already has it. Without
/// `mesh`, the file's first mesh primitive suitable for mesh particles is
/// taken (`info` = the file's contents; the id comes from the mesh's name).
/// Returns the id and whether an asset was added.
pub fn add_mesh_asset(
    project: &mut ProjectV2,
    path: &str,
    info: &crate::model_info::ModelInfo,
    mesh: Option<(u32, u32)>,
    id: Option<&str>,
) -> Result<(String, bool), String> {
    let (mi, pi) = match mesh {
        Some((m, p)) => (m, p),
        None => {
            let best = info
                .primitives()
                .find(|(_, _, _, p)| p.triangles && p.has_uvs && !p.skinned)
                .or_else(|| info.primitives().next())
                .ok_or_else(|| format!("{} has no meshes", path))?;
            (best.0 as u32, best.1 as u32)
        }
    };
    let (model_mesh, _) = info.primitive(mi as usize, pi as usize).ok_or_else(|| {
        let list: Vec<String> = info.primitives().map(|(m, p, mesh, _)| format!("Mesh{}/Primitive{} \"{}\"", m, p, mesh.name)).collect();
        format!("{} has no Mesh{}/Primitive{} (it has {})", path, mi, pi, if list.is_empty() { "no meshes".into() } else { list.join(", ") })
    })?;
    let def = MeshAssetDef::new(path, mi, pi, Some(model_mesh.name.clone()));
    if let Some(existing) =
        project.assets.iter().find(|a| a.kind.as_mesh().is_some_and(|m| (&m.path, m.mesh, m.primitive) == (&def.path, mi, pi)))
    {
        return Ok((existing.id.clone(), false));
    }
    let base = if id.is_none() { Some(model_mesh.name.clone()) } else { None };
    let id = new_asset_id(project, base.as_deref().unwrap_or(path), id)?;
    project.assets.push(AssetDef { id: id.clone(), kind: AssetKind::Mesh(def) });
    Ok((id, true))
}

/// The file of an asset (texture or mesh model).
pub fn texture_path(asset: &AssetDef) -> &str {
    asset.kind.path()
}

/// Whether an asset path is a built-in texture (`omagari://textures/...`).
pub fn is_builtin(path: &str) -> bool {
    path.starts_with(BUILTIN_TEXTURES_PREFIX)
}

/// What is wrong with a texture path as a game asset path, if anything:
/// `Err` = the game cannot load it at all, `Ok(Some)` = it may not.
pub fn path_problem(path: &str) -> Result<Option<String>, String> {
    if is_builtin(path) || path.is_empty() {
        return Ok(None);
    }
    let p = Path::new(path);
    if p.is_absolute() || p.has_root() || path.contains(':') {
        return Err(format!(
            "{} is not relative to the game assets folder; import the file so it is copied there",
            path
        ));
    }
    if p.components().any(|c| c == Component::ParentDir) {
        return Ok(Some(format!(
            "{} points outside the game assets folder (`..`); the game may not load it",
            path
        )));
    }
    if path.contains('\\') {
        return Ok(Some(format!("{} uses `\\`; asset paths use `/`", path)));
    }
    Ok(None)
}

/// The file of a game texture path.
pub fn texture_file(path: &str, game_root: &Path) -> PathBuf {
    game_root.join(path)
}

/// The bytes of a texture: a built-in, or a file in the game assets folder.
pub fn texture_bytes(path: &str, game_root: &Path) -> Result<Vec<u8>, String> {
    if let Some(file) = path.strip_prefix(BUILTIN_TEXTURES_PREFIX) {
        return BUILTIN_TEXTURES
            .iter()
            .find(|(f, _)| *f == file)
            .map(|(_, bytes)| bytes.to_vec())
            .ok_or_else(|| format!("{} is not a built-in texture", path));
    }
    let file = texture_file(path, game_root);
    fs::read(&file).map_err(|e| format!("{}: {}", file.display(), e))
}

/// Size and pixel layout of a texture file, read from its header.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TextureInfo {
    pub width: u32,
    pub height: u32,
    /// `PNG`, `JPG`, ...
    pub format: &'static str,
    /// 1 (gray), 2 (gray + alpha), 3 (RGB) or 4 (RGBA).
    pub channels: u8,
    /// Bits per channel.
    pub bits: u8,
}

impl TextureInfo {
    /// `333×333 PNG, RGBA 8-bit`.
    pub fn describe(&self) -> String {
        let layout = match self.channels {
            1 => "grayscale",
            2 => "grayscale + alpha",
            3 => "RGB",
            _ => "RGBA",
        };
        format!("{}×{} {}, {} {}-bit", self.width, self.height, self.format, layout, self.bits)
    }

    /// Bevy loads 16-bit grayscale PNGs as integer textures (`R16Uint`,
    /// `Rg16Uint`), which particles and materials cannot sample.
    pub fn loads_as_integer_texture(&self) -> bool {
        self.bits == 16 && self.channels <= 2
    }
}

fn format_name(format: Option<ImageFormat>) -> &'static str {
    match format {
        Some(ImageFormat::Png) => "PNG",
        Some(ImageFormat::Jpeg) => "JPG",
        _ => "image",
    }
}

fn info_from_reader<R: std::io::BufRead + std::io::Seek>(reader: ImageReader<R>) -> Result<TextureInfo, String> {
    let reader = reader.with_guessed_format().map_err(|e| e.to_string())?;
    let format = format_name(reader.format());
    let decoder = reader.into_decoder().map_err(|e| e.to_string())?;
    let (width, height) = decoder.dimensions();
    let color = decoder.color_type();
    let channels = color.channel_count();
    Ok(TextureInfo {
        width,
        height,
        format,
        channels,
        bits: (color.bits_per_pixel() / channels as u16) as u8,
    })
}

/// Header of a texture in memory.
pub fn texture_info(bytes: &[u8]) -> Result<TextureInfo, String> {
    info_from_reader(ImageReader::new(Cursor::new(bytes)))
}

/// Header of a texture (built-in or game file), reading only the start of a
/// file. `Err` for files that cannot be decoded here (KTX2, broken files).
pub fn texture_info_at(path: &str, game_root: &Path) -> Result<TextureInfo, String> {
    if is_builtin(path) {
        return texture_info(&texture_bytes(path, game_root)?);
    }
    let file = texture_file(path, game_root);
    let reader = ImageReader::open(&file).map_err(|e| format!("{}: {}", file.display(), e))?;
    info_from_reader(reader)
}

pub fn decode(bytes: &[u8]) -> Result<DynamicImage, String> {
    image::load_from_memory(bytes).map_err(|e| e.to_string())
}

/// One channel of a colour as a byte (luminance: Rec. 709 weights on the
/// stored values).
pub fn channel_value(pixel: [u8; 4], channel: ChannelDef) -> u8 {
    match channel {
        ChannelDef::R => pixel[0],
        ChannelDef::G => pixel[1],
        ChannelDef::B => pixel[2],
        ChannelDef::A => pixel[3],
        ChannelDef::Luminance => {
            let l = 0.2126 * pixel[0] as f32 + 0.7152 * pixel[1] as f32 + 0.0722 * pixel[2] as f32;
            l.round().clamp(0.0, 255.0) as u8
        }
    }
}

/// One channel of an image as a grayscale image.
pub fn channel_image(image: &RgbaImage, channel: ChannelDef) -> GrayImage {
    GrayImage::from_fn(image.width(), image.height(), |x, y| {
        Luma([channel_value(image.get_pixel(x, y).0, channel)])
    })
}

/// Where the extracted channel of a texture goes: next to it
/// (`vfx/textures/pack_g.png`), or into `vfx/textures/` for built-ins.
pub fn extracted_path(path: &str, channel: ChannelDef) -> String {
    let p = Path::new(path.strip_prefix(BUILTIN_TEXTURES_PREFIX).unwrap_or(path));
    let stem = p.file_stem().unwrap_or_default().to_string_lossy();
    let file = format!("{}_{}.png", stem, channel.suffix());
    let parent = if is_builtin(path) {
        IMPORT_DIR.to_string()
    } else {
        p.parent().map(|d| d.to_string_lossy().replace('\\', "/")).unwrap_or_default()
    };
    if parent.is_empty() { file } else { format!("{}/{}", parent, file) }
}

/// Writes one channel of a texture as an 8-bit grayscale PNG (see
/// [`extracted_path`]; an older extraction is overwritten). Returns the new
/// file's path relative to the game assets folder.
pub fn extract_channel(path: &str, channel: ChannelDef, game_root: &Path) -> Result<String, String> {
    if !is_builtin(path) && !DECODABLE_EXTENSIONS.contains(&extension(path).as_str()) {
        return Err(format!("{}: channels can be extracted from PNG and JPG files only", path));
    }
    let image = decode(&texture_bytes(path, game_root)?)?;
    let gray = channel_image(&image.to_rgba8(), channel);
    let out = extracted_path(path, channel);
    let file = texture_file(&out, game_root);
    if let Some(dir) = file.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("{}: {}", dir.display(), e))?;
    }
    gray.save_with_format(&file, ImageFormat::Png)
        .map_err(|e| format!("{}: {}", file.display(), e))?;
    Ok(out)
}

/// Extracts a channel of the project's asset `id` and adds the result as a
/// new asset with the same settings (an asset with that path is reused).
/// Returns the new asset id and path.
pub fn extract_channel_asset(
    project: &mut ProjectV2,
    id: &str,
    channel: ChannelDef,
    game_root: &Path,
) -> Result<(String, String), String> {
    let texture = project.texture(id).ok_or_else(|| format!("no texture asset \"{}\"", id))?.clone();
    let path = extract_channel(&texture.path, channel, game_root)?;
    if let Some(existing) = project.assets.iter().find(|a| texture_path(a) == path) {
        return Ok((existing.id.clone(), path));
    }
    let new_id = project.unique_asset_id(&format!("{}_{}", id, channel.suffix()));
    project.assets.push(AssetDef {
        id: new_id.clone(),
        kind: AssetKind::Texture(TextureAssetDef { path: path.clone(), ..texture }),
    });
    Ok((new_id, path))
}

/// Texture files in the game assets folder (relative paths with `/`, file
/// size in bytes), sorted; hidden folders are skipped and at most `limit`
/// files are listed.
pub fn scan_textures(game_root: &Path, limit: usize) -> Vec<(String, u64)> {
    scan_files(game_root, limit, is_texture_file)
}

/// Files under the game assets folder that `keep` accepts: relative paths
/// with `/` and sizes in bytes, sorted, hidden entries skipped, at most
/// `limit`.
pub fn scan_files(game_root: &Path, limit: usize, keep: fn(&Path) -> bool) -> Vec<(String, u64)> {
    fn walk(dir: &Path, root: &Path, depth: usize, limit: usize, keep: fn(&Path) -> bool, out: &mut Vec<(String, u64)>) {
        let Ok(entries) = fs::read_dir(dir) else { return };
        let mut entries: Vec<_> = entries.filter_map(|e| e.ok()).collect();
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            if out.len() >= limit {
                return;
            }
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            if path.is_dir() {
                if depth < 8 {
                    walk(&path, root, depth + 1, limit, keep, out);
                }
            } else if keep(&path) {
                if let Ok(relative) = path.strip_prefix(root) {
                    let relative = relative.to_string_lossy().replace('\\', "/");
                    out.push((relative, entry.metadata().map(|m| m.len()).unwrap_or(0)));
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(game_root, game_root, 0, limit, keep, &mut out);
    out
}

/// A texture decoded for the editor's previews.
pub struct DecodedPreview {
    pub info: TextureInfo,
    /// The texture scaled down to fit `max`×`max` (RGBA, 8 bits).
    pub pixels: RgbaImage,
    /// Some pixel is not fully opaque.
    pub uses_alpha: bool,
    /// R = G = B everywhere (the colour channels carry the same picture).
    pub grayscale: bool,
}

pub fn decode_preview(bytes: &[u8], max: u32) -> Result<DecodedPreview, String> {
    let info = texture_info(bytes)?;
    let image = decode(bytes)?;
    let rgba = image.to_rgba8();
    let pixels = if rgba.width() > max || rgba.height() > max {
        let scale = max as f32 / rgba.width().max(rgba.height()) as f32;
        let w = ((rgba.width() as f32 * scale).round() as u32).max(1);
        let h = ((rgba.height() as f32 * scale).round() as u32).max(1);
        image::imageops::thumbnail(&rgba, w, h)
    } else {
        rgba
    };
    let uses_alpha = pixels.pixels().any(|p| p.0[3] < 255);
    let grayscale = pixels.pixels().all(|p| p.0[0] == p.0[1] && p.0[1] == p.0[2]);
    Ok(DecodedPreview { info, pixels, uses_alpha, grayscale })
}

/// The preview pixels of one view: `None` = RGB (opaque), else one channel
/// as gray.
pub fn view_pixels(pixels: &RgbaImage, channel: Option<ChannelDef>) -> RgbaImage {
    RgbaImage::from_fn(pixels.width(), pixels.height(), |x, y| {
        let p = pixels.get_pixel(x, y).0;
        match channel {
            None => image::Rgba([p[0], p[1], p[2], 255]),
            Some(c) => {
                let v = channel_value(p, c);
                image::Rgba([v, v, v, 255])
            }
        }
    })
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use image::Rgba;

    /// A fresh temporary folder for one test.
    pub fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("omagari_{}_{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Writes a 4×2 RGBA PNG whose channels differ: R = 10·x, G = 200,
    /// B = 50·y, A = 255 − 20·x.
    pub fn write_test_png(path: &Path) {
        let image = RgbaImage::from_fn(4, 2, |x, y| Rgba([10 * x as u8, 200, 50 * y as u8, 255 - 20 * x as u8]));
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).unwrap();
        }
        image.save_with_format(path, ImageFormat::Png).unwrap();
    }

    #[test]
    fn purposes_are_guessed_from_file_names() {
        assert_eq!(guess_purpose("T_Perlin_Noise.png"), TexturePurpose::Noise);
        assert_eq!(guess_purpose("voronoi3.jpg"), TexturePurpose::Noise);
        assert_eq!(guess_purpose("Cellular.png"), TexturePurpose::Noise);
        assert_eq!(guess_purpose("dissolve_mask.png"), TexturePurpose::Mask);
        assert_eq!(guess_purpose("fire_gradient.png"), TexturePurpose::Gradient);
        assert_eq!(guess_purpose("ColorRamp.png"), TexturePurpose::Gradient);
        assert_eq!(guess_purpose("smoke.png"), TexturePurpose::Sprite);
        assert_eq!(guess_purpose("C:/noise/smoke.png"), TexturePurpose::Sprite, "only the file name counts");
    }

    #[test]
    fn import_copies_renames_and_reuses() {
        let dir = temp_dir("import");
        let game = dir.join("game");
        fs::create_dir_all(&game).unwrap();
        let outside = dir.join("downloads");
        write_test_png(&outside.join("smoke.png"));

        let first = import_file(&outside.join("smoke.png"), &game).unwrap();
        assert_eq!(first, Imported { path: "vfx/textures/smoke.png".into(), kind: ImportKind::Copied });
        assert!(game.join("vfx/textures/smoke.png").is_file());

        // The same file again: reused, nothing new written.
        let again = import_file(&outside.join("smoke.png"), &game).unwrap();
        assert_eq!(again, Imported { path: "vfx/textures/smoke.png".into(), kind: ImportKind::Reused });

        // Another file with the same name: numeric suffix.
        let other = dir.join("other");
        fs::create_dir_all(&other).unwrap();
        RgbaImage::from_pixel(2, 2, Rgba([1, 2, 3, 4])).save(other.join("smoke.png")).unwrap();
        let renamed = import_file(&other.join("smoke.png"), &game).unwrap();
        assert_eq!(renamed, Imported { path: "vfx/textures/smoke_2.png".into(), kind: ImportKind::Copied });
        assert_eq!(import_file(&other.join("smoke.png"), &game).unwrap().kind, ImportKind::Reused);

        // Inside the game folder: referenced in place.
        write_test_png(&game.join("fx").join("Glow #1.png"));
        let inside = import_file(&game.join("fx").join("Glow #1.png"), &game).unwrap();
        assert_eq!(inside, Imported { path: "fx/Glow #1.png".into(), kind: ImportKind::InPlace });
        // Copies get a name without `#` (asset path labels).
        let game2 = dir.join("game2");
        fs::create_dir_all(&game2).unwrap();
        let copied = import_file(&game.join("fx").join("Glow #1.png"), &game2).unwrap();
        assert_eq!(copied.path, "vfx/textures/Glow _1.png");

        assert!(import_file(&dir.join("nope.png"), &game).is_err());
        fs::write(dir.join("notes.txt"), "x").unwrap();
        assert!(import_file(&dir.join("notes.txt"), &game).unwrap_err().contains("not a texture"));
        let _ = fs::remove_dir_all(&dir);
    }

    /// Models: a `.glb` outside the game folder is copied to `vfx/models/`,
    /// a `.gltf` outside it is refused (its buffers would not come along),
    /// and its meshes become mesh assets by glTF label.
    #[test]
    fn models_import_as_mesh_assets() {
        use crate::model_info::tests::glb::write_rock_glb;
        use crate::model_info::ModelInfo;
        let dir = temp_dir("import_model");
        let game = dir.join("game");
        fs::create_dir_all(&game).unwrap();
        write_rock_glb(&dir.join("art/debris.glb"));
        let imported = import_file(&dir.join("art/debris.glb"), &game).unwrap();
        assert_eq!(imported, Imported { path: "vfx/models/debris.glb".into(), kind: ImportKind::Copied });
        fs::write(dir.join("art/scene.gltf"), "{}").unwrap();
        assert!(import_file(&dir.join("art/scene.gltf"), &game).unwrap_err().contains("export a .glb"));
        fs::write(game.join("in_place.gltf"), "{}").unwrap();
        assert_eq!(import_file(&game.join("in_place.gltf"), &game).unwrap().kind, ImportKind::InPlace);

        let info = ModelInfo::load(&game.join(&imported.path)).unwrap();
        let mut project = ProjectV2::default();
        // Default: the first mesh particles can draw (Rock has UVs).
        let (id, added) = add_mesh_asset(&mut project, &imported.path, &info, None, None).unwrap();
        assert_eq!((id.as_str(), added), ("Rock", true));
        let rock = project.mesh("Rock").unwrap();
        assert_eq!(rock.asset_path(), "vfx/models/debris.glb#Mesh0/Primitive0");
        assert_eq!(rock.name.as_deref(), Some("Rock"));
        // The same mesh again: the same asset; another mesh: a new one.
        assert_eq!(add_mesh_asset(&mut project, &imported.path, &info, Some((0, 0)), None).unwrap(), ("Rock".into(), false));
        let (bare, _) = add_mesh_asset(&mut project, &imported.path, &info, Some((1, 0)), Some("plain")).unwrap();
        assert_eq!(project.mesh(&bare).unwrap().label(), "Mesh1/Primitive0");
        let err = add_mesh_asset(&mut project, &imported.path, &info, Some((5, 0)), None).unwrap_err();
        assert!(err.contains("has no Mesh5/Primitive0") && err.contains("Mesh0/Primitive0 \"Rock\""), "{}", err);

        // Users, rename and delete follow mesh particles.
        project.layers.push(omagari::format::LayerDef::Particles(omagari::format::ParticlesLayerDef {
            name: "debris".into(),
            mesh: Some(omagari::format::ParticleMeshDef::Asset("Rock".into())),
            ..Default::default()
        }));
        assert_eq!(project.asset_users("Rock"), vec!["debris".to_string()]);
        assert!(project.delete_asset("Rock").is_err());
        project.rename_asset("Rock", "stone").unwrap();
        assert_eq!(project.layers[0].as_particles().unwrap().mesh_asset(), Some("stone"));
        assert!(project.delete_asset("plain").is_ok());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn imported_files_become_assets_once() {
        let mut project = ProjectV2::default();
        let (id, added) = add_texture_asset(&mut project, "vfx/textures/noise_a.png", TexturePurpose::Noise, None).unwrap();
        assert_eq!((id.as_str(), added), ("noise_a", true));
        let (id, added) = add_texture_asset(&mut project, "vfx/textures/noise_a.png", TexturePurpose::Sprite, None).unwrap();
        assert_eq!((id.as_str(), added), ("noise_a", false), "same path: same asset");
        assert_eq!(project.assets.len(), 1);
        let (id, _) = add_texture_asset(&mut project, "other/noise_a.png", TexturePurpose::Noise, None).unwrap();
        assert_eq!(id, "noise_a2");
        assert!(add_texture_asset(&mut project, "x.png", TexturePurpose::Noise, Some("noise_a")).is_err());
        let (id, _) = add_texture_asset(&mut project, "x.png", TexturePurpose::Noise, Some("mine")).unwrap();
        assert_eq!(id, "mine");
        assert_eq!(project.texture("noise_a").unwrap().purpose, TexturePurpose::Noise);
    }

    #[test]
    fn extracted_channels_are_grayscale_pngs_next_to_the_source() {
        let game = temp_dir("extract");
        write_test_png(&game.join("vfx/textures/pack.png"));
        let mut project = ProjectV2::default();
        project.assets.push(AssetDef {
            id: "pack".into(),
            kind: AssetKind::Texture(TextureAssetDef {
                grid: Some((1, 2)),
                ..TextureAssetDef::new("vfx/textures/pack.png", TexturePurpose::Mask)
            }),
        });

        let (id, path) = extract_channel_asset(&mut project, "pack", ChannelDef::B, &game).unwrap();
        assert_eq!((id.as_str(), path.as_str()), ("pack_b", "vfx/textures/pack_b.png"));
        let copy = project.texture("pack_b").unwrap();
        assert_eq!(copy.purpose, TexturePurpose::Mask, "purpose copied");
        assert_eq!(copy.grid, Some((1, 2)));
        let written = image::open(game.join(&path)).unwrap();
        assert!(matches!(written, DynamicImage::ImageLuma8(_)), "8-bit grayscale");
        let gray = written.to_luma8();
        assert_eq!((gray.width(), gray.height()), (4, 2));
        assert_eq!(gray.get_pixel(3, 1).0, [50]);
        assert_eq!(gray.get_pixel(3, 0).0, [0]);

        extract_channel_asset(&mut project, "pack", ChannelDef::A, &game).unwrap();
        let alpha = image::open(game.join("vfx/textures/pack_a.png")).unwrap().to_luma8();
        assert_eq!(alpha.get_pixel(2, 0).0, [215]);
        let (_, path) = extract_channel_asset(&mut project, "pack", ChannelDef::Luminance, &game).unwrap();
        let lum = image::open(game.join(&path)).unwrap().to_luma8();
        // 0.2126·30 + 0.7152·200 + 0.0722·0 = 149.4
        assert_eq!(lum.get_pixel(3, 0).0, [149]);

        // Again: the file is rewritten, the asset reused.
        let before = project.assets.len();
        assert_eq!(extract_channel_asset(&mut project, "pack", ChannelDef::B, &game).unwrap().0, "pack_b");
        assert_eq!(project.assets.len(), before);

        // Built-ins go to vfx/textures/.
        assert_eq!(extracted_path("omagari://textures/cloud.png", ChannelDef::R), "vfx/textures/cloud_r.png");
        assert_eq!(extracted_path("top.png", ChannelDef::G), "top_g.png");
        assert!(extract_channel("vfx/a.ktx2", ChannelDef::R, &game).is_err());
        let _ = fs::remove_dir_all(&game);
    }

    #[test]
    fn texture_headers_and_paths() {
        let info = texture_info_at("omagari://textures/cloud2.png", Path::new(".")).unwrap();
        assert_eq!(info.describe(), "333×333 PNG, RGB 16-bit");
        assert!(!info.loads_as_integer_texture());
        let gray16 = TextureInfo { width: 1, height: 1, format: "PNG", channels: 1, bits: 16 };
        assert!(gray16.loads_as_integer_texture());

        assert_eq!(path_problem("vfx/textures/a.png"), Ok(None));
        assert!(path_problem("C:/x/a.png").is_err());
        assert!(path_problem("/abs/a.png").is_err());
        assert!(path_problem("../other/a.png").unwrap().is_some());
        assert!(path_problem("vfx\\a.png").unwrap().is_some());
        assert_eq!(path_problem("omagari://textures/cloud.png"), Ok(None));
    }

    /// An imported texture loads in a game (default asset source, package
    /// path relative to its `assets/`) with its settings: noise repeats and
    /// is linear data, a sprite keeps Bevy's defaults (sRGB, default sampler).
    #[test]
    fn imported_textures_load_in_the_game_with_their_settings() {
        use bevy::asset::AssetPlugin;
        use bevy::image::{CompressedImageFormats, ImageAddressMode, ImageFilterMode, ImageLoader, ImageSampler};
        use bevy::prelude::*;
        use bevy::render::render_resource::TextureFormat;
        use bevy_hanabi::prelude::EffectAsset;
        use omagari::format::{LayerDef, ParticlesLayerDef, TextureFilter};
        use omagari::VfxPackage;

        let dir = temp_dir("runtime_import");
        let game = dir.join("game");
        fs::create_dir_all(&game).unwrap();
        GrayImage::from_fn(8, 8, |x, y| Luma([(x * 30 + y) as u8]))
            .save(dir.join("perlin_noise.png"))
            .unwrap();
        write_test_png(&dir.join("smoke.png"));
        image::RgbImage::from_pixel(6, 4, image::Rgb([250, 120, 20])).save(dir.join("ember.jpg")).unwrap();

        let mut project = ProjectV2::default();
        for file in ["perlin_noise.png", "smoke.png", "ember.jpg"] {
            let imported = import_file(&dir.join(file), &game).unwrap();
            add_texture_asset(&mut project, &imported.path, guess_purpose(file), None).unwrap();
        }
        assert_eq!(project.texture("perlin_noise").unwrap().purpose, TexturePurpose::Noise);
        let noise = project.assets[0].kind.as_texture_mut().unwrap();
        noise.filter = Some(TextureFilter::Nearest);
        for id in ["perlin_noise", "smoke", "ember"] {
            project.layers.push(LayerDef::Particles(ParticlesLayerDef {
                name: id.into(),
                texture: Some(id.into()),
                ..Default::default()
            }));
        }
        crate::controller::export_package(&project, "imported.omagari.ron", &game.join("vfx")).unwrap();

        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            AssetPlugin { file_path: game.to_string_lossy().to_string(), ..default() },
            bevy::image::ImagePlugin::default(),
        ))
        .register_asset_loader(ImageLoader::new(CompressedImageFormats::empty()))
        .init_asset::<EffectAsset>()
        .add_plugins(omagari::OmagariPlugin);
        let package: Handle<VfxPackage> = app.world().resource::<AssetServer>().load("vfx/imported.vfx.ron");
        for _ in 0..500 {
            app.update();
            let server = app.world().resource::<AssetServer>();
            if server.is_loaded_with_dependencies(&package) || server.load_state(&package).is_failed() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let server = app.world().resource::<AssetServer>();
        assert!(server.is_loaded_with_dependencies(&package), "{:?}", server.recursive_dependency_load_state(&package));
        let package = app.world().resource::<Assets<VfxPackage>>().get(&package).unwrap();
        let images = app.world().resource::<Assets<Image>>();
        let image = |i: usize| images.get(package.particle_layers[i].texture.as_ref().unwrap()).unwrap();

        let noise = image(0);
        assert_eq!(noise.texture_descriptor.format, TextureFormat::Rgba8Unorm, "linear data");
        let ImageSampler::Descriptor(sampler) = &noise.sampler else {
            panic!("noise needs its own sampler: {:?}", noise.sampler);
        };
        assert_eq!(sampler.address_mode_u, ImageAddressMode::Repeat);
        assert_eq!(sampler.address_mode_v, ImageAddressMode::Repeat);
        assert_eq!(sampler.mag_filter, ImageFilterMode::Nearest);

        let sprite = image(1);
        assert_eq!(sprite.texture_descriptor.format, TextureFormat::Rgba8UnormSrgb);
        assert!(matches!(sprite.sampler, ImageSampler::Default), "{:?}", sprite.sampler);

        // JPG loads with Bevy's `jpeg` feature (on in the editor build).
        let jpg = image(2);
        assert_eq!(jpg.texture_descriptor.size.width, 6);
        assert_eq!(jpg.texture_descriptor.format, TextureFormat::Rgba8UnormSrgb);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn previews_and_channel_views() {
        let dir = temp_dir("preview");
        write_test_png(&dir.join("p.png"));
        let preview = decode_preview(&fs::read(dir.join("p.png")).unwrap(), 2).unwrap();
        assert_eq!((preview.pixels.width(), preview.pixels.height()), (2, 1), "scaled to fit");
        assert_eq!(preview.info.width, 4);
        assert!(preview.uses_alpha);
        assert!(!preview.grayscale);
        let full = decode_preview(&fs::read(dir.join("p.png")).unwrap(), 64).unwrap();
        let g = view_pixels(&full.pixels, Some(ChannelDef::G));
        assert_eq!(g.get_pixel(0, 0).0, [200, 200, 200, 255]);
        let rgb = view_pixels(&full.pixels, None);
        assert_eq!(rgb.get_pixel(3, 1).0, [30, 200, 50, 255]);
        let builtin = decode_preview(&texture_bytes("omagari://textures/glow1.png", &dir).unwrap(), 96).unwrap();
        assert!(builtin.grayscale);
        let found = scan_textures(&dir, 10);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, "p.png");
        let _ = fs::remove_dir_all(&dir);
    }
}
