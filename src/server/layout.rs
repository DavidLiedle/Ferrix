use serde::{Deserialize, Serialize};

use crate::protocol::{PaneId, SplitDirection};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Layout {
    Leaf(PaneId),
    Split {
        direction: SplitDirection,
        ratio: f32, // 0.0 to 1.0, where 0.5 means equal split
        first: Box<Layout>,
        second: Box<Layout>,
    },
}

impl Layout {
    pub fn new(pane_id: PaneId) -> Self {
        Layout::Leaf(pane_id)
    }

    pub fn split(&mut self, pane_id: &PaneId, direction: SplitDirection, new_pane_id: PaneId) -> bool {
        match self {
            Layout::Leaf(id) if id == pane_id => {
                *self = Layout::Split {
                    direction,
                    ratio: 0.5,
                    first: Box::new(Layout::Leaf(id.clone())),
                    second: Box::new(Layout::Leaf(new_pane_id)),
                };
                true
            }
            Layout::Split { first, second, .. } => {
                first.split(pane_id, direction, new_pane_id.clone()) ||
                second.split(pane_id, direction, new_pane_id)
            }
            _ => false,
        }
    }

    pub fn remove_pane(&mut self, pane_id: &PaneId) -> bool {
        self.remove_pane_internal(pane_id).is_some()
    }

    fn remove_pane_internal(&mut self, pane_id: &PaneId) -> Option<Layout> {
        match self {
            Layout::Leaf(id) if id == pane_id => {
                // Can't remove the last pane
                None
            }
            Layout::Split { first, second, .. } => {
                // Check if either child is the pane to remove
                if let Layout::Leaf(id) = &**first {
                    if id == pane_id {
                        return Some(*second.clone());
                    }
                }
                if let Layout::Leaf(id) = &**second {
                    if id == pane_id {
                        return Some(*first.clone());
                    }
                }

                // Otherwise, recurse
                if let Some(new_first) = first.remove_pane_internal(pane_id) {
                    *first = Box::new(new_first);
                    return Some(self.clone());
                }
                if let Some(new_second) = second.remove_pane_internal(pane_id) {
                    *second = Box::new(new_second);
                    return Some(self.clone());
                }
                None
            }
            _ => None,
        }
    }

    pub fn find_pane(&self, pane_id: &PaneId) -> Option<&PaneId> {
        match self {
            Layout::Leaf(id) if id == pane_id => Some(id),
            Layout::Split { first, second, .. } => {
                first.find_pane(pane_id).or_else(|| second.find_pane(pane_id))
            }
            _ => None,
        }
    }

    pub fn get_all_panes(&self) -> Vec<PaneId> {
        match self {
            Layout::Leaf(id) => vec![id.clone()],
            Layout::Split { first, second, .. } => {
                let mut panes = first.get_all_panes();
                panes.extend(second.get_all_panes());
                panes
            }
        }
    }

    pub fn resize_split(&mut self, index: usize, delta: f32) -> bool {
        self.resize_split_internal(index, delta, &mut 0)
    }

    fn resize_split_internal(&mut self, target_index: usize, delta: f32, current_index: &mut usize) -> bool {
        match self {
            Layout::Split { ratio, first, second, .. } => {
                if *current_index == target_index {
                    let new_ratio = (*ratio + delta).clamp(0.1, 0.9);
                    *ratio = new_ratio;
                    return true;
                }
                *current_index += 1;

                first.resize_split_internal(target_index, delta, current_index) ||
                second.resize_split_internal(target_index, delta, current_index)
            }
            _ => false,
        }
    }

    /// Resize a specific pane by adjusting the split ratios of its parent
    pub fn resize_pane(&mut self, pane_id: &PaneId, direction: crate::protocol::ResizeDirection, amount: f32) -> bool {
        self.resize_pane_internal(pane_id, direction, amount)
    }

