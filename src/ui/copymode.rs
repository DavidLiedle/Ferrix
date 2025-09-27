use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::config::CopyModeStyle;

pub struct CopyMode {
    active: bool,
    mode: CopyModeStyle,
    buffer: Vec<String>,
    cursor_row: usize,
    cursor_col: usize,
    selection_start: Option<(usize, usize)>,
    selection_end: Option<(usize, usize)>,
}

impl CopyMode {
    pub fn new(mode: CopyModeStyle) -> Self {
        Self {
            active: false,
            mode,
            buffer: Vec::new(),
            cursor_row: 0,
            cursor_col: 0,
            selection_start: None,
            selection_end: None,
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