use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::error::Result;
use crate::protocol::{WindowId, PaneId};
use super::pane::Pane;

pub struct Window {
    pub id: WindowId,
    pub name: String,
    pub panes: Vec<Arc<RwLock<Pane>>>,
    pub current_pane: Option<PaneId>,
}

impl Window {
    pub fn new(id: WindowId, name: String) -> Self {
        let pane_id = PaneId(Uuid::new_v4());
        let default_pane = Pane::new(pane_id.clone());

        Self {
            id,
            name,
            panes: vec![Arc::new(RwLock::new(default_pane))],
            current_pane: Some(pane_id),
        }
    }

    pub async fn handle_input(&mut self, data: Vec<u8>) -> Result<()> {
        if let Some(current_pane_id) = &self.current_pane {
            for pane in &self.panes {
                let pane_guard = pane.read().await;
                if pane_guard.id == *current_pane_id {
                    drop(pane_guard);
                    let mut pane_guard = pane.write().await;
                    pane_guard.handle_input(data).await?;
                    break;
                }
            }
        }
        Ok(())
    }

    pub async fn resize(&mut self, cols: u16, rows: u16) -> Result<()> {
        if let Some(current_pane_id) = &self.current_pane {
            for pane in &self.panes {
                let pane_guard = pane.read().await;
                if pane_guard.id == *current_pane_id {
                    drop(pane_guard);
                    let mut pane_guard = pane.write().await;
                    pane_guard.resize(cols, rows).await?;
                    break;
                }
            }
        }
        Ok(())
    }

    pub async fn get_output(&mut self) -> Result<Option<Vec<u8>>> {
        if let Some(current_pane_id) = &self.current_pane {
            for pane in &self.panes {
                let pane_guard = pane.read().await;
                if pane_guard.id == *current_pane_id {
                    drop(pane_guard);
                    let mut pane_guard = pane.write().await;
                    return pane_guard.get_output().await;
                }
            }
        }
        Ok(None)
    }
}