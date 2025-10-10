# Ferrix Architecture

**Version**: 0.21.1
**Last Updated**: 2025-10-10
**Status**: Living Document

---

## Table of Contents

1. [Overview](#overview)
2. [Design Philosophy](#design-philosophy)
3. [High-Level Architecture](#high-level-architecture)
4. [Core Components](#core-components)
5. [Module Organization](#module-organization)
6. [Data Flow](#data-flow)
7. [IPC & Networking](#ipc--networking)
8. [Feature Architecture](#feature-architecture)
9. [Threading Model](#threading-model)
10. [Error Handling](#error-handling)
11. [Security Architecture](#security-architecture)
12. [Performance Optimizations](#performance-optimizations)
13. [Testing Strategy](#testing-strategy)
14. [Design Decisions](#design-decisions)

---

## Overview

Ferrix is a modern terminal multiplexer built with Rust that combines the reliability of GNU Screen with the features of tmux, while adding innovative capabilities only possible with modern technology. The architecture is designed for:

- **Modularity**: Feature flags allow users to build only what they need
- **Performance**: Async I/O, zero-copy operations, and optional GPU acceleration
- **Reliability**: Comprehensive error handling, circuit breakers, and recovery mechanisms
- **Extensibility**: Plugin system with WASM runtime for safe third-party extensions
- **Security**: TLS, mutual authentication, rate limiting, and comprehensive auditing

### Key Statistics

- **Total Lines**: ~50,000+ lines of Rust
- **Modules**: 15 core + 3 optional
- **Test Coverage**: 258 tests (251 unit + 7 integration)
- **Binary Size**: 5-6MB (minimal) to 9.7MB (full features)
- **Supported Platforms**: macOS, Linux (Windows partial)

---

## Design Philosophy

### 1. **Simplicity by Default, Power When Needed**

```rust
// Default build: Minimal dependencies
cargo build --release              // 5-6MB

// Power user: All features
cargo build --release --features full  // 9.7MB
```

Ferrix embraces **feature flags** (Option B strategy from Cargo.toml) to give users control:
- Default build is tmux-equivalent
- Optional features add advanced capabilities
- No bloat unless explicitly requested

### 2. **Memory Safety Without Sacrificing Performance**

- Async Rust with Tokio for efficient I/O multiplexing
- Zero-copy PTY data forwarding where possible
- Arena allocation for terminal buffers
- Lock-free data structures where appropriate

### 3. **Fail-Safe, Not Fail-Secure**

- Circuit breakers prevent cascade failures
- Graceful degradation (e.g., GPU → CPU rendering fallback)
- Session recovery from snapshots
- Comprehensive error context with suggestions

### 4. **Developer-First UX**

- Self-documenting errors with actionable suggestions
- Shell completions for all major shells
- Extensive documentation and examples
- Clear separation of concerns

---

## High-Level Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                         CLIENT LAYER                            │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐       │
│  │   CLI    │  │    UI    │  │  Input   │  │  Mouse   │       │
│  │ Commands │  │ Renderer │  │ Handler  │  │ Support  │       │
│  └────┬─────┘  └────┬─────┘  └────┬─────┘  └────┬─────┘       │
│       │             │             │             │               │
│       └─────────────┴─────────────┴─────────────┘               │
│                          │                                      │
└──────────────────────────┼──────────────────────────────────────┘
                           │
                  ┌────────▼────────┐
                  │   IPC LAYER     │
                  │  (Unix Socket)  │
                  │   or TCP/TLS    │
                  └────────┬────────┘
                           │
┌──────────────────────────▼──────────────────────────────────────┐
│                         SERVER LAYER                            │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │              Session Manager                             │  │
│  │  ┌──────────┐  ┌──────────┐  ┌──────────┐             │  │
│  │  │ Session  │  │ Session  │  │ Session  │   ...       │  │
│  │  │    1     │  │    2     │  │    3     │             │  │
│  │  └────┬─────┘  └────┬─────┘  └────┬─────┘             │  │
│  └───────┼─────────────┼─────────────┼────────────────────┘  │
│          │             │             │                        │
│  ┌───────▼─────────────▼─────────────▼────────────────────┐  │
│  │              Window Manager                            │  │
│  │  ┌──────────┐  ┌──────────┐  ┌──────────┐            │  │
│  │  │ Window 1 │  │ Window 2 │  │ Window 3 │   ...      │  │
│  │  └────┬─────┘  └────┬─────┘  └────┬─────┘            │  │
│  └───────┼─────────────┼─────────────┼─────────────────────┘  │
│          │             │             │                        │
│  ┌───────▼─────────────▼─────────────▼────────────────────┐  │
│  │              Pane Manager                              │  │
│  │  ┌──────────┐  ┌──────────┐  ┌──────────┐            │  │
│  │  │  Pane 1  │  │  Pane 2  │  │  Pane 3  │   ...      │  │
│  │  │  (PTY)   │  │  (PTY)   │  │  (PTY)   │            │  │
│  │  └────┬─────┘  └────┬─────┘  └────┬─────┘            │  │
│  └───────┼─────────────┼─────────────┼─────────────────────┘  │
│          │             │             │                        │
└──────────┼─────────────┼─────────────┼────────────────────────┘
           │             │             │
    ┌──────▼──────┐ ┌───▼──────┐ ┌───▼──────┐
    │  PTY + Shell│ │PTY + Shell│ │PTY + Shell│
    │   (bash)    │ │   (zsh)   │ │   (fish)  │
    └─────────────┘ └───────────┘ └───────────┘
```

### Communication Patterns

1. **Client → Server**: Request/Response over IPC or TCP
2. **Server → Client**: Push updates for PTY output, events
3. **PTY → Server**: Async output streaming
4. **Server → PTY**: Input forwarding

---

## Core Components

### 1. **Server** (`src/server/`)

The server is the heart of Ferrix, managing sessions, windows, panes, and PTY processes.

#### Key Modules:

- **`mod.rs`** (1200+ lines): Main server orchestration
  - Session management
  - Client connection handling
  - Message routing
  - PTY output polling

- **`session_manager.rs`** (800+ lines): Session lifecycle
  - Session creation/destruction
  - Session persistence
  - Auto-save coordination

- **`session.rs`** (1100+ lines): Per-session state
  - Window management
  - Pane layout
  - Configuration
  - Hooks

- **`window.rs`** (600+ lines): Window abstraction
  - Pane layout (split/tile)
  - Active pane tracking
  - Window-level state

- **`pane.rs`** (500+ lines): Individual pane
  - PTY interface
  - Scrollback buffer
  - Activity monitoring

- **`pty.rs`** (400+ lines): PTY management
  - Process spawning
  - Non-blocking I/O
  - Size updates (SIGWINCH)

#### Session Architecture:

```
Session
  ├── id: Uuid
  ├── name: String
  ├── windows: Vec<Window>
  ├── active_window: usize
  ├── config: SessionConfig
  └── hooks: Vec<Hook>

Window
  ├── id: Uuid
  ├── name: String
  ├── panes: Vec<Pane>
  ├── active_pane: usize
  └── layout: Layout

Pane
  ├── id: Uuid
  ├── pty: PtyMaster
  ├── buffer: TerminalBuffer
  ├── scrollback: Scrollback
  └── cursor: CursorPosition
```

### 2. **Client** (`src/client/`)

The client handles user interaction and rendering.

#### Key Responsibilities:

- **Terminal Setup**: Raw mode, alternate screen
- **Event Loop**: Keyboard, mouse, resize events
- **Rendering**: TUI with ratatui
- **Input Processing**: Keybindings, command mode
- **Server Communication**: Message serialization

#### Event Loop:

```rust
loop {
    select! {
        // User input (keyboard/mouse)
        event = event_stream.next() => handle_input(event),

        // Server messages (PTY output, updates)
        msg = server_rx.recv() => handle_server_message(msg),

        // Periodic refresh (60 FPS)
        _ = refresh_interval.tick() => render_frame(),
    }
}
```

### 3. **UI** (`src/ui/`)

Rendering and user interface components.

#### Modules:

- **`mod.rs`**: Main UI orchestration
- **`statusbar.rs`**: Status line rendering
- **`commandmode.rs`**: Command palette (`:` commands)
- **`copymode.rs`**: Vi-like copy mode
- **`help.rs`**: Help screen
- **`gpu_renderer.rs`** (605 lines): GPU-accelerated rendering
  - Font rasterization with fontdue
  - Glyph atlas (2048x2048 texture)
  - WGSL shaders
  - Text attributes (bold, italic, underline)

#### Rendering Pipeline:

```
Terminal Buffer → Layout Engine → Renderer → Terminal
                                     ↓
                            ┌────────┴────────┐
                            │                 │
                        CPU Path         GPU Path
                        (ratatui)        (wgpu)
```

### 4. **Protocol** (`src/protocol/`)

Binary protocol for client-server communication.

#### Message Types:

```rust
pub enum ClientMessage {
    CreateSession { name: String, ... },
    AttachSession { name: String },
    DetachSession,
    SendInput { session_id: Uuid, data: Vec<u8> },
    ResizeWindow { width: u16, height: u16 },
    // ... 20+ message types
}

pub enum ServerMessage {
    SessionCreated { session_id: Uuid },
    Output { pane_id: Uuid, data: Vec<u8> },
    SessionList { sessions: Vec<SessionInfo> },
    Error { message: String },
    // ... 15+ message types
}
```

#### Codec:

- **Format**: Bincode (binary serialization)
- **Framing**: Length-prefixed (u32 big-endian)
- **Compression**: Optional (performance feature)

### 5. **PTY** (`src/server/pty.rs`)

Pseudo-terminal management using `portable-pty`.

#### Architecture:

```rust
pub struct PtyMaster {
    master: Box<dyn MasterPty>,
    reader: Option<Box<dyn Read + Send>>,
    writer: Option<Box<dyn Write + Send>>,
    size: PtySize,
    pid: Option<u32>,
}
```

#### Key Operations:

- **Spawn**: Fork + exec shell
- **Read**: Non-blocking output capture
- **Write**: Input forwarding
- **Resize**: SIGWINCH signal

---

## Module Organization

### Core Modules (Always Available)

#### `src/cli/` - Command Line Interface
- Argument parsing with Clap
- Command routing
- Shell completion generation

#### `src/config/` - Configuration Management
- TOML parsing
- Keybinding definitions
- Hot reload support
- Session-specific config

#### `src/error/` - Error Handling
- Custom error types with thiserror
- Error suggestions (helpful messages)
- Result type alias

#### `src/format/` - Output Formatting
- ANSI escape sequence handling
- Color conversion
- Terminal state parsing

#### `src/input/` - Input Processing
- Keybinding resolution
- Input modes (vim/emacs)
- Key chord handling

#### `src/resilience/` - Reliability Infrastructure
- Circuit breakers
- Backpressure handling
- Error recovery strategies
- Health checks

#### `src/utils/` - Utility Functions
- ID generation
- Time utilities
- Path handling

### Optional Modules (Feature-Gated)

#### `src/ai/` - AI Assistant (`feature = "ai-assist"`)
- Command history learning
- Pattern recognition
- Context-aware suggestions
- Error fix recommendations

#### `src/auth/` - Authentication (`feature = "remote"`)
- User database (bcrypt password hashing)
- Rate limiting (5 attempts, 15min lockout)
- Session tokens
- Permission system

#### `src/plugin/` - Plugin System (`feature = "plugin"`)
- WASM runtime (wasmtime)
- Plugin marketplace
- Sandbox security
- API bindings

#### `src/transport/` - Network Transports (`feature = "ssh"/"mosh"`)
- TCP transport
- SSH tunnel support
- Mosh-style UDP (predictive echo)
- TLS/mTLS

---

## Data Flow

### Input Flow (User Keypress)

```
User Keyboard
      ↓
Terminal (raw mode)
      ↓
crossterm event
      ↓
Client input handler
      ↓
Keybinding resolver
      ↓
┌─────┴──────┐
│            │
Local Action │ Server Action
(copy mode)  │ (send input)
      ↓      │      ↓
   Execute   │  ClientMessage::SendInput
             │      ↓
             │  IPC/Network
             │      ↓
             │  Server message handler
             │      ↓
             │  Session → Window → Pane
             │      ↓
             │  PTY write
             │      ↓
             └─→ Shell process
```

### Output Flow (Shell Output)

```
Shell process
      ↓
PTY (file descriptor)
      ↓
Non-blocking read
      ↓
Server PTY poller
      ↓
Pane buffer update
      ↓
ServerMessage::Output
      ↓
IPC/Network
      ↓
Client message handler
      ↓
Terminal buffer update
      ↓
Render engine
      ↓
┌─────┴──────┐
│            │
CPU Render   │ GPU Render
(ratatui)    │ (wgpu)
      ↓      │      ↓
   stdout    │  GPU frame buffer
             │      ↓
             └─→ Present
```

### Snapshot Flow

```
User: save-snapshot
      ↓
Client → Server: SaveSnapshot
      ↓
Session state serialization
      ├── Windows
      ├── Panes
      ├── Terminal buffers
      ├── Cursor positions
      └── Scrollback
      ↓
Compress (flate2)
      ↓
Write to ~/.ferrix/snapshots/
      ↓
Add metadata (timestamp, description)
```

---

## IPC & Networking

### Local IPC (Unix Domain Socket)

**Default path**: `/tmp/ferrix.sock`

```rust
// Server
let listener = UnixListener::bind("/tmp/ferrix.sock")?;
loop {
    let (stream, _) = listener.accept().await?;
    tokio::spawn(handle_client(stream));
}

// Client
let stream = UnixStream::connect("/tmp/ferrix.sock").await?;
let (reader, writer) = stream.into_split();
```

**Advantages**:
- Zero-copy on same machine
- File system permissions
- No network exposure

### Remote Access (TCP + TLS)

**Port**: Configurable (default 7777)

```rust
// Server
let listener = TcpListener::bind("0.0.0.0:7777").await?;
let tls_acceptor = TlsAcceptor::from(tls_config);

loop {
    let (stream, addr) = listener.accept().await?;
    let tls_stream = tls_acceptor.accept(stream).await?;
    tokio::spawn(handle_client(tls_stream));
}
```

**Security**:
- TLS 1.3 with rustls
- Optional mutual TLS (client certificates)
- Rate limiting per IP
- Authentication required

### Message Protocol

**Wire Format**:

```
┌──────────────┬─────────────────┐
│ Length (u32) │ Bincode Payload │
├──────────────┼─────────────────┤
│   4 bytes    │   N bytes       │
└──────────────┴─────────────────┘
```

**Codec Implementation**:

```rust
// src/protocol/codec.rs
pub struct MessageCodec;

impl Encoder<ClientMessage> for MessageCodec {
    fn encode(&mut self, msg: ClientMessage, dst: &mut BytesMut) {
        let data = bincode::serialize(&msg).unwrap();
        dst.reserve(4 + data.len());
        dst.put_u32(data.len() as u32);
        dst.put_slice(&data);
    }
}
```

---

## Feature Architecture

### Recording & Replay (`feature = "recording"`)

**Architecture**:

```rust
pub struct SessionRecording {
    events: Vec<RecordedEvent>,
    start_time: DateTime<Utc>,
    metadata: RecordingMetadata,
}

pub enum RecordedEvent {
    Output { timestamp: Duration, data: Vec<u8> },
    Input { timestamp: Duration, keys: String },
    Resize { timestamp: Duration, size: (u16, u16) },
}
```

**Replay**:
- Real-time or fast-forward
- Seek to timestamp
- Export to asciicast format
- Export to HTML with embedded player

### Time Travel (`feature = "time-travel"`)

**Implementation**:

```rust
pub struct TimeTravel {
    snapshots: VecDeque<Snapshot>,
    current_index: usize,
    interval: Duration, // Auto-snapshot every N seconds
}
```

**Operations**:
- `step_back()`: Go to previous snapshot
- `step_forward()`: Go to next snapshot
- `goto(timestamp)`: Jump to specific time
- `playback(speed)`: Replay at custom speed

### Versioning (`feature = "versioning"`)

**Git-like session versioning**:

```rust
pub struct SessionVersion {
    id: Uuid,
    parent: Option<Uuid>,
    message: String,
    timestamp: DateTime<Utc>,
    snapshot: Snapshot,
}

pub struct VersionBranch {
    name: String,
    head: Uuid,
    commits: Vec<SessionVersion>,
}
```

**Commands**:
- `init-versioning`: Initialize version control
- `commit-session`: Save current state
- `branch <name>`: Create new branch
- `checkout <branch>`: Switch branches
- `merge <branch>`: Merge branches

### Collaboration (`feature = "collaboration"`)

**Multi-user session sharing**:

```rust
pub struct CollaborativeSession {
    session_id: Uuid,
    participants: HashMap<Uuid, Participant>,
    input_sync: BroadcastChannel<Input>,
    cursor_positions: HashMap<Uuid, CursorPosition>,
}
```

**Features**:
- Multiple users attach to same session
- Real-time cursor tracking
- Input synchronization
- User presence indicators

### Plugin System (`feature = "plugin"`)

**WASM-based plugins**:

```rust
pub struct PluginRuntime {
    engine: wasmtime::Engine,
    store: wasmtime::Store<PluginState>,
    modules: HashMap<String, Module>,
}
```

**Plugin API**:

```rust
// Plugin receives PTY output
fn on_output(data: &[u8]) -> Result<Vec<u8>>;

// Plugin can inject input
fn on_input(data: &[u8]) -> Result<Vec<u8>>;

// Plugin can render status bar widgets
fn render_widget() -> Result<String>;
```

**Marketplace** (`src/plugin/marketplace.rs`):
- Plugin discovery
- Version management
- Signature verification
- Dependency resolution

### GPU Rendering (`feature = "gpu"`)

**wgpu-based hardware acceleration**:

```rust
pub struct GpuRenderer {
    device: Arc<Device>,
    queue: Arc<Queue>,
    surface: Surface<'static>,
    render_pipeline: RenderPipeline,
    glyph_cache: GlyphCache,
}
```

**Performance**:
- Target: 60 FPS for smooth rendering
- Glyph caching (2048x2048 atlas)
- Font rasterization with fontdue
- GPU texture updates via wgpu::Queue

---

## Threading Model

### Async Runtime: Tokio

**Core threads**:

```rust
#[tokio::main]
async fn main() {
    // Server threads
    tokio::spawn(server_accept_loop());    // Accept connections
    tokio::spawn(pty_polling_loop());      // Poll PTY outputs
    tokio::spawn(metrics_collector());     // Collect metrics

    // Per-client threads
    for client in clients {
        tokio::spawn(handle_client(client));
    }
}
```

### Lock Strategy

**Fine-grained locking**:

```rust
pub struct SessionManager {
    sessions: Arc<RwLock<HashMap<Uuid, Arc<RwLock<Session>>>>>,
}
```

**Lock hierarchy** (prevents deadlocks):
1. SessionManager lock
2. Session lock
3. Window lock
4. Pane lock

**Lock-free optimization** (PTY polling):

```rust
// Release session lock BEFORE broadcasting
let output = {
    let session = sessions.read().await;
    session.get_output()  // Fast, inside lock
};  // Lock released here

// Broadcast outside lock
broadcast_channel.send(output).await;
```

---

## Error Handling

### Error Types

```rust
#[derive(Error, Debug)]
pub enum FerrixError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("Session not found: {0}")]
    SessionNotFound(String),

    #[error("Not connected to server")]
    NotConnected,

    #[error("Other error: {0}")]
    Other(String),
}
```

### Error Context

```rust
impl FerrixError {
    pub fn suggestion(&self) -> Option<String> {
        match self {
            FerrixError::SessionNotFound(name) => Some(format!(
                "Session '{}' not found. Try:\n\
                 • ferrix list - to see available sessions\n\
                 • ferrix new -s {} - to create this session",
                name, name
            )),
            // ... more suggestions
        }
    }
}
```

### Recovery Mechanisms

**Circuit Breaker** (`src/resilience/circuit_breaker.rs`):

```rust
pub struct CircuitBreaker {
    state: State,           // Closed | Open | HalfOpen
    failure_count: usize,
    success_threshold: usize,
    failure_threshold: usize,
    timeout: Duration,
}
```

**Backpressure** (`src/server/backpressure.rs`):

```rust
pub struct BackpressureManager {
    buffer_size: usize,
    high_watermark: usize,
    low_watermark: usize,
}
```

**Auto-Recovery**:
- PTY process restart on crash
- Session restoration from auto-save
- Connection retry with exponential backoff

---

## Security Architecture

### Authentication

**bcrypt password hashing**:

```rust
use bcrypt::{hash, verify, DEFAULT_COST};

pub fn hash_password(password: &str) -> Result<String> {
    hash(password, DEFAULT_COST)
        .map_err(|e| FerrixError::Other(format!("Hash failed: {}", e)))
}
```

### Rate Limiting

**Per-IP rate limiting** (`src/server/rate_limiter.rs`):

```rust
pub struct RateLimiter {
    attempts: HashMap<IpAddr, AttemptTracker>,
    max_attempts: usize,      // 5 attempts
    lockout_duration: Duration, // 15 minutes
}
```

### TLS Configuration

**rustls with TLS 1.3**:

```rust
let mut config = ServerConfig::builder()
    .with_safe_defaults()
    .with_no_client_auth()  // or with_client_cert_verifier for mTLS
    .with_single_cert(certs, key)?;
```

### Audit Logging

```rust
pub struct AuditLog {
    pub timestamp: DateTime<Utc>,
    pub user: String,
    pub action: Action,
    pub resource: String,
    pub result: Result<(), String>,
}
```

---

## Performance Optimizations

### 1. **Zero-Copy PTY Forwarding**

```rust
// Avoid unnecessary copies
let buf = pane.buffer.as_slice();
client.send_raw(buf).await?;  // Direct slice send
```

### 2. **Output Batching**

```rust
// Collect multiple PTY outputs before sending
let batch = output_queue.drain(..).collect();
client.send_batch(batch).await?;
```

### 3. **Delta Compression** (`feature = "performance"`)

```rust
// Only send changed bytes
let delta = compute_delta(last_frame, current_frame);
client.send_delta(delta).await?;
```

### 4. **GPU Acceleration** (`feature = "gpu"`)

- Offload rendering to GPU
- Glyph caching eliminates rasterization overhead
- Target: 60 FPS vs 30 FPS CPU rendering

### 5. **Lock-Free Structures**

```rust
use crossbeam::channel::unbounded;

let (tx, rx) = unbounded();  // Lock-free MPSC
```

---

## Testing Strategy

### Unit Tests (251 tests)

**Per-module tests**:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_creation() { /* ... */ }

    #[tokio::test]
    async fn test_pty_spawn() { /* ... */ }
}
```

### Integration Tests (7 tests)

**Full system tests** (`tests/integration_test_real.rs`):

```rust
#[tokio::test]
async fn test_full_session_lifecycle() {
    // 1. Start server
    let server = start_test_server().await;

    // 2. Create session
    let session = create_session("test").await;

    // 3. Send input
    send_keys(session, "echo hello\n").await;

    // 4. Verify output
    assert_output_contains(session, "hello").await;

    // 5. Cleanup
    kill_session(session).await;
}
```

### Benchmarks (5 suites)

**Criterion benchmarks**:

- `benches/startup.rs`: Server startup time
- `benches/large_output.rs`: PTY throughput
- `benches/performance.rs`: Message encoding/decoding

**Results** (macOS M1):
- Startup: 45ms
- Session creation: 12ms
- Message round-trip: 0.3ms

---

## Design Decisions

### 1. **Why Rust?**

- **Memory safety**: No segfaults, no data races
- **Performance**: Zero-cost abstractions, no GC
- **Concurrency**: Tokio async runtime
- **Ecosystem**: Excellent crates for TUI, networking, serialization

### 2. **Why Client-Server Architecture?**

- **Session persistence**: Server keeps sessions alive when clients detach
- **Remote access**: Natural fit for TCP/TLS
- **Multi-client**: Multiple clients can attach to same session
- **Resource isolation**: Server can run with elevated privileges

### 3. **Why Bincode for Protocol?**

- **Fast**: Binary serialization, minimal overhead
- **Type-safe**: Rust's serde derives ensure correctness
- **Compact**: Smaller messages than JSON

**Alternatives considered**:
- JSON: Too verbose, slower parsing
- MessagePack: Similar to bincode, but less Rust-idiomatic
- Protobuf: Overkill for internal protocol

### 4. **Why Feature Flags?**

- **User choice**: Don't pay for features you don't use
- **Binary size**: Minimal build is 40% smaller than full
- **Compilation time**: Default build 2x faster
- **Security**: Fewer dependencies = smaller attack surface

### 5. **Why WASM for Plugins?**

- **Safety**: Sandbox prevents malicious plugins
- **Portability**: Single binary works across platforms
- **Performance**: Near-native speed with wasmtime
- **Ecosystem**: Growing WASM community

**Alternatives considered**:
- Native plugins (.so/.dylib): Security risk
- Python/Lua: Slower, requires embedding interpreter
- JavaScript (V8): Large dependency, higher overhead

### 6. **Why Ratatui for TUI?**

- **Cross-platform**: Works on all Unix-like systems
- **Mature**: Battle-tested in production
- **Efficient**: Minimal redraws, flicker-free
- **Extensible**: Easy to add custom widgets

### 7. **Why Unix Domain Sockets for IPC?**

- **Fast**: Zero-copy on localhost
- **Secure**: File system permissions
- **Standard**: Works on all Unix systems
- **Simple**: No port conflicts

---

## Future Architecture Improvements

### Planned for v1.1

1. **mTLS Support**: Full mutual TLS authentication
2. **Horizontal Scaling**: Multiple server instances with shared state
3. **Plugin Hot Reload**: Update plugins without restart
4. **Incremental Snapshots**: Delta-based snapshots for efficiency

### Planned for v2.0

1. **gRPC Protocol**: Replace bincode with gRPC for better language interop
2. **Distributed Sessions**: Sessions spanning multiple machines
3. **WebSocket Support**: Browser-based clients
4. **Kubernetes Integration**: Native k8s operator

---

## Glossary

- **PTY (Pseudo-Terminal)**: Pair of virtual devices (master/slave) for terminal emulation
- **Multiplexer**: Software that manages multiple terminal sessions in a single window
- **Session**: Persistent container for windows and panes
- **Window**: Tab-like container for panes
- **Pane**: Individual terminal with its own PTY and shell
- **IPC (Inter-Process Communication)**: Communication between client and server
- **TUI (Text User Interface)**: Terminal-based user interface
- **WASM (WebAssembly)**: Portable bytecode format for safe sandboxed execution

---

## References

- [Cargo.toml](Cargo.toml) - Dependency and feature configuration
- [FEATURES.md](docs/FEATURES.md) - Feature flag documentation
- [DEVELOPER_GUIDE.md](docs/DEVELOPER_GUIDE.md) - Contribution guide
- [SECURITY.md](SECURITY.md) - Security policy and reporting
- [V1_RELEASE_CHECKLIST.md](docs/V1_RELEASE_CHECKLIST.md) - Release readiness

---

**Contributors**: David Liedle, Claude (AI Assistant)
**License**: MIT OR Apache-2.0
**Repository**: https://github.com/davidliedle/Ferrix

---

*This document is a living architecture guide. Please update it as the system evolves.*
