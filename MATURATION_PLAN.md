# Ferrix Core Maturation Plan

## Overview
While Ferrix has all architectural components implemented, the integration between backend and UI needs to mature for the features to be usable. This document outlines the priority order for making Ferrix a fully functional terminal multiplexer.

## Priority 1: Window/Pane UI Integration (Essential)

### Current State
- ✅ Server has complete window/pane management with binary tree layout
- ✅ Protocol messages defined for all operations
- ❌ Client has no key handlers for window/pane operations
- ❌ Client doesn't render multiple panes

### Required Work
1. Add key binding handlers in `client/mod.rs`:
   - `Ctrl-b %` → Split vertical
   - `Ctrl-b "` → Split horizontal
   - `Ctrl-b arrow` → Navigate panes
   - `Ctrl-b c` → New window
   - `Ctrl-b n/p` → Next/prev window
   - `Ctrl-b z` → Zoom pane

2. Implement pane rendering in client:
   - Parse window layout from server
   - Divide terminal space based on layout tree
   - Route output to correct pane area
   - Draw pane borders

3. Handle multiple PTYs:
   - Track PTY per pane in server
   - Route input to focused pane
   - Multiplex output streams

### Estimated Effort: 3-4 days

## Priority 2: Fix Session State Persistence

### Current State
- ✅ Basic snapshot save/load works
- ❌ Window/pane layouts not saved (TODOs in code)
- ❌ PTY processes not properly restored

### Required Work
1. Serialize window Layout tree in snapshots
2. Restore Layout tree on snapshot load
3. Recreate PTY processes for each pane
4. Restore working directories and environment

### Estimated Effort: 2 days

## Priority 3: Copy Mode Activation

### Current State
- ✅ Complete CopyMode implementation with vim motions
- ❌ No way to enter copy mode from client
- ❌ No visual feedback during selection

### Required Work
1. Add `Ctrl-b [` handler to enter copy mode
2. Implement copy mode rendering overlay
3. Handle copy mode key events separately
4. Integrate with system clipboard

### Estimated Effort: 2-3 days

## Priority 4: Status Bar

### Current State
- ✅ StatusBar struct defined
- ❌ Never rendered
- ❌ No data collection

### Required Work
1. Render status bar at bottom of terminal
2. Collect session/window/pane info
3. Update on state changes
4. Make configurable

### Estimated Effort: 1-2 days

## Priority 5: Configuration System

### Current State
- ✅ Config parsing works
- ❌ Key bindings hardcoded
- ❌ No hot reload

### Required Work
1. Load key bindings from config
2. Implement file watcher
3. Apply config changes without restart
4. Add config validation feedback

### Estimated Effort: 2 days

## Priority 6: Plugin System Runtime Fix

### Current State
- ✅ WASM plugin architecture complete
- ❌ Store clone issue blocks execution
- ❌ WASI API needs updating

### Required Work
1. Refactor to Arc<Mutex<Store>> or per-plugin stores
2. Update WASI integration for latest wasmtime
3. Implement plugin communication channels
4. Add plugin discovery and loading

### Estimated Effort: 3-4 days

## Priority 7: Remote Sessions

### Current State
- ✅ Complete TLS implementation
- ✅ Authentication framework
- ❌ Not exposed in CLI
- ❌ Server doesn't listen for remote connections

### Required Work
1. Add `ferrix connect` command
2. Add `--remote` flag to server
3. Integrate RemoteClient into client flow
4. Add connection management UI

### Estimated Effort: 2-3 days

## Priority 8: GPU Acceleration

### Current State
- ✅ Basic wgpu setup
- ❌ API compatibility issues
- ❌ Not integrated with rendering

### Required Work
1. Update to latest wgpu API
2. Implement terminal renderer in WGSL
3. Add fallback for non-GPU systems
4. Benchmark and optimize

### Estimated Effort: 4-5 days

## Quick Wins (Can do immediately)

1. **Add missing key handlers** - Even without full pane rendering, add handlers that call server methods
2. **Fix daemonization** - Simple fork() implementation for server
3. **Improve error messages** - Add context to protocol errors
4. **Add debug logging** - Help diagnose integration issues
5. **Create integration tests** - Test client-server flows end-to-end

## Testing Strategy

1. **Unit tests** - Already implemented, keep passing
2. **Integration tests** - Add client-server interaction tests
3. **Manual testing checklist** - Document all features to test
4. **Stress testing** - Many panes, large output, rapid input
5. **Cross-platform testing** - Linux, macOS, eventually Windows

## Success Metrics

A mature Ferrix should:
- Support basic tmux workflow (windows, panes, copy mode)
- Handle 50+ panes without performance issues
- Recover from crashes without data loss
- Support remote connections securely
- Allow extending via plugins

## Timeline

With focused effort:
- **Week 1**: Priorities 1-2 (Window/Pane UI + Persistence)
- **Week 2**: Priorities 3-4 (Copy Mode + Status Bar)
- **Week 3**: Priority 5-6 (Config + Plugins)
- **Week 4**: Priority 7-8 (Remote + GPU)

This would bring Ferrix to a truly usable state where all advertised features work end-to-end.