#!/bin/bash

echo "Testing enhanced ANSI support in Ferrix..."

# Kill any existing Ferrix server
pkill -f "ferrix server" 2>/dev/null
sleep 1

# Start the server in the background
echo "Starting Ferrix server..."
./target/release/ferrix server &
SERVER_PID=$!
sleep 2

# Create a test session
echo "Creating test session..."
./target/release/ferrix new -s ansi-test --detached

# Test basic functionality
echo "Testing basic commands..."
echo "ls -la" | ./target/release/ferrix attach -t ansi-test 2>/dev/null | head -20

# List sessions to verify
echo "Listing sessions..."
./target/release/ferrix list

# Clean up
echo "Cleaning up..."
./target/release/ferrix kill -t ansi-test 2>/dev/null
kill $SERVER_PID 2>/dev/null

echo "Test complete!"
echo ""
echo "To test vim support manually:"
echo "1. ./target/release/ferrix server &"
echo "2. ./target/release/ferrix new -s test"
echo "3. Type 'vim' in the session"
echo "4. Check if alternate screen works (vim should clear screen)"
echo "5. Check if cursor visibility toggles properly"
echo "6. Press Ctrl-a d to detach"