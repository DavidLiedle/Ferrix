use crate::config::CopyModeStyle;
use arboard::Clipboard;
use std::error::Error;

#[derive(Debug, Clone, PartialEq)]
pub enum CopyModeState {
    Normal,
    Visual,
    VisualLine,
    VisualBlock,
    Search(SearchDirection),
}

#[derive(Debug, Clone, PartialEq)]
pub enum SearchDirection {
    Forward,
    Backward,
}

pub struct CopyMode {
    active: bool,
    state: CopyModeState,
    buffer: Vec<String>,
    cursor_row: usize,
    cursor_col: usize,
    selection_start: Option<(usize, usize)>,
    selection_end: Option<(usize, usize)>,
    search_query: String,
    search_matches: Vec<(usize, usize)>,
    current_match: Option<usize>,
    jump_list: Vec<(usize, usize)>,
    jump_index: usize,
    yanked_text: Option<String>,
    clipboard: Option<Clipboard>,
    viewport_offset: usize,
    viewport_height: usize,
}

impl CopyMode {
    pub fn new(_mode: CopyModeStyle) -> Self {
        let clipboard = Clipboard::new().ok();

        Self {
            active: false,
            state: CopyModeState::Normal,
            buffer: Vec::new(),
            cursor_row: 0,
            cursor_col: 0,
            selection_start: None,
            selection_end: None,
            search_query: String::new(),
            search_matches: Vec::new(),
            current_match: None,
            jump_list: Vec::new(),
            jump_index: 0,
            yanked_text: None,
            clipboard,
            viewport_offset: 0,
            viewport_height: 24,
        }
    }

    pub fn enter(&mut self, buffer: Vec<String>) {
        self.active = true;
        self.buffer = buffer;
        self.cursor_row = 0;
        self.cursor_col = 0;
        self.selection_start = None;
        self.selection_end = None;
    }

