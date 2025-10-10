//! Server Infrastructure Integration
//!
//! Coordinates all production infrastructure components:
//! - Metrics collection
//! - Health checking
//! - Resource limits
//! - Backpressure monitoring
//! - Rate limiting
//!
//! This module provides a unified interface for the server to interact
//! with all rock-solid infrastructure in a coordinated way.

use std::sync::Arc;
use crate::config::limits::ResourceLimits;
use super::metrics::ServerMetrics;
use super::health::HealthChecker;
use super::backpressure::PressureMonitor;
use super::rate_limiting::{RateLimits, ClientId};
use super::circuit_breaker::CircuitBreakerManager;

/// Centralized infrastructure for production operations
#[derive(Clone)]
pub struct ServerInfrastructure {
    /// Metrics collection
    pub metrics: Arc<ServerMetrics>,

    /// Health monitoring
    pub health: Arc<HealthChecker>,

    /// Resource limits configuration
    pub limits: Arc<ResourceLimits>,

    /// Backpressure detection
    pub pressure: Arc<PressureMonitor>,

    /// Rate limiting
    pub rate_limits: Arc<RateLimits>,

    /// Circuit breakers for fault tolerance
    pub circuit_breakers: Arc<CircuitBreakerManager>,
}

impl ServerInfrastructure {
    /// Create new infrastructure with default configuration
    pub fn new() -> Self {
        let limits = Arc::new(ResourceLimits::default());
        let metrics = ServerMetrics::global();
        let health = Arc::new(HealthChecker::new());
        let pressure = Arc::new(PressureMonitor::new(limits.clone(), metrics.clone()));
        let rate_limits = Arc::new(RateLimits::new(limits.clone(), metrics.clone()));
        let circuit_breakers = Arc::new(CircuitBreakerManager::new());

        Self {
            metrics,
            health,
            limits,
            pressure,
            rate_limits,
            circuit_breakers,
        }
    }

    /// Create with custom resource limits
    pub fn with_limits(limits: ResourceLimits) -> Self {
        let limits = Arc::new(limits);
        let metrics = ServerMetrics::global();
        let health = Arc::new(HealthChecker::new());
        let pressure = Arc::new(PressureMonitor::new(limits.clone(), metrics.clone()));
        let rate_limits = Arc::new(RateLimits::new(limits.clone(), metrics.clone()));
        let circuit_breakers = Arc::new(CircuitBreakerManager::new());

        Self {
            metrics,
            health,
            limits,
            pressure,
            rate_limits,
            circuit_breakers,
        }
    }

    /// Check if server can accept a new session
    pub async fn can_create_session(&self, client_id: &ClientId, current_sessions: usize) -> bool {
        // Check resource limits
        if !self.limits.can_create_session(current_sessions) {
            tracing::warn!(
                "Session creation rejected: at limit ({}/{})",
                current_sessions,
                self.limits.max_concurrent_sessions
            );
            return false;
        }

        // Check backpressure
        let pressure_status = self.pressure.check_pressure().await;
        if pressure_status.should_reject_new_sessions() {
            tracing::warn!(
                "Session creation rejected due to system pressure: {:?}",
                pressure_status.level
            );
            return false;
        }

        // Check rate limiting
        if !self.rate_limits.check_session_creation(client_id).await {
            tracing::warn!(
                "Session creation rate limited for client {:?}",
                client_id
            );
            return false;
        }

        true
    }

    /// Check if server can accept a new client connection
    pub async fn can_accept_client(&self, current_clients: usize) -> bool {
        // Check resource limits
        if !self.limits.can_accept_client(current_clients) {
            tracing::warn!(
                "Client connection rejected: at limit ({}/{})",
                current_clients,
                self.limits.max_clients
            );
            return false;
        }

        // Check backpressure
        let pressure_status = self.pressure.check_pressure().await;
        if pressure_status.should_reject_new_sessions() {
            tracing::warn!(
                "Client connection rejected due to system pressure: {:?}",
                pressure_status.level
            );
            return false;
        }

        true
    }

