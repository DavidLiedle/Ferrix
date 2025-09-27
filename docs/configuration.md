# Ferrix Configuration Guide

## Overview

Ferrix uses a flexible configuration system that supports both modern TOML format and traditional RC format (like tmux/screen). The configuration file is typically located at `~/.ferrixrc`.

## Configuration File Locations

Ferrix searches for configuration in the following order:

1. `$FERRIXRC` environment variable
2. `~/.ferrixrc` (user home directory)
3. `~/.config/ferrix/ferrixrc` (XDG config directory)
4. `/etc/ferrixrc` (system-wide)

## File Formats

### Traditional RC Format

Similar to `.tmux.conf` or `.screenrc`:

```bash
# Comments start with #
set option value
bind key command
hook event command
```

### TOML Format

Modern structured format:

```toml
[settings]
default_shell = "/bin/zsh"
mouse_support = true

[settings.status_bar]
enabled = true
position = "bottom"

[[keybindings]]
key = "c"
command = "new-window"
```

## Core Directives (RC Format)

### set - Configuration Options

```bash
# General settings
set prefix C-a                      # Prefix key (default: C-b)
set default-shell /bin/zsh          # Default shell
set default-directory ~              # Starting directory
set history-limit 50000              # Scrollback buffer size
set mouse on                         # Enable mouse support
set escape-time 0                    # No delay for escape key
set repeat-time 500                  # Repeat time for repeatable commands

# Status bar
set status on                        # Enable status bar
set status-position bottom           # Position: top or bottom
set status-interval 15               # Refresh interval in seconds
set status-left " #S "               # Left side format
set status-right " %H:%M "           # Right side format
set status-style bg=black,fg=white   # Status bar colors

# Window options
set base-index 1                     # Start numbering at 1
set renumber-windows on              # Renumber when closed
set automatic-rename on              # Auto-set window title
set aggressive-resize on             # Resize to smallest client

# Pane options
set pane-border-style fg=white       # Inactive border color
set pane-active-border-style fg=green # Active border color

# Copy mode
set mode-keys vi                     # vi or emacs
set copy-mode-mouse-select on        # Mouse selection
set copy-mode-clipboard on           # System clipboard

# Activity monitoring
set monitor-activity on              # Monitor for activity
set visual-activity off              # Visual notification
set visual-bell on                   # Visual bell
set bell-action any                  # Bell in any window
```

### bind - Key Bindings

```bash
# Window management
bind c new-window -n 'shell'
bind n next-window
bind p previous-window
bind & kill-window
bind , command-prompt -I "#W" "rename-window '%%'"

# Pane management
bind | split-window -h               # Vertical split
bind - split-window -v               # Horizontal split
bind h select-pane -L                # Navigate left
bind j select-pane -D                # Navigate down
bind k select-pane -U                # Navigate up
bind l select-pane -R                # Navigate right

# Pane resizing (repeatable with -r)
bind -r H resize-pane -L 5
bind -r J resize-pane -D 5
bind -r K resize-pane -U 5
bind -r L resize-pane -R 5

# Session management
bind s choose-session                # Session picker
bind $ rename-session                # Rename session
bind S save-snapshot                 # Quick snapshot
bind R load-snapshot                 # Restore snapshot

# Copy mode
bind [ copy-mode                     # Enter copy mode
bind ] paste-buffer                  # Paste buffer
bind v copy-mode \; send -X begin-selection
bind y copy-mode \; send -X copy-selection

# Layouts
bind M-1 select-layout even-horizontal
bind M-2 select-layout even-vertical
bind M-3 select-layout main-horizontal
bind M-4 select-layout main-vertical
bind M-5 select-layout tiled

# Reload config
bind r source-file ~/.ferrixrc \; display "Config reloaded!"
```

### unbind - Remove Key Bindings

```bash
unbind C-b                           # Remove default prefix
unbind %                             # Remove default split
unbind '"'                           # Remove default split
```

### hook - Event Handlers

