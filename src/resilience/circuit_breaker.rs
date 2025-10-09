//! Circuit Breaker Pattern
//!
//! Prevents cascading failures by detecting failing components and
//! temporarily blocking requests to give them time to recover.
//!
//! ## States
//!
//! - **Closed**: Normal operation, requests pass through
//! - **Open**: Circuit is tripped, requests fail fast
//! - **HalfOpen**: Testing if component has recovered
//!
//! ## State Transitions
//!
//! ```
//! Closed --[failure_threshold exceeded]--> Open
//! Open --[reset_timeout elapsed]--> HalfOpen
//! HalfOpen --[success]--> Closed
//! HalfOpen --[failure]--> Open
//! ```
//!
//! ## Usage
//!
//! ```rust
//! use ferrix::resilience::circuit_breaker::{CircuitBreaker, CircuitBreakerConfig};
//!
//! let config = CircuitBreakerConfig::default();
//! let breaker = CircuitBreaker::new("pty-spawner", config);
//!
//! match breaker.call(|| spawn_pty()).await {
//!     Ok(pty) => { /* use pty */ },
//!     Err(CircuitBreakerError::Open) => {
//!         // Circuit is open, fail fast
//!     },
//!     Err(CircuitBreakerError::Operation(e)) => {
//!         // Operation failed
//!     },
//! }
//! ```

use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// Circuit breaker state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    /// Normal operation - requests pass through
    Closed,
    /// Circuit tripped - requests fail fast
    Open,
    /// Testing recovery - single request allowed
    HalfOpen,
}

/// Circuit breaker configuration
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Number of failures in window before opening circuit
    pub failure_threshold: u32,

    /// Time window for counting failures
    pub failure_window: Duration,

    /// How long to wait before trying to recover (transition to HalfOpen)
    pub reset_timeout: Duration,

    /// Number of successes in HalfOpen before closing circuit
    pub half_open_success_threshold: u32,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            failure_window: Duration::from_secs(60),
            reset_timeout: Duration::from_secs(30),
            half_open_success_threshold: 2,
        }
    }
}

impl CircuitBreakerConfig {
    /// Create new configuration
    pub fn new() -> Self {
        Self::default()
    }

    /// Set failure threshold
    pub fn failure_threshold(mut self, threshold: u32) -> Self {
        self.failure_threshold = threshold;
        self
    }

    /// Set failure window duration
    pub fn failure_window(mut self, window: Duration) -> Self {
        self.failure_window = window;
        self
    }

    /// Set reset timeout
    pub fn reset_timeout(mut self, timeout: Duration) -> Self {
        self.reset_timeout = timeout;
        self
    }

    /// Set half-open success threshold
    pub fn half_open_success_threshold(mut self, threshold: u32) -> Self {
        self.half_open_success_threshold = threshold;
        self
    }
}

/// Circuit breaker error
#[derive(Debug)]
pub enum CircuitBreakerError<E> {
    /// Circuit is open, request blocked
    Open,
    /// Operation failed
    Operation(E),
}

impl<E: std::fmt::Display> std::fmt::Display for CircuitBreakerError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CircuitBreakerError::Open => write!(f, "Circuit breaker is open"),
            CircuitBreakerError::Operation(e) => write!(f, "Operation failed: {}", e),
        }
    }
}

impl<E: std::error::Error> std::error::Error for CircuitBreakerError<E> {}

/// Internal state tracking
#[derive(Debug)]
struct CircuitBreakerState {
    state: CircuitState,
    failure_count: u32,
    success_count: u32, // For HalfOpen state
    last_failure_time: Option<Instant>,
    opened_at: Option<Instant>,
    failure_timestamps: Vec<Instant>,
}

impl CircuitBreakerState {
    fn new() -> Self {
        Self {
            state: CircuitState::Closed,
            failure_count: 0,
            success_count: 0,
            last_failure_time: None,
            opened_at: None,
            failure_timestamps: Vec::new(),
        }
    }
}

/// Circuit breaker
pub struct CircuitBreaker {
    name: String,
    config: CircuitBreakerConfig,
    state: Arc<RwLock<CircuitBreakerState>>,
}

impl CircuitBreaker {
    /// Create a new circuit breaker
    pub fn new(name: impl Into<String>, config: CircuitBreakerConfig) -> Self {
        Self {
            name: name.into(),
            config,
            state: Arc::new(RwLock::new(CircuitBreakerState::new())),
        }
    }

