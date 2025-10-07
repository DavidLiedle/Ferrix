use std::path::{Path, PathBuf};
use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use crate::error::{Result, FerrixError};
use crate::protocol::SessionId;

/// Per-session configuration that overrides global settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    /// Session-specific general settings
    #[serde(skip_serializing_if = "Option::is_none")]
    pub general: Option<SessionGeneralConfig>,

    /// Session-specific status bar settings
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_bar: Option<SessionStatusBarConfig>,

    /// Session-specific window settings
    #[serde(skip_serializing_if = "Option::is_none")]
    pub windows: Option<SessionWindowConfig>,

    /// Session-specific pane settings
    #[serde(skip_serializing_if = "Option::is_none")]
    pub panes: Option<SessionPaneConfig>,

    /// Session-specific environment variables
    #[serde(default)]
    pub environment: HashMap<String, String>,

    /// Startup commands to run when session is created
    #[serde(default)]
    pub startup_commands: Vec<String>,

    /// Default layout preset for new windows
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_layout: Option<String>,

    /// Session-specific hooks
    #[serde(default)]
    pub hooks: SessionHooks,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionGeneralConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_shell: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub scrollback_lines: Option<usize>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub mouse: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub clipboard: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionStatusBarConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub left: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub center: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub right: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionWindowConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub automatic_rename: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub renumber_windows: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_index: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionPaneConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_index: Option<usize>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_panes_time: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SessionHooks {
    /// Commands to run after session creation
    #[serde(default)]
    pub after_session_create: Vec<String>,

    /// Commands to run before session destroy
    #[serde(default)]
    pub before_session_destroy: Vec<String>,

    /// Commands to run after window creation
    #[serde(default)]
    pub after_window_create: Vec<String>,

    /// Commands to run after pane creation
    #[serde(default)]
    pub after_pane_create: Vec<String>,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionConfig {
    /// Create a new empty session config
    pub fn new() -> Self {
        Self {
            general: None,
            status_bar: None,
            windows: None,
            panes: None,
            environment: HashMap::new(),
            startup_commands: Vec::new(),
            default_layout: None,
            hooks: SessionHooks::default(),
        }
    }

    /// Load session config from a file
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: SessionConfig = toml::from_str(&content)
            .map_err(|e| FerrixError::Config(format!("Failed to parse config: {}", e)))?;
        Ok(config)
    }

    /// Save session config to a file
    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let content = toml::to_string_pretty(self)
            .map_err(|e| FerrixError::Config(format!("Failed to serialize config: {}", e)))?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Merge this session config with a global config
    /// Session config takes precedence over global config
    pub fn merge_with_global(&self, global: &super::Config) -> super::Config {
        let mut merged = global.clone();

        // Merge general settings
        if let Some(ref session_general) = self.general {
            if let Some(shell) = &session_general.default_shell {
                merged.general.default_shell = shell.clone();
            }
            if let Some(scrollback) = session_general.scrollback_lines {
                merged.general.scrollback_lines = scrollback;
            }
            if let Some(mouse) = session_general.mouse {
                merged.general.mouse = mouse;
            }
            if let Some(clipboard) = session_general.clipboard {
                merged.general.clipboard = clipboard;
            }
        }

        // Merge status bar settings
        if let Some(ref session_status) = self.status_bar {
            if let Some(enabled) = session_status.enabled {
                merged.status_bar.enabled = enabled;
            }
            if let Some(ref left) = session_status.left {
                merged.status_bar.left = left.clone();
            }
            if let Some(ref center) = session_status.center {
                merged.status_bar.center = center.clone();
            }
            if let Some(ref right) = session_status.right {
                merged.status_bar.right = right.clone();
            }
        }

        // Merge window settings
        if let Some(ref session_windows) = self.windows {
            if let Some(renumber) = session_windows.renumber_windows {
                merged.windows.renumber = renumber;
            }
            if let Some(base_index) = session_windows.base_index {
                merged.windows.base_index = base_index;
            }
            // Note: automatic_rename would need to be added to WindowConfig if needed
        }

        // Merge pane settings
        if let Some(ref session_panes) = self.panes {
            if let Some(base_index) = session_panes.base_index {
                merged.panes.base_index = base_index;
            }
            // Note: display_panes_time would need to be added to PaneConfig if needed
        }

        merged
    }
}

/// Manager for session-specific configurations
pub struct SessionConfigManager {
    /// Cache of loaded session configs
    configs: HashMap<SessionId, SessionConfig>,

    /// Directory where session configs are stored
    config_dir: PathBuf,
}

impl SessionConfigManager {
    /// Create a new session config manager
    pub fn new() -> Result<Self> {
        let config_dir = dirs::config_dir()
            .ok_or_else(|| FerrixError::Config("Could not find config directory".to_string()))?
            .join("ferrix")
            .join("sessions");

        // Create the directory if it doesn't exist
        std::fs::create_dir_all(&config_dir)?;

        Ok(Self {
            configs: HashMap::new(),
            config_dir,
        })
    }

    /// Get the config path for a session
    fn get_config_path(&self, session_id: &SessionId) -> PathBuf {
        self.config_dir.join(format!("{}.toml", session_id.0))
    }

    /// Load config for a specific session
    pub fn load_session_config(&mut self, session_id: &SessionId) -> Option<&SessionConfig> {
        if !self.configs.contains_key(session_id) {
            let config_path = self.get_config_path(session_id);
            if config_path.exists() {
                if let Ok(config) = SessionConfig::load_from_file(&config_path) {
                    self.configs.insert(session_id.clone(), config);
                }
            }
        }

        self.configs.get(session_id)
    }

