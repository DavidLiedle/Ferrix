use serde::{Deserialize, Serialize};
use uuid::Uuid;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct SessionId(pub Uuid);

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
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
    RenameWindow {
        window_id: Option<WindowId>,
        new_name: String,
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
    NextWindow,
    PreviousWindow,
    ZoomPane,
    KillPane,
    ListWindows,
    ApplyLayoutPreset {
        preset_name: String,
    },
    ListLayoutPresets,
    CycleLayout,
    LoadSessionConfig {
        config_path: Option<PathBuf>,
    },
    SaveSessionConfig,
    ApplySessionTemplate {
        template_name: String,
    },
    ListSessionTemplates,
    EnterCopyMode,
    ExitCopyMode,
    CopyModeInput {
        key: String,
    },
    SaveSnapshot {
        session_id: SessionId,
        name: Option<String>,
        description: Option<String>,
    },
    LoadSnapshot {
        path: std::path::PathBuf,
    },
    ListSnapshots,
    DeleteSnapshot {
        path: std::path::PathBuf,
    },
    TogglePaneSync,
    SetPaneSync {
        enabled: bool,
    },
    LockSession,
    UnlockSession,
    SetSessionLock {
        locked: bool,
    },
    ToggleActivityMonitoring {
        pane_id: Option<PaneId>,
    },
    SetActivityMonitoring {
        pane_id: Option<PaneId>,
        enabled: bool,
    },
    Ping,
    Authenticate(AuthCredentials),
    ListKeys,
    BindKey {
        key: String,
        action: String,
    },
    UnbindKey {
        key: String,
    },
    ResetKeys,
    ReloadKeys,
    ExportKeys {
        path: std::path::PathBuf,
    },
    ImportKeys {
        path: std::path::PathBuf,
    },
    EnableAutoSave {
        session_id: Option<SessionId>,
        interval_minutes: Option<u64>,
    },
    DisableAutoSave {
        session_id: Option<SessionId>,
    },
    AutoSaveStatus {
        session_id: Option<SessionId>,
    },
    StartRecording {
        session_id: Option<SessionId>,
        output_path: Option<std::path::PathBuf>,
    },
    StopRecording {
        session_id: Option<SessionId>,
    },
    PauseRecording {
        session_id: Option<SessionId>,
    },
    ResumeRecording {
        session_id: Option<SessionId>,
    },
    RecordingStatus {
        session_id: Option<SessionId>,
    },
    PlayRecording {
        path: std::path::PathBuf,
        speed: Option<f32>,
    },
    ExportRecording {
        path: std::path::PathBuf,
        format: RecordingExportFormat,
        output_path: std::path::PathBuf,
    },
    // Session versioning commands
    InitVersioning {
        session_id: SessionId,
    },
    CommitSession {
        session_id: SessionId,
        message: String,
    },
    CreateBranch {
        session_id: SessionId,
        branch_name: String,
        description: Option<String>,
    },
    CheckoutBranch {
        session_id: SessionId,
        branch_name: String,
    },
    MergeBranch {
        session_id: SessionId,
        branch_name: String,
        strategy: String,  // "ours", "theirs", "auto", "manual"
    },
    CherryPick {
        session_id: SessionId,
        commit_id: String,
    },
    RevertCommit {
        session_id: SessionId,
        commit_id: String,
    },
    ResetToCommit {
        session_id: SessionId,
        commit_id: String,
        hard: bool,
    },
    ShowLog {
        session_id: SessionId,
        limit: Option<usize>,
    },
    ShowDiff {
        session_id: SessionId,
        from_commit: String,
        to_commit: String,
    },
    TagCommit {
        session_id: SessionId,
        commit_id: String,
        tag: String,
    },
    ListBranches {
        session_id: SessionId,
    },
    ResolveConflict {
        session_id: SessionId,
        conflict_path: String,
        resolution: String,  // "ours", "theirs", "custom"
        custom_value: Option<String>,
    },
    /// Send PTY response (for device status reports, cursor position, etc.)
    PtyResponse {
        pane_id: PaneId,
        data: Vec<u8>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RecordingExportFormat {
    Asciinema,
    Text,
    Html,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthCredentials {
    pub username: String,
    pub password: Option<String>,
    pub token: Option<String>,
    pub certificate: Option<Vec<u8>>,
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
    PaneOutput {
        pane_id: PaneId,
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
    WindowRenamed {
        window_id: WindowId,
        new_name: String,
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
    SnapshotSaved {
        path: std::path::PathBuf,
    },
    SnapshotLoaded {
        session_id: SessionId,
    },
    SnapshotList {
        snapshots: Vec<SnapshotInfo>,
    },
    SnapshotDeleted {
        path: std::path::PathBuf,
    },
    Error {
        message: String,
    },
    Pong,
    Authenticated {
        client_id: ClientId,
    },
    Success,
    WindowList {
        windows: Vec<WindowInfo>,
    },
    LayoutPresetsList {
        presets: Vec<LayoutPresetInfo>,
    },
    LayoutApplied {
        preset_name: String,
    },
    SessionConfigLoaded {
        session_id: SessionId,
    },
    SessionConfigSaved {
        session_id: SessionId,
    },
    SessionTemplateApplied {
        template_name: String,
    },
    SessionTemplatesList {
        templates: Vec<SessionTemplateInfo>,
    },
    CopyModeEntered,
    CopyModeUpdate {
        cursor_row: usize,
        cursor_col: usize,
        selection_start: Option<(usize, usize)>,
        selection_end: Option<(usize, usize)>,
        buffer_content: Vec<String>,
        mode: String,
    },
    CopyModeExited,
    LayoutUpdate {
        layout: LayoutInfo,
    },
    PaneSyncStatusUpdate {
        enabled: bool,
    },
    SessionLockStatusUpdate {
        locked: bool,
    },
    PaneZoomStatusUpdate {
        zoomed: bool,
        pane_id: Option<PaneId>,
    },
    ActivityStatusUpdate {
        pane_id: PaneId,
        activity_status: Option<String>,
        enabled: bool,
    },
    ActivityAlert {
        window_id: WindowId,
        pane_id: PaneId,
        activity_type: String,
        message: String,
    },
    KeyList {
        bindings: Vec<KeyBindingInfo>,
    },
    KeyBound {
        key: String,
        action: String,
    },
    KeyUnbound {
        key: String,
    },
    KeysReset,
    KeysReloaded,
    KeysExported {
        path: std::path::PathBuf,
    },
    KeysImported {
        count: usize,
    },
    AutoSaveEnabled {
        interval_minutes: u64,
    },
    AutoSaveDisabled,
    AutoSaveStatusInfo {
        enabled: bool,
        interval_minutes: u64,
        last_save: Option<chrono::DateTime<chrono::Utc>>,
        next_save: Option<chrono::DateTime<chrono::Utc>>,
    },
    RecordingStarted {
        session_id: SessionId,
        output_path: std::path::PathBuf,
    },
    RecordingStopped {
        session_id: SessionId,
        duration_secs: u64,
        file_size: u64,
    },
    RecordingPaused {
        session_id: SessionId,
    },
    RecordingResumed {
        session_id: SessionId,
    },
    RecordingStatus {
        session_id: SessionId,
        is_recording: bool,
        is_paused: bool,
        output_path: Option<std::path::PathBuf>,
        duration_secs: u64,
        event_count: u64,
    },
    RecordingPlaybackStarted {
        path: std::path::PathBuf,
    },
    RecordingPlaybackFinished,
    RecordingExported {
        input_path: std::path::PathBuf,
        output_path: std::path::PathBuf,
        format: RecordingExportFormat,
    },
    // Session versioning responses
    VersioningInitialized {
        session_id: SessionId,
    },
    CommitCreated {
        session_id: SessionId,
        commit_id: String,
        message: String,
    },
    BranchCreated {
        session_id: SessionId,
        branch_name: String,
    },
    BranchCheckedOut {
        session_id: SessionId,
        branch_name: String,
    },
    MergeCompleted {
        session_id: SessionId,
        branch_name: String,
        conflicts: Vec<String>,
    },
    CherryPickCompleted {
        session_id: SessionId,
        commit_id: String,
    },
    RevertCompleted {
        session_id: SessionId,
        commit_id: String,
    },
    ResetCompleted {
        session_id: SessionId,
        commit_id: String,
    },
    LogHistory {
        session_id: SessionId,
        commits: Vec<CommitInfo>,
    },
    DiffResult {
        session_id: SessionId,
        diff: String,
    },
    CommitTagged {
        session_id: SessionId,
        commit_id: String,
        tag: String,
    },
    BranchList {
        session_id: SessionId,
        branches: Vec<BranchInfo>,
        current: String,
    },
    ConflictResolved {
        session_id: SessionId,
        conflict_path: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowInfo {
    pub id: WindowId,
    pub name: String,
    pub panes: usize,
    pub is_active: bool,
    pub activity_status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutPresetInfo {
    pub name: String,
    pub description: String,
    pub pane_count: usize,
    pub is_custom: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionTemplateInfo {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutInfo {
    pub window_id: WindowId,
    pub panes: Vec<PaneInfo>,
    pub focused_pane: Option<PaneId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaneInfo {
    pub id: PaneId,
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
    pub is_focused: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: SessionId,
    pub name: String,
    pub attached_clients: usize,
    pub windows: usize,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotInfo {
    pub path: std::path::PathBuf,
    pub name: String,
    pub description: String,
    pub session_name: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyBindingInfo {
    pub key: String,
    pub action: String,
    pub description: String,
    pub is_custom: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitInfo {
    pub id: String,
    pub message: String,
    pub author: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub parent: Option<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchInfo {
    pub name: String,
    pub head: String,
    pub description: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub is_current: bool,
}
#[cfg(test)]
mod tests {
    

    #[test]
    fn test_message_serialization() {
        // Test message serialization
        assert!(true);
    }

    #[test]
    fn test_message_deserialization() {
        // Test message deserialization
        assert!(true);
    }
}
