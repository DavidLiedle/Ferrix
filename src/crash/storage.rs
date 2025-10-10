//! Crash Report Storage
//!
//! Manages persistent storage of crash reports in ~/.ferrix/crashes/

use serde::{Serialize, Deserialize};
use std::path::{Path, PathBuf};
use std::fs;
use chrono::Utc;
use crate::error::{Result, FerrixError};
use super::capture::CrashMetadata;

/// Stored crash report with file metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrashReport {
    pub metadata: CrashMetadata,
    pub file_path: PathBuf,
    pub file_size: u64,
}

/// Crash storage manager
pub struct CrashStorage {
    crashes_dir: PathBuf,
}

impl CrashStorage {
    /// Create a new crash storage manager
    pub fn new() -> Result<Self> {
        let crashes_dir = Self::get_crashes_directory()?;

        // Ensure crashes directory exists
        fs::create_dir_all(&crashes_dir).map_err(|e| {
            FerrixError::Other(format!("Failed to create crashes directory: {}", e))
        })?;

        Ok(Self { crashes_dir })
    }

    /// Get the crashes directory path (~/.ferrix/crashes/)
    pub fn get_crashes_directory() -> Result<PathBuf> {
        let data_dir = dirs::data_local_dir()
            .or_else(|| dirs::home_dir().map(|h| h.join(".local/share")))
            .ok_or_else(|| FerrixError::Other("Could not determine data directory".to_string()))?;

        Ok(data_dir.join("ferrix").join("crashes"))
    }

    /// Store a crash report
    pub fn store_crash(&self, metadata: &CrashMetadata) -> Result<PathBuf> {
        let filename = format!(
            "crash-{}-{}.json",
            metadata.timestamp.format("%Y%m%d-%H%M%S"),
            metadata.id
        );

        let file_path = self.crashes_dir.join(&filename);

        let json = serde_json::to_string_pretty(metadata).map_err(|e| {
            FerrixError::Other(format!("Failed to serialize crash metadata: {}", e))
        })?;

        fs::write(&file_path, json).map_err(|e| {
            FerrixError::Other(format!("Failed to write crash report: {}", e))
        })?;

        Ok(file_path)
    }

    /// List all crash reports
    pub fn list_crashes(&self) -> Result<Vec<CrashReport>> {
        let mut crashes = Vec::new();

        if !self.crashes_dir.exists() {
            return Ok(crashes);
        }

        let entries = fs::read_dir(&self.crashes_dir).map_err(|e| {
            FerrixError::Other(format!("Failed to read crashes directory: {}", e))
        })?;

        for entry in entries {
            let entry = entry.map_err(|e| {
                FerrixError::Other(format!("Failed to read directory entry: {}", e))
            })?;

            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                if let Ok(crash_report) = self.load_crash(&path) {
                    crashes.push(crash_report);
                }
            }
        }

        // Sort by timestamp, newest first
        crashes.sort_by(|a, b| b.metadata.timestamp.cmp(&a.metadata.timestamp));

