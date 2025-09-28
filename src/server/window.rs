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

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[tokio::test]
    async fn test_window_creation() {
        let window_id = WindowId(Uuid::new_v4());
        let window_name = "test_window".to_string();
        let window = Window::new(window_id.clone(), window_name.clone());

        assert_eq!(window.id, window_id);
        assert_eq!(window.name, window_name);
        assert_eq!(window.panes.len(), 1);
        assert!(window.current_pane.is_some());
        assert_eq!(window.width, 80);
        assert_eq!(window.height, 24);
    }

    #[tokio::test]
    async fn test_window_default_dimensions() {
        let window_id = WindowId(Uuid::new_v4());
        let window = Window::new(window_id, "test".to_string());

        assert_eq!(window.width, 80);
        assert_eq!(window.height, 24);
    }

    #[tokio::test]
    async fn test_window_split_pane_horizontal() {
        let window_id = WindowId(Uuid::new_v4());
        let mut window = Window::new(window_id, "test".to_string());

        let current_pane_id = window.current_pane.clone().unwrap();
        let result = window.split_pane(&current_pane_id, SplitDirection::Horizontal).await;

        assert!(result.is_ok());
        let new_pane_id = result.unwrap();
        assert_eq!(window.panes.len(), 2);
        assert!(window.panes.contains_key(&new_pane_id));
    }

    #[tokio::test]
    async fn test_window_split_pane_vertical() {
        let window_id = WindowId(Uuid::new_v4());
        let mut window = Window::new(window_id, "test".to_string());

        let current_pane_id = window.current_pane.clone().unwrap();
        let result = window.split_pane(&current_pane_id, SplitDirection::Vertical).await;

        assert!(result.is_ok());
        assert_eq!(window.panes.len(), 2);
    }

    #[tokio::test]
    async fn test_window_split_nonexistent_pane() {
        let window_id = WindowId(Uuid::new_v4());
        let mut window = Window::new(window_id, "test".to_string());

        let nonexistent_pane = PaneId(Uuid::new_v4());
        let result = window.split_pane(&nonexistent_pane, SplitDirection::Horizontal).await;

        assert!(result.is_err());
        assert_eq!(window.panes.len(), 1);
    }

    #[tokio::test]
    async fn test_window_close_pane() {
        let window_id = WindowId(Uuid::new_v4());
        let mut window = Window::new(window_id, "test".to_string());

        // First split to have multiple panes
        let current_pane_id = window.current_pane.clone().unwrap();
        let new_pane_id = window.split_pane(&current_pane_id, SplitDirection::Horizontal).await.unwrap();

        // Now close one pane
        let result = window.close_pane(&new_pane_id).await;
        assert!(result.is_ok());
        assert_eq!(window.panes.len(), 1);
        assert!(!window.panes.contains_key(&new_pane_id));
    }

    #[tokio::test]
    async fn test_window_close_last_pane() {
        let window_id = WindowId(Uuid::new_v4());
        let mut window = Window::new(window_id, "test".to_string());

        let current_pane_id = window.current_pane.clone().unwrap();
        let result = window.close_pane(&current_pane_id).await;

        // Should fail to close the last pane
        assert!(result.is_err());
        assert_eq!(window.panes.len(), 1);
    }

    #[tokio::test]
    async fn test_window_close_nonexistent_pane() {
        let window_id = WindowId(Uuid::new_v4());
        let mut window = Window::new(window_id, "test".to_string());

        let nonexistent_pane = PaneId(Uuid::new_v4());
        let result = window.close_pane(&nonexistent_pane).await;

        assert!(result.is_err());
        assert_eq!(window.panes.len(), 1);
    }

    #[tokio::test]
    async fn test_window_navigate_pane() {
        let window_id = WindowId(Uuid::new_v4());
        let mut window = Window::new(window_id, "test".to_string());

        // Split to have multiple panes
        let original_pane = window.current_pane.clone().unwrap();
        let new_pane_id = window.split_pane(&original_pane, SplitDirection::Horizontal).await.unwrap();

        // Navigation should work
        let result = window.navigate_pane(NavigationDirection::Down).await;
        assert!(result.is_ok());

        let result = window.navigate_pane(NavigationDirection::Up).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_window_handle_input() {
        let window_id = WindowId(Uuid::new_v4());
        let mut window = Window::new(window_id, "test".to_string());

        let test_input = b"test input".to_vec();
        let result = window.handle_input(test_input).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_window_handle_input_no_current_pane() {
        let window_id = WindowId(Uuid::new_v4());
        let mut window = Window::new(window_id, "test".to_string());

        // Clear current pane
        window.current_pane = None;

        let test_input = b"test input".to_vec();
        let result = window.handle_input(test_input).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_window_resize() {
        let window_id = WindowId(Uuid::new_v4());
        let mut window = Window::new(window_id, "test".to_string());

        let new_width = 120;
        let new_height = 40;

        let result = window.resize(new_width, new_height).await;
        assert!(result.is_ok());
        assert_eq!(window.width, new_width);
        assert_eq!(window.height, new_height);
    }

    #[tokio::test]
    async fn test_window_get_output() {
        let window_id = WindowId(Uuid::new_v4());
        let mut window = Window::new(window_id, "test".to_string());

        let result = window.get_output().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_window_get_all_pane_outputs() {
        let window_id = WindowId(Uuid::new_v4());
        let mut window = Window::new(window_id, "test".to_string());

        // Split to have multiple panes
        let current_pane_id = window.current_pane.clone().unwrap();
        window.split_pane(&current_pane_id, SplitDirection::Horizontal).await.unwrap();

        let result = window.get_all_pane_outputs().await;
        assert!(result.is_ok());

        let outputs = result.unwrap();
        // Should have at most 2 outputs (one for each pane, but they might be empty)
        assert!(outputs.len() <= 2);
    }

    #[tokio::test]
    async fn test_window_get_pane_count() {
        let window_id = WindowId(Uuid::new_v4());
        let window = Window::new(window_id, "test".to_string());

        assert_eq!(window.get_pane_count(), 1);
    }

    #[tokio::test]
    async fn test_window_get_focused_pane() {
        let window_id = WindowId(Uuid::new_v4());
        let window = Window::new(window_id, "test".to_string());

        let focused = window.get_focused_pane();
        assert!(focused.is_some());
        assert_eq!(focused, window.current_pane);
    }

    #[tokio::test]
    async fn test_window_toggle_zoom() {
        let window_id = WindowId(Uuid::new_v4());
        let mut window = Window::new(window_id, "test".to_string());

        let result = window.toggle_zoom().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_window_current_pane_update_on_close() {
        let window_id = WindowId(Uuid::new_v4());
        let mut window = Window::new(window_id, "test".to_string());

        let original_pane = window.current_pane.clone().unwrap();
        let new_pane_id = window.split_pane(&original_pane, SplitDirection::Horizontal).await.unwrap();

        // Set current pane to the new one
        window.current_pane = Some(new_pane_id.clone());

        // Close the current pane
        let result = window.close_pane(&new_pane_id).await;
        assert!(result.is_ok());

        // Current pane should be updated to something else
        assert!(window.current_pane.is_some());
        assert_ne!(window.current_pane, Some(new_pane_id));
    }

    #[tokio::test]
    async fn test_window_multiple_splits() {
        let window_id = WindowId(Uuid::new_v4());
        let mut window = Window::new(window_id, "test".to_string());

        let pane1 = window.current_pane.clone().unwrap();
        let pane2 = window.split_pane(&pane1, SplitDirection::Horizontal).await.unwrap();
        let pane3 = window.split_pane(&pane2, SplitDirection::Vertical).await.unwrap();

        assert_eq!(window.panes.len(), 3);
        assert!(window.panes.contains_key(&pane1));
        assert!(window.panes.contains_key(&pane2));
        assert!(window.panes.contains_key(&pane3));
    }
}