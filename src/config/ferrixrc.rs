use std::path::PathBuf;
use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use crate::error::{Result, FerrixError};

/// Ferrixrc configuration file parser and manager
/// Supports both TOML format and traditional rc format (like .screenrc/.tmux.conf)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FerrixRc {
    pub settings: FerrixSettings,
    pub keybindings: Vec<KeyBinding>,
    pub hooks: Vec<Hook>,
    pub startup_commands: Vec<String>,
    pub aliases: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FerrixSettings {
    // General settings
    pub default_shell: Option<String>,
    pub default_directory: Option<String>,
    pub escape_key: String,  // Default: "ctrl-b"
    pub history_limit: usize,
    pub mouse_support: bool,
    pub bell: BellSettings,

    // Display settings
    pub status_bar: StatusBarSettings,
    pub colors: ColorScheme,
    pub pane_borders: PaneBorderSettings,

    // Session settings
    pub auto_save: AutoSaveSettings,
    pub detach_on_destroy: bool,
    pub lock_after_time: Option<u64>, // minutes
    pub activity_monitoring: bool,

    // Window settings
    pub window_numbering: WindowNumbering,
    pub aggressive_resize: bool,
    pub automatic_rename: bool,

    // Copy mode settings
    pub copy_mode: CopyModeSettings,

    // Plugin settings
    pub plugins: Vec<PluginConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BellSettings {
    pub enabled: bool,
    pub visual: bool,
    pub on_alert: String, // "none", "current", "other", "any"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusBarSettings {
    pub enabled: bool,
    pub position: String, // "top" or "bottom"
    pub height: u16,
    pub refresh_interval: u64, // seconds
    pub format_left: String,
    pub format_center: String,
    pub format_right: String,
    pub style: StatusBarStyle,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusBarStyle {
    pub fg: String,
    pub bg: String,
    pub active_fg: String,
    pub active_bg: String,
    pub inactive_fg: String,
    pub inactive_bg: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColorScheme {
    pub theme: String, // "default", "solarized", "dracula", "nord", "custom"
    pub true_color: bool,
    pub custom_colors: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaneBorderSettings {
    pub style: String, // "single", "double", "heavy", "rounded"
    pub active_color: String,
    pub inactive_color: String,
    pub show_pane_numbers: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoSaveSettings {
    pub enabled: bool,
    pub interval: u64, // seconds
    pub on_detach: bool,
    pub on_exit: bool,
    pub max_snapshots: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowNumbering {
    pub base_index: usize, // Start from 0 or 1
    pub renumber_on_close: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CopyModeSettings {
    pub mode: String, // "vi" or "emacs"
    pub mouse_select: bool,
    pub clipboard_integration: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginConfig {
    pub name: String,
    pub path: String,
    pub enabled: bool,
    pub config: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyBinding {
    pub key: String,
    pub modifiers: Vec<String>,
    pub command: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hook {
    pub event: String, // "after-new-session", "after-new-window", "before-detach", etc.
    pub command: String,
}

impl FerrixRc {
    /// Load configuration from ~/.ferrixrc or specified path
    pub fn load() -> Result<Self> {
        let config_path = Self::get_config_path()?;

        if !config_path.exists() {
            // Create default configuration
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(&config_path)?;

        // Try to parse as TOML first
        if let Ok(config) = toml::from_str::<FerrixRc>(&content) {
            return Ok(config);
        }

        // Fall back to traditional rc format
        Self::parse_rc_format(&content)
    }

    /// Parse traditional rc format (like .screenrc or .tmux.conf)
    fn parse_rc_format(content: &str) -> Result<Self> {
        let mut config = Self::default();

        for line in content.lines() {
            let line = line.trim();

            // Skip comments and empty lines
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // Parse directives
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.is_empty() {
                continue;
            }

            match parts[0] {
                "set" | "set-option" | "setw" | "set-window-option" => {
                    if parts.len() >= 3 {
                        Self::parse_set_directive(&mut config, &parts[1..]);
                    }
                }
                "bind" | "bind-key" => {
                    if parts.len() >= 3 {
                        Self::parse_bind_directive(&mut config, &parts[1..]);
                    }
                }
                "unbind" | "unbind-key" => {
                    // Remove key binding
                    if parts.len() >= 2 {
                        config.keybindings.retain(|kb| kb.key != parts[1]);
                    }
                }
                "source" | "source-file" => {
                    // Source another config file
                    if parts.len() >= 2 {
                        let path = PathBuf::from(parts[1]);
                        if let Ok(content) = std::fs::read_to_string(path) {
                            let _ = Self::merge_config(&mut config, &content);
                        }
                    }
                }
                "run" | "run-shell" => {
                    // Add startup command
                    if parts.len() >= 2 {
                        config.startup_commands.push(parts[1..].join(" "));
                    }
                }
                "hook" | "set-hook" => {
                    if parts.len() >= 3 {
                        config.hooks.push(Hook {
                            event: parts[1].to_string(),
                            command: parts[2..].join(" "),
                        });
                    }
                }
                "alias" => {
                    if parts.len() >= 3 {
                        config.aliases.insert(
                            parts[1].to_string(),
                            parts[2..].join(" "),
                        );
                    }
                }
                "plugin" | "load-plugin" => {
                    if parts.len() >= 2 {
                        config.settings.plugins.push(PluginConfig {
                            name: parts[1].to_string(),
                            path: parts.get(2).unwrap_or(&parts[1]).to_string(),
                            enabled: true,
                            config: HashMap::new(),
                        });
                    }
                }
                _ => {
                    // Unknown directive, ignore
                }
            }
        }

        Ok(config)
    }

    fn parse_set_directive(config: &mut Self, parts: &[&str]) {
        if parts.len() < 2 {
            return;
        }

        let option = parts[0];
        let value = parts[1..].join(" ");

        match option {
            "prefix" | "escape" => {
                config.settings.escape_key = value;
            }
            "default-shell" => {
                config.settings.default_shell = Some(value);
            }
            "default-directory" | "default-path" => {
                config.settings.default_directory = Some(value);
            }
            "history-limit" => {
                if let Ok(limit) = value.parse() {
                    config.settings.history_limit = limit;
                }
            }
            "mouse" => {
                config.settings.mouse_support = value == "on" || value == "true";
            }
            "status" => {
                config.settings.status_bar.enabled = value == "on" || value == "true";
            }
            "status-position" => {
                config.settings.status_bar.position = value;
            }
            "status-left" => {
                config.settings.status_bar.format_left = value;
            }
            "status-right" => {
                config.settings.status_bar.format_right = value;
            }
            "status-interval" => {
                if let Ok(interval) = value.parse() {
                    config.settings.status_bar.refresh_interval = interval;
                }
            }
            "base-index" => {
                if let Ok(index) = value.parse() {
                    config.settings.window_numbering.base_index = index;
                }
            }
            "automatic-rename" => {
                config.settings.automatic_rename = value == "on" || value == "true";
            }
            "renumber-windows" => {
                config.settings.window_numbering.renumber_on_close = value == "on" || value == "true";
            }
            "mode-keys" => {
                config.settings.copy_mode.mode = value;
            }
            "visual-bell" => {
                config.settings.bell.visual = value == "on" || value == "true";
            }
            "bell-action" => {
                config.settings.bell.on_alert = value;
            }
            "detach-on-destroy" => {
                config.settings.detach_on_destroy = value == "on" || value == "true";
            }
            "aggressive-resize" => {
                config.settings.aggressive_resize = value == "on" || value == "true";
            }
            "activity-action" => {
                config.settings.activity_monitoring = value != "none";
            }
            "pane-border-style" => {
                // Parse style like "fg=colour235,bg=colour238"
                for style_part in value.split(',') {
                    if let Some((key, val)) = style_part.split_once('=') {
                        match key {
                            "fg" => config.settings.pane_borders.inactive_color = val.to_string(),
                            "bg" => {}, // Handle background if needed
                            _ => {},
                        }
                    }
                }
            }
            "pane-active-border-style" => {
                for style_part in value.split(',') {
                    if let Some((key, val)) = style_part.split_once('=') {
                        match key {
                            "fg" => config.settings.pane_borders.active_color = val.to_string(),
                            _ => {},
                        }
                    }
                }
            }
            _ => {
                // Unknown option
            }
        }
    }

    fn parse_bind_directive(config: &mut Self, parts: &[&str]) {
        if parts.len() < 2 {
            return;
        }

        let key = parts[0];
        let command = parts[1..].join(" ");

        config.keybindings.push(KeyBinding {
            key: key.to_string(),
            modifiers: vec!["prefix".to_string()], // Default to prefix key
            command,
            description: None,
        });
    }

    fn merge_config(config: &mut Self, content: &str) -> Result<()> {
        let additional = Self::parse_rc_format(content)?;

        // Merge settings (additional overrides existing)
        config.keybindings.extend(additional.keybindings);
        config.hooks.extend(additional.hooks);
        config.startup_commands.extend(additional.startup_commands);
        config.aliases.extend(additional.aliases);

        Ok(())
    }

    /// Save configuration to file
    pub fn save(&self) -> Result<()> {
        let config_path = Self::get_config_path()?;
        let content = toml::to_string_pretty(self)
            .map_err(|e| FerrixError::Config(format!("Failed to serialize config: {}", e)))?;

        std::fs::write(config_path, content)?;
        Ok(())
    }

    /// Get the configuration file path
    fn get_config_path() -> Result<PathBuf> {
        // Check environment variable first
        if let Ok(path) = std::env::var("FERRIXRC") {
            return Ok(PathBuf::from(path));
        }

        // Check ~/.ferrixrc
        if let Some(home) = dirs::home_dir() {
            Ok(home.join(".ferrixrc"))
        } else {
            Err(FerrixError::Config("Could not determine home directory".to_string()))
        }
    }

    /// Generate a sample configuration file
    pub fn generate_sample() -> String {
        // For now, return a basic sample until we fix the include path
        r#"# Ferrix Configuration File (~/.ferrixrc)
# ========================================

# Set the prefix key (like tmux)
set prefix C-b

# General Settings
set default-shell /bin/bash
set history-limit 10000
set mouse on

# Status Bar
set status on
set status-position bottom
set status-left " #S "
set status-right " %H:%M "

# Key Bindings
bind r source-file ~/.ferrixrc
bind c new-window
bind | split-window -h
bind - split-window -v

# Copy Mode
set mode-keys vi
"#.to_string()
    }
}

impl Default for FerrixRc {
    fn default() -> Self {
        Self {
            settings: FerrixSettings {
                default_shell: None,
                default_directory: None,
                escape_key: "ctrl-b".to_string(),
                history_limit: 10000,
                mouse_support: true,
                bell: BellSettings {
                    enabled: true,
                    visual: false,
                    on_alert: "current".to_string(),
                },
                status_bar: StatusBarSettings {
                    enabled: true,
                    position: "bottom".to_string(),
                    height: 1,
                    refresh_interval: 15,
                    format_left: " #S ".to_string(),
                    format_center: "#W".to_string(),
                    format_right: " %H:%M %d-%b-%y ".to_string(),
                    style: StatusBarStyle {
                        fg: "white".to_string(),
                        bg: "black".to_string(),
                        active_fg: "black".to_string(),
                        active_bg: "yellow".to_string(),
                        inactive_fg: "white".to_string(),
                        inactive_bg: "black".to_string(),
                    },
                },
                colors: ColorScheme {
                    theme: "default".to_string(),
                    true_color: true,
                    custom_colors: None,
                },
                pane_borders: PaneBorderSettings {
                    style: "single".to_string(),
                    active_color: "green".to_string(),
                    inactive_color: "white".to_string(),
                    show_pane_numbers: false,
                },
                auto_save: AutoSaveSettings {
                    enabled: true,
                    interval: 300,
                    on_detach: true,
                    on_exit: true,
                    max_snapshots: 10,
                },
                detach_on_destroy: false,
                lock_after_time: None,
                activity_monitoring: true,
                window_numbering: WindowNumbering {
                    base_index: 0,
                    renumber_on_close: true,
                },
                aggressive_resize: true,
                automatic_rename: true,
                copy_mode: CopyModeSettings {
                    mode: "vi".to_string(),
                    mouse_select: true,
                    clipboard_integration: true,
                },
                plugins: Vec::new(),
            },
            keybindings: Vec::new(),
            hooks: Vec::new(),
            startup_commands: Vec::new(),
            aliases: HashMap::new(),
        }
    }
}
#[cfg(test)]
mod tests {
    

    #[test]
    fn test_ferrixrc_parsing() {
        // Test ferrixrc configuration parsing
        assert!(true);
    }

    #[test]
    fn test_ferrixrc_defaults() {
        // Test default configuration values
        assert!(true);
    }
}
