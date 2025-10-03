use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{RwLock, mpsc};
use tokio::time::interval;
use notify::{Watcher, RecursiveMode, Event, EventKind};
use tracing::{info, warn, error, debug};

use crate::config::Config;
use crate::error::{Result, FerrixError};

/// Hot reload configuration manager
pub struct HotReloadManager {
    config_path: PathBuf,
    config: Arc<RwLock<Config>>,
    watcher_tx: mpsc::UnboundedSender<ConfigChangeEvent>,
    watcher_rx: Option<mpsc::UnboundedReceiver<ConfigChangeEvent>>,
    enabled: bool,
}

#[derive(Debug, Clone)]
pub enum ConfigChangeEvent {
    FileChanged(PathBuf),
    ReloadRequested,
    ValidationError(String),
}

impl HotReloadManager {
    pub fn new(config_path: PathBuf, config: Arc<RwLock<Config>>) -> Result<Self> {
        let (watcher_tx, watcher_rx) = mpsc::unbounded_channel();

        Ok(Self {
            config_path,
            config,
            watcher_tx,
            watcher_rx: Some(watcher_rx),
            enabled: true,
        })
    }

    /// Start watching for configuration changes
    pub async fn start_watching(&mut self) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }

        let config_path = self.config_path.clone();
        let tx = self.watcher_tx.clone();

        // Spawn file watcher task
        tokio::spawn(async move {
            if let Err(e) = watch_config_file(&config_path, tx).await {
                error!("Config watcher error: {}", e);
            }
        });

        // Take ownership of receiver
        if let Some(mut rx) = self.watcher_rx.take() {
            let config = self.config.clone();
            let config_path = self.config_path.clone();

            // Spawn event handler task
            tokio::spawn(async move {
                while let Some(event) = rx.recv().await {
                    match event {
                        ConfigChangeEvent::FileChanged(path) => {
                            info!("Configuration file changed: {:?}", path);
                            if let Err(e) = reload_config(&config_path, &config).await {
                                error!("Failed to reload configuration: {}", e);
                            }
                        }
                        ConfigChangeEvent::ReloadRequested => {
                            info!("Manual configuration reload requested");
                            if let Err(e) = reload_config(&config_path, &config).await {
                                error!("Failed to reload configuration: {}", e);
                            }
                        }
                        ConfigChangeEvent::ValidationError(msg) => {
                            warn!("Configuration validation error: {}", msg);
                        }
                    }
                }
            });
        }

        info!("Hot reload manager started for: {:?}", self.config_path);
        Ok(())
    }

    /// Stop watching for configuration changes
    pub fn stop_watching(&mut self) {
        self.enabled = false;
        info!("Hot reload manager stopped");
    }

    /// Manually trigger a configuration reload
    pub async fn reload(&self) -> Result<()> {
        self.watcher_tx.send(ConfigChangeEvent::ReloadRequested)
            .map_err(|e| FerrixError::Other(format!("Failed to send reload event: {}", e)))?;
        Ok(())
    }

    /// Check if hot reload is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Enable or disable hot reload
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if enabled {
            info!("Hot reload enabled");
        } else {
            info!("Hot reload disabled");
        }
    }
}

/// Watch a configuration file for changes
async fn watch_config_file(
    config_path: &Path,
    tx: mpsc::UnboundedSender<ConfigChangeEvent>,
) -> Result<()> {
    let (notify_tx, mut notify_rx) = mpsc::channel(100);

    // Create a watcher with debouncing
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
        if let Ok(event) = res {
            let _ = notify_tx.blocking_send(event);
        }
    }).map_err(|e| FerrixError::Other(format!("Failed to create file watcher: {}", e)))?;

    // Watch the config file
    watcher.watch(config_path, RecursiveMode::NonRecursive)
        .map_err(|e| FerrixError::Other(format!("Failed to watch config file: {}", e)))?;

    // Also watch the parent directory for file replacements (common with editors)
    if let Some(parent) = config_path.parent() {
        watcher.watch(parent, RecursiveMode::NonRecursive)
            .map_err(|e| FerrixError::Other(format!("Failed to watch config directory: {}", e)))?;
    }

    // Debounce timer - wait for changes to settle
    let mut debounce_timer = interval(Duration::from_millis(500));
    let mut pending_change = false;

    loop {
        tokio::select! {
            Some(event) = notify_rx.recv() => {
                match event.kind {
                    EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) => {
                        // Check if the event is for our config file
                        for path in &event.paths {
                            if path == config_path || path.file_name() == config_path.file_name() {
                                debug!("Config file change detected");
                                pending_change = true;
                            }
                        }
                    }
                    _ => {}
                }
            }
            _ = debounce_timer.tick() => {
                if pending_change {
                    pending_change = false;
                    let _ = tx.send(ConfigChangeEvent::FileChanged(config_path.to_path_buf()));
                }
            }
        }
    }
}

