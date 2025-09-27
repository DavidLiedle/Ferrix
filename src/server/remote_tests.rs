#[cfg(test)]
mod remote_tests {
    use super::*;
    use crate::error::Result;
    use crate::server::remote::{RemoteServer, RemoteClient, PasswordAuthHandler};
    use crate::auth::UserStore;
    use crate::protocol::{AuthCredentials, ClientMessage, ServerMessage};
    use std::net::SocketAddr;
    use std::sync::Arc;
    use tempfile::TempDir;
    use tokio::time::{timeout, Duration};

    #[tokio::test]
    async fn test_password_auth_handler_creation() -> Result<()> {
        let temp_dir = TempDir::new().unwrap();
        std::env::set_var("HOME", temp_dir.path());

        let auth_handler = PasswordAuthHandler::new().await?;

        // Should start with no users
        let user_count = auth_handler.user_store.user_count().await;
        assert_eq!(user_count, 0);

        Ok(())
    }

    #[tokio::test]
    async fn test_password_auth_user_management() -> Result<()> {
        let temp_dir = TempDir::new().unwrap();
        std::env::set_var("HOME", temp_dir.path());

        let auth_handler = PasswordAuthHandler::new().await?;

        // Add a user
        let client_id = auth_handler.add_user("testuser".to_string(), "testpass".to_string()).await?;

        // Test that we can authenticate
        // (user_store is private, so we test via authentication)

        // Test authentication
        let credentials = AuthCredentials {
            username: "testuser".to_string(),
            password: Some("testpass".to_string()),
            token: None,
            certificate: None,
        };

        let authenticated_id = auth_handler.authenticate(&credentials).await?;
        assert_eq!(authenticated_id, client_id);

        Ok(())
    }

    #[tokio::test]
    async fn test_password_auth_invalid_credentials() -> Result<()> {
        let temp_dir = TempDir::new().unwrap();
        std::env::set_var("HOME", temp_dir.path());

        let auth_handler = PasswordAuthHandler::new().await?;

        // Add a user
        auth_handler.add_user("testuser".to_string(), "testpass".to_string()).await?;

        // Test with wrong password
        let wrong_credentials = AuthCredentials {
            username: "testuser".to_string(),
            password: Some("wrongpass".to_string()),
            key: None,
        };

        let auth_result = auth_handler.authenticate(&wrong_credentials).await;
        assert!(auth_result.is_err());

        // Test with no password
        let no_pass_credentials = AuthCredentials {
            username: "testuser".to_string(),
            password: None,
            key: None,
        };

        let auth_result = auth_handler.authenticate(&no_pass_credentials).await;
        assert!(auth_result.is_err());

        Ok(())
    }

    #[tokio::test]
    async fn test_password_auth_authorization() -> Result<()> {
        let temp_dir = TempDir::new().unwrap();
        std::env::set_var("HOME", temp_dir.path());

        let auth_handler = PasswordAuthHandler::new().await?;

        // Add a user
        let client_id = auth_handler.add_user("testuser".to_string(), "testpass".to_string()).await?;

        // Test authorization (users get "all" permission by default)
        let has_permission = auth_handler.authorize(&client_id, "session:create").await?;
        assert!(has_permission);

        // Test with non-existent user
        let fake_client_id = crate::protocol::ClientId(uuid::Uuid::new_v4());
        let no_permission = auth_handler.authorize(&fake_client_id, "session:create").await?;
        assert!(!no_permission);

        Ok(())
    }

    #[tokio::test]
    async fn test_remote_server_creation() -> Result<()> {
        let temp_dir = TempDir::new().unwrap();
        std::env::set_var("HOME", temp_dir.path());

        let bind_addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let server = Arc::new(crate::server::Server::new(temp_dir.path().join("test.sock")));
        let auth_handler = Arc::new(PasswordAuthHandler::new().await?);

        let remote_server = RemoteServer::new(bind_addr, server, auth_handler);

        // Server should be created successfully
        // Note: We don't actually start it to avoid binding to ports in tests
        Ok(())
    }

    #[tokio::test]
    async fn test_remote_client_creation() -> Result<()> {
        let server_addr: SocketAddr = "127.0.0.1:9999".parse().unwrap();
        let credentials = AuthCredentials {
            username: "testuser".to_string(),
            password: Some("testpass".to_string()),
            token: None,
            certificate: None,
        };

        let remote_client = RemoteClient::new(server_addr, credentials.clone());

        // Client should be created successfully
        // Note: We don't test actual connection to avoid network dependencies
        Ok(())
    }

    #[tokio::test]
    async fn test_auth_credentials_serialization() -> Result<()> {
        let credentials = AuthCredentials {
            username: "testuser".to_string(),
            password: Some("testpass".to_string()),
            key: Some("testkey".to_string()),
        };

        // Test serialization
        let json = serde_json::to_string(&credentials)?;
        assert!(json.contains("testuser"));

        // Test deserialization
        let deserialized: AuthCredentials = serde_json::from_str(&json)?;
        assert_eq!(deserialized.username, credentials.username);
        assert_eq!(deserialized.password, credentials.password);
        assert_eq!(deserialized.key, credentials.key);

        Ok(())
    }

    #[tokio::test]
    async fn test_default_admin_creation() -> Result<()> {
        let temp_dir = TempDir::new().unwrap();
        std::env::set_var("HOME", temp_dir.path());

        let auth_handler = PasswordAuthHandler::new().await?;

        // Ensure default admin (user_store is private)
        auth_handler.ensure_default_admin().await?;

        // Test admin login
        let admin_credentials = AuthCredentials {
            username: "admin".to_string(),
            password: Some("password".to_string()),
            key: None,
        };

        let auth_result = auth_handler.authenticate(&admin_credentials).await;
        assert!(auth_result.is_ok());

        Ok(())
    }

    #[test]
    fn test_remote_message_types() -> Result<()> {
        // Test that remote-specific messages can be created and serialized
        let auth_message = ClientMessage::Authenticate(AuthCredentials {
            username: "test".to_string(),
            password: Some("pass".to_string()),
            key: None,
        });

        let json = serde_json::to_string(&auth_message)?;
        assert!(json.contains("Authenticate"));

        let auth_response = ServerMessage::Authenticated {
            client_id: crate::protocol::ClientId(uuid::Uuid::new_v4()),
        };

        let json = serde_json::to_string(&auth_response)?;
        assert!(json.contains("Authenticated"));

        Ok(())
    }

    #[tokio::test]
    async fn test_concurrent_auth_operations() -> Result<()> {
        let temp_dir = TempDir::new().unwrap();
        std::env::set_var("HOME", temp_dir.path());

        let auth_handler = Arc::new(PasswordAuthHandler::new().await?);

        // Test concurrent user creation
        let mut handles = vec![];
        for i in 0..5 {
            let auth_handler = auth_handler.clone();
            let handle = tokio::spawn(async move {
                auth_handler.add_user(
                    format!("user{}", i),
                    format!("pass{}", i),
                ).await
            });
            handles.push(handle);
        }

        // Wait for all operations to complete
        for handle in handles {
            let result = handle.await.unwrap();
            assert!(result.is_ok());
        }

        // All operations should succeed (user_store is private)

        Ok(())
    }
}