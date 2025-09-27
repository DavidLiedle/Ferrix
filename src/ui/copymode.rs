use crate::config::CopyModeStyle;

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
    mode: CopyModeStyle,
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
}

impl CopyMode {
    pub fn new(mode: CopyModeStyle) -> Self {
        Self {
            active: false,
            mode,
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
    pub fn yank_selection(&mut self) {
        if let Some(text) = self.get_selected_text() {
            self.yanked_text = Some(text);
            self.exit_visual_mode();
        }
    }

    pub fn get_yanked_text(&self) -> Option<&str> {
        self.yanked_text.as_deref()
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