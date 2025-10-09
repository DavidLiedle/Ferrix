// Mosh-inspired UDP transport implementation
// Mosh uses UDP with state synchronization for reliable connection over unreliable networks

use async_trait::async_trait;
use bytes::{Bytes, BytesMut};
use tokio::net::UdpSocket;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::Mutex;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

use crate::error::{Result, FerrixError};
use super::{Transport, TransportStats};

/// Mosh-style UDP transport with state synchronization
pub struct MoshTransport {
    host: String,
    port: u16,
    key: Vec<u8>,
    socket: Option<Arc<UdpSocket>>,
    remote_addr: Option<SocketAddr>,

    // State synchronization
    send_sequence: Arc<Mutex<u64>>,
    recv_sequence: Arc<Mutex<u64>>,
    pending_acks: Arc<Mutex<VecDeque<(u64, Instant)>>>,

    // Packet buffers
    send_buffer: Arc<Mutex<VecDeque<MoshPacket>>>,
    #[allow(dead_code)] // Reserved for future out-of-order packet reordering
    recv_buffer: Arc<Mutex<VecDeque<MoshPacket>>>,

    stats: Arc<Mutex<TransportStats>>,
}

/// Mosh packet structure
#[derive(Debug, Clone)]
struct MoshPacket {
    sequence: u64,
    #[allow(dead_code)] // Reserved for RTT calculation
    timestamp: Instant,
    data: Bytes,
    #[allow(dead_code)] // Reserved for selective retransmission
    ack_received: bool,
}

impl MoshTransport {
    pub fn new(host: String, port: u16, key: Vec<u8>) -> Self {
        Self {
            host,
            port,
            key,
            socket: None,
            remote_addr: None,
            send_sequence: Arc::new(Mutex::new(0)),
            recv_sequence: Arc::new(Mutex::new(0)),
            pending_acks: Arc::new(Mutex::new(VecDeque::new())),
            send_buffer: Arc::new(Mutex::new(VecDeque::new())),
            recv_buffer: Arc::new(Mutex::new(VecDeque::new())),
            stats: Arc::new(Mutex::new(TransportStats::default())),
        }
    }

    /// Encode packet with sequence number and simple encryption
    fn encode_packet(&self, sequence: u64, data: &[u8]) -> Vec<u8> {
        let mut packet = Vec::with_capacity(8 + data.len());

        // Add sequence number
        packet.extend_from_slice(&sequence.to_be_bytes());

        // Simple XOR encryption with key (in production, use ChaCha20-Poly1305)
        let encrypted: Vec<u8> = data.iter()
            .enumerate()
            .map(|(i, &b)| b ^ self.key[i % self.key.len()])
            .collect();

        packet.extend_from_slice(&encrypted);
        packet
    }

    /// Decode packet
    fn decode_packet(&self, packet: &[u8]) -> Result<(u64, Bytes)> {
        if packet.len() < 8 {
            return Err(FerrixError::Other("Invalid packet size".to_string()));
        }

        let sequence = u64::from_be_bytes(
            packet[0..8].try_into()
                .map_err(|_| FerrixError::Other("Failed to parse sequence number".to_string()))?
        );

        // Decrypt
        let decrypted: Vec<u8> = packet[8..].iter()
            .enumerate()
            .map(|(i, &b)| b ^ self.key[i % self.key.len()])
            .collect();

        Ok((sequence, Bytes::from(decrypted)))
    }

    /// Start keepalive task
    async fn start_keepalive(&self) {
        let socket = match self.socket.as_ref() {
            Some(s) => s.clone(),
            None => return,
        };
        let remote_addr = match self.remote_addr {
            Some(addr) => addr,
            None => return,
        };
        let send_sequence = self.send_sequence.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(1));

            loop {
                interval.tick().await;

                // Send keepalive packet
                let seq = {
                    let mut seq = send_sequence.lock().await;
                    *seq += 1;
                    *seq
                };

                let keepalive = vec![0u8; 1]; // Empty keepalive
                if socket.send_to(&keepalive, remote_addr).await.is_err() {
                    break;
                }
            }
        });
    }

    /// Retransmit unacknowledged packets
    async fn retransmit_task(&self) {
        let socket = match self.socket.as_ref() {
            Some(s) => s.clone(),
            None => return,
        };
        let remote_addr = match self.remote_addr {
            Some(addr) => addr,
            None => return,
        };
        let pending_acks = self.pending_acks.clone();
        let send_buffer = self.send_buffer.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(100));

            loop {
                interval.tick().await;

                let now = Instant::now();
                let acks = pending_acks.lock().await;
                let buffer = send_buffer.lock().await;

                // Retransmit packets older than 100ms
                for (seq, timestamp) in acks.iter() {
                    if now.duration_since(*timestamp) > Duration::from_millis(100) {
                        if let Some(packet) = buffer.iter().find(|p| p.sequence == *seq) {
                            let _ = socket.send_to(&packet.data, remote_addr).await;
                        }
                    }
                }
            }
        });
    }
}