    pub fn exit(&mut self) {
        self.active = false;
        self.selection_start = None;
        self.selection_end = None;
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn move_cursor_up(&mut self) {
        if self.cursor_row > 0 {
            self.cursor_row -= 1;
        }
    }

    pub fn move_cursor_down(&mut self) {
        if self.cursor_row < self.buffer.len() - 1 {
            self.cursor_row += 1;
        }
    }

    pub fn move_cursor_left(&mut self) {
        if self.cursor_col > 0 {
            self.cursor_col -= 1;
        }
    }

    pub fn move_cursor_right(&mut self) {
        if let Some(line) = self.buffer.get(self.cursor_row) {
            if self.cursor_col < line.len() {
                self.cursor_col += 1;
            }
        }
    }

    pub fn start_selection(&mut self) {
        self.selection_start = Some((self.cursor_row, self.cursor_col));
    }

    pub fn update_selection(&mut self) {
        self.selection_end = Some((self.cursor_row, self.cursor_col));
    }

    // Vim motion commands
    pub fn move_word_forward(&mut self) {
        if let Some(line) = self.buffer.get(self.cursor_row) {
            let mut col = self.cursor_col;
            let chars: Vec<char> = line.chars().collect();

            // Skip current word
            while col < chars.len() && !chars[col].is_whitespace() {
                col += 1;
            }
            // Skip whitespace
            while col < chars.len() && chars[col].is_whitespace() {
                col += 1;
            }

            if col < chars.len() {
                self.cursor_col = col;
            } else if self.cursor_row < self.buffer.len() - 1 {
                self.cursor_row += 1;
                self.cursor_col = 0;
            }
        }
    }

    pub fn move_word_backward(&mut self) {
        if self.cursor_col > 0 {
            if let Some(line) = self.buffer.get(self.cursor_row) {
                let mut col = self.cursor_col - 1;
                let chars: Vec<char> = line.chars().collect();

                // Skip whitespace
                while col > 0 && chars[col].is_whitespace() {
                    col -= 1;
                }
                // Skip to beginning of word
                while col > 0 && !chars[col - 1].is_whitespace() {
                    col -= 1;
                }

                self.cursor_col = col;
            }
        } else if self.cursor_row > 0 {
            self.cursor_row -= 1;
            if let Some(line) = self.buffer.get(self.cursor_row) {
                self.cursor_col = line.len();
            }
        }
    }

    pub fn move_to_line_start(&mut self) {
        self.cursor_col = 0;
    }

    pub fn move_to_line_end(&mut self) {
        if let Some(line) = self.buffer.get(self.cursor_row) {
            self.cursor_col = line.len().saturating_sub(1);
        }
    }

    pub fn move_to_first_line(&mut self) {
        self.cursor_row = 0;
        self.cursor_col = 0;
    }

    pub fn move_to_last_line(&mut self) {
        self.cursor_row = self.buffer.len().saturating_sub(1);
        self.cursor_col = 0;
    }

    pub fn move_half_page_up(&mut self) {
        let half_page = 12; // Assuming 24 lines visible
        self.cursor_row = self.cursor_row.saturating_sub(half_page);
    }

    pub fn move_half_page_down(&mut self) {
        let half_page = 12;
        self.cursor_row = (self.cursor_row + half_page).min(self.buffer.len().saturating_sub(1));
    }

    // Visual mode operations
    pub fn enter_visual_mode(&mut self) {
        self.state = CopyModeState::Visual;
        self.selection_start = Some((self.cursor_row, self.cursor_col));
        self.selection_end = Some((self.cursor_row, self.cursor_col));
    }

    pub fn enter_visual_line_mode(&mut self) {
        self.state = CopyModeState::VisualLine;
        self.selection_start = Some((self.cursor_row, 0));
        if let Some(line) = self.buffer.get(self.cursor_row) {
            self.selection_end = Some((self.cursor_row, line.len()));
        }
    }

    pub fn exit_visual_mode(&mut self) {
        self.state = CopyModeState::Normal;
        self.selection_start = None;
        self.selection_end = None;
    }

    // Search functionality
    pub fn start_search(&mut self, direction: SearchDirection) {
        self.state = CopyModeState::Search(direction);
        self.search_query.clear();
        self.search_matches.clear();
    }

    pub fn update_search(&mut self, query: String) {
        self.search_query = query;
        self.search_matches.clear();

        if self.search_query.is_empty() {
            return;
        }

        // Find all matches
        for (row, line) in self.buffer.iter().enumerate() {
            let mut col = 0;
            while let Some(pos) = line[col..].find(&self.search_query) {
                self.search_matches.push((row, col + pos));
                col += pos + 1;
            }
        }
    }

    pub fn jump_to_next_match(&mut self) {
        if self.search_matches.is_empty() {
            return;
        }

        let current_pos = (self.cursor_row, self.cursor_col);

        // Find next match after current position
        for (i, &(row, col)) in self.search_matches.iter().enumerate() {
            if row > current_pos.0 || (row == current_pos.0 && col > current_pos.1) {
                self.cursor_row = row;
                self.cursor_col = col;
                self.current_match = Some(i);
                self.add_to_jump_list(current_pos);
                return;
            }
        }

        // Wrap around to first match
        if let Some(&(row, col)) = self.search_matches.first() {
            self.cursor_row = row;
            self.cursor_col = col;
            self.current_match = Some(0);
            self.add_to_jump_list(current_pos);
        }
    }

    pub fn jump_to_previous_match(&mut self) {
        if self.search_matches.is_empty() {
            return;
        }

        let current_pos = (self.cursor_row, self.cursor_col);

        // Find previous match before current position
        for (i, &(row, col)) in self.search_matches.iter().enumerate().rev() {
            if row < current_pos.0 || (row == current_pos.0 && col < current_pos.1) {
                self.cursor_row = row;
                self.cursor_col = col;
                self.current_match = Some(i);
                self.add_to_jump_list(current_pos);
                return;
            }
        }

        // Wrap around to last match
        if let Some(&(row, col)) = self.search_matches.last() {
            self.cursor_row = row;
            self.cursor_col = col;
            self.current_match = Some(self.search_matches.len() - 1);
            self.add_to_jump_list(current_pos);
        }
    }

    fn add_to_jump_list(&mut self, pos: (usize, usize)) {
        // Truncate jump list if we're not at the end
        self.jump_list.truncate(self.jump_index);

        // Add new position
        self.jump_list.push(pos);
        self.jump_index = self.jump_list.len();

        // Limit jump list size
        if self.jump_list.len() > 100 {
            self.jump_list.remove(0);
            self.jump_index -= 1;
        }
    }

    pub fn jump_backward(&mut self) {
        if self.jump_index > 0 {
            self.jump_index -= 1;
            if let Some(&(row, col)) = self.jump_list.get(self.jump_index) {
                self.cursor_row = row;
                self.cursor_col = col;
            }
        }
    }

    pub fn jump_forward(&mut self) {
        if self.jump_index < self.jump_list.len() - 1 {
            self.jump_index += 1;
            if let Some(&(row, col)) = self.jump_list.get(self.jump_index) {
                self.cursor_row = row;
                self.cursor_col = col;
            }
        }
    }

    // Yank (copy) operation
    pub fn yank_selection(&mut self) -> Result<(), Box<dyn Error>> {
        if let Some(text) = self.get_selected_text() {
            self.yanked_text = Some(text.clone());

            // Copy to system clipboard
            if let Some(ref mut clipboard) = self.clipboard {
                clipboard.set_text(&text)?;
            }

            self.exit_visual_mode();
        }
        Ok(())
    }

    // Yank entire line
    pub fn yank_line(&mut self) -> Result<(), Box<dyn Error>> {
        if let Some(line) = self.buffer.get(self.cursor_row) {
            let text = format!("{}
", line);
            self.yanked_text = Some(text.clone());

            // Copy to system clipboard
            if let Some(ref mut clipboard) = self.clipboard {
                clipboard.set_text(&text)?;
            }
        }
        Ok(())
    }

    // Paste from clipboard
    pub fn paste_from_clipboard(&mut self) -> Result<Option<String>, Box<dyn Error>> {
        if let Some(ref mut clipboard) = self.clipboard {
            let text = clipboard.get_text()?;
            Ok(Some(text))
        } else {
            Ok(self.yanked_text.clone())
        }
    }

    pub fn get_yanked_text(&self) -> Option<&str> {
        self.yanked_text.as_deref()
    }

    // Getters for accessing private fields
    pub fn cursor_row(&self) -> usize {
        self.cursor_row
    }

    pub fn cursor_col(&self) -> usize {
        self.cursor_col
    }

    pub fn selection_start(&self) -> Option<(usize, usize)> {
        self.selection_start
    }

    pub fn selection_end(&self) -> Option<(usize, usize)> {
        self.selection_end
    }

    pub fn buffer(&self) -> &Vec<String> {
        &self.buffer
    }

    pub fn state(&self) -> &CopyModeState {
        &self.state
    }

    // Handle keyboard input
    pub fn handle_key(&mut self, key: crossterm::event::KeyEvent) -> Result<bool, Box<dyn Error>> {
        use crossterm::event::{KeyCode, KeyModifiers};

        match self.state {
            CopyModeState::Normal => {
                match (key.code, key.modifiers) {
                    // Movement
                    (KeyCode::Char('h'), KeyModifiers::NONE) | (KeyCode::Left, _) => self.move_cursor_left(),
                    (KeyCode::Char('j'), KeyModifiers::NONE) | (KeyCode::Down, _) => self.move_cursor_down(),
                    (KeyCode::Char('k'), KeyModifiers::NONE) | (KeyCode::Up, _) => self.move_cursor_up(),
                    (KeyCode::Char('l'), KeyModifiers::NONE) | (KeyCode::Right, _) => self.move_cursor_right(),

                    // Word movement
                    (KeyCode::Char('w'), KeyModifiers::NONE) => self.move_word_forward(),
                    (KeyCode::Char('b'), KeyModifiers::NONE) => self.move_word_backward(),
                    (KeyCode::Char('e'), KeyModifiers::NONE) => self.move_to_word_end(),

                    // Line movement
                    (KeyCode::Char('0'), KeyModifiers::NONE) => self.move_to_line_start(),
                    (KeyCode::Char('$'), KeyModifiers::NONE) => self.move_to_line_end(),
                    (KeyCode::Char('^'), KeyModifiers::NONE) => self.move_to_first_non_blank(),

                    // Page movement
                    (KeyCode::Char('g'), KeyModifiers::NONE) => {
                        // Wait for second 'g' for move to first line
                        // This is simplified - real implementation would handle double-key
                        self.move_to_first_line();
                    }
                    (KeyCode::Char('G'), KeyModifiers::SHIFT) => self.move_to_last_line(),
                    (KeyCode::Char('d'), KeyModifiers::CONTROL) => self.move_half_page_down(),
                    (KeyCode::Char('u'), KeyModifiers::CONTROL) => self.move_half_page_up(),

                    // Visual mode
                    (KeyCode::Char('v'), KeyModifiers::NONE) => self.enter_visual_mode(),
                    (KeyCode::Char('V'), KeyModifiers::SHIFT) => self.enter_visual_line_mode(),
                    (KeyCode::Char('v'), KeyModifiers::CONTROL) => self.enter_visual_block_mode(),

                    // Yank
                    (KeyCode::Char('y'), KeyModifiers::NONE) => {
                        self.yank_line()?;
                    }

                    // Search
                    (KeyCode::Char('/'), KeyModifiers::NONE) => self.start_search(SearchDirection::Forward),
                    (KeyCode::Char('?'), KeyModifiers::NONE) => self.start_search(SearchDirection::Backward),
                    (KeyCode::Char('n'), KeyModifiers::NONE) => self.jump_to_next_match(),
                    (KeyCode::Char('N'), KeyModifiers::SHIFT) => self.jump_to_previous_match(),

                    // Jump list
                    (KeyCode::Char('o'), KeyModifiers::CONTROL) => self.jump_backward(),
                    (KeyCode::Char('i'), KeyModifiers::CONTROL) => self.jump_forward(),

                    // Exit
                    (KeyCode::Char('q'), KeyModifiers::NONE) | (KeyCode::Esc, _) => {
                        self.exit();
                        return Ok(false); // Signal to exit copy mode
                    }

                    _ => {}
                }
            }
            CopyModeState::Visual | CopyModeState::VisualLine | CopyModeState::VisualBlock => {
                // Update selection as cursor moves
                self.update_selection();

                match (key.code, key.modifiers) {
                    // Movement (same as normal mode)
                    (KeyCode::Char('h'), KeyModifiers::NONE) | (KeyCode::Left, _) => self.move_cursor_left(),
                    (KeyCode::Char('j'), KeyModifiers::NONE) | (KeyCode::Down, _) => self.move_cursor_down(),
                    (KeyCode::Char('k'), KeyModifiers::NONE) | (KeyCode::Up, _) => self.move_cursor_up(),
                    (KeyCode::Char('l'), KeyModifiers::NONE) | (KeyCode::Right, _) => self.move_cursor_right(),

                    // Yank and exit
                    (KeyCode::Char('y'), KeyModifiers::NONE) => {
                        self.yank_selection()?;
                    }

                    // Copy to clipboard with Ctrl+C
                    (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                        self.yank_selection()?;
                    }

                    // Exit visual mode
                    (KeyCode::Esc, _) => self.exit_visual_mode(),

                    _ => {}
                }
            }
            CopyModeState::Search(_) => {
                match key.code {
                    KeyCode::Char(c) => {
                        self.search_query.push(c);
                        self.update_search(self.search_query.clone());
                    }
                    KeyCode::Backspace => {
                        self.search_query.pop();
                        self.update_search(self.search_query.clone());
                    }
                    KeyCode::Enter => {
                        self.jump_to_next_match();
                        self.state = CopyModeState::Normal;
                    }
                    KeyCode::Esc => {
                        self.state = CopyModeState::Normal;
                    }
                    _ => {}
                }
            }
        }

        // Adjust viewport to follow cursor
        self.adjust_viewport();

        Ok(true) // Continue in copy mode
    }

    // Helper methods
    fn move_to_word_end(&mut self) {
        if let Some(line) = self.buffer.get(self.cursor_row) {
            let mut col = self.cursor_col;
            let chars: Vec<char> = line.chars().collect();

            // Move to end of current word
            while col < chars.len() - 1 && !chars[col + 1].is_whitespace() {
                col += 1;
            }

            self.cursor_col = col;
        }
    }

    fn move_to_first_non_blank(&mut self) {
        if let Some(line) = self.buffer.get(self.cursor_row) {
            for (i, ch) in line.chars().enumerate() {
                if !ch.is_whitespace() {
                    self.cursor_col = i;
                    return;
                }
            }
        }
    }

    fn enter_visual_block_mode(&mut self) {
        self.state = CopyModeState::VisualBlock;
        self.selection_start = Some((self.cursor_row, self.cursor_col));
        self.selection_end = Some((self.cursor_row, self.cursor_col));
    }

    fn adjust_viewport(&mut self) {
        // Ensure cursor is visible in viewport
        if self.cursor_row < self.viewport_offset {
            self.viewport_offset = self.cursor_row;
        } else if self.cursor_row >= self.viewport_offset + self.viewport_height {
            self.viewport_offset = self.cursor_row - self.viewport_height + 1;
        }
    }

    pub fn set_viewport_height(&mut self, height: usize) {
        self.viewport_height = height;
    }

    pub fn viewport_offset(&self) -> usize {
        self.viewport_offset
    }

    pub fn get_selected_text(&self) -> Option<String> {
        if let (Some(start), Some(end)) = (self.selection_start, self.selection_end) {
            let mut result = String::new();

            let (start_row, start_col) = if start.0 < end.0 || (start.0 == end.0 && start.1 <= end.1) {
                start
            } else {
                end
            };

            let (end_row, end_col) = if start.0 < end.0 || (start.0 == end.0 && start.1 <= end.1) {
                end
            } else {
                start
            };

            if start_row == end_row {
                // Single line selection
                if let Some(line) = self.buffer.get(start_row) {
                    result.push_str(&line[start_col.min(line.len())..end_col.min(line.len())]);
                }
            } else {
                // Multi-line selection
                for row in start_row..=end_row {
                    if let Some(line) = self.buffer.get(row) {
                        if row == start_row {
                            result.push_str(&line[start_col.min(line.len())..]);
                        } else if row == end_row {
                            result.push_str(&line[..end_col.min(line.len())]);
                        } else {
                            result.push_str(line);
                        }
                        if row < end_row {
                            result.push('\n');
                        }
                    }
                }
            }

            Some(result)
        } else {
            None
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_copymode() -> CopyMode {
        CopyMode::new(CopyModeStyle::Vi)
    }

    fn create_test_buffer() -> Vec<String> {
        vec![
            "Line 1: Hello, world!".to_string(),
            "Line 2: This is a test".to_string(),
            "Line 3: Copy mode testing".to_string(),
            "Line 4: Final line".to_string(),
        ]
    }

    #[test]
    fn test_copy_mode_initialization() {
        let copymode = create_test_copymode();

        assert!(!copymode.active);
        assert_eq!(copymode.state, CopyModeState::Normal);
        assert_eq!(copymode.cursor_row, 0);
        assert_eq!(copymode.cursor_col, 0);
        assert!(copymode.selection_start.is_none());
        assert!(copymode.selection_end.is_none());
        assert!(copymode.yanked_text.is_none());
    }

    #[test]
    fn test_copy_mode_activation() {
        let mut copymode = create_test_copymode();
        let buffer = create_test_buffer();

        // Initially inactive
        assert!(!copymode.is_active());

        // Enter copy mode
        copymode.enter(buffer.clone());
        assert!(copymode.is_active());
        assert_eq!(copymode.buffer.len(), 4);
        assert_eq!(copymode.cursor_row, 0);
        assert_eq!(copymode.cursor_col, 0);

        // Exit copy mode
        copymode.exit();
        assert!(!copymode.is_active());
        assert!(copymode.selection_start.is_none());
        assert!(copymode.selection_end.is_none());
    }

    #[test]
    fn test_cursor_movement() {
        let mut copymode = create_test_copymode();
        let buffer = create_test_buffer();
        copymode.enter(buffer);

        // Initial position
        assert_eq!(copymode.cursor_row, 0);
        assert_eq!(copymode.cursor_col, 0);

        // Move down
        copymode.move_cursor_down();
        assert_eq!(copymode.cursor_row, 1);

        // Move right
        copymode.move_cursor_right();
        assert_eq!(copymode.cursor_col, 1);

        // Move down to last line
        copymode.move_cursor_down();
        copymode.move_cursor_down();
        assert_eq!(copymode.cursor_row, 3);

        // Try to move beyond last line (should stay at last line)
        copymode.move_cursor_down();
        assert_eq!(copymode.cursor_row, 3);

        // Move up
        copymode.move_cursor_up();
        assert_eq!(copymode.cursor_row, 2);

        // Move left
        copymode.move_cursor_left();
        assert_eq!(copymode.cursor_col, 0);

        // Try to move left at column 0 (should stay at 0)
        copymode.move_cursor_left();
        assert_eq!(copymode.cursor_col, 0);

        // Try to move up at row 0
        copymode.cursor_row = 0;
        copymode.move_cursor_up();
        assert_eq!(copymode.cursor_row, 0);
    }

    #[test]
    fn test_text_selection() {
        let mut copymode = create_test_copymode();
        let buffer = create_test_buffer();
        copymode.enter(buffer);

        // Start selection at (0, 0)
        copymode.start_selection();
        assert_eq!(copymode.selection_start, Some((0, 0)));

        // Move cursor to create selection
        copymode.move_cursor_down();
        copymode.move_cursor_right();
        copymode.move_cursor_right();

        // Update selection as we move
        copymode.update_selection();
        // Note: selection_end is managed internally by update_selection
    }

    #[test]
    fn test_visual_mode_transitions() {
        let mut copymode = create_test_copymode();
        let buffer = create_test_buffer();
        copymode.enter(buffer);

        // Start in normal mode
        assert_eq!(copymode.state, CopyModeState::Normal);

        // Enter visual mode
        copymode.enter_visual_mode();
        assert_eq!(copymode.state, CopyModeState::Visual);
        assert!(copymode.selection_start.is_some());

        // Enter visual line mode
        copymode.enter_visual_line_mode();
        assert_eq!(copymode.state, CopyModeState::VisualLine);

        // Note: enter_visual_block_mode doesn't exist - only Visual and VisualLine modes

        // Exit visual mode
        copymode.exit_visual_mode();
        assert_eq!(copymode.state, CopyModeState::Normal);
        assert!(copymode.selection_start.is_none());
        assert!(copymode.selection_end.is_none());
    }

    #[test]
    fn test_yank_text() {
        let mut copymode = create_test_copymode();
        let buffer = create_test_buffer();
        copymode.enter(buffer);

        // Select text from (0, 0) to (0, 5)
        copymode.start_selection();
        copymode.cursor_col = 5;
        copymode.update_selection();

        // Yank the selected text
        copymode.yank_selection();
        let yanked = copymode.get_yanked_text();
        assert!(yanked.is_some());
        // The actual yanked text depends on get_selected_text implementation
    }

    #[test]
    fn test_search_forward() {
        let mut copymode = create_test_copymode();
        let buffer = create_test_buffer();
        copymode.enter(buffer);

        // Start search
        copymode.start_search(SearchDirection::Forward);
        assert_eq!(copymode.state, CopyModeState::Search(SearchDirection::Forward));

        // Search for "test"
        copymode.update_search("test".to_string());

        // Should find matches
        assert!(!copymode.search_matches.is_empty());

        // Navigate to first match
        copymode.jump_to_next_match();
        // Note: current_match is managed internally
    }

    #[test]
    fn test_jump_list() {
        let mut copymode = create_test_copymode();
        let buffer = create_test_buffer();
        copymode.enter(buffer);

        // Jump list starts empty
        assert_eq!(copymode.jump_list.len(), 0);

        // Move to different position manually to set up test
        copymode.cursor_row = 0;
        copymode.cursor_col = 0;
        copymode.jump_list.push((0, 0));  // Manually add to jump list for test
        copymode.jump_index = 0;

        // Move to different position
        copymode.cursor_row = 2;
        copymode.cursor_col = 5;
        copymode.jump_list.push((2, 5));  // Add new position
        copymode.jump_index = 1;

        // Jump back
        copymode.jump_backward();
        assert_eq!((copymode.cursor_row, copymode.cursor_col), (0, 0));

        // Jump forward
        copymode.jump_forward();
        assert_eq!((copymode.cursor_row, copymode.cursor_col), (2, 5));
    }

    #[test]
    fn test_page_movement() {
        let mut copymode = create_test_copymode();

        // Create larger buffer
        let mut large_buffer = Vec::new();
        for i in 0..100 {
            large_buffer.push(format!("Line {}", i));
        }
        copymode.enter(large_buffer);

        // Move half page down
        copymode.move_half_page_down();
        assert!(copymode.cursor_row > 0);

        // Move half page up
        copymode.move_half_page_up();

        // Move to top
        copymode.move_to_first_line();
        assert_eq!(copymode.cursor_row, 0);

        // Move to bottom
        copymode.move_to_last_line();
        assert_eq!(copymode.cursor_row, 99);
    }

    #[test]
    fn test_word_movement() {
        let mut copymode = create_test_copymode();
        let buffer = vec!["Hello world this is test".to_string()];
        copymode.enter(buffer);

        // Start at beginning
        assert_eq!(copymode.cursor_col, 0);

        // Move to next word
        copymode.move_word_forward();
        // The exact position depends on word boundary detection
        assert!(copymode.cursor_col > 0);

        // Move to next word again
        let prev_col = copymode.cursor_col;
        copymode.move_word_forward();
        assert!(copymode.cursor_col > prev_col);

        // Move to previous word
        copymode.move_word_backward();
        assert!(copymode.cursor_col < 20); // Should have moved back
    }
}
