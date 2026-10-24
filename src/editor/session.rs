//! Editor session state: undo history, dirty state, live preview and inline
//! validation of the open project, plus the UI state shared by the panels.
//!
//! [`track_project`] runs after the UI every frame: it serializes the project
//! (only when the resource changed), feeds the undo history, rebuilds the
//! preview after edits (debounced) and re-runs the checks (debounced).

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use omagari::format::{ChannelDef, ProjectV2};

use super::actions::Action;
use super::assets::ImportDialog;
use super::history::{Activity, History, UNDO_LIMIT};
use super::layer_view::LayerView;
use crate::controller::{EffectResource, PreviewCommand};
use crate::mcp::analysis::{check_project, Diagnostics};
use crate::settings::GameAssets;

/// Seconds after the last edit before the preview rebuilds.
pub const LIVE_PREVIEW_DELAY: f64 = 0.15;
/// Seconds after the last edit before the checks run again.
pub const VALIDATION_DELAY: f64 = 0.3;
/// Length of the PARAMETERS tab's "animate" (min → max → min), seconds.
pub const PARAM_ANIMATION_SECONDS: f32 = 4.0;

/// A parameter being animated min → max → min on the preview.
#[derive(Clone, Debug, PartialEq)]
pub struct ParamAnimation {
    pub name: String,
    pub elapsed: f32,
}

impl ParamAnimation {
    /// Position in the range, 0 → 1 → 0 over [`PARAM_ANIMATION_SECONDS`].
    pub fn fraction(&self) -> f32 {
        let half = PARAM_ANIMATION_SECONDS * 0.5;
        let t = (self.elapsed / half).clamp(0.0, 2.0);
        if t <= 1.0 { t } else { 2.0 - t }
    }

    pub fn finished(&self) -> bool {
        self.elapsed >= PARAM_ANIMATION_SECONDS
    }
}

/// The tabs of the right panel.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum RightTab {
    #[default]
    Layer,
    SocketsFit,
    Preview,
    Parameters,
    Triggers,
    Assets,
    Hierarchy,
}

/// A modal question.
#[derive(Clone, Debug, PartialEq)]
pub enum Dialog {
    /// `then` would throw away unsaved changes: save, discard or cancel.
    UnsavedChanges { then: Action },
    /// Delete a layer?
    DeleteLayer { index: usize, name: String },
    /// Pick a file name; `then` runs after a successful save.
    SaveAs { name: String, then: Option<Action> },
}

#[derive(Resource)]
pub struct EditorSession {
    /// Undo history over the serialized project.
    pub history: History<String>,
    /// The serialized project as of the last [`track_project`].
    pub text: String,
    text_rev: u64,
    text_changed_at: f64,
    /// The serialized package part (no preview block): live preview rebuilds
    /// when it changes.
    package_text: String,
    package_rev: u64,
    package_changed_at: f64,
    built_rev: u64,
    validated_rev: u64,
    /// Rebuild the preview automatically after edits.
    pub live_preview: bool,
    /// The last checks of the open project (`check_project`).
    pub diagnostics: Diagnostics,
    /// Index of the layer shown in the inspector.
    pub selected: Option<usize>,
    /// Per-layer visibility / solo of the preview.
    pub view: LayerView,
    /// Set when [`Self::view`] changed: the preview rebuilds.
    pub view_changed: bool,
    /// Actions requested by the UI, shortcuts and MCP; run by
    /// `perform_actions`.
    pub actions: Vec<Action>,
    pub dialog: Option<Dialog>,
    /// What the user is doing in the UI this frame (undo coalescing).
    pub activity: Activity,
    pub tab: RightTab,
    pub settings_open: bool,
    pub problems_open: bool,
    /// F was pressed: the camera turns to the target.
    pub focus_camera: bool,
    /// Draw the samples of trail layers (debug, not saved).
    pub show_trail_samples: bool,
    /// Asset shown in the ASSETS tab's details.
    pub selected_asset: Option<String>,
    /// Channel the large asset preview shows; `None` = RGB.
    pub asset_view: Option<ChannelDef>,
    /// The id being typed in the asset rename field: (asset, text).
    pub rename_draft: Option<(String, String)>,
    /// A parameter / trigger name being typed: (`param:<name>` or
    /// `trigger:<name>`, text).
    pub name_draft: Option<(String, String)>,
    /// The PARAMETERS tab's "animate" in progress.
    pub param_animation: Option<ParamAnimation>,
    /// Auto-fired preview triggers carry a random point near the target.
    pub auto_fire_random: bool,
    /// Files are dragged over the window.
    pub drop_hover: bool,
    /// The pointer is over the editor's panels or a dialog (last frame).
    pub pointer_on_ui: bool,
    /// The HIERARCHY tab shows the weapon model.
    pub hierarchy_weapon: bool,
    /// The HIERARCHY tab's search text.
    pub hierarchy_search: String,
    /// The native file dialog of IMPORT, while it is open.
    pub import_dialog: ImportDialog,
}

