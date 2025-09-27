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
        let filename = format!(
            "{}_{}_{}.ferrix.snapshot",
            snapshot.session.name,
            snapshot.metadata.created_at.format("%Y%m%d_%H%M%S"),
            snapshot.metadata.id
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

    pub fn auto_snapshot(&self, snapshot: &SessionSnapshot) -> Result<PathBuf> {
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