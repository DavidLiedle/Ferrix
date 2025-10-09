// ============================================================================
// TIER 1: Core Server Modules (always available)
// ============================================================================
pub mod session;
pub mod window;
pub mod pane;
pub mod pty;
pub mod layout;
pub mod layout_presets;
pub mod snapshot;
pub mod recovery;
pub mod session_manager;
pub mod activity;
pub mod scrollback;
pub mod hooks;

// ============================================================================
// TIER 2: Advanced Features (feature-gated)
// ============================================================================
pub mod recording;

#[cfg(feature = "remote")]
pub mod remote;

#[cfg(feature = "remote")]
pub mod rate_limiter;

#[cfg(feature = "performance")]
pub mod performance;

// ============================================================================
// TIER 3: Experimental Features (feature-gated)
// ============================================================================
#[cfg(feature = "versioning")]
pub mod versioning;

#[cfg(feature = "collaboration")]
pub mod collaboration;

#[cfg(feature = "time-travel")]
pub mod timetravel;
// #[cfg(test)]
// mod pty_tests;
// #[cfg(test)]
// mod remote_tests;
// #[cfg(test)]
// mod recovery_tests;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::net::UnixListener;
use tokio::sync::{RwLock, mpsc};
use tokio_util::codec::Framed;
use futures::{StreamExt, SinkExt};
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::error::Result;
use crate::protocol::{ClientMessage, FerrixCodec, ServerMessage, SessionId, ClientId, SessionInfo, SnapshotInfo};
use session::Session;
use snapshot::SnapshotManager;
use recovery::RecoveryManager;
use hooks::{HookManager, HookEvent, HookContext};

// Type alias for the sessions map to reduce complexity
type SessionMap = Arc<RwLock<HashMap<SessionId, Arc<RwLock<Session>>>>>;

#[derive(Clone)]
pub struct Server {
    sessions: SessionMap,
    clients: Arc<RwLock<HashMap<ClientId, ClientConnection>>>,
    keybinding_manager: Arc<RwLock<crate::config::keybindings::KeyBindingManager>>,
    hooks: Arc<RwLock<HookManager>>,
    socket_path: PathBuf,
}

pub struct ClientConnection {
    pub id: ClientId,
    pub attached_session: Option<SessionId>,
    pub sender: mpsc::Sender<ServerMessage>,
}

