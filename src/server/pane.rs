use crate::error::Result;
use crate::protocol::PaneId;
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
}

impl Pane {
    pub fn new(id: PaneId) -> Self {
        // Default constructor for backward compatibility
        Self::new_with_config(id, 10000) // Default 10k lines
    }

    pub fn new_with_config(id: PaneId, scrollback_lines: usize) -> Self {
        let mut pane = Self {
            id,
            pty: None,
            cols: 80,
            rows: 24,
            working_directory: std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")),
            command: std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string()),
            scrollback: LineScrollback::new(scrollback_lines),
            cursor_position: (0, 0),
        };

        if let Err(e) = pane.start_pty() {
            tracing::error!("Failed to start PTY: {}", e);
        }

        pane
    }

    fn start_pty(&mut self) -> Result<()> {
        self.pty = Some(Pty::new(self.cols, self.rows)?);
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
}

impl Drop for Pane {
    fn drop(&mut self) {
        tracing::debug!("Dropping pane {}", self.id.0);
        // PTY will be dropped automatically and its Drop will handle cleanup
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
        assert_eq!(pane.scrollback.capacity(), 0);
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