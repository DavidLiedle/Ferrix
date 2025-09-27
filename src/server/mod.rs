pub mod session;
pub mod window;
pub mod pane;
pub mod pty;
pub mod layout;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::net::UnixListener;
use tokio::sync::{RwLock, mpsc};
use tokio_util::codec::Framed;
use futures::{StreamExt, SinkExt};
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::error::{FerrixError, Result};
use crate::protocol::{ClientMessage, FerrixCodec, ServerMessage, SessionId, ClientId, SessionInfo};
use session::Session;

pub struct Server {
    sessions: Arc<RwLock<HashMap<SessionId, Arc<RwLock<Session>>>>>,
    clients: Arc<RwLock<HashMap<ClientId, ClientConnection>>>,
    socket_path: PathBuf,
}

struct ClientConnection {
    id: ClientId,
    attached_session: Option<SessionId>,
    sender: mpsc::Sender<ServerMessage>,
}

impl Server {
    pub fn new(socket_path: PathBuf) -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            clients: Arc::new(RwLock::new(HashMap::new())),
            socket_path,
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        if self.socket_path.exists() {
            std::fs::remove_file(&self.socket_path)?;
        }

        let listener = UnixListener::bind(&self.socket_path)?;
        info!("Server listening on {:?}", self.socket_path);

        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let client_id = ClientId(Uuid::new_v4());
                    let sessions = self.sessions.clone();
                    let clients = self.clients.clone();
                    let client_id_log = client_id.clone();

                    tokio::spawn(async move {
                        if let Err(e) = handle_client(stream, client_id, sessions, clients).await {
                            error!("Error handling client {}: {}", client_id_log.0, e);
                        }
                    });
                }
                Err(e) => {
                    error!("Failed to accept connection: {}", e);
                }
            }
        }
    }
}

async fn handle_client(
    stream: tokio::net::UnixStream,
    client_id: ClientId,
    sessions: Arc<RwLock<HashMap<SessionId, Arc<RwLock<Session>>>>>,
    clients: Arc<RwLock<HashMap<ClientId, ClientConnection>>>,
) -> Result<()> {
    info!("New client connected: {}", client_id.0);

    let (tx, mut rx) = mpsc::channel::<ServerMessage>(100);

    {
        let mut clients_guard = clients.write().await;
        clients_guard.insert(
            client_id.clone(),
            ClientConnection {
                id: client_id.clone(),
                attached_session: None,
                sender: tx.clone(),
            },
        );
    }

    let mut framed = Framed::new(stream, FerrixCodec);

    loop {
        tokio::select! {
            Some(result) = framed.next() => {
                match result {
                    Ok(message) => {
                        let response = handle_message(
                            message,
                            &client_id,
                            &sessions,
                            &clients,
                        ).await?;

                        if let Some(resp) = response {
                            framed.send(resp).await?;
                        }
                    }
                    Err(e) => {
                        error!("Error receiving message from client {}: {}", client_id.0, e);
                        break;
                    }
                }
            }
            Some(message) = rx.recv() => {
                framed.send(message).await?;
            }
        }
    }

    {
        let mut clients_guard = clients.write().await;
        clients_guard.remove(&client_id);
    }

    info!("Client disconnected: {}", client_id.0);
    Ok(())
}

