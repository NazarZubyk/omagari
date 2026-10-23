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
    pub visible_effects: Vec<String>,
    pub filename: Option<String>,
}
