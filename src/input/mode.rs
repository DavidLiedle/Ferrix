use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use crate::error::Result;

/// Configuration for input modes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModeConfig {
    /// Current active mode
    pub active_mode: ModeType,

    /// Mode-specific settings
    pub vim: VimConfig,
    pub emacs: EmacsConfig,

    /// Global mode settings
    pub show_mode_in_status: bool,
    pub mode_indicator_position: IndicatorPosition,
    pub chord_timeout_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ModeType {
    Default,
    Vim,
    Emacs,
    Hybrid,  // Allows both vim and emacs commands
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum IndicatorPosition {
    StatusLeft,
    StatusRight,
    StatusCenter,
    BottomRight,
    TopRight,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VimConfig {
    pub enabled: bool,
    pub start_in_insert_mode: bool,
    pub use_system_clipboard: bool,
    pub show_line_numbers: bool,
    pub relative_line_numbers: bool,
    pub escape_keys: Vec<String>,
    pub leader_key: String,
    pub timeout_len: u64,
    pub custom_mappings: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmacsConfig {
    pub enabled: bool,
    pub meta_key: String,
    pub kill_ring_size: usize,
    pub use_system_clipboard: bool,
    pub electric_pair_mode: bool,
    pub show_matching_parens: bool,
    pub custom_bindings: HashMap<String, String>,
}

impl Default for ModeConfig {
    fn default() -> Self {
        Self {
            active_mode: ModeType::Default,
            vim: VimConfig::default(),
            emacs: EmacsConfig::default(),
            show_mode_in_status: true,
            mode_indicator_position: IndicatorPosition::StatusLeft,
            chord_timeout_ms: 500,
        }
    }
}

impl Default for VimConfig {
    fn default() -> Self {
        let mut custom_mappings = HashMap::new();

        // Default vim-style leader mappings
        custom_mappings.insert("<leader>w".to_string(), ":w".to_string());
        custom_mappings.insert("<leader>q".to_string(), ":q".to_string());
        custom_mappings.insert("<leader>h".to_string(), ":split".to_string());
        custom_mappings.insert("<leader>v".to_string(), ":vsplit".to_string());

        Self {
            enabled: false,
            start_in_insert_mode: false,
            use_system_clipboard: true,
            show_line_numbers: true,
            relative_line_numbers: false,
            escape_keys: vec!["jj".to_string(), "jk".to_string()],
            leader_key: "Space".to_string(),
            timeout_len: 500,
            custom_mappings,
        }
    }
}

impl Default for EmacsConfig {
    fn default() -> Self {
        let mut custom_bindings = HashMap::new();

        // Default emacs-style bindings
        custom_bindings.insert("C-x C-s".to_string(), "save-buffer".to_string());
        custom_bindings.insert("C-x C-f".to_string(), "find-file".to_string());
        custom_bindings.insert("C-x b".to_string(), "switch-buffer".to_string());

        Self {
            enabled: false,
            meta_key: "Alt".to_string(),
            kill_ring_size: 30,
            use_system_clipboard: true,
            electric_pair_mode: true,
            show_matching_parens: true,
            custom_bindings,
        }
    }
}

/// Mode manager that handles switching between input modes
pub struct ModeManager {
    config: ModeConfig,
    current_mode: ModeType,
    mode_states: HashMap<ModeType, Box<dyn ModeState>>,
}

trait ModeState: Send + Sync {
    fn enter(&mut self);
    fn exit(&mut self);
    fn get_display_name(&self) -> String;
    fn get_bindings(&self) -> HashMap<String, String>;
}

struct DefaultMode;
struct VimMode {
    sub_mode: super::chord::InputMode,
}
struct EmacsMode;
struct HybridMode;

impl ModeState for DefaultMode {
    fn enter(&mut self) {}
    fn exit(&mut self) {}
    fn get_display_name(&self) -> String {
        "DEFAULT".to_string()
    }
    fn get_bindings(&self) -> HashMap<String, String> {
        HashMap::new()
    }
}

impl ModeState for VimMode {
    fn enter(&mut self) {
        self.sub_mode = super::chord::InputMode::Normal;
    }

    fn exit(&mut self) {}

    fn get_display_name(&self) -> String {
        match self.sub_mode {
            super::chord::InputMode::Normal => "NORMAL",
            super::chord::InputMode::Insert => "INSERT",
            super::chord::InputMode::Visual => "VISUAL",
            super::chord::InputMode::VisualLine => "V-LINE",
            super::chord::InputMode::VisualBlock => "V-BLOCK",
            super::chord::InputMode::Command => "COMMAND",
            super::chord::InputMode::Replace => "REPLACE",
            _ => "VIM",
        }.to_string()
    }

    fn get_bindings(&self) -> HashMap<String, String> {
        let mut bindings = HashMap::new();

        // Basic vim bindings
        bindings.insert("h".to_string(), "move-left".to_string());
        bindings.insert("j".to_string(), "move-down".to_string());
        bindings.insert("k".to_string(), "move-up".to_string());
        bindings.insert("l".to_string(), "move-right".to_string());
        bindings.insert("i".to_string(), "enter-insert-mode".to_string());
        bindings.insert("v".to_string(), "enter-visual-mode".to_string());
        bindings.insert(":".to_string(), "enter-command-mode".to_string());

        bindings
    }
}

impl ModeState for EmacsMode {
    fn enter(&mut self) {}
    fn exit(&mut self) {}

    fn get_display_name(&self) -> String {
        "EMACS".to_string()
    }

    fn get_bindings(&self) -> HashMap<String, String> {
        let mut bindings = HashMap::new();

        // Basic emacs bindings
        bindings.insert("C-f".to_string(), "forward-char".to_string());
        bindings.insert("C-b".to_string(), "backward-char".to_string());
        bindings.insert("C-n".to_string(), "next-line".to_string());
        bindings.insert("C-p".to_string(), "previous-line".to_string());
        bindings.insert("C-a".to_string(), "beginning-of-line".to_string());
        bindings.insert("C-e".to_string(), "end-of-line".to_string());

        bindings
    }
}

impl ModeState for HybridMode {
    fn enter(&mut self) {}
    fn exit(&mut self) {}

    fn get_display_name(&self) -> String {
        "HYBRID".to_string()
    }

    fn get_bindings(&self) -> HashMap<String, String> {
        let mut bindings = HashMap::new();

        // Combine both vim and emacs bindings
        // Vim movement
        bindings.insert("h".to_string(), "move-left".to_string());
        bindings.insert("j".to_string(), "move-down".to_string());

        // Emacs movement
        bindings.insert("C-f".to_string(), "forward-char".to_string());
        bindings.insert("C-b".to_string(), "backward-char".to_string());

        bindings
    }
}

impl ModeManager {
    pub fn new(config: ModeConfig) -> Self {
        let mut mode_states: HashMap<ModeType, Box<dyn ModeState>> = HashMap::new();

        mode_states.insert(ModeType::Default, Box::new(DefaultMode));
        mode_states.insert(ModeType::Vim, Box::new(VimMode {
            sub_mode: super::chord::InputMode::Normal,
        }));
        mode_states.insert(ModeType::Emacs, Box::new(EmacsMode));
        mode_states.insert(ModeType::Hybrid, Box::new(HybridMode));

        let current_mode = config.active_mode;

        Self {
            config,
            current_mode,
            mode_states,
        }
    }

    pub fn switch_mode(&mut self, mode: ModeType) -> Result<()> {
        // Exit current mode
        if let Some(state) = self.mode_states.get_mut(&self.current_mode) {
            state.exit();
        }

        // Enter new mode
        self.current_mode = mode;
        if let Some(state) = self.mode_states.get_mut(&mode) {
            state.enter();
        }

        self.config.active_mode = mode;
        Ok(())
    }

    pub fn get_current_mode(&self) -> ModeType {
        self.current_mode
    }

    pub fn get_mode_display(&self) -> String {
        self.mode_states
            .get(&self.current_mode)
            .map(|state| state.get_display_name())
            .unwrap_or_else(|| "UNKNOWN".to_string())
    }

    pub fn get_active_bindings(&self) -> HashMap<String, String> {
        self.mode_states
            .get(&self.current_mode)
            .map(|state| state.get_bindings())
            .unwrap_or_default()
    }

    pub fn update_config(&mut self, config: ModeConfig) {
        self.config = config;

        // If mode changed in config, switch to it
        if self.current_mode != self.config.active_mode {
            let _ = self.switch_mode(self.config.active_mode);
        }
    }
}

/// Helper to integrate with configuration system
pub fn load_mode_config() -> Result<ModeConfig> {
    // Try to load from config file
    if let Ok(config_str) = std::fs::read_to_string(config_path()) {
        if let Ok(config) = toml::from_str(&config_str) {
            return Ok(config);
        }
    }

    // Return default if no config found
    Ok(ModeConfig::default())
}

pub fn save_mode_config(config: &ModeConfig) -> Result<()> {
    let config_str = toml::to_string_pretty(config)
        .map_err(|e| crate::error::FerrixError::Config(format!("Failed to serialize mode config: {}", e)))?;

    // Ensure directory exists
    if let Some(parent) = config_path().parent() {
        std::fs::create_dir_all(parent)?;
    }

    std::fs::write(config_path(), config_str)?;
    Ok(())
}

fn config_path() -> std::path::PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("~/.config"))
        .join("ferrix")
        .join("modes.toml")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mode_manager() {
        let config = ModeConfig::default();
        let mut manager = ModeManager::new(config);

        // Test initial mode
        assert_eq!(manager.get_current_mode(), ModeType::Default);
        assert_eq!(manager.get_mode_display(), "DEFAULT");

        // Test switching to vim mode
        manager.switch_mode(ModeType::Vim).unwrap();
        assert_eq!(manager.get_current_mode(), ModeType::Vim);
        assert_eq!(manager.get_mode_display(), "NORMAL");

        // Test vim bindings are active
        let bindings = manager.get_active_bindings();
        assert!(bindings.contains_key("h"));
        assert_eq!(bindings.get("h").unwrap(), "move-left");
    }

    #[test]
    fn test_mode_config_defaults() {
        let config = ModeConfig::default();

        assert_eq!(config.active_mode, ModeType::Default);
        assert!(!config.vim.enabled);
        assert!(!config.emacs.enabled);
        assert_eq!(config.chord_timeout_ms, 500);
    }
}