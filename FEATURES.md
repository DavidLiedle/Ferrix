# Ferrix Features Documentation

## Version 0.11.0 Release

## Overview
Ferrix is a modern terminal multiplexer written in Rust that combines the reliability of GNU Screen with the features of Tmux, while adding innovative capabilities only possible with modern technology. This release adds major enhancements including GPU rendering, session versioning, plugin marketplace, and advanced input modes.

## Core Features

### 1. Session Management
- **Persistent Sessions**: Sessions remain active even when detached
- **Session Recovery**: Automatic session persistence on graceful shutdown
- **Multi-Client Support**: Multiple clients can attach to the same session simultaneously
- **Session Snapshots**: Save and restore session states

### 2. Window and Pane Management
- **Multiple Windows**: Create and manage multiple windows within a session
- **Pane Splitting**: Split panes horizontally or vertically
- **Layout Presets**: Quick access to predefined layouts
- **Dynamic Resizing**: Resize panes on the fly
- **Pane Navigation**: Navigate between panes using keyboard shortcuts

### 3. Layout Presets
Built-in layouts for common workflows:
- `single` - Single pane
- `vsplit` - Two equal vertical panes
- `hsplit` - Two equal horizontal panes
- `main-left` - Main pane on left (70%)
- `main-right` - Main pane on right (70%)
- `main-top` - Main pane on top (70%)
- `main-bottom` - Main pane on bottom (70%)
- `3v` - Three equal vertical panes
- `3h` - Three equal horizontal panes
- `2x2` - Four panes in a grid
- `ide` - IDE layout with sidebar and terminal
- `3x2` - Six panes in a 3x2 grid

### 4. Copy Mode
Vi-style copy mode with advanced features:
- **Text Selection**: Visual, visual-line, and visual-block modes
- **Clipboard Integration**: System clipboard support via arboard
- **Search**: Forward and reverse search with highlighting
- **Jump List**: Navigate through position history
- **Yank & Paste**: Copy and paste text within and across sessions

### 5. Command Mode
Vi-style command mode with extensive commands:
- `:q` - Quit/detach
- `:w [name]` - Save snapshot
- `:split` / `:vsplit` - Split panes
- `:new [name]` - Create new window
- `:layout <preset>` - Apply layout preset
- `:copy` - Enter copy mode
- `:resize <dir> <n>` - Resize panes
- `:set <option>` - Configure settings

### 6. Mouse Support
Modern mouse interactions:
- **Click to Focus**: Click on panes to focus
- **Drag to Resize**: Drag pane borders to resize
- **Text Selection**: Click and drag to select text
- **Scroll Support**: Mouse wheel scrolling in panes
- **Double-Click**: Select words with double-click
- **Right-Click Menu**: Context-sensitive actions

### 7. New Features (v0.9.0)

#### Scrollback Buffer
- **Optimized Storage**: Efficient memory usage with configurable capacity
- **Fast Scrolling**: Smooth scrolling through terminal history
- **Search in History**: Find text in scrollback buffer
- **Configurable Size**: Set buffer size per pane or globally

#### Pane Synchronization
- **Broadcast Mode**: Send input to all panes simultaneously
- **Toggle Control**: Easy on/off switching with `toggle-pane-sync`
- **Visual Indicator**: Status bar shows sync state
- **Selective Sync**: Can be enabled per window

#### Session Locking
- **Read-Only Mode**: Lock session to prevent accidental changes
- **Security Feature**: Allow viewing without modification
- **Visual Feedback**: Status bar indicates locked state
- **Quick Toggle**: Easy lock/unlock commands

#### Activity Monitoring
- **Visual Indicators**: 🔔 for bell, ● for activity, ○ for silence
- **Per-Pane Monitoring**: Track individual pane activity
- **Status Bar Integration**: Activity shown in window list
- **Configurable Thresholds**: Set silence detection timeout

#### Window Management
- **Window Renaming**: Rename windows for better organization
- **Pane Zoom**: Focus on single pane with `toggle-zoom`
- **Activity Status**: See activity indicators per window
- **Enhanced Navigation**: Better window switching commands

#### Keybinding System
- **Custom Bindings**: Define your own key mappings
- **Config File Support**: Load bindings from configuration
- **Runtime Modification**: Change bindings without restart
- **Import/Export**: Share keybinding configurations
- **Conflict Detection**: Validates binding conflicts

#### Auto-Save System
- **Automatic Snapshots**: Save sessions at regular intervals
- **Configurable Intervals**: Set save frequency (default 5 minutes)
- **Background Operation**: Non-blocking auto-save
- **Recovery Support**: Restore from auto-saves after crashes

### 8. Advanced Features

#### Process Management
- **Proper PTY Handling**: Clean process lifecycle management
- **Memory Leak Prevention**: Automatic cleanup of resources
- **Orphaned Process Prevention**: Ensures child processes are terminated
- **Signal Handling**: Graceful shutdown with SIGTERM/SIGINT

#### Session Sharing
- **Multi-Client Architecture**: Multiple users can collaborate
- **Synchronized Updates**: Real-time updates across all clients
- **Independent Views**: Each client maintains their own viewport

#### Recovery & Persistence
- **Automatic Snapshots**: Sessions saved on shutdown
- **Manual Snapshots**: Save session state at any time
- **Crash Recovery**: Restore sessions after unexpected termination
- **Environment Preservation**: Maintain environment variables
- **Auto-Save Recovery**: Restore from periodic auto-saves

## Key Bindings

