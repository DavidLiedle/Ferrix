#!/bin/bash
# Revolutionary Features Test Suite for Ferrix
# Tests collaborative sessions, AI suggestions, and time-travel debugging

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Test results
PASSED=0
FAILED=0

# Helper functions
log_info() {
    echo -e "${YELLOW}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[PASS]${NC} $1"
    ((PASSED++))
}

log_error() {
    echo -e "${RED}[FAIL]${NC} $1"
    ((FAILED++))
}

cleanup() {
    log_info "Cleaning up test environment..."
    pkill -f "ferrix server" 2>/dev/null || true
    rm -f /tmp/ferrix.sock 2>/dev/null || true
    rm -rf /tmp/ferrix-test-* 2>/dev/null || true
}

# Trap cleanup on exit
trap cleanup EXIT

# Build Ferrix
log_info "Building Ferrix with revolutionary features..."
cargo build --release --features "collaborative ai timetravel" || {
    log_error "Failed to build Ferrix"
    exit 1
}

FERRIX="./target/release/ferrix"

# Start server with all features enabled
log_info "Starting Ferrix server with revolutionary features..."
$FERRIX server --foreground --enable-ai --enable-collaboration --enable-timetravel &
SERVER_PID=$!
sleep 2

# Verify server is running
if ! kill -0 $SERVER_PID 2>/dev/null; then
    log_error "Server failed to start"
    exit 1
fi
log_success "Server started successfully"

# Test 1: Collaborative Sessions
log_info "Testing collaborative sessions..."

# Create collaborative session
$FERRIX new -s collab-test --collaborative --detached || {
    log_error "Failed to create collaborative session"
}

# Simulate second user joining
echo "echo 'User 1 was here'" | $FERRIX attach collab-test --collaborative-id user1 &
USER1_PID=$!
sleep 1

echo "echo 'User 2 was here'" | $FERRIX attach collab-test --collaborative-id user2 &
USER2_PID=$!
sleep 1

# Check if both users are in the session
PARTICIPANTS=$($FERRIX list-participants collab-test 2>/dev/null | wc -l)
if [ "$PARTICIPANTS" -ge 2 ]; then
    log_success "Collaborative session: Multiple users connected"
else
    log_error "Collaborative session: Failed to connect multiple users"
fi

# Kill test session
$FERRIX kill collab-test 2>/dev/null || true

# Test 2: AI Command Suggestions
log_info "Testing AI command suggestions..."

# Create session with AI enabled
$FERRIX new -s ai-test --enable-ai --detached || {
    log_error "Failed to create AI-enabled session"
}

# Test command suggestion endpoints
TEST_COMMANDS=(
    "git st"
    "docker ps"
    "cargo bu"
    "npm ins"
    "python3 -m ven"
)

for cmd in "${TEST_COMMANDS[@]}"; do
    SUGGESTIONS=$($FERRIX suggest --session ai-test --partial "$cmd" 2>/dev/null | wc -l)
    if [ "$SUGGESTIONS" -gt 0 ]; then
        log_success "AI suggestions: Got suggestions for '$cmd'"
    else
        log_error "AI suggestions: No suggestions for '$cmd'"
    fi
done

# Test learning from history
echo "git status" | $FERRIX attach ai-test --run-command
echo "git add ." | $FERRIX attach ai-test --run-command
echo "git commit -m 'test'" | $FERRIX attach ai-test --run-command

# Check if AI learned the pattern
LEARNED=$($FERRIX suggest --session ai-test --partial "git" 2>/dev/null | grep -c "commit\|status\|add" || true)
if [ "$LEARNED" -gt 0 ]; then
    log_success "AI suggestions: Learning from command history"
else
    log_error "AI suggestions: Not learning from history"
fi

$FERRIX kill ai-test 2>/dev/null || true

# Test 3: Time-Travel Debugging
log_info "Testing time-travel debugging..."

# Create session with recording enabled
$FERRIX new -s timetravel-test --record --detached || {
    log_error "Failed to create recorded session"
}

# Generate some activity
echo "echo 'Event 1: Starting'" | $FERRIX attach timetravel-test --run-command
sleep 0.5
echo "echo 'Event 2: Processing'" | $FERRIX attach timetravel-test --run-command
sleep 0.5
echo "echo 'Event 3: Error occurred!'" | $FERRIX attach timetravel-test --run-command
sleep 0.5
echo "echo 'Event 4: Attempting recovery'" | $FERRIX attach timetravel-test --run-command
sleep 0.5
echo "echo 'Event 5: Fixed!'" | $FERRIX attach timetravel-test --run-command