impl Default for EditorSession {
    fn default() -> Self {
        let project = ProjectV2::default();
        let text = serialize(&project);
        EditorSession {
            history: History::new(text.clone(), UNDO_LIMIT),
            text,
            text_rev: 0,
            text_changed_at: 0.0,
            package_text: package_key(&project),
            package_rev: 0,
            package_changed_at: 0.0,
            built_rev: 0,
            validated_rev: u64::MAX,
            live_preview: true,
            diagnostics: Diagnostics::default(),
            selected: None,
            view: LayerView::default(),
            view_changed: false,
            actions: Vec::new(),
            dialog: None,
            activity: Activity::Idle,
            tab: RightTab::default(),
            settings_open: false,
            problems_open: false,
            focus_camera: false,
            show_trail_samples: false,
            selected_asset: None,
            asset_view: None,
            rename_draft: None,
            name_draft: None,
            param_animation: None,
            auto_fire_random: false,
            drop_hover: false,
            pointer_on_ui: false,
            hierarchy_weapon: false,
            hierarchy_search: String::new(),
            import_dialog: ImportDialog::default(),
        }
    }
}

/// The project text the history and dirty state compare.
pub fn serialize(project: &ProjectV2) -> String {
    project.to_ron_string().unwrap_or_default()
}

/// Everything a package build reads (all but the preview block).
pub fn package_key(project: &ProjectV2) -> String {
    ron::to_string(&(
        &project.parameters,
        &project.triggers,
        &project.assets,
        &project.fit,
        &project.layers,
        &project.sockets,
    ))
    .unwrap_or_default()
}

impl EditorSession {
    pub fn is_dirty(&self) -> bool {
        self.history.is_dirty(&self.text)
    }

    /// A different project was opened or created: the history starts over.
    /// `saved` = it is what is on disk.
    /// The caller renders the new project itself, so live preview skips it.
    pub fn reset(&mut self, project: &ProjectV2, saved: bool) {
        let text = serialize(project);
        self.history.reset(text.clone(), saved);
        self.text = text;
        self.package_text = package_key(project);
        self.package_rev += 1;
        self.preview_built();
        self.revalidate();
        self.selected = (!project.layers.is_empty()).then_some(0);
        self.view = LayerView::default();
        self.selected_asset = None;
        self.rename_draft = None;
        self.name_draft = None;
        self.param_animation = None;
    }

    /// The project was written to disk as it is now.
    pub fn mark_saved(&mut self, project: &ProjectV2) {
        let text = serialize(project);
        self.history.mark_saved(&text);
    }

    /// The preview was rebuilt by hand: live preview has nothing to do.
    pub fn preview_built(&mut self) {
        self.built_rev = self.package_rev;
        self.view_changed = false;
    }

    /// Runs the checks again on the next [`track_project`] (e.g. after the
    /// game assets changed on disk).
    pub fn revalidate(&mut self) {
        self.validated_rev = u64::MAX;
    }

    /// Selects a layer and shows it in the inspector.
    pub fn select(&mut self, index: usize) {
        self.selected = Some(index);
        self.tab = RightTab::Layer;
    }
}

/// Window title: file name and an unsaved marker.
pub fn window_title(filename: Option<&str>, dirty: bool) -> String {
    let name = filename.filter(|f| !f.is_empty()).unwrap_or("untitled");
    format!("{}{} - Omagari", if dirty { "● " } else { "" }, name)
}

