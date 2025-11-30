# Phase 1 Testing Guide & Results

**Date:** 2025-01-04
**Feature:** Pane-level dirty tracking
**Status:** Ready for testing

---

## Testing Instructions

### Prerequisites
- Ferrix server is running (already started)
- Terminal window ready

### Test 1: Basic Session Creation ✅

**Already verified:** Server starts and responds to commands

```bash
./target/release/ferrix new -s phase1-test
```

**Expected:** Session creates without errors

---

### Test 2: Multiple Panes (Split Testing)

**Objective:** Verify layout changes trigger full redraws

**Steps:**
1. After attaching, split panes:
   - `Ctrl-b %` (vertical split)
   - `Ctrl-b "` (horizontal split)
   - Create 4-6 panes total

**What to observe:**
- ✅ Borders render correctly
- ✅ No visual artifacts
- ✅ Panes are sized correctly
- ✅ No flickering during splits

**How dirty tracking works here:**
- `layout_dirty = true` on each split
- All panes + borders are redrawn (correct behavior)

---

### Test 3: Single Pane Activity (The Big One!)

**Objective:** Verify only dirty panes render

**Steps:**
1. With 4-6 panes open, focus on one pane
2. Type: `echo "test"` and press Enter
3. Type: `ls -la`
4. Run: `cat /etc/hosts`

**What to observe:**
- ✅ Only the active pane should update
- ✅ Other panes should remain completely still
- ✅ Borders shouldn't flicker
- ✅ Status bar updates (shows pane activity)

**How dirty tracking works here:**
- Only the active pane's ID is in `dirty_panes`
- Only that pane's `draw_pane_content()` is called
- **~90% reduction** in render work

**To verify (advanced):**
Open another terminal and run:
```bash
# Monitor CPU usage of Ferrix client
ps aux | grep ferrix | grep -v grep
```
Should be very low (<5% CPU) even with one active pane

---

### Test 4: Idle Terminal (The Ultimate Test!)

**Objective:** Verify zero rendering when idle

**Steps:**
1. With multiple panes open, stop typing
2. Wait 10 seconds without any input
3. Don't move mouse, don't touch keyboard

**What to observe:**
- ✅ Screen is completely still
- ✅ No redraws happening
- ✅ No flickering
- ✅ CPU usage drops to ~0%

**How dirty tracking works here:**
- `dirty_panes` is empty
- `layout_dirty = false`
- `status_bar_dirty = false` (after 1 second)
- `render_layout()` returns immediately
- **100% reduction** in render work!

**To verify:**
```bash
# In another terminal, monitor CPU
top -pid $(pgrep -f "ferrix attach") -stats cpu -n 0 -s 1
```
Should show <1% CPU after going idle

---

### Test 5: Streaming Output

**Objective:** Verify only active pane updates with continuous output

**Steps:**
1. In one pane, run: `while true; do echo $(date); sleep 0.1; done`
2. Watch the output stream
3. Look at other panes

**What to observe:**
- ✅ Streaming pane updates smoothly
- ✅ Other panes remain completely still
- ✅ No flickering in inactive panes
- ✅ Status bar updates occasionally

**How dirty tracking works here:**
- Each `PaneOutput` marks only that pane dirty
- Render throttling ensures max 60 FPS
- Other panes never marked dirty
- **~90% reduction** for 10-pane layout

---

### Test 6: Pane Navigation

**Objective:** Verify focus changes update correctly

**Steps:**
1. Press `Ctrl-b` then arrow keys to switch panes
2. Move between all panes
3. Observe borders

**What to observe:**
- ✅ Active border changes color (focus indicator)
- ✅ Borders redraw cleanly
- ✅ No artifacts left behind

**How dirty tracking works here:**
- Layout change triggers `layout_dirty = true`
- All panes redrawn (correct behavior for focus change)

---

### Test 7: Resize Terminal

**Objective:** Verify window resize works correctly

**Steps:**
1. Drag terminal window to make it larger
2. Drag to make it smaller
3. Try different aspect ratios

**What to observe:**
- ✅ All panes resize proportionally
- ✅ Borders remain correct
- ✅ Content reflows properly
- ✅ No visual glitches

**How dirty tracking works here:**
- Window resize triggers `layout_dirty = true`
- Full redraw (correct behavior)

---

### Test 8: Close Panes

**Objective:** Verify pane closure updates layout

**Steps:**
1. Focus a pane
2. Press `Ctrl-b x` then `y` to close
3. Close several panes
4. Leave one pane remaining

**What to observe:**
- ✅ Layout adjusts correctly
- ✅ Remaining panes resize to fill space
- ✅ Single pane uses full screen (no borders)

**How dirty tracking works here:**
- Pane closure triggers `layout_dirty = true`
- Full redraw (correct behavior)

---

### Test 9: Status Bar Updates

**Objective:** Verify status bar dirty tracking

**Steps:**
1. Watch the status bar (bottom of screen)
2. Note the time display updates
3. Switch windows (if you have multiple)
4. Run commands to trigger messages

**What to observe:**
- ✅ Time updates every second
- ✅ Session name shown correctly
- ✅ Window indicators correct
- ✅ Messages appear when actions happen

**How dirty tracking works here:**
- `status_bar_dirty = true` every second (time update)
- `status_bar_dirty = true` on messages
- Only status bar redraws, not panes

---

## Performance Metrics to Collect

### Before Testing (v1.0 baseline)
Run old version and collect:
- Idle CPU: ~X%
- Active (1 pane): ~Y%
- Streaming (1 pane): ~Z%

### After Testing (v2.0 Phase 1)
**Expected improvements:**
- Idle CPU: ~0% (**100% reduction**)
- Active (1 pane): ~X/10% (**90% reduction**)
- Streaming (1 pane): ~Z/10% (**90% reduction**)

---

## Known Issues to Watch For

### Visual Regressions
- [ ] Check for flickering during normal use
- [ ] Borders should not have artifacts
- [ ] Status bar should not tear
- [ ] Copy mode should render correctly
- [ ] Help overlay should render correctly

### Functional Regressions
- [ ] All key bindings should work
- [ ] Mouse selection should work
- [ ] Pane resizing should work
- [ ] Window switching should work
- [ ] Session detach/attach should work

---

## Test Results

### Test 1: Basic Creation
- Status:
- Notes:

### Test 2: Multiple Panes
- Status:
- Notes:

### Test 3: Single Pane Activity
- Status:
- Notes:
- CPU Usage:

### Test 4: Idle Terminal
- Status:
- Notes:
- CPU Usage:

### Test 5: Streaming Output
- Status:
- Notes:
- CPU Usage:

### Test 6: Pane Navigation
- Status:
- Notes:

### Test 7: Resize Terminal
- Status:
- Notes:

### Test 8: Close Panes
- Status:
- Notes:

### Test 9: Status Bar
- Status:
- Notes:

---

## Overall Assessment

**Visual Quality:** ___/10
**Performance:** ___/10
**Stability:** ___/10

**Ready for Phase 2:** [ ] Yes [ ] No

**Issues Found:**
1.
2.
3.

**Notes:**


---

## How to Run Tests

```bash
# Start server (if not running)
./target/release/ferrix server --foreground &

# Create and attach to test session
./target/release/ferrix new -s phase1-test

# Follow the test steps above

# When done, detach
Ctrl-b d

# Kill session
./target/release/ferrix kill phase1-test

# Stop server
pkill -f "ferrix server"
```

---

*Test guide created: 2025-01-04*
*Tester: _______*
*Date tested: _______*
