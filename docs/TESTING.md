# Ferrix Manual Integration Testing Guide

## Overview

This document provides step-by-step manual integration testing procedures for Ferrix. These tests ensure all features work correctly from end to end.

## Prerequisites

```bash
# 1. Build Ferrix in release mode
cargo build --release

# 2. Clean up any existing server instances
pkill -f "ferrix server" || true
rm -f /tmp/ferrix.sock

# 3. Start the server
./target/release/ferrix server --foreground &
SERVER_PID=$!
sleep 2
```

## Test Suite

### Test 1: Basic Session Management

**Objective**: Verify session creation, listing, attach, and detach functionality.

```bash
# Step 1: Create a new session
./target/release/ferrix new -s test-session --detached

# Step 2: Verify session appears in list
./target/release/ferrix list
# Expected: Should show "test-session" with UUID and timestamp

# Step 3: Attach to the session
./target/release/ferrix attach test-session
# Expected: Terminal should enter session with status bar showing "test-session"

# Step 4: Run a command in the session
echo "Hello from Ferrix!"
# Expected: Output displays normally

# Step 5: Detach from session
# Press: Ctrl-b d
# Expected: Return to regular terminal, session still running

# Step 6: Reattach to verify persistence
./target/release/ferrix attach test-session
# Expected: Session state preserved, scrollback intact
```

**Pass Criteria**: ✓ Session creates, appears in list, attaches, detaches, and reattaches successfully.

---

### Test 2: Window Management

**Objective**: Test window creation, navigation, renaming, and window selection by number.

```bash
# Inside test-session from Test 1

# Step 1: Create new windows
# Press: Ctrl-b c (creates window 1)
# Press: Ctrl-b c (creates window 2)
# Press: Ctrl-b c (creates window 3)

# Step 2: Test window navigation
# Press: Ctrl-b n (next window)
# Press: Ctrl-b p (previous window)
# Expected: Status bar updates with current window number

# Step 3: Test window selection by number (v0.9.2 feature)
# Press: Ctrl-b 0
# Expected: Switches to window 0
# Press: Ctrl-b 2
# Expected: Switches to window 2

# Step 4: Rename current window
# Press: Ctrl-b ,
# Type: editor
# Press: Enter
# Expected: Status bar shows "2:editor" instead of "2:shell"

# Step 5: List all windows
# Press: Ctrl-b w
# Expected: Shows list of all windows with their names

# Step 6: Kill a window
# Press: Ctrl-b &
# Type: y
# Expected: Window closes, switches to another window
```

**Pass Criteria**: ✓ Windows create, navigate, rename, select by number, list, and kill successfully.

---

### Test 3: Pane Management and Resizing

**Objective**: Test pane splitting, navigation, and resizing (v0.9.2 feature).

```bash
# Inside test-session from previous tests

# Step 1: Create horizontal split
# Press: Ctrl-b "
# Expected: Window splits horizontally, two panes visible

# Step 2: Create vertical split in bottom pane
# Press: Ctrl-b %
# Expected: Bottom pane splits vertically

# Step 3: Navigate between panes
# Press: Ctrl-b ↑ (up arrow)
# Press: Ctrl-b ↓ (down arrow)
# Press: Ctrl-b → (right arrow)
# Press: Ctrl-b ← (left arrow)
# Expected: Focus moves between panes

# Step 4: Test pane resizing (v0.9.2 feature)
# Manually test resize-pane command via CLI:
# In another terminal:
./target/release/ferrix resize-pane -D 5
# Expected: Current pane shrinks vertically by 5 rows

./target/release/ferrix resize-pane -R 10
# Expected: Current pane grows horizontally by 10 columns

./target/release/ferrix resize-pane -U 3
# Expected: Current pane grows vertically by 3 rows

./target/release/ferrix resize-pane -L 5
# Expected: Current pane shrinks horizontally by 5 columns

# Step 5: Test zoom
# Press: Ctrl-b z
# Expected: Current pane expands to fill window
# Press: Ctrl-b z (again)
# Expected: Pane returns to original size

# Step 6: Kill a pane
# Press: Ctrl-b x
# Type: y
# Expected: Current pane closes
```