    /// Get current circuit state
    pub async fn state(&self) -> CircuitState {
        let state = self.state.read().await;
        state.state
    }

    /// Execute an operation through the circuit breaker
    pub async fn call<F, Fut, T, E>(
        &self,
        operation: F,
    ) -> Result<T, CircuitBreakerError<E>>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<T, E>>,
        E: std::fmt::Debug,
    {
        // Check if we should allow the request
        {
            let mut state = self.state.write().await;
            self.update_state_for_request(&mut state).await;

            if state.state == CircuitState::Open {
                tracing::warn!(
                    "Circuit breaker '{}' is OPEN, rejecting request",
                    self.name
                );
                return Err(CircuitBreakerError::Open);
            }
        }

        // Execute the operation
        match operation().await {
            Ok(result) => {
                self.on_success().await;
                Ok(result)
            }
            Err(err) => {
                tracing::debug!(
                    "Circuit breaker '{}' recorded failure: {:?}",
                    self.name,
                    err
                );
                self.on_failure().await;
                Err(CircuitBreakerError::Operation(err))
            }
        }
    }

    /// Update state based on time and current state
    async fn update_state_for_request(&self, state: &mut CircuitBreakerState) {
        match state.state {
            CircuitState::Closed => {
                // Clean up old failures outside the window
                let now = Instant::now();
                state.failure_timestamps.retain(|t| {
                    now.duration_since(*t) < self.config.failure_window
                });
                state.failure_count = state.failure_timestamps.len() as u32;
            }
            CircuitState::Open => {
                // Check if reset timeout has elapsed
                if let Some(opened_at) = state.opened_at {
                    if opened_at.elapsed() >= self.config.reset_timeout {
                        tracing::info!(
                            "Circuit breaker '{}' transitioning to HALF_OPEN",
                            self.name
                        );
                        state.state = CircuitState::HalfOpen;
                        state.success_count = 0;
                    }
                }
            }
            CircuitState::HalfOpen => {
                // Allow single request through
            }
        }
    }

    /// Record a successful operation
    async fn on_success(&self) {
        let mut state = self.state.write().await;

        match state.state {
            CircuitState::Closed => {
                // Normal operation, nothing special
            }
            CircuitState::HalfOpen => {
                state.success_count += 1;
                if state.success_count >= self.config.half_open_success_threshold {
                    tracing::info!(
                        "Circuit breaker '{}' transitioning to CLOSED (recovered)",
                        self.name
                    );
                    state.state = CircuitState::Closed;
                    state.failure_count = 0;
                    state.success_count = 0;
                    state.failure_timestamps.clear();
                    state.opened_at = None;
                }
            }
            CircuitState::Open => {
                // Shouldn't happen, but reset if it does
                tracing::warn!(
                    "Circuit breaker '{}' received success in OPEN state",
                    self.name
                );
            }
        }
    }

    /// Record a failed operation
    async fn on_failure(&self) {
        let mut state = self.state.write().await;
        let now = Instant::now();

        match state.state {
            CircuitState::Closed => {
                state.failure_count += 1;
                state.failure_timestamps.push(now);
                state.last_failure_time = Some(now);

                // Check if we should open the circuit
                if state.failure_count >= self.config.failure_threshold {
                    tracing::warn!(
                        "Circuit breaker '{}' transitioning to OPEN ({} failures in {:?})",
                        self.name,
                        state.failure_count,
                        self.config.failure_window
                    );
                    state.state = CircuitState::Open;
                    state.opened_at = Some(now);
                }
            }
            CircuitState::HalfOpen => {
                // Failed during recovery, go back to Open
                tracing::warn!(
                    "Circuit breaker '{}' failed during recovery, returning to OPEN",
                    self.name
                );
                state.state = CircuitState::Open;
                state.opened_at = Some(now);
                state.success_count = 0;
            }
            CircuitState::Open => {
                // Already open, just update timestamp
                state.last_failure_time = Some(now);
            }
        }
    }

