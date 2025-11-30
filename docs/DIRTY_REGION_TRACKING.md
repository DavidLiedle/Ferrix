# Dirty Region Tracking Design

## Current Rendering (v1.0)

### Flow
```
render_layout()
  └─> draw_panes(layout)
       ├─> draw_pane_border(pane1)  [rewrites ALL border cells]
       ├─> draw_pane_content(pane1) [rewrites ALL content cells]
       ├─> draw_pane_border(pane2)
       ├─> draw_pane_content(pane2)
       └─> ...
```

### Problems
1. **Full redraw every frame** - Even if terminal is idle
2. **No change detection** - Redraws unchanged content
3. **Excessive escape sequences** - More bytes than necessary
4. **CPU waste** - Renders ~100% even when idle

### Benchmarks (v1.0 Baseline)
- Single pane (80x24): ~6000 cells/frame
- 10 panes: ~60,000 cells/frame
- Idle terminal: Still redraws everything every 16ms

---

## Proposed Solution: Three-Level Dirty Tracking

### Level 1: Pane-Level Dirty Flags
**Track which panes have changed**

```rust
pub struct ClientState {
    dirty_panes: HashSet<PaneId>,
    layout_dirty: bool,
    status_bar_dirty: bool,
}
```

**When to mark dirty:**
- Pane receives output from PTY
- Pane is resized/moved
- Pane focus changes
- Layout changes (split/close)

**Benefit:** Skip rendering entire panes that haven't changed

---

### Level 2: Cell-Level Dirty Tracking
**Track which cells changed in each pane**

```rust
pub struct AnsiParser {
    // Existing fields...
    screen: Vec<Vec<Cell>>,

    // NEW: Track dirty regions
    dirty_cells: BitSet,     // Bit vector for fast lookup
    dirty_rows: HashSet<u16>, // Rows that have changes
}

impl AnsiParser {
    fn mark_cell_dirty(&mut self, row: u16, col: u16) {
        let index = (row as usize * self.width as usize) + col as usize;
        self.dirty_cells.insert(index);
        self.dirty_rows.insert(row);
    }

    fn clear_dirty(&mut self) {
        self.dirty_cells.clear();
        self.dirty_rows.clear();
    }

    fn get_dirty_regions(&self) -> Vec<DirtyRegion> {
        // Convert dirty cells to minimal rectangles
        // e.g., if cells (5,10) to (5,20) are dirty, return one region
    }
}
```

**When to mark dirty:**
- Character written to screen
- Color/attribute changes
- Cursor moves (only that cell)
- Scroll events (mark scrolled rows)

**Benefit:** Only redraw changed cells, not entire pane

---

### Level 3: Render Caching
**Cache static elements that rarely change**

```rust
pub struct RenderCache {
    // Cache rendered borders (only changes on resize/focus)
    border_cache: HashMap<PaneId, BorderCache>,

    // Cache status bar segments (update independently)
    status_left: Option<String>,
    status_center: Option<String>,
    status_right: Option<String>,
    status_timestamp: Instant,

    // Track last rendered state for diffing
    last_layout: Option<LayoutInfo>,
}

struct BorderCache {
    pane_id: PaneId,
    dimensions: (u16, u16, u16, u16), // x, y, width, height
    is_focused: bool,
    rendered: Vec<u8>, // Pre-rendered ANSI escape sequences
}
```

**When to invalidate:**
- Border: Pane resize, focus change, layout change
- Status bar: Time update (1s), session change, message

**Benefit:** Reuse pre-rendered static elements

---

## Implementation Strategy

### Phase 1: Pane-Level Tracking (Quick Win)
**Effort:** 1-2 days
**Impact:** ~50-70% reduction in render work for idle sessions

```rust
impl Client {
    async fn render_layout_optimized(&mut self) -> Result<()> {
        // Only render dirty panes
        if let Some(layout) = &self.current_layout.clone() {
            for pane in &layout.panes {
                // NEW: Check if pane is dirty before rendering
                if self.dirty_panes.contains(&pane.id) || self.layout_dirty {
                    self.draw_pane_border(pane).await?;
                    self.draw_pane_content(pane).await?;
                    self.dirty_panes.remove(&pane.id);
                }
            }

            // Render status bar only if dirty
            if self.status_bar_dirty {
                self.render_status_bar().await?;
                self.status_bar_dirty = false;
            }

            self.layout_dirty = false;
        }

        Ok(())
    }
}
```

---

### Phase 2: Cell-Level Tracking (Big Win)
**Effort:** 3-4 days
**Impact:** ~90% reduction for incremental output (e.g., typing, logs)

```rust
async fn draw_pane_content_optimized(&mut self, pane: &PaneInfo) -> Result<()> {
    let parser = self.pane_parsers.get(&pane.id)?;

    // NEW: Only render dirty regions
    let dirty_regions = parser.get_dirty_regions();

    if dirty_regions.is_empty() {
        return Ok(()); // Nothing to draw!
    }

    for region in dirty_regions {
        // Render only this rectangle
        for row in region.start_row..=region.end_row {
            execute!(stdout, MoveTo(content_x, content_y + row))?;

            // Render only columns in dirty region
            for col in region.start_col..=region.end_col {
                let cell = &parser.screen[row][col];
                // ... render cell
            }
        }
    }

    // Clear dirty flags
    parser.clear_dirty();

    Ok(())
}
```

---

### Phase 3: Render Caching (Polish)
**Effort:** 2-3 days
**Impact:** ~30% additional reduction + smoother resizes