**Pass Criteria**: ✓ Panes split, navigate, resize in all directions, zoom/unzoom, and kill successfully.

---

### Test 4: Activity Monitoring

**Objective**: Test activity monitoring features (v0.9.0).

```bash
# Step 1: Enable activity monitoring
./target/release/ferrix toggle-activity-monitoring
# Expected: Confirmation message

# Step 2: Create a window with output activity
# Inside session:
# Press: Ctrl-b c
echo "test output"
seq 1 100
# Expected: Activity indicator (●) appears in status bar for this window

# Step 3: Test bell monitoring
# Press: Ctrl-b c (new window)
echo -e "\a"
# Expected: Bell indicator (🔔) appears in status bar

# Step 4: Test silence monitoring
# Create a window and leave it idle for configured silence threshold
# Expected: Silence indicator (○) appears after threshold

# Step 5: Check monitoring status
./target/release/ferrix set-activity-monitoring on
# Expected: Monitoring confirmed as enabled

# Step 6: Disable monitoring
./target/release/ferrix set-activity-monitoring off
# Expected: Indicators stop appearing
```

**Pass Criteria**: ✓ Activity indicators (●, 🔔, ○) appear correctly based on pane activity.

---

### Test 5: Pane Synchronization

**Objective**: Test synchronized input across panes (v0.9.0).

```bash
# Inside test-session

# Step 1: Create multiple panes
# Press: Ctrl-b "
# Press: Ctrl-b %
# Expected: 3+ panes visible

# Step 2: Enable pane synchronization
./target/release/ferrix toggle-pane-sync
# Expected: Confirmation message, status bar shows sync indicator

# Step 3: Type a command
echo "synchronized"
# Expected: Command appears in ALL panes simultaneously

# Step 4: Verify output in all panes
# Expected: All panes show "synchronized" output

# Step 5: Disable synchronization
./target/release/ferrix set-pane-sync off
# Expected: Typing now only affects active pane

# Step 6: Verify independent input
echo "only in one pane"
# Expected: Only active pane shows output
```

**Pass Criteria**: ✓ Input synchronizes across all panes when enabled, independent when disabled.

---

### Test 6: Session Locking

**Objective**: Test read-only session locking (v0.9.0).

```bash
# Step 1: Lock the session
./target/release/ferrix lock-session
# Expected: Status bar shows [LOCKED]

# Step 2: Attempt to type commands
# Inside session:
echo "this should not work"
# Expected: No input accepted, or read-only mode message

# Step 3: Attempt to create window
# Press: Ctrl-b c
# Expected: Action blocked or ignored

# Step 4: Unlock session
./target/release/ferrix unlock-session
# Expected: [LOCKED] disappears from status bar

# Step 5: Verify normal operation restored
echo "now this works"
# Expected: Command executes normally

# Step 6: Test explicit lock setting
./target/release/ferrix set-session-lock on
./target/release/ferrix set-session-lock off
# Expected: Lock toggles correctly
```

**Pass Criteria**: ✓ Session locks preventing modifications, unlocks restoring full functionality.

---

### Test 7: Keybinding Management

**Objective**: Test custom keybinding features (v0.9.0-v0.9.2).

```bash
# Step 1: List current keybindings
./target/release/ferrix list-keys
# Expected: Displays all current keybindings with descriptions

# Step 2: Export keybindings to custom path (v0.9.2 feature)
./target/release/ferrix export-keys /tmp/my-keys.toml
# Expected: File created at /tmp/my-keys.toml

# Step 3: Verify export file
cat /tmp/my-keys.toml
# Expected: TOML format with all keybindings

# Step 4: Bind a custom key
./target/release/ferrix bind-key "Ctrl-b S" "save-snapshot"
# Expected: Confirmation message

# Step 5: Test the new binding
# Press: Ctrl-b S
# Expected: Save snapshot prompt or action

# Step 6: Unbind a key
./target/release/ferrix unbind-key "Ctrl-b S"
# Expected: Confirmation, key no longer bound

# Step 7: Import keybindings from custom path (v0.9.2 feature)
./target/release/ferrix import-keys /tmp/my-keys.toml
# Expected: Keybindings loaded from file

# Step 8: Reset to defaults
./target/release/ferrix reset-keys
# Expected: All custom bindings removed, defaults restored

# Step 9: Reload from config
./target/release/ferrix reload-keys
# Expected: Keybindings reloaded from ~/.ferrix/keybindings.toml
```

