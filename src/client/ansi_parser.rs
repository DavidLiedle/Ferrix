use crossterm::style::{Color, Attribute, SetForegroundColor, SetBackgroundColor, SetAttribute, ResetColor};
use crossterm::cursor::{MoveTo, MoveUp, MoveDown, MoveLeft, MoveRight};
use crossterm::terminal::{Clear, ClearType};
use std::io::Write;
use crate::server::scrollback::CellScrollback;

/// A single cell in the terminal screen buffer
#[derive(Clone, Debug)]
pub struct Cell {
    pub ch: char,
    pub fg: Color,
    pub bg: Color,
    pub attributes: Vec<Attribute>,
}

impl Default for Cell {
    fn default() -> Self {
        Cell {
            ch: ' ',
            fg: Color::Reset,
            bg: Color::Reset,
            attributes: Vec::new(),
        }
    }
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
    attributes: Vec<Attribute>,
    /// Buffer for incomplete escape sequences
    escape_buffer: Vec<u8>,
    /// Whether we're currently parsing an escape sequence
    in_escape: bool,
    /// Saved cursor position for save/restore operations
    saved_cursor: Option<(u16, u16)>,
    /// Screen buffer
    screen: Vec<Vec<Cell>>,
    /// Scrollback buffer
    scrollback: CellScrollback,
}

impl AnsiParser {
    pub fn new(width: u16, height: u16) -> Self {
        Self::new_with_scrollback(width, height, 1000)
    }

    pub fn new_with_scrollback(width: u16, height: u16, scrollback_lines: usize) -> Self {
        let screen = vec![vec![Cell::default(); width as usize]; height as usize];
        Self {
            cursor_x: 0,
            cursor_y: 0,
            width,
            height,
            foreground: Color::Reset,
            background: Color::Reset,
            attributes: Vec::new(),
            escape_buffer: Vec::new(),
            in_escape: false,
            saved_cursor: None,
            screen,
            scrollback: CellScrollback::new(scrollback_lines),
        }
    }

    pub fn resize(&mut self, width: u16, height: u16) {
        self.width = width;
        self.height = height;
        // Clamp cursor to new bounds
        self.cursor_x = self.cursor_x.min(width.saturating_sub(1));
        self.cursor_y = self.cursor_y.min(height.saturating_sub(1));
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
            // CSI sequences end with a letter
            if let Some(&last) = self.escape_buffer.last() {
                return (b'A'..=b'Z').contains(&last) || (b'a'..=b'z').contains(&last);
            }
        }

        // Check for OSC sequences (ESC ])
        if self.escape_buffer[1] == b']' {
            // OSC sequences end with BEL or ST
            let len = self.escape_buffer.len();
            if len >= 2 {
                let last = self.escape_buffer[len - 1];
                let second_last = if len >= 3 { self.escape_buffer[len - 2] } else { 0 };
                return last == 0x07 || (second_last == 0x1B && last == b'\\');
            }
        }

