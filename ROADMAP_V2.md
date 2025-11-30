# Ferrix v2.0 Roadmap: Terminal-Focused Excellence

**Status:** Planning
**Goal:** Make Ferrix the most stable, feature-rich terminal multiplexer in Rust
**Philosophy:** Stay terminal-based, maximize compatibility, prioritize stability

---

## Executive Summary

v1.0 established Ferrix as a production-ready terminal multiplexer with enterprise-grade reliability. v2.0 will focus on becoming the **best terminal-based multiplexer** by improving the core terminal experience, not by adding GUI features.

**Core Decision:** Ferrix will remain a **terminal multiplexer that runs inside existing terminals** (like tmux/screen), not become a standalone terminal emulator (like Alacritty/WezTerm).

---

## What We Removed (v1.0 → v2.0 Transition)

### Deprecated Features
- ❌ **GPU Rendering** (52KB code removed)
  - Required native OS windows (winit)
  - Incompatible with SSH/remote sessions
  - Not aligned with terminal multiplexer philosophy

- ❌ **Battery Status** (removed due to security vulnerability)
  - Dependency had RUSTSEC-2021-0119 (nix crate)
  - Not core to terminal multiplexing
  - Use system monitoring tools instead

**Impact:**  Cleaner codebase, better focus, no security vulnerabilities

---

## v2.0 Focus Areas

### 1. Terminal Protocol Excellence
**Goal:** Best-in-class terminal emulation within terminals

**Improvements:**
- **Sixel Graphics Protocol** - Inline images in terminal
  - View images, charts, plots directly in panes
  - Compatible with many modern terminals
- **Kitty Graphics Protocol** - Advanced graphics support
  - Better performance than Sixel
  - Supports animations
- **True Color Improvements**
  - Better 24-bit color handling
  - Color scheme validation
- **Unicode & Emoji**
  - Proper Unicode width handling (full-width chars)
  - Nerd Fonts support for icons
  - Emoji rendering improvements

**Why:** Modern terminals support these - Ferrix should too

---

### 2. Performance & Responsiveness
**Goal:** Imperceptible latency, smooth experience

**Improvements:**
- **Dirty Region Tracking**
  - Only redraw changed cells
  - Reduce CPU usage by ~70%
- **Render Caching**
  - Cache static elements (borders, status bar)
  - Reuse when unchanged
- **Adaptive Batching**
  - Smart output buffering
  - Balance latency vs throughput
- **Zero-Copy Optimizations**
  - Minimize memory allocations
  - Direct buffer passing where possible

**Target Metrics:**
- Keystroke latency: < 1ms (v1.0 baseline)
- Large output (10MB): < 100ms
- 100 pane rendering: < 5ms

---

### 3. Enhanced Visual Feedback
**Goal:** Modern UX within terminal constraints

**Improvements:**
- **Better Status Bar**
  - Conditional segments (show/hide based on state)
  - More variables (git branch, k8s context, custom scripts)
  - Refresh optimization (only update changed segments)
- **Mode Indicators**
  - Visual feedback for copy/command/help modes
  - Non-intrusive overlays
- **Progress Indicators**
  - Loading spinners for slow operations
  - Copy mode search progress
- **Message System Improvements**
  - Color-coded by severity (already implemented)
  - Better positioning options
  - Fade-out animations (using terminal features)

**Why:** Users should always know what state they're in

---

### 4. Advanced Copy Mode
**Goal:** The best copy/search experience in any multiplexer

**Improvements:**
- **Regex Search**
  - Full regex support (not just literal)
  - Syntax highlighting for matches
- **Multiple Cursors** (experimental)
  - Select multiple regions
  - Copy all at once
- **Visual Block Mode**
  - Column selection
  - Rectangle operations
- **Smart Selection**
  - Double-click words
  - Triple-click lines
  - URL/path detection
- **Search History**
  - Remember recent searches
  - Quick re-search

**Why:** Copy mode is used constantly - make it excellent

---

### 5. Theme & Customization System
**Goal:** Beautiful defaults, infinite customization

**Improvements:**
- **Theme Files** (TOML format)
  - Separate from config.toml
  - Easy to share
  - Hot-reload support