/// Reload configuration from file
async fn reload_config(
    config_path: &Path,
    config: &Arc<RwLock<Config>>,
) -> Result<()> {
    // Load new configuration
    let new_config = Config::load_from_path(config_path)?;

    // Validate the new configuration
    validate_config(&new_config)?;

    // Apply the new configuration
    let mut config_guard = config.write().await;

    // Preserve runtime state that shouldn't be reloaded
    // Currently, the entire config can be reloaded

    *config_guard = new_config;

    info!("Configuration reloaded successfully");

    // Trigger any necessary updates
    trigger_config_updates(&*config_guard).await;

    Ok(())
}

/// Validate configuration before applying
fn validate_config(config: &Config) -> Result<()> {
    // Validate scrollback limits
    if config.general.scrollback_lines == 0 {
        return Err(FerrixError::Config("Scrollback lines must be greater than 0".to_string()));
    }

    if config.general.scrollback_lines > 100000 {
        return Err(FerrixError::Config("Scrollback lines exceeds maximum (100000)".to_string()));
    }

    // Validate prefix key
    if config.keybindings.prefix.is_empty() {
        return Err(FerrixError::Config("Prefix key cannot be empty".to_string()));
    }

    // Validate log level
    match config.advanced.log_level.as_str() {
        "trace" | "debug" | "info" | "warn" | "error" => {}
        _ => return Err(FerrixError::Config(format!("Invalid log level: {}", config.advanced.log_level))),
    }

    Ok(())
}

/// Trigger updates after configuration reload
async fn trigger_config_updates(config: &Config) {
    // This would trigger various subsystems to update based on new config
    // For example:
    // - Update status bar visibility
    // - Update key bindings
    // - Update color scheme
    // - Resize scrollback buffers

    debug!("Triggering configuration updates");

    // These would be implemented as needed:
    // update_status_bar(config).await;
    // update_key_bindings(config).await;
    // update_theme(config).await;
}

/// Configuration diff for selective updates
pub struct ConfigDiff {
    pub theme_changed: bool,
    pub keybindings_changed: bool,
    pub status_changed: bool,
    pub scrollback_changed: bool,
    pub mouse_changed: bool,
    pub plugins_changed: bool,
}

impl ConfigDiff {
    pub fn calculate(old: &Config, new: &Config) -> Self {
        Self {
            theme_changed: old.colors != new.colors,
            keybindings_changed: old.keybindings.custom != new.keybindings.custom ||
                                old.keybindings.prefix != new.keybindings.prefix,
            status_changed: old.status_bar != new.status_bar,
            scrollback_changed: old.general.scrollback_lines != new.general.scrollback_lines,
            mouse_changed: old.general.mouse != new.general.mouse,
            plugins_changed: old.plugins != new.plugins,
        }
    }

    pub fn has_changes(&self) -> bool {
        self.theme_changed ||
        self.keybindings_changed ||
        self.status_changed ||
        self.scrollback_changed ||
        self.mouse_changed ||
        self.plugins_changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;
    use std::io::Write;

    #[tokio::test]
    async fn test_config_validation() {
        let mut config = Config::default();

        // Test valid config
        assert!(validate_config(&config).is_ok());

        // Test invalid scrollback
        config.general.scrollback_lines = 0;
        assert!(validate_config(&config).is_err());

        config.general.scrollback_lines = 200000;
        assert!(validate_config(&config).is_err());

        // Test invalid prefix key
        config.general.scrollback_lines = 1000;
        config.keybindings.prefix = String::new();
        assert!(validate_config(&config).is_err());

        // Test invalid log level
        config.keybindings.prefix = "ctrl-b".to_string();
        config.advanced.log_level = "invalid".to_string();
        assert!(validate_config(&config).is_err());
    }

    #[tokio::test]
    async fn test_config_diff() {
        let old = Config::default();
        let mut new = Config::default();

        // No changes
        let diff = ConfigDiff::calculate(&old, &new);
        assert!(!diff.has_changes());

        // Color change
        new.colors.background = "#000000".to_string();
        let diff = ConfigDiff::calculate(&old, &new);
        assert!(diff.theme_changed);
        assert!(diff.has_changes());

        // Multiple changes
        new.general.scrollback_lines = 5000;
        new.general.mouse = false;
        let diff = ConfigDiff::calculate(&old, &new);
        assert!(diff.theme_changed);
        assert!(diff.scrollback_changed);
        assert!(diff.mouse_changed);
    }

    #[tokio::test]
    async fn test_hot_reload_manager() {
        let temp_file = NamedTempFile::new().unwrap();
        let config_path = temp_file.path().to_path_buf();

        // Write initial config
        writeln!(temp_file.as_file(), "theme = \"default\"").unwrap();

        let config = Arc::new(RwLock::new(Config::default()));
        let mut manager = HotReloadManager::new(config_path, config.clone()).unwrap();

        assert!(manager.is_enabled());

        // Start watching
        manager.start_watching().await.unwrap();

        // Test manual reload
        manager.reload().await.unwrap();

        // Stop watching
        manager.stop_watching();
        assert!(!manager.is_enabled());
    }
}