use crate::error::Result;
use crate::protocol::PaneId;
use crate::format::{FormatProvider, FormatValue};
use super::pty::Pty;
use super::scrollback::LineScrollback;
use std::path::PathBuf;

pub struct Pane {
    pub id: PaneId,
    pub pty: Option<Pty>,
    pub cols: u16,
    pub rows: u16,
    pub working_directory: PathBuf,
    pub command: String,
    pub scrollback: LineScrollback,
    pub cursor_position: (u16, u16),
    pub remain_on_exit: bool,
    pub exit_status: Option<i32>,
    pub is_dead: bool,
    /// Raw PTY output buffer for session persistence
    /// Stores the last N bytes of raw PTY output to replay when clients attach
    pub raw_output_buffer: Vec<u8>,
    pub max_raw_buffer_size: usize,
    /// Scroll position in the scrollback buffer (0 = viewing current output)
    /// Positive values indicate scrolling up into history
    pub scroll_position: usize,
}

impl Pane {
    pub fn new(id: PaneId) -> Self {
        // Default constructor for backward compatibility
        use crate::config::limits::ResourceLimits;
        let limits = ResourceLimits::default();
        Self::new_with_limits(id, &limits, std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")))
    }

    pub fn new_with_config(id: PaneId, scrollback_lines: usize) -> Self {
        Self::new_with_working_dir(id, scrollback_lines, 50000, std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")))
    }

    pub fn new_with_limits(id: PaneId, limits: &crate::config::limits::ResourceLimits, working_dir: PathBuf) -> Self {
        Self::new_with_working_dir(id, limits.max_scrollback_lines, limits.max_raw_buffer_bytes, working_dir)
    }

    pub fn new_with_working_dir(id: PaneId, scrollback_lines: usize, max_raw_buffer_bytes: usize, working_dir: PathBuf) -> Self {
        let mut pane = Self {
            id,
            pty: None,
            cols: 80,
            rows: 24,
            working_directory: working_dir,
            command: std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string()),
            scrollback: LineScrollback::new(scrollback_lines),
            cursor_position: (0, 0),
            remain_on_exit: false,
            exit_status: None,
            is_dead: false,
            raw_output_buffer: Vec::new(),
            max_raw_buffer_size: max_raw_buffer_bytes,
            scroll_position: 0, // Start at current output (not scrolled)
        };

        if let Err(e) = pane.start_pty() {
            tracing::error!("Failed to start PTY: {}", e);
        }

        pane
    }

    fn start_pty(&mut self) -> Result<()> {
        self.pty = Some(Pty::new_with_cwd(self.cols, self.rows, self.working_directory.clone())?);
        Ok(())
    }

    pub async fn handle_input(&mut self, data: Vec<u8>) -> Result<()> {
        if let Some(pty) = &mut self.pty {
            pty.write(data).await?;
        }
        Ok(())
    }

    pub async fn resize(&mut self, cols: u16, rows: u16) -> Result<()> {
        self.cols = cols;
        self.rows = rows;
        if let Some(pty) = &mut self.pty {
            pty.resize(cols, rows).await?;
        }
        Ok(())
    }

    pub async fn get_output(&mut self) -> Result<Option<Vec<u8>>> {
        if let Some(pty) = &mut self.pty {
            return pty.read().await;
        }
        Ok(None)
    }

    /// Append raw output to the buffer for session persistence
    /// Keeps only the last max_raw_buffer_size bytes
    pub fn append_raw_output(&mut self, data: &[u8]) {
        self.raw_output_buffer.extend_from_slice(data);

        // Trim buffer if it exceeds max size
        if self.raw_output_buffer.len() > self.max_raw_buffer_size {
            let excess = self.raw_output_buffer.len() - self.max_raw_buffer_size;
            self.raw_output_buffer.drain(0..excess);
        }
    }

    /// Get the raw output buffer for replaying to newly attached clients
    pub fn get_raw_output_buffer(&self) -> &[u8] {
        &self.raw_output_buffer
    }

    /// Mark the pane as dead (PTY has exited)
    pub fn mark_dead(&mut self, exit_status: Option<i32>) {
        self.is_dead = true;
        self.exit_status = exit_status;
        self.pty = None;
    }

    /// Check if the pane is dead (PTY has exited)
    pub fn is_dead(&self) -> bool {
        self.is_dead
    }

    /// Respawn the pane (restart the PTY)
    pub fn respawn(&mut self) -> Result<()> {
        tracing::info!("Respawning pane {}", self.id.0);
        self.is_dead = false;
        self.exit_status = None;
        self.start_pty()?;
        Ok(())
    }

    /// Enable or disable remain-on-exit
    pub fn set_remain_on_exit(&mut self, remain: bool) {
        self.remain_on_exit = remain;
    }
}

impl Drop for Pane {
    fn drop(&mut self) {
        tracing::debug!("Dropping pane {}", self.id.0);
        // PTY will be dropped automatically and its Drop will handle cleanup
    }
}

// Format variable provider for Pane
impl FormatProvider for Pane {
    fn get_variable(&self, name: &str) -> Option<FormatValue> {
        match name {
            // Pane identification
            "pane_id" => Some(FormatValue::String(self.id.0.to_string())),

            // Pane size
            "pane_width" => Some(FormatValue::Number(self.cols as i64)),
            "pane_height" => Some(FormatValue::Number(self.rows as i64)),

            // Pane state
            "pane_current_command" => Some(FormatValue::String(self.command.clone())),
            "pane_current_path" => Some(FormatValue::String(
                self.working_directory.display().to_string()
            )),

            // PTY status
            "pane_pid" => {
                self.pty.as_ref()
                    .and_then(|pty| pty.get_child_pid())
                    .map(|pid| FormatValue::Number(pid as i64))
                    .or(Some(FormatValue::Number(0)))
            },
            "pane_active" => {
                // NOTE: Cannot determine if this is the active pane because Pane
                // doesn't have access to the window's current_pane field. Pane
                // is accessed from Window, so Window knows which is active, but
                // Pane itself doesn't have that context. Returning true as a
                // reasonable default for formatting purposes.
                Some(FormatValue::Boolean(true))
            },
            "pane_dead" => Some(FormatValue::Boolean(self.pty.is_none())),

            // Cursor information
            "cursor_x" => Some(FormatValue::Number(self.cursor_position.0 as i64)),
            "cursor_y" => Some(FormatValue::Number(self.cursor_position.1 as i64)),

            // Scrollback
            "scroll_position" => Some(FormatValue::Number(self.scroll_position as i64)),
            "history_size" => Some(FormatValue::Number(
                self.scrollback.max_lines() as i64
            )),
            "history_bytes" => Some(FormatValue::Number(
                self.scrollback.memory_usage() as i64
            )),

            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[tokio::test]
    async fn test_pane_creation() {
        let pane_id = PaneId(Uuid::new_v4());
        let pane = Pane::new(pane_id.clone());

        assert_eq!(pane.id, pane_id);
        assert_eq!(pane.cols, 80);
        assert_eq!(pane.rows, 24);
        assert_eq!(pane.cursor_position, (0, 0));
        assert!(pane.scrollback.is_empty());
        assert!(!pane.working_directory.as_os_str().is_empty());
        assert!(!pane.command.is_empty());
    }

    #[tokio::test]
    async fn test_pane_default_properties() {
        let pane_id = PaneId(Uuid::new_v4());
        let pane = Pane::new(pane_id);

        // Default shell should be set
        let expected_shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
        assert_eq!(pane.command, expected_shell);

        // Working directory should be valid
        assert!(pane.working_directory.exists() || pane.working_directory == PathBuf::from("/"));

        // Default dimensions
        assert_eq!(pane.cols, 80);
        assert_eq!(pane.rows, 24);
    }

    #[tokio::test]
    async fn test_pane_resize() {
        let pane_id = PaneId(Uuid::new_v4());
        let mut pane = Pane::new(pane_id);

        let new_cols = 120;
        let new_rows = 30;

        let result = pane.resize(new_cols, new_rows).await;

        assert!(result.is_ok());
        assert_eq!(pane.cols, new_cols);
        assert_eq!(pane.rows, new_rows);
    }

    #[tokio::test]
    async fn test_pane_handle_input_without_pty() {
        let pane_id = PaneId(Uuid::new_v4());
        let mut pane = Pane::new(pane_id);

        // Force pty to None to test the no-pty case
        pane.pty = None;

        let test_input = b"test input".to_vec();
        let result = pane.handle_input(test_input).await;

        // Should succeed even without PTY
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_pane_get_output_without_pty() {
        let pane_id = PaneId(Uuid::new_v4());
        let mut pane = Pane::new(pane_id);

        // Force pty to None to test the no-pty case
        pane.pty = None;

        let result = pane.get_output().await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None);
    }

    #[tokio::test]
    async fn test_pane_scrollback_initialization() {
        let pane_id = PaneId(Uuid::new_v4());
        let pane = Pane::new(pane_id);

        assert!(pane.scrollback.is_empty());
        assert_eq!(pane.scrollback.len(), 0);
    }

    #[tokio::test]
    async fn test_pane_cursor_position_initialization() {
        let pane_id = PaneId(Uuid::new_v4());
        let pane = Pane::new(pane_id);

        assert_eq!(pane.cursor_position.0, 0);
        assert_eq!(pane.cursor_position.1, 0);
    }

    #[tokio::test]
    async fn test_pane_id_uniqueness() {
        let pane_id1 = PaneId(Uuid::new_v4());
        let pane_id2 = PaneId(Uuid::new_v4());

        let pane1 = Pane::new(pane_id1.clone());
        let pane2 = Pane::new(pane_id2.clone());

        assert_ne!(pane1.id, pane2.id);
        assert_eq!(pane1.id, pane_id1);
        assert_eq!(pane2.id, pane_id2);
    }
}