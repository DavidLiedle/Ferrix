use crate::error::Result;
use crate::protocol::PaneId;
use super::pty::Pty;

pub struct Pane {
    pub id: PaneId,
    pub pty: Option<Pty>,
    pub cols: u16,
    pub rows: u16,
}

impl Pane {
    pub fn new(id: PaneId) -> Self {
        let mut pane = Self {
            id,
            pty: None,
            cols: 80,
            rows: 24,
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