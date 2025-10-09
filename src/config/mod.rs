pub mod parser;
pub mod keybindings;
pub mod ferrixrc;
pub mod loader;
pub mod hot_reload;
pub mod session_config;
pub mod limits;
// #[cfg(test)]
// mod tests;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::fs;
use directories::ProjectDirs;

use crate::error::{FerrixError, Result};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[derive(Default)]
pub struct Config {
    #[serde(default)]
    pub general: GeneralConfig,
    #[serde(default)]
    pub keybindings: KeyBindings,
    #[serde(default)]
    pub status_bar: StatusBarConfig,
    #[serde(default)]
    pub colors: ColorConfig,
    #[serde(default)]
    pub windows: WindowConfig,
    #[serde(default)]
    pub panes: PaneConfig,
    #[serde(default)]
    pub copy_mode: CopyModeConfig,
    #[serde(default)]
    pub plugins: PluginConfig,
    #[serde(default)]
    pub advanced: AdvancedConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GeneralConfig {
    pub default_shell: String,
    pub escape_key: String,
    pub mouse: bool,
    pub clipboard: bool,
    pub term: String,
    pub scrollback_lines: usize,
    pub automatic_rename: bool,
    pub display_panes_time: u64,
    pub auto_detach_on_exit: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct KeyBindings {
    pub prefix: String,
    pub custom: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StatusBarConfig {
    pub enabled: bool,
    pub position: StatusBarPosition,
    pub left: String,
    pub center: String,
    pub right: String,
    pub refresh_rate: u64,
    pub height: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum StatusBarPosition {
    Top,
    Bottom,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ColorConfig {
    pub background: String,
    pub foreground: String,
    pub pane_border: String,
    pub pane_active_border: String,
    pub status_bg: String,
    pub status_fg: String,
    pub status_current_bg: String,
    pub status_current_fg: String,
    pub copy_mode_bg: String,
    pub copy_mode_fg: String,
    pub copy_mode_selection_bg: String,
    pub copy_mode_selection_fg: String,
    pub message_bg: String,
    pub message_fg: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WindowConfig {
    pub renumber: bool,
    pub base_index: usize,
    pub aggressive_resize: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PaneConfig {
    pub base_index: usize,
    pub display_borders: bool,
    pub border_style: BorderStyle,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum BorderStyle {
    Single,
    Double,
    Heavy,
    Rounded,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CopyModeConfig {
    pub mode: CopyModeStyle,
    pub use_system_clipboard: bool,
    pub exit_after_selection: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum CopyModeStyle {
    Vi,
    Emacs,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PluginConfig {
    pub enabled: bool,
    pub directory: String,
    pub autoload: bool,
    pub plugins: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AdvancedConfig {
    pub auto_save_session: bool,
    pub auto_save_interval: u64,
    pub enable_crash_recovery: bool,
    pub recovery_backup_dir: String,
    pub allow_remote: bool,
    pub remote_port: u16,
    pub remote_encryption: bool,
    pub gpu_acceleration: bool,
    pub log_level: String,
    pub log_file: String,
}


impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            default_shell: std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string()),
            escape_key: "ctrl-b".to_string(),
            mouse: true,
            clipboard: true,
            term: "xterm-256color".to_string(),
            scrollback_lines: 10000,
            automatic_rename: true,
            display_panes_time: 2000,
            auto_detach_on_exit: true, // Default to enabled for better UX
        }
    }
}

impl Default for KeyBindings {
    fn default() -> Self {
        Self {
            prefix: "ctrl-b".to_string(),
            custom: HashMap::new(),
        }
    }
}

impl Default for StatusBarConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            position: StatusBarPosition::Bottom,
            left: "[#{session_name}] #{windows} #{git_branch}".to_string(),
            center: "#{cpu} #{memory} #{battery}".to_string(),
            right: "#{user}@#{host} #{time}".to_string(),
            refresh_rate: 1000,
            height: 1,
        }
    }
}

impl Default for ColorConfig {
    fn default() -> Self {
        Self {
            background: "#1e1e1e".to_string(),
            foreground: "#d4d4d4".to_string(),
            pane_border: "#444444".to_string(),
            pane_active_border: "#569cd6".to_string(),
            status_bg: "darkgreen".to_string(),
            status_fg: "black".to_string(),
            status_current_bg: "#569cd6".to_string(),
            status_current_fg: "#ffffff".to_string(),
            copy_mode_bg: "#3c3c3c".to_string(),
            copy_mode_fg: "#ffffff".to_string(),
            copy_mode_selection_bg: "#264f78".to_string(),
            copy_mode_selection_fg: "#ffffff".to_string(),
            message_bg: "#569cd6".to_string(),
            message_fg: "#ffffff".to_string(),
        }
    }
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            renumber: true,
            base_index: 0,
            aggressive_resize: false,
        }
    }
}

impl Default for PaneConfig {
    fn default() -> Self {
        Self {
            base_index: 0,
            display_borders: true,
            border_style: BorderStyle::Single,
        }
    }
}

impl Default for CopyModeConfig {
    fn default() -> Self {
        Self {
            mode: CopyModeStyle::Vi,
            use_system_clipboard: true,
            exit_after_selection: false,
        }
    }
}

impl Default for PluginConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            directory: "~/.ferrix/plugins".to_string(),
            autoload: true,
            plugins: Vec::new(),
        }
    }
}

