use std::collections::VecDeque;

pub struct CommandMode {
    active: bool,
    prompt: String,
    input: String,
    cursor_position: usize,
    history: VecDeque<String>,
    history_position: Option<usize>,
}

impl CommandMode {
    pub fn new() -> Self {
        Self {
            active: false,
            prompt: ":".to_string(),
            input: String::new(),
            cursor_position: 0,
            history: VecDeque::with_capacity(100),
            history_position: None,
        }
    }

    pub fn enter(&mut self) {
        self.active = true;
        self.input.clear();
        self.cursor_position = 0;
        self.history_position = None;
    }

    pub fn exit(&mut self) {
        self.active = false;
        if !self.input.is_empty() {
            self.history.push_back(self.input.clone());
            if self.history.len() > 100 {
                self.history.pop_front();
            }
        }
        self.input.clear();
        self.cursor_position = 0;
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn insert_char(&mut self, ch: char) {
        self.input.insert(self.cursor_position, ch);
        self.cursor_position += 1;
    }

    pub fn delete_char(&mut self) {
        if self.cursor_position > 0 {
            self.cursor_position -= 1;
            self.input.remove(self.cursor_position);
        }
    }

    pub fn move_cursor_left(&mut self) {
        if self.cursor_position > 0 {
            self.cursor_position -= 1;
        }
    }

    pub fn move_cursor_right(&mut self) {
        if self.cursor_position < self.input.len() {
            self.cursor_position += 1;
        }
    }

    pub fn move_cursor_home(&mut self) {
        self.cursor_position = 0;
    }

    pub fn move_cursor_end(&mut self) {
        self.cursor_position = self.input.len();
    }

    pub fn history_previous(&mut self) {
        if self.history.is_empty() {
            return;
        }

        match self.history_position {
            None => {
                self.history_position = Some(self.history.len() - 1);
                self.input = self.history[self.history.len() - 1].clone();
                self.cursor_position = self.input.len();
            }
            Some(pos) if pos > 0 => {
                self.history_position = Some(pos - 1);
                self.input = self.history[pos - 1].clone();
                self.cursor_position = self.input.len();
            }
            _ => {}
        }
    }

    pub fn history_next(&mut self) {
        match self.history_position {
            Some(pos) if pos < self.history.len() - 1 => {
                self.history_position = Some(pos + 1);
                self.input = self.history[pos + 1].clone();
                self.cursor_position = self.input.len();
            }
            Some(_) => {
                self.history_position = None;
                self.input.clear();
                self.cursor_position = 0;
            }
            None => {}
        }
    }

    pub fn get_command(&self) -> &str {
        &self.input
    }

    pub fn get_display(&self) -> String {
        format!("{}{}", self.prompt, self.input)
    }

    pub fn parse_command(&self) -> Option<ParsedCommand> {
        let trimmed = self.input.trim();
        if trimmed.is_empty() {
            return None;
        }

        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.is_empty() {
            return None;
        }

        let command = parts[0].to_string();
        let args = parts[1..].iter().map(|s| s.to_string()).collect();

        Some(ParsedCommand { command, args })
    }
}

#[derive(Debug, Clone)]
pub struct ParsedCommand {
    pub command: String,
    pub args: Vec<String>,
}

impl ParsedCommand {
    pub fn is(&self, cmd: &str) -> bool {
        self.command == cmd || self.command.starts_with(cmd)
    }
}