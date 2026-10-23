//! Editor commands. Buttons, menus, shortcuts, the window's close button and
//! MCP preview requests queue [`Action`]s in the session; [`perform_actions`]
//! runs them after the UI, asking first when unsaved changes would be lost or
//! a layer deleted.

use std::path::PathBuf;

use bevy::prelude::*;
use bevy::window::WindowCloseRequested;

use omagari::format::{builtin_texture_path, ChannelDef, LayerDef, ProjectV2, TexturePurpose};
use omagari::runtime::resolve_asset_path_in;

use super::assets::{select_asset, TexturePreviews};
use super::effect::new_particles_layer;
use super::helpers::ui_text;
use super::session::{Dialog, EditorSession};
use crate::asset_library::{self as library, ImportKind};
use crate::controller::*;
use crate::mcp::{self, PreviewRequest};
use crate::settings::{GameAssets, GAME_ASSET_SOURCE};

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// An empty project.
    New,
    /// A project from the guide recipe with this index ([`mcp::guide_recipes`]).
    NewFromRecipe(usize),
    /// Open `saved_effects/<name>`.
    Open(String),
    /// An MCP `preview_in_editor` request: open the project and play it.
    Preview(PreviewRequest),
    Quit,
    Save,
    /// Save, then run the action (when the save worked).
    SaveThen(Box<Action>),
    SaveAs,
    /// Save under this file name (`.omagari.ron` is added when missing).
    SaveTo(String),
    Export,
    Undo,
    Redo,
    Render,
    Stop,
    Restart,
    FocusCamera,
    AddParticlesLayer,
    SelectLayer(usize),
    DuplicateLayer(usize),
    DeleteLayer(usize),
    MoveLayer { from: usize, to: usize },
    /// Opens the native file dialog of IMPORT.
    OpenImportDialog,
    /// Imports texture files (file dialog, drag & drop, `--import`) into
    /// the game assets folder and adds them as assets.
    ImportFiles(Vec<PathBuf>),
    /// Adds a built-in texture (file name, e.g. `cloud.png`) as an asset.
    AddBuiltinTexture(String),
    /// Shows an asset in the ASSETS tab.
    SelectAsset(String),
    /// Renames an asset and every reference to it.
    RenameAsset { from: String, to: String },
    /// Removes an asset (refused while layers use it).
    DeleteAsset(String),
    /// Writes one channel of an asset as a grayscale PNG and adds it.
    ExtractChannel { asset: String, channel: ChannelDef },
    /// Fires a trigger on the preview, optionally at a random point near the
    /// target.
    FireTrigger { name: String, random_point: bool },
    /// Runs the action without asking (the user already confirmed).
    Confirmed(Box<Action>),
}

impl Action {
    /// Actions that replace the open project.
    fn discards_project(&self) -> bool {
        matches!(
            self,
            Action::New | Action::NewFromRecipe(_) | Action::Open(_) | Action::Preview(_) | Action::Quit
        )
    }
}

/// The file name a typed name is saved under: `.omagari.ron` is added when
/// missing; `None` when nothing usable is left.
pub fn project_file_name(input: &str) -> Option<String> {
    let name = input.trim();
    let name = if name.ends_with(".omagari.ron") {
        name.to_string()
    } else {
        format!("{}.omagari.ron", name.trim_end_matches(".ron").trim_end_matches(".omagari"))
    };
    let base = name.trim_end_matches(".omagari.ron");
    let ok = !base.is_empty() && !base.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|']);
    ok.then_some(name)
}

/// A name for a copy of a layer that no other layer has.
pub fn copy_name(name: &str, existing: &[String]) -> String {
    let base = name.trim_end_matches(|c: char| c.is_ascii_digit());
    let base = if base.is_empty() { name } else { base };
    let mut n = 2;
    loop {
        let candidate = format!("{}{}", base, n);
        if !existing.contains(&candidate) {
            return candidate;
        }
        n += 1;
    }
}

