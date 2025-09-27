use std::sync::Arc;
use std::net::SocketAddr;
use std::path::PathBuf;
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::{TlsAcceptor, TlsConnector};
use rustls::{ServerConfig, ClientConfig, RootCertStore};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_util::codec::Framed;
use futures::{StreamExt, SinkExt};
use tracing::{info, warn, error};
use std::collections::HashMap;
use tokio::sync::RwLock;

use crate::error::{Result, FerrixError};
use crate::protocol::{ClientMessage, ServerMessage, FerrixCodec, ClientId, SessionId};
use super::Server;

/// Remote session server for network access
pub struct RemoteServer {
    bind_addr: SocketAddr,
    tls_config: Option<Arc<ServerConfig>>,
    auth_handler: Arc<dyn AuthenticationHandler>,
    server: Arc<Server>,
}

/// Client connector for remote sessions
pub struct RemoteClient {
    server_addr: SocketAddr,
    tls_config: Option<Arc<ClientConfig>>,
    auth_credentials: AuthCredentials,
}

/// Authentication handler trait
#[async_trait::async_trait]
pub trait AuthenticationHandler: Send + Sync {
    async fn authenticate(&self, credentials: &AuthCredentials) -> Result<ClientId>;
    async fn authorize(&self, client_id: &ClientId, action: &str) -> Result<bool>;
}

/// Authentication credentials
#[derive(Debug, Clone)]
pub struct AuthCredentials {
    pub username: String,
    pub password: Option<String>,
    pub token: Option<String>,
    pub certificate: Option<Vec<u8>>,
}

/// Simple password-based authentication
pub struct PasswordAuthHandler {
    users: Arc<RwLock<HashMap<String, UserInfo>>>,
}

struct UserInfo {
    password_hash: String,
    client_id: ClientId,
    permissions: Vec<String>,
}

impl RemoteServer {
    pub fn new(
        bind_addr: SocketAddr,
        server: Arc<Server>,
        auth_handler: Arc<dyn AuthenticationHandler>,
    ) -> Self {
        Self {
            bind_addr,
            tls_config: None,
            auth_handler,
            server,
        }
    }