- **Built-in Themes**
  - dracula
  - solarized (dark/light)
  - gruvbox
  - nord
  - tokyo-night
  - catppuccin
  - one-dark
  - monokai
- **Theme Variables**
  - Define once, use everywhere
  - Override specific colors
  - Support for ANSI, 256-color, true-color
- **Community Themes**
  - Repository for user themes
  - Easy installation

**Why:** Aesthetics matter, users want choice

---

### 6. Better Terminal Compatibility
**Goal:** Work flawlessly in any modern terminal

**Improvements:**
- **Terminal Detection**
  - Auto-detect capabilities ($TERM, terminfo)
  - Graceful degradation for limited terminals
- **Escape Sequence Testing**
  - Comprehensive test suite
  - Compatibility matrix
- **Alternate Screen Buffer Improvements**
  - Better state management
  - Clean restore on crashes
- **Bracket Paste Mode**
  - Already supported, enhance edge cases

**Tested Terminals:**
- iTerm2 (macOS)
- Alacritty
- Kitty
- WezTerm
- GNOME Terminal
- Konsole
- Windows Terminal
- tmux (nested)
- screen (nested)

**Why:** Users shouldn't worry about compatibility

---

### 7. Scripting & Automation
**Goal:** Programmable multiplexer

**Improvements:**
- **Command Mode Enhancements**
  - More tmux-compatible commands
  - Scripting variables
  - Conditionals
- **Hook System Expansion**
  - More hook points (window-created, pane-killed, etc.)
  - Shell script hooks
  - Lua hooks (optional, feature-gated)
- **Layout Templates**
  - Save/restore complex layouts
  - Parameterized layouts
  - Project-specific configs
- **Session Templates**
  - Define multi-window setups
  - Auto-start applications
  - Environment variables per pane

**Example:**
```toml
[[templates]]
name = "dev"
windows = [
    { name = "editor", command = "nvim" },
    { name = "server", command = "cargo watch -x run" },
    { name = "logs", command = "tail -f logs/app.log", split = "horizontal" }
]
```

**Why:** Power users want automation

---

### 8. Better Mouse Support
**Goal:** Modern mouse experience

**Improvements:**
- **Smooth Scrolling**
  - Use terminal scroll events
  - Configurable speed
- **Better Selection**
  - Word/line selection
  - Smart selection (URLs, paths, IPs)
  - Copy on selection (optional)
- **Drag & Drop** (terminal permitting)
  - Reorder panes
  - Resize boundaries
- **Right-Click Context Menu** (experimental)
  - Copy/paste
  - Split pane
  - Close pane

**Why:** Not everyone is a keyboard purist

---

### 9. Documentation & Onboarding
**Goal:** Easy to learn, easy to master

**Improvements:**
- **Interactive Tutorial**
  - First-run tutorial session
  - Learn by doing
  - Can be replayed
- **Built-in Help**
  - Better ? help screen (already good)
  - Context-sensitive help
  - Man pages
- **Migration Guides**
  - tmux → Ferrix guide
  - screen → Ferrix guide
  - Config conversion tools
- **Video Tutorials**
  - YouTube channel
  - Asciinema recordings

**Why:** Lower barrier to entry

---

### 10. Stability & Testing
**Goal:** Rock-solid reliability (continuation of v1.0 philosophy)

**Improvements:**
- **More Tests**
  - Target: 400+ tests (from 277)
  - Property-based testing (proptest)
  - Chaos engineering tests
- **Fuzzing**
  - ANSI parser fuzzing
  - Protocol fuzzing
  - Config file fuzzing
- **Long-Running Tests**
  - 24-hour stability tests
  - Memory leak detection
  - Handle exhaustion tests
- **Compatibility Testing**
  - CI tests across terminals
  - Platform testing (Linux, macOS, BSD, Windows)

**Why:** Stability is the #1 priority

---

## Implementation Phases

### Phase 1: Foundation (4-6 weeks)
**Goal:** Core improvements that enable everything else

- ✅ Remove GPU code (completed)
- ✅ Remove battery vulnerability (completed)
- ⏳ Implement dirty region tracking
- ⏳ Add render caching
- ⏳ Theme system infrastructure
- ⏳ Create 5-7 built-in themes

