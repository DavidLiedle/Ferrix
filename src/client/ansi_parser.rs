use crossterm::style::{Color, Attribute};
use crossterm::cursor::{MoveTo, Hide, Show};
use std::io::Write;
use crate::server::scrollback::CellScrollback;

/// Compact attribute bitflags to avoid Vec allocations
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AttributeFlags {
    flags: u8,
}

impl AttributeFlags {
    const BOLD: u8 = 0b0000_0001;
    const DIM: u8 = 0b0000_0010;
    const ITALIC: u8 = 0b0000_0100;
    const UNDERLINED: u8 = 0b0000_1000;
    const SLOW_BLINK: u8 = 0b0001_0000;
    const RAPID_BLINK: u8 = 0b0010_0000;
    const REVERSE: u8 = 0b0100_0000;
    const CROSSED_OUT: u8 = 0b1000_0000;

    pub fn new() -> Self {
        Self { flags: 0 }
    }

    pub fn set(&mut self, attr: Attribute) {
        match attr {
            Attribute::Bold => self.flags |= Self::BOLD,
            Attribute::Dim => self.flags |= Self::DIM,
            Attribute::Italic => self.flags |= Self::ITALIC,
            Attribute::Underlined => self.flags |= Self::UNDERLINED,
            Attribute::SlowBlink => self.flags |= Self::SLOW_BLINK,
            Attribute::RapidBlink => self.flags |= Self::RAPID_BLINK,
            Attribute::Reverse => self.flags |= Self::REVERSE,
            Attribute::CrossedOut => self.flags |= Self::CROSSED_OUT,
            _ => {} // Ignore other attributes for now
        }
    }

    pub fn clear(&mut self, attr: Attribute) {
        match attr {
            Attribute::Bold => self.flags &= !Self::BOLD,
            Attribute::Dim => self.flags &= !Self::DIM,
            Attribute::Italic => self.flags &= !Self::ITALIC,
            Attribute::Underlined => self.flags &= !Self::UNDERLINED,
            Attribute::SlowBlink => self.flags &= !Self::SLOW_BLINK,
            Attribute::RapidBlink => self.flags &= !Self::RAPID_BLINK,
            Attribute::Reverse => self.flags &= !Self::REVERSE,
            Attribute::CrossedOut => self.flags &= !Self::CROSSED_OUT,
            _ => {}
        }
    }

    pub fn to_attributes(&self) -> Vec<Attribute> {
        let mut attrs = Vec::with_capacity(8);
        if self.flags & Self::BOLD != 0 { attrs.push(Attribute::Bold); }
        if self.flags & Self::DIM != 0 { attrs.push(Attribute::Dim); }
        if self.flags & Self::ITALIC != 0 { attrs.push(Attribute::Italic); }
        if self.flags & Self::UNDERLINED != 0 { attrs.push(Attribute::Underlined); }
        if self.flags & Self::SLOW_BLINK != 0 { attrs.push(Attribute::SlowBlink); }
        if self.flags & Self::RAPID_BLINK != 0 { attrs.push(Attribute::RapidBlink); }
        if self.flags & Self::REVERSE != 0 { attrs.push(Attribute::Reverse); }
        if self.flags & Self::CROSSED_OUT != 0 { attrs.push(Attribute::CrossedOut); }
        attrs
    }
}

/// A single cell in the terminal screen buffer
/// Optimized to use bitflags instead of Vec<Attribute> to save memory
#[derive(Clone, Debug)]
pub struct Cell {
    pub ch: char,
    pub fg: Color,
    pub bg: Color,
    pub attributes: AttributeFlags,
}

impl Default for Cell {
    fn default() -> Self {
        Cell {
            ch: ' ',
            fg: Color::Reset,
            bg: Color::Reset,
            attributes: AttributeFlags::new(),
        }
    }
}

/// DEC Private Mode states
#[derive(Default)]
struct DecModes {
    cursor_visible: bool,           // ?25
    auto_wrap: bool,                // ?7
    origin_mode: bool,              // ?6
    reverse_video: bool,            // ?5
    cursor_keys_mode: bool,         // ?1
    column_132: bool,               // ?3
    smooth_scroll: bool,            // ?4
    alternate_screen_active: bool,  // ?1047, ?1049
    bracketed_paste: bool,          // ?2004
    mouse_tracking: MouseMode,      // ?1000, ?1002, ?1003, ?1006
}

#[derive(Default, Clone, Copy, PartialEq)]
enum MouseMode {
    #[default]
    Off,
    X10,        // ?1000
    Button,     // ?1002
    Any,        // ?1003
    Sgr,        // ?1006 (Select Graphic Rendition)
}

/// ANSI escape sequence parser for terminal emulation
pub struct AnsiParser {
    /// Current cursor position relative to pane
    cursor_x: u16,
    cursor_y: u16,
    /// Pane dimensions
    width: u16,
    height: u16,
    /// Current text attributes
    foreground: Color,
    background: Color,
    attributes: AttributeFlags,
    /// Buffer for incomplete escape sequences
    escape_buffer: Vec<u8>,
    /// Buffer for incomplete UTF-8 sequences
    utf8_buffer: Vec<u8>,
    /// Whether we're currently parsing an escape sequence
    in_escape: bool,
    /// Saved cursor position for save/restore operations
    saved_cursor: Option<(u16, u16)>,
    /// Screen buffer
    screen: Vec<Vec<Cell>>,
    /// Scrollback buffer
    scrollback: CellScrollback,

    // New fields for enhanced ANSI support
    /// Alternate screen buffer
    alternate_screen: Option<Vec<Vec<Cell>>>,
    /// Saved cursor for alternate screen
    alternate_cursor: Option<(u16, u16)>,
    /// DEC private modes
    modes: DecModes,
    /// Scrolling region (top, bottom) - 0-indexed
    scroll_region: Option<(u16, u16)>,
    /// Tab stops
    tab_stops: Vec<u16>,
    /// Saved attributes for cursor save/restore
    saved_attrs: Option<(Color, Color, AttributeFlags)>,
    /// Pending responses to send back to PTY (for device status reports, etc)
    pending_responses: Vec<Vec<u8>>,
}

impl AnsiParser {
    pub fn new(width: u16, height: u16) -> Self {
        Self::new_with_scrollback(width, height, 1000)
    }

