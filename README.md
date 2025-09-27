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

## ✨ Features

### Currently Working
- ✅ **Session Management** - Create, attach, detach, list, and kill sessions
- ✅ **Client-Server Architecture** - Robust separation with async Rust
- ✅ **PTY Process Management** - Full pseudo-terminal handling
- ✅ **Basic Window Support** - Single window per session with shell spawning
- ✅ **Detach/Reattach** - Seamlessly disconnect and reconnect to sessions
- ✅ **Session Snapshots** - Save, load, list, and delete session snapshots
- ✅ **Snapshot Import/Export** - Export snapshots to compressed archives
- ✅ **Configuration System** - Generate and validate TOML configuration

### Implemented (Architecture Complete, Integration Needed)
- ✔️ **Window & Pane Management** - Binary tree layout engine with splits and navigation
- ✔️ **Enhanced Copy Mode** - Vi-style navigation with visual selection modes
- ✔️ **Session Versioning** - Git-like branching, commits, and merges for sessions
- ✔️ **Remote Sessions** - TCP/TLS support with authentication framework
- ✔️ **Plugin System** - WASM plugin architecture with sandboxed execution

### In Development
- 🚧 **Status Bar** - Customizable with session info and system stats
- 🚧 **Command Mode** - Runtime commands for advanced control
- 🚧 **Native Clipboard** - Cross-platform clipboard integration
- 🚧 **GPU Acceleration** - Optional wgpu-based rendering (API updates needed)
- 🚧 **Hot Reload Config** - Live configuration updates without restart

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

- ✅ **Phase 1: Core Multiplexer** - Complete and functional
- ✅ **Phase 2: Windows and Panes** - Architecture implemented, needs UI integration
- ✅ **Phase 3: Configuration** - Basic system working, UI components in progress
- ✅ **Phase 4: Advanced Features** - Core implementations complete
- 🚧 **Phase 5: Polish** - Testing, optimization, and production hardening needed

### Current State

Ferrix is a **functional terminal multiplexer** with core features working. The project has grown beyond initial plans with advanced architecture for windows, panes, plugins, and remote sessions all implemented. However, integration between components and UI polish is still in progress.

**What you can do today:**
- Create and manage multiple terminal sessions
- Detach and reattach to running sessions
- Save and restore session snapshots
- Run commands in detached sessions

**What's coming soon:**
- Full window splitting and pane navigation
- Plugin system activation
- Remote session connectivity
- Complete configuration system

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