**Deliverable:** v2.0.0-alpha1

---

### Phase 2: Graphics & Protocol (4-5 weeks)
**Goal:** Best-in-class terminal protocol support

- ⏳ Sixel graphics protocol
- ⏳ Kitty graphics protocol (if time permits)
- ⏳ Unicode width handling
- ⏳ Nerd Fonts support
- ⏳ Better true-color support

**Deliverable:** v2.0.0-alpha2

---

### Phase 3: UX Polish (3-4 weeks)
**Goal:** Make it beautiful and responsive

- ⏳ Enhanced status bar
- ⏳ Better mode indicators
- ⏳ Advanced copy mode features
- ⏳ Improved mouse support
- ⏳ Message system improvements

**Deliverable:** v2.0.0-beta1

---

### Phase 4: Power User Features (3-4 weeks)
**Goal:** Scripting and automation

- ⏳ Layout templates
- ⏳ Session templates
- ⏳ Hook system expansion
- ⏳ Command mode enhancements
- ⏳ Lua scripting (experimental)

**Deliverable:** v2.0.0-beta2

---

### Phase 5: Compatibility & Testing (2-3 weeks)
**Goal:** Works everywhere, bulletproof

- ⏳ Terminal compatibility matrix
- ⏳ Add 100+ tests
- ⏳ Fuzzing campaign
- ⏳ 7-day load test
- ⏳ Cross-platform CI

**Deliverable:** v2.0.0-rc1

---

### Phase 6: Documentation & Release (2 weeks)
**Goal:** Ship it!

- ⏳ Interactive tutorial
- ⏳ Migration guides
- ⏳ Video tutorials
- ⏳ Release blog post
- ⏳ Community outreach

**Deliverable:** v2.0.0 🎉

---

## Success Metrics

### Performance
- ✅ Keystroke latency < 1ms (v1.0 baseline maintained)
- 🎯 Render latency < 5ms for 100 panes
- 🎯 Memory usage < 50MB for 50 sessions
- 🎯 No memory leaks in 7-day test

### Compatibility
- 🎯 100% compatibility with top 10 terminals
- 🎯 Works in nested tmux/screen
- 🎯 SSH session support (already works)
- 🎯 Windows Terminal support

### Features
- 🎯 Sixel/Kitty graphics working
- 🎯 10+ built-in themes
- 🎯 Layout templates functional
- 🎯 Interactive tutorial complete

### Stability
- ✅ 277+ tests passing (v1.0 baseline)
- 🎯 400+ tests passing
- 🎯 Zero crashes in 7-day load test
- 🎯 Clean fuzzing results

---

## Non-Goals for v2.0

**What we will NOT do:**

- ❌ Become a GUI application
- ❌ GPU rendering / native windowing
- ❌ Become a terminal emulator
- ❌ Web-based client
- ❌ Mobile apps
- ❌ Blockchain integration 😄

**Why:** Stay focused on being the best **terminal multiplexer**

---

## Timeline

**Total Duration:** ~20-25 weeks (5-6 months)

- Planning: ✅ Complete
- Phase 1: Weeks 1-6
- Phase 2: Weeks 7-11
- Phase 3: Weeks 12-15
- Phase 4: Weeks 16-19
- Phase 5: Weeks 20-22
- Phase 6: Weeks 23-25

**Target Release:** Q2 2025 (flexible based on quality)

---

## Community Engagement

- **Weekly progress updates** (blog/GitHub discussions)
- **Alpha/beta testing program** (call for testers)
- **Theme contest** (community submissions)
- **Bug bounty** (for v2.0 release)
- **Documentation sprint** (community contributions)

---

## Conclusion

v2.0 is about **perfecting the terminal multiplexing experience**. We're not chasing shiny features - we're building the most reliable, fastest, best-looking terminal multiplexer in Rust.

**Core Values:**
1. **Stability** - Never compromise
2. **Performance** - Every millisecond counts
3. **Compatibility** - Works everywhere
4. **Simplicity** - Easy to use, powerful when needed
5. **Beauty** - Aesthetics matter

Let's build something great! 🚀

---

*Last Updated: 2025-01-04*
*Status: In Planning*
*Next Milestone: v2.0.0-alpha1*