async fn handle_message(
    message: ClientMessage,
    client_id: &ClientId,
    sessions: &Arc<RwLock<HashMap<SessionId, Arc<RwLock<Session>>>>>,
    clients: &Arc<RwLock<HashMap<ClientId, ClientConnection>>>,
) -> Result<Option<ServerMessage>> {
    match message {
        ClientMessage::CreateSession { name } => {
            let session_id = SessionId(Uuid::new_v4());
            let session_name = name.unwrap_or_else(|| format!("session-{}", Uuid::new_v4()));

            let session = Session::new(session_id.clone(), session_name.clone());

            {
                let mut sessions_guard = sessions.write().await;
                sessions_guard.insert(session_id.clone(), Arc::new(RwLock::new(session)));
            }

            info!("Created session: {} ({})", session_name, session_id.0);

            Ok(Some(ServerMessage::SessionCreated {
                session_id,
                name: session_name,
            }))
        }

        ClientMessage::AttachSession { session_id } => {
            let sessions_guard = sessions.read().await;

            if let Some(session) = sessions_guard.get(&session_id) {
                {
                    let mut clients_guard = clients.write().await;
                    if let Some(client) = clients_guard.get_mut(client_id) {
                        client.attached_session = Some(session_id.clone());
                    }
                }

                // Start a task to poll PTY output
                let session_clone = session.clone();
                let clients_clone = clients.clone();
                let client_id_clone = client_id.clone();
                let session_id_clone = session_id.clone();

                tokio::spawn(async move {
                    loop {
                        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

                        let mut session_guard = session_clone.write().await;
                        if let Ok(Some(output)) = session_guard.get_output().await {
                            if !output.is_empty() {
                                let clients_guard = clients_clone.read().await;
                                if let Some(client) = clients_guard.get(&client_id_clone) {
                                    if client.attached_session == Some(session_id_clone.clone()) {
                                        let _ = client.sender.send(ServerMessage::Output { data: output }).await;
                                    } else {
                                        break;
                                    }
                                } else {
                                    break;
                                }
                            }
                        }
                    }
                });

                info!("Client {} attached to session {}", client_id.0, session_id.0);

                Ok(Some(ServerMessage::SessionAttached { session_id }))
            } else {
                Ok(Some(ServerMessage::Error {
                    message: format!("Session not found: {}", session_id.0),
                }))
            }
        }

        ClientMessage::DetachSession => {
            {
                let mut clients_guard = clients.write().await;
                if let Some(client) = clients_guard.get_mut(client_id) {
                    client.attached_session = None;
                }
            }

            info!("Client {} detached from session", client_id.0);

            Ok(Some(ServerMessage::SessionDetached))
        }

        ClientMessage::ListSessions => {
            let sessions_guard = sessions.read().await;
            let mut session_list = Vec::new();

            for (id, session) in sessions_guard.iter() {
                let session_guard = session.read().await;
                session_list.push(SessionInfo {
                    id: id.clone(),
                    name: session_guard.name.clone(),
                    attached_clients: 0, // TODO: Count actual attached clients
                    windows: session_guard.windows.len(),
                    created_at: session_guard.created_at,
                });
            }

            Ok(Some(ServerMessage::SessionList {
                sessions: session_list,
            }))
        }

        ClientMessage::KillSession { session_id } => {
            let mut sessions_guard = sessions.write().await;

            if sessions_guard.remove(&session_id).is_some() {
                info!("Killed session {}", session_id.0);
                Ok(Some(ServerMessage::SessionKilled { session_id }))
            } else {
                Ok(Some(ServerMessage::Error {
                    message: format!("Session not found: {}", session_id.0),
                }))
            }
        }

        ClientMessage::Input { data } => {
            let clients_guard = clients.read().await;
            if let Some(client) = clients_guard.get(client_id) {
                if let Some(session_id) = &client.attached_session {
                    let sessions_guard = sessions.read().await;
                    if let Some(session) = sessions_guard.get(session_id) {
                        let mut session_guard = session.write().await;
                        session_guard.handle_input(data).await?;
                    }
                }
            }
            Ok(None)
        }

        ClientMessage::Resize { cols, rows } => {
            let clients_guard = clients.read().await;
            if let Some(client) = clients_guard.get(client_id) {
                if let Some(session_id) = &client.attached_session {
                    let sessions_guard = sessions.read().await;
                    if let Some(session) = sessions_guard.get(session_id) {
                        let mut session_guard = session.write().await;
                        session_guard.resize(cols, rows).await?;
                    }
                }
            }
            Ok(None)
        }

        ClientMessage::Ping => {
            Ok(Some(ServerMessage::Pong))
        }

        _ => {
            warn!("Unhandled message type: {:?}", message);
            Ok(Some(ServerMessage::Error {
                message: "Unimplemented feature".to_string(),
            }))
        }
    }
}