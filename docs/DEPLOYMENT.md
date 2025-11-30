# Ferrix Deployment Guide

**Version**: 1.0.0
**Last Updated**: 2025-10-05
**Target Audience**: System administrators deploying Ferrix in production environments

## Table of Contents

1. [Overview](#overview)
2. [System Requirements](#system-requirements)
3. [Installation Methods](#installation-methods)
4. [Production Configuration](#production-configuration)
5. [Security Best Practices](#security-best-practices)
6. [Service Management](#service-management)
7. [Monitoring & Logging](#monitoring--logging)
8. [Backup & Recovery](#backup--recovery)
9. [Troubleshooting](#troubleshooting)

---

## Overview

Ferrix is a modern terminal multiplexer built with Rust, designed for both local and remote session management. This guide covers production deployment scenarios including:

- Single-user local deployments
- Multi-user server deployments
- Remote access with TLS
- High-availability configurations

## System Requirements

### Minimum Requirements

- **OS**: Linux (kernel 3.10+), macOS (10.15+), FreeBSD
- **CPU**: 1 core (2+ recommended for multiple sessions)
- **RAM**: 100MB base + 10-50MB per active session
- **Disk**: 20MB binary + space for session data and logs

### Recommended Requirements

- **CPU**: 2+ cores for >10 concurrent sessions
- **RAM**: 512MB+ for production workloads
- **Disk**: 100MB+ for logs and snapshots (SSD recommended)

### Dependencies

**Runtime** (no external dependencies required):
- Ferrix is statically linked - single binary deployment

**Optional**:
- `systemd` or `launchd` for service management (recommended)
- Valid TLS certificates for remote access
- Monitoring tools (Prometheus, Grafana, etc.)

---

## Installation Methods

### Method 1: From Source (Recommended for Production)

```bash
# Install Rust toolchain
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env

# Clone repository
git clone https://github.com/davidliedle/Ferrix.git
cd Ferrix

# Build release binary (optimized)
cargo build --release

# Install to system (requires sudo)
sudo cp target/release/ferrix /usr/local/bin/
sudo chmod +x /usr/local/bin/ferrix

# Verify installation
ferrix --version
```

### Method 2: Pre-built Binaries

```bash
# Download latest release (replace VERSION)
VERSION="1.0.0"
wget https://github.com/davidliedle/Ferrix/releases/download/v${VERSION}/ferrix-linux-x86_64

# Install
sudo mv ferrix-linux-x86_64 /usr/local/bin/ferrix
sudo chmod +x /usr/local/bin/ferrix
```

### Method 3: Package Managers

```bash
# Homebrew (macOS/Linux)
brew install ferrix

# Cargo
cargo install ferrix

# Arch Linux (AUR)
yay -S ferrix
```

---

## Production Configuration

### Directory Structure

Ferrix uses the following directory layout:

```
~/.ferrix/
├── ferrix.sock          # Unix socket (local connections)
├── ferrix.out           # Daemon stdout log
├── ferrix.err           # Daemon stderr log
├── ferrix.pid           # Process ID file
├── config/
│   ├── ferrix.toml      # Main configuration
│   ├── keybindings.toml # Custom key bindings
│   └── sessions/        # Session-specific configs
├── snapshots/           # Session snapshots
├── logs/                # Application logs
└── plugins/             # WASM plugins
```

### Main Configuration (`~/.ferrix/config/ferrix.toml`)

```toml
# Ferrix Production Configuration

[general]
# Socket path (Unix domain socket for local connections)
socket_path = "/var/run/ferrix/ferrix.sock"

# Default shell
default_shell = "/bin/bash"

# Enable activity monitoring
enable_activity_monitoring = true

# Auto-save interval (minutes, 0 to disable)
auto_save_interval = 15

[server]
# Bind address for remote access (empty = local only)
bind_address = "0.0.0.0:7777"

# Enable TLS for remote connections
tls_enabled = true
tls_cert_path = "/etc/ferrix/certs/server.crt"
tls_key_path = "/etc/ferrix/certs/server.key"

# Authentication
require_authentication = true
max_auth_attempts = 5
lockout_duration_minutes = 15

[logging]
# Log level: trace, debug, info, warn, error
level = "info"

# Log rotation
max_log_size_mb = 100
max_log_files = 10

[performance]
# Maximum concurrent sessions (0 = unlimited)
max_sessions = 100

# Scrollback buffer size per pane (lines)
scrollback_lines = 10000

[security]
# Session lock timeout (minutes, 0 = disabled)
lock_timeout = 30

# Require password for session unlock
require_unlock_password = true
```

### Creating Configuration

```bash
# Generate default config
ferrix config init

# Edit configuration
$EDITOR ~/.ferrix/config/ferrix.toml
```

---

## Security Best Practices

### 1. Local Deployment Security

**File Permissions**:
```bash
# Secure Ferrix directory
chmod 700 ~/.ferrix
chmod 600 ~/.ferrix/config/ferrix.toml

# Secure socket (if system-wide)
sudo chown root:ferrix /var/run/ferrix/ferrix.sock
sudo chmod 660 /var/run/ferrix/ferrix.sock
```

**User Isolation**:
```bash
# Create dedicated user for multi-user systems
sudo useradd -r -s /bin/false ferrix
sudo mkdir -p /var/lib/ferrix
sudo chown ferrix:ferrix /var/lib/ferrix
```

### 2. Remote Access Security

**⚠️ IMPORTANT**: See [DEPENDENCY_AUDIT.md](../DEPENDENCY_AUDIT.md) for known security advisories.

**TLS Certificate Setup**:
```bash
# Generate self-signed certificate (testing only)
openssl req -x509 -newkey rsa:4096 -keyout server.key \
  -out server.crt -days 365 -nodes \
  -subj "/CN=ferrix.example.com"

# Production: Use Let's Encrypt
sudo certbot certonly --standalone -d ferrix.example.com

# Install certificates
sudo mkdir -p /etc/ferrix/certs
sudo cp server.crt server.key /etc/ferrix/certs/
sudo chmod 600 /etc/ferrix/certs/server.key
```

**Firewall Configuration**:
```bash
# UFW (Ubuntu/Debian)
sudo ufw allow 7777/tcp comment 'Ferrix remote access'

# firewalld (RHEL/CentOS)
sudo firewall-cmd --permanent --add-port=7777/tcp
sudo firewall-cmd --reload

# iptables
sudo iptables -A INPUT -p tcp --dport 7777 -j ACCEPT
```

**User Management**:
```bash
# Add remote user
ferrix user add admin

# Set strong password
ferrix user passwd admin

# List users
ferrix user list
```

---

## Service Management

### systemd (Recommended for Linux)

**System Service** (`/etc/systemd/system/ferrix.service`):
```ini
[Unit]
Description=Ferrix Terminal Multiplexer Server
After=network.target

[Service]
Type=forking
User=ferrix
Group=ferrix
WorkingDirectory=/var/lib/ferrix
ExecStart=/usr/local/bin/ferrix server --daemonize
ExecStop=/usr/local/bin/ferrix kill-server
Restart=on-failure
RestartSec=5s

# Security hardening
PrivateTmp=true
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/var/lib/ferrix /var/log/ferrix

# Resource limits
LimitNOFILE=65536
MemoryLimit=2G
CPUQuota=200%

[Install]
WantedBy=multi-user.target
```

**User Service** (`~/.config/systemd/user/ferrix.service`):
```ini
[Unit]
Description=Ferrix Terminal Multiplexer (User)
After=default.target

[Service]
Type=simple
ExecStart=%h/.cargo/bin/ferrix server --foreground
Restart=on-failure

[Install]
WantedBy=default.target
```

**Management Commands**:
```bash
# System service
sudo systemctl enable ferrix
sudo systemctl start ferrix
sudo systemctl status ferrix

# User service
systemctl --user enable ferrix
systemctl --user start ferrix
```

### launchd (macOS)

**Plist** (`~/Library/LaunchAgents/com.ferrix.server.plist`):
```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.ferrix.server</string>
    <key>ProgramArguments</key>
    <array>
        <string>/usr/local/bin/ferrix</string>
        <string>server</string>
        <string>--foreground</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>/tmp/ferrix.out</string>
    <key>StandardErrorPath</key>
    <string>/tmp/ferrix.err</string>
</dict>
</plist>
```

**Load Service**:
```bash
launchctl load ~/Library/LaunchAgents/com.ferrix.server.plist
launchctl start com.ferrix.server
```

---

## Monitoring & Logging

### Logging Configuration

**Structured Logging** (using tracing):
```bash
# Set log level via environment
export RUST_LOG=ferrix=info

# Per-module logging
export RUST_LOG=ferrix::server=debug,ferrix::client=info
```

**Log Files**:
- `~/.ferrix/ferrix.out` - Stdout capture
- `~/.ferrix/ferrix.err` - Stderr capture
- Application logs via `tracing-subscriber`

### Monitoring Metrics

**Built-in Status**:
```bash
# Session statistics
ferrix list

# Server status
ferrix status

# Resource usage per session
ferrix stats <session-name>
```

### Integration with Monitoring Systems

**Prometheus Export** (future feature):
```yaml
# prometheus.yml
scrape_configs:
  - job_name: 'ferrix'
    static_configs:
      - targets: ['localhost:7778']
```

**Health Checks**:
```bash
#!/bin/bash
# /usr/local/bin/ferrix-health-check.sh

if ! ferrix ping >/dev/null 2>&1; then
    echo "Ferrix server not responding"
    exit 1
fi

SESSION_COUNT=$(ferrix list | wc -l)
if [ $SESSION_COUNT -eq 0 ]; then
    echo "No active sessions"
    exit 0
fi

echo "Ferrix healthy: $SESSION_COUNT sessions"
exit 0
```

---

## Backup & Recovery

### What to Backup

**Essential**:
- `~/.ferrix/config/` - All configuration files
- `~/.ferrix/snapshots/` - Session snapshots

**Optional**:
- `~/.ferrix/logs/` - Historical logs (if needed)
- `~/.ferrix/plugins/` - Installed plugins

**Do Not Backup**:
- `~/.ferrix/ferrix.sock` - Runtime socket
- `~/.ferrix/ferrix.pid` - Process ID

### Backup Script

```bash
#!/bin/bash
# /usr/local/bin/ferrix-backup.sh

BACKUP_DIR="/var/backups/ferrix"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)

mkdir -p "$BACKUP_DIR"

# Create snapshot of all sessions
for session in $(ferrix list | awk '{print $1}'); do
    ferrix save-snapshot "$session" --name "backup_${TIMESTAMP}"
done

# Backup configuration and snapshots
tar -czf "$BACKUP_DIR/ferrix_${TIMESTAMP}.tar.gz" \
    ~/.ferrix/config \
    ~/.ferrix/snapshots

# Retention (keep last 30 days)
find "$BACKUP_DIR" -name "ferrix_*.tar.gz" -mtime +30 -delete

echo "Backup completed: $BACKUP_DIR/ferrix_${TIMESTAMP}.tar.gz"
```

### Recovery Procedure

```bash
# 1. Stop Ferrix
systemctl stop ferrix

# 2. Restore from backup
tar -xzf /var/backups/ferrix/ferrix_20251005_120000.tar.gz -C ~/

# 3. Restart Ferrix
systemctl start ferrix

# 4. Restore sessions from snapshots
ferrix list-snapshots
ferrix load-snapshot ~/.ferrix/snapshots/session_20251005_120000.ferrix.snapshot
```

---

## Troubleshooting

### Server Won't Start

**Check logs**:
```bash
cat ~/.ferrix/ferrix.err
journalctl -u ferrix -n 50
```

**Common Issues**:

1. **Socket already in use**:
```bash
# Remove stale socket
rm ~/.ferrix/ferrix.sock
# Or change socket path in config
```

2. **Permission denied**:
```bash
# Fix permissions
chmod 700 ~/.ferrix
chmod 600 ~/.ferrix/config/*.toml
```

3. **Port already in use** (remote mode):
```bash
# Check what's using port 7777
sudo lsof -i :7777
# Kill or change Ferrix port in config
```

### Connection Issues

**Cannot connect to server**:
```bash
# Check server is running
ps aux | grep ferrix

# Check socket exists
ls -l ~/.ferrix/ferrix.sock

# Test connection
ferrix ping
```

**Remote connection fails**:
```bash
# Check firewall
sudo ufw status

# Check TLS certificates
openssl x509 -in /etc/ferrix/certs/server.crt -text -noout

# Test TLS connection
openssl s_client -connect localhost:7777
```

### Performance Issues

**High memory usage**:
```bash
# Check session count
ferrix list

# Reduce scrollback buffer in config
# scrollback_lines = 5000

# Kill idle sessions
ferrix kill <session-name>
```

**Slow rendering**:
- Reduce scrollback size in configuration (fewer lines to render)
- Avoid running extremely verbose commands in many panes simultaneously
- Check terminal emulator compatibility and performance settings

### Session Recovery

**Session lost after crash**:
```bash
# Check recovery files
ls ~/.ferrix/recovery/

# List available snapshots
ferrix list-snapshots

# Restore from snapshot
ferrix load-snapshot <snapshot-path>
```

---

## Production Checklist

Before deploying to production:

- [ ] Read [SECURITY_AUDIT.md](../SECURITY_AUDIT.md) and [DEPENDENCY_AUDIT.md](../DEPENDENCY_AUDIT.md)
- [ ] Configure TLS certificates (if remote access)
- [ ] Set up user authentication
- [ ] Configure firewall rules
- [ ] Enable systemd/launchd service
- [ ] Set up log rotation
- [ ] Configure automated backups
- [ ] Test recovery procedures
- [ ] Set up monitoring/alerting
- [ ] Document customizations
- [ ] Train users on Ferrix usage

## Getting Help

- **Documentation**: https://github.com/davidliedle/Ferrix/tree/main/docs
- **Issues**: https://github.com/davidliedle/Ferrix/issues
- **Security**: See [SECURITY_AUDIT.md](../SECURITY_AUDIT.md) for reporting vulnerabilities

---

**Version History**:
- 1.0.0 (2025-10-05) - Initial production deployment guide