**Pass Criteria**: ✓ Keybindings list, export/import to custom paths, bind, unbind, reset, and reload successfully.

---

### Test 8: Auto-Save Functionality

**Objective**: Test automatic session snapshots (v0.9.0).

```bash
# Step 1: Check initial auto-save status
./target/release/ferrix auto-save-status
# Expected: Shows enabled/disabled status, interval, last save time

# Step 2: Enable auto-save with custom interval
./target/release/ferrix enable-auto-save --interval 2
# Expected: Confirmation, 2-minute interval set

# Step 3: Verify status
./target/release/ferrix auto-save-status
# Expected: Shows "Enabled", "Interval: 2 minutes", next save time

# Step 4: Wait for auto-save to trigger
sleep 130  # Wait 2+ minutes
# Expected: Check ~/.ferrix/auto/ for automatic snapshots

# Step 5: List snapshots to verify auto-save
./target/release/ferrix list-snapshots
# Expected: Shows auto-saved snapshots

# Step 6: Disable auto-save
./target/release/ferrix disable-auto-save
# Expected: Confirmation, auto-saves stop

# Step 7: Verify disabled
./target/release/ferrix auto-save-status
# Expected: Shows "Disabled"
```

**Pass Criteria**: ✓ Auto-save enables, saves at intervals, status reports correctly, and disables successfully.

---

### Test 9: SendKeys Command

**Objective**: Test programmatic key sending (v0.9.2 feature).

```bash
# Step 1: Create a detached session
./target/release/ferrix new -s send-test --detached

# Step 2: Send keys by session name
./target/release/ferrix send-keys send-test "echo 'Hello from send-keys'"
# Expected: Confirmation message

# Step 3: Attach to verify
./target/release/ferrix attach send-test
# Expected: "Hello from send-keys" appears in terminal

# Step 4: Detach and send by session ID
# Press: Ctrl-b d
SESSION_ID=$(./target/release/ferrix list | grep send-test | awk '{print $2}' | tr -d '()')
./target/release/ferrix send-keys "$SESSION_ID" "echo 'Sent by ID'"
# Expected: Confirmation

# Step 5: Attach and verify
./target/release/ferrix attach send-test
# Expected: "Sent by ID" appears

# Step 6: Test with special keys
# Press: Ctrl-b d
./target/release/ferrix send-keys send-test "ls" "-la" "Enter"
# Expected: Keys sent including Enter

# Step 7: Attach to verify command executed
./target/release/ferrix attach send-test
# Expected: Directory listing visible
```

**Pass Criteria**: ✓ SendKeys sends text to sessions by name and ID, executes commands correctly.

---

### Test 10: Copy Mode and Mouse Selection

**Objective**: Test copy mode with mouse selection (v0.9.2 feature).

```bash
# Inside test-session

# Step 1: Generate content for copying
seq 1 100
echo "Test line for copying"

# Step 2: Enter copy mode
# Press: Ctrl-b [
# Expected: Status changes to show copy mode

# Step 3: Navigate with vi keys
# Press: k (up)
# Press: j (down)
# Press: h (left)
# Press: l (right)
# Expected: Cursor moves through scrollback

# Step 4: Search
# Press: /
# Type: Test
# Press: Enter
# Expected: Cursor jumps to "Test" match

# Step 5: Test visual selection
# Press: v (start selection)
# Press: l l l l (expand selection)
# Expected: Text highlights

# Step 6: Copy selection
# Press: y
# Expected: Exits copy mode, selection copied

# Step 7: Paste
# Press: Ctrl-b ]
# Expected: Copied text pastes into terminal

# Step 8: Test mouse selection (v0.9.2 feature)
# Enter copy mode again: Ctrl-b [
# Click and drag with mouse to select text
# Expected: Text selection via mouse works

# Step 9: Exit copy mode
# Press: q
# Expected: Returns to normal mode
```

