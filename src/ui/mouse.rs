use crossterm::event::{MouseEvent, MouseEventKind, MouseButton};
use crate::error::Result;
use crate::protocol::{PaneId, ClientMessage};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseResizeMode {
    Horizontal,  // Left/Right
    Vertical,    // Up/Down
    Both,        // Corner (both directions)
}

#[derive(Debug, Clone)]
pub struct MouseHandler {
    pub enabled: bool,
    // Track drag state for pane resizing
    dragging: bool,
    drag_start: Option<(u16, u16)>,
    drag_pane: Option<PaneId>,
    resize_direction: Option<MouseResizeMode>,
    // Track selection for copy mode
    selecting: bool,
    selection_start: Option<(u16, u16)>,
    selection_end: Option<(u16, u16)>,
    // Last click position for double-click detection
    last_click: Option<(u16, u16, std::time::Instant)>,
}

impl MouseHandler {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            dragging: false,
            drag_start: None,
            drag_pane: None,
            resize_direction: None,
            selecting: false,
            selection_start: None,
            selection_end: None,
            last_click: None,
        }
    }

    pub fn handle_mouse_event(
        &mut self,
        event: MouseEvent,
        pane_layout: &crate::protocol::LayoutInfo,
    ) -> Result<Option<MouseAction>> {
        if !self.enabled {
            return Ok(None);
        }

        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                // Check if this is a double-click
                if self.is_double_click(event.column, event.row) {
                    return Ok(Some(MouseAction::SelectWord {
                        x: event.column,
                        y: event.row
                    }));
                }

                // Update last click info
                self.last_click = Some((event.column, event.row, std::time::Instant::now()));

                // Check if click is on a pane border for resizing
                if let Some((pane_id, direction)) = self.get_pane_border_at(event.column, event.row, pane_layout) {
                    self.start_drag(event.column, event.row, pane_id.clone(), direction);
                    return Ok(Some(MouseAction::StartResize { pane: pane_id, direction }));
                }

                // Check if click is inside a pane for focusing
                if let Some(pane_id) = self.get_pane_at(event.column, event.row, pane_layout) {
                    // Start text selection
                    self.start_selection(event.column, event.row);
                    return Ok(Some(MouseAction::FocusPane { pane: pane_id }));
                }
            }

            MouseEventKind::Drag(MouseButton::Left) => {
                if self.dragging {
                    if let (Some((start_x, start_y)), Some(pane), Some(direction)) =
                        (self.drag_start, &self.drag_pane, self.resize_direction) {
                        let delta_x = event.column as i16 - start_x as i16;
                        let delta_y = event.row as i16 - start_y as i16;
                        return Ok(Some(MouseAction::ResizePanes {
                            pane: pane.clone(),
                            delta_x,
                            delta_y,
                            direction,
                        }));
                    }
                } else if self.selecting {
                    self.update_selection(event.column, event.row);
                    if let Some(start) = self.selection_start {
                        return Ok(Some(MouseAction::UpdateSelection {
                            start,
                            end: (event.column, event.row)
                        }));
                    }
                }
            }

            MouseEventKind::Up(MouseButton::Left) => {
                if self.dragging {
                    self.end_drag();
                    return Ok(Some(MouseAction::EndResize));
                } else if self.selecting {
                    self.end_selection();
                    if let (Some(start), Some(end)) = (self.selection_start, self.selection_end) {
                        return Ok(Some(MouseAction::CompleteSelection { start, end }));
                    }
                }
            }

            MouseEventKind::ScrollDown => {
                if let Some(pane_id) = self.get_pane_at(event.column, event.row, pane_layout) {
                    return Ok(Some(MouseAction::ScrollPane {
                        pane: pane_id.clone(),
                        lines: 3,
                        up: false
                    }));
                }
            }

            MouseEventKind::ScrollUp => {
                if let Some(pane_id) = self.get_pane_at(event.column, event.row, pane_layout) {
                    return Ok(Some(MouseAction::ScrollPane {
                        pane: pane_id.clone(),
                        lines: 3,
                        up: true
                    }));
                }
            }

            MouseEventKind::Down(MouseButton::Right) => {
                // Right-click to paste from clipboard
                if let Some(pane_id) = self.get_pane_at(event.column, event.row, pane_layout) {
                    return Ok(Some(MouseAction::PasteClipboard { pane: pane_id }));
                }
            }

            MouseEventKind::Down(MouseButton::Middle) => {
                // Middle-click to paste from primary selection (X11 style)
                if let Some(pane_id) = self.get_pane_at(event.column, event.row, pane_layout) {
                    return Ok(Some(MouseAction::PastePrimary { pane: pane_id }));
                }
            }

            _ => {}
        }

        Ok(None)
    }

    fn is_double_click(&self, x: u16, y: u16) -> bool {
        if let Some((last_x, last_y, last_time)) = self.last_click {
            let elapsed = last_time.elapsed();
            // Check if click is within 500ms and same position
            elapsed.as_millis() < 500 && last_x == x && last_y == y
        } else {
            false
        }
    }

    fn get_pane_at(&self, x: u16, y: u16, layout: &crate::protocol::LayoutInfo) -> Option<PaneId> {
        for pane in &layout.panes {
            if x >= pane.x && x < pane.x + pane.width &&
               y >= pane.y && y < pane.y + pane.height {
                return Some(pane.id.clone());
            }
        }
        None
    }

    fn get_pane_border_at(&self, x: u16, y: u16, layout: &crate::protocol::LayoutInfo) -> Option<(PaneId, MouseResizeMode)> {
        // More forgiving border detection - check 1-2 pixels on either side of border
        for pane in &layout.panes {
            let right_border = pane.x + pane.width;
            let bottom_border = pane.y + pane.height;

            let near_right = x >= right_border.saturating_sub(1) && x <= right_border;
            let near_bottom = y >= bottom_border.saturating_sub(1) && y <= bottom_border;
            let in_horizontal_range = y >= pane.y && y < pane.y + pane.height;
            let in_vertical_range = x >= pane.x && x < pane.x + pane.width;

            // Check for corner (both directions)
            if near_right && near_bottom && in_horizontal_range && in_vertical_range {
                return Some((pane.id.clone(), MouseResizeMode::Both));
            }

            // Check if near right border (horizontal resizing)
            if near_right && in_horizontal_range {
                return Some((pane.id.clone(), MouseResizeMode::Horizontal));
            }

            // Check if near bottom border (vertical resizing)
            if near_bottom && in_vertical_range {
                return Some((pane.id.clone(), MouseResizeMode::Vertical));
            }
        }
        None
    }

    fn start_drag(&mut self, x: u16, y: u16, pane: PaneId, direction: MouseResizeMode) {
        self.dragging = true;
        self.drag_start = Some((x, y));
        self.drag_pane = Some(pane);
        self.resize_direction = Some(direction);
    }

    fn end_drag(&mut self) {
        self.dragging = false;
        self.drag_start = None;
        self.drag_pane = None;
        self.resize_direction = None;
    }

    fn start_selection(&mut self, x: u16, y: u16) {
        self.selecting = true;
        self.selection_start = Some((x, y));
        self.selection_end = Some((x, y));
    }

    fn update_selection(&mut self, x: u16, y: u16) {
        if self.selecting {
            self.selection_end = Some((x, y));
        }
    }

    fn end_selection(&mut self) {
        self.selecting = false;
    }

    pub fn clear_selection(&mut self) {
        self.selecting = false;
        self.selection_start = None;
        self.selection_end = None;
    }

    pub fn get_selection(&self) -> Option<((u16, u16), (u16, u16))> {
        if let (Some(start), Some(end)) = (self.selection_start, self.selection_end) {
            Some((start, end))
        } else {
            None
        }
    }
}

