# Changelog

All notable changes to Ferrix will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.10.1] - 2025-10-03

### Fixed
- **Directory Creation**: Fixed missing `.ferrix` and snapshot directories on first run
  - Auto-creates `~/.ferrix/` directory for configuration and recovery files
  - Auto-creates `~/.ferrix/snapshots/` directory for session snapshots
  - Fallback to `/tmp/ferrix/` if home directory not available
- **Error Messages**: Improved error message when server is not running
  - Now suggests running `ferrix server` when connection fails
  - Clearer indication of socket path in error messages

## [0.10.0] - 2025-10-03

### Added
- **Full ANSI/VT100 Terminal Emulation**: Complete support for vim, less, htop, and other TUI applications
  - DEC Private Modes support (?25h/l, ?1049h/l, ?2004h/l, etc.)
  - 256-color and RGB color support
  - Alternate screen buffer for full-screen applications
  - Complete SGR (Select Graphic Rendition) implementation
  - Line drawing and special characters support
- **Enhanced Copy Mode**: Visual selection with clipboard integration
  - Visual mode for text selection
  - Yank to system clipboard via arboard
  - Search functionality with highlighting
  - Vi-style navigation commands
- **Command Mode**: 30+ tmux-compatible commands
  - Session, window, and pane management commands
  - Configuration commands (set-option, show-options)
  - Recording and snapshot commands
  - Plugin management commands
- **Plugin System**: WASM-based plugin architecture
  - Hot loading and reloading of plugins
  - Plugin API for extending functionality
  - Event system for plugin communication
  - Isolated execution with wasmtime 27.0
- **Session Recording & Replay**: Record terminal sessions for later playback
  - Compression support with gzip
  - Metadata tracking (duration, terminal size, etc.)
  - Recording events: input, output, resize
  - Playback with timing preservation
- **Hot Configuration Reload**: Live configuration updates without restart
  - File watching with debouncing
  - Automatic validation and rollback on error
  - Supports all configuration options
- **Remote Sessions**: TCP/TLS support for remote multiplexing
  - Secure TLS connections with rustls
  - Authentication system with user management
  - Remote attach/detach capabilities
- **Performance Optimizations**: Enhanced for large outputs
  - Adaptive batching based on throughput
  - Delta compression for screen updates
  - Backpressure handling to prevent buffer overflow
  - Optimized PTY reading with larger buffers
- **Comprehensive Test Suite**: 250+ tests
  - Unit tests for all core components
  - Integration tests for component interaction
  - Protocol tests for message handling
  - End-to-end tests for real workflows

### Changed
- Improved pane resizing with layout-aware algorithms
- Enhanced search functionality with regex support
- Better error handling and recovery mechanisms
- Updated dependencies to latest versions

### Fixed
- Fixed CSI parameter parsing for DEC Private Mode sequences
- Fixed protocol message mismatches in command mode
- Fixed test compilation errors for internal types
- Fixed CLI argument ordering in tests

## [0.9.3] - 2025-10-03

### Fixed
- **Session Persistence**: Fixed critical server crash on client detach
- **PTY Channel Management**: PTY async task now handles disconnected clients gracefully
- **Detach/Reattach Cycles**: Multiple detach/reattach cycles now work correctly
- **Per-Session PTY Polling**: Moved PTY poller from per-client to per-session architecture

### Technical Details
- PTY reader thread no longer exits when output channel send fails
- PTY poller broadcasts to all attached clients instead of single client
- Sessions persist independently of client connections
- Server remains stable when all clients detach from a session

This release enables proper terminal multiplexer functionality where sessions can be detached and reattached without losing state or crashing the server.

## [0.9.2] - 2025-10-03