**Pass Criteria**: ✓ Copy mode navigation, search, selection, yank, paste, and mouse selection all work.

---

### Test 11: Snapshot Persistence

**Objective**: Test snapshot save, restore, and management.

```bash
# Step 1: Create a session with distinct state
./target/release/ferrix new -s snapshot-test --detached
./target/release/ferrix send-keys snapshot-test "export TEST_VAR=snapshot_value"
./target/release/ferrix send-keys snapshot-test "echo \$TEST_VAR"

# Step 2: Save snapshot with metadata
./target/release/ferrix save-snapshot snapshot-test --name "Test State" --description "Testing snapshot functionality"
# Expected: Confirmation with snapshot path

# Step 3: List snapshots
./target/release/ferrix list-snapshots
# Expected: Shows "Test State" snapshot with metadata

# Step 4: Kill the session
./target/release/ferrix kill snapshot-test
# Expected: Session terminated

# Step 5: Verify session is gone
./target/release/ferrix list
# Expected: snapshot-test not in list

# Step 6: Restore from snapshot
SNAPSHOT_PATH=$(./target/release/ferrix list-snapshots | grep "Test State" | awk '{print $NF}')
./target/release/ferrix load-snapshot "$SNAPSHOT_PATH"
# Expected: Session restored

# Step 7: Attach and verify state
./target/release/ferrix attach snapshot-test
echo $TEST_VAR
# Expected: Shows "snapshot_value"

# Step 8: Clean up
# Press: Ctrl-b d
./target/release/ferrix delete-snapshot "$SNAPSHOT_PATH"
# Expected: Snapshot deleted
```

**Pass Criteria**: ✓ Snapshots save with metadata, restore complete session state, and delete successfully.

---

### Test 12: Plugin Download

**Objective**: Test HTTP-based plugin download (v0.9.2 feature).

```bash
# Step 1: Create a test HTTP endpoint (mock)
# For testing purposes, use a simple file server or skip if no plugin URL available

# Step 2: Test plugin download command
# Note: This requires an actual plugin URL or mock HTTP server
# Example (with hypothetical URL):
./target/release/ferrix plugin download https://example.com/plugins/test-plugin.wasm
# Expected: Plugin downloads to ~/.ferrix/plugins/

# Step 3: Verify plugin file exists
ls -lh ~/.ferrix/plugins/
# Expected: test-plugin.wasm present with correct size

# Step 4: Verify executable permissions (Unix)
stat ~/.ferrix/plugins/test-plugin.wasm
# Expected: Shows executable permissions (755)

# Step 5: List installed plugins
./target/release/ferrix plugin list
# Expected: Shows test-plugin in list

# Note: Full plugin execution testing requires valid WASM plugins
```

**Pass Criteria**: ✓ Plugin downloads via HTTP, saves with correct permissions, and appears in plugin list.

---

### Test 13: Scrollback Buffer Optimization

**Objective**: Test efficient scrollback handling (v0.9.0).

```bash
# Inside test-session

# Step 1: Generate large scrollback
seq 1 10000
# Expected: Output scrolls smoothly

# Step 2: Enter copy mode
# Press: Ctrl-b [

# Step 3: Jump to top
# Press: gg
# Expected: Instantly jumps to beginning of scrollback

# Step 4: Jump to bottom
# Press: G
# Expected: Instantly jumps to end of scrollback

# Step 5: Page navigation
# Press: Ctrl-f (page down)
# Press: Ctrl-b (page up)
# Expected: Smooth paging through large scrollback

# Step 6: Search in large buffer
# Press: /
# Type: 5000
# Press: Enter
# Expected: Quickly finds and jumps to line 5000

# Step 7: Test half-page scrolling
# Press: Ctrl-d (half page down)
# Press: Ctrl-u (half page up)
# Expected: Smooth navigation

# Step 8: Exit copy mode
# Press: q
```

**Pass Criteria**: ✓ Large scrollback buffers handle efficiently, search is fast, navigation is smooth.

---

### Test 14: Multiple Session Stress Test

**Objective**: Test system stability with multiple concurrent sessions.

