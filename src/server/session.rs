use std::sync::Arc;
use std::time::Duration;
use std::path::PathBuf;
use tokio::sync::RwLock;
use chrono::Utc;
use uuid::Uuid;

use crate::error::{Result, FerrixError};
use crate::protocol::{SessionId, WindowId, PaneId, SplitDirection};
use crate::format::{FormatProvider, FormatValue};
use super::window::Window;
use super::snapshot::{SessionSnapshot, SnapshotMetadata, SessionState, WindowState, PaneState};
use super::layout::NavigationDirection;
use super::recording::SessionRecorder;

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

    pub recorder: Option<SessionRecorder>,

    pub current_layout_index: usize,
    pub current_layout_preset: Option<String>,
    pub session_config: Option<crate::config::session_config::SessionConfig>,

    #[cfg(feature = "versioning")]
    pub versioning: Option<Box<super::versioning::SessionVersioning>>,
}

#[derive(Debug, Clone)]
pub struct RecordingStatus {
    pub is_recording: bool,
    pub is_paused: bool,
    pub output_path: Option<PathBuf>,
    pub duration_secs: u64,
    pub event_count: u64,
}

// Move these methods to proper location
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

            recorder: None,

            current_layout_index: 0,
            current_layout_preset: Some("single".to_string()),
            session_config: None,

            #[cfg(feature = "versioning")]
            versioning: None,
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

        // Record input if recording is active
        self.record_input(data.clone()).await;

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
        // Record resize if recording is active
        self.record_resize(cols, rows).await;

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

        // Record output after the borrow is released
        for (_, ref output) in &outputs {
            if !output.is_empty() {
                self.record_output(output.clone()).await;
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

    /// Toggle between current and last pane (tmux last-pane)
    pub async fn select_last_pane(&mut self) -> Result<()> {
        if let Some(current_window_id) = &self.current_window {
            for window in &self.windows {
                let window_guard = window.read().await;
                if window_guard.id == *current_window_id {
                    drop(window_guard);
                    let mut window_guard = window.write().await;
                    return window_guard.select_last_pane().await;
                }
            }
        }
        Err(FerrixError::Other("No current window".to_string()))
    }

    /// Select a pane by its index (0-based)
    pub fn select_pane_by_index(&mut self, index: usize) -> Result<()> {
        if let Some(current_window_id) = &self.current_window {
            for window in &self.windows {
                // Need to block on the async read
                let window_guard = futures::executor::block_on(window.read());
                if window_guard.id == *current_window_id {
                    drop(window_guard);
                    let mut window_guard = futures::executor::block_on(window.write());
                    return window_guard.select_pane_by_index(index);
                }
            }
        }
        Err(FerrixError::Other("No current window".to_string()))
    }

    /// Respawn a pane (restart its PTY)
    pub async fn respawn_pane(&mut self, pane_id: PaneId) -> Result<()> {
        if let Some(current_window_id) = &self.current_window {
            for window in &self.windows {
                let window_guard = window.read().await;
                if window_guard.id == *current_window_id {
                    drop(window_guard);
                    let mut window_guard = window.write().await;
                    return window_guard.respawn_pane(&pane_id).await;
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

    pub fn load_session_config(&mut self, config_path: Option<PathBuf>) -> Result<()> {
        use crate::config::session_config::SessionConfig;

        // Try to load from specified path or default location
        let config = if let Some(path) = config_path {
            SessionConfig::load_from_file(path)?
        } else {
            // Try default location
            let config_dir = dirs::config_dir()
                .ok_or_else(|| FerrixError::Config("Could not find config directory".to_string()))?
                .join("ferrix")
                .join("sessions");

            let config_path = config_dir.join(format!("{}.toml", self.id.0));
            if config_path.exists() {
                SessionConfig::load_from_file(config_path)?
            } else {
                return Ok(()); // No config file, that's fine
            }
        };

        // Apply environment variables
        for (key, value) in &config.environment {
            std::env::set_var(key, value);
        }

        // Run startup commands
        for command in &config.startup_commands {
            // Execute command in the first pane of the first window
            if let Some(window) = self.windows.first() {
                let window_guard = window.blocking_read();
                if let Some(pane) = window_guard.panes.values().next() {
                    let mut pane_guard = pane.blocking_write();
                    // Use handle_input async method with blocking
                    let mut cmd_bytes = command.as_bytes().to_vec();
                    cmd_bytes.push(b'\n');
                    let _ = tokio::task::block_in_place(|| {
                        tokio::runtime::Handle::current().block_on(
                            pane_guard.handle_input(cmd_bytes)
                        )
                    });
                }
            }
        }

        // Apply default layout if specified
        if let Some(ref layout_name) = config.default_layout {
            self.apply_layout_preset(layout_name);
        }

        // Run after_session_create hooks
        for hook in &config.hooks.after_session_create {
            // Execute hook command
            std::process::Command::new("sh")
                .arg("-c")
                .arg(hook)
                .env("FERRIX_SESSION_ID", format!("{}", self.id.0))
                .env("FERRIX_SESSION_NAME", &self.name)
                .spawn()
                .ok();
        }

        self.session_config = Some(config);
        Ok(())
    }

    pub fn save_session_config(&self) -> Result<()> {
        if let Some(ref config) = self.session_config {
            let config_dir = dirs::config_dir()
                .ok_or_else(|| FerrixError::Config("Could not find config directory".to_string()))?
                .join("ferrix")
                .join("sessions");

            std::fs::create_dir_all(&config_dir)?;

            let config_path = config_dir.join(format!("{}.toml", self.id.0));
            config.save_to_file(config_path)?;
        }
        Ok(())
    }

    pub fn apply_template(&mut self, template_name: &str) -> Result<()> {
        use crate::config::session_config::SessionConfigTemplate;

        let templates = SessionConfigTemplate::all_templates();
        let template = templates
            .into_iter()
            .find(|t| t.name.to_lowercase() == template_name.to_lowercase())
            .ok_or_else(|| FerrixError::Config(format!("Template '{}' not found", template_name)))?;

        self.session_config = Some(template.config.clone());
        self.load_session_config(None)?;
        Ok(())
    }

    pub fn get_effective_config(&self, global_config: &crate::config::Config) -> crate::config::Config {
        if let Some(ref session_config) = self.session_config {
            session_config.merge_with_global(global_config)
        } else {
            global_config.clone()
        }
    }

    pub fn apply_layout_preset(&mut self, preset_name: &str) -> bool {
        use crate::server::layout_presets::LayoutPreset;

        if let Some(preset) = LayoutPreset::from_name(preset_name) {
            if let Some(current_window_id) = &self.current_window {
                // Find the current window
                for window in &mut self.windows {
                    let mut window_guard = window.blocking_write();
                    if window_guard.id == *current_window_id {
                        // Apply the preset layout
                        window_guard.apply_preset_layout(preset);
                        // Store the preset name for tracking
                        self.current_layout_preset = Some(preset_name.to_string());
                        return true;
                    }
                }
            }
        }
        false
    }

    pub fn cycle_layout(&mut self) -> String {
        use crate::server::layout_presets::LayoutPreset;

        let all_presets = LayoutPreset::all_presets();
        self.current_layout_index = (self.current_layout_index + 1) % all_presets.len();

        let preset = &all_presets[self.current_layout_index];
        let preset_name = preset.name().to_string();

        if let Some(current_window_id) = &self.current_window {
            // Find the current window
            for window in &mut self.windows {
                let mut window_guard = window.blocking_write();
                if window_guard.id == *current_window_id {
                    window_guard.apply_preset_layout(preset.clone());
                    // Store the preset name for tracking
                    self.current_layout_preset = Some(preset_name.clone());
                    break;
                }
            }
        }

        preset_name
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
                    let _ = copy_mode.yank_selection();
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
        self.copy_mode.as_ref().map(|copy_mode| CopyModeState {
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
                    panes: std::collections::HashMap::new(),  // Will be populated below
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
            created_at: chrono::Utc::now(),
            environment: std::collections::HashMap::new(),  // Can be populated from session_state.environment
            config: None,  // Can be populated with session config if needed
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

            recorder: None,

            current_layout_index: 0,
            current_layout_preset: Some("single".to_string()),
            session_config: None,

            #[cfg(feature = "versioning")]
            versioning: None,
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
                            window_guard.panes.keys().next().cloned()
                                .expect("Window must have at least one pane")
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
                            window_guard.panes.keys().next().cloned()
                                .expect("Window must have at least one pane")
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

#[cfg(feature = "versioning")]
impl Session {
    // Versioning methods
    pub async fn init_versioning(&mut self) -> Result<()> {
        use super::versioning::SessionVersioning;

        if self.versioning.is_some() {
            return Err(FerrixError::Other("Versioning already initialized".to_string()));
        }

        let mut versioning = SessionVersioning::new(self.id.clone())?;

        // Create initial commit with current state
        let snapshot = self.to_snapshot().await;
        versioning.init_with_snapshot(snapshot)?;

        self.versioning = Some(Box::new(versioning));
        Ok(())
    }

    pub async fn commit_changes(&mut self, message: &str, author: &str) -> Result<super::versioning::CommitId> {
        // Create snapshot before borrowing versioning
        let snapshot = self.to_snapshot().await;

        let versioning = self.versioning.as_mut()
            .ok_or_else(|| FerrixError::Other("Versioning not initialized".to_string()))?;

        versioning.commit(snapshot, message, author)
    }

    pub async fn create_branch(&mut self, name: &str, from_commit: Option<&str>) -> Result<()> {
        let versioning = self.versioning.as_mut()
            .ok_or_else(|| FerrixError::Other("Versioning not initialized".to_string()))?;

        versioning.create_branch(name, from_commit)
    }

    pub async fn checkout_branch(&mut self, name: &str) -> Result<()> {
        let versioning = self.versioning.as_mut()
            .ok_or_else(|| FerrixError::Other("Versioning not initialized".to_string()))?;

        let snapshot = versioning.checkout_branch(name)?;
        self.restore_from_snapshot(snapshot).await;
        Ok(())
    }

    pub async fn merge_branch(&mut self, source: &str, auto_resolve: bool) -> Result<(Vec<String>, Vec<String>)> {
        let versioning = self.versioning.as_mut()
            .ok_or_else(|| FerrixError::Other("Versioning not initialized".to_string()))?;

        let (merged_snapshot, conflicts, resolved) = versioning.merge_branch(source, auto_resolve)?;

        if conflicts.is_empty() {
            self.restore_from_snapshot(merged_snapshot).await;
        }

        Ok((conflicts, resolved))
    }

    pub fn list_branches(&self) -> Vec<super::versioning::Branch> {
        self.versioning.as_ref()
            .map(|v| v.list_branches())
            .unwrap_or_default()
    }

    pub fn get_current_branch(&self) -> Option<&str> {
        self.versioning.as_ref()
            .and_then(|v| v.current_branch())
    }

    pub fn get_commit_log(&self, limit: usize) -> Vec<super::versioning::Commit> {
        self.versioning.as_ref()
            .map(|v| v.get_log(limit))
            .unwrap_or_default()
    }

    pub fn diff_commits(&self, from: Option<&str>, to: Option<&str>) -> Result<String> {
        let versioning = self.versioning.as_ref()
            .ok_or_else(|| FerrixError::Other("Versioning not initialized".to_string()))?;

        versioning.diff(from, to)
    }
}

// Core session methods (always available)
impl Session {
    #[allow(dead_code)]
    async fn restore_from_snapshot(&mut self, snapshot: super::snapshot::SessionSnapshot) {
        // Restore session metadata
        self.name = snapshot.session.name;
        self.created_at = snapshot.session.created_at;

        // Clear existing windows
        self.windows.clear();

        // Restore windows and panes from snapshot
        if snapshot.windows.is_empty() {
            // Fallback: create a default window if no windows in snapshot
            let window_id = WindowId(Uuid::new_v4());
            let default_window = Window::new(window_id.clone(), "bash".to_string());
            self.windows.push(Arc::new(RwLock::new(default_window)));
            self.current_window = Some(window_id);
        } else {
            // Restore windows
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

                self.windows.push(Arc::new(RwLock::new(window)));
            }

            // Restore current window
            self.current_window = snapshot.session.current_window.clone();
        }

        // Restore environment variables if provided
        for (key, value) in snapshot.environment {
            std::env::set_var(key, value);
        }

        // Restore configuration if provided
        if let Some(config_value) = snapshot.config {
            // Try to deserialize the config value into SessionConfig
            if let Ok(config) = serde_json::from_value::<crate::config::session_config::SessionConfig>(config_value) {
                self.session_config = Some(config);
            }
        }
    }

    pub async fn to_snapshot(&self) -> super::snapshot::SessionSnapshot {
        use super::snapshot::{SessionSnapshot, SessionState, WindowState, PaneState, SnapshotMetadata};
        use chrono::Utc;

        let mut all_panes = Vec::new();

        let windows: Vec<WindowState> = self.windows.iter().enumerate().map(|(idx, w)| {
            let window = w.blocking_read();

            // Collect panes from this window
            let mut window_panes: std::collections::HashMap<String, PaneState> = std::collections::HashMap::new();
            for (pane_id, pane_arc) in &window.panes {
                let pane = pane_arc.blocking_read();
                let pane_state = PaneState {
                    id: pane.id.clone(),
                    window_id: window.id.clone(),
                    working_directory: pane.working_directory.clone(),
                    command: pane.command.clone(),
                    cols: pane.cols,
                    rows: pane.rows,
                    scrollback: vec![], // Scrollback lines - simplified for now
                    cursor_position: pane.cursor_position,
                };
                window_panes.insert(pane_id.0.to_string(), pane_state.clone());
                all_panes.push(pane_state);
            }

            // Get terminal dimensions from first pane or use defaults
            let (width, height) = window.panes.values().next()
                .map(|p| {
                    let pane = p.blocking_read();
                    (pane.cols, pane.rows)
                })
                .unwrap_or((80, 24));

            WindowState {
                id: window.id.clone(),
                session_id: self.id.clone(),
                name: window.name.clone(),
                index: idx,
                layout: window.layout.clone(),
                current_pane: window.current_pane.clone(),
                width,
                height,
                panes: window_panes,
            }
        }).collect();

        // Collect environment variables
        let environment: Vec<(String, String)> = std::env::vars()
            .filter(|(k, _)| {
                // Only save relevant environment variables
                matches!(k.as_str(), "TERM" | "SHELL" | "PATH" | "HOME" | "USER" | "EDITOR")
            })
            .collect();

        // Calculate checksum of snapshot data using md5 (recording is always enabled)
        let snapshot_data = format!("{:?}{:?}{:?}", self.name, windows.len(), all_panes.len());
        let checksum = format!("{:x}", md5::compute(snapshot_data.as_bytes()));

        SessionSnapshot {
            metadata: SnapshotMetadata {
                id: Uuid::new_v4(),
                name: format!("Snapshot of {}", self.name),
                description: "Session snapshot".to_string(),
                created_at: Utc::now(),
                ferrix_version: env!("CARGO_PKG_VERSION").to_string(),
                checksum: Some(checksum),
            },
            session: SessionState {
                id: self.id.clone(),
                name: self.name.clone(),
                current_window: self.current_window.clone(),
                created_at: self.created_at,
                environment: environment.clone(),
            },
            windows,
            panes: all_panes,
            created_at: Utc::now(),
            environment: environment.into_iter().collect(),
            config: None,
        }
    }

    // Recording methods
    pub async fn start_recording(&mut self, output_path: PathBuf) -> Result<()> {
        if self.recorder.is_some() {
            return Err(FerrixError::Other("Recording already in progress".to_string()));
        }

        let metadata = super::recording::RecordingMetadata {
            version: 1,
            session_id: self.id.clone(),
            session_name: self.name.clone(),
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            duration_ms: None,
            terminal_size: (80, 24), // Will be updated on first resize
            shell: std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string()),
            user: std::env::var("USER").unwrap_or_else(|_| "unknown".to_string()),
            hostname: hostname::get()
                .ok()
                .and_then(|h| h.into_string().ok())
                .unwrap_or_else(|| "unknown".to_string()),
            compressed: output_path.extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext == "gz")
                .unwrap_or(false),
        };

        self.recorder = Some(SessionRecorder::new(metadata, output_path)?);
        Ok(())
    }

    pub async fn stop_recording(&mut self) -> Result<(u64, u64)> {
        if let Some(mut recorder) = self.recorder.take() {
            recorder.stop().await
        } else {
            Err(FerrixError::Other("No recording in progress".to_string()))
        }
    }

    pub async fn pause_recording(&mut self) -> Result<()> {
        if let Some(recorder) = &mut self.recorder {
            recorder.pause();
            Ok(())
        } else {
            Err(FerrixError::Other("No recording in progress".to_string()))
        }
    }

    pub async fn resume_recording(&mut self) -> Result<()> {
        if let Some(recorder) = &mut self.recorder {
            recorder.resume();
            Ok(())
        } else {
            Err(FerrixError::Other("No recording in progress".to_string()))
        }
    }

    pub async fn get_recording_status(&self) -> RecordingStatus {
        if let Some(recorder) = &self.recorder {
            RecordingStatus {
                is_recording: true,
                is_paused: recorder.is_paused(),
                output_path: Some(recorder.get_output_path()),
                duration_secs: recorder.get_duration().as_secs(),
                event_count: recorder.get_event_count(),
            }
        } else {
            RecordingStatus {
                is_recording: false,
                is_paused: false,
                output_path: None,
                duration_secs: 0,
                event_count: 0,
            }
        }
    }

    // Record output from a pane (should be called when output is received)
    pub async fn record_output(&mut self, data: Vec<u8>) {
        if let Some(recorder) = &mut self.recorder {
            recorder.record_output(data).await.ok();
        }
    }

    // Record input to a pane (should be called when input is sent)
    pub async fn record_input(&mut self, data: Vec<u8>) {
        if let Some(recorder) = &mut self.recorder {
            recorder.record_input(data).await.ok();
        }
    }

    // Record terminal resize event
    pub async fn record_resize(&mut self, cols: u16, rows: u16) {
        if let Some(recorder) = &mut self.recorder {
            recorder.record_resize(cols, rows).await.ok();
        }
    }
}

// Format variable provider for Session
impl FormatProvider for Session {
    fn get_variable(&self, name: &str) -> Option<FormatValue> {
        match name {
            // Session identification
            "session_id" => Some(FormatValue::String(self.id.0.to_string())),
            "session_name" => Some(FormatValue::String(self.name.clone())),

            // Session state
            "session_created" => Some(FormatValue::Timestamp(self.created_at)),
            "session_attached" => {
                // TODO: Track attached clients count
                Some(FormatValue::Number(1))
            },
            "session_windows" => Some(FormatValue::Number(self.windows.len() as i64)),

            // Session flags
            "session_locked" => Some(FormatValue::Boolean(self.locked)),
            "pane_synchronized" => Some(FormatValue::Boolean(self.pane_sync_enabled)),

            // Recording status
            "session_recording" => Some(FormatValue::Boolean(self.recorder.is_some())),

            // Layout info
            "session_layout" => Some(FormatValue::String(
                self.current_layout_preset.clone().unwrap_or_else(|| "unknown".to_string())
            )),

            // Version control (if enabled)
            #[cfg(feature = "versioning")]
            "session_versioned" => Some(FormatValue::Boolean(self.versioning.is_some())),

            _ => None,
        }
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
}
