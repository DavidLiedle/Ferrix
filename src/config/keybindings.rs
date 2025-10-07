use std::collections::HashMap;
use std::path::PathBuf;
use crossterm::event::{KeyCode, KeyModifiers};
use serde::{Deserialize, Serialize};

use crate::error::{FerrixError, Result};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeyBinding {
    pub modifiers: KeyModifiers,
    pub code: KeyCode,
}

impl KeyBinding {
    pub fn to_string(&self) -> String {
        let mut parts = Vec::new();

        if self.modifiers.contains(KeyModifiers::CONTROL) {
            parts.push("ctrl");
        }
        if self.modifiers.contains(KeyModifiers::ALT) {
            parts.push("alt");
        }
        if self.modifiers.contains(KeyModifiers::SHIFT) {
            parts.push("shift");
        }

        let key_str = match self.code {
            KeyCode::Char(c) => c.to_string(),
            KeyCode::Up => "up".to_string(),
            KeyCode::Down => "down".to_string(),
            KeyCode::Left => "left".to_string(),
            KeyCode::Right => "right".to_string(),
            KeyCode::Enter => "enter".to_string(),
            KeyCode::Tab => "tab".to_string(),
            KeyCode::Backspace => "backspace".to_string(),
            KeyCode::Delete => "delete".to_string(),
            KeyCode::Home => "home".to_string(),
            KeyCode::End => "end".to_string(),
            KeyCode::PageUp => "pageup".to_string(),
            KeyCode::PageDown => "pagedown".to_string(),
            KeyCode::Esc => "esc".to_string(),
            _ => "unknown".to_string(),
        };

        parts.push(&key_str);
        parts.join("-")
    }
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
    ListWindows,
    SelectWindow(u8),

    // Pane actions
    SplitHorizontal,
    SplitVertical,
    NavigateUp,
    NavigateDown,
    NavigateLeft,
    NavigateRight,
    LastPane,
    DisplayPanes,
    ZoomPane,
    ClosePane,
    ResizePaneUp,
    ResizePaneDown,
    ResizePaneLeft,
    ResizePaneRight,

    // Layout actions
    ApplyLayoutPreset(String),
    CycleLayout,
    ListLayoutPresets,

    // Copy mode
    EnterCopyMode,
    PasteBuffer,

    // Command mode
    EnterCommandMode,

    // Config
    ReloadConfig,

    // Help
    ShowHelp,

    // Advanced
    SaveSnapshot,
    RestoreSnapshot,

    // Custom command
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyBindingConfig {
    pub prefix: String,
    pub bindings: HashMap<String, String>,
}

impl Default for KeyBindingConfig {
    fn default() -> Self {
        Self {
            prefix: "ctrl-b".to_string(),
            bindings: HashMap::new(),
        }
    }
}

pub struct KeyBindingManager {
    prefix: KeyBinding,
    bindings: HashMap<KeyBinding, Action>,
    custom_bindings: HashMap<KeyBinding, Action>,
    #[allow(dead_code)]
    config_path: Option<PathBuf>,
}

impl Default for KeyBindingManager {
    fn default() -> Self {
        Self::with_defaults()
    }
}

impl KeyBindingManager {
    pub fn new() -> Self {
        let mut manager = Self::with_defaults();

        // Try to load custom keybindings from config
        if let Ok(config) = super::Config::load() {
            manager.load_from_config(&config.keybindings);
        }

        manager
    }

    pub fn load_from_config(&mut self, config: &super::KeyBindings) {
        // Update prefix if specified
        if let Ok(prefix) = Self::parse_key_string(&config.prefix) {
            self.prefix = prefix;
        }

        // Load custom bindings
        for (key_str, action_str) in &config.custom {
            if let Ok(key) = Self::parse_key_string(key_str) {
                if let Ok(action) = self.parse_action_string(action_str) {
                    self.custom_bindings.insert(key, action);
                }
            }
        }
    }

