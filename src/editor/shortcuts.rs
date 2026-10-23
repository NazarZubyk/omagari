//! Keyboard shortcuts of the editor.

use bevy_egui::egui::{self, Key, KeyboardShortcut, Modifiers};

use super::actions::Action;

/// What a shortcut does; turned into an [`Action`] with the current
/// selection.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Shortcut {
    Save,
    Export,
    Undo,
    Redo,
    DeleteLayer,
    DuplicateLayer,
    FocusCamera,
    Render,
}

/// Shortcuts in matching order: egui ignores extra Shift when matching, so
/// Ctrl+Shift+Z must come before Ctrl+Z.
pub const SHORTCUTS: [(Modifiers, Key, Shortcut); 9] = [
    (Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::Z, Shortcut::Redo),
    (Modifiers::COMMAND, Key::Z, Shortcut::Undo),
    (Modifiers::COMMAND, Key::Y, Shortcut::Redo),
    (Modifiers::COMMAND, Key::S, Shortcut::Save),
    (Modifiers::COMMAND, Key::E, Shortcut::Export),
    (Modifiers::COMMAND, Key::D, Shortcut::DuplicateLayer),
    (Modifiers::NONE, Key::Delete, Shortcut::DeleteLayer),
    (Modifiers::NONE, Key::F, Shortcut::FocusCamera),
    (Modifiers::NONE, Key::Space, Shortcut::Render),
];

impl Shortcut {
    /// Whether the shortcut works while a text field has focus. Only save and
    /// export, which never mean anything to a text field; Ctrl+Z/Y, Delete,
    /// Space and F belong to the text being typed.
    pub fn works_while_typing(self) -> bool {
        matches!(self, Shortcut::Save | Shortcut::Export)
    }

    /// Text shown in tooltips and menus.
    pub fn label(self) -> &'static str {
        match self {
            Shortcut::Save => "Ctrl+S",
            Shortcut::Export => "Ctrl+E",
            Shortcut::Undo => "Ctrl+Z",
            Shortcut::Redo => "Ctrl+Y / Ctrl+Shift+Z",
            Shortcut::DeleteLayer => "Delete",
            Shortcut::DuplicateLayer => "Ctrl+D",
            Shortcut::FocusCamera => "F",
            Shortcut::Render => "Space",
        }
    }

    /// The action, given the selected layer (layer shortcuts need one).
    pub fn action(self, selected: Option<usize>) -> Option<Action> {
        Some(match self {
            Shortcut::Save => Action::Save,
            Shortcut::Export => Action::Export,
            Shortcut::Undo => Action::Undo,
            Shortcut::Redo => Action::Redo,
            Shortcut::DeleteLayer => Action::DeleteLayer(selected?),
            Shortcut::DuplicateLayer => Action::DuplicateLayer(selected?),
            Shortcut::FocusCamera => Action::FocusCamera,
            Shortcut::Render => Action::Render,
        })
    }
}

/// Consumes this frame's shortcut key presses (so focused widgets don't also
/// react to them) and returns their actions. `typing` = a text field has
/// focus.
pub fn read_shortcuts(ctx: &egui::Context, typing: bool, selected: Option<usize>) -> Vec<Action> {
    let mut actions = Vec::new();
    ctx.input_mut(|input| {
        for (modifiers, key, shortcut) in SHORTCUTS {
            if typing && !shortcut.works_while_typing() {
                continue;
            }
            if input.consume_shortcut(&KeyboardShortcut::new(modifiers, key)) {
                actions.extend(shortcut.action(selected));
            }
        }
    });
    actions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn more_specific_shortcuts_come_first() {
        // A shortcut must not be shadowed by an earlier one that matches the
        // same key with a subset of its modifiers.
        for (i, (mods, key, _)) in SHORTCUTS.iter().enumerate() {
            for (earlier_mods, earlier_key, _) in &SHORTCUTS[..i] {
                if earlier_key == key {
                    assert!(
                        !mods.contains(*earlier_mods) || mods == earlier_mods,
                        "{:?} is shadowed",
                        key
                    );
                }
            }
        }
    }

    #[test]
    fn only_save_and_export_work_while_typing() {
        let typing: Vec<Shortcut> =
            SHORTCUTS.iter().map(|(_, _, s)| *s).filter(|s| s.works_while_typing()).collect();
        assert_eq!(typing, vec![Shortcut::Save, Shortcut::Export]);
    }

    #[test]
    fn layer_shortcuts_need_a_selection() {
        assert_eq!(Shortcut::DeleteLayer.action(None), None);
        assert_eq!(Shortcut::DeleteLayer.action(Some(2)), Some(Action::DeleteLayer(2)));
        assert_eq!(Shortcut::DuplicateLayer.action(Some(0)), Some(Action::DuplicateLayer(0)));
        assert_eq!(Shortcut::Undo.action(None), Some(Action::Undo));
    }
}
