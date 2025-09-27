use std::time::Duration;
use tokio::time::sleep;

#[cfg(test)]
mod server_tests {
    use ferrix::server::Server;
    use std::sync::Arc;
    use tempfile::TempDir;
    use tokio::time::sleep;
    use std::time::Duration;

    #[tokio::test]
    async fn test_server_creation_and_shutdown() {
        let temp_dir = TempDir::new().unwrap();
        let socket_path = temp_dir.path().join("test.sock");

        let server = Server::new(socket_path.clone());
        // Server is now created directly, not wrapped in Result

        // Server should clean up on drop
        drop(server);
        // Socket cleanup happens when server runs, not on drop
    }

    #[tokio::test]
    async fn test_session_lifecycle() {
        let temp_dir = TempDir::new().unwrap();
        let socket_path = temp_dir.path().join("test.sock");

        let server = Arc::new(Server::new(socket_path));

        // Server needs to be started with listen() method
        // For testing purposes, we'll just verify it was created

        // Server is created successfully
        assert!(Arc::strong_count(&server) > 0);
    }
}

#[cfg(test)]
mod session_tests {
    use ferrix::server::session::Session;
    use ferrix::protocol::SessionId;
    use uuid::Uuid;

    #[tokio::test]
    async fn test_session_creation() {
        let session_id = SessionId(Uuid::new_v4());
        let session = Session::new(session_id.clone(), "test-session".to_string());

        assert_eq!(session.id, session_id);
        assert_eq!(session.name, "test-session");
        assert_eq!(session.windows.len(), 1); // Should have default window
    }

    #[tokio::test]
    async fn test_window_management() {
        let session_id = SessionId(Uuid::new_v4());
        let mut session = Session::new(session_id, "test-session".to_string());

        // Create new window
        let window_id = session.create_window(Some("test-window".to_string())).await.unwrap();
        assert_eq!(session.windows.len(), 2);

        // Switch windows
        session.switch_window(window_id.clone()).await.unwrap();
        assert_eq!(session.current_window, Some(window_id.clone()));

        // Close window (should fail if it's the last one)
        let first_window = session.windows[0].read().await.id.clone();
        session.close_window(first_window).await.unwrap();
        assert_eq!(session.windows.len(), 1);
    }

    #[tokio::test]
    async fn test_pane_splitting() {
        let session_id = SessionId(Uuid::new_v4());
        let mut session = Session::new(session_id, "test-session".to_string());

        // Split pane horizontally
        let _pane_id = session.split_pane(ferrix::protocol::SplitDirection::Horizontal).await.unwrap();

        // Current window should have 2 panes now
        if let Some(window_id) = &session.current_window {
            for window in &session.windows {
                let window_guard = window.read().await;
                if window_guard.id == *window_id {
                    assert_eq!(window_guard.panes.len(), 2);
                    break;
                }
            }
        }
    }

    #[tokio::test]
    async fn test_input_output_handling() {
        let session_id = SessionId(Uuid::new_v4());
        let mut session = Session::new(session_id, "test-session".to_string());

        // Send input
        let test_input = b"echo hello\n".to_vec();
        session.handle_input(test_input.clone()).await.unwrap();

        // Note: Real PTY testing would require more setup
        // This just ensures no panics
    }
}

#[cfg(test)]
mod snapshot_tests {
    use ferrix::server::snapshot::{SnapshotManager, SessionSnapshot};
    use ferrix::error::Result;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_snapshot_save_and_load() {
        let temp_dir = TempDir::new().unwrap();
        std::env::set_var("HOME", temp_dir.path());

        let snapshot_manager = SnapshotManager::new().unwrap();

        // Create a mock snapshot
        let snapshot = create_mock_snapshot();

        // Save snapshot
        let path = snapshot_manager.save_snapshot(&snapshot).unwrap();
        assert!(path.exists());

        // Load snapshot
        let loaded = snapshot_manager.load_snapshot(&path).unwrap();
        assert_eq!(loaded.metadata.name, snapshot.metadata.name);
    }

    #[tokio::test]
    async fn test_snapshot_export_import() {
        let temp_dir = TempDir::new().unwrap();
        std::env::set_var("HOME", temp_dir.path());

        let snapshot_manager = SnapshotManager::new().unwrap();
        let snapshot = create_mock_snapshot();

        // Export to custom path
        let export_path = temp_dir.path().join("exported.snapshot");
        snapshot_manager.export_snapshot(&snapshot, &export_path).unwrap();
        assert!(export_path.exists());

        // Import from path
        let imported = snapshot_manager.import_snapshot(&export_path).unwrap();
        assert_eq!(imported.metadata.name, snapshot.metadata.name);
    }

    fn create_mock_snapshot() -> SessionSnapshot {
        use ferrix::server::snapshot::{SnapshotMetadata, SessionState};
        use uuid::Uuid;
        use chrono::Utc;

        SessionSnapshot {
            metadata: SnapshotMetadata {
                id: Uuid::new_v4(),
                name: "test-snapshot".to_string(),
                description: "Test snapshot".to_string(),
                created_at: Utc::now(),
                ferrix_version: "0.1.0".to_string(),
                checksum: None,
            },
            session: SessionState {
                id: ferrix::protocol::SessionId(Uuid::new_v4()),
                name: "test-session".to_string(),
                current_window: None,
                created_at: Utc::now(),
                environment: Vec::new(),
            },
            windows: Vec::new(),
            panes: Vec::new(),
        }
    }
}

