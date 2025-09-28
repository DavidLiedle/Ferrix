# Ferrix - Modern Terminal Multiplexer

<div align="center">

```
╔═══════════════════════════════════════════╗
║   _____ _____ ____  ____  ___ __  __     ║
║  |  ___| ____|  _ \|  _ \|_ _|\ \/ /     ║
║  | |_  |  _| | |_) | |_) || |  \  /      ║
║  |  _| | |___|  _ <|  _ < | |  /  \      ║
║  |_|   |_____|_| \_\_| \_\___/_/\_\      ║
║                                           ║
║    Modern Terminal Multiplexer             ║
║         Built with Rust                   ║
╚═══════════════════════════════════════════╝
```

**A modern take on terminal multiplexing inspired by [GNU Screen and Tmux](https://github.com/cloudstreet-dev/GNU-Screen-vs-Tmux)**

</div>

## What is Ferrix?

Ferrix is a modern terminal multiplexer that combines the reliability of GNU Screen with features from Tmux, while exploring new possibilities with Rust's safety and performance. The name combines "Fe" (iron - representing Rust's memory safety) with "Matrix" (representing the matrix of terminal sessions).

> **⚠️ Development Status**: Ferrix is in active development. While core terminal multiplexing functionality works, many advanced features are still experimental or in-progress. For production use, consider mature alternatives like tmux or screen.

## ✨ Features

### Currently Working (v0.9.0)
- ✅ **Session Management** - Create, attach, detach, list, and kill sessions
- ✅ **Client-Server Architecture** - Robust separation with async Rust
- ✅ **Multiple Windows & Panes** - Split panes, navigate between them, and manage layouts
- ✅ **PTY Process Management** - Each pane runs its own independent terminal
- ✅ **Detach/Reattach** - Seamlessly disconnect and reconnect to sessions
- ✅ **Visual Pane Rendering** - Borders, focus indication, and content display
- ✅ **Session Snapshots** - Save and restore complete session state including layouts
- ✅ **Enhanced Status Bar** - Shows session info, window/pane counts, activity indicators, and time
- ✅ **Copy Mode** - Enter copy mode for text selection (UI in progress)
- ✅ **Configuration System** - Generate and validate TOML configuration
- ✅ **Scrollback Buffer** - Optimized scrollback with configurable capacity
- ✅ **Pane Synchronization** - Broadcast input to all panes in a window
- ✅ **Session Locking** - Read-only mode for secure session viewing
- ✅ **Activity Monitoring** - Visual indicators for pane activity (bell, output, silence)
- ✅ **Window Renaming** - Rename windows for better organization
- ✅ **Pane Zoom** - Focus on a single pane by zooming it to full window
- ✅ **Keybinding Customization** - Load custom keybindings from config, runtime modification
- ✅ **Auto-Save Intervals** - Automatic session snapshots at configurable intervals

### Partially Implemented
- 🔧 **Enhanced Copy Mode** - Vi-style navigation framework (needs UI completion)
- 🔧 **Command Mode** - Architecture ready, needs command parser
- 🔧 **Session Versioning** - Core implementation exists, needs integration
- 🔧 **Remote Sessions** - TCP/TLS framework ready, needs testing
- 🔧 **Plugin System** - WASM architecture complete, needs activation

### Planned
- 📋 **Native Clipboard** - Cross-platform clipboard integration
- 📋 **GPU Acceleration** - Optional wgpu-based rendering
- 📋 **Hot Reload Config** - Live configuration updates
- 📋 **Advanced Scripting** - Lua or Rhai scripting support

## 🚀 Quick Start

### Installation

```bash
# Clone the repository
git clone https://github.com/davidliedle/Ferrix
cd Ferrix/ferrix

# Build from source
cargo build --release

# Install to your PATH
cargo install --path .
```

### Basic Usage