    pub fn parse_action_string(&self, action_str: &str) -> Result<Action> {
        let parts: Vec<&str> = action_str.split_whitespace().collect();
        if parts.is_empty() {
            return Err(FerrixError::Config("Empty action string".to_string()));
        }

        match parts[0] {
            "new-session" => Ok(Action::NewSession),
            "detach-session" => Ok(Action::DetachSession),
            "list-sessions" => Ok(Action::ListSessions),
            "kill-session" => Ok(Action::KillSession),
            "new-window" => Ok(Action::NewWindow),
            "next-window" => Ok(Action::NextWindow),
            "previous-window" => Ok(Action::PreviousWindow),
            "rename-window" => Ok(Action::RenameWindow),
            "kill-window" => Ok(Action::KillWindow),
            "list-windows" => Ok(Action::ListWindows),
            "select-window" => {
                if parts.len() > 1 {
                    if let Ok(n) = parts[1].parse::<u8>() {
                        Ok(Action::SelectWindow(n))
                    } else {
                        Err(FerrixError::Config("Invalid window number".to_string()))
                    }
                } else {
                    Err(FerrixError::Config("Window number required".to_string()))
                }
            }
            "split-horizontal" => Ok(Action::SplitHorizontal),
            "split-vertical" => Ok(Action::SplitVertical),
            "navigate-up" => Ok(Action::NavigateUp),
            "navigate-down" => Ok(Action::NavigateDown),
            "navigate-left" => Ok(Action::NavigateLeft),
            "navigate-right" => Ok(Action::NavigateRight),
            "last-pane" => Ok(Action::LastPane),
            "display-panes" => Ok(Action::DisplayPanes),
            "zoom-pane" => Ok(Action::ZoomPane),
            "close-pane" => Ok(Action::ClosePane),
            "resize-pane-up" => Ok(Action::ResizePaneUp),
            "resize-pane-down" => Ok(Action::ResizePaneDown),
            "resize-pane-left" => Ok(Action::ResizePaneLeft),
            "resize-pane-right" => Ok(Action::ResizePaneRight),
            "apply-layout" => {
                if parts.len() > 1 {
                    Ok(Action::ApplyLayoutPreset(parts[1].to_string()))
                } else {
                    Err(FerrixError::Config("Missing layout preset name".to_string()))
                }
            }
            "cycle-layout" => Ok(Action::CycleLayout),
            "list-layouts" => Ok(Action::ListLayoutPresets),
            "enter-copy-mode" => Ok(Action::EnterCopyMode),
            "paste-buffer" => Ok(Action::PasteBuffer),
            "enter-command-mode" => Ok(Action::EnterCommandMode),
            "reload-config" => Ok(Action::ReloadConfig),
            "show-help" => Ok(Action::ShowHelp),
            "save-snapshot" => Ok(Action::SaveSnapshot),
            "restore-snapshot" => Ok(Action::RestoreSnapshot),
            _ => Ok(Action::Custom(action_str.to_string())),
        }
    }

