#!/bin/bash

# Test script for layout management in an attached session

FERRIX="./target/release/ferrix"

echo "=== Testing Layout Management in Attached Session ==="
echo ""

# Clean up
echo "Cleaning up existing sessions..."
for session_id in $($FERRIX list 2>/dev/null | awk '{print $1}'); do
    $FERRIX kill "$session_id" 2>/dev/null
done

echo ""
echo "Creating a test session and attaching to test layouts..."
echo "This will open an interactive session. Test the following:"
echo ""
echo "Once attached, try these key combinations:"
echo "  Ctrl-b then 'l' - Should apply the next layout"
echo "  Ctrl-b then '1' - Should switch to window 1"
echo "  Ctrl-b then 'c' - Should create a new window"
echo "  Ctrl-b then 'd' - Detach from session"
echo ""
echo "Press Enter to start the test session..."
read

$FERRIX new -s layout-test

echo ""
echo "Session ended. Cleaning up..."
$FERRIX kill layout-test 2>/dev/null

echo "Test complete!"