/// Moves `list[from]` to `to`, keeping the order of the others.
pub fn move_item<T>(list: &mut Vec<T>, from: usize, to: usize) -> bool {
    if from >= list.len() || to >= list.len() || from == to {
        return false;
    }
    let item = list.remove(from);
    list.insert(to, item);
    true
}

#[allow(clippy::too_many_arguments)]
pub fn perform_actions(
    mut project: ResMut<ProjectV2>,
    mut res: ResMut<EffectResource>,
    mut session: ResMut<EditorSession>,
    mut preview: MessageWriter<PreviewCommand>,
    mut exit: MessageWriter<AppExit>,
    game_assets: Res<GameAssets>,
    asset_server: Option<Res<AssetServer>>,
    previews: Option<ResMut<TexturePreviews>>,
) {
    let actions = std::mem::take(&mut session.actions);
    if actions.is_empty() {
        return;
    }
    let mut ctx = ActionContext {
        project: &mut project,
        app: &mut res.context,
        session: &mut session,
        preview: Vec::new(),
        exit: false,
        game_assets: &game_assets,
        rewritten: Vec::new(),
    };
    for action in actions {
        ctx.perform(action, false);
    }
    let (commands, quit, rewritten) = (ctx.preview, ctx.exit, ctx.rewritten);
    preview.write_batch(commands);
    // Files written over: loaded textures and previews show the new pixels.
    if let Some(server) = asset_server {
        for path in rewritten.iter() {
            server.reload(resolve_asset_path_in(path, Some(GAME_ASSET_SOURCE)));
        }
    }
    if let Some(mut previews) = previews {
        for path in rewritten.iter() {
            previews.invalidate(path);
        }
    }
    if quit {
        exit.write(AppExit::Success);
    }
}

struct ActionContext<'a> {
    project: &'a mut ProjectV2,
    app: &'a mut super::AppContext,
    session: &'a mut EditorSession,
    /// Preview commands to send.
    preview: Vec<PreviewCommand>,
    exit: bool,
    game_assets: &'a GameAssets,
    /// Game texture paths whose files were written over.
    rewritten: Vec<String>,
}

