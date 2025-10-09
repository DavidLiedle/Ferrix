//! Hook system for triggering commands on events
//!
//! Implements tmux-style hooks that allow running commands when specific events occur.
//! Hooks enable automation and customization of Ferrix behavior.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use crate::error::Result;
use crate::protocol::{SessionId, WindowId, PaneId};

/// Hook event types
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum HookEvent {
    // Session hooks
    SessionCreated,
    SessionClosed,
    SessionRenamed,
    SessionWindowChanged,

    // Client hooks
    ClientAttached,
    ClientDetached,
    ClientResized,
    ClientSessionChanged,

    // Window hooks
    WindowCreated,
    WindowClosed,
    WindowRenamed,
    WindowLinked,
    WindowUnlinked,
    WindowPaneChanged,

    // Pane hooks
    PaneCreated,
    PaneClosed,
    PaneFocusIn,
    PaneFocusOut,
    PaneTitleChanged,
    PaneDied,
    PaneExited,
    PaneModeChanged,
    PaneSetClipboard,

    // Layout hooks
    LayoutChange,

    // Activity hooks
    AlertActivity,
    AlertBell,
    AlertSilence,

    // Command hooks (after-* for all commands)
    AfterCommand(String),
}

impl HookEvent {
    /// Get the hook name as a string (for configuration)
    pub fn name(&self) -> String {
        match self {
            // Session
            HookEvent::SessionCreated => "session-created".to_string(),
            HookEvent::SessionClosed => "session-closed".to_string(),
            HookEvent::SessionRenamed => "session-renamed".to_string(),
            HookEvent::SessionWindowChanged => "session-window-changed".to_string(),

            // Client
            HookEvent::ClientAttached => "client-attached".to_string(),
            HookEvent::ClientDetached => "client-detached".to_string(),
            HookEvent::ClientResized => "client-resized".to_string(),
            HookEvent::ClientSessionChanged => "client-session-changed".to_string(),

            // Window
            HookEvent::WindowCreated => "window-created".to_string(),
            HookEvent::WindowClosed => "window-closed".to_string(),
            HookEvent::WindowRenamed => "window-renamed".to_string(),
            HookEvent::WindowLinked => "window-linked".to_string(),
            HookEvent::WindowUnlinked => "window-unlinked".to_string(),
            HookEvent::WindowPaneChanged => "window-pane-changed".to_string(),

            // Pane
            HookEvent::PaneCreated => "pane-created".to_string(),
            HookEvent::PaneClosed => "pane-closed".to_string(),
            HookEvent::PaneFocusIn => "pane-focus-in".to_string(),
            HookEvent::PaneFocusOut => "pane-focus-out".to_string(),
            HookEvent::PaneTitleChanged => "pane-title-changed".to_string(),
            HookEvent::PaneDied => "pane-died".to_string(),
            HookEvent::PaneExited => "pane-exited".to_string(),
            HookEvent::PaneModeChanged => "pane-mode-changed".to_string(),
            HookEvent::PaneSetClipboard => "pane-set-clipboard".to_string(),

            // Layout
            HookEvent::LayoutChange => "layout-change".to_string(),

            // Activity
            HookEvent::AlertActivity => "alert-activity".to_string(),
            HookEvent::AlertBell => "alert-bell".to_string(),
            HookEvent::AlertSilence => "alert-silence".to_string(),

            // Command
            HookEvent::AfterCommand(cmd) => format!("after-{}", cmd),
        }
    }

    /// Parse a hook name string into a HookEvent
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            // Session
            "session-created" => Some(HookEvent::SessionCreated),
            "session-closed" => Some(HookEvent::SessionClosed),
            "session-renamed" => Some(HookEvent::SessionRenamed),
            "session-window-changed" => Some(HookEvent::SessionWindowChanged),

            // Client
            "client-attached" => Some(HookEvent::ClientAttached),
            "client-detached" => Some(HookEvent::ClientDetached),
            "client-resized" => Some(HookEvent::ClientResized),
            "client-session-changed" => Some(HookEvent::ClientSessionChanged),

            // Window
            "window-created" => Some(HookEvent::WindowCreated),
            "window-closed" => Some(HookEvent::WindowClosed),
            "window-renamed" => Some(HookEvent::WindowRenamed),
            "window-linked" => Some(HookEvent::WindowLinked),
            "window-unlinked" => Some(HookEvent::WindowUnlinked),
            "window-pane-changed" => Some(HookEvent::WindowPaneChanged),