    pub fn new_with_scrollback(width: u16, height: u16, scrollback_lines: usize) -> Self {
        let screen = vec![vec![Cell::default(); width as usize]; height as usize];

        // Initialize default tab stops every 8 columns
        let mut tab_stops = Vec::new();
        for i in (8..width).step_by(8) {
            tab_stops.push(i);
        }

        let modes = DecModes {
            cursor_visible: true,  // Cursor visible by default
            auto_wrap: true,       // Auto-wrap enabled by default
            ..Default::default()
        };

        Self {
            cursor_x: 0,
            cursor_y: 0,
            width,
            height,
            foreground: Color::Reset,
            background: Color::Reset,
            attributes: AttributeFlags::new(),
            escape_buffer: Vec::new(),
            utf8_buffer: Vec::new(),
            in_escape: false,
            saved_cursor: None,
            screen,
            scrollback: CellScrollback::new(scrollback_lines),

            // Initialize new fields
            alternate_screen: None,
            alternate_cursor: None,
            modes,
            scroll_region: None,
            tab_stops,
            saved_attrs: None,
            pending_responses: Vec::new(),
        }
    }

    pub fn resize(&mut self, width: u16, height: u16) {
        self.width = width;
        self.height = height;

        // Resize the screen buffer to match new dimensions
        self.screen.resize(height as usize, vec![Cell::default(); width as usize]);
        for row in &mut self.screen {
            row.resize(width as usize, Cell::default());
        }

        // Clamp cursor to new bounds
        self.cursor_x = self.cursor_x.min(width.saturating_sub(1));
        self.cursor_y = self.cursor_y.min(height.saturating_sub(1));
    }

    /// Reset terminal to initial state (RIS - ESC c)
    fn reset(&mut self) {
        self.cursor_x = 0;
        self.cursor_y = 0;
        self.foreground = Color::Reset;
        self.background = Color::Reset;
        self.attributes = AttributeFlags::new();
        self.saved_cursor = None;
        self.clear_screen();
        // Reset modes to defaults
        self.modes.cursor_visible = true;
        self.modes.auto_wrap = true;
    }

    /// Parse and render ANSI data to stdout
    pub fn parse_and_render(
        &mut self,
        data: &[u8],
        pane_x: u16,
        pane_y: u16,
        stdout: &mut impl Write,
    ) -> std::io::Result<()> {
        let mut i = 0;
        while i < data.len() {
            if self.in_escape {
                self.escape_buffer.push(data[i]);
                if self.is_complete_sequence() {
                    self.process_escape_sequence(pane_x, pane_y, stdout)?;
                    self.escape_buffer.clear();
                    self.in_escape = false;
                }
                i += 1;
            } else if data[i] == 0x1B {
                // ESC character - start of escape sequence
                self.in_escape = true;
                self.escape_buffer.clear();
                self.escape_buffer.push(0x1B);
                i += 1;
            } else {
                // Regular character or control character
                self.process_character(data[i], pane_x, pane_y, stdout)?;
                i += 1;
            }
        }
        Ok(())
    }

    fn is_complete_sequence(&self) -> bool {
        if self.escape_buffer.len() < 2 {
            return false;
        }

        // Check for CSI sequences (ESC [)
        if self.escape_buffer[1] == b'[' {
            // Safety: If CSI gets too long, force completion
            if self.escape_buffer.len() > 256 {
                return true;
            }
            // CSI sequences end with a letter
            if let Some(&last) = self.escape_buffer.last() {
                return last.is_ascii_uppercase() || last.is_ascii_lowercase();
            }
        }

        // Check for OSC sequences (ESC ])
        if self.escape_buffer[1] == b']' {
            // OSC sequences end with BEL (0x07) or ST (ESC \)
            let len = self.escape_buffer.len();

            // Safety: If OSC gets too long without terminator, force completion
            // This prevents infinite accumulation from malformed sequences
            if len > 1024 {
                return true;
            }

            if len >= 2 {
                let last = self.escape_buffer[len - 1];
                let second_last = if len >= 3 { self.escape_buffer[len - 2] } else { 0 };
                return last == 0x07 || (second_last == 0x1B && last == b'\\');
            }
            // OSC not complete yet, keep accumulating
            return false;
        }

        // Check for known two-character escape sequences
        if self.escape_buffer.len() == 2 {
            match self.escape_buffer[1] {
                b'7' | b'8' | b'M' | b'D' | b'E' | b'c' => return true,
                // CSI and OSC need more bytes, keep accumulating
                b'[' | b']' => return false,
                // Unknown sequence - consider complete to avoid hanging
                _ => return true,
            }
        }

        // For sequences longer than 2 bytes that aren't CSI/OSC, consider complete
        // This handles malformed sequences
        if self.escape_buffer.len() > 2 {
            // Already checked CSI and OSC above, so this must be something else
            return true;
        }

        false
    }

    fn process_escape_sequence(
        &mut self,
        pane_x: u16,
        pane_y: u16,
        stdout: &mut impl Write,
    ) -> std::io::Result<()> {
        if self.escape_buffer.len() < 2 {
            return Ok(());
        }

        match self.escape_buffer[1] {
            b'[' => self.process_csi_sequence(pane_x, pane_y, stdout)?,
            b']' => self.process_osc_sequence()?,
            b'7' => self.save_cursor(),
            b'8' => self.restore_cursor(),
            b'M' => self.reverse_index()?,
            b'D' => self.index()?,
            b'E' => self.next_line()?,
            _ => {} // Unknown sequence, ignore
        }
        Ok(())
    }

