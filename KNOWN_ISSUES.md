# Known Issues - Ferrix v0.9.2

## Critical Issues

### 1. Terminal Rendering Issue - "Gobbledygook" Output
**Status**: Identified, partial fix applied
**Severity**: Critical - blocks normal usage
**Affected**: v0.9.2

**Symptoms**:
- When attaching to a session, terminal displays garbled output or appears blank
- Cursor may not be visible
- Shell prompt doesn't appear correctly

**Root Cause**:
The client is operating in TUI mode (with alternate screen and raw mode enabled) but is also receiving and writing raw PTY output directly to stdout. This causes conflicts:

1. Client enters alternate screen mode (`EnterAlternateScreen`)
2. Client enables raw terminal mode (`enable_raw_mode()`)
3. Client hides cursor (`cursor::Hide`)
4. Server sends raw PTY output via `ServerMessage::Output{}`
5. Client writes this directly to stdout (line 817 in `src/client/mod.rs`)
6. Raw ANSI escape codes appear as garbled text in the TUI

**Partial Fix Applied**:
- Added `cursor::Show` after positioning cursor in pane rendering (line 1071)
- This makes the cursor visible, but doesn't fix the output rendering issue

**Complete Fix Required**:
The client needs to operate in one of two modes:

**Option A: Passthrough Mode** (for simple single-pane sessions)
- Don't use alternate screen
- Don't hide cursor
- Write PTY output directly to stdout
- Works like a simple terminal

**Option B: TUI Mode** (for multi-pane/window sessions)
- Use alternate screen
- Render panes with borders
- Only accept `PaneOutput` messages, not `Output` messages
- Parse ANSI codes and render into pane buffers

**Recommended Solution**:
Implement mode detection:
```rust
// In run_attached():
let use_tui_mode = self.current_layout.is_some() &&
                   self.current_layout.as_ref().unwrap().panes.len() > 1;

if use_tui_mode {
    // Enter alternate screen, enable raw mode, use pane rendering
} else {
    // Direct passthrough mode - just relay PTY I/O
}
```

**Workaround**:
None currently available for end users.

**Files Affected**:
- `src/client/mod.rs:814-820` (`handle_output()`)
- `src/client/mod.rs:1416-1420` (direct stdout write in message handler)
- `src/client/mod.rs:196-230` (`run_attached()` initialization)

---

## Medium Priority Issues

### 2. Unused Code Warnings
**Status**: Not fixed
**Severity**: Low - doesn't affect functionality

23 compiler warnings for unused imports, variables, and fields. These are safe to ignore but should be cleaned up for production:
- 7 unused imports
- 4 unused variables
- 3 dead code items
- 7 unused struct fields
- 1 unused must_use Result
- 1 private interface warning

**Fix**: Run `cargo fix --lib -p ferrix` and manually review changes.

---

## Future Enhancements

### 3. Server Startup in Test Environment
**Status**: Known limitation
**Severity**: Low - only affects automated testing

The automated test script has issues with server startup in non-interactive environments. Manual testing works fine.

**Workaround**: Use manual testing procedures from `docs/TESTING.md`

---

## Fixed Issues

### Cursor Not Visible (FIXED in build)
**Status**: ✓ FIXED
**Fixed in**: Current build (unreleased)

**What was fixed**:
Added `cursor::Show` command after cursor positioning in pane rendering (line 1071 of `src/client/mod.rs`).

**Result**:
Cursor is now visible when positioned in panes.

---

## Testing Status

- ✓ Build compiles successfully
- ✓ CLI commands present
- ✓ Version correct (0.9.2)
- ✗ Basic attach/interact workflow (blocked by Issue #1)
- ? Advanced features untested due to Issue #1

---

**Last Updated**: 2025-10-03
**Version**: 0.9.2 (unreleased with fixes)
