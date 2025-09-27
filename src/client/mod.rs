pub mod connection;
pub mod renderer;

use std::path::PathBuf;
use tokio::net::UnixStream;
use tokio_util::codec::Framed;
use futures::{StreamExt, SinkExt};
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
    execute,
    cursor,
};
use std::io::stdout;
use tracing::{debug, error, info};

use crate::error::{FerrixError, Result};
use crate::protocol::{ClientMessage, ServerMessage, SessionId, codec::FerrixClientCodec, LayoutInfo, PaneInfo, PaneId};
use std::collections::HashMap;

pub struct Client {
    socket_path: PathBuf,
    attached_session: Option<SessionId>,
    framed: Option<Framed<UnixStream, FerrixClientCodec>>,
    current_layout: Option<LayoutInfo>,
    terminal_size: (u16, u16), // (cols, rows)
    pane_buffers: HashMap<PaneId, Vec<u8>>, // Buffer terminal output per pane
}

impl Client {
    pub fn new(socket_path: PathBuf) -> Self {
        Self {
            socket_path,
            attached_session: None,
            framed: None,
            current_layout: None,
            terminal_size: (80, 24),
            pane_buffers: HashMap::new(),
        }
    }

    pub async fn connect(&mut self) -> Result<()> {
        let stream = UnixStream::connect(&self.socket_path)
            .await
            .map_err(|e| FerrixError::Ipc(format!("Failed to connect to server: {}", e)))?;

        self.framed = Some(Framed::new(stream, FerrixClientCodec::new()));
        info!("Connected to server at {:?}", self.socket_path);
        Ok(())
    }

    pub async fn create_session(&mut self, name: Option<String>) -> Result<SessionId> {
        if let Some(framed) = &mut self.framed {
            framed.send(ClientMessage::CreateSession { name }).await?;

            if let Some(Ok(ServerMessage::SessionCreated { session_id, name })) = framed.next().await {
                info!("Created session: {} ({})", name, session_id.0);
                return Ok(session_id);
            }
        }
        Err(FerrixError::Protocol("Failed to create session".to_string()))
    }

    pub async fn attach_session(&mut self, session_id: SessionId) -> Result<()> {
        if let Some(framed) = &mut self.framed {
            framed.send(ClientMessage::AttachSession { session_id: session_id.clone() }).await?;

            // Wait for session attached confirmation
            if let Some(response) = framed.next().await {
                match response? {
                    ServerMessage::SessionAttached { .. } => {
                        self.attached_session = Some(session_id.clone());
                        info!("Attached to session: {}", session_id.0);
                    }
                    ServerMessage::Error { message } => {
                        return Err(FerrixError::Other(message));
                    }
                    _ => {
                        return Err(FerrixError::Other("Unexpected server response during attach".to_string()));
                    }
                }
            } else {
                return Err(FerrixError::Other("No response from server during attach".to_string()));
            }

            // Enter main session loop
            self.session_loop().await
        } else {
            Err(FerrixError::NotConnected)
        }
    }

    pub async fn detach_session(&mut self) -> Result<()> {
        if let Some(framed) = &mut self.framed {
            framed.send(ClientMessage::DetachSession).await?;

            if let Some(Ok(ServerMessage::SessionDetached)) = framed.next().await {
                self.attached_session = None;
                info!("Detached from session");
                return Ok(());
            }
        }
        Err(FerrixError::Protocol("Failed to detach from session".to_string()))
    }

    pub async fn list_sessions(&mut self) -> Result<Vec<crate::protocol::SessionInfo>> {
        if let Some(framed) = &mut self.framed {
            framed.send(ClientMessage::ListSessions).await?;

            if let Some(Ok(ServerMessage::SessionList { sessions })) = framed.next().await {
                return Ok(sessions);
            }
        }
        Err(FerrixError::Protocol("Failed to list sessions".to_string()))
    }

