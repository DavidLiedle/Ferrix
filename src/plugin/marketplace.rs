use std::collections::HashMap;
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use semver::Version;
use tokio::fs;
use reqwest;
use sha2::{Sha256, Digest};
use tar::Archive;
use flate2::read::GzDecoder;

use crate::error::{FerrixError, Result};

/// The plugin marketplace client for discovering and installing plugins
#[derive(Debug)]
pub struct MarketplaceClient {
    /// Base URL of the marketplace API
    api_url: String,

    /// Local plugin directory
    plugin_dir: PathBuf,

    /// Cache directory for downloaded plugins
    cache_dir: PathBuf,

    /// HTTP client for API requests
    client: reqwest::Client,

    /// Authentication token for private plugins
    auth_token: Option<String>,

    /// Cached plugin metadata
    cache: PluginCache,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginMetadata {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: Version,
    pub author: String,
    pub license: String,
    pub homepage: Option<String>,
    pub repository: Option<String>,
    pub categories: Vec<String>,
    pub tags: Vec<String>,
    pub dependencies: Vec<PluginDependency>,
    pub downloads: u64,
    pub rating: f32,
    pub reviews_count: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub checksum: String,
    pub size: u64,
    pub min_ferrix_version: Option<Version>,
    pub max_ferrix_version: Option<Version>,
    pub screenshots: Vec<String>,
    pub verified: bool,
    pub featured: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginDependency {
    pub plugin_id: String,
    pub version_requirement: String,
    pub optional: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginRelease {
    pub version: Version,
    pub download_url: String,
    pub changelog: String,
    pub published_at: DateTime<Utc>,
    pub checksum: String,
    pub size: u64,
    pub pre_release: bool,
    pub yanked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginReview {
    pub user: String,
    pub rating: u8,  // 1-5
    pub title: String,
    pub comment: String,
    pub created_at: DateTime<Utc>,
    pub helpful_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplaceSearchQuery {
    pub query: Option<String>,
    pub categories: Vec<String>,
    pub tags: Vec<String>,
    pub author: Option<String>,
    pub sort_by: SortOrder,
    pub page: u32,
    pub per_page: u32,
    pub verified_only: bool,
    pub min_rating: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SortOrder {
    Downloads,
    Rating,
    Recent,
    Alphabetical,
    Trending,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResults {
    pub plugins: Vec<PluginMetadata>,
    pub total: u64,
    pub page: u32,
    pub per_page: u32,
    pub has_more: bool,
}

#[derive(Debug, Default)]
struct PluginCache {
    metadata: HashMap<String, PluginMetadata>,
    search_cache: HashMap<String, (SearchResults, DateTime<Utc>)>,
    #[allow(dead_code)]
    cache_duration: std::time::Duration,
}

impl MarketplaceClient {
    /// Create a new marketplace client
    pub fn new(api_url: String) -> Result<Self> {
        let plugin_dir = dirs::data_dir()
            .ok_or_else(|| FerrixError::Other("Could not find data directory".to_string()))?
            .join("ferrix")
            .join("plugins");

        let cache_dir = dirs::cache_dir()
            .ok_or_else(|| FerrixError::Other("Could not find cache directory".to_string()))?
            .join("ferrix")
            .join("marketplace");

        // Create directories if they don't exist
        std::fs::create_dir_all(&plugin_dir)?;
        std::fs::create_dir_all(&cache_dir)?;

        Ok(Self {
            api_url,
            plugin_dir,
            cache_dir,
            client: reqwest::Client::new(),
            auth_token: None,
            cache: PluginCache {
                cache_duration: std::time::Duration::from_secs(3600), // 1 hour cache
                ..Default::default()
            },
        })
    }

    /// Set authentication token for private plugins
    pub fn set_auth_token(&mut self, token: String) {
        self.auth_token = Some(token);
    }

    /// Search for plugins in the marketplace
    pub async fn search(&mut self, query: MarketplaceSearchQuery) -> Result<SearchResults> {
        // Check cache first
        let cache_key = serde_json::to_string(&query).unwrap_or_default();
        if let Some((cached_results, cached_at)) = self.cache.search_cache.get(&cache_key) {
            if cached_at.signed_duration_since(Utc::now()).num_seconds().abs() < 3600 {
                return Ok(cached_results.clone());
            }
        }

        // Make API request
        let mut request = self.client
            .get(format!("{}/api/v1/search", self.api_url))
            .query(&query);

        if let Some(token) = &self.auth_token {
            request = request.bearer_auth(token);
        }

        let response = request.send().await
            .map_err(|e| FerrixError::Other(format!("Failed to search marketplace: {}", e)))?;

        if !response.status().is_success() {
            return Err(FerrixError::Other(format!(
                "Marketplace API error: {}",
                response.status()
            )));
        }

        let results: SearchResults = response.json().await
            .map_err(|e| FerrixError::Other(format!("Failed to parse search results: {}", e)))?;

        // Update cache
        self.cache.search_cache.insert(cache_key, (results.clone(), Utc::now()));

        // Cache individual plugin metadata
        for plugin in &results.plugins {
            self.cache.metadata.insert(plugin.id.clone(), plugin.clone());
        }

        Ok(results)
    }

    /// Get detailed information about a plugin
    pub async fn get_plugin_info(&mut self, plugin_id: &str) -> Result<PluginMetadata> {
        // Check cache first
        if let Some(cached) = self.cache.metadata.get(plugin_id) {
            return Ok(cached.clone());
        }

        let mut request = self.client
            .get(format!("{}/api/v1/plugins/{}", self.api_url, plugin_id));

        if let Some(token) = &self.auth_token {
            request = request.bearer_auth(token);
        }

        let response = request.send().await
            .map_err(|e| FerrixError::Other(format!("Failed to get plugin info: {}", e)))?;

        if !response.status().is_success() {
            return Err(FerrixError::Other(format!(
                "Plugin not found or API error: {}",
                response.status()
            )));
        }

        let metadata: PluginMetadata = response.json().await
            .map_err(|e| FerrixError::Other(format!("Failed to parse plugin metadata: {}", e)))?;

        // Update cache
        self.cache.metadata.insert(plugin_id.to_string(), metadata.clone());

        Ok(metadata)
    }

    /// Install a plugin from the marketplace
    pub fn install_plugin<'a>(&'a mut self, plugin_id: &'a str, version: Option<Version>) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<PathBuf>> + Send + 'a>> {
        Box::pin(self.install_plugin_impl(plugin_id, version))
    }

    async fn install_plugin_impl(&mut self, plugin_id: &str, version: Option<Version>) -> Result<PathBuf> {
        // Get plugin metadata
        let metadata = self.get_plugin_info(plugin_id).await?;

        // Check compatibility
        if let Some(min_version) = &metadata.min_ferrix_version {
            let current_version = Version::parse(env!("CARGO_PKG_VERSION"))
                .unwrap_or_else(|_| Version::new(0, 1, 0));
            if current_version < *min_version {
                return Err(FerrixError::Other(format!(
                    "Plugin requires Ferrix version {} or higher",
                    min_version
                )));
            }
        }

        // Get the appropriate release
        let release = self.get_plugin_release(plugin_id, version).await?;

        // Check if already installed
        let install_path = self.plugin_dir.join(&metadata.id);
        if install_path.exists() {
            // Check if update is needed
            let installed_version = self.get_installed_version(&metadata.id).await?;
            if installed_version >= release.version {
                return Ok(install_path);
            }
        }

        // Download the plugin
        let download_path = self.download_plugin(&metadata, &release).await?;

        // Verify checksum
        self.verify_checksum(&download_path, &release.checksum).await?;

        // Install dependencies
        for dep in &metadata.dependencies {
            if !dep.optional {
                self.install_plugin(&dep.plugin_id, None).await?;
            }
        }

        // Extract and install
        self.extract_plugin(&download_path, &install_path).await?;

        // Save metadata
        let metadata_path = install_path.join("marketplace.json");
        let metadata_json = serde_json::to_string_pretty(&metadata)
            .map_err(|e| FerrixError::Other(format!("Failed to serialize metadata: {}", e)))?;
        fs::write(metadata_path, metadata_json).await?;

        Ok(install_path)
    }

    /// Uninstall a plugin
    pub async fn uninstall_plugin(&mut self, plugin_id: &str) -> Result<()> {
        let install_path = self.plugin_dir.join(plugin_id);

        if !install_path.exists() {
            return Err(FerrixError::Other(format!("Plugin {} is not installed", plugin_id)));
        }

        // Check for dependent plugins
        let dependents = self.find_dependent_plugins(plugin_id).await?;
        if !dependents.is_empty() {
            return Err(FerrixError::Other(format!(
                "Cannot uninstall: plugins {:?} depend on {}",
                dependents, plugin_id
            )));
        }

        // Remove the plugin directory
        fs::remove_dir_all(install_path).await?;

        // Clear from cache
        self.cache.metadata.remove(plugin_id);

        Ok(())
    }

    /// Update a plugin to the latest version
    pub async fn update_plugin(&mut self, plugin_id: &str) -> Result<PathBuf> {
        self.install_plugin(plugin_id, None).await
    }

    /// List installed plugins
    pub async fn list_installed(&self) -> Result<Vec<InstalledPlugin>> {
        let mut installed = Vec::new();

        let mut entries = fs::read_dir(&self.plugin_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            if entry.file_type().await?.is_dir() {
                let metadata_path = entry.path().join("marketplace.json");
                if metadata_path.exists() {
                    let metadata_json = fs::read_to_string(metadata_path).await?;
                    if let Ok(metadata) = serde_json::from_str::<PluginMetadata>(&metadata_json) {
                        installed.push(InstalledPlugin {
                            metadata,
                            path: entry.path(),
                            enabled: true,  // Could check actual status
                        });
                    }
                }
            }
        }

        Ok(installed)
    }

    /// Get plugin reviews
    pub async fn get_reviews(&self, plugin_id: &str, page: u32) -> Result<Vec<PluginReview>> {
        let mut request = self.client
            .get(format!("{}/api/v1/plugins/{}/reviews", self.api_url, plugin_id))
            .query(&[("page", page.to_string())]);

        if let Some(token) = &self.auth_token {
            request = request.bearer_auth(token);
        }

        let response = request.send().await
            .map_err(|e| FerrixError::Other(format!("Failed to get reviews: {}", e)))?;

        let reviews: Vec<PluginReview> = response.json().await
            .map_err(|e| FerrixError::Other(format!("Failed to parse reviews: {}", e)))?;

        Ok(reviews)
    }

    /// Submit a review for a plugin
    pub async fn submit_review(&self, plugin_id: &str, rating: u8, title: &str, comment: &str) -> Result<()> {
        if !(1..=5).contains(&rating) {
            return Err(FerrixError::Other("Rating must be between 1 and 5".to_string()));
        }

        let review = serde_json::json!({
            "rating": rating,
            "title": title,
            "comment": comment,
        });

        let mut request = self.client
            .post(format!("{}/api/v1/plugins/{}/reviews", self.api_url, plugin_id))
            .json(&review);

        if let Some(token) = &self.auth_token {
            request = request.bearer_auth(token);
        } else {
            return Err(FerrixError::Other("Authentication required to submit reviews".to_string()));
        }

        let response = request.send().await
            .map_err(|e| FerrixError::Other(format!("Failed to submit review: {}", e)))?;

        if !response.status().is_success() {
            return Err(FerrixError::Other(format!(
                "Failed to submit review: {}",
                response.status()
            )));
        }

        Ok(())
    }

    // Helper methods

    async fn get_plugin_release(&self, plugin_id: &str, version: Option<Version>) -> Result<PluginRelease> {
        let url = if let Some(v) = version {
            format!("{}/api/v1/plugins/{}/releases/{}", self.api_url, plugin_id, v)
        } else {
            format!("{}/api/v1/plugins/{}/releases/latest", self.api_url, plugin_id)
        };

        let mut request = self.client.get(url);

        if let Some(token) = &self.auth_token {
            request = request.bearer_auth(token);
        }

        let response = request.send().await
            .map_err(|e| FerrixError::Other(format!("Failed to get release info: {}", e)))?;

        let release: PluginRelease = response.json().await
            .map_err(|e| FerrixError::Other(format!("Failed to parse release info: {}", e)))?;

        Ok(release)
    }

    async fn download_plugin(&self, metadata: &PluginMetadata, release: &PluginRelease) -> Result<PathBuf> {
        let cache_path = self.cache_dir.join(format!("{}_{}.tar.gz", metadata.id, release.version));

        // Check if already cached
        if cache_path.exists() {
            // Verify cached file
            if self.verify_checksum(&cache_path, &release.checksum).await.is_ok() {
                return Ok(cache_path);
            }
            // Remove corrupted cache file
            fs::remove_file(&cache_path).await?;
        }

        // Download the plugin
        let mut request = self.client.get(&release.download_url);

        if let Some(token) = &self.auth_token {
            request = request.bearer_auth(token);
        }

        let response = request.send().await
            .map_err(|e| FerrixError::Other(format!("Failed to download plugin: {}", e)))?;

        let bytes = response.bytes().await
            .map_err(|e| FerrixError::Other(format!("Failed to download plugin content: {}", e)))?;

        fs::write(&cache_path, bytes).await?;

        Ok(cache_path)
    }

    async fn verify_checksum(&self, file_path: &Path, expected: &str) -> Result<()> {
        let content = fs::read(file_path).await?;
        let mut hasher = Sha256::new();
        hasher.update(&content);
        let result = format!("{:x}", hasher.finalize());

        if result != expected {
            return Err(FerrixError::Other("Checksum verification failed".to_string()));
        }

        Ok(())
    }

    async fn extract_plugin(&self, archive_path: &Path, install_path: &Path) -> Result<()> {
        // Create install directory
        fs::create_dir_all(install_path).await?;

        // Extract tar.gz archive
        let file = std::fs::File::open(archive_path)?;
        let decoder = GzDecoder::new(file);
        let mut archive = Archive::new(decoder);

        archive.unpack(install_path)
            .map_err(|e| FerrixError::Other(format!("Failed to extract plugin: {}", e)))?;

        Ok(())
    }

    async fn get_installed_version(&self, plugin_id: &str) -> Result<Version> {
        let metadata_path = self.plugin_dir.join(plugin_id).join("marketplace.json");

        if !metadata_path.exists() {
            return Err(FerrixError::Other("Plugin metadata not found".to_string()));
        }

        let metadata_json = fs::read_to_string(metadata_path).await?;
        let metadata: PluginMetadata = serde_json::from_str(&metadata_json)
            .map_err(|e| FerrixError::Other(format!("Failed to parse metadata: {}", e)))?;

        Ok(metadata.version)
    }

    async fn find_dependent_plugins(&self, plugin_id: &str) -> Result<Vec<String>> {
        let mut dependents = Vec::new();

        let installed = self.list_installed().await?;
        for plugin in installed {
            for dep in &plugin.metadata.dependencies {
                if dep.plugin_id == plugin_id && !dep.optional {
                    dependents.push(plugin.metadata.id.clone());
                    break;
                }
            }
        }

        Ok(dependents)
    }
}

#[derive(Debug, Clone)]
pub struct InstalledPlugin {
    pub metadata: PluginMetadata,
    pub path: PathBuf,
    pub enabled: bool,
}

/// Plugin marketplace server for hosting plugins
pub struct MarketplaceServer {
    #[allow(dead_code)]
    storage: Box<dyn PluginStorage>,
    #[allow(dead_code)]
    auth: Box<dyn AuthProvider>,
}

#[async_trait::async_trait]
pub trait PluginStorage: Send + Sync {
    async fn store_plugin(&self, plugin: &PluginMetadata, data: Vec<u8>) -> Result<()>;
    async fn get_plugin(&self, plugin_id: &str, version: &Version) -> Result<Vec<u8>>;
    async fn list_plugins(&self, query: &MarketplaceSearchQuery) -> Result<SearchResults>;
    async fn update_metadata(&self, plugin: &PluginMetadata) -> Result<()>;
}

#[async_trait::async_trait]
pub trait AuthProvider: Send + Sync {
    async fn verify_token(&self, token: &str) -> Result<UserInfo>;
    async fn has_permission(&self, user: &UserInfo, action: &str) -> bool;
}

#[derive(Debug, Clone)]
pub struct UserInfo {
    pub id: String,
    pub username: String,
    pub email: String,
    pub roles: Vec<String>,
}

impl Default for MarketplaceSearchQuery {
    fn default() -> Self {
        Self {
            query: None,
            categories: Vec::new(),
            tags: Vec::new(),
            author: None,
            sort_by: SortOrder::Downloads,
            page: 1,
            per_page: 20,
            verified_only: false,
            min_rating: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_marketplace_client() {
        let client = MarketplaceClient::new("https://marketplace.ferrix.io".to_string());
        assert!(client.is_ok());
    }

    #[tokio::test]
    async fn test_search_query() {
        let query = MarketplaceSearchQuery {
            query: Some("terminal".to_string()),
            categories: vec!["productivity".to_string()],
            ..Default::default()
        };

        assert_eq!(query.page, 1);
        assert_eq!(query.per_page, 20);
    }
}