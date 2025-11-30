#!/bin/bash
# Quick automated test to verify Phase 1 is working

set -e

echo "Quick Phase 1 Test"
echo "=================="
echo ""

# Kill any existing server
pkill -f "ferrix server" 2>/dev/null || true
sleep 1

# Build
echo "Building..."
cargo build --release -q

# Start server in background
echo "Starting server..."
./target/release/ferrix server --foreground &
SERVER_PID=$!
sleep 2

# Cleanup function
cleanup() {
    echo "Cleaning up..."
    kill $SERVER_PID 2>/dev/null || true
    ./target/release/ferrix kill -t test 2>/dev/null || true
    pkill -f "ferrix server" 2>/dev/null || true
}
trap cleanup EXIT

# Create a session
echo "Creating test session..."
./target/release/ferrix new -s test --detached || {
    echo "Failed to create session"
    exit 1
}

# List sessions
echo "Listing sessions..."
./target/release/ferrix list

# Check server is running
if ps -p $SERVER_PID > /dev/null; then
    echo "✓ Server is running"
else
    echo "✗ Server died"
    exit 1
fi

echo ""
echo "✓ Basic functionality working!"
echo ""
echo "To test dirty tracking manually:"
echo "  1. Run: ./target/release/ferrix attach -t test"
echo "  2. Split panes: Ctrl-b % and Ctrl-b \""
echo "  3. Type in one pane and observe others don't flicker"
echo "  4. Leave idle and check CPU usage (should be low)"
echo ""

# Kill the test session
./target/release/ferrix kill test

echo "Test complete!"
