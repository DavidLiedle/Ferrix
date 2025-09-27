#[cfg(test)]
mod protocol_tests {
    use super::*;
    use crate::error::Result;
    use crate::protocol::{
        FerrixCodec, FerrixClientCodec, ClientMessage, ServerMessage,
        SessionId, WindowId, PaneId, ClientId, AuthCredentials
    };
    use tokio_util::codec::{Encoder, Decoder};
    use bytes::{BytesMut, Buf};
    use uuid::Uuid;

    #[test]
    fn test_ferrix_codec_creation() {
        let codec = FerrixCodec::new();
        // Codec should be created successfully
    }

    #[test]
    fn test_ferrix_client_codec_creation() {
        let codec = FerrixClientCodec::new();
        // Client codec should be created successfully
    }

    #[tokio::test]
    async fn test_client_message_encoding() -> Result<()> {
        let mut codec = FerrixCodec::new();
        let mut buf = BytesMut::new();

        let session_id = SessionId(Uuid::new_v4());
        let message = ClientMessage::CreateSession {
            name: Some("test-session".to_string()),
        };

        // Test encoding
        codec.encode(message, &mut buf)?;
        assert!(!buf.is_empty());

        Ok(())
    }

    #[tokio::test]
    async fn test_server_message_encoding() -> Result<()> {
        let mut codec = FerrixCodec::new();
        let mut buf = BytesMut::new();

        let session_id = SessionId(Uuid::new_v4());
        let message = ServerMessage::SessionCreated {
            session_id,
            session_name: "test-session".to_string(),
        };

        // Test encoding
        codec.encode(message, &mut buf)?;
        assert!(!buf.is_empty());

        Ok(())
    }