impl ActionContext<'_> {
    fn status(&mut self, text: impl Into<String>) {
        self.app.status = Some(text.into());
    }

    fn render(&mut self) {
        self.preview.push(PreviewCommand::Render);
        self.session.preview_built();
    }

    /// Replaces the open project (new / open / preview request).
    fn replace_project(&mut self, project: ProjectV2, filename: Option<String>, saved: bool) {
        *self.project = project;
        self.app.filename = filename;
        self.app.suggested_name = None;
        self.session.reset(self.project, saved);
        self.session.dialog = None;
        if self.project.layers.is_empty() {
            self.preview.push(PreviewCommand::Stop);
        } else {
            self.render();
        }
    }

    fn perform(&mut self, action: Action, confirmed: bool) {
        if !confirmed && action.discards_project() && self.session.is_dirty() {
            self.session.dialog = Some(Dialog::UnsavedChanges { then: action });
            return;
        }
        match action {
            Action::Confirmed(action) => self.perform(*action, true),
            Action::New => {
                self.replace_project(ProjectV2::default(), None, true);
                self.status("New project");
            }
            Action::NewFromRecipe(index) => {
                let Some(recipe) = mcp::guide_recipes().into_iter().nth(index) else {
                    return;
                };
                match ProjectV2::from_ron_str(&recipe.ron) {
                    Ok(project) => {
                        // Not on disk yet: unsaved, SAVE asks for a name.
                        self.replace_project(project, None, false);
                        self.app.suggested_name = Some(recipe.file_name());
                        self.status(format!("New project from the recipe \"{}\"", ui_text(&recipe.title)));
                    }
                    Err(e) => self.status(format!("Recipe \"{}\" does not load: {}", recipe.title, e)),
                }
            }
            Action::Open(name) => match load_project(&name) {
                Ok(project) => {
                    self.replace_project(project, Some(name.clone()), true);
                    self.status(format!("Opened {}", name));
                }
                Err(e) => self.status(format!("Cannot open {}: {}", name, e)),
            },
            Action::Preview(request) => self.preview_request(request),
            Action::Quit => self.exit = true,
            Action::Save => match self.app.filename.clone().filter(|f| validate_project_filename(f)) {
                Some(name) => {
                    self.save_to(&name);
                }
                None => self.ask_name(None),
            },
            Action::SaveThen(then) => match self.app.filename.clone().filter(|f| validate_project_filename(f)) {
                Some(name) => {
                    if self.save_to(&name) {
                        self.perform(*then, true);
                    }
                }
                None => self.ask_name(Some(*then)),
            },
            Action::SaveAs => self.ask_name(None),
            Action::SaveTo(input) => match project_file_name(&input) {
                Some(name) => {
                    self.save_to(&name);
                }
                None => self.status(format!("\"{}\" is not a usable file name", input)),
            },
            Action::Export => self.export(),
            Action::Undo => {
                let current = self.session.text.clone();
                if let Some(text) = self.session.history.undo(&current) {
                    self.restore(&text);
                }
            }
            Action::Redo => {
                let current = self.session.text.clone();
                if let Some(text) = self.session.history.redo(&current) {
                    self.restore(&text);
                }
            }
            Action::Render => self.render(),
            Action::FireTrigger { name, random_point } => {
                self.preview.push(PreviewCommand::Fire { name: name.clone(), random_point });
                self.status(format!(
                    "Fired \"{}\"{}",
                    name,
                    if random_point { " at a random point near the target" } else { "" }
                ));
            }
            Action::Stop => self.preview.push(PreviewCommand::Stop),
            Action::Restart => {
                self.preview.push(PreviewCommand::Restart);
                self.session.preview_built();
            }
            Action::FocusCamera => self.session.focus_camera = true,
            Action::AddParticlesLayer => {
                let names: Vec<String> = self.project.layers.iter().map(|l| l.name().to_string()).collect();
                let layer = new_particles_layer(&mut self.project.assets, &names);
                self.project.layers.push(LayerDef::Particles(layer));
                self.session.select(self.project.layers.len() - 1);
            }
            Action::SelectLayer(index) => {
                if index < self.project.layers.len() {
                    self.session.select(index);
                }
            }
            Action::DuplicateLayer(index) => {
                let Some(layer) = self.project.layers.get(index) else {
                    return;
                };
                let names: Vec<String> = self.project.layers.iter().map(|l| l.name().to_string()).collect();
                let mut copy = layer.clone();
                match &mut copy {
                    LayerDef::Particles(p) => p.name = copy_name(&p.name, &names),
                }
                self.project.layers.insert(index + 1, copy);
                self.session.select(index + 1);
            }
            Action::DeleteLayer(index) => {
                let Some(layer) = self.project.layers.get(index) else {
                    return;
                };
                if !confirmed {
                    self.session.dialog = Some(Dialog::DeleteLayer {
                        index,
                        name: layer.name().to_string(),
                    });
                    return;
                }
                let name = self.project.layers.remove(index).name().to_string();
                self.session.selected = match self.project.layers.len() {
                    0 => None,
                    n => Some(index.min(n - 1)),
                };
                self.status(format!("Deleted layer \"{}\" (Ctrl+Z brings it back)", name));
            }
            Action::MoveLayer { from, to } => {
                if move_item(&mut self.project.layers, from, to) {
                    if self.session.selected == Some(from) {
                        self.session.selected = Some(to);
                    }
                }
            }
            Action::OpenImportDialog => {
                let root = std::path::absolute(self.game_assets.assets_root()).ok();
                self.session.import_dialog.open(root);
            }
            Action::ImportFiles(files) => self.import_files(files),
            Action::AddBuiltinTexture(file) => {
                let path = builtin_texture_path(&file);
                let id = file.strip_suffix(".png").unwrap_or(&file).to_string();
                match library::add_texture_asset(self.project, &path, TexturePurpose::Sprite, Some(&id))
                    .or_else(|_| library::add_texture_asset(self.project, &path, TexturePurpose::Sprite, None))
                {
                    Ok((asset, added)) => {
                        if added {
                            self.status(format!("Added the built-in texture {} as \"{}\"", file, asset));
                        }
                        select_asset(self.session, &asset);
                    }
                    Err(e) => self.status(e),
                }
            }
            Action::SelectAsset(id) => select_asset(self.session, &id),
            Action::RenameAsset { from, to } => match self.project.rename_asset(&from, &to) {
                Ok(()) => {
                    let to = to.trim().to_string();
                    let users = self.project.asset_users(&to).len();
                    self.status(format!("Renamed asset \"{}\" to \"{}\" ({} layer(s) updated)", from, to, users));
                    select_asset(self.session, &to);
                }
                Err(e) => {
                    self.status(format!("Cannot rename \"{}\": {}", from, e));
                    self.session.rename_draft = None;
                }
            },
            Action::DeleteAsset(id) => match self.project.delete_asset(&id) {
                Ok(_) => {
                    self.session.selected_asset = None;
                    self.status(format!("Removed asset \"{}\" (the file stays on disk; Ctrl+Z brings it back)", id));
                }
                Err(e) => self.status(format!("Cannot delete: {}; pick another texture on those layers first", e)),
            },
            Action::ExtractChannel { asset, channel } => {
                let root = self.game_assets.assets_root();
                match library::extract_channel_asset(self.project, &asset, channel, &root) {
                    Ok((id, path)) => {
                        self.status(format!(
                            "Extracted {} of \"{}\" to {} (asset \"{}\")",
                            channel.name(),
                            asset,
                            path,
                            id
                        ));
                        self.rewritten.push(path);
                        self.session.revalidate();
                        select_asset(self.session, &id);
                    }
                    Err(e) => self.status(format!("Cannot extract {} of \"{}\": {}", channel.name(), asset, e)),
                }
            }
        }
    }

    /// Imports files into the game assets folder and adds them as assets
    /// (purpose guessed from the file name); the last one is shown in the
    /// ASSETS tab.
    fn import_files(&mut self, files: Vec<PathBuf>) {
        let root = self.game_assets.assets_root();
        let mut done = Vec::new();
        let mut skipped = Vec::new();
        let mut last = None;
        for file in files {
            let name = file.file_name().unwrap_or_default().to_string_lossy().to_string();
            let imported = match library::import_file(&file, &root) {
                Ok(imported) => imported,
                Err(e) => {
                    skipped.push(e);
                    continue;
                }
            };
            match library::add_texture_asset(self.project, &imported.path, library::guess_purpose(&name), None) {
                Ok((id, _)) => {
                    let how = match imported.kind {
                        ImportKind::Copied => format!("copied to {}", imported.path),
                        ImportKind::Reused => format!("same file as {}", imported.path),
                        ImportKind::InPlace => format!("used in place: {}", imported.path),
                    };
                    done.push(format!("\"{}\" ({})", id, how));
                    last = Some(id);
                }
                Err(e) => skipped.push(e),
            }
        }
        if let Some(id) = last {
            select_asset(self.session, &id);
        }
        self.session.revalidate();
        let mut status = String::new();
        if !done.is_empty() {
            status = format!("Imported {}", done.join(", "));
        }
        if !skipped.is_empty() {
            if !status.is_empty() {
                status.push_str("; ");
            }
            status.push_str(&format!("not imported: {}", skipped.join("; ")));
        }
        if !status.is_empty() {
            self.status(status);
        }
    }

    /// Restores an undo/redo state.
    fn restore(&mut self, text: &str) {
        match ProjectV2::from_ron_str(text) {
            Ok(project) => {
                *self.project = project;
                // Keep the committed text identical to what the tracker
                // will serialize, so the restore is not a new edit.
                self.session.history.normalize(super::session::serialize(self.project));
            }
            Err(e) => self.status(format!("Undo failed: {}", e)),
        }
    }

    fn ask_name(&mut self, then: Option<Action>) {
        let name = self
            .app
            .filename
            .clone()
            .or_else(|| self.app.suggested_name.clone())
            .unwrap_or_default();
        self.session.dialog = Some(Dialog::SaveAs { name, then });
    }

    fn save_to(&mut self, name: &str) -> bool {
        match save_project(name, self.project) {
            Ok(path) => {
                self.app.filename = Some(name.to_string());
                self.session.mark_saved(self.project);
                self.status(format!("Saved {}", path.display()));
                if let Some(Dialog::SaveAs { then, .. }) = &self.session.dialog {
                    let then = then.clone();
                    self.session.dialog = None;
                    if let Some(then) = then {
                        self.perform(then, true);
                    }
                }
                true
            }
            Err(e) => {
                self.status(format!("Save failed: {}", e));
                false
            }
        }
    }

    fn export(&mut self) {
        let Some(name) = self.app.filename.clone().filter(|f| validate_project_filename(f)) else {
            self.status("Name the project first (SAVE AS): the export is named after it");
            return;
        };
        let result = export_effects_to_files(&name, self.project, self.game_assets);
        self.status(match result {
            Ok(path) => format!("Exported {}", path.display()),
            Err(e) => format!("Export failed: {}", e),
        });
    }

    /// Loads and plays the project an MCP preview request names, with its
    /// stage and motion.
    fn preview_request(&mut self, request: PreviewRequest) {
        let filename = request.name.clone();
        match load_project(&filename) {
            Ok(mut loaded) => {
                request.apply_to(&mut loaded);
                if let Some(stage) = request.stage.as_deref().and_then(super::scene::Stage::parse) {
                    stage.store(&mut loaded);
                }
                if let Some(camera) = request.camera.as_deref().and_then(super::camera::PreviewCamera::parse) {
                    camera.store(&mut loaded);
                }
                // The request's stage, motion and values are viewing
                // choices: the project still counts as saved.
                self.replace_project(loaded, Some(filename.clone()), true);
                if let Some(trigger) = request.trigger {
                    self.preview.push(PreviewCommand::Fire { name: trigger, random_point: false });
                }
                self.status(format!("Previewing {} (requested by Claude)", filename));
            }
            Err(e) => {
                warn!("preview request for {}: {}", filename, e);
                self.status(format!("Preview request for {} failed: {}", filename, e));
            }
        }
    }
}

