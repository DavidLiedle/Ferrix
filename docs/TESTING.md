# Ferrix Testing Guide

## What is a Testing Cycle?

A **testing cycle** in Ferrix refers to the complete workflow of:
1. Starting the server
2. Creating sessions
3. Working with windows and panes
4. Detaching from sessions
5. Reattaching to sessions
6. Testing features
7. Cleaning up

This ensures all core functionality works correctly and sessions persist properly.

## Quick Test Cycle

```bash
# 1. Build Ferrix
cargo build --release

# 2. Start server (in one terminal)
./target/release/ferrix server --foreground

# 3. Run test cycle (in another terminal)
./test_cycle.sh
```

## Complete Manual Testing Cycle

### Step 1: Setup and Build

```bash
# Clone and build
git clone https://github.com/davidliedle/Ferrix
cd Ferrix
cargo build --release

# Add to PATH (optional)
export PATH="$PWD/target/release:$PATH"
```

### Step 2: Start the Server

```bash
# Terminal 1: Start server in foreground (for logging)
./target/release/ferrix server --foreground

# Or start as daemon
./target/release/ferrix server
```

### Step 3: Create Your First Session

```bash
# Terminal 2: Create a session
./target/release/ferrix new -s test-session

# You're now attached to 'test-session'
# You should see a shell prompt
```

### Step 4: Test Basic Commands

Inside the session, test basic functionality:

```bash
# Run some commands
ls -la
echo "Hello from Ferrix!"
pwd

# Test scrollback
seq 1 100
# Press Ctrl-b [ to enter copy mode
# Use vi keys (hjkl) to navigate
# Press q to exit copy mode
```

### Step 5: Window Management

```bash
# Create new window
# Press: Ctrl-b c

# Switch between windows
# Press: Ctrl-b n (next)
# Press: Ctrl-b p (previous)
# Press: Ctrl-b 0 (go to window 0)
# Press: Ctrl-b 1 (go to window 1)

# Rename current window
# Press: Ctrl-b ,
# Type: editor
# Press: Enter

# List windows
# Press: Ctrl-b w
```

### Step 6: Pane Management

```bash
# Split horizontally
# Press: Ctrl-b "

# Split vertically
# Press: Ctrl-b %

# Navigate panes
# Press: Ctrl-b arrow-keys

# Resize panes (if configured)
# Press: Ctrl-b Alt+arrow-keys

# Close current pane
# Press: Ctrl-b x
# Confirm: y

# Zoom pane (toggle full screen)
# Press: Ctrl-b z
```

### Step 7: Detach and Reattach

```bash
# Detach from session
# Press: Ctrl-b d

# You're back at regular terminal
# Session continues running in background

# List sessions
./target/release/ferrix list

# Reattach to session
./target/release/ferrix attach test-session

# You're back in the session with all windows/panes intact
```

### Step 8: Test Persistence Features

#### Save Snapshot

```bash
# From outside session
./target/release/ferrix save-snapshot test-session --name "test-point"

# List snapshots
./target/release/ferrix list-snapshots
```

#### Kill and Restore

```bash
# Kill the session
./target/release/ferrix kill test-session

# Verify it's gone
./target/release/ferrix list

# Restore from snapshot
./target/release/ferrix load-snapshot ~/.ferrix/snapshots/test-session*.snapshot

# Attach to restored session
./target/release/ferrix attach test-session
```

### Step 9: Test Revolutionary Features

#### AI Command Suggestions (when implemented)

```bash
# In session, start typing a command
# Suggestions should appear
cd /
# Should suggest: cd /home, cd /var, etc.

git st
# Should suggest: git status

cargo bu
# Should suggest: cargo build
```

#### Collaborative Sessions

```bash
# Terminal 1: Create collaborative session
./target/release/ferrix new -s collab --collaborative

# Terminal 2: Join the session
./target/release/ferrix attach collab

# Both terminals now share the same session
# Type in one, see it in both!
```

#### Time-Travel Debugging

