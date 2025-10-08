// TCP transport implementation

use async_trait::async_trait;
use bytes::Bytes;
use tokio::net::TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use std::net::SocketAddr;

use crate::error::{Result, FerrixError};
use super::{Transport, TransportStats};

/// TCP-based transport
pub struct TcpTransport {
    addr: SocketAddr,
    stream: Option<TcpStream>,
    stats: TransportStats,
}

impl TcpTransport {
    pub fn new(addr: SocketAddr) -> Self {
        Self {
            addr,
            stream: None,
            stats: TransportStats::default(),
        }
    }
}

#[async_trait]
impl Transport for TcpTransport {
    async fn connect(&mut self) -> Result<()> {
        let stream = TcpStream::connect(&self.addr).await
            .map_err(|e| FerrixError::Other(format!("Failed to connect to {}: {}", self.addr, e)))?;

        stream.set_nodelay(true)?;
        self.stream = Some(stream);
        Ok(())
    }

    async fn send(&mut self, data: Bytes) -> Result<()> {
        let stream = self.stream.as_mut()
            .ok_or_else(|| FerrixError::Other("Not connected".to_string()))?;

        stream.write_all(&data).await?;
        stream.flush().await?;

        self.stats.bytes_sent += data.len() as u64;
        self.stats.packets_sent += 1;

        Ok(())
    }

    async fn receive(&mut self) -> Result<Bytes> {
        let stream = self.stream.as_mut()
            .ok_or_else(|| FerrixError::Other("Not connected".to_string()))?;

        let mut buf = vec![0u8; 65536];
        let n = stream.read(&mut buf).await?;

        if n == 0 {
            return Err(FerrixError::Other("Connection closed".to_string()));
        }

        buf.truncate(n);
        self.stats.bytes_received += n as u64;
        self.stats.packets_received += 1;

        Ok(Bytes::from(buf))
    }

    async fn close(&mut self) -> Result<()> {
        if let Some(mut stream) = self.stream.take() {
            stream.shutdown().await?;
        }
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.stream.is_some()
    }

    fn remote_addr(&self) -> Option<SocketAddr> {
        Some(self.addr)
    }

    fn stats(&self) -> TransportStats {
        self.stats.clone()
    }
}
