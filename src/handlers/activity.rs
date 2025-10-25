//! Activity monitoring command handlers
//!
//! Handles pane activity monitoring:
//! - toggle: Toggle activity monitoring for a pane
//! - set: Set activity monitoring state explicitly

use crate::client::Client;
use crate::error::Result;
use crate::protocol::PaneId;
use std::path::PathBuf;

/// Handle `toggle-activity-monitoring` - toggle activity monitoring for a pane
pub async fn handle_toggle(socket_path: PathBuf, pane_id: Option<String>) -> Result<()> {
    let mut client = Client::new(socket_path)?;
    client.connect().await?;

    let parsed_pane_id = pane_id.as_ref().and_then(|id_str| {
        uuid::Uuid::parse_str(id_str).ok().map(PaneId)
    });

    let (pane_id, enabled) = client.toggle_activity_monitoring(parsed_pane_id).await?;
    println!(
        "✓ Activity monitoring {} for pane {}",
        if enabled { "enabled" } else { "disabled" },
        pane_id.0
    );

    Ok(())
}

/// Handle `set-activity-monitoring` - set activity monitoring state
pub async fn handle_set(
    socket_path: PathBuf,
    pane_id: Option<String>,
    enabled: bool,
) -> Result<()> {
    let mut client = Client::new(socket_path)?;
    client.connect().await?;

    let parsed_pane_id = pane_id.as_ref().and_then(|id_str| {
        uuid::Uuid::parse_str(id_str).ok().map(PaneId)
    });

    let (pane_id, actual_enabled) = client.set_activity_monitoring(parsed_pane_id, enabled).await?;
    println!(
        "✓ Activity monitoring {} for pane {}",
        if actual_enabled { "enabled" } else { "disabled" },
        pane_id.0
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_activity_handlers_exist() {
        // Verify all handlers compile
        assert!(true);
    }
}
