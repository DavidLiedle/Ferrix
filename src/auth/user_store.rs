use std::collections::HashMap;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use tokio::fs;
use tokio::sync::RwLock;
use bcrypt::{hash, verify, DEFAULT_COST};
use uuid::Uuid;

use crate::error::{Result, FerrixError};
use crate::protocol::ClientId;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredUser {
    pub username: String,
    pub password_hash: String,
    pub client_id: ClientId,
    pub permissions: Vec<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
struct UserDatabase {
    users: HashMap<String, StoredUser>,
    version: u32,
}

impl Default for UserDatabase {
    fn default() -> Self {
        Self {
            users: HashMap::new(),
            version: 1,
        }
    }
}

pub struct UserStore {
    users: RwLock<HashMap<String, StoredUser>>,
    file_path: PathBuf,
}

impl UserStore {
    /// Create a new UserStore instance with the default path (~/.ferrix/users.json)
    pub async fn new() -> Result<Self> {
        let file_path = Self::get_default_path()?;
        Self::new_with_path(file_path).await
    }

    /// Create a new UserStore instance with a custom path
    pub async fn new_with_path(file_path: PathBuf) -> Result<Self> {
        let store = Self {
            users: RwLock::new(HashMap::new()),
            file_path,
        };

        // Load existing users from file
        if let Err(e) = store.load_users().await {
            // If the file doesn't exist or is invalid, start with an empty store
            tracing::warn!("Failed to load users from {:?}: {}. Starting with empty user store.", store.file_path, e);
        }

        Ok(store)
    }

    /// Get the default path for the user database file
    pub fn get_default_path() -> Result<PathBuf> {
        let home_dir = dirs::home_dir()
            .ok_or_else(|| FerrixError::Other("Unable to determine home directory".to_string()))?;

        let ferrix_dir = home_dir.join(".ferrix");
        Ok(ferrix_dir.join("users.json"))
    }

    /// Load users from the JSON file
    pub async fn load_users(&self) -> Result<()> {
        if !self.file_path.exists() {
            // File doesn't exist, start with empty store
            return Ok(());
        }

        let content = fs::read_to_string(&self.file_path).await
            .map_err(|e| FerrixError::Other(format!("Failed to read user file: {}", e)))?;

        let database: UserDatabase = serde_json::from_str(&content)
            .map_err(|e| FerrixError::Other(format!("Failed to parse user file: {}", e)))?;

        let mut users = self.users.write().await;
        *users = database.users;

        tracing::info!("Loaded {} users from {:?}", users.len(), self.file_path);
        Ok(())
    }

    /// Save users to the JSON file
    pub async fn save_users(&self) -> Result<()> {
        // Ensure the parent directory exists
        if let Some(parent) = self.file_path.parent() {
            fs::create_dir_all(parent).await
                .map_err(|e| FerrixError::Other(format!("Failed to create directory: {}", e)))?;
        }

        let users = self.users.read().await;
        let database = UserDatabase {
            users: users.clone(),
            version: 1,
        };

        let content = serde_json::to_string_pretty(&database)
            .map_err(|e| FerrixError::Other(format!("Failed to serialize users: {}", e)))?;

        fs::write(&self.file_path, content).await
            .map_err(|e| FerrixError::Other(format!("Failed to write user file: {}", e)))?;

        tracing::info!("Saved {} users to {:?}", users.len(), self.file_path);
        Ok(())
    }

    /// Add a new user with password hashing
    pub async fn add_user(&self, username: String, password: String) -> Result<ClientId> {
        let mut users = self.users.write().await;

        // Check if user already exists
        if users.contains_key(&username) {
            return Err(FerrixError::Other(format!("User '{}' already exists", username)));
        }

        // Hash the password
        let password_hash = hash(password.as_bytes(), DEFAULT_COST)
            .map_err(|e| FerrixError::Other(format!("Failed to hash password: {}", e)))?;

        let client_id = ClientId(Uuid::new_v4());
        let user = StoredUser {
            username: username.clone(),
            password_hash,
            client_id: client_id.clone(),
            permissions: vec!["all".to_string()], // Default permissions
            created_at: chrono::Utc::now(),
        };

        users.insert(username.clone(), user);
        drop(users); // Release the lock before saving

        self.save_users().await?;
        tracing::info!("Added user '{}'", username);

        Ok(client_id)
    }

    /// Remove a user
    pub async fn remove_user(&self, username: &str) -> Result<()> {
        let mut users = self.users.write().await;

        if users.remove(username).is_none() {
            return Err(FerrixError::Other(format!("User '{}' not found", username)));
        }

        drop(users); // Release the lock before saving
        self.save_users().await?;
        tracing::info!("Removed user '{}'", username);

        Ok(())
    }

    /// List all users
    pub async fn list_users(&self) -> Result<Vec<String>> {
        let users = self.users.read().await;
        let usernames: Vec<String> = users.keys().cloned().collect();
        Ok(usernames)
    }

    /// Change a user's password
    pub async fn change_password(&self, username: &str, new_password: String) -> Result<()> {
        let mut users = self.users.write().await;

        if let Some(user) = users.get_mut(username) {
            let password_hash = hash(new_password.as_bytes(), DEFAULT_COST)
                .map_err(|e| FerrixError::Other(format!("Failed to hash password: {}", e)))?;

            user.password_hash = password_hash;
        } else {
            return Err(FerrixError::Other(format!("User '{}' not found", username)));
        }

        drop(users); // Release the lock before saving
        self.save_users().await?;
        tracing::info!("Changed password for user '{}'", username);

        Ok(())
    }

    /// Verify a user's password and return their ClientId
    pub async fn verify_password(&self, username: &str, password: &str) -> Result<ClientId> {
        let users = self.users.read().await;

        if let Some(user) = users.get(username) {
            if verify(password, &user.password_hash)
                .map_err(|e| FerrixError::Other(format!("Failed to verify password: {}", e)))? {
                return Ok(user.client_id.clone());
            }
        }

        Err(FerrixError::Other("Invalid username or password".to_string()))
    }

    /// Get user information by username
    pub async fn get_user(&self, username: &str) -> Result<StoredUser> {
        let users = self.users.read().await;
        users.get(username)
            .cloned()
            .ok_or_else(|| FerrixError::Other(format!("User '{}' not found", username)))
    }

    /// Get user information by ClientId
    pub async fn get_user_by_client_id(&self, client_id: &ClientId) -> Result<StoredUser> {
        let users = self.users.read().await;
        users.values()
            .find(|user| user.client_id == *client_id)
            .cloned()
            .ok_or_else(|| FerrixError::Other("User not found for client ID".to_string()))
    }

    /// Check if a user has a specific permission
    pub async fn check_permission(&self, client_id: &ClientId, permission: &str) -> Result<bool> {
        let user = self.get_user_by_client_id(client_id).await?;
        Ok(user.permissions.contains(&"all".to_string()) || user.permissions.contains(&permission.to_string()))
    }

    /// Get the number of users
    pub async fn user_count(&self) -> usize {
        let users = self.users.read().await;
        users.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_user_store_basic_operations() {
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("test_users.json");

        let store = UserStore::new_with_path(file_path).await.unwrap();

        // Test adding a user
        let client_id = store.add_user("testuser".to_string(), "password123".to_string()).await.unwrap();
        assert_eq!(store.user_count().await, 1);

        // Test verifying password
        let verified_client_id = store.verify_password("testuser", "password123").await.unwrap();
        assert_eq!(client_id, verified_client_id);

        // Test wrong password
        assert!(store.verify_password("testuser", "wrongpassword").await.is_err());

        // Test listing users
        let users = store.list_users().await.unwrap();
        assert_eq!(users.len(), 1);
        assert!(users.contains(&"testuser".to_string()));

        // Test changing password
        store.change_password("testuser", "newpassword".to_string()).await.unwrap();
        assert!(store.verify_password("testuser", "password123").await.is_err());
        assert!(store.verify_password("testuser", "newpassword").await.is_ok());

        // Test removing user
        store.remove_user("testuser").await.unwrap();
        assert_eq!(store.user_count().await, 0);
    }

    #[tokio::test]
    async fn test_user_store_persistence() {
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("test_users.json");

        // Create store and add user
        {
            let store = UserStore::new_with_path(file_path.clone()).await.unwrap();
            store.add_user("persistent_user".to_string(), "password123".to_string()).await.unwrap();
        }

        // Create new store instance and verify user persisted
        {
            let store = UserStore::new_with_path(file_path).await.unwrap();
            assert_eq!(store.user_count().await, 1);
            assert!(store.verify_password("persistent_user", "password123").await.is_ok());
        }
    }
}