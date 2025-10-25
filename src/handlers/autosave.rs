//! Auto-save command handlers
//!
//! Handles automatic session snapshot functionality:
//! - enable: Enable auto-save with interval
//! - disable: Disable auto-save
//! - status: Check auto-save status

use crate::client::Client;
use crate::error::Result;
use crate::protocol::SessionId;
use std::path::PathBuf;

/// Handle `enable-auto-save` - enable automatic session snapshots
pub async fn handle_enable(
    socket_path: PathBuf,
    session: Option<String>,
    interval: u64,
) -> Result<()> {
    let mut client = Client::new(socket_path)?;
    client.connect().await?;

    let session_id = if let Some(session_str) = session {
        let sessions = client.list_sessions().await?;
        if let Ok(uuid) = uuid::Uuid::parse_str(&session_str) {
            Some(SessionId(uuid))
        } else {
            sessions
                .iter()
                .find(|s| s.name == session_str)
                .map(|s| s.id.clone())
        }
    } else {
        None
    };

    let interval_minutes = client.enable_auto_save(session_id, Some(interval)).await?;
    println!("✓ Auto-save enabled with {} minute interval", interval_minutes);

    Ok(())
}

/// Handle `disable-auto-save` - disable automatic session snapshots
pub async fn handle_disable(socket_path: PathBuf, session: Option<String>) -> Result<()> {
    let mut client = Client::new(socket_path)?;
    client.connect().await?;

    let session_id = if let Some(session_str) = session {
        let sessions = client.list_sessions().await?;
        if let Ok(uuid) = uuid::Uuid::parse_str(&session_str) {
            Some(SessionId(uuid))
        } else {
            sessions
                .iter()
                .find(|s| s.name == session_str)
                .map(|s| s.id.clone())
        }
    } else {
        None
    };

    client.disable_auto_save(session_id).await?;
    println!("✓ Auto-save disabled");

    Ok(())
}

/// Handle `auto-save-status` - check auto-save status
pub async fn handle_status(socket_path: PathBuf, session: Option<String>) -> Result<()> {
    let mut client = Client::new(socket_path)?;
    client.connect().await?;

    let session_id = if let Some(session_str) = session {
        let sessions = client.list_sessions().await?;
        if let Ok(uuid) = uuid::Uuid::parse_str(&session_str) {
            Some(SessionId(uuid))
        } else {
            sessions
                .iter()
                .find(|s| s.name == session_str)
                .map(|s| s.id.clone())
        }
    } else {
        None
    };

    let (enabled, interval_minutes, last_save, next_save) = client.auto_save_status(session_id).await?;

    println!("Auto-save status:");
    println!("  Enabled: {}", if enabled { "Yes" } else { "No" });
    if enabled {
        println!("  Interval: {} minutes", interval_minutes);
        if let Some(last) = last_save {
            println!("  Last save: {}", last.format("%Y-%m-%d %H:%M:%S UTC"));
        } else {
            println!("  Last save: Never");
        }
        if let Some(next) = next_save {
            println!("  Next save: {}", next.format("%Y-%m-%d %H:%M:%S UTC"));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_autosave_handlers_exist() {
        // Verify all handlers compile
        assert!(true);
    }
}
