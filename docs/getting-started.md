# Getting Started with Ferrix

## Installation

### From Source

```bash
# Clone the repository
git clone https://github.com/davidliedle/Ferrix.git
cd Ferrix

# Build with cargo
cargo build --release

# Install to system
sudo cp target/release/ferrix /usr/local/bin/
```

### Using Cargo

```bash
cargo install ferrix
```

### Package Managers

Coming soon for:
- Homebrew (macOS/Linux)
- APT (Debian/Ubuntu)
- DNF/YUM (Fedora/RHEL)
- Pacman (Arch)

## First Run

### Starting Ferrix

```bash
# Start with default session
ferrix

# Start server only
ferrix server --foreground

# Create named session
ferrix new -s main

# Create detached session
ferrix new -s background -d
```

### Basic Navigation

Once attached to a session:

- **Prefix key**: `Ctrl-b` (default, configurable)
- **Detach**: `Ctrl-b d`
- **Create window**: `Ctrl-b c`
- **Next window**: `Ctrl-b n`
- **Previous window**: `Ctrl-b p`
- **Split horizontal**: `Ctrl-b "`
- **Split vertical**: `Ctrl-b %`

### Generate Configuration

```bash
# Generate default config
ferrix generate-config

# This creates ~/.ferrixrc
# Edit to customize behavior
```

## Core Concepts

### Sessions
A session is a collection of windows that persists even when detached. Sessions can be:
- Created with custom names
- Attached and detached
- Saved as snapshots
- Restored after crashes

### Windows
Windows are full-screen containers within a session. Each window can have:
- Multiple panes
- Custom names
- Independent processes

### Panes
Panes are subdivisions of windows. They can be:
- Split horizontally or vertically
- Resized dynamically
- Navigated with keyboard
- Zoomed to full window

## Quick Examples

### Session Management

```bash
# List all sessions
ferrix list

# Attach to specific session
ferrix attach mysession

# Kill a session
ferrix kill mysession

# Save session snapshot
ferrix save-snapshot mysession --name "before-update"

# Restore from snapshot
ferrix load-snapshot ~/.ferrix/snapshots/mysession_*.snapshot
```

### Window Operations

Inside a session:
```
Ctrl-b c        Create new window
Ctrl-b ,        Rename current window
Ctrl-b &        Kill current window
Ctrl-b 0-9      Switch to window by number
Ctrl-b w        Choose window from list
```

### Pane Operations

```
Ctrl-b "        Split pane horizontally
Ctrl-b %        Split pane vertically
Ctrl-b o        Switch to next pane
Ctrl-b ;        Toggle last active pane
Ctrl-b x        Kill current pane
Ctrl-b z        Toggle pane zoom
Ctrl-b {        Move pane left
Ctrl-b }        Move pane right
```

### Copy Mode

```
Ctrl-b [        Enter copy mode
Ctrl-b ]        Paste from buffer

In copy mode (vi-style):
h,j,k,l         Navigate
v               Start selection
y               Copy selection
/               Search forward
?               Search backward
q               Exit copy mode
```

## Configuration Basics

Create `~/.ferrixrc`:

```bash
# Set prefix key (like GNU Screen)
set prefix C-a

# Enable mouse support
set mouse on

# Set default shell
set default-shell /bin/zsh

# Status bar at top
set status-position top

# Custom key bindings
bind r source-file ~/.ferrixrc
bind | split-window -h
bind - split-window -v
```

## Troubleshooting

### Server Won't Start
```bash
# Check if socket exists
ls -la /tmp/ferrix.sock

# Remove stale socket
rm /tmp/ferrix.sock

# Start with custom socket
ferrix -s /tmp/my-ferrix.sock server
```

### Can't Attach to Session
```bash
# Check running sessions
ferrix list

# Check server status
ps aux | grep ferrix
```

### Configuration Not Loading
```bash
# Validate config file
ferrix validate-config

# Check config location
echo $FERRIXRC
ls -la ~/.ferrixrc
```

## Next Steps

- Read the [Configuration Guide](./configuration.md) for customization
- Learn [Key Bindings](./keybindings.md) for efficiency
- Explore [Advanced Features](./advanced.md) like snapshots
- Check [Commands Reference](./commands.md) for all options

## Getting Help

- Use `ferrix --help` for command help
- Check `man ferrix` for manual page (coming soon)
- Visit [GitHub Issues](https://github.com/davidliedle/Ferrix/issues) for support
- Read the [FAQ](./faq.md) for common questions