```bash
# Session hooks
hook after-new-session 'display "Welcome to Ferrix!"'
hook before-session-exit 'save-snapshot --auto'
hook after-session-renamed 'display "Session renamed to #S"'

# Window hooks
hook after-new-window 'display "Window #I created"'
hook after-rename-window 'display "Window renamed to #W"'
hook window-linked 'display "Window linked"'
hook window-unlinked 'display "Window unlinked"'

# Pane hooks
hook after-split-window 'select-layout tiled'
hook pane-died 'display "Pane process exited"'
hook pane-exited 'display "Pane closed with status #{pane_dead_status}"'

# Client hooks
hook client-attached 'display "Client attached"'
hook client-detached 'run-shell "echo Detached at $(date) >> ~/.ferrix.log"'
hook client-session-changed 'display "Switched to #S"'
```

### alias - Command Aliases

```bash
alias ks kill-session
alias kw kill-window
alias kp kill-pane
alias ns new-session
alias nw new-window
alias ss save-snapshot
alias rs restore-snapshot
alias ls list-sessions
alias lw list-windows
alias lp list-panes
```

### source - Include Files

```bash
source-file ~/.ferrix/theme.conf
source-file ~/.ferrix/keybindings.conf
source-file ~/.ferrix/local.conf
```

### run - Startup Commands

```bash
run-shell 'echo "Ferrix started at $(date)" >> ~/.ferrix.log'
run 'ferrix new-window -n monitoring'
run 'ferrix send-keys -t monitoring "htop" C-m'
```

### plugin - Load Plugins

```bash
plugin ferrix-resurrect ~/.ferrix/plugins/resurrect
plugin ferrix-continuum ~/.ferrix/plugins/continuum
plugin ferrix-sensible ~/.ferrix/plugins/sensible
```

## Variables and Formatting

### Status Bar Variables

```bash
# Session
#S              Session name
#{session_id}   Session ID
#{session_windows} Number of windows

# Window
#I              Window index
#W              Window name
#{window_id}    Window ID
#{window_panes} Number of panes

# Pane
#P              Pane index
#{pane_id}      Pane ID
#{pane_current_command} Current command
#{pane_current_path}    Current path

# Time
%H              Hour (24h)
%I              Hour (12h)
%M              Minutes
%S              Seconds
%p              AM/PM
%Y              Year
%m              Month
%d              Day

# System
#H              Hostname
#h              Hostname (short)
#(command)      Shell command output
#{user}         Username
```

### Conditional Formatting

```bash
# Show different colors based on state
#{?client_prefix,#[bg=red]PREFIX,#[bg=green]NORMAL}

# Show icon if activity
#{?window_activity_flag,*,}

# Conditional content
#{?session_attached,(attached),(detached)}
```

## Color Configuration

### Named Colors
- black, red, green, yellow, blue, magenta, cyan, white
- brightblack, brightred, brightgreen, brightyellow
- brightblue, brightmagenta, brightcyan, brightwhite

### Color Numbers
- colour0 to colour255 (256 color palette)

### Hex Colors
- `#RRGGBB` format (requires true color support)

### Style Attributes
- bold, dim, underscore, blink, reverse, hidden, italics, strikethrough

### Examples

```bash
set status-style bg=black,fg=white,bold
set window-status-current-style bg=yellow,fg=black
set pane-border-style fg=colour235
set pane-active-border-style fg=#00ff00,bold
```

## Complete Example Configuration