    /// Save config for a session
    pub fn save_session_config(&mut self, session_id: &SessionId, config: SessionConfig) -> Result<()> {
        let config_path = self.get_config_path(session_id);
        config.save_to_file(&config_path)?;
        self.configs.insert(session_id.clone(), config);
        Ok(())
    }

    /// Remove config for a session
    pub fn remove_session_config(&mut self, session_id: &SessionId) -> Result<()> {
        self.configs.remove(session_id);
        let config_path = self.get_config_path(session_id);
        if config_path.exists() {
            std::fs::remove_file(config_path)?;
        }
        Ok(())
    }

    /// List all available session configs
    pub fn list_session_configs(&self) -> Result<Vec<(SessionId, PathBuf)>> {
        let mut configs = Vec::new();

        if let Ok(entries) = std::fs::read_dir(&self.config_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("toml") {
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        if let Ok(uuid) = stem.parse::<uuid::Uuid>() {
                            configs.push((SessionId(uuid), path));
                        }
                    }
                }
            }
        }

        Ok(configs)
    }

    /// Apply session config to global config
    pub fn get_merged_config(&mut self, session_id: &SessionId, global: &super::Config) -> super::Config {
        if let Some(session_config) = self.load_session_config(session_id) {
            session_config.merge_with_global(global)
        } else {
            global.clone()
        }
    }
}

/// Template for creating new session configs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfigTemplate {
    pub name: String,
    pub description: String,
    pub config: SessionConfig,
}

impl SessionConfigTemplate {
    /// Create a development template
    pub fn development() -> Self {
        let mut config = SessionConfig::new();

        config.general = Some(SessionGeneralConfig {
            default_shell: Some("/bin/bash".to_string()),
            scrollback_lines: Some(50000),
            mouse: Some(true),
            clipboard: Some(true),
        });

        config.status_bar = Some(SessionStatusBarConfig {
            enabled: Some(true),
            left: Some("[{session}] {git_branch}".to_string()),
            center: Some("{cpu} {memory}".to_string()),
            right: Some("{time:%H:%M:%S}".to_string()),
        });

        config.environment.insert("EDITOR".to_string(), "vim".to_string());
        config.environment.insert("TERM".to_string(), "xterm-256color".to_string());

        config.default_layout = Some("ide".to_string());

        config.startup_commands = vec![
            "echo 'Development session started'".to_string(),
        ];

        Self {
            name: "Development".to_string(),
            description: "Configuration optimized for software development".to_string(),
            config,
        }
    }

    /// Create a remote/SSH template
    pub fn remote() -> Self {
        let mut config = SessionConfig::new();

        config.general = Some(SessionGeneralConfig {
            mouse: Some(false),
            clipboard: Some(false),
            scrollback_lines: Some(10000),
            default_shell: None,
        });

        config.status_bar = Some(SessionStatusBarConfig {
            enabled: Some(true),
            left: Some("[{session}@{host}]".to_string()),
            center: Some("{windows}".to_string()),
            right: Some("{network} {time:%H:%M}".to_string()),
        });

        config.environment.insert("TERM".to_string(), "screen-256color".to_string());

        Self {
            name: "Remote".to_string(),
            description: "Configuration for remote/SSH sessions".to_string(),
            config,
        }
    }

    /// Create a monitoring template
    pub fn monitoring() -> Self {
        let mut config = SessionConfig::new();

        config.status_bar = Some(SessionStatusBarConfig {
            enabled: Some(true),
            left: Some("[{session}]".to_string()),
            center: Some("{cpu} {memory} {disk} {load}".to_string()),
            right: Some("{uptime} {time:%H:%M:%S}".to_string()),
        });

        config.default_layout = Some("grid-2x2".to_string());

        config.startup_commands = vec![
            "htop".to_string(),
        ];

        Self {
            name: "Monitoring".to_string(),
            description: "Configuration for system monitoring".to_string(),
            config,
        }
    }

    /// Get all available templates
    pub fn all_templates() -> Vec<SessionConfigTemplate> {
        vec![
            Self::development(),
            Self::remote(),
            Self::monitoring(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    

    #[test]
    fn test_session_config_new() {
        let config = SessionConfig::new();
        assert!(config.general.is_none());
        assert!(config.status_bar.is_none());
        assert!(config.environment.is_empty());
    }

    #[test]
    fn test_merge_with_global() {
        let mut session_config = SessionConfig::new();
        session_config.general = Some(SessionGeneralConfig {
            default_shell: Some("/bin/zsh".to_string()),
            scrollback_lines: Some(20000),
            mouse: None,
            clipboard: None,
        });

        let global_config = super::super::Config::default();
        let merged = session_config.merge_with_global(&global_config);

        assert_eq!(merged.general.default_shell, "/bin/zsh");
        assert_eq!(merged.general.scrollback_lines, 20000);
    }

    #[test]
    fn test_templates() {
        let dev_template = SessionConfigTemplate::development();
        assert_eq!(dev_template.name, "Development");
        assert!(dev_template.config.general.is_some());

        let remote_template = SessionConfigTemplate::remote();
        assert_eq!(remote_template.name, "Remote");

        let monitoring_template = SessionConfigTemplate::monitoring();
        assert_eq!(monitoring_template.name, "Monitoring");
    }

    #[test]
    fn test_session_config_manager() {
        let manager = SessionConfigManager::new();
        assert!(manager.is_ok());
    }
}