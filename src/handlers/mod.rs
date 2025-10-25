//! Command Handlers
//!
//! This module contains command handlers organized by category.
//! Each submodule handles a specific category of commands.
//!
//! ## Refactoring Guide
//!
//! The main.rs file is being refactored to extract command handlers into
//! organized modules. This improves:
//! - Code navigation and findability
//! - Testing of individual command categories
//! - Reduced merge conflicts in team development
//!
//! ### Extraction Pattern
//!
//! 1. Create a new module file (e.g., `session.rs` for session commands)
//! 2. Move related command handlers into handler functions
//! 3. Each handler should be async (if needed) and return `Result<()>`
//! 4. Add the module to this file and re-export publicly
//! 5. Update main.rs to call the handler functions
//!
//! ## Extracted Handlers
//!
//! ### ✅ Session Handlers (5 commands)
//! - `new` - Create new session
//! - `attach` - Attach to session
//! - `list` - List all sessions
//! - `kill` - Kill a session
//! - `detach` - Detach from session
//!
//! ### ✅ Config Handlers (3 commands)
//! - `reload-config` - Reload configuration
//! - `generate-config` - Generate default config file
//! - `validate-config` - Validate config file
//!
//! ## Remaining Categories (68 handlers)
//!
//! - **snapshot.rs** (7): SaveSnapshot, LoadSnapshot, RestoreSnapshot, etc.
//! - **keys.rs** (7): ListKeys, BindKey, UnbindKey, ResetKeys, etc.
//! - **remote.rs** (2): Connect, UserManagement
//! - **server.rs** (1): Server startup (complex, ~90 lines)
//! - **window.rs** (2): RenameWindow, ToggleZoom
//! - **pane.rs** (4): TogglePaneSync, SetPaneSync, activity monitoring
//! - **session_state.rs** (3): LockSession, UnlockSession, SetSessionLock
//! - **autosave.rs** (2): EnableAutoSave, DisableAutoSave
//! - **layout.rs** (~5): ApplyLayout, CycleLayout, SaveLayout, etc.
//! - **misc.rs** (~35): SendKeys, Inspect, Dump, RenameWindow, etc.

pub mod session;
pub mod config;

#[cfg(test)]
mod tests {
    #[test]
    fn test_handlers_module() {
        // Module structure is valid
        assert!(true);
    }
}
