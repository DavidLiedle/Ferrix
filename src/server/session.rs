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
    pub copy_mode: Option<crate::ui::copymode::CopyMode>,
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
            copy_mode: None,
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

    pub async fn get_all_pane_outputs(&mut self) -> Result<Vec<(PaneId, Vec<u8>)>> {
        let mut outputs = Vec::new();

        if let Some(current_window_id) = &self.current_window {
            for window in &self.windows {
                let window_guard = window.read().await;
                if window_guard.id == *current_window_id {
                    drop(window_guard);
                    let mut window_guard = window.write().await;
                    let pane_outputs = window_guard.get_all_pane_outputs().await?;
                    outputs.extend(pane_outputs);
                    break;
                }
            }
        }

        Ok(outputs)
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

    pub async fn zoom_pane(&mut self) -> Result<()> {
        if let Some(current_window_id) = &self.current_window {
            for window in &self.windows {
                let window_guard = window.read().await;
                if window_guard.id == *current_window_id {
                    drop(window_guard);
                    let mut window_guard = window.write().await;
                    window_guard.toggle_zoom().await;
                    return Ok(());
                }
            }
        }
        Err(FerrixError::Other("No current window".to_string()))
    }

    pub fn list_windows(&self) -> Vec<crate::protocol::WindowInfo> {
        let mut window_list = Vec::new();
        for window in &self.windows {
            if let Ok(window_guard) = window.try_read() {
                window_list.push(crate::protocol::WindowInfo {
                    id: window_guard.id.clone(),
                    name: window_guard.name.clone(),
                    panes: window_guard.get_pane_count(),
                    is_active: self.current_window.as_ref() == Some(&window_guard.id),
                });
            }
        }
        window_list
    }

    pub async fn enter_copy_mode(&mut self) -> Result<()> {
        if self.copy_mode.is_none() {
            self.copy_mode = Some(crate::ui::copymode::CopyMode::new(crate::config::CopyModeStyle::Vi));
            Ok(())
        } else {
            Err(FerrixError::Other("Already in copy mode".to_string()))
        }
    }

    pub fn get_current_window(&self) -> Option<&Arc<RwLock<Window>>> {
        if let Some(current_window_id) = &self.current_window {
            for window in &self.windows {
                if let Ok(window_guard) = window.try_read() {
                    if window_guard.id == *current_window_id {
                        drop(window_guard);
                        return Some(window);
                    }
                }
            }
        }
        None
    }

    pub async fn get_layout_info(&self) -> Option<crate::protocol::LayoutInfo> {
        if let Some(current_window) = self.get_current_window() {
            let window_guard = current_window.read().await;
            let dimensions = window_guard.layout.get_dimensions(window_guard.width, window_guard.height);

            let panes = dimensions.into_iter().map(|(pane_id, x, y, width, height)| {
                crate::protocol::PaneInfo {
                    id: pane_id.clone(),
                    x,
                    y,
                    width,
                    height,
                    is_focused: window_guard.current_pane.as_ref() == Some(&pane_id),
                }
            }).collect();

            Some(crate::protocol::LayoutInfo {
                window_id: window_guard.id.clone(),
                panes,
                focused_pane: window_guard.current_pane.clone(),
            })
        } else {
            None
        }
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

        // Gather window and pane states from actual windows
        let mut windows = Vec::new();
        let mut panes = Vec::new();

        for (index, window_arc) in self.windows.iter().enumerate() {
            if let Ok(window) = window_arc.try_read() {
                // Create window state
                let window_state = WindowState {
                    id: window.id.clone(),
                    session_id: self.id.clone(),
                    name: window.name.clone(),
                    index,
                    layout: window.layout.clone(),
                    current_pane: window.current_pane.clone(),
                    width: window.width,
                    height: window.height,
                };
                windows.push(window_state);

                // Create pane states for this window
                for (pane_id, pane_arc) in &window.panes {
                    if let Ok(pane) = pane_arc.try_read() {
                        let pane_state = PaneState {
                            id: pane_id.clone(),
                            window_id: window.id.clone(),
                            working_directory: pane.working_directory.clone(),
                            command: pane.command.clone(),
                            cols: pane.cols,
                            rows: pane.rows,
                            scrollback: pane.scrollback.clone(),
                            cursor_position: pane.cursor_position,
                        };
                        panes.push(pane_state);
                    }
                }
            }
        }

        SessionSnapshot {
            metadata,
            session: session_state,
            windows,
            panes,
        }
    }

    pub fn from_snapshot(snapshot: SessionSnapshot) -> Self {
        let mut windows = Vec::new();
        let mut current_window = snapshot.session.current_window.clone();

        if snapshot.windows.is_empty() {
            // Fallback: create a default window if no windows in snapshot
            let window_id = WindowId(Uuid::new_v4());
            let default_window = Window::new(window_id.clone(), "bash".to_string());
            windows.push(Arc::new(RwLock::new(default_window)));
            current_window = Some(window_id);
        } else {
            // Restore windows from snapshot
            for window_state in &snapshot.windows {
                let mut window = Window::new(window_state.id.clone(), window_state.name.clone());

                // Restore window properties
                window.layout = window_state.layout.clone();
                window.current_pane = window_state.current_pane.clone();
                window.width = window_state.width;
                window.height = window_state.height;

                // Clear the default pane that Window::new creates
                window.panes.clear();

                // Restore panes for this window
                for pane_state in &snapshot.panes {
                    if pane_state.window_id == window_state.id {
                        let mut pane = super::pane::Pane::new(pane_state.id.clone());

                        // Restore pane properties
                        pane.working_directory = pane_state.working_directory.clone();
                        pane.command = pane_state.command.clone();
                        pane.cols = pane_state.cols;
                        pane.rows = pane_state.rows;
                        pane.scrollback = pane_state.scrollback.clone();
                        pane.cursor_position = pane_state.cursor_position;

                        window.panes.insert(pane_state.id.clone(), Arc::new(RwLock::new(pane)));
                    }
                }

                // If no panes were restored, create a default one
                if window.panes.is_empty() {
                    let pane_id = PaneId(Uuid::new_v4());
                    let default_pane = super::pane::Pane::new(pane_id.clone());
                    window.panes.insert(pane_id.clone(), Arc::new(RwLock::new(default_pane)));
                    window.current_pane = Some(pane_id);
                }

                windows.push(Arc::new(RwLock::new(window)));
            }
        }

        Self {
            id: snapshot.session.id,
            name: snapshot.session.name,
            windows,
            current_window,
            created_at: snapshot.session.created_at,
            copy_mode: None,
        }
    }
}