### Default Mode
- `Ctrl-b` - Prefix key (configurable)
- `Prefix + %` - Split vertical
- `Prefix + "` - Split horizontal
- `Prefix + arrow` - Navigate panes
- `Prefix + c` - Create window
- `Prefix + n/p` - Next/previous window
- `Prefix + d` - Detach session
- `Prefix + [` - Enter copy mode
- `Prefix + :` - Enter command mode
- `Prefix + z` - Toggle pane zoom
- `Prefix + ,` - Rename current window
- `Prefix + ?` - Show all keybindings
- `Prefix + M` - Toggle mouse mode

### Copy Mode (Vi-style)
- `h/j/k/l` - Move cursor
- `w/b/e` - Word movement
- `0/$` - Line start/end
- `g/G` - Document start/end
- `v/V/Ctrl-v` - Visual modes
- `y` - Yank selection
- `/` - Forward search
- `?` - Reverse search
- `n/N` - Next/previous match
- `q/Esc` - Exit copy mode

### Command Mode
- `Tab` - Command completion
- `Up/Down` - Command history
- `Enter` - Execute command
- `Esc` - Cancel command

## Configuration

### Config File Location
- `~/.ferrix/config.toml` - User configuration
- `/etc/ferrix/config.toml` - System-wide defaults

### Example Configuration
```toml
[general]
prefix_key = "ctrl-b"
mouse = true
history_limit = 10000
default_shell = "/bin/zsh"

[appearance]
status_bar = true
status_position = "bottom"
theme = "dark"

[copy_mode]
mode = "vi"  # or "emacs"

[keybindings]
split_vertical = "%"
split_horizontal = "\""
new_window = "c"
```

## Architecture

### Client-Server Model
- **Server Process**: Manages sessions, windows, and panes
- **Client Process**: Handles UI rendering and user input
- **IPC Protocol**: Binary protocol over Unix domain sockets
- **Async Runtime**: Built on Tokio for high performance

### Technology Stack
- **Language**: Rust for safety and performance
- **Terminal Handling**: crossterm for cross-platform support
- **PTY Management**: portable-pty for process control
- **Async Runtime**: Tokio for concurrent operations
- **Serialization**: Bincode for efficient IPC

## Performance Features
- **Zero-Copy Buffers**: Minimize memory allocations
- **Lazy Rendering**: Only render visible content
- **Efficient Scrollback**: Optimized buffer management
- **Smart Polling**: Adaptive PTY output polling

## Security Features
- **Process Isolation**: Each pane runs in its own PTY
- **Secure IPC**: Unix socket with filesystem permissions
- **Credential Protection**: Proper handling of sensitive data
- **Signal Safety**: Robust signal handling

## Comparison with Other Multiplexers

| Feature | Ferrix | Tmux | Screen |
|---------|--------|------|--------|
| Language | Rust | C | C |
| Copy Mode | Vi/Emacs | Vi/Emacs | Custom |
| Mouse Support | Full | Partial | Limited |
| Layout Presets | 12+ | Few | None |
| Session Sharing | Native | Plugin | Native |
| Clipboard Integration | Native | External | None |
| GPU Rendering | Planned | No | No |
| Memory Safety | Yes | No | No |

## New Features in v0.11.0

### Session Versioning (Git-like)
- **Branch Management**: Create and merge session branches
- **Commit History**: Track session changes with commits
- **Cherry-pick**: Apply specific changes from other branches
- **Revert/Reset**: Undo changes or reset to previous states
- **Three-way Merge**: Automatic conflict resolution

### Per-Session Configurations
- **Override Global Settings**: Each session can have custom configuration
- **Session Templates**: Pre-configured templates for different workflows
- **Environment Variables**: Session-specific environment
- **Hooks**: Run commands on session events
- **Hot Reload**: Configuration changes apply immediately

### Advanced Input Modes
- **Vim Mode**: Complete vi/vim modal editing support
- **Emacs Mode**: Full emacs key bindings
- **Key Chords**: Multi-key sequences for complex commands
- **Leader Keys**: Customizable leader key sequences
- **Mode Indicators**: Visual feedback for current mode

### Plugin Marketplace
- **Central Repository**: Browse and install plugins
- **Dependency Management**: Automatic dependency resolution
- **Reviews & Ratings**: Community feedback system
- **Version Control**: Plugin versioning and updates
- **WASM Plugins**: Secure sandboxed execution

### GPU Acceleration
- **Hardware Detection**: Automatic GPU capability detection
- **Fallback Rendering**: Graceful degradation to terminal renderer
- **Performance Profiling**: Real-time FPS monitoring
- **MSAA Support**: Anti-aliasing for smooth text
- **Adaptive Settings**: Automatic quality adjustments

### Enhanced Status Bar
- **System Monitoring**: Live CPU, memory, disk usage with indicators
- **Git Integration**: Branch and repository status
- **Battery Status**: Power level and charging state
- **Custom Widgets**: Extensible widget system
- **Visual Indicators**: Color-coded status information

## Completed Enhancements
- ✅ GPU-accelerated rendering (v0.11.0)
- ✅ Plugin system with marketplace (v0.11.0)
- ✅ Session versioning (v0.11.0)
- ✅ Advanced input modes (v0.11.0)
- ✅ Per-session configurations (v0.11.0)
- ✅ Enhanced status bar (v0.11.0)

## Future Enhancements (Planned)
- Remote session support (SSH)
- Collaborative editing features
- Time travel debugging
- Advanced scripting support
- Terminal recording and playback
- AI-powered command suggestions