#!/bin/bash

echo "Testing Ferrix attach functionality..."
echo "Will attach to test-session, send some commands, and detach"
echo ""

# Use expect or timeout to automatically detach after a few seconds
# For now, we'll just test that the attach command starts
timeout 2 ./target/release/ferrix attach -t test-session << EOF
echo "Hello from Ferrix!"
exit
EOF

echo ""
echo "Test completed. Checking if session still exists..."
./target/release/ferrix list