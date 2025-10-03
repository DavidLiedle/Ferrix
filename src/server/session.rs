use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use chrono::Utc;
use uuid::Uuid;

use crate::error::{Result, FerrixError};
use crate::protocol::{SessionId, WindowId, PaneId, SplitDirection};
use super::window::Window;
use super::snapshot::{SessionSnapshot, SnapshotMetadata, SessionState, WindowState, PaneState};
use super::layout::NavigationDirection;

#[derive(Debug, Clone)]
pub struct CopyModeState {
    pub cursor_row: usize,
    pub cursor_col: usize,
    pub selection_start: Option<(usize, usize)>,
    pub selection_end: Option<(usize, usize)>,
    pub buffer_content: Vec<String>,
    pub mode: String,
}

pub struct Session {
    pub id: SessionId,
    pub name: String,
    pub windows: Vec<Arc<RwLock<Window>>>,
    pub current_window: Option<WindowId>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub copy_mode: Option<crate::ui::copymode::CopyMode>,
    pub pane_sync_enabled: bool,
    pub locked: bool,
    pub auto_save_enabled: bool,
    pub auto_save_interval: Duration,
    pub last_auto_save: Option<chrono::DateTime<chrono::Utc>>,
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
            pane_sync_enabled: false,
            locked: false,
            auto_save_enabled: false,
            auto_save_interval: Duration::from_secs(300), // Default 5 minutes
            last_auto_save: None,
        }
    }

    pub async fn handle_input(&mut self, data: Vec<u8>) -> Result<()> {
        // If session is locked, don't pass input to the underlying panes
        if self.locked {
            return Ok(());
        }

        // If copy mode is active, don't pass input to the underlying pane
        if self.copy_mode.is_some() {
            return Ok(());
        }

        if let Some(current_window_id) = &self.current_window {
            for window in &self.windows {
                let window_guard = window.read().await;
                if window_guard.id == *current_window_id {
                    drop(window_guard);
                    let mut window_guard = window.write().await;

                    if self.pane_sync_enabled {
                        // Broadcast input to all panes in the current window
                        window_guard.handle_input_broadcast(data).await?;
                    } else {
                        // Send input only to the focused pane
                        window_guard.handle_input(data).await?;
                    }
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

    pub async fn zoom_pane(&mut self) -> Result<(bool, Option<PaneId>)> {
        if let Some(current_window_id) = &self.current_window {
            for window in &self.windows {
                let window_guard = window.read().await;
                if window_guard.id == *current_window_id {
                    drop(window_guard);
                    let mut window_guard = window.write().await;
                    let zoomed = window_guard.toggle_zoom().await?;
                    let zoomed_pane = if zoomed {
                        window_guard.get_zoomed_pane()
                    } else {
                        None
                    };
                    return Ok((zoomed, zoomed_pane));
                }
            }
        }
        Err(FerrixError::Other("No current window".to_string()))
    }

    pub async fn rename_window(&mut self, window_id: Option<WindowId>, new_name: String) -> Result<WindowId> {
        let target_window_id = window_id.unwrap_or_else(|| self.current_window.clone().unwrap_or_else(|| WindowId(Uuid::new_v4())));

        for window in &self.windows {
            let window_guard = window.read().await;
            if window_guard.id == target_window_id {
                drop(window_guard);
                let mut window_guard = window.write().await;
                window_guard.rename(new_name);
                return Ok(target_window_id);
            }
        }

        Err(FerrixError::WindowNotFound(format!("{:?}", target_window_id)))
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
                    activity_status: window_guard.get_window_activity_summary(),
                });
            }
        }
        window_list
    }

    pub async fn enter_copy_mode(&mut self) -> Result<()> {
        if self.copy_mode.is_none() {
            // Get buffer first
            let buffer = self.get_current_pane_buffer().await;

            // Create and initialize copy mode
            let mut copy_mode = crate::ui::copymode::CopyMode::new(crate::config::CopyModeStyle::Vi);
            copy_mode.enter(buffer);
            self.copy_mode = Some(copy_mode);

            Ok(())
        } else {
            Err(FerrixError::Other("Already in copy mode".to_string()))
        }
    }

    pub async fn exit_copy_mode(&mut self) -> Result<()> {
        if let Some(copy_mode) = &mut self.copy_mode {
            copy_mode.exit();
            self.copy_mode = None;
            Ok(())
        } else {
            Err(FerrixError::Other("Not in copy mode".to_string()))
        }
    }

    pub async fn handle_copy_mode_input(&mut self, key: String) -> Result<Option<CopyModeState>> {
        if let Some(copy_mode) = &mut self.copy_mode {
            // Handle the key input using the copy mode implementation
            match key.as_str() {
                "h" | "Left" => copy_mode.move_cursor_left(),
                "j" | "Down" => copy_mode.move_cursor_down(),
                "k" | "Up" => copy_mode.move_cursor_up(),
                "l" | "Right" => copy_mode.move_cursor_right(),
                "w" => copy_mode.move_word_forward(),
                "b" => copy_mode.move_word_backward(),
                "0" => copy_mode.move_to_line_start(),
                "$" => copy_mode.move_to_line_end(),
                "g" => copy_mode.move_to_first_line(),
                "G" => copy_mode.move_to_last_line(),
                "v" => copy_mode.enter_visual_mode(),
                "V" => copy_mode.enter_visual_line_mode(),
                "y" => {
                    copy_mode.yank_selection();
                    // Exit copy mode after yanking
                    copy_mode.exit();
                    self.copy_mode = None;
                    return Ok(None);
                }
                "Escape" => {
                    if *copy_mode.state() == crate::ui::copymode::CopyModeState::Visual
                        || *copy_mode.state() == crate::ui::copymode::CopyModeState::VisualLine {
                        copy_mode.exit_visual_mode();
                    } else {
                        copy_mode.exit();
                        self.copy_mode = None;
                        return Ok(None);
                    }
                }
                "/" => copy_mode.start_search(crate::ui::copymode::SearchDirection::Forward),
                "?" => copy_mode.start_search(crate::ui::copymode::SearchDirection::Backward),
                "n" => copy_mode.jump_to_next_match(),
                "N" => copy_mode.jump_to_previous_match(),
                "Ctrl+u" => copy_mode.move_half_page_up(),
                "Ctrl+d" => copy_mode.move_half_page_down(),
                "Ctrl+o" => copy_mode.jump_backward(),
                "Ctrl+i" => copy_mode.jump_forward(),
                _ => {
                    // For other keys, we might want to handle them differently
                    // For now, just ignore them
                }
            }

            // Update selection if in visual mode
            if *copy_mode.state() == crate::ui::copymode::CopyModeState::Visual
                || *copy_mode.state() == crate::ui::copymode::CopyModeState::VisualLine {
                copy_mode.update_selection();
            }

            // Return updated state
            Ok(Some(CopyModeState {
                cursor_row: copy_mode.cursor_row(),
                cursor_col: copy_mode.cursor_col(),
                selection_start: copy_mode.selection_start(),
                selection_end: copy_mode.selection_end(),
                buffer_content: copy_mode.buffer().clone(),
                mode: match copy_mode.state() {
                    crate::ui::copymode::CopyModeState::Normal => "COPY".to_string(),
                    crate::ui::copymode::CopyModeState::Visual => "VISUAL".to_string(),
                    crate::ui::copymode::CopyModeState::VisualLine => "VISUAL LINE".to_string(),
                    crate::ui::copymode::CopyModeState::VisualBlock => "VISUAL BLOCK".to_string(),
                    crate::ui::copymode::CopyModeState::Search(_) => "SEARCH".to_string(),
                },
            }))
        } else {
            Err(FerrixError::Other("Not in copy mode".to_string()))
        }
    }

    pub async fn get_copy_mode_state(&self) -> Option<CopyModeState> {
        if let Some(copy_mode) = &self.copy_mode {
            Some(CopyModeState {
                cursor_row: copy_mode.cursor_row(),
                cursor_col: copy_mode.cursor_col(),
                selection_start: copy_mode.selection_start(),
                selection_end: copy_mode.selection_end(),
                buffer_content: copy_mode.buffer().clone(),
                mode: match copy_mode.state() {
                    crate::ui::copymode::CopyModeState::Normal => "COPY".to_string(),
                    crate::ui::copymode::CopyModeState::Visual => "VISUAL".to_string(),
                    crate::ui::copymode::CopyModeState::VisualLine => "VISUAL LINE".to_string(),
                    crate::ui::copymode::CopyModeState::VisualBlock => "VISUAL BLOCK".to_string(),
                    crate::ui::copymode::CopyModeState::Search(_) => "SEARCH".to_string(),
                },
            })
        } else {
            None
        }
    }

    async fn get_current_pane_buffer(&mut self) -> Vec<String> {
        // Get the buffer content from the current pane
        if let Some(current_window) = self.get_current_window() {
            let window_guard = current_window.read().await;
            if let Some(current_pane_id) = &window_guard.current_pane {
                if let Some(pane_arc) = window_guard.panes.get(current_pane_id) {
                    let pane_guard = pane_arc.read().await;
                    return pane_guard.scrollback.to_vec();
                }
            }
        }

        // Fallback: return some dummy content for testing
        vec![
            "Welcome to Ferrix Terminal Multiplexer".to_string(),
            "Copy mode is now active!".to_string(),
            "Use vim-like keys to navigate:".to_string(),
            "  h/j/k/l or arrow keys - move cursor".to_string(),
            "  v - enter visual mode".to_string(),
            "  V - enter visual line mode".to_string(),
            "  y - yank (copy) selection".to_string(),
            "  / - search forward".to_string(),
            "  ? - search backward".to_string(),
            "  Esc - exit copy mode".to_string(),
            "".to_string(),
            "This is line 12".to_string(),
            "This is line 13 with some longer text to test scrolling".to_string(),
            "Line 14".to_string(),
            "Line 15".to_string(),
        ]
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
                            scrollback: pane.scrollback.to_vec(),
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
                        pane.scrollback.from_vec(pane_state.scrollback.clone());
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
            pane_sync_enabled: false,
            locked: false,
            auto_save_enabled: false,
            auto_save_interval: Duration::from_secs(300),
            last_auto_save: None,
        }
    }

    pub fn toggle_pane_sync(&mut self) -> bool {
        self.pane_sync_enabled = !self.pane_sync_enabled;
        self.pane_sync_enabled
    }

    pub fn set_pane_sync(&mut self, enabled: bool) -> bool {
        self.pane_sync_enabled = enabled;
        self.pane_sync_enabled
    }

    pub fn is_pane_sync_enabled(&self) -> bool {
        self.pane_sync_enabled
    }

    pub fn lock_session(&mut self) -> bool {
        self.locked = true;
        self.locked
    }

    pub fn unlock_session(&mut self) -> bool {
        self.locked = false;
        self.locked
    }

    pub fn set_session_lock(&mut self, locked: bool) -> bool {
        self.locked = locked;
        self.locked
    }

    pub fn is_session_locked(&self) -> bool {
        self.locked
    }
    pub fn enable_auto_save(&mut self, interval_seconds: u64) {
        self.auto_save_enabled = true;
        self.auto_save_interval = Duration::from_secs(interval_seconds);
    }

    pub fn disable_auto_save(&mut self) {
        self.auto_save_enabled = false;
    }

    pub fn should_auto_save(&self) -> bool {
        if !self.auto_save_enabled {
            return false;
        }

        match self.last_auto_save {
            None => true,
            Some(last_save) => {
                let elapsed = Utc::now() - last_save;
                elapsed.num_seconds() as u64 >= self.auto_save_interval.as_secs()
            }
        }
    }

    pub fn mark_auto_saved(&mut self) {
        self.last_auto_save = Some(Utc::now());
    }

    pub fn get_auto_save_interval(&self) -> Duration {
        self.auto_save_interval
    }

    pub fn is_auto_save_enabled(&self) -> bool {
        self.auto_save_enabled
    }

    // Activity monitoring methods
    pub async fn toggle_activity_monitoring(&mut self, pane_id: Option<PaneId>) -> Result<(PaneId, bool)> {
        if let Some(current_window_id) = &self.current_window {
            for window in &self.windows {
                let window_guard = window.read().await;
                if window_guard.id == *current_window_id {
                    drop(window_guard);
                    let mut window_guard = window.write().await;

                    let target_pane = pane_id.unwrap_or_else(|| {
                        window_guard.current_pane.clone().unwrap_or_else(|| {
                            window_guard.panes.keys().next().cloned().unwrap()
                        })
                    });

                    let currently_enabled = window_guard.activity_monitor.is_monitoring_enabled(&target_pane);
                    if currently_enabled {
                        window_guard.activity_monitor.disable_monitoring(&target_pane);
                    } else {
                        window_guard.activity_monitor.enable_monitoring(&target_pane);
                    }

                    return Ok((target_pane, !currently_enabled));
                }
            }
        }
        Err(FerrixError::Other("No current window".to_string()))
    }

    pub async fn set_activity_monitoring(&mut self, pane_id: Option<PaneId>, enabled: bool) -> Result<(PaneId, bool)> {
        if let Some(current_window_id) = &self.current_window {
            for window in &self.windows {
                let window_guard = window.read().await;
                if window_guard.id == *current_window_id {
                    drop(window_guard);
                    let mut window_guard = window.write().await;

                    let target_pane = pane_id.unwrap_or_else(|| {
                        window_guard.current_pane.clone().unwrap_or_else(|| {
                            window_guard.panes.keys().next().cloned().unwrap()
                        })
                    });

                    if enabled {
                        window_guard.activity_monitor.enable_monitoring(&target_pane);
                    } else {
                        window_guard.activity_monitor.disable_monitoring(&target_pane);
                    }

                    return Ok((target_pane, enabled));
                }
            }
        }
        Err(FerrixError::Other("No current window".to_string()))
    }

    pub async fn get_activity_status(&self, pane_id: &PaneId) -> Option<String> {
        if let Some(current_window_id) = &self.current_window {
            for window in &self.windows {
                let window_guard = window.read().await;
                if window_guard.id == *current_window_id {
                    return window_guard.activity_monitor.get_activity_status(pane_id);
                }
            }
        }
        None
    }

    // Pane resizing methods
    pub async fn resize_pane(&mut self, direction: crate::protocol::ResizeDirection, amount: i16) -> Result<()> {
        if let Some(current_window_id) = &self.current_window {
            for window in &self.windows {
                let window_guard = window.read().await;
                if window_guard.id == *current_window_id {
                    if let Some(current_pane_id) = &window_guard.current_pane {
                        let pane_id = current_pane_id.clone();
                        drop(window_guard);

                        let window_guard = window.write().await;
                        if let Some(pane_arc) = window_guard.panes.get(&pane_id) {
                            let mut pane = pane_arc.write().await;

                            // Calculate new dimensions based on direction
                            let (new_cols, new_rows) = match direction {
                                crate::protocol::ResizeDirection::Up => {
                                    (pane.cols, pane.rows.saturating_add(amount as u16))
                                }
                                crate::protocol::ResizeDirection::Down => {
                                    (pane.cols, pane.rows.saturating_sub(amount as u16).max(1))
                                }
                                crate::protocol::ResizeDirection::Left => {
                                    (pane.cols.saturating_sub(amount as u16).max(1), pane.rows)
                                }
                                crate::protocol::ResizeDirection::Right => {
                                    (pane.cols.saturating_add(amount as u16), pane.rows)
                                }
                            };

                            // Resize the pane
                            pane.resize(new_cols, new_rows).await?;
                            return Ok(());
                        }
                    }
                    return Err(FerrixError::Other("No current pane".to_string()));
                }
            }
        }
        Err(FerrixError::Other("No current window".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[tokio::test]
    async fn test_session_creation() {
        let session_id = SessionId(Uuid::new_v4());
        let session_name = "test_session".to_string();
        let session = Session::new(session_id.clone(), session_name.clone());

        assert_eq!(session.id, session_id);
        assert_eq!(session.name, session_name);
        assert_eq!(session.windows.len(), 1);
        assert!(session.current_window.is_some());
        assert!(session.copy_mode.is_none());
        assert!(session.created_at <= Utc::now());
    }

    #[tokio::test]
    async fn test_session_create_window() {
        let session_id = SessionId(Uuid::new_v4());
        let mut session = Session::new(session_id, "test".to_string());

        let initial_window_count = session.windows.len();
        let new_window_id = session.create_window(Some("new_window".to_string())).await.unwrap();

        assert_eq!(session.windows.len(), initial_window_count + 1);
        assert_eq!(session.current_window, Some(new_window_id));
    }

    #[tokio::test]
    async fn test_session_switch_window() {
        let session_id = SessionId(Uuid::new_v4());
        let mut session = Session::new(session_id, "test".to_string());

        let initial_window = session.current_window.clone().unwrap();
        let new_window_id = session.create_window(None).await.unwrap();

        // Switch back to initial window
        let result = session.switch_window(initial_window.clone()).await;
        assert!(result.is_ok());
        assert_eq!(session.current_window, Some(initial_window));

        // Switch to invalid window should fail
        let invalid_id = WindowId(Uuid::new_v4());
        let result = session.switch_window(invalid_id).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_session_close_window() {
        let session_id = SessionId(Uuid::new_v4());
        let mut session = Session::new(session_id, "test".to_string());

        // Cannot close last window
        let current_window = session.current_window.clone().unwrap();
        let result = session.close_window(current_window).await;
        assert!(result.is_err());

        // Create another window and try closing
        let new_window_id = session.create_window(None).await.unwrap();
        let result = session.close_window(new_window_id).await;
        assert!(result.is_ok());
        assert_eq!(session.windows.len(), 1);
    }

    #[tokio::test]
    async fn test_session_next_previous_window() {
        let session_id = SessionId(Uuid::new_v4());
        let mut session = Session::new(session_id, "test".to_string());

        // With single window, next/previous should work but not change anything
        let initial_window = session.current_window.clone();
        session.next_window().await.unwrap();
        assert_eq!(session.current_window, initial_window);
        session.previous_window().await.unwrap();
        assert_eq!(session.current_window, initial_window);

        // Create additional windows
        let window2 = session.create_window(None).await.unwrap();
        let window3 = session.create_window(None).await.unwrap();

        // Current should be window3, go to next (should wrap to first)
        session.next_window().await.unwrap();
        assert_ne!(session.current_window, Some(window3.clone()));

        // Test previous
        session.previous_window().await.unwrap();
        assert_eq!(session.current_window, Some(window3));
    }

    #[tokio::test]
    async fn test_session_split_pane() {
        let session_id = SessionId(Uuid::new_v4());
        let mut session = Session::new(session_id, "test".to_string());

        let result = session.split_pane(SplitDirection::Horizontal).await;
        assert!(result.is_ok());

        let result = session.split_pane(SplitDirection::Vertical).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_session_handle_input_without_copy_mode() {
        let session_id = SessionId(Uuid::new_v4());
        let mut session = Session::new(session_id, "test".to_string());

        let test_input = b"test input".to_vec();
        let result = session.handle_input(test_input).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_session_handle_input_with_copy_mode() {
        let session_id = SessionId(Uuid::new_v4());
        let mut session = Session::new(session_id, "test".to_string());

        // Enter copy mode first
        let result = session.enter_copy_mode().await;
        assert!(result.is_ok());
        assert!(session.copy_mode.is_some());

        // Input should be ignored in copy mode
        let test_input = b"test input".to_vec();
        let result = session.handle_input(test_input).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_session_copy_mode_lifecycle() {
        let session_id = SessionId(Uuid::new_v4());
        let mut session = Session::new(session_id, "test".to_string());

        // Initially not in copy mode
        assert!(session.copy_mode.is_none());

        // Enter copy mode
        let result = session.enter_copy_mode().await;
        assert!(result.is_ok());
        assert!(session.copy_mode.is_some());

        // Cannot enter copy mode twice
        let result = session.enter_copy_mode().await;
        assert!(result.is_err());

        // Exit copy mode
        let result = session.exit_copy_mode().await;
        assert!(result.is_ok());
        assert!(session.copy_mode.is_none());

        // Cannot exit copy mode when not in it
        let result = session.exit_copy_mode().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_session_resize() {
        let session_id = SessionId(Uuid::new_v4());
        let mut session = Session::new(session_id, "test".to_string());

        let result = session.resize(100, 50).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_session_list_windows() {
        let session_id = SessionId(Uuid::new_v4());
        let session = Session::new(session_id, "test".to_string());

        let window_list = session.list_windows();
        assert_eq!(window_list.len(), 1);
        assert!(window_list[0].is_active);
        assert_eq!(window_list[0].name, "bash");
    }

    #[tokio::test]
    async fn test_session_snapshot_creation() {
        let session_id = SessionId(Uuid::new_v4());
        let session = Session::new(session_id, "test_session".to_string());

        let snapshot = session.create_snapshot(
            Some("test_snapshot".to_string()),
            Some("Test snapshot description".to_string())
        );

        assert_eq!(snapshot.metadata.name, "test_snapshot");
        assert_eq!(snapshot.metadata.description, "Test snapshot description");
        assert_eq!(snapshot.session.id, session.id);
        assert_eq!(snapshot.session.name, session.name);
        assert_eq!(snapshot.windows.len(), 1);
        assert_eq!(snapshot.panes.len(), 1);
    }

    #[tokio::test]
    async fn test_session_from_snapshot() {
        let session_id = SessionId(Uuid::new_v4());
        let original_session = Session::new(session_id, "original".to_string());

        let snapshot = original_session.create_snapshot(None, None);
        let restored_session = Session::from_snapshot(snapshot);

        assert_eq!(restored_session.id, original_session.id);
        assert_eq!(restored_session.name, original_session.name);
        assert_eq!(restored_session.windows.len(), 1);
        assert!(restored_session.current_window.is_some());
        assert!(restored_session.copy_mode.is_none());
    }

    #[tokio::test]
    async fn test_session_rename_window() {
        let session_id = SessionId(Uuid::new_v4());
        let mut session = Session::new(session_id, "test".to_string());

        // Get the default window ID
        let window_id = session.current_window.clone().unwrap();

        // Rename the window
        let result = session.rename_window(Some(window_id.clone()), "new-name".to_string()).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), window_id);

        // Check that the window was actually renamed
        let window_list = session.list_windows();
        assert_eq!(window_list.len(), 1);
        assert_eq!(window_list[0].name, "new-name");
    }

    #[tokio::test]
    async fn test_session_rename_current_window() {
        let session_id = SessionId(Uuid::new_v4());
        let mut session = Session::new(session_id, "test".to_string());

        let original_window_id = session.current_window.clone().unwrap();

        // Rename current window (no window_id specified)
        let result = session.rename_window(None, "current-renamed".to_string()).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), original_window_id);

        // Check that the current window was renamed
        let window_list = session.list_windows();
        assert_eq!(window_list.len(), 1);
        assert_eq!(window_list[0].name, "current-renamed");
    }

    #[tokio::test]
    async fn test_session_rename_nonexistent_window() {
        let session_id = SessionId(Uuid::new_v4());
        let mut session = Session::new(session_id, "test".to_string());

        // Try to rename a non-existent window
        let fake_window_id = WindowId(Uuid::new_v4());
        let result = session.rename_window(Some(fake_window_id), "should-fail".to_string()).await;
        assert!(result.is_err());
    }
}