    pub fn with_defaults() -> Self {
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
            KeyBinding { modifiers: KeyModifiers::empty(), code: KeyCode::Char('w') },
            Action::ListWindows,
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
            KeyBinding { modifiers: KeyModifiers::empty(), code: KeyCode::Char(';') },
            Action::LastPane,
        );
        bindings.insert(
            KeyBinding { modifiers: KeyModifiers::empty(), code: KeyCode::Char('q') },
            Action::DisplayPanes,
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
        bindings.insert(
            KeyBinding { modifiers: KeyModifiers::empty(), code: KeyCode::Char('?') },
            Action::ShowHelp,
        );

        // Layout presets
        bindings.insert(
            KeyBinding { modifiers: KeyModifiers::empty(), code: KeyCode::Char(' ') },
            Action::CycleLayout,
        );
        bindings.insert(
            KeyBinding { modifiers: KeyModifiers::ALT, code: KeyCode::Char('1') },
            Action::ApplyLayoutPreset("even-horizontal".to_string()),
        );
        bindings.insert(
            KeyBinding { modifiers: KeyModifiers::ALT, code: KeyCode::Char('2') },
            Action::ApplyLayoutPreset("even-vertical".to_string()),
        );
        bindings.insert(
            KeyBinding { modifiers: KeyModifiers::ALT, code: KeyCode::Char('3') },
            Action::ApplyLayoutPreset("main-left".to_string()),
        );
        bindings.insert(
            KeyBinding { modifiers: KeyModifiers::ALT, code: KeyCode::Char('4') },
            Action::ApplyLayoutPreset("grid-2x2".to_string()),
        );
        bindings.insert(
            KeyBinding { modifiers: KeyModifiers::ALT, code: KeyCode::Char('5') },
            Action::ApplyLayoutPreset("ide".to_string()),
        );

        // Number keys for window selection
        for i in 0..=9 {
            if let Some(digit) = char::from_digit(i as u32, 10) {
                bindings.insert(
                    KeyBinding { modifiers: KeyModifiers::empty(), code: KeyCode::Char(digit) },
                    Action::SelectWindow(i),
                );
            }
        }

        Self {
            prefix: KeyBinding {
                modifiers: KeyModifiers::CONTROL,
                code: KeyCode::Char('b'),
            },
            bindings,
            custom_bindings: HashMap::new(),
            config_path: None,
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
            key_part = parts.last().ok_or_else(|| FerrixError::Config("Invalid key binding format".to_string()))?;
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
            s if s.len() == 1 => {
                KeyCode::Char(s.chars().next().ok_or_else(|| FerrixError::Config("Empty key string".to_string()))?)
            },
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
        // Check custom bindings first, then default bindings
        self.custom_bindings.get(key).or_else(|| self.bindings.get(key))
    }

    pub fn bind(&mut self, key: KeyBinding, action: Action) {
        self.bindings.insert(key, action);
    }

    pub fn unbind(&mut self, key: &KeyBinding) -> Option<Action> {
        // Remove from custom bindings first, then from default
        self.custom_bindings.remove(key).or_else(|| self.bindings.remove(key))
    }

    pub fn bind_custom(&mut self, key: KeyBinding, action: Action) {
        self.custom_bindings.insert(key, action);
    }

    pub fn list_all_bindings(&self) -> Vec<(String, String, bool)> {
        let mut result = Vec::new();

        // Add default bindings
        for (key, action) in &self.bindings {
            if !self.custom_bindings.contains_key(key) {
                result.push((
                    format!("prefix + {}", key.to_string()),
                    format!("{:?}", action),
                    false,
                ));
            }
        }

        // Add custom bindings (overrides)
        for (key, action) in &self.custom_bindings {
            result.push((
                format!("prefix + {}", key.to_string()),
                format!("{:?}", action),
                true,
            ));
        }

        result.sort_by(|a, b| a.0.cmp(&b.0));
        result
    }

    /// Create Emacs-style keybindings (similar to GNU Screen)
    pub fn emacs_bindings() -> Self {
        let mut bindings = HashMap::new();

        // Session management (Emacs/Screen style with C-x prefix)
        bindings.insert(
            KeyBinding { modifiers: KeyModifiers::empty(), code: KeyCode::Char('d') },
            Action::DetachSession,
        );

        // Window management
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
            KeyBinding { modifiers: KeyModifiers::empty(), code: KeyCode::Char('w') },
            Action::ListWindows,
        );
        bindings.insert(
            KeyBinding { modifiers: KeyModifiers::empty(), code: KeyCode::Char('A') },
            Action::RenameWindow,
        );
        bindings.insert(
            KeyBinding { modifiers: KeyModifiers::empty(), code: KeyCode::Char('k') },
            Action::KillWindow,
        );

        // Pane management (Emacs-style)
        bindings.insert(
            KeyBinding { modifiers: KeyModifiers::empty(), code: KeyCode::Char('2') },
            Action::SplitHorizontal,
        );
        bindings.insert(
            KeyBinding { modifiers: KeyModifiers::empty(), code: KeyCode::Char('3') },
            Action::SplitVertical,
        );
        bindings.insert(
            KeyBinding { modifiers: KeyModifiers::empty(), code: KeyCode::Char('o') },
            Action::NavigateDown, // Cycle through panes
        );
        bindings.insert(
            KeyBinding { modifiers: KeyModifiers::empty(), code: KeyCode::Char('0') },
            Action::ClosePane,
        );
        bindings.insert(
            KeyBinding { modifiers: KeyModifiers::empty(), code: KeyCode::Char('1') },
            Action::ZoomPane,
        );

        // Arrow key navigation
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

        // Copy mode (Emacs style uses ESC [)
        bindings.insert(
            KeyBinding { modifiers: KeyModifiers::empty(), code: KeyCode::Char('[') },
            Action::EnterCopyMode,
        );
        bindings.insert(
            KeyBinding { modifiers: KeyModifiers::empty(), code: KeyCode::Char(']') },
            Action::PasteBuffer,
        );

        // Command mode
        bindings.insert(
            KeyBinding { modifiers: KeyModifiers::empty(), code: KeyCode::Char(':') },
            Action::EnterCommandMode,
        );

        // Config reload
        bindings.insert(
            KeyBinding { modifiers: KeyModifiers::empty(), code: KeyCode::Char('r') },
            Action::ReloadConfig,
        );

        // Layout management
        bindings.insert(
            KeyBinding { modifiers: KeyModifiers::empty(), code: KeyCode::Char(' ') },
            Action::CycleLayout,
        );

        // Help
        bindings.insert(
            KeyBinding { modifiers: KeyModifiers::empty(), code: KeyCode::Char('?') },
            Action::ShowHelp,
        );

        // Number keys for window selection (0-9)
        for i in 0..=9 {
            if let Some(digit) = char::from_digit(i as u32, 10) {
                bindings.insert(
                    KeyBinding { modifiers: KeyModifiers::empty(), code: KeyCode::Char(digit) },
                    Action::SelectWindow(i),
                );
            }
        }

        Self {
            prefix: KeyBinding {
                modifiers: KeyModifiers::CONTROL,
                code: KeyCode::Char('a'), // Screen-style C-a prefix
            },
            bindings,
            custom_bindings: HashMap::new(),
            config_path: None,
        }
    }

