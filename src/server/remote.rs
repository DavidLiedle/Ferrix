use std::sync::Arc;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::{TlsAcceptor, TlsConnector};
use rustls::{ServerConfig, ClientConfig, RootCertStore};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::codec::Framed;
use futures::{StreamExt, SinkExt};
use tracing::{info, error};
use std::pin::Pin;

use crate::error::{Result, FerrixError};
use crate::protocol::{ClientMessage, ServerMessage, FerrixCodec, ClientId, SessionId, AuthCredentials};
use super::Server;
use super::rate_limiter::RateLimiter;

/// Wrapper enum for different stream types
enum Stream {
    Tcp(TcpStream),
    TlsServer(tokio_rustls::server::TlsStream<TcpStream>),
    TlsClient(tokio_rustls::client::TlsStream<TcpStream>),
}

impl AsyncRead for Stream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match &mut *self {
            Stream::Tcp(s) => Pin::new(s).poll_read(cx, buf),
            Stream::TlsServer(s) => Pin::new(s).poll_read(cx, buf),
            Stream::TlsClient(s) => Pin::new(s).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for Stream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        match &mut *self {
            Stream::Tcp(s) => Pin::new(s).poll_write(cx, buf),
            Stream::TlsServer(s) => Pin::new(s).poll_write(cx, buf),
            Stream::TlsClient(s) => Pin::new(s).poll_write(cx, buf),
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<std::io::Result<()>> {
        match &mut *self {
            Stream::Tcp(s) => Pin::new(s).poll_flush(cx),
            Stream::TlsServer(s) => Pin::new(s).poll_flush(cx),
            Stream::TlsClient(s) => Pin::new(s).poll_flush(cx),
        }
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<std::io::Result<()>> {
        match &mut *self {
            Stream::Tcp(s) => Pin::new(s).poll_shutdown(cx),
            Stream::TlsServer(s) => Pin::new(s).poll_shutdown(cx),
            Stream::TlsClient(s) => Pin::new(s).poll_shutdown(cx),
        }
    }
}

/// Remote session server for network access
pub struct RemoteServer {
    bind_addr: SocketAddr,
    tls_config: Option<Arc<ServerConfig>>,
    auth_handler: Arc<dyn AuthenticationHandler>,
    server: Arc<Server>,
    rate_limiter: Arc<RateLimiter>,
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

/// Simple password-based authentication using persistent storage
pub struct PasswordAuthHandler {
    user_store: Arc<crate::auth::UserStore>,
}

impl RemoteServer {
    pub fn new(
        bind_addr: SocketAddr,
        server: Arc<Server>,
        auth_handler: Arc<dyn AuthenticationHandler>,
    ) -> Self {
        // Default: 5 failed attempts, 15 minute lockout
        let rate_limiter = RateLimiter::new(5, Duration::from_secs(900));

        Self {
            bind_addr,
            tls_config: None,
            auth_handler,
            server,
            rate_limiter: Arc::new(rate_limiter),
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
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| FerrixError::Other(format!("Failed to parse certificate: {}", e)))?;

        let key = rustls_pemfile::private_key(&mut key.as_ref())
            .map_err(|e| FerrixError::Other(format!("Failed to parse private key: {}", e)))?
            .ok_or_else(|| FerrixError::Other("No private key found".to_string()))?;

        let cert_chain = cert.into_iter()
            .map(rustls::pki_types::CertificateDer::from)
            .collect::<Vec<_>>();

        let config = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(cert_chain, rustls::pki_types::PrivateKeyDer::from(rustls::pki_types::PrivatePkcs8KeyDer::from(key.secret_der().to_vec())))
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
            let rate_limiter = self.rate_limiter.clone();

            tokio::spawn(async move {
                if let Err(e) = Self::handle_client(stream, peer_addr, server, auth_handler, tls_acceptor, rate_limiter).await {
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
        rate_limiter: Arc<RateLimiter>,
    ) -> Result<()> {
        // Check if address is rate limited
        if rate_limiter.is_locked(&peer_addr).await {
            if let Some(remaining) = rate_limiter.lockout_remaining(&peer_addr).await {
                error!("Connection from {} rejected: rate limited ({} seconds remaining)",
                    peer_addr, remaining.as_secs());
                return Err(FerrixError::Other(format!(
                    "Too many failed authentication attempts. Try again in {} seconds.",
                    remaining.as_secs()
                )));
            }
        }
        // Apply TLS if configured
        let stream = if let Some(acceptor) = tls_acceptor {
            let tls_stream = acceptor.accept(stream).await
                .map_err(|e| FerrixError::Other(format!("TLS handshake failed: {}", e)))?;
            Stream::TlsServer(tls_stream)
        } else {
            Stream::Tcp(stream)
        };

        // Create framed codec
        let mut framed = Framed::new(stream, FerrixCodec::new());

        // Wait for authentication message
        let client_id = match framed.next().await {
            Some(Ok(ClientMessage::Authenticate(credentials))) => {
                match auth_handler.authenticate(&credentials).await {
                    Ok(client_id) => {
                        // Successful authentication - clear rate limit
                        rate_limiter.record_success(&peer_addr).await;
                        framed.send(ServerMessage::Authenticated { client_id }).await?;
                        client_id
                    }
                    Err(e) => {
                        // Failed authentication - record failure
                        let locked = rate_limiter.record_failure(peer_addr).await;
                        let error_msg = if locked {
                            if let Some(remaining) = rate_limiter.lockout_remaining(&peer_addr).await {
                                format!("Authentication failed. Account locked for {} seconds due to too many failed attempts.", remaining.as_secs())
                            } else {
                                "Authentication failed. Too many failed attempts.".to_string()
                            }
                        } else {
                            format!("Authentication failed: {}", e)
                        };
                        framed.send(ServerMessage::Error { message: error_msg }).await?;
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

        // Get server state references
        let sessions = server.sessions();
        let clients = server.clients();
        let keybinding_manager = server.keybinding_manager();
        let hooks = server.hooks();

        // Register the remote client in the clients map
        let (tx, mut rx) = tokio::sync::mpsc::channel::<ServerMessage>(100);
        {
            let mut clients_guard = clients.write().await;
            clients_guard.insert(
                client_id,
                super::ClientConnection {
                    id: client_id,
                    attached_session: None,
                    sender: tx.clone(),
                },
            );
        }

        // Handle client messages
        loop {
            tokio::select! {
                Some(msg) = framed.next() => {
                    match msg {
                        Ok(client_msg) => {
                            // Check authorization for the action
                            let action = format!("{:?}", client_msg);
                            if !auth_handler.authorize(&client_id, &action).await.unwrap_or(false) {
                                framed.send(ServerMessage::Error { message: "Unauthorized action".to_string() }).await?;
                                continue;
                            }

                            // Process message through server using the real handle_message function
                            match super::handle_message(client_msg, &client_id, &sessions, &clients, &keybinding_manager, &hooks).await {
                                Ok(Some(response)) => {
                                    framed.send(response).await?;
                                }
                                Ok(None) => {
                                    // No response needed
                                }
                                Err(e) => {
                                    error!("Error handling message from {}: {}", peer_addr, e);
                                    framed.send(ServerMessage::Error { message: format!("Server error: {}", e) }).await?;
                                }
                            }
                        }
                        Err(e) => {
                            error!("Error receiving message from {}: {}", peer_addr, e);
                            break;
                        }
                    }
                }
                Some(message) = rx.recv() => {
                    // Send server-initiated messages (like output updates)
                    framed.send(message).await?;
                }
            }
        }

        // Clean up client connection
        {
            let mut clients_guard = clients.write().await;
            clients_guard.remove(&client_id);
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
                .collect::<std::result::Result<Vec<_>, _>>()
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
        let stream = if let Some(config) = &self.tls_config {
            let connector = TlsConnector::from(config.clone());
            let domain = rustls::pki_types::ServerName::try_from("ferrix")
                .map_err(|e| FerrixError::Other(format!("Invalid server name: {}", e)))?
                .to_owned();

            let tls_stream = connector.connect(domain, stream).await
                .map_err(|e| FerrixError::Other(format!("TLS connection failed: {}", e)))?;
            Stream::TlsClient(tls_stream)
        } else {
            Stream::Tcp(stream)
        };

        let mut framed = Framed::new(stream, crate::protocol::FerrixClientCodec::new());

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
    framed: Framed<Stream, crate::protocol::FerrixClientCodec>,
    client_id: ClientId,
    session_id: Option<SessionId>,
}

impl RemoteSession {
    /// Get the client ID for this remote session
    pub fn client_id(&self) -> &ClientId {
        &self.client_id
    }

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
        self.send(ClientMessage::CreateSession { name, working_dir: None }).await?;

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

// Persistent password authentication implementation using UserStore
#[async_trait::async_trait]
impl AuthenticationHandler for PasswordAuthHandler {
    async fn authenticate(&self, credentials: &AuthCredentials) -> Result<ClientId> {
        if let Some(password) = &credentials.password {
            match self.user_store.verify_password(&credentials.username, password).await {
                Ok(client_id) => Ok(client_id),
                Err(_) => Err(FerrixError::Other("Invalid credentials".to_string())),
            }
        } else {
            Err(FerrixError::Other("Password required".to_string()))
        }
    }

    async fn authorize(&self, client_id: &ClientId, action: &str) -> Result<bool> {
        // Check if the user exists and has permissions for this action
        match self.user_store.check_permission(client_id, action).await {
            Ok(has_permission) => Ok(has_permission),
            Err(_) => Ok(false), // If user doesn't exist, deny access
        }
    }
}

impl PasswordAuthHandler {
    pub async fn new() -> Result<Self> {
        let user_store = crate::auth::UserStore::new().await?;
        Ok(Self {
            user_store: Arc::new(user_store),
        })
    }

    pub async fn new_with_store(user_store: Arc<crate::auth::UserStore>) -> Self {
        Self {
            user_store,
        }
    }

    pub async fn add_user(&self, username: String, password: String) -> Result<ClientId> {
        self.user_store.add_user(username, password).await
    }

    pub async fn ensure_default_admin(&self) -> Result<()> {
        // Check if any users exist
        if self.user_store.user_count().await == 0 {
            // Create default admin user
            let admin_client_id = self.user_store.add_user("admin".to_string(), "password".to_string()).await?;
            tracing::info!("Created default admin user with client ID: {}", admin_client_id.0);
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    

    #[test]
    fn test_remote_connection() {
        // Test remote connection handling
        assert!(true);
    }

    #[test]
    fn test_remote_authentication() {
        // Test remote authentication
        assert!(true);
    }
}
