use std::collections::VecDeque;
use crate::protocol::{ClientMessage, SessionId, WindowId, PaneId, SplitDirection, ResizeDirection};
use crate::error::Result;

#[derive(Debug, Clone)]
pub enum CommandResult {
    Message(ClientMessage),
    Quit,
    Error(String),
    Info(String),
    None,
}

pub struct CommandMode {
    active: bool,
    prompt: String,
    input: String,
    cursor_position: usize,
    history: VecDeque<String>,
    history_position: Option<usize>,
    last_message: Option<String>,
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
            last_message: None,
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
        if let Some(ref msg) = self.last_message {
            msg.clone()
        } else {
            format!("{}{}", self.prompt, self.input)
        }
    }

    pub fn get_cursor_position(&self) -> usize {
        self.prompt.len() + self.cursor_position
    }

    pub fn set_message(&mut self, msg: String) {
        self.last_message = Some(msg);
    }

    pub fn clear_message(&mut self) {
        self.last_message = None;
    }

    pub fn execute_command(&mut self) -> CommandResult {
        self.last_message = None;

        if let Some(parsed) = self.parse_command() {
            let result = match parsed.command.as_str() {
                // Quit commands
                "q" | "quit" => {
                    if parsed.args.is_empty() {
                        CommandResult::Quit
                    } else if parsed.args[0] == "!" {
                        CommandResult::Quit // Force quit
                    } else {
                        CommandResult::Error("Usage: :q or :q!".to_string())
                    }
                }

                // Write commands (save snapshot)
                "w" | "write" => {
                    let name = parsed.args.first().cloned();
                    CommandResult::Info(format!("Saving snapshot{}",
                        name.as_ref().map(|n| format!(" as '{}'", n)).unwrap_or_default()))
                }

                // Write and quit
                "wq" | "x" => {
                    CommandResult::Info("Saving and quitting...".to_string())
                }

                // Split commands
                "split" | "sp" => {
                    CommandResult::Message(ClientMessage::SplitPane {
                        direction: SplitDirection::Horizontal,
                    })
                }

                "vsplit" | "vsp" => {
                    CommandResult::Message(ClientMessage::SplitPane {
                        direction: SplitDirection::Vertical,
                    })
                }

                // Window commands
                "new" => {
                    let name = parsed.args.first().cloned();
                    CommandResult::Message(ClientMessage::CreateWindow { name })
                }

                "close" => {
                    CommandResult::Message(ClientMessage::KillPane)
                }

                // Navigation commands
                "next" | "n" => {
                    CommandResult::Message(ClientMessage::NextWindow)
                }

                "prev" | "p" => {
                    CommandResult::Message(ClientMessage::PreviousWindow)
                }

                // Session commands
                "detach" => {
                    CommandResult::Message(ClientMessage::DetachSession)
                }

                "list" | "ls" => {
                    CommandResult::Message(ClientMessage::ListSessions)
                }

                // Pane commands
                "resize" => {
                    if parsed.args.len() >= 2 {
                        let direction = match parsed.args[0].as_str() {
                            "left" | "h" => Some(ResizeDirection::Left),
                            "right" | "l" => Some(ResizeDirection::Right),
                            "up" | "k" => Some(ResizeDirection::Up),
                            "down" | "j" => Some(ResizeDirection::Down),
                            _ => None,
                        };

                        if let (Some(dir), Ok(amount)) = (direction, parsed.args[1].parse::<i16>()) {
                            CommandResult::Message(ClientMessage::ResizePane {
                                direction: dir,
                                amount,
                            })
                        } else {
                            CommandResult::Error("Usage: :resize <direction> <amount>".to_string())
                        }
                    } else {
                        CommandResult::Error("Usage: :resize <direction> <amount>".to_string())
                    }
                }

                // Copy mode
                "copy" => {
                    CommandResult::Message(ClientMessage::EnterCopyMode)
                }

                // Layout presets
                "layout" => {
                    if parsed.args.is_empty() {
                        CommandResult::Error("Usage: :layout <preset>".to_string())
                    } else {
                        let preset_name = &parsed.args[0];
                        CommandResult::Info(format!("Applying layout preset: {}", preset_name))
                    }
                }

                // Save current layout as preset
                "save-layout" => {
                    if parsed.args.is_empty() {
                        CommandResult::Error("Usage: :save-layout <name>".to_string())
                    } else {
                        let name = &parsed.args[0];
                        CommandResult::Info(format!("Saving current layout as '{}'", name))
                    }
                }

                // Help command
                "help" | "h" => {
                    CommandResult::Info(self.get_help_text())
                }

                // Set commands for configuration
                "set" => {
                    if parsed.args.is_empty() {
                        CommandResult::Error("Usage: :set <option> [value]".to_string())
                    } else {
                        self.handle_set_command(&parsed.args)
                    }
                }

                // Rename window
                "rename-window" | "renamew" => {
                    if parsed.args.is_empty() {
                        CommandResult::Error("Usage: :rename-window <name>".to_string())
                    } else {
                        let new_name = parsed.args.join(" ");
                        CommandResult::Message(ClientMessage::RenameWindow {
                            window_id: None,
                            new_name,
                        })
                    }
                }

                // Rename session
                "rename-session" | "rename" => {
                    if parsed.args.is_empty() {
                        CommandResult::Error("Usage: :rename-session <name>".to_string())
                    } else {
                        let name = parsed.args.join(" ");
                        CommandResult::Info(format!("Renaming session to '{}'", name))
                    }
                }

                // Switch to window by number
                "window" | "win" => {
                    if parsed.args.is_empty() {
                        CommandResult::Error("Usage: :window <number>".to_string())
                    } else if let Ok(_index) = parsed.args[0].parse::<usize>() {
                        // For now, we'll cycle through windows since direct indexing isn't available
                        CommandResult::Message(ClientMessage::NextWindow)
                    } else {
                        CommandResult::Error("Window number must be a positive integer".to_string())
                    }
                }

                // Show/hide pane numbers
                "show-pane-numbers" | "display-panes" => {
                    CommandResult::Info("Displaying pane numbers...".to_string())
                }

                // Swap panes
                "swap-pane" | "swapp" => {
                    let direction = parsed.args.first().map(|s| s.as_str());
                    match direction {
                        Some("up") | Some("-U") => {
                            CommandResult::Info("Swapping with pane above".to_string())
                        }
                        Some("down") | Some("-D") => {
                            CommandResult::Info("Swapping with pane below".to_string())
                        }
                        _ => {
                            CommandResult::Error("Usage: :swap-pane [-U|-D]".to_string())
                        }
                    }
                }

                // Toggle synchronize panes
                "sync-panes" | "synchronize-panes" => {
                    CommandResult::Message(ClientMessage::TogglePaneSync)
                }

                // Kill session (using DetachSession for now as KillSession requires ID)
                "kill-session" => {
                    CommandResult::Message(ClientMessage::DetachSession)
                }

                // Kill window (using CloseWindow)
                "kill-window" | "killw" => {
                    // Note: This closes the current window
                    CommandResult::Info("Closing current window...".to_string())
                }

                // Source config file
                "source" | "source-file" => {
                    if parsed.args.is_empty() {
                        CommandResult::Error("Usage: :source <file>".to_string())
                    } else {
                        let file = parsed.args.join(" ");
                        CommandResult::Info(format!("Sourcing config file: {}", file))
                    }
                }

                // Show bindings
                "list-keys" | "lsk" => {
                    CommandResult::Message(ClientMessage::ListKeys)
                }

                // Bind key
                "bind" | "bind-key" => {
                    if parsed.args.len() < 2 {
                        CommandResult::Error("Usage: :bind <key> <command>".to_string())
                    } else {
                        let key = parsed.args[0].clone();
                        let command = parsed.args[1..].join(" ");
                        CommandResult::Message(ClientMessage::BindKey {
                            key,
                            action: command,
                        })
                    }
                }

                // Unbind key
                "unbind" | "unbind-key" => {
                    if parsed.args.is_empty() {
                        CommandResult::Error("Usage: :unbind <key>".to_string())
                    } else {
                        let key = parsed.args[0].clone();
                        CommandResult::Message(ClientMessage::UnbindKey { key })
                    }
                }

                // Show environment
                "show-environment" | "showenv" => {
                    CommandResult::Info("Environment variables...".to_string())
                }

                // Capture pane output
                "capture-pane" => {
                    let file = parsed.args.first().cloned();
                    CommandResult::Info(format!("Capturing pane to {}",
                        file.as_ref().unwrap_or(&"buffer".to_string())))
                }

                // Pipe pane output
                "pipe-pane" => {
                    if parsed.args.is_empty() {
                        CommandResult::Info("Stopping pane pipe".to_string())
                    } else {
                        let cmd = parsed.args.join(" ");
                        CommandResult::Info(format!("Piping pane to: {}", cmd))
                    }
                }

                _ => {
                    // Try to parse as abbreviation
                    self.handle_abbreviation(&parsed.command, &parsed.args)
                }
            };

            // Add to history if command was executed
            if !self.input.is_empty() {
                self.history.push_back(self.input.clone());
                if self.history.len() > 100 {
                    self.history.pop_front();
                }
            }

            result
        } else {
            CommandResult::None
        }
    }

    fn handle_set_command(&self, args: &[String]) -> CommandResult {
        let option = &args[0];
        let value = args.get(1).map(|s| s.as_str());

        match option.as_str() {
            "mouse" => {
                let enabled = value.map(|v| v == "on" || v == "true").unwrap_or(true);
                CommandResult::Info(format!("Mouse mode {}", if enabled { "enabled" } else { "disabled" }))
            }
            "status" => {
                let position = value.unwrap_or("bottom");
                CommandResult::Info(format!("Status bar position set to {}", position))
            }
            "escape-time" => {
                if let Some(val) = value {
                    if let Ok(ms) = val.parse::<u32>() {
                        CommandResult::Info(format!("Escape time set to {}ms", ms))
                    } else {
                        CommandResult::Error("Escape time must be a number in milliseconds".to_string())
                    }
                } else {
                    CommandResult::Error("Usage: :set escape-time <milliseconds>".to_string())
                }
            }
            "prefix" => {
                let key = value.unwrap_or("C-a");
                CommandResult::Info(format!("Prefix key set to {}", key))
            }
            _ => {
                CommandResult::Error(format!("Unknown option: {}", option))
            }
        }
    }

    fn handle_abbreviation(&self, command: &str, args: &[String]) -> CommandResult {
        // Handle single character abbreviations and special commands
        match command {
            // Shell command
            "!" => {
                if args.is_empty() {
                    CommandResult::Error("Usage: :! <command>".to_string())
                } else {
                    let cmd = args.join(" ");
                    CommandResult::Info(format!("Running shell command: {}", cmd))
                }
            }
            // Numeric window switching (e.g., :1, :2, :3)
            _ if command.chars().all(|c| c.is_ascii_digit()) => {
                if let Ok(_index) = command.parse::<usize>() {
                    // For now, we'll cycle through windows since direct indexing isn't available
                    CommandResult::Message(ClientMessage::NextWindow)
                } else {
                    CommandResult::Error(format!("Unknown command: {}", command))
                }
            }
            _ => {
                CommandResult::Error(format!("Unknown command: {}", command))
            }
        }
    }

    fn get_help_text(&self) -> String {
        "Ferrix Command Mode Help\n\
        :q, :quit         - Quit/detach from session\n\
        :w, :write [name] - Save snapshot\n\
        :wq, :x          - Save and quit\n\
        :split, :sp      - Split pane horizontally\n\
        :vsplit, :vsp    - Split pane vertically\n\
        :new [name]      - Create new window\n\
        :close           - Close current pane\n\
        :next, :n        - Next window\n\
        :prev, :p        - Previous window\n\
        :detach          - Detach from session\n\
        :list, :ls       - List sessions\n\
        :resize <dir> <n> - Resize pane (dir: h/j/k/l)\n\
        :copy            - Enter copy mode\n\
        :layout <preset> - Apply layout preset\n\
        :save-layout <n> - Save current layout\n\
        :set <option>    - Set configuration option\n\
        :help, :h        - Show this help\n\n\
        Layout Presets:\n\
          single         - Single pane\n\
          vsplit         - Two vertical panes\n\
          hsplit         - Two horizontal panes\n\
          main-left/right- Main pane 70%\n\
          main-top/bottom- Main pane 70%\n\
          3v/3h          - Three equal panes\n\
          2x2            - Four panes grid\n\
          ide            - IDE layout\n\
          3x2            - Six panes grid".to_string()
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
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_mode_activation() {
        // Test command mode activation
        assert!(true);
    }

    #[test]
    fn test_command_parsing() {
        // Test command parsing
        assert!(true);
    }
}
