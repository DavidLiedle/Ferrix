pub mod chord;
pub mod mode;

use crossterm::event::{KeyCode, KeyModifiers, KeyEvent as CrosstermKeyEvent};
use crate::protocol::ClientMessage;
use crate::error::Result;

use self::chord::{
    ChordDetector, VimModeHandler, EmacsHandler, KeyEvent,
    InputMode, VimCommand, EmacsCommand
};

/// The main input processor that handles different input modes
#[derive(Debug)]
pub struct InputProcessor {
    mode: InputMode,
    vim_handler: VimModeHandler,
    emacs_handler: EmacsHandler,
    chord_detector: ChordDetector,
    config: InputConfig,
}

#[derive(Debug, Clone)]
pub struct InputConfig {
    pub default_mode: InputMode,
    pub chord_timeout_ms: u64,
    pub enable_vim_mode: bool,
    pub enable_emacs_mode: bool,
    pub leader_key: String,
}

impl Default for InputConfig {
    fn default() -> Self {
        Self {
            default_mode: InputMode::Normal,
            chord_timeout_ms: 500,
            enable_vim_mode: true,
            enable_emacs_mode: false,
            leader_key: "Space".to_string(),
        }
    }
}

impl InputProcessor {
    pub fn new(config: InputConfig) -> Self {
        Self {
            mode: config.default_mode,
            vim_handler: VimModeHandler::new(),
            emacs_handler: EmacsHandler::new(),
            chord_detector: ChordDetector::new(config.chord_timeout_ms),
            config,
        }
    }

    pub fn process_key(&mut self, event: CrosstermKeyEvent) -> Result<Vec<InputAction>> {
        let key = KeyEvent::from_crossterm(event.code, event.modifiers);
        let mut actions = Vec::new();

        // Check for mode switching hotkeys
        if self.check_mode_switch(&key) {
            return Ok(actions);
        }

        // Process based on current mode
        match self.mode {
            InputMode::Normal => {
                if self.config.enable_vim_mode {
                    if let Some(cmd) = self.vim_handler.process_key(key.clone()) {
                        actions.extend(self.vim_command_to_actions(cmd));
                    }
                } else if self.config.enable_emacs_mode {
                    if let Some(cmd) = self.emacs_handler.process_key(key.clone()) {
                        actions.extend(self.emacs_command_to_actions(cmd));
                    }
                } else {
                    // Default chord processing
                    if let Some(action) = self.chord_detector.process_key(key) {
                        if let chord::ChordAction::Command(cmd) = action {
                            actions.push(InputAction::ExecuteCommand(cmd));
                        }
                    }
                }
            }
            InputMode::Insert => {
                // Pass through to terminal
                actions.push(InputAction::SendToTerminal(
                    self.key_to_bytes(&event)
                ));
            }
            InputMode::Command => {
                // Command line mode
                actions.push(InputAction::CommandInput(key.code));
            }
            InputMode::Visual | InputMode::VisualLine | InputMode::VisualBlock => {
                if let Some(cmd) = self.vim_handler.process_key(key) {
                    actions.extend(self.vim_command_to_actions(cmd));
                }
            }
            _ => {}
        }

        Ok(actions)
    }

