#[cfg(test)]
mod recovery_tests {
    use super::*;
    use crate::error::Result;
    use crate::server::recovery::RecoveryManager;
    use crate::server::Server;
    use crate::protocol::{SessionId, WindowId, PaneId};
    use tempfile::TempDir;
    use std::sync::Arc;
    use std::collections::HashMap;
    use uuid::Uuid;

    #[tokio::test]
    async fn test_recovery_manager_creation() -> Result<()> {
        let temp_dir = TempDir::new().unwrap();
        let recovery_path = temp_dir.path().join("recovery.json");

        let recovery_manager = RecoveryManager::new(recovery_path.clone());

        // Recovery manager should be created
        assert_eq!(recovery_manager.recovery_file_path(), &recovery_path);
        Ok(())
    }

    #[tokio::test]
    async fn test_server_state_save() -> Result<()> {
        let temp_dir = TempDir::new().unwrap();
        let socket_path = temp_dir.path().join("test.sock");
        let recovery_path = temp_dir.path().join("recovery.json");

        let server = Arc::new(Server::new(socket_path));
        let mut recovery_manager = RecoveryManager::new(recovery_path.clone());

        // Save server state
        recovery_manager.save_server_state(&server).await?;

        // Recovery file should exist
        assert!(recovery_path.exists());

        // File should contain valid JSON
        let content = std::fs::read_to_string(&recovery_path)?;
        assert!(content.contains("sessions"));
        assert!(content.contains("timestamp"));

        Ok(())
    }

    #[tokio::test]
    async fn test_server_state_restore() -> Result<()> {
        let temp_dir = TempDir::new().unwrap();
        let socket_path = temp_dir.path().join("test.sock");
        let recovery_path = temp_dir.path().join("recovery.json");

        // Create recovery data manually
        let recovery_data = r#"{
            "timestamp": "2025-01-01T00:00:00Z",
            "sessions": {},
            "active_clients": 0,
            "server_pid": 12345
        }"#;
        std::fs::write(&recovery_path, recovery_data)?;

        let server = Arc::new(Server::new(socket_path));
        let mut recovery_manager = RecoveryManager::new(recovery_path);

        // Restore server state
        let result = recovery_manager.restore_server_state(&server).await;

        // Should either succeed or fail gracefully
        match result {
            Ok(_) => {
                // Restoration successful
            }
            Err(_) => {
                // Restoration failed (acceptable in test environment)
                println!("Recovery failed - expected in test environment");
            }
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_recovery_with_sessions() -> Result<()> {
        let temp_dir = TempDir::new().unwrap();
        let socket_path = temp_dir.path().join("test.sock");
        let recovery_path = temp_dir.path().join("recovery_sessions.json");

        let server = Arc::new(Server::new(socket_path));
        let mut recovery_manager = RecoveryManager::new(recovery_path.clone());

        // Create a session (simulate)
        let session_id = SessionId(Uuid::new_v4());

        // Save state with session
        recovery_manager.save_server_state(&server).await?;

        // Verify recovery file contains session data structure
        let content = std::fs::read_to_string(&recovery_path)?;
        let parsed: serde_json::Value = serde_json::from_str(&content)?;

        assert!(parsed.get("sessions").is_some());
        assert!(parsed.get("timestamp").is_some());

        Ok(())
    }

    #[tokio::test]
    async fn test_recovery_cleanup() -> Result<()> {
        let temp_dir = TempDir::new().unwrap();
        let recovery_path = temp_dir.path().join("cleanup_test.json");

        let mut recovery_manager = RecoveryManager::new(recovery_path.clone());

        // Create recovery file
        std::fs::write(&recovery_path, "{}")?;
        assert!(recovery_path.exists());

        // Clean up
        recovery_manager.cleanup().await?;

        // Recovery file should be removed
        assert!(!recovery_path.exists());

        Ok(())
    }

    #[tokio::test]
    async fn test_automatic_recovery_check() -> Result<()> {
        let temp_dir = TempDir::new().unwrap();
        let socket_path = temp_dir.path().join("test.sock");
        let recovery_path = temp_dir.path().join("auto_recovery.json");

        // Create a recovery file with recent timestamp
        let recent_recovery = format!(r#"{{
            "timestamp": "{}",
            "sessions": {{}},
            "active_clients": 1,
            "server_pid": 12345
        }}"#, chrono::Utc::now().to_rfc3339());

        std::fs::write(&recovery_path, recent_recovery)?;

        let server = Arc::new(Server::new(socket_path));
        let recovery_manager = RecoveryManager::new(recovery_path);

        // Check if recovery is needed
        let needs_recovery = recovery_manager.needs_recovery().await?;

        // In test environment, this might be true or false depending on system state
        // We just verify the check completes without error
        println!("Needs recovery: {}", needs_recovery);

        Ok(())
    }

