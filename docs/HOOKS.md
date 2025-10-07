# Hooks System

Ferrix supports tmux-style hooks for event-driven automation. Hooks allow you to run commands automatically when specific events occur.

## Overview

Hooks enable you to:
- Automate repetitive tasks
- Customize Ferrix behavior
- Integrate with external tools
- Monitor session activity
- Implement custom workflows

## Hook Types

### Session Hooks

| Hook | When Triggered | Context |
|------|----------------|---------|
| `session-created` | After a new session is created | session_id |
| `session-closed` | After a session is killed | session_id |
| `session-renamed` | After a session is renamed | session_id, old_name, new_name |
| `session-window-changed` | After switching to a different window | session_id, window_id |

### Client Hooks

| Hook | When Triggered | Context |
|------|----------------|---------|
| `client-attached` | After a client attaches to a session | session_id |
| `client-detached` | After a client detaches from a session | session_id |
| `client-resized` | After a client terminal is resized | session_id, cols, rows |
| `client-session-changed` | After a client switches sessions | session_id |

### Window Hooks

| Hook | When Triggered | Context |
|------|----------------|---------|
| `window-created` | After a new window is created | session_id, window_id |
| `window-closed` | After a window is closed | session_id, window_id |
| `window-renamed` | After a window is renamed | session_id, window_id, old_name, new_name |
| `window-linked` | After a window is linked to a session | session_id, window_id |
| `window-unlinked` | After a window is unlinked from a session | session_id, window_id |
| `window-pane-changed` | After switching to a different pane | session_id, window_id, pane_id |

### Pane Hooks

| Hook | When Triggered | Context |
|------|----------------|---------|
| `pane-created` | After a new pane is created | session_id, window_id, pane_id |
| `pane-closed` | After a pane is closed | session_id, window_id, pane_id |
| `pane-focus-in` | After a pane receives focus | session_id, window_id, pane_id |
| `pane-focus-out` | After a pane loses focus | session_id, window_id, pane_id |
| `pane-title-changed` | After a pane's title changes | session_id, window_id, pane_id, title |
| `pane-died` | After a pane's process dies unexpectedly | session_id, window_id, pane_id |
| `pane-exited` | After a pane's process exits normally | session_id, window_id, pane_id, exit_code |
| `pane-mode-changed` | After a pane changes mode (copy, normal) | session_id, window_id, pane_id, mode |
| `pane-set-clipboard` | After a pane sets clipboard content | session_id, window_id, pane_id |

### Layout Hooks

| Hook | When Triggered | Context |
|------|----------------|---------|
| `layout-change` | After the pane layout changes | session_id, window_id |

### Activity Hooks

| Hook | When Triggered | Context |
|------|----------------|---------|
| `alert-activity` | When activity is detected in a window | session_id, window_id |
| `alert-bell` | When a bell is received | session_id, window_id, pane_id |
| `alert-silence` | When silence is detected | session_id, window_id |

### Command Hooks

| Hook | When Triggered | Context |
|------|----------------|---------|
| `after-<command>` | After any command completes | session_id, command, args |

Examples:
- `after-split-window` - After creating a split
- `after-select-pane` - After selecting a pane
- `after-kill-pane` - After killing a pane

## Setting Hooks

### Global Hooks

Global hooks apply to all sessions:

```bash
# Set a global hook
ferrix set-hook -g session-created 'display-message "New session created!"'

# Multiple commands (run sequentially)
ferrix set-hook -g client-attached 'refresh-client ; display-message "Welcome!"'
```

### Session-Specific Hooks

Session-specific hooks only apply to a particular session:

```bash
# Set hook for current session
ferrix set-hook session-created 'run-shell "logger Session created"'

# Set hook for specific session
ferrix set-hook -t my-session pane-created 'display-message "New pane in #{pane_id}"'
```

## Removing Hooks

```bash
# Remove global hook
ferrix set-hook -gu session-created

# Remove session-specific hook
ferrix set-hook -u session-created

# Remove hook from specific session
ferrix set-hook -t my-session -u pane-created
```

## Listing Hooks

```bash
# List all hooks
ferrix show-hooks

# List global hooks only
ferrix show-hooks -g

# List hooks for specific session
ferrix show-hooks -t my-session
```

## Hook Commands

Hook commands can include:
- Ferrix commands (e.g., `display-message`, `refresh-client`)
- Shell commands (via `run-shell`)
- Format variables (e.g., `#{session_name}`, `#{pane_id}`)

### Using Format Variables

Hooks can access context information through format variables:

```bash
# Display session name when attached
ferrix set-hook -g client-attached 'display-message "Attached to #{session_name}"'

# Log pane creation with details
ferrix set-hook pane-created 'run-shell "echo #{pane_id} >> ~/.ferrix/panes.log"'

# Show window info on switch
ferrix set-hook session-window-changed 'display-message "Window #{window_index}: #{window_name}"'
```

