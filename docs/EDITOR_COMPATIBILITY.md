# Editor Compatibility Guide

Ferrix provides comprehensive terminal emulation support for modern text editors and terminal applications through the `portable_pty` library.

## Tested Editors

### ✅ Vim

**Status:** Fully compatible

**Features Verified:**
- Alternate screen buffer (clear screen on entry)
- Cursor visibility toggling
- Raw terminal mode
- Arrow key navigation
- Function keys (F1-F12)
- Escape sequence pass-through
- Terminal resizing (`SIGWINCH`)
- Syntax highlighting
- Visual mode
- Command mode

**Usage:**
```bash
ferrix new -s vim-session
vim myfile.txt
# Edit normally, then:
# :wq to save and quit
# Ctrl+B d to detach from Ferrix session
```

**Known Quirks:**
- None - vim works exactly as in a standard terminal

### ✅ Emacs (Terminal Mode)

**Status:** Fully compatible

**Features Verified:**
- Terminal mode (`emacs -nw`)
- Mode line rendering
- Minibuffer
- Syntax highlighting
- Multiple buffers
- Split windows
- Key bindings (Ctrl+X, Meta/Alt keys)

**Usage:**
```bash
ferrix new -s emacs-session
emacs -nw myfile.txt
# Edit normally, then:
# Ctrl+X Ctrl+C to exit
# Ctrl+B d to detach from Ferrix session
```

**Recommendations:**
- Use `emacs -nw` (no window system) for best compatibility
- GUI Emacs (`emacs` with X11/Wayland) not applicable in terminal multiplexer

### ✅ Nano

**Status:** Fully compatible

**Features Verified:**
- Status bar rendering
- Ctrl key combinations
- File editing
- Search functionality
- Help display

**Usage:**
```bash
ferrix new -s nano-session
nano myfile.txt
# Ctrl+O to save, Ctrl+X to exit
```

**Notes:**
- Simplest editor, excellent compatibility
- Good for testing basic terminal functionality

### ✅ Less (Pager)

**Status:** Fully compatible

**Features Verified:**
- Alternate screen buffer
- Scrolling
- Search functionality
- Quit restoration

**Usage:**
```bash
ferrix new -s less-session
less /var/log/system.log
# q to quit
```

### ✅ HTop

**Status:** Fully compatible

**Features Verified:**
- Real-time process monitoring
- Color rendering
- Mouse support
- Interactive controls

**Usage:**
```bash
ferrix new -s htop-session
htop
# q to quit
```

## Terminal Emulation Details

### Escape Sequences Supported

Ferrix (via `portable_pty`) supports standard VT100/ANSI escape sequences:

#### Cursor Control
- `\x1b[A` - Cursor up
- `\x1b[B` - Cursor down
- `\x1b[C` - Cursor forward
- `\x1b[D` - Cursor back
- `\x1b[H` - Cursor home
- `\x1b[{row};{col}H` - Cursor position

#### Screen Control
- `\x1b[2J` - Clear screen
- `\x1b[K` - Clear line
- `\x1b[?1049h` - Enable alternate screen buffer
- `\x1b[?1049l` - Disable alternate screen buffer

#### Cursor Visibility
- `\x1b[?25h` - Show cursor
- `\x1b[?25l` - Hide cursor

#### Colors and Attributes
- `\x1b[{n}m` - Set graphics mode (colors, bold, etc.)
- 256-color support
- True color (24-bit RGB) support

### Terminal Type

Ferrix sets `TERM=xterm-256color` by default, which provides:
- 256 color support
- Full terminfo capabilities
- Compatibility with modern terminal applications

### Window Sizing

Ferrix properly handles:
- Initial terminal dimensions (default: 80x24)
- Dynamic resizing via `SIGWINCH`
- `TIOCGWINSZ` ioctl for dimension queries
- Resize propagation to all panes

## Testing Compatibility

### Automated Tests

Run the comprehensive editor compatibility test suite:

```bash
# Basic compatibility tests
./tests/scripts/test_editors.sh

# Rust integration tests (requires editors installed)
cargo test --test editor_compatibility -- --ignored
```

### Manual Testing Checklist

#### Vim
- [ ] `:set number` shows line numbers
- [ ] Syntax highlighting works
- [ ] Arrow keys navigate correctly
- [ ] `dd` deletes line
- [ ] Visual mode (`v`) works
- [ ] Search (`/pattern`) works
- [ ] `:sp` splits window
- [ ] Resize window (Ctrl+W +/-) works

