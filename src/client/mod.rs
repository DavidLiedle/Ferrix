pub mod connection;
pub mod renderer;
pub mod ansi_parser;
// #[cfg(test)]
// mod tests;

use std::path::PathBuf;
use tokio::net::UnixStream;
use tokio_util::codec::Framed;
use futures::{StreamExt, SinkExt};
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers, MouseEvent, EnableMouseCapture, DisableMouseCapture},
    terminal::{self},
    execute,
    cursor,
};
use std::io::{stdout, IsTerminal};
use tracing::{debug, error, info, warn};

use crate::error::{FerrixError, Result};
use crate::protocol::{ClientMessage, ServerMessage, SessionId, codec::FerrixClientCodec, LayoutInfo, PaneInfo, PaneId};
use crate::config::{Config, keybindings::{KeyBindingManager, KeyBinding, Action}};
use crate::ui::copymode::{CopyMode, CopyModeState, SearchDirection};
use crate::ui::mouse::{MouseHandler, MouseAction};
use crate::ui::commandmode::{CommandMode, CommandResult};
use crate::ui::window_selector::{WindowSelector, WindowInfo};
use crate::ui::help::HelpOverlay;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub enum MessageType {
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Debug, Clone)]
pub struct Message {
    pub text: String,
    pub msg_type: MessageType,
    pub timestamp: Instant,
}

pub struct Client {
    socket_path: PathBuf,
    attached_session: Option<SessionId>,
    attached_session_name: Option<String>,
    framed: Option<Framed<UnixStream, FerrixClientCodec>>,
    current_layout: Option<LayoutInfo>,
    terminal_size: (u16, u16), // (cols, rows)
    pane_buffers: HashMap<PaneId, Vec<u8>>, // Buffer terminal output per pane
    pane_parsers: HashMap<PaneId, ansi_parser::AnsiParser>, // ANSI parser per pane
    copy_mode: CopyMode,
    command_mode: CommandMode,
    mouse_handler: MouseHandler,
    window_selector: WindowSelector,
    help_overlay: HelpOverlay,
    config: Arc<RwLock<Config>>,
    key_binding_manager: Arc<KeyBindingManager>,
    prefix_mode: bool, // Track if we're waiting for the second key after prefix
    // Mouse selection state (works outside copy mode)
    active_selection: Option<((u16, u16), (u16, u16))>, // (start, end) in screen coordinates
    // Status bar messages
    messages: VecDeque<Message>,
    message_timeout: Duration,
    // Render throttling to prevent flicker
    last_render: Instant,
    render_interval: Duration,
    pending_render: bool, // Track if we skipped a render and need to catch up
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

        let copy_mode_style = config.copy_mode.mode.clone();
        let mouse_enabled = config.general.mouse;

