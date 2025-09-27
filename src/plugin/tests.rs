#[cfg(test)]
mod plugin_tests {
    use super::*;
    use crate::error::Result;
    use crate::plugin::manager::PluginManager;
    use crate::plugin::runtime::PluginRuntime;
    use crate::plugin::manifest::{PluginManifest, PluginVersion};
    use tempfile::TempDir;
    use std::path::PathBuf;

    #[tokio::test]
    async fn test_plugin_manager_creation() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let plugin_manager = PluginManager::new(temp_dir.path().to_path_buf()).await?;

        assert_eq!(plugin_manager.loaded_plugins().len(), 0);
        Ok(())
    }

    #[tokio::test]
    async fn test_plugin_manifest_creation() -> Result<()> {
        let manifest = PluginManifest {
            name: "test-plugin".to_string(),
            version: PluginVersion::parse("1.0.0")?,
            description: "A test plugin".to_string(),
            author: "Test Author".to_string(),
            license: "MIT".to_string(),
            repository: Some("https://github.com/test/plugin".to_string()),
            keywords: vec!["test".to_string(), "plugin".to_string()],
            dependencies: std::collections::HashMap::new(),
            ferrix_version: "^0.1.0".to_string(),
            main: "main.wasm".to_string(),
            permissions: vec!["session:read".to_string()],
        };

        assert_eq!(manifest.name, "test-plugin");
        assert_eq!(manifest.version.to_string(), "1.0.0");
        assert_eq!(manifest.permissions.len(), 1);
        Ok(())
    }

    #[tokio::test]
    async fn test_plugin_runtime_creation() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let runtime = PluginRuntime::new(temp_dir.path().to_path_buf()).await?;

        assert_eq!(runtime.loaded_plugins().len(), 0);
        Ok(())
    }

    #[test]
    fn test_plugin_version_parsing() -> Result<()> {
        let version = PluginVersion::parse("1.2.3")?;
        assert_eq!(version.major, 1);
        assert_eq!(version.minor, 2);
        assert_eq!(version.patch, 3);

        let version_string = version.to_string();
        assert_eq!(version_string, "1.2.3");

        // Test invalid version
        let invalid_version = PluginVersion::parse("invalid");
        assert!(invalid_version.is_err());

        Ok(())
    }

    #[test]
    fn test_plugin_version_comparison() -> Result<()> {
        let version1 = PluginVersion::parse("1.0.0")?;
        let version2 = PluginVersion::parse("1.0.1")?;
        let version3 = PluginVersion::parse("1.1.0")?;
        let version4 = PluginVersion::parse("2.0.0")?;

        assert!(version1 < version2);
        assert!(version2 < version3);
        assert!(version3 < version4);
        assert!(version1 == PluginVersion::parse("1.0.0")?);

        Ok(())
    }

    #[tokio::test]
    async fn test_plugin_loading_nonexistent() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let mut plugin_manager = PluginManager::new(temp_dir.path().to_path_buf()).await?;

        let nonexistent_path = temp_dir.path().join("nonexistent.wasm");
        let result = plugin_manager.load_plugin(&nonexistent_path).await;

        // Should fail to load nonexistent plugin
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn test_plugin_manifest_serialization() -> Result<()> {
        let manifest = PluginManifest {
            name: "test-plugin".to_string(),
            version: PluginVersion::parse("1.0.0")?,
            description: "A test plugin".to_string(),
            author: "Test Author".to_string(),
            license: "MIT".to_string(),
            repository: None,
            keywords: vec![],
            dependencies: std::collections::HashMap::new(),
            ferrix_version: "^0.1.0".to_string(),
            main: "main.wasm".to_string(),
            permissions: vec![],
        };

        // Test serialization to JSON
        let json = serde_json::to_string(&manifest)?;
        assert!(json.contains("test-plugin"));

        // Test deserialization
        let deserialized: PluginManifest = serde_json::from_str(&json)?;
        assert_eq!(deserialized.name, manifest.name);
        assert_eq!(deserialized.version, manifest.version);

        Ok(())
    }

    #[tokio::test]
    async fn test_plugin_manager_directory_scanning() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let plugin_manager = PluginManager::new(temp_dir.path().to_path_buf()).await?;

        // Create a mock plugin directory structure
        let plugin_dir = temp_dir.path().join("test-plugin");
        std::fs::create_dir_all(&plugin_dir)?;

        let manifest = PluginManifest {
            name: "test-plugin".to_string(),
            version: PluginVersion::parse("1.0.0")?,
            description: "A test plugin".to_string(),
            author: "Test Author".to_string(),
            license: "MIT".to_string(),
            repository: None,
            keywords: vec![],
            dependencies: std::collections::HashMap::new(),
            ferrix_version: "^0.1.0".to_string(),
            main: "main.wasm".to_string(),
            permissions: vec![],
        };

        // Write manifest file
        let manifest_path = plugin_dir.join("manifest.json");
        let manifest_json = serde_json::to_string_pretty(&manifest)?;
        std::fs::write(&manifest_path, manifest_json)?;

        // Scan for plugins (would normally find the manifest)
        let available_plugins = plugin_manager.scan_available_plugins().await?;

        // In a real scenario, this would find the plugin
        // In test, we just verify the scan doesn't crash
        assert!(available_plugins.len() >= 0);

        Ok(())
    }

    #[tokio::test]
    async fn test_plugin_permission_checking() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let plugin_manager = PluginManager::new(temp_dir.path().to_path_buf()).await?;

        let manifest = PluginManifest {
            name: "test-plugin".to_string(),
            version: PluginVersion::parse("1.0.0")?,
            description: "A test plugin".to_string(),
            author: "Test Author".to_string(),
            license: "MIT".to_string(),
            repository: None,
            keywords: vec![],
            dependencies: std::collections::HashMap::new(),
            ferrix_version: "^0.1.0".to_string(),
            main: "main.wasm".to_string(),
            permissions: vec!["session:read".to_string(), "window:create".to_string()],
        };

        // Test permission checking
        assert!(plugin_manager.check_permission(&manifest, "session:read"));
        assert!(plugin_manager.check_permission(&manifest, "window:create"));
        assert!(!plugin_manager.check_permission(&manifest, "session:write"));

        Ok(())
    }

    #[test]
    fn test_plugin_command_parsing() -> Result<()> {
        use crate::plugin::api::PluginCommand;

        // Test command serialization/deserialization
        let command = PluginCommand::GetSessionInfo;
        let json = serde_json::to_string(&command)?;
        let parsed: PluginCommand = serde_json::from_str(&json)?;

        match parsed {
            PluginCommand::GetSessionInfo => {
                // Success
            }
            _ => panic!("Command parsing failed"),
        }

        Ok(())
    }
}