```bash
# Step 1: Create 10 detached sessions
for i in {1..10}; do
  ./target/release/ferrix new -s "stress-$i" --detached
done

# Step 2: Verify all sessions created
./target/release/ferrix list | wc -l
# Expected: Shows 10+ sessions

# Step 3: Send commands to all sessions
for i in {1..10}; do
  ./target/release/ferrix send-keys "stress-$i" "echo 'Session $i active'"
done
# Expected: All commands queue successfully

# Step 4: Attach to random sessions
./target/release/ferrix attach stress-5
# Expected: Attaches successfully
# Press: Ctrl-b d

./target/release/ferrix attach stress-3
# Expected: Attaches successfully
# Press: Ctrl-b d

# Step 5: Save snapshots of all sessions
for i in {1..10}; do
  ./target/release/ferrix save-snapshot "stress-$i" --name "stress-snapshot-$i"
done
# Expected: All snapshots save successfully

# Step 6: Kill all stress sessions
for i in {1..10}; do
  ./target/release/ferrix kill "stress-$i"
done
# Expected: All sessions terminate cleanly

# Step 7: Clean up snapshots
for i in {1..10}; do
  SNAPSHOT=$(./target/release/ferrix list-snapshots | grep "stress-snapshot-$i" | awk '{print $NF}')
  [ -n "$SNAPSHOT" ] && ./target/release/ferrix delete-snapshot "$SNAPSHOT"
done
```

**Pass Criteria**: ✓ Multiple sessions create, operate independently, save snapshots, and clean up without errors.

---

### Test 15: Build and Version Verification

**Objective**: Verify build completeness and version information.

```bash
# Step 1: Check version
./target/release/ferrix --version
# Expected: Shows "ferrix 0.9.2"

# Step 2: Display help
./target/release/ferrix --help
# Expected: Shows all command options

# Step 3: List all commands
./target/release/ferrix help
# Expected: Complete command list including v0.9.2 features

# Step 4: Verify binary size
ls -lh ./target/release/ferrix
# Expected: Reasonable binary size (typically 5-10 MB for release build)

# Step 5: Check for compilation warnings
cargo build --release 2>&1 | grep warning
# Expected: No critical warnings, only acceptable minor warnings

# Step 6: Verify all CLI commands exist
./target/release/ferrix resize-pane --help
./target/release/ferrix send-keys --help
./target/release/ferrix export-keys --help
./target/release/ferrix import-keys --help
./target/release/ferrix enable-auto-save --help
# Expected: All commands have help text
```

**Pass Criteria**: ✓ Version correct, all commands present, build clean, binary size reasonable.

---

## Test Execution Checklist

Use this checklist to track your testing progress:

- [ ] Test 1: Basic Session Management
- [ ] Test 2: Window Management
- [ ] Test 3: Pane Management and Resizing
- [ ] Test 4: Activity Monitoring
- [ ] Test 5: Pane Synchronization
- [ ] Test 6: Session Locking
- [ ] Test 7: Keybinding Management
- [ ] Test 8: Auto-Save Functionality
- [ ] Test 9: SendKeys Command
- [ ] Test 10: Copy Mode and Mouse Selection
- [ ] Test 11: Snapshot Persistence
- [ ] Test 12: Plugin Download
- [ ] Test 13: Scrollback Buffer Optimization
- [ ] Test 14: Multiple Session Stress Test
- [ ] Test 15: Build and Version Verification

## Cleanup After Testing

```bash
# Kill all test sessions
./target/release/ferrix list | grep -E "(test-|stress-|send-|snapshot-)" | awk '{print $1}' | while read session; do
  ./target/release/ferrix kill "$session" 2>/dev/null || true
done

# Stop the server
kill $SERVER_PID 2>/dev/null || pkill -f "ferrix server"

# Clean up socket
rm -f /tmp/ferrix.sock

# Clean up test snapshots
rm -f ~/.ferrix/snapshots/*test*.snapshot
rm -f ~/.ferrix/snapshots/*stress*.snapshot

# Clean up test files
rm -f /tmp/my-keys.toml
```

## Reporting Issues

If any test fails, collect the following information:

