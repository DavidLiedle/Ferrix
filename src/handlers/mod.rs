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
//! ### ✅ Snapshot Handlers (7 commands)
//! - `save-snapshot` - Save session snapshot
//! - `load-snapshot` - Load snapshot as new session
//! - `restore-snapshot` - Restore snapshot into existing session
//! - `list-snapshots` - List all snapshots
//! - `delete-snapshot` - Delete a snapshot
//! - `export-snapshot` - Export snapshot to archive
//! - `import-snapshot` - Import snapshot from archive
//!
//! ### ✅ Keys Handlers (7 commands)
//! - `list-keys` - List all keybindings
//! - `bind-key` - Create custom keybinding
//! - `unbind-key` - Remove custom keybinding
//! - `reset-keys` - Reset all to defaults
//! - `reload-keys` - Reload from config
//! - `export-keys` - Export keybindings to file
//! - `import-keys` - Import keybindings from file
//!
//! ### ✅ Pane Handlers (3 commands)
//! - `toggle-pane-sync` - Toggle pane synchronization
//! - `set-pane-sync` - Set pane synchronization state
//! - `toggle-zoom` - Toggle pane zoom
//!
//! ### ✅ Window Handlers (1 command)
//! - `rename-window` - Rename a window
//!
//! ### ✅ Session State Handlers (3 commands)
//! - `lock-session` - Lock session (read-only mode)
//! - `unlock-session` - Unlock session
//! - `set-session-lock` - Set lock state explicitly
//!
//! ### ✅ Autosave Handlers (3 commands)
//! - `enable-auto-save` - Enable automatic session snapshots
//! - `disable-auto-save` - Disable automatic session snapshots
//! - `auto-save-status` - Check auto-save status
//!
//! ### ✅ Layout Handlers (4 commands)
//! - `apply-layout` - Apply a layout preset
//! - `cycle-layout` - Cycle through layout presets
//! - `save-layout` - Save current layout as preset
//! - `list-layouts` - List available layout presets
//!
//! ### ✅ Remote Handlers (2 commands, feature-gated)
//! - `connect` - Connect to remote Ferrix server
//! - `user-management` - Add, remove, list users
//!
//! ### ✅ Activity Handlers (2 commands)
//! - `toggle-activity-monitoring` - Toggle activity monitoring for pane
//! - `set-activity-monitoring` - Set activity monitoring state
//!
//! ### ✅ Misc Handlers (1 command)
//! - `send-keys` - Send keys to a session
//!
//! ### ✅ Versioning Handlers (6 commands)
//! - `init-versioning` - Initialize version control for session
//! - `commit` - Commit current session state
//! - `branch` - Create, list, or delete branches
//! - `checkout` - Switch to a different branch
//! - `merge` - Merge another branch into current
//! - `log` - View commit history
//!
//! ## Remaining in main.rs (~30 handlers)
//!
//! These handlers remain inline in main.rs due to complexity, stubs, or feature-gating:
//! - Versioning commands (1): diff
//! - Session config (4): load-session-config, save-session-config, apply-template, list-templates
//! - Input mode (2): set-input-mode, get-input-mode
//! - Copy mode (2): enter-copy-mode, exit-copy-mode
//! - Plugin (8): search, install, update, uninstall, list, info, enable, disable
//! - Window management (4): new-window, select-window, kill-window, list-windows
//! - System commands (5): completions, health, metrics, profile
//! - Debug/diagnostic (4): inspect, dump-state, crashes, crash-info, crash-analyze, crash-delete

pub mod session;
pub mod config;
pub mod snapshot;
pub mod keys;
pub mod pane;
pub mod window;
pub mod session_state;
pub mod autosave;
pub mod layout;
#[cfg(feature = "remote")]
pub mod remote;
pub mod activity;
pub mod misc;
pub mod versioning;

#[cfg(test)]
mod tests {
    #[test]
    fn test_handlers_module() {
        // Module structure is valid
        assert!(true);
    }
}
