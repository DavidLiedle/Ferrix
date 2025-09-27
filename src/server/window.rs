use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::error::Result;
use crate::protocol::{WindowId, PaneId, SplitDirection};
use super::pane::Pane;
use super::layout::{Layout, NavigationDirection};

pub struct Window {
    pub id: WindowId,
    pub name: String,
    pub panes: HashMap<PaneId, Arc<RwLock<Pane>>>,
    pub current_pane: Option<PaneId>,
    pub layout: Layout,
    pub width: u16,
    pub height: u16,
}

impl Window {
    pub fn new(id: WindowId, name: String) -> Self {
        let pane_id = PaneId(Uuid::new_v4());
        let default_pane = Pane::new(pane_id.clone());

        let mut panes = HashMap::new();
        panes.insert(pane_id.clone(), Arc::new(RwLock::new(default_pane)));

        Self {
            id,
            name,
            panes,
            current_pane: Some(pane_id.clone()),
            layout: Layout::new(pane_id),
            width: 80,
            height: 24,
        }
    }

    pub async fn split_pane(&mut self, pane_id: &PaneId, direction: SplitDirection) -> Result<PaneId> {
        let new_pane_id = PaneId(Uuid::new_v4());
        let new_pane = Pane::new(new_pane_id.clone());

        self.panes.insert(new_pane_id.clone(), Arc::new(RwLock::new(new_pane)));

        if self.layout.split(pane_id, direction, new_pane_id.clone()) {
            self.update_pane_dimensions().await?;
            Ok(new_pane_id)
        } else {
            self.panes.remove(&new_pane_id);
            Err(crate::error::FerrixError::PaneNotFound(format!("{:?}", pane_id)))
        }
    }

    pub async fn close_pane(&mut self, pane_id: &PaneId) -> Result<()> {
        if self.panes.len() <= 1 {
            return Err(crate::error::FerrixError::Other("Cannot close last pane".to_string()));
        }

        if self.layout.remove_pane(pane_id) {
            self.panes.remove(pane_id);

            // Update current pane if needed
            if self.current_pane.as_ref() == Some(pane_id) {
                self.current_pane = self.layout.get_all_panes().first().cloned();
            }

            self.update_pane_dimensions().await?;
            Ok(())
        } else {
            Err(crate::error::FerrixError::PaneNotFound(format!("{:?}", pane_id)))
        }
    }

    pub async fn navigate_pane(&mut self, direction: NavigationDirection) -> Result<()> {
        if let Some(current) = &self.current_pane {
            if let Some(new_pane_id) = self.layout.navigate(current, direction) {
                self.current_pane = Some(new_pane_id);
            }
        }
        Ok(())
    }

    async fn update_pane_dimensions(&mut self) -> Result<()> {
        let dimensions = self.layout.get_dimensions(self.width, self.height);

        for (pane_id, _x, _y, width, height) in dimensions {
            if let Some(pane) = self.panes.get(&pane_id) {
                let mut pane_guard = pane.write().await;
                pane_guard.resize(width, height).await?;
            }
        }

        Ok(())
    }

    pub async fn handle_input(&mut self, data: Vec<u8>) -> Result<()> {
        if let Some(current_pane_id) = &self.current_pane {
            if let Some(pane) = self.panes.get(current_pane_id) {
                let mut pane_guard = pane.write().await;
                pane_guard.handle_input(data).await?;
            }
        }
        Ok(())
    }

    pub async fn resize(&mut self, cols: u16, rows: u16) -> Result<()> {
        self.width = cols;
        self.height = rows;
        self.update_pane_dimensions().await
    }

    pub async fn get_output(&mut self) -> Result<Option<Vec<u8>>> {
        if let Some(current_pane_id) = &self.current_pane {
            if let Some(pane) = self.panes.get(current_pane_id) {
                let mut pane_guard = pane.write().await;
                return pane_guard.get_output().await;
            }
        }
        Ok(None)
    }

    pub async fn get_all_pane_outputs(&mut self) -> Result<Vec<(PaneId, Vec<u8>)>> {
        let mut outputs = Vec::new();

        for (pane_id, pane) in &self.panes {
            let mut pane_guard = pane.write().await;
            if let Some(data) = pane_guard.get_output().await? {
                if !data.is_empty() {
                    outputs.push((pane_id.clone(), data));
                }
            }
        }

        Ok(outputs)
    }

    pub fn get_pane_count(&self) -> usize {
        self.panes.len()
    }

    pub fn get_focused_pane(&self) -> Option<PaneId> {
        self.current_pane.clone()
    }

    pub async fn toggle_zoom(&mut self) -> Result<()> {
        self.layout.toggle_zoom();
        self.update_pane_dimensions().await
    }
}