    pub async fn kill_session(&mut self, session_id: SessionId) -> Result<()> {
        if let Some(framed) = &mut self.framed {
            framed.send(ClientMessage::KillSession { session_id: session_id.clone() }).await?;

            if let Some(Ok(ServerMessage::SessionKilled { .. })) = framed.next().await {
                info!("Killed session {}", session_id.0);
                return Ok(());
            }
        }
        Err(FerrixError::Protocol("Failed to kill session".to_string()))
    }

    async fn run_attached(&mut self) -> Result<()> {
        terminal::enable_raw_mode()?;
        execute!(stdout(), EnterAlternateScreen, cursor::Hide)?;

        let (term_width, term_height) = terminal::size()?;
        self.terminal_size = (term_width, term_height);
        if let Some(framed) = &mut self.framed {
            framed.send(ClientMessage::Resize { cols: term_width, rows: term_height }).await?;
        }

        let result = self.handle_attached_session().await;

        terminal::disable_raw_mode()?;
        execute!(stdout(), LeaveAlternateScreen, cursor::Show)?;

        result
    }

    async fn handle_attached_session(&mut self) -> Result<()> {
        let mut event_reader = event::EventStream::new();

        loop {
            tokio::select! {
                Some(event_result) = event_reader.next() => {
                    match event_result {
                        Ok(Event::Key(key_event)) => {
                            if self.handle_key_event(key_event).await? {
                                break;
                            }
                        }
                        Ok(Event::Resize(cols, rows)) => {
                            self.terminal_size = (cols, rows);
                            if let Some(framed) = &mut self.framed {
                                framed.send(ClientMessage::Resize { cols, rows }).await?;
                            }
                            // Re-render with new size
                            self.render_layout().await?;
                        }
                        _ => {}
                    }
                }

                Some(message_result) = async {
                    if let Some(framed) = &mut self.framed {
                        framed.next().await
                    } else {
                        None
                    }
                } => {
                    match message_result {
                        Ok(ServerMessage::Output { data }) => {
                            self.handle_output(data).await?;
                        }
                        Ok(ServerMessage::PaneOutput { pane_id, data }) => {
                            self.handle_pane_output(pane_id, data).await?;
                        }
                        Ok(ServerMessage::SessionDetached) => {
                            info!("Session detached");
                            break;
                        }
                        Ok(ServerMessage::Error { message }) => {
                            error!("Server error: {}", message);
                            break;
                        }
                        Ok(ServerMessage::LayoutUpdate { layout }) => {
                            self.current_layout = Some(layout);
                            self.render_layout().await?;
                        }
                        _ => {}
                    }
                }
            }
        }

        Ok(())
    }

