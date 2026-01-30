//! Retry Logic with Exponential Backoff
//!
//! Provides configurable retry mechanisms for transient failures,
//! preventing cascading failures and improving reliability under load.
//!
//! ## Use Cases
//!
//! - PTY spawn failures (resource temporarily unavailable)
//! - File system operations (EAGAIN, EINTR)
//! - Network operations (connection refused)
//! - Lock contention timeouts
//!
//! ## Features
//!
//! - Exponential backoff with jitter
//! - Configurable max retries and base delay
//! - Predicate-based retry decisions
//! - Metrics tracking for retry patterns
//!
//! ## Usage
//!
//! ```rust
//! use ferrix::resilience::retry::{RetryPolicy, with_retry};
//!
//! // Simple retry with defaults
//! let result = with_retry(|| spawn_pty(), RetryPolicy::default()).await?;
//!
//! // Custom retry policy
//! let policy = RetryPolicy::new()
//!     .max_retries(5)
//!     .base_delay(Duration::from_millis(50))
//!     .max_delay(Duration::from_secs(5));
//!
//! let result = with_retry(|| spawn_pty(), policy).await?;
//! ```

use std::future::Future;
use std::time::Duration;
use rand::Rng;

/// Retry policy configuration
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    /// Maximum number of retry attempts
    pub max_retries: u32,

    /// Base delay for exponential backoff
    pub base_delay: Duration,

    /// Maximum delay between retries
    pub max_delay: Duration,

    /// Jitter factor (0.0 - 1.0)
    /// Adds randomness to prevent thundering herd
    pub jitter_factor: f64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(10),
            jitter_factor: 0.1, // 10% jitter
        }
    }
}

impl RetryPolicy {
    /// Create a new retry policy
    pub fn new() -> Self {
        Self::default()
    }

    /// Set maximum number of retries
    pub fn max_retries(mut self, retries: u32) -> Self {
        self.max_retries = retries;
        self
    }

    /// Set base delay for exponential backoff
    pub fn base_delay(mut self, delay: Duration) -> Self {
        self.base_delay = delay;
        self
    }

    /// Set maximum delay between retries
    pub fn max_delay(mut self, delay: Duration) -> Self {
        self.max_delay = delay;
        self
    }

    /// Set jitter factor (0.0 - 1.0)
    pub fn jitter_factor(mut self, factor: f64) -> Self {
        self.jitter_factor = factor.clamp(0.0, 1.0);
        self
    }

    /// Calculate delay for a given attempt number (0-indexed)
    pub fn calculate_delay(&self, attempt: u32) -> Duration {
        // Exponential backoff: base * 2^attempt
        let exponential_delay = self.base_delay.as_millis() as f64
            * 2_f64.powi(attempt as i32);

        // Cap at max_delay
        let capped_delay = exponential_delay.min(self.max_delay.as_millis() as f64);

        // Add jitter to prevent thundering herd
        let jitter = if self.jitter_factor > 0.0 {
            let mut rng = rand::thread_rng();
            let jitter_range = capped_delay * self.jitter_factor;
            rng.gen_range(-jitter_range..=jitter_range)
        } else {
            0.0
        };

        let final_delay = (capped_delay + jitter).max(0.0) as u64;
        Duration::from_millis(final_delay)
    }
}

/// Retry result
#[derive(Debug)]
pub enum RetryResult<T, E> {
    /// Operation succeeded
    Success(T),
    /// Operation failed after all retries
    Failed {
        last_error: E,
        attempts: u32,
    },
}

impl<T, E> RetryResult<T, E> {
    /// Convert to Result
    pub fn into_result(self) -> Result<T, E> {
        match self {
            RetryResult::Success(value) => Ok(value),
            RetryResult::Failed { last_error, .. } => Err(last_error),
        }
    }

    /// Check if successful
    pub fn is_success(&self) -> bool {
        matches!(self, RetryResult::Success(_))
    }
}

/// Execute an operation with retry logic
///
/// # Arguments
/// * `operation` - Async function to retry
/// * `policy` - Retry policy configuration
///
/// # Returns
/// Result of the operation after retries
pub async fn with_retry<F, Fut, T, E>(
    mut operation: F,
    policy: RetryPolicy,
) -> RetryResult<T, E>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
    E: std::fmt::Debug,
{
    let mut last_error = None;

    for attempt in 0..=policy.max_retries {
        match operation().await {
            Ok(result) => {
                if attempt > 0 {
                    tracing::info!(
                        "Operation succeeded after {} retries",
                        attempt
                    );
                }
                return RetryResult::Success(result);
            }
            Err(err) => {
                tracing::debug!(
                    "Operation failed (attempt {}/{}): {:?}",
                    attempt + 1,
                    policy.max_retries + 1,
                    err
                );

                last_error = Some(err);

                // Don't sleep after the last attempt
                if attempt < policy.max_retries {
                    let delay = policy.calculate_delay(attempt);
                    tracing::debug!("Retrying in {:?}", delay);
                    tokio::time::sleep(delay).await;
                }
            }
        }
    }

    tracing::warn!(
        "Operation failed after {} attempts",
        policy.max_retries + 1
    );

    RetryResult::Failed {
        last_error: last_error.expect("loop executed at least once"),
        attempts: policy.max_retries + 1,
    }
}

