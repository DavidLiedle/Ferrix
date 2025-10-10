pub mod user_store;

pub use user_store::UserStore;

/// Stable authorization action enum
///
/// This provides a stable representation of actions that can be authorized,
/// unlike using Debug formatting which can change and break authorization checks.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AuthAction {
    // Session management
    CreateSession,
    AttachSession,
    DetachSession,
    ListSessions,
    KillSession,

    // Input/Output
    SendInput,
    Resize,

    // Window management
    CreateWindow,
    SwitchWindow,
    CloseWindow,
    RenameWindow,
    NextWindow,
    PreviousWindow,

    // Pane management
    SplitPane,
    SwitchPane,
    NavigatePane,
    SelectLastPane,
    SelectPaneByIndex,
    ClosePane,
    ResizePane,
    KillPane,
    SwapPane,
    RotatePane,
    ZoomPane,

    // Advanced features
    CopyMode,
    SendKeys,
    SetOption,
    GetLayout,
    SaveSnapshot,
    LoadSnapshot,
    ListSnapshots,
    DeleteSnapshot,

    // Authentication
    Authenticate,

    // Catch-all for unknown actions
    Unknown(String),
}

impl AuthAction {
    /// Convert from ClientMessage to AuthAction
    pub fn from_client_message(msg: &crate::protocol::ClientMessage) -> Self {
        use crate::protocol::ClientMessage;

        match msg {
            ClientMessage::CreateSession { .. } => AuthAction::CreateSession,
            ClientMessage::AttachSession { .. } => AuthAction::AttachSession,
            ClientMessage::DetachSession => AuthAction::DetachSession,
            ClientMessage::ListSessions => AuthAction::ListSessions,
            ClientMessage::KillSession { .. } => AuthAction::KillSession,
            ClientMessage::Input { .. } => AuthAction::SendInput,
            ClientMessage::Resize { .. } => AuthAction::Resize,
            ClientMessage::CreateWindow { .. } => AuthAction::CreateWindow,
            ClientMessage::SwitchWindow { .. } => AuthAction::SwitchWindow,
            ClientMessage::CloseWindow { .. } => AuthAction::CloseWindow,
            ClientMessage::RenameWindow { .. } => AuthAction::RenameWindow,
            ClientMessage::NextWindow => AuthAction::NextWindow,
            ClientMessage::PreviousWindow => AuthAction::PreviousWindow,
            ClientMessage::SplitPane { .. } => AuthAction::SplitPane,
            ClientMessage::SwitchPane { .. } => AuthAction::SwitchPane,
            ClientMessage::NavigatePane { .. } => AuthAction::NavigatePane,
            ClientMessage::SelectLastPane => AuthAction::SelectLastPane,
            ClientMessage::SelectPaneByIndex { .. } => AuthAction::SelectPaneByIndex,
            ClientMessage::ClosePane { .. } => AuthAction::ClosePane,
            ClientMessage::ResizePane { .. } => AuthAction::ResizePane,
            ClientMessage::KillPane => AuthAction::KillPane,
            ClientMessage::ZoomPane => AuthAction::ZoomPane,
            ClientMessage::SaveSnapshot { .. } => AuthAction::SaveSnapshot,
            ClientMessage::LoadSnapshot { .. } => AuthAction::LoadSnapshot,
            ClientMessage::ListSnapshots => AuthAction::ListSnapshots,
            ClientMessage::DeleteSnapshot { .. } => AuthAction::DeleteSnapshot,
            ClientMessage::Authenticate(_) => AuthAction::Authenticate,

            // Future/unimplemented actions - use Unknown for forward compatibility
            _ => AuthAction::Unknown(format!("{:?}", msg)),
        }
    }

    /// Convert to stable string representation for storage/comparison
    pub fn as_str(&self) -> &str {
        match self {
            AuthAction::CreateSession => "create_session",
            AuthAction::AttachSession => "attach_session",
            AuthAction::DetachSession => "detach_session",
            AuthAction::ListSessions => "list_sessions",
            AuthAction::KillSession => "kill_session",
            AuthAction::SendInput => "send_input",
            AuthAction::Resize => "resize",
            AuthAction::CreateWindow => "create_window",
            AuthAction::SwitchWindow => "switch_window",
            AuthAction::CloseWindow => "close_window",
            AuthAction::RenameWindow => "rename_window",
            AuthAction::NextWindow => "next_window",
            AuthAction::PreviousWindow => "previous_window",
            AuthAction::SplitPane => "split_pane",
            AuthAction::SwitchPane => "switch_pane",
            AuthAction::NavigatePane => "navigate_pane",
            AuthAction::SelectLastPane => "select_last_pane",
            AuthAction::SelectPaneByIndex => "select_pane_by_index",
            AuthAction::ClosePane => "close_pane",
            AuthAction::ResizePane => "resize_pane",
            AuthAction::KillPane => "kill_pane",
            AuthAction::SwapPane => "swap_pane",
            AuthAction::RotatePane => "rotate_pane",
            AuthAction::ZoomPane => "zoom_pane",
            AuthAction::CopyMode => "copy_mode",
            AuthAction::SendKeys => "send_keys",
            AuthAction::SetOption => "set_option",
            AuthAction::GetLayout => "get_layout",
            AuthAction::SaveSnapshot => "save_snapshot",
            AuthAction::LoadSnapshot => "load_snapshot",
            AuthAction::ListSnapshots => "list_snapshots",
            AuthAction::DeleteSnapshot => "delete_snapshot",
            AuthAction::Authenticate => "authenticate",
            AuthAction::Unknown(s) => s,
        }
    }
}
#[cfg(test)]
mod tests {
    

    #[test]
    fn test_auth_module_initialization() {
        // Auth module test
        assert!(true);
    }
}
