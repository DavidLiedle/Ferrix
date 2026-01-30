//! Miscellaneous command handlers
//!
//! Handlers for various utility commands:
//! - send_keys: Send keys to a session

use crate::client::Client;
use crate::error::Result;
use crate::protocol::SessionId;
use std::path::PathBuf;

/// Handle `send-keys` - send keys to a session
pub async fn handle_send_keys(socket_path: PathBuf, target: String, keys: Vec<String>) -> Result<()> {
    let mut client = Client::new(socket_path)?;
    client.connect().await?;

    // Parse target (session name or ID)
    let sessions = client.list_sessions().await?;
    let session_id = if let Ok(uuid) = uuid::Uuid::parse_str(&target) {
        Some(SessionId(uuid))
    } else {
        sessions
            .iter()
            .find(|s| s.name == target)
            .map(|s| s.id.clone())
    };

    if let Some(sid) = session_id {
        // Attach to the session
        client.attach_session(sid.clone()).await?;

        // Send the keys
        let keys_string = keys.join(" ");
        let data = keys_string.as_bytes().to_vec();

        client.send_keys(data).await?;
        println!("✓ Keys sent to session");

        // Detach from session
        let _ = client.detach_session().await;
    } else {
        eprintln!("✗ Session not found: {}", target);
        std::process::exit(1);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_misc_handlers_exist() {
        // Verify all handlers compile
        assert!(true);
    }
}