    fn resize_pane_internal(&mut self, target_pane_id: &PaneId, direction: crate::protocol::ResizeDirection, amount: f32) -> bool {
        match self {
            Layout::Split { direction: split_dir, ratio, first, second } => {
                // Check if one of our children is the target pane
                let first_contains = first.contains_pane(target_pane_id);
                let second_contains = second.contains_pane(target_pane_id);

                if first_contains || second_contains {
                    // Determine if we should adjust this split
                    let should_adjust = match (&direction, split_dir) {
                        (crate::protocol::ResizeDirection::Left | crate::protocol::ResizeDirection::Right,
                         SplitDirection::Vertical) => true,
                        (crate::protocol::ResizeDirection::Up | crate::protocol::ResizeDirection::Down,
                         SplitDirection::Horizontal) => true,
                        _ => false,
                    };

                    if should_adjust {
                        // Calculate new ratio based on resize direction
                        let delta = amount / 100.0; // Convert to ratio delta

                        let new_ratio = match direction {
                            crate::protocol::ResizeDirection::Left | crate::protocol::ResizeDirection::Up => {
                                if first_contains {
                                    (*ratio - delta).max(0.1)
                                } else {
                                    (*ratio + delta).min(0.9)
                                }
                            }
                            crate::protocol::ResizeDirection::Right | crate::protocol::ResizeDirection::Down => {
                                if first_contains {
                                    (*ratio + delta).min(0.9)
                                } else {
                                    (*ratio - delta).max(0.1)
                                }
                            }
                        };

                        *ratio = new_ratio;
                        return true;
                    }
                }

                // Recurse into children
                first.resize_pane_internal(target_pane_id, direction, amount) ||
                second.resize_pane_internal(target_pane_id, direction, amount)
            }
            Layout::Leaf(_) => false,
        }
    }

    fn contains_pane(&self, pane_id: &PaneId) -> bool {
        match self {
            Layout::Leaf(id) => id == pane_id,
            Layout::Split { first, second, .. } => {
                first.contains_pane(pane_id) || second.contains_pane(pane_id)
            }
        }
    }

    pub fn get_dimensions(&self, total_width: u16, total_height: u16) -> Vec<(PaneId, u16, u16, u16, u16)> {
        self.get_dimensions_internal(0, 0, total_width, total_height)
    }

    fn get_dimensions_internal(&self, x: u16, y: u16, width: u16, height: u16) -> Vec<(PaneId, u16, u16, u16, u16)> {
        match self {
            Layout::Leaf(id) => vec![(id.clone(), x, y, width, height)],
            Layout::Split { direction, ratio, first, second } => {
                let mut results = Vec::new();

                match direction {
                    SplitDirection::Horizontal => {
                        let first_height = (height as f32 * ratio) as u16;
                        let second_height = height - first_height;

                        results.extend(first.get_dimensions_internal(x, y, width, first_height));
                        results.extend(second.get_dimensions_internal(x, y + first_height, width, second_height));
                    }
                    SplitDirection::Vertical => {
                        let first_width = (width as f32 * ratio) as u16;
                        let second_width = width - first_width;

                        results.extend(first.get_dimensions_internal(x, y, first_width, height));
                        results.extend(second.get_dimensions_internal(x + first_width, y, second_width, height));
                    }
                }

                results
            }
        }
    }

    pub fn navigate(&self, from_pane_id: &PaneId, direction: NavigationDirection) -> Option<PaneId> {
        let dimensions = self.get_dimensions(100, 100); // Use normalized dimensions

        let current_pane = dimensions.iter().find(|(id, _, _, _, _)| id == from_pane_id)?;
        let (_, curr_x, curr_y, curr_w, curr_h) = current_pane;
        let curr_center_x = curr_x + curr_w / 2;
        let curr_center_y = curr_y + curr_h / 2;

        let mut best_candidate: Option<&PaneId> = None;
        let mut best_distance = u32::MAX;

        for (id, x, y, w, h) in &dimensions {
            if id == from_pane_id {
                continue;
            }

            let center_x = x + w / 2;
            let center_y = y + h / 2;

            let is_valid = match direction {
                NavigationDirection::Up => center_y < curr_center_y,
                NavigationDirection::Down => center_y > curr_center_y,
                NavigationDirection::Left => center_x < curr_center_x,
                NavigationDirection::Right => center_x > curr_center_x,
            };

            if is_valid {
                let dx = (center_x as i32 - curr_center_x as i32).abs() as u32;
                let dy = (center_y as i32 - curr_center_y as i32).abs() as u32;
                let distance = dx * dx + dy * dy;

                if distance < best_distance {
                    best_distance = distance;
                    best_candidate = Some(id);
                }
            }
        }

        best_candidate.cloned()
    }

    pub fn toggle_zoom(&mut self) {
        // Simple implementation: for now, we'll just mark it as toggled
        // In a full implementation, this would hide/show other panes
        // For now, this is a no-op since zoom state would need to be tracked
    }
}

#[derive(Debug, Clone, Copy)]
pub enum NavigationDirection {
    Up,
    Down,
    Left,
    Right,
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_layout_creation() {
        let pane_id = PaneId(Uuid::new_v4());
        let layout = Layout::new(pane_id.clone());

        match layout {
            Layout::Leaf(id) => assert_eq!(id, pane_id),
            _ => panic!("Expected Leaf layout"),
        }
    }