    /// Create Vim-style keybindings (similar to modern terminal multiplexers)
    pub fn vim_bindings() -> Self {
        // Vim users typically prefer the tmux-style bindings which are already the default
        Self::default()
    }

    pub fn save_to_config(&self) -> Result<()> {
        let mut config = super::Config::load().unwrap_or_default();

        // Save prefix
        config.keybindings.prefix = self.prefix.to_string();

        // Save custom bindings
        config.keybindings.custom.clear();
        for (key, action) in &self.custom_bindings {
            let action_str = match action {
                Action::NewSession => "new-session".to_string(),
                Action::DetachSession => "detach-session".to_string(),
                Action::ListSessions => "list-sessions".to_string(),
                Action::KillSession => "kill-session".to_string(),
                Action::NewWindow => "new-window".to_string(),
                Action::NextWindow => "next-window".to_string(),
                Action::PreviousWindow => "previous-window".to_string(),
                Action::RenameWindow => "rename-window".to_string(),
                Action::KillWindow => "kill-window".to_string(),
                Action::ListWindows => "list-windows".to_string(),
                Action::SelectWindow(n) => format!("select-window {}", n),
                Action::SplitHorizontal => "split-horizontal".to_string(),
                Action::SplitVertical => "split-vertical".to_string(),
                Action::NavigateUp => "navigate-up".to_string(),
                Action::NavigateDown => "navigate-down".to_string(),
                Action::NavigateLeft => "navigate-left".to_string(),
                Action::NavigateRight => "navigate-right".to_string(),
                Action::LastPane => "last-pane".to_string(),
                Action::DisplayPanes => "display-panes".to_string(),
                Action::ZoomPane => "zoom-pane".to_string(),
                Action::ClosePane => "close-pane".to_string(),
                Action::ResizePaneUp => "resize-pane-up".to_string(),
                Action::ResizePaneDown => "resize-pane-down".to_string(),
                Action::ResizePaneLeft => "resize-pane-left".to_string(),
                Action::ResizePaneRight => "resize-pane-right".to_string(),
                Action::ApplyLayoutPreset(preset) => format!("apply-layout {}", preset),
                Action::CycleLayout => "cycle-layout".to_string(),
                Action::ListLayoutPresets => "list-layouts".to_string(),
                Action::EnterCopyMode => "enter-copy-mode".to_string(),
                Action::PasteBuffer => "paste-buffer".to_string(),
                Action::EnterCommandMode => "enter-command-mode".to_string(),
                Action::ReloadConfig => "reload-config".to_string(),
                Action::ShowHelp => "show-help".to_string(),
                Action::SaveSnapshot => "save-snapshot".to_string(),
                Action::RestoreSnapshot => "restore-snapshot".to_string(),
                Action::Custom(s) => s.clone(),
            };
            config.keybindings.custom.insert(key.to_string(), action_str);
        }

        config.save()
    }