    fn process_csi_sequence(
        &mut self,
        pane_x: u16,
        pane_y: u16,
        stdout: &mut impl Write,
    ) -> std::io::Result<()> {
        let sequence = String::from_utf8_lossy(&self.escape_buffer[2..]);

        // Check for DEC private mode (starts with ?)
        let is_private = sequence.starts_with('?');
        let clean_seq = if is_private {
            &sequence[1..]
        } else {
            &sequence
        };

        let (params, command) = self.parse_csi_params(clean_seq);

        match command {
            'A' => self.cursor_up(params.first().copied().unwrap_or(1)),
            'B' => self.cursor_down(params.first().copied().unwrap_or(1)),
            'C' => self.cursor_forward(params.first().copied().unwrap_or(1)),
            'D' => self.cursor_backward(params.first().copied().unwrap_or(1)),
            'E' => self.cursor_next_line(params.first().copied().unwrap_or(1)),
            'F' => self.cursor_previous_line(params.first().copied().unwrap_or(1)),
            'G' => self.cursor_horizontal_absolute(params.first().copied().unwrap_or(1)),
            'H' | 'f' => {
                let row = params.first().copied().unwrap_or(1);
                let col = params.get(1).copied().unwrap_or(1);
                self.cursor_position(row, col);
            }
            'J' => self.erase_display(params.first().copied().unwrap_or(0), pane_x, pane_y, stdout)?,
            'K' => self.erase_line(params.first().copied().unwrap_or(0), pane_x, pane_y, stdout)?,
            'L' => self.insert_lines(params.first().copied().unwrap_or(1)),
            'M' => self.delete_lines(params.first().copied().unwrap_or(1)),
            '@' => self.insert_chars(params.first().copied().unwrap_or(1)),
            'P' => self.delete_chars(params.first().copied().unwrap_or(1)),
            'X' => self.erase_chars(params.first().copied().unwrap_or(1)),
            'S' => self.scroll_up_region(params.first().copied().unwrap_or(1)),
            'T' => self.scroll_down_region(params.first().copied().unwrap_or(1)),
            'r' => {
                let top = params.first().copied().unwrap_or(1);
                let bottom = params.get(1).copied().unwrap_or(self.height);
                self.set_scroll_region(top, bottom);
            }
            'm' => self.set_graphics_mode(&params, stdout)?,
            'n' => self.device_status_report(params.first().copied().unwrap_or(0), is_private)?,
            's' => self.save_cursor_with_attrs(),
            'u' => self.restore_cursor_with_attrs(),
            'l' => {
                if is_private {
                    self.reset_dec_mode(&params, stdout)?;
                } else {
                    self.reset_mode(&params)?;
                }
            }
            'h' => {
                if is_private {
                    self.set_dec_mode(&params, stdout)?;
                } else {
                    self.set_mode(&params)?;
                }
            }
            _ => {} // Unknown command
        }
        Ok(())
    }

    fn parse_csi_params(&self, sequence: &str) -> (Vec<u16>, char) {
        let mut params = Vec::new();
        let mut current_param = String::new();
        let mut command = ' ';

        for ch in sequence.chars() {
            if ch.is_ascii_digit() {
                current_param.push(ch);
            } else if ch == ';' {
                if !current_param.is_empty() {
                    params.push(current_param.parse().unwrap_or(0));
                    current_param.clear();
                }
            } else if ch.is_ascii_alphabetic() {
                if !current_param.is_empty() {
                    params.push(current_param.parse().unwrap_or(0));
                }
                command = ch;
                break;
            }
        }

        (params, command)
    }

    fn process_osc_sequence(&mut self) -> std::io::Result<()> {
        // OSC sequences are typically for setting window title, etc.
        // For now, we'll ignore them
        Ok(())
    }

    fn process_character(
        &mut self,
        ch: u8,
        _pane_x: u16,
        _pane_y: u16,
        _stdout: &mut impl Write,
    ) -> std::io::Result<()> {
        // Process character and store in cell buffer (not direct stdout)
        // The actual rendering happens via render() method
        match ch {
            0x07 => {} // BEL - ignore
            0x08 => self.cursor_backward(1), // BS
            0x09 => self.tab()?, // HT
            0x0A => self.line_feed()?, // LF
            0x0D => self.carriage_return(), // CR
            0x0E => {} // SO - ignore
            0x0F => {} // SI - ignore
            _ if ch >= 0x20 => {
                // Store printable character in cell buffer
                if self.cursor_y < self.height && self.cursor_x < self.width {
                    let cell = &mut self.screen[self.cursor_y as usize][self.cursor_x as usize];
                    cell.ch = ch as char;
                    cell.fg = self.foreground;
                    cell.bg = self.background;
                    cell.attributes = self.attributes;

                    // Advance cursor with auto-wrap
                    self.cursor_x += 1;
                    if self.cursor_x >= self.width {
                        if self.modes.auto_wrap {
                            self.cursor_x = 0;
                            self.cursor_y += 1;
                            if self.cursor_y >= self.height {
                                self.scroll_up();
                                self.cursor_y = self.height - 1;
                            }
                        } else {
                            self.cursor_x = self.width - 1;
                        }
                    }
                }
            }
            _ => {} // Other control characters - ignore
        }
        Ok(())
    }

    // Cursor movement functions
    fn cursor_up(&mut self, n: u16) {
        self.cursor_y = self.cursor_y.saturating_sub(n);
    }

    fn cursor_down(&mut self, n: u16) {
        self.cursor_y = (self.cursor_y + n).min(self.height - 1);
    }

    fn cursor_forward(&mut self, n: u16) {
        self.cursor_x = (self.cursor_x + n).min(self.width - 1);
    }

    fn cursor_backward(&mut self, n: u16) {
        self.cursor_x = self.cursor_x.saturating_sub(n);
    }

    fn cursor_next_line(&mut self, n: u16) {
        self.cursor_y = (self.cursor_y + n).min(self.height - 1);
        self.cursor_x = 0;
    }

    fn cursor_previous_line(&mut self, n: u16) {
        self.cursor_y = self.cursor_y.saturating_sub(n);
        self.cursor_x = 0;
    }

    fn cursor_horizontal_absolute(&mut self, n: u16) {
        self.cursor_x = (n.saturating_sub(1)).min(self.width - 1);
    }

    fn cursor_position(&mut self, row: u16, col: u16) {
        self.cursor_y = (row.saturating_sub(1)).min(self.height - 1);
        self.cursor_x = (col.saturating_sub(1)).min(self.width - 1);
    }


    fn carriage_return(&mut self) {
        self.cursor_x = 0;
    }

    fn line_feed(&mut self) -> std::io::Result<()> {
        self.cursor_y += 1;
        if self.cursor_y >= self.height {
            self.scroll_up();
            self.cursor_y = self.height - 1;
        }
        Ok(())
    }

    fn tab(&mut self) -> std::io::Result<()> {
        // Move to next tab stop using custom tab stops if available
        let next_tab = self.tab_stops.iter()
            .find(|&&stop| stop > self.cursor_x)
            .copied()
            .unwrap_or_else(|| {
                // Fallback to default 8-column tab stops
                ((self.cursor_x / 8) + 1) * 8
            });
        self.cursor_x = next_tab.min(self.width - 1);
        Ok(())
    }

    fn index(&mut self) -> std::io::Result<()> {
        self.cursor_y += 1;
        if self.cursor_y >= self.height {
            self.scroll_up();
            self.cursor_y = self.height - 1;
        }
        Ok(())
    }

