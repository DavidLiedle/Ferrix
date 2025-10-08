use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::error::Result;
use crate::protocol::{WindowId, PaneId, SplitDirection};
use crate::format::{FormatProvider, FormatValue};
use super::pane::Pane;
use super::layout::{Layout, NavigationDirection};
use super::activity::{ActivityMonitor, ActivityType};

pub struct Window {
    pub id: WindowId,
    pub name: String,
    pub panes: HashMap<PaneId, Arc<RwLock<Pane>>>,
    pub current_pane: Option<PaneId>,
    pub last_pane: Option<PaneId>,
    pub layout: Layout,
    pub width: u16,
    pub height: u16,
    pub zoomed_pane: Option<PaneId>,
    pub activity_monitor: ActivityMonitor,
    /// Ordered list of pane IDs for indexing (index 0 = first pane, etc.)
    pub pane_order: Vec<PaneId>,
}

impl Window {
    pub fn apply_preset_layout(&mut self, preset: crate::server::layout_presets::LayoutPreset) {
        // Clear existing panes (except the first one)
        let first_pane_id = if let Some(id) = &self.current_pane {
            id.clone()
        } else if let Some(first_pane) = self.panes.keys().next() {
            first_pane.clone()
        } else {
            // Create a new pane if no panes exist
            let pane_id = PaneId(Uuid::new_v4());
            let pane = Pane::new(pane_id.clone());
            self.panes.insert(pane_id.clone(), Arc::new(RwLock::new(pane)));
            pane_id
        };

        // Clear all panes except the first one
        let mut first_pane = None;
        if let Some(pane) = self.panes.remove(&first_pane_id) {
            first_pane = Some((first_pane_id.clone(), pane));
        }
        self.panes.clear();

        // Restore the first pane
        if let Some((id, pane)) = first_pane {
            self.panes.insert(id.clone(), pane);
            self.current_pane = Some(id);
        }

        // Apply the new layout
        let mut new_layout = preset.to_layout();

        // Replace the pane IDs in the layout with actual panes
        self.populate_layout_with_panes(&mut new_layout);

        self.layout = new_layout;

        // Clear zoom when applying a new layout
        self.zoomed_pane = None;
    }

    fn populate_layout_with_panes(&mut self, layout: &mut Layout) {
        match layout {
            Layout::Leaf(pane_id) => {
                // If this pane doesn't exist, create it
                if !self.panes.contains_key(pane_id) {
                    let pane = Pane::new(pane_id.clone());
                    self.panes.insert(pane_id.clone(), Arc::new(RwLock::new(pane)));
                    self.activity_monitor.enable_monitoring(pane_id);
                }
            }
            Layout::Split { first, second, .. } => {
                self.populate_layout_with_panes(first);
                self.populate_layout_with_panes(second);
            }
        }
    }

