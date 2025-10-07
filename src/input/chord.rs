use std::time::{Duration, Instant};
use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use crossterm::event::{KeyCode, KeyModifiers};

/// Key chord detection and processing
#[derive(Debug, Clone)]
pub struct ChordDetector {
    /// Current chord being built
    current_chord: Vec<KeyEvent>,

    /// Timestamp of the first key in current chord
    chord_start: Option<Instant>,

    /// Timeout for chord completion
    timeout: Duration,

    /// Registered chord sequences
    chord_map: HashMap<Vec<KeyEvent>, ChordAction>,

    /// Whether chord mode is active
    active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct KeyEvent {
    pub code: String,
    pub modifiers: String,
}

impl KeyEvent {
    pub fn from_crossterm(code: KeyCode, modifiers: KeyModifiers) -> Self {
        Self {
            code: format!("{:?}", code),
            modifiers: format!("{:?}", modifiers),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChordAction {
    Command(String),
    EnterMode(InputMode),
    ExecuteSequence(Vec<String>),
    Custom(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum InputMode {
    Normal,
    Insert,
    Command,
    Visual,
    VisualLine,
    VisualBlock,
    Replace,
    Emacs,
}

impl ChordDetector {
    pub fn new(timeout_ms: u64) -> Self {
        let mut detector = Self {
            current_chord: Vec::new(),
            chord_start: None,
            timeout: Duration::from_millis(timeout_ms),
            chord_map: HashMap::new(),
            active: false,
        };

        // Initialize with default chord bindings
        detector.init_default_chords();
        detector
    }

    fn init_default_chords(&mut self) {
        // Vim-style leader key sequences
        self.register_chord(
            vec!["Space", "w"],
            ChordAction::Command("list-windows".to_string())
        );

        self.register_chord(
            vec!["Space", "s"],
            ChordAction::Command("list-sessions".to_string())
        );

        self.register_chord(
            vec!["Space", "h"],
            ChordAction::Command("split-horizontal".to_string())
        );

        self.register_chord(
            vec!["Space", "v"],
            ChordAction::Command("split-vertical".to_string())
        );

        // Vim mode transitions
        self.register_chord(
            vec!["Escape"],
            ChordAction::EnterMode(InputMode::Normal)
        );

        self.register_chord(
            vec!["i"],
            ChordAction::EnterMode(InputMode::Insert)
        );

        self.register_chord(
            vec!["v"],
            ChordAction::EnterMode(InputMode::Visual)
        );

        self.register_chord(
            vec!["Shift+V"],
            ChordAction::EnterMode(InputMode::VisualLine)
        );

        self.register_chord(
            vec!["Ctrl+v"],
            ChordAction::EnterMode(InputMode::VisualBlock)
        );

        // Emacs-style chords
        self.register_chord(
            vec!["Ctrl+x", "Ctrl+c"],
            ChordAction::Command("quit".to_string())
        );

        self.register_chord(
            vec!["Ctrl+x", "Ctrl+s"],
            ChordAction::Command("save-session".to_string())
        );

        self.register_chord(
            vec!["Ctrl+x", "2"],
            ChordAction::Command("split-horizontal".to_string())
        );

        self.register_chord(
            vec!["Ctrl+x", "3"],
            ChordAction::Command("split-vertical".to_string())
        );
    }

    pub fn register_chord(&mut self, keys: Vec<&str>, action: ChordAction) {
        let chord: Vec<KeyEvent> = keys.iter().map(|k| {
            let parts: Vec<&str> = k.split('+').collect();
            let (modifiers, code) = if parts.len() > 1 {
                (parts[0..parts.len()-1].join("+"), parts.last().unwrap().to_string())
            } else {
                (String::new(), k.to_string())
            };

            KeyEvent {
                code,
                modifiers,
            }
        }).collect();

        self.chord_map.insert(chord, action);
    }

    pub fn process_key(&mut self, key: KeyEvent) -> Option<ChordAction> {
        if !self.active {
            return None;
        }

        // Check timeout
        if let Some(start) = self.chord_start {
            if start.elapsed() > self.timeout {
                self.reset();
            }
        }

        // Start chord if needed
        if self.current_chord.is_empty() {
            self.chord_start = Some(Instant::now());
        }

        // Add key to current chord
        self.current_chord.push(key);

        // Check for exact match
        if let Some(action) = self.chord_map.get(&self.current_chord) {
            let action = action.clone();
            self.reset();
            return Some(action);
        }

        // Check if this could be a prefix of any chord
        let is_prefix = self.chord_map.keys().any(|chord| {
            chord.len() > self.current_chord.len() &&
            chord[..self.current_chord.len()] == self.current_chord[..]
        });

        if !is_prefix {
            // No match possible, reset
            self.reset();
        }

        None
    }

    pub fn reset(&mut self) {
        self.current_chord.clear();
        self.chord_start = None;
    }

    pub fn activate(&mut self) {
        self.active = true;
    }

    pub fn deactivate(&mut self) {
        self.active = false;
        self.reset();
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn get_pending_chord(&self) -> Option<Vec<String>> {
        if self.current_chord.is_empty() {
            None
        } else {
            Some(self.current_chord.iter().map(|k| {
                if k.modifiers.is_empty() {
                    k.code.clone()
                } else {
                    format!("{}+{}", k.modifiers, k.code)
                }
            }).collect())
        }
    }
}

/// Vim-style modal input handler
#[derive(Debug)]
pub struct VimModeHandler {
    mode: InputMode,
    chord_detector: ChordDetector,
    repeat_count: Option<usize>,
    last_command: Option<String>,
    registers: HashMap<char, String>,
    current_register: Option<char>,
}

impl Default for VimModeHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl VimModeHandler {
    pub fn new() -> Self {
        Self {
            mode: InputMode::Normal,
            chord_detector: ChordDetector::new(500),
            repeat_count: None,
            last_command: None,
            registers: HashMap::new(),
            current_register: None,
        }
    }

    pub fn get_pending_chord(&self) -> Option<Vec<String>> {
        self.chord_detector.get_pending_chord()
    }

    pub fn get_mode(&self) -> InputMode {
        self.mode
    }

    /// Store text in a named register (e.g., "a, "b, etc.)
    pub fn set_register(&mut self, register: char, content: String) {
        self.registers.insert(register, content);
    }

    /// Get text from a named register
    pub fn get_register(&self, register: char) -> Option<&String> {
        self.registers.get(&register)
    }

    /// Set which register to use for next yank/delete/paste
    pub fn select_register(&mut self, register: char) {
        self.current_register = Some(register);
    }

    /// Get the currently selected register, or default to unnamed register
    pub fn current_register(&self) -> Option<char> {
        self.current_register
    }

    /// Store the last command for repeat with '.'
    pub fn set_last_command(&mut self, command: String) {
        self.last_command = Some(command);
    }

    /// Get the last command for repeating
    pub fn last_command(&self) -> Option<&String> {
        self.last_command.as_ref()
    }

    pub fn set_mode(&mut self, mode: InputMode) {
        self.mode = mode;

        // Activate chord detection in normal mode
        if mode == InputMode::Normal {
            self.chord_detector.activate();
        } else {
            self.chord_detector.deactivate();
        }
    }

    pub fn process_key(&mut self, key: KeyEvent) -> Option<VimCommand> {
        match self.mode {
            InputMode::Normal => self.process_normal_mode(key),
            InputMode::Insert => self.process_insert_mode(key),
            InputMode::Visual | InputMode::VisualLine | InputMode::VisualBlock => {
                self.process_visual_mode(key)
            }
            InputMode::Command => self.process_command_mode(key),
            InputMode::Replace => self.process_replace_mode(key),
            _ => None,
        }
    }

    fn process_normal_mode(&mut self, key: KeyEvent) -> Option<VimCommand> {
        // Check for chord action first
        if let Some(action) = self.chord_detector.process_key(key.clone()) {
            return match action {
                ChordAction::EnterMode(mode) => {
                    self.set_mode(mode);
                    None
                }
                ChordAction::Command(cmd) => Some(VimCommand::Execute(cmd)),
                _ => None,
            };
        }

        // Handle digits for repeat count
        if let Ok(digit) = key.code.parse::<usize>() {
            if digit > 0 || self.repeat_count.is_some() {
                let count = self.repeat_count.unwrap_or(0) * 10 + digit;
                self.repeat_count = Some(count);
                return None;
            }
        }

        // Process vim commands
        let count = self.repeat_count.take().unwrap_or(1);

        match key.code.as_str() {
            // Movement
            "h" => Some(VimCommand::Move(VimMotion::Left(count))),
            "j" => Some(VimCommand::Move(VimMotion::Down(count))),
            "k" => Some(VimCommand::Move(VimMotion::Up(count))),
            "l" => Some(VimCommand::Move(VimMotion::Right(count))),
            "w" => Some(VimCommand::Move(VimMotion::WordForward(count))),
            "b" => Some(VimCommand::Move(VimMotion::WordBackward(count))),
            "e" => Some(VimCommand::Move(VimMotion::WordEnd(count))),
            "0" => Some(VimCommand::Move(VimMotion::LineStart)),
            "$" => Some(VimCommand::Move(VimMotion::LineEnd)),
            "g" => Some(VimCommand::Move(VimMotion::FileStart)),
            "G" => Some(VimCommand::Move(VimMotion::FileEnd)),

            // Editing
            "x" => Some(VimCommand::Delete(VimMotion::Right(count))),
            "d" => Some(VimCommand::DeleteMotion),
            "y" => Some(VimCommand::YankMotion),
            "p" => Some(VimCommand::Paste(count)),
            "P" => Some(VimCommand::PasteBefore(count)),
            "u" => Some(VimCommand::Undo(count)),
            "r" => Some(VimCommand::Redo(count)),
            "." => Some(VimCommand::RepeatLast(count)),

            // Mode changes
            "i" => {
                self.set_mode(InputMode::Insert);
                None
            }
            "a" => {
                self.set_mode(InputMode::Insert);
                Some(VimCommand::Move(VimMotion::Right(1)))
            }
            "o" => {
                self.set_mode(InputMode::Insert);
                Some(VimCommand::NewLineBelow)
            }
            "O" => {
                self.set_mode(InputMode::Insert);
                Some(VimCommand::NewLineAbove)
            }
            ":" => {
                self.set_mode(InputMode::Command);
                None
            }

            _ => None,
        }
    }

    fn process_insert_mode(&mut self, key: KeyEvent) -> Option<VimCommand> {
        if key.code == "Escape" {
            self.set_mode(InputMode::Normal);
            None
        } else {
            Some(VimCommand::InsertChar(key.code))
        }
    }

    fn process_visual_mode(&mut self, key: KeyEvent) -> Option<VimCommand> {
        match key.code.as_str() {
            "Escape" => {
                self.set_mode(InputMode::Normal);
                Some(VimCommand::ClearSelection)
            }
            "y" => {
                self.set_mode(InputMode::Normal);
                Some(VimCommand::YankSelection)
            }
            "d" | "x" => {
                self.set_mode(InputMode::Normal);
                Some(VimCommand::DeleteSelection)
            }
            // Movement extends selection
            "h" => Some(VimCommand::ExtendSelection(VimMotion::Left(1))),
            "j" => Some(VimCommand::ExtendSelection(VimMotion::Down(1))),
            "k" => Some(VimCommand::ExtendSelection(VimMotion::Up(1))),
            "l" => Some(VimCommand::ExtendSelection(VimMotion::Right(1))),
            _ => None,
        }
    }

    fn process_command_mode(&mut self, _key: KeyEvent) -> Option<VimCommand> {
        // Command mode would be handled by a separate command line interface
        None
    }

    fn process_replace_mode(&mut self, key: KeyEvent) -> Option<VimCommand> {
        if key.code == "Escape" {
            self.set_mode(InputMode::Normal);
            None
        } else {
            Some(VimCommand::ReplaceChar(key.code))
        }
    }
}

#[derive(Debug, Clone)]
pub enum VimCommand {
    Move(VimMotion),
    Delete(VimMotion),
    DeleteMotion,
    YankMotion,
    Paste(usize),
    PasteBefore(usize),
    Undo(usize),
    Redo(usize),
    RepeatLast(usize),
    InsertChar(String),
    ReplaceChar(String),
    NewLineBelow,
    NewLineAbove,
    Execute(String),
    ExtendSelection(VimMotion),
    YankSelection,
    DeleteSelection,
    ClearSelection,
}

#[derive(Debug, Clone)]
pub enum VimMotion {
    Left(usize),
    Right(usize),
    Up(usize),
    Down(usize),
    WordForward(usize),
    WordBackward(usize),
    WordEnd(usize),
    LineStart,
    LineEnd,
    FileStart,
    FileEnd,
}

/// Emacs-style input handler
#[derive(Debug)]
pub struct EmacsHandler {
    chord_detector: ChordDetector,
    mark_set: bool,
    kill_ring: Vec<String>,
    kill_ring_index: usize,
}

impl Default for EmacsHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl EmacsHandler {
    pub fn new() -> Self {
        let mut handler = Self {
            chord_detector: ChordDetector::new(500),
            mark_set: false,
            kill_ring: Vec::new(),
            kill_ring_index: 0,
        };

        handler.init_emacs_bindings();
        handler.chord_detector.activate();
        handler
    }

    fn init_emacs_bindings(&mut self) {
        // Movement
        self.chord_detector.register_chord(
            vec!["Ctrl+f"],
            ChordAction::Command("forward-char".to_string())
        );
        self.chord_detector.register_chord(
            vec!["Ctrl+b"],
            ChordAction::Command("backward-char".to_string())
        );
        self.chord_detector.register_chord(
            vec!["Ctrl+n"],
            ChordAction::Command("next-line".to_string())
        );
        self.chord_detector.register_chord(
            vec!["Ctrl+p"],
            ChordAction::Command("previous-line".to_string())
        );
        self.chord_detector.register_chord(
            vec!["Alt+f"],
            ChordAction::Command("forward-word".to_string())
        );
        self.chord_detector.register_chord(
            vec!["Alt+b"],
            ChordAction::Command("backward-word".to_string())
        );

        // Editing
        self.chord_detector.register_chord(
            vec!["Ctrl+d"],
            ChordAction::Command("delete-char".to_string())
        );
        self.chord_detector.register_chord(
            vec!["Ctrl+k"],
            ChordAction::Command("kill-line".to_string())
        );
        self.chord_detector.register_chord(
            vec!["Ctrl+w"],
            ChordAction::Command("kill-region".to_string())
        );
        self.chord_detector.register_chord(
            vec!["Ctrl+y"],
            ChordAction::Command("yank".to_string())
        );
        self.chord_detector.register_chord(
            vec!["Alt+y"],
            ChordAction::Command("yank-pop".to_string())
        );

        // Selection
        self.chord_detector.register_chord(
            vec!["Ctrl+Space"],
            ChordAction::Command("set-mark".to_string())
        );
        self.chord_detector.register_chord(
            vec!["Ctrl+g"],
            ChordAction::Command("keyboard-quit".to_string())
        );
    }

    pub fn process_key(&mut self, key: KeyEvent) -> Option<EmacsCommand> {
        if let Some(action) = self.chord_detector.process_key(key) {
            match action {
                ChordAction::Command(cmd) => self.process_emacs_command(&cmd),
                _ => None,
            }
        } else {
            None
        }
    }

    fn process_emacs_command(&mut self, command: &str) -> Option<EmacsCommand> {
        match command {
            "forward-char" => Some(EmacsCommand::Move(EmacsMotion::ForwardChar)),
            "backward-char" => Some(EmacsCommand::Move(EmacsMotion::BackwardChar)),
            "next-line" => Some(EmacsCommand::Move(EmacsMotion::NextLine)),
            "previous-line" => Some(EmacsCommand::Move(EmacsMotion::PreviousLine)),
            "forward-word" => Some(EmacsCommand::Move(EmacsMotion::ForwardWord)),
            "backward-word" => Some(EmacsCommand::Move(EmacsMotion::BackwardWord)),
            "beginning-of-line" => Some(EmacsCommand::Move(EmacsMotion::BeginningOfLine)),
            "end-of-line" => Some(EmacsCommand::Move(EmacsMotion::EndOfLine)),

            "delete-char" => Some(EmacsCommand::DeleteChar),
            "kill-line" => Some(EmacsCommand::KillLine),
            "kill-region" => Some(EmacsCommand::KillRegion),
            "yank" => Some(EmacsCommand::Yank),
            "yank-pop" => Some(EmacsCommand::YankPop),

            "set-mark" => {
                self.mark_set = !self.mark_set;
                Some(EmacsCommand::SetMark)
            }
            "keyboard-quit" => {
                self.mark_set = false;
                Some(EmacsCommand::Quit)
            }

            _ => None,
        }
    }

    pub fn add_to_kill_ring(&mut self, text: String) {
        self.kill_ring.push(text);
        if self.kill_ring.len() > 30 {
            self.kill_ring.remove(0);
        }
        self.kill_ring_index = self.kill_ring.len();
    }

    pub fn get_from_kill_ring(&mut self) -> Option<String> {
        if self.kill_ring.is_empty() {
            None
        } else {
            self.kill_ring_index = self.kill_ring.len() - 1;
            self.kill_ring.get(self.kill_ring_index).cloned()
        }
    }

    pub fn rotate_kill_ring(&mut self) -> Option<String> {
        if self.kill_ring.is_empty() {
            return None;
        }

        if self.kill_ring_index > 0 {
            self.kill_ring_index -= 1;
        } else {
            self.kill_ring_index = self.kill_ring.len() - 1;
        }

        self.kill_ring.get(self.kill_ring_index).cloned()
    }
}

#[derive(Debug, Clone)]
pub enum EmacsCommand {
    Move(EmacsMotion),
    DeleteChar,
    KillLine,
    KillRegion,
    Yank,
    YankPop,
    SetMark,
    Quit,
}

#[derive(Debug, Clone)]
pub enum EmacsMotion {
    ForwardChar,
    BackwardChar,
    NextLine,
    PreviousLine,
    ForwardWord,
    BackwardWord,
    BeginningOfLine,
    EndOfLine,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chord_detection() {
        let mut detector = ChordDetector::new(500);
        detector.activate();

        detector.register_chord(
            vec!["Ctrl+x", "Ctrl+c"],
            ChordAction::Command("quit".to_string())
        );

        let key1 = KeyEvent {
            code: "x".to_string(),
            modifiers: "Ctrl".to_string(),
        };

        let key2 = KeyEvent {
            code: "c".to_string(),
            modifiers: "Ctrl".to_string(),
        };

        // First key shouldn't trigger action
        assert!(detector.process_key(key1).is_none());

        // Second key should complete the chord
        if let Some(ChordAction::Command(cmd)) = detector.process_key(key2) {
            assert_eq!(cmd, "quit");
        } else {
            panic!("Expected quit command");
        }
    }

    #[test]
    fn test_vim_mode_handler() {
        let mut handler = VimModeHandler::new();

        // Start in normal mode
        assert_eq!(handler.get_mode(), InputMode::Normal);

        // Switch to insert mode
        let key_i = KeyEvent {
            code: "i".to_string(),
            modifiers: String::new(),
        };
        handler.process_key(key_i);
        assert_eq!(handler.get_mode(), InputMode::Insert);

        // Escape back to normal mode
        let key_esc = KeyEvent {
            code: "Escape".to_string(),
            modifiers: String::new(),
        };
        handler.process_key(key_esc);
        assert_eq!(handler.get_mode(), InputMode::Normal);
    }

    #[test]
    fn test_emacs_handler() {
        let mut handler = EmacsHandler::new();

        let key = KeyEvent {
            code: "f".to_string(),
            modifiers: "Ctrl".to_string(),
        };

        if let Some(EmacsCommand::Move(EmacsMotion::ForwardChar)) = handler.process_key(key) {
            // Success
        } else {
            panic!("Expected forward-char command");
        }
    }
}