use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct SessionId(pub Uuid);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct ClientId(pub Uuid);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct WindowId(pub Uuid);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct PaneId(pub Uuid);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClientMessage {
    CreateSession {
        name: Option<String>,
    },
    AttachSession {
        session_id: SessionId,
    },
    DetachSession,
    ListSessions,
    KillSession {
        session_id: SessionId,
    },
    Input {
        data: Vec<u8>,
    },
    Resize {
        cols: u16,
        rows: u16,
    },
    CreateWindow {
        name: Option<String>,
    },
    SwitchWindow {
        window_id: WindowId,
    },
    CloseWindow {
        window_id: WindowId,
    },
    SplitPane {
        direction: SplitDirection,
    },
    SwitchPane {
        pane_id: PaneId,
    },
    NavigatePane {
        direction: PaneNavigationDirection,
    },
    ClosePane {
        pane_id: PaneId,
    },
    ResizePane {
        direction: ResizeDirection,
        amount: i16,
    },
    Ping,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerMessage {
    SessionCreated {
        session_id: SessionId,
        name: String,
    },
    SessionAttached {
        session_id: SessionId,
    },
    SessionDetached,
    SessionList {
        sessions: Vec<SessionInfo>,
    },
    SessionKilled {
        session_id: SessionId,
    },
    Output {
        data: Vec<u8>,
    },
    WindowCreated {
        window_id: WindowId,
        name: String,
    },
    WindowSwitched {
        window_id: WindowId,
    },
    WindowClosed {
        window_id: WindowId,
    },
    PaneCreated {
        pane_id: PaneId,
    },
    PaneSwitched {
        pane_id: PaneId,
    },
    PaneClosed {
        pane_id: PaneId,
    },
    Error {
        message: String,
    },
    Pong,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: SessionId,
    pub name: String,
    pub attached_clients: usize,
    pub windows: usize,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum SplitDirection {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum PaneNavigationDirection {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ResizeDirection {
    Up,
    Down,
    Left,
    Right,
}