#[cfg(test)]
mod config_tests {
    use super::*;
    use crate::error::Result;
    use crate::config::{Config};
    use crate::config::keybindings::KeyBindingManager;
    use crate::config::loader::ConfigLoader;
    use tempfile::{TempDir, NamedTempFile};
    use std::io::Write;
    use tokio::time::{sleep, Duration};

    #[tokio::test]
    async fn test_config_hot_reload() -> Result<()> {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("ferrix.toml");

        // Write initial config
        let initial_config = r#"
            [general]
            default_shell = "/bin/bash"
            mouse = true
            scrollback_lines = 1000
            escape_key = "C-a"
            term = "xterm-256color"
            clipboard = true
            automatic_rename = false
            display_panes_time = 1500
        "#;
        std::fs::write(&config_path, initial_config).unwrap();

        // Load initial config
        let mut config = Config::load_from_path(&config_path)?;
        assert_eq!(config.general.scrollback_lines, 1000);

        // Modify config file
        let updated_config = r#"
            [general]
            default_shell = "/bin/bash"
            mouse = true
            scrollback_lines = 2000
            escape_key = "C-a"
            term = "xterm-256color"
            clipboard = true
            automatic_rename = false
            display_panes_time = 1500
        "#;
        std::fs::write(&config_path, updated_config).unwrap();

        // Reload config
        config.reload_from_path(&config_path)?;
        assert_eq!(config.general.scrollback_lines, 2000);

        Ok(())
    }

    #[tokio::test]
    async fn test_key_binding_manager() -> Result<()> {
        let temp_dir = TempDir::new().unwrap();
        let mut key_manager = KeyBindingManager::new();

        // Test default bindings
        assert!(key_manager.has_binding("C-a c"));
        assert!(key_manager.has_binding("C-a \""));

        // Add custom binding
        key_manager.add_binding("C-a t".to_string(), "new-window".to_string());
        assert!(key_manager.has_binding("C-a t"));

        // Get action for binding
        let action = key_manager.get_action("C-a t");
        assert_eq!(action, Some("new-window".to_string()));

        Ok(())
    }

    #[tokio::test]
    async fn test_config_validation() -> Result<()> {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("invalid.toml");

        // Write invalid config (missing required field)
        let invalid_config = r#"
            [general]
            mouse = true
            # missing default_shell
        "#;
        std::fs::write(&config_path, invalid_config).unwrap();

        // Should fail to load
        let result = Config::load_from_path(&config_path);
        assert!(result.is_err());

        Ok(())
    }

    #[tokio::test]
    async fn test_config_defaults() -> Result<()> {
        // Test that default config can be created
        let config = Config::default();

        assert_eq!(config.general.default_shell, "/bin/bash");
        assert_eq!(config.general.mouse, false);
        assert_eq!(config.general.scrollback_lines, 2000);
        assert_eq!(config.general.escape_key, "C-a");

        Ok(())
    }

    #[tokio::test]
    async fn test_config_file_watching() -> Result<()> {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("watch_test.toml");

        // Write initial config
        let config_content = r#"
            [general]
            default_shell = "/bin/bash"
            mouse = false
            scrollback_lines = 1000
            escape_key = "C-a"
            term = "xterm-256color"
            clipboard = true
            automatic_rename = false
            display_panes_time = 1500
        "#;
        std::fs::write(&config_path, config_content).unwrap();

        // Create config loader with file watching
        let mut loader = ConfigLoader::new(config_path.clone()).await?;

        // Load initial config
        let config = loader.load().await?;
        assert!(!config.general.mouse);

        // Modify file
        let updated_content = r#"
            [general]
            default_shell = "/bin/bash"
            mouse = true
            scrollback_lines = 1000
            escape_key = "C-a"
            term = "xterm-256color"
            clipboard = true
            automatic_rename = false
            display_panes_time = 1500
        "#;
        std::fs::write(&config_path, updated_content).unwrap();

        // Give file system time to register change
        sleep(Duration::from_millis(100)).await;

        // Check if reload is triggered
        if let Ok(reloaded_config) = loader.load().await {
            assert!(reloaded_config.general.mouse);
        }

        Ok(())
    }

    #[test]
    fn test_key_sequence_parsing() -> Result<()> {
        let mut key_manager = KeyBindingManager::new();

        // Test various key sequence formats
        assert!(key_manager.parse_key_sequence("C-a").is_some());
        assert!(key_manager.parse_key_sequence("M-x").is_some());
        assert!(key_manager.parse_key_sequence("C-M-a").is_some());
        assert!(key_manager.parse_key_sequence("F1").is_some());
        assert!(key_manager.parse_key_sequence("Space").is_some());

        // Test invalid sequences
        assert!(key_manager.parse_key_sequence("Invalid-Key").is_none());
        assert!(key_manager.parse_key_sequence("").is_none());

        Ok(())
    }