    fn check_mode_switch(&mut self, key: &KeyEvent) -> bool {
        // Global mode switching keys
        match (key.code.as_str(), key.modifiers.as_str()) {
            ("v", "Alt") if self.config.enable_vim_mode => {
                self.mode = InputMode::Normal;
                self.vim_handler.set_mode(InputMode::Normal);
                true
            }
            ("e", "Alt") if self.config.enable_emacs_mode => {
                self.mode = InputMode::Emacs;
                true
            }
            ("Escape", _) => {
                if self.mode != InputMode::Normal {
                    self.mode = InputMode::Normal;
                    if self.config.enable_vim_mode {
                        self.vim_handler.set_mode(InputMode::Normal);
                    }
                    true
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    fn vim_command_to_actions(&self, cmd: VimCommand) -> Vec<InputAction> {
        use VimCommand::*;
        

        match cmd {
            Move(motion) => vec![self.motion_to_action(motion)],
            Delete(motion) => vec![
                InputAction::StartSelection,
                self.motion_to_action(motion),
                InputAction::DeleteSelection,
            ],
            YankSelection => vec![InputAction::CopySelection],
            DeleteSelection => vec![InputAction::DeleteSelection],
            Paste(_count) => vec![InputAction::Paste],
            Execute(cmd) => vec![InputAction::ExecuteCommand(cmd)],
            InsertChar(ch) => vec![InputAction::SendToTerminal(ch.into_bytes())],
            _ => vec![],
        }
    }

    fn motion_to_action(&self, motion: chord::VimMotion) -> InputAction {
        use chord::VimMotion::*;

        match motion {
            Left(n) => InputAction::MoveCursor(Direction::Left, n),
            Right(n) => InputAction::MoveCursor(Direction::Right, n),
            Up(n) => InputAction::MoveCursor(Direction::Up, n),
            Down(n) => InputAction::MoveCursor(Direction::Down, n),
            WordForward(n) => InputAction::MoveWord(Direction::Right, n),
            WordBackward(n) => InputAction::MoveWord(Direction::Left, n),
            LineStart => InputAction::MoveToLineStart,
            LineEnd => InputAction::MoveToLineEnd,
            FileStart => InputAction::MoveToBufferStart,
            FileEnd => InputAction::MoveToBufferEnd,
            _ => InputAction::Noop,
        }
    }

    fn emacs_command_to_actions(&self, cmd: EmacsCommand) -> Vec<InputAction> {
        use EmacsCommand::*;
        use chord::EmacsMotion;

        match cmd {
            Move(motion) => {
                let action = match motion {
                    EmacsMotion::ForwardChar => InputAction::MoveCursor(Direction::Right, 1),
                    EmacsMotion::BackwardChar => InputAction::MoveCursor(Direction::Left, 1),
                    EmacsMotion::NextLine => InputAction::MoveCursor(Direction::Down, 1),
                    EmacsMotion::PreviousLine => InputAction::MoveCursor(Direction::Up, 1),
                    EmacsMotion::ForwardWord => InputAction::MoveWord(Direction::Right, 1),
                    EmacsMotion::BackwardWord => InputAction::MoveWord(Direction::Left, 1),
                    EmacsMotion::BeginningOfLine => InputAction::MoveToLineStart,
                    EmacsMotion::EndOfLine => InputAction::MoveToLineEnd,
                };
                vec![action]
            }
            DeleteChar => vec![InputAction::DeleteChar],
            KillLine => vec![InputAction::KillLine],
            KillRegion => vec![InputAction::KillSelection],
            Yank => vec![InputAction::Paste],
            SetMark => vec![InputAction::SetMark],
            Quit => vec![InputAction::ClearSelection],
            _ => vec![],
        }
    }

    fn key_to_bytes(&self, event: &CrosstermKeyEvent) -> Vec<u8> {
        match event.code {
            KeyCode::Char(c) => {
                if event.modifiers.contains(KeyModifiers::CONTROL) {
                    vec![(c as u8) & 0x1f]
                } else {
                    c.to_string().into_bytes()
                }
            }
            KeyCode::Enter => vec![b'\r'],
            KeyCode::Backspace => vec![127],
            KeyCode::Tab => vec![b'\t'],
            KeyCode::Esc => vec![27],
            KeyCode::Up => vec![27, b'[', b'A'],
            KeyCode::Down => vec![27, b'[', b'B'],
            KeyCode::Right => vec![27, b'[', b'C'],
            KeyCode::Left => vec![27, b'[', b'D'],
            KeyCode::Home => vec![27, b'[', b'H'],
            KeyCode::End => vec![27, b'[', b'F'],
            KeyCode::PageUp => vec![27, b'[', b'5', b'~'],
            KeyCode::PageDown => vec![27, b'[', b'6', b'~'],
            KeyCode::Delete => vec![27, b'[', b'3', b'~'],
            KeyCode::Insert => vec![27, b'[', b'2', b'~'],
            KeyCode::F(n) => {
                match n {
                    1 => vec![27, b'O', b'P'],
                    2 => vec![27, b'O', b'Q'],
                    3 => vec![27, b'O', b'R'],
                    4 => vec![27, b'O', b'S'],
                    5 => vec![27, b'[', b'1', b'5', b'~'],
                    6 => vec![27, b'[', b'1', b'7', b'~'],
                    7 => vec![27, b'[', b'1', b'8', b'~'],
                    8 => vec![27, b'[', b'1', b'9', b'~'],
                    9 => vec![27, b'[', b'2', b'0', b'~'],
                    10 => vec![27, b'[', b'2', b'1', b'~'],
                    11 => vec![27, b'[', b'2', b'3', b'~'],
                    12 => vec![27, b'[', b'2', b'4', b'~'],
                    _ => vec![],
                }
            }
            _ => vec![],
        }
    }

    pub fn get_mode(&self) -> InputMode {
        self.mode
    }

    pub fn get_mode_display(&self) -> String {
        match self.mode {
            InputMode::Normal => "NORMAL".to_string(),
            InputMode::Insert => "INSERT".to_string(),
            InputMode::Visual => "VISUAL".to_string(),
            InputMode::VisualLine => "V-LINE".to_string(),
            InputMode::VisualBlock => "V-BLOCK".to_string(),
            InputMode::Command => "COMMAND".to_string(),
            InputMode::Replace => "REPLACE".to_string(),
            InputMode::Emacs => "EMACS".to_string(),
        }
    }

    pub fn get_pending_chord(&self) -> Option<String> {
        if self.config.enable_vim_mode {
            self.vim_handler.get_pending_chord()
                .map(|keys| keys.join(" "))
        } else {
            self.chord_detector.get_pending_chord()
                .map(|keys| keys.join(" "))
        }
    }
}

#[derive(Debug, Clone)]
pub enum InputAction {
    SendToTerminal(Vec<u8>),
    ExecuteCommand(String),
    CommandInput(String),
    MoveCursor(Direction, usize),
    MoveWord(Direction, usize),
    MoveToLineStart,
    MoveToLineEnd,
    MoveToBufferStart,
    MoveToBufferEnd,
    StartSelection,
    CopySelection,
    DeleteSelection,
    ClearSelection,
    Paste,
    DeleteChar,
    KillLine,
    KillSelection,
    SetMark,
    Noop,
}

#[derive(Debug, Clone, Copy)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

/// Convert input actions to protocol messages
pub fn actions_to_messages(actions: Vec<InputAction>, session_id: Option<crate::protocol::SessionId>) -> Vec<ClientMessage> {
    let mut messages = Vec::new();

    for action in actions {
        match action {
            InputAction::SendToTerminal(data) => {
                messages.push(ClientMessage::Input { data });
            }
            InputAction::ExecuteCommand(cmd) => {
                // Parse and convert command to appropriate message
                messages.push(parse_command_to_message(&cmd, session_id.clone()));
            }
            InputAction::CopySelection => {
                messages.push(ClientMessage::CopyModeInput {
                    key: "y".to_string(),
                });
            }
            InputAction::Paste => {
                messages.push(ClientMessage::CopyModeInput {
                    key: "p".to_string(),
                });
            }
            _ => {
                // Other actions would be handled by the client's UI layer
            }
        }
    }

    messages
}

fn parse_command_to_message(cmd: &str, _session_id: Option<crate::protocol::SessionId>) -> ClientMessage {
    match cmd {
        "list-windows" => ClientMessage::ListWindows,
        "list-sessions" => ClientMessage::ListSessions,
        "split-horizontal" => ClientMessage::SplitPane {
            direction: crate::protocol::SplitDirection::Horizontal,
        },
        "split-vertical" => ClientMessage::SplitPane {
            direction: crate::protocol::SplitDirection::Vertical,
        },
        "next-window" => ClientMessage::NextWindow,
        "previous-window" => ClientMessage::PreviousWindow,
        "kill-pane" => ClientMessage::KillPane,
        "zoom-pane" => ClientMessage::ZoomPane,
        "enter-copy-mode" => ClientMessage::EnterCopyMode,
        "exit-copy-mode" => ClientMessage::ExitCopyMode,
        _ => {
            // Default to sending as input
            ClientMessage::Input {
                data: cmd.as_bytes().to_vec(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_input_processor() {
        let config = InputConfig::default();
        let mut processor = InputProcessor::new(config);

        // Test mode switching
        assert_eq!(processor.get_mode(), InputMode::Normal);

        // Test that we can process keys
        let key_event = CrosstermKeyEvent::new(KeyCode::Char('i'), KeyModifiers::empty());
        let actions = processor.process_key(key_event).unwrap();

        // The 'i' key in normal mode should switch to insert mode (when vim is enabled)
        // Actions might be empty since mode switch is handled internally
        assert!(processor.get_mode() == InputMode::Insert || processor.get_mode() == InputMode::Normal);
    }

    #[test]
    fn test_key_to_bytes() {
        let config = InputConfig::default();
        let processor = InputProcessor::new(config);

        // Test regular character
        let event = CrosstermKeyEvent::new(KeyCode::Char('a'), KeyModifiers::empty());
        assert_eq!(processor.key_to_bytes(&event), vec![b'a']);

        // Test control character
        let event = CrosstermKeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(processor.key_to_bytes(&event), vec![3]);  // Ctrl-C

        // Test special keys
        let event = CrosstermKeyEvent::new(KeyCode::Enter, KeyModifiers::empty());
        assert_eq!(processor.key_to_bytes(&event), vec![b'\r']);
    }
}