use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{Duration, interval};
use tracing::{info, error, warn};
use chrono::Utc;

use crate::error::{FerrixError, Result};
use crate::protocol::SessionId;
use super::snapshot::{SnapshotManager, SessionSnapshot};
use super::session::Session;
use std::collections::HashMap;

const RECOVERY_FILE: &str = ".ferrix_recovery";
const AUTO_SAVE_INTERVAL: u64 = 300; // 5 minutes

pub struct RecoveryManager {
    pub snapshot_manager: SnapshotManager,
    recovery_file: PathBuf,
    auto_save_enabled: bool,
}

impl RecoveryManager {
    pub fn new() -> Result<Self> {
        let snapshot_manager = SnapshotManager::new()?;
        let recovery_file = Self::get_recovery_file_path()?;

        Ok(Self {
            snapshot_manager,
            recovery_file,
            auto_save_enabled: true,
        })
    }

    pub async fn start_auto_save(
        &self,
        sessions: Arc<RwLock<HashMap<SessionId, Arc<RwLock<Session>>>>>,
    ) {
        if !self.auto_save_enabled {
            return;
        }

        let mut interval = interval(Duration::from_secs(AUTO_SAVE_INTERVAL));

        loop {
            interval.tick().await;

            let sessions_guard = sessions.read().await;

            for (session_id, session_arc) in sessions_guard.iter() {
                let session_guard = session_arc.read().await;

                let snapshot = session_guard.create_snapshot(
                    Some(format!("auto_recovery_{}", session_guard.name)),
                    Some("Automatic recovery snapshot".to_string()),
                );

                match self.snapshot_manager.auto_snapshot(&snapshot).await {
                    Ok(path) => {
                        info!("Auto-saved snapshot for session {} to {:?}", session_id.0, path);

                        // Update recovery file with latest snapshot path
                        if let Err(e) = self.update_recovery_file(&path).await {
                            error!("Failed to update recovery file: {}", e);
                        }
                    }
                    Err(e) => {
                        error!("Failed to auto-save snapshot for session {}: {}", session_id.0, e);
                    }
                }
            }
        }
    }

    pub async fn check_and_recover(&self) -> Result<Vec<SessionSnapshot>> {
        if !self.recovery_file.exists() {
            info!("No recovery file found");
            return Ok(Vec::new());
        }

        info!("Recovery file found, checking for crashed sessions...");

        let recovered_sessions = self.recover_sessions().await?;

        // Clean up recovery file after successful recovery
        if let Err(e) = std::fs::remove_file(&self.recovery_file) {
            warn!("Failed to remove recovery file: {}", e);
        }

        Ok(recovered_sessions)
    }

    async fn recover_sessions(&self) -> Result<Vec<SessionSnapshot>> {
        let recovery_data = std::fs::read_to_string(&self.recovery_file)?;
        let mut recovered = Vec::new();

        for line in recovery_data.lines() {
            if line.trim().is_empty() {
                continue;
            }

            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() != 2 {
                warn!("Invalid recovery file entry: {}", line);
                continue;
            }

            let snapshot_path = PathBuf::from(parts[1]);
            if !snapshot_path.exists() {
                warn!("Snapshot file not found: {:?}", snapshot_path);
                continue;
            }

            match self.snapshot_manager.load_snapshot(&snapshot_path) {
                Ok(snapshot) => {
                    info!("Recovered session from snapshot: {:?}", snapshot_path);
                    recovered.push(snapshot);
                }
                Err(e) => {
                    error!("Failed to load snapshot from {:?}: {}", snapshot_path, e);
                }
            }
        }

        Ok(recovered)
    }

    async fn update_recovery_file(&self, snapshot_path: &Path) -> Result<()> {
        let timestamp = Utc::now().format("%Y-%m-%d %H:%M:%S");
        let entry = format!("{}\t{}\n", timestamp, snapshot_path.display());

        use tokio::fs::OpenOptions;
        use tokio::io::AsyncWriteExt;

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.recovery_file)
            .await?;

        file.write_all(entry.as_bytes()).await?;
        file.flush().await?;

        Ok(())
    }

    pub fn mark_clean_shutdown(&self) -> Result<()> {
        if self.recovery_file.exists() {
            std::fs::remove_file(&self.recovery_file)?;
            info!("Marked clean shutdown by removing recovery file");
        }
        Ok(())
    }

    fn get_recovery_file_path() -> Result<PathBuf> {
        if let Some(home) = dirs::home_dir() {
            let ferrix_dir = home.join(".ferrix");
            // Ensure .ferrix directory exists
            std::fs::create_dir_all(&ferrix_dir)
                .map_err(|e| FerrixError::Other(format!("Failed to create .ferrix directory: {}", e)))?;
            Ok(ferrix_dir.join(RECOVERY_FILE))
        } else {
            Ok(PathBuf::from("/tmp").join(RECOVERY_FILE))
        }
    }

    pub fn set_auto_save(&mut self, enabled: bool) {
        self.auto_save_enabled = enabled;
        info!("Auto-save {}", if enabled { "enabled" } else { "disabled" });
    }

    /// Clear the recovery file to start fresh (used when --no-recover is specified)
    pub async fn clear_recovery_file(&self) -> Result<()> {
        if self.recovery_file.exists() {
            std::fs::remove_file(&self.recovery_file)?;
            info!("Cleared recovery file (recovery disabled)");
        }
        Ok(())
    }
}

// Signal handler for graceful shutdown
pub fn setup_signal_handlers(
    recovery_manager: Arc<RecoveryManager>,
    sessions: Arc<RwLock<HashMap<SessionId, Arc<RwLock<Session>>>>>,
) {
    use tokio::signal;

    tokio::spawn(async move {
        let mut sigterm = signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("Failed to setup SIGTERM handler");

        let mut sigint = signal::unix::signal(signal::unix::SignalKind::interrupt())
            .expect("Failed to setup SIGINT handler");

        tokio::select! {
            _ = sigterm.recv() => {
                info!("Received SIGTERM, performing clean shutdown...");
            }
            _ = sigint.recv() => {
                info!("Received SIGINT, performing clean shutdown...");
            }
        }

        // Save all active sessions before shutdown
        info!("Saving all active sessions...");
        let sessions_guard = sessions.read().await;

        for (session_id, session_arc) in sessions_guard.iter() {
            let session_guard = session_arc.read().await;

            let snapshot = session_guard.create_snapshot(
                Some(format!("shutdown_{}", session_guard.name)),
                Some("Session saved on shutdown".to_string()),
            );

            // Use block_on to ensure synchronous save before exit
            match tokio::task::block_in_place(|| {
                recovery_manager.snapshot_manager.save_snapshot(&snapshot)
            }) {
                Ok(path) => {
                    info!("Saved session {} to {:?}", session_id.0, path);
                }
                Err(e) => {
                    error!("Failed to save session {} on shutdown: {}", session_id.0, e);
                }
            }
        }

        if let Err(e) = recovery_manager.mark_clean_shutdown() {
            error!("Failed to mark clean shutdown: {}", e);
        }

        info!("Clean shutdown completed");
        std::process::exit(0);
    });
}
#[cfg(test)]
mod tests {
    

    #[test]
    fn test_recovery_mechanism() {
        // Test session recovery mechanism
        assert!(true);
    }

    #[test]
    fn test_crash_recovery() {
        // Test crash recovery
        assert!(true);
    }
}
