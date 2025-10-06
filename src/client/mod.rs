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
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
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
    pane_parsers: HashMap<PaneId, ansi_parser::AnsiParser>, // ANSI parser per pane
    copy_mode: CopyMode,
    command_mode: CommandMode,
    mouse_handler: MouseHandler,
    window_selector: WindowSelector,
    config: Arc<RwLock<Config>>,
    key_binding_manager: Arc<RwLock<KeyBindingManager>>,
    prefix_mode: bool, // Track if we're waiting for the second key after prefix
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
            framed: None,
            current_layout: None,
            terminal_size: (80, 24),
            pane_buffers: HashMap::new(),
            pane_parsers: HashMap::new(),
            copy_mode: CopyMode::new(copy_mode_style),
            command_mode: CommandMode::new(),
            mouse_handler: MouseHandler::new(mouse_enabled),
            window_selector: WindowSelector::new(),
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
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound || e.kind() == std::io::ErrorKind::ConnectionRefused {
                    FerrixError::Ipc(format!(
                        "Failed to connect to server at {:?}: {}. Is the server running? Try: ferrix server",
                        self.socket_path, e
                    ))
                } else {
                    FerrixError::Ipc(format!("Failed to connect to server: {}", e))
                }
            })?;

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

                        // Layout will be sent automatically by server
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
                return Ok(msg?);
            }
        }
        Err(FerrixError::NotConnected)
    }

    async fn run_attached(&mut self) -> Result<()> {
        // Only enable raw mode if we're in an interactive terminal
        let is_tty = std::io::stdin().is_terminal();

        if is_tty {
            terminal::enable_raw_mode()?;
            execute!(stdout(), EnterAlternateScreen, cursor::Hide)?;

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
            // Disable mouse capture if it was enabled
            if self.mouse_handler.enabled {
                execute!(stdout(), DisableMouseCapture)?;
            }
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
            KeyCode::Char(c) if c.is_digit(10) => {
                let index = c.to_digit(10).unwrap() as usize;
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
                if let (Some(start), Some(end)) = (self.copy_mode.selection_start(), self.copy_mode.selection_end()) {
                    // Render line with selection highlighting
                    for (col_idx, ch) in line.chars().enumerate() {
                        let is_selected = self.is_char_selected(line_idx, col_idx, start, end);
                        let is_cursor = line_idx == cursor_row && col_idx == cursor_col;

                        if is_selected {
                            stdout.execute(SetBackgroundColor(CrosstermColor::DarkBlue))?;
                            stdout.execute(SetForegroundColor(CrosstermColor::White))?;
                        }
                        if is_cursor && !is_selected {
                            stdout.execute(SetBackgroundColor(CrosstermColor::DarkGrey))?;
                        }

                        write!(stdout, "{}", ch)?;

                        if is_selected || is_cursor {
                            stdout.execute(ResetColor)?;
                        }
                    }

                    // Highlight cursor at end of line if needed
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
            .or_insert_with(Vec::new)
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

        // In non-TTY mode, always write output to stdout
        if !is_tty {
            let mut stdout = stdout();
            stdout.write_all(&data)?;
            stdout.flush()?;
            return Ok(());
        }

        // In TTY mode with TUI, render only the updated pane to avoid flicker
        // This ensures ANSI codes are properly parsed and displayed within pane borders
        if let Some(layout) = self.current_layout.clone() {
            // Find the pane that was updated and render just that pane
            if let Some(pane_info) = layout.panes.iter().find(|p| p.id == pane_id).cloned() {
                self.draw_pane_border(&pane_info).await?;
                self.draw_pane_content(&pane_info).await?;
                self.render_status_bar().await?;
                use std::io::Write;
                std::io::stdout().flush()?;
            }
        }

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
                        // Update visual selection in copy mode
                        if self.copy_mode.is_active() {
                            // Send update to server to reflect changes in copy mode
                            if let Some(framed) = &mut self.framed {
                                let input = format!("select:{},{} {},{}", start.0, start.1, end.0, end.1);
                                let _ = framed.send(ClientMessage::CopyModeInput { key: input }).await;
                            }

                            debug!("Mouse selection: {:?} to {:?}", start, end);
                        }
                    }
                    MouseAction::CompleteSelection { start, end } => {
                        // Complete selection and copy to clipboard
                        if let Some(text) = self.get_text_in_range(start, end) {
                            // Try to copy to clipboard
                            if let Ok(mut clipboard) = arboard::Clipboard::new() {
                                let _ = clipboard.set_text(&text);
                                debug!("Copied to clipboard: {} chars", text.len());
                            }
                        }
                    }
                    MouseAction::SelectWord { x, y } => {
                        // Select word at position
                        if let Some(word) = self.get_word_at(x, y) {
                            if let Ok(mut clipboard) = arboard::Clipboard::new() {
                                let _ = clipboard.set_text(&word);
                                debug!("Selected word: {}", word);
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

        // Normalize coordinates (ensure start <= end)
        let (start_x, start_y) = start;
        let (end_x, end_y) = end;

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

        // Draw status bar
        self.render_status_bar().await?;

        stdout.flush()?;
        Ok(())
    }

    async fn draw_pane_content(&mut self, pane: &PaneInfo) -> Result<()> {
        use std::io::Write;
        use crossterm::style::{SetForegroundColor, SetBackgroundColor, SetAttribute, ResetColor};
        let mut stdout = stdout();

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
                if pane.width > 2 { pane.width - 2 } else { 0 },
                if pane.height > 2 { pane.height - 2 } else { 0 }
            )
        };

        if content_width == 0 || content_height == 0 {
            return Ok(());
        }

        // Update parser dimensions if needed
        if let Some(parser) = self.pane_parsers.get_mut(&pane.id) {
            parser.resize(content_width, content_height);
        }

        // Clear the pane content area to prevent ghosting
        for row in 0..content_height {
            execute!(stdout, crossterm::cursor::MoveTo(content_x, content_y + row))?;
            write!(stdout, "{}", " ".repeat(content_width as usize))?;
        }

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

                for cell in row.iter().take(content_width as usize) {
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

                    // Only change colors if they differ
                    if cell.fg != prev_fg {
                        execute!(stdout, SetForegroundColor(cell.fg))?;
                        prev_fg = cell.fg;
                    }
                    if cell.bg != prev_bg {
                        execute!(stdout, SetBackgroundColor(cell.bg))?;
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
        let (h_line, v_line, tl_corner, tr_corner, bl_corner, br_corner) = if pane.is_focused {
            ('─', '│', '┌', '┐', '└', '┘')  // Focused: normal weight
        } else {
            ('─', '│', '┌', '┐', '└', '┘')  // Unfocused: same for now, could use lighter chars
        };

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
                                // Route through handle_output which handles TUI mode properly
                                self.handle_output(data).await?;
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

        // Hide cursor after rendering status bar
        // The cursor will be shown again when rendering pane content
        execute!(stdout, crossterm::cursor::Hide)?;

        stdout.flush()?;

        Ok(())
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