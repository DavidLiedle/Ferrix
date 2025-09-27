# Ferrix - Revolutionary Terminal Multiplexer 🚀

<div align="center">

```
╔═══════════════════════════════════════════╗
║   _____ _____ ____  ____  ___ __  __     ║
║  |  ___| ____|  _ \|  _ \|_ _|\ \/ /     ║
║  | |_  |  _| | |_) | |_) || |  \  /      ║
║  |  _| | |___|  _ <|  _ < | |  /  \      ║
║  |_|   |_____|_| \_\_| \_\___/_/\_\      ║
║                                           ║
║  Revolutionary Terminal Multiplexer        ║
║         Built with Rust                   ║
╚═══════════════════════════════════════════╝
```

**The [terminal multiplexer prophecy](https://github.com/cloudstreet-dev/GNU-Screen-vs-Tmux) fulfilled!**

</div>

## What is Ferrix?

Ferrix is a revolutionary terminal multiplexer that combines the reliability of GNU Screen with the features of Tmux, while introducing modern innovations only possible with today's technology. The name combines "Fe" (iron - representing Rust's memory safety) with "Matrix" (representing the matrix of terminal sessions).

## ✨ Features

### Core Features (Phase 1 - Implemented)
- ✅ **Session Management** - Create, attach, detach, and kill sessions
- ✅ **Client-Server Architecture** - Robust separation with async/await throughout
- ✅ **PTY Process Management** - Full pseudo-terminal handling
- ✅ **Basic Window Support** - Single window per session with shell spawning
- ✅ **Detach/Reattach** - Seamlessly disconnect and reconnect to sessions
- ✅ **Memory Safe** - Written in 100% safe Rust

### Planned Features
- 🚧 **Window & Pane Management** - Split horizontally/vertically, resize, navigate
- 🚧 **Configuration System** - TOML-based config with hot-reloading
- 🚧 **Status Bar** - Customizable with git branch, battery, system stats
- 🚧 **Copy Mode** - Vim and Emacs bindings for text selection
- 🚧 **Command Mode** - Runtime commands for advanced control

### Innovative Features (What Sets Ferrix Apart)
- 📸 **Session Snapshots** - Save and restore exact session state
- 🔌 **WASM Plugin System** - Safe plugins that can't crash the multiplexer
- 📋 **Native Clipboard Integration** - Works seamlessly across all platforms
- 🔐 **Encrypted Remote Sessions** - Built-in encryption for security
- 🌳 **Session Versioning** - Git-like branching for experimental sessions
- 🎮 **GPU Acceleration** - Optional rendering acceleration for smooth performance
- 💾 **Crash Recovery** - Automatic session recovery after unexpected exits

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

All commands are prefixed with `Ctrl-b` by default (configurable):

| Key Combo | Action |
|-----------|--------|
| `Ctrl-b d` | Detach from current session |
| `Ctrl-b c` | Create new window (planned) |
| `Ctrl-b n` | Next window (planned) |
| `Ctrl-b p` | Previous window (planned) |
| `Ctrl-b %` | Split pane vertically (planned) |
| `Ctrl-b "` | Split pane horizontally (planned) |
| `Ctrl-b z` | Zoom/unzoom pane (planned) |
| `Ctrl-b [` | Enter copy mode (planned) |
| `Ctrl-b :` | Enter command mode (planned) |

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

## 🏗️ Architecture

Ferrix uses a modern client-server architecture:

- **Server Process** - Manages all sessions, windows, and panes
- **Client Process** - Handles user input and terminal rendering
- **Binary Protocol** - Efficient communication using bincode
- **Async/Await** - Tokio-based async runtime for performance
- **Zero-Copy** - Optimizations where possible for efficiency

## 📊 Performance

Ferrix is designed with performance in mind:

- Startup time < 50ms
- Memory usage < 10MB per session
- Zero perceptible lag in normal use
- Efficient binary protocol for minimal overhead

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

## 🗺️ Roadmap

- [x] Phase 1: Core Multiplexer - Basic attach/detach functionality
- [ ] Phase 2: Windows and Panes - Full window/pane management
- [ ] Phase 3: Configuration and UI - Customization and status bar
- [ ] Phase 4: Advanced Features - Snapshots, plugins, remote sessions
- [ ] Phase 5: Optimization and Polish - Production readiness

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