    #[test]
    fn test_action_parsing() -> Result<()> {
        let key_manager = KeyBindingManager::new();

        // Test valid actions
        assert!(key_manager.parse_action("new-window").is_some());
        assert!(key_manager.parse_action("split-pane -v").is_some());
        assert!(key_manager.parse_action("select-pane -U").is_some());

        // Test custom actions
        assert!(key_manager.parse_action("custom-command").is_some());

        Ok(())
    }

    #[tokio::test]
    async fn test_config_backup_and_restore() -> Result<()> {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("backup_test.toml");
        let backup_path = temp_dir.path().join("backup_test.toml.backup");

        // Write initial config
        let config_content = r#"
            [general]
            default_shell = "/bin/bash"
            mouse = true
            scrollback_lines = 1000
            escape_key = "C-a"
            term = "xterm-256color"
            clipboard = true
            automatic_rename = false
            display_panes_time = 1500
        "#;
        std::fs::write(&config_path, config_content).unwrap();

        let config = Config::load_from_path(&config_path)?;

        // Create backup
        config.create_backup(&backup_path)?;
        assert!(backup_path.exists());

        // Modify original
        let bad_config = "invalid toml content";
        std::fs::write(&config_path, bad_config).unwrap();

        // Restore from backup
        config.restore_from_backup(&backup_path, &config_path)?;

        // Should be able to load again
        let restored_config = Config::load_from_path(&config_path)?;
        assert_eq!(restored_config.general.scrollback_lines, 1000);

        Ok(())
    }

    #[tokio::test]
    async fn test_config_merge() -> Result<()> {
        let base_config = Config::default();
        let mut user_config = Config::default();

        // Modify user config
        user_config.general.mouse = true;
        user_config.general.scrollback_lines = 5000;

        // Merge configs
        let merged = base_config.merge(&user_config)?;

        // Should have user values where specified
        assert!(merged.general.mouse);
        assert_eq!(merged.general.scrollback_lines, 5000);

        // Should have base values where not overridden
        assert_eq!(merged.general.default_shell, base_config.general.default_shell);

        Ok(())
    }

    #[test]
    fn test_config_serialization() -> Result<()> {
        let config = Config::default();

        // Test TOML serialization
        let toml_str = config.to_toml_string()?;
        assert!(toml_str.contains("[general]"));
        assert!(toml_str.contains("default_shell"));

        // Test deserialization
        let parsed_config = Config::from_toml_string(&toml_str)?;
        assert_eq!(parsed_config.general.default_shell, config.general.default_shell);

        Ok(())
    }

    #[tokio::test]
    async fn test_concurrent_config_access() -> Result<()> {
        use std::sync::Arc;
        use tokio::sync::RwLock;

        let config = Arc::new(RwLock::new(Config::default()));
        let mut handles = vec![];

        // Test concurrent reads
        for _ in 0..10 {
            let config = config.clone();
            let handle = tokio::spawn(async move {
                let config_read = config.read().await;
                assert_eq!(config_read.general.default_shell, "/bin/bash");
            });
            handles.push(handle);
        }

        // Test concurrent write
        let config_write = config.clone();
        let write_handle = tokio::spawn(async move {
            let mut config_write = config_write.write().await;
            config_write.general.mouse = true;
        });
        handles.push(write_handle);

        // Wait for all operations
        for handle in handles {
            handle.await.unwrap();
        }

        // Verify write succeeded
        let final_config = config.read().await;
        assert!(final_config.general.mouse);

        Ok(())
    }

    #[test]
    fn test_color_config() -> Result<()> {
        let mut config = Config::default();

        // Test color customization
        config.colors.status_bg = "blue".to_string();
        config.colors.status_fg = "white".to_string();

        assert_eq!(config.colors.status_bg, "blue");
        assert_eq!(config.colors.status_fg, "white");

        Ok(())
    }

    #[test]
    fn test_plugin_config() -> Result<()> {
        let mut config = Config::default();

        // Test plugin configuration
        config.plugins.enabled = true;
        config.plugins.auto_load = false;
        config.plugins.plugin_dir = "/custom/plugins".to_string();

        assert!(config.plugins.enabled);
        assert!(!config.plugins.auto_load);
        assert_eq!(config.plugins.plugin_dir, "/custom/plugins");

        Ok(())
    }
}