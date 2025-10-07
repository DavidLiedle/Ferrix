#!/bin/bash

# Test script for multiple pane functionality
# This script demonstrates the new pane splitting and navigation features

echo "=== Testing Ferrix Multiple Pane Functionality ==="
echo

echo "1. Listing sessions:"
./target/release/ferrix list
echo

echo "2. Attaching to session to test pane functionality..."
echo "   Note: In the attached session, you can:"
echo "   - Press Ctrl+A then % to split vertically"
echo "   - Press Ctrl+A then \" to split horizontally"
echo "   - Use arrow keys to navigate between panes"
echo "   - Press Ctrl+A then z to zoom/unzoom panes"
echo "   - Press Ctrl+A then x to close current pane"
echo "   - Press Ctrl+A then d to detach"
echo

echo "Attaching in 3 seconds..."
sleep 3

# Attach to the test session
./target/release/ferrix attach test-panes