//! Circuit Breaker Pattern
//!
//! Implements the circuit breaker pattern to prevent cascading failures by
//! detecting when a subsystem is failing and temporarily blocking requests to it.
//!
//! States:
//! - Closed: Normal operation, requests pass through
//! - Open: Too many failures, requests are blocked
//! - HalfOpen: Testing if the subsystem has recovered

use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use std::collections::HashMap;

/// Circuit breaker state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    /// Circuit is closed - requests pass through normally
    Closed,
    /// Circuit is open - requests are blocked due to failures
    Open,
    /// Circuit is half-open - testing if service has recovered
    HalfOpen,
}

/// Circuit breaker configuration
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Number of consecutive failures before opening circuit
    pub failure_threshold: u32,

    /// Duration to wait before attempting to close an open circuit
    pub timeout: Duration,

    /// Number of successful requests needed in half-open state to close circuit
    pub success_threshold: u32,

    /// Window size for counting failures (rolling window)
    pub window_size: Duration,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            timeout: Duration::from_secs(60),
            success_threshold: 2,
            window_size: Duration::from_secs(60),
        }
    }
}

/// Circuit breaker for a specific subsystem
#[derive(Debug)]
struct Circuit {
    state: CircuitState,
    failure_count: u32,
    success_count: u32,
    last_failure_time: Option<Instant>,
    opened_at: Option<Instant>,
    failures_in_window: Vec<Instant>,
}

impl Circuit {
    fn new() -> Self {
        Self {
            state: CircuitState::Closed,
            failure_count: 0,
            success_count: 0,
            last_failure_time: None,
            opened_at: None,
            failures_in_window: Vec::new(),
        }
    }

    /// Clean up old failures outside the window
    fn clean_window(&mut self, window_size: Duration) {
        let now = Instant::now();
        self.failures_in_window.retain(|&time| {
            now.duration_since(time) < window_size
        });
    }
}

/// Circuit breaker manager for multiple subsystems
pub struct CircuitBreakerManager {
    config: CircuitBreakerConfig,
    circuits: Arc<RwLock<HashMap<String, Circuit>>>,
}

impl CircuitBreakerManager {
    /// Create a new circuit breaker manager with default configuration
    pub fn new() -> Self {
        Self::with_config(CircuitBreakerConfig::default())
    }

