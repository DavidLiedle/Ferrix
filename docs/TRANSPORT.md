# Ferrix Transport Layer

Extended protocol support for SSH and Mosh connections.

## Overview

Ferrix supports multiple transport protocols for connecting clients to servers:

1. **TCP** - Direct TCP connections (built-in)
2. **TLS** - Encrypted TCP with TLS (requires `remote` feature)
3. **SSH** - SSH tunnels (requires `ssh` feature)
4. **Mosh** - UDP-based stateful protocol (requires `mosh` feature)

## Features

### SSH Transport

SSH transport creates an encrypted tunnel to a remote Ferrix server over SSH.

**Benefits**:
- Leverage existing SSH infrastructure
- Strong authentication (password, key, agent)
- Automatic encryption
- Port forwarding to local Ferrix server

**Usage**:

```bash
# Build with SSH support
cargo build --features ssh

# Connect via SSH tunnel
ferrix attach --transport ssh \
  --ssh-host example.com \
  --ssh-user myuser \
  --ssh-key ~/.ssh/id_rsa \
  --forward-port 7878 \
  my-session
```

**Configuration**:

```toml
# ferrix.toml
[transport.ssh]
host = "example.com"
port = 22
username = "myuser"
auth = "PublicKey"
private_key_path = "~/.ssh/id_rsa"
forward_port = 7878
```

**Authentication Methods**:

1. **Password**: Simple password authentication
   ```rust
   SshAuth::Password("my_password".to_string())
   ```

2. **Public Key**: SSH key-based authentication
   ```rust
   SshAuth::PublicKey {
       private_key_path: "~/.ssh/id_rsa".to_string(),
       passphrase: Some("key_passphrase".to_string()),
   }
   ```

3. **Agent**: Use SSH agent for authentication
   ```rust
   SshAuth::Agent
   ```

### Mosh Transport

Mosh-inspired UDP transport provides better connectivity over unreliable networks.

**Benefits**:
- Tolerates network changes (IP address switching)
- Better performance on high-latency connections
- Automatic keepalive
- Packet retransmission
- State synchronization

**Usage**:

```bash
# Build with Mosh support
cargo build --features mosh

# Connect via Mosh
ferrix attach --transport mosh \
  --mosh-host example.com \
  --mosh-port 60001 \
  --mosh-key /path/to/key \
  my-session
```

**Configuration**:

```toml
# ferrix.toml
[transport.mosh]
host = "example.com"
port = 60001
key_file = "~/.ferrix/mosh.key"
keepalive_interval = 1000  # ms
retransmit_timeout = 100   # ms
```

## Transport API

All transports implement the `Transport` trait:

```rust
#[async_trait]
pub trait Transport: Send + Sync {
    async fn connect(&mut self) -> Result<()>;
    async fn send(&mut self, data: Bytes) -> Result<()>;
    async fn receive(&mut self) -> Result<Bytes>;
    async fn close(&mut self) -> Result<()>;
    fn is_connected(&self) -> bool;
    fn remote_addr(&self) -> Option<SocketAddr>;
    fn stats(&self) -> TransportStats;
}
```

## Statistics

All transports track performance metrics:

```rust
pub struct TransportStats {
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub packets_sent: u64,
    pub packets_received: u64,
    pub latency_ms: Option<u64>,
    pub packet_loss_rate: f32,
}
```

Access stats via:

```bash
ferrix transport-stats my-session
```

## Examples

### SSH Tunnel Example

```rust
use ferrix::transport::{Transport, ssh::SshTransport, SshAuth};

let mut transport = SshTransport::new(
    "example.com".to_string(),
    22,
    "myuser".to_string(),
    SshAuth::PublicKey {
        private_key_path: "~/.ssh/id_rsa".to_string(),
        passphrase: None,
    },
    7878,  // Forward port
);

transport.connect().await?;
transport.send(data).await?;
let response = transport.receive().await?;
transport.close().await?;
```

### Mosh Example

```rust
use ferrix::transport::{Transport, mosh::MoshTransport};

let key = std::fs::read("~/.ferrix/mosh.key")?;

let mut transport = MoshTransport::new(
    "example.com".to_string(),
    60001,
    key,
);

transport.connect().await?;
transport.send(data).await?;
let response = transport.receive().await?;

// Check stats
let stats = transport.stats();
println!("Packet loss: {:.2}%", stats.packet_loss_rate * 100.0);

transport.close().await?;
```

## Security Considerations

### SSH
- Uses standard SSH security (OpenSSH-compatible)
- Supports all OpenSSH authentication methods
- No additional encryption needed (SSH provides it)

### Mosh
- **IMPORTANT**: Current implementation uses simple XOR encryption
- **Production use requires**: ChaCha20-Poly1305 or AES-GCM
- Key exchange should use Diffie-Hellman
- Implement perfect forward secrecy

## Performance

### SSH
- Overhead: ~5-10% due to SSH encryption
- Latency: +0-2ms on local network
- Good for: Stable connections, existing SSH infrastructure

### Mosh
- Overhead: ~2-5% due to UDP + state sync
- Latency: -5-20ms on lossy networks (better than TCP)
- Packet loss tolerance: Up to 30%
- Good for: Mobile, WiFi, high-latency connections

## Troubleshooting

### SSH Connection Issues

```bash
# Check SSH connectivity
ssh -v user@host

# Verify port forwarding
ssh -L 7878:localhost:7878 user@host

# Check Ferrix server is running
ferrix list
```

### Mosh Connection Issues

```bash
# Check UDP port is open
nc -u example.com 60001

# Verify key file
ls -la ~/.ferrix/mosh.key

# Check packet loss
ferrix transport-stats --show-loss my-session
```

## Future Enhancements

- [ ] WebRTC transport for NAT traversal
- [ ] QUIC transport for modern HTTP/3-based connections
- [ ] Wireguard integration
- [ ] Automatic transport fallback (try Mosh, fall back to SSH, fall back to TCP)
- [ ] Transport-level compression
- [ ] Multi-path transport (simultaneous WiFi + cellular)

## Related Documentation

- [Remote Sessions](./commands.md#remote-sessions)
- [Security Audit](./SECURITY_AUDIT.md)
- [Configuration](./configuration.md)
