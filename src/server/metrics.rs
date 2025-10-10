//! Production Metrics Infrastructure
//!
//! Provides comprehensive metrics collection for monitoring Ferrix in production.
//! Metrics are collected using atomic operations for lock-free performance.
//!
//! ## Metric Categories
//!
//! - **Connection Metrics**: Active connections, connection lifecycle
//! - **Session Metrics**: Active sessions, session lifecycle
//! - **Performance Metrics**: PTY throughput, message latency
//! - **Resource Metrics**: Memory usage, file descriptors, CPU
//!
//! ## Usage
//!
//! ```rust
//! let metrics = ServerMetrics::global();
//! metrics.connection_opened();
//! metrics.pty_bytes_written(1024);
//! ```

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use once_cell::sync::Lazy;

/// Global metrics instance
static GLOBAL_METRICS: Lazy<Arc<ServerMetrics>> = Lazy::new(|| {
    Arc::new(ServerMetrics::new())
});

/// Server-wide metrics collection
#[derive(Debug)]
pub struct ServerMetrics {
    // Connection metrics
    active_connections: AtomicUsize,
    total_connections: AtomicU64,
    failed_connections: AtomicU64,

    // Session metrics
    active_sessions: AtomicUsize,
    sessions_created: AtomicU64,
    sessions_destroyed: AtomicU64,

    // Window/Pane metrics
    active_windows: AtomicUsize,
    active_panes: AtomicUsize,

    // Performance metrics
    pty_bytes_read: AtomicU64,
    pty_bytes_written: AtomicU64,
    messages_sent: AtomicU64,
    messages_received: AtomicU64,

    // Error metrics
    pty_spawn_failures: AtomicU64,
    protocol_errors: AtomicU64,
    auth_failures: AtomicU64,
}

impl ServerMetrics {
    /// Create new metrics instance
    pub fn new() -> Self {
        Self {
            active_connections: AtomicUsize::new(0),
            total_connections: AtomicU64::new(0),
            failed_connections: AtomicU64::new(0),

            active_sessions: AtomicUsize::new(0),
            sessions_created: AtomicU64::new(0),
            sessions_destroyed: AtomicU64::new(0),

            active_windows: AtomicUsize::new(0),
            active_panes: AtomicUsize::new(0),

            pty_bytes_read: AtomicU64::new(0),
            pty_bytes_written: AtomicU64::new(0),
            messages_sent: AtomicU64::new(0),
            messages_received: AtomicU64::new(0),

            pty_spawn_failures: AtomicU64::new(0),
            protocol_errors: AtomicU64::new(0),
            auth_failures: AtomicU64::new(0),
        }
    }

    /// Get global metrics instance
    pub fn global() -> Arc<Self> {
        GLOBAL_METRICS.clone()
    }

    // Connection lifecycle

    pub fn connection_opened(&self) {
        self.active_connections.fetch_add(1, Ordering::Relaxed);
        self.total_connections.fetch_add(1, Ordering::Relaxed);
    }

    pub fn connection_closed(&self) {
        self.active_connections.fetch_sub(1, Ordering::Relaxed);
    }

    pub fn connection_failed(&self) {
        self.failed_connections.fetch_add(1, Ordering::Relaxed);
    }

    // Session lifecycle

    pub fn session_created(&self) {
        self.active_sessions.fetch_add(1, Ordering::Relaxed);
        self.sessions_created.fetch_add(1, Ordering::Relaxed);
    }

    pub fn session_destroyed(&self) {
        self.active_sessions.fetch_sub(1, Ordering::Relaxed);
        self.sessions_destroyed.fetch_add(1, Ordering::Relaxed);
    }

    // Window/Pane tracking

    pub fn window_created(&self) {
        self.active_windows.fetch_add(1, Ordering::Relaxed);
    }

    pub fn window_destroyed(&self) {
        self.active_windows.fetch_sub(1, Ordering::Relaxed);
    }

    pub fn pane_created(&self) {
        self.active_panes.fetch_add(1, Ordering::Relaxed);
    }

    pub fn pane_destroyed(&self) {
        self.active_panes.fetch_sub(1, Ordering::Relaxed);
    }

    // Performance tracking

    pub fn pty_bytes_read(&self, bytes: u64) {
        self.pty_bytes_read.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn pty_bytes_written(&self, bytes: u64) {
        self.pty_bytes_written.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn message_sent(&self) {
        self.messages_sent.fetch_add(1, Ordering::Relaxed);
    }

    pub fn message_received(&self) {
        self.messages_received.fetch_add(1, Ordering::Relaxed);
    }

    // Error tracking

    pub fn pty_spawn_failed(&self) {
        self.pty_spawn_failures.fetch_add(1, Ordering::Relaxed);
    }

    pub fn protocol_error(&self) {
        self.protocol_errors.fetch_add(1, Ordering::Relaxed);
    }

    pub fn auth_failed(&self) {
        self.auth_failures.fetch_add(1, Ordering::Relaxed);
    }

    // Snapshot for reporting

    /// Get current snapshot of all metrics
    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            active_connections: self.active_connections.load(Ordering::Relaxed),
            total_connections: self.total_connections.load(Ordering::Relaxed),
            failed_connections: self.failed_connections.load(Ordering::Relaxed),

            active_sessions: self.active_sessions.load(Ordering::Relaxed),
            sessions_created: self.sessions_created.load(Ordering::Relaxed),
            sessions_destroyed: self.sessions_destroyed.load(Ordering::Relaxed),

            active_windows: self.active_windows.load(Ordering::Relaxed),
            active_panes: self.active_panes.load(Ordering::Relaxed),

            pty_bytes_read: self.pty_bytes_read.load(Ordering::Relaxed),
            pty_bytes_written: self.pty_bytes_written.load(Ordering::Relaxed),
            messages_sent: self.messages_sent.load(Ordering::Relaxed),
            messages_received: self.messages_received.load(Ordering::Relaxed),

            pty_spawn_failures: self.pty_spawn_failures.load(Ordering::Relaxed),
            protocol_errors: self.protocol_errors.load(Ordering::Relaxed),
            auth_failures: self.auth_failures.load(Ordering::Relaxed),
        }
    }
}

impl Default for ServerMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Point-in-time snapshot of metrics
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MetricsSnapshot {
    // Connections
    pub active_connections: usize,
    pub total_connections: u64,
    pub failed_connections: u64,

    // Sessions
    pub active_sessions: usize,
    pub sessions_created: u64,
    pub sessions_destroyed: u64,

    // Windows/Panes
    pub active_windows: usize,
    pub active_panes: usize,

    // Performance
    pub pty_bytes_read: u64,
    pub pty_bytes_written: u64,
    pub messages_sent: u64,
    pub messages_received: u64,

    // Errors
    pub pty_spawn_failures: u64,
    pub protocol_errors: u64,
    pub auth_failures: u64,
}

impl MetricsSnapshot {
    /// Format as human-readable string
    pub fn format(&self) -> String {
        format!(
            r#"Ferrix Server Metrics
====================

Connections:
  Active: {}
  Total: {}
  Failed: {}

Sessions:
  Active: {}
  Created: {}
  Destroyed: {}

Windows/Panes:
  Active Windows: {}
  Active Panes: {}

Performance:
  PTY Bytes Read: {} ({})
  PTY Bytes Written: {} ({})
  Messages Sent: {}
  Messages Received: {}

Errors:
  PTY Spawn Failures: {}
  Protocol Errors: {}
  Auth Failures: {}
"#,
            self.active_connections,
            self.total_connections,
            self.failed_connections,
            self.active_sessions,
            self.sessions_created,
            self.sessions_destroyed,
            self.active_windows,
            self.active_panes,
            self.pty_bytes_read,
            format_bytes(self.pty_bytes_read),
            self.pty_bytes_written,
            format_bytes(self.pty_bytes_written),
            self.messages_sent,
            self.messages_received,
            self.pty_spawn_failures,
            self.protocol_errors,
            self.auth_failures,
        )
    }
}

/// Format bytes in human-readable format
fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_creation() {
        let metrics = ServerMetrics::new();
        let snapshot = metrics.snapshot();

        assert_eq!(snapshot.active_connections, 0);
        assert_eq!(snapshot.total_connections, 0);
        assert_eq!(snapshot.active_sessions, 0);
    }

    #[test]
    fn test_connection_metrics() {
        let metrics = ServerMetrics::new();

        metrics.connection_opened();
        metrics.connection_opened();
        assert_eq!(metrics.snapshot().active_connections, 2);
        assert_eq!(metrics.snapshot().total_connections, 2);

        metrics.connection_closed();
        assert_eq!(metrics.snapshot().active_connections, 1);
        assert_eq!(metrics.snapshot().total_connections, 2); // Total never decreases

        metrics.connection_failed();
        assert_eq!(metrics.snapshot().failed_connections, 1);
    }

    #[test]
    fn test_session_metrics() {
        let metrics = ServerMetrics::new();

        metrics.session_created();
        metrics.session_created();
        assert_eq!(metrics.snapshot().active_sessions, 2);
        assert_eq!(metrics.snapshot().sessions_created, 2);

        metrics.session_destroyed();
        assert_eq!(metrics.snapshot().active_sessions, 1);
        assert_eq!(metrics.snapshot().sessions_destroyed, 1);
    }

    #[test]
    fn test_pty_metrics() {
        let metrics = ServerMetrics::new();

        metrics.pty_bytes_read(1024);
        metrics.pty_bytes_written(2048);

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.pty_bytes_read, 1024);
        assert_eq!(snapshot.pty_bytes_written, 2048);
    }

    #[test]
    fn test_error_metrics() {
        let metrics = ServerMetrics::new();

        metrics.pty_spawn_failed();
        metrics.protocol_error();
        metrics.auth_failed();

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.pty_spawn_failures, 1);
        assert_eq!(snapshot.protocol_errors, 1);
        assert_eq!(snapshot.auth_failures, 1);
    }

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(500), "500 B");
        assert_eq!(format_bytes(2048), "2.00 KB");
        assert_eq!(format_bytes(2 * 1024 * 1024), "2.00 MB");
        assert_eq!(format_bytes(3 * 1024 * 1024 * 1024), "3.00 GB");
    }

    #[test]
    fn test_global_metrics() {
        let metrics1 = ServerMetrics::global();
        let metrics2 = ServerMetrics::global();

        // Should be the same instance
        assert!(Arc::ptr_eq(&metrics1, &metrics2));
    }
}