```bash
# Start Ferrix (creates a new session if none exist)
ferrix

# Start the server explicitly
ferrix server --foreground

# Create a new named session
ferrix new -s my-session

# List all sessions
ferrix list

# Attach to a session
ferrix attach -t my-session

# Detach from current session
# Press Ctrl-b d while in a session

# Kill a session
ferrix kill -t my-session
```

### Key Bindings

All commands are prefixed with `Ctrl-a` (similar to GNU Screen):

| Key Combo | Action |
|-----------|--------|
| `Ctrl-a d` | Detach from current session |
| `Ctrl-a c` | Create new window |
| `Ctrl-a n` | Next window |
| `Ctrl-a p` | Previous window |
| `Ctrl-a %` | Split pane vertically |
| `Ctrl-a "` | Split pane horizontally |
| `Ctrl-a` + arrows | Navigate between panes |
| `Ctrl-a z` | Zoom/unzoom current pane |
| `Ctrl-a x` | Close current pane |
| `Ctrl-a [` | Enter copy mode |
| `Ctrl-a w` | List windows |

## 🔧 Configuration

Ferrix uses TOML for configuration. The default configuration file is located at `~/.config/ferrix/config.toml`.

Example configuration:

```toml
[general]
default_shell = "/bin/zsh"
escape_key = "ctrl-a"  # Use Ctrl-a like GNU Screen
mouse = true
clipboard = true

[colors]
background = "#1e1e1e"
foreground = "#d4d4d4"
pane_border = "#444444"
pane_active_border = "#569cd6"

[status_bar]
position = "bottom"
left = "[{session}]"
right = "{time:%H:%M}"
```


## 📊 Architecture & Performance

Ferrix uses a modern async Rust architecture:

- **Async/Await** - Tokio-based runtime for efficient concurrency
- **Binary Protocol** - Efficient bincode serialization for IPC
- **Memory Safe** - 100% safe Rust with no unsafe blocks
- **Modular Design** - Clean separation of concerns for maintainability

Performance characteristics:
- Fast startup and low memory footprint
- Efficient client-server communication
- Responsive terminal handling

## 🤝 Contributing

Contributions are welcome! Please see the [DEVELOPMENT_PLAN.md](DEVELOPMENT_PLAN.md) for the project roadmap and architecture details.

### Development Setup

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Clone and build
git clone https://github.com/davidliedle/Ferrix
cd Ferrix
cargo build --release

# Run tests
cargo test

# Run benchmarks
cargo bench
```

## 🗺️ Development Status

### Current Version: v0.2.0

Ferrix is a **working terminal multiplexer** with essential features implemented. While still in active development, it provides a functional alternative for basic terminal multiplexing needs.

**What works today:**
- ✅ Create and manage multiple terminal sessions
- ✅ Split windows into multiple panes (vertical/horizontal)
- ✅ Navigate between panes with keyboard shortcuts
- ✅ Detach and reattach to running sessions
- ✅ Each pane runs an independent shell process
- ✅ Visual pane borders with focus indication
- ✅ Status bar showing session information
- ✅ Save and restore session snapshots with layouts

**Known limitations:**
- Terminal emulation is basic (no full ANSI support yet)
- Copy mode UI needs completion
- Performance optimization needed for large outputs
- Some edge cases in pane resizing
- Limited to local sessions (remote support not activated)

**Upcoming improvements:**
- Better terminal emulation compliance
- Completed copy/paste functionality
- Performance optimizations
- Plugin system activation
- Remote session support

## 📜 License

Ferrix is dual-licensed under MIT OR Apache-2.0. Choose whichever license works best for you.

## 🙏 Acknowledgments

Inspired by:
- GNU Screen - The original terminal multiplexer
- Tmux - The feature-rich successor
- The Rust community - For amazing libraries and support

## 💬 Community

- Report issues on [GitHub Issues](https://github.com/davidliedle/Ferrix/issues)
- Discussions on [GitHub Discussions](https://github.com/davidliedle/Ferrix/discussions)

---

<div align="center">

**Built with ❤️ and Rust by DavidCanHelp and Claude Code Opus 4.1**

</div>