#[async_trait]
impl Transport for MoshTransport {
    async fn connect(&mut self) -> Result<()> {
        #[cfg(not(feature = "mosh"))]
        {
            return Err(FerrixError::Other(
                "Mosh transport requires 'mosh' feature to be enabled".to_string()
            ));
        }

        #[cfg(feature = "mosh")]
        {
            // Bind to local UDP socket
            let socket = UdpSocket::bind("0.0.0.0:0").await
                .map_err(|e| FerrixError::Other(format!("Failed to bind UDP socket: {}", e)))?;

            // Resolve remote address
            let remote_addr = tokio::net::lookup_host(format!("{}:{}", self.host, self.port))
                .await
                .map_err(|e| FerrixError::Other(format!("Failed to resolve host: {}", e)))?
                .next()
                .ok_or_else(|| FerrixError::Other("No address found for host".to_string()))?;

            // Send initial connection packet
            let init_packet = self.encode_packet(0, b"FERRIX_MOSH_INIT");
            socket.send_to(&init_packet, remote_addr).await
                .map_err(|e| FerrixError::Other(format!("Failed to send init packet: {}", e)))?;

            self.socket = Some(Arc::new(socket));
            self.remote_addr = Some(remote_addr);

            // Start background tasks
            self.start_keepalive().await;
            self.retransmit_task().await;

            Ok(())
        }

        #[cfg(not(feature = "mosh"))]
        Ok(())
    }

    async fn send(&mut self, data: Bytes) -> Result<()> {
        let socket = self.socket.as_ref()
            .ok_or_else(|| FerrixError::Other("Not connected".to_string()))?;

        let remote_addr = self.remote_addr
            .ok_or_else(|| FerrixError::Other("Remote address not set".to_string()))?;

        // Get next sequence number
        let sequence = {
            let mut seq = self.send_sequence.lock().await;
            *seq += 1;
            *seq
        };

        // Encode packet
        let packet_data = self.encode_packet(sequence, &data);

        // Send packet
        socket.send_to(&packet_data, remote_addr).await
            .map_err(|e| FerrixError::Other(format!("Failed to send packet: {}", e)))?;

        // Track for retransmission
        let packet = MoshPacket {
            sequence,
            timestamp: Instant::now(),
            data: Bytes::from(packet_data),
            ack_received: false,
        };

        self.send_buffer.lock().await.push_back(packet.clone());
        self.pending_acks.lock().await.push_back((sequence, Instant::now()));

        // Update stats
        let mut stats = self.stats.lock().await;
        stats.bytes_sent += data.len() as u64;
        stats.packets_sent += 1;

        Ok(())
    }

    async fn receive(&mut self) -> Result<Bytes> {
        let socket = self.socket.as_ref()
            .ok_or_else(|| FerrixError::Other("Not connected".to_string()))?;

        let mut buf = vec![0u8; 65536];
        let (n, _addr) = socket.recv_from(&mut buf).await
            .map_err(|e| FerrixError::Other(format!("Failed to receive packet: {}", e)))?;

        buf.truncate(n);

        // Decode packet
        let (sequence, data) = self.decode_packet(&buf)?;

        // Check sequence number
        let expected_seq = {
            let mut seq = self.recv_sequence.lock().await;
            let expected = *seq + 1;
            *seq = sequence;
            expected
        };

        if sequence < expected_seq {
            // Duplicate packet, ignore
            return self.receive().await;
        }

        // Update stats
        let mut stats = self.stats.lock().await;
        stats.bytes_received += data.len() as u64;
        stats.packets_received += 1;

        // Calculate packet loss
        if sequence > expected_seq {
            let lost = (sequence - expected_seq) as f32;
            stats.packet_loss_rate = lost / sequence as f32;
        }

        Ok(data)
    }

    async fn close(&mut self) -> Result<()> {
        if let Some(socket) = self.socket.take() {
            if let Some(addr) = self.remote_addr {
                // Send disconnect packet
                let disconnect = self.encode_packet(u64::MAX, b"DISCONNECT");
                let _ = socket.send_to(&disconnect, addr).await;
            }
        }

        self.remote_addr = None;
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.socket.is_some() && self.remote_addr.is_some()
    }

    fn remote_addr(&self) -> Option<SocketAddr> {
        self.remote_addr
    }

    fn stats(&self) -> TransportStats {
        // This is blocking - in production, use async or return Arc<Mutex<>>
        futures::executor::block_on(async {
            self.stats.lock().await.clone()
        })
    }
}