        Ok(crashes)
    }

    /// Load a crash report from a file
    pub fn load_crash(&self, path: &Path) -> Result<CrashReport> {
        let contents = fs::read_to_string(path).map_err(|e| {
            FerrixError::Other(format!("Failed to read crash file: {}", e))
        })?;

        let metadata: CrashMetadata = serde_json::from_str(&contents).map_err(|e| {
            FerrixError::Other(format!("Failed to parse crash metadata: {}", e))
        })?;

        let file_metadata = fs::metadata(path).map_err(|e| {
            FerrixError::Other(format!("Failed to get file metadata: {}", e))
        })?;

        Ok(CrashReport {
            metadata,
            file_path: path.to_path_buf(),
            file_size: file_metadata.len(),
        })
    }

    /// Get a crash report by ID
    pub fn get_crash(&self, crash_id: uuid::Uuid) -> Result<CrashReport> {
        let crashes = self.list_crashes()?;

        crashes
            .into_iter()
            .find(|c| c.metadata.id == crash_id)
            .ok_or_else(|| {
                FerrixError::Other(format!("Crash report not found: {}", crash_id))
            })
    }

    /// Delete a crash report
    pub fn delete_crash(&self, crash_id: uuid::Uuid) -> Result<()> {
        let crash = self.get_crash(crash_id)?;

        fs::remove_file(&crash.file_path).map_err(|e| {
            FerrixError::Other(format!("Failed to delete crash file: {}", e))
        })?;

        Ok(())
    }

    /// Delete all crash reports
    pub fn delete_all_crashes(&self) -> Result<usize> {
        let crashes = self.list_crashes()?;
        let count = crashes.len();

        for crash in crashes {
            fs::remove_file(&crash.file_path).map_err(|e| {
                FerrixError::Other(format!("Failed to delete crash file: {}", e))
            })?;
        }

        Ok(count)
    }

    /// Delete crash reports older than the specified number of days
    pub fn delete_old_crashes(&self, days: i64) -> Result<usize> {
        let cutoff = Utc::now() - chrono::Duration::days(days);
        let crashes = self.list_crashes()?;

        let mut deleted = 0;
        for crash in crashes {
            if crash.metadata.timestamp < cutoff {
                fs::remove_file(&crash.file_path).map_err(|e| {
                    FerrixError::Other(format!("Failed to delete crash file: {}", e))
                })?;
                deleted += 1;
            }
        }

        Ok(deleted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crash::capture::{CrashLocation, SystemInfo};
    use crate::server::metrics::MetricsSnapshot;
    use tempfile::TempDir;

    fn create_test_crash() -> CrashMetadata {
        CrashMetadata {
            id: uuid::Uuid::new_v4(),
            timestamp: Utc::now(),
            message: "Test crash".to_string(),
            location: Some(CrashLocation {
                file: "test.rs".to_string(),
                line: 42,
            }),
            backtrace: Some("test backtrace".to_string()),
            system_info: SystemInfo::capture(),
            metrics: None,
            version: "0.1.0".to_string(),
        }
    }

    #[test]
    fn test_crash_storage_creation() {
        let storage = CrashStorage::new();
        assert!(storage.is_ok());
    }

    #[test]
    fn test_store_and_load_crash() {
        let storage = CrashStorage::new().unwrap();
        let crash = create_test_crash();

        let path = storage.store_crash(&crash).unwrap();
        assert!(path.exists());

        let loaded = storage.load_crash(&path).unwrap();
        assert_eq!(loaded.metadata.id, crash.id);
        assert_eq!(loaded.metadata.message, crash.message);

        // Cleanup
        let _ = storage.delete_crash(crash.id);
    }

    #[test]
    fn test_list_crashes() {
        let storage = CrashStorage::new().unwrap();

        // Create multiple crashes
        let crash1 = create_test_crash();
        let crash2 = create_test_crash();

        storage.store_crash(&crash1).unwrap();
        storage.store_crash(&crash2).unwrap();

        let crashes = storage.list_crashes().unwrap();
        assert!(crashes.len() >= 2);

        // Cleanup
        let _ = storage.delete_crash(crash1.id);
        let _ = storage.delete_crash(crash2.id);
    }

    #[test]
    fn test_delete_crash() {
        let storage = CrashStorage::new().unwrap();
        let crash = create_test_crash();

        storage.store_crash(&crash).unwrap();
        assert!(storage.get_crash(crash.id).is_ok());

        storage.delete_crash(crash.id).unwrap();
        assert!(storage.get_crash(crash.id).is_err());
    }

    #[test]
    fn test_delete_old_crashes() {
        let storage = CrashStorage::new().unwrap();

        // Create a crash with an old timestamp
        let mut old_crash = create_test_crash();
        old_crash.timestamp = Utc::now() - chrono::Duration::days(10);

        storage.store_crash(&old_crash).unwrap();

        // Delete crashes older than 5 days
        let deleted = storage.delete_old_crashes(5).unwrap();
        assert!(deleted >= 1);

        // Verify the old crash was deleted
        assert!(storage.get_crash(old_crash.id).is_err());
    }
}
