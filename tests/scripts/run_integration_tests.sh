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