impl Default for AdvancedConfig {
    fn default() -> Self {
        Self {
            auto_save_session: true,
            auto_save_interval: 300,
            enable_crash_recovery: true,
            recovery_backup_dir: "~/.ferrix/recovery".to_string(),
            allow_remote: false,
            remote_port: 7755,
            remote_encryption: true,
            gpu_acceleration: false,
            log_level: "info".to_string(),
            log_file: "~/.ferrix/ferrix.log".to_string(),
        }
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        let config_path = Self::get_config_path()?;

        if config_path.exists() {
            let contents = fs::read_to_string(&config_path)
                .map_err(|e| FerrixError::Config(format!("Failed to read config file: {}", e)))?;

            toml::from_str(&contents)
                .map_err(|e| FerrixError::Config(format!("Failed to parse config file: {}", e)))
        } else {
            Ok(Config::default())
        }
    }

    pub fn load_from_path(path: &Path) -> Result<Self> {
        let contents = fs::read_to_string(path)
            .map_err(|e| FerrixError::Config(format!("Failed to read config file: {}", e)))?;

        toml::from_str(&contents)
            .map_err(|e| FerrixError::Config(format!("Failed to parse config file: {}", e)))
    }

    pub fn save(&self) -> Result<()> {
        let config_path = Self::get_config_path()?;

        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| FerrixError::Config(format!("Failed to create config directory: {}", e)))?;
        }

        let contents = toml::to_string_pretty(self)
            .map_err(|e| FerrixError::Config(format!("Failed to serialize config: {}", e)))?;

        fs::write(&config_path, contents)
            .map_err(|e| FerrixError::Config(format!("Failed to write config file: {}", e)))?;

        Ok(())
    }

    pub fn get_config_path() -> Result<PathBuf> {
        if let Ok(path) = std::env::var("FERRIX_CONFIG") {
            return Ok(PathBuf::from(path));
        }

        if let Some(proj_dirs) = ProjectDirs::from("com", "ferrix", "Ferrix") {
            let config_dir = proj_dirs.config_dir();
            Ok(config_dir.join("config.toml"))
        } else {
            Ok(PathBuf::from("~/.config/ferrix/config.toml"))
        }
    }

    pub fn expand_tilde(path: &str) -> PathBuf {
        if let Some(stripped) = path.strip_prefix("~/") {
            if let Some(home) = dirs::home_dir() {
                return home.join(stripped);
            }
        }
        PathBuf::from(path)
    }
}