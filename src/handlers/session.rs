//! Session command handlers
//!
//! Handles the most commonly used commands for session management:
//! - new: Create a new session
//! - attach: Attach to an existing session
//! - list: List all active sessions
//! - kill: Kill a session
//! - detach: Detach from current session (client-side only)

use crate::client::Client;
use crate::error::Result;
use crate::protocol::SessionId;
use std::path::PathBuf;

/// Handle the `new` command - create a new session
///
/// # Arguments
/// * `socket_path` - Path to the server socket
/// * `session` - Optional session name
/// * `detached` - If true, create but don't attach
///
/// # Example
/// ```ignore
/// handle_new(socket_path, Some("my-session"), false).await?;
/// ```
pub async fn handle_new(
    socket_path: PathBuf,
    session: Option<String>,
    detached: bool,
) -> Result<()> {
    let mut client = Client::new(socket_path)?;
    client.connect().await?;

    let session_id = client.create_session(session.clone()).await?;

    if !detached {
        client.attach_session(session_id).await?;
    } else {
        println!("Session created: {}", session_id.0);
    }

    Ok(())
}

/// Handle the `attach` command - attach to an existing session
///
/// # Arguments
/// * `socket_path` - Path to the server socket
/// * `target` - Optional session name or ID (attaches to first if None)
///
/// # Behavior
/// - If target is a UUID, attaches to that session ID
/// - If target is a name, finds session by name
/// - If target is None, attaches to first available session
pub async fn handle_attach(socket_path: PathBuf, target: Option<String>) -> Result<()> {
    let mut client = Client::new(socket_path)?;
    client.connect().await?;

    if let Some(target_str) = target {
        let sessions = client.list_sessions().await?;

        let session_id = if let Ok(uuid) = uuid::Uuid::parse_str(&target_str) {
            SessionId(uuid)
        } else {
            sessions
                .iter()
                .find(|s| s.name == target_str)
                .map(|s| s.id.clone())
                .ok_or_else(|| crate::error::FerrixError::SessionNotFound(target_str.clone()))?
        };

        client.attach_session(session_id).await?;
    } else {
        let sessions = client.list_sessions().await?;

        if sessions.is_empty() {
            eprintln!("No sessions available");
        } else {
            let first_session = &sessions[0];
            client.attach_session(first_session.id.clone()).await?;
        }
    }

    Ok(())
}

/// Handle the `list` command - list all active sessions
///
/// # Arguments
/// * `socket_path` - Path to the server socket
///
/// # Output Format
/// ```text
/// Active sessions:
///   session-name (uuid) - N windows - created at YYYY-MM-DD HH:MM:SS
/// ```
pub async fn handle_list(socket_path: PathBuf) -> Result<()> {
    let mut client = Client::new(socket_path)?;
    client.connect().await?;

    let sessions = client.list_sessions().await?;

    if sessions.is_empty() {
        println!("No active sessions");
    } else {
        println!("Active sessions:");
        for session in sessions {
            // Show shortened UUID (first 8 chars) for cleaner output
            let short_id = session.id.0.to_string();
            let short_id = &short_id[..8];
            println!(
                "  {:<20} ({}) - {} window{} - created {}",
                session.name,
                short_id,
                session.windows,
                if session.windows == 1 { "" } else { "s" },
                session.created_at.format("%Y-%m-%d %H:%M:%S")
            );
        }
    }

    Ok(())
}

/// Handle the `kill` command - kill a session
///
/// # Arguments
/// * `socket_path` - Path to the server socket
/// * `target` - Session name or ID to kill
///
/// # Behavior
/// - If target is a UUID, kills that session ID
/// - If target is a name, finds and kills session by name
/// - Returns error if session not found
pub async fn handle_kill(socket_path: PathBuf, target: String) -> Result<()> {
    let mut client = Client::new(socket_path)?;
    client.connect().await?;

    let sessions = client.list_sessions().await?;

    let session_id = if let Ok(uuid) = uuid::Uuid::parse_str(&target) {
        SessionId(uuid)
    } else {
        sessions
            .iter()
            .find(|s| s.name == target)
            .map(|s| s.id.clone())
            .ok_or_else(|| crate::error::FerrixError::SessionNotFound(target.clone()))?
    };

    client.kill_session(session_id).await?;
    println!("Session killed");

    Ok(())
}

/// Handle the `detach` command - detach from current session
///
/// # Note
/// This command only works when executed from within an attached session
/// (via the key binding, typically Ctrl-b d). When run from the CLI,
/// it simply displays a help message.
pub fn handle_detach() {
    eprintln!("Detach must be used from within an attached session (Ctrl-b d)");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detach_message() {
        // Detach from CLI just prints a message
        handle_detach();
        // No panic = success
    }
}