```rust
async fn draw_pane_border_cached(&mut self, pane: &PaneInfo) -> Result<()> {
    // Check cache
    if let Some(cached) = self.render_cache.border_cache.get(&pane.id) {
        if cached.is_valid_for(pane) {
            // Write cached border directly
            std::io::stdout().write_all(&cached.rendered)?;
            return Ok(());
        }
    }

    // Render and cache
    let mut buffer = Vec::new();
    // ... render border to buffer

    self.render_cache.border_cache.insert(pane.id, BorderCache {
        pane_id: pane.id.clone(),
        dimensions: (pane.x, pane.y, pane.width, pane.height),
        is_focused: pane.is_focused,
        rendered: buffer.clone(),
    });

    std::io::stdout().write_all(&buffer)?;
    Ok(())
}
```

---

## Expected Performance Gains

### Scenario: Idle Terminal (10 panes, 80x24 each)
- **Before:** Redraws 19,200 cells every 16ms = 1.2M cells/second
- **After (Phase 1):** 0 cells (no dirty panes) = **100% reduction**

### Scenario: Typing in one pane
- **Before:** Redraws 19,200 cells
- **After (Phase 2):** Redraws ~10 cells (cursor + characters) = **99.9% reduction**

### Scenario: Tail -f logs (100 lines/sec in one pane)
- **Before:** Redraws 19,200 cells every 16ms
- **After (Phase 2):** Redraws ~1920 cells (scrolled content) = **90% reduction**

### Scenario: Resize window
- **Before:** Redraws all cells
- **After (Phase 3):** Uses cached borders where possible = **~30% faster**

---

## Data Structures

### BitSet for Dirty Cells
```rust
use bit_set::BitSet;

pub struct DirtyCellTracker {
    width: usize,
    height: usize,
    dirty: BitSet,
}

impl DirtyCellTracker {
    fn mark_dirty(&mut self, row: u16, col: u16) {
        let index = (row as usize * self.width) + col as usize;
        self.dirty.insert(index);
    }

    fn is_dirty(&self, row: u16, col: u16) -> bool {
        let index = (row as usize * self.width) + col as usize;
        self.dirty.contains(index)
    }

    // Memory: 1 bit per cell = 80x24 = 2400 bits = 300 bytes
}
```

### Dirty Regions (Optimized for rectangular areas)
```rust
pub struct DirtyRegion {
    start_row: u16,
    end_row: u16,
    start_col: u16,
    end_col: u16,
}

impl DirtyRegion {
    // Merge overlapping/adjacent regions
    fn merge(&self, other: &DirtyRegion) -> Option<DirtyRegion> {
        // ...
    }

    // Calculate area
    fn area(&self) -> usize {
        ((self.end_row - self.start_row + 1) as usize) *
        ((self.end_col - self.start_col + 1) as usize)
    }
}
```

---

## Testing Strategy

### Unit Tests
```rust
#[test]
fn test_dirty_cell_tracking() {
    let mut parser = AnsiParser::new(80, 24);

    // Write character
    parser.process(b"Hello");
    assert!(parser.is_row_dirty(0));
    assert_eq!(parser.dirty_cells.len(), 5); // H, e, l, l, o

    // Clear
    parser.clear_dirty();
    assert_eq!(parser.dirty_cells.len(), 0);
}

#[test]
fn test_region_merging() {
    let r1 = DirtyRegion { start_row: 0, end_row: 5, start_col: 0, end_col: 10 };
    let r2 = DirtyRegion { start_row: 4, end_row: 10, start_col: 0, end_col: 10 };

    let merged = r1.merge(&r2).unwrap();
    assert_eq!(merged.start_row, 0);
    assert_eq!(merged.end_row, 10);
}
```

### Integration Tests
- Terminal output comparison (before/after should be identical)
- Benchmark: measure cells rendered per frame
- Visual inspection: no rendering artifacts

### Benchmarks
```rust
#[bench]
fn bench_dirty_tracking_overhead(b: &mut Bencher) {
    let mut parser = AnsiParser::new(80, 24);
    b.iter(|| {
        parser.mark_cell_dirty(0, 0);
        parser.clear_dirty();
    });
    // Target: < 100ns overhead
}
```

---

## Migration Path

1. **v2.0.0-alpha1**: Implement Phase 1 (pane-level)
   - Add dirty flags
   - Skip clean panes
   - Verify correctness

2. **v2.0.0-alpha2**: Implement Phase 2 (cell-level)
   - Add BitSet tracking to AnsiParser
   - Modify draw_pane_content
   - Benchmark improvements

3. **v2.0.0-beta1**: Implement Phase 3 (caching)
   - Border caching
   - Status bar caching
   - Polish edge cases

---

## Risks & Mitigations

### Risk: Dirty tracking bugs (missed updates)
**Mitigation:**
- Force full redraw on layout changes
- Add debug mode to visualize dirty regions
- Comprehensive integration tests

### Risk: Increased memory usage
**Mitigation:**
- BitSet is compact (1 bit/cell)
- Clear dirty flags after each render
- Limit cache sizes

### Risk: Complexity
**Mitigation:**
- Phased rollout
- Keep old code path available (feature flag)
- Extensive testing

---

## Success Criteria

✅ **Correctness:** Visual output identical to v1.0
✅ **Performance:** 90%+ reduction in CPU for idle terminals
✅ **Memory:** < 10% increase in memory usage
✅ **Compatibility:** No regressions on any terminal
✅ **Tests:** All existing + 50+ new tests pass

---

*Design by: Claude + DavidCanHelp*
*Date: 2025-01-04*
*Status: Ready for Implementation*