        Ok(Self {
            socket_path,
            attached_session: None,
            attached_session_name: None,
            framed: None,
            current_layout: None,
            terminal_size: (80, 24),
            pane_buffers: HashMap::new(),
            pane_parsers: HashMap::new(),
            copy_mode: CopyMode::new(copy_mode_style),
            command_mode: CommandMode::new(),
            mouse_handler: MouseHandler::new(mouse_enabled),
            window_selector: WindowSelector::new(),
            help_overlay: HelpOverlay::new(),
            config: Arc::new(RwLock::new(config)),
            key_binding_manager: Arc::new(key_binding_manager),
            prefix_mode: false,
            active_selection: None,
            messages: VecDeque::new(),
            message_timeout: Duration::from_secs(3),
            last_render: Instant::now(),
            render_interval: Duration::from_millis(16), // ~60 FPS max
            pending_render: false,
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
        // Swap the entire Arc for lock-free reads
        self.key_binding_manager = Arc::new(key_binding_manager);

        info!("Configuration reloaded successfully");
        Ok(())
    }

    pub async fn connect(&mut self) -> Result<()> {
        self.connect_with_auto_start(true).await
    }

    pub async fn connect_with_auto_start(&mut self, auto_start: bool) -> Result<()> {
        let stream_result = UnixStream::connect(&self.socket_path).await;

        let stream = match stream_result {
            Ok(s) => s,
            Err(e) if (e.kind() == std::io::ErrorKind::NotFound || e.kind() == std::io::ErrorKind::ConnectionRefused) && auto_start => {
                // Try to auto-start the server
                info!("Server not running, attempting to start it...");
                if let Err(start_err) = self.try_start_server() {
                    return Err(FerrixError::Ipc(format!(
                        "Failed to connect to server at {:?}: {}. Tried to auto-start server but failed: {}",
                        self.socket_path, e, start_err
                    )));
                }

                // Try connecting again after starting
                UnixStream::connect(&self.socket_path)
                    .await
                    .map_err(|e2| {
                        FerrixError::Ipc(format!(
                            "Failed to connect to server at {:?} after auto-start: {}",
                            self.socket_path, e2
                        ))
                    })?
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound || e.kind() == std::io::ErrorKind::ConnectionRefused => {
                return Err(FerrixError::Ipc(format!(
                    "Failed to connect to server at {:?}: {}. Is the server running? Try: ferrix server",
                    self.socket_path, e
                )));
            }
            Err(e) => {
                return Err(FerrixError::Ipc(format!("Failed to connect to server: {}", e)));
            }
        };

        self.framed = Some(Framed::new(stream, FerrixClientCodec::new()));
        info!("Connected to server at {:?}", self.socket_path);
        Ok(())
    }

    fn try_start_server(&self) -> Result<()> {
        use std::process::Command;

        // Get the path to the current executable
        let exe_path = std::env::current_exe()
            .map_err(|e| FerrixError::Other(format!("Failed to get current executable path: {}", e)))?;

        // Start the server as a background daemon
        Command::new(&exe_path)
            .arg("server")
            .spawn()
            .map_err(|e| FerrixError::Other(format!("Failed to start server: {}", e)))?;

        // Give the server time to fully start and initialize
        // Need extra time for daemonization and socket creation
        std::thread::sleep(std::time::Duration::from_millis(2000));

        Ok(())
    }

    pub async fn create_session(&mut self, name: Option<String>) -> Result<SessionId> {
        if let Some(framed) = &mut self.framed {
            let working_dir = std::env::current_dir().ok();
            framed.send(ClientMessage::CreateSession { name, working_dir }).await?;

            if let Some(Ok(ServerMessage::SessionCreated { session_id, name })) = framed.next().await {
                info!("Created session: {} ({})", name, session_id.0);
                return Ok(session_id);
            }
        }
        Err(FerrixError::Protocol("Failed to create session".to_string()))
    }

    pub async fn attach_session(&mut self, session_id: SessionId) -> Result<()> {
        if let Some(framed) = &mut self.framed {
            // Set up terminal BEFORE attaching so buffer is displayed on alternate screen
            let is_tty = std::io::stdin().is_terminal();
            if is_tty {
                terminal::enable_raw_mode()?;
                execute!(stdout(), crossterm::terminal::EnterAlternateScreen)?;
                execute!(stdout(), crossterm::terminal::Clear(crossterm::terminal::ClearType::All))?;
                execute!(stdout(), cursor::Hide)?;

                // Enable mouse support if configured
                if self.mouse_handler.enabled {
                    execute!(stdout(), EnableMouseCapture)?;
                }
            }

            framed.send(ClientMessage::AttachSession { session_id: session_id.clone() }).await?;

            // Wait for session attached confirmation
            if let Some(response) = framed.next().await {
                match response? {
                    ServerMessage::SessionAttached { session_id: attached_id, name } => {
                        self.attached_session = Some(attached_id.clone());
                        self.attached_session_name = Some(name.clone());
                        info!("Attached to session: {} ({})", name, attached_id.0);

                        // Send terminal size immediately after attaching
                        use std::io::IsTerminal;
                        if std::io::stdin().is_terminal() {
                            let (cols, rows) = crossterm::terminal::size()?;
                            self.terminal_size = (cols, rows);
                            framed.send(ClientMessage::Resize { cols, rows }).await?;
                        } else {
                            // Non-TTY environment, use default size
                            framed.send(ClientMessage::Resize { cols: 80, rows: 24 }).await?;
                        }

                        // Wait for the raw output buffer to be sent and rendered
                        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
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

            // Enter main session loop (terminal already set up)
            self.run_attached_without_setup().await
        } else {
            Err(FerrixError::NotConnected)
        }
    }

    pub async fn detach_session(&mut self) -> Result<()> {
        if let Some(framed) = &mut self.framed {
            framed.send(ClientMessage::DetachSession).await?;

            if let Some(Ok(ServerMessage::SessionDetached)) = framed.next().await {
                self.attached_session = None;
                self.attached_session_name = None;
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

    pub async fn inspect_session(&mut self, session_id: SessionId) -> Result<crate::protocol::DetailedSessionInfo> {
        if let Some(framed) = &mut self.framed {
            framed.send(ClientMessage::InspectSession { session_id }).await?;

            if let Some(Ok(ServerMessage::SessionInspection { inspection })) = framed.next().await {
                return Ok(inspection);
            }
        }
        Err(FerrixError::Protocol("Failed to inspect session".to_string()))
    }

    pub async fn dump_state(&mut self, session_id: SessionId, include_buffers: bool) -> Result<crate::protocol::SessionStateDump> {
        if let Some(framed) = &mut self.framed {
            framed.send(ClientMessage::DumpState { session_id, include_buffers }).await?;

            if let Some(Ok(ServerMessage::StateDump { dump })) = framed.next().await {
                return Ok(dump);
            }
        }
        Err(FerrixError::Protocol("Failed to dump state".to_string()))
    }

    /// Send a generic protocol message to the server
    pub async fn send(&mut self, message: ClientMessage) -> Result<()> {
        if let Some(ref mut framed) = self.framed {
            framed.send(message).await?;
        } else {
            return Err(FerrixError::NotConnected);
        }
        Ok(())
    }

    /// Receive a message from the server
    pub async fn receive(&mut self) -> Result<ServerMessage> {
        if let Some(ref mut framed) = self.framed {
            if let Some(msg) = framed.next().await {
                return msg;
            }
        }
        Err(FerrixError::NotConnected)
    }

    #[allow(dead_code)]
    async fn run_attached(&mut self) -> Result<()> {
        // Only enable raw mode if we're in an interactive terminal
        let is_tty = std::io::stdin().is_terminal();

        if is_tty {
            terminal::enable_raw_mode()?;
            execute!(stdout(), crossterm::terminal::EnterAlternateScreen)?;
            // Clear screen to start fresh
            execute!(stdout(), crossterm::terminal::Clear(crossterm::terminal::ClearType::All))?;
            execute!(stdout(), cursor::Hide)?;

            // Enable mouse support if configured
            if self.mouse_handler.enabled {
                execute!(stdout(), EnableMouseCapture)?;
            }

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
            use std::io::Write;

            // Disable mouse capture if it was enabled
            if self.mouse_handler.enabled {
                execute!(stdout(), DisableMouseCapture)?;
            }

            // Reset terminal state BEFORE leaving alternate screen
            write!(stdout(), "\x1b[?1000l")?;  // Disable X10 mouse tracking
            write!(stdout(), "\x1b[?1002l")?;  // Disable button event tracking
            write!(stdout(), "\x1b[?1003l")?;  // Disable any event tracking
            write!(stdout(), "\x1b[?1006l")?;  // Disable SGR extended mode
            write!(stdout(), "\x1b[?25h")?;    // Show cursor
            write!(stdout(), "\x1b[m")?;        // Reset all attributes
            std::io::stdout().flush()?;

            // Disable raw mode while still on alternate screen
            terminal::disable_raw_mode()?;

            // Leave alternate screen
            execute!(stdout(), crossterm::terminal::LeaveAlternateScreen)?;

            // Clear the main screen after returning to it
            execute!(stdout(),
                crossterm::terminal::Clear(crossterm::terminal::ClearType::All),
                crossterm::cursor::MoveTo(0, 0),
                crossterm::cursor::Show
            )?;

            std::io::stdout().flush()?;
        }

        result
    }

    async fn run_attached_without_setup(&mut self) -> Result<()> {
        // Terminal setup already done in attach_session, just run the loop
        let is_tty = std::io::stdin().is_terminal();
        let result = self.handle_attached_session().await;

        if is_tty {
            use std::io::Write;

            // Disable mouse capture if it was enabled
            if self.mouse_handler.enabled {
                execute!(stdout(), DisableMouseCapture)?;
            }

            // Reset terminal state BEFORE leaving alternate screen
            write!(stdout(), "\x1b[?1000l")?;  // Disable X10 mouse tracking
            write!(stdout(), "\x1b[?1002l")?;  // Disable button event tracking
            write!(stdout(), "\x1b[?1003l")?;  // Disable any event tracking
            write!(stdout(), "\x1b[?1006l")?;  // Disable SGR extended mode
            write!(stdout(), "\x1b[?25h")?;    // Show cursor
            write!(stdout(), "\x1b[m")?;        // Reset all attributes
            std::io::stdout().flush()?;

            // Disable raw mode while still on alternate screen
            terminal::disable_raw_mode()?;

            // Leave alternate screen
            execute!(stdout(), crossterm::terminal::LeaveAlternateScreen)?;

            // Clear the main screen after returning to it
            execute!(stdout(),
                crossterm::terminal::Clear(crossterm::terminal::ClearType::All),
                crossterm::cursor::MoveTo(0, 0),
                crossterm::cursor::Show
            )?;

            std::io::stdout().flush()?;
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
        let (_stdin_tx, mut stdin_rx) = if !is_tty {
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

        // Add a timer for status bar updates (every second for live clock)
        let mut status_bar_timer = tokio::time::interval(tokio::time::Duration::from_secs(1));
        status_bar_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                // Render status bar periodically
                _ = status_bar_timer.tick() => {
                    if let Err(e) = self.render_status_bar().await {
                        tracing::error!("Failed to render status bar: {}", e);
                    }
                }

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
                        Ok(Event::Mouse(mouse_event)) => {
                            self.handle_mouse_event(mouse_event).await?;
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
                            self.show_error(message);
                            if std::io::stdin().is_terminal() {
                                self.render_layout().await?;
                            }
                            // Don't break - allow non-fatal errors to show in status bar
                        }
                        Ok(ServerMessage::DisplayMessage { text, msg_type }) => {
                            match msg_type.as_str() {
                                "info" => self.show_info(text),
                                "success" => self.show_success(text),
                                "warning" => self.show_warning(text),
                                "error" => self.show_error(text),
                                _ => self.show_info(text),
                            }
                            if std::io::stdin().is_terminal() {
                                self.render_layout().await?;
                            }
                        }
                        Ok(ServerMessage::LayoutUpdate { layout }) => {
                            self.handle_layout_update(layout).await?;
                        }
                        Ok(ServerMessage::CopyModeEntered) => {
                            self.handle_copy_mode_entered().await?;
                        }
                        Ok(ServerMessage::CopyModeUpdate { cursor_row, cursor_col, selection_start, selection_end, buffer_content, mode }) => {
                            self.handle_copy_mode_update(cursor_row, cursor_col, selection_start, selection_end, buffer_content, mode).await?;
                        }
                        Ok(ServerMessage::CopyModeExited) => {
                            self.handle_copy_mode_exited().await?;
                        }
                        _ => {}
                    }
                }
            }
        }

        Ok(())
    }

    async fn handle_key_event(&mut self, key_event: KeyEvent) -> Result<bool> {
        // If help overlay is visible, handle help keys
        if self.help_overlay.is_visible() && self.help_overlay.handle_key(key_event) {
            self.render_layout().await?;
            return Ok(false);
        }

        // If window selector is visible, handle window selector keys
        if self.window_selector.is_visible() {
            return self.handle_window_selector_key(key_event).await;
        }

        // If in command mode, handle command mode keys
        if self.command_mode.is_active() {
            return self.handle_command_mode_key(key_event).await;
        }

        // If in copy mode, handle copy mode keys
        if self.copy_mode.is_active() {
            return self.handle_copy_mode_key(key_event).await;
        }

        // Check for colon to enter command mode
        if key_event.code == KeyCode::Char(':') && key_event.modifiers == KeyModifiers::empty() {
            self.command_mode.enter();
            self.render_command_line().await?;
            return Ok(false);
        }

        let key_binding = KeyBinding {
            modifiers: key_event.modifiers,
            code: key_event.code,
        };

        // Check for prefix key and actions (lock-free read via Arc)
        let (is_prefix, action_to_execute) = {
            let key_manager = &self.key_binding_manager;
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
                    // Convert to lowercase for control character calculation
                    let lower_c = c.to_ascii_lowercase();
                    if lower_c.is_ascii_lowercase() {
                        data.push((lower_c as u8) - b'a' + 1);
                    } else if c == '@' {
                        data.push(0); // Ctrl-@ is NUL
                    } else if c == '[' {
                        data.push(27); // Ctrl-[ is ESC
                    } else if c == '\\' {
                        data.push(28); // Ctrl-\ is FS
                    } else if c == ']' {
                        data.push(29); // Ctrl-] is GS
                    } else if c == '^' {
                        data.push(30); // Ctrl-^ is RS
                    } else if c == '_' {
                        data.push(31); // Ctrl-_ is US
                    } else {
                        // For other characters, just send the character
                        data.push(c as u8);
                    }
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
                // Get the current pane's buffer content
                let mut buffer = Vec::new();
                if let Some(layout) = &self.current_layout {
                    for pane_info in &layout.panes {
                        if pane_info.is_focused {
                            if let Some(pane_buffer) = self.pane_buffers.get(&pane_info.id) {
                                // Convert buffer to lines
                                let text = String::from_utf8_lossy(pane_buffer);
                                buffer = text.lines().map(|s| s.to_string()).collect();
                            }
                            break;
                        }
                    }
                }

                // Enter copy mode with the buffer
                self.copy_mode.enter(buffer);
                self.render_copy_mode().await?;
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
            Action::LastPane => {
                if let Some(framed) = &mut self.framed {
                    framed.send(ClientMessage::SelectLastPane).await?;
                }
            }
            Action::ClosePane => {
                if let Some(framed) = &mut self.framed {
                    framed.send(ClientMessage::KillPane).await?;
                }
            }
            Action::ListSessions => {
                // Request session list from server
                if let Some(framed) = &mut self.framed {
                    framed.send(ClientMessage::ListSessions).await?;

                    // Wait for response and display sessions
                    if let Some(response) = framed.next().await {
                        match response? {
                            ServerMessage::SessionList { sessions } => {
                                if sessions.is_empty() {
                                    println!("No active sessions");
                                } else {
                                    println!("Active sessions ({}):", sessions.len());
                                    for session in sessions {
                                        println!("  {} - {} ({} clients attached)",
                                            session.id.0,
                                            session.name,
                                            session.attached_clients
                                        );
                                    }
                                }
                            }
                            ServerMessage::Error { message } => {
                                error!("Failed to list sessions: {}", message);
                            }
                            _ => {
                                error!("Unexpected response to ListSessions");
                            }
                        }
                    }
                }
            }
            Action::ListWindows => {
                // Request window list from server and show selector
                if let Some(framed) = &mut self.framed {
                    framed.send(ClientMessage::ListWindows).await?;

                    // Wait for response
                    if let Some(response) = framed.next().await {
                        match response? {
                            ServerMessage::WindowList { windows } => {
                                // Convert to our WindowInfo format and show selector
                                let window_infos: Vec<WindowInfo> = windows.iter().enumerate().map(|(i, w)| {
                                    WindowInfo {
                                        id: w.id.clone(),
                                        name: w.name.clone(),
                                        index: i,
                                        active: w.is_active,
                                        pane_count: w.panes,
                                    }
                                }).collect();

                                self.window_selector.show(window_infos);
                            }
                            ServerMessage::Error { message } => {
                                warn!("Failed to get window list: {}", message);
                            }
                            _ => {
                                warn!("Unexpected response to ListWindows");
                            }
                        }
                    }
                }
            }
            Action::ReloadConfig => {
                self.reload_config().await?;
            }
            Action::SelectWindow(num) => {
                // Handle window selection (0-9)
                if let Some(framed) = &mut self.framed {
                    // Request window list
                    framed.send(ClientMessage::ListWindows).await?;

                    if let Some(response) = framed.next().await {
                        match response? {
                            ServerMessage::WindowList { windows } => {
                                // Select window by index (0-9)
                                if let Some(window) = windows.get(num as usize) {
                                    // Switch to this window
                                    framed.send(ClientMessage::SwitchWindow {
                                        window_id: window.id.clone(),
                                    }).await?;

                                    // Wait for confirmation
                                    if let Some(confirm_response) = framed.next().await {
                                        match confirm_response? {
                                            ServerMessage::WindowSwitched { .. } => {
                                                info!("Switched to window {}", num);
                                            }
                                            ServerMessage::Error { message } => {
                                                warn!("Failed to switch window: {}", message);
                                            }
                                            _ => {}
                                        }
                                    }
                                } else {
                                    info!("Window {} does not exist", num);
                                }
                            }
                            _ => {
                                warn!("Unexpected response to ListWindows");
                            }
                        }
                    }
                }
            }
            Action::ApplyLayoutPreset(preset_name) => {
                if let Some(framed) = &mut self.framed {
                    framed.send(ClientMessage::ApplyLayoutPreset {
                        preset_name: preset_name.clone()
                    }).await?;
                    info!("Applied layout preset: {}", preset_name);
                }
            }
            Action::CycleLayout => {
                if let Some(framed) = &mut self.framed {
                    framed.send(ClientMessage::CycleLayout).await?;
                    info!("Cycling through layout presets");
                }
            }
            Action::ListLayoutPresets => {
                if let Some(framed) = &mut self.framed {
                    framed.send(ClientMessage::ListLayoutPresets).await?;

                    // Wait for response and display
                    if let Some(response) = framed.next().await {
                        match response? {
                            ServerMessage::LayoutPresetsList { presets } => {
                                // Display available layout presets
                                println!("\n Available Layout Presets:");
                                println!(" ========================");
                                for preset in presets {
                                    let custom_marker = if preset.is_custom { " (custom)" } else { "" };
                                    println!(" • {:<20} - {} ({} panes){}",
                                        preset.name,
                                        preset.description,
                                        preset.pane_count,
                                        custom_marker
                                    );
                                }
                                println!("\n Press Space to cycle through layouts");
                                println!(" Use Alt+1 to Alt+5 for quick presets\n");
                            }
                            _ => {
                                warn!("Unexpected response to ListLayoutPresets");
                            }
                        }
                    }
                }
            }
            Action::ShowHelp => {
                // Toggle help overlay
                if self.help_overlay.is_visible() {
                    self.help_overlay.hide();
                } else {
                    self.help_overlay.show();
                }
                // Render with help overlay
                self.render_layout().await?;
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

    async fn handle_window_selector_key(&mut self, key_event: KeyEvent) -> Result<bool> {
        self.window_selector.update_interaction();

        match key_event.code {
            KeyCode::Esc => {
                self.window_selector.hide();
                return Ok(false);
            }
            KeyCode::Enter => {
                if let Some(window_id) = self.window_selector.get_selected() {
                    self.window_selector.hide();

                    // Switch to the selected window
                    if let Some(framed) = &mut self.framed {
                        framed.send(ClientMessage::SwitchWindow { window_id }).await?;
                    }
                }
                return Ok(false);
            }
            KeyCode::Up => {
                self.window_selector.previous();
                return Ok(false);
            }
            KeyCode::Down => {
                self.window_selector.next();
                return Ok(false);
            }
            KeyCode::Char(c) if c.is_ascii_digit() => {
                let index = c.to_digit(10)
                    .ok_or_else(|| FerrixError::Other("Invalid digit".to_string()))? as usize;
                if let Some(window_id) = self.window_selector.select_by_index(index) {
                    self.window_selector.hide();

                    // Switch to the selected window
                    if let Some(framed) = &mut self.framed {
                        framed.send(ClientMessage::SwitchWindow { window_id }).await?;
                    }
                }
                return Ok(false);
            }
            KeyCode::Backspace => {
                self.window_selector.backspace_filter();
                return Ok(false);
            }
            KeyCode::Char(c) => {
                self.window_selector.add_filter_char(c);
                return Ok(false);
            }
            _ => {}
        }

        Ok(false)
    }

    async fn handle_copy_mode_key(&mut self, key_event: KeyEvent) -> Result<bool> {
        // Handle the key locally with our CopyMode implementation
        match self.copy_mode.handle_key(key_event) {
            Ok(continue_in_copy_mode) => {
                if !continue_in_copy_mode {
                    // Exit copy mode
                    if let Some(framed) = &mut self.framed {
                        framed.send(ClientMessage::ExitCopyMode).await?;
                    }
                } else {
                    // Update the display to show current copy mode state
                    self.render_copy_mode().await?;
                }
            }
            Err(e) => {
                tracing::error!("Copy mode error: {}", e);
            }
        }

        Ok(false)
    }

    async fn render_copy_mode(&mut self) -> Result<()> {
        use crossterm::{cursor, terminal, ExecutableCommand, style::{SetForegroundColor, SetBackgroundColor, Color as CrosstermColor, ResetColor}};
        use std::io::Write;

        let mut stdout = stdout();

        // Clear screen and render copy mode UI
        stdout.execute(terminal::Clear(terminal::ClearType::All))?;
        stdout.execute(cursor::MoveTo(0, 0))?;

        // Get terminal dimensions
        let (term_width, term_height) = self.terminal_size;
        let display_height = (term_height - 3) as usize; // Leave room for status and help line

        // Set viewport height
        self.copy_mode.set_viewport_height(display_height);

        // Get buffer and viewport from copy mode
        let buffer = self.copy_mode.buffer();
        let viewport_offset = self.copy_mode.viewport_offset();
        let cursor_row = self.copy_mode.cursor_row();
        let cursor_col = self.copy_mode.cursor_col();

        // Render visible lines with line numbers
        for i in 0..display_height {
            let line_idx = viewport_offset + i;

            // Move to start of line
            stdout.execute(cursor::MoveTo(0, i as u16))?;

            if let Some(line) = buffer.get(line_idx) {
                // Line number (dim gray)
                stdout.execute(SetForegroundColor(CrosstermColor::DarkGrey))?;
                write!(stdout, "{:4} ", line_idx + 1)?;
                stdout.execute(ResetColor)?;

                // Highlight selection if in visual mode
                if self.is_line_selected(line_idx) {
                    let (start_col, end_col) = self.get_selection_range_for_line(line_idx);
                    self.render_line_with_selection(line, line_idx, start_col, end_col, &mut stdout)?;

                    // Highlight cursor at end of line if needed
                    if line_idx == cursor_row && cursor_col == line.len() {
                        stdout.execute(SetBackgroundColor(CrosstermColor::DarkGrey))?;
                        write!(stdout, " ")?;
                        stdout.execute(ResetColor)?;
                    }
                } else if self.copy_mode.selection_start().is_some() {
                    // Selection exists but not on this line - render normally
                    for (col_idx, ch) in line.chars().enumerate() {
                        if line_idx == cursor_row && col_idx == cursor_col {
                            stdout.execute(SetBackgroundColor(CrosstermColor::DarkGrey))?;
                            write!(stdout, "{}", ch)?;
                            stdout.execute(ResetColor)?;
                        } else {
                            write!(stdout, "{}", ch)?;
                        }
                    }

                    // Show cursor at end of line
                    if line_idx == cursor_row && cursor_col == line.len() {
                        stdout.execute(SetBackgroundColor(CrosstermColor::DarkGrey))?;
                        write!(stdout, " ")?;
                        stdout.execute(ResetColor)?;
                    }
                } else {
                    // Normal rendering with cursor highlight
                    for (col_idx, ch) in line.chars().enumerate() {
                        if line_idx == cursor_row && col_idx == cursor_col {
                            stdout.execute(SetBackgroundColor(CrosstermColor::DarkGrey))?;
                            write!(stdout, "{}", ch)?;
                            stdout.execute(ResetColor)?;
                        } else {
                            write!(stdout, "{}", ch)?;
                        }
                    }

                    // Show cursor at end of line
                    if line_idx == cursor_row && cursor_col == line.len() {
                        stdout.execute(SetBackgroundColor(CrosstermColor::DarkGrey))?;
                        write!(stdout, " ")?;
                        stdout.execute(ResetColor)?;
                    }
                }
            } else {
                // Empty line indicator
                stdout.execute(SetForegroundColor(CrosstermColor::DarkBlue))?;
                write!(stdout, "   ~ ")?;
                stdout.execute(ResetColor)?;
            }

            // Clear to end of line
            stdout.execute(terminal::Clear(terminal::ClearType::UntilNewLine))?;
        }

        // Status line
        stdout.execute(cursor::MoveTo(0, term_height - 2))?;
        stdout.execute(SetBackgroundColor(CrosstermColor::DarkCyan))?;
        stdout.execute(SetForegroundColor(CrosstermColor::Black))?;

        let mode_str = match self.copy_mode.state() {
            CopyModeState::Normal => "COPY",
            CopyModeState::Visual => "VISUAL",
            CopyModeState::VisualLine => "VISUAL LINE",
            CopyModeState::VisualBlock => "VISUAL BLOCK",
            CopyModeState::Search(SearchDirection::Forward) => "SEARCH /",
            CopyModeState::Search(SearchDirection::Backward) => "SEARCH ?",
        };

        // Build status line
        let status_left = format!(" -- {} MODE -- ", mode_str);
        let status_right = format!(" {}:{} ({}/{}) ",
               cursor_row + 1,
               cursor_col + 1,
               cursor_row + 1,
               buffer.len());
        let padding = " ".repeat((term_width as usize).saturating_sub(status_left.len() + status_right.len()));

        write!(stdout, "{}{}{}", status_left, padding, status_right)?;
        stdout.execute(ResetColor)?;

        // Help line
        stdout.execute(cursor::MoveTo(0, term_height - 1))?;
        stdout.execute(SetForegroundColor(CrosstermColor::DarkGrey))?;

        let help_text = match self.copy_mode.state() {
            CopyModeState::Normal => {
                " [h/j/k/l] move  [v] visual  [V] line  [y] yank line  [/] search  [q] quit"
            }
            CopyModeState::Visual |
            CopyModeState::VisualLine |
            CopyModeState::VisualBlock => {
                " [h/j/k/l] move  [y] yank  [Ctrl-c] copy  [Esc] cancel  [q] quit"
            }
            CopyModeState::Search(_) => {
                " Type to search  [Enter] find  [n/N] next/prev  [Esc] cancel"
            }
        };

        write!(stdout, "{}", help_text)?;
        stdout.execute(terminal::Clear(terminal::ClearType::UntilNewLine))?;
        stdout.execute(ResetColor)?;

        // Show cursor (for visual feedback)
        stdout.execute(cursor::Show)?;
        stdout.flush()?;
        Ok(())
    }

    async fn handle_command_mode_key(&mut self, key_event: KeyEvent) -> Result<bool> {
        match key_event.code {
            KeyCode::Esc => {
                self.command_mode.exit();
                self.render_layout().await?;
            }
            KeyCode::Enter => {
                let result = self.command_mode.execute_command();
                self.command_mode.exit();

                match result {
                    CommandResult::Message(msg) => {
                        if let Some(framed) = &mut self.framed {
                            framed.send(msg).await?;
                        }
                    }
                    CommandResult::Quit => {
                        return Ok(true); // Detach from session
                    }
                    CommandResult::Error(err) => {
                        self.command_mode.set_message(format!("Error: {}", err));
                        self.command_mode.enter(); // Stay in command mode to show error
                        self.render_command_line().await?;
                        return Ok(false);
                    }
                    CommandResult::Info(info) => {
                        self.command_mode.set_message(info);
                        self.command_mode.enter(); // Stay in command mode to show info
                        self.render_command_line().await?;
                        return Ok(false);
                    }
                    CommandResult::None => {}
                }

                self.render_layout().await?;
            }
            KeyCode::Char(c) => {
                self.command_mode.insert_char(c);
                self.render_command_line().await?;
            }
            KeyCode::Backspace => {
                self.command_mode.delete_char();
                self.render_command_line().await?;
            }
            KeyCode::Left => {
                self.command_mode.move_cursor_left();
                self.render_command_line().await?;
            }
            KeyCode::Right => {
                self.command_mode.move_cursor_right();
                self.render_command_line().await?;
            }
            KeyCode::Home => {
                self.command_mode.move_cursor_home();
                self.render_command_line().await?;
            }
            KeyCode::End => {
                self.command_mode.move_cursor_end();
                self.render_command_line().await?;
            }
            KeyCode::Up => {
                self.command_mode.history_previous();
                self.render_command_line().await?;
            }
            KeyCode::Down => {
                self.command_mode.history_next();
                self.render_command_line().await?;
            }
            _ => {}
        }

        Ok(false)
    }

    async fn render_command_line(&mut self) -> Result<()> {
        use std::io::Write;
        let mut stdout = stdout();

        // Move cursor to bottom line
        let (_, rows) = self.terminal_size;
        execute!(stdout, cursor::MoveTo(0, rows - 1))?;

        // Clear the line
        execute!(stdout, crossterm::terminal::Clear(crossterm::terminal::ClearType::CurrentLine))?;

        // Display command mode prompt and input
        let display = self.command_mode.get_display();
        write!(stdout, "{}", display)?;

        // Position cursor correctly
        let cursor_pos = self.command_mode.get_cursor_position();
        execute!(stdout, cursor::MoveTo(cursor_pos as u16, rows - 1))?;
        execute!(stdout, cursor::Show)?;

        stdout.flush()?;
        Ok(())
    }

    /// Check if a character at (row, col) is within the selection range
    /// Reserved for future character-level selection rendering
    #[allow(dead_code)]
    fn is_char_selected(&self, row: usize, col: usize, start: (usize, usize), end: (usize, usize)) -> bool {
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

        if row < start_row || row > end_row {
            return false;
        }

        if row == start_row && row == end_row {
            col >= start_col && col <= end_col
        } else if row == start_row {
            col >= start_col
        } else if row == end_row {
            col <= end_col
        } else {
            true
        }
    }

    async fn handle_output(&mut self, data: Vec<u8>) -> Result<()> {
        // Legacy single-pane output handling
        // In TUI mode, route this through pane rendering
        // In non-TUI mode, write directly to stdout

        if let Some(layout) = &self.current_layout {
            // Find the first/focused pane and route output there
            if let Some(pane) = layout.panes.first() {
                return self.handle_pane_output(pane.id.clone(), data).await;
            }
        }

        // Fallback: direct output (for non-TUI mode or no layout)
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
            .or_default()
            .extend(&data);

        // Keep buffer size reasonable based on configuration
        if let Some(buffer) = self.pane_buffers.get_mut(&pane_id) {
            let config = self.config.read().await;
            let max_buffer_size = config.general.scrollback_lines * 100; // ~100 chars per line estimate
            drop(config); // Release lock early

            if buffer.len() > max_buffer_size {
                // More efficient than drain(0..) - keeps last portion
                let keep_size = max_buffer_size / 2; // Keep half to avoid frequent resizing
                let buffer_len = buffer.len();
                buffer.copy_within(buffer_len - keep_size.., 0);
                buffer.truncate(keep_size);
            }
        }

        // Get or create ANSI parser for this pane with configured scrollback
        let config = self.config.read().await;
        let scrollback_lines = config.general.scrollback_lines;
        drop(config);

        let parser = self.pane_parsers.entry(pane_id.clone())
            .or_insert_with(|| {
                // Initialize parser with pane dimensions if available
                if let Some(layout) = &self.current_layout {
                    if let Some(pane) = layout.panes.iter().find(|p| p.id == pane_id) {
                        let width = if pane.width > 2 { pane.width - 2 } else { 80 };
                        let height = if pane.height > 2 { pane.height - 2 } else { 24 };
                        return ansi_parser::AnsiParser::new_with_scrollback(width, height, scrollback_lines);
                    }
                }
                ansi_parser::AnsiParser::new_with_scrollback(80, 24, scrollback_lines)
            });

        // Process the data through the ANSI parser
        parser.process(&data);

        // Send any pending PTY responses back to the server
        let responses = parser.take_pending_responses();
        if !responses.is_empty() {
            for response_data in responses {
                if let Some(framed) = &mut self.framed {
                    let _ = framed.send(ClientMessage::PtyResponse {
                        pane_id: pane_id.clone(),
                        data: response_data,
                    }).await;
                }
            }
        }

        // Render the updated pane content from the parsed buffer
        // This allows proper scrollback and session persistence
        if is_tty {
            // Throttle rendering to prevent flicker (max 60 FPS)
            let now = Instant::now();
            if now.duration_since(self.last_render) >= self.render_interval {
                if let Some(layout) = self.current_layout.clone() {
                    if let Some(pane_info) = layout.panes.iter().find(|p| p.id == pane_id).cloned() {
                        // Render just the updated pane content
                        self.draw_pane_content(&pane_info).await?;
                        // Re-render status bar to keep it visible
                        self.render_status_bar().await?;
                        self.last_render = now;
                        self.pending_render = false;
                    }
                }
            } else {
                // Mark that we have a pending render to do later
                self.pending_render = true;
            }
        } else {
            // In non-TTY mode, write raw output
            let mut stdout = stdout();
            stdout.write_all(&data)?;
            stdout.flush()?;
        }

        Ok(())
    }

    async fn render_layout(&mut self) -> Result<()> {
        if let Some(layout) = &self.current_layout.clone() {
            // Draw panes first
            self.draw_panes(layout).await?;

            // Then draw status bar on top
            self.render_status_bar().await?;
        }

        // Render help overlay if visible (it's an overlay over everything)
        if self.help_overlay.is_visible() {
            self.help_overlay.render_crossterm()
                .map_err(|e| FerrixError::Terminal(format!("Failed to render help: {}", e)))?;
        }

        Ok(())
    }

    async fn clear_screen(&mut self) -> Result<()> {
        execute!(stdout(), crossterm::terminal::Clear(crossterm::terminal::ClearType::All))?;
        Ok(())
    }

    async fn handle_mouse_event(&mut self, event: MouseEvent) -> Result<()> {
        // Only handle mouse events if we have a layout
        if let Some(layout) = &self.current_layout {
            // Pass the event to the mouse handler
            if let Some(action) = self.mouse_handler.handle_mouse_event(event, layout)? {
                // Handle the action
                match action {
                    MouseAction::FocusPane { .. } | MouseAction::ScrollPane { .. } => {
                        // Convert to client message and send to server
                        if let Some(msg) = action.to_client_message() {
                            if let Some(framed) = &mut self.framed {
                                framed.send(msg).await?;
                            }
                        }
                    }
                    MouseAction::UpdateSelection { start, end } => {
                        // Update visual selection (works both in and out of copy mode)
                        self.active_selection = Some((start, end));

                        if self.copy_mode.is_active() {
                            // Send update to server to reflect changes in copy mode
                            if let Some(framed) = &mut self.framed {
                                let input = format!("select:{},{} {},{}", start.0, start.1, end.0, end.1);
                                let _ = framed.send(ClientMessage::CopyModeInput { key: input }).await;
                            }
                        }

                        // Trigger a redraw to show the selection
                        // Status bar rendering disabled in passthrough mode
                        debug!("Mouse selection: {:?} to {:?}", start, end);
                    }
                    MouseAction::CompleteSelection { start, end } => {
                        // Complete selection and copy to clipboard
                        if let Some(text) = self.get_text_in_range(start, end) {
                            if !text.is_empty() {
                                // Try to copy to clipboard
                                match arboard::Clipboard::new() {
                                    Ok(mut clipboard) => {
                                        match clipboard.set_text(&text) {
                                            Ok(_) => {
                                                info!("Copied to clipboard: {} chars - '{}'", text.len(),
                                                    if text.len() > 50 { &text[..50] } else { &text });
                                            }
                                            Err(e) => {
                                                warn!("Failed to copy to clipboard: {}", e);
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        warn!("Failed to create clipboard: {}", e);
                                    }
                                }
                            } else {
                                debug!("Empty selection, not copying");
                            }
                        } else {
                            warn!("get_text_in_range returned None for selection {:?} to {:?}", start, end);
                        }

                        // Clear the visual selection
                        self.active_selection = None;

                        // Redraw disabled in passthrough mode
                    }
                    MouseAction::SelectWord { x, y } => {
                        // Select word at position
                        if let Some(word) = self.get_word_at(x, y) {
                            if !word.is_empty() {
                                match arboard::Clipboard::new() {
                                    Ok(mut clipboard) => {
                                        match clipboard.set_text(&word) {
                                            Ok(_) => info!("Selected word copied: '{}'", word),
                                            Err(e) => warn!("Failed to copy word to clipboard: {}", e),
                                        }
                                    }
                                    Err(e) => warn!("Failed to create clipboard for word: {}", e),
                                }
                            }
                        }
                    }
                    MouseAction::PasteClipboard { .. } => {
                        // Paste from clipboard
                        if let Ok(mut clipboard) = arboard::Clipboard::new() {
                            if let Ok(text) = clipboard.get_text() {
                                if let Some(framed) = &mut self.framed {
                                    framed.send(ClientMessage::Input { data: text.into_bytes() }).await?;
                                }
                            }
                        }
                    }
                    MouseAction::StartResize { pane: _, direction: _ } => {
                        // Visual feedback could be added here (cursor change, etc.)
                    }
                    MouseAction::ResizePanes { pane: _, delta_x, delta_y, direction } => {
                        use crate::ui::mouse::MouseResizeMode;
                        use crate::protocol::ResizeDirection;

                        // Only send resize commands periodically (every few pixels) to avoid flooding
                        if delta_x.abs() >= 3 || delta_y.abs() >= 3 {
                            if let Some(framed) = &mut self.framed {
                                // Send resize based on direction and delta
                                match direction {
                                    MouseResizeMode::Horizontal if delta_x > 0 => {
                                        framed.send(ClientMessage::ResizePane {
                                            direction: ResizeDirection::Right,
                                            amount: delta_x,
                                        }).await?;
                                    }
                                    MouseResizeMode::Horizontal if delta_x < 0 => {
                                        framed.send(ClientMessage::ResizePane {
                                            direction: ResizeDirection::Left,
                                            amount: -delta_x,
                                        }).await?;
                                    }
                                    MouseResizeMode::Vertical if delta_y > 0 => {
                                        framed.send(ClientMessage::ResizePane {
                                            direction: ResizeDirection::Down,
                                            amount: delta_y,
                                        }).await?;
                                    }
                                    MouseResizeMode::Vertical if delta_y < 0 => {
                                        framed.send(ClientMessage::ResizePane {
                                            direction: ResizeDirection::Up,
                                            amount: -delta_y,
                                        }).await?;
                                    }
                                    MouseResizeMode::Both => {
                                        // For corner resize, prioritize larger delta
                                        if delta_x.abs() > delta_y.abs() {
                                            if delta_x > 0 {
                                                framed.send(ClientMessage::ResizePane {
                                                    direction: ResizeDirection::Right,
                                                    amount: delta_x,
                                                }).await?;
                                            } else {
                                                framed.send(ClientMessage::ResizePane {
                                                    direction: ResizeDirection::Left,
                                                    amount: -delta_x,
                                                }).await?;
                                            }
                                        } else if delta_y > 0 {
                                            framed.send(ClientMessage::ResizePane {
                                                direction: ResizeDirection::Down,
                                                amount: delta_y,
                                            }).await?;
                                        } else {
                                            framed.send(ClientMessage::ResizePane {
                                                direction: ResizeDirection::Up,
                                                amount: -delta_y,
                                            }).await?;
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                    MouseAction::EndResize => {
                        debug!("Ended resize operation");
                        // Clean up any resize visual feedback
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }

    fn get_text_in_range(&self, start: (u16, u16), end: (u16, u16)) -> Option<String> {
        // Find the focused pane and get its parser
        let current_layout = self.current_layout.as_ref()?;
        let focused_pane = current_layout.panes.iter().find(|p| p.is_focused)?;
        let parser = self.pane_parsers.get(&focused_pane.id)?;

        let screen = parser.render();
        let mut result = String::new();

        // Convert screen coordinates to pane-relative coordinates
        let pane_count = current_layout.panes.len();
        let (content_x, content_y) = if pane_count == 1 {
            (focused_pane.x, focused_pane.y)
        } else {
            (focused_pane.x + 1, focused_pane.y + 1)
        };

        // Convert to pane-relative coordinates
        let start_x = start.0.saturating_sub(content_x);
        let start_y = start.1.saturating_sub(content_y);
        let end_x = end.0.saturating_sub(content_x);
        let end_y = end.1.saturating_sub(content_y);

        debug!("Selection screen coords: ({},{}) to ({},{})", start.0, start.1, end.0, end.1);
        debug!("Selection pane coords: ({},{}) to ({},{})", start_x, start_y, end_x, end_y);

        if start_y == end_y {
            // Single line selection
            if let Some(row) = screen.get(start_y as usize) {
                let start_col = start_x.min(end_x) as usize;
                let end_col = start_x.max(end_x) as usize;
                for cell in row.iter().skip(start_col).take(end_col - start_col + 1) {
                    result.push(cell.ch);
                }
            }
        } else {
            // Multi-line selection
            let (first_y, last_y) = if start_y < end_y {
                (start_y, end_y)
            } else {
                (end_y, start_y)
            };

            for y in first_y..=last_y {
                if let Some(row) = screen.get(y as usize) {
                    if y == first_y {
                        // First line: from start_x to end
                        for cell in row.iter().skip(start_x as usize) {
                            result.push(cell.ch);
                        }
                    } else if y == last_y {
                        // Last line: from beginning to end_x
                        for cell in row.iter().take((end_x + 1) as usize) {
                            result.push(cell.ch);
                        }
                    } else {
                        // Middle lines: entire line
                        for cell in row {
                            result.push(cell.ch);
                        }
                    }
                    if y != last_y {
                        result.push('\n');
                    }
                }
            }
        }

        Some(result.trim_end().to_string())
    }

    fn is_cell_selected(&self, cell_x: u16, cell_y: u16, sel_start: (u16, u16), sel_end: (u16, u16)) -> bool {
        // Normalize selection coordinates (ensure start is before end)
        let (start_x, start_y) = sel_start;
        let (end_x, end_y) = sel_end;

        let (first_x, first_y, last_x, last_y) = if start_y < end_y || (start_y == end_y && start_x <= end_x) {
            (start_x, start_y, end_x, end_y)
        } else {
            (end_x, end_y, start_x, start_y)
        };

        // Check if cell is within selection bounds
        if cell_y < first_y || cell_y > last_y {
            return false;
        }

        if cell_y == first_y && cell_y == last_y {
            // Single line selection
            cell_x >= first_x && cell_x <= last_x
        } else if cell_y == first_y {
            // First line of multi-line selection
            cell_x >= first_x
        } else if cell_y == last_y {
            // Last line of multi-line selection
            cell_x <= last_x
        } else {
            // Middle lines - fully selected
            true
        }
    }

    fn get_word_at(&self, x: u16, y: u16) -> Option<String> {
        // Find the focused pane and get its parser
        let current_layout = self.current_layout.as_ref()?;
        let focused_pane = current_layout.panes.iter().find(|p| p.is_focused)?;
        let parser = self.pane_parsers.get(&focused_pane.id)?;

        let screen = parser.render();
        let row = screen.get(y as usize)?;

        // Define word boundary characters
        let is_word_char = |ch: char| ch.is_alphanumeric() || ch == '_' || ch == '-' || ch == '.';

        // Check if the position has a word character
        let cell_at_pos = row.get(x as usize)?;
        if !is_word_char(cell_at_pos.ch) {
            return None;
        }

        // Find word boundaries
        let mut start_x = x as usize;
        let mut end_x = x as usize;

        // Expand left
        while start_x > 0 {
            if let Some(cell) = row.get(start_x - 1) {
                if is_word_char(cell.ch) {
                    start_x -= 1;
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        // Expand right
        while end_x < row.len() - 1 {
            if let Some(cell) = row.get(end_x + 1) {
                if is_word_char(cell.ch) {
                    end_x += 1;
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        // Extract the word
        let word: String = row.iter()
            .skip(start_x)
            .take(end_x - start_x + 1)
            .map(|cell| cell.ch)
            .collect();

        Some(word)
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

        // Status bar rendering disabled in passthrough mode

        stdout.flush()?;
        Ok(())
    }

    async fn draw_pane_content(&mut self, pane: &PaneInfo) -> Result<()> {
        use crossterm::style::{SetForegroundColor, SetBackgroundColor, SetAttribute, ResetColor};

        // Use a buffer to collect all output before flushing
        // This prevents partial updates from causing flicker
        let mut buffer = Vec::with_capacity(8192);
        let mut stdout = std::io::Cursor::new(&mut buffer);

        // Check if we're in a single-pane layout
        let pane_count = if let Some(layout) = &self.current_layout {
            layout.panes.len()
        } else {
            1
        };

        // Calculate content area
        // For single pane: use full area (no borders)
        // For multiple panes: account for borders
        let (content_x, content_y, content_width, content_height) = if pane_count == 1 {
            (pane.x, pane.y, pane.width, pane.height)
        } else {
            (
                pane.x + 1,
                pane.y + 1,
                pane.width.saturating_sub(2),
                pane.height.saturating_sub(2)
            )
        };

        if content_width == 0 || content_height == 0 {
            return Ok(());
        }

        // Update parser dimensions if needed
        if let Some(parser) = self.pane_parsers.get_mut(&pane.id) {
            parser.resize(content_width, content_height);
        }

        // Render pane content from the ANSI parser buffer
        // This allows proper scrollback and session persistence

        // Get ANSI parser for this pane or use raw buffer fallback
        if let Some(parser) = self.pane_parsers.get(&pane.id) {
            // Render using ANSI parser
            let rendered = parser.render();

            for (row_idx, row) in rendered.iter().enumerate() {
                if row_idx >= content_height as usize {
                    break;
                }

                execute!(stdout, crossterm::cursor::MoveTo(content_x, content_y + row_idx as u16))?;

                // Track previous cell state to minimize escape sequences
                let mut prev_attrs = crate::client::ansi_parser::AttributeFlags::new();
                let mut prev_fg = crossterm::style::Color::Reset;
                let mut prev_bg = crossterm::style::Color::Reset;

                for (col_idx, cell) in row.iter().enumerate().take(content_width as usize) {
                    // Check if this cell is within the active selection
                    let cell_x = content_x + col_idx as u16;
                    let cell_y = content_y + row_idx as u16;
                    let is_selected = if let Some((sel_start, sel_end)) = self.active_selection {
                        self.is_cell_selected(cell_x, cell_y, sel_start, sel_end)
                    } else {
                        false
                    };

                    // Only change attributes if they differ from previous cell
                    if cell.attributes != prev_attrs {
                        // Reset if previous cell had attributes
                        if prev_attrs != crate::client::ansi_parser::AttributeFlags::new() {
                            execute!(stdout, crossterm::style::SetAttribute(crossterm::style::Attribute::Reset))?;
                        }

                        // Apply new attributes
                        for attr in cell.attributes.to_attributes() {
                            use crossterm::style::Attribute as CrosstermAttr;
                            let crossterm_attr = match attr {
                                crossterm::style::Attribute::Bold => CrosstermAttr::Bold,
                                crossterm::style::Attribute::Dim => CrosstermAttr::Dim,
                                crossterm::style::Attribute::Italic => CrosstermAttr::Italic,
                                crossterm::style::Attribute::Underlined => CrosstermAttr::Underlined,
                                crossterm::style::Attribute::SlowBlink => CrosstermAttr::SlowBlink,
                                crossterm::style::Attribute::Reverse => CrosstermAttr::Reverse,
                                crossterm::style::Attribute::Hidden => CrosstermAttr::Hidden,
                                crossterm::style::Attribute::CrossedOut => CrosstermAttr::CrossedOut,
                                _ => continue,
                            };
                            execute!(stdout, SetAttribute(crossterm_attr))?;
                        }
                        prev_attrs = cell.attributes;
                    }

                    // Apply selection highlighting (reverse video for selected cells)
                    let (fg, bg) = if is_selected {
                        // Use reverse video for selection
                        (crossterm::style::Color::Black, crossterm::style::Color::White)
                    } else {
                        (cell.fg, cell.bg)
                    };

                    // Only change colors if they differ
                    if fg != prev_fg {
                        execute!(stdout, SetForegroundColor(fg))?;
                        prev_fg = fg;
                    }
                    if bg != prev_bg {
                        execute!(stdout, SetBackgroundColor(bg))?;
                        prev_bg = cell.bg;
                    }

                    // Write the character
                    write!(stdout, "{}", cell.ch)?;
                }

                // Reset attributes and colors at end of row
                if prev_attrs != crate::client::ansi_parser::AttributeFlags::new() {
                    execute!(stdout, crossterm::style::SetAttribute(crossterm::style::Attribute::Reset))?;
                }
                if prev_fg != crossterm::style::Color::Reset || prev_bg != crossterm::style::Color::Reset {
                    execute!(stdout, ResetColor)?;
                }
            }

            // Position cursor where the parser says it should be
            let (cursor_x, cursor_y) = parser.get_cursor_position();
            if cursor_x < content_width && cursor_y < content_height {
                execute!(stdout, crossterm::cursor::MoveTo(
                    content_x + cursor_x,
                    content_y + cursor_y
                ))?;
                // Show cursor in normal mode
                execute!(stdout, crossterm::cursor::Show)?;
            }
        } else if let Some(buffer) = self.pane_buffers.get(&pane.id) {
            // Fallback to raw buffer rendering
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
            // No content yet - just show blank pane
            // (the shell will render its own prompt when ready)
        }

        // Write all buffered output atomically to prevent flicker
        use std::io::Write as IoWrite;
        std::io::stdout().write_all(&buffer)?;
        std::io::stdout().flush()?;

        Ok(())
    }

    async fn draw_pane_border(&mut self, pane: &PaneInfo) -> Result<()> {
        use std::io::Write;
        let mut stdout = stdout();

        // Check if we're in a single-pane layout
        let pane_count = if let Some(layout) = &self.current_layout {
            layout.panes.len()
        } else {
            1
        };

        // Only draw borders if there are multiple panes
        // For single pane, skip borders for a cleaner, full-screen look
        if pane_count == 1 {
            return Ok(());
        }

        // Use thin line characters for a more minimal, trim look
        // Enhancement idea: Could differentiate focused/unfocused panes with different
        // border styles (e.g., bold/double lines for focused, thin for unfocused).
        // This would require passing focus information to the draw_pane_borders function.
        let (h_line, v_line, tl_corner, tr_corner, bl_corner, br_corner) =
            ('─', '│', '┌', '┐', '└', '┘');

        // Top border
        execute!(stdout, crossterm::cursor::MoveTo(pane.x, pane.y))?;
        write!(stdout, "{}", tl_corner)?;
        for _ in 1..pane.width-1 {
            write!(stdout, "{}", h_line)?;
        }
        if pane.width > 1 {
            write!(stdout, "{}", tr_corner)?;
        }

        // Side borders
        for y in 1..pane.height-1 {
            execute!(stdout, crossterm::cursor::MoveTo(pane.x, pane.y + y))?;
            write!(stdout, "{}", v_line)?;
            if pane.width > 1 {
                execute!(stdout, crossterm::cursor::MoveTo(pane.x + pane.width - 1, pane.y + y))?;
                write!(stdout, "{}", v_line)?;
            }
        }

        // Bottom border
        if pane.height > 1 {
            execute!(stdout, crossterm::cursor::MoveTo(pane.x, pane.y + pane.height - 1))?;
            write!(stdout, "{}", bl_corner)?;
            for _ in 1..pane.width-1 {
                write!(stdout, "{}", h_line)?;
            }
            if pane.width > 1 {
                write!(stdout, "{}", br_corner)?;
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

    pub async fn restore_snapshot(&mut self, session_id: SessionId, path: std::path::PathBuf) -> Result<()> {
        if let Some(framed) = &mut self.framed {
            framed.send(ClientMessage::RestoreSnapshot { session_id, path }).await?;

            if let Some(response) = framed.next().await {
                match response? {
                    ServerMessage::Output { .. } => Ok(()),
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

    pub async fn toggle_pane_sync(&mut self) -> Result<bool> {
        if let Some(framed) = &mut self.framed {
            framed.send(ClientMessage::TogglePaneSync).await?;

            if let Some(response) = framed.next().await {
                match response? {
                    ServerMessage::PaneSyncStatusUpdate { enabled } => Ok(enabled),
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

    pub async fn set_pane_sync(&mut self, enabled: bool) -> Result<bool> {
        if let Some(framed) = &mut self.framed {
            framed.send(ClientMessage::SetPaneSync { enabled }).await?;

            if let Some(response) = framed.next().await {
                match response? {
                    ServerMessage::PaneSyncStatusUpdate { enabled } => Ok(enabled),
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

    pub async fn lock_session(&mut self) -> Result<bool> {
        if let Some(framed) = &mut self.framed {
            framed.send(ClientMessage::LockSession).await?;

            if let Some(response) = framed.next().await {
                match response? {
                    ServerMessage::SessionLockStatusUpdate { locked } => Ok(locked),
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

    pub async fn unlock_session(&mut self) -> Result<bool> {
        if let Some(framed) = &mut self.framed {
            framed.send(ClientMessage::UnlockSession).await?;

            if let Some(response) = framed.next().await {
                match response? {
                    ServerMessage::SessionLockStatusUpdate { locked } => Ok(locked),
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

    pub async fn set_session_lock(&mut self, locked: bool) -> Result<bool> {
        if let Some(framed) = &mut self.framed {
            framed.send(ClientMessage::SetSessionLock { locked }).await?;

            if let Some(response) = framed.next().await {
                match response? {
                    ServerMessage::SessionLockStatusUpdate { locked } => Ok(locked),
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

    pub async fn toggle_zoom(&mut self) -> Result<(bool, Option<crate::protocol::PaneId>)> {
        if let Some(framed) = &mut self.framed {
            framed.send(ClientMessage::ZoomPane).await?;

            if let Some(response) = framed.next().await {
                match response? {
                    ServerMessage::PaneZoomStatusUpdate { zoomed, pane_id } => Ok((zoomed, pane_id)),
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

    pub async fn rename_window(&mut self, window_id: Option<crate::protocol::WindowId>, new_name: String) -> Result<crate::protocol::WindowId> {
        if let Some(framed) = &mut self.framed {
            framed.send(ClientMessage::RenameWindow { window_id, new_name }).await?;

            if let Some(response) = framed.next().await {
                match response? {
                    ServerMessage::WindowRenamed { window_id, .. } => Ok(window_id),
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

    async fn handle_copy_mode_entered(&mut self) -> Result<()> {
        // Legacy server-side copy mode handler
        // Now we handle copy mode locally in the client
        info!("Server notified copy mode entered");
        Ok(())
    }

    async fn handle_copy_mode_update(&mut self, _cursor_row: usize, _cursor_col: usize, _selection_start: Option<(usize, usize)>, _selection_end: Option<(usize, usize)>, _buffer_content: Vec<String>, _mode: String) -> Result<()> {
        // Legacy server-side copy mode handler
        // Now we handle copy mode locally in the client
        Ok(())
    }

    async fn handle_copy_mode_exited(&mut self) -> Result<()> {
        self.copy_mode.exit();

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


    fn show_message(&mut self, text: String, msg_type: MessageType) {
        let message = Message {
            text,
            msg_type,
            timestamp: Instant::now(),
        };
        self.messages.push_back(message);
        // Keep only last 5 messages
        while self.messages.len() > 5 {
            self.messages.pop_front();
        }
    }

    pub fn show_info(&mut self, text: String) {
        self.show_message(text, MessageType::Info);
    }

    pub fn show_success(&mut self, text: String) {
        self.show_message(text, MessageType::Success);
    }

    pub fn show_warning(&mut self, text: String) {
        self.show_message(text, MessageType::Warning);
    }

    pub fn show_error(&mut self, text: String) {
        self.show_message(text, MessageType::Error);
    }

    fn cleanup_messages(&mut self) {
        let now = Instant::now();
        self.messages.retain(|msg| {
            now.duration_since(msg.timestamp) < self.message_timeout
        });
    }

    fn get_current_message(&self) -> Option<&Message> {
        let now = Instant::now();
        // Return the most recent non-expired message
        self.messages.iter().rev().find(|msg| {
            now.duration_since(msg.timestamp) < self.message_timeout
        })
    }

    async fn render_status_bar(&mut self) -> Result<()> {
        use crossterm::{cursor::MoveTo, style::{Color, SetBackgroundColor, SetForegroundColor, ResetColor}, execute};
        use std::io::{stdout, Write};

        // Clean up expired messages
        self.cleanup_messages();

        let mut stdout = stdout();
        let (cols, rows) = self.terminal_size;

        // Render status bar at the bottom of the screen
        execute!(stdout, MoveTo(0, rows - 1))?;

        // Build status bar content
        let session_name = self.attached_session_name.clone()
            .unwrap_or_else(|| "No Session".to_string());

        let window_info = if let Some(layout) = &self.current_layout {
            format!("{}:{} P:{}", layout.window_index, layout.window_name, layout.panes.len())
        } else {
            "W:- P:-".to_string()
        };

        let time = chrono::Local::now().format("%H:%M:%S").to_string();

        // Get colors from config
        let config = self.config.read().await;
        let status_bg_color = self.parse_color(&config.colors.status_bg);
        let status_fg_color = self.parse_color(&config.colors.status_fg);
        drop(config);

        // Format status bar with padding
        let left_section = format!(" Ferrix [{}]", session_name);

        // If there's a message, show it in the center; otherwise show window info
        let (center_section, center_color) = if let Some(message) = self.get_current_message() {
            let color = match message.msg_type {
                MessageType::Info => Color::Cyan,
                MessageType::Success => Color::Green,
                MessageType::Warning => Color::Yellow,
                MessageType::Error => Color::Red,
            };
            (message.text.clone(), color)
        } else {
            (format!("[{}]", window_info), status_fg_color)
        };

        let right_section = format!("{} ", time);

        // Calculate spacing to fill the screen width
        let used_width = left_section.len() + center_section.len() + right_section.len();
        let available_width = cols as usize;

        if used_width <= available_width {
            let left_padding = (available_width - used_width) / 2;
            let right_padding = available_width - used_width - left_padding;

            // Render with color-coded center section
            execute!(stdout, SetBackgroundColor(status_bg_color), SetForegroundColor(status_fg_color))?;
            write!(stdout, "{}{}", left_section, " ".repeat(left_padding))?;
            execute!(stdout, SetForegroundColor(center_color))?;
            write!(stdout, "{}", center_section)?;
            execute!(stdout, SetForegroundColor(status_fg_color))?;
            write!(stdout, "{}{}", " ".repeat(right_padding), right_section)?;
        } else {
            // Truncate if too long
            execute!(stdout, SetBackgroundColor(status_bg_color), SetForegroundColor(status_fg_color))?;
            let truncated = format!("{}{}{}", left_section, center_section, right_section);
            let display_text = if truncated.len() > available_width {
                &truncated[..available_width]
            } else {
                &truncated
            };
            write!(stdout, "{}", display_text)?;
        }

        execute!(stdout, ResetColor)?;

        // Hide cursor after rendering status bar
        // The cursor will be shown again when rendering pane content
        execute!(stdout, crossterm::cursor::Hide)?;

        stdout.flush()?;

        Ok(())
    }

    fn parse_color(&self, color_str: &str) -> crossterm::style::Color {
        use crossterm::style::Color;

        if let Some(stripped) = color_str.strip_prefix('#') {
            // Parse hex color
            if let Ok(hex) = u32::from_str_radix(stripped, 16) {
                let r = ((hex >> 16) & 0xFF) as u8;
                let g = ((hex >> 8) & 0xFF) as u8;
                let b = (hex & 0xFF) as u8;
                return Color::Rgb { r, g, b };
            }
        }

        // Parse named colors
        match color_str.to_lowercase().as_str() {
            "black" => Color::Black,
            "red" => Color::Red,
            "green" => Color::Green,
            "darkgreen" | "dark_green" => Color::DarkGreen,
            "yellow" => Color::Yellow,
            "blue" => Color::Blue,
            "magenta" => Color::Magenta,
            "cyan" => Color::Cyan,
            "white" => Color::White,
            "gray" | "grey" => Color::Grey,
            _ => Color::Reset,
        }
    }

    fn is_line_selected(&self, line_idx: usize) -> bool {
        if let (Some(start), Some(end)) = (self.copy_mode.selection_start(), self.copy_mode.selection_end()) {
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
        if let (Some(start), Some(end)) = (self.copy_mode.selection_start(), self.copy_mode.selection_end()) {
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

    fn render_line_with_selection(&self, line: &str, _line_idx: usize, start_col: usize, end_col: usize, stdout: &mut std::io::Stdout) -> Result<()> {
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

    // Activity Monitoring Methods
    pub async fn toggle_activity_monitoring(&mut self, pane_id: Option<crate::protocol::PaneId>) -> Result<(crate::protocol::PaneId, bool)> {
        if let Some(framed) = &mut self.framed {
            framed.send(ClientMessage::ToggleActivityMonitoring { pane_id }).await?;

            if let Some(response) = framed.next().await {
                match response? {
                    ServerMessage::ActivityStatusUpdate { pane_id, enabled, .. } => Ok((pane_id, enabled)),
                    ServerMessage::Error { message } => Err(FerrixError::Other(message)),
                    _ => Err(FerrixError::Other("Unexpected server response".to_string())),
                }
            } else {
                Err(FerrixError::Other("No response from server".to_string()))
            }
        } else {
            Err(FerrixError::NotConnected)
        }
    }

    pub async fn set_activity_monitoring(&mut self, pane_id: Option<crate::protocol::PaneId>, enabled: bool) -> Result<(crate::protocol::PaneId, bool)> {
        if let Some(framed) = &mut self.framed {
            framed.send(ClientMessage::SetActivityMonitoring { pane_id, enabled }).await?;

            if let Some(response) = framed.next().await {
                match response? {
                    ServerMessage::ActivityStatusUpdate { pane_id, enabled, .. } => Ok((pane_id, enabled)),
                    ServerMessage::Error { message } => Err(FerrixError::Other(message)),
                    _ => Err(FerrixError::Other("Unexpected server response".to_string())),
                }
            } else {
                Err(FerrixError::Other("No response from server".to_string()))
            }
        } else {
            Err(FerrixError::NotConnected)
        }
    }

    // Keybinding Management Methods
    pub async fn list_keys(&mut self) -> Result<Vec<crate::protocol::KeyBindingInfo>> {
        if let Some(framed) = &mut self.framed {
            framed.send(ClientMessage::ListKeys).await?;

            if let Some(response) = framed.next().await {
                match response? {
                    ServerMessage::KeyList { bindings } => Ok(bindings),
                    ServerMessage::Error { message } => Err(FerrixError::Other(message)),
                    _ => Err(FerrixError::Other("Unexpected server response".to_string())),
                }
            } else {
                Err(FerrixError::Other("No response from server".to_string()))
            }
        } else {
            Err(FerrixError::NotConnected)
        }
    }

    pub async fn bind_key(&mut self, key: String, action: String) -> Result<()> {
        if let Some(framed) = &mut self.framed {
            framed.send(ClientMessage::BindKey { key, action }).await?;

            if let Some(response) = framed.next().await {
                match response? {
                    ServerMessage::KeyBound { .. } => Ok(()),
                    ServerMessage::Error { message } => Err(FerrixError::Other(message)),
                    _ => Err(FerrixError::Other("Unexpected server response".to_string())),
                }
            } else {
                Err(FerrixError::Other("No response from server".to_string()))
            }
        } else {
            Err(FerrixError::NotConnected)
        }
    }

    pub async fn unbind_key(&mut self, key: String) -> Result<()> {
        if let Some(framed) = &mut self.framed {
            framed.send(ClientMessage::UnbindKey { key }).await?;

            if let Some(response) = framed.next().await {
                match response? {
                    ServerMessage::KeyUnbound { .. } => Ok(()),
                    ServerMessage::Error { message } => Err(FerrixError::Other(message)),
                    _ => Err(FerrixError::Other("Unexpected server response".to_string())),
                }
            } else {
                Err(FerrixError::Other("No response from server".to_string()))
            }
        } else {
            Err(FerrixError::NotConnected)
        }
    }

    pub async fn reset_keys(&mut self) -> Result<()> {
        if let Some(framed) = &mut self.framed {
            framed.send(ClientMessage::ResetKeys).await?;

            if let Some(response) = framed.next().await {
                match response? {
                    ServerMessage::KeysReset => Ok(()),
                    ServerMessage::Error { message } => Err(FerrixError::Other(message)),
                    _ => Err(FerrixError::Other("Unexpected server response".to_string())),
                }
            } else {
                Err(FerrixError::Other("No response from server".to_string()))
            }
        } else {
            Err(FerrixError::NotConnected)
        }
    }

    pub async fn reload_keys(&mut self) -> Result<()> {
        if let Some(framed) = &mut self.framed {
            framed.send(ClientMessage::ReloadKeys).await?;

            if let Some(response) = framed.next().await {
                match response? {
                    ServerMessage::KeysReloaded => Ok(()),
                    ServerMessage::Error { message } => Err(FerrixError::Other(message)),
                    _ => Err(FerrixError::Other("Unexpected server response".to_string())),
                }
            } else {
                Err(FerrixError::Other("No response from server".to_string()))
            }
        } else {
            Err(FerrixError::NotConnected)
        }
    }

    pub async fn export_keys(&mut self, path: std::path::PathBuf) -> Result<std::path::PathBuf> {
        if let Some(framed) = &mut self.framed {
            framed.send(ClientMessage::ExportKeys { path }).await?;

            if let Some(response) = framed.next().await {
                match response? {
                    ServerMessage::KeysExported { path } => Ok(path),
                    ServerMessage::Error { message } => Err(FerrixError::Other(message)),
                    _ => Err(FerrixError::Other("Unexpected server response".to_string())),
                }
            } else {
                Err(FerrixError::Other("No response from server".to_string()))
            }
        } else {
            Err(FerrixError::NotConnected)
        }
    }

    pub async fn import_keys(&mut self, path: std::path::PathBuf) -> Result<usize> {
        if let Some(framed) = &mut self.framed {
            framed.send(ClientMessage::ImportKeys { path }).await?;

            if let Some(response) = framed.next().await {
                match response? {
                    ServerMessage::KeysImported { count } => Ok(count),
                    ServerMessage::Error { message } => Err(FerrixError::Other(message)),
                    _ => Err(FerrixError::Other("Unexpected server response".to_string())),
                }
            } else {
                Err(FerrixError::Other("No response from server".to_string()))
            }
        } else {
            Err(FerrixError::NotConnected)
        }
    }

    pub async fn send_keys(&mut self, data: Vec<u8>) -> Result<()> {
        if let Some(framed) = &mut self.framed {
            framed.send(ClientMessage::Input { data }).await?;
            Ok(())
        } else {
            Err(FerrixError::NotConnected)
        }
    }

    // Auto-Save Methods
    pub async fn enable_auto_save(&mut self, session_id: Option<crate::protocol::SessionId>, interval_minutes: Option<u64>) -> Result<u64> {
        if let Some(framed) = &mut self.framed {
            framed.send(ClientMessage::EnableAutoSave { session_id, interval_minutes }).await?;

            if let Some(response) = framed.next().await {
                match response? {
                    ServerMessage::AutoSaveEnabled { interval_minutes } => Ok(interval_minutes),
                    ServerMessage::Error { message } => Err(FerrixError::Other(message)),
                    _ => Err(FerrixError::Other("Unexpected server response".to_string())),
                }
            } else {
                Err(FerrixError::Other("No response from server".to_string()))
            }
        } else {
            Err(FerrixError::NotConnected)
        }
    }

    pub async fn disable_auto_save(&mut self, session_id: Option<crate::protocol::SessionId>) -> Result<()> {
        if let Some(framed) = &mut self.framed {
            framed.send(ClientMessage::DisableAutoSave { session_id }).await?;

            if let Some(response) = framed.next().await {
                match response? {
                    ServerMessage::AutoSaveDisabled => Ok(()),
                    ServerMessage::Error { message } => Err(FerrixError::Other(message)),
                    _ => Err(FerrixError::Other("Unexpected server response".to_string())),
                }
            } else {
                Err(FerrixError::Other("No response from server".to_string()))
            }
        } else {
            Err(FerrixError::NotConnected)
        }
    }

    pub async fn auto_save_status(&mut self, session_id: Option<crate::protocol::SessionId>) -> Result<(bool, u64, Option<chrono::DateTime<chrono::Utc>>, Option<chrono::DateTime<chrono::Utc>>)> {
        if let Some(framed) = &mut self.framed {
            framed.send(ClientMessage::AutoSaveStatus { session_id }).await?;

            if let Some(response) = framed.next().await {
                match response? {
                    ServerMessage::AutoSaveStatusInfo { enabled, interval_minutes, last_save, next_save } => {
                        Ok((enabled, interval_minutes, last_save, next_save))
                    }
                    ServerMessage::Error { message } => Err(FerrixError::Other(message)),
                    _ => Err(FerrixError::Other("Unexpected server response".to_string())),
                }
            } else {
                Err(FerrixError::Other("No response from server".to_string()))
            }
        } else {
            Err(FerrixError::NotConnected)
        }
    }

}