### Conditional Hooks

Use format conditionals for smart behavior:

```bash
# Only show message if more than 3 windows
ferrix set-hook window-created 'display-message "#{?window_count>3,Warning: Many windows,}"'

# Different actions based on pane count
ferrix set-hook pane-closed 'run-shell "#{?window_panes==0,ferrix kill-window,}"'
```

## Practical Examples

### 1. Auto-save on Detach

Automatically save session state when detaching:

```bash
ferrix set-hook -g client-detached 'save-snapshot #{session_name}'
```

### 2. Welcome Message

Show a welcome message when attaching to a session:

```bash
ferrix set-hook -g client-attached 'display-message "Welcome to #{session_name}!"'
```

### 3. Pane Creation Logger

Log every pane creation with timestamp:

```bash
ferrix set-hook -g pane-created 'run-shell "echo [#{datetime}] Pane #{pane_id} created >> ~/pane-log.txt"'
```

### 4. Auto-refresh Status Bar

Refresh status bar when session changes:

```bash
ferrix set-hook -g session-window-changed 'refresh-client -S'
```

### 5. Bell Notification

Send desktop notification on bell:

```bash
ferrix set-hook -g alert-bell 'run-shell "notify-send \"Bell in #{session_name}\""'
```

### 6. Auto-rename Window

Automatically rename window based on running command:

```bash
ferrix set-hook -g pane-focus-in 'rename-window "#{pane_current_command}"'
```

### 7. Session Created Logger

Log new session creation:

```bash
ferrix set-hook -g session-created 'run-shell "echo [#{datetime}] Session #{session_name} created >> ~/.ferrix/sessions.log"'
```

### 8. Pane Exit Handler

Run cleanup when pane exits:

```bash
ferrix set-hook pane-exited 'run-shell "cleanup-pane #{pane_id}"'
```

### 9. Auto-layout on Split

Apply layout automatically after splitting:

```bash
ferrix set-hook after-split-window 'select-layout tiled'
```

### 10. Activity Monitor

Track window activity:

```bash
ferrix set-hook -g alert-activity 'display-message "Activity in #{window_name}"'
```

## Configuration File

Hooks can be defined in your configuration file:

```toml
[[hooks.global]]
event = "session-created"
command = "display-message 'Session created!'"

[[hooks.global]]
event = "client-attached"
command = "refresh-client"

[[hooks.session]]
session = "dev-session"
event = "pane-created"
command = "run-shell 'logger Pane created'"
```

## Implementation Status

### ✅ Implemented

- Core hook system (HookManager, HookEvent, HookContext)
- Session lifecycle hooks (created, closed)
- Client hooks (attached, detached, resized)
- Hook triggering with recursion prevention
- Global and session-specific hooks
- Hook storage and retrieval

### 🚧 In Progress

- Hook configuration file support
- Hook command execution (currently logs only)
- Window and pane hook triggers
- Activity and alert hooks

### 📋 Planned

- `set-hook` command implementation
- `show-hooks` command implementation
- Hook format variable expansion
- Hook command parsing
- Integration with all event types

## Technical Details

### Hook Storage

Hooks are stored in the `HookManager`:
- **Global hooks**: Apply to all sessions, stored in a HashMap by event type
- **Session hooks**: Apply to specific sessions, stored per-session

### Hook Triggering

When an event occurs:
1. Event is created with context (session_id, window_id, pane_id, etc.)
2. `HookManager::trigger()` is called
3. Manager retrieves all applicable hooks (global + session-specific)
4. Commands are executed in order
5. Recursion prevention ensures hooks don't trigger themselves

### Performance

- Hooks use async execution to avoid blocking
- Recursion prevention via `executing` flag
- Minimal locking with Arc<RwLock>
- Commands run asynchronously

## Debugging Hooks

Enable tracing to see hook execution:

```bash
RUST_LOG=ferrix::server::hooks=debug ferrix server
```

This shows:
- Hook triggers
- Commands being executed
- Recursion prevention
- Hook errors

## Best Practices

1. **Keep hooks simple**: Complex logic belongs in scripts
2. **Use `run-shell` for external commands**: Safer than direct execution
3. **Test hooks before making them global**: Use session-specific first
4. **Avoid recursion**: Don't trigger the same event in a hook
5. **Use format variables**: Make hooks reusable and context-aware
6. **Log important events**: Use `run-shell` to append to log files
7. **Handle errors**: Hooks that fail don't stop normal operation

## See Also

- [Format Variables](FORMAT_VARIABLES.md) - Available variables for hook commands
- [Configuration Guide](../README.md#configuration) - Configuring hooks in TOML
- [Command Reference](COMMANDS.md) - Commands you can use in hooks
- [tmux Hooks](https://man7.org/linux/man-pages/man1/tmux.1.html#HOOKS) - Original tmux hook system
