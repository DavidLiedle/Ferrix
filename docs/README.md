# Ferrix Documentation

Welcome to the Ferrix documentation! Ferrix is a revolutionary Rust-based terminal multiplexer that combines the reliability of GNU Screen with the features of tmux, while adding innovative capabilities only possible with modern technology.

## 📚 Documentation Index

- [Getting Started](./getting-started.md) - Installation and basic usage
- [Configuration Guide](./configuration.md) - Complete guide to configuration
- [Commands Reference](./commands.md) - All CLI commands
- [Snapshots & Recovery](./snapshots.md) - Session persistence and crash recovery
- [Feature Flags](./FEATURES.md) - Build-time features and groups
- [Hooks](./HOOKS.md) - Event hooks and automation
- [Format Variables](./FORMAT_VARIABLES.md) - Status bar and formatting
- [Shell Completions](./SHELL_COMPLETIONS.md) - Completion scripts for common shells
- [Testing](./TESTING.md) - Test strategy and workflows

## 🚀 Quick Start

```bash
# Start Ferrix with default session
ferrix

# Create named session
ferrix new -s mysession

# List sessions
ferrix list

# Attach to session
ferrix attach mysession

# Save snapshot
ferrix save-snapshot mysession

# Generate config file
ferrix generate-config
```

## ✨ Key Features

### Core Multiplexing
- **Client-server architecture** with Unix socket communication
- **Session management** with persistent state
- **Window and pane management** with flexible layouts
- **Full terminal emulation** with PTY support

### Revolutionary Features
- **Session Snapshots**: Save and restore exact session state
- **Crash Recovery**: Automatic recovery after system failures
- **Configuration System**: Flexible, file-based configuration
- **Native Clipboard**: System clipboard integration
- **WASM Plugins**: Extensible plugin system (coming soon)

### Developer-Friendly
- **Written in Rust**: Memory-safe and blazingly fast
- **Async/await**: Modern concurrent architecture
- **Extensible**: Plugin system for custom functionality
- **Well-documented**: Comprehensive documentation and examples

## 🎯 Design Philosophy

Ferrix follows these core principles:

1. **Compatibility**: Familiar to Screen/tmux users
2. **Reliability**: Never lose your work
3. **Performance**: Optimized for speed
4. **Extensibility**: Plugin-ready architecture
5. **Usability**: Intuitive and well-documented

## 📋 System Requirements

- **OS**: Linux, macOS, BSD (Windows WSL supported)
- **Rust**: 1.70+ (for building from source)
- **Terminal**: Any modern terminal emulator
- **Shell**: Any POSIX-compatible shell

## 🔗 Quick Links

- [GitHub Repository](https://github.com/davidliedle/Ferrix)
- [Issue Tracker](https://github.com/davidliedle/Ferrix/issues)
- [Release Notes](./CHANGELOG.md)
- [License](../LICENSE)

## 📖 Table of Contents

1. **User Guide**
   - Getting Started
   - Basic Usage
   - Configuration
   - Key Bindings

2. **Features**
   - Sessions & Windows
   - Panes & Layouts
   - Copy Mode
   - Snapshots & Recovery

3. **Advanced Topics**
   - Hooks & Automation
   - Scripting
   - Plugin Development
   - Performance Tuning

4. **Reference**
   - Commands
   - Configuration Options
   - Environment Variables
   - Troubleshooting

---

*Ferrix - The Terminal Multiplexer Prophecy Fulfilled*