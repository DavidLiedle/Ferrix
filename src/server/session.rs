use std::sync::Arc;
use tokio::sync::RwLock;
use chrono::Utc;
use uuid::Uuid;

use crate::error::{Result, FerrixError};
use crate::protocol::{SessionId, WindowId, PaneId, SplitDirection};
use super::window::Window;
use super::snapshot::{SessionSnapshot, SnapshotMetadata, SessionState, WindowState, PaneState};
use super::layout::NavigationDirection;

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

    pub async fn create_window(&mut self, name: Option<String>) -> Result<WindowId> {
        let window_id = WindowId(Uuid::new_v4());
        let window_name = name.unwrap_or_else(|| format!("window-{}", self.windows.len()));
        let new_window = Window::new(window_id.clone(), window_name);

        self.windows.push(Arc::new(RwLock::new(new_window)));
        self.current_window = Some(window_id.clone());

        Ok(window_id)
    }

    pub async fn switch_window(&mut self, window_id: WindowId) -> Result<()> {
        for window in &self.windows {
            let window_guard = window.read().await;
            if window_guard.id == window_id {
                self.current_window = Some(window_id);
                return Ok(());
            }
        }
        Err(FerrixError::WindowNotFound(format!("{:?}", window_id)))
    }

    pub async fn close_window(&mut self, window_id: WindowId) -> Result<()> {
        if self.windows.len() <= 1 {
            return Err(FerrixError::Other("Cannot close last window".to_string()));
        }

        let mut window_index = None;
        for (i, window) in self.windows.iter().enumerate() {
            let window_guard = window.read().await;
            if window_guard.id == window_id {
                window_index = Some(i);
                break;
            }
        }

        if let Some(index) = window_index {
            self.windows.remove(index);

            // Update current window if needed
            if self.current_window == Some(window_id) {
                if let Some(first_window) = self.windows.first() {
                    let window_guard = first_window.read().await;
                    self.current_window = Some(window_guard.id.clone());
                }
            }
            Ok(())
        } else {
            Err(FerrixError::WindowNotFound(format!("{:?}", window_id)))
        }
    }

    pub async fn next_window(&mut self) -> Result<()> {
        if self.windows.len() <= 1 {
            return Ok(());
        }

        if let Some(current_id) = &self.current_window {
            for (i, window) in self.windows.iter().enumerate() {
                let window_guard = window.read().await;
                if window_guard.id == *current_id {
                    let next_index = (i + 1) % self.windows.len();
                    let next_window = &self.windows[next_index];
                    let next_guard = next_window.read().await;
                    self.current_window = Some(next_guard.id.clone());
                    return Ok(());
                }
            }
        }
        Ok(())
    }

    pub async fn previous_window(&mut self) -> Result<()> {
        if self.windows.len() <= 1 {
            return Ok(());
        }

        if let Some(current_id) = &self.current_window {
            for (i, window) in self.windows.iter().enumerate() {
                let window_guard = window.read().await;
                if window_guard.id == *current_id {
                    let prev_index = if i == 0 { self.windows.len() - 1 } else { i - 1 };
                    let prev_window = &self.windows[prev_index];
                    let prev_guard = prev_window.read().await;
                    self.current_window = Some(prev_guard.id.clone());
                    return Ok(());
                }
            }
        }
        Ok(())
    }

    pub async fn split_pane(&mut self, direction: SplitDirection) -> Result<PaneId> {
        if let Some(current_window_id) = &self.current_window {
            for window in &self.windows {
                let window_guard = window.read().await;
                if window_guard.id == *current_window_id {
                    let current_pane = window_guard.current_pane.clone();
                    drop(window_guard);

                    if let Some(pane_id) = current_pane {
                        let mut window_guard = window.write().await;
                        return window_guard.split_pane(&pane_id, direction).await;
                    }
                }
            }
        }
        Err(FerrixError::Other("No current window".to_string()))
    }

    pub async fn navigate_pane(&mut self, direction: NavigationDirection) -> Result<()> {
        if let Some(current_window_id) = &self.current_window {
            for window in &self.windows {
                let window_guard = window.read().await;
                if window_guard.id == *current_window_id {
                    drop(window_guard);
                    let mut window_guard = window.write().await;
                    return window_guard.navigate_pane(direction).await;
                }
            }
        }
        Err(FerrixError::Other("No current window".to_string()))
    }

    pub async fn close_pane(&mut self, pane_id: PaneId) -> Result<()> {
        if let Some(current_window_id) = &self.current_window {
            for window in &self.windows {
                let window_guard = window.read().await;
                if window_guard.id == *current_window_id {
                    drop(window_guard);
                    let mut window_guard = window.write().await;
                    return window_guard.close_pane(&pane_id).await;
                }
            }
        }
        Err(FerrixError::Other("No current window".to_string()))
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