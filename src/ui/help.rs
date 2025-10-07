use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Help overlay - shows keybindings and usage tips
pub struct HelpOverlay {
    visible: bool,
    scroll_offset: usize,
    selected_category: usize,
}

#[derive(Debug, Clone)]
struct HelpCategory {
    name: &'static str,
    items: Vec<HelpItem>,
}

#[derive(Debug, Clone)]
struct HelpItem {
    keys: &'static str,
    description: &'static str,
}

impl Default for HelpOverlay {
    fn default() -> Self {
        Self::new()
    }
}

impl HelpOverlay {
    pub fn new() -> Self {
        Self {
            visible: false,
            scroll_offset: 0,
            selected_category: 0,
        }
    }

    pub fn show(&mut self) {
        self.visible = true;
        self.scroll_offset = 0;
        self.selected_category = 0;
    }

    pub fn hide(&mut self) {
        self.visible = false;
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    pub fn handle_key(&mut self, key_event: KeyEvent) -> bool {
        if !self.visible {
            return false;
        }

        match (key_event.modifiers, key_event.code) {
            (KeyModifiers::NONE, KeyCode::Char('q'))
            | (KeyModifiers::NONE, KeyCode::Char('?'))
            | (KeyModifiers::NONE, KeyCode::Esc) => {
                self.hide();
                true
            }
            (KeyModifiers::NONE, KeyCode::Down) | (KeyModifiers::NONE, KeyCode::Char('j')) => {
                self.scroll_offset += 1;
                true
            }
            (KeyModifiers::NONE, KeyCode::Up) | (KeyModifiers::NONE, KeyCode::Char('k')) => {
                self.scroll_offset = self.scroll_offset.saturating_sub(1);
                true
            }
            (KeyModifiers::NONE, KeyCode::PageDown) => {
                self.scroll_offset += 10;
                true
            }
            (KeyModifiers::NONE, KeyCode::PageUp) => {
                self.scroll_offset = self.scroll_offset.saturating_sub(10);
                true
            }
            (KeyModifiers::NONE, KeyCode::Tab) => {
                let categories = self.get_categories();
                self.selected_category = (self.selected_category + 1) % categories.len();
                self.scroll_offset = 0;
                true
            }
            _ => true, // Consume all keys while help is visible
        }
    }

    fn get_categories(&self) -> Vec<HelpCategory> {
        vec![
            HelpCategory {
                name: "Session Management",
                items: vec![
                    HelpItem { keys: "Ctrl-b d", description: "Detach from session" },
                    HelpItem { keys: "Ctrl-b $", description: "Rename session" },
                    HelpItem { keys: "Ctrl-b s", description: "List sessions" },
                    HelpItem { keys: "ferrix new -s <name>", description: "Create new session (CLI)" },
                    HelpItem { keys: "ferrix attach <name>", description: "Attach to session (CLI)" },
                    HelpItem { keys: "ferrix list", description: "List all sessions (CLI)" },
                ],
            },
            HelpCategory {
                name: "Window Management",
                items: vec![
                    HelpItem { keys: "Ctrl-b c", description: "Create new window" },
                    HelpItem { keys: "Ctrl-b n", description: "Next window" },
                    HelpItem { keys: "Ctrl-b p", description: "Previous window" },
                    HelpItem { keys: "Ctrl-b 0-9", description: "Select window by number" },
                    HelpItem { keys: "Ctrl-b ,", description: "Rename current window" },
                    HelpItem { keys: "Ctrl-b w", description: "List windows" },
                    HelpItem { keys: "Ctrl-b &", description: "Kill current window" },
                ],
            },
            HelpCategory {
                name: "Pane Management",
                items: vec![
                    HelpItem { keys: "Ctrl-b %", description: "Split pane vertically" },
                    HelpItem { keys: "Ctrl-b \"", description: "Split pane horizontally" },
                    HelpItem { keys: "Ctrl-b ↑↓←→", description: "Navigate between panes" },
                    HelpItem { keys: "Ctrl-b ;", description: "Toggle to last pane" },
                    HelpItem { keys: "Ctrl-b q", description: "Display pane numbers" },
                    HelpItem { keys: "Ctrl-b q 0-9", description: "Select pane by number" },
                    HelpItem { keys: "Ctrl-b z", description: "Toggle pane zoom" },
                    HelpItem { keys: "Ctrl-b x", description: "Close current pane" },
                    HelpItem { keys: "Ctrl-b {/}", description: "Swap panes" },
                ],
            },
            HelpCategory {
                name: "Copy Mode & Selection",
                items: vec![
                    HelpItem { keys: "Ctrl-b [", description: "Enter copy mode" },
                    HelpItem { keys: "Space", description: "Start selection (in copy mode)" },
                    HelpItem { keys: "Enter", description: "Copy selection (in copy mode)" },
                    HelpItem { keys: "/", description: "Search forward (in copy mode)" },
                    HelpItem { keys: "?", description: "Search backward (in copy mode)" },
                    HelpItem { keys: "q", description: "Exit copy mode" },
                    HelpItem { keys: "hjkl", description: "Vi-style navigation (in copy mode)" },
                    HelpItem { keys: "Ctrl-b ]", description: "Paste buffer" },
                ],
            },
            HelpCategory {
                name: "Mouse Support",
                items: vec![
                    HelpItem { keys: "Left-click", description: "Focus pane" },
                    HelpItem { keys: "Left-drag", description: "Select text" },
                    HelpItem { keys: "Double-click", description: "Select word" },
                    HelpItem { keys: "Right-click", description: "Paste from clipboard" },
                    HelpItem { keys: "Middle-click", description: "Paste primary selection (X11)" },
                    HelpItem { keys: "Scroll wheel", description: "Scroll pane up/down" },
                    HelpItem { keys: "Drag border", description: "Resize panes" },
                ],
            },
            HelpCategory {
                name: "Command Mode",
                items: vec![
                    HelpItem { keys: "Ctrl-b :", description: "Enter command mode" },
                    HelpItem { keys: ":split-window", description: "Split current pane" },
                    HelpItem { keys: ":new-window", description: "Create new window" },
                    HelpItem { keys: ":kill-pane", description: "Close current pane" },
                    HelpItem { keys: ":list-keys", description: "Show all keybindings" },
                    HelpItem { keys: ":save-snapshot", description: "Save session snapshot" },
                    HelpItem { keys: ":load-snapshot", description: "Load session snapshot" },
                ],
            },
            HelpCategory {
                name: "Advanced Features",
                items: vec![
                    HelpItem { keys: "Ctrl-b r", description: "Reload configuration" },
                    HelpItem { keys: "Ctrl-b Space", description: "Cycle through layouts" },
                    HelpItem { keys: "Ctrl-b =", description: "Toggle pane synchronization" },
                    HelpItem { keys: "Ctrl-b !", description: "Break pane into new window" },
                    HelpItem { keys: "Ctrl-b @", description: "Toggle activity monitoring" },
                ],
            },
            HelpCategory {
                name: "Getting Help",
                items: vec![
                    HelpItem { keys: "Ctrl-b ?", description: "Show this help (toggle)" },
                    HelpItem { keys: "ferrix --help", description: "CLI help" },
                    HelpItem { keys: "ferrix <command> --help", description: "Command-specific help" },
                    HelpItem { keys: "Tab", description: "Switch help category" },
                    HelpItem { keys: "j/k or ↑/↓", description: "Scroll help" },
                    HelpItem { keys: "q or Esc", description: "Close help" },
                ],
            },
        ]
    }

    /// Render help overlay using crossterm directly (for non-ratatui terminals)
    pub fn render_crossterm(&self) -> Result<(), std::io::Error> {
        use crossterm::{
            cursor::MoveTo,
            style::{Color as CColor, SetForegroundColor, SetBackgroundColor, ResetColor, Attribute, SetAttribute}, queue,
        };
        use std::io::{stdout, Write};

        if !self.visible {
            return Ok(());
        }

        let mut stdout = stdout();
        let (width, height) = crossterm::terminal::size()?;

        // Calculate centered overlay (80% of screen)
        let overlay_width = (width as f32 * 0.8) as u16;
        let overlay_height = (height as f32 * 0.8) as u16;
        let start_x = (width - overlay_width) / 2;
        let start_y = (height - overlay_height) / 2;

        // Draw background box
        for y in start_y..start_y + overlay_height {
            queue!(stdout, MoveTo(start_x, y))?;
            queue!(stdout, SetBackgroundColor(CColor::Black))?;
            write!(stdout, "{}", " ".repeat(overlay_width as usize))?;
        }

        // Draw header
        queue!(stdout, MoveTo(start_x + 2, start_y + 1))?;
        queue!(stdout, SetForegroundColor(CColor::Cyan))?;
        queue!(stdout, SetAttribute(Attribute::Bold))?;
        write!(stdout, "Ferrix Help")?;
        queue!(stdout, SetAttribute(Attribute::Reset))?;
        queue!(stdout, SetForegroundColor(CColor::DarkGrey))?;
        write!(stdout, " - Press 'q' or '?' to close | Tab to switch category")?;

        // Get current category
        let categories = self.get_categories();
        let category = &categories[self.selected_category];

        // Draw category title
        queue!(stdout, MoveTo(start_x + 2, start_y + 3))?;
        queue!(stdout, SetForegroundColor(CColor::Yellow))?;
        queue!(stdout, SetAttribute(Attribute::Bold))?;
        write!(stdout, "▶ {}", category.name)?;
        queue!(stdout, SetAttribute(Attribute::Reset))?;

        // Draw help items
        let mut y = start_y + 5;
        for item in &category.items {
            if y >= start_y + overlay_height - 5 {
                break; // Don't overflow
            }
            queue!(stdout, MoveTo(start_x + 4, y))?;
            queue!(stdout, SetForegroundColor(CColor::Green))?;
            write!(stdout, "{:<25}", item.keys)?;
            queue!(stdout, SetForegroundColor(CColor::White))?;
            write!(stdout, "{}", item.description)?;
            y += 1;
        }

        // Draw category tabs at bottom
        queue!(stdout, MoveTo(start_x + 2, start_y + overlay_height - 3))?;
        queue!(stdout, SetForegroundColor(CColor::DarkGrey))?;
        write!(stdout, "Categories: ")?;

        for (i, cat) in categories.iter().enumerate() {
            if i == self.selected_category {
                queue!(stdout, SetForegroundColor(CColor::Cyan))?;
                queue!(stdout, SetAttribute(Attribute::Bold))?;
                write!(stdout, " [{}] ", cat.name)?;
                queue!(stdout, SetAttribute(Attribute::Reset))?;
            } else {
                queue!(stdout, SetForegroundColor(CColor::DarkGrey))?;
                write!(stdout, "  {}  ", cat.name)?;
            }
        }

        queue!(stdout, ResetColor)?;
        stdout.flush()?;
        Ok(())
    }

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        if !self.visible {
            return;
        }

        // Create a centered overlay area (80% of screen)
        let overlay_area = Self::centered_rect(80, 80, area);

        // Create main layout: header + content
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),  // Header
                Constraint::Min(0),     // Content
            ])
            .split(overlay_area);

        // Render header
        let header = Paragraph::new(vec![
            Line::from(vec![
                Span::styled("Ferrix Help", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                Span::raw(" - "),
                Span::styled("Press 'q' or '?' to close", Style::default().fg(Color::Gray)),
                Span::raw(" | "),
                Span::styled("Tab to switch category", Style::default().fg(Color::Gray)),
            ]),
        ])
        .block(Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .title("Help"));

        frame.render_widget(header, chunks[0]);

        // Render category content
        let categories = self.get_categories();
        let category = &categories[self.selected_category];

        let mut items: Vec<ListItem> = Vec::new();

        // Add category title
        items.push(ListItem::new(Line::from(vec![
            Span::styled(
                format!("▶ {}", category.name),
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            ),
        ])));
        items.push(ListItem::new(Line::from(""))); // Blank line

        // Add help items
        for item in &category.items {
            items.push(ListItem::new(Line::from(vec![
                Span::styled(
                    format!("  {:<20}", item.keys),
                    Style::default().fg(Color::Green),
                ),
                Span::raw(" "),
                Span::styled(item.description, Style::default().fg(Color::White)),
            ])));
        }

        // Add category tabs at bottom
        items.push(ListItem::new(Line::from(""))); // Blank line
        items.push(ListItem::new(Line::from(""))); // Blank line
        let mut tab_spans = vec![Span::styled("Categories: ", Style::default().fg(Color::Gray))];
        for (i, cat) in categories.iter().enumerate() {
            if i == self.selected_category {
                tab_spans.push(Span::styled(
                    format!(" [{}] ", cat.name),
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                ));
            } else {
                tab_spans.push(Span::styled(
                    format!("  {}  ", cat.name),
                    Style::default().fg(Color::DarkGray),
                ));
            }
        }
        items.push(ListItem::new(Line::from(tab_spans)));

        let list = List::new(items)
            .block(Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)));

        frame.render_widget(list, chunks[1]);
    }

    fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
        let popup_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage((100 - percent_y) / 2),
                Constraint::Percentage(percent_y),
                Constraint::Percentage((100 - percent_y) / 2),
            ])
            .split(r);

        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage((100 - percent_x) / 2),
                Constraint::Percentage(percent_x),
                Constraint::Percentage((100 - percent_x) / 2),
            ])
            .split(popup_layout[1])[1]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_help_overlay_visibility() {
        let mut help = HelpOverlay::new();
        assert!(!help.is_visible());

        help.show();
        assert!(help.is_visible());

        help.hide();
        assert!(!help.is_visible());
    }

    #[test]
    fn test_help_overlay_categories() {
        let help = HelpOverlay::new();
        let categories = help.get_categories();

        assert!(!categories.is_empty());
        assert!(categories.iter().any(|c| c.name == "Session Management"));
        assert!(categories.iter().any(|c| c.name == "Pane Management"));
    }

    #[test]
    fn test_help_key_handling() {
        let mut help = HelpOverlay::new();
        help.show();

        // Test quit key
        let quit_event = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
        assert!(help.handle_key(quit_event));
        assert!(!help.is_visible());

        // Test scroll
        help.show();
        let down_event = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
        help.handle_key(down_event);
        assert_eq!(help.scroll_offset, 1);
    }
}
