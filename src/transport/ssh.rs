// SSH transport implementation using libssh2

use async_trait::async_trait;
use bytes::Bytes;
use std::net::{SocketAddr, TcpStream as StdTcpStream};
use std::path::Path;
use std::io::{Read, Write};

use crate::error::{Result, FerrixError};
use super::{Transport, TransportStats, SshAuth};

#[cfg(feature = "ssh")]
use ssh2::Session;

/// SSH tunnel transport
pub struct SshTransport {
    host: String,
    port: u16,
    username: String,
    auth: SshAuth,
    forward_port: u16,

    #[cfg(feature = "ssh")]
    session: Option<Session>,
    #[cfg(feature = "ssh")]
    channel: Option<ssh2::Channel>,

    stats: TransportStats,
}

impl SshTransport {
    pub fn new(
        host: String,
        port: u16,
        username: String,
        auth: SshAuth,
        forward_port: u16,
    ) -> Self {
        Self {
            host,
            port,
            username,
            auth,
            forward_port,
            #[cfg(feature = "ssh")]
            session: None,
            #[cfg(feature = "ssh")]
            channel: None,
            stats: TransportStats::default(),
        }
    }

    #[cfg(feature = "ssh")]
    fn authenticate_session(&self, session: &Session) -> Result<()> {
        match &self.auth {
            SshAuth::Password(password) => {
                session.userauth_password(&self.username, password)
                    .map_err(|e| FerrixError::Other(format!("SSH password authentication failed: {}", e)))?;
            }
            SshAuth::PublicKey { private_key_path, passphrase } => {
                let key_path = Path::new(private_key_path);
                session.userauth_pubkey_file(
                    &self.username,
                    None,
                    key_path,
                    passphrase.as_deref()
                ).map_err(|e| FerrixError::Other(format!("SSH key authentication failed: {}", e)))?;
            }
            SshAuth::Agent => {
                let mut agent = session.agent()
                    .map_err(|e| FerrixError::Other(format!("Failed to connect to SSH agent: {}", e)))?;

                agent.connect()
                    .map_err(|e| FerrixError::Other(format!("Failed to connect to SSH agent: {}", e)))?;

                agent.list_identities()
                    .map_err(|e| FerrixError::Other(format!("Failed to list SSH identities: {}", e)))?;

                let identities = agent.identities()
                    .map_err(|e| FerrixError::Other(format!("Failed to get SSH identities: {}", e)))?;

                for identity in identities {
                    if agent.userauth(&self.username, &identity).is_ok() {
                        return Ok(());
                    }
                }

                return Err(FerrixError::Other("No valid SSH identity found".to_string()));
            }
        }

        if !session.authenticated() {
            return Err(FerrixError::Other("SSH authentication failed".to_string()));
        }

        Ok(())
    }
}

#[async_trait]
impl Transport for SshTransport {
    async fn connect(&mut self) -> Result<()> {
        #[cfg(not(feature = "ssh"))]
        {
            return Err(FerrixError::Other(
                "SSH transport requires 'ssh' feature to be enabled".to_string()
            ));
        }

        #[cfg(feature = "ssh")]
        {
            // Connect to SSH server
            let tcp = StdTcpStream::connect(format!("{}:{}", self.host, self.port))
                .map_err(|e| FerrixError::Other(format!("Failed to connect to SSH server: {}", e)))?;

            tcp.set_nodelay(true)?;

            let mut session = Session::new()
                .map_err(|e| FerrixError::Other(format!("Failed to create SSH session: {}", e)))?;

            session.set_tcp_stream(tcp);
            session.handshake()
                .map_err(|e| FerrixError::Other(format!("SSH handshake failed: {}", e)))?;

            // Authenticate
            self.authenticate_session(&session)?;

            // Create port forwarding channel to local Ferrix server
            let channel = session.channel_direct_tcpip(
                "127.0.0.1",
                self.forward_port,
                None
            ).map_err(|e| FerrixError::Other(format!("Failed to create SSH tunnel: {}", e)))?;

            self.session = Some(session);
            self.channel = Some(channel);

            Ok(())
        }
    }

    async fn send(&mut self, data: Bytes) -> Result<()> {
        #[cfg(not(feature = "ssh"))]
        {
            let _ = data;
            Err(FerrixError::Other("SSH transport not available".to_string()))
        }

        #[cfg(feature = "ssh")]
        {
            let channel = self.channel.as_mut()
                .ok_or_else(|| FerrixError::Other("Not connected".to_string()))?;

            channel.write_all(&data)
                .map_err(|e| FerrixError::Other(format!("Failed to send data: {}", e)))?;

            channel.flush()
                .map_err(|e| FerrixError::Other(format!("Failed to flush: {}", e)))?;

            self.stats.bytes_sent += data.len() as u64;
            self.stats.packets_sent += 1;

            Ok(())
        }
    }

    async fn receive(&mut self) -> Result<Bytes> {
        #[cfg(not(feature = "ssh"))]
        {
            Err(FerrixError::Other("SSH transport not available".to_string()))
        }

        #[cfg(feature = "ssh")]
        {
            let channel = self.channel.as_mut()
                .ok_or_else(|| FerrixError::Other("Not connected".to_string()))?;

            let mut buf = vec![0u8; 65536];
            let n = channel.read(&mut buf)
                .map_err(|e| FerrixError::Other(format!("Failed to receive data: {}", e)))?;

            if n == 0 {
                return Err(FerrixError::Other("SSH connection closed".to_string()));
            }

            buf.truncate(n);
            self.stats.bytes_received += n as u64;
            self.stats.packets_received += 1;

            Ok(Bytes::from(buf))
        }
    }

    async fn close(&mut self) -> Result<()> {
        #[cfg(feature = "ssh")]
        {
            if let Some(mut channel) = self.channel.take() {
                let _ = channel.close();
                let _ = channel.wait_close();
            }

            if let Some(session) = self.session.take() {
                let _ = session.disconnect(None, "Ferrix disconnect", None);
            }
        }

        Ok(())
    }

    fn is_connected(&self) -> bool {
        #[cfg(not(feature = "ssh"))]
        {
            false
        }

        #[cfg(feature = "ssh")]
        {
            self.session.is_some() && self.channel.is_some()
        }
    }

    fn remote_addr(&self) -> Option<SocketAddr> {
        // SSH doesn't provide direct socket address
        None
    }

    fn stats(&self) -> TransportStats {
        self.stats.clone()
    }
}
