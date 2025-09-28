use std::fs;
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::error::{FerrixError, Result};
use crate::protocol::{SessionId, WindowId, PaneId};
use super::layout::Layout;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSnapshot {
    pub metadata: SnapshotMetadata,
    pub session: SessionState,
    pub windows: Vec<WindowState>,
    pub panes: Vec<PaneState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotMetadata {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub created_at: DateTime<Utc>,
    pub ferrix_version: String,
    pub checksum: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionState {
    pub id: SessionId,
    pub name: String,
    pub current_window: Option<WindowId>,
    pub created_at: DateTime<Utc>,
    pub environment: Vec<(String, String)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowState {
    pub id: WindowId,
    pub session_id: SessionId,
    pub name: String,
    pub index: usize,
    pub layout: Layout,
    pub current_pane: Option<PaneId>,
    pub width: u16,
    pub height: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaneState {
    pub id: PaneId,
    pub window_id: WindowId,
    pub working_directory: PathBuf,
    pub command: String,
    pub cols: u16,
    pub rows: u16,
    pub scrollback: Vec<String>,
    pub cursor_position: (u16, u16),
}

pub struct SnapshotManager {
    snapshot_dir: PathBuf,
}

impl SnapshotManager {
    pub fn new() -> Result<Self> {
        let snapshot_dir = Self::get_snapshot_dir()?;
        fs::create_dir_all(&snapshot_dir)
            .map_err(|e| FerrixError::Other(format!("Failed to create snapshot directory: {}", e)))?;

        Ok(Self { snapshot_dir })
    }

    pub fn save_snapshot(&self, snapshot: &SessionSnapshot) -> Result<PathBuf> {
        // Sanitize session name for filesystem
        let safe_name = snapshot.session.name
            .replace('/', "_")
            .replace('\\', "_")
            .replace(':', "_")
            .replace('*', "_")
            .replace('?', "_")
            .replace('"', "_")
            .replace('<', "_")
            .replace('>', "_")
            .replace('|', "_");

        let filename = format!(
            "{}_{}_{}.ferrix.snapshot",
            safe_name,
            snapshot.metadata.created_at.format("%Y%m%d_%H%M%S"),
            snapshot.metadata.id.as_simple()
        );

        let path = self.snapshot_dir.join(filename);

        // Calculate checksum before saving
        let mut snapshot_with_checksum = snapshot.clone();
        snapshot_with_checksum.metadata.checksum = Some(self.calculate_checksum(&snapshot)?);

        // Serialize to JSON (could also use bincode for smaller size)
        let json = serde_json::to_string_pretty(&snapshot_with_checksum)
            .map_err(|e| FerrixError::Other(format!("Failed to serialize snapshot: {}", e)))?;

        // Write to file
        fs::write(&path, json)
            .map_err(|e| FerrixError::Other(format!("Failed to write snapshot file: {}", e)))?;

        Ok(path)
    }

    pub fn load_snapshot(&self, path: &Path) -> Result<SessionSnapshot> {
        let json = fs::read_to_string(path)
            .map_err(|e| FerrixError::Other(format!("Failed to read snapshot file: {}", e)))?;

        let snapshot: SessionSnapshot = serde_json::from_str(&json)
            .map_err(|e| FerrixError::Other(format!("Failed to deserialize snapshot: {}", e)))?;

        // Verify checksum if present
        if let Some(stored_checksum) = &snapshot.metadata.checksum {
            let calculated_checksum = self.calculate_checksum(&snapshot)?;
            if stored_checksum != &calculated_checksum {
                return Err(FerrixError::Other("Snapshot checksum verification failed".to_string()));
            }
        }

        Ok(snapshot)
    }

    pub fn list_snapshots(&self) -> Result<Vec<SnapshotInfo>> {
        let mut snapshots = Vec::new();

        for entry in fs::read_dir(&self.snapshot_dir)
            .map_err(|e| FerrixError::Other(format!("Failed to read snapshot directory: {}", e)))?
        {
            let entry = entry.map_err(|e| FerrixError::Other(format!("Failed to read directory entry: {}", e)))?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("snapshot") {
                if let Ok(snapshot) = self.load_snapshot(&path) {
                    snapshots.push(SnapshotInfo {
                        path: path.clone(),
                        metadata: snapshot.metadata.clone(),
                        session_name: snapshot.session.name.clone(),
                    });
                }
            }
        }

        // Sort by creation time (newest first)
        snapshots.sort_by(|a, b| b.metadata.created_at.cmp(&a.metadata.created_at));

        Ok(snapshots)
    }

    pub fn delete_snapshot(&self, path: &Path) -> Result<()> {
        fs::remove_file(path)
            .map_err(|e| FerrixError::Other(format!("Failed to delete snapshot: {}", e)))?;
        Ok(())
    }

    pub async fn auto_snapshot(&self, snapshot: &SessionSnapshot) -> Result<PathBuf> {
        // Auto-snapshots go in a subdirectory
        let auto_dir = self.snapshot_dir.join("auto");
        fs::create_dir_all(&auto_dir)
            .map_err(|e| FerrixError::Other(format!("Failed to create auto snapshot directory: {}", e)))?;

        let filename = format!(
            "auto_{}_{}_{}.ferrix.snapshot",
            snapshot.session.name,
            snapshot.metadata.created_at.format("%Y%m%d_%H%M%S"),
            snapshot.metadata.id
        );

        let path = auto_dir.join(filename);

        // Serialize to JSON
        let json = serde_json::to_string(&snapshot)
            .map_err(|e| FerrixError::Other(format!("Failed to serialize auto snapshot: {}", e)))?;

        fs::write(&path, json)
            .map_err(|e| FerrixError::Other(format!("Failed to write auto snapshot: {}", e)))?;

        // Clean up old auto-snapshots (keep last 10)
        self.cleanup_auto_snapshots(10)?;

        Ok(path)
    }

    fn cleanup_auto_snapshots(&self, keep_count: usize) -> Result<()> {
        let auto_dir = self.snapshot_dir.join("auto");

        if !auto_dir.exists() {
            return Ok(());
        }

        let mut entries: Vec<_> = fs::read_dir(&auto_dir)
            .map_err(|e| FerrixError::Other(format!("Failed to read auto snapshot directory: {}", e)))?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("snapshot"))
            .collect();

        // Sort by modification time (oldest first)
        entries.sort_by_key(|e| e.metadata().ok().and_then(|m| m.modified().ok()));

        // Remove old snapshots if we have more than keep_count
        if entries.len() > keep_count {
            for entry in entries.iter().take(entries.len() - keep_count) {
                let _ = fs::remove_file(entry.path());
            }
        }

        Ok(())
    }

    fn calculate_checksum(&self, snapshot: &SessionSnapshot) -> Result<String> {
        // Simple checksum using session ID and timestamp
        // In production, would use proper cryptographic hash
        let data = format!(
            "{}:{}:{}",
            snapshot.session.id.0,
            snapshot.metadata.created_at.timestamp(),
            snapshot.windows.len()
        );

        Ok(format!("{:x}", md5::compute(data.as_bytes())))
    }

    fn get_snapshot_dir() -> Result<PathBuf> {
        if let Ok(path) = std::env::var("FERRIX_SNAPSHOT_DIR") {
            return Ok(PathBuf::from(path));
        }

        if let Some(home) = dirs::home_dir() {
            Ok(home.join(".ferrix").join("snapshots"))
        } else {
            Ok(PathBuf::from("/tmp/ferrix/snapshots"))
        }
    }

    pub fn export_snapshot(&self, snapshot: &SessionSnapshot, export_path: &Path) -> Result<()> {
        // Export as compressed archive
        let json = serde_json::to_string_pretty(snapshot)
            .map_err(|e| FerrixError::Other(format!("Failed to serialize for export: {}", e)))?;

        // Use flate2 for compression
        use flate2::write::GzEncoder;
        use flate2::Compression;
        use std::io::Write;

        let file = fs::File::create(export_path)
            .map_err(|e| FerrixError::Other(format!("Failed to create export file: {}", e)))?;

        let mut encoder = GzEncoder::new(file, Compression::default());
        encoder.write_all(json.as_bytes())
            .map_err(|e| FerrixError::Other(format!("Failed to compress snapshot: {}", e)))?;

        encoder.finish()
            .map_err(|e| FerrixError::Other(format!("Failed to finish compression: {}", e)))?;

        Ok(())
    }

    pub fn import_snapshot(&self, import_path: &Path) -> Result<SessionSnapshot> {
        use flate2::read::GzDecoder;
        use std::io::Read;

        let file = fs::File::open(import_path)
            .map_err(|e| FerrixError::Other(format!("Failed to open import file: {}", e)))?;

        let mut decoder = GzDecoder::new(file);
        let mut json = String::new();
        decoder.read_to_string(&mut json)
            .map_err(|e| FerrixError::Other(format!("Failed to decompress snapshot: {}", e)))?;

        let snapshot: SessionSnapshot = serde_json::from_str(&json)
            .map_err(|e| FerrixError::Other(format!("Failed to deserialize imported snapshot: {}", e)))?;

        Ok(snapshot)
    }
}

#[derive(Debug, Clone)]
pub struct SnapshotInfo {
    pub path: PathBuf,
    pub metadata: SnapshotMetadata,
    pub session_name: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::env;
    use uuid::Uuid;

    fn create_test_snapshot() -> SessionSnapshot {
        let session_id = SessionId(Uuid::new_v4());
        let window_id = WindowId(Uuid::new_v4());
        let pane_id = PaneId(Uuid::new_v4());

        SessionSnapshot {
            metadata: SnapshotMetadata {
                id: Uuid::new_v4(),
                name: "test_snapshot".to_string(),
                description: "Test snapshot description".to_string(),
                created_at: Utc::now(),
                ferrix_version: "0.1.0".to_string(),
                checksum: None,
            },
            session: SessionState {
                id: session_id.clone(),
                name: "test_session".to_string(),
                current_window: Some(window_id.clone()),
                created_at: Utc::now(),
                environment: vec![("HOME".to_string(), "/home/test".to_string())],
            },
            windows: vec![WindowState {
                id: window_id.clone(),
                session_id,
                name: "test_window".to_string(),
                index: 0,
                layout: Layout::new(pane_id.clone()),
                current_pane: Some(pane_id.clone()),
                width: 80,
                height: 24,
            }],
            panes: vec![PaneState {
                id: pane_id,
                window_id,
                working_directory: PathBuf::from("/"),
                command: "/bin/bash".to_string(),
                cols: 80,
                rows: 24,
                scrollback: vec!["line 1".to_string(), "line 2".to_string()],
                cursor_position: (0, 0),
            }],
        }
    }

    #[test]
    fn test_snapshot_manager_creation() {
        let temp_dir = TempDir::new().unwrap();
        env::set_var("FERRIX_SNAPSHOT_DIR", temp_dir.path());

        let manager = SnapshotManager::new();
        assert!(manager.is_ok());

        let manager = manager.unwrap();
        assert_eq!(manager.snapshot_dir, temp_dir.path());
    }

    #[test]
    fn test_snapshot_save_and_load() {
        let temp_dir = TempDir::new().unwrap();
        env::set_var("FERRIX_SNAPSHOT_DIR", temp_dir.path());

        let manager = SnapshotManager::new().unwrap();
        let snapshot = create_test_snapshot();

        // Save snapshot
        let save_result = manager.save_snapshot(&snapshot);
        assert!(save_result.is_ok());

        let saved_path = save_result.unwrap();
        assert!(saved_path.exists());

        // Load snapshot
        let load_result = manager.load_snapshot(&saved_path);
        assert!(load_result.is_ok());

        let loaded_snapshot = load_result.unwrap();
        assert_eq!(loaded_snapshot.metadata.name, snapshot.metadata.name);
        assert_eq!(loaded_snapshot.session.name, snapshot.session.name);
        assert_eq!(loaded_snapshot.windows.len(), snapshot.windows.len());
        assert_eq!(loaded_snapshot.panes.len(), snapshot.panes.len());
    }

    #[test]
    fn test_snapshot_checksum_verification() {
        let temp_dir = TempDir::new().unwrap();
        env::set_var("FERRIX_SNAPSHOT_DIR", temp_dir.path());

        let manager = SnapshotManager::new().unwrap();
        let snapshot = create_test_snapshot();

        // Save snapshot (this will calculate and add checksum)
        let saved_path = manager.save_snapshot(&snapshot).unwrap();

        // Load snapshot (this will verify checksum)
        let load_result = manager.load_snapshot(&saved_path);
        assert!(load_result.is_ok());
    }

    #[test]
    fn test_snapshot_list() {
        let temp_dir = TempDir::new().unwrap();
        env::set_var("FERRIX_SNAPSHOT_DIR", temp_dir.path());

        let manager = SnapshotManager::new().unwrap();

        // Initially no snapshots
        let snapshots = manager.list_snapshots().unwrap();
        assert_eq!(snapshots.len(), 0);

        // Save a snapshot
        let snapshot = create_test_snapshot();
        manager.save_snapshot(&snapshot).unwrap();

        // Should now have one snapshot
        let snapshots = manager.list_snapshots().unwrap();
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].session_name, "test_session");
    }

    #[test]
    fn test_snapshot_delete() {
        let temp_dir = TempDir::new().unwrap();
        env::set_var("FERRIX_SNAPSHOT_DIR", temp_dir.path());

        let manager = SnapshotManager::new().unwrap();
        let snapshot = create_test_snapshot();

        // Save snapshot
        let saved_path = manager.save_snapshot(&snapshot).unwrap();
        assert!(saved_path.exists());

        // Delete snapshot
        let delete_result = manager.delete_snapshot(&saved_path);
        assert!(delete_result.is_ok());
        assert!(!saved_path.exists());
    }

    #[tokio::test]
    async fn test_auto_snapshot() {
        let temp_dir = TempDir::new().unwrap();
        env::set_var("FERRIX_SNAPSHOT_DIR", temp_dir.path());

        let manager = SnapshotManager::new().unwrap();
        let snapshot = create_test_snapshot();

        // Save auto snapshot
        let auto_result = manager.auto_snapshot(&snapshot).await;
        assert!(auto_result.is_ok());

        let auto_path = auto_result.unwrap();
        assert!(auto_path.exists());
        assert!(auto_path.to_string_lossy().contains("auto"));
    }

    #[test]
    fn test_snapshot_export_import() {
        let temp_dir = TempDir::new().unwrap();
        env::set_var("FERRIX_SNAPSHOT_DIR", temp_dir.path());

        let manager = SnapshotManager::new().unwrap();
        let snapshot = create_test_snapshot();

        let export_path = temp_dir.path().join("export.gz");

        // Export snapshot
        let export_result = manager.export_snapshot(&snapshot, &export_path);
        assert!(export_result.is_ok());
        assert!(export_path.exists());

        // Import snapshot
        let import_result = manager.import_snapshot(&export_path);
        assert!(import_result.is_ok());

        let imported_snapshot = import_result.unwrap();
        assert_eq!(imported_snapshot.metadata.name, snapshot.metadata.name);
        assert_eq!(imported_snapshot.session.name, snapshot.session.name);
    }

    #[test]
    fn test_snapshot_metadata() {
        let metadata = SnapshotMetadata {
            id: Uuid::new_v4(),
            name: "test".to_string(),
            description: "test description".to_string(),
            created_at: Utc::now(),
            ferrix_version: "1.0.0".to_string(),
            checksum: Some("abc123".to_string()),
        };

        assert_eq!(metadata.name, "test");
        assert_eq!(metadata.description, "test description");
        assert_eq!(metadata.ferrix_version, "1.0.0");
        assert_eq!(metadata.checksum, Some("abc123".to_string()));
    }

    #[test]
    fn test_session_state() {
        let session_id = SessionId(Uuid::new_v4());
        let window_id = WindowId(Uuid::new_v4());

        let session_state = SessionState {
            id: session_id.clone(),
            name: "test_session".to_string(),
            current_window: Some(window_id.clone()),
            created_at: Utc::now(),
            environment: vec![("PATH".to_string(), "/usr/bin".to_string())],
        };

        assert_eq!(session_state.id, session_id);
        assert_eq!(session_state.name, "test_session");
        assert_eq!(session_state.current_window, Some(window_id));
        assert_eq!(session_state.environment.len(), 1);
    }

    #[test]
    fn test_window_state() {
        let window_id = WindowId(Uuid::new_v4());
        let session_id = SessionId(Uuid::new_v4());
        let pane_id = PaneId(Uuid::new_v4());

        let window_state = WindowState {
            id: window_id.clone(),
            session_id: session_id.clone(),
            name: "test_window".to_string(),
            index: 1,
            layout: Layout::new(pane_id.clone()),
            current_pane: Some(pane_id.clone()),
            width: 100,
            height: 50,
        };

        assert_eq!(window_state.id, window_id);
        assert_eq!(window_state.session_id, session_id);
        assert_eq!(window_state.name, "test_window");
        assert_eq!(window_state.index, 1);
        assert_eq!(window_state.current_pane, Some(pane_id));
        assert_eq!(window_state.width, 100);
        assert_eq!(window_state.height, 50);
    }

    #[test]
    fn test_pane_state() {
        let pane_id = PaneId(Uuid::new_v4());
        let window_id = WindowId(Uuid::new_v4());

        let pane_state = PaneState {
            id: pane_id.clone(),
            window_id: window_id.clone(),
            working_directory: PathBuf::from("/home/user"),
            command: "/bin/zsh".to_string(),
            cols: 120,
            rows: 40,
            scrollback: vec!["output line 1".to_string()],
            cursor_position: (5, 10),
        };

        assert_eq!(pane_state.id, pane_id);
        assert_eq!(pane_state.window_id, window_id);
        assert_eq!(pane_state.working_directory, PathBuf::from("/home/user"));
        assert_eq!(pane_state.command, "/bin/zsh");
        assert_eq!(pane_state.cols, 120);
        assert_eq!(pane_state.rows, 40);
        assert_eq!(pane_state.scrollback.len(), 1);
        assert_eq!(pane_state.cursor_position, (5, 10));
    }

    #[test]
    fn test_snapshot_info() {
        let metadata = SnapshotMetadata {
            id: Uuid::new_v4(),
            name: "info_test".to_string(),
            description: "test".to_string(),
            created_at: Utc::now(),
            ferrix_version: "1.0.0".to_string(),
            checksum: None,
        };

        let snapshot_info = SnapshotInfo {
            path: PathBuf::from("/test/path"),
            metadata: metadata.clone(),
            session_name: "test_session".to_string(),
        };

        assert_eq!(snapshot_info.path, PathBuf::from("/test/path"));
        assert_eq!(snapshot_info.metadata.name, metadata.name);
        assert_eq!(snapshot_info.session_name, "test_session");
    }

    #[test]
    fn test_snapshot_filename_format() {
        let temp_dir = TempDir::new().unwrap();
        env::set_var("FERRIX_SNAPSHOT_DIR", temp_dir.path());

        let manager = SnapshotManager::new().unwrap();
        let snapshot = create_test_snapshot();

        let saved_path = manager.save_snapshot(&snapshot).unwrap();
        let filename = saved_path.file_name().unwrap().to_string_lossy();

        // Should contain session name, timestamp, and UUID
        assert!(filename.contains("test_session"));
        assert!(filename.ends_with(".ferrix.snapshot"));
    }

    #[test]
    fn test_auto_snapshot_cleanup() {
        let temp_dir = TempDir::new().unwrap();
        env::set_var("FERRIX_SNAPSHOT_DIR", temp_dir.path());

        let manager = SnapshotManager::new().unwrap();

        // Cleanup should work even with no auto directory
        let result = manager.cleanup_auto_snapshots(5);
        assert!(result.is_ok());
    }

    #[test]
    fn test_calculate_checksum() {
        let temp_dir = TempDir::new().unwrap();
        env::set_var("FERRIX_SNAPSHOT_DIR", temp_dir.path());

        let manager = SnapshotManager::new().unwrap();
        let snapshot = create_test_snapshot();

        let checksum = manager.calculate_checksum(&snapshot);
        assert!(checksum.is_ok());

        let checksum_str = checksum.unwrap();
        assert!(!checksum_str.is_empty());
        assert!(checksum_str.chars().all(|c| c.is_ascii_hexdigit()));
    }
}