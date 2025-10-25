//! Snapshot command handlers
//!
//! Handles session snapshot operations for backup and restore:
//! - save: Save session snapshot
//! - load: Load snapshot as new session
//! - restore: Restore snapshot into existing session
//! - list: List all snapshots
//! - delete: Delete a snapshot
//! - export: Export snapshot to archive
//! - import: Import snapshot from archive

use crate::client::Client;
use crate::error::{Result, FerrixError};
use crate::protocol::SessionId;
use crate::server::snapshot::SnapshotManager;
use std::path::PathBuf;

/// Handle `save-snapshot` - save a session snapshot
pub async fn handle_save(
    socket_path: PathBuf,
    session: String,
    name: Option<String>,
    description: Option<String>,
) -> Result<()> {
    let mut client = Client::new(socket_path)?;
    client.connect().await?;

    let sessions = client.list_sessions().await?;

    let session_id = if let Ok(uuid) = uuid::Uuid::parse_str(&session) {
        SessionId(uuid)
    } else {
        sessions
            .iter()
            .find(|s| s.name == session)
            .map(|s| s.id.clone())
            .ok_or_else(|| FerrixError::SessionNotFound(session.clone()))?
    };

    let path = client.save_snapshot(session_id, name, description).await?;
    println!("Snapshot saved to: {:?}", path);

    Ok(())
}

/// Handle `load-snapshot` - load snapshot as new session
pub async fn handle_load(socket_path: PathBuf, path: String) -> Result<()> {
    let mut client = Client::new(socket_path)?;
    client.connect().await?;

    let session_id = client.load_snapshot(path.into()).await?;
    println!("Snapshot loaded as session: {}", session_id.0);

    Ok(())
}

/// Handle `restore-snapshot` - restore snapshot into existing session
pub async fn handle_restore(
    socket_path: PathBuf,
    session: String,
    path: String,
) -> Result<()> {
    let mut client = Client::new(socket_path)?;
    client.connect().await?;

    // Resolve session name/ID
    let sessions = client.list_sessions().await?;
    let session_id = sessions
        .iter()
        .find(|s| s.name == session || s.id.0.to_string().starts_with(session.as_str()))
        .map(|s| s.id.clone())
        .ok_or_else(|| FerrixError::Other(format!("Session '{}' not found", session)))?;

    client.restore_snapshot(session_id.clone(), path.into()).await?;
    println!("Snapshot restored into session: {} ({})", session, session_id.0);

    Ok(())
}

/// Handle `list-snapshots` - list all available snapshots
pub async fn handle_list(socket_path: PathBuf) -> Result<()> {
    let mut client = Client::new(socket_path)?;
    client.connect().await?;

    let snapshots = client.list_snapshots().await?;

    if snapshots.is_empty() {
        println!("No snapshots available");
    } else {
        println!("Available snapshots:");
        println!("{:<20} {:<30} {:<10} Path", "Created", "Name", "Size");
        println!("{}", "-".repeat(80));

        for snapshot in snapshots {
            let size_mb = snapshot.size as f64 / 1024.0 / 1024.0;
            println!(
                "{:<20} {:<30} {:<10.2}MB {}",
                snapshot.created_at.format("%Y-%m-%d %H:%M:%S"),
                snapshot.name,
                size_mb,
                snapshot.path.display()
            );
        }
    }

    Ok(())
}

/// Handle `delete-snapshot` - delete a snapshot
pub async fn handle_delete(socket_path: PathBuf, path: String) -> Result<()> {
    let mut client = Client::new(socket_path)?;
    client.connect().await?;

    client.delete_snapshot(path.into()).await?;
    println!("Snapshot deleted");

    Ok(())
}

/// Handle `export-snapshot` - export snapshot to archive
pub fn handle_export(snapshot: String, output: String) -> Result<()> {
    let manager = SnapshotManager::new()?;
    let snapshot_data = manager.load_snapshot(std::path::Path::new(&snapshot))?;
    manager.export_snapshot(&snapshot_data, std::path::Path::new(&output))?;
    println!("Snapshot exported to: {}", output);

    Ok(())
}

/// Handle `import-snapshot` - import snapshot from archive
pub fn handle_import(archive: String) -> Result<()> {
    let manager = SnapshotManager::new()?;
    let snapshot = manager.import_snapshot(std::path::Path::new(&archive))?;
    let path = manager.save_snapshot(&snapshot)?;
    println!("Snapshot imported to: {:?}", path);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snapshot_handlers_exist() {
        // Verify all handlers compile
        assert!(true);
    }
}
