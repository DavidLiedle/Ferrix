# Ferrix Testing Guide

This document describes the testing infrastructure for Ferrix.

## Test Organization

### Unit Tests (`src/**/*.rs`)
- 356 unit tests covering individual components
- Located in `#[cfg(test)]` modules within source files
- Run with: `cargo test --lib`

### Integration Tests (`tests/`)
- End-to-end tests of client-server functionality
- Organized by tier (tier1=protocol, tier2=lifecycle, tier3=advanced)
- Stress tests and performance validation
- Run with: `cargo test --test <test_name>`

### Benchmarks (`benches/`)
- Performance benchmarks using Criterion
- Run with: `cargo bench`
- Validates optimization targets:
  - Keystroke latency: <1ms
  - Render throughput: 60 FPS (16.67ms/frame)
  - Lock-free config access
  - Clone reduction in hot paths

## Integration Test Framework

### Helper Modules (`tests/integration/helpers/`)

**TestFixture**: Provides isolated test environment
```rust
let fixture = TestFixture::new();
let socket_path = fixture.socket_path();
```

**TestServer**: Manages server lifecycle
```rust
let mut server = TestServer::start_default(socket_path.clone()).await;
assert!(server.is_running());
```

**TestClient**: Wraps CLI command execution
```rust
let client = TestClient::new(socket_path.clone());
let output = client.new_session("my-session", true);
assert!(output.status.success());
```

**Assertions**: Common test assertions
```rust
use integration::helpers::{assert_session_exists, assert_eventually};

let sessions = TestClient::parse_session_list(&output);
assert_session_exists(&sessions, "my-session");
```

### Example Integration Test

See `tests/integration_framework_demo.rs` for a complete example:

```rust
#[tokio::test]
async fn test_basic_session_lifecycle() {
    let fixture = TestFixture::new();
    let mut server = TestServer::start_default(fixture.socket_path().clone()).await;
    let client = TestClient::new(fixture.socket_path().clone());

    // Create session
    let output = client.new_session("test", true);
    assert!(output.status.success());

    // Verify exists
    let sessions = TestClient::parse_session_list(&client.list_sessions());
    assert_session_exists(&sessions, "test");

    // Clean up
    client.kill_session("test");
    assert!(server.is_running());
}
```

## Running Tests

```bash
# All tests
cargo test

# Unit tests only
cargo test --lib

# Integration tests
cargo test --test integration_framework_demo

# Specific test
cargo test test_basic_session_lifecycle

# Benchmarks
cargo bench

# With output
cargo test -- --nocapture
```

## Writing New Tests

1. Use the helper modules for consistent test structure
2. Always use `TestFixture` for isolated environments
3. Clean up resources (kill sessions, stop servers)
4. Add assertions for both success and failure cases
5. Document expected behavior in comments

## Performance Targets

| Metric | Target | Benchmark |
|--------|--------|-----------|
| Keystroke latency | <1ms | `bench_keystroke_latency` |
| Render throughput | 60 FPS | `bench_render_frame` |
| Session creation | <100ms | - |
| Memory per pane | <1MB | - |

## Coverage

Current test coverage:
- **356 unit tests** (components, modules, utilities)
- **38 integration tests** (e2e workflows, stress tests)
- **10 benchmarks** (performance validation)

## Future Work

Recommended additions:
- [ ] Long-running stability tests (24h+)
- [ ] Memory leak detection (valgrind/ASAN)
- [ ] Fuzzing (AFL, cargo-fuzz)
- [ ] Multi-platform CI (Linux, macOS, FreeBSD)
- [ ] Performance regression detection in CI
