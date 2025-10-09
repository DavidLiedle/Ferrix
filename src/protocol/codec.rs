use bytes::{Buf, BufMut, BytesMut};
use tokio_util::codec::{Decoder, Encoder};

use crate::error::{FerrixError, Result};
use super::messages::{ClientMessage, ServerMessage};

/// Maximum message size: 10MB
/// Prevents OOM attacks from malicious clients sending huge messages
const MAX_MESSAGE_SIZE: usize = 10 * 1024 * 1024;

pub struct FerrixCodec;

impl Default for FerrixCodec {
    fn default() -> Self {
        Self::new()
    }
}

impl FerrixCodec {
    pub fn new() -> Self {
        FerrixCodec
    }
}

impl Decoder for FerrixCodec {
    type Item = ClientMessage;
    type Error = FerrixError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>> {
        if src.len() < 4 {
            return Ok(None);
        }

        let mut length_bytes = [0u8; 4];
        length_bytes.copy_from_slice(&src[..4]);
        let length = u32::from_be_bytes(length_bytes) as usize;

        // Protect against malicious huge messages that could cause OOM
        if length > MAX_MESSAGE_SIZE {
            return Err(FerrixError::Protocol(format!(
                "Message too large: {} bytes (max: {} bytes)",
                length, MAX_MESSAGE_SIZE
            )));
        }

        if src.len() < 4 + length {
            return Ok(None);
        }

        src.advance(4);
        let data = src.split_to(length);

        let message: ClientMessage = bincode::deserialize(&data)?;
        Ok(Some(message))
    }
}

impl Encoder<ServerMessage> for FerrixCodec {
    type Error = FerrixError;

    fn encode(&mut self, item: ServerMessage, dst: &mut BytesMut) -> Result<()> {
        let data = bincode::serialize(&item)?;
        let length = data.len() as u32;

        dst.reserve(4 + data.len());
        dst.put_u32(length);
        dst.put_slice(&data);

        Ok(())
    }
}

pub struct FerrixClientCodec;

impl Default for FerrixClientCodec {
    fn default() -> Self {
        Self::new()
    }
}

impl FerrixClientCodec {
    pub fn new() -> Self {
        FerrixClientCodec
    }
}

impl Decoder for FerrixClientCodec {
    type Item = ServerMessage;
    type Error = FerrixError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>> {
        if src.len() < 4 {
            return Ok(None);
        }

        let mut length_bytes = [0u8; 4];
        length_bytes.copy_from_slice(&src[..4]);
        let length = u32::from_be_bytes(length_bytes) as usize;

        // Protect against malicious huge messages that could cause OOM
        if length > MAX_MESSAGE_SIZE {
            return Err(FerrixError::Protocol(format!(
                "Message too large: {} bytes (max: {} bytes)",
                length, MAX_MESSAGE_SIZE
            )));
        }

        if src.len() < 4 + length {
            return Ok(None);
        }

        src.advance(4);
        let data = src.split_to(length);

        let message: ServerMessage = bincode::deserialize(&data)?;
        Ok(Some(message))
    }
}

impl Encoder<ClientMessage> for FerrixClientCodec {
    type Error = FerrixError;

    fn encode(&mut self, item: ClientMessage, dst: &mut BytesMut) -> Result<()> {
        let data = bincode::serialize(&item)?;
        let length = data.len() as u32;

        dst.reserve(4 + data.len());
        dst.put_u32(length);
        dst.put_slice(&data);

        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use bytes::BytesMut;

    #[test]
    fn test_message_size_limit_server_codec() {
        let mut codec = FerrixCodec::new();
        let mut buf = BytesMut::new();

        // Create a message claiming to be larger than MAX_MESSAGE_SIZE
        let fake_length = (MAX_MESSAGE_SIZE + 1) as u32;
        buf.put_u32(fake_length);

        // Should reject the oversized message
        let result = codec.decode(&mut buf);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Message too large"));
    }

    #[test]
    fn test_message_size_limit_client_codec() {
        let mut codec = FerrixClientCodec::new();
        let mut buf = BytesMut::new();

        // Create a message claiming to be larger than MAX_MESSAGE_SIZE
        let fake_length = (MAX_MESSAGE_SIZE + 1) as u32;
        buf.put_u32(fake_length);

        // Should reject the oversized message
        let result = codec.decode(&mut buf);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Message too large"));
    }

    #[test]
    fn test_message_size_limit_accepts_valid() {
        let mut codec = FerrixCodec::new();
        let mut buf = BytesMut::new();

        // Create a message within limits (but incomplete data)
        let valid_length = 1000u32;
        buf.put_u32(valid_length);

        // Should not error (just return Ok(None) because message incomplete)
        let result = codec.decode(&mut buf);
        assert!(result.is_ok());
        assert!(result.unwrap().is_none()); // Incomplete message
    }
}
