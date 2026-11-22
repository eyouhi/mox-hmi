use crate::bridge::{self, NoticeLevel, WorkspaceMode};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorKey {
    Character(String),
    F1,
    Escape,
    Delete,
    Backspace,
    Other,
}

impl EditorKey {
    fn character(&self) -> Option<String> {
        match self {
            Self::Character(value) => Some(value.to_ascii_lowercase()),
            _ => None,
        }
    }
}

pub fn handle_editor_shortcut(key: EditorKey, command: bool, shift: bool) -> bool {
    if key == EditorKey::F1 {
        bridge::toggle_guide();
        return true;
    }
    if key == EditorKey::Escape {
        if bridge::guide_open() {
            bridge::set_guide_open(false);
        } else {
            bridge::select_node(None);
        }
        return true;
    }
    if bridge::guide_open() {
        return false;
    }

    let character = key.character();
    if command && character.as_deref() == Some("z") {
        bridge::suppress_scene_shortcuts_for(250.0);
        if shift {
            bridge::redo();
        } else {
            bridge::undo();
        }
        return true;
    }
    if command && character.as_deref() == Some("y") {
        bridge::suppress_scene_shortcuts_for(250.0);
        bridge::redo();
        return true;
    }
    if bridge::WORKSPACE_MODE() == WorkspaceMode::Run {
        return false;
    }
    if command && character.as_deref() == Some("s") {
        bridge::suppress_scene_shortcuts_for(250.0);
        let _ = bridge::persist_now();
        return true;
    }
    if command && character.as_deref() == Some("c") {
        bridge::suppress_scene_shortcuts_for(250.0);
        bridge::copy_selection();
        return true;
    }
    if command && character.as_deref() == Some("v") {
        bridge::suppress_scene_shortcuts_for(250.0);
        if let Err(error) = bridge::paste_clipboard() {
            bridge::set_notice(NoticeLevel::Error, error);
        }
        crate::ui::common::focus_canvas();
        return true;
    }
    if command && character.as_deref() == Some("x") {
        bridge::suppress_scene_shortcuts_for(250.0);
        if let Err(error) = bridge::cut_selection() {
            bridge::set_notice(NoticeLevel::Error, error);
        }
        return true;
    }
    if command && character.as_deref() == Some("a") {
        bridge::suppress_scene_shortcuts_for(250.0);
        bridge::select_all_nodes();
        return true;
    }
    if command && character.as_deref() == Some("d") {
        bridge::suppress_scene_shortcuts_for(250.0);
        if let Err(error) = bridge::duplicate_selection() {
            bridge::set_notice(NoticeLevel::Error, error);
        }
        crate::ui::common::focus_canvas();
        return true;
    }
    if matches!(key, EditorKey::Delete | EditorKey::Backspace) {
        bridge::suppress_scene_shortcuts_for(250.0);
        if let Err(error) = bridge::delete_selection() {
            bridge::set_notice(NoticeLevel::Error, error);
        }
        crate::ui::common::focus_canvas();
        return true;
    }
    false
}
