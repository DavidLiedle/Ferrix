// Transport layer abstractions for different connection types

pub mod ssh;
pub mod mosh;
pub mod tcp;

use async_trait::async_trait;
use bytes::Bytes;
use crate::error::Result;
use std::net::SocketAddr;

/// Generic transport trait that all connection types implement
#[async_trait]
pub trait Transport: Send + Sync {
    /// Connect to a remote endpoint
    async fn connect(&mut self) -> Result<()>;

    /// Send data over the transport
    async fn send(&mut self, data: Bytes) -> Result<()>;

    /// Receive data from the transport
    async fn receive(&mut self) -> Result<Bytes>;

    /// Close the transport connection
    async fn close(&mut self) -> Result<()>;

    /// Check if the transport is connected
    fn is_connected(&self) -> bool;

    /// Get the remote address (if applicable)
    fn remote_addr(&self) -> Option<SocketAddr>;

    /// Get transport statistics
    fn stats(&self) -> TransportStats;
}

/// Statistics about transport performance
#[derive(Debug, Clone, Default)]
pub struct TransportStats {
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub packets_sent: u64,
    pub packets_received: u64,
    pub latency_ms: Option<u64>,
    pub packet_loss_rate: f32,
}

/// Transport configuration
#[derive(Debug, Clone)]
pub enum TransportConfig {
    /// Direct TCP connection
    Tcp {
        addr: SocketAddr,
        tls: bool,
    },

    /// SSH tunnel
    Ssh {
        host: String,
        port: u16,
        username: String,
        auth: SshAuth,
        forward_port: u16,
    },

    /// Mosh UDP connection
    Mosh {
        host: String,
        port: u16,
        key: Vec<u8>,
    },
}

/// SSH authentication methods
#[derive(Debug, Clone)]
pub enum SshAuth {
    Password(String),
    PublicKey {
        private_key_path: String,
        passphrase: Option<String>,
    },
    Agent,
}