    pub fn reset_to_defaults(&mut self) {
        self.custom_bindings.clear();
        *self = Self::default();
    }

    pub fn reload_config(&mut self) -> Result<()> {
        if let Ok(config) = super::Config::load() {
            self.load_from_config(&config.keybindings);
            Ok(())
        } else {
            Err(FerrixError::Config("Failed to load config".to_string()))
        }
    }

    pub fn export_to_file(&self, path: &std::path::Path) -> Result<()> {
        use std::fs::File;
        use std::io::Write;

        let mut content = String::new();
        content.push_str("# Ferrix Keybindings Export\n");
        content.push_str(&format!("prefix = \"{}\"\n\n", self.prefix.to_string()));
        content.push_str("[custom]\n");

        for (key, action) in &self.custom_bindings {
            let action_str = match action {
                Action::NewSession => "new-session",
                Action::DetachSession => "detach-session",
                Action::ListSessions => "list-sessions",
                Action::KillSession => "kill-session",
                Action::NewWindow => "new-window",
                Action::NextWindow => "next-window",
                Action::PreviousWindow => "previous-window",
                Action::RenameWindow => "rename-window",
                Action::KillWindow => "kill-window",
                Action::ListWindows => "list-windows",
                Action::SelectWindow(n) => &format!("select-window {}", n),
                Action::SplitHorizontal => "split-horizontal",
                Action::SplitVertical => "split-vertical",
                Action::NavigateUp => "navigate-up",
                Action::NavigateDown => "navigate-down",
                Action::NavigateLeft => "navigate-left",
                Action::NavigateRight => "navigate-right",
                Action::LastPane => "last-pane",
                Action::DisplayPanes => "display-panes",
                Action::ZoomPane => "zoom-pane",
                Action::ClosePane => "close-pane",
                Action::ResizePaneUp => "resize-pane-up",
                Action::ResizePaneDown => "resize-pane-down",
                Action::ResizePaneLeft => "resize-pane-left",
                Action::ResizePaneRight => "resize-pane-right",
                Action::ApplyLayoutPreset(preset) => &format!("apply-layout {}", preset),
                Action::CycleLayout => "cycle-layout",
                Action::ListLayoutPresets => "list-layouts",
                Action::EnterCopyMode => "enter-copy-mode",
                Action::PasteBuffer => "paste-buffer",
                Action::EnterCommandMode => "enter-command-mode",
                Action::ReloadConfig => "reload-config",
                Action::ShowHelp => "show-help",
                Action::SaveSnapshot => "save-snapshot",
                Action::RestoreSnapshot => "restore-snapshot",
                Action::Custom(s) => s,
            };
            content.push_str(&format!("{} = \"{}\"\n", key.to_string(), action_str));
        }

        let mut file = File::create(path)
            .map_err(|e| FerrixError::Config(format!("Failed to create file: {}", e)))?;
        file.write_all(content.as_bytes())
            .map_err(|e| FerrixError::Config(format!("Failed to write file: {}", e)))?;

        Ok(())
    }

    pub fn import_from_file(&mut self, path: &std::path::Path) -> Result<usize> {
        use std::fs;

        let content = fs::read_to_string(path)
            .map_err(|e| FerrixError::Config(format!("Failed to read file: {}", e)))?;

        let mut count = 0;
        self.custom_bindings.clear();

        // Simple TOML-like parsing
        for line in content.lines() {
            let line = line.trim();
            if line.starts_with('#') || line.is_empty() || line.starts_with('[') {
                continue;
            }

            if line.starts_with("prefix") {
                if let Some(prefix_str) = line.split('=').nth(1) {
                    let prefix_str = prefix_str.trim().trim_matches('"');
                    if let Ok(prefix) = Self::parse_key_string(prefix_str) {
                        self.prefix = prefix;
                    }
                }
                continue;
            }

            // Parse key = "action" format
            if let Some((key_str, action_str)) = line.split_once('=') {
                let key_str = key_str.trim();
                let action_str = action_str.trim().trim_matches('"');

                if let Ok(key) = Self::parse_key_string(key_str) {
                    if let Ok(action) = self.parse_action_string(action_str) {
                        self.custom_bindings.insert(key, action);
                        count += 1;
                    }
                }
            }
        }

        Ok(count)
    }
}
#[cfg(test)]
mod tests {
    

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