        // Other simple two-character sequences
        self.escape_buffer.len() >= 2
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
        let (params, command) = self.parse_csi_params(&sequence);

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
            'm' => self.set_graphics_mode(&params, stdout)?,
            's' => self.save_cursor(),
            'u' => self.restore_cursor(),
            'l' => self.reset_mode(&params)?,
            'h' => self.set_mode(&params)?,
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
        pane_x: u16,
        pane_y: u16,
        stdout: &mut impl Write,
    ) -> std::io::Result<()> {
        match ch {
            0x07 => {} // BEL - ignore
            0x08 => self.cursor_backward(1), // BS
            0x09 => self.tab()?, // HT
            0x0A => self.line_feed()?, // LF
            0x0D => self.carriage_return(), // CR
            0x0E => {} // SO - ignore
            0x0F => {} // SI - ignore
            _ if ch >= 0x20 => {
                // Printable character
                let abs_x = pane_x + self.cursor_x;
                let abs_y = pane_y + self.cursor_y;

                crossterm::execute!(stdout, MoveTo(abs_x, abs_y))?;

                // Apply current colors and attributes
                for attr in &self.attributes {
                    crossterm::execute!(stdout, SetAttribute(*attr))?;
                }
                if !matches!(self.foreground, Color::Reset) {
                    crossterm::execute!(stdout, SetForegroundColor(self.foreground))?;
                }
                if !matches!(self.background, Color::Reset) {
                    crossterm::execute!(stdout, SetBackgroundColor(self.background))?;
                }

                stdout.write_all(&[ch])?;

                // Reset colors
                crossterm::execute!(stdout, ResetColor)?;

                // Advance cursor
                self.cursor_x += 1;
                if self.cursor_x >= self.width {
                    self.cursor_x = 0;
                    self.cursor_y = (self.cursor_y + 1).min(self.height - 1);
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
        self.cursor_y = (self.cursor_y + 1).min(self.height - 1);
        Ok(())
    }

    fn tab(&mut self) -> std::io::Result<()> {
        // Move to next tab stop (every 8 columns)
        let next_tab = ((self.cursor_x / 8) + 1) * 8;
        self.cursor_x = next_tab.min(self.width - 1);
        Ok(())
    }

    fn index(&mut self) -> std::io::Result<()> {
        self.cursor_y = (self.cursor_y + 1).min(self.height - 1);
        Ok(())
    }

    fn reverse_index(&mut self) -> std::io::Result<()> {
        self.cursor_y = self.cursor_y.saturating_sub(1);
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
            self.attributes.clear();
            return Ok(());
        }

        for &param in params {
            match param {
                0 => {
                    // Reset all
                    self.foreground = Color::Reset;
                    self.background = Color::Reset;
                    self.attributes.clear();
                }
                1 => self.attributes.push(Attribute::Bold),
                2 => self.attributes.push(Attribute::Dim),
                3 => self.attributes.push(Attribute::Italic),
                4 => self.attributes.push(Attribute::Underlined),
                5 => self.attributes.push(Attribute::SlowBlink),
                6 => self.attributes.push(Attribute::RapidBlink),
                7 => self.attributes.push(Attribute::Reverse),
                8 => self.attributes.push(Attribute::Hidden),
                9 => self.attributes.push(Attribute::CrossedOut),

                // Foreground colors
                30 => self.foreground = Color::Black,
                31 => self.foreground = Color::Red,
                32 => self.foreground = Color::Green,
                33 => self.foreground = Color::Yellow,
                34 => self.foreground = Color::Blue,
                35 => self.foreground = Color::Magenta,
                36 => self.foreground = Color::Cyan,
                37 => self.foreground = Color::White,
                39 => self.foreground = Color::Reset,

                // Background colors
                40 => self.background = Color::Black,
                41 => self.background = Color::Red,
                42 => self.background = Color::Green,
                43 => self.background = Color::Yellow,
                44 => self.background = Color::Blue,
                45 => self.background = Color::Magenta,
                46 => self.background = Color::Cyan,
                47 => self.background = Color::White,
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
            // Printable character
            0x20..=0x7E | 0x80..=0xFF => {
                // Place character at current position
                if self.cursor_y < self.height && self.cursor_x < self.width {
                    let cell = &mut self.screen[self.cursor_y as usize][self.cursor_x as usize];
                    cell.ch = ch as char;
                    cell.fg = self.foreground;
                    cell.bg = self.background;
                    cell.attributes = self.attributes.clone();

                    // Advance cursor
                    self.cursor_x += 1;
                    if self.cursor_x >= self.width {
                        self.cursor_x = 0;
                        self.cursor_y += 1;
                        if self.cursor_y >= self.height {
                            self.scroll_up();
                            self.cursor_y = self.height - 1;
                        }
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
            _ => {}
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
            _ => {} // Ignore unsupported sequences
        }
    }

    fn handle_osc_sequence(&mut self) {
        // OSC sequences are for operating system commands
        // We'll ignore most of these for now
    }

    fn handle_sgr_simple(&mut self, params: &[u16]) {
        if params.is_empty() {
            // Reset all attributes
            self.foreground = Color::Reset;
            self.background = Color::Reset;
            self.attributes.clear();
            return;
        }

        for &param in params {
            match param {
                0 => {
                    // Reset all
                    self.foreground = Color::Reset;
                    self.background = Color::Reset;
                    self.attributes.clear();
                }
                1 => self.attributes.push(crossterm::style::Attribute::Bold),
                2 => self.attributes.push(crossterm::style::Attribute::Dim),
                3 => self.attributes.push(crossterm::style::Attribute::Italic),
                4 => self.attributes.push(crossterm::style::Attribute::Underlined),
                5 => self.attributes.push(crossterm::style::Attribute::SlowBlink),
                7 => self.attributes.push(crossterm::style::Attribute::Reverse),
                8 => self.attributes.push(crossterm::style::Attribute::Hidden),
                9 => self.attributes.push(crossterm::style::Attribute::CrossedOut),

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
}