```bash
# System information
uname -a
echo $TERM
echo $SHELL

# Ferrix version
./target/release/ferrix --version

# Server logs
cat /tmp/ferrix-server.log

# Session list
./target/release/ferrix list
```

Create an issue at: https://github.com/DavidLiedle/Ferrix/issues

Include:
- Test number and name that failed
- Expected behavior
- Actual behavior
- System information
- Relevant log output

## Automated Test Execution Script

Save this as `run_integration_tests.sh`:

```bash
#!/bin/bash
set -e

echo "🧪 Ferrix Manual Integration Test Suite"
echo "========================================"
echo ""

# Color codes
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Test counter
TESTS_PASSED=0
TESTS_FAILED=0

# Build
echo "📦 Building Ferrix..."
cargo build --release || {
  echo -e "${RED}✗ Build failed${NC}"
  exit 1
}

# Clean up
echo "🧹 Cleaning up old processes..."
pkill -f "ferrix server" || true
rm -f /tmp/ferrix.sock
sleep 1

# Start server
echo "🚀 Starting server..."
./target/release/ferrix server > /tmp/ferrix-server.log 2>&1 &
SERVER_PID=$!
sleep 2

# Function to run a test
run_test() {
  local test_name="$1"
  echo ""
  echo -e "${YELLOW}▶ Running: $test_name${NC}"
}

# Function to mark test passed
pass_test() {
  echo -e "${GREEN}✓ PASSED${NC}"
  ((TESTS_PASSED++))
}

# Function to mark test failed
fail_test() {
  local reason="$1"
  echo -e "${RED}✗ FAILED: $reason${NC}"
  ((TESTS_FAILED++))
}

# Test 1: Basic Session Management
run_test "Test 1: Basic Session Management"
./target/release/ferrix new -s test-session --detached && \
./target/release/ferrix list | grep -q "test-session" && \
pass_test || fail_test "Session creation or listing failed"

# Test 2: SendKeys Command
run_test "Test 9: SendKeys Command (v0.9.2)"
./target/release/ferrix new -s send-test --detached && \
./target/release/ferrix send-keys send-test "echo 'test'" && \
pass_test || fail_test "SendKeys command failed"

# Test 3: Snapshot Management
run_test "Test 11: Snapshot Persistence"
./target/release/ferrix save-snapshot test-session --name "Integration Test" && \
./target/release/ferrix list-snapshots | grep -q "Integration Test" && \
pass_test || fail_test "Snapshot save/list failed"

# Test 4: Keybinding Export (v0.9.2)
run_test "Test 7: Keybinding Export (v0.9.2)"
./target/release/ferrix export-keys /tmp/test-keys.toml && \
[ -f /tmp/test-keys.toml ] && \
pass_test || fail_test "Keybinding export failed"

# Test 5: Auto-Save
run_test "Test 8: Auto-Save Status"
./target/release/ferrix auto-save-status && \
pass_test || fail_test "Auto-save status check failed"

# Cleanup
echo ""
echo "🧹 Cleaning up..."
./target/release/ferrix kill test-session 2>/dev/null || true
./target/release/ferrix kill send-test 2>/dev/null || true
kill $SERVER_PID 2>/dev/null || pkill -f "ferrix server"
rm -f /tmp/test-keys.toml

# Summary
echo ""
echo "========================================"
echo -e "${GREEN}Tests Passed: $TESTS_PASSED${NC}"
echo -e "${RED}Tests Failed: $TESTS_FAILED${NC}"
echo ""

if [ $TESTS_FAILED -eq 0 ]; then
  echo -e "${GREEN}✨ All tests passed!${NC}"
  exit 0
else
  echo -e "${RED}❌ Some tests failed${NC}"
  exit 1
fi
```

Make executable:
```bash
chmod +x run_integration_tests.sh
./run_integration_tests.sh
```

## Conclusion

This manual integration test suite covers all major features of Ferrix including the newly implemented v0.9.2 features:
- Pane resizing in all directions
- SendKeys command
- Window selection by number
- Custom path export/import for keybindings
- Copy mode mouse selection
- Plugin download

Successful completion of all tests confirms production readiness of Ferrix v0.9.2.