            // Pane
            "pane-created" => Some(HookEvent::PaneCreated),
            "pane-closed" => Some(HookEvent::PaneClosed),
            "pane-focus-in" => Some(HookEvent::PaneFocusIn),
            "pane-focus-out" => Some(HookEvent::PaneFocusOut),
            "pane-title-changed" => Some(HookEvent::PaneTitleChanged),
            "pane-died" => Some(HookEvent::PaneDied),
            "pane-exited" => Some(HookEvent::PaneExited),
            "pane-mode-changed" => Some(HookEvent::PaneModeChanged),
            "pane-set-clipboard" => Some(HookEvent::PaneSetClipboard),

            // Layout
            "layout-change" => Some(HookEvent::LayoutChange),

            // Activity
            "alert-activity" => Some(HookEvent::AlertActivity),
            "alert-bell" => Some(HookEvent::AlertBell),
            "alert-silence" => Some(HookEvent::AlertSilence),

            // Command hooks
            name if name.starts_with("after-") => {
                name.strip_prefix("after-")
                    .map(|cmd| HookEvent::AfterCommand(cmd.to_string()))
            }

            _ => None,
        }
    }
}

/// Hook context - information available when a hook is triggered
#[derive(Debug, Clone, Default)]
pub struct HookContext {
    pub session_id: Option<SessionId>,
    pub window_id: Option<WindowId>,
    pub pane_id: Option<PaneId>,
    pub hook_name: String,
    pub extra: HashMap<String, String>,
}

impl HookContext {
    pub fn new(hook_name: String) -> Self {
        Self {
            hook_name,
            ..Default::default()
        }
    }

    pub fn with_session(mut self, session_id: SessionId) -> Self {
        self.session_id = Some(session_id);
        self
    }

    pub fn with_window(mut self, window_id: WindowId) -> Self {
        self.window_id = Some(window_id);
        self
    }

    pub fn with_pane(mut self, pane_id: PaneId) -> Self {
        self.pane_id = Some(pane_id);
        self
    }

    pub fn with_extra(mut self, key: String, value: String) -> Self {
        self.extra.insert(key, value);
        self
    }
}

/// A hook command to execute
#[derive(Debug, Clone)]
pub struct Hook {
    pub event: HookEvent,
    pub command: String,
    pub global: bool,
    pub session_id: Option<SessionId>,
}

impl Hook {
    pub fn new(event: HookEvent, command: String) -> Self {
        Self {
            event,
            command,
            global: false,
            session_id: None,
        }
    }

    pub fn global(mut self) -> Self {
        self.global = true;
        self
    }

    pub fn for_session(mut self, session_id: SessionId) -> Self {
        self.session_id = Some(session_id);
        self.global = false;
        self
    }
}

/// Hook manager - stores and triggers hooks
pub struct HookManager {
    /// Global hooks (apply to all sessions)
    global_hooks: HashMap<HookEvent, Vec<String>>,

    /// Session-specific hooks
    session_hooks: HashMap<SessionId, HashMap<HookEvent, Vec<String>>>,

    /// Track if we're currently executing a hook (prevent recursion)
    executing: bool,
}

impl HookManager {
    pub fn new() -> Self {
        Self {
            global_hooks: HashMap::new(),
            session_hooks: HashMap::new(),
            executing: false,
        }
    }

    /// Set a global hook
    pub fn set_global_hook(&mut self, event: HookEvent, command: String) {
        self.global_hooks
            .entry(event)
            .or_default()
            .push(command);
    }

    /// Set a session-specific hook
    pub fn set_session_hook(&mut self, session_id: SessionId, event: HookEvent, command: String) {
        self.session_hooks
            .entry(session_id)
            .or_default()
            .entry(event)
            .or_default()
            .push(command);
    }

    /// Remove a global hook
    pub fn unset_global_hook(&mut self, event: &HookEvent) {
        self.global_hooks.remove(event);
    }

    /// Remove a session-specific hook
    pub fn unset_session_hook(&mut self, session_id: &SessionId, event: &HookEvent) {
        if let Some(hooks) = self.session_hooks.get_mut(session_id) {
            hooks.remove(event);
        }
    }

    /// Get all hooks that should run for an event
    pub fn get_hooks(&self, event: &HookEvent, session_id: Option<&SessionId>) -> Vec<String> {
        let mut commands = Vec::new();

        // Add global hooks
        if let Some(global_cmds) = self.global_hooks.get(event) {
            commands.extend(global_cmds.clone());
        }

        // Add session-specific hooks
        if let Some(sid) = session_id {
            if let Some(session_hooks) = self.session_hooks.get(sid) {
                if let Some(session_cmds) = session_hooks.get(event) {
                    commands.extend(session_cmds.clone());
                }
            }
        }

        commands
    }

