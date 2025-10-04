use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};
use std::time::{Duration, Instant};

use crate::protocol::WindowId;

/// Information about a window for the selector
#[derive(Clone, Debug)]
pub struct WindowInfo {
    pub id: WindowId,
    pub name: String,
    pub index: usize,
    pub active: bool,
    pub pane_count: usize,
}

/// Visual window selector that displays all windows and allows selection
pub struct WindowSelector {
    windows: Vec<WindowInfo>,
    selected: usize,
    visible: bool,
    last_update: Instant,
    filter: String,
}

impl WindowSelector {
    pub fn new() -> Self {
        Self {
            windows: Vec::new(),
            selected: 0,
            visible: false,
            last_update: Instant::now(),
            filter: String::new(),
        }
    }

    /// Show the window selector with the given windows
    pub fn show(&mut self, windows: Vec<WindowInfo>) {
        self.windows = windows;
        self.selected = 0;
        self.visible = true;
        self.last_update = Instant::now();
        self.filter.clear();
    }

    /// Hide the window selector
    pub fn hide(&mut self) {
        self.visible = false;
        self.filter.clear();
    }

    /// Check if the selector is currently visible
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Navigate to the next window
    pub fn next(&mut self) {
        if !self.filtered_windows().is_empty() {
            self.selected = (self.selected + 1) % self.filtered_windows().len();
        }
    }

    /// Navigate to the previous window
    pub fn previous(&mut self) {
        if !self.filtered_windows().is_empty() {
            if self.selected == 0 {
                self.selected = self.filtered_windows().len() - 1;
            } else {
                self.selected = self.selected.saturating_sub(1);
            }
        }
    }

    /// Select a window by index (0-9)
    pub fn select_by_index(&mut self, index: usize) -> Option<WindowId> {
        let filtered = self.filtered_windows();
        if index < filtered.len() {
            self.selected = index;
            self.get_selected()
        } else {
            None
        }
    }

    /// Get the currently selected window
    pub fn get_selected(&self) -> Option<WindowId> {
        let filtered = self.filtered_windows();
        filtered.get(self.selected).map(|w| w.id.clone())
    }

    /// Add a character to the filter
    pub fn add_filter_char(&mut self, ch: char) {
        self.filter.push(ch);
        // Reset selection when filter changes
        self.selected = 0;
    }

    /// Remove the last character from the filter
    pub fn backspace_filter(&mut self) {
        self.filter.pop();
        // Reset selection when filter changes
        self.selected = 0;
    }

    /// Clear the filter
    pub fn clear_filter(&mut self) {
        self.filter.clear();
        self.selected = 0;
    }

    /// Get windows filtered by the current filter string
    fn filtered_windows(&self) -> Vec<&WindowInfo> {
        if self.filter.is_empty() {
            self.windows.iter().collect()
        } else {
            self.windows
                .iter()
                .filter(|w| {
                    w.name.to_lowercase().contains(&self.filter.to_lowercase())
                })
                .collect()
        }
    }

    /// Render the window selector
    pub fn render(&self, frame: &mut Frame, area: Rect) {
        if !self.visible {
            return;
        }

        // Calculate centered area for the selector
        let popup_area = centered_rect(60, 70, area);

        // Create the main block
        let block = Block::default()
            .title(" Window Selector ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow));

        let inner = block.inner(popup_area);
        frame.render_widget(block, popup_area);

        // Split the inner area for filter and list
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Filter area
                Constraint::Min(5),    // Window list
                Constraint::Length(2), // Help text
            ])
            .split(inner);

        // Render filter input
        let filter_text = if self.filter.is_empty() {
            "Type to filter windows..."
        } else {
            &self.filter
        };

