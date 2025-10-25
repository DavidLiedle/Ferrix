//! Key binding command handlers
//!
//! Handles keybinding management operations:
//! - list: List all current keybindings
//! - bind: Create custom keybinding
//! - unbind: Remove custom keybinding
//! - reset: Reset all to defaults
//! - reload: Reload from config
//! - export: Export keybindings to file
//! - import: Import keybindings from file

use crate::client::Client;
use crate::error::Result;
use std::path::PathBuf;

/// Handle `list-keys` - list all keybindings
pub async fn handle_list(socket_path: PathBuf) -> Result<()> {
    let mut client = Client::new(socket_path)?;
    client.connect().await?;

    let bindings = client.list_keys().await?;

    if bindings.is_empty() {
        println!("No keybindings configured");
    } else {
        println!("Current keybindings:");
        println!("{:<20} {:<30} {:<10} Description", "Key", "Action", "Type");
        println!("{}", "-".repeat(80));
        for binding in bindings {
            println!(
                "{:<20} {:<30} {:<10} {}",
                binding.key,
                binding.action,
                if binding.is_custom { "custom" } else { "default" },
                binding.description
            );
        }
    }

    Ok(())
}

/// Handle `bind-key` - create custom keybinding
pub async fn handle_bind(socket_path: PathBuf, key: String, action: String) -> Result<()> {
    let mut client = Client::new(socket_path)?;
    client.connect().await?;
    client.bind_key(key.clone(), action.clone()).await?;
    println!("✓ Key '{}' bound to action '{}'", key, action);

    Ok(())
}

/// Handle `unbind-key` - remove custom keybinding
pub async fn handle_unbind(socket_path: PathBuf, key: String) -> Result<()> {
    let mut client = Client::new(socket_path)?;
    client.connect().await?;
    client.unbind_key(key.clone()).await?;
    println!("✓ Key '{}' unbound", key);

    Ok(())
}

/// Handle `reset-keys` - reset all keybindings to defaults
pub async fn handle_reset(socket_path: PathBuf) -> Result<()> {
    let mut client = Client::new(socket_path)?;
    client.connect().await?;
    client.reset_keys().await?;
    println!("✓ All keybindings reset to defaults");

    Ok(())
}

/// Handle `reload-keys` - reload keybindings from config
pub async fn handle_reload(socket_path: PathBuf) -> Result<()> {
    let mut client = Client::new(socket_path)?;
    client.connect().await?;
    client.reload_keys().await?;
    println!("✓ Keybindings reloaded from configuration");

    Ok(())
}

/// Handle `export-keys` - export keybindings to file
pub async fn handle_export(socket_path: PathBuf, path: String) -> Result<()> {
    let mut client = Client::new(socket_path)?;
    client.connect().await?;
    let export_path = client.export_keys(PathBuf::from(path)).await?;
    println!("✓ Keybindings exported to: {}", export_path.display());

    Ok(())
}

/// Handle `import-keys` - import keybindings from file
pub async fn handle_import(socket_path: PathBuf, path: String) -> Result<()> {
    let mut client = Client::new(socket_path)?;
    client.connect().await?;
    let count = client.import_keys(PathBuf::from(&path)).await?;
    println!("✓ Successfully imported {} keybindings from: {}", count, path);

    Ok(())
}
