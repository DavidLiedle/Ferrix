use std::sync::Arc;
use std::collections::HashSet;
use tokio::sync::{RwLock, broadcast};
use tokio::time::{interval, Duration};
use tracing::{info, warn};
use dashmap::DashMap;

use crate::protocol::{SessionId, ClientId, PaneId, ServerMessage};
use crate::error::Result;
use super::session::Session;
use super::ClientConnection;

/// Manages sessions and their associated clients, handling multi-client attachment
pub struct SessionManager {
    /// All active sessions - DashMap provides lock-free concurrent access
    sessions: Arc<DashMap<SessionId, Arc<RwLock<Session>>>>,

    /// Mapping of session IDs to the clients attached to them
    session_clients: Arc<DashMap<SessionId, HashSet<ClientId>>>,

    /// All connected clients
    clients: Arc<DashMap<ClientId, ClientConnection>>,

    /// Broadcast channel for session updates
    update_sender: broadcast::Sender<SessionUpdate>,

    /// Session output polling tasks
    session_pollers: Arc<DashMap<SessionId, tokio::task::JoinHandle<()>>>,

    /// Auto-save task handle
    auto_save_task: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
}

#[derive(Debug, Clone)]
pub struct SessionUpdate {
    pub session_id: SessionId,
    pub update_type: UpdateType,
}

#[derive(Debug, Clone)]
pub enum UpdateType {
    PaneOutput { pane_id: PaneId, data: Vec<u8> },
    LayoutChanged,
    SessionClosed,
}

impl SessionManager {
    pub fn new(
        sessions: Arc<DashMap<SessionId, Arc<RwLock<Session>>>>,
        clients: Arc<DashMap<ClientId, ClientConnection>>,
    ) -> Self {
        let (update_sender, _) = broadcast::channel(1000);

        Self {
            sessions,
            session_clients: Arc::new(DashMap::new()),
            clients,
            update_sender,
            session_pollers: Arc::new(DashMap::new()),
            auto_save_task: Arc::new(RwLock::new(None)),
        }
    }

    /// Attach a client to a session
    pub async fn attach_client(&self, client_id: ClientId, session_id: SessionId) -> Result<()> {
        // Check if session exists - DashMap provides lock-free concurrent access
        if !self.sessions.contains_key(&session_id) {
            return Err(crate::error::FerrixError::SessionNotFound(session_id.0.to_string()));
        }

        // Add client to session mapping
        self.session_clients.entry(session_id.clone())
            .or_default()
            .insert(client_id);

        // Update client's attached session
        if let Some(mut client) = self.clients.get_mut(&client_id) {
            client.attached_session = Some(session_id.clone());
        }

        // Start polling task for this session if not already running
        if !self.session_pollers.contains_key(&session_id) {
            let handle = self.start_session_poller(session_id.clone()).await;
            self.session_pollers.insert(session_id.clone(), handle);
        }

        info!("Client {} attached to session {}", client_id.0, session_id.0);
        Ok(())
    }

    /// Detach a client from its current session
    pub async fn detach_client(&self, client_id: ClientId) -> Result<()> {
        // Get and clear the client's attached session
        let session_id = self.clients.get_mut(&client_id)
            .and_then(|mut client| {
                let session = client.attached_session.clone();
                client.attached_session = None;
                session
            });

        if let Some(session_id) = session_id {
            // Remove client from session mapping
            let should_stop_poller = self.session_clients.get_mut(&session_id)
                .map(|mut clients| {
                    clients.remove(&client_id);
                    clients.is_empty()
                })
                .unwrap_or(false);

            // Stop polling task if no clients are attached
            if should_stop_poller {
                if let Some((_, handle)) = self.session_pollers.remove(&session_id) {
                    handle.abort();
                    info!("Stopped polling for session {} (no attached clients)", session_id.0);
                }
            }

            info!("Client {} detached from session {}", client_id.0, session_id.0);
        }

        Ok(())
    }

    /// Start a polling task for a session that broadcasts updates to all attached clients
    async fn start_session_poller(&self, session_id: SessionId) -> tokio::task::JoinHandle<()> {
        let sessions = self.sessions.clone();
        let session_clients = self.session_clients.clone();
        let clients = self.clients.clone();
        let update_sender = self.update_sender.clone();

        tokio::spawn(async move {
            info!("Starting output poller for session {}", session_id.0);

            loop {
                tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

                // Get session - DashMap lock-free access
                let session_arc = sessions.get(&session_id).map(|entry| entry.value().clone());

                if let Some(session_arc) = session_arc {
                    let mut session_guard = session_arc.write().await;

                    // Get all pane outputs
                    if let Ok(pane_outputs) = session_guard.get_all_pane_outputs().await {
                        for (pane_id, output) in pane_outputs {
                            if !output.is_empty() {
                                // Get list of attached clients - DashMap lock-free access
                                let client_ids = session_clients.get(&session_id)
                                    .map(|entry| entry.value().clone())
                                    .unwrap_or_default();

                                if client_ids.is_empty() {
                                    // No clients attached, exit poller
                                    info!("No clients attached to session {}, stopping poller", session_id.0);
                                    return;
                                }

                                // Send output to all attached clients
                                for client_id in client_ids {
                                    if let Some(client) = clients.get(&client_id) {
                                        let _ = client.sender.send(ServerMessage::PaneOutput {
                                            pane_id: pane_id.clone(),
                                            data: output.clone()
                                        }).await;
                                    }
                                }

                                // Also broadcast the update
                                let _ = update_sender.send(SessionUpdate {
                                    session_id: session_id.clone(),
                                    update_type: UpdateType::PaneOutput {
                                        pane_id,
                                        data: output
                                    }
                                });
                            }
                        }
                    }
                } else {
                    // Session no longer exists, exit poller
                    warn!("Session {} no longer exists, stopping poller", session_id.0);
                    return;
                }
            }
        })
    }

