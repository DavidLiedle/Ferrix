# Ferrix User Guide

## Table of Contents
1. [Introduction](#introduction)
2. [Installation](#installation)
3. [Getting Started](#getting-started)
4. [Basic Commands](#basic-commands)
5. [Sessions](#sessions)
6. [Windows and Panes](#windows-and-panes)
7. [Copy Mode](#copy-mode)
8. [Configuration](#configuration)
9. [Remote Sessions](#remote-sessions)
10. [Plugins](#plugins)
11. [Advanced Features](#advanced-features)

## Introduction

Ferrix is a revolutionary terminal multiplexer that combines the best features of GNU Screen and tmux while introducing innovative capabilities only possible with modern technology. Built in Rust for performance and safety, Ferrix offers:

- **Session persistence** across system restarts
- **Advanced window and pane management** with intelligent layouts
- **Vim-style copy mode** with search and visual selection
- **Remote session support** with TLS encryption
- **Plugin system** for extensibility
- **Git-like session versioning** for checkpoint and rollback
- **GPU-accelerated rendering** (when available)

## Installation

### From Source

```bash
# Clone the repository
git clone https://github.com/yourusername/ferrix.git
cd ferrix

# Build in release mode
cargo build --release

# Install to system
sudo cp target/release/ferrix /usr/local/bin/

# Verify installation
ferrix --version
```

### Using Cargo

```bash
cargo install ferrix
```

## Getting Started

### Creating Your First Session

```bash
# Start a new session
ferrix new

# Start a named session
ferrix new -s development

# Start a detached session
ferrix new -s background --detached
```

### Listing Sessions

```bash
# List all sessions
ferrix list

# List sessions with details
ferrix list --verbose
```

### Attaching to Sessions

```bash
# Attach to the most recent session
ferrix attach

# Attach to a specific session
ferrix attach -t development

# Force attach (detach other clients)
ferrix attach -t development --force
```

## Basic Commands

Once attached to a session, use the prefix key (default: `Ctrl-b`) followed by a command key:

### Essential Commands
- `Ctrl-b d` - Detach from session
- `Ctrl-b ?` - Show help
- `Ctrl-b :` - Enter command mode
- `Ctrl-b [` - Enter copy mode
- `Ctrl-b ]` - Paste buffer

### Window Management
- `Ctrl-b c` - Create new window
- `Ctrl-b n` - Next window
- `Ctrl-b p` - Previous window
- `Ctrl-b 0-9` - Switch to window by number
- `Ctrl-b w` - List windows
- `Ctrl-b ,` - Rename current window
- `Ctrl-b &` - Kill current window

### Pane Management
- `Ctrl-b %` - Split pane horizontally
- `Ctrl-b "` - Split pane vertically
- `Ctrl-b o` - Switch to next pane
- `Ctrl-b ;` - Switch to last active pane
- `Ctrl-b ↑/↓/←/→` - Navigate panes by direction
- `Ctrl-b q` - Show pane numbers
- `Ctrl-b x` - Kill current pane
- `Ctrl-b z` - Zoom/unzoom pane
- `Ctrl-b {/}` - Swap panes

## Sessions

### Session Management

```bash
# Create session with custom shell
ferrix new -s dev -c /usr/bin/zsh

# Create session in specific directory
ferrix new -s project -d ~/projects/myapp

# Kill a session
ferrix kill -t old-session

# Kill all sessions
ferrix kill-server
```

### Session Persistence

Ferrix automatically persists sessions. After a system restart:

```bash
# Restore all sessions
ferrix restore

# Restore specific session
ferrix restore -t development
```

### Session Snapshots

Create checkpoints of your session state:

```bash
# Save a snapshot
ferrix save-snapshot development --name "Before refactoring"

# List snapshots
ferrix list-snapshots

# Restore from snapshot
ferrix restore-snapshot development --snapshot-id abc123

# Export snapshot
ferrix export-snapshot development --output ~/backups/dev.snapshot
```

## Windows and Panes

### Advanced Layouts

Ferrix supports sophisticated pane layouts:

```bash
# In command mode (:)
:layout even-horizontal
:layout even-vertical
:layout main-horizontal
:layout main-vertical
:layout tiled
```

### Custom Splits

```bash
# Split with specific size (percentage)
:split-window -h -p 30  # 30% horizontal split
:split-window -v -p 75  # 75% vertical split

# Split with command
:split-window -h -c "npm run watch"
```

### Window Groups

Organize windows into groups:

```bash
:new-window -n frontend -g web
:new-window -n backend -g web
:new-window -n database -g infra
:switch-group web
```

## Copy Mode

Ferrix features a powerful vim-style copy mode:

### Entering Copy Mode
- `Ctrl-b [` - Enter copy mode
- `Ctrl-b PgUp` - Enter copy mode and scroll up

### Navigation (Vim-style)
- `h/j/k/l` - Move cursor left/down/up/right
- `w/b` - Move word forward/backward
- `0/$` - Move to line start/end
- `gg/G` - Go to top/bottom
- `Ctrl-f/Ctrl-b` - Page down/up
- `Ctrl-d/Ctrl-u` - Half page down/up

### Selection
- `v` - Start character selection
- `V` - Start line selection
- `Ctrl-v` - Start block selection
- `Space` - Toggle selection
- `Esc` - Cancel selection

### Search
- `/` - Search forward
- `?` - Search backward
- `n/N` - Next/previous match

### Yank and Paste
- `y` - Yank selection
- `Enter` - Copy selection and exit
- `q` - Exit copy mode
- `Ctrl-b ]` - Paste yanked text

## Configuration

### Configuration File

Ferrix uses `~/.ferrix/config.toml` for configuration:

```toml
# ~/.ferrix/config.toml

# General settings
prefix = "C-b"              # Prefix key
mouse = true                # Enable mouse support
base_index = 1              # Start window numbering at 1
escape_time = 10            # Escape key delay (ms)
history_limit = 10000       # Scrollback lines

# Display
status_position = "bottom"  # Status bar position
status_interval = 1         # Status refresh interval (seconds)
clock_mode = "24"           # 24-hour clock

# Copy mode
copy_mode_style = "vim"     # vim or emacs
copy_mode_highlight = true  # Highlight search matches

# Colors
[colors]
status_bg = "black"
status_fg = "white"
active_border = "green"
inactive_border = "gray"

# Plugins
[[plugins]]
name = "git-status"
path = "~/.ferrix/plugins/git-status.wasm"
enabled = true
```

### Ferrixrc File

For complex configurations, use `~/.ferrixrc`:

```bash
# ~/.ferrixrc

# Set options
set -g prefix C-a
set -g mouse on
set -g status-position top
set -g base-index 1
set -g pane-base-index 1

# Key bindings
bind c new-window -c "#{pane_current_path}"
bind % split-window -h -c "#{pane_current_path}"
bind '"' split-window -v -c "#{pane_current_path}"

# Vim-style pane navigation
bind h select-pane -L
bind j select-pane -D
bind k select-pane -U
bind l select-pane -R

# Resize panes
bind -r H resize-pane -L 5
bind -r J resize-pane -D 5
bind -r K resize-pane -U 5
bind -r L resize-pane -R 5

# Custom commands
bind S command-prompt -p "Save session:" "save-snapshot %1"
bind R command-prompt -p "Restore session:" "restore-snapshot %1"

# Hooks
set-hook after-new-window 'display "Window created"'
set-hook after-kill-pane 'display "Pane closed"'

# Status bar
set -g status-left '[#S] '
set -g status-right '%H:%M %d-%b-%y'
set -g window-status-format '#I:#W'
set -g window-status-current-format '#I:#W*'
```

## Remote Sessions

### Setting Up Remote Server

On the server machine:

```bash
# Start remote server with TLS
ferrix server --bind 0.0.0.0:9999 \
  --cert ~/.ferrix/server.crt \
  --key ~/.ferrix/server.key \
  --auth-mode password

# Or with token authentication
ferrix server --bind 0.0.0.0:9999 \
  --auth-mode token \
  --auth-token $SECRET_TOKEN
```

### Connecting to Remote Sessions

From client machine:

```bash
# Connect to remote server
ferrix connect user@remote.host:9999

# Connect with specific session
ferrix connect user@remote.host:9999 -t development

# List remote sessions
ferrix connect user@remote.host:9999 --list
```

### SSH Integration

Ferrix integrates seamlessly with SSH:

```bash
# Start remote session over SSH
ssh user@host -t ferrix new -s remote

# Attach to remote session over SSH
ssh user@host -t ferrix attach -t remote
```

## Plugins

### Installing Plugins

```bash
# Install from repository
ferrix plugin install git-status

# Install from file
ferrix plugin install ~/.ferrix/plugins/custom.wasm

# List installed plugins
ferrix plugin list

# Enable/disable plugin
ferrix plugin enable git-status
ferrix plugin disable git-status
```

### Available Plugins

- **git-status**: Shows git information in status bar
- **system-monitor**: CPU/memory usage display
- **clipboard-sync**: Sync clipboard across sessions
- **autocomplete**: Command autocomplete
- **themes**: Additional color schemes

### Writing Custom Plugins

See the [Developer Documentation](DEVELOPER_GUIDE.md#plugins) for plugin development.

## Advanced Features

### New Features in v0.9.0

#### Activity Monitoring

Monitor and track activity in your panes:

```bash
# Enable activity monitoring
ferrix toggle-activity-monitoring

# Set monitoring for specific pane
ferrix set-activity-monitoring on

# Activity indicators in status bar:
# 🔔 - Bell triggered
# ● - Recent output activity
# ○ - Silence detected (no output for threshold period)
```

#### Pane Synchronization

Send input to all panes in a window simultaneously:

```bash
# Toggle synchronization
Ctrl-b s

# Or via command mode
:toggle-pane-sync

# Set sync status explicitly
:set-pane-sync on
:set-pane-sync off
```

Perfect for running the same command across multiple servers or environments.

#### Session Locking

Lock sessions for read-only viewing:

```bash
# Lock current session
Ctrl-b L

# Or via command
ferrix lock-session

# Unlock session
ferrix unlock-session

# The status bar shows [LOCKED] when session is locked
```

#### Enhanced Window Management

```bash
# Rename current window
Ctrl-b ,

# Zoom current pane to full window
Ctrl-b z

# Activity indicators appear in window list
# Example: 1:editor● 2:logs○ 3:terminal🔔
```

#### Custom Keybindings

Manage and customize your keybindings:

```bash
# List all keybindings
ferrix list-keys

# Add custom keybinding
ferrix bind-key "Ctrl-b S" "save-snapshot"

# Remove keybinding
ferrix unbind-key "Ctrl-b x"

# Export keybindings to file
ferrix export-keys ~/my-bindings.toml

# Import keybindings from file
ferrix import-keys ~/my-bindings.toml

# Reset to defaults
ferrix reset-keys

# Reload from config
ferrix reload-keys
```

Example keybindings configuration:

```toml
# ~/.ferrix/keybindings.toml
[[keybindings]]
key = "Ctrl-b r"
command = "reload-config"
description = "Reload configuration"

[[keybindings]]
key = "Ctrl-b S"
command = "save-snapshot"
description = "Save session snapshot"

[[keybindings]]
key = "Ctrl-b M"
command = "toggle-mouse"
description = "Toggle mouse support"
```

#### Automatic Session Snapshots

Enable automatic saving of your sessions:

```bash
# Enable auto-save with default 5-minute interval
ferrix enable-auto-save

# Set custom interval (10 minutes)
ferrix enable-auto-save --interval 10

# Check auto-save status
ferrix auto-save-status
# Output: Auto-save: Enabled
#         Interval: 5 minutes
#         Last save: 2024-01-20 14:30:15 UTC
#         Next save: 2024-01-20 14:35:15 UTC

# Disable auto-save
ferrix disable-auto-save
```

Auto-saves are stored in `~/.ferrix/auto/` and can be restored like regular snapshots.

#### Optimized Scrollback Buffer

Improved terminal history with efficient memory usage:

```bash
# Configure scrollback buffer size
set history-limit 50000

# Search in scrollback (in copy mode)
Ctrl-b [
/search-term

# The scrollback buffer is optimized for:
# - Fast scrolling performance
# - Efficient memory usage
# - Quick search operations
```

### Session Versioning

Ferrix provides git-like versioning for sessions:

```bash
# Initialize versioning for session
ferrix version init development

# Create a checkpoint
ferrix version commit development -m "Before major refactor"

# View history
ferrix version log development

# Create branch
ferrix version branch development feature-branch

# Switch branches
ferrix version checkout development feature-branch

# Merge branches
ferrix version merge development feature-branch

# Revert to previous state
ferrix version revert development HEAD~1
```

### Scriptable Automation

Create automation scripts:

```bash
#!/bin/bash
# setup-dev.sh

ferrix new -d -s development <<EOF
  new-window -n editor
  send "vim ." Enter
  split-window -h -p 30
  send "npm run watch" Enter
  new-window -n terminal
  new-window -n logs
  send "tail -f app.log" Enter
  select-window -t editor
EOF
```

### Integration with Tools

#### Vim Integration

Add to `~/.vimrc`:

```vim
" Send selection to ferrix pane
vnoremap <leader>s y:!ferrix send -t .1 '<C-r>"'<CR>

" Open file in new ferrix pane
nnoremap <leader>o :!ferrix split-window -h vim %<CR>
```

#### Shell Integration

Add to `~/.bashrc` or `~/.zshrc`:

```bash
# Auto-attach to ferrix
if [ -z "$FERRIX" ]; then
  ferrix attach || ferrix new
fi

# Quick session switcher
fs() {
  session=$(ferrix list | fzf | cut -d: -f1)
  [ -n "$session" ] && ferrix attach -t "$session"
}
```

## Troubleshooting

### Common Issues

**Session won't attach:**
```bash
# Check if server is running
ferrix list

# Force kill stuck session
ferrix kill -t session-name

# Clear socket file
rm /tmp/ferrix-$USER/default
```

**Copy mode not working:**
- Ensure terminal supports the required escape sequences
- Check `$TERM` environment variable
- Try: `export TERM=screen-256color`

**Performance issues:**
```bash
# Reduce history limit
ferrix set -g history-limit 2000

# Disable plugins
ferrix plugin disable --all

# Check system resources
ferrix info
```

### Getting Help

```bash
# Built-in help
ferrix help
ferrix help [command]

# Show keybindings
# (Inside session)
Ctrl-b ?

# Version and build info
ferrix --version --verbose
```

## Tips and Tricks

1. **Quick window switching**: Use `Ctrl-b '` to select window by name
2. **Synchronize panes**: Use `Ctrl-b s` to toggle synchronization across all panes
3. **Save layout**: `Ctrl-b :save-layout my-layout` to save current layout
4. **Monitor activity**: Activity indicators (🔔 ● ○) show automatically in status bar
5. **Pipe pane output**: `Ctrl-b :pipe-pane -o 'cat >> output.log'`
6. **Zoom for focus**: `Ctrl-b z` to zoom current pane, repeat to unzoom
7. **Lock for viewing**: `Ctrl-b L` to lock session in read-only mode
8. **Auto-save peace of mind**: Enable with `ferrix enable-auto-save` for automatic backups
9. **Custom shortcuts**: Use `ferrix bind-key` to create your own keybindings
10. **Rename windows**: `Ctrl-b ,` to give windows meaningful names

## Conclusion

Ferrix brings modern innovation to terminal multiplexing while maintaining the reliability and performance users expect. For more advanced usage and development, see the [Developer Guide](DEVELOPER_GUIDE.md).

## Quick Reference Card

```
Session Management          Window Commands           Pane Commands
------------------          ---------------           -------------
new       Create session    c    Create window       %    Split horizontal
attach    Attach session    n    Next window         "    Split vertical
detach    Detach (Ctrl-d)   p    Previous window     o    Next pane
list      List sessions     0-9  Select window       ↑↓←→ Navigate panes
kill      Kill session      ,    Rename window       z    Zoom pane
                            &    Kill window         x    Kill pane
                            w    List windows        q    Show pane numbers

Copy Mode                   Command Mode              Special
---------                   ------------              -------
[    Enter copy mode       :    Enter command        ?    Show help
v    Visual mode           q    Exit mode            d    Detach
y    Yank selection                                  [    Copy mode
/    Search forward                                  ]    Paste buffer
q    Exit copy mode
```