```bash
# ~/.ferrixrc - Ferrix Configuration File
# =========================================

# General Settings
# ----------------
set prefix C-a                       # GNU Screen style prefix
set default-shell /bin/zsh
set default-directory ~/projects
set history-limit 100000
set mouse on
set escape-time 0
set repeat-time 500

# Display Settings
# ---------------
set status on
set status-position bottom
set status-interval 5
set status-left "#[fg=green,bold]#S #[fg=yellow]▶ "
set status-right "#[fg=cyan]#(whoami)@#h #[fg=yellow]%H:%M:%S "
set status-style bg=colour235,fg=white

# Window Settings
# --------------
set base-index 1
set renumber-windows on
set automatic-rename on
set aggressive-resize on
set window-status-format " #I:#W "
set window-status-current-format " #I:#W* "
set window-status-current-style bg=colour240,fg=white,bold

# Pane Settings
# ------------
set pane-base-index 1
set pane-border-style fg=colour240
set pane-active-border-style fg=green,bold
set display-panes-time 2000
set display-panes-colour colour233
set display-panes-active-colour colour245

# Copy Mode
# ---------
set mode-keys vi
setw -g mode-style bg=yellow,fg=black
set copy-mode-mouse-select on
set copy-mode-clipboard on

# Activity Monitoring
# ------------------
set monitor-activity on
set visual-activity off
set visual-bell on
set bell-action other

# Key Bindings
# -----------
# Unbind defaults
unbind %
unbind '"'

# Reload config
bind r source-file ~/.ferrixrc \; display-message "Config reloaded!"

# Session management
bind N new-session
bind S save-snapshot
bind R command-prompt -p "Load snapshot:" "load-snapshot '%%'"

# Window management
bind c new-window -c "#{pane_current_path}"
bind C new-window
bind , command-prompt -I "#W" "rename-window '%%'"
bind & kill-window

# Pane management
bind | split-window -h -c "#{pane_current_path}"
bind - split-window -v -c "#{pane_current_path}"
bind \\ split-window -h
bind _ split-window -v

# Vim-style pane navigation
bind h select-pane -L
bind j select-pane -D
bind k select-pane -U
bind l select-pane -R

# Pane resizing
bind -r H resize-pane -L 5
bind -r J resize-pane -D 5
bind -r K resize-pane -U 5
bind -r L resize-pane -R 5

# Quick pane selection
bind -r C-h select-window -t :-
bind -r C-l select-window -t :+

# Copy mode
bind [ copy-mode
bind ] paste-buffer
bind v copy-mode \; send -X begin-selection
bind y copy-mode \; send -X copy-selection-and-cancel
bind p paste-buffer

# Toggle features
bind m set mouse \; display "Mouse: #{?mouse,ON,OFF}"
bind b set status \; display "Status bar: #{?status,ON,OFF}"
bind z resize-pane -Z

# Layouts
bind M-1 select-layout even-horizontal
bind M-2 select-layout even-vertical
bind M-3 select-layout main-horizontal
bind M-4 select-layout main-vertical
bind M-5 select-layout tiled
bind Space next-layout

# Hooks
# -----
hook after-new-session 'display-message "Session #S created"'
hook after-new-window 'display-message "Window #I:#W created"'
hook after-kill-pane 'select-layout tiled'
hook client-detached 'save-snapshot --auto'

# Aliases
# -------
alias ks kill-session
alias kw kill-window
alias kp kill-pane
alias ss save-snapshot
alias ls list-sessions

# Auto-save Settings
# -----------------
set auto-save on
set auto-save-interval 300
set auto-save-on-detach on
set auto-save-max-snapshots 20

# Startup Commands
# ---------------
run 'ferrix set-option -g @plugin_dir ~/.ferrix/plugins'
run 'ferrix new-window -n system -d'
run 'ferrix send-keys -t system "htop" C-m'

# Load local overrides if exists
source-file -q ~/.ferrixrc.local
```

## Validation and Testing

```bash
# Generate default config
ferrix generate-config

# Validate configuration
ferrix validate-config

# Test specific config file
ferrix validate-config /path/to/config

# Reload configuration in running session
# Press: Ctrl-b : source-file ~/.ferrixrc
```

## Tips and Best Practices

1. **Start Simple**: Begin with a minimal config and add features gradually
2. **Use Comments**: Document your customizations
3. **Version Control**: Keep your config in git
4. **Modular Config**: Split into multiple files with `source-file`
5. **Test Changes**: Validate before applying
6. **Backup**: Save working configs before major changes
7. **Share**: Learn from other users' configurations

## Migration from tmux/screen

See [Migration Guide](./migration.md) for converting existing configurations.

## Troubleshooting

### Config Not Loading
- Check file exists: `ls -la ~/.ferrixrc`
- Validate syntax: `ferrix validate-config`
- Check permissions: `chmod 644 ~/.ferrixrc`

### Key Bindings Not Working
- Verify prefix key is correct
- Check for conflicts with terminal emulator
- Use `unbind` to remove conflicts

### Colors Not Displaying
- Verify terminal supports colors: `echo $TERM`
- Check true color support: `printf "\x1b[38;2;255;100;0mTRUECOLOR\x1b[0m\n"`

## Further Reading

- [Key Bindings Reference](./keybindings.md)
- [Commands Reference](./commands.md)
- [Advanced Features](./advanced.md)
- [Plugin Development](./plugins.md)