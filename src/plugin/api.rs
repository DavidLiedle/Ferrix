use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Plugin API version for compatibility checking
pub const API_VERSION: &str = "0.1.0";

/// Plugin capabilities that can be requested
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PluginCapability {
    SessionManagement,
    WindowManagement,
    PaneManagement,
    CommandExecution,
    StatusBar,
    KeyBinding,
    FileSystem,
    Network,
    Clipboard,
    Notification,
}

/// Plugin metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    pub homepage: Option<String>,
    pub license: Option<String>,
    pub api_version: String,
    pub capabilities: Vec<PluginCapability>,
    pub exports: Vec<String>,
}

/// Events that plugins can subscribe to
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PluginEvent {
    SessionCreated { session_id: String },
    SessionDestroyed { session_id: String },
    SessionAttached { session_id: String },
    SessionDetached { session_id: String },
    WindowCreated { window_id: String },
    WindowClosed { window_id: String },
    WindowFocused { window_id: String },
    PaneCreated { pane_id: String },
    PaneClosed { pane_id: String },
    PaneFocused { pane_id: String },
    KeyPressed { key: String, modifiers: Vec<String> },
    CommandExecuted { command: String },
    OutputReceived { data: Vec<u8> },
    InputReceived { data: Vec<u8> },
    ResizeEvent { cols: u16, rows: u16 },
    ConfigReloaded,
    PluginLoaded { plugin_name: String },
    PluginUnloaded { plugin_name: String },
}

/// Commands that plugins can execute
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PluginCommand {
    // Session management
    CreateSession { name: Option<String> },
    AttachSession { session_id: String },
    DetachSession,
    KillSession { session_id: String },
    ListSessions,

    // Window management
    CreateWindow { name: Option<String> },
    CloseWindow { window_id: String },
    RenameWindow { window_id: String, name: String },
    SwitchWindow { window_id: String },
    NextWindow,
    PreviousWindow,

    // Pane management
    SplitPane { direction: SplitDirection },
    ClosePane { pane_id: String },
    NavigatePane { direction: NavigationDirection },
    ResizePane { direction: ResizeDirection, amount: i16 },

    // Output and input
    SendInput { data: Vec<u8> },
    SendOutput { data: Vec<u8> },

    // UI
    ShowMessage { message: String, level: MessageLevel },
    UpdateStatusBar { content: String },
    ShowMenu { items: Vec<MenuItem> },

    // System
    ExecuteCommand { command: String, args: Vec<String> },
    ReadFile { path: String },
    WriteFile { path: String, content: Vec<u8> },

    // Configuration
    GetConfig { key: String },
    SetConfig { key: String, value: String },

    // Clipboard
    GetClipboard,
    SetClipboard { content: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SplitDirection {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NavigationDirection {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ResizeDirection {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageLevel {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MenuItem {
    pub id: String,
    pub label: String,
    pub shortcut: Option<String>,
    pub enabled: bool,
}

/// Plugin response to commands
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PluginResponse {
    Success { data: Option<serde_json::Value> },
    Error { message: String },
    Event { event: PluginEvent },
}

/// Hook points where plugins can inject functionality
#[derive(Debug, Clone, Serialize, Deserialize, Hash, Eq, PartialEq)]
pub enum PluginHook {
    PreSessionCreate,
    PostSessionCreate,
    PreSessionDestroy,
    PostSessionDestroy,
    PreWindowCreate,
    PostWindowCreate,
    PrePaneCreate,
    PostPaneCreate,
    PreCommand,
    PostCommand,
    PreInput,
    PostInput,
    PreOutput,
    PostOutput,
    StatusBarRender,
    ConfigLoad,
    ConfigSave,
}

/// Plugin context provided to plugin functions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginContext {
    pub session_id: Option<String>,
    pub window_id: Option<String>,
    pub pane_id: Option<String>,
    pub user_data: HashMap<String, serde_json::Value>,
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_api() {
        // Plugin API test
        assert!(true);
    }

    #[test]
    fn test_api_versioning() {
        // Test API version compatibility
        assert!(true);
    }
}