    /// Trigger a hook event
    pub async fn trigger(&mut self, event: HookEvent, context: HookContext) -> Result<()> {
        // Prevent recursive hook execution
        if self.executing {
            tracing::debug!("Skipping hook {} - already executing hooks", event.name());
            return Ok(());
        }

        self.executing = true;

        let commands = self.get_hooks(&event, context.session_id.as_ref());

        if commands.is_empty() {
            self.executing = false;
            return Ok(());
        }

        tracing::debug!("Triggering hook: {} with {} commands", event.name(), commands.len());

        for command in commands {
            tracing::info!("Hook {} executing: {}", event.name(), command);

            // Execute the command using /bin/sh -c
            let result = tokio::process::Command::new("/bin/sh")
                .arg("-c")
                .arg(&command)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn();

            match result {
                Ok(mut child) => {
                    // Don't wait for the command to complete - fire and forget
                    tokio::spawn(async move {
                        match child.wait().await {
                            Ok(status) => {
                                tracing::debug!("Hook command completed with status: {}", status);
                            }
                            Err(e) => {
                                tracing::warn!("Hook command failed: {}", e);
                            }
                        }
                    });
                }
                Err(e) => {
                    tracing::error!("Failed to spawn hook command: {}", e);
                }
            }
        }

        self.executing = false;
        Ok(())
    }

    /// List all hooks
    pub fn list_hooks(&self) -> Vec<(String, String, bool)> {
        let mut hooks = Vec::new();

        // Global hooks
        for (event, commands) in &self.global_hooks {
            for command in commands {
                hooks.push((event.name(), command.clone(), true));
            }
        }

        // Session hooks
        for (session_id, session_hooks) in &self.session_hooks {
            for (event, commands) in session_hooks {
                for command in commands {
                    hooks.push((
                        format!("{}:{}", session_id.0, event.name()),
                        command.clone(),
                        false,
                    ));
                }
            }
        }

        hooks
    }
}

impl Default for HookManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Thread-safe hook manager
pub type SharedHookManager = Arc<RwLock<HookManager>>;

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_hook_event_names() {
        assert_eq!(HookEvent::SessionCreated.name(), "session-created");
        assert_eq!(HookEvent::PaneFocusIn.name(), "pane-focus-in");
        assert_eq!(HookEvent::AfterCommand("split-window".to_string()).name(), "after-split-window");
    }

    #[test]
    fn test_hook_event_parsing() {
        assert_eq!(HookEvent::from_name("session-created"), Some(HookEvent::SessionCreated));
        assert_eq!(HookEvent::from_name("pane-focus-in"), Some(HookEvent::PaneFocusIn));
        assert_eq!(
            HookEvent::from_name("after-split-window"),
            Some(HookEvent::AfterCommand("split-window".to_string()))
        );
        assert_eq!(HookEvent::from_name("invalid-hook"), None);
    }

    #[tokio::test]
    async fn test_hook_manager_global() {
        let mut manager = HookManager::new();

        manager.set_global_hook(
            HookEvent::SessionCreated,
            "display-message 'Session created!'".to_string()
        );

        let hooks = manager.get_hooks(&HookEvent::SessionCreated, None);
        assert_eq!(hooks.len(), 1);
        assert_eq!(hooks[0], "display-message 'Session created!'");
    }

    #[tokio::test]
    async fn test_hook_manager_session_specific() {
        let mut manager = HookManager::new();
        let session_id = SessionId(Uuid::new_v4());

        manager.set_session_hook(
            session_id.clone(),
            HookEvent::WindowCreated,
            "refresh-client".to_string()
        );

        let hooks = manager.get_hooks(&HookEvent::WindowCreated, Some(&session_id));
        assert_eq!(hooks.len(), 1);
        assert_eq!(hooks[0], "refresh-client");

        // Different session should not see the hook
        let other_session = SessionId(Uuid::new_v4());
        let other_hooks = manager.get_hooks(&HookEvent::WindowCreated, Some(&other_session));
        assert_eq!(other_hooks.len(), 0);
    }

    #[tokio::test]
    async fn test_hook_context() {
        let session_id = SessionId(Uuid::new_v4());
        let window_id = WindowId(Uuid::new_v4());

        let context = HookContext::new("test-hook".to_string())
            .with_session(session_id.clone())
            .with_window(window_id.clone())
            .with_extra("old_name".to_string(), "old".to_string())
            .with_extra("new_name".to_string(), "new".to_string());

        assert_eq!(context.session_id, Some(session_id));
        assert_eq!(context.window_id, Some(window_id));
        assert_eq!(context.extra.get("old_name"), Some(&"old".to_string()));
    }
}
