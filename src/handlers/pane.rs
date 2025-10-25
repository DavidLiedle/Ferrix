//! Pane command handlers
//!
//! Handles pane synchronization and management:
//! - toggle_sync: Toggle pane synchronization
//! - set_sync: Set pane synchronization state
//! - toggle_zoom: Toggle pane zoom

use crate::client::Client;
use crate::error::Result;
use std::path::PathBuf;

/// Handle `toggle-pane-sync` - toggle pane synchronization
pub async fn handle_toggle_sync(socket_path: PathBuf) -> Result<()> {
    let mut client = Client::new(socket_path)?;
    client.connect().await?;
    let enabled = client.toggle_pane_sync().await?;
    println!("✓ Pane synchronization {}", if enabled { "enabled" } else { "disabled" });

    Ok(())
}

/// Handle `set-pane-sync` - set pane synchronization state
pub async fn handle_set_sync(socket_path: PathBuf, enabled: bool) -> Result<()> {
    let mut client = Client::new(socket_path)?;
    client.connect().await?;
    let actual_enabled = client.set_pane_sync(enabled).await?;
    println!("✓ Pane synchronization {}", if actual_enabled { "enabled" } else { "disabled" });

    Ok(())
}

/// Handle `toggle-zoom` - toggle pane zoom
pub async fn handle_toggle_zoom(socket_path: PathBuf) -> Result<()> {
    let mut client = Client::new(socket_path)?;
    client.connect().await?;
    let (zoomed, pane_id) = client.toggle_zoom().await?;

    if zoomed {
        if let Some(pane_id) = pane_id {
            println!("✓ Pane {} zoomed (expanded to full window)", pane_id.0);
        } else {
            println!("✓ Pane zoomed");
        }
    } else {
        println!("✓ Pane unzoomed (restored to normal layout)");
    }

    Ok(())
}