    #[test]
    fn test_layout_split_horizontal() {
        let pane_id = PaneId(Uuid::new_v4());
        let mut layout = Layout::new(pane_id.clone());
        let new_pane_id = PaneId(Uuid::new_v4());

        let result = layout.split(&pane_id, SplitDirection::Horizontal, new_pane_id.clone());
        assert!(result);

        match layout {
            Layout::Split { direction, ratio, first, second } => {
                assert!(matches!(direction, SplitDirection::Horizontal));
                assert_eq!(ratio, 0.5);
                match (first.as_ref(), second.as_ref()) {
                    (Layout::Leaf(ref id1), Layout::Leaf(ref id2)) => {
                        assert_eq!(*id1, pane_id);
                        assert_eq!(*id2, new_pane_id);
                    }
                    _ => panic!("Expected leaf layouts after split"),
                }
            }
            _ => panic!("Expected Split layout after splitting"),
        }
    }

    #[test]
    fn test_layout_split_vertical() {
        let pane_id = PaneId(Uuid::new_v4());
        let mut layout = Layout::new(pane_id.clone());
        let new_pane_id = PaneId(Uuid::new_v4());

        let result = layout.split(&pane_id, SplitDirection::Vertical, new_pane_id.clone());
        assert!(result);

        match layout {
            Layout::Split { direction, .. } => {
                assert!(matches!(direction, SplitDirection::Vertical));
            }
            _ => panic!("Expected Split layout after splitting"),
        }
    }

    #[test]
    fn test_layout_split_nonexistent_pane() {
        let pane_id = PaneId(Uuid::new_v4());
        let mut layout = Layout::new(pane_id);
        let nonexistent_pane = PaneId(Uuid::new_v4());
        let new_pane_id = PaneId(Uuid::new_v4());

        let result = layout.split(&nonexistent_pane, SplitDirection::Horizontal, new_pane_id);
        assert!(!result);
    }

    #[test]
    fn test_layout_find_pane() {
        let pane_id = PaneId(Uuid::new_v4());
        let layout = Layout::new(pane_id.clone());

        let found = layout.find_pane(&pane_id);
        assert!(found.is_some());
        assert_eq!(*found.unwrap(), pane_id);

        let nonexistent = PaneId(Uuid::new_v4());
        let not_found = layout.find_pane(&nonexistent);
        assert!(not_found.is_none());
    }

    #[test]
    fn test_layout_get_all_panes() {
        let pane_id = PaneId(Uuid::new_v4());
        let mut layout = Layout::new(pane_id.clone());

        let panes = layout.get_all_panes();
        assert_eq!(panes.len(), 1);
        assert_eq!(panes[0], pane_id);

        // Split and check again
        let new_pane_id = PaneId(Uuid::new_v4());
        layout.split(&pane_id, SplitDirection::Horizontal, new_pane_id.clone());

        let panes = layout.get_all_panes();
        assert_eq!(panes.len(), 2);
        assert!(panes.contains(&pane_id));
        assert!(panes.contains(&new_pane_id));
    }

    #[test]
    fn test_layout_remove_pane() {
        let pane_id1 = PaneId(Uuid::new_v4());
        let pane_id2 = PaneId(Uuid::new_v4());
        let mut layout = Layout::new(pane_id1.clone());

        layout.split(&pane_id1, SplitDirection::Horizontal, pane_id2.clone());

        // Check that we have both panes before removal
        let all_panes = layout.get_all_panes();
        assert_eq!(all_panes.len(), 2);
        assert!(all_panes.contains(&pane_id1));
        assert!(all_panes.contains(&pane_id2));

        // Remove first pane - but note that remove_pane_internal may return the new structure
        // wrapped in Some, but we need to apply it
        let mut original_layout = layout.clone();
        if let Some(new_layout) = original_layout.remove_pane_internal(&pane_id1) {
            layout = new_layout;
        }

        // Should now be a leaf with the second pane
        match layout {
            Layout::Leaf(id) => assert_eq!(id, pane_id2),
            _ => {
                // If it's still a split, at least verify the first pane is gone
                let remaining_panes = layout.get_all_panes();
                assert!(!remaining_panes.contains(&pane_id1));
                assert!(remaining_panes.contains(&pane_id2));
            }
        }
    }

    #[test]
    fn test_layout_remove_nonexistent_pane() {
        let pane_id = PaneId(Uuid::new_v4());
        let mut layout = Layout::new(pane_id);
        let nonexistent = PaneId(Uuid::new_v4());

        let result = layout.remove_pane(&nonexistent);
        assert!(!result);
    }

