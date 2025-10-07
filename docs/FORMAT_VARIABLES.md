# Format Variables

Ferrix supports tmux-style format strings with `#{variable}` syntax for dynamic status bars and scriptable workflows.

## Syntax

```
#{variable_name}                    - Basic variable expansion
#{variable:modifier}                - Variable with modifier
#{?condition,true,false}            - Conditional format
##                                  - Literal '#' character
```

### Conditional Formats

```
#{?variable,true_value,false_value}           - Simple boolean check
#{?var==value,true,false}                     - String comparison
#{?var>5,high,low}                            - Numeric comparison
#{?count>=10,#{icon},}                        - Nested expansion
```

**Supported Operators:**
- `==` - Equals
- `!=` - Not equals
- `>` - Greater than
- `<` - Less than
- `>=` - Greater than or equal
- `<=` - Less than or equal

### Format Modifiers

| Modifier | Description | Example | Result |
|----------|-------------|---------|--------|
| `:p<n>` | Right-pad to n characters | `#{name:p10}` | `"      test"` |
| `:l<n>` | Left-pad to n characters | `#{name:l10}` | `"test      "` |
| `:=<n>` | Trim to n characters | `#{path:=20}` | First 20 chars |
| `:s/old/new/` | String substitution | `#{path:s/home/usr/}` | Replace text |
| `:s\|old\|new\|` | Substitution (alt delimiter) | `#{path:s\|/home\|~\|}` | Replace text |
| `:u` | Uppercase | `#{text:u}` | `"HELLO"` |
| `:d` | Lowercase | `#{text:d}` | `"hello"` |

## Available Variables

### Session Variables

| Variable | Type | Description | Example |
|----------|------|-------------|---------|
| `#{session_name}` | String | Session name | `my-session` |
| `#{session_id}` | String | Session UUID | `550e8400-e29b-41d4-a716-446655440000` |
| `#{session_windows}` | Number | Number of windows in session | `3` |
| `#{session_attached}` | Number | Number of attached clients | `1` |
| `#{session_created}` | Timestamp | Session creation time | `2025-10-07T12:00:00Z` |
| `#{session_locked}` | Boolean | Session locked status | `1` or `0` |
| `#{session_recording}` | Boolean | Recording in progress | `1` or `0` |
| `#{session_layout}` | String | Current layout preset | `single`, `even-horizontal` |
| `#{pane_synchronized}` | Boolean | Pane synchronization enabled | `1` or `0` |

### Window Variables

| Variable | Type | Description | Example |
|----------|------|-------------|---------|
| `#{window_name}` | String | Window name | `bash` |
| `#{window_id}` | String | Window UUID | `550e8400-e29b-41d4-a716-446655440000` |
| `#{window_index}` | Number | Window index | `0` |
| `#{window_count}` | Number | Total windows | `3` |
| `#{window_panes}` | Number | Number of panes in window | `2` |
| `#{window_width}` | Number | Window width in columns | `80` |
| `#{window_height}` | Number | Window height in rows | `24` |
| `#{window_zoomed_flag}` | Boolean | Window has zoomed pane | `1` or `0` |
| `#{window_active}` | Boolean | Is current window | `1` or `0` |
| `#{window_activity_flag}` | Boolean | Window has unseen activity | `1` or `0` |
| `#{window_layout}` | String | Window layout | `Layout { ... }` |

### Pane Variables

| Variable | Type | Description | Example |
|----------|------|-------------|---------|
| `#{pane_id}` | String | Pane UUID | `550e8400-e29b-41d4-a716-446655440000` |
| `#{pane_width}` | Number | Pane width in columns | `80` |
| `#{pane_height}` | Number | Pane height in rows | `24` |
| `#{pane_current_command}` | String | Command running in pane | `/bin/bash` |
| `#{pane_current_path}` | String | Working directory | `/home/user` |
| `#{pane_pid}` | Number | PTY process ID | `12345` |
| `#{pane_active}` | Boolean | Is current pane | `1` or `0` |
| `#{pane_dead}` | Boolean | Pane has no running process | `1` or `0` |
| `#{pane_title}` | String | Pane title | `editor` |
| `#{cursor_x}` | Number | Cursor column position | `10` |
| `#{cursor_y}` | Number | Cursor row position | `5` |
| `#{history_size}` | Number | Scrollback buffer capacity | `10000` |
| `#{history_bytes}` | Number | Memory used by scrollback | `524288` |