#### Emacs
- [ ] `C-x C-f` opens file
- [ ] `C-x C-s` saves file
- [ ] `C-x b` switches buffers
- [ ] `C-x 2` splits window
- [ ] `M-x` (Meta/Alt+x) works
- [ ] Syntax highlighting active
- [ ] Mode line displays correctly

#### Nano
- [ ] Ctrl+O saves file
- [ ] Ctrl+W searches
- [ ] Ctrl+K cuts line
- [ ] Ctrl+U pastes
- [ ] Help menu (Ctrl+G) displays

## Known Limitations

### Not Supported
- **GUI Editors**: Emacs GUI, gVim, etc. (terminal multiplexer limitation)
- **X11/Wayland**: Cannot forward graphical display
- **Clipboard Integration**: System clipboard requires manual configuration

### Workarounds

#### Clipboard Access
Use Ferrix's built-in copy mode:
```bash
# Enter copy mode
Ctrl+B [

# Navigate with vim keys (h,j,k,l)
# Space to start selection
# Enter to copy
# Ctrl+B ] to paste
```

#### Mouse Support
Enable mouse support in your editor:
```vim
" Vim
:set mouse=a
```

```elisp
; Emacs
(xterm-mouse-mode 1)
```

## Troubleshooting

### Issue: Escape Sequences Visible as Text

**Symptom:** You see `^[[0m` or similar characters instead of colors.

**Solution:**
```bash
# Ensure TERM is set correctly
echo $TERM  # Should show xterm-256color

# If not, add to your shell rc file:
export TERM=xterm-256color
```

### Issue: Arrow Keys Don't Work

**Symptom:** Arrow keys produce letters (A, B, C, D) or beeps.

**Solution:**
```vim
" Add to ~/.vimrc
set nocompatible
```

This ensures vim uses modern terminal key sequences.

### Issue: Colors Look Wrong

**Symptom:** Color scheme doesn't match outside Ferrix.

**Solution:**
```vim
" Force 256 color mode in vim
set t_Co=256
colorscheme molokai  " or your preferred scheme
```

### Issue: Resize Doesn't Work

**Symptom:** Editor doesn't adjust when terminal is resized.

**Solution:**
Ferrix automatically sends `SIGWINCH`. If editor doesn't respond:

```bash
# Manually trigger resize in Ferrix
Ctrl+B :resize-pane -x 120 -y 40
```

## Performance Considerations

### Large Files
- Vim/Emacs handle large files efficiently in Ferrix
- Scrollback buffer: Default 10,000 lines (configurable)
- No performance degradation with syntax highlighting

### Rapid Output
- PTY buffer: 64KB (optimized for throughput)
- Adaptive polling: Faster polling during high throughput
- No dropped output under normal conditions

## Advanced Configuration

### Custom TERM Variable

```bash
# In Ferrix session
export TERM=screen-256color  # For tmux compatibility

# Or xterm-256color (default)
export TERM=xterm-256color
```

### Terminfo Customization

If you need specific terminal capabilities:

```bash
# Export custom terminfo
tic -o ~/.terminfo custom_term.ti

# Use in Ferrix
export TERM=custom_term
```

## Compatibility Matrix

| Editor/App | Support | Alternate Screen | Colors | Mouse | Resize |
|------------|---------|------------------|--------|-------|--------|
| Vim        | ✅      | ✅               | ✅     | ✅    | ✅     |
| Emacs (-nw)| ✅      | ✅               | ✅     | ✅    | ✅     |
| Nano       | ✅      | ✅               | ✅     | ✅    | ✅     |
| Less       | ✅      | ✅               | ✅     | ❌    | ✅     |
| HTop       | ✅      | ✅               | ✅     | ✅    | ✅     |
| Tmux       | ✅      | ✅               | ✅     | ✅    | ✅     |
| Screen     | ✅      | ✅               | ✅     | ⚠️    | ✅     |

Legend:
- ✅ Full support
- ⚠️ Partial support / may require configuration
- ❌ Not supported

## References

- [VT100 Escape Sequences](https://vt100.net/docs/vt100-ug/chapter3.html)
- [ANSI Terminal Colors](https://en.wikipedia.org/wiki/ANSI_escape_code)
- [Terminfo Database](https://man7.org/linux/man-pages/man5/terminfo.5.html)
- [Portable PTY Library](https://docs.rs/portable-pty/latest/portable_pty/)

## Contributing

Found an editor compatibility issue? Please report it with:
1. Editor name and version
2. Steps to reproduce
3. Expected vs actual behavior
4. Output of `echo $TERM`
5. Ferrix version (`ferrix --version`)

Issues can be reported at: https://github.com/davidliedle/Ferrix/issues
