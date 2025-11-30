#!/bin/bash
# Phase 1 Dirty Tracking Test Script
# Tests the pane-level dirty tracking implementation

set -e

echo "==================================================================="
echo "Ferrix v2.0 Phase 1 Test Script"
echo "Testing: Pane-level dirty tracking"
echo "==================================================================="
echo ""

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Build the project
echo -e "${BLUE}Step 1: Building Ferrix...${NC}"
cargo build --release
echo -e "${GREEN}✓ Build successful${NC}"
echo ""

# Kill any existing Ferrix server
echo -e "${BLUE}Step 2: Cleaning up any existing Ferrix server...${NC}"
pkill -f "ferrix server" 2>/dev/null || true
sleep 1
echo -e "${GREEN}✓ Cleanup complete${NC}"
echo ""

# Start the server
echo -e "${BLUE}Step 3: Starting Ferrix server...${NC}"
./target/release/ferrix server --foreground &
SERVER_PID=$!
sleep 2
echo -e "${GREEN}✓ Server started (PID: $SERVER_PID)${NC}"
echo ""

# Function to cleanup on exit
cleanup() {
    echo ""
    echo -e "${YELLOW}Cleaning up...${NC}"
    kill $SERVER_PID 2>/dev/null || true
    pkill -f "ferrix server" 2>/dev/null || true
    ./target/release/ferrix kill -t test-session 2>/dev/null || true
}
trap cleanup EXIT

echo "==================================================================="
echo "MANUAL TEST INSTRUCTIONS"
echo "==================================================================="
echo ""
echo "The server is running. Now run the following commands in another"
echo "terminal to test the dirty tracking:"
echo ""
echo -e "${BLUE}Test 1: Create a session with multiple panes${NC}"
echo "  ./target/release/ferrix new -s test-session"
echo ""
echo -e "${BLUE}Test 2: Split the pane multiple times${NC}"
echo "  Press: Ctrl-b % (vertical split)"
echo "  Press: Ctrl-b \" (horizontal split)"
echo "  Expected: Clean layout rendering, no flickering"
echo ""
echo -e "${BLUE}Test 3: Type in one pane${NC}"
echo "  Navigate: Ctrl-b arrow keys"
echo "  Type: Some text, run commands (ls, echo, etc.)"
echo "  Expected: Only the active pane should update"
echo ""
echo -e "${BLUE}Test 4: Leave terminal idle${NC}"
echo "  Do nothing for 10 seconds"
echo "  Expected: No screen updates, CPU should be low"
echo ""
echo -e "${BLUE}Test 5: Run streaming output in one pane${NC}"
echo "  In one pane: tail -f /var/log/system.log"
echo "  Or: while true; do echo \$(date); sleep 1; done"
echo "  Expected: Only that pane updates, others stay still"
echo ""
echo -e "${BLUE}Test 6: Close a pane${NC}"
echo "  Press: Ctrl-b x (then y to confirm)"
echo "  Expected: Clean layout redraw"
echo ""
echo -e "${BLUE}Test 7: Resize the terminal window${NC}"
echo "  Drag terminal window to resize"
echo "  Expected: All panes resize correctly"
echo ""
echo "==================================================================="
echo ""

# Wait for user to test
echo -e "${YELLOW}Press Ctrl-C when done testing...${NC}"
wait $SERVER_PID
