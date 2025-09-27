use bytes::{Buf, BufMut, BytesMut};
use tokio_util::codec::{Decoder, Encoder};

use crate::error::{FerrixError, Result};
use super::messages::{ClientMessage, ServerMessage};

pub struct FerrixCodec;

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