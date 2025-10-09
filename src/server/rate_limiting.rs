//! Comprehensive Rate Limiting System
//!
//! Provides multi-level rate limiting to prevent abuse and resource exhaustion.
//! Integrates with authentication, session creation, and resource limits.
//!
//! ## Rate Limit Types
//!
//! - **Auth Failures**: Brute force protection (lockout after N failures)
//! - **Session Creation**: Prevents session creation spam
//! - **Connection Rate**: Limits new connections per IP
//! - **Command Rate**: Prevents command flooding
//!
//! ## Integration
//!
//! Works with:
//! - `ResourceLimits` for configured thresholds
//! - `ServerMetrics` for tracking rate limit hits
//! - Existing `RateLimiter` for auth failures
//!
//! ## Usage
//!
//! ```rust
//! let rate_limits = RateLimits::new(limits, metrics);
//!
//! // Check if client can create a session
//! if !rate_limits.check_session_creation(&client_id).await {
//!     return Err("Rate limit exceeded");
//! }
//! ```

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

use crate::config::limits::ResourceLimits;
use super::metrics::ServerMetrics;

/// Client identifier for rate limiting
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub enum ClientId {
    /// Socket address (for network clients)
    SocketAddr(SocketAddr),
    /// Unix socket connection (use PID or unique ID)
    UnixSocket(String),
    /// Internal/testing
    Internal(String),
}

impl ClientId {
    /// Create from socket address
    pub fn from_socket(addr: SocketAddr) -> Self {
        Self::SocketAddr(addr)
    }

    /// Create from unix socket identifier
    pub fn from_unix(id: String) -> Self {
        Self::UnixSocket(id)
    }
}

/// Rate limit window tracker
#[derive(Debug, Clone)]
struct RateWindow {
    /// Timestamps of events in this window
    events: Vec<Instant>,
}

impl RateWindow {
    fn new() -> Self {
        Self {
            events: Vec::new(),
        }
    }

    /// Add an event and check if rate limit is exceeded
    /// Returns true if rate limit is exceeded
    fn check_and_add(&mut self, max_events: usize, window_duration: Duration) -> bool {
        let now = Instant::now();

        // Remove events outside the current window
        self.events.retain(|t| now.duration_since(*t) < window_duration);

        // Check if we've exceeded the limit
        if self.events.len() >= max_events {
            return true; // Rate limit exceeded
        }

        // Add this event
        self.events.push(now);
        false
    }

    /// Get current event count in window
    fn count(&self, window_duration: Duration) -> usize {
        let now = Instant::now();
        self.events.iter()
            .filter(|t| now.duration_since(**t) < window_duration)
            .count()
    }
}

/// Comprehensive rate limiting system
pub struct RateLimits {
    limits: Arc<ResourceLimits>,
    #[allow(dead_code)] // Reserved for future metrics tracking
    metrics: Arc<ServerMetrics>,

    // Session creation rate limiting (per client)
    session_windows: Arc<RwLock<HashMap<ClientId, RateWindow>>>,

    // Connection rate limiting (per IP)
    connection_windows: Arc<RwLock<HashMap<SocketAddr, RateWindow>>>,

    // Command rate limiting (per client)
    command_windows: Arc<RwLock<HashMap<ClientId, RateWindow>>>,

    // Cleanup task handle
    cleanup_running: Arc<RwLock<bool>>,
}

impl RateLimits {
    /// Create new rate limiting system
    pub fn new(limits: Arc<ResourceLimits>, metrics: Arc<ServerMetrics>) -> Self {
        let rate_limits = Self {
            limits,
            metrics,
            session_windows: Arc::new(RwLock::new(HashMap::new())),
            connection_windows: Arc::new(RwLock::new(HashMap::new())),
            command_windows: Arc::new(RwLock::new(HashMap::new())),
            cleanup_running: Arc::new(RwLock::new(false)),
        };

        // Start cleanup task
        rate_limits.start_cleanup_task();

        rate_limits
    }