/// The window's close button: quits, after asking about unsaved changes
/// (`close_when_requested` is off).
pub fn handle_close_requests(mut requests: MessageReader<WindowCloseRequested>, mut session: ResMut<EditorSession>) {
    if requests.read().count() > 0 {
        session.actions.push(Action::Quit);
    }
}

/// Picks up project previews the MCP server asks for.
///
/// The server writes a JSON request (`{ "name", "stage", "motion" }`, see
/// [`mcp::PreviewRequest`]; the older form is the file name, optionally
/// followed by a stage name on the next line) to
/// `saved_effects/.preview_request` and waits for the editor to delete it.
pub fn poll_preview_requests(mut since_last_poll: Local<f32>, time: Res<Time<Real>>, mut session: ResMut<EditorSession>) {
    *since_last_poll += time.delta_secs();
    if *since_last_poll < 0.25 {
        return;
    }
    *since_last_poll = 0.0;

    let request = Folder::SavedEffects.full_file_path(mcp::PREVIEW_REQUEST_FILE.to_string());
    let Ok(content) = std::fs::read_to_string(&request) else {
        return;
    };
    let _ = std::fs::remove_file(&request);
    match mcp::PreviewRequest::parse(&content) {
        Ok(request) => session.actions.push(Action::Preview(request)),
        Err(e) => warn!("preview request: {}", e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_names_become_project_file_names() {
        assert_eq!(project_file_name("fire").as_deref(), Some("fire.omagari.ron"));
        assert_eq!(project_file_name(" fire.omagari.ron ").as_deref(), Some("fire.omagari.ron"));
        assert_eq!(project_file_name("fire.omagari").as_deref(), Some("fire.omagari.ron"));
        assert_eq!(project_file_name("fire.ron").as_deref(), Some("fire.omagari.ron"));
        assert_eq!(project_file_name(""), None);
        assert_eq!(project_file_name(".omagari.ron"), None);
        assert_eq!(project_file_name("a/b"), None);
        for name in ["fire", "x.omagari"] {
            assert!(validate_project_filename(&project_file_name(name).unwrap()));
        }
    }

    #[test]
    fn copies_get_unique_names() {
        let existing = vec!["sparks".to_string(), "sparks2".to_string()];
        assert_eq!(copy_name("sparks", &existing), "sparks3");
        assert_eq!(copy_name("sparks2", &existing), "sparks3");
        assert_eq!(copy_name("smoke", &existing), "smoke2");
        assert_eq!(copy_name("7", &[]), "72");
    }

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<PreviewCommand>()
            .init_resource::<EditorSession>()
            .init_resource::<EffectResource>()
            .insert_resource(GameAssets::default())
            .insert_resource(ProjectV2::default())
            .add_systems(Update, (perform_actions, super::super::session::track_project).chain());
        app.world_mut().spawn((Window::default(), bevy::window::PrimaryWindow));
        app.update();
        app
    }

    fn act(app: &mut App, action: Action) {
        app.world_mut().resource_mut::<EditorSession>().actions.push(action);
        app.update();
    }

    fn capacity(app: &App) -> u32 {
        app.world().resource::<ProjectV2>().layers[0].as_particles().unwrap().effect.capacity
    }

    fn session(app: &App) -> &EditorSession {
        app.world().resource::<EditorSession>()
    }

    /// Recipe → edit → undo → redo → delete (asks) through the real systems.
    #[test]
    fn edits_undo_redo_and_confirmations() {
        let mut app = app();
        act(&mut app, Action::NewFromRecipe(0));
        let layers = app.world().resource::<ProjectV2>().layers.len();
        assert!(layers > 0);
        assert!(session(&app).is_dirty(), "a recipe project is not on disk yet");
        assert_eq!(session(&app).selected, Some(0));
        let original = capacity(&app);

        app.world_mut().resource_mut::<ProjectV2>().layers[0].as_particles_mut().unwrap().effect.capacity = 7;
        app.update();
        assert!(session(&app).history.can_undo(&session(&app).text));

        act(&mut app, Action::Undo);
        assert_eq!(capacity(&app), original);
        app.update();
        assert!(session(&app).history.can_redo(&session(&app).text), "restoring is not a new edit");
        act(&mut app, Action::Redo);
        assert_eq!(capacity(&app), 7);

        act(&mut app, Action::DeleteLayer(0));
        assert_eq!(app.world().resource::<ProjectV2>().layers.len(), layers, "asks first");
        assert!(matches!(session(&app).dialog, Some(Dialog::DeleteLayer { index: 0, .. })));
        act(&mut app, Action::Confirmed(Box::new(Action::DeleteLayer(0))));
        assert_eq!(app.world().resource::<ProjectV2>().layers.len(), layers - 1);
        act(&mut app, Action::Undo);
        assert_eq!(app.world().resource::<ProjectV2>().layers.len(), layers);

        // Unsaved changes: replacing the project asks first.
        act(&mut app, Action::New);
        assert!(matches!(session(&app).dialog, Some(Dialog::UnsavedChanges { then: Action::New })));
        assert_eq!(app.world().resource::<ProjectV2>().layers.len(), layers);
        act(&mut app, Action::Confirmed(Box::new(Action::New)));
        assert!(app.world().resource::<ProjectV2>().layers.is_empty());
        assert!(!session(&app).is_dirty());
        assert!(!session(&app).history.can_undo(&session(&app).text), "a new project has no history");
    }

    #[test]
    fn duplicate_and_move_keep_the_selection_on_the_layer() {
        let mut app = app();
        act(&mut app, Action::AddParticlesLayer);
        act(&mut app, Action::DuplicateLayer(0));
        let names: Vec<String> =
            app.world().resource::<ProjectV2>().layers.iter().map(|l| l.name().to_string()).collect();
        assert_eq!(names, vec!["particles", "particles2"]);
        assert_eq!(session(&app).selected, Some(1));
        act(&mut app, Action::MoveLayer { from: 1, to: 0 });
        assert_eq!(app.world().resource::<ProjectV2>().layers[0].name(), "particles2");
        assert_eq!(session(&app).selected, Some(0));
    }

    fn project(app: &App) -> &ProjectV2 {
        app.world().resource::<ProjectV2>()
    }

    /// Import (as a drop would), rename, delete (refused while used),
    /// extract a channel and add a built-in, all undoable project edits.
    #[test]
    fn asset_actions_edit_the_project() {
        let dir = crate::asset_library::tests::temp_dir("asset_actions");
        let game = dir.join("game");
        std::fs::create_dir_all(&game).unwrap();
        crate::asset_library::tests::write_test_png(&dir.join("Voronoi.png"));
        let mut app = app();
        app.insert_resource(GameAssets::resolve(Some(game.to_string_lossy().to_string()), None, &Default::default()));
        act(&mut app, Action::AddParticlesLayer);

        act(&mut app, Action::ImportFiles(vec![dir.join("Voronoi.png"), dir.join("readme.txt")]));
        let texture = project(&app).texture("Voronoi").expect("imported").clone();
        assert_eq!(texture.path, "vfx/textures/Voronoi.png");
        assert_eq!(texture.purpose, TexturePurpose::Noise, "guessed from the name");
        assert!(game.join("vfx/textures/Voronoi.png").is_file());
        assert_eq!(session(&app).selected_asset.as_deref(), Some("Voronoi"));
        assert_eq!(session(&app).tab, super::super::session::RightTab::Assets);
        let status = app.world().resource::<EffectResource>().context.status.clone().unwrap();
        assert!(status.contains("copied to vfx/textures/Voronoi.png") && status.contains("not imported"), "{}", status);

        // Use it on the layer, then rename: the layer follows.
        app.world_mut().resource_mut::<ProjectV2>().layers[0].as_particles_mut().unwrap().texture = Some("Voronoi".into());
        app.update();
        act(&mut app, Action::RenameAsset { from: "Voronoi".into(), to: "cells".into() });
        assert_eq!(project(&app).layers[0].as_particles().unwrap().texture.as_deref(), Some("cells"));
        assert_eq!(session(&app).selected_asset.as_deref(), Some("cells"));
        act(&mut app, Action::Undo);
        assert_eq!(project(&app).layers[0].as_particles().unwrap().texture.as_deref(), Some("Voronoi"));
        act(&mut app, Action::Redo);

        act(&mut app, Action::DeleteAsset("cells".into()));
        assert!(project(&app).asset("cells").is_some(), "used by a layer");
        let status = app.world().resource::<EffectResource>().context.status.clone().unwrap();
        assert!(status.contains("used by \"particles\""), "{}", status);

        act(&mut app, Action::ExtractChannel { asset: "cells".into(), channel: ChannelDef::B });
        let extracted = project(&app).texture("cells_b").expect("extracted").clone();
        assert_eq!(extracted.path, "vfx/textures/Voronoi_b.png");
        assert_eq!(extracted.purpose, TexturePurpose::Noise);
        assert!(game.join("vfx/textures/Voronoi_b.png").is_file());
        act(&mut app, Action::DeleteAsset("cells_b".into()));
        assert!(project(&app).asset("cells_b").is_none(), "unused assets can be deleted");

        let before = project(&app).assets.len();
        act(&mut app, Action::AddBuiltinTexture("glow1.png".into()));
        assert_eq!(project(&app).texture("glow1").unwrap().path, "omagari://textures/glow1.png");
        act(&mut app, Action::AddBuiltinTexture("glow1.png".into()));
        assert_eq!(project(&app).assets.len(), before + 1, "added once");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn move_item_keeps_the_order_of_the_others() {
        let mut list = vec![1, 2, 3, 4];
        assert!(move_item(&mut list, 0, 2));
        assert_eq!(list, vec![2, 3, 1, 4]);
        assert!(move_item(&mut list, 3, 0));
        assert_eq!(list, vec![4, 2, 3, 1]);
        assert!(!move_item(&mut list, 1, 1));
        assert!(!move_item(&mut list, 0, 9));
    }
}