    /// Enable TLS with certificate and key
    pub fn with_tls(mut self, cert_path: &PathBuf, key_path: &PathBuf) -> Result<Self> {
        let cert = std::fs::read(cert_path)
            .map_err(|e| FerrixError::Other(format!("Failed to read certificate: {}", e)))?;

        let key = std::fs::read(key_path)
            .map_err(|e| FerrixError::Other(format!("Failed to read key: {}", e)))?;

        let cert = rustls_pemfile::certs(&mut cert.as_ref())
            .map(|c| c.map(|c| c.to_vec()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| FerrixError::Other(format!("Failed to parse certificate: {}", e)))?;

        let key = rustls_pemfile::private_key(&mut key.as_ref())
            .map_err(|e| FerrixError::Other(format!("Failed to parse private key: {}", e)))?
            .ok_or_else(|| FerrixError::Other("No private key found".to_string()))?;

        let cert_chain = cert.into_iter()
            .map(|c| rustls::pki_types::CertificateDer::from(c))
            .collect::<Vec<_>>();

        let config = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(cert_chain, rustls::pki_types::PrivateKeyDer::from(key.secret_der().to_vec()))
            .map_err(|e| FerrixError::Other(format!("Failed to create TLS config: {}", e)))?;

        self.tls_config = Some(Arc::new(config));
        Ok(self)
    }

    /// Start the remote server
    pub async fn start(self) -> Result<()> {
        let listener = TcpListener::bind(&self.bind_addr).await
            .map_err(|e| FerrixError::Other(format!("Failed to bind to address: {}", e)))?;

        info!("Remote server listening on {}", self.bind_addr);

        let tls_acceptor = self.tls_config.as_ref().map(|config| TlsAcceptor::from(config.clone()));

        loop {
            let (stream, peer_addr) = listener.accept().await
                .map_err(|e| FerrixError::Other(format!("Failed to accept connection: {}", e)))?;

            info!("New remote connection from {}", peer_addr);

            let server = self.server.clone();
            let auth_handler = self.auth_handler.clone();
            let tls_acceptor = tls_acceptor.clone();

            tokio::spawn(async move {
                if let Err(e) = Self::handle_client(stream, peer_addr, server, auth_handler, tls_acceptor).await {
                    error!("Error handling remote client {}: {}", peer_addr, e);
                }
            });
        }
    }

    async fn handle_client(
        stream: TcpStream,
        peer_addr: SocketAddr,
        server: Arc<Server>,
        auth_handler: Arc<dyn AuthenticationHandler>,
        tls_acceptor: Option<TlsAcceptor>,
    ) -> Result<()> {
        // Apply TLS if configured
        let stream: Box<dyn AsyncReadExt + AsyncWriteExt + Send + Unpin> = if let Some(acceptor) = tls_acceptor {
            let tls_stream = acceptor.accept(stream).await
                .map_err(|e| FerrixError::Other(format!("TLS handshake failed: {}", e)))?;
            Box::new(tls_stream)
        } else {
            Box::new(stream)
        };

        // Create framed codec
        let mut framed = Framed::new(stream, FerrixCodec::new());

        // Wait for authentication message
        let client_id = match framed.next().await {
            Some(Ok(ClientMessage::Authenticate(credentials))) => {
                match auth_handler.authenticate(&credentials).await {
                    Ok(client_id) => {
                        framed.send(ServerMessage::Authenticated { client_id: client_id.clone() }).await?;
                        client_id
                    }
                    Err(e) => {
                        framed.send(ServerMessage::Error { message: format!("Authentication failed: {}", e) }).await?;
                        return Ok(());
                    }
                }
            }
            _ => {
                framed.send(ServerMessage::Error { message: "Authentication required".to_string() }).await?;
                return Ok(());
            }
        };

        info!("Remote client {} authenticated as {:?}", peer_addr, client_id);

        // Handle client messages
        while let Some(msg) = framed.next().await {
            match msg {
                Ok(client_msg) => {
                    // Check authorization for the action
                    let action = format!("{:?}", client_msg);
                    if !auth_handler.authorize(&client_id, &action).await.unwrap_or(false) {
                        framed.send(ServerMessage::Error { message: "Unauthorized action".to_string() }).await?;
                        continue;
                    }

                    // Process message through server
                    // This would integrate with the existing server message handling
                    // For now, just echo back success
                    framed.send(ServerMessage::Success).await?;
                }
                Err(e) => {
                    error!("Error receiving message from {}: {}", peer_addr, e);
                    break;
                }
            }
        }

        info!("Remote client {} disconnected", peer_addr);
        Ok(())
    }
}

impl RemoteClient {
    pub fn new(server_addr: SocketAddr, credentials: AuthCredentials) -> Self {
        Self {
            server_addr,
            tls_config: None,
            auth_credentials: credentials,
        }
    }

    /// Enable TLS for client connection
    pub fn with_tls(mut self, ca_cert_path: Option<&PathBuf>) -> Result<Self> {
        let mut root_store = RootCertStore::empty();

        if let Some(ca_path) = ca_cert_path {
            // Load custom CA certificate
            let ca_cert = std::fs::read(ca_path)
                .map_err(|e| FerrixError::Other(format!("Failed to read CA certificate: {}", e)))?;

            let ca_certs = rustls_pemfile::certs(&mut ca_cert.as_ref())
                .map(|c| c.map(|c| c.to_vec()))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| FerrixError::Other(format!("Failed to parse CA certificate: {}", e)))?;

            for cert in ca_certs {
                root_store.add(rustls::pki_types::CertificateDer::from(cert))
                    .map_err(|e| FerrixError::Other(format!("Failed to add CA certificate: {}", e)))?;
            }
        } else {
            // Use system root certificates
            root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        }

        let config = ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth();

        self.tls_config = Some(Arc::new(config));
        Ok(self)
    }

    /// Connect to remote server
    pub async fn connect(&self) -> Result<RemoteSession> {
        let stream = TcpStream::connect(&self.server_addr).await
            .map_err(|e| FerrixError::Other(format!("Failed to connect to server: {}", e)))?;

        // Apply TLS if configured
        let stream: Box<dyn AsyncReadExt + AsyncWriteExt + Send + Unpin> = if let Some(config) = &self.tls_config {
            let connector = TlsConnector::from(config.clone());
            let domain = rustls::pki_types::ServerName::try_from("ferrix")
                .map_err(|e| FerrixError::Other(format!("Invalid server name: {}", e)))?
                .to_owned();

            let tls_stream = connector.connect(domain, stream).await
                .map_err(|e| FerrixError::Other(format!("TLS connection failed: {}", e)))?;
            Box::new(tls_stream)
        } else {
            Box::new(stream)
        };

        let mut framed = Framed::new(stream, FerrixCodec::new());

        // Send authentication
        framed.send(ClientMessage::Authenticate(self.auth_credentials.clone())).await?;

        // Wait for authentication response
        match framed.next().await {
            Some(Ok(ServerMessage::Authenticated { client_id })) => {
                info!("Successfully authenticated with remote server as {:?}", client_id);

                Ok(RemoteSession {
                    framed,
                    client_id,
                    session_id: None,
                })
            }
            Some(Ok(ServerMessage::Error { message })) => {
                Err(FerrixError::Other(format!("Authentication failed: {}", message)))
            }
            _ => {
                Err(FerrixError::Other("Unexpected authentication response".to_string()))
            }
        }
    }
}