```bash
# Enable recording
./target/release/ferrix new -s debug --record

# Do some work...
# Make an error...
# Fix it...

# Enter time-travel mode
# Press: Ctrl-b T

# Navigate through time
# Press: h (backward)
# Press: l (forward)
# Press: / (search events)
```

### Step 10: Configuration Testing

```bash
# Generate config
./target/release/ferrix generate-config

# Edit ~/.ferrixrc
vim ~/.ferrixrc

# Add custom settings:
set prefix C-a              # Change prefix to Ctrl-a
set mouse on                # Enable mouse
set status-position top     # Status bar at top

# Validate config
./target/release/ferrix validate-config

# Create new session to test config
./target/release/ferrix new -s config-test
```

### Step 11: Stress Testing

```bash
# Create multiple sessions
for i in {1..5}; do
  ./target/release/ferrix new -s "session-$i" -d
done

# List all
./target/release/ferrix list

# Create many windows in one session
./target/release/ferrix attach session-1
# Press Ctrl-b c repeatedly (create 10+ windows)

# Create many panes
# Press Ctrl-b " and Ctrl-b % repeatedly

# Test with large output
seq 1 1000000

# Test with colored output
ls -la --color=auto /
```

### Step 12: Cleanup

```bash
# Kill all test sessions
./target/release/ferrix list | grep session- | cut -d' ' -f1 | xargs -I{} ./target/release/ferrix kill {}

# Or kill server entirely
pkill -f "ferrix server"

# Clean up socket if needed
rm /tmp/ferrix.sock
```

## Automated Test Script

Create `test_cycle.sh`:

```bash
#!/bin/bash
set -e

echo "🧪 Starting Ferrix Test Cycle"

# Build
echo "📦 Building Ferrix..."
cargo build --release

# Kill any existing server
echo "🔄 Cleaning up old processes..."
pkill -f "ferrix server" || true
sleep 1

# Start server
echo "🚀 Starting server..."
./target/release/ferrix server --foreground &
SERVER_PID=$!
sleep 2

# Function to run ferrix commands
ferrix() {
    ./target/release/ferrix "$@"
}

# Test cycle
echo "✅ Creating session..."
ferrix new -s test-cycle -d

echo "📋 Listing sessions..."
ferrix list

echo "💾 Saving snapshot..."
ferrix save-snapshot test-cycle --name "test-snapshot"

echo "📚 Listing snapshots..."
ferrix list-snapshots

echo "🔥 Killing session..."
ferrix kill test-cycle

echo "♻️ Restoring from snapshot..."
SNAPSHOT=$(ferrix list-snapshots | grep test-snapshot | awk '{print $NF}')
ferrix load-snapshot "$SNAPSHOT"

echo "✅ Verifying restore..."
ferrix list

echo "🧹 Cleanup..."
ferrix kill test-cycle || true
kill $SERVER_PID

echo "✨ Test cycle complete!"
```

Make it executable:
```bash
chmod +x test_cycle.sh
```

## Testing Checklist

- [ ] **Server Operations**
  - [ ] Server starts successfully
  - [ ] Server handles multiple clients
  - [ ] Server survives client disconnection
  - [ ] Socket cleanup works

- [ ] **Session Management**
  - [ ] Create named session
  - [ ] Create unnamed session
  - [ ] List sessions shows correct info
  - [ ] Attach to existing session
  - [ ] Detach with Ctrl-b d
  - [ ] Kill session

- [ ] **Window Operations**
  - [ ] Create new window (Ctrl-b c)
  - [ ] Switch windows (Ctrl-b n/p/0-9)
  - [ ] Rename window (Ctrl-b ,)
  - [ ] Kill window (Ctrl-b &)
  - [ ] List windows (Ctrl-b w)

