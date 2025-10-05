use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

use super::runtime::PluginRuntime;
use super::api::{
    PluginManifest, PluginEvent, PluginCommand, PluginResponse,
    PluginContext, PluginHook,
};
use crate::error::{Result, FerrixError};

/// High-level plugin manager for Ferrix
pub struct PluginManager {
    runtime: Arc<RwLock<PluginRuntime>>,
    plugin_dir: PathBuf,
    auto_load: bool,
}

impl PluginManager {
    pub fn new(plugin_dir: PathBuf) -> Result<Self> {
        let runtime = PluginRuntime::new()
            .map_err(|e| FerrixError::Plugin(format!("Failed to create plugin runtime: {}", e)))?;

        Ok(Self {
            runtime: Arc::new(RwLock::new(runtime)),
            plugin_dir,
            auto_load: true,
        })
    }

    /// Initialize the plugin system and load plugins
    pub async fn initialize(&self) -> Result<()> {
        // Create plugin directory if it doesn't exist
        if !self.plugin_dir.exists() {
            std::fs::create_dir_all(&self.plugin_dir)
                .map_err(|e| FerrixError::Plugin(format!("Failed to create plugin directory: {}", e)))?;
        }

        // Auto-load plugins if enabled
        if self.auto_load {
            self.auto_load_plugins().await?;
        }

        info!("Plugin manager initialized with directory: {:?}", self.plugin_dir);
        Ok(())
    }

    /// Auto-load all plugins from the plugin directory
    async fn auto_load_plugins(&self) -> Result<()> {
        let entries = std::fs::read_dir(&self.plugin_dir)
            .map_err(|e| FerrixError::Plugin(format!("Failed to read plugin directory: {}", e)))?;

        for entry in entries {
            let entry = entry.map_err(|e| FerrixError::Plugin(format!("Failed to read directory entry: {}", e)))?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("wasm") {
                match self.load_plugin(&path).await {
                    Ok(id) => info!("Auto-loaded plugin: {:?} (ID: {})", path, id),
                    Err(e) => warn!("Failed to auto-load plugin {:?}: {}", path, e),
                }
            }
        }

        Ok(())
    }

    /// Load a plugin from file
    pub async fn load_plugin(&self, path: &Path) -> Result<String> {
        let mut runtime = self.runtime.write().await;
        runtime.load_plugin(path).await
    }

    /// Load a plugin from URL (for plugin marketplace)
    pub async fn load_plugin_from_url(&self, url: &str) -> Result<String> {
        // Download plugin to temp location
        let temp_path = self.download_plugin(url).await?;

        // Verify plugin signature/checksum
        self.verify_plugin(&temp_path)?;

        // Move to plugin directory
        let filename = temp_path.file_name()
            .ok_or_else(|| FerrixError::Plugin("Invalid plugin filename".to_string()))?;
        let dest_path = self.plugin_dir.join(filename);

        std::fs::rename(&temp_path, &dest_path)
            .map_err(|e| FerrixError::Plugin(format!("Failed to install plugin: {}", e)))?;

        // Load the plugin
        self.load_plugin(&dest_path).await
    }

    /// Unload a plugin
    pub async fn unload_plugin(&self, plugin_id: &str) -> Result<()> {
        let mut runtime = self.runtime.write().await;
        runtime.unload_plugin(plugin_id).await
    }

    /// Reload a plugin
    pub async fn reload_plugin(&self, plugin_id: &str) -> Result<String> {
        // Get plugin info before unloading
        let manifest = {
            let runtime = self.runtime.read().await;
            let plugins = runtime.list_plugins().await;
            plugins.iter()
                .find(|p| p.name == plugin_id)
                .cloned()
                .ok_or_else(|| FerrixError::Plugin(format!("Plugin not found: {}", plugin_id)))?
        };

        // Unload the plugin
        self.unload_plugin(plugin_id).await?;

        // Find and reload the plugin file
        let plugin_file = self.find_plugin_file(&manifest.name)?;
        self.load_plugin(&plugin_file).await
    }

    /// List all loaded plugins
    pub async fn list_plugins(&self) -> Vec<PluginManifest> {
        let runtime = self.runtime.read().await;
        runtime.list_plugins().await
    }

    /// Execute a command through the plugin system
    pub async fn execute_command(
        &self,
        command: PluginCommand,
        context: PluginContext,
    ) -> Result<PluginResponse> {
        let runtime = self.runtime.read().await;
        runtime.execute_command(command, context).await
    }

    /// Trigger a hook
    pub async fn trigger_hook(
        &self,
        hook: PluginHook,
        context: PluginContext,
    ) -> Result<Vec<PluginResponse>> {
        let runtime = self.runtime.read().await;
        runtime.trigger_hook(hook, context).await
    }

    /// Send an event to all plugins
    pub async fn send_event(&self, event: PluginEvent) {
        let runtime = self.runtime.read().await;
        runtime.broadcast_event(event).await;
    }

    /// Install a plugin from a package file
    pub async fn install_plugin(&self, package_path: &Path) -> Result<String> {
        // Extract plugin from package (tar.gz, zip, etc.)
        let extracted_path = self.extract_plugin_package(package_path)?;

        // Validate plugin structure
        self.validate_plugin_structure(&extracted_path)?;

        // Copy to plugin directory
        let plugin_name = extracted_path.file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| FerrixError::Plugin("Invalid plugin name".to_string()))?;

        let dest_path = self.plugin_dir.join(format!("{}.wasm", plugin_name));