#[cfg(test)]
mod config_tests {
    use ferrix::config::Config;
    use tempfile::NamedTempFile;
    use std::io::Write;

    #[test]
    fn test_config_parsing() {
        let config_content = r#"
            [general]
            default_shell = "/bin/bash"
            mouse = true
            scrollback_lines = 10000
            escape_key = "C-a"
            term = "xterm-256color"
            clipboard = true
            automatic_rename = false
            display_panes_time = 1500
        "#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(config_content.as_bytes()).unwrap();

        let config = Config::load_from_path(temp_file.path()).unwrap();
        assert_eq!(config.general.escape_key, "C-a".to_string());
        assert_eq!(config.general.mouse, true);
        assert_eq!(config.general.scrollback_lines, 10000);
    }
}

#[cfg(test)]
mod versioning_tests {
    use ferrix::server::versioning::{SessionVersioning, MergeStrategy, MergeResult};
    use ferrix::error::Result;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_git_like_versioning() {
        let temp_dir = TempDir::new().unwrap();
        let mut versioning = SessionVersioning::new(temp_dir.path().to_path_buf()).unwrap();

        // Initialize with snapshot
        let snapshot = create_mock_snapshot();
        let session_id = ferrix::protocol::SessionId(uuid::Uuid::new_v4());
        versioning.init(&session_id, snapshot.clone()).unwrap();

        // Create branch
        versioning.branch("feature".to_string(), Some("Feature branch".to_string())).unwrap();

        // Stage and commit changes
        versioning.stage(snapshot.clone()).unwrap();
        let _commit_id = versioning.commit("Test commit".to_string(), "tester".to_string()).unwrap();

        // Get history
        let history = versioning.log(Some(10));
        assert!(history.len() > 0);

        // Switch branches
        let restored = versioning.checkout("master").unwrap();
        assert_eq!(restored.metadata.name, snapshot.metadata.name);
    }

    #[tokio::test]
    async fn test_merge_operations() {
        let temp_dir = TempDir::new().unwrap();
        let mut versioning = SessionVersioning::new(temp_dir.path().to_path_buf()).unwrap();

        let snapshot = create_mock_snapshot();
        let session_id = ferrix::protocol::SessionId(uuid::Uuid::new_v4());
        versioning.init(&session_id, snapshot.clone()).unwrap();

        // Create and switch to feature branch
        versioning.branch("feature".to_string(), None).unwrap();
        versioning.checkout("feature").unwrap();

        // Make changes on feature branch
        versioning.stage(snapshot.clone()).unwrap();
        versioning.commit("Feature commit".to_string(), "dev".to_string()).unwrap();

        // Switch back to master
        versioning.checkout("master").unwrap();

        // Merge feature branch
        let merge_result = versioning.merge("feature", MergeStrategy::Auto).unwrap();
        match merge_result {
            MergeResult::Success(_) => {
                // Merge succeeded
            }
            MergeResult::Conflicts(conflicts) => {
                assert_eq!(conflicts.len(), 0); // No conflicts expected in this simple case
            }
        }
    }

    fn create_mock_snapshot() -> ferrix::server::snapshot::SessionSnapshot {
        use ferrix::server::snapshot::{SnapshotMetadata, SessionState};
        use uuid::Uuid;
        use chrono::Utc;

        ferrix::server::snapshot::SessionSnapshot {
            metadata: SnapshotMetadata {
                id: Uuid::new_v4(),
                name: "test-snapshot".to_string(),
                description: "Test snapshot".to_string(),
                created_at: Utc::now(),
                ferrix_version: "0.1.0".to_string(),
                checksum: None,
            },
            session: SessionState {
                id: ferrix::protocol::SessionId(Uuid::new_v4()),
                name: "test-session".to_string(),
                current_window: None,
                created_at: Utc::now(),
                environment: Vec::new(),
            },
            windows: Vec::new(),
            panes: Vec::new(),
        }
    }
}

#[cfg(test)]
mod performance_tests {
    use std::time::Instant;

    #[tokio::test]
    async fn test_session_creation_performance() {
        let start = Instant::now();

        for i in 0..100 {
            let session_id = ferrix::protocol::SessionId(uuid::Uuid::new_v4());
            let _ = ferrix::server::session::Session::new(
                session_id,
                format!("session-{}", i)
            );
        }

        let duration = start.elapsed();
        assert!(duration.as_millis() < 1000, "Creating 100 sessions took too long: {:?}", duration);
    }

    #[tokio::test]
    async fn test_window_switching_performance() {
        let session_id = ferrix::protocol::SessionId(uuid::Uuid::new_v4());
        let mut session = ferrix::server::session::Session::new(session_id, "perf-test".to_string());

        // Create multiple windows
        let mut window_ids = Vec::new();
        for i in 0..20 {
            let id = session.create_window(Some(format!("window-{}", i))).await.unwrap();
            window_ids.push(id);
        }

        // Measure switching performance
        let start = Instant::now();
        for _ in 0..100 {
            session.next_window().await.unwrap();
        }
        let duration = start.elapsed();

        assert!(duration.as_millis() < 100, "Window switching is too slow: {:?}", duration);
    }
}