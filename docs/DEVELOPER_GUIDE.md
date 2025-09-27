# Ferrix Developer Guide

## Table of Contents
1. [Architecture Overview](#architecture-overview)
2. [Building from Source](#building-from-source)
3. [Project Structure](#project-structure)
4. [Core Components](#core-components)
5. [Protocol Specification](#protocol-specification)
6. [Plugin Development](#plugin-development)
7. [Contributing](#contributing)
8. [Testing](#testing)
9. [Performance Optimization](#performance-optimization)
10. [Security Considerations](#security-considerations)

## Architecture Overview

Ferrix follows a client-server architecture with asynchronous message passing:

```
┌─────────────┐     Binary      ┌─────────────┐
│   Client    │◄──────────────► │   Server    │
│  (Terminal) │    Protocol      │  (Sessions) │
└─────────────┘                  └─────────────┘
       │                                │
       ▼                                ▼
┌─────────────┐              ┌─────────────────┐
│     UI      │              │  Session Mgr    │
│  Renderer   │              ├─────────────────┤
└─────────────┘              │  Window Mgr     │
                             ├─────────────────┤
                             │   Pane Mgr      │
                             ├─────────────────┤
                             │   PTY Handler   │
                             └─────────────────┘
```

### Key Design Principles

1. **Async-First**: Built on Tokio for maximum concurrency
2. **Type Safety**: Leverages Rust's type system for reliability
3. **Zero-Copy**: Minimizes memory allocations in hot paths
4. **Modular**: Clear separation of concerns with trait boundaries
5. **Extensible**: Plugin system for custom functionality

## Building from Source

### Prerequisites

- Rust 1.70+ (with cargo)
- C compiler (for native dependencies)
- pkg-config
- OpenSSL development libraries (for TLS)

### Development Build

```bash
# Clone repository
git clone https://github.com/yourusername/ferrix.git
cd ferrix

# Build debug version
cargo build

# Run tests
cargo test

# Run with verbose logging
RUST_LOG=debug ./target/debug/ferrix
```

### Release Build

```bash
# Build optimized binary
cargo build --release

# Strip symbols for smaller binary
strip target/release/ferrix

# Run benchmarks
cargo bench
```

### Feature Flags

```bash
# Build with specific features
cargo build --features "gpu,remote,plugins"

# Build minimal version
cargo build --no-default-features --features "core"
```

Available features:
- `gpu`: GPU-accelerated rendering
- `remote`: Remote session support
- `plugins`: WASM plugin system
- `tls`: TLS encryption for remote sessions
- `compression`: Protocol compression

## Project Structure

```
ferrix/
├── src/
│   ├── main.rs              # Entry point and CLI
│   ├── client/              # Client implementation
│   │   ├── mod.rs
│   │   ├── terminal.rs      # Terminal handling
│   │   └── ui.rs           # User interface
│   ├── server/              # Server implementation
│   │   ├── mod.rs
│   │   ├── session.rs       # Session management
│   │   ├── window.rs        # Window management
│   │   ├── pane.rs         # Pane handling
│   │   ├── layout.rs        # Layout algorithms
│   │   ├── pty.rs          # PTY management
│   │   ├── snapshot.rs      # Session persistence
│   │   ├── versioning.rs    # Git-like versioning
│   │   └── remote.rs        # Remote session server
│   ├── protocol/            # Wire protocol
│   │   ├── mod.rs
│   │   └── messages.rs      # Message definitions
│   ├── config/              # Configuration
│   │   ├── mod.rs
│   │   └── parser.rs        # Config parsing
│   ├── plugin/              # Plugin system
│   │   ├── mod.rs
│   │   ├── runtime.rs       # WASM runtime
│   │   └── api.rs          # Plugin API
│   ├── ui/                  # UI components
│   │   ├── mod.rs
│   │   ├── renderer.rs      # Rendering engine
│   │   └── copymode.rs      # Copy mode implementation
│   ├── error.rs             # Error handling
│   └── lib.rs              # Library interface
├── tests/
│   └── integration_test.rs  # Integration tests
├── benches/
│   └── performance.rs       # Benchmarks
├── examples/
│   └── plugin_example.rs    # Example plugin
└── docs/                    # Documentation
```

## Core Components

### Server Module (`src/server/`)

The server manages all sessions, windows, and panes:

```rust
// src/server/mod.rs
pub struct Server {
    sessions: Arc<RwLock<HashMap<SessionId, Arc<RwLock<Session>>>>>,
    clients: Arc<RwLock<HashMap<ClientId, ClientConnection>>>,
    event_loop: Arc<Mutex<EventLoop>>,
}

impl Server {
    pub async fn run(&mut self) -> Result<()> {
        // Main server loop
        loop {
            tokio::select! {
                Some(client) = self.accept_client() => {
                    self.handle_client(client).await?;
                }
                Some(event) = self.event_loop.next() => {
                    self.process_event(event).await?;
                }
            }
        }
    }
}
```

### Session Management (`src/server/session.rs`)

Sessions are the top-level container:

```rust
pub struct Session {
    pub id: SessionId,
    pub name: String,
    pub windows: Vec<Arc<RwLock<Window>>>,
    pub current_window: Option<WindowId>,
    pub created_at: DateTime<Utc>,
    pub activity: DateTime<Utc>,
}

impl Session {
    pub async fn create_window(&mut self, name: Option<String>) -> Result<WindowId> {
        let window = Window::new(WindowId::new(), name);
        self.windows.push(Arc::new(RwLock::new(window)));
        Ok(window.id)
    }

    pub async fn handle_input(&mut self, data: Vec<u8>) -> Result<()> {
        // Route input to current pane
        if let Some(window) = self.get_current_window() {
            window.write().await.handle_input(data).await
        }
    }
}
```

### Layout Engine (`src/server/layout.rs`)

The layout engine uses a binary tree for flexible pane arrangements:

```rust
#[derive(Debug, Clone)]
pub enum Layout {
    Leaf(PaneId),
    Split {
        direction: SplitDirection,
        ratio: f32,
        first: Box<Layout>,
        second: Box<Layout>,
    },
}

impl Layout {
    pub fn split(&mut self, pane: &PaneId, direction: SplitDirection, new_pane: PaneId) -> bool {
        match self {
            Layout::Leaf(id) if id == pane => {
                *self = Layout::Split {
                    direction,
                    ratio: 0.5,
                    first: Box::new(Layout::Leaf(*id)),
                    second: Box::new(Layout::Leaf(new_pane)),
                };
                true
            }
            Layout::Split { first, second, .. } => {
                first.split(pane, direction, new_pane) ||
                second.split(pane, direction, new_pane)
            }
            _ => false,
        }
    }

    pub fn get_dimensions(&self, width: u16, height: u16) -> Vec<(PaneId, u16, u16, u16, u16)> {
        // Calculate pane positions and sizes
        self.calculate_recursive(0, 0, width, height)
    }
}
```

### PTY Handler (`src/server/pty.rs`)

Manages pseudo-terminal interactions:

```rust
use portable_pty::{PtySystem, CommandBuilder, PtySize};

pub struct PtyHandler {
    master: Box<dyn MasterPty>,
    child: Box<dyn Child>,
}

impl PtyHandler {
    pub fn spawn(command: Option<String>) -> Result<Self> {
        let pty_system = native_pty_system();
        let pair = pty_system.openpty(PtySize::default())?;

        let cmd = CommandBuilder::new(command.unwrap_or_else(|| {
            std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string())
        }));

        let child = pair.slave.spawn_command(cmd)?;

        Ok(PtyHandler {
            master: pair.master,
            child,
        })
    }

    pub async fn read(&mut self) -> Result<Vec<u8>> {
        let mut buffer = vec![0u8; 4096];
        let reader = self.master.try_clone_reader()?;
        let n = reader.read(&mut buffer).await?;
        buffer.truncate(n);
        Ok(buffer)
    }

    pub async fn write(&mut self, data: &[u8]) -> Result<()> {
        self.master.write_all(data).await
    }
}
```

## Protocol Specification

### Message Format

All messages use a binary protocol with length-prefixed framing:

```
┌────────────┬────────────┬─────────────┐
│ Length(4B) │ Type(1B)   │ Payload(nB) │
└────────────┴────────────┴─────────────┘
```

### Message Types

```rust
#[derive(Serialize, Deserialize)]
pub enum ClientMessage {
    // Session management
    CreateSession { name: Option<String> },
    AttachSession { id: SessionId, force: bool },
    DetachSession,
    ListSessions,

    // Window operations
    CreateWindow { name: Option<String> },
    SwitchWindow { id: WindowId },
    RenameWindow { id: WindowId, name: String },

    // Pane operations
    SplitPane { direction: SplitDirection },
    NavigatePane { direction: NavigationDirection },
    ResizePane { direction: Direction, amount: i32 },

    // Input/Output
    Input(Vec<u8>),
    Resize { width: u16, height: u16 },

    // Commands
    Command { command: String, args: Vec<String> },
}

#[derive(Serialize, Deserialize)]
pub enum ServerMessage {
    // Responses
    Success { message: Option<String> },
    Error { message: String },

    // Session info
    SessionList(Vec<SessionInfo>),
    SessionCreated { id: SessionId },

    // Output
    Output { pane_id: PaneId, data: Vec<u8> },

    // UI updates
    LayoutChanged(LayoutInfo),
    StatusUpdate(StatusInfo),

    // Notifications
    Bell,
    Activity { window_id: WindowId },
}
```

### Wire Protocol Implementation

```rust
pub async fn send_message<T: Serialize>(
    stream: &mut TcpStream,
    msg: &T,
) -> Result<()> {
    let payload = bincode::serialize(msg)?;
    let length = payload.len() as u32;

    stream.write_all(&length.to_be_bytes()).await?;
    stream.write_all(&payload).await?;
    stream.flush().await?;

    Ok(())
}

pub async fn receive_message<T: DeserializeOwned>(
    stream: &mut TcpStream,
) -> Result<T> {
    let mut length_bytes = [0u8; 4];
    stream.read_exact(&mut length_bytes).await?;
    let length = u32::from_be_bytes(length_bytes) as usize;

    let mut payload = vec![0u8; length];
    stream.read_exact(&mut payload).await?;

    Ok(bincode::deserialize(&payload)?)
}
```

## Plugin Development

### Plugin API

Plugins are WebAssembly modules with a defined interface:

```rust
// Plugin trait that all plugins must implement
#[async_trait]
pub trait Plugin: Send + Sync {
    /// Called when plugin is loaded
    async fn initialize(&mut self, config: PluginConfig) -> Result<()>;

    /// Handle events from Ferrix
    async fn handle_event(&mut self, event: PluginEvent) -> Result<()>;

    /// Called periodically (if plugin requests it)
    async fn update(&mut self) -> Result<()>;

    /// Called before plugin is unloaded
    async fn shutdown(&mut self) -> Result<()>;
}

#[derive(Serialize, Deserialize)]
pub enum PluginEvent {
    SessionCreated { id: SessionId, name: String },
    SessionDestroyed { id: SessionId },
    WindowCreated { session: SessionId, window: WindowId },
    PaneCreated { window: WindowId, pane: PaneId },
    Input { pane: PaneId, data: Vec<u8> },
    Output { pane: PaneId, data: Vec<u8> },
    StatusLineRequest,
    Custom(serde_json::Value),
}
```

### Example Plugin

```rust
// examples/git_status_plugin.rs
use ferrix_plugin_api::*;

#[derive(Default)]
struct GitStatusPlugin {
    current_branch: Option<String>,
    dirty_files: usize,
}

#[async_trait]
impl Plugin for GitStatusPlugin {
    async fn initialize(&mut self, _config: PluginConfig) -> Result<()> {
        self.update_git_status().await?;
        Ok(())
    }

    async fn handle_event(&mut self, event: PluginEvent) -> Result<()> {
        match event {
            PluginEvent::StatusLineRequest => {
                let status = self.format_status();
                self.send_response(PluginResponse::StatusLine(status)).await?;
            }
            PluginEvent::WindowCreated { .. } => {
                self.update_git_status().await?;
            }
            _ => {}
        }
        Ok(())
    }

    async fn update(&mut self) -> Result<()> {
        self.update_git_status().await
    }

    async fn shutdown(&mut self) -> Result<()> {
        Ok(())
    }
}

impl GitStatusPlugin {
    async fn update_git_status(&mut self) -> Result<()> {
        // Use git2 crate to get repository status
        let repo = git2::Repository::open_from_env()?;
        let head = repo.head()?;

        self.current_branch = head.shorthand().map(String::from);

        let statuses = repo.statuses(None)?;
        self.dirty_files = statuses.iter().count();

        Ok(())
    }

    fn format_status(&self) -> String {
        match (&self.current_branch, self.dirty_files) {
            (Some(branch), 0) => format!(" {}", branch),
            (Some(branch), n) => format!(" {} ±{}", branch, n),
            _ => String::new(),
        }
    }
}

// Export plugin
ferrix_plugin_export!(GitStatusPlugin);
```

### Building Plugins

```bash
# Build plugin to WASM
cargo build --target wasm32-wasi --release

# Install plugin
ferrix plugin install target/wasm32-wasi/release/git_status.wasm

# Test plugin
ferrix plugin test git_status
```

## Contributing

### Development Workflow

1. Fork the repository
2. Create a feature branch: `git checkout -b feature/amazing-feature`
3. Make your changes
4. Run tests: `cargo test`
5. Run linter: `cargo clippy`
6. Format code: `cargo fmt`
7. Commit: `git commit -m "feat: add amazing feature"`
8. Push: `git push origin feature/amazing-feature`
9. Open a Pull Request

### Code Style

We follow Rust standard style guidelines:

```rust
// Good
pub fn process_data(input: &[u8]) -> Result<Vec<u8>> {
    let mut output = Vec::with_capacity(input.len());
    for byte in input {
        output.push(process_byte(*byte)?);
    }
    Ok(output)
}

// Avoid
pub fn ProcessData(input: &[u8]) -> Result<Vec<u8>> {
    let mut output = vec![];
    for i in 0..input.len() {
        output.push(process_byte(input[i])?);
    }
    return Ok(output);
}
```

### Commit Messages

Follow conventional commits:

- `feat:` New feature
- `fix:` Bug fix
- `docs:` Documentation
- `test:` Tests
- `refactor:` Code refactoring
- `perf:` Performance improvement
- `chore:` Maintenance

## Testing

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_layout_split() {
        let mut layout = Layout::new(PaneId::new());
        let new_pane = PaneId::new();

        assert!(layout.split(&original_pane, SplitDirection::Horizontal, new_pane));
        assert_eq!(layout.get_all_panes().len(), 2);
    }

    #[tokio::test]
    async fn test_async_operations() {
        let mut session = Session::new(SessionId::new(), "test".to_string());
        let window_id = session.create_window(None).await.unwrap();
        assert!(session.windows.iter().any(|w| w.read().await.id == window_id));
    }
}
```

### Integration Tests

```rust
// tests/integration_test.rs
#[tokio::test]
async fn test_full_session_lifecycle() {
    let server = TestServer::start().await;
    let client = TestClient::connect(&server.addr()).await;

    // Create session
    let session_id = client.create_session("test").await.unwrap();

    // Attach to session
    client.attach_session(session_id).await.unwrap();

    // Create window and pane
    let window_id = client.create_window().await.unwrap();
    let pane_id = client.split_pane(SplitDirection::Vertical).await.unwrap();

    // Send input
    client.send_input(b"echo hello\n").await.unwrap();

    // Verify output
    let output = client.read_output().await.unwrap();
    assert!(String::from_utf8_lossy(&output).contains("hello"));

    // Clean up
    client.detach().await.unwrap();
    server.shutdown().await;
}
```

### Benchmarks

```rust
// benches/performance.rs
use criterion::{criterion_group, criterion_main, Criterion};

fn benchmark_message_serialization(c: &mut Criterion) {
    let msg = ClientMessage::Input(vec![0u8; 1024]);

    c.bench_function("serialize message", |b| {
        b.iter(|| bincode::serialize(&msg))
    });
}

fn benchmark_layout_calculation(c: &mut Criterion) {
    let mut layout = create_complex_layout();

    c.bench_function("calculate layout 10 panes", |b| {
        b.iter(|| layout.get_dimensions(1920, 1080))
    });
}

criterion_group!(benches, benchmark_message_serialization, benchmark_layout_calculation);
criterion_main!(benches);
```

### Test Coverage

```bash
# Generate coverage report
cargo tarpaulin --out Html --output-dir coverage

# View coverage
open coverage/index.html

# Minimum coverage requirement: 80%
```

## Performance Optimization

### Key Optimizations

1. **Zero-Copy I/O**: Use `bytes::Bytes` for avoiding allocations
2. **Buffer Pooling**: Reuse buffers with `object_pool`
3. **Lazy Evaluation**: Defer expensive computations
4. **Parallel Processing**: Use `rayon` for CPU-bound tasks
5. **Efficient Data Structures**: Choose appropriate collections

### Profiling

```bash
# CPU profiling with perf
cargo build --release
perf record --call-graph=dwarf ./target/release/ferrix
perf report

# Memory profiling with Valgrind
valgrind --tool=massif ./target/release/ferrix
ms_print massif.out.*

# Flamegraph generation
cargo flamegraph --bin ferrix
```

### Performance Guidelines

```rust
// Prefer pre-allocation
let mut buffer = Vec::with_capacity(expected_size);

// Use COW for read-heavy operations
use std::borrow::Cow;
fn process(data: Cow<str>) -> Cow<str> {
    if data.contains("modify") {
        let mut owned = data.into_owned();
        owned.push_str("_modified");
        Cow::Owned(owned)
    } else {
        data
    }
}

// Avoid unnecessary clones
fn bad(vec: Vec<String>) -> Vec<String> {
    vec.clone() // Unnecessary
}

fn good(vec: &[String]) -> Vec<&str> {
    vec.iter().map(|s| s.as_str()).collect()
}
```

## Security Considerations

### Authentication

```rust
pub trait AuthenticationHandler: Send + Sync {
    async fn authenticate(&self, credentials: AuthCredentials) -> Result<bool>;
    async fn authorize(&self, client: &ClientId, action: &Action) -> Result<bool>;
}

// Implement secure password handling
use argon2::{Argon2, PasswordHash, PasswordVerifier};

pub struct PasswordAuth {
    users: HashMap<String, String>, // username -> hash
}

impl AuthenticationHandler for PasswordAuth {
    async fn authenticate(&self, credentials: AuthCredentials) -> Result<bool> {
        if let (Some(password), Some(hash)) = (
            credentials.password,
            self.users.get(&credentials.username),
        ) {
            let parsed_hash = PasswordHash::new(hash)?;
            Ok(Argon2::default()
                .verify_password(password.as_bytes(), &parsed_hash)
                .is_ok())
        } else {
            Ok(false)
        }
    }
}
```

### TLS Configuration

```rust
use rustls::{ServerConfig, Certificate, PrivateKey};

pub fn create_tls_config(cert_path: &Path, key_path: &Path) -> Result<ServerConfig> {
    let cert = load_certs(cert_path)?;
    let key = load_key(key_path)?;

    let config = ServerConfig::builder()
        .with_safe_defaults()
        .with_no_client_auth()
        .with_single_cert(cert, key)?;

    Ok(config)
}
```

### Input Sanitization

```rust
pub fn sanitize_input(input: &[u8]) -> Vec<u8> {
    input.iter()
        .filter(|&&byte| {
            // Allow printable ASCII and specific control characters
            (byte >= 0x20 && byte <= 0x7E) ||  // Printable ASCII
            byte == 0x09 ||  // Tab
            byte == 0x0A ||  // Line feed
            byte == 0x0D ||  // Carriage return
            byte == 0x1B     // Escape (for ANSI sequences)
        })
        .copied()
        .collect()
}
```

### Security Checklist

- [ ] Always validate input from untrusted sources
- [ ] Use secure random number generation (`rand::rngs::OsRng`)
- [ ] Implement rate limiting for API endpoints
- [ ] Audit dependencies regularly (`cargo audit`)
- [ ] Enable all compiler security features
- [ ] Implement proper session timeout
- [ ] Use constant-time comparison for secrets
- [ ] Sanitize all terminal output
- [ ] Validate plugin signatures before loading
- [ ] Implement proper ACLs for multi-user scenarios

## Debugging

### Debug Builds

```bash
# Build with debug symbols
cargo build

# Enable detailed logging
RUST_LOG=ferrix=trace ./target/debug/ferrix

# Use GDB for debugging
gdb ./target/debug/ferrix
(gdb) break ferrix::server::session::Session::new
(gdb) run new -s debug
```

### Logging

```rust
use log::{debug, info, warn, error};

pub fn process_message(msg: ClientMessage) {
    debug!("Received message: {:?}", msg);

    match handle_message(msg) {
        Ok(response) => info!("Processed successfully: {:?}", response),
        Err(e) => error!("Failed to process: {}", e),
    }
}

// Configure logging in main.rs
env_logger::Builder::from_env(Env::default().default_filter_or("info"))
    .format_timestamp_millis()
    .init();
```

### Common Issues

**Problem**: Deadlock in async code
```rust
// Bad: Can cause deadlock
let guard = resource.write().await;
let other = other_resource.write().await; // Deadlock if other thread has opposite order

// Good: Consistent lock ordering
let resources = Resources::lock_all(&[resource, other_resource]).await;
```

**Problem**: PTY not responding
```bash
# Check PTY allocation
ls -la /dev/pts/

# Verify process is running
ps aux | grep ferrix

# Check file descriptors
lsof -p $(pgrep ferrix)
```

## Future Roadmap

### Planned Features

1. **GPU Rendering**: Hardware-accelerated terminal rendering
2. **Collaborative Sessions**: Real-time session sharing
3. **Cloud Sync**: Session synchronization across devices
4. **Mobile Support**: iOS/Android clients
5. **AI Integration**: Smart command completion and suggestions
6. **Advanced Scripting**: Lua/Python embedding for automation

### Architecture Evolution

```
Future Architecture:

┌──────────────────────────────────────┐
│          Ferrix Cloud               │
│     (Session Sync & Sharing)        │
└──────────────────────────────────────┘
                    │
     ┌──────────────┼──────────────┐
     ▼              ▼              ▼
┌─────────┐   ┌─────────┐   ┌─────────┐
│Desktop  │   │ Mobile  │   │   Web   │
│ Client  │   │ Client  │   │ Client  │
└─────────┘   └─────────┘   └─────────┘
     │              │              │
     └──────────────┼──────────────┘
                    ▼
          ┌──────────────────┐
          │   Ferrix Core    │
          │   (Local/Remote) │
          └──────────────────┘
```

## Resources

- [Ferrix GitHub Repository](https://github.com/yourusername/ferrix)
- [API Documentation](https://docs.rs/ferrix)
- [Community Discord](https://discord.gg/ferrix)
- [Blog Posts and Tutorials](https://ferrix.dev/blog)

## License

Ferrix is licensed under the MIT License. See [LICENSE](../LICENSE) for details.