/// Execute an operation with retry logic (only retry if predicate returns true)
///
/// # Arguments
/// * `operation` - Async function to retry
/// * `policy` - Retry policy configuration
/// * `should_retry` - Predicate to determine if error is retryable
///
/// # Returns
/// Result of the operation after retries
pub async fn with_retry_if<F, Fut, T, E, P>(
    mut operation: F,
    policy: RetryPolicy,
    mut should_retry: P,
) -> RetryResult<T, E>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
    E: std::fmt::Debug,
    P: FnMut(&E) -> bool,
{
    let mut last_error = None;

    for attempt in 0..=policy.max_retries {
        match operation().await {
            Ok(result) => {
                if attempt > 0 {
                    tracing::info!(
                        "Operation succeeded after {} retries",
                        attempt
                    );
                }
                return RetryResult::Success(result);
            }
            Err(err) => {
                let is_retryable = should_retry(&err);

                tracing::debug!(
                    "Operation failed (attempt {}/{}, retryable: {}): {:?}",
                    attempt + 1,
                    policy.max_retries + 1,
                    is_retryable,
                    err
                );

                last_error = Some(err);

                // If error is not retryable, fail immediately
                if !is_retryable {
                    tracing::warn!("Error is not retryable, failing immediately");
                    break;
                }

                // Don't sleep after the last attempt
                if attempt < policy.max_retries {
                    let delay = policy.calculate_delay(attempt);
                    tracing::debug!("Retrying in {:?}", delay);
                    tokio::time::sleep(delay).await;
                }
            }
        }
    }

    let attempts = if last_error.as_ref().map(should_retry).unwrap_or(false) {
        policy.max_retries + 1
    } else {
        // Failed on non-retryable error
        1
    };

    RetryResult::Failed {
        last_error: last_error.expect("loop executed at least once"),
        attempts,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[test]
    fn test_retry_policy_defaults() {
        let policy = RetryPolicy::default();
        assert_eq!(policy.max_retries, 3);
        assert_eq!(policy.base_delay, Duration::from_millis(100));
        assert_eq!(policy.max_delay, Duration::from_secs(10));
        assert_eq!(policy.jitter_factor, 0.1);
    }

    #[test]
    fn test_retry_policy_builder() {
        let policy = RetryPolicy::new()
            .max_retries(5)
            .base_delay(Duration::from_millis(50))
            .max_delay(Duration::from_secs(5))
            .jitter_factor(0.2);

        assert_eq!(policy.max_retries, 5);
        assert_eq!(policy.base_delay, Duration::from_millis(50));
        assert_eq!(policy.max_delay, Duration::from_secs(5));
        assert_eq!(policy.jitter_factor, 0.2);
    }

    #[test]
    fn test_exponential_backoff() {
        let policy = RetryPolicy::new()
            .base_delay(Duration::from_millis(100))
            .max_delay(Duration::from_secs(10))
            .jitter_factor(0.0); // No jitter for predictable testing

        // Attempt 0: 100ms * 2^0 = 100ms
        let delay0 = policy.calculate_delay(0);
        assert!(delay0.as_millis() >= 100 && delay0.as_millis() <= 110);

        // Attempt 1: 100ms * 2^1 = 200ms
        let delay1 = policy.calculate_delay(1);
        assert!(delay1.as_millis() >= 200 && delay1.as_millis() <= 220);

        // Attempt 2: 100ms * 2^2 = 400ms
        let delay2 = policy.calculate_delay(2);
        assert!(delay2.as_millis() >= 400 && delay2.as_millis() <= 440);
    }

    #[test]
    fn test_max_delay_cap() {
        let policy = RetryPolicy::new()
            .base_delay(Duration::from_secs(1))
            .max_delay(Duration::from_secs(5))
            .jitter_factor(0.0);

        // Attempt 10: Would be 1024 seconds, but capped at 5
        let delay = policy.calculate_delay(10);
        assert!(delay <= Duration::from_secs(5));
    }

    #[tokio::test]
    async fn test_with_retry_success_first_attempt() {
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let policy = RetryPolicy::new().max_retries(3);

        let result = with_retry(
            || {
                let c = counter_clone.clone();
                async move {
                    c.fetch_add(1, Ordering::Relaxed);
                    Ok::<_, String>("success")
                }
            },
            policy,
        ).await;

        assert!(result.is_success());
        assert_eq!(counter.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn test_with_retry_success_after_retries() {
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let policy = RetryPolicy::new()
            .max_retries(3)
            .base_delay(Duration::from_millis(10));

        let result = with_retry(
            || {
                let c = counter_clone.clone();
                async move {
                    let count = c.fetch_add(1, Ordering::Relaxed);
                    if count < 2 {
                        Err("not yet")
                    } else {
                        Ok::<_, &str>("success")
                    }
                }
            },
            policy,
        ).await;

        assert!(result.is_success());
        assert_eq!(counter.load(Ordering::Relaxed), 3); // Failed twice, succeeded on 3rd
    }

    #[tokio::test]
    async fn test_with_retry_exhausted() {
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let policy = RetryPolicy::new()
            .max_retries(2)
            .base_delay(Duration::from_millis(10));

        let result = with_retry(
            || {
                let c = counter_clone.clone();
                async move {
                    c.fetch_add(1, Ordering::Relaxed);
                    Err::<(), _>("always fails")
                }
            },
            policy,
        ).await;

        assert!(!result.is_success());
        assert_eq!(counter.load(Ordering::Relaxed), 3); // Initial + 2 retries
    }

    #[tokio::test]
    async fn test_with_retry_if_predicate() {
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let policy = RetryPolicy::new()
            .max_retries(3)
            .base_delay(Duration::from_millis(10));

        // Only retry on "retryable" errors
        let result = with_retry_if(
            || {
                let c = counter_clone.clone();
                async move {
                    c.fetch_add(1, Ordering::Relaxed);
                    Err::<(), _>("not_retryable")
                }
            },
            policy,
            |err| err == &"retryable",
        ).await;

        assert!(!result.is_success());
        // Should fail immediately without retries (non-retryable error)
        assert_eq!(counter.load(Ordering::Relaxed), 1);
    }
}