        let filter_widget = Paragraph::new(filter_text)
            .block(
                Block::default()
                    .title(" Filter ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Cyan)),
            )
            .style(if self.filter.is_empty() {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default().fg(Color::White)
            });

        frame.render_widget(filter_widget, chunks[0]);

        // Render window list
        let filtered = self.filtered_windows();
        let items: Vec<ListItem> = filtered
            .iter()
            .enumerate()
            .map(|(i, window)| {
                let index_str = if i < 10 {
                    format!("{}:", i)
                } else {
                    "  ".to_string()
                };

                let status_indicator = if window.active {
                    " * "
                } else {
                    "   "
                };

                let pane_info = format!(" ({} panes)", window.pane_count);

                let content = Line::from(vec![
                    Span::styled(
                        index_str,
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(status_indicator),
                    Span::styled(
                        &window.name,
                        if i == self.selected {
                            Style::default()
                                .fg(Color::White)
                                .add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(Color::Gray)
                        },
                    ),
                    Span::styled(pane_info, Style::default().fg(Color::DarkGray)),
                ]);

                ListItem::new(content)
            })
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .title(format!(" Windows ({}) ", filtered.len()))
                    .borders(Borders::ALL),
            )
            .highlight_style(
                Style::default()
                    .bg(Color::Yellow)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("> ");

        let mut state = ListState::default();
        state.select(Some(self.selected));

        frame.render_stateful_widget(list, chunks[1], &mut state);

        // Render help text
        let help_text = Line::from(vec![
            Span::styled("↑↓", Style::default().fg(Color::Yellow)),
            Span::raw(" Navigate  "),
            Span::styled("0-9", Style::default().fg(Color::Yellow)),
            Span::raw(" Quick select  "),
            Span::styled("Enter", Style::default().fg(Color::Yellow)),
            Span::raw(" Select  "),
            Span::styled("Esc", Style::default().fg(Color::Yellow)),
            Span::raw(" Cancel"),
        ]);

        let help = Paragraph::new(help_text)
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::DarkGray));

        frame.render_widget(help, chunks[2]);
    }

    /// Check if the selector should auto-hide (e.g., after timeout)
    pub fn should_auto_hide(&self, timeout: Duration) -> bool {
        self.visible && self.last_update.elapsed() > timeout
    }

    /// Update the last interaction time
    pub fn update_interaction(&mut self) {
        self.last_update = Instant::now();
    }
}

/// Helper function to create a centered rectangle
fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn create_test_windows() -> Vec<WindowInfo> {
        vec![
            WindowInfo {
                id: WindowId(Uuid::new_v4()),
                name: "bash".to_string(),
                index: 0,
                active: true,
                pane_count: 2,
            },
            WindowInfo {
                id: WindowId(Uuid::new_v4()),
                name: "vim".to_string(),
                index: 1,
                active: false,
                pane_count: 1,
            },
            WindowInfo {
                id: WindowId(Uuid::new_v4()),
                name: "logs".to_string(),
                index: 2,
                active: false,
                pane_count: 3,
            },
        ]
    }

    #[test]
    fn test_window_selector_initialization() {
        let selector = WindowSelector::new();
        assert!(!selector.is_visible());
        assert_eq!(selector.selected, 0);
        assert!(selector.filter.is_empty());
    }

    #[test]
    fn test_show_hide() {
        let mut selector = WindowSelector::new();
        let windows = create_test_windows();

        selector.show(windows.clone());
        assert!(selector.is_visible());
        assert_eq!(selector.windows.len(), 3);

        selector.hide();
        assert!(!selector.is_visible());
        assert!(selector.filter.is_empty());
    }

    #[test]
    fn test_navigation() {
        let mut selector = WindowSelector::new();
        let windows = create_test_windows();
        selector.show(windows);

        // Test next navigation
        selector.next();
        assert_eq!(selector.selected, 1);

        selector.next();
        assert_eq!(selector.selected, 2);

        selector.next();
        assert_eq!(selector.selected, 0); // Wrap around

        // Test previous navigation
        selector.previous();
        assert_eq!(selector.selected, 2); // Wrap around

        selector.previous();
        assert_eq!(selector.selected, 1);
    }

    #[test]
    fn test_select_by_index() {
        let mut selector = WindowSelector::new();
        let windows = create_test_windows();
        let window_id = windows[1].id.clone();
        selector.show(windows);

        let result = selector.select_by_index(1);
        assert!(result.is_some());
        assert_eq!(result.unwrap(), window_id);

        // Out of bounds
        let result = selector.select_by_index(10);
        assert!(result.is_none());
    }

    #[test]
    fn test_filtering() {
        let mut selector = WindowSelector::new();
        let windows = create_test_windows();
        selector.show(windows);

        // Add filter
        selector.add_filter_char('v');
        selector.add_filter_char('i');
        assert_eq!(selector.filter, "vi");

        let filtered = selector.filtered_windows();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].name, "vim");

        // Backspace
        selector.backspace_filter();
        assert_eq!(selector.filter, "v");

        // Clear filter
        selector.clear_filter();
        assert_eq!(selector.filter, "");
        assert_eq!(selector.filtered_windows().len(), 3);
    }

    #[test]
    fn test_auto_hide_timeout() {
        let mut selector = WindowSelector::new();
        selector.show(create_test_windows());

        // Should not auto-hide immediately
        assert!(!selector.should_auto_hide(Duration::from_secs(5)));

        // Simulate time passing (can't actually wait in tests)
        // Just verify the logic exists
        selector.update_interaction();
    }
}