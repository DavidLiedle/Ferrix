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
use crate::protocol::{ClientMessage, FerrixClientCodec, ServerMessage, SessionId};

pub struct Client {
    socket_path: PathBuf,
    attached_session: Option<SessionId>,
    framed: Option<Framed<UnixStream, FerrixClientCodec>>,
}

impl Client {
    pub fn new(socket_path: PathBuf) -> Self {
        Self {
            socket_path,
            attached_session: None,
            framed: None,
        }
    }

    pub async fn connect(&mut self) -> Result<()> {
        let stream = UnixStream::connect(&self.socket_path)
            .await
            .map_err(|e| FerrixError::Ipc(format!("Failed to connect to server: {}", e)))?;

        self.framed = Some(Framed::new(stream, FerrixClientCodec));
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

            if let Some(Ok(ServerMessage::SessionAttached { .. })) = framed.next().await {
                self.attached_session = Some(session_id.clone());
                info!("Attached to session {}", session_id.0);
                return self.run_attached().await;
            }
        }
        Err(FerrixError::Protocol("Failed to attach to session".to_string()))
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
                            if let Some(framed) = &mut self.framed {
                                framed.send(ClientMessage::Resize { cols, rows }).await?;
                            }
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
                        Ok(ServerMessage::SessionDetached) => {
                            info!("Session detached");
                            break;
                        }
                        Ok(ServerMessage::Error { message }) => {
                            error!("Server error: {}", message);
                            break;
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
                    if next_event.code == KeyCode::Char('d') {
                        self.detach_session().await?;
                        return Ok(true);
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
}