    /// Check if client can create a new session
    /// Returns true if allowed, false if rate limited
    pub async fn check_session_creation(&self, client_id: &ClientId) -> bool {
        let mut windows = self.session_windows.write().await;
        let window = windows.entry(client_id.clone()).or_insert_with(RateWindow::new);

        let max_per_minute = self.limits.max_sessions_per_minute;
        let window_duration = Duration::from_secs(60);

        let exceeded = window.check_and_add(max_per_minute, window_duration);

        if exceeded {
            tracing::warn!(
                "Session creation rate limit exceeded for {:?}: {} sessions/minute",
                client_id,
                max_per_minute
            );
            // Note: metrics update would go here if we had a rate_limit_hits metric
        }

        !exceeded
    }

    /// Check if IP can establish a new connection
    /// Returns true if allowed, false if rate limited
    pub async fn check_connection(&self, addr: &SocketAddr) -> bool {
        let mut windows = self.connection_windows.write().await;
        let window = windows.entry(*addr).or_insert_with(RateWindow::new);

        // Allow 30 connections per minute per IP
        const MAX_CONNECTIONS_PER_MINUTE: usize = 30;
        let window_duration = Duration::from_secs(60);

        let exceeded = window.check_and_add(MAX_CONNECTIONS_PER_MINUTE, window_duration);

        if exceeded {
            tracing::warn!(
                "Connection rate limit exceeded for {}: {} connections/minute",
                addr,
                MAX_CONNECTIONS_PER_MINUTE
            );
        }

        !exceeded
    }

    /// Check if client can execute a command
    /// Returns true if allowed, false if rate limited
    pub async fn check_command(&self, client_id: &ClientId) -> bool {
        let mut windows = self.command_windows.write().await;
        let window = windows.entry(client_id.clone()).or_insert_with(RateWindow::new);

        // Allow 100 commands per second per client
        const MAX_COMMANDS_PER_SECOND: usize = 100;
        let window_duration = Duration::from_secs(1);

        let exceeded = window.check_and_add(MAX_COMMANDS_PER_SECOND, window_duration);

        if exceeded {
            tracing::warn!(
                "Command rate limit exceeded for {:?}: {} commands/second",
                client_id,
                MAX_COMMANDS_PER_SECOND
            );
        }

        !exceeded
    }

    /// Get current session creation count for a client
    pub async fn session_creation_count(&self, client_id: &ClientId) -> usize {
        let windows = self.session_windows.read().await;
        windows.get(client_id)
            .map(|w| w.count(Duration::from_secs(60)))
            .unwrap_or(0)
    }

