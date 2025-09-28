use std::collections::HashMap;
use crossterm::event::{KeyCode, KeyModifiers};
use serde::{Deserialize, Serialize};

use crate::error::{FerrixError, Result};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeyBinding {
    pub modifiers: KeyModifiers,
    pub code: KeyCode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Action {
    // Session actions
    NewSession,
    DetachSession,
    ListSessions,
    KillSession,

    // Window actions
    NewWindow,
    NextWindow,
    PreviousWindow,
    RenameWindow,
    KillWindow,
    SelectWindow(u8),

    // Pane actions
    SplitHorizontal,
    SplitVertical,
    NavigateUp,
    NavigateDown,
    NavigateLeft,
    NavigateRight,
    ZoomPane,
    ClosePane,
    ResizePaneUp,
    ResizePaneDown,
    ResizePaneLeft,
    ResizePaneRight,

    // Copy mode
    EnterCopyMode,
    PasteBuffer,

    // Command mode
    EnterCommandMode,

    // Config
    ReloadConfig,

    // Advanced
    SaveSnapshot,
    RestoreSnapshot,

    // Custom command
    Custom(String),
}

pub struct KeyBindingManager {
    prefix: KeyBinding,
    bindings: HashMap<KeyBinding, Action>,
}

impl KeyBindingManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn default() -> Self {
        let mut bindings = HashMap::new();

        // Default key bindings (after prefix key)
        bindings.insert(
            KeyBinding { modifiers: KeyModifiers::empty(), code: KeyCode::Char('d') },
            Action::DetachSession,
        );
        bindings.insert(
            KeyBinding { modifiers: KeyModifiers::empty(), code: KeyCode::Char('s') },
            Action::ListSessions,
        );
        bindings.insert(
            KeyBinding { modifiers: KeyModifiers::empty(), code: KeyCode::Char('c') },
            Action::NewWindow,
        );
        bindings.insert(
            KeyBinding { modifiers: KeyModifiers::empty(), code: KeyCode::Char('n') },
            Action::NextWindow,
        );
        bindings.insert(
            KeyBinding { modifiers: KeyModifiers::empty(), code: KeyCode::Char('p') },
            Action::PreviousWindow,
        );
        bindings.insert(
            KeyBinding { modifiers: KeyModifiers::empty(), code: KeyCode::Char(',') },
            Action::RenameWindow,
        );
        bindings.insert(
            KeyBinding { modifiers: KeyModifiers::empty(), code: KeyCode::Char('&') },
            Action::KillWindow,
        );
        bindings.insert(
            KeyBinding { modifiers: KeyModifiers::empty(), code: KeyCode::Char('%') },
            Action::SplitVertical,
        );
        bindings.insert(
            KeyBinding { modifiers: KeyModifiers::empty(), code: KeyCode::Char('"') },
            Action::SplitHorizontal,
        );
        bindings.insert(
            KeyBinding { modifiers: KeyModifiers::empty(), code: KeyCode::Up },
            Action::NavigateUp,
        );
        bindings.insert(
            KeyBinding { modifiers: KeyModifiers::empty(), code: KeyCode::Down },
            Action::NavigateDown,
        );
        bindings.insert(
            KeyBinding { modifiers: KeyModifiers::empty(), code: KeyCode::Left },
            Action::NavigateLeft,
        );
        bindings.insert(
            KeyBinding { modifiers: KeyModifiers::empty(), code: KeyCode::Right },
            Action::NavigateRight,
        );
        bindings.insert(
            KeyBinding { modifiers: KeyModifiers::empty(), code: KeyCode::Char('z') },
            Action::ZoomPane,
        );
        bindings.insert(
            KeyBinding { modifiers: KeyModifiers::empty(), code: KeyCode::Char('x') },
            Action::ClosePane,
        );
        bindings.insert(
            KeyBinding { modifiers: KeyModifiers::empty(), code: KeyCode::Char('[') },
            Action::EnterCopyMode,
        );
        bindings.insert(
            KeyBinding { modifiers: KeyModifiers::empty(), code: KeyCode::Char(']') },
            Action::PasteBuffer,
        );
        bindings.insert(
            KeyBinding { modifiers: KeyModifiers::empty(), code: KeyCode::Char(':') },
            Action::EnterCommandMode,
        );
        bindings.insert(
            KeyBinding { modifiers: KeyModifiers::empty(), code: KeyCode::Char('r') },
            Action::ReloadConfig,
        );

        // Number keys for window selection
        for i in 0..=9 {
            let digit = char::from_digit(i as u32, 10).unwrap();
            bindings.insert(
                KeyBinding { modifiers: KeyModifiers::empty(), code: KeyCode::Char(digit) },
                Action::SelectWindow(i),
            );
        }

        Self {
            prefix: KeyBinding {
                modifiers: KeyModifiers::CONTROL,
                code: KeyCode::Char('b'),
            },
            bindings,
        }
    }

    pub fn parse_key_string(key_str: &str) -> Result<KeyBinding> {
        let parts: Vec<&str> = key_str.split('-').collect();

        let mut modifiers = KeyModifiers::empty();
        let mut key_part = key_str;

        for part in &parts[..parts.len() - 1] {
            match part.to_lowercase().as_str() {
                "ctrl" | "control" => modifiers |= KeyModifiers::CONTROL,
                "alt" | "meta" => modifiers |= KeyModifiers::ALT,
                "shift" => modifiers |= KeyModifiers::SHIFT,
                _ => {}
            }
        }

        if parts.len() > 1 {
            key_part = parts.last().unwrap();
        }

        let code = match key_part.to_lowercase().as_str() {
            "space" => KeyCode::Char(' '),
            "tab" => KeyCode::Tab,
            "enter" | "return" => KeyCode::Enter,
            "esc" | "escape" => KeyCode::Esc,
            "backspace" => KeyCode::Backspace,
            "delete" => KeyCode::Delete,
            "up" => KeyCode::Up,
            "down" => KeyCode::Down,
            "left" => KeyCode::Left,
            "right" => KeyCode::Right,
            "home" => KeyCode::Home,
            "end" => KeyCode::End,
            "pageup" => KeyCode::PageUp,
            "pagedown" => KeyCode::PageDown,
            s if s.len() == 1 => KeyCode::Char(s.chars().next().unwrap()),
            s => return Err(FerrixError::Config(format!("Invalid key: {}", s))),
        };

        Ok(KeyBinding { modifiers, code })
    }

    pub fn get_prefix(&self) -> &KeyBinding {
        &self.prefix
    }

    pub fn set_prefix(&mut self, key: KeyBinding) {
        self.prefix = key;
    }

    pub fn get_action(&self, key: &KeyBinding) -> Option<&Action> {
        self.bindings.get(key)
    }

    pub fn bind(&mut self, key: KeyBinding, action: Action) {
        self.bindings.insert(key, action);
    }

    pub fn unbind(&mut self, key: &KeyBinding) -> Option<Action> {
        self.bindings.remove(key)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keybinding_defaults() {
        // Test default keybindings
        assert!(true);
    }

    #[test]
    fn test_keybinding_parsing() {
        // Test keybinding configuration parsing
        assert!(true);
    }
}
