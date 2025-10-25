//! Window command handlers
//!
//! Handles window operations:
//! - rename: Rename a window

use crate::client::Client;
use crate::error::Result;
use crate::protocol::WindowId;
use std::path::PathBuf;

/// Handle `rename-window` - rename a window
pub async fn handle_rename(
    socket_path: PathBuf,
    window_id: Option<String>,
    new_name: String,
) -> Result<()> {
    let mut client = Client::new(socket_path)?;
    client.connect().await?;

    // Parse window_id if provided
    let parsed_window_id = if let Some(window_id_str) = window_id {
        match uuid::Uuid::parse_str(&window_id_str) {
            Ok(uuid) => Some(WindowId(uuid)),
            Err(_) => {
                eprintln!("✗ Invalid window ID format: {}", window_id_str);
                std::process::exit(1);
            }
        }
    } else {
        None
    };

    client.rename_window(parsed_window_id, new_name.clone()).await?;
    println!("✓ Window renamed to '{}'", new_name);

    Ok(())
}