/// Active remote session
pub struct RemoteSession {
    framed: Framed<Box<dyn AsyncReadExt + AsyncWriteExt + Send + Unpin>, FerrixCodec>,
    client_id: ClientId,
    session_id: Option<SessionId>,
}

impl RemoteSession {
    /// Send a message to the remote server
    pub async fn send(&mut self, message: ClientMessage) -> Result<()> {
        self.framed.send(message).await
            .map_err(|e| FerrixError::Other(format!("Failed to send message: {}", e)))
    }

    /// Receive a message from the remote server
    pub async fn receive(&mut self) -> Result<Option<ServerMessage>> {
        match self.framed.next().await {
            Some(Ok(msg)) => Ok(Some(msg)),
            Some(Err(e)) => Err(FerrixError::Other(format!("Failed to receive message: {}", e))),
            None => Ok(None),
        }
    }

    /// Create a new session on the remote server
    pub async fn create_session(&mut self, name: Option<String>) -> Result<SessionId> {
        self.send(ClientMessage::CreateSession { name }).await?;

        match self.receive().await? {
            Some(ServerMessage::SessionCreated { session_id, .. }) => {
                self.session_id = Some(session_id.clone());
                Ok(session_id)
            }
            Some(ServerMessage::Error { message }) => {
                Err(FerrixError::Other(format!("Failed to create session: {}", message)))
            }
            _ => Err(FerrixError::Other("Unexpected response".to_string()))
        }
    }

    /// Attach to a remote session
    pub async fn attach_session(&mut self, session_id: SessionId) -> Result<()> {
        self.send(ClientMessage::AttachSession { session_id: session_id.clone() }).await?;

        match self.receive().await? {
            Some(ServerMessage::SessionAttached { .. }) => {
                self.session_id = Some(session_id);
                Ok(())
            }
            Some(ServerMessage::Error { message }) => {
                Err(FerrixError::Other(format!("Failed to attach session: {}", message)))
            }
            _ => Err(FerrixError::Other("Unexpected response".to_string()))
        }
    }

    /// Send input to the attached session
    pub async fn send_input(&mut self, data: Vec<u8>) -> Result<()> {
        self.send(ClientMessage::Input { data }).await
    }

    /// Disconnect from the remote server
    pub async fn disconnect(mut self) -> Result<()> {
        if self.session_id.is_some() {
            self.send(ClientMessage::DetachSession).await?;
        }
        Ok(())
    }
}

// Simple password authentication implementation
#[async_trait::async_trait]
impl AuthenticationHandler for PasswordAuthHandler {
    async fn authenticate(&self, credentials: &AuthCredentials) -> Result<ClientId> {
        let users = self.users.read().await;

        if let Some(user) = users.get(&credentials.username) {
            if let Some(password) = &credentials.password {
                // In production, use proper password hashing (bcrypt, argon2, etc.)
                if user.password_hash == format!("{:x}", md5::compute(password)) {
                    return Ok(user.client_id.clone());
                }
            }
        }

        Err(FerrixError::Other("Invalid credentials".to_string()))
    }

    async fn authorize(&self, client_id: &ClientId, action: &str) -> Result<bool> {
        let users = self.users.read().await;

        for user in users.values() {
            if user.client_id == *client_id {
                // Check if user has permission for this action
                // For now, allow all actions for authenticated users
                return Ok(true);
            }
        }

        Ok(false)
    }
}

impl PasswordAuthHandler {
    pub fn new() -> Self {
        Self {
            users: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn add_user(&self, username: String, password: String, client_id: ClientId) {
        let mut users = self.users.write().await;
        users.insert(username, UserInfo {
            password_hash: format!("{:x}", md5::compute(password)),
            client_id,
            permissions: vec!["all".to_string()],
        });
    }
}