#[cfg(test)]
mod client_tests {
    use super::*;
    use crate::error::Result;
    use crate::protocol::{ClientMessage, ServerMessage, SessionId, WindowId, PaneId};
    use std::sync::Arc;
    use tokio::sync::mpsc;
    use uuid::Uuid;

    #[tokio::test]
    async fn test_client_creation() -> Result<()> {
        let (tx, _rx) = mpsc::unbounded_channel();
        let (server_tx, _server_rx) = mpsc::unbounded_channel();

        let client = Client::new(tx, server_tx);

        assert_eq!(client.current_session, None);
        assert!(client.sessions.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn test_client_session_attachment() -> Result<()> {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let (server_tx, _server_rx) = mpsc::unbounded_channel();

        let mut client = Client::new(tx, server_tx);
        let session_id = SessionId(Uuid::new_v4());

        // Simulate session attachment
        client.handle_message(ServerMessage::SessionAttached {
            session_id: session_id.clone(),
            session_name: "test-session".to_string(),
        }).await?;

        assert_eq!(client.current_session, Some(session_id));
        Ok(())
    }

    #[tokio::test]
    async fn test_client_session_creation() -> Result<()> {
        let (tx, _rx) = mpsc::unbounded_channel();
        let (server_tx, _server_rx) = mpsc::unbounded_channel();

        let mut client = Client::new(tx, server_tx);
        let session_id = SessionId(Uuid::new_v4());

        // Simulate session creation response
        client.handle_message(ServerMessage::SessionCreated {
            session_id: session_id.clone(),
            session_name: "new-session".to_string(),
        }).await?;

        assert!(client.sessions.contains_key(&session_id));
        Ok(())
    }

    #[tokio::test]
    async fn test_client_window_updates() -> Result<()> {
        let (tx, _rx) = mpsc::unbounded_channel();
        let (server_tx, _server_rx) = mpsc::unbounded_channel();

        let mut client = Client::new(tx, server_tx);
        let session_id = SessionId(Uuid::new_v4());
        let window_id = WindowId(Uuid::new_v4());

        // Setup session first
        client.current_session = Some(session_id.clone());

        // Simulate window created message
        client.handle_message(ServerMessage::WindowCreated {
            session_id: session_id.clone(),
            window_id: window_id.clone(),
            window_name: "test-window".to_string(),
        }).await?;

        // Verify window is tracked
        if let Some(session) = client.sessions.get(&session_id) {
            assert!(session.windows.contains_key(&window_id));
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_client_pane_output() -> Result<()> {
        let (tx, _rx) = mpsc::unbounded_channel();
        let (server_tx, _server_rx) = mpsc::unbounded_channel();

        let mut client = Client::new(tx, server_tx);
        let session_id = SessionId(Uuid::new_v4());
        let pane_id = PaneId(Uuid::new_v4());

        client.current_session = Some(session_id.clone());

        // Simulate pane output
        let test_data = b"Hello, World!".to_vec();
        client.handle_message(ServerMessage::PaneOutput {
            session_id: session_id.clone(),
            pane_id: pane_id.clone(),
            data: test_data.clone(),
        }).await?;

        // Output should be processed without error
        Ok(())
    }

    #[tokio::test]
    async fn test_client_copy_mode_entry() -> Result<()> {
        let (tx, _rx) = mpsc::unbounded_channel();
        let (server_tx, _server_rx) = mpsc::unbounded_channel();

        let mut client = Client::new(tx, server_tx);
        let session_id = SessionId(Uuid::new_v4());

        client.current_session = Some(session_id.clone());

        // Simulate copy mode entry
        client.handle_message(ServerMessage::CopyModeEntered {
            session_id: session_id.clone(),
            content: vec!["line1".to_string(), "line2".to_string()],
        }).await?;

        assert!(client.copy_mode.active);
        Ok(())
    }

    #[tokio::test]
    async fn test_client_error_handling() -> Result<()> {
        let (tx, _rx) = mpsc::unbounded_channel();
        let (server_tx, _server_rx) = mpsc::unbounded_channel();

        let mut client = Client::new(tx, server_tx);

        // Simulate error message
        client.handle_message(ServerMessage::Error {
            message: "Test error".to_string(),
        }).await?;

        // Should handle error gracefully
        Ok(())
    }

    #[tokio::test]
    async fn test_client_key_handling() -> Result<()> {
        let (tx, _rx) = mpsc::unbounded_channel();
        let (server_tx, _server_rx) = mpsc::unbounded_channel();

        let client = Client::new(tx, server_tx);

        // Test key sequence parsing
        let key_combo = client.parse_key_sequence("C-a");
        assert!(key_combo.is_some());

        let invalid_key = client.parse_key_sequence("Invalid");
        assert!(invalid_key.is_none());

        Ok(())
    }
}