impl Server {
    pub fn new(socket_path: PathBuf) -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            clients: Arc::new(RwLock::new(HashMap::new())),
            keybinding_manager: Arc::new(RwLock::new(crate::config::keybindings::KeyBindingManager::new())),
            hooks: Arc::new(RwLock::new(HookManager::new())),
            socket_path,
        }
    }

    /// Get sessions reference for remote server access
    pub fn sessions(&self) -> SessionMap {
        self.sessions.clone()
    }

    /// Get clients reference for remote server access
    pub fn clients(&self) -> Arc<RwLock<HashMap<ClientId, ClientConnection>>> {
        self.clients.clone()
    }

    /// Get keybinding manager reference for remote server access
    pub fn keybinding_manager(&self) -> Arc<RwLock<crate::config::keybindings::KeyBindingManager>> {
        self.keybinding_manager.clone()
    }

    pub fn hooks(&self) -> Arc<RwLock<HookManager>> {
        self.hooks.clone()
    }

    pub async fn run(&mut self, enable_recovery: bool) -> Result<()> {
        if self.socket_path.exists() {
            std::fs::remove_file(&self.socket_path)?;
        }

        // Check for crash recovery
        let recovery_manager = Arc::new(RecoveryManager::new()?);

        // Attempt to recover crashed sessions (only if enabled)
        if enable_recovery {
            match recovery_manager.check_and_recover().await {
                Ok(recovered_sessions) => {
                    for snapshot in recovered_sessions {
                        let session = Session::from_snapshot(snapshot.clone());
                        let session_id = session.id.clone();
                        let session_name = session.name.clone();

                        let mut sessions_guard = self.sessions.write().await;
                        sessions_guard.insert(session_id.clone(), Arc::new(RwLock::new(session)));

                        info!("Recovered session {} ({}) from crash", session_name, session_id.0);
                    }
                }
                Err(e) => {
                    warn!("Failed to recover sessions: {}", e);
                }
            }
        } else {
            // Clear recovery file to start fresh
            if let Err(e) = recovery_manager.clear_recovery_file().await {
                warn!("Failed to clear recovery file: {}", e);
            }
        }

        // Setup signal handlers for graceful shutdown
        recovery::setup_signal_handlers(recovery_manager.clone(), self.sessions.clone());

        // Start auto-save task
        {
            let sessions_clone = self.sessions.clone();
            let recovery_manager_clone = recovery_manager.clone();
            tokio::spawn(async move {
                recovery_manager_clone.start_auto_save(sessions_clone).await;
            });
        }

        let listener = UnixListener::bind(&self.socket_path)?;
        info!("Server listening on {:?}", self.socket_path);

        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let client_id = ClientId(Uuid::new_v4());
                    let sessions = self.sessions.clone();
                    let clients = self.clients.clone();
                    let keybinding_manager = self.keybinding_manager.clone();
                    let hooks = self.hooks.clone();
                    let client_id_log = client_id;

                    tokio::spawn(async move {
                        if let Err(e) = handle_client(stream, client_id, sessions, clients, keybinding_manager, hooks).await {
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
    sessions: SessionMap,
    clients: Arc<RwLock<HashMap<ClientId, ClientConnection>>>,
    keybinding_manager: Arc<RwLock<crate::config::keybindings::KeyBindingManager>>,
    hooks: Arc<RwLock<HookManager>>,
) -> Result<()> {
    info!("New client connected: {}", client_id.0);

    let (tx, mut rx) = mpsc::channel::<ServerMessage>(100);

    {
        let mut clients_guard = clients.write().await;
        clients_guard.insert(
            client_id,
            ClientConnection {
                id: client_id,
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
                            &keybinding_manager,
                            &hooks,
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

pub async fn handle_message(
    message: ClientMessage,
    client_id: &ClientId,
    sessions: &SessionMap,
    clients: &Arc<RwLock<HashMap<ClientId, ClientConnection>>>,
    keybinding_manager: &Arc<RwLock<crate::config::keybindings::KeyBindingManager>>,
    hooks: &Arc<RwLock<HookManager>>,
) -> Result<Option<ServerMessage>> {
    match message {
        ClientMessage::CreateSession { name, working_dir } => {
            let session_id = SessionId(Uuid::new_v4());
            let session_name = name.unwrap_or_else(|| {
                // Generate a simple sequential session name like tmux (0, 1, 2, ...)
                let sessions_guard = futures::executor::block_on(sessions.read());
                let session_count = sessions_guard.len();
                format!("{}", session_count)
            });

            tracing::info!("Creating session with working_dir: {:?}", working_dir);
            let session = if let Some(cwd) = working_dir {
                tracing::info!("Using client working directory: {:?}", cwd);
                Session::new_with_working_dir(session_id.clone(), session_name.clone(), cwd)
            } else {
                tracing::info!("No working_dir provided, using server's current dir");
                Session::new(session_id.clone(), session_name.clone())
            };
            let session_arc = Arc::new(RwLock::new(session));

            {
                let mut sessions_guard = sessions.write().await;
                sessions_guard.insert(session_id.clone(), session_arc.clone());
            }

            // Start persistent PTY poller for this session
            // This runs independently of client connections
            let session_clone = session_arc.clone();
            let clients_clone = clients.clone();
            let session_id_clone = session_id.clone();
            let sessions_clone = sessions.clone();

            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

                    // Get pane outputs with minimal lock duration
                    let pane_outputs = {
                        let mut session_guard = session_clone.write().await;
                        session_guard.get_all_pane_outputs().await
                    };

                    if let Ok(pane_outputs) = pane_outputs {
                        for (pane_id, output) in pane_outputs {
                            if !output.is_empty() {
                                // Broadcast to all clients attached to this session
                                let clients_guard = clients_clone.read().await;
                                for (_, client) in clients_guard.iter() {
                                    if client.attached_session == Some(session_id_clone.clone()) {
                                        // Ignore send errors - client might have disconnected
                                        let _ = client.sender.send(ServerMessage::PaneOutput {
                                            pane_id: pane_id.clone(),
                                            data: output.clone()
                                        }).await;
                                    }
                                }
                            }
                        }
                    }

                    // Check if all panes are dead and auto-destroy session
                    let (all_panes_dead, auto_detach_enabled) = {
                        let session_guard = session_clone.read().await;
                        (session_guard.are_all_panes_dead().await, session_guard.auto_detach_on_exit)
                    };

                    if all_panes_dead && auto_detach_enabled {
                        // All panes are dead - detach clients and destroy session
                        tracing::info!("All panes in session {} are dead, destroying session", session_id_clone.0);

                        // Send detach message to all attached clients
                        let clients_guard = clients_clone.read().await;
                        for (_, client) in clients_guard.iter() {
                            if client.attached_session == Some(session_id_clone.clone()) {
                                let _ = client.sender.send(ServerMessage::SessionDetached).await;
                            }
                        }
                        drop(clients_guard);

                        // Remove session from sessions map
                        {
                            let mut sessions_guard = sessions_clone.write().await;
                            sessions_guard.remove(&session_id_clone);
                        }

                        // Exit the polling loop since session is destroyed
                        break;
                    }
                }
            });

            info!("Created session: {} ({})", session_name, session_id.0);

            // Trigger SessionCreated hook
            {
                let mut hooks_guard = hooks.write().await;
                let context = HookContext::new("session-created".to_string())
                    .with_session(session_id.clone());
                let _ = hooks_guard.trigger(HookEvent::SessionCreated, context).await;
            }

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

                // Note: PTY polling is now handled per-session, not per-client
                // This is started when the session is created, not when clients attach

                info!("Client {} attached to session {}", client_id.0, session_id.0);

                // Trigger ClientAttached hook
                {
                    let mut hooks_guard = hooks.write().await;
                    let context = HookContext::new("client-attached".to_string())
                        .with_session(session_id.clone());
                    let _ = hooks_guard.trigger(HookEvent::ClientAttached, context).await;
                }

                // Send layout info immediately after attach
                let session_guard = session.read().await;
                let session_name = session_guard.name.clone();
                if let Some(layout) = session_guard.get_layout_info().await {
                    // Get the client's sender channel
                    let clients_guard = clients.read().await;
                    if let Some(client) = clients_guard.get(client_id) {
                        // Send layout update asynchronously
                        let _ = client.sender.send(ServerMessage::LayoutUpdate { layout }).await;
                    }
                }

                // Send raw output buffers for all panes to restore session content
                // This allows newly attached clients to see previous output
                if let Some(current_window) = session_guard.get_current_window() {
                    let window_guard = current_window.read().await;
                    for (pane_id, pane_arc) in &window_guard.panes {
                        let pane_guard = pane_arc.read().await;
                        let buffer = pane_guard.get_raw_output_buffer();
                        if !buffer.is_empty() {
                            let clients_guard = clients.read().await;
                            if let Some(client) = clients_guard.get(client_id) {
                                let _ = client.sender.send(ServerMessage::PaneOutput {
                                    pane_id: pane_id.clone(),
                                    data: buffer.to_vec()
                                }).await;
                            }
                        }
                    }
                }
                drop(session_guard);

                // Send SessionAttached response
                Ok(Some(ServerMessage::SessionAttached { session_id, name: session_name }))
            } else {
                Ok(Some(ServerMessage::Error {
                    message: format!("Session not found: {}", session_id.0),
                }))
            }
        }

        ClientMessage::DetachSession => {
            let detached_session_id = {
                let mut clients_guard = clients.write().await;
                if let Some(client) = clients_guard.get_mut(client_id) {
                    let session_id = client.attached_session.clone();
                    client.attached_session = None;
                    session_id
                } else {
                    None
                }
            };

            info!("Client {} detached from session", client_id.0);

            // Trigger ClientDetached hook
            if let Some(session_id) = detached_session_id {
                let mut hooks_guard = hooks.write().await;
                let context = HookContext::new("client-detached".to_string())
                    .with_session(session_id);
                let _ = hooks_guard.trigger(HookEvent::ClientDetached, context).await;
            }

            Ok(Some(ServerMessage::SessionDetached))
        }

        ClientMessage::ListSessions => {
            let sessions_guard = sessions.read().await;
            let mut session_list = Vec::new();

            for (id, session) in sessions_guard.iter() {
                let session_guard = session.read().await;

                // Count actual attached clients for this session
                let clients_guard = clients.read().await;
                let attached_count = clients_guard.values()
                    .filter(|client| client.attached_session.as_ref() == Some(id))
                    .count();

                session_list.push(SessionInfo {
                    id: id.clone(),
                    name: session_guard.name.clone(),
                    attached_clients: attached_count,
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

                // Trigger SessionClosed hook
                {
                    let mut hooks_guard = hooks.write().await;
                    let context = HookContext::new("session-closed".to_string())
                        .with_session(session_id.clone());
                    let _ = hooks_guard.trigger(HookEvent::SessionClosed, context).await;
                }

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
                        // Reserve 1 row for status bar at bottom
                        let pane_rows = rows.saturating_sub(1).max(1);
                        session_guard.resize(cols, pane_rows).await?;

                        // Trigger ClientResized hook
                        {
                            let mut hooks_guard = hooks.write().await;
                            let context = HookContext::new("client-resized".to_string())
                                .with_session(session_id.clone());
                            let _ = hooks_guard.trigger(HookEvent::ClientResized, context).await;
                        }

                        // Send updated layout after resize
                        if let Some(layout) = session_guard.get_layout_info().await {
                            return Ok(Some(ServerMessage::LayoutUpdate { layout }));
                        }
                    }
                }
            }
            Ok(None)
        }

        ClientMessage::SaveSnapshot { session_id, name, description } => {
            let snapshot_manager = match SnapshotManager::new() {
                Ok(sm) => sm,
                Err(e) => {
                    return Ok(Some(ServerMessage::Error {
                        message: format!("Failed to initialize snapshot manager: {}", e),
                    }));
                }
            };

            let sessions_guard = sessions.read().await;
            if let Some(session_arc) = sessions_guard.get(&session_id) {
                let session_guard = session_arc.read().await;

                // Create snapshot from session state
                let snapshot = session_guard.create_snapshot(name, description);

                match snapshot_manager.save_snapshot(&snapshot) {
                    Ok(path) => {
                        info!("Saved snapshot to {:?}", path);
                        Ok(Some(ServerMessage::SnapshotSaved { path }))
                    }
                    Err(e) => {
                        error!("Failed to save snapshot: {}", e);
                        Ok(Some(ServerMessage::Error {
                            message: format!("Failed to save snapshot: {}", e),
                        }))
                    }
                }
            } else {
                Ok(Some(ServerMessage::Error {
                    message: format!("Session not found: {}", session_id.0),
                }))
            }
        }

        ClientMessage::ListSnapshots => {
            let snapshot_manager = match SnapshotManager::new() {
                Ok(sm) => sm,
                Err(e) => {
                    return Ok(Some(ServerMessage::Error {
                        message: format!("Failed to initialize snapshot manager: {}", e),
                    }));
                }
            };

            match snapshot_manager.list_snapshots() {
                Ok(snapshots) => {
                    let snapshot_infos: Vec<SnapshotInfo> = snapshots
                        .into_iter()
                        .map(|info| {
                            let size = std::fs::metadata(&info.path)
                                .map(|m| m.len())
                                .unwrap_or(0);

                            SnapshotInfo {
                                path: info.path,
                                name: info.metadata.name,
                                description: info.metadata.description,
                                session_name: info.session_name,
                                created_at: info.metadata.created_at,
                                size,
                            }
                        })
                        .collect();

                    Ok(Some(ServerMessage::SnapshotList { snapshots: snapshot_infos }))
                }
                Err(e) => {
                    Ok(Some(ServerMessage::Error {
                        message: format!("Failed to list snapshots: {}", e),
                    }))
                }
            }
        }

        ClientMessage::LoadSnapshot { path } => {
            let snapshot_manager = match SnapshotManager::new() {
                Ok(sm) => sm,
                Err(e) => {
                    return Ok(Some(ServerMessage::Error {
                        message: format!("Failed to initialize snapshot manager: {}", e),
                    }));
                }
            };

            match snapshot_manager.load_snapshot(&path) {
                Ok(snapshot) => {
                    // Create new session from snapshot
                    let session = Session::from_snapshot(snapshot.clone());
                    let session_id = session.id.clone();

                    let mut sessions_guard = sessions.write().await;
                    sessions_guard.insert(session_id.clone(), Arc::new(RwLock::new(session)));

                    info!("Loaded snapshot from {:?}", path);
                    Ok(Some(ServerMessage::SnapshotLoaded { session_id }))
                }
                Err(e) => {
                    Ok(Some(ServerMessage::Error {
                        message: format!("Failed to load snapshot: {}", e),
                    }))
                }
            }
        }

        ClientMessage::RestoreSnapshot { session_id, path } => {
            let snapshot_manager = match SnapshotManager::new() {
                Ok(sm) => sm,
                Err(e) => {
                    return Ok(Some(ServerMessage::Error {
                        message: format!("Failed to initialize snapshot manager: {}", e),
                    }));
                }
            };

            match snapshot_manager.load_snapshot(&path) {
                Ok(snapshot) => {
                    // Restore into existing session
                    let sessions_guard = sessions.read().await;
                    if let Some(session_arc) = sessions_guard.get(&session_id) {
                        let mut session = session_arc.write().await;
                        session.restore_from_snapshot(snapshot).await;
                        drop(session);
                        drop(sessions_guard);

                        info!("Restored snapshot from {:?} into session {}", path, session_id.0);
                        Ok(Some(ServerMessage::Output {
                            data: "Snapshot restored successfully\r\n".as_bytes().to_vec(),
                        }))
                    } else {
                        Ok(Some(ServerMessage::Error {
                            message: format!("Session {} not found", session_id.0),
                        }))
                    }
                }
                Err(e) => {
                    Ok(Some(ServerMessage::Error {
                        message: format!("Failed to load snapshot: {}", e),
                    }))
                }
            }
        }

        ClientMessage::DeleteSnapshot { path } => {
            let snapshot_manager = match SnapshotManager::new() {
                Ok(sm) => sm,
                Err(e) => {
                    return Ok(Some(ServerMessage::Error {
                        message: format!("Failed to initialize snapshot manager: {}", e),
                    }));
                }
            };

            match snapshot_manager.delete_snapshot(&path) {
                Ok(()) => {
                    info!("Deleted snapshot at {:?}", path);
                    Ok(Some(ServerMessage::SnapshotDeleted { path }))
                }
                Err(e) => {
                    Ok(Some(ServerMessage::Error {
                        message: format!("Failed to delete snapshot: {}", e),
                    }))
                }
            }
        }

        ClientMessage::Ping => {
            Ok(Some(ServerMessage::Pong))
        }

        ClientMessage::CreateWindow { name } => {
            if let Some(client) = clients.read().await.get(client_id) {
                if let Some(session_id) = &client.attached_session {
                    let mut sessions_guard = sessions.write().await;
                    if let Some(session) = sessions_guard.get_mut(session_id) {
                        let mut session_guard = session.write().await;
                        match session_guard.create_window(name.clone()).await {
                            Ok(window_id) => {
                                Ok(Some(ServerMessage::WindowCreated {
                                    window_id,
                                    name: name.unwrap_or_else(|| "window".to_string()),
                                }))
                            }
                            Err(e) => Ok(Some(ServerMessage::Error {
                                message: format!("Failed to create window: {}", e),
                            }))
                        }
                    } else {
                        Ok(Some(ServerMessage::Error {
                            message: "Session not found".to_string(),
                        }))
                    }
                } else {
                    Ok(Some(ServerMessage::Error {
                        message: "Not attached to a session".to_string(),
                    }))
                }
            } else {
                Ok(Some(ServerMessage::Error {
                    message: "Client not found".to_string(),
                }))
            }
        }

        ClientMessage::SwitchWindow { window_id } => {
            if let Some(client) = clients.read().await.get(client_id) {
                if let Some(session_id) = &client.attached_session {
                    let mut sessions_guard = sessions.write().await;
                    if let Some(session) = sessions_guard.get_mut(session_id) {
                        let mut session_guard = session.write().await;
                        match session_guard.switch_window(window_id.clone()).await {
                            Ok(()) => {
                                Ok(Some(ServerMessage::WindowSwitched { window_id }))
                            }
                            Err(e) => Ok(Some(ServerMessage::Error {
                                message: format!("Failed to switch window: {}", e),
                            }))
                        }
                    } else {
                        Ok(Some(ServerMessage::Error {
                            message: "Session not found".to_string(),
                        }))
                    }
                } else {
                    Ok(Some(ServerMessage::Error {
                        message: "Not attached to a session".to_string(),
                    }))
                }
            } else {
                Ok(Some(ServerMessage::Error {
                    message: "Client not found".to_string(),
                }))
            }
        }

        ClientMessage::CloseWindow { window_id } => {
            if let Some(client) = clients.read().await.get(client_id) {
                if let Some(session_id) = &client.attached_session {
                    let mut sessions_guard = sessions.write().await;
                    if let Some(session) = sessions_guard.get_mut(session_id) {
                        let mut session_guard = session.write().await;
                        match session_guard.close_window(window_id.clone()).await {
                            Ok(()) => {
                                Ok(Some(ServerMessage::WindowClosed { window_id }))
                            }
                            Err(e) => Ok(Some(ServerMessage::Error {
                                message: format!("Failed to close window: {}", e),
                            }))
                        }
                    } else {
                        Ok(Some(ServerMessage::Error {
                            message: "Session not found".to_string(),
                        }))
                    }
                } else {
                    Ok(Some(ServerMessage::Error {
                        message: "Not attached to a session".to_string(),
                    }))
                }
            } else {
                Ok(Some(ServerMessage::Error {
                    message: "Client not found".to_string(),
                }))
            }
        }

        ClientMessage::SplitPane { direction } => {
            if let Some(client) = clients.read().await.get(client_id) {
                if let Some(session_id) = &client.attached_session {
                    let mut sessions_guard = sessions.write().await;
                    if let Some(session) = sessions_guard.get_mut(session_id) {
                        let mut session_guard = session.write().await;
                        match session_guard.split_pane(direction).await {
                            Ok(pane_id) => {
                                // Send layout update after successful split
                                if let Some(layout) = session_guard.get_layout_info().await {
                                    Ok(Some(ServerMessage::LayoutUpdate { layout }))
                                } else {
                                    Ok(Some(ServerMessage::PaneCreated { pane_id }))
                                }
                            }
                            Err(e) => Ok(Some(ServerMessage::Error {
                                message: format!("Failed to split pane: {}", e),
                            }))
                        }
                    } else {
                        Ok(Some(ServerMessage::Error {
                            message: "Session not found".to_string(),
                        }))
                    }
                } else {
                    Ok(Some(ServerMessage::Error {
                        message: "Not attached to a session".to_string(),
                    }))
                }
            } else {
                Ok(Some(ServerMessage::Error {
                    message: "Client not found".to_string(),
                }))
            }
        }

        ClientMessage::SwitchPane { pane_id } => {
            Ok(Some(ServerMessage::PaneSwitched { pane_id }))
        }

        ClientMessage::NavigatePane { direction } => {
            if let Some(client) = clients.read().await.get(client_id) {
                if let Some(session_id) = &client.attached_session {
                    let mut sessions_guard = sessions.write().await;
                    if let Some(session) = sessions_guard.get_mut(session_id) {
                        let mut session_guard = session.write().await;

                        use crate::protocol::messages::PaneNavigationDirection;
                        use layout::NavigationDirection;

                        let nav_direction = match direction {
                            PaneNavigationDirection::Up => NavigationDirection::Up,
                            PaneNavigationDirection::Down => NavigationDirection::Down,
                            PaneNavigationDirection::Left => NavigationDirection::Left,
                            PaneNavigationDirection::Right => NavigationDirection::Right,
                        };

                        match session_guard.navigate_pane(nav_direction).await {
                            Ok(()) => {
                                // Send layout update to reflect focus change
                                if let Some(layout) = session_guard.get_layout_info().await {
                                    Ok(Some(ServerMessage::LayoutUpdate { layout }))
                                } else {
                                    Ok(None)
                                }
                            }
                            Err(e) => Ok(Some(ServerMessage::Error {
                                message: format!("Failed to navigate pane: {}", e),
                            }))
                        }
                    } else {
                        Ok(Some(ServerMessage::Error {
                            message: "Session not found".to_string(),
                        }))
                    }
                } else {
                    Ok(Some(ServerMessage::Error {
                        message: "Not attached to a session".to_string(),
                    }))
                }
            } else {
                Ok(Some(ServerMessage::Error {
                    message: "Client not found".to_string(),
                }))
            }
        }

        ClientMessage::SelectLastPane => {
            if let Some(client) = clients.read().await.get(client_id) {
                if let Some(session_id) = &client.attached_session {
                    let mut sessions_guard = sessions.write().await;
                    if let Some(session) = sessions_guard.get_mut(session_id) {
                        let mut session_guard = session.write().await;

                        match session_guard.select_last_pane().await {
                            Ok(()) => {
                                // Send layout update to reflect focus change
                                if let Some(layout) = session_guard.get_layout_info().await {
                                    Ok(Some(ServerMessage::LayoutUpdate { layout }))
                                } else {
                                    Ok(None)
                                }
                            }
                            Err(e) => Ok(Some(ServerMessage::Error {
                                message: format!("Failed to select last pane: {}", e),
                            }))
                        }
                    } else {
                        Ok(Some(ServerMessage::Error {
                            message: "Session not found".to_string(),
                        }))
                    }
                } else {
                    Ok(Some(ServerMessage::Error {
                        message: "Not attached to a session".to_string(),
                    }))
                }
            } else {
                Ok(Some(ServerMessage::Error {
                    message: "Client not found".to_string(),
                }))
            }
        }

        ClientMessage::SelectPaneByIndex { index } => {
            if let Some(client) = clients.read().await.get(client_id) {
                if let Some(session_id) = &client.attached_session {
                    let mut sessions_guard = sessions.write().await;
                    if let Some(session) = sessions_guard.get_mut(session_id) {
                        let mut session_guard = session.write().await;

                        match session_guard.select_pane_by_index(index) {
                            Ok(()) => {
                                // Send layout update to reflect focus change
                                if let Some(layout) = session_guard.get_layout_info().await {
                                    Ok(Some(ServerMessage::LayoutUpdate { layout }))
                                } else {
                                    Ok(None)
                                }
                            }
                            Err(e) => Ok(Some(ServerMessage::Error {
                                message: format!("Failed to select pane by index: {}", e),
                            }))
                        }
                    } else {
                        Ok(Some(ServerMessage::Error {
                            message: "Session not found".to_string(),
                        }))
                    }
                } else {
                    Ok(Some(ServerMessage::Error {
                        message: "Not attached to a session".to_string(),
                    }))
                }
            } else {
                Ok(Some(ServerMessage::Error {
                    message: "Client not found".to_string(),
                }))
            }
        }

        ClientMessage::ClosePane { pane_id } => {
            if let Some(client) = clients.read().await.get(client_id) {
                if let Some(session_id) = &client.attached_session {
                    let mut sessions_guard = sessions.write().await;
                    if let Some(session) = sessions_guard.get_mut(session_id) {
                        let mut session_guard = session.write().await;
                        match session_guard.close_pane(pane_id.clone()).await {
                            Ok(()) => {
                                Ok(Some(ServerMessage::PaneClosed { pane_id }))
                            }
                            Err(e) => Ok(Some(ServerMessage::Error {
                                message: format!("Failed to close pane: {}", e),
                            }))
                        }
                    } else {
                        Ok(Some(ServerMessage::Error {
                            message: "Session not found".to_string(),
                        }))
                    }
                } else {
                    Ok(Some(ServerMessage::Error {
                        message: "Not attached to a session".to_string(),
                    }))
                }
            } else {
                Ok(Some(ServerMessage::Error {
                    message: "Client not found".to_string(),
                }))
            }
        }

        ClientMessage::ResizePane { direction, amount } => {
            if let Some(client) = clients.read().await.get(client_id) {
                if let Some(session_id) = &client.attached_session {
                    let mut sessions_guard = sessions.write().await;
                    if let Some(session) = sessions_guard.get_mut(session_id) {
                        let mut session_guard = session.write().await;
                        match session_guard.resize_pane(direction, amount).await {
                            Ok(()) => {
                                // Send layout update after resizing
                                if let Some(layout) = session_guard.get_layout_info().await {
                                    Ok(Some(ServerMessage::LayoutUpdate { layout }))
                                } else {
                                    Ok(Some(ServerMessage::Success))
                                }
                            }
                            Err(e) => Ok(Some(ServerMessage::Error {
                                message: format!("Failed to resize pane: {}", e),
                            }))
                        }
                    } else {
                        Ok(Some(ServerMessage::Error {
                            message: "Session not found".to_string(),
                        }))
                    }
                } else {
                    Ok(Some(ServerMessage::Error {
                        message: "Not attached to a session".to_string(),
                    }))
                }
            } else {
                Ok(Some(ServerMessage::Error {
                    message: "Client not found".to_string(),
                }))
            }
        }

        ClientMessage::NextWindow => {
            if let Some(client) = clients.read().await.get(client_id) {
                if let Some(session_id) = &client.attached_session {
                    let mut sessions_guard = sessions.write().await;
                    if let Some(session) = sessions_guard.get_mut(session_id) {
                        let mut session_guard = session.write().await;
                        match session_guard.next_window().await {
                            Ok(()) => {
                                // Send layout update after switching windows
                                if let Some(layout) = session_guard.get_layout_info().await {
                                    Ok(Some(ServerMessage::LayoutUpdate { layout }))
                                } else {
                                    Ok(None)
                                }
                            },
                            Err(e) => Ok(Some(ServerMessage::Error {
                                message: format!("Failed to switch to next window: {}", e),
                            }))
                        }
                    } else {
                        Ok(Some(ServerMessage::Error {
                            message: "Session not found".to_string(),
                        }))
                    }
                } else {
                    Ok(Some(ServerMessage::Error {
                        message: "Not attached to a session".to_string(),
                    }))
                }
            } else {
                Ok(Some(ServerMessage::Error {
                    message: "Client not found".to_string(),
                }))
            }
        }

        ClientMessage::PreviousWindow => {
            if let Some(client) = clients.read().await.get(client_id) {
                if let Some(session_id) = &client.attached_session {
                    let mut sessions_guard = sessions.write().await;
                    if let Some(session) = sessions_guard.get_mut(session_id) {
                        let mut session_guard = session.write().await;
                        match session_guard.previous_window().await {
                            Ok(()) => {
                                // Send layout update after switching windows
                                if let Some(layout) = session_guard.get_layout_info().await {
                                    Ok(Some(ServerMessage::LayoutUpdate { layout }))
                                } else {
                                    Ok(None)
                                }
                            },
                            Err(e) => Ok(Some(ServerMessage::Error {
                                message: format!("Failed to switch to previous window: {}", e),
                            }))
                        }
                    } else {
                        Ok(Some(ServerMessage::Error {
                            message: "Session not found".to_string(),
                        }))
                    }
                } else {
                    Ok(Some(ServerMessage::Error {
                        message: "Not attached to a session".to_string(),
                    }))
                }
            } else {
                Ok(Some(ServerMessage::Error {
                    message: "Client not found".to_string(),
                }))
            }
        }

        ClientMessage::ZoomPane => {
            if let Some(client) = clients.read().await.get(client_id) {
                if let Some(session_id) = &client.attached_session {
                    let mut sessions_guard = sessions.write().await;
                    if let Some(session) = sessions_guard.get_mut(session_id) {
                        let mut session_guard = session.write().await;
                        match session_guard.zoom_pane().await {
                            Ok((zoomed, pane_id)) => Ok(Some(ServerMessage::PaneZoomStatusUpdate {
                                zoomed,
                                pane_id,
                            })),
                            Err(e) => Ok(Some(ServerMessage::Error {
                                message: format!("Failed to zoom pane: {}", e),
                            }))
                        }
                    } else {
                        Ok(Some(ServerMessage::Error {
                            message: "Session not found".to_string(),
                        }))
                    }
                } else {
                    Ok(Some(ServerMessage::Error {
                        message: "Not attached to a session".to_string(),
                    }))
                }
            } else {
                Ok(Some(ServerMessage::Error {
                    message: "Client not found".to_string(),
                }))
            }
        }

        ClientMessage::KillPane => {
            if let Some(client) = clients.read().await.get(client_id) {
                if let Some(session_id) = &client.attached_session {
                    let mut sessions_guard = sessions.write().await;
                    if let Some(session) = sessions_guard.get_mut(session_id) {
                        let mut session_guard = session.write().await;
                        if let Some(current_window) = session_guard.get_current_window() {
                            let focused_pane_id = {
                                let window_guard = current_window.read().await;
                                window_guard.get_focused_pane()
                            };
                            if let Some(focused_pane_id) = focused_pane_id {
                                match session_guard.close_pane(focused_pane_id).await {
                                    Ok(()) => Ok(None),
                                    Err(e) => Ok(Some(ServerMessage::Error {
                                        message: format!("Failed to kill pane: {}", e),
                                    }))
                                }
                            } else {
                                Ok(Some(ServerMessage::Error {
                                    message: "No focused pane".to_string(),
                                }))
                            }
                        } else {
                            Ok(Some(ServerMessage::Error {
                                message: "No current window".to_string(),
                            }))
                        }
                    } else {
                        Ok(Some(ServerMessage::Error {
                            message: "Session not found".to_string(),
                        }))
                    }
                } else {
                    Ok(Some(ServerMessage::Error {
                        message: "Not attached to a session".to_string(),
                    }))
                }
            } else {
                Ok(Some(ServerMessage::Error {
                    message: "Client not found".to_string(),
                }))
            }
        }

        ClientMessage::ListWindows => {
            if let Some(client) = clients.read().await.get(client_id) {
                if let Some(session_id) = &client.attached_session {
                    let sessions_guard = sessions.read().await;
                    if let Some(session) = sessions_guard.get(session_id) {
                        let session_guard = session.read().await;
                        let window_list = session_guard.list_windows();
                        Ok(Some(ServerMessage::WindowList { windows: window_list }))
                    } else {
                        Ok(Some(ServerMessage::Error {
                            message: "Session not found".to_string(),
                        }))
                    }
                } else {
                    Ok(Some(ServerMessage::Error {
                        message: "Not attached to a session".to_string(),
                    }))
                }
            } else {
                Ok(Some(ServerMessage::Error {
                    message: "Client not found".to_string(),
                }))
            }
        }

        ClientMessage::ApplyLayoutPreset { preset_name } => {
            if let Some(client) = clients.read().await.get(client_id) {
                if let Some(session_id) = &client.attached_session {
                    let sessions_guard = sessions.read().await;
                    if let Some(session) = sessions_guard.get(session_id) {
                        let mut session_guard = session.write().await;
                        if session_guard.apply_layout_preset(&preset_name) {
                            Ok(Some(ServerMessage::LayoutApplied { preset_name }))
                        } else {
                            Ok(Some(ServerMessage::Error {
                                message: format!("Unknown layout preset: {}", preset_name),
                            }))
                        }
                    } else {
                        Ok(Some(ServerMessage::Error {
                            message: "Session not found".to_string(),
                        }))
                    }
                } else {
                    Ok(Some(ServerMessage::Error {
                        message: "No session attached".to_string(),
                    }))
                }
            } else {
                Ok(Some(ServerMessage::Error {
                    message: "Client not found".to_string(),
                }))
            }
        }

        ClientMessage::CycleLayout => {
            if let Some(client) = clients.read().await.get(client_id) {
                if let Some(session_id) = &client.attached_session {
                    let sessions_guard = sessions.read().await;
                    if let Some(session) = sessions_guard.get(session_id) {
                        let mut session_guard = session.write().await;
                        let preset_name = session_guard.cycle_layout();
                        Ok(Some(ServerMessage::LayoutApplied { preset_name }))
                    } else {
                        Ok(Some(ServerMessage::Error {
                            message: "Session not found".to_string(),
                        }))
                    }
                } else {
                    Ok(Some(ServerMessage::Error {
                        message: "No session attached".to_string(),
                    }))
                }
            } else {
                Ok(Some(ServerMessage::Error {
                    message: "Client not found".to_string(),
                }))
            }
        }

        ClientMessage::ListLayoutPresets => {
            use crate::server::layout_presets::LayoutPreset;

            let all_presets = LayoutPreset::all_presets();
            let preset_infos: Vec<crate::protocol::LayoutPresetInfo> = all_presets
                .iter()
                .map(|preset| {
                    let layout = preset.to_layout();
                    crate::protocol::LayoutPresetInfo {
                        name: preset.name().to_string(),
                        description: preset.description().to_string(),
                        pane_count: layout.count_panes(),
                        is_custom: matches!(preset, LayoutPreset::Custom(_, _)),
                    }
                })
                .collect();

            Ok(Some(ServerMessage::LayoutPresetsList { presets: preset_infos }))
        }

        ClientMessage::EnterCopyMode => {
            if let Some(client) = clients.read().await.get(client_id) {
                if let Some(session_id) = &client.attached_session {
                    let mut sessions_guard = sessions.write().await;
                    if let Some(session) = sessions_guard.get_mut(session_id) {
                        let mut session_guard = session.write().await;
                        match session_guard.enter_copy_mode().await {
                            Ok(()) => {
                                // Get the buffer content and send initial update
                                if let Some(copy_mode_state) = session_guard.get_copy_mode_state().await {
                                    Ok(Some(ServerMessage::CopyModeUpdate {
                                        cursor_row: copy_mode_state.cursor_row,
                                        cursor_col: copy_mode_state.cursor_col,
                                        selection_start: copy_mode_state.selection_start,
                                        selection_end: copy_mode_state.selection_end,
                                        buffer_content: copy_mode_state.buffer_content,
                                        mode: copy_mode_state.mode,
                                    }))
                                } else {
                                    Ok(Some(ServerMessage::CopyModeEntered))
                                }
                            }
                            Err(e) => Ok(Some(ServerMessage::Error {
                                message: format!("Failed to enter copy mode: {}", e),
                            }))
                        }
                    } else {
                        Ok(Some(ServerMessage::Error {
                            message: "Session not found".to_string(),
                        }))
                    }
                } else {
                    Ok(Some(ServerMessage::Error {
                        message: "Not attached to a session".to_string(),
                    }))
                }
            } else {
                Ok(Some(ServerMessage::Error {
                    message: "Client not found".to_string(),
                }))
            }
        }

        ClientMessage::ExitCopyMode => {
            if let Some(client) = clients.read().await.get(client_id) {
                if let Some(session_id) = &client.attached_session {
                    let mut sessions_guard = sessions.write().await;
                    if let Some(session) = sessions_guard.get_mut(session_id) {
                        let mut session_guard = session.write().await;
                        match session_guard.exit_copy_mode().await {
                            Ok(()) => Ok(Some(ServerMessage::CopyModeExited)),
                            Err(e) => Ok(Some(ServerMessage::Error {
                                message: format!("Failed to exit copy mode: {}", e),
                            }))
                        }
                    } else {
                        Ok(Some(ServerMessage::Error {
                            message: "Session not found".to_string(),
                        }))
                    }
                } else {
                    Ok(Some(ServerMessage::Error {
                        message: "Not attached to a session".to_string(),
                    }))
                }
            } else {
                Ok(Some(ServerMessage::Error {
                    message: "Client not found".to_string(),
                }))
            }
        }

        ClientMessage::CopyModeInput { key } => {
            if let Some(client) = clients.read().await.get(client_id) {
                if let Some(session_id) = &client.attached_session {
                    let mut sessions_guard = sessions.write().await;
                    if let Some(session) = sessions_guard.get_mut(session_id) {
                        let mut session_guard = session.write().await;
                        match session_guard.handle_copy_mode_input(key).await {
                            Ok(Some(copy_mode_state)) => {
                                Ok(Some(ServerMessage::CopyModeUpdate {
                                    cursor_row: copy_mode_state.cursor_row,
                                    cursor_col: copy_mode_state.cursor_col,
                                    selection_start: copy_mode_state.selection_start,
                                    selection_end: copy_mode_state.selection_end,
                                    buffer_content: copy_mode_state.buffer_content,
                                    mode: copy_mode_state.mode,
                                }))
                            }
                            Ok(None) => Ok(None), // No update needed
                            Err(e) => Ok(Some(ServerMessage::Error {
                                message: format!("Failed to handle copy mode input: {}", e),
                            }))
                        }
                    } else {
                        Ok(Some(ServerMessage::Error {
                            message: "Session not found".to_string(),
                        }))
                    }
                } else {
                    Ok(Some(ServerMessage::Error {
                        message: "Not attached to a session".to_string(),
                    }))
                }
            } else {
                Ok(Some(ServerMessage::Error {
                    message: "Client not found".to_string(),
                }))
            }
        }

        ClientMessage::TogglePaneSync => {
            if let Some(client) = clients.read().await.get(client_id) {
                if let Some(session_id) = &client.attached_session {
                    let mut sessions_guard = sessions.write().await;
                    if let Some(session) = sessions_guard.get_mut(session_id) {
                        let mut session_guard = session.write().await;
                        let enabled = session_guard.toggle_pane_sync();
                        Ok(Some(ServerMessage::PaneSyncStatusUpdate { enabled }))
                    } else {
                        Ok(Some(ServerMessage::Error {
                            message: "Session not found".to_string(),
                        }))
                    }
                } else {
                    Ok(Some(ServerMessage::Error {
                        message: "Not attached to a session".to_string(),
                    }))
                }
            } else {
                Ok(Some(ServerMessage::Error {
                    message: "Client not found".to_string(),
                }))
            }
        }

        ClientMessage::SetPaneSync { enabled } => {
            if let Some(client) = clients.read().await.get(client_id) {
                if let Some(session_id) = &client.attached_session {
                    let mut sessions_guard = sessions.write().await;
                    if let Some(session) = sessions_guard.get_mut(session_id) {
                        let mut session_guard = session.write().await;
                        let actual_enabled = session_guard.set_pane_sync(enabled);
                        Ok(Some(ServerMessage::PaneSyncStatusUpdate { enabled: actual_enabled }))
                    } else {
                        Ok(Some(ServerMessage::Error {
                            message: "Session not found".to_string(),
                        }))
                    }
                } else {
                    Ok(Some(ServerMessage::Error {
                        message: "Not attached to a session".to_string(),
                    }))
                }
            } else {
                Ok(Some(ServerMessage::Error {
                    message: "Client not found".to_string(),
                }))
            }
        }

        ClientMessage::LockSession => {
            if let Some(client) = clients.read().await.get(client_id) {
                if let Some(session_id) = &client.attached_session {
                    let mut sessions_guard = sessions.write().await;
                    if let Some(session) = sessions_guard.get_mut(session_id) {
                        let mut session_guard = session.write().await;
                        let locked = session_guard.lock_session();
                        Ok(Some(ServerMessage::SessionLockStatusUpdate { locked }))
                    } else {
                        Ok(Some(ServerMessage::Error {
                            message: "Session not found".to_string(),
                        }))
                    }
                } else {
                    Ok(Some(ServerMessage::Error {
                        message: "Not attached to a session".to_string(),
                    }))
                }
            } else {
                Ok(Some(ServerMessage::Error {
                    message: "Client not found".to_string(),
                }))
            }
        }

        ClientMessage::UnlockSession => {
            if let Some(client) = clients.read().await.get(client_id) {
                if let Some(session_id) = &client.attached_session {
                    let mut sessions_guard = sessions.write().await;
                    if let Some(session) = sessions_guard.get_mut(session_id) {
                        let mut session_guard = session.write().await;
                        let locked = session_guard.unlock_session();
                        Ok(Some(ServerMessage::SessionLockStatusUpdate { locked }))
                    } else {
                        Ok(Some(ServerMessage::Error {
                            message: "Session not found".to_string(),
                        }))
                    }
                } else {
                    Ok(Some(ServerMessage::Error {
                        message: "Not attached to a session".to_string(),
                    }))
                }
            } else {
                Ok(Some(ServerMessage::Error {
                    message: "Client not found".to_string(),
                }))
            }
        }

        ClientMessage::SetSessionLock { locked } => {
            if let Some(client) = clients.read().await.get(client_id) {
                if let Some(session_id) = &client.attached_session {
                    let mut sessions_guard = sessions.write().await;
                    if let Some(session) = sessions_guard.get_mut(session_id) {
                        let mut session_guard = session.write().await;
                        let actual_locked = session_guard.set_session_lock(locked);
                        Ok(Some(ServerMessage::SessionLockStatusUpdate { locked: actual_locked }))
                    } else {
                        Ok(Some(ServerMessage::Error {
                            message: "Session not found".to_string(),
                        }))
                    }
                } else {
                    Ok(Some(ServerMessage::Error {
                        message: "Not attached to a session".to_string(),
                    }))
                }
            } else {
                Ok(Some(ServerMessage::Error {
                    message: "Client not found".to_string(),
                }))
            }
        }

        ClientMessage::RenameWindow { window_id, new_name } => {
            if let Some(client) = clients.read().await.get(client_id) {
                if let Some(session_id) = &client.attached_session {
                    let mut sessions_guard = sessions.write().await;
                    if let Some(session) = sessions_guard.get_mut(session_id) {
                        let mut session_guard = session.write().await;
                        match session_guard.rename_window(window_id, new_name.clone()).await {
                            Ok(renamed_window_id) => {
                                Ok(Some(ServerMessage::WindowRenamed {
                                    window_id: renamed_window_id,
                                    new_name,
                                }))
                            }
                            Err(e) => Ok(Some(ServerMessage::Error {
                                message: format!("Failed to rename window: {}", e),
                            }))
                        }
                    } else {
                        Ok(Some(ServerMessage::Error {
                            message: "Session not found".to_string(),
                        }))
                    }
                } else {
                    Ok(Some(ServerMessage::Error {
                        message: "Not attached to a session".to_string(),
                    }))
                }
            } else {
                Ok(Some(ServerMessage::Error {
                    message: "Client not found".to_string(),
                }))
            }
        }

        ClientMessage::ToggleActivityMonitoring { pane_id } => {
            if let Some(client) = clients.read().await.get(client_id) {
                if let Some(session_id) = &client.attached_session {
                    let mut sessions_guard = sessions.write().await;
                    if let Some(session) = sessions_guard.get_mut(session_id) {
                        let mut session_guard = session.write().await;
                        match session_guard.toggle_activity_monitoring(pane_id).await {
                            Ok((target_pane, enabled)) => {
                                let activity_status = session_guard.get_activity_status(&target_pane).await;
                                Ok(Some(ServerMessage::ActivityStatusUpdate {
                                    pane_id: target_pane,
                                    activity_status,
                                    enabled,
                                }))
                            }
                            Err(e) => Ok(Some(ServerMessage::Error {
                                message: format!("Failed to toggle activity monitoring: {}", e),
                            }))
                        }
                    } else {
                        Ok(Some(ServerMessage::Error {
                            message: "Session not found".to_string(),
                        }))
                    }
                } else {
                    Ok(Some(ServerMessage::Error {
                        message: "Not attached to a session".to_string(),
                    }))
                }
            } else {
                Ok(Some(ServerMessage::Error {
                    message: "Client not found".to_string(),
                }))
            }
        }

        ClientMessage::SetActivityMonitoring { pane_id, enabled } => {
            if let Some(client) = clients.read().await.get(client_id) {
                if let Some(session_id) = &client.attached_session {
                    let mut sessions_guard = sessions.write().await;
                    if let Some(session) = sessions_guard.get_mut(session_id) {
                        let mut session_guard = session.write().await;
                        match session_guard.set_activity_monitoring(pane_id, enabled).await {
                            Ok((target_pane, actual_enabled)) => {
                                let activity_status = session_guard.get_activity_status(&target_pane).await;
                                Ok(Some(ServerMessage::ActivityStatusUpdate {
                                    pane_id: target_pane,
                                    activity_status,
                                    enabled: actual_enabled,
                                }))
                            }
                            Err(e) => Ok(Some(ServerMessage::Error {
                                message: format!("Failed to set activity monitoring: {}", e),
                            }))
                        }
                    } else {
                        Ok(Some(ServerMessage::Error {
                            message: "Session not found".to_string(),
                        }))
                    }
                } else {
                    Ok(Some(ServerMessage::Error {
                        message: "Not attached to a session".to_string(),
                    }))
                }
            } else {
                Ok(Some(ServerMessage::Error {
                    message: "Client not found".to_string(),
                }))
            }
        }

        ClientMessage::ListKeys => {
            use crate::protocol::KeyBindingInfo;

            let manager = keybinding_manager.read().await;
            let all_bindings = manager.list_all_bindings();

            let bindings: Vec<KeyBindingInfo> = all_bindings.iter().map(|(key, action, is_custom)| {
                KeyBindingInfo {
                    key: key.clone(),
                    action: action.clone(),
                    description: String::new(),  // Could add descriptions later
                    is_custom: *is_custom,
                }
            }).collect();

            Ok(Some(ServerMessage::KeyList { bindings }))
        }

        ClientMessage::BindKey { key, action } => {
            use crate::config::keybindings::KeyBindingManager;

            match KeyBindingManager::parse_key_string(&key) {
                Ok(key_binding) => {
                    let mut manager = keybinding_manager.write().await;
                    let action_enum = manager.parse_action_string(&action)
                        .unwrap_or(crate::config::keybindings::Action::Custom(action.clone()));

                    manager.bind_custom(key_binding, action_enum);

                    // Try to save to config
                    let _ = manager.save_to_config();

                    Ok(Some(ServerMessage::KeyBound { key, action }))
                }
                Err(_) => {
                    Ok(Some(ServerMessage::Error {
                        message: format!("Invalid key format: {}", key),
                    }))
                }
            }
        }

        ClientMessage::UnbindKey { key } => {
            use crate::config::keybindings::KeyBindingManager;

            match KeyBindingManager::parse_key_string(&key) {
                Ok(key_binding) => {
                    let mut manager = keybinding_manager.write().await;
                    manager.unbind(&key_binding);

                    // Try to save to config
                    let _ = manager.save_to_config();

                    Ok(Some(ServerMessage::KeyUnbound { key }))
                }
                Err(_) => {
                    Ok(Some(ServerMessage::Error {
                        message: format!("Invalid key format: {}", key),
                    }))
                }
            }
        }

        ClientMessage::ResetKeys => {
            let mut manager = keybinding_manager.write().await;
            manager.reset_to_defaults();

            // Try to save to config
            let _ = manager.save_to_config();

            Ok(Some(ServerMessage::KeysReset))
        }

        ClientMessage::ReloadKeys => {
            let mut manager = keybinding_manager.write().await;
            match manager.reload_config() {
                Ok(_) => Ok(Some(ServerMessage::KeysReloaded)),
                Err(e) => Ok(Some(ServerMessage::Error {
                    message: format!("Failed to reload keybindings: {}", e),
                }))
            }
        }

        ClientMessage::ExportKeys { path } => {
            let manager = keybinding_manager.read().await;

            match manager.export_to_file(&path) {
                Ok(_) => Ok(Some(ServerMessage::KeysExported { path })),
                Err(e) => Ok(Some(ServerMessage::Error {
                    message: format!("Failed to export keybindings: {}", e),
                }))
            }
        }

        ClientMessage::ImportKeys { path } => {
            let mut manager = keybinding_manager.write().await;
            match manager.import_from_file(&path) {
                Ok(count) => Ok(Some(ServerMessage::KeysImported { count })),
                Err(e) => Ok(Some(ServerMessage::Error {
                    message: format!("Failed to import keybindings: {}", e),
                }))
            }
        }

        ClientMessage::EnableAutoSave { session_id, interval_minutes } => {
            let target_session_id = if let Some(sid) = session_id {
                Some(sid)
            } else {
                clients.read().await.get(client_id)
                    .and_then(|c| c.attached_session.clone())
            };

            if let Some(sid) = target_session_id {
                let mut sessions_guard = sessions.write().await;
                if let Some(session) = sessions_guard.get_mut(&sid) {
                    let mut session_guard = session.write().await;
                    let interval = interval_minutes.unwrap_or(5);
                    session_guard.auto_save_enabled = true;
                    session_guard.auto_save_interval = std::time::Duration::from_secs(interval * 60);

                    Ok(Some(ServerMessage::AutoSaveEnabled { interval_minutes: interval }))
                } else {
                    Ok(Some(ServerMessage::Error {
                        message: "Session not found".to_string(),
                    }))
                }
            } else {
                Ok(Some(ServerMessage::Error {
                    message: "No session specified".to_string(),
                }))
            }
        }

        ClientMessage::DisableAutoSave { session_id } => {
            let target_session_id = if let Some(sid) = session_id {
                Some(sid)
            } else {
                clients.read().await.get(client_id)
                    .and_then(|c| c.attached_session.clone())
            };

            if let Some(sid) = target_session_id {
                let mut sessions_guard = sessions.write().await;
                if let Some(session) = sessions_guard.get_mut(&sid) {
                    let mut session_guard = session.write().await;
                    session_guard.auto_save_enabled = false;

                    Ok(Some(ServerMessage::AutoSaveDisabled))
                } else {
                    Ok(Some(ServerMessage::Error {
                        message: "Session not found".to_string(),
                    }))
                }
            } else {
                Ok(Some(ServerMessage::Error {
                    message: "No session specified".to_string(),
                }))
            }
        }

        ClientMessage::AutoSaveStatus { session_id } => {
            let target_session_id = if let Some(sid) = session_id {
                Some(sid)
            } else {
                clients.read().await.get(client_id)
                    .and_then(|c| c.attached_session.clone())
            };

            if let Some(sid) = target_session_id {
                let sessions_guard = sessions.read().await;
                if let Some(session) = sessions_guard.get(&sid) {
                    let session_guard = session.read().await;
                    let interval_minutes = session_guard.auto_save_interval.as_secs() / 60;
                    let next_save = session_guard.last_auto_save.and_then(|last| {
                        chrono::Duration::from_std(session_guard.auto_save_interval)
                            .ok()
                            .map(|duration| last + duration)
                    });

                    Ok(Some(ServerMessage::AutoSaveStatusInfo {
                        enabled: session_guard.auto_save_enabled,
                        interval_minutes,
                        last_save: session_guard.last_auto_save,
                        next_save,
                    }))
                } else {
                    Ok(Some(ServerMessage::Error {
                        message: "Session not found".to_string(),
                    }))
                }
            } else {
                Ok(Some(ServerMessage::Error {
                    message: "No session specified".to_string(),
                }))
            }
        }

        ClientMessage::StartRecording { session_id, output_path } => {
            let sid = session_id.or_else(|| {
                let clients_guard = futures::executor::block_on(clients.read());
                clients_guard.get(client_id).and_then(|c| c.attached_session.clone())
            });

            if let Some(sid) = sid {
                let sessions_guard = sessions.read().await;
                if let Some(session_arc) = sessions_guard.get(&sid) {
                    let mut session_guard = session_arc.write().await;

                    let path = output_path.unwrap_or_else(|| {
                        let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
                        let recordings_dir = dirs::home_dir()
                            .map(|h| h.join(".ferrix/recordings"))
                            .unwrap_or_else(|| PathBuf::from("./recordings"));
                        std::fs::create_dir_all(&recordings_dir).ok();
                        recordings_dir.join(format!("session_{}_{}.ferrix-rec", sid.0, timestamp))
                    });

                    match session_guard.start_recording(path.clone()).await {
                        Ok(_) => {
                            info!("Started recording session {} to {:?}", sid.0, path);
                            Ok(Some(ServerMessage::RecordingStarted {
                                session_id: sid,
                                output_path: path
                            }))
                        }
                        Err(e) => {
                            Ok(Some(ServerMessage::Error {
                                message: format!("Failed to start recording: {}", e),
                            }))
                        }
                    }
                } else {
                    Ok(Some(ServerMessage::Error {
                        message: format!("Session not found: {:?}", sid),
                    }))
                }
            } else {
                Ok(Some(ServerMessage::Error {
                    message: "No session specified or attached".to_string(),
                }))
            }
        }

        ClientMessage::StopRecording { session_id } => {
            let sid = session_id.or_else(|| {
                let clients_guard = futures::executor::block_on(clients.read());
                clients_guard.get(client_id).and_then(|c| c.attached_session.clone())
            });

            if let Some(sid) = sid {
                let sessions_guard = sessions.read().await;
                if let Some(session_arc) = sessions_guard.get(&sid) {
                    let mut session_guard = session_arc.write().await;

                    match session_guard.stop_recording().await {
                        Ok((duration_secs, file_size)) => {
                            info!("Stopped recording session {} (duration: {}s, size: {} bytes)",
                                  sid.0, duration_secs, file_size);
                            Ok(Some(ServerMessage::RecordingStopped {
                                session_id: sid,
                                duration_secs,
                                file_size
                            }))
                        }
                        Err(e) => {
                            Ok(Some(ServerMessage::Error {
                                message: format!("Failed to stop recording: {}", e),
                            }))
                        }
                    }
                } else {
                    Ok(Some(ServerMessage::Error {
                        message: format!("Session not found: {:?}", sid),
                    }))
                }
            } else {
                Ok(Some(ServerMessage::Error {
                    message: "No session specified or attached".to_string(),
                }))
            }
        }

        ClientMessage::PauseRecording { session_id } => {
            let sid = session_id.or_else(|| {
                let clients_guard = futures::executor::block_on(clients.read());
                clients_guard.get(client_id).and_then(|c| c.attached_session.clone())
            });

            if let Some(sid) = sid {
                let sessions_guard = sessions.read().await;
                if let Some(session_arc) = sessions_guard.get(&sid) {
                    let mut session_guard = session_arc.write().await;

                    match session_guard.pause_recording().await {
                        Ok(_) => {
                            info!("Paused recording for session {}", sid.0);
                            Ok(Some(ServerMessage::RecordingPaused { session_id: sid }))
                        }
                        Err(e) => {
                            Ok(Some(ServerMessage::Error {
                                message: format!("Failed to pause recording: {}", e),
                            }))
                        }
                    }
                } else {
                    Ok(Some(ServerMessage::Error {
                        message: format!("Session not found: {:?}", sid),
                    }))
                }
            } else {
                Ok(Some(ServerMessage::Error {
                    message: "No session specified or attached".to_string(),
                }))
            }
        }

        ClientMessage::ResumeRecording { session_id } => {
            let sid = session_id.or_else(|| {
                let clients_guard = futures::executor::block_on(clients.read());
                clients_guard.get(client_id).and_then(|c| c.attached_session.clone())
            });

            if let Some(sid) = sid {
                let sessions_guard = sessions.read().await;
                if let Some(session_arc) = sessions_guard.get(&sid) {
                    let mut session_guard = session_arc.write().await;

                    match session_guard.resume_recording().await {
                        Ok(_) => {
                            info!("Resumed recording for session {}", sid.0);
                            Ok(Some(ServerMessage::RecordingResumed { session_id: sid }))
                        }
                        Err(e) => {
                            Ok(Some(ServerMessage::Error {
                                message: format!("Failed to resume recording: {}", e),
                            }))
                        }
                    }
                } else {
                    Ok(Some(ServerMessage::Error {
                        message: format!("Session not found: {:?}", sid),
                    }))
                }
            } else {
                Ok(Some(ServerMessage::Error {
                    message: "No session specified or attached".to_string(),
                }))
            }
        }

        ClientMessage::RecordingStatus { session_id } => {
            let sid = session_id.or_else(|| {
                let clients_guard = futures::executor::block_on(clients.read());
                clients_guard.get(client_id).and_then(|c| c.attached_session.clone())
            });

            if let Some(sid) = sid {
                let sessions_guard = sessions.read().await;
                if let Some(session_arc) = sessions_guard.get(&sid) {
                    let session_guard = session_arc.read().await;

                    let status = session_guard.get_recording_status().await;
                    Ok(Some(ServerMessage::RecordingStatus {
                        session_id: sid,
                        is_recording: status.is_recording,
                        is_paused: status.is_paused,
                        output_path: status.output_path,
                        duration_secs: status.duration_secs,
                        event_count: status.event_count,
                    }))
                } else {
                    Ok(Some(ServerMessage::Error {
                        message: format!("Session not found: {:?}", sid),
                    }))
                }
            } else {
                Ok(Some(ServerMessage::Error {
                    message: "No session specified or attached".to_string(),
                }))
            }
        }

        ClientMessage::PlayRecording { path, speed } => {
            use crate::server::recording::SessionPlayer;

            let speed = speed.unwrap_or(1.0);
            let clients_clone = clients.clone();
            let client_id_clone = *client_id;

            tokio::spawn(async move {
                match SessionPlayer::load(&path) {
                    Ok(mut player) => {
                        info!("Starting playback of recording: {:?}", path);

                        // Notify client playback has started
                        let clients_guard = clients_clone.read().await;
                        if let Some(client) = clients_guard.get(&client_id_clone) {
                            let _ = client.sender.send(ServerMessage::RecordingPlaybackStarted {
                                path: path.clone()
                            }).await;
                        }
                        drop(clients_guard);

                        // Play the recording
                        if let Err(e) = player.play(speed, |event| {
                            let clients_clone2 = clients_clone.clone();
                            let client_id_clone2 = client_id_clone;

                            Box::pin(async move {
                                use crate::server::recording::RecordingEvent;

                                let clients_guard = clients_clone2.read().await;
                                if let Some(client) = clients_guard.get(&client_id_clone2) {
                                    match event {
                                        RecordingEvent::Output { data, .. } => {
                                            let _ = client.sender.send(ServerMessage::Output { data }).await;
                                        }
                                        RecordingEvent::Input { .. } => {
                                            // Optionally show input events during playback
                                        }
                                        RecordingEvent::Resize { .. } => {
                                            // Handle resize events during playback
                                            // This would need a new server message type
                                        }
                                        _ => {}
                                    }
                                }
                            })
                        }).await {
                            error!("Playback error: {}", e);
                        }

                        // Notify client playback has finished
                        let clients_guard = clients_clone.read().await;
                        if let Some(client) = clients_guard.get(&client_id_clone) {
                            let _ = client.sender.send(ServerMessage::RecordingPlaybackFinished).await;
                        }
                    }
                    Err(e) => {
                        error!("Failed to load recording: {}", e);
                        let clients_guard = clients_clone.read().await;
                        if let Some(client) = clients_guard.get(&client_id_clone) {
                            let _ = client.sender.send(ServerMessage::Error {
                                message: format!("Failed to load recording: {}", e),
                            }).await;
                        }
                    }
                }
            });

            Ok(None) // Response is sent asynchronously
        }

        ClientMessage::ExportRecording { path, format, output_path } => {
            use crate::server::recording::SessionPlayer;

            match SessionPlayer::load(&path) {
                Ok(player) => {
                    let export_result = match format {
                        crate::protocol::RecordingExportFormat::Asciinema => {
                            player.export_asciinema(&output_path)
                        }
                        crate::protocol::RecordingExportFormat::Text => {
                            player.export_text(&output_path)
                        }
                        crate::protocol::RecordingExportFormat::Html => {
                            player.export_html(&output_path)
                        }
                    };

                    match export_result {
                        Ok(_) => {
                            info!("Exported recording to {:?} as {:?}", output_path, format);
                            Ok(Some(ServerMessage::RecordingExported {
                                input_path: path,
                                output_path,
                                format,
                            }))
                        }
                        Err(e) => {
                            Ok(Some(ServerMessage::Error {
                                message: format!("Failed to export recording: {}", e),
                            }))
                        }
                    }
                }
                Err(e) => {
                    Ok(Some(ServerMessage::Error {
                        message: format!("Failed to load recording: {}", e),
                    }))
                }
            }
        }

        // Session versioning commands
        #[cfg(feature = "versioning")]
        ClientMessage::InitVersioning { session_id } => {
            let sessions_guard = sessions.read().await;
            if let Some(session) = sessions_guard.get(&session_id) {
                let mut session_guard = session.write().await;
                match session_guard.init_versioning().await {
                    Ok(()) => {
                        Ok(Some(ServerMessage::Success))
                    }
                    Err(e) => Ok(Some(ServerMessage::Error {
                        message: format!("Failed to initialize versioning: {}", e),
                    }))
                }
            } else {
                Ok(Some(ServerMessage::Error {
                    message: "Session not found".to_string(),
                }))
            }
        }

        #[cfg(feature = "versioning")]
        ClientMessage::CommitSession { session_id, message } => {
            let sessions_guard = sessions.read().await;
            if let Some(session) = sessions_guard.get(&session_id) {
                let mut session_guard = session.write().await;
                let author_name = "User".to_string(); // TODO: Get from session or client
                match session_guard.commit_changes(&message, &author_name).await {
                    Ok(commit_id) => {
                        Ok(Some(ServerMessage::CommitCreated {
                            session_id: session_id.clone(),
                            commit_id: commit_id.0,
                            message: message.clone(),
                        }))
                    }
                    Err(e) => Ok(Some(ServerMessage::Error {
                        message: format!("Failed to commit: {}", e),
                    }))
                }
            } else {
                Ok(Some(ServerMessage::Error {
                    message: "Session not found".to_string(),
                }))
            }
        }

        #[cfg(feature = "versioning")]
        ClientMessage::CreateBranch { session_id, branch_name, description: _ } => {
            let sessions_guard = sessions.read().await;
            if let Some(session) = sessions_guard.get(&session_id) {
                let mut session_guard = session.write().await;
                match session_guard.create_branch(&branch_name, None).await {
                    Ok(()) => {
                        Ok(Some(ServerMessage::BranchCreated {
                            session_id: session_id.clone(),
                            branch_name
                        }))
                    }
                    Err(e) => Ok(Some(ServerMessage::Error {
                        message: format!("Failed to create branch: {}", e),
                    }))
                }
            } else {
                Ok(Some(ServerMessage::Error {
                    message: "Session not found".to_string(),
                }))
            }
        }

        #[cfg(feature = "versioning")]
        ClientMessage::CheckoutBranch { session_id, branch_name } => {
            let sessions_guard = sessions.read().await;
            if let Some(session) = sessions_guard.get(&session_id) {
                let mut session_guard = session.write().await;
                match session_guard.checkout_branch(&branch_name).await {
                    Ok(()) => {
                        Ok(Some(ServerMessage::BranchCheckedOut {
                            session_id: session_id.clone(),
                            branch_name
                        }))
                    }
                    Err(e) => Ok(Some(ServerMessage::Error {
                        message: format!("Failed to checkout branch: {}", e),
                    }))
                }
            } else {
                Ok(Some(ServerMessage::Error {
                    message: "Session not found".to_string(),
                }))
            }
        }

        #[cfg(feature = "versioning")]
        ClientMessage::MergeBranch { session_id, branch_name, strategy } => {
            let sessions_guard = sessions.read().await;
            if let Some(session) = sessions_guard.get(&session_id) {
                let mut session_guard = session.write().await;
                let auto_resolve = strategy == "auto";
                match session_guard.merge_branch(&branch_name, auto_resolve).await {
                    Ok((conflicts, _resolved)) => {
                        Ok(Some(ServerMessage::MergeCompleted {
                            session_id: session_id.clone(),
                            branch_name: branch_name.clone(),
                            conflicts,
                        }))
                    }
                    Err(e) => Ok(Some(ServerMessage::Error {
                        message: format!("Failed to merge: {}", e),
                    }))
                }
            } else {
                Ok(Some(ServerMessage::Error {
                    message: "Session not found".to_string(),
                }))
            }
        }

        #[cfg(feature = "versioning")]
        ClientMessage::ListBranches { session_id } => {
            let sessions_guard = sessions.read().await;
            if let Some(session) = sessions_guard.get(&session_id) {
                let session_guard = session.read().await;
                let branches = session_guard.list_branches();
                let branch_infos: Vec<crate::protocol::BranchInfo> = branches.into_iter().map(|b| {
                    crate::protocol::BranchInfo {
                        name: b.name.clone(),
                        head: b.head.0,
                        description: b.description,
                        created_at: b.created_at,
                        is_current: session_guard.get_current_branch() == Some(&b.name),
                    }
                }).collect();
                Ok(Some(ServerMessage::BranchList {
                    session_id: session_id.clone(),
                    branches: branch_infos,
                    current: session_guard.get_current_branch().unwrap_or("master").to_string(),
                }))
            } else {
                Ok(Some(ServerMessage::Error {
                    message: "Session not found".to_string(),
                }))
            }
        }

        #[cfg(feature = "versioning")]
        ClientMessage::ShowLog { session_id, limit } => {
            let sessions_guard = sessions.read().await;
            if let Some(session) = sessions_guard.get(&session_id) {
                let session_guard = session.read().await;
                let log_entries = session_guard.get_commit_log(limit.unwrap_or(10));
                let commit_infos: Vec<crate::protocol::CommitInfo> = log_entries.into_iter().map(|c| {
                    crate::protocol::CommitInfo {
                        id: c.id.0,
                        message: c.message,
                        author: c.author,
                        timestamp: c.timestamp,
                        parent: c.parent.map(|p| p.0),
                        tags: c.tags,
                    }
                }).collect();
                Ok(Some(ServerMessage::LogHistory {
                    session_id: session_id.clone(),
                    commits: commit_infos,
                }))
            } else {
                Ok(Some(ServerMessage::Error {
                    message: "Session not found".to_string(),
                }))
            }
        }

        #[cfg(feature = "versioning")]
        ClientMessage::ShowDiff { session_id, from_commit, to_commit } => {
            let sessions_guard = sessions.read().await;
            if let Some(session) = sessions_guard.get(&session_id) {
                let session_guard = session.read().await;
                match session_guard.diff_commits(Some(&from_commit), Some(&to_commit)) {
                    Ok(diff) => {
                        Ok(Some(ServerMessage::DiffResult {
                            session_id: session_id.clone(),
                            diff
                        }))
                    }
                    Err(e) => Ok(Some(ServerMessage::Error {
                        message: format!("Failed to generate diff: {}", e),
                    }))
                }
            } else {
                Ok(Some(ServerMessage::Error {
                    message: "Session not found".to_string(),
                }))
            }
        }

        ClientMessage::PtyResponse { pane_id, data } => {
            // Find the session that has this pane and write the response to it
            let sessions_guard = sessions.read().await;
            for session in sessions_guard.values() {
                let session_guard = session.read().await;
                // Search all windows in the session
                for window in &session_guard.windows {
                    let mut window_guard = window.write().await;
                    if let Some(pane) = window_guard.panes.get_mut(&pane_id) {
                        // Write the response data to the pane's PTY
                        let mut pane_guard = pane.write().await;
                        if let Some(pty) = &mut pane_guard.pty {
                            let _ = pty.write(data).await;
                        }
                        return Ok(None);
                    }
                }
            }
            Ok(None)
        }

        _ => {
            warn!("Unhandled message type: {:?}", message);
            Ok(Some(ServerMessage::Error {
                message: "Unimplemented feature".to_string(),
            }))
        }
    }
}