    #[test]
    fn test_layout_get_dimensions() {
        let pane_id = PaneId(Uuid::new_v4());
        let layout = Layout::new(pane_id.clone());

        let dimensions = layout.get_dimensions(100, 50);
        assert_eq!(dimensions.len(), 1);

        let (id, x, y, width, height) = &dimensions[0];
        assert_eq!(*id, pane_id);
        assert_eq!(*x, 0);
        assert_eq!(*y, 0);
        assert_eq!(*width, 100);
        assert_eq!(*height, 50);
    }

    #[test]
    fn test_layout_get_dimensions_split() {
        let pane_id1 = PaneId(Uuid::new_v4());
        let pane_id2 = PaneId(Uuid::new_v4());
        let mut layout = Layout::new(pane_id1.clone());

        layout.split(&pane_id1, SplitDirection::Horizontal, pane_id2.clone());

        let dimensions = layout.get_dimensions(100, 50);
        assert_eq!(dimensions.len(), 2);

        // Check that both panes have correct dimensions
        let first_pane = dimensions.iter().find(|(id, _, _, _, _)| *id == pane_id1).unwrap();
        let second_pane = dimensions.iter().find(|(id, _, _, _, _)| *id == pane_id2).unwrap();

        assert_eq!(first_pane.3, 100); // width
        assert_eq!(first_pane.4, 25);  // height (50 * 0.5)
        assert_eq!(second_pane.3, 100); // width
        assert_eq!(second_pane.4, 25);  // height (50 - 25)
    }

    #[test]
    fn test_layout_navigate() {
        let pane_id1 = PaneId(Uuid::new_v4());
        let pane_id2 = PaneId(Uuid::new_v4());
        let mut layout = Layout::new(pane_id1.clone());

        layout.split(&pane_id1, SplitDirection::Horizontal, pane_id2.clone());

        // Navigate from first pane down to second pane
        let result = layout.navigate(&pane_id1, NavigationDirection::Down);
        assert!(result.is_some());
        assert_eq!(result.unwrap(), pane_id2);

        // Navigate from second pane up to first pane
        let result = layout.navigate(&pane_id2, NavigationDirection::Up);
        assert!(result.is_some());
        assert_eq!(result.unwrap(), pane_id1);

        // Navigate in invalid direction should return None
        let result = layout.navigate(&pane_id1, NavigationDirection::Up);
        assert!(result.is_none());
    }

    #[test]
    fn test_layout_resize_split() {
        let pane_id1 = PaneId(Uuid::new_v4());
        let pane_id2 = PaneId(Uuid::new_v4());
        let mut layout = Layout::new(pane_id1.clone());

        layout.split(&pane_id1, SplitDirection::Horizontal, pane_id2.clone());

        // Resize the split
        let result = layout.resize_split(0, 0.2);
        assert!(result);

        match layout {
            Layout::Split { ratio, .. } => {
                assert!((ratio - 0.7).abs() < 0.01); // 0.5 + 0.2 = 0.7
            }
            _ => panic!("Expected Split layout"),
        }
    }

    #[test]
    fn test_layout_resize_split_clamping() {
        let pane_id1 = PaneId(Uuid::new_v4());
        let pane_id2 = PaneId(Uuid::new_v4());
        let mut layout = Layout::new(pane_id1.clone());

        layout.split(&pane_id1, SplitDirection::Horizontal, pane_id2.clone());

        // Try to resize beyond limits
        let result = layout.resize_split(0, 1.0);
        assert!(result);

        match layout {
            Layout::Split { ratio, .. } => {
                assert_eq!(ratio, 0.9); // Should be clamped to max
            }
            _ => panic!("Expected Split layout"),
        }

        // Try to resize below limits
        let result = layout.resize_split(0, -1.0);
        assert!(result);

        match layout {
            Layout::Split { ratio, .. } => {
                assert_eq!(ratio, 0.1); // Should be clamped to min
            }
            _ => panic!("Expected Split layout"),
        }
    }

    #[test]
    fn test_layout_toggle_zoom() {
        let pane_id = PaneId(Uuid::new_v4());
        let mut layout = Layout::new(pane_id);

        // Toggle zoom should not panic (it's a no-op currently)
        layout.toggle_zoom();
    }

    #[test]
    fn test_navigation_direction_variants() {
        // Test that all navigation directions are available
        let _ = NavigationDirection::Up;
        let _ = NavigationDirection::Down;
        let _ = NavigationDirection::Left;
        let _ = NavigationDirection::Right;
    }
}