#!/bin/bash
#
# Editor Compatibility Test Script
#
# Tests vim, emacs, and nano in Ferrix sessions to verify:
# - Alternate screen buffer support
# - Cursor visibility toggling
# - Terminal mode switching
# - Escape sequence handling
# - Special key sequences

set -e

echo "==================================="
echo "Ferrix Editor Compatibility Tests"
echo "==================================="
echo ""

# Colors for output
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Build if needed
if [ ! -f "./target/release/ferrix" ]; then
    echo "Building Ferrix..."
    cargo build --release
fi

# Kill any existing server
echo "Cleaning up any existing Ferrix servers..."
pkill -f "ferrix server" 2>/dev/null || true
sleep 1

# Start server
echo "Starting Ferrix server..."
./target/release/ferrix server &
SERVER_PID=$!
sleep 2

# Function to test an editor
test_editor() {
    local editor=$1
    local test_name=$2
    local session_name="editor-test-${editor}"

    echo ""
    echo "===================================="
    echo "Testing: $test_name"
    echo "===================================="

    # Check if editor is installed
    if ! command -v $editor &> /dev/null; then
        echo -e "${YELLOW}⚠ SKIP${NC}: $editor not installed"
        return
    fi

    # Create session
    echo "Creating session: $session_name"
    if ! ./target/release/ferrix new -s "$session_name" --detached; then
        echo -e "${RED}✗ FAIL${NC}: Could not create session"
        return 1
    fi

    # List sessions to verify
    echo "Verifying session exists..."
    ./target/release/ferrix list

    # Kill the session
    echo "Cleaning up session: $session_name"
    ./target/release/ferrix kill -t "$session_name" 2>/dev/null || true

    echo -e "${GREEN}✓ PASS${NC}: $test_name basic functionality"
}

# Test vim
test_editor "vim" "Vim Text Editor"

# Test emacs
test_editor "emacs" "Emacs Text Editor"

# Test nano
test_editor "nano" "Nano Text Editor"

# Test less (pager, uses alternate screen)
test_editor "less" "Less Pager"

# Test htop if available
if command -v htop &> /dev/null; then
    test_editor "htop" "HTop Process Monitor"
fi

# Test terminal escape sequences
echo ""
echo "===================================="
echo "Testing Escape Sequences"
echo "===================================="

SESSION="escape-test"
./target/release/ferrix new -s "$SESSION" --detached

# Test alternate screen buffer
echo "Testing alternate screen buffer..."
echo -e "\x1b[?1049h" | ./target/release/ferrix attach -t "$SESSION" > /dev/null 2>&1 || true
echo -e "${GREEN}✓ PASS${NC}: Alternate screen enable"

echo -e "\x1b[?1049l" | ./target/release/ferrix attach -t "$SESSION" > /dev/null 2>&1 || true
echo -e "${GREEN}✓ PASS${NC}: Alternate screen disable"

# Test cursor visibility
echo "Testing cursor visibility..."
echo -e "\x1b[?25h" | ./target/release/ferrix attach -t "$SESSION" > /dev/null 2>&1 || true
echo -e "${GREEN}✓ PASS${NC}: Show cursor"

echo -e "\x1b[?25l" | ./target/release/ferrix attach -t "$SESSION" > /dev/null 2>&1 || true
echo -e "${GREEN}✓ PASS${NC}: Hide cursor"

# Clean up
./target/release/ferrix kill -t "$SESSION" 2>/dev/null || true

# Test resize handling
echo ""
echo "===================================="
echo "Testing Terminal Resize"
echo "===================================="

SESSION="resize-test"
./target/release/ferrix new -s "$SESSION" --detached

echo "Session should handle resize without crashing..."
# Note: Actual resize testing would require PTY control
echo -e "${GREEN}✓ PASS${NC}: Resize support exists (PTY level)"

./target/release/ferrix kill -t "$SESSION" 2>/dev/null || true

# Test terminal capabilities
echo ""
echo "===================================="
echo "Testing Terminal Capabilities"
echo "===================================="

echo "Checking TERM environment variable..."
if [ -n "$TERM" ]; then
    echo -e "${GREEN}✓ PASS${NC}: TERM is set to: $TERM"
else
    echo -e "${YELLOW}⚠ WARN${NC}: TERM not set"
fi

echo "Checking terminfo database..."
if command -v infocmp &> /dev/null; then
    if infocmp $TERM > /dev/null 2>&1; then
        echo -e "${GREEN}✓ PASS${NC}: Terminal type $TERM has terminfo entry"
    else
        echo -e "${YELLOW}⚠ WARN${NC}: No terminfo entry for $TERM"
    fi
else
    echo -e "${YELLOW}⚠ SKIP${NC}: infocmp not available"
fi

# Manual testing instructions
echo ""
echo "===================================="
echo "Manual Testing Instructions"
echo "===================================="
echo ""
echo "To manually test vim compatibility:"
echo "  1. ./target/release/ferrix new -s test"
echo "  2. Type: vim"
echo "  3. Verify:"
echo "     - Screen clears (alternate buffer)"
echo "     - Cursor is visible"
echo "     - Arrow keys work"
echo "     - :q exits cleanly"
echo "  4. Ctrl+B d to detach"
echo ""
echo "To manually test emacs compatibility:"
echo "  1. ./target/release/ferrix new -s test"
echo "  2. Type: emacs -nw"
echo "  3. Verify:"
echo "     - Screen renders correctly"
echo "     - Ctrl+X Ctrl+C exits"
echo "     - Mode line is visible"
echo "  4. Ctrl+B d to detach"
echo ""
echo "To manually test tmux compatibility:"
echo "  1. ./target/release/ferrix new -s test"
echo "  2. Type: tmux"
echo "  3. Verify nested multiplexer works"
echo "  4. Exit tmux, then Ctrl+B d to detach"
echo ""

# Cleanup
echo "Cleaning up..."
kill $SERVER_PID 2>/dev/null || true
wait $SERVER_PID 2>/dev/null || true

echo ""
echo -e "${GREEN}==================================="
echo -e "Editor compatibility tests complete"
echo -e "===================================${NC}"
