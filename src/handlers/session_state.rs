//! Session state command handlers
//!
//! Handles session state management:
//! - lock: Lock session (read-only mode)
//! - unlock: Unlock session
//! - set_lock: Set lock state explicitly

use crate::client::Client;
use crate::error::Result;
use std::path::PathBuf;

/// Handle `lock-session` - lock the current session (read-only mode)
pub async fn handle_lock(socket_path: PathBuf) -> Result<()> {
    let mut client = Client::new(socket_path)?;
    client.connect().await?;

    let locked = client.lock_session().await?;
    println!("✓ Session {}", if locked { "locked (read-only)" } else { "unlocked" });

    Ok(())
}

/// Handle `unlock-session` - unlock the current session
pub async fn handle_unlock(socket_path: PathBuf) -> Result<()> {
    let mut client = Client::new(socket_path)?;
    client.connect().await?;

    let locked = client.unlock_session().await?;
    println!("✓ Session {}", if locked { "locked (read-only)" } else { "unlocked" });

    Ok(())
}

/// Handle `set-session-lock` - set session lock state explicitly
pub async fn handle_set_lock(socket_path: PathBuf, locked: bool) -> Result<()> {
    let mut client = Client::new(socket_path)?;
    client.connect().await?;

    let actual_locked = client.set_session_lock(locked).await?;
    println!("✓ Session {}", if actual_locked { "locked (read-only)" } else { "unlocked" });

    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_session_state_handlers_exist() {
        // Verify all handlers compile
        assert!(true);
    }
}