# Test playback
EVENTS=$($FERRIX timetravel list-events timetravel-test 2>/dev/null | wc -l)
if [ "$EVENTS" -ge 5 ]; then
    log_success "Time-travel: Recording events successfully"
else
    log_error "Time-travel: Failed to record events"
fi

# Test rewind to specific event
$FERRIX timetravel goto timetravel-test --event 3 2>/dev/null && {
    log_success "Time-travel: Can navigate to specific event"
} || {
    log_error "Time-travel: Failed to navigate to event"
}

# Test bookmark creation
$FERRIX timetravel bookmark timetravel-test --name "error-point" --event 3 2>/dev/null && {
    log_success "Time-travel: Created bookmark successfully"
} || {
    log_error "Time-travel: Failed to create bookmark"
}

# Test analysis
ANALYSIS=$($FERRIX timetravel analyze timetravel-test 2>/dev/null | grep -c "Event" || true)
if [ "$ANALYSIS" -gt 0 ]; then
    log_success "Time-travel: Analysis working"
else
    log_error "Time-travel: Analysis failed"
fi

$FERRIX kill timetravel-test 2>/dev/null || true

# Test 4: Integration of all features
log_info "Testing integration of all revolutionary features..."

# Create a session with everything enabled
$FERRIX new -s integration-test \
    --collaborative \
    --enable-ai \
    --record \
    --detached || {
    log_error "Failed to create integrated session"
}

# Verify all features are active
COLLAB_ENABLED=$($FERRIX info integration-test | grep -c "Collaborative: enabled" || true)
AI_ENABLED=$($FERRIX info integration-test | grep -c "AI Assistant: enabled" || true)
RECORDING=$($FERRIX info integration-test | grep -c "Recording: enabled" || true)

if [ "$COLLAB_ENABLED" -gt 0 ] && [ "$AI_ENABLED" -gt 0 ] && [ "$RECORDING" -gt 0 ]; then
    log_success "Integration: All features working together"
else
    log_error "Integration: Some features not properly integrated"
fi

$FERRIX kill integration-test 2>/dev/null || true

# Test 5: Performance with revolutionary features
log_info "Testing performance impact..."

# Create baseline session
START_TIME=$(date +%s%N)
$FERRIX new -s baseline --detached
$FERRIX kill baseline
END_TIME=$(date +%s%N)
BASELINE_TIME=$((($END_TIME - $START_TIME) / 1000000))

# Create session with all features
START_TIME=$(date +%s%N)
$FERRIX new -s features --collaborative --enable-ai --record --detached
$FERRIX kill features
END_TIME=$(date +%s%N)
FEATURES_TIME=$((($END_TIME - $START_TIME) / 1000000))

# Calculate overhead (should be less than 2x)
OVERHEAD=$(echo "scale=2; $FEATURES_TIME / $BASELINE_TIME" | bc)
if (( $(echo "$OVERHEAD < 2" | bc -l) )); then
    log_success "Performance: Acceptable overhead (${OVERHEAD}x)"
else
    log_error "Performance: High overhead (${OVERHEAD}x)"
fi

# Test 6: Persistence of revolutionary features
log_info "Testing persistence of revolutionary features..."

# Create session with features and snapshot it
$FERRIX new -s persist-test --collaborative --enable-ai --record --detached
echo "test data" | $FERRIX attach persist-test --run-command
$FERRIX save-snapshot persist-test --name "revolutionary-snapshot"

# Kill and restore
$FERRIX kill persist-test
$FERRIX load-snapshot ~/.ferrix/snapshots/persist-test*.snapshot

# Verify features are restored
RESTORED_FEATURES=$($FERRIX info persist-test 2>/dev/null | grep -c "enabled" || true)
if [ "$RESTORED_FEATURES" -ge 3 ]; then
    log_success "Persistence: Revolutionary features restored from snapshot"
else
    log_error "Persistence: Features not properly restored"
fi

$FERRIX kill persist-test 2>/dev/null || true

# Final cleanup
kill $SERVER_PID 2>/dev/null || true

# Print summary
echo ""
echo "========================================="
echo "Revolutionary Features Test Results"
echo "========================================="
echo -e "${GREEN}Passed:${NC} $PASSED"
echo -e "${RED}Failed:${NC} $FAILED"
echo ""

if [ $FAILED -eq 0 ]; then
    echo -e "${GREEN}All revolutionary features are working!${NC}"
    echo "Ferrix truly offers capabilities beyond GNU Screen and tmux!"
    exit 0
else
    echo -e "${RED}Some revolutionary features need attention.${NC}"
    exit 1
fi