use std::sync::Arc;
use tokio::sync::RwLock;
use chrono::Utc;
use uuid::Uuid;

use crate::error::Result;
use crate::protocol::{SessionId, WindowId, PaneId};
use super::window::Window;
use super::snapshot::{SessionSnapshot, SnapshotMetadata, SessionState, WindowState, PaneState};

pub struct Session {
    pub id: SessionId,
    pub name: String,
    pub windows: Vec<Arc<RwLock<Window>>>,
    pub current_window: Option<WindowId>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl Session {
    pub fn new(id: SessionId, name: String) -> Self {
        let window_id = WindowId(Uuid::new_v4());
        let default_window = Window::new(window_id.clone(), "bash".to_string());

        Self {
            id,
            name,
            windows: vec![Arc::new(RwLock::new(default_window))],
            current_window: Some(window_id),
            created_at: Utc::now(),
        }
    }

    pub async fn handle_input(&mut self, data: Vec<u8>) -> Result<()> {
        if let Some(current_window_id) = &self.current_window {
            for window in &self.windows {
                let window_guard = window.read().await;
                if window_guard.id == *current_window_id {
                    drop(window_guard);
                    let mut window_guard = window.write().await;
                    window_guard.handle_input(data).await?;
                    break;
                }
            }
        }
        Ok(())
    }

    pub async fn resize(&mut self, cols: u16, rows: u16) -> Result<()> {
        if let Some(current_window_id) = &self.current_window {
            for window in &self.windows {
                let window_guard = window.read().await;
                if window_guard.id == *current_window_id {
                    drop(window_guard);
                    let mut window_guard = window.write().await;
                    window_guard.resize(cols, rows).await?;
                    break;
                }
            }
        }
        Ok(())
    }

    pub async fn get_output(&mut self) -> Result<Option<Vec<u8>>> {
        if let Some(current_window_id) = &self.current_window {
            for window in &self.windows {
                let window_guard = window.read().await;
                if window_guard.id == *current_window_id {
                    drop(window_guard);
                    let mut window_guard = window.write().await;
                    return window_guard.get_output().await;
                }
            }
        }
        Ok(None)
    }

    pub fn create_snapshot(&self, name: Option<String>, description: Option<String>) -> SessionSnapshot {
        let metadata = SnapshotMetadata {
            id: Uuid::new_v4(),
            name: name.unwrap_or_else(|| format!("{}_snapshot", self.name)),
            description: description.unwrap_or_else(|| format!("Snapshot of session {}", self.name)),
            created_at: Utc::now(),
            ferrix_version: env!("CARGO_PKG_VERSION").to_string(),
            checksum: None,
        };

        let session_state = SessionState {
            id: self.id.clone(),
            name: self.name.clone(),
            current_window: self.current_window.clone(),
            created_at: self.created_at,
            environment: std::env::vars().collect(),
        };

        // TODO: Properly gather window and pane states from actual windows
        let windows = Vec::new();
        let panes = Vec::new();

        SessionSnapshot {
            metadata,
            session: session_state,
            windows,
            panes,
        }
    }

    pub fn from_snapshot(snapshot: SessionSnapshot) -> Self {
        // TODO: Properly restore windows and panes from snapshot
        let window_id = WindowId(Uuid::new_v4());
        let default_window = Window::new(window_id.clone(), "bash".to_string());

        Self {
            id: snapshot.session.id,
            name: snapshot.session.name,
            windows: vec![Arc::new(RwLock::new(default_window))],
            current_window: snapshot.session.current_window.or(Some(window_id)),
            created_at: snapshot.session.created_at,
        }
    }
}