### System Variables

| Variable | Type | Description | Example |
|----------|------|-------------|---------|
| `#{host}` | String | Hostname | `my-computer` |
| `#{user}` | String | Current user | `david` |
| `#{time}` | String | Current time | `14:30:45` |
| `#{date}` | String | Current date | `2025-10-07` |
| `#{datetime}` | String | Current datetime (RFC3339) | `2025-10-07T14:30:45+00:00` |
| `#{cpu}` | String | CPU usage | `🟢CPU: 25.3%` |
| `#{memory}` | String | Memory usage | `🟢MEM: 4.2GB/42%` |
| `#{uptime}` | String | System uptime | `2 days, 3:45:12` |
| `#{load}` | String | Load average | `0.45 0.52 0.48` |

### Version Control

| Variable | Type | Description | Example |
|----------|------|-------------|---------|
| `#{git_branch}` | String | Current git branch | `🌿main✓` |

*Requires `versioning` feature for status indicators*

### Battery (Feature: `battery-status`)

| Variable | Type | Description | Example |
|----------|------|-------------|---------|
| `#{battery}` | String | Battery level with icon | `🔋⚡ 85%` |
| `#{battery_percentage}` | Number | Battery percentage | `85` |

## Examples

### Status Bar Configuration

```toml
[status_bar]
enabled = true
position = "bottom"
left = "[#{session_name}] #{windows} #{git_branch}"
center = "#{cpu} #{memory} #{battery}"
right = "#{user}@#{host} #{time}"
```

### Custom Formats

```bash
# Session info
"Session: #{session_name} (#{session_windows} windows)"

# Window list with active indicator
"#{window_index}:#{window_name}#{?window_active,*,}"

# Pane details with truncated path
"#{pane_current_command} in #{pane_current_path:=30}"

# System monitoring with color indicators
"CPU: #{?cpu_usage>80,🔴,#{?cpu_usage>50,🟡,🟢}} #{cpu}"

# Conditional session status
"#{?session_locked,🔒 LOCKED,#{?pane_synchronized,🔗 SYNC,}}"

# Formatted username and hostname
"#{user:u}@#{host:=20}"

# Complex status bar
"[#{session_name:=15}] #{windows} | #{?battery,#{battery},} #{time}"
```

## Legacy Compatibility

Ferrix also supports `{variable}` syntax for backward compatibility:

```
{session} → #{session_name}
{windows} → (formatted window list)
{time}    → #{time}
```

## Advanced Examples

### Dynamic Window List
```bash
# Show window index and name, with '*' for active window
"#{window_index}:#{window_name:=10}#{?window_active,*,}"
```

### CPU Usage with Color Coding
```bash
# Red if >80%, yellow if >50%, green otherwise
"#{?cpu_usage>80,🔴,#{?cpu_usage>50,🟡,🟢}} CPU: #{cpu_usage}%"
```

### Path Abbreviation
```bash
# Replace /home/user with ~ and truncate to 30 chars
"#{pane_current_path:s|/home/user|~|:=30}"
```

### Session Status Flags
```bash
# Show multiple status indicators
"#{?session_locked,🔒,}#{?pane_synchronized,🔗,}#{?session_recording,⏺,}"
```

## See Also

- [FEATURES.md](../FEATURES.md) - Feature flag system
- [Configuration Guide](../README.md#configuration) - Full configuration options
- [tmux Formats](https://github.com/tmux/tmux/wiki/Formats) - Original tmux format system