    /// Get a list of clients attached to a session
    pub async fn get_session_clients(&self, session_id: &SessionId) -> Vec<ClientId> {
        self.session_clients.get(session_id)
            .map(|entry| entry.value().iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Broadcast a message to all clients attached to a session
    pub async fn broadcast_to_session(&self, session_id: &SessionId, message: ServerMessage) {
        let client_ids = self.get_session_clients(session_id).await;

        for client_id in client_ids {
            if let Some(client) = self.clients.get(&client_id) {
                let _ = client.sender.send(message.clone()).await;
            }
        }
    }

    /// Subscribe to session updates
    pub fn subscribe_updates(&self) -> broadcast::Receiver<SessionUpdate> {
        self.update_sender.subscribe()
    }

    /// Clean up when a client disconnects
    pub async fn handle_client_disconnect(&self, client_id: ClientId) {
        // Detach the client from any session
        let _ = self.detach_client(client_id).await;

        // Remove the client from the clients map - DashMap direct removal
        self.clients.remove(&client_id);

        info!("Cleaned up disconnected client {}", client_id.0);
    }

    /// Start auto-save task for all sessions
    pub async fn start_auto_save(&self, check_interval: Duration) {
        let sessions = self.sessions.clone();

        let handle = tokio::spawn(async move {
            let mut timer = interval(check_interval);

            loop {
                timer.tick().await;

                // DashMap iteration - no global lock needed
                for entry in sessions.iter() {
                    let session_id = entry.key();
                    let session_arc = entry.value();
                    let session = session_arc.read().await;

                    if session.should_auto_save() {
                        // Create a snapshot for auto-save
                        let auto_save_name = format!("auto-save-{}", chrono::Utc::now().format("%Y%m%d-%H%M%S"));
                        let snapshot = session.create_snapshot(Some(auto_save_name.clone()), Some("Automatic save".to_string()));

                        // Create snapshot manager to save
                        match super::snapshot::SnapshotManager::new() {
                            Ok(snapshot_manager) => {
                                match snapshot_manager.save_snapshot(&snapshot) {
                                    Ok(path) => {
                                        drop(session);
                                        let mut session_mut = session_arc.write().await;
                                        session_mut.mark_auto_saved();
                                        info!("Auto-saved session {} to {:?}", session_id.0, path);
                                    }
                                    Err(e) => {
                                        warn!("Failed to auto-save session {}: {}", session_id.0, e);
                                    }
                                }
                            }
                            Err(e) => {
                                warn!("Failed to create snapshot manager: {}", e);
                            }
                        }
                    }
                }
            }
        });

        let mut auto_save_task = self.auto_save_task.write().await;
        *auto_save_task = Some(handle);
    }

    /// Stop auto-save task
    pub async fn stop_auto_save(&self) {
        let mut auto_save_task = self.auto_save_task.write().await;
        if let Some(handle) = auto_save_task.take() {
            handle.abort();
            info!("Auto-save task stopped");
        }
    }

    /// Enable auto-save for a specific session
    pub async fn enable_session_auto_save(&self, session_id: SessionId, interval_seconds: u64) -> Result<()> {
        if let Some(session_arc) = self.sessions.get(&session_id) {
            let mut session = session_arc.write().await;
            session.enable_auto_save(interval_seconds);
            info!("Enabled auto-save for session {} with interval {}s", session_id.0, interval_seconds);
            Ok(())
        } else {
            Err(crate::error::FerrixError::SessionNotFound(session_id.0.to_string()))
        }
    }

    /// Disable auto-save for a specific session
    pub async fn disable_session_auto_save(&self, session_id: SessionId) -> Result<()> {
        if let Some(session_arc) = self.sessions.get(&session_id) {
            let mut session = session_arc.write().await;
            session.disable_auto_save();
            info!("Disabled auto-save for session {}", session_id.0);
            Ok(())
        } else {
            Err(crate::error::FerrixError::SessionNotFound(session_id.0.to_string()))
        }
    }
}