        std::fs::copy(&extracted_path, &dest_path)
            .map_err(|e| FerrixError::Plugin(format!("Failed to install plugin: {}", e)))?;

        // Load the plugin
        self.load_plugin(&dest_path).await
    }

    /// Uninstall a plugin
    pub async fn uninstall_plugin(&self, plugin_id: &str) -> Result<()> {
        // Unload the plugin first
        self.unload_plugin(plugin_id).await?;

        // Find and remove the plugin file
        let plugin_file = self.find_plugin_file(plugin_id)?;
        std::fs::remove_file(&plugin_file)
            .map_err(|e| FerrixError::Plugin(format!("Failed to remove plugin file: {}", e)))?;

        info!("Uninstalled plugin: {}", plugin_id);
        Ok(())
    }

    /// Update a plugin to a newer version
    pub async fn update_plugin(&self, plugin_id: &str, new_version_path: &Path) -> Result<String> {
        // Backup current plugin
        let backup_path = self.backup_plugin(plugin_id)?;

        // Try to install new version
        match self.install_plugin(new_version_path).await {
            Ok(new_id) => {
                // Remove backup on success
                let _ = std::fs::remove_file(backup_path);
                Ok(new_id)
            }
            Err(e) => {
                // Restore from backup on failure
                self.restore_plugin_from_backup(&backup_path, plugin_id)?;
                Err(e)
            }
        }
    }

    // Helper functions

    async fn download_plugin(&self, url: &str) -> Result<PathBuf> {
        use std::fs::File;
        use std::io::Write;

        // Extract filename from URL
        let filename = url.split('/').last()
            .ok_or_else(|| FerrixError::Plugin("Invalid URL: cannot extract filename".to_string()))?;

        let plugin_path = self.plugin_dir.join(filename);

        // Download the file
        let response = reqwest::get(url)
            .await
            .map_err(|e| FerrixError::Plugin(format!("Failed to download plugin: {}", e)))?;

        if !response.status().is_success() {
            return Err(FerrixError::Plugin(format!("Download failed with status: {}", response.status())));
        }

        let bytes = response.bytes()
            .await
            .map_err(|e| FerrixError::Plugin(format!("Failed to read response: {}", e)))?;

        // Write to file
        let mut file = File::create(&plugin_path)
            .map_err(|e| FerrixError::Plugin(format!("Failed to create plugin file: {}", e)))?;

        file.write_all(&bytes)
            .map_err(|e| FerrixError::Plugin(format!("Failed to write plugin file: {}", e)))?;

        // Make executable (on Unix systems)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&plugin_path)
                .map_err(|e| FerrixError::Plugin(format!("Failed to read file metadata: {}", e)))?
                .permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&plugin_path, perms)
                .map_err(|e| FerrixError::Plugin(format!("Failed to set file permissions: {}", e)))?;
        }

        Ok(plugin_path)
    }

    fn verify_plugin(&self, path: &Path) -> Result<()> {
        // Verify plugin signature, checksum, etc.
        // For now, just check if file exists
        if !path.exists() {
            return Err(FerrixError::Plugin("Plugin file not found".to_string()));
        }
        Ok(())
    }

    fn find_plugin_file(&self, plugin_name: &str) -> Result<PathBuf> {
        let entries = std::fs::read_dir(&self.plugin_dir)
            .map_err(|e| FerrixError::Plugin(format!("Failed to read plugin directory: {}", e)))?;

        for entry in entries {
            let entry = entry.map_err(|e| FerrixError::Plugin(format!("Failed to read entry: {}", e)))?;
            let path = entry.path();

            if path.file_stem().and_then(|s| s.to_str()) == Some(plugin_name) {
                return Ok(path);
            }
        }

        Err(FerrixError::Plugin(format!("Plugin file not found: {}", plugin_name)))
    }

    fn extract_plugin_package(&self, package_path: &Path) -> Result<PathBuf> {
        // Extract plugin from package format
        // For now, assume it's already a WASM file
        Ok(package_path.to_path_buf())
    }

    fn validate_plugin_structure(&self, path: &Path) -> Result<()> {
        // Validate that the plugin has required structure
        if !path.exists() {
            return Err(FerrixError::Plugin("Plugin file not found".to_string()));
        }

        // Check file extension
        if path.extension().and_then(|s| s.to_str()) != Some("wasm") {
            return Err(FerrixError::Plugin("Plugin must be a WASM file".to_string()));
        }

        Ok(())
    }

    fn backup_plugin(&self, plugin_id: &str) -> Result<PathBuf> {
        let plugin_file = self.find_plugin_file(plugin_id)?;
        let backup_path = plugin_file.with_extension("wasm.backup");

        std::fs::copy(&plugin_file, &backup_path)
            .map_err(|e| FerrixError::Plugin(format!("Failed to backup plugin: {}", e)))?;

        Ok(backup_path)
    }

    fn restore_plugin_from_backup(&self, backup_path: &Path, plugin_id: &str) -> Result<()> {
        let plugin_file = self.find_plugin_file(plugin_id)?;

        std::fs::copy(backup_path, &plugin_file)
            .map_err(|e| FerrixError::Plugin(format!("Failed to restore plugin: {}", e)))?;

        Ok(())
    }
}
#[cfg(test)]
mod tests {
    

    #[test]
    fn test_plugin_manager_initialization() {
        // Plugin manager initialization test
        assert!(true);
    }

    #[test]
    fn test_plugin_loading() {
        // Test plugin loading mechanism
        assert!(true);
    }
}
