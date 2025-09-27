pub mod connection;
pub mod renderer;
// #[cfg(test)]
// mod tests;

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
use std::io::{stdout, IsTerminal};
use tracing::{debug, error, info};

use crate::error::{FerrixError, Result};
use crate::protocol::{ClientMessage, ServerMessage, SessionId, codec::FerrixClientCodec, LayoutInfo, PaneInfo, PaneId};
use crate::config::{Config, keybindings::{KeyBindingManager, KeyBinding, Action}};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct Client {
    socket_path: PathBuf,
    attached_session: Option<SessionId>,
    framed: Option<Framed<UnixStream, FerrixClientCodec>>,
    current_layout: Option<LayoutInfo>,
    terminal_size: (u16, u16), // (cols, rows)
    pane_buffers: HashMap<PaneId, Vec<u8>>, // Buffer terminal output per pane
    copy_mode: CopyModeState,
    config: Arc<RwLock<Config>>,
    key_binding_manager: Arc<RwLock<KeyBindingManager>>,
    prefix_mode: bool, // Track if we're waiting for the second key after prefix
}

#[derive(Debug, Clone)]
struct CopyModeState {
    active: bool,
    cursor_row: usize,
    cursor_col: usize,
    selection_start: Option<(usize, usize)>,
    selection_end: Option<(usize, usize)>,
    buffer_content: Vec<String>,
    mode: String, // COPY, VISUAL, VISUAL_LINE
}

impl Client {
    pub fn new(socket_path: PathBuf) -> Result<Self> {
        let config = Config::load().unwrap_or_default();
        let mut key_binding_manager = KeyBindingManager::new();

        // Update key binding manager with config
        if let Ok(prefix_key) = KeyBindingManager::parse_key_string(&config.keybindings.prefix) {
            key_binding_manager.set_prefix(prefix_key);
        }

        // Add custom key bindings from config
        for (key_str, action_str) in &config.keybindings.custom {
            if let Ok(key) = KeyBindingManager::parse_key_string(key_str) {
                let action = Action::Custom(action_str.clone());
                key_binding_manager.bind(key, action);
            }
        }

        Ok(Self {
            socket_path,
            attached_session: None,
            framed: None,
            current_layout: None,
            terminal_size: (80, 24),
            pane_buffers: HashMap::new(),
            copy_mode: CopyModeState {
                active: false,
                cursor_row: 0,
                cursor_col: 0,
                selection_start: None,
                selection_end: None,
                buffer_content: Vec::new(),
                mode: "COPY".to_string(),
            },
            config: Arc::new(RwLock::new(config)),
            key_binding_manager: Arc::new(RwLock::new(key_binding_manager)),
            prefix_mode: false,
        })
    }

    pub async fn reload_config(&mut self) -> Result<()> {
        let new_config = Config::load().unwrap_or_default();
        let mut key_binding_manager = KeyBindingManager::new();

        // Update key binding manager with new config
        if let Ok(prefix_key) = KeyBindingManager::parse_key_string(&new_config.keybindings.prefix) {
            key_binding_manager.set_prefix(prefix_key);
        }

        // Add custom key bindings from config
        for (key_str, action_str) in &new_config.keybindings.custom {
            if let Ok(key) = KeyBindingManager::parse_key_string(key_str) {
                let action = Action::Custom(action_str.clone());
                key_binding_manager.bind(key, action);
            }
        }

        // Update the shared state
        *self.config.write().await = new_config;
        *self.key_binding_manager.write().await = key_binding_manager;

        info!("Configuration reloaded successfully");
        Ok(())
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
            self.run_attached().await
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
        // Only enable raw mode if we're in an interactive terminal
        let is_tty = std::io::stdin().is_terminal();

        if is_tty {
            terminal::enable_raw_mode()?;
            execute!(stdout(), EnterAlternateScreen, cursor::Hide)?;

            let (term_width, term_height) = terminal::size()?;
            self.terminal_size = (term_width, term_height);
            if let Some(framed) = &mut self.framed {
                framed.send(ClientMessage::Resize { cols: term_width, rows: term_height }).await?;
            }
        } else {
            // Use default terminal size when not in a TTY
            self.terminal_size = (80, 24);
            if let Some(framed) = &mut self.framed {
                framed.send(ClientMessage::Resize { cols: 80, rows: 24 }).await?;
            }
        }

        let result = self.handle_attached_session().await;

        if is_tty {
            terminal::disable_raw_mode()?;
            execute!(stdout(), LeaveAlternateScreen, cursor::Show)?;
        }

        result
    }