### Added
- **Pane Resizing**: Full directional resize functionality (Up/Down/Left/Right) with PTY integration
- **SendKeys Command**: Send keys to target session by name or ID with proper attach/detach workflow
- **Window Selection by Number**: Select windows 0-9 via keybindings with full integration
- **Custom Path Export/Import for Keybindings**: Export and import keybindings to/from custom file paths
- **Copy Mode Mouse Selection**: Mouse-based text selection in copy mode with server communication
- **Plugin Download**: HTTP-based plugin download with reqwest, including executable permissions

### Changed
- All stub implementations replaced with real, production-ready code
- KeyBindingManager now supports file I/O with TOML format parsing
- Client now has `send_keys()` method for programmatic input sending
- Server handlers fully integrated with session management for all new features
- Added `reqwest` dependency for HTTP downloads

### Fixed
- Removed all TODO comments requiring implementation
- Eliminated all "not yet implemented" error messages
- Added missing `warn` import in client module
- Fixed async/await patterns in server message handlers

### Technical
- Zero compilation errors, only minor warnings
- Enhanced error handling across all new features
- Improved documentation for all v0.9.0-0.9.2 features
- Production-ready codebase with no remaining stubs

## [0.9.1] - 2025-09-28

### Changed
- Comprehensive documentation update for v0.9.0 features
- Updated README.md with complete feature list
- Updated FEATURES.md with detailed v0.9.0 capabilities
- Enhanced docs/commands.md with new CLI commands
- Improved docs/configuration.md with keybinding and auto-save configuration
- Expanded docs/USER_GUIDE.md with guides for all new features

### Fixed
- Documentation inconsistencies and outdated information
- Missing documentation for v0.9.0 features

## [0.9.0] - 2024-01-28

### Added

#### Core Features
- **Scrollback Buffer Optimization** - Efficient terminal history with configurable capacity
- **Pane Synchronization** - Broadcast input to all panes in a window simultaneously
- **Session Locking** - Read-only mode for secure session viewing without modification
- **Activity Monitoring** - Visual indicators for pane activity (bell 🔔, output ●, silence ○)
- **Window Renaming** - Rename windows for better organization
- **Pane Zoom** - Focus on a single pane by expanding it to full window
- **Keybinding Customization** - Load custom keybindings from config with runtime modification
- **Auto-Save Intervals** - Automatic session snapshots at configurable intervals (default 5 minutes)

#### Technical Improvements
- ANSI parser for improved terminal emulation
- Activity tracking system with per-pane monitoring
- Enhanced configuration system with custom keybindings support
- Background auto-save task manager for periodic snapshots
- Improved protocol messages for new features

#### CLI Commands
- `rename-window` - Rename windows
- `toggle-pane-sync` / `set-pane-sync` - Control pane synchronization
- `lock-session` / `unlock-session` / `set-session-lock` - Session locking controls
- `toggle-zoom` - Toggle pane zoom
- `toggle-activity-monitoring` / `set-activity-monitoring` - Activity monitoring controls
- `list-keys` / `bind-key` / `unbind-key` / `reset-keys` / `reload-keys` - Keybinding management
- `export-keys` / `import-keys` - Export/import keybinding configurations
- `enable-auto-save` / `disable-auto-save` / `auto-save-status` - Auto-save controls

### Changed
- Enhanced status bar with activity indicators and additional information
- Improved window list display with activity status
- Updated configuration structure to support new features

### Fixed
- Various minor bug fixes and performance improvements

## [0.8.0] - Previous Release

### Added
- Initial implementation of core terminal multiplexer functionality
- Session management (create, attach, detach, list, kill)
- Window and pane management
- Basic copy mode framework
- Session snapshots
- Configuration system
- Status bar

## [0.1.0] - Initial Release

### Added
- Basic client-server architecture
- PTY process management
- Simple session handling
- Basic terminal rendering

[0.9.0]: https://github.com/DavidLiedle/Ferrix/releases/tag/v0.9.0
[0.8.0]: https://github.com/DavidLiedle/Ferrix/releases/tag/v0.8.0
[0.1.0]: https://github.com/DavidLiedle/Ferrix/releases/tag/v0.1.0