    /// Create a new circuit breaker manager with custom configuration
    pub fn with_config(config: CircuitBreakerConfig) -> Self {
        Self {
            config,
            circuits: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Check if a circuit is allowing requests
    pub async fn is_request_allowed(&self, circuit_name: &str) -> bool {
        let mut circuits = self.circuits.write().await;
        let circuit = circuits.entry(circuit_name.to_string())
            .or_insert_with(Circuit::new);

        circuit.clean_window(self.config.window_size);

        match circuit.state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                // Check if timeout has passed
                if let Some(opened_at) = circuit.opened_at {
                    if opened_at.elapsed() >= self.config.timeout {
                        // Transition to half-open
                        circuit.state = CircuitState::HalfOpen;
                        circuit.success_count = 0;
                        tracing::info!("Circuit breaker '{}' transitioning to half-open", circuit_name);
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            CircuitState::HalfOpen => true,
        }
    }

    /// Record a successful request
    pub async fn record_success(&self, circuit_name: &str) {
        let mut circuits = self.circuits.write().await;
        let circuit = circuits.entry(circuit_name.to_string())
            .or_insert_with(Circuit::new);

        match circuit.state {
            CircuitState::Closed => {
                // Reset failure count on success
                circuit.failure_count = 0;
                circuit.failures_in_window.clear();
            }
            CircuitState::HalfOpen => {
                circuit.success_count += 1;
                if circuit.success_count >= self.config.success_threshold {
                    // Close the circuit
                    circuit.state = CircuitState::Closed;
                    circuit.failure_count = 0;
                    circuit.success_count = 0;
                    circuit.failures_in_window.clear();
                    circuit.opened_at = None;
                    tracing::info!("Circuit breaker '{}' closed after successful recovery", circuit_name);
                }
            }
            CircuitState::Open => {
                // Shouldn't happen, but reset if it does
                circuit.state = CircuitState::Closed;
                circuit.failure_count = 0;
                circuit.success_count = 0;
                circuit.failures_in_window.clear();
            }
        }
    }

    /// Record a failed request
    pub async fn record_failure(&self, circuit_name: &str) {
        let mut circuits = self.circuits.write().await;
        let circuit = circuits.entry(circuit_name.to_string())
            .or_insert_with(Circuit::new);

        let now = Instant::now();
        circuit.last_failure_time = Some(now);
        circuit.failures_in_window.push(now);
        circuit.clean_window(self.config.window_size);

        match circuit.state {
            CircuitState::Closed => {
                circuit.failure_count += 1;
                if circuit.failures_in_window.len() >= self.config.failure_threshold as usize {
                    // Open the circuit
                    circuit.state = CircuitState::Open;
                    circuit.opened_at = Some(now);
                    tracing::warn!(
                        "Circuit breaker '{}' opened after {} failures in {:?}",
                        circuit_name,
                        circuit.failures_in_window.len(),
                        self.config.window_size
                    );
                }
            }
            CircuitState::HalfOpen => {
                // Failed during recovery, reopen circuit
                circuit.state = CircuitState::Open;
                circuit.opened_at = Some(now);
                circuit.success_count = 0;
                tracing::warn!("Circuit breaker '{}' reopened after failure during recovery", circuit_name);
            }
            CircuitState::Open => {
                // Already open, just record the failure
            }
        }
    }

    /// Get the current state of a circuit
    pub async fn get_state(&self, circuit_name: &str) -> CircuitState {
        let circuits = self.circuits.read().await;
        circuits.get(circuit_name)
            .map(|c| c.state)
            .unwrap_or(CircuitState::Closed)
    }

    /// Get statistics for all circuits
    pub async fn get_stats(&self) -> CircuitBreakerStats {
        let circuits = self.circuits.read().await;
        let mut open_count = 0;
        let mut half_open_count = 0;
        let mut closed_count = 0;

        for circuit in circuits.values() {
            match circuit.state {
                CircuitState::Open => open_count += 1,
                CircuitState::HalfOpen => half_open_count += 1,
                CircuitState::Closed => closed_count += 1,
            }
        }

        CircuitBreakerStats {
            total_circuits: circuits.len(),
            open_circuits: open_count,
            half_open_circuits: half_open_count,
            closed_circuits: closed_count,
            config: self.config.clone(),
        }
    }

    /// Manually reset a circuit (useful for testing or manual intervention)
    pub async fn reset_circuit(&self, circuit_name: &str) {
        let mut circuits = self.circuits.write().await;
        if let Some(circuit) = circuits.get_mut(circuit_name) {
            circuit.state = CircuitState::Closed;
            circuit.failure_count = 0;
            circuit.success_count = 0;
            circuit.failures_in_window.clear();
            circuit.opened_at = None;
            tracing::info!("Circuit breaker '{}' manually reset", circuit_name);
        }
    }
}

impl Default for CircuitBreakerManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Circuit breaker statistics
#[derive(Debug, Clone)]
pub struct CircuitBreakerStats {
    pub total_circuits: usize,
    pub open_circuits: usize,
    pub half_open_circuits: usize,
    pub closed_circuits: usize,
    pub config: CircuitBreakerConfig,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_circuit_starts_closed() {
        let manager = CircuitBreakerManager::new();
        assert_eq!(manager.get_state("test").await, CircuitState::Closed);
        assert!(manager.is_request_allowed("test").await);
    }

    #[tokio::test]
    async fn test_circuit_opens_after_failures() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            timeout: Duration::from_secs(60),
            success_threshold: 2,
            window_size: Duration::from_secs(60),
        };
        let manager = CircuitBreakerManager::with_config(config);

        // Record failures
        for _ in 0..3 {
            manager.record_failure("test").await;
        }

        assert_eq!(manager.get_state("test").await, CircuitState::Open);
        assert!(!manager.is_request_allowed("test").await);
    }

    #[tokio::test]
    async fn test_circuit_transitions_to_half_open() {
        let config = CircuitBreakerConfig {
            failure_threshold: 2,
            timeout: Duration::from_millis(100),
            success_threshold: 2,
            window_size: Duration::from_secs(60),
        };
        let manager = CircuitBreakerManager::with_config(config);

        // Open the circuit
        manager.record_failure("test").await;
        manager.record_failure("test").await;
        assert_eq!(manager.get_state("test").await, CircuitState::Open);

        // Wait for timeout
        tokio::time::sleep(Duration::from_millis(150)).await;

        // Should transition to half-open
        assert!(manager.is_request_allowed("test").await);
        assert_eq!(manager.get_state("test").await, CircuitState::HalfOpen);
    }

    #[tokio::test]
    async fn test_circuit_closes_after_successful_recovery() {
        let config = CircuitBreakerConfig {
            failure_threshold: 2,
            timeout: Duration::from_millis(100),
            success_threshold: 2,
            window_size: Duration::from_secs(60),
        };
        let manager = CircuitBreakerManager::with_config(config);

        // Open the circuit
        manager.record_failure("test").await;
        manager.record_failure("test").await;

        // Wait for timeout
        tokio::time::sleep(Duration::from_millis(150)).await;

        // Transition to half-open
        assert!(manager.is_request_allowed("test").await);

        // Record successful requests
        manager.record_success("test").await;
        manager.record_success("test").await;

        assert_eq!(manager.get_state("test").await, CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_circuit_reopens_on_failure_during_recovery() {
        let config = CircuitBreakerConfig {
            failure_threshold: 2,
            timeout: Duration::from_millis(100),
            success_threshold: 2,
            window_size: Duration::from_secs(60),
        };
        let manager = CircuitBreakerManager::with_config(config);

        // Open the circuit
        manager.record_failure("test").await;
        manager.record_failure("test").await;

        // Wait for timeout
        tokio::time::sleep(Duration::from_millis(150)).await;

        // Transition to half-open
        assert!(manager.is_request_allowed("test").await);

        // Fail during recovery
        manager.record_failure("test").await;

        assert_eq!(manager.get_state("test").await, CircuitState::Open);
    }

    #[tokio::test]
    async fn test_manual_reset() {
        let manager = CircuitBreakerManager::new();

        // Open the circuit
        for _ in 0..5 {
            manager.record_failure("test").await;
        }
        assert_eq!(manager.get_state("test").await, CircuitState::Open);

        // Manually reset
        manager.reset_circuit("test").await;
        assert_eq!(manager.get_state("test").await, CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_rolling_window() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            timeout: Duration::from_secs(60),
            success_threshold: 2,
            window_size: Duration::from_millis(200),
        };
        let manager = CircuitBreakerManager::with_config(config);

        // Record failures spread over time
        manager.record_failure("test").await;
        tokio::time::sleep(Duration::from_millis(100)).await;
        manager.record_failure("test").await;

        // Wait for first failure to exit window
        tokio::time::sleep(Duration::from_millis(150)).await;

        // Only 1 failure in window now
        assert_eq!(manager.get_state("test").await, CircuitState::Closed);

        // Add 2 more failures
        manager.record_failure("test").await;
        manager.record_failure("test").await;

        // Should open (3 failures in window)
        assert_eq!(manager.get_state("test").await, CircuitState::Open);
    }

    #[tokio::test]
    async fn test_stats() {
        let manager = CircuitBreakerManager::new();

        // Create multiple circuits in different states
        manager.record_failure("circuit1").await;
        manager.record_failure("circuit1").await;
        manager.record_failure("circuit1").await;
        manager.record_failure("circuit1").await;
        manager.record_failure("circuit1").await;

        manager.record_success("circuit2").await;

        let stats = manager.get_stats().await;
        assert_eq!(stats.total_circuits, 2);
        assert_eq!(stats.open_circuits, 1);
        assert_eq!(stats.closed_circuits, 1);
    }
}