/// Feeds the undo history, rebuilds the preview and re-runs the checks after
/// edits, and keeps the window title up to date.
pub fn track_project(
    project: Res<ProjectV2>,
    mut session: ResMut<EditorSession>,
    time: Res<Time<Real>>,
    game_assets: Res<GameAssets>,
    res: Res<EffectResource>,
    mut preview: MessageWriter<PreviewCommand>,
    mut window: Single<&mut Window, With<PrimaryWindow>>,
) {
    let now = time.elapsed_secs_f64();
    let session = &mut *session;
    if project.is_changed() || session.validated_rev == u64::MAX {
        let text = serialize(&project);
        if text != session.text {
            session.text = text;
            session.text_rev += 1;
            session.text_changed_at = now;
            let package = package_key(&project);
            if package != session.package_text {
                session.package_text = package;
                session.package_rev += 1;
                session.package_changed_at = now;
            }
        }
    }
    session.history.observe(&session.text, now, session.activity);

    session.view.retain_layers(&project);
    if let Some(selected) = session.selected {
        if selected >= project.layers.len() {
            session.selected = project.layers.len().checked_sub(1);
        }
    }

    let edited = session.live_preview
        && session.package_rev != session.built_rev
        && now - session.package_changed_at >= LIVE_PREVIEW_DELAY;
    if edited || session.view_changed {
        preview.write(PreviewCommand::Render);
        session.preview_built();
    }

    if session.validated_rev != session.text_rev && now - session.text_changed_at >= VALIDATION_DELAY {
        session.diagnostics = check_project(&project, &game_assets.assets_root());
        session.validated_rev = session.text_rev;
    }

    let title = window_title(res.context.filename.as_deref(), session.is_dirty());
    if window.title != title {
        window.title = title;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omagari::format::{LayerDef, ParticlesLayerDef, PreviewDef};

    fn project() -> ProjectV2 {
        let mut project = ProjectV2::default();
        project.layers.push(LayerDef::Particles(ParticlesLayerDef::default()));
        project
    }

    #[test]
    fn parameter_animation_goes_min_max_min() {
        let at = |elapsed: f32| ParamAnimation { name: "p".into(), elapsed };
        assert_eq!(at(0.0).fraction(), 0.0);
        assert_eq!(at(PARAM_ANIMATION_SECONDS * 0.25).fraction(), 0.5);
        assert_eq!(at(PARAM_ANIMATION_SECONDS * 0.5).fraction(), 1.0);
        assert_eq!(at(PARAM_ANIMATION_SECONDS * 0.75).fraction(), 0.5);
        assert_eq!(at(PARAM_ANIMATION_SECONDS).fraction(), 0.0);
        assert!(at(PARAM_ANIMATION_SECONDS).finished() && !at(1.0).finished());
    }

    #[test]
    fn title_shows_name_and_unsaved_marker() {
        assert_eq!(window_title(None, false), "untitled - Omagari");
        assert_eq!(window_title(Some("fire.omagari.ron"), true), "● fire.omagari.ron - Omagari");
        assert_eq!(window_title(Some(""), false), "untitled - Omagari");
    }

    #[test]
    fn package_key_ignores_the_preview_block() {
        let a = project();
        let mut b = a.clone();
        b.preview = Some(PreviewDef::default());
        assert_eq!(package_key(&a), package_key(&b));
        assert_ne!(serialize(&a), serialize(&b));
        b.layers[0].as_particles_mut().unwrap().effect.capacity += 1;
        assert_ne!(package_key(&a), package_key(&b));
    }

    /// Undo restores a project from its text; it must come back identical,
    /// or restoring would count as a new edit.
    #[test]
    fn project_text_round_trips() {
        for recipe in crate::mcp::guide_recipes() {
            let project = ProjectV2::from_ron_str(&recipe.ron).unwrap();
            let text = serialize(&project);
            let back = ProjectV2::from_ron_str(&text).unwrap();
            assert_eq!(serialize(&back), text, "{}", recipe.title);
        }
    }

    /// Dirty tracking through the session: open = clean, edit = dirty, save =
    /// clean, undo past the save = dirty.
    #[test]
    fn dirty_state_follows_edits_saves_and_undo() {
        let mut session = EditorSession::default();
        let mut p = project();
        session.reset(&p, true);
        session.text = serialize(&p);
        assert!(!session.is_dirty());

        p.layers[0].as_particles_mut().unwrap().name = "renamed".into();
        session.text = serialize(&p);
        assert!(session.is_dirty());
        session.history.observe(&session.text.clone(), 1.0, Activity::Idle);
        session.mark_saved(&p);
        assert!(!session.is_dirty());

        let restored = session.history.undo(&session.text.clone()).unwrap();
        session.text = restored;
        assert!(session.is_dirty());
        assert_eq!(session.selected, Some(0));
    }
}
