# Changelog

All notable changes to Ferrix will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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