    async fn handle_key_event(&mut self, key_event: KeyEvent) -> Result<bool> {
        if key_event.modifiers == KeyModifiers::CONTROL && key_event.code == KeyCode::Char('b') {
            if let Some(next_key) = event::read().ok() {
                if let Event::Key(next_event) = next_key {
                    match next_event.code {
                        // Detach
                        KeyCode::Char('d') => {
                            self.detach_session().await?;
                            return Ok(true);
                        }
                        // Split pane vertically
                        KeyCode::Char('%') => {
                            if let Some(framed) = &mut self.framed {
                                framed.send(ClientMessage::SplitPane {
                                    direction: crate::protocol::SplitDirection::Vertical
                                }).await?;
                            }
                        }
                        // Split pane horizontally
                        KeyCode::Char('"') => {
                            if let Some(framed) = &mut self.framed {
                                framed.send(ClientMessage::SplitPane {
                                    direction: crate::protocol::SplitDirection::Horizontal
                                }).await?;
                            }
                        }
                        // Create new window
                        KeyCode::Char('c') => {
                            if let Some(framed) = &mut self.framed {
                                framed.send(ClientMessage::CreateWindow { name: None }).await?;
                            }
                        }
                        // Next window
                        KeyCode::Char('n') => {
                            if let Some(framed) = &mut self.framed {
                                framed.send(ClientMessage::NextWindow).await?;
                            }
                        }
                        // Previous window
                        KeyCode::Char('p') => {
                            if let Some(framed) = &mut self.framed {
                                framed.send(ClientMessage::PreviousWindow).await?;
                            }
                        }
                        // Zoom pane
                        KeyCode::Char('z') => {
                            if let Some(framed) = &mut self.framed {
                                framed.send(ClientMessage::ZoomPane).await?;
                            }
                        }
                        // Enter copy mode
                        KeyCode::Char('[') => {
                            if let Some(framed) = &mut self.framed {
                                framed.send(ClientMessage::EnterCopyMode).await?;
                            }
                        }
                        // Navigate panes with arrow keys
                        KeyCode::Up => {
                            if let Some(framed) = &mut self.framed {
                                framed.send(ClientMessage::NavigatePane {
                                    direction: crate::protocol::PaneNavigationDirection::Up
                                }).await?;
                            }
                        }
                        KeyCode::Down => {
                            if let Some(framed) = &mut self.framed {
                                framed.send(ClientMessage::NavigatePane {
                                    direction: crate::protocol::PaneNavigationDirection::Down
                                }).await?;
                            }
                        }
                        KeyCode::Left => {
                            if let Some(framed) = &mut self.framed {
                                framed.send(ClientMessage::NavigatePane {
                                    direction: crate::protocol::PaneNavigationDirection::Left
                                }).await?;
                            }
                        }
                        KeyCode::Right => {
                            if let Some(framed) = &mut self.framed {
                                framed.send(ClientMessage::NavigatePane {
                                    direction: crate::protocol::PaneNavigationDirection::Right
                                }).await?;
                            }
                        }
                        // Kill pane
                        KeyCode::Char('x') => {
                            if let Some(framed) = &mut self.framed {
                                framed.send(ClientMessage::KillPane).await?;
                            }
                        }
                        // List windows
                        KeyCode::Char('w') => {
                            if let Some(framed) = &mut self.framed {
                                framed.send(ClientMessage::ListWindows).await?;
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        let mut data = Vec::new();
        match key_event.code {
            KeyCode::Char(c) => {
                if key_event.modifiers == KeyModifiers::CONTROL {
                    data.push((c as u8) - b'a' + 1);
                } else {
                    data.push(c as u8);
                }
            }
            KeyCode::Enter => data.push(b'\r'),
            KeyCode::Tab => data.push(b'\t'),
            KeyCode::Backspace => data.push(127),
            KeyCode::Esc => data.push(27),
            KeyCode::Up => data.extend_from_slice(b"\x1b[A"),
            KeyCode::Down => data.extend_from_slice(b"\x1b[B"),
            KeyCode::Right => data.extend_from_slice(b"\x1b[C"),
            KeyCode::Left => data.extend_from_slice(b"\x1b[D"),
            _ => {}
        }

        if !data.is_empty() {
            if let Some(framed) = &mut self.framed {
                framed.send(ClientMessage::Input { data }).await?;
            }
        }

        Ok(false)
    }

    async fn handle_output(&mut self, data: Vec<u8>) -> Result<()> {
        use std::io::Write;
        let mut stdout = stdout();
        stdout.write_all(&data)?;
        stdout.flush()?;
        Ok(())
    }

    async fn handle_pane_output(&mut self, pane_id: PaneId, data: Vec<u8>) -> Result<()> {
        // Store the output in the pane buffer
        self.pane_buffers.entry(pane_id.clone()).or_insert_with(Vec::new).extend(data);

        // Keep buffer size reasonable (last 1000 lines worth)
        if let Some(buffer) = self.pane_buffers.get_mut(&pane_id) {
            const MAX_BUFFER_SIZE: usize = 100_000; // ~1000 lines
            if buffer.len() > MAX_BUFFER_SIZE {
                buffer.drain(0..buffer.len() - MAX_BUFFER_SIZE);
            }
        }

        // Re-render layout to show updated content
        self.render_layout().await?;
        Ok(())
    }

    async fn render_layout(&mut self) -> Result<()> {
        if let Some(layout) = self.current_layout.clone() {
            self.clear_screen().await?;
            self.draw_panes(&layout).await?;
        }
        Ok(())
    }

    async fn clear_screen(&mut self) -> Result<()> {
        execute!(stdout(), crossterm::terminal::Clear(crossterm::terminal::ClearType::All))?;
        Ok(())
    }

    async fn draw_panes(&mut self, layout: &LayoutInfo) -> Result<()> {
        use std::io::Write;
        let mut stdout = stdout();

        for pane in &layout.panes {
            // Draw pane border
            self.draw_pane_border(pane).await?;

            // Draw pane content
            self.draw_pane_content(pane).await?;
        }

        // Draw status bar
        self.render_status_bar().await?;

        stdout.flush()?;
        Ok(())
    }

    async fn draw_pane_content(&mut self, pane: &PaneInfo) -> Result<()> {
        use std::io::Write;
        let mut stdout = stdout();

        // Calculate content area (inside borders)
        let content_x = pane.x + 1;
        let content_y = pane.y + 1;
        let content_width = if pane.width > 2 { pane.width - 2 } else { 0 };
        let content_height = if pane.height > 2 { pane.height - 2 } else { 0 };

        if content_width == 0 || content_height == 0 {
            return Ok(());
        }

        // Get pane buffer content
        if let Some(buffer) = self.pane_buffers.get(&pane.id) {
            // Convert buffer to string and split into lines
            let content = String::from_utf8_lossy(buffer);
            let lines: Vec<&str> = content.lines().collect();

            // Show last N lines that fit in the pane
            let visible_lines = std::cmp::min(lines.len(), content_height as usize);
            let start_line = if lines.len() > content_height as usize {
                lines.len() - content_height as usize
            } else {
                0
            };

            for (i, line) in lines[start_line..start_line + visible_lines].iter().enumerate() {
                execute!(stdout, crossterm::cursor::MoveTo(content_x, content_y + i as u16))?;

                // Truncate line to fit in pane width
                let display_line = if line.len() > content_width as usize {
                    &line[..content_width as usize]
                } else {
                    line
                };

                write!(stdout, "{}", display_line)?;
            }
        } else {
            // No content yet, show pane info
            execute!(stdout, crossterm::cursor::MoveTo(content_x, content_y))?;
            if pane.is_focused {
                write!(stdout, "🔸 Focused Pane {:.8}", pane.id.0)?;
            } else {
                write!(stdout, "⚪ Pane {:.8}", pane.id.0)?;
            }
        }

        Ok(())
    }

    async fn draw_pane_border(&mut self, pane: &PaneInfo) -> Result<()> {
        use std::io::Write;
        let mut stdout = stdout();

        let border_char = if pane.is_focused { '█' } else { '│' };
        let corner_char = if pane.is_focused { '█' } else { '┌' };

        // Top border
        execute!(stdout, crossterm::cursor::MoveTo(pane.x, pane.y))?;
        write!(stdout, "{}", corner_char)?;
        for _ in 1..pane.width-1 {
            write!(stdout, "─")?;
        }
        if pane.width > 1 {
            write!(stdout, "{}", if pane.is_focused { '█' } else { '┐' })?;
        }

        // Side borders
        for y in 1..pane.height-1 {
            execute!(stdout, crossterm::cursor::MoveTo(pane.x, pane.y + y))?;
            write!(stdout, "{}", border_char)?;
            if pane.width > 1 {
                execute!(stdout, crossterm::cursor::MoveTo(pane.x + pane.width - 1, pane.y + y))?;
                write!(stdout, "{}", border_char)?;
            }
        }

        // Bottom border
        if pane.height > 1 {
            execute!(stdout, crossterm::cursor::MoveTo(pane.x, pane.y + pane.height - 1))?;
            write!(stdout, "{}", if pane.is_focused { '█' } else { '└' })?;
            for _ in 1..pane.width-1 {
                write!(stdout, "─")?;
            }
            if pane.width > 1 {
                write!(stdout, "{}", if pane.is_focused { '█' } else { '┘' })?;
            }
        }

        Ok(())
    }

    pub async fn save_snapshot(&mut self, session_id: SessionId, name: Option<String>, description: Option<String>) -> Result<std::path::PathBuf> {
        if let Some(framed) = &mut self.framed {
            framed.send(ClientMessage::SaveSnapshot { session_id, name, description }).await?;

            if let Some(response) = framed.next().await {
                match response? {
                    ServerMessage::SnapshotSaved { path } => Ok(path),
                    ServerMessage::Error { message } => {
                        Err(FerrixError::Other(message))
                    }
                    _ => Err(FerrixError::Other("Unexpected server response".to_string())),
                }
            } else {
                Err(FerrixError::Other("No response from server".to_string()))
            }
        } else {
            Err(FerrixError::NotConnected)
        }
    }

    pub async fn load_snapshot(&mut self, path: std::path::PathBuf) -> Result<SessionId> {
        if let Some(framed) = &mut self.framed {
            framed.send(ClientMessage::LoadSnapshot { path }).await?;

            if let Some(response) = framed.next().await {
                match response? {
                    ServerMessage::SnapshotLoaded { session_id } => Ok(session_id),
                    ServerMessage::Error { message } => {
                        Err(FerrixError::Other(message))
                    }
                    _ => Err(FerrixError::Other("Unexpected server response".to_string())),
                }
            } else {
                Err(FerrixError::Other("No response from server".to_string()))
            }
        } else {
            Err(FerrixError::NotConnected)
        }
    }

    pub async fn list_snapshots(&mut self) -> Result<Vec<crate::protocol::SnapshotInfo>> {
        if let Some(framed) = &mut self.framed {
            framed.send(ClientMessage::ListSnapshots).await?;

            if let Some(response) = framed.next().await {
                match response? {
                    ServerMessage::SnapshotList { snapshots } => Ok(snapshots),
                    ServerMessage::Error { message } => {
                        Err(FerrixError::Other(message))
                    }
                    _ => Err(FerrixError::Other("Unexpected server response".to_string())),
                }
            } else {
                Err(FerrixError::Other("No response from server".to_string()))
            }
        } else {
            Err(FerrixError::NotConnected)
        }
    }

    pub async fn delete_snapshot(&mut self, path: std::path::PathBuf) -> Result<()> {
        if let Some(framed) = &mut self.framed {
            framed.send(ClientMessage::DeleteSnapshot { path: path.clone() }).await?;

            if let Some(response) = framed.next().await {
                match response? {
                    ServerMessage::SnapshotDeleted { .. } => Ok(()),
                    ServerMessage::Error { message } => {
                        Err(FerrixError::Other(message))
                    }
                    _ => Err(FerrixError::Other("Unexpected server response".to_string())),
                }
            } else {
                Err(FerrixError::Other("No response from server".to_string()))
            }
        } else {
            Err(FerrixError::NotConnected)
        }
    }


    async fn session_loop(&mut self) -> Result<()> {
        use crossterm::{
            event::{Event, EventStream, KeyCode, KeyEvent, KeyModifiers},
            terminal::{self, enable_raw_mode, disable_raw_mode, Clear, ClearType},
            execute,
        };
        use futures::StreamExt;

        // Enable raw mode for terminal control
        enable_raw_mode()?;

        // Clear screen and position cursor
        let mut stdout = std::io::stdout();
        execute!(stdout, Clear(ClearType::All), crossterm::cursor::MoveTo(0, 0))?;

        let mut reader = EventStream::new();

        loop {
            tokio::select! {
                // Handle user input
                maybe_event = reader.next() => {
                    if let Some(Ok(event)) = maybe_event {
                        if let Event::Key(key_event) = event {
                            if !self.handle_key_event(key_event).await? {
                                break; // Exit session loop (detach)
                            }
                        } else if let Event::Resize(cols, rows) = event {
                            if let Some(framed) = &mut self.framed {
                                framed.send(ClientMessage::Resize { cols, rows }).await?;
                            }
                        }
                    }
                }

                // Handle server messages
                maybe_message = async {
                    if let Some(framed) = &mut self.framed {
                        framed.next().await
                    } else {
                        None
                    }
                } => {
                    if let Some(message_result) = maybe_message {
                        match message_result? {
                            ServerMessage::Output { data } => {
                                // Handle single-pane output (legacy)
                                std::io::Write::write_all(&mut stdout, &data)?;
                                std::io::Write::flush(&mut stdout)?;
                            }
                            ServerMessage::PaneOutput { pane_id, data } => {
                                self.handle_pane_output(pane_id, data).await?;
                            }
                            ServerMessage::LayoutUpdate { layout } => {
                                self.handle_layout_update(layout).await?;
                            }
                            ServerMessage::CopyModeEntered => {
                                self.handle_copy_mode_entered().await?;
                            }
                            ServerMessage::SessionDetached => {
                                info!("Session detached");
                                break;
                            }
                            ServerMessage::Error { message } => {
                                error!("Server error: {}", message);
                            }
                            _ => {
                                // Handle other server messages
                            }
                        }
                    } else {
                        // Connection lost
                        break;
                    }
                }
            }
        }

        // Cleanup: disable raw mode
        disable_raw_mode()?;
        execute!(stdout, Clear(ClearType::All), crossterm::cursor::MoveTo(0, 0))?;
        println!("Session detached.");

        Ok(())
    }

    async fn handle_copy_mode_entered(&mut self) -> Result<()> {
        use std::io::Write;
        let mut stdout = std::io::stdout();

        // Show copy mode indicator
        execute!(
            stdout,
            crossterm::cursor::MoveTo(0, 0),
            crossterm::style::SetBackgroundColor(crossterm::style::Color::Blue),
            crossterm::style::SetForegroundColor(crossterm::style::Color::White)
        )?;
        write!(stdout, " COPY MODE - Use arrow keys to navigate, Enter to copy, Esc to exit ")?;
        execute!(stdout, crossterm::style::ResetColor)?;
        std::io::Write::flush(&mut stdout)?;

        info!("Entered copy mode");
        Ok(())
    }

    async fn handle_layout_update(&mut self, layout: crate::protocol::LayoutInfo) -> Result<()> {
        // Store the layout for rendering
        self.current_layout = Some(layout.clone());

        // Re-render the screen with the new layout
        self.render_layout().await?;

        Ok(())
    }


    async fn render_status_bar(&self) -> Result<()> {
        use crossterm::{cursor::MoveTo, style::{Color, SetBackgroundColor, SetForegroundColor, ResetColor}, execute};
        use std::io::{stdout, Write};

        let mut stdout = stdout();
        let (cols, rows) = self.terminal_size;

        // Render status bar at the bottom of the screen
        execute!(stdout, MoveTo(0, rows - 1))?;
        execute!(stdout, SetBackgroundColor(Color::DarkBlue), SetForegroundColor(Color::White))?;

        // Build status bar content
        let session_name = self.attached_session
            .as_ref()
            .map(|s| format!("{:.8}", s.0))
            .unwrap_or_else(|| "No Session".to_string());

        let window_info = if let Some(layout) = &self.current_layout {
            format!("W:{} P:{}", layout.window_id.0.to_string()[..8].to_string(), layout.panes.len())
        } else {
            "W:- P:-".to_string()
        };

        let time = chrono::Local::now().format("%H:%M:%S").to_string();

        // Format status bar with padding
        let left_section = format!(" Ferrix [{}]", session_name);
        let center_section = format!("[{}]", window_info);
        let right_section = format!("{} ", time);

        // Calculate spacing to fill the screen width
        let used_width = left_section.len() + center_section.len() + right_section.len();
        let available_width = cols as usize;

        if used_width <= available_width {
            let left_padding = (available_width - used_width) / 2;
            let right_padding = available_width - used_width - left_padding;

            write!(stdout, "{}{}{}{}",
                left_section,
                " ".repeat(left_padding),
                center_section,
                " ".repeat(right_padding))?;
            write!(stdout, "{}", right_section)?;
        } else {
            // Truncate if too long
            let truncated = format!("{}{}{}", left_section, center_section, right_section);
            let display_text = if truncated.len() > available_width {
                &truncated[..available_width]
            } else {
                &truncated
            };
            write!(stdout, "{}", display_text)?;
        }

        execute!(stdout, ResetColor)?;
        stdout.flush()?;

        Ok(())
    }

}