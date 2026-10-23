//! Editor-only UI code: egui widgets for the data model in `omagari::format`.

pub mod effect;
pub mod expr;
pub mod helpers;
pub mod modifiers;

use omagari::format::ExprWriterEditor;

/// Editor state shared by the widgets.
#[derive(Default)]
pub struct AppContext {
    pub expr_clipboard: Option<ExprWriterEditor>,
    /// Names of the open project's layers (parent pickers).
    pub layer_names: Vec<String>,
    pub filename: Option<String>,
    /// Result of the last save/export, shown in the toolbar.
    pub status: Option<String>,
    /// Text of the game assets folder field in the settings menu.
    pub game_assets_input: Option<String>,
}