#[derive(Debug, Clone)]
pub enum MouseAction {
    FocusPane { pane: PaneId },
    StartResize { pane: PaneId, direction: MouseResizeMode },
    ResizePanes { pane: PaneId, delta_x: i16, delta_y: i16, direction: MouseResizeMode },
    EndResize,
    ScrollPane { pane: PaneId, lines: u16, up: bool },
    UpdateSelection { start: (u16, u16), end: (u16, u16) },
    CompleteSelection { start: (u16, u16), end: (u16, u16) },
    SelectWord { x: u16, y: u16 },
    PasteClipboard { pane: PaneId },
    PastePrimary { pane: PaneId },
}

impl MouseAction {
    /// Convert mouse action to client message for server
    pub fn to_client_message(&self) -> Option<ClientMessage> {
        match self {
            MouseAction::FocusPane { pane } => {
                Some(ClientMessage::SwitchPane { pane_id: pane.clone() })
            }
            MouseAction::ScrollPane { pane: _, lines, up } => {
                // Send scroll commands as input to the pane
                let scroll_cmd = if *up {
                    format!("\x1b[{}A", lines) // Scroll up
                } else {
                    format!("\x1b[{}B", lines) // Scroll down
                };
                Some(ClientMessage::Input {
                    data: scroll_cmd.into_bytes()
                })
            }
            _ => None,
        }
    }
}