    fn reverse_index(&mut self) -> std::io::Result<()> {
        // RI (Reverse Index): Move cursor up, scroll down if at top of scroll region
        let (top, _bottom) = self.get_scroll_region();

        if self.cursor_y as usize == top {
            // At top of scroll region - scroll down (insert line at top)
            self.scroll_down_region(1);
        } else {
            // Not at top - just move cursor up
            self.cursor_y = self.cursor_y.saturating_sub(1);
        }
        Ok(())
    }

    fn next_line(&mut self) -> std::io::Result<()> {
        self.cursor_x = 0;
        self.cursor_y = (self.cursor_y + 1).min(self.height - 1);
        Ok(())
    }

    fn erase_display(&mut self, mode: u16, pane_x: u16, pane_y: u16, stdout: &mut impl Write) -> std::io::Result<()> {
        match mode {
            0 => {
                // Clear from cursor to end of screen
                for y in self.cursor_y..self.height {
                    for x in 0..self.width {
                        if y == self.cursor_y && x < self.cursor_x {
                            continue;
                        }
                        crossterm::execute!(stdout, MoveTo(pane_x + x, pane_y + y))?;
                        stdout.write_all(b" ")?;
                    }
                }
            }
            1 => {
                // Clear from beginning to cursor
                for y in 0..=self.cursor_y {
                    for x in 0..self.width {
                        if y == self.cursor_y && x > self.cursor_x {
                            continue;
                        }
                        crossterm::execute!(stdout, MoveTo(pane_x + x, pane_y + y))?;
                        stdout.write_all(b" ")?;
                    }
                }
            }
            2 | 3 => {
                // Clear entire screen
                for y in 0..self.height {
                    for x in 0..self.width {
                        crossterm::execute!(stdout, MoveTo(pane_x + x, pane_y + y))?;
                        stdout.write_all(b" ")?;
                    }
                }
                if mode == 2 {
                    self.cursor_x = 0;
                    self.cursor_y = 0;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn erase_line(&mut self, mode: u16, pane_x: u16, pane_y: u16, stdout: &mut impl Write) -> std::io::Result<()> {
        let y = pane_y + self.cursor_y;

        match mode {
            0 => {
                // Clear from cursor to end of line
                for x in self.cursor_x..self.width {
                    crossterm::execute!(stdout, MoveTo(pane_x + x, y))?;
                    stdout.write_all(b" ")?;
                }
            }
            1 => {
                // Clear from beginning to cursor
                for x in 0..=self.cursor_x {
                    crossterm::execute!(stdout, MoveTo(pane_x + x, y))?;
                    stdout.write_all(b" ")?;
                }
            }
            2 => {
                // Clear entire line
                for x in 0..self.width {
                    crossterm::execute!(stdout, MoveTo(pane_x + x, y))?;
                    stdout.write_all(b" ")?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn set_graphics_mode(&mut self, params: &[u16], _stdout: &mut impl Write) -> std::io::Result<()> {
        if params.is_empty() {
            // Reset all attributes
            self.foreground = Color::Reset;
            self.background = Color::Reset;
            self.attributes = AttributeFlags::new();
            return Ok(());
        }

        let mut i = 0;
        while i < params.len() {
            let param = params[i];
            match param {
                0 => {
                    // Reset all
                    self.foreground = Color::Reset;
                    self.background = Color::Reset;
                    self.attributes = AttributeFlags::new();
                }
                1 => self.attributes.set(Attribute::Bold),
                2 => self.attributes.set(Attribute::Dim),
                3 => self.attributes.set(Attribute::Italic),
                4 => self.attributes.set(Attribute::Underlined),
                5 => self.attributes.set(Attribute::SlowBlink),
                6 => self.attributes.set(Attribute::RapidBlink),
                7 => self.attributes.set(Attribute::Reverse),
                8 => {}, // Hidden not supported in bitflags yet
                9 => self.attributes.set(Attribute::CrossedOut),

                // Reset specific attributes
                21 | 22 => {
                    self.attributes.clear(Attribute::Bold);
                    self.attributes.clear(Attribute::Dim);
                }
                23 => self.attributes.clear(Attribute::Italic),
                24 => self.attributes.clear(Attribute::Underlined),
                25 => {
                    self.attributes.clear(Attribute::SlowBlink);
                    self.attributes.clear(Attribute::RapidBlink);
                }
                27 => self.attributes.clear(Attribute::Reverse),
                28 => {}, // Hidden not supported
                29 => self.attributes.clear(Attribute::CrossedOut),

                // Foreground colors
                30 => self.foreground = Color::Black,
                31 => self.foreground = Color::DarkRed,
                32 => self.foreground = Color::DarkGreen,
                33 => self.foreground = Color::DarkYellow,
                34 => self.foreground = Color::DarkBlue,
                35 => self.foreground = Color::DarkMagenta,
                36 => self.foreground = Color::DarkCyan,
                37 => self.foreground = Color::Grey,

                // Extended foreground colors (38;5;n for 256-color, 38;2;r;g;b for RGB)
                38 => {
                    if i + 1 < params.len() {
                        match params[i + 1] {
                            5 => {
                                // 256-color mode
                                if i + 2 < params.len() {
                                    self.foreground = Color::AnsiValue(params[i + 2] as u8);
                                    i += 2;
                                }
                            }
                            2 => {
                                // RGB color mode
                                if i + 4 < params.len() {
                                    self.foreground = Color::Rgb {
                                        r: params[i + 2] as u8,
                                        g: params[i + 3] as u8,
                                        b: params[i + 4] as u8,
                                    };
                                    i += 4;
                                }
                            }
                            _ => {}
                        }
                    }
                }
                39 => self.foreground = Color::Reset,

                // Background colors
                40 => self.background = Color::Black,
                41 => self.background = Color::DarkRed,
                42 => self.background = Color::DarkGreen,
                43 => self.background = Color::DarkYellow,
                44 => self.background = Color::DarkBlue,
                45 => self.background = Color::DarkMagenta,
                46 => self.background = Color::DarkCyan,
                47 => self.background = Color::Grey,

                // Extended background colors (48;5;n for 256-color, 48;2;r;g;b for RGB)
                48 => {
                    if i + 1 < params.len() {
                        match params[i + 1] {
                            5 => {
                                // 256-color mode
                                if i + 2 < params.len() {
                                    self.background = Color::AnsiValue(params[i + 2] as u8);
                                    i += 2;
                                }
                            }
                            2 => {
                                // RGB color mode
                                if i + 4 < params.len() {
                                    self.background = Color::Rgb {
                                        r: params[i + 2] as u8,
                                        g: params[i + 3] as u8,
                                        b: params[i + 4] as u8,
                                    };
                                    i += 4;
                                }
                            }
                            _ => {}
                        }
                    }
                }
                49 => self.background = Color::Reset,

                // Bright foreground colors
                90 => self.foreground = Color::DarkGrey,
                91 => self.foreground = Color::Red,
                92 => self.foreground = Color::Green,
                93 => self.foreground = Color::Yellow,
                94 => self.foreground = Color::Blue,
                95 => self.foreground = Color::Magenta,
                96 => self.foreground = Color::Cyan,
                97 => self.foreground = Color::White,

                // Bright background colors
                100 => self.background = Color::DarkGrey,
                101 => self.background = Color::Red,
                102 => self.background = Color::Green,
                103 => self.background = Color::Yellow,
                104 => self.background = Color::Blue,
                105 => self.background = Color::Magenta,
                106 => self.background = Color::Cyan,
                107 => self.background = Color::White,

                _ => {} // Unknown or unsupported
            }
            i += 1;
        }
        Ok(())
    }

    fn set_mode(&mut self, _params: &[u16]) -> std::io::Result<()> {
        // Handle mode setting (e.g., cursor visibility, alternate screen)
        // For now, we'll ignore these
        Ok(())
    }

    fn reset_mode(&mut self, _params: &[u16]) -> std::io::Result<()> {
        // Handle mode resetting
        // For now, we'll ignore these
        Ok(())
    }

    fn handle_character(&mut self, ch: u8) {
        match ch {
            // Backspace
            0x08 => {
                if self.cursor_x > 0 {
                    self.cursor_x -= 1;
                }
            }
            // Tab
            0x09 => {
                // Move to next tab stop (every 8 columns)
                let next_tab = ((self.cursor_x / 8) + 1) * 8;
                self.cursor_x = next_tab.min(self.width - 1);
            }
            // Line feed / newline
            0x0A => {
                self.cursor_y += 1;
                if self.cursor_y >= self.height {
                    // Scroll up
                    self.scroll_up();
                    self.cursor_y = self.height - 1;
                }
            }
            // Carriage return
            0x0D => {
                self.cursor_x = 0;
            }
            // Printable character or UTF-8 continuation
            0x20..=0x7E => {
                // ASCII printable character - directly use it
                if self.cursor_y < self.height && self.cursor_x < self.width {
                    let cell = &mut self.screen[self.cursor_y as usize][self.cursor_x as usize];
                    cell.ch = ch as char;
                    cell.fg = self.foreground;
                    cell.bg = self.background;
                    cell.attributes = self.attributes;

                    self.cursor_x += 1;
                    if self.cursor_x >= self.width {
                        if self.modes.auto_wrap {
                            self.cursor_x = 0;
                            self.cursor_y += 1;
                            if self.cursor_y >= self.height {
                                self.scroll_up();
                                self.cursor_y = self.height - 1;
                            }
                        } else {
                            self.cursor_x = self.width - 1;
                        }
                    }
                }
            }
            0x80..=0xFF => {
                // UTF-8 multi-byte character
                self.utf8_buffer.push(ch);

                // Try to decode the UTF-8 sequence
                if let Ok(s) = std::str::from_utf8(&self.utf8_buffer) {
                    // Successfully decoded - get the character
                    if let Some(decoded_char) = s.chars().next() {
                        // Place the decoded character
                        if self.cursor_y < self.height && self.cursor_x < self.width {
                            let cell = &mut self.screen[self.cursor_y as usize][self.cursor_x as usize];
                            cell.ch = decoded_char;
                            cell.fg = self.foreground;
                            cell.bg = self.background;
                            cell.attributes = self.attributes;

                            self.cursor_x += 1;
                            if self.cursor_x >= self.width {
                                if self.modes.auto_wrap {
                                    self.cursor_x = 0;
                                    self.cursor_y += 1;
                                    if self.cursor_y >= self.height {
                                        self.scroll_up();
                                        self.cursor_y = self.height - 1;
                                    }
                                } else {
                                    self.cursor_x = self.width - 1;
                                }
                            }
                        }
                        // Clear the buffer after successful decode
                        self.utf8_buffer.clear();
                    }
                } else {
                    // Not yet complete or invalid - keep accumulating
                    // Safety: clear buffer if it gets too long (invalid UTF-8)
                    if self.utf8_buffer.len() > 4 {
                        self.utf8_buffer.clear();
                    }
                }
            }
            _ => {} // Ignore other control characters
        }
    }

    fn handle_escape_sequence(&mut self) {
        if self.escape_buffer.len() < 2 {
            return;
        }

        match self.escape_buffer[1] {
            b'[' => self.handle_csi_sequence(),
            b']' => self.handle_osc_sequence(),
            b'7' => self.save_cursor(),
            b'8' => self.restore_cursor(),
            b'M' => {
                // RI - Reverse Index (move cursor up, scroll if at top)
                if self.cursor_y > 0 {
                    self.cursor_y -= 1;
                } else {
                    // At top of screen - scroll down (insert line at top)
                    self.scroll_down_at_top();
                }
            }
            b'D' => {
                // IND - Index (move cursor down, scroll if at bottom)
                self.cursor_y += 1;
                if self.cursor_y >= self.height {
                    self.scroll_up();
                    self.cursor_y = self.height - 1;
                }
            }
            b'E' => {
                // NEL - Next Line (CR + LF)
                self.cursor_x = 0;
                self.cursor_y += 1;
                if self.cursor_y >= self.height {
                    self.scroll_up();
                    self.cursor_y = self.height - 1;
                }
            }
            b'c' => {
                // RIS - Reset to Initial State
                self.reset();
            }
            _ => {
                // Unknown escape sequence - ignore
            }
        }
    }

    fn handle_csi_sequence(&mut self) {
        // CSI sequences are in the format: ESC [ params command
        if self.escape_buffer.len() < 3 {
            return;
        }

        // Parse parameters
        let params_end = self.escape_buffer.len() - 1;
        let params_str = String::from_utf8_lossy(&self.escape_buffer[2..params_end]);
        let params: Vec<u16> = params_str
            .split(';')
            .filter_map(|s| s.parse().ok())
            .collect();

        let command = self.escape_buffer[params_end];

        match command {
            // Cursor movement
            b'A' => {
                // Move cursor up
                let n = params.first().copied().unwrap_or(1);
                self.cursor_y = self.cursor_y.saturating_sub(n);
            }
            b'B' => {
                // Move cursor down
                let n = params.first().copied().unwrap_or(1);
                self.cursor_y = (self.cursor_y + n).min(self.height - 1);
            }
            b'C' => {
                // Move cursor right
                let n = params.first().copied().unwrap_or(1);
                self.cursor_x = (self.cursor_x + n).min(self.width - 1);
            }
            b'D' => {
                // Move cursor left
                let n = params.first().copied().unwrap_or(1);
                self.cursor_x = self.cursor_x.saturating_sub(n);
            }
            b'E' => {
                // CNL - Cursor Next Line
                let n = params.first().copied().unwrap_or(1);
                self.cursor_y = (self.cursor_y + n).min(self.height - 1);
                self.cursor_x = 0;
            }
            b'F' => {
                // CPL - Cursor Previous Line
                let n = params.first().copied().unwrap_or(1);
                self.cursor_y = self.cursor_y.saturating_sub(n);
                self.cursor_x = 0;
            }
            b'G' => {
                // CHA - Cursor Horizontal Absolute
                let n = params.first().copied().unwrap_or(1).saturating_sub(1);
                self.cursor_x = n.min(self.width - 1);
            }
            b'H' | b'f' => {
                // Move cursor to position
                let row = params.first().copied().unwrap_or(1).saturating_sub(1);
                let col = params.get(1).copied().unwrap_or(1).saturating_sub(1);
                self.cursor_y = row.min(self.height - 1);
                self.cursor_x = col.min(self.width - 1);
            }
            b'J' => {
                // Clear screen
                let mode = params.first().copied().unwrap_or(0);
                match mode {
                    0 => self.clear_from_cursor_to_end(),
                    1 => self.clear_from_start_to_cursor(),
                    2 => self.clear_screen(),
                    _ => {}
                }
            }
            b'K' => {
                // Clear line
                let mode = params.first().copied().unwrap_or(0);
                match mode {
                    0 => self.clear_line_from_cursor(),
                    1 => self.clear_line_to_cursor(),
                    2 => self.clear_line(),
                    _ => {}
                }
            }
            b'm' => {
                // SGR - Select Graphic Rendition
                self.handle_sgr_simple(&params);
            }
            b's' => self.save_cursor(),
            b'u' => self.restore_cursor(),
            b'n' => {
                // DSR - Device Status Report
                let mode = params.first().copied().unwrap_or(0);
                match mode {
                    5 => {
                        // Device status - respond with "OK"
                        self.pending_responses.push(b"\x1b[0n".to_vec());
                    }
                    6 => {
                        // Cursor position report
                        let row = self.cursor_y + 1;  // 1-indexed
                        let col = self.cursor_x + 1;  // 1-indexed
                        let response = format!("\x1b[{};{}R", row, col);
                        self.pending_responses.push(response.into_bytes());
                    }
                    _ => {}
                }
            }
            _ => {} // Ignore unsupported sequences
        }
    }

    fn handle_osc_sequence(&mut self) {
        // OSC sequences are for operating system commands (window title, etc.)
        // We silently consume these sequences
        // Future: Could implement window title setting, etc.
    }

    fn handle_sgr_simple(&mut self, params: &[u16]) {
        if params.is_empty() {
            // Reset all attributes
            self.foreground = Color::Reset;
            self.background = Color::Reset;
            self.attributes = AttributeFlags::new();
            return;
        }

        for &param in params {
            match param {
                0 => {
                    // Reset all
                    self.foreground = Color::Reset;
                    self.background = Color::Reset;
                    self.attributes = AttributeFlags::new();
                }
                1 => self.attributes.set(crossterm::style::Attribute::Bold),
                2 => self.attributes.set(crossterm::style::Attribute::Dim),
                3 => self.attributes.set(crossterm::style::Attribute::Italic),
                4 => self.attributes.set(crossterm::style::Attribute::Underlined),
                5 => self.attributes.set(crossterm::style::Attribute::SlowBlink),
                7 => self.attributes.set(crossterm::style::Attribute::Reverse),
                8 => {}, // Hidden not supported
                9 => self.attributes.set(crossterm::style::Attribute::CrossedOut),

                // Foreground colors
                30 => self.foreground = Color::Black,
                31 => self.foreground = Color::DarkRed,
                32 => self.foreground = Color::DarkGreen,
                33 => self.foreground = Color::DarkYellow,
                34 => self.foreground = Color::DarkBlue,
                35 => self.foreground = Color::DarkMagenta,
                36 => self.foreground = Color::DarkCyan,
                37 => self.foreground = Color::Grey,
                39 => self.foreground = Color::Reset,

                // Background colors
                40 => self.background = Color::Black,
                41 => self.background = Color::DarkRed,
                42 => self.background = Color::DarkGreen,
                43 => self.background = Color::DarkYellow,
                44 => self.background = Color::DarkBlue,
                45 => self.background = Color::DarkMagenta,
                46 => self.background = Color::DarkCyan,
                47 => self.background = Color::Grey,
                49 => self.background = Color::Reset,

                // Bright foreground colors
                90 => self.foreground = Color::DarkGrey,
                91 => self.foreground = Color::Red,
                92 => self.foreground = Color::Green,
                93 => self.foreground = Color::Yellow,
                94 => self.foreground = Color::Blue,
                95 => self.foreground = Color::Magenta,
                96 => self.foreground = Color::Cyan,
                97 => self.foreground = Color::White,

                // Bright background colors
                100 => self.background = Color::DarkGrey,
                101 => self.background = Color::Red,
                102 => self.background = Color::Green,
                103 => self.background = Color::Yellow,
                104 => self.background = Color::Blue,
                105 => self.background = Color::Magenta,
                106 => self.background = Color::Cyan,
                107 => self.background = Color::White,

                _ => {} // Ignore unsupported SGR codes
            }
        }
    }

    fn save_cursor(&mut self) {
        self.saved_cursor = Some((self.cursor_x, self.cursor_y));
    }

    fn restore_cursor(&mut self) {
        if let Some((x, y)) = self.saved_cursor {
            self.cursor_x = x;
            self.cursor_y = y;
        }
    }

    fn save_cursor_with_attrs(&mut self) {
        self.saved_cursor = Some((self.cursor_x, self.cursor_y));
        self.saved_attrs = Some((self.foreground, self.background, self.attributes));
    }

    fn restore_cursor_with_attrs(&mut self) {
        if let Some((x, y)) = self.saved_cursor {
            self.cursor_x = x;
            self.cursor_y = y;
        }
        if let Some((fg, bg, attrs)) = &self.saved_attrs {
            self.foreground = *fg;
            self.background = *bg;
            self.attributes = *attrs;
        }
    }

    /// Handle DEC private mode set (DECSET)
    fn set_dec_mode(&mut self, params: &[u16], stdout: &mut impl Write) -> std::io::Result<()> {
        for &param in params {
            match param {
                1 => self.modes.cursor_keys_mode = true,
                3 => self.modes.column_132 = true,
                4 => self.modes.smooth_scroll = true,
                5 => self.modes.reverse_video = true,
                6 => self.modes.origin_mode = true,
                7 => self.modes.auto_wrap = true,
                25 => {
                    self.modes.cursor_visible = true;
                    crossterm::execute!(stdout, Show)?;
                }
                1000 => self.modes.mouse_tracking = MouseMode::X10,
                1002 => self.modes.mouse_tracking = MouseMode::Button,
                1003 => self.modes.mouse_tracking = MouseMode::Any,
                1006 => self.modes.mouse_tracking = MouseMode::Sgr,
                1047 => self.use_alternate_screen(false)?,
                1048 => self.save_cursor_with_attrs(),
                1049 => {
                    self.save_cursor_with_attrs();
                    self.use_alternate_screen(true)?;
                }
                2004 => self.modes.bracketed_paste = true,
                _ => {} // Ignore unknown modes
            }
        }
        Ok(())
    }

    /// Handle DEC private mode reset (DECRST)
    fn reset_dec_mode(&mut self, params: &[u16], stdout: &mut impl Write) -> std::io::Result<()> {
        for &param in params {
            match param {
                1 => self.modes.cursor_keys_mode = false,
                3 => self.modes.column_132 = false,
                4 => self.modes.smooth_scroll = false,
                5 => self.modes.reverse_video = false,
                6 => self.modes.origin_mode = false,
                7 => self.modes.auto_wrap = false,
                25 => {
                    self.modes.cursor_visible = false;
                    crossterm::execute!(stdout, Hide)?;
                }
                1000 | 1002 | 1003 | 1006 => self.modes.mouse_tracking = MouseMode::Off,
                1047 => self.use_normal_screen()?,
                1048 => self.restore_cursor_with_attrs(),
                1049 => {
                    self.use_normal_screen()?;
                    self.restore_cursor_with_attrs();
                }
                2004 => self.modes.bracketed_paste = false,
                _ => {} // Ignore unknown modes
            }
        }
        Ok(())
    }

    /// Switch to alternate screen buffer
    fn use_alternate_screen(&mut self, clear: bool) -> std::io::Result<()> {
        if !self.modes.alternate_screen_active {
            // Save current screen and cursor
            self.alternate_screen = Some(self.screen.clone());
            self.alternate_cursor = Some((self.cursor_x, self.cursor_y));

            // Create new screen buffer
            self.screen = vec![vec![Cell::default(); self.width as usize]; self.height as usize];
            if clear {
                self.cursor_x = 0;
                self.cursor_y = 0;
            }
            self.modes.alternate_screen_active = true;
        }
        Ok(())
    }

    /// Switch back to normal screen buffer
    fn use_normal_screen(&mut self) -> std::io::Result<()> {
        if self.modes.alternate_screen_active {
            // Restore saved screen and cursor
            if let Some(saved_screen) = self.alternate_screen.take() {
                self.screen = saved_screen;
            }
            if let Some((x, y)) = self.alternate_cursor.take() {
                self.cursor_x = x;
                self.cursor_y = y;
            }
            self.modes.alternate_screen_active = false;
        }
        Ok(())
    }

    /// Insert blank lines at cursor position
    fn insert_lines(&mut self, count: u16) {
        let count = count as usize;
        let cursor_row = self.cursor_y as usize;

        // Determine scroll region
        let (top, bottom) = self.get_scroll_region();

        if cursor_row >= top && cursor_row <= bottom {
            // Shift lines down within scroll region
            for _ in 0..count {
                if bottom < self.screen.len() {
                    self.screen.remove(bottom);
                }
                self.screen.insert(cursor_row, vec![Cell::default(); self.width as usize]);
            }
        }
    }

    /// Delete lines at cursor position
    fn delete_lines(&mut self, count: u16) {
        let count = count as usize;
        let cursor_row = self.cursor_y as usize;

        // Determine scroll region
        let (top, bottom) = self.get_scroll_region();

        if cursor_row >= top && cursor_row <= bottom {
            // Remove lines and add blank lines at bottom
            for _ in 0..count {
                if cursor_row < self.screen.len() {
                    self.screen.remove(cursor_row);
                    self.screen.insert(bottom, vec![Cell::default(); self.width as usize]);
                }
            }
        }
    }

    /// Insert blank characters at cursor position
    fn insert_chars(&mut self, count: u16) {
        let cursor_row = self.cursor_y as usize;
        let cursor_col = self.cursor_x as usize;
        let count = count as usize;

        if cursor_row < self.screen.len() {
            let row = &mut self.screen[cursor_row];
            for _ in 0..count {
                if cursor_col < row.len() {
                    row.insert(cursor_col, Cell::default());
                    if row.len() > self.width as usize {
                        row.pop();
                    }
                }
            }
        }
    }

    /// Delete characters at cursor position
    fn delete_chars(&mut self, count: u16) {
        let cursor_row = self.cursor_y as usize;
        let cursor_col = self.cursor_x as usize;
        let count = count as usize;

        if cursor_row < self.screen.len() {
            let row = &mut self.screen[cursor_row];
            for _ in 0..count {
                if cursor_col < row.len() {
                    row.remove(cursor_col);
                }
            }
            // Fill with blanks at the end
            while row.len() < self.width as usize {
                row.push(Cell::default());
            }
        }
    }

    /// Erase characters from cursor position (replace with spaces)
    fn erase_chars(&mut self, count: u16) {
        let cursor_row = self.cursor_y as usize;
        let cursor_col = self.cursor_x as usize;
        let count = count as usize;

        if cursor_row < self.screen.len() {
            let row = &mut self.screen[cursor_row];
            for i in 0..count {
                let col = cursor_col + i;
                if col < row.len() {
                    row[col] = Cell::default();
                }
            }
        }
    }

    /// Set scrolling region
    fn set_scroll_region(&mut self, top: u16, bottom: u16) {
        // Convert from 1-indexed to 0-indexed
        let top = (top.saturating_sub(1)).min(self.height - 1);
        let bottom = (bottom.saturating_sub(1)).min(self.height - 1);

        if top < bottom {
            self.scroll_region = Some((top, bottom));

            // If origin mode is set, move cursor to home position within region
            if self.modes.origin_mode {
                self.cursor_x = 0;
                self.cursor_y = top;
            }
        }
    }

    /// Get current scroll region (0-indexed)
    fn get_scroll_region(&self) -> (usize, usize) {
        if let Some((top, bottom)) = self.scroll_region {
            (top as usize, bottom as usize)
        } else {
            (0, (self.height - 1) as usize)
        }
    }

    /// Scroll up within the scroll region
    fn scroll_up_region(&mut self, count: u16) {
        let (top, bottom) = self.get_scroll_region();
        let count = count as usize;

        for _ in 0..count {
            if top <= bottom && bottom < self.screen.len() {
                // Save line to scrollback if it's the top line
                if top == 0 && !self.modes.alternate_screen_active {
                    self.scrollback.push(self.screen[top].clone());
                }

                // Remove line from top of region
                self.screen.remove(top);
                // Add blank line at bottom of region
                self.screen.insert(bottom, vec![Cell::default(); self.width as usize]);
            }
        }
    }

    /// Scroll down within the scroll region
    fn scroll_down_region(&mut self, count: u16) {
        let (top, bottom) = self.get_scroll_region();
        let count = count as usize;

        for _ in 0..count {
            if top <= bottom && bottom < self.screen.len() {
                // Remove line from bottom of region
                self.screen.remove(bottom);
                // Add blank line at top of region
                self.screen.insert(top, vec![Cell::default(); self.width as usize]);
            }
        }
    }

    /// Device Status Report - respond with cursor position or status
    fn device_status_report(&mut self, mode: u16, _is_private: bool) -> std::io::Result<()> {
        match mode {
            5 => {
                // Device status report - respond with "OK" status
                // Response: CSI 0 n (terminal is OK/ready)
                self.pending_responses.push(b"\x1b[0n".to_vec());
            }
            6 => {
                // Cursor position report
                // Response: CSI row ; col R
                let row = self.cursor_y + 1;  // Convert to 1-indexed
                let col = self.cursor_x + 1;
                let response = format!("\x1b[{};{}R", row, col);
                self.pending_responses.push(response.into_bytes());
            }
            _ => {}
        }
        Ok(())
    }

    fn scroll_up(&mut self) {
        // Save the top line to scrollback using the optimized buffer
        if !self.screen.is_empty() {
            self.scrollback.push(self.screen[0].clone());
        }

        // Shift all lines up
        for y in 0..self.height as usize - 1 {
            self.screen[y] = self.screen[y + 1].clone();
        }

        // Clear the bottom line
        let last_row = self.height as usize - 1;
        self.screen[last_row] = vec![Cell::default(); self.width as usize];
    }

    fn scroll_down_at_top(&mut self) {
        // Scroll down: shift all lines down by one
        // Bottom line is lost, top line becomes blank
        if self.height > 0 {
            // Remove bottom line
            if self.screen.len() == self.height as usize {
                self.screen.pop();
            }
            // Insert blank line at top
            self.screen.insert(0, vec![Cell::default(); self.width as usize]);
        }
    }

    fn clear_screen(&mut self) {
        for row in &mut self.screen {
            for cell in row {
                *cell = Cell::default();
            }
        }
    }

    fn clear_from_cursor_to_end(&mut self) {
        // Clear from cursor to end of line
        if (self.cursor_y as usize) < self.screen.len() {
            for x in self.cursor_x as usize..self.width as usize {
                self.screen[self.cursor_y as usize][x] = Cell::default();
            }
        }

        // Clear all lines below
        for y in (self.cursor_y + 1) as usize..self.height as usize {
            for x in 0..self.width as usize {
                self.screen[y][x] = Cell::default();
            }
        }
    }

    fn clear_from_start_to_cursor(&mut self) {
        // Clear all lines above
        for y in 0..self.cursor_y as usize {
            for x in 0..self.width as usize {
                self.screen[y][x] = Cell::default();
            }
        }

        // Clear from start of line to cursor
        if (self.cursor_y as usize) < self.screen.len() {
            for x in 0..=self.cursor_x as usize {
                self.screen[self.cursor_y as usize][x] = Cell::default();
            }
        }
    }

    fn clear_line(&mut self) {
        if (self.cursor_y as usize) < self.screen.len() {
            for x in 0..self.width as usize {
                self.screen[self.cursor_y as usize][x] = Cell::default();
            }
        }
    }

    fn clear_line_from_cursor(&mut self) {
        if (self.cursor_y as usize) < self.screen.len() {
            for x in self.cursor_x as usize..self.width as usize {
                self.screen[self.cursor_y as usize][x] = Cell::default();
            }
        }
    }

    fn clear_line_to_cursor(&mut self) {
        if (self.cursor_y as usize) < self.screen.len() {
            for x in 0..=self.cursor_x as usize {
                self.screen[self.cursor_y as usize][x] = Cell::default();
            }
        }
    }

    pub fn get_cursor_position(&self) -> (u16, u16) {
        (self.cursor_x, self.cursor_y)
    }

    /// Process raw terminal data through the ANSI parser
    pub fn process(&mut self, data: &[u8]) {
        // Process character by character to build the screen buffer
        let mut i = 0;
        while i < data.len() {
            if self.in_escape {
                self.escape_buffer.push(data[i]);
                if self.is_complete_sequence() {
                    self.handle_escape_sequence();
                    self.escape_buffer.clear();
                    self.in_escape = false;
                }
            } else if data[i] == 0x1B {
                // ESC character - start of escape sequence
                self.in_escape = true;
                self.escape_buffer.clear();
                self.escape_buffer.push(0x1B);
            } else {
                // Regular character
                self.handle_character(data[i]);
            }
            i += 1;
        }
    }

    /// Render the current screen buffer
    pub fn render(&self) -> &Vec<Vec<Cell>> {
        &self.screen
    }

    /// Take any pending responses that need to be sent back to the PTY
    pub fn take_pending_responses(&mut self) -> Vec<Vec<u8>> {
        std::mem::take(&mut self.pending_responses)
    }
}