- [ ] **Pane Operations**
  - [ ] Split horizontal (Ctrl-b ")
  - [ ] Split vertical (Ctrl-b %)
  - [ ] Navigate panes (Ctrl-b arrows)
  - [ ] Resize panes
  - [ ] Close pane (Ctrl-b x)
  - [ ] Zoom pane (Ctrl-b z)

- [ ] **Copy Mode**
  - [ ] Enter copy mode (Ctrl-b [)
  - [ ] Navigate with vi keys
  - [ ] Select text (v)
  - [ ] Copy selection (y)
  - [ ] Paste buffer (Ctrl-b ])

- [ ] **Persistence**
  - [ ] Sessions survive detach
  - [ ] Snapshot save works
  - [ ] Snapshot restore works
  - [ ] Auto-save functions
  - [ ] Crash recovery works

- [ ] **Configuration**
  - [ ] Generate config file
  - [ ] Validate config file
  - [ ] Custom keybindings work
  - [ ] Status bar customization

- [ ] **Revolutionary Features**
  - [ ] Collaborative sessions
  - [ ] AI command suggestions
  - [ ] Time-travel debugging
  - [ ] Session templates

## Troubleshooting Test Failures

### Server Won't Start
```bash
# Check if socket exists
ls -la /tmp/ferrix.sock
rm /tmp/ferrix.sock

# Check if another instance is running
ps aux | grep ferrix
pkill -f ferrix
```

### Can't Attach to Session
```bash
# Verify server is running
ps aux | grep "ferrix server"

# Check sessions exist
./target/release/ferrix list

# Check socket permissions
ls -la /tmp/ferrix.sock
```

### Keybindings Don't Work
```bash
# Check terminal emulator settings
echo $TERM

# Verify config is loaded
./target/release/ferrix validate-config

# Test with default config
mv ~/.ferrixrc ~/.ferrixrc.bak
./target/release/ferrix new -s test
```

### Performance Issues
```bash
# Check system resources
top
htop

# Monitor Ferrix specifically
top -p $(pgrep ferrix)

# Check for memory leaks
valgrind ./target/release/ferrix new -s test
```

## Debugging Tips

### Enable Debug Logging
```bash
# Set debug mode
RUST_LOG=debug ./target/release/ferrix server --foreground

# Or use CLI flag
./target/release/ferrix -d server --foreground
```

### Trace System Calls
```bash
# Linux
strace -f ./target/release/ferrix new -s test

# macOS
sudo dtruss ./target/release/ferrix new -s test
```

### Check Core Dumps
```bash
# Enable core dumps
ulimit -c unlimited

# Run and crash
./target/release/ferrix new -s crash-test

# Analyze core
gdb ./target/release/ferrix core
```

## Performance Testing

### Measure Startup Time
```bash
time ./target/release/ferrix new -s perf-test -d
```

### Measure Latency
```bash
# In session, test input latency
time echo "test command"

# Test output handling
time seq 1 100000
```

### Memory Usage
```bash
# Monitor memory during session
watch -n 1 'ps aux | grep ferrix'

# Create many windows/panes and monitor
for i in {1..20}; do
  ./target/release/ferrix new-window -t test
done
```

## Integration Testing

### With Popular Tools

```bash
# Test with vim
vim test.txt
# Verify: syntax highlighting, mouse support, splits

# Test with htop
htop
# Verify: colors, interaction, refresh

# Test with git
git log --graph --oneline
# Verify: colors, paging, unicode

# Test with Docker
docker ps
docker logs container-id -f
# Verify: streaming output, colors
```

### Terminal Compatibility

Test with different terminals:
- iTerm2 (macOS)
- Terminal.app (macOS)
- GNOME Terminal (Linux)
- Konsole (KDE)
- Alacritty
- Windows Terminal (WSL)

## Report Issues

If you encounter issues during testing:

1. **Collect Information**
   ```bash
   ./target/release/ferrix --version
   uname -a
   echo $TERM
   echo $SHELL
   ```

2. **Capture Logs**
   ```bash
   RUST_LOG=debug ./target/release/ferrix server --foreground 2>&1 | tee ferrix.log
   ```

3. **Create Issue**
   - Go to: https://github.com/davidliedle/Ferrix/issues
   - Include: logs, steps to reproduce, expected vs actual behavior

## Next Steps

After successful testing:
- Read [Configuration Guide](./configuration.md) for customization
- Explore [Advanced Features](./advanced.md)
- Try [Snapshots & Recovery](./snapshots.md)
- Join the community discussions

---

Happy Testing! 🧪🚀