# Ferrix Commands Reference

## Command Line Interface

### Global Options

```bash
ferrix [OPTIONS] [COMMAND]

Options:
  -s, --socket <SOCKET>  Socket path (default: /tmp/ferrix.sock)
  -d, --debug           Enable debug output
  -h, --help           Print help
  -V, --version        Print version
```

## Server Commands

### server
Start the Ferrix server

```bash
ferrix server [OPTIONS]

Options:
  -f, --foreground     Run in foreground (don't daemonize)

Examples:
  ferrix server                    # Start server as daemon
  ferrix server --foreground       # Start server in foreground
  ferrix -s /tmp/my.sock server    # Use custom socket
```

## Session Commands

### new (alias: n)
Create a new session

```bash
ferrix new [OPTIONS]

Options:
  -s, --session <NAME>    Session name
  -c, --command <CMD>     Initial command to run
  -d, --detached          Create detached (don't attach)

Examples:
  ferrix new                       # Create and attach unnamed session
  ferrix new -s main               # Create named session
  ferrix new -s dev -d             # Create detached session
  ferrix new -c "vim ~/.ferrixrc"  # Start with command
```

### attach (alias: a)
Attach to an existing session

```bash
ferrix attach [TARGET]

Arguments:
  [TARGET]    Session name or ID (optional, attaches to most recent if omitted)

Examples:
  ferrix attach                    # Attach to most recent session
  ferrix attach main               # Attach by name
  ferrix attach 4c09a048           # Attach by ID prefix
```

### detach (alias: d)
Detach from current session

```bash
ferrix detach

Note: Usually used via keybinding (Ctrl-b d) from within session
```

### list (alias: ls)
List all sessions

```bash
ferrix list

Output format:
  session-name (session-id) - N windows - created at TIMESTAMP

Examples:
  ferrix list
  ferrix ls
```

### kill (alias: k)
Kill a session

```bash
ferrix kill <TARGET>

Arguments:
  <TARGET>    Session name or ID

Examples:
  ferrix kill main                 # Kill by name
  ferrix kill 4c09a048             # Kill by ID
  ferrix kill -a                   # Kill all sessions
```

## Snapshot Commands

### save-snapshot
Save a session snapshot

```bash
ferrix save-snapshot <SESSION> [OPTIONS]

Arguments:
  <SESSION>    Session name or ID

Options:
  -n, --name <NAME>           Snapshot name
  -d, --description <DESC>    Snapshot description

Examples:
  ferrix save-snapshot main
  ferrix save-snapshot main --name "stable"
  ferrix save-snapshot dev -n "before-update" -d "Working state before package updates"
```

### load-snapshot
Load a session from snapshot

```bash
ferrix load-snapshot <PATH>

Arguments:
  <PATH>    Path to snapshot file

Examples:
  ferrix load-snapshot ~/.ferrix/snapshots/main_20240120.snapshot
  ferrix load-snapshot /backup/session.snapshot
```

### list-snapshots
List available snapshots

```bash
ferrix list-snapshots

Output format:
  Created              Name                Size       Path
  2024-01-20 14:30    stable              1.2MB      ~/.ferrix/snapshots/...
```

### delete-snapshot
Delete a snapshot

```bash
ferrix delete-snapshot <PATH>

Arguments:
  <PATH>    Path to snapshot file

Examples:
  ferrix delete-snapshot ~/.ferrix/snapshots/old.snapshot
```

### export-snapshot
Export snapshot to compressed archive

```bash
ferrix export-snapshot <SNAPSHOT> <OUTPUT>

Arguments:
  <SNAPSHOT>    Path to snapshot file
  <OUTPUT>      Path for exported archive

Examples:
  ferrix export-snapshot ~/.ferrix/snapshots/main.snapshot /tmp/backup.gz
```

### import-snapshot
Import snapshot from compressed archive

```bash
ferrix import-snapshot <ARCHIVE>

Arguments:
  <ARCHIVE>    Path to compressed archive

Examples:
  ferrix import-snapshot /tmp/backup.gz
```

## Configuration Commands

### generate-config
Generate a default configuration file

```bash
ferrix generate-config [OPTIONS]

Options:
  -f, --force             Overwrite existing config
  -o, --output <PATH>     Output path (default: ~/.ferrixrc)

Examples:
  ferrix generate-config
  ferrix generate-config --force
  ferrix generate-config -o /etc/ferrixrc
```

### validate-config
Validate configuration file

```bash
ferrix validate-config [PATH]

Arguments:
  [PATH]    Config file path (default: ~/.ferrixrc)

Examples:
  ferrix validate-config
  ferrix validate-config /etc/ferrixrc
```

### reload-config
Reload configuration (not yet implemented)

```bash
ferrix reload-config

Note: Currently use keybinding (Ctrl-b r) or source-file command
```

## Window Commands (Within Session)

These commands are typically used via keybindings within a session:

### new-window
Create a new window

```bash
Keybinding: Ctrl-b c

Command mode: :new-window [OPTIONS]
Options:
  -n <NAME>    Window name
  -c <PATH>    Starting directory

Examples:
  :new-window
  :new-window -n editor
  :new-window -c ~/projects
```

### next-window / previous-window
Navigate between windows

```bash
Keybindings:
  Ctrl-b n    Next window
  Ctrl-b p    Previous window
  Ctrl-b 0-9  Go to window by number
```

### rename-window
Rename current window

```bash
Keybinding: Ctrl-b ,

Command mode: :rename-window <NAME>
```

### kill-window
Kill current window

```bash
Keybinding: Ctrl-b &

Command mode: :kill-window
```

