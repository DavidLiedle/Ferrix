//! Configuration command handlers
//!
//! Handles configuration-related commands:
//! - reload: Reload configuration (hot-reload)
//! - generate: Generate default configuration file
//! - validate: Validate configuration file

use crate::config::Config;
use crate::config::ferrixrc::FerrixRc;
use crate::error::Result;
use std::path::PathBuf;

/// Handle the `reload-config` command
///
/// # Note
/// Config hot reload is automatically handled when attached to a session.
/// This command just displays informational message about how to reload.
pub fn handle_reload() {
    println!("Note: Config hot reload is automatically handled when attached to a session.");
    println!("Use Ctrl-b r to reload config while in a session, or restart the client.");
}

/// Handle the `generate-config` command - generate default configuration file
///
/// # Arguments
/// * `force` - If true, overwrite existing configuration file
/// * `output` - Optional custom output path (uses default if None)
///
/// # Behavior
/// - Creates config directory if it doesn't exist
/// - Generates default configuration
/// - Refuses to overwrite unless --force is specified
///
/// # Example
/// ```ignore
/// handle_generate(false, None)?; // Generate to default location
/// handle_generate(true, Some("custom.toml"))?; // Force to custom path
/// ```
pub fn handle_generate(force: bool, output: Option<String>) -> Result<()> {
    let config_path = if let Some(path) = output {
        PathBuf::from(path)
    } else {
        Config::get_config_path()?
    };

    if config_path.exists() && !force {
        eprintln!("Configuration file already exists at {:?}", config_path);
        eprintln!("Use --force to overwrite");
        return Ok(());
    }

    // Create config directory if it doesn't exist
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Generate and write config to the specified path
    let default_config = Config::default();
    let contents = toml::to_string_pretty(&default_config)
        .map_err(|e| crate::error::FerrixError::Config(format!("Failed to serialize config: {}", e)))?;

    std::fs::write(&config_path, contents)
        .map_err(|e| crate::error::FerrixError::Config(format!("Failed to write config file: {}", e)))?;

    println!("Generated configuration file at {:?}", config_path);
    println!("Edit this file to customize Ferrix behavior");
    println!("Key bindings can be customized in the [keybindings] section");

    Ok(())
}

/// Handle the `validate-config` command - validate configuration file
///
/// # Arguments
/// * `path` - Optional path to config file to validate
///
/// # Behavior
/// - If path is None, checks FERRIXRC env var, then ~/.ferrixrc
/// - Loads and validates the configuration
/// - Displays summary of configuration contents if valid
/// - Shows error details if invalid
///
/// # Output (valid config)
/// ```text
/// ✓ Configuration is valid
///   - N keybindings defined
///   - N hooks registered
///   - N aliases configured
///   - N startup commands
///   - N plugins configured
/// ```
pub fn handle_validate(path: Option<String>) -> Result<()> {
    let config_path = if let Some(p) = path {
        PathBuf::from(p)
    } else if let Ok(p) = std::env::var("FERRIXRC") {
        PathBuf::from(p)
    } else if let Some(home) = dirs::home_dir() {
        home.join(".ferrixrc")
    } else {
        eprintln!("Could not determine config file location");
        return Ok(());
    };

    if !config_path.exists() {
        eprintln!("Configuration file not found at {:?}", config_path);
        return Ok(());
    }

    println!("Validating configuration file: {:?}", config_path);

    match FerrixRc::load() {
        Ok(config) => {
            println!("✓ Configuration is valid");
            println!("  - {} keybindings defined", config.keybindings.len());
            println!("  - {} hooks registered", config.hooks.len());
            println!("  - {} aliases configured", config.aliases.len());
            println!("  - {} startup commands", config.startup_commands.len());
            println!("  - {} plugins configured", config.settings.plugins.len());
        }
        Err(e) => {
            eprintln!("✗ Configuration validation failed:");
            eprintln!("  {}", e);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reload_message() {
        // Just ensure it doesn't panic
        handle_reload();
    }

    #[test]
    fn test_generate_to_temp() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("ferrix.toml");

        // Generate config
        let result = handle_generate(false, Some(config_path.to_str().unwrap().to_string()));
        assert!(result.is_ok());

        // Verify file was created
        assert!(config_path.exists());

        // Try again without force - should fail gracefully
        let result = handle_generate(false, Some(config_path.to_str().unwrap().to_string()));
        assert!(result.is_ok()); // Doesn't error, just prints message
    }

    #[test]
    fn test_validate_nonexistent() {
        // Validating nonexistent file should not panic
        let result = handle_validate(Some("/nonexistent/path/config.toml".to_string()));
        assert!(result.is_ok());
    }
}
