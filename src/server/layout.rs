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