//! Pane command handlers
//!
//! Handles pane synchronization and management:
//! - toggle_sync: Toggle pane synchronization
//! - set_sync: Set pane synchronization state
//! - toggle_zoom: Toggle pane zoom
//! - split: Split the current pane
//! - select: Select a specific pane
//! - kill: Kill a specific pane
//! - resize: Resize the current pane

use crate::client::Client;
use crate::error::Result;
use crate::protocol::{ClientMessage, ServerMessage, PaneId, SplitDirection, ResizeDirection};
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

/// Handle `split-pane` - split the current pane
pub async fn handle_split(
    socket_path: PathBuf,
    vertical: bool,
    horizontal: bool,
    _percentage: Option<u8>,
) -> Result<()> {
    let mut client = Client::new(socket_path)?;
    client.connect().await?;

    // Determine split direction (default to horizontal if neither specified)
    let split_dir = if vertical {
        SplitDirection::Vertical
    } else if horizontal {
        SplitDirection::Horizontal
    } else {
        // Default to horizontal split
        SplitDirection::Horizontal
    };

    client.send(ClientMessage::SplitPane { direction: split_dir }).await?;

    match client.receive().await? {
        ServerMessage::PaneCreated { pane_id } => {
            let dir_str = if vertical { "vertically" } else { "horizontally" };
            println!("✓ Pane split {} (new pane: {})", dir_str, pane_id.0);
            Ok(())
        }
        ServerMessage::Error { message } => {
            eprintln!("✗ Failed to split pane: {}", message);
            std::process::exit(1);
        }
        _ => {
            eprintln!("✗ Unexpected server response");
            std::process::exit(1);
        }
    }
}

/// Handle `select-pane` - select a specific pane
pub async fn handle_select(socket_path: PathBuf, target: String) -> Result<()> {
    let mut client = Client::new(socket_path)?;
    client.connect().await?;

    // Try to parse as index first, then as UUID
    let message = if let Ok(index) = target.parse::<usize>() {
        ClientMessage::SelectPaneByIndex { index }
    } else if let Ok(uuid) = uuid::Uuid::parse_str(&target) {
        ClientMessage::SwitchPane { pane_id: PaneId(uuid) }
    } else {
        eprintln!("✗ Invalid pane target: {}. Use pane index or UUID", target);
        std::process::exit(1);
    };

    client.send(message).await?;

    match client.receive().await? {
        ServerMessage::PaneSwitched { pane_id } => {
            println!("✓ Selected pane {}", pane_id.0);
            Ok(())
        }
        ServerMessage::Error { message } => {
            eprintln!("✗ Failed to select pane: {}", message);
            std::process::exit(1);
        }
        _ => {
            eprintln!("✗ Unexpected server response");
            std::process::exit(1);
        }
    }
}

/// Handle `kill-pane` - kill a specific pane
pub async fn handle_kill_pane(socket_path: PathBuf, target: Option<String>) -> Result<()> {
    let mut client = Client::new(socket_path)?;
    client.connect().await?;

    let message = if let Some(target_str) = target {
        if let Ok(uuid) = uuid::Uuid::parse_str(&target_str) {
            ClientMessage::ClosePane { pane_id: PaneId(uuid) }
        } else {
            eprintln!("✗ Invalid pane ID: {}", target_str);
            std::process::exit(1);
        }
    } else {
        // Kill current pane
        ClientMessage::KillPane
    };

    client.send(message).await?;

    match client.receive().await? {
        ServerMessage::PaneClosed { pane_id } => {
            println!("✓ Pane {} killed", pane_id.0);
            Ok(())
        }
        ServerMessage::Error { message } => {
            eprintln!("✗ Failed to kill pane: {}", message);
            std::process::exit(1);
        }
        _ => {
            eprintln!("✗ Unexpected server response");
            std::process::exit(1);
        }
    }
}

/// Handle `resize-pane` - resize the current pane
pub async fn handle_resize(socket_path: PathBuf, direction: String, amount: u16) -> Result<()> {
    let mut client = Client::new(socket_path)?;
    client.connect().await?;

    let resize_dir = match direction.to_lowercase().as_str() {
        "up" | "u" => ResizeDirection::Up,
        "down" | "d" => ResizeDirection::Down,
        "left" | "l" => ResizeDirection::Left,
        "right" | "r" => ResizeDirection::Right,
        _ => {
            eprintln!("✗ Invalid direction: {}. Use 'up', 'down', 'left', or 'right'", direction);
            std::process::exit(1);
        }
    };

    client.send(ClientMessage::ResizePane {
        direction: resize_dir,
        amount: amount as i16,
    }).await?;

    match client.receive().await? {
        ServerMessage::Success => {
            println!("✓ Pane resized {} by {}", direction, amount);
            Ok(())
        }
        ServerMessage::Error { message } => {
            eprintln!("✗ Failed to resize pane: {}", message);
            std::process::exit(1);
        }
        _ => {
            eprintln!("✗ Unexpected server response");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_pane_handlers_exist() {
        // Verify all handlers compile
        assert!(true);
    }
}
