use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};
use std::time::{Duration, Instant};

use crate::protocol::PaneId;

/// Information about a pane for the display-panes overlay
#[derive(Clone, Debug)]
pub struct PaneDisplayInfo {
    pub id: PaneId,
    pub index: usize,
    pub rect: Rect,
    pub is_active: bool,
}

/// Display-panes overlay - shows numbered panes like tmux's display-panes
pub struct DisplayPanes {
    panes: Vec<PaneDisplayInfo>,
    visible: bool,
    show_time: Instant,
    timeout: Duration,
}

impl Default for DisplayPanes {
    fn default() -> Self {
        Self::new()
    }
}

impl DisplayPanes {
    pub fn new() -> Self {
        Self {
            panes: Vec::new(),
            visible: false,
            show_time: Instant::now(),
            timeout: Duration::from_secs(1), // Default 1 second like tmux
        }
    }

    /// Show the display-panes overlay with the given panes
    pub fn show(&mut self, panes: Vec<PaneDisplayInfo>) {
        self.panes = panes;
        self.visible = true;
        self.show_time = Instant::now();
    }

    /// Hide the display-panes overlay
    pub fn hide(&mut self) {
        self.visible = false;
    }

    /// Check if the overlay is currently visible
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Check if the overlay has timed out
    pub fn is_timed_out(&self) -> bool {
        self.visible && self.show_time.elapsed() > self.timeout
    }

    /// Select a pane by its index digit (0-9)
    pub fn select_by_digit(&self, digit: char) -> Option<PaneId> {
        if let Some(index) = digit.to_digit(10) {
            self.panes
                .iter()
                .find(|p| p.index == index as usize)
                .map(|p| p.id.clone())
        } else {
            None
        }
    }

    /// Get all panes
    pub fn get_panes(&self) -> &[PaneDisplayInfo] {
        &self.panes
    }

    /// Set the timeout duration
    pub fn set_timeout(&mut self, timeout: Duration) {
        self.timeout = timeout;
    }

    /// Render the display-panes overlay
    pub fn render(&self, frame: &mut Frame, area: Rect) {
        if !self.visible {
            return;
        }

        // Render a big number in the center of each pane
        for pane_info in &self.panes {
            // Only show panes that are within the visible area
            if pane_info.rect.width > 0 && pane_info.rect.height > 0 {
                self.render_pane_number(frame, pane_info);
            }
        }
    }

    fn render_pane_number(&self, frame: &mut Frame, pane_info: &PaneDisplayInfo) {
        let number = pane_info.index.to_string();

        // Calculate center position for the number
        let rect = pane_info.rect;

        // Create a big, centered number display
        let style = if pane_info.is_active {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Green)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        };

        // Create ASCII art for large numbers
        let big_number = self.create_big_number(&number);

        // Calculate positioning to center the number
        let lines_count = big_number.len() as u16;
        let max_width = big_number.iter().map(|l| l.len()).max().unwrap_or(0) as u16;

        let vertical_offset = rect.height.saturating_sub(lines_count) / 2;
        let horizontal_offset = rect.width.saturating_sub(max_width) / 2;

        // Create the display area centered in the pane
        if vertical_offset < rect.height && horizontal_offset < rect.width {
            let display_area = Rect {
                x: rect.x + horizontal_offset,
                y: rect.y + vertical_offset,
                width: max_width.min(rect.width - horizontal_offset),
                height: lines_count.min(rect.height - vertical_offset),
            };

            // Create the paragraph with the big number
            let lines: Vec<Line> = big_number
                .into_iter()
                .map(|line| Line::from(Span::styled(line, style)))
                .collect();

            let paragraph = Paragraph::new(lines);
            frame.render_widget(paragraph, display_area);
        }
    }

    /// Create ASCII art representation of a number (simple version)
    fn create_big_number(&self, num: &str) -> Vec<String> {
        // Simple 3-line ASCII art for digits
        let patterns = [
            // 0
            vec![" ▄▀▀▄ ", "█  █", " ▀▀  "],
            // 1
            vec!["  █  ", "  █  ", "  █  "],
            // 2
            vec![" ▀▀▄ ", " ▄▀  ", "█▀▀▀"],
            // 3
            vec![" ▀▀▄ ", "  ▀▄ ", " ▀▀  "],
            // 4
            vec!["█  █", "█▀▀█", "   █"],
            // 5
            vec!["█▀▀▀", "▀▀▀▄ ", " ▀▀  "],
            // 6
            vec![" ▄▀▀ ", "█▀▀▄ ", " ▀▀  "],
            // 7
            vec!["▀▀▀█", "  █  ", " █   "],
            // 8
            vec![" ▄▀▄ ", " ▀▄▄ ", " ▀▀  "],
            // 9
            vec![" ▄▀▄ ", " ▀▀█ ", "  ▀  "],
        ];

        let mut result = vec![String::new(), String::new(), String::new()];

        for ch in num.chars() {
            if let Some(digit) = ch.to_digit(10) {
                let pattern = &patterns[digit as usize];
                for (i, line) in pattern.iter().enumerate() {
                    result[i].push_str(line);
                    result[i].push(' '); // Space between digits
                }
            }
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_display_panes_show_hide() {
        let mut display = DisplayPanes::new();
        assert!(!display.is_visible());

        let pane_id = PaneId(Uuid::new_v4());
        let panes = vec![PaneDisplayInfo {
            id: pane_id.clone(),
            index: 0,
            rect: Rect::new(0, 0, 80, 24),
            is_active: true,
        }];

        display.show(panes);
        assert!(display.is_visible());

        display.hide();
        assert!(!display.is_visible());
    }

    #[test]
    fn test_select_by_digit() {
        let mut display = DisplayPanes::new();
        let pane_id = PaneId(Uuid::new_v4());
        let panes = vec![PaneDisplayInfo {
            id: pane_id.clone(),
            index: 5,
            rect: Rect::new(0, 0, 80, 24),
            is_active: true,
        }];

        display.show(panes);

        assert_eq!(display.select_by_digit('5'), Some(pane_id));
        assert_eq!(display.select_by_digit('0'), None);
        assert_eq!(display.select_by_digit('9'), None);
    }

    #[test]
    fn test_timeout() {
        let mut display = DisplayPanes::new();
        display.set_timeout(Duration::from_millis(10));

        let pane_id = PaneId(Uuid::new_v4());
        let panes = vec![PaneDisplayInfo {
            id: pane_id,
            index: 0,
            rect: Rect::new(0, 0, 80, 24),
            is_active: true,
        }];

        display.show(panes);
        assert!(display.is_visible());
        assert!(!display.is_timed_out());

        std::thread::sleep(Duration::from_millis(15));
        assert!(display.is_timed_out());
    }
}