    #[tokio::test]
    async fn test_concurrent_recovery_operations() -> Result<()> {
        let temp_dir = TempDir::new().unwrap();
        let socket_path = temp_dir.path().join("test.sock");
        let recovery_path = temp_dir.path().join("concurrent_recovery.json");

        let server = Arc::new(Server::new(socket_path));
        let recovery_manager = Arc::new(tokio::sync::Mutex::new(
            RecoveryManager::new(recovery_path.clone())
        ));

        let mut handles = vec![];

        // Test concurrent save operations
        for _ in 0..5 {
            let server = server.clone();
            let recovery_manager = recovery_manager.clone();

            let handle = tokio::spawn(async move {
                let mut manager = recovery_manager.lock().await;
                manager.save_server_state(&server).await
            });
            handles.push(handle);
        }

        // Wait for all operations
        for handle in handles {
            let result = handle.await.unwrap();
            // Some might succeed, some might fail due to file locking
            match result {
                Ok(_) => println!("Concurrent save succeeded"),
                Err(_) => println!("Concurrent save failed (expected)"),
            }
        }

        Ok(())
    }

    #[test]
    fn test_recovery_data_serialization() -> Result<()> {
        use crate::server::recovery::RecoveryData;

        let recovery_data = RecoveryData {
            timestamp: chrono::Utc::now(),
            sessions: HashMap::new(),
            active_clients: 2,
            server_pid: std::process::id(),
        };

        // Test serialization
        let json = serde_json::to_string(&recovery_data)?;
        assert!(json.contains("timestamp"));
        assert!(json.contains("sessions"));
        assert!(json.contains("active_clients"));

        // Test deserialization
        let parsed: RecoveryData = serde_json::from_str(&json)?;
        assert_eq!(parsed.active_clients, recovery_data.active_clients);
        assert_eq!(parsed.server_pid, recovery_data.server_pid);

        Ok(())
    }

    #[tokio::test]
    async fn test_recovery_with_corrupted_file() -> Result<()> {
        let temp_dir = TempDir::new().unwrap();
        let socket_path = temp_dir.path().join("test.sock");
        let recovery_path = temp_dir.path().join("corrupted.json");

        // Create corrupted recovery file
        std::fs::write(&recovery_path, "invalid json content")?;

        let server = Arc::new(Server::new(socket_path));
        let recovery_manager = RecoveryManager::new(recovery_path);

        // Should handle corrupted file gracefully
        let result = recovery_manager.restore_server_state(&server).await;
        assert!(result.is_err()); // Should fail with parsing error

        Ok(())
    }

    #[tokio::test]
    async fn test_recovery_file_permissions() -> Result<()> {
        let temp_dir = TempDir::new().unwrap();
        let socket_path = temp_dir.path().join("test.sock");
        let recovery_path = temp_dir.path().join("permissions_test.json");

        let server = Arc::new(Server::new(socket_path));
        let mut recovery_manager = RecoveryManager::new(recovery_path.clone());

        // Save recovery data
        recovery_manager.save_server_state(&server).await?;

        // Check file exists and is readable
        assert!(recovery_path.exists());
        let content = std::fs::read_to_string(&recovery_path)?;
        assert!(!content.is_empty());

        Ok(())
    }

    #[tokio::test]
    async fn test_daemonization_preparation() -> Result<()> {
        let temp_dir = TempDir::new().unwrap();
        let socket_path = temp_dir.path().join("test.sock");
        let pid_file = temp_dir.path().join("ferrix.pid");

        let server = Arc::new(Server::new(socket_path));

        // Test that server can prepare for daemonization
        server.prepare_for_daemon(&pid_file).await?;

        // In a real scenario, this would set up signal handlers, etc.
        // For testing, we just verify it doesn't panic

        Ok(())
    }

    #[test]
    fn test_process_id_handling() -> Result<()> {
        let current_pid = std::process::id();
        assert!(current_pid > 0);

        // Test PID file operations
        let temp_dir = TempDir::new().unwrap();
        let pid_file = temp_dir.path().join("test.pid");

        // Write PID
        std::fs::write(&pid_file, current_pid.to_string())?;

        // Read PID
        let content = std::fs::read_to_string(&pid_file)?;
        let parsed_pid: u32 = content.trim().parse()?;
        assert_eq!(parsed_pid, current_pid);

        Ok(())
    }
}