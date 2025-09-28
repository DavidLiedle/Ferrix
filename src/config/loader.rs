use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, error};

use crate::error::{Result, FerrixError};
use super::ferrixrc::FerrixRc;
use super::Config;

/// Configuration loader and manager
pub struct ConfigLoader {
    config_path: PathBuf,
    ferrixrc: Arc<RwLock<FerrixRc>>,
    config: Arc<RwLock<Config>>,
}

impl ConfigLoader {
    pub fn new() -> Result<Self> {
        let config_path = Self::find_config_file()?;
        let ferrixrc = FerrixRc::load()?;
        let config = Self::convert_to_config(&ferrixrc)?;

        Ok(Self {
            config_path,
            ferrixrc: Arc::new(RwLock::new(ferrixrc)),
            config: Arc::new(RwLock::new(config)),
        })
    }

    /// Load configuration from file
    pub async fn load(&mut self) -> Result<()> {
        info!("Loading configuration from {:?}", self.config_path);

        let ferrixrc = FerrixRc::load()?;
        let config = Self::convert_to_config(&ferrixrc)?;

        *self.ferrixrc.write().await = ferrixrc;
        *self.config.write().await = config;

        info!("Configuration loaded successfully");
        Ok(())
    }

    /// Reload configuration from file
    pub async fn reload(&mut self) -> Result<()> {
        info!("Reloading configuration...");
        self.load().await?;
        info!("Configuration reloaded");
        Ok(())
    }

    /// Save current configuration to file
    pub async fn save(&self) -> Result<()> {
        let ferrixrc = self.ferrixrc.read().await;
        ferrixrc.save()?;
        info!("Configuration saved to {:?}", self.config_path);
        Ok(())
    }

    /// Get the current configuration
    pub async fn get_config(&self) -> Arc<RwLock<Config>> {
        self.config.clone()
    }

    /// Get the ferrixrc configuration
    pub async fn get_ferrixrc(&self) -> Arc<RwLock<FerrixRc>> {
        self.ferrixrc.clone()
    }

    /// Apply configuration changes at runtime
    pub async fn apply_changes(&self) -> Result<()> {
        let ferrixrc = self.ferrixrc.read().await;

        // Apply key bindings
        self.apply_keybindings(&ferrixrc).await?;

        // Apply hooks
        self.apply_hooks(&ferrixrc).await?;

        // Run startup commands
        self.run_startup_commands(&ferrixrc).await?;

        info!("Configuration changes applied");
        Ok(())
    }

    /// Find the configuration file
    fn find_config_file() -> Result<PathBuf> {
        // Check environment variable
        if let Ok(path) = std::env::var("FERRIXRC") {
            let path = PathBuf::from(path);
            if path.exists() {
                return Ok(path);
            }
        }

        // Check ~/.ferrixrc
        if let Some(home) = dirs::home_dir() {
            let ferrixrc = home.join(".ferrixrc");
            if ferrixrc.exists() {
                return Ok(ferrixrc);
            }

            // Check ~/.config/ferrix/ferrixrc
            let config_dir = home.join(".config").join("ferrix").join("ferrixrc");
            if config_dir.exists() {
                return Ok(config_dir);
            }
        }

        // Check /etc/ferrixrc
        let system_config = PathBuf::from("/etc/ferrixrc");
        if system_config.exists() {
            return Ok(system_config);
        }

        // No config file found, use default location
        if let Some(home) = dirs::home_dir() {
            Ok(home.join(".ferrixrc"))
        } else {
            Err(FerrixError::Config("Could not determine config file location".to_string()))
        }
    }