    /// Record successful session creation
    pub fn record_session_created(&self) {
        self.metrics.session_created();
    }

    /// Record session destruction
    pub fn record_session_destroyed(&self) {
        self.metrics.session_destroyed();
    }

    /// Record client connection
    pub fn record_connection_opened(&self) {
        self.metrics.connection_opened();
    }

    /// Record client disconnection
    pub fn record_connection_closed(&self) {
        self.metrics.connection_closed();
    }

    /// Record window creation
    pub fn record_window_created(&self) {
        self.metrics.window_created();
    }

    /// Record window destruction
    pub fn record_window_destroyed(&self) {
        self.metrics.window_destroyed();
    }

    /// Record pane creation
    pub fn record_pane_created(&self) {
        self.metrics.pane_created();
    }

    /// Record pane destruction
    pub fn record_pane_destroyed(&self) {
        self.metrics.pane_destroyed();
    }

    /// Record PTY bytes read
    pub fn record_pty_bytes_read(&self, bytes: u64) {
        self.metrics.pty_bytes_read(bytes);
    }

    /// Record PTY bytes written
    pub fn record_pty_bytes_written(&self, bytes: u64) {
        self.metrics.pty_bytes_written(bytes);
    }

    /// Record protocol message sent
    pub fn record_message_sent(&self) {
        self.metrics.message_sent();
    }

    /// Record protocol message received
    pub fn record_message_received(&self) {
        self.metrics.message_received();
    }

    /// Get current metrics snapshot
    pub fn get_metrics(&self) -> super::metrics::MetricsSnapshot {
        self.metrics.snapshot()
    }

    /// Get current health status
    pub async fn get_health(&self) -> super::health::HealthStatus {
        self.health.check().await
    }

    /// Get current pressure status
    pub async fn get_pressure(&self) -> super::backpressure::PressureStatus {
        self.pressure.check_pressure().await
    }

    /// Check if a PTY operation is allowed through circuit breaker
    pub async fn is_pty_operation_allowed(&self) -> bool {
        self.circuit_breakers.is_request_allowed("pty").await
    }

    /// Record successful PTY operation
    pub async fn record_pty_success(&self) {
        self.circuit_breakers.record_success("pty").await;
    }

    /// Record failed PTY operation
    pub async fn record_pty_failure(&self) {
        self.circuit_breakers.record_failure("pty").await;
    }

    /// Get circuit breaker statistics
    pub async fn get_circuit_stats(&self) -> super::circuit_breaker::CircuitBreakerStats {
        self.circuit_breakers.get_stats().await
    }
}

impl Default for ServerInfrastructure {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    #[tokio::test]
    async fn test_infrastructure_creation() {
        let infra = ServerInfrastructure::new();

        // Should have all components initialized
        assert_eq!(infra.metrics.snapshot().active_sessions, 0);
    }

    #[tokio::test]
    async fn test_can_create_session_under_limit() {
        let infra = ServerInfrastructure::new();
        let client_id = ClientId::from_socket(SocketAddr::new(
            IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            8080
        ));

        // Should allow session creation when under limits
        assert!(infra.can_create_session(&client_id, 0).await);
    }

    #[tokio::test]
    async fn test_can_create_session_at_limit() {
        let mut limits = ResourceLimits::default();
        limits.max_concurrent_sessions = 5;
        let infra = ServerInfrastructure::with_limits(limits);

        let client_id = ClientId::from_socket(SocketAddr::new(
            IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            8080
        ));

        // Should reject when at limit
        assert!(!infra.can_create_session(&client_id, 5).await);
    }

    #[tokio::test]
    async fn test_metrics_recording() {
        let infra = ServerInfrastructure::new();

        infra.record_session_created();
        infra.record_connection_opened();

        let snapshot = infra.get_metrics();
        assert_eq!(snapshot.active_sessions, 1);
        assert_eq!(snapshot.active_connections, 1);
    }
}
