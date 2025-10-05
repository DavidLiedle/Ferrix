use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::{Duration, Instant};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Rate limiter for authentication attempts
pub struct RateLimiter {
    attempts: Arc<RwLock<HashMap<SocketAddr, AttemptRecord>>>,
    max_attempts: u32,
    lockout_duration: Duration,
    cleanup_interval: Duration,
}

#[derive(Debug, Clone)]
struct AttemptRecord {
    count: u32,
    first_attempt: Instant,
    locked_until: Option<Instant>,
}

impl RateLimiter {
    /// Create a new rate limiter
    ///
    /// # Arguments
    /// * `max_attempts` - Maximum number of failed attempts before lockout
    /// * `lockout_duration` - How long to lock out after max attempts
    pub fn new(max_attempts: u32, lockout_duration: Duration) -> Self {
        let limiter = Self {
            attempts: Arc::new(RwLock::new(HashMap::new())),
            max_attempts,
            lockout_duration,
            cleanup_interval: Duration::from_secs(300), // 5 minutes
        };

        // Spawn cleanup task
        let attempts = limiter.attempts.clone();
        let cleanup_interval = limiter.cleanup_interval;
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(cleanup_interval).await;
                let mut attempts_guard = attempts.write().await;
                let now = Instant::now();

                // Remove expired records
                attempts_guard.retain(|_, record| {
                    if let Some(locked_until) = record.locked_until {
                        // Keep locked records until lockout expires
                        locked_until > now
                    } else {
                        // Keep active records for cleanup_interval duration
                        record.first_attempt.elapsed() < cleanup_interval
                    }
                });
            }
        });

        limiter
    }

    /// Check if an address is currently locked out
    pub async fn is_locked(&self, addr: &SocketAddr) -> bool {
        let attempts = self.attempts.read().await;
        if let Some(record) = attempts.get(addr) {
            if let Some(locked_until) = record.locked_until {
                return locked_until > Instant::now();
            }
        }
        false
    }

    /// Record a failed authentication attempt
    /// Returns true if the address should be locked out
    pub async fn record_failure(&self, addr: SocketAddr) -> bool {
        let mut attempts = self.attempts.write().await;
        let now = Instant::now();

        let record = attempts.entry(addr).or_insert(AttemptRecord {
            count: 0,
            first_attempt: now,
            locked_until: None,
        });

        // If already locked, keep the lock
        if let Some(locked_until) = record.locked_until {
            if locked_until > now {
                return true;
            }
            // Lock expired, reset
            record.count = 0;
            record.first_attempt = now;
            record.locked_until = None;
        }

        record.count += 1;

        if record.count >= self.max_attempts {
            record.locked_until = Some(now + self.lockout_duration);
            tracing::warn!(
                "Rate limit exceeded for {}: {} failed attempts, locked for {:?}",
                addr, record.count, self.lockout_duration
            );
            true
        } else {
            false
        }
    }

    /// Record a successful authentication (clears the record)
    pub async fn record_success(&self, addr: &SocketAddr) {
        let mut attempts = self.attempts.write().await;
        attempts.remove(addr);
    }

    /// Get time remaining in lockout for an address
    pub async fn lockout_remaining(&self, addr: &SocketAddr) -> Option<Duration> {
        let attempts = self.attempts.read().await;
        if let Some(record) = attempts.get(addr) {
            if let Some(locked_until) = record.locked_until {
                let now = Instant::now();
                if locked_until > now {
                    return Some(locked_until.duration_since(now));
                }
            }
        }
        None
    }

    /// Get the number of failed attempts for an address
    pub async fn attempt_count(&self, addr: &SocketAddr) -> u32 {
        let attempts = self.attempts.read().await;
        attempts.get(addr).map(|r| r.count).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    #[tokio::test]
    async fn test_rate_limiter_basic() {
        let limiter = RateLimiter::new(3, Duration::from_secs(60));
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);

        // First 2 attempts should not lock
        assert!(!limiter.record_failure(addr).await);
        assert!(!limiter.record_failure(addr).await);
        assert!(!limiter.is_locked(&addr).await);

        // 3rd attempt should lock
        assert!(limiter.record_failure(addr).await);
        assert!(limiter.is_locked(&addr).await);
    }

    #[tokio::test]
    async fn test_rate_limiter_success_clears() {
        let limiter = RateLimiter::new(3, Duration::from_secs(60));
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);

        limiter.record_failure(addr).await;
        limiter.record_failure(addr).await;
        assert_eq!(limiter.attempt_count(&addr).await, 2);

        limiter.record_success(&addr).await;
        assert_eq!(limiter.attempt_count(&addr).await, 0);
    }

    #[tokio::test]
    async fn test_rate_limiter_lockout_expiry() {
        let limiter = RateLimiter::new(2, Duration::from_millis(100));
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);

        // Lock the address
        limiter.record_failure(addr).await;
        limiter.record_failure(addr).await;
        assert!(limiter.is_locked(&addr).await);

        // Wait for lockout to expire
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert!(!limiter.is_locked(&addr).await);
    }
}