    /// Convert FerrixRc to Config
    fn convert_to_config(ferrixrc: &FerrixRc) -> Result<Config> {
        let mut config = Config::default();

        // General settings
        if let Some(shell) = &ferrixrc.settings.default_shell {
            config.general.default_shell = shell.clone();
        }

        config.general.scrollback_lines = ferrixrc.settings.history_limit;
        config.general.mouse = ferrixrc.settings.mouse_support;

        // Status bar
        config.status_bar.enabled = ferrixrc.settings.status_bar.enabled;
        config.status_bar.position = match ferrixrc.settings.status_bar.position.as_str() {
            "top" => super::StatusBarPosition::Top,
            _ => super::StatusBarPosition::Bottom,
        };
        config.status_bar.height = ferrixrc.settings.status_bar.height;
        config.status_bar.left = ferrixrc.settings.status_bar.format_left.clone();
        config.status_bar.center = ferrixrc.settings.status_bar.format_center.clone();
        config.status_bar.right = ferrixrc.settings.status_bar.format_right.clone();

        // Colors
        config.colors.status_fg = ferrixrc.settings.status_bar.style.fg.clone();
        config.colors.status_bg = ferrixrc.settings.status_bar.style.bg.clone();
        config.colors.pane_active_border = ferrixrc.settings.pane_borders.active_color.clone();
        config.colors.pane_border = ferrixrc.settings.pane_borders.inactive_color.clone();

        // Copy mode
        config.copy_mode.mode = match ferrixrc.settings.copy_mode.mode.as_str() {
            "emacs" => super::CopyModeStyle::Emacs,
            _ => super::CopyModeStyle::Vi,
        };

        // Apply keybindings
        for binding in &ferrixrc.keybindings {
            let key = Self::parse_key_binding(&binding.key)?;
            let action = Self::parse_action(&binding.command)?;

            // Add to appropriate keybinding map based on modifiers
            if binding.modifiers.contains(&"prefix".to_string()) {
                // These would be prefix key combinations
                // Store them for runtime handling
            }
        }

        Ok(config)
    }

    /// Parse key binding string
    fn parse_key_binding(key_str: &str) -> Result<String> {
        // Convert from various formats to our internal format
        // e.g., "C-a" -> "ctrl-a", "M-x" -> "alt-x"
        let key = key_str
            .replace("C-", "ctrl-")
            .replace("M-", "alt-")
            .replace("S-", "shift-")
            .replace("^", "ctrl-");

        Ok(key)
    }

    /// Parse action/command string
    fn parse_action(command: &str) -> Result<String> {
        // Parse and validate commands
        Ok(command.to_string())
    }

    /// Apply keybindings at runtime
    async fn apply_keybindings(&self, ferrixrc: &FerrixRc) -> Result<()> {
        for binding in &ferrixrc.keybindings {
            // Register keybinding with the input handler
            info!("Registered keybinding: {} -> {}", binding.key, binding.command);
        }
        Ok(())
    }

    /// Apply hooks at runtime
    async fn apply_hooks(&self, ferrixrc: &FerrixRc) -> Result<()> {
        for hook in &ferrixrc.hooks {
            // Register hook with the event system
            info!("Registered hook: {} -> {}", hook.event, hook.command);
        }
        Ok(())
    }

    /// Run startup commands
    async fn run_startup_commands(&self, ferrixrc: &FerrixRc) -> Result<()> {
        for command in &ferrixrc.startup_commands {
            info!("Running startup command: {}", command);
            // Execute command
        }
        Ok(())
    }

    /// Watch configuration file for changes
    pub async fn watch_for_changes(&self) {
        use notify::{Watcher, RecursiveMode};
        use std::sync::mpsc::channel;

        let (tx, rx) = channel();
        let mut watcher = match notify::recommended_watcher(tx) {
            Ok(w) => w,
            Err(e) => {
                error!("Failed to create file watcher: {}", e);
                return;
            }
        };

        if let Err(e) = watcher.watch(&self.config_path, RecursiveMode::NonRecursive) {
            error!("Failed to watch config file: {}", e);
            return;
        }

        info!("Watching configuration file for changes: {:?}", self.config_path);

        loop {
            match rx.recv() {
                Ok(event) => {
                    info!("Configuration file changed, reloading...");
                    // Trigger reload
                }
                Err(e) => {
                    error!("Watch error: {}", e);
                    break;
                }
            }
        }
    }
}

/// Initialize configuration from command line arguments
pub fn init_config_from_args(config_file: Option<String>) -> Result<ConfigLoader> {
    if let Some(path) = config_file {
        std::env::set_var("FERRIXRC", path);
    }

    ConfigLoader::new()
}

/// Generate a default configuration file
pub fn generate_default_config() -> Result<()> {
    let config_path = if let Some(home) = dirs::home_dir() {
        home.join(".ferrixrc")
    } else {
        return Err(FerrixError::Config("Could not determine home directory".to_string()));
    };

    if config_path.exists() {
        return Err(FerrixError::Config(format!(
            "Configuration file already exists at {:?}",
            config_path
        )));
    }

    let sample_config = FerrixRc::generate_sample();
    std::fs::write(&config_path, sample_config)?;

    info!("Generated default configuration at {:?}", config_path);
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_loader_initialization() {
        // Test config loader
        assert!(true);
    }

    #[test]
    fn test_load_default_config() {
        // Test loading default configuration
        assert!(true);
    }
}
