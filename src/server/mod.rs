pub mod session;
pub mod window;
pub mod pane;
pub mod pty;
pub mod layout;
pub mod snapshot;
pub mod recovery;
pub mod collaboration;
pub mod timetravel;
pub mod remote;
pub mod versioning;
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

#[derive(Clone)]
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

        // Check for crash recovery
        let recovery_manager = Arc::new(RecoveryManager::new()?);

        // Attempt to recover crashed sessions
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

        // Setup signal handlers for graceful shutdown
        recovery::setup_signal_handlers(recovery_manager.clone());

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
                        if let Ok(pane_outputs) = session_guard.get_all_pane_outputs().await {
                            for (pane_id, output) in pane_outputs {
                                if !output.is_empty() {
                                    let clients_guard = clients_clone.read().await;
                                    if let Some(client) = clients_guard.get(&client_id_clone) {
                                        if client.attached_session == Some(session_id_clone.clone()) {
                                            let _ = client.sender.send(ServerMessage::PaneOutput {
                                                pane_id,
                                                data: output
                                            }).await;
                                        } else {
                                            return; // Exit if client detached
                                        }
                                    } else {
                                        return; // Exit if client not found
                                    }
                                }
                            }
                        }
                    }
                });

                info!("Client {} attached to session {}", client_id.0, session_id.0);

                // Send initial layout info
                let session_guard = session.read().await;
                if let Some(_layout) = session_guard.get_layout_info().await {
                    drop(session_guard);
                    // For now, we'll send the SessionAttached message first
                    // In a full implementation, we'd want to batch these or use a different approach
                    Ok(Some(ServerMessage::SessionAttached { session_id }))
                } else {
                    Ok(Some(ServerMessage::SessionAttached { session_id }))
                }
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
            if let Some(client) = clients.read().await.get(&client_id) {
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
            if let Some(client) = clients.read().await.get(&client_id) {
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
            if let Some(client) = clients.read().await.get(&client_id) {
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
            if let Some(client) = clients.read().await.get(&client_id) {
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
            if let Some(client) = clients.read().await.get(&client_id) {
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

        ClientMessage::ClosePane { pane_id } => {
            if let Some(client) = clients.read().await.get(&client_id) {
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
            Ok(Some(ServerMessage::Error {
                message: format!("Pane resizing not yet implemented (direction: {:?}, amount: {})", direction, amount),
            }))
        }

        ClientMessage::NextWindow => {
            if let Some(client) = clients.read().await.get(&client_id) {
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
            if let Some(client) = clients.read().await.get(&client_id) {
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
            if let Some(client) = clients.read().await.get(&client_id) {
                if let Some(session_id) = &client.attached_session {
                    let mut sessions_guard = sessions.write().await;
                    if let Some(session) = sessions_guard.get_mut(session_id) {
                        let mut session_guard = session.write().await;
                        match session_guard.zoom_pane().await {
                            Ok(()) => Ok(None),
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
            if let Some(client) = clients.read().await.get(&client_id) {
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
            if let Some(client) = clients.read().await.get(&client_id) {
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

        ClientMessage::EnterCopyMode => {
            if let Some(client) = clients.read().await.get(&client_id) {
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
            if let Some(client) = clients.read().await.get(&client_id) {
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
            if let Some(client) = clients.read().await.get(&client_id) {
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

        _ => {
            warn!("Unhandled message type: {:?}", message);
            Ok(Some(ServerMessage::Error {
                message: "Unimplemented feature".to_string(),
            }))
        }
    }
}