    #[tokio::test]
    async fn test_message_round_trip() -> Result<()> {
        let mut codec = FerrixCodec::new();
        let mut buf = BytesMut::new();

        let session_id = SessionId(Uuid::new_v4());
        let original_message = ClientMessage::CreateSession {
            name: Some("test-session".to_string()),
        };

        // Encode message
        codec.encode(original_message.clone(), &mut buf)?;
        assert!(!buf.is_empty());

        // Decode message
        match codec.decode(&mut buf)? {
            Some(decoded_message) => {
                // Compare messages (would need PartialEq implementation)
                match (&original_message, &decoded_message) {
                    (ClientMessage::CreateSession { name: name1 },
                     ClientMessage::CreateSession { name: name2 }) => {
                        assert_eq!(name1, name2);
                    }
                    _ => panic!("Message types don't match"),
                }
            }
            None => panic!("Failed to decode message"),
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_authentication_message() -> Result<()> {
        let mut codec = FerrixCodec::new();
        let mut buf = BytesMut::new();

        let credentials = AuthCredentials {
            username: "testuser".to_string(),
            password: Some("testpass".to_string()),
            key: None,
        };

        let message = ClientMessage::Authenticate(credentials.clone());

        // Test encoding
        codec.encode(message, &mut buf)?;
        assert!(!buf.is_empty());

        // Test decoding
        match codec.decode(&mut buf)? {
            Some(ClientMessage::Authenticate(decoded_creds)) => {
                assert_eq!(decoded_creds.username, credentials.username);
                assert_eq!(decoded_creds.password, credentials.password);
            }
            _ => panic!("Wrong message type decoded"),
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_pane_output_message() -> Result<()> {
        let mut codec = FerrixCodec::new();
        let mut buf = BytesMut::new();

        let session_id = SessionId(Uuid::new_v4());
        let pane_id = PaneId(Uuid::new_v4());
        let test_data = b"Hello, World!".to_vec();

        let message = ServerMessage::PaneOutput {
            session_id: session_id.clone(),
            pane_id: pane_id.clone(),
            data: test_data.clone(),
        };

        // Test encoding
        codec.encode(message, &mut buf)?;
        assert!(!buf.is_empty());

        // Test decoding
        match codec.decode(&mut buf)? {
            Some(ServerMessage::PaneOutput { session_id: s_id, pane_id: p_id, data }) => {
                assert_eq!(s_id, session_id);
                assert_eq!(p_id, pane_id);
                assert_eq!(data, test_data);
            }
            _ => panic!("Wrong message type decoded"),
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_large_message_handling() -> Result<()> {
        let mut codec = FerrixCodec::new();
        let mut buf = BytesMut::new();

        // Create a large data payload
        let large_data = vec![b'X'; 65536]; // 64KB

        let session_id = SessionId(Uuid::new_v4());
        let pane_id = PaneId(Uuid::new_v4());

        let message = ServerMessage::PaneOutput {
            session_id: session_id.clone(),
            pane_id: pane_id.clone(),
            data: large_data.clone(),
        };

        // Test encoding large message
        codec.encode(message, &mut buf)?;
        assert!(!buf.is_empty());

        // Test decoding large message
        match codec.decode(&mut buf)? {
            Some(ServerMessage::PaneOutput { data, .. }) => {
                assert_eq!(data.len(), large_data.len());
                assert_eq!(data, large_data);
            }
            _ => panic!("Wrong message type decoded"),
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_partial_message_handling() -> Result<()> {
        let mut codec = FerrixCodec::new();
        let mut buf = BytesMut::new();

        let message = ClientMessage::CreateSession {
            name: Some("test".to_string()),
        };

        // Encode message
        codec.encode(message, &mut buf)?;

        // Split buffer to simulate partial message
        let total_len = buf.len();
        let partial_buf = buf.split_to(total_len / 2);
        let mut partial_buf = partial_buf;

        // Try to decode partial message (should return None)
        let result = codec.decode(&mut partial_buf)?;
        assert!(result.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn test_error_message() -> Result<()> {
        let mut codec = FerrixCodec::new();
        let mut buf = BytesMut::new();

        let error_msg = "Something went wrong";
        let message = ServerMessage::Error {
            message: error_msg.to_string(),
        };

        // Test encoding
        codec.encode(message, &mut buf)?;
        assert!(!buf.is_empty());

        // Test decoding
        match codec.decode(&mut buf)? {
            Some(ServerMessage::Error { message }) => {
                assert_eq!(message, error_msg);
            }
            _ => panic!("Wrong message type decoded"),
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_copy_mode_messages() -> Result<()> {
        let mut codec = FerrixCodec::new();
        let mut buf = BytesMut::new();

        let session_id = SessionId(Uuid::new_v4());
        let content = vec!["line1".to_string(), "line2".to_string(), "line3".to_string()];

        let message = ServerMessage::CopyModeEntered {
            session_id: session_id.clone(),
            content: content.clone(),
        };

        // Test encoding
        codec.encode(message, &mut buf)?;
        assert!(!buf.is_empty());

        // Test decoding
        match codec.decode(&mut buf)? {
            Some(ServerMessage::CopyModeEntered { session_id: s_id, content: c }) => {
                assert_eq!(s_id, session_id);
                assert_eq!(c, content);
            }
            _ => panic!("Wrong message type decoded"),
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_concurrent_codec_usage() -> Result<()> {
        use std::sync::Arc;
        use tokio::sync::Mutex;

        let codec = Arc::new(Mutex::new(FerrixCodec::new()));
        let mut handles = vec![];

        // Test concurrent encoding/decoding
        for i in 0..10 {
            let codec = codec.clone();
            let handle = tokio::spawn(async move {
                let mut codec = codec.lock().await;
                let mut buf = BytesMut::new();

                let message = ClientMessage::CreateSession {
                    name: Some(format!("session-{}", i)),
                };

                codec.encode(message, &mut buf).unwrap();
                assert!(!buf.is_empty());

                match codec.decode(&mut buf).unwrap() {
                    Some(ClientMessage::CreateSession { name }) => {
                        assert_eq!(name, Some(format!("session-{}", i)));
                    }
                    _ => panic!("Wrong message type"),
                }
            });
            handles.push(handle);
        }

        // Wait for all operations to complete
        for handle in handles {
            handle.await.unwrap();
        }

        Ok(())
    }

    #[test]
    fn test_id_types() {
        let session_id = SessionId(Uuid::new_v4());
        let window_id = WindowId(Uuid::new_v4());
        let pane_id = PaneId(Uuid::new_v4());
        let client_id = ClientId(Uuid::new_v4());

        // Test Display implementation
        let session_str = format!("{}", session_id);
        assert_eq!(session_str.len(), 36); // UUID string length

        // Test equality
        let same_session_id = SessionId(session_id.0);
        assert_eq!(session_id, same_session_id);

        // Test inequality
        let different_session_id = SessionId(Uuid::new_v4());
        assert_ne!(session_id, different_session_id);
    }

    #[test]
    fn test_split_direction() {
        use crate::protocol::SplitDirection;

        let horizontal = SplitDirection::Horizontal;
        let vertical = SplitDirection::Vertical;

        // Test serialization
        let h_json = serde_json::to_string(&horizontal).unwrap();
        let v_json = serde_json::to_string(&vertical).unwrap();

        assert!(h_json.contains("Horizontal"));
        assert!(v_json.contains("Vertical"));

        // Test deserialization
        let h_parsed: SplitDirection = serde_json::from_str(&h_json).unwrap();
        let v_parsed: SplitDirection = serde_json::from_str(&v_json).unwrap();

        assert_eq!(h_parsed, horizontal);
        assert_eq!(v_parsed, vertical);
    }
}