    /// Manually reset the circuit breaker (for testing/ops)
    pub async fn reset(&self) {
        let mut state = self.state.write().await;
        tracing::info!("Circuit breaker '{}' manually reset to CLOSED", self.name);
        state.state = CircuitState::Closed;
        state.failure_count = 0;
        state.success_count = 0;
        state.failure_timestamps.clear();
        state.opened_at = None;
        state.last_failure_time = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_defaults() {
        let config = CircuitBreakerConfig::default();
        assert_eq!(config.failure_threshold, 5);
        assert_eq!(config.failure_window, Duration::from_secs(60));
        assert_eq!(config.reset_timeout, Duration::from_secs(30));
        assert_eq!(config.half_open_success_threshold, 2);
    }

    #[test]
    fn test_config_builder() {
        let config = CircuitBreakerConfig::new()
            .failure_threshold(10)
            .failure_window(Duration::from_secs(120))
            .reset_timeout(Duration::from_secs(60))
            .half_open_success_threshold(3);

        assert_eq!(config.failure_threshold, 10);
        assert_eq!(config.failure_window, Duration::from_secs(120));
        assert_eq!(config.reset_timeout, Duration::from_secs(60));
        assert_eq!(config.half_open_success_threshold, 3);
    }

    #[tokio::test]
    async fn test_circuit_breaker_starts_closed() {
        let config = CircuitBreakerConfig::default();
        let breaker = CircuitBreaker::new("test", config);

        assert_eq!(breaker.state().await, CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_circuit_breaker_opens_after_threshold() {
        let config = CircuitBreakerConfig::new()
            .failure_threshold(3)
            .failure_window(Duration::from_secs(60));

        let breaker = CircuitBreaker::new("test", config);

        // Fail 3 times
        for _ in 0..3 {
            let _ = breaker.call(|| async { Err::<(), _>("fail") }).await;
        }

        assert_eq!(breaker.state().await, CircuitState::Open);
    }

    #[tokio::test]
    async fn test_circuit_breaker_blocks_when_open() {
        let config = CircuitBreakerConfig::new()
            .failure_threshold(2)
            .failure_window(Duration::from_secs(60));

        let breaker = CircuitBreaker::new("test", config);

        // Open the circuit
        let _ = breaker.call(|| async { Err::<(), _>("fail") }).await;
        let _ = breaker.call(|| async { Err::<(), _>("fail") }).await;

        assert_eq!(breaker.state().await, CircuitState::Open);

        // Next call should be blocked
        let result = breaker.call(|| async { Ok::<(), String>(()) }).await;
        assert!(matches!(result, Err(CircuitBreakerError::Open)));
    }

    #[tokio::test]
    async fn test_circuit_breaker_half_open_transition() {
        let config = CircuitBreakerConfig::new()
            .failure_threshold(2)
            .reset_timeout(Duration::from_millis(50));

        let breaker = CircuitBreaker::new("test", config);

        // Open the circuit
        let _ = breaker.call(|| async { Err::<(), _>("fail") }).await;
        let _ = breaker.call(|| async { Err::<(), _>("fail") }).await;
        assert_eq!(breaker.state().await, CircuitState::Open);

        // Wait for reset timeout
        tokio::time::sleep(Duration::from_millis(60)).await;

        // Next request should transition to HalfOpen
        let _ = breaker.call(|| async { Ok::<(), String>(()) }).await;
        // After successful call in HalfOpen, might already be Closed
        let state = breaker.state().await;
        assert!(state == CircuitState::HalfOpen || state == CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_circuit_breaker_recovers() {
        let config = CircuitBreakerConfig::new()
            .failure_threshold(2)
            .reset_timeout(Duration::from_millis(50))
            .half_open_success_threshold(2);

        let breaker = CircuitBreaker::new("test", config);

        // Open the circuit
        let _ = breaker.call(|| async { Err::<(), _>("fail") }).await;
        let _ = breaker.call(|| async { Err::<(), _>("fail") }).await;
        assert_eq!(breaker.state().await, CircuitState::Open);

        // Wait for reset timeout
        tokio::time::sleep(Duration::from_millis(60)).await;

        // Two successful calls should close the circuit
        let _ = breaker.call(|| async { Ok::<(), String>(()) }).await;
        let _ = breaker.call(|| async { Ok::<(), String>(()) }).await;

        assert_eq!(breaker.state().await, CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_circuit_breaker_manual_reset() {
        let config = CircuitBreakerConfig::new().failure_threshold(2);
        let breaker = CircuitBreaker::new("test", config);

        // Open the circuit
        let _ = breaker.call(|| async { Err::<(), _>("fail") }).await;
        let _ = breaker.call(|| async { Err::<(), _>("fail") }).await;
        assert_eq!(breaker.state().await, CircuitState::Open);

        // Manual reset
        breaker.reset().await;
        assert_eq!(breaker.state().await, CircuitState::Closed);
    }
}