    pub fn new(id: WindowId, name: String) -> Self {
        Self::new_with_working_dir(id, name, std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("/")))
    }

    pub fn new_with_working_dir(id: WindowId, name: String, working_dir: std::path::PathBuf) -> Self {
        let pane_id = PaneId(Uuid::new_v4());
        let default_pane = Pane::new_with_working_dir(pane_id.clone(), 10000, working_dir);

        let mut panes = HashMap::new();
        panes.insert(pane_id.clone(), Arc::new(RwLock::new(default_pane)));

        let mut activity_monitor = ActivityMonitor::new();
        activity_monitor.enable_monitoring(&pane_id);

        Self {
            id,
            name,
            panes,
            current_pane: Some(pane_id.clone()),
            last_pane: None,
            layout: Layout::new(pane_id.clone()),
            width: 80,
            height: 24,
            zoomed_pane: None,
            activity_monitor,
            pane_order: vec![pane_id],
        }
    }

    pub async fn split_pane(&mut self, pane_id: &PaneId, direction: SplitDirection) -> Result<PaneId> {
        let new_pane_id = PaneId(Uuid::new_v4());
        let new_pane = Pane::new(new_pane_id.clone());

        self.panes.insert(new_pane_id.clone(), Arc::new(RwLock::new(new_pane)));

        if self.layout.split(pane_id, direction, new_pane_id.clone()) {
            // Enable activity monitoring for the new pane
            self.activity_monitor.enable_monitoring(&new_pane_id);
            // Add new pane to the ordered list
            self.pane_order.push(new_pane_id.clone());
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

            // Clean up activity monitoring for the closed pane
            self.activity_monitor.cleanup_pane(pane_id);

            // Remove from pane order
            self.pane_order.retain(|id| id != pane_id);

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
                // Save current pane as last_pane before switching
                self.last_pane = self.current_pane.clone();
                self.current_pane = Some(new_pane_id);
            }
        }
        Ok(())
    }

    /// Toggle between current and last pane (like tmux's last-pane)
    pub async fn select_last_pane(&mut self) -> Result<()> {
        if let Some(last) = &self.last_pane {
            // Verify the last pane still exists
            if self.panes.contains_key(last) {
                // Swap current and last
                let temp = self.current_pane.clone();
                self.current_pane = Some(last.clone());
                self.last_pane = temp;
            } else {
                // Last pane was closed, clear it
                self.last_pane = None;
            }
        }
        Ok(())
    }

    /// Select a pane by its index (0-based)
    pub fn select_pane_by_index(&mut self, index: usize) -> Result<()> {
        if index < self.pane_order.len() {
            let pane_id = self.pane_order[index].clone();
            // Save current as last before switching
            self.last_pane = self.current_pane.clone();
            self.current_pane = Some(pane_id);
            Ok(())
        } else {
            Err(crate::error::FerrixError::Other(format!("Pane index {} out of range", index)))
        }
    }

    /// Get the index of a pane (returns None if pane not found)
    pub fn get_pane_index(&self, pane_id: &PaneId) -> Option<usize> {
        self.pane_order.iter().position(|id| id == pane_id)
    }

    /// Get all pane IDs with their indices
    pub fn get_pane_indices(&self) -> Vec<(usize, PaneId)> {
        self.pane_order.iter().enumerate().map(|(i, id)| (i, id.clone())).collect()
    }

    /// Respawn a pane (restart its PTY)
    pub async fn respawn_pane(&mut self, pane_id: &PaneId) -> Result<()> {
        if let Some(pane) = self.panes.get(pane_id) {
            let mut pane_guard = pane.write().await;
            let cols = pane_guard.cols;
            let rows = pane_guard.rows;
            pane_guard.respawn()?;
            // Resize the pane to current dimensions
            pane_guard.resize(cols, rows).await?;
            Ok(())
        } else {
            Err(crate::error::FerrixError::PaneNotFound(format!("{:?}", pane_id)))
        }
    }

    async fn update_pane_dimensions(&mut self) -> Result<()> {
        if let Some(zoomed_pane_id) = &self.zoomed_pane {
            // When zoomed, the selected pane takes up the entire window
            if let Some(pane) = self.panes.get(zoomed_pane_id) {
                let mut pane_guard = pane.write().await;
                pane_guard.resize(self.width, self.height).await?;
            }
        } else {
            // Normal layout - use the layout manager
            let dimensions = self.layout.get_dimensions(self.width, self.height);

            for (pane_id, _x, _y, width, height) in dimensions {
                if let Some(pane) = self.panes.get(&pane_id) {
                    let mut pane_guard = pane.write().await;
                    pane_guard.resize(width, height).await?;
                }
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

    pub async fn handle_input_broadcast(&mut self, data: Vec<u8>) -> Result<()> {
        // Send input to all panes in this window
        for pane in self.panes.values() {
            let mut pane_guard = pane.write().await;
            pane_guard.handle_input(data.clone()).await?;
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
            match pane_guard.get_output().await {
                Ok(Some(data)) => {
                    if !data.is_empty() {
                        // Store raw output for session persistence
                        pane_guard.append_raw_output(&data);

                        // Record activity for panes that have output
                        // Only record if it's not the current pane (focused pane)
                        if self.current_pane.as_ref() != Some(pane_id) {
                            self.activity_monitor.record_activity(pane_id, ActivityType::Output);
                        }
                        outputs.push((pane_id.clone(), data));
                    }
                }
                Ok(None) => {
                    // No data available, pane is still alive
                }
                Err(_) => {
                    // PTY error (usually means process died), mark pane as dead
                    if !pane_guard.is_dead() {
                        tracing::info!("Pane {} PTY died, marking as dead", pane_id.0);
                        pane_guard.mark_dead(None);
                    }
                }
            }
        }

        // Mark current pane as seen since user is viewing it
        if let Some(current_pane) = &self.current_pane {
            self.activity_monitor.mark_as_seen(current_pane);
        }

        Ok(outputs)
    }

    pub fn get_pane_count(&self) -> usize {
        self.panes.len()
    }

    pub fn get_focused_pane(&self) -> Option<PaneId> {
        self.current_pane.clone()
    }

    /// Check if all panes in this window are dead
    pub async fn are_all_panes_dead(&self) -> bool {
        if self.panes.is_empty() {
            return true;
        }

        for pane in self.panes.values() {
            let pane_guard = pane.read().await;
            if !pane_guard.is_dead() {
                return false;
            }
        }
        true
    }

    pub async fn toggle_zoom(&mut self) -> Result<bool> {
        if self.zoomed_pane.is_some() {
            self.unzoom_pane().await?;
            Ok(false) // Not zoomed anymore
        } else if let Some(current_pane) = self.current_pane.clone() {
            self.zoom_pane(&current_pane).await?;
            Ok(true) // Now zoomed
        } else {
            Ok(false) // No current pane to zoom
        }
    }

    pub async fn zoom_pane(&mut self, pane_id: &PaneId) -> Result<()> {
        if self.panes.contains_key(pane_id) {
            self.zoomed_pane = Some(pane_id.clone());
            self.update_pane_dimensions().await?;
        }
        Ok(())
    }

    pub async fn unzoom_pane(&mut self) -> Result<()> {
        self.zoomed_pane = None;
        self.update_pane_dimensions().await
    }

    pub fn is_zoomed(&self) -> bool {
        self.zoomed_pane.is_some()
    }

    pub fn get_zoomed_pane(&self) -> Option<PaneId> {
        self.zoomed_pane.clone()
    }

    pub fn rename(&mut self, new_name: String) {
        self.name = new_name;
    }

    // Activity monitoring methods
    pub fn record_activity(&mut self, pane_id: &PaneId, activity_type: ActivityType) {
        // Record activity for the pane
        self.activity_monitor.record_activity(pane_id, activity_type);
    }

    pub fn mark_pane_as_seen(&mut self, pane_id: &PaneId) {
        self.activity_monitor.mark_as_seen(pane_id);
    }

    pub fn get_activity_status(&self, pane_id: &PaneId) -> Option<String> {
        self.activity_monitor.get_activity_status(pane_id)
    }

    pub fn get_window_activity_summary(&self) -> Option<String> {
        // Check if any pane has activity
        let mut has_bell = false;
        let mut has_activity = false;
        let mut has_silence = false;

        for pane_id in self.panes.keys() {
            if self.activity_monitor.has_bell(pane_id) {
                has_bell = true;
                break;
            }
            if self.activity_monitor.has_unseen_activity(pane_id) {
                has_activity = true;
            }
            if self.activity_monitor.check_for_silence(pane_id) {
                has_silence = true;
            }
        }

        if has_bell {
            return Some("🔔".to_string());
        }
        if has_activity {
            return Some("●".to_string());
        }
        if has_silence {
            return Some("○".to_string());
        }

        None
    }

    pub fn enable_activity_monitoring(&mut self, pane_id: &PaneId) {
        self.activity_monitor.enable_monitoring(pane_id);
    }

    pub fn disable_activity_monitoring(&mut self, pane_id: &PaneId) {
        self.activity_monitor.disable_monitoring(pane_id);
    }

    pub fn toggle_activity_monitoring(&mut self, pane_id: &PaneId) -> bool {
        if self.activity_monitor.is_monitoring_enabled(pane_id) {
            self.activity_monitor.disable_monitoring(pane_id);
            false
        } else {
            self.activity_monitor.enable_monitoring(pane_id);
            true
        }
    }
}

// Format variable provider for Window
impl FormatProvider for Window {
    fn get_variable(&self, name: &str) -> Option<FormatValue> {
        match name {
            // Window identification
            "window_id" => Some(FormatValue::String(self.id.0.to_string())),
            "window_name" => Some(FormatValue::String(self.name.clone())),

            // Window state
            "window_width" => Some(FormatValue::Number(self.width as i64)),
            "window_height" => Some(FormatValue::Number(self.height as i64)),
            "window_panes" => Some(FormatValue::Number(self.panes.len() as i64)),

            // Window flags
            "window_zoomed_flag" => Some(FormatValue::Boolean(self.zoomed_pane.is_some())),
            "window_active" => {
                // TODO: Track if this is the active window
                Some(FormatValue::Boolean(true))
            },

            // Layout
            "window_layout" => Some(FormatValue::String(
                format!("{:?}", self.layout)
            )),

            // Activity monitoring (check any pane has unseen activity)
            "window_activity_flag" => Some(FormatValue::Boolean(
                self.panes.keys().any(|pane_id| self.activity_monitor.has_unseen_activity(pane_id))
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
        assert_eq!(window.zoomed_pane, None);
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

        // Test zoom toggle - should zoom in
        let result = window.toggle_zoom().await;
        assert!(result.is_ok());
        assert!(result.unwrap()); // Should return true (zoomed)
        assert!(window.is_zoomed());

        // Test zoom toggle again - should zoom out
        let result = window.toggle_zoom().await;
        assert!(result.is_ok());
        assert!(!result.unwrap()); // Should return false (not zoomed)
        assert!(!window.is_zoomed());
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

    #[tokio::test]
    async fn test_window_zoom_specific_pane() {
        let window_id = WindowId(Uuid::new_v4());
        let mut window = Window::new(window_id, "test".to_string());

        // Split to have multiple panes
        let pane1 = window.current_pane.clone().unwrap();
        let pane2 = window.split_pane(&pane1, SplitDirection::Horizontal).await.unwrap();

        // Zoom specific pane
        let result = window.zoom_pane(&pane2).await;
        assert!(result.is_ok());
        assert!(window.is_zoomed());
        assert_eq!(window.get_zoomed_pane(), Some(pane2));

        // Unzoom
        let result = window.unzoom_pane().await;
        assert!(result.is_ok());
        assert!(!window.is_zoomed());
        assert_eq!(window.get_zoomed_pane(), None);
    }

    #[tokio::test]
    async fn test_window_zoom_nonexistent_pane() {
        let window_id = WindowId(Uuid::new_v4());
        let mut window = Window::new(window_id, "test".to_string());

        let nonexistent_pane = PaneId(Uuid::new_v4());
        let result = window.zoom_pane(&nonexistent_pane).await;
        assert!(result.is_ok()); // Should not error, but should not zoom
        assert!(!window.is_zoomed());
    }
}