### list-windows
List all windows

```bash
Keybinding: Ctrl-b w

Command mode: :list-windows
```

## Pane Commands (Within Session)

### split-window
Split current pane

```bash
Keybindings:
  Ctrl-b "    Split horizontally
  Ctrl-b %    Split vertically

Command mode:
  :split-window -h    Horizontal split
  :split-window -v    Vertical split
```

### select-pane
Navigate between panes

```bash
Keybindings:
  Ctrl-b arrow-keys    Navigate by direction
  Ctrl-b o             Next pane (cycle)
  Ctrl-b ;             Last active pane

Vi-style (if configured):
  Ctrl-b h    Move left
  Ctrl-b j    Move down
  Ctrl-b k    Move up
  Ctrl-b l    Move right
```

### resize-pane
Resize current pane

```bash
Keybindings (if configured):
  Ctrl-b Alt+arrows    Resize by 5 cells

Command mode:
  :resize-pane -L 5    Resize left
  :resize-pane -R 5    Resize right
  :resize-pane -U 5    Resize up
  :resize-pane -D 5    Resize down
```

### kill-pane
Kill current pane

```bash
Keybinding: Ctrl-b x

Command mode: :kill-pane
```

### zoom-pane
Toggle pane zoom (full window)

```bash
Keybinding: Ctrl-b z

Command mode: :resize-pane -Z
```

## Copy Mode Commands

### Enter Copy Mode

```bash
Keybinding: Ctrl-b [

Once in copy mode:
  Vi-style navigation (if mode-keys vi):
    h,j,k,l     Navigate
    w,b,e       Word movement
    0,$         Line start/end
    g,G         Document start/end
    /,?         Search forward/backward
    v           Start selection
    V           Line selection
    y           Copy selection
    q           Exit copy mode

  Emacs-style (if mode-keys emacs):
    Arrow keys   Navigate
    Ctrl-Space   Start selection
    Alt-w        Copy selection
    Ctrl-g       Exit copy mode
```

### Paste Buffer

```bash
Keybinding: Ctrl-b ]

Command mode: :paste-buffer
```

## Layout Commands

### select-layout
Choose a preset layout

```bash
Keybindings (if configured):
  Ctrl-b Alt-1    Even horizontal
  Ctrl-b Alt-2    Even vertical
  Ctrl-b Alt-3    Main horizontal
  Ctrl-b Alt-4    Main vertical
  Ctrl-b Alt-5    Tiled

Command mode:
  :select-layout even-horizontal
  :select-layout even-vertical
  :select-layout main-horizontal
  :select-layout main-vertical
  :select-layout tiled
```

### next-layout
Cycle through layouts

```bash
Keybinding: Ctrl-b Space

Command mode: :next-layout
```

## Utility Commands

### send-keys
Send keys to a pane

```bash
ferrix send-keys [TARGET] <KEYS>

Arguments:
  [TARGET]    target-session:window.pane
  <KEYS>      Keys to send

Examples:
  ferrix send-keys "hello"
  ferrix send-keys -t dev:0 "ls -la" Enter
  ferrix send-keys -t main:editor.0 C-c
```

### info
Display session information

```bash
ferrix info [TARGET]

Arguments:
  [TARGET]    Session, window, or pane target

Examples:
  ferrix info
  ferrix info main
  ferrix info main:0
```

### source-file
Execute commands from file

```bash
Command mode: :source-file <PATH>

Examples:
  :source-file ~/.ferrixrc
  :source-file ~/scripts/setup.ferrix
```

### display-message
Show a message

```bash
Command mode: :display-message <MESSAGE>

Examples:
  :display-message "Config reloaded"
  :display-message "Session: #S, Window: #W"
```

### command-prompt
Open command prompt

```bash
Keybinding: Ctrl-b :

Opens prompt for entering commands
```

## Target Specification

Many commands accept a target specification:

```
session:window.pane

Examples:
  main            Session 'main'
  main:0          Window 0 in session 'main'
  main:editor     Window 'editor' in session 'main'
  main:0.1        Pane 1 in window 0 of session 'main'
  :0              Window 0 in current session
  .1              Pane 1 in current window
```

## Environment Variables

```bash
FERRIXRC          Path to config file
FERRIX_SOCKET     Default socket path
FERRIX_TMPDIR     Temporary directory
FERRIX_DEBUG      Enable debug output
```

## Exit Codes

```
0    Success
1    General error
2    Session not found
3    Window not found
4    Pane not found
5    Server not running
6    Connection failed
7    Config error
8    Snapshot error
```

## Advanced Usage

### Scripting

```bash
#!/bin/bash
# setup-dev.sh

# Create development session
ferrix new -s dev -d

# Create windows
ferrix new-window -t dev -n editor
ferrix new-window -t dev -n terminal
ferrix new-window -t dev -n logs

# Send commands
ferrix send-keys -t dev:editor "vim ." C-m
ferrix send-keys -t dev:logs "tail -f /var/log/app.log" C-m

# Attach
ferrix attach dev
```

### Aliases

Add to shell config:

```bash
alias f='ferrix'
alias fa='ferrix attach'
alias fn='ferrix new'
alias fl='ferrix list'
alias fs='ferrix save-snapshot'
```

### Integration

With other tools:

```bash
# With fzf for session selection
ferrix attach $(ferrix list | fzf | cut -d' ' -f1)

# With git for automatic snapshots
git config alias.ferrix-snapshot '!ferrix save-snapshot dev --name "git-$(git rev-parse --short HEAD)"'

# With tmuxinator-style session definitions
cat session.yml | ferrix load-config
```