    async fn handle_attached_session(&mut self) -> Result<()> {
        let is_tty = std::io::stdin().is_terminal();

        // Only create event stream if we're in a TTY
        let mut event_reader = if is_tty {
            Some(event::EventStream::new())
        } else {
            None
        };

        // For non-TTY mode, spawn a task to read from stdin
        let (stdin_tx, mut stdin_rx) = if !is_tty {
            let (tx, rx) = tokio::sync::mpsc::channel::<Vec<u8>>(100);
            let tx_clone = tx.clone();
            tokio::spawn(async move {
                use tokio::io::AsyncReadExt;
                let mut stdin = tokio::io::stdin();
                let mut buffer = vec![0u8; 1024];
                loop {
                    match stdin.read(&mut buffer).await {
                        Ok(0) => break, // EOF
                        Ok(n) => {
                            let data = buffer[..n].to_vec();
                            if tx_clone.send(data).await.is_err() {
                                break;
                            }
                        }
                        Err(e) => {
                            tracing::error!("Error reading stdin: {}", e);
                            break;
                        }
                    }
                }
            });
            (Some(tx), Some(rx))
        } else {
            (None, None)
        };

        loop {
            tokio::select! {
                // Handle stdin input in non-TTY mode
                Some(data) = async {
                    if let Some(rx) = &mut stdin_rx {
                        rx.recv().await
                    } else {
                        None
                    }
                } => {
                    if let Some(framed) = &mut self.framed {
                        framed.send(ClientMessage::Input { data }).await?;
                    }
                }

                Some(event_result) = async {
                    if let Some(reader) = &mut event_reader {
                        reader.next().await
                    } else {
                        None
                    }
                } => {
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
                            if std::io::stdin().is_terminal() {
                                self.render_layout().await?;
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        Ok(())
    }

    async fn handle_key_event(&mut self, key_event: KeyEvent) -> Result<bool> {
        // If in copy mode, handle copy mode keys
        if self.copy_mode.active {
            return self.handle_copy_mode_key(key_event).await;
        }

        let key_binding = KeyBinding {
            modifiers: key_event.modifiers,
            code: key_event.code,
        };

        // Check for prefix key and actions
        let (is_prefix, action_to_execute) = {
            let key_manager = self.key_binding_manager.read().await;
            let prefix_key = key_manager.get_prefix().clone();

            let is_prefix = key_binding == prefix_key;
            let action = if self.prefix_mode && !is_prefix {
                key_manager.get_action(&key_binding).cloned()
            } else {
                None
            };

            (is_prefix, action)
        };

        // Check if this is the prefix key
        if is_prefix {
            self.prefix_mode = true;
            return Ok(false);
        }

        // If we're in prefix mode, check for action bindings
        if self.prefix_mode {
            self.prefix_mode = false;

            if let Some(action) = action_to_execute {
                return self.execute_action(action).await;
            }

            // If no binding found, fall through to normal key handling
        }

        // Handle normal key input (not command keys)
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

    async fn execute_action(&mut self, action: Action) -> Result<bool> {
        match action {
            Action::DetachSession => {
                self.detach_session().await?;
                return Ok(true);
            }
            Action::SplitVertical => {
                if let Some(framed) = &mut self.framed {
                    framed.send(ClientMessage::SplitPane {
                        direction: crate::protocol::SplitDirection::Vertical
                    }).await?;
                }
            }
            Action::SplitHorizontal => {
                if let Some(framed) = &mut self.framed {
                    framed.send(ClientMessage::SplitPane {
                        direction: crate::protocol::SplitDirection::Horizontal
                    }).await?;
                }
            }
            Action::NewWindow => {
                if let Some(framed) = &mut self.framed {
                    framed.send(ClientMessage::CreateWindow { name: None }).await?;
                }
            }
            Action::NextWindow => {
                if let Some(framed) = &mut self.framed {
                    framed.send(ClientMessage::NextWindow).await?;
                }
            }
            Action::PreviousWindow => {
                if let Some(framed) = &mut self.framed {
                    framed.send(ClientMessage::PreviousWindow).await?;
                }
            }
            Action::ZoomPane => {
                if let Some(framed) = &mut self.framed {
                    framed.send(ClientMessage::ZoomPane).await?;
                }
            }
            Action::EnterCopyMode => {
                if let Some(framed) = &mut self.framed {
                    framed.send(ClientMessage::EnterCopyMode).await?;
                }
            }
            Action::NavigateUp => {
                if let Some(framed) = &mut self.framed {
                    framed.send(ClientMessage::NavigatePane {
                        direction: crate::protocol::PaneNavigationDirection::Up
                    }).await?;
                }
            }
            Action::NavigateDown => {
                if let Some(framed) = &mut self.framed {
                    framed.send(ClientMessage::NavigatePane {
                        direction: crate::protocol::PaneNavigationDirection::Down
                    }).await?;
                }
            }
            Action::NavigateLeft => {
                if let Some(framed) = &mut self.framed {
                    framed.send(ClientMessage::NavigatePane {
                        direction: crate::protocol::PaneNavigationDirection::Left
                    }).await?;
                }
            }
            Action::NavigateRight => {
                if let Some(framed) = &mut self.framed {
                    framed.send(ClientMessage::NavigatePane {
                        direction: crate::protocol::PaneNavigationDirection::Right
                    }).await?;
                }
            }
            Action::ClosePane => {
                if let Some(framed) = &mut self.framed {
                    framed.send(ClientMessage::KillPane).await?;
                }
            }
            Action::ListSessions => {
                if let Some(framed) = &mut self.framed {
                    framed.send(ClientMessage::ListWindows).await?;
                }
            }
            Action::ReloadConfig => {
                self.reload_config().await?;
            }
            Action::SelectWindow(num) => {
                // Handle window selection (0-9)
                // For now, we'll just log this as window selection isn't directly supported
                // In a full implementation, you'd need to get the list of windows first
                // and then switch to the nth window
                info!("Window selection {} requested (not yet implemented)", num);
            }
            Action::Custom(command) => {
                // Handle custom commands
                info!("Executing custom command: {}", command);
                // For now, just log the custom command
                // In a full implementation, you'd parse and execute the command
            }
            _ => {
                // Handle other actions as needed
                debug!("Unhandled action: {:?}", action);
            }
        }
        Ok(false)
    }

    async fn handle_copy_mode_key(&mut self, key_event: KeyEvent) -> Result<bool> {
        let mut key_str = String::new();

        // Handle key events and convert to string representation for server
        match key_event.code {
            KeyCode::Char(c) => {
                if key_event.modifiers == KeyModifiers::CONTROL {
                    key_str = format!("Ctrl+{}", c);
                } else {
                    key_str = c.to_string();
                }
            }
            KeyCode::Esc => {
                // Exit copy mode
                if let Some(framed) = &mut self.framed {
                    framed.send(ClientMessage::ExitCopyMode).await?;
                }
                return Ok(false);
            }
            KeyCode::Up => key_str = "Up".to_string(),
            KeyCode::Down => key_str = "Down".to_string(),
            KeyCode::Left => key_str = "Left".to_string(),
            KeyCode::Right => key_str = "Right".to_string(),
            KeyCode::Enter => key_str = "Enter".to_string(),
            KeyCode::Tab => key_str = "Tab".to_string(),
            KeyCode::Backspace => key_str = "Backspace".to_string(),
            // Vim-style movement keys
            _ => {
                // For other keys, convert to character if possible
                if let KeyCode::Char(c) = key_event.code {
                    key_str = c.to_string();
                }
            }
        }

        if !key_str.is_empty() {
            if let Some(framed) = &mut self.framed {
                framed.send(ClientMessage::CopyModeInput { key: key_str }).await?;
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
        use std::io::Write;

        let is_tty = std::io::stdin().is_terminal();

        // Store the output in the pane buffer
        self.pane_buffers.entry(pane_id.clone())
            .or_insert_with(Vec::new)
            .extend(&data);

        // Keep buffer size reasonable (last 1000 lines worth)
        if let Some(buffer) = self.pane_buffers.get_mut(&pane_id) {
            const MAX_BUFFER_SIZE: usize = 100_000; // ~1000 lines
            if buffer.len() > MAX_BUFFER_SIZE {
                buffer.drain(0..buffer.len() - MAX_BUFFER_SIZE);
            }
        }

        // In non-TTY mode, always write output to stdout
        if !is_tty {
            let mut stdout = stdout();
            stdout.write_all(&data)?;
            stdout.flush()?;
            return Ok(());
        }

        // In TTY mode, handle focused pane
        if let Some(layout) = &self.current_layout {
            for pane_info in &layout.panes {
                if pane_info.id == pane_id && pane_info.is_focused {
                    // Write the data directly to stdout for immediate display
                    let mut stdout = stdout();
                    stdout.write_all(&data)?;
                    stdout.flush()?;
                    return Ok(());
                }
            }
        }

        // Otherwise re-render the full layout (only if in TTY)
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


    #[allow(dead_code)]
    async fn session_loop(&mut self) -> Result<()> {
        use crossterm::{
            event::{Event, EventStream},
            terminal::{enable_raw_mode, disable_raw_mode, Clear, ClearType},
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
                            ServerMessage::CopyModeUpdate { cursor_row, cursor_col, selection_start, selection_end, buffer_content, mode } => {
                                self.handle_copy_mode_update(cursor_row, cursor_col, selection_start, selection_end, buffer_content, mode).await?;
                            }
                            ServerMessage::CopyModeExited => {
                                self.handle_copy_mode_exited().await?;
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
        self.copy_mode.active = true;
        self.render_copy_mode().await?;
        info!("Entered copy mode");
        Ok(())
    }

    async fn handle_copy_mode_update(&mut self, cursor_row: usize, cursor_col: usize, selection_start: Option<(usize, usize)>, selection_end: Option<(usize, usize)>, buffer_content: Vec<String>, mode: String) -> Result<()> {
        self.copy_mode.cursor_row = cursor_row;
        self.copy_mode.cursor_col = cursor_col;
        self.copy_mode.selection_start = selection_start;
        self.copy_mode.selection_end = selection_end;
        self.copy_mode.buffer_content = buffer_content;
        self.copy_mode.mode = mode;

        if self.copy_mode.active {
            self.render_copy_mode().await?;
        }
        Ok(())
    }

    async fn handle_copy_mode_exited(&mut self) -> Result<()> {
        self.copy_mode.active = false;
        self.copy_mode.selection_start = None;
        self.copy_mode.selection_end = None;

        // Clear screen and re-render normal layout
        self.clear_screen().await?;
        self.render_layout().await?;

        info!("Exited copy mode");
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

    async fn render_copy_mode(&mut self) -> Result<()> {
        use crossterm::{
            cursor::MoveTo,
            style::{Color, SetBackgroundColor, SetForegroundColor, ResetColor},
            terminal::{Clear, ClearType},
            execute,
        };
        use std::io::{stdout, Write};

        let mut stdout = stdout();
        let (cols, rows) = self.terminal_size;

        // Clear the screen
        execute!(stdout, Clear(ClearType::All))?;

        // Render copy mode indicator at the top
        execute!(stdout, MoveTo(0, 0))?;
        execute!(stdout, SetBackgroundColor(Color::Blue), SetForegroundColor(Color::White))?;

        let mode_indicator = format!(" {} MODE ", self.copy_mode.mode);
        let help_text = " h/j/k/l:move v:visual V:visual-line y:yank /:search Esc:exit ";
        let padding = cols.saturating_sub(mode_indicator.len() as u16 + help_text.len() as u16);

        write!(stdout, "{}{}{}", mode_indicator, " ".repeat(padding as usize), help_text)?;
        execute!(stdout, ResetColor)?;

        // Render buffer content starting from row 1
        let visible_rows = (rows - 2) as usize; // Reserve space for header and status
        let buffer_len = self.copy_mode.buffer_content.len();

        // Calculate scroll offset to keep cursor visible
        let scroll_offset = if self.copy_mode.cursor_row >= visible_rows {
            self.copy_mode.cursor_row - visible_rows + 1
        } else {
            0
        };

        // Render buffer lines
        for (i, line) in self.copy_mode.buffer_content
            .iter()
            .enumerate()
            .skip(scroll_offset)
            .take(visible_rows)
        {
            let screen_row = (i - scroll_offset + 1) as u16;
            execute!(stdout, MoveTo(0, screen_row))?;

            // Check if this line has selection
            let line_has_selection = self.is_line_selected(i);
            let (start_col, end_col) = self.get_selection_range_for_line(i);

            if line_has_selection {
                // Render line with selection highlighting
                self.render_line_with_selection(line, i, start_col, end_col, &mut stdout)?;
            } else {
                // Normal line
                write!(stdout, "{}", line)?;
            }

            // Highlight cursor position
            if i == self.copy_mode.cursor_row {
                let cursor_col = self.copy_mode.cursor_col.min(line.len());
                execute!(stdout, MoveTo(cursor_col as u16, screen_row))?;
                execute!(stdout, SetBackgroundColor(Color::White), SetForegroundColor(Color::Black))?;

                if cursor_col < line.len() {
                    let cursor_char = line.chars().nth(cursor_col).unwrap_or(' ');
                    write!(stdout, "{}", cursor_char)?;
                } else {
                    write!(stdout, " ")?;
                }
                execute!(stdout, ResetColor)?;
            }
        }

        // Render status line at the bottom
        execute!(stdout, MoveTo(0, rows - 1))?;
        execute!(stdout, SetBackgroundColor(Color::DarkGrey), SetForegroundColor(Color::White))?;

        let status = format!(
            " Line {}/{} Col {} | {} lines | Selection: {} ",
            self.copy_mode.cursor_row + 1,
            buffer_len,
            self.copy_mode.cursor_col + 1,
            buffer_len,
            if self.copy_mode.selection_start.is_some() && self.copy_mode.selection_end.is_some() {
                "active"
            } else {
                "none"
            }
        );

        let status_padding = cols.saturating_sub(status.len() as u16);
        write!(stdout, "{}{}", status, " ".repeat(status_padding as usize))?;
        execute!(stdout, ResetColor)?;

        stdout.flush()?;
        Ok(())
    }

    fn is_line_selected(&self, line_idx: usize) -> bool {
        if let (Some(start), Some(end)) = (self.copy_mode.selection_start, self.copy_mode.selection_end) {
            let (start_row, _) = start;
            let (end_row, _) = end;
            let min_row = start_row.min(end_row);
            let max_row = start_row.max(end_row);
            line_idx >= min_row && line_idx <= max_row
        } else {
            false
        }
    }

    fn get_selection_range_for_line(&self, line_idx: usize) -> (usize, usize) {
        if let (Some(start), Some(end)) = (self.copy_mode.selection_start, self.copy_mode.selection_end) {
            let (start_row, start_col) = start;
            let (end_row, end_col) = end;

            let (min_pos, max_pos) = if start_row < end_row || (start_row == end_row && start_col <= end_col) {
                (start, end)
            } else {
                (end, start)
            };

            if line_idx == min_pos.0 && line_idx == max_pos.0 {
                // Single line selection
                (min_pos.1, max_pos.1)
            } else if line_idx == min_pos.0 {
                // First line of multi-line selection
                (min_pos.1, usize::MAX)
            } else if line_idx == max_pos.0 {
                // Last line of multi-line selection
                (0, max_pos.1)
            } else {
                // Middle line of multi-line selection
                (0, usize::MAX)
            }
        } else {
            (0, 0)
        }
    }

    fn render_line_with_selection(&self, line: &str, line_idx: usize, start_col: usize, end_col: usize, stdout: &mut std::io::Stdout) -> Result<()> {
        use crossterm::{style::{Color, SetBackgroundColor, SetForegroundColor, ResetColor}, execute};
        use std::io::Write;

        let chars: Vec<char> = line.chars().collect();

        // Before selection
        if start_col > 0 {
            let before_selection: String = chars.iter().take(start_col).collect();
            write!(stdout, "{}", before_selection)?;
        }

        // Selection
        let selection_start = start_col;
        let selection_end = if end_col == usize::MAX { chars.len() } else { end_col.min(chars.len()) };

        if selection_start < chars.len() {
            execute!(stdout, SetBackgroundColor(Color::Yellow), SetForegroundColor(Color::Black))?;
            let selected_text: String = chars.iter()
                .skip(selection_start)
                .take(selection_end - selection_start)
                .collect();
            write!(stdout, "{}", selected_text)?;
            execute!(stdout, ResetColor)?;
        }

        // After selection
        if selection_end < chars.len() {
            let after_selection: String = chars.iter().skip(selection_end).collect();
            write!(stdout, "{}", after_selection)?;
        }

        Ok(())
    }

}