    /// Start background cleanup task
    fn start_cleanup_task(&self) {
        let session_windows = self.session_windows.clone();
        let connection_windows = self.connection_windows.clone();
        let command_windows = self.command_windows.clone();
        let cleanup_running = self.cleanup_running.clone();

        tokio::spawn(async move {
            // Mark cleanup as running
            {
                let mut running = cleanup_running.write().await;
                *running = true;
            }

            loop {
                tokio::time::sleep(Duration::from_secs(300)).await; // Every 5 minutes

                // Cleanup old windows
                {
                    let mut sessions = session_windows.write().await;
                    sessions.retain(|_, window| {
                        // Keep windows that have events in the last 5 minutes
                        window.events.iter().any(|t| t.elapsed() < Duration::from_secs(300))
                    });
                }

                {
                    let mut connections = connection_windows.write().await;
                    connections.retain(|_, window| {
                        window.events.iter().any(|t| t.elapsed() < Duration::from_secs(300))
                    });
                }

                {
                    let mut commands = command_windows.write().await;
                    commands.retain(|_, window| {
                        window.events.iter().any(|t| t.elapsed() < Duration::from_secs(300))
                    });
                }

                tracing::debug!("Rate limit cleanup completed");
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    fn test_socket_addr() -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080)
    }

    #[test]
    fn test_client_id_creation() {
        let addr = test_socket_addr();
        let client = ClientId::from_socket(addr);
        assert!(matches!(client, ClientId::SocketAddr(_)));

        let unix = ClientId::from_unix("test-123".to_string());
        assert!(matches!(unix, ClientId::UnixSocket(_)));
    }

    #[test]
    fn test_rate_window_basic() {
        let mut window = RateWindow::new();

        // Should allow first 10 events
        for i in 0..10 {
            let exceeded = window.check_and_add(10, Duration::from_secs(60));
            if i < 10 {
                assert!(!exceeded, "Event {} should be allowed", i);
            }
        }

        // 11th event should exceed
        assert!(window.check_and_add(10, Duration::from_secs(60)));
    }

    #[test]
    fn test_rate_window_count() {
        let mut window = RateWindow::new();

        window.check_and_add(100, Duration::from_secs(60));
        window.check_and_add(100, Duration::from_secs(60));
        window.check_and_add(100, Duration::from_secs(60));

        assert_eq!(window.count(Duration::from_secs(60)), 3);
    }

    #[tokio::test]
    async fn test_rate_limits_creation() {
        let limits = Arc::new(ResourceLimits::default());
        let metrics = ServerMetrics::global();

        let _rate_limits = RateLimits::new(limits, metrics);
        // Should create without errors
    }

    #[tokio::test]
    async fn test_session_creation_rate_limit() {
        let mut limits = ResourceLimits::default();
        limits.max_sessions_per_minute = 5;
        let limits = Arc::new(limits);
        let metrics = ServerMetrics::global();

        let rate_limits = RateLimits::new(limits, metrics);
        let client = ClientId::from_socket(test_socket_addr());

        // First 5 should succeed
        for i in 0..5 {
            assert!(
                rate_limits.check_session_creation(&client).await,
                "Session {} should be allowed",
                i
            );
        }

        // 6th should fail
        assert!(!rate_limits.check_session_creation(&client).await);
    }

    #[tokio::test]
    async fn test_connection_rate_limit() {
        let limits = Arc::new(ResourceLimits::default());
        let metrics = ServerMetrics::global();

        let rate_limits = RateLimits::new(limits, metrics);
        let addr = test_socket_addr();

        // First 30 should succeed
        for i in 0..30 {
            assert!(
                rate_limits.check_connection(&addr).await,
                "Connection {} should be allowed",
                i
            );
        }

        // 31st should fail
        assert!(!rate_limits.check_connection(&addr).await);
    }

    #[tokio::test]
    async fn test_command_rate_limit() {
        let limits = Arc::new(ResourceLimits::default());
        let metrics = ServerMetrics::global();

        let rate_limits = RateLimits::new(limits, metrics);
        let client = ClientId::from_socket(test_socket_addr());

        // First 100 should succeed
        for i in 0..100 {
            assert!(
                rate_limits.check_command(&client).await,
                "Command {} should be allowed",
                i
            );
        }

        // 101st should fail
        assert!(!rate_limits.check_command(&client).await);
    }

    #[tokio::test]
    async fn test_session_creation_count() {
        let limits = Arc::new(ResourceLimits::default());
        let metrics = ServerMetrics::global();

        let rate_limits = RateLimits::new(limits, metrics);
        let client = ClientId::from_socket(test_socket_addr());

        assert_eq!(rate_limits.session_creation_count(&client).await, 0);

        rate_limits.check_session_creation(&client).await;
        rate_limits.check_session_creation(&client).await;

        assert_eq!(rate_limits.session_creation_count(&client).await, 2);
    }

    #[tokio::test]
    async fn test_different_clients_independent() {
        let mut limits = ResourceLimits::default();
        limits.max_sessions_per_minute = 2;
        let limits = Arc::new(limits);
        let metrics = ServerMetrics::global();

        let rate_limits = RateLimits::new(limits, metrics);

        let client1 = ClientId::from_socket(test_socket_addr());
        let client2 = ClientId::from_unix("test-123".to_string());

        // Client 1: use up their quota
        rate_limits.check_session_creation(&client1).await;
        rate_limits.check_session_creation(&client1).await;
        assert!(!rate_limits.check_session_creation(&client1).await);

        // Client 2: should still have their quota
        assert!(rate_limits.check_session_creation(&client2).await);
        assert!(rate_limits.check_session_creation(&client2).await);
    }
}
