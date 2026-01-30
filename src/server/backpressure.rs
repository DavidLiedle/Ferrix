//! Backpressure Detection and Graceful Degradation
//!
//! Monitors system pressure and triggers graceful degradation before hitting
//! hard resource limits. This prevents cascading failures and maintains
//! responsiveness under load.
//!
//! ## Pressure Signals
//!
//! - **Memory Pressure**: System memory usage approaching capacity
//! - **Connection Pressure**: Active connections approaching limit
//! - **PTY Pressure**: PTY creation rate or failure rate too high
//! - **Buffer Pressure**: Outbound message queues growing
//!
//! ## Graceful Degradation Actions
//!
//! When pressure is detected:
//! 1. Log warnings about approaching limits
//! 2. Reject new non-essential operations (new sessions, etc.)
//! 3. Apply flow control to slow down input
//! 4. Consider dropping less important data (excess scrollback)
//!
//! ## Usage
//!
//! ```rust
//! let monitor = PressureMonitor::new(limits);
//!
//! // Periodically check pressure
//! let status = monitor.check_pressure().await;
//! if status.should_reject_new_sessions() {
//!     return Err("Server under pressure");
//! }
//! ```

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use sysinfo::System;

use crate::config::limits::ResourceLimits;
use super::metrics::ServerMetrics;

/// Pressure level indicator
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PressureLevel {
    /// Normal operation
    Normal,
    /// Approaching limits - log warnings
    Warning,
    /// At limits - reject non-essential operations
    Critical,
    /// System failure imminent - emergency measures
    Emergency,
}

impl PressureLevel {
    /// Get human-readable description
    pub fn description(&self) -> &str {
        match self {
            PressureLevel::Normal => "normal",
            PressureLevel::Warning => "warning",
            PressureLevel::Critical => "critical",
            PressureLevel::Emergency => "emergency",
        }
    }

    /// Should reject new sessions at this pressure level?
    pub fn should_reject_new_sessions(&self) -> bool {
        matches!(self, PressureLevel::Critical | PressureLevel::Emergency)
    }

    /// Should apply flow control at this pressure level?
    pub fn should_apply_flow_control(&self) -> bool {
        matches!(self, PressureLevel::Warning | PressureLevel::Critical | PressureLevel::Emergency)
    }

    /// Should drop excess scrollback at this pressure level?
    pub fn should_drop_scrollback(&self) -> bool {
        matches!(self, PressureLevel::Emergency)
    }
}

/// Comprehensive pressure status
#[derive(Debug, Clone)]
pub struct PressureStatus {
    /// Overall pressure level (worst of all signals)
    pub level: PressureLevel,

    /// Individual pressure signals
    pub memory_pressure: PressureLevel,
    pub connection_pressure: PressureLevel,
    pub session_pressure: PressureLevel,
    pub pty_pressure: PressureLevel,

    /// Timestamp of this status check
    pub timestamp: Instant,

    /// Human-readable reason for pressure
    pub reason: Option<String>,
}

impl PressureStatus {
    /// Create a new normal status
    pub fn normal() -> Self {
        Self {
            level: PressureLevel::Normal,
            memory_pressure: PressureLevel::Normal,
            connection_pressure: PressureLevel::Normal,
            session_pressure: PressureLevel::Normal,
            pty_pressure: PressureLevel::Normal,
            timestamp: Instant::now(),
            reason: None,
        }
    }

    /// Should reject new sessions?
    pub fn should_reject_new_sessions(&self) -> bool {
        self.level.should_reject_new_sessions()
    }

    /// Should apply flow control?
    pub fn should_apply_flow_control(&self) -> bool {
        self.level.should_apply_flow_control()
    }
}

/// Backpressure monitor
pub struct PressureMonitor {
    limits: Arc<ResourceLimits>,
    metrics: Arc<ServerMetrics>,

    // Cached status with TTL
    cached_status: Arc<RwLock<Option<(Instant, PressureStatus)>>>,
    cache_duration: Duration,

    // Cached system info for memory checks
    cached_system: Mutex<Option<(System, Instant)>>,

    // Emergency shutdown flag
    emergency_mode: AtomicBool,

    // PTY failure tracking
    pty_failures_last_minute: Arc<RwLock<Vec<Instant>>>,
    last_pty_failure_count: AtomicU64,
}

impl PressureMonitor {
    /// Create new pressure monitor
    pub fn new(limits: Arc<ResourceLimits>, metrics: Arc<ServerMetrics>) -> Self {
        Self {
            limits,
            metrics,
            cached_status: Arc::new(RwLock::new(None)),
            cache_duration: Duration::from_secs(1), // Check every 1 second
            cached_system: Mutex::new(None),
            emergency_mode: AtomicBool::new(false),
            pty_failures_last_minute: Arc::new(RwLock::new(Vec::new())),
            last_pty_failure_count: AtomicU64::new(0),
        }
    }

    /// Get memory statistics, using cache if available and recent
    fn get_memory_stats(&self) -> (u64, u64) {
        let mut cache = self.cached_system.lock().expect("mutex not poisoned");
        let now = Instant::now();

        // Check if we have a cached system and it's still fresh (< 2 seconds old)
        if let Some((ref mut sys, ref mut last_refresh)) = *cache {
            if now.duration_since(*last_refresh) < Duration::from_secs(2) {
                // Cache is fresh, just refresh memory
                sys.refresh_memory();
                return (sys.total_memory(), sys.used_memory());
            }
        }

        // Cache is stale or doesn't exist, create new
        let mut sys = System::new_all();
        sys.refresh_memory();
        let total = sys.total_memory();
        let used = sys.used_memory();
        *cache = Some((sys, now));
        (total, used)
    }

    /// Check current pressure status (with caching)
    pub async fn check_pressure(&self) -> PressureStatus {
        // Check cache first
        {
            let cached = self.cached_status.read().await;
            if let Some((timestamp, status)) = &*cached {
                if timestamp.elapsed() < self.cache_duration {
                    return status.clone();
                }
            }
        }

        // Perform fresh check
        let status = self.check_pressure_internal().await;

        // Update cache
        {
            let mut cached = self.cached_status.write().await;
            *cached = Some((Instant::now(), status.clone()));
        }

        // Log warnings if pressure detected
        if status.level > PressureLevel::Normal {
            tracing::warn!(
                "System pressure detected: {} - {}",
                status.level.description(),
                status.reason.as_deref().unwrap_or("unknown reason")
            );
        }

        status
    }

    /// Internal pressure check (no caching)
    async fn check_pressure_internal(&self) -> PressureStatus {
        // Check if emergency mode is active
        if self.emergency_mode.load(Ordering::Relaxed) {
            return PressureStatus {
                level: PressureLevel::Emergency,
                memory_pressure: PressureLevel::Emergency,
                connection_pressure: PressureLevel::Emergency,
                session_pressure: PressureLevel::Emergency,
                pty_pressure: PressureLevel::Emergency,
                timestamp: Instant::now(),
                reason: Some("Emergency mode activated".to_string()),
            };
        }

        let snapshot = self.metrics.snapshot();

        // Check memory pressure
        let memory_pressure = self.check_memory_pressure();

        // Check connection pressure
        let connection_pressure = self.check_connection_pressure(snapshot.active_connections);

        // Check session pressure
        let session_pressure = self.check_session_pressure(snapshot.active_sessions);

        // Check PTY pressure
        let pty_pressure = self.check_pty_pressure(snapshot.pty_spawn_failures).await;

        // Overall level is worst of all signals
        let level = [memory_pressure, connection_pressure, session_pressure, pty_pressure]
            .iter()
            .max()
            .copied()
            .unwrap_or(PressureLevel::Normal);

        // Build reason string
        let mut reasons = Vec::new();
        if memory_pressure > PressureLevel::Normal {
            reasons.push(format!("memory: {}", memory_pressure.description()));
        }
        if connection_pressure > PressureLevel::Normal {
            reasons.push(format!("connections: {}", connection_pressure.description()));
        }
        if session_pressure > PressureLevel::Normal {
            reasons.push(format!("sessions: {}", session_pressure.description()));
        }
        if pty_pressure > PressureLevel::Normal {
            reasons.push(format!("pty: {}", pty_pressure.description()));
        }

        PressureStatus {
            level,
            memory_pressure,
            connection_pressure,
            session_pressure,
            pty_pressure,
            timestamp: Instant::now(),
            reason: if reasons.is_empty() {
                None
            } else {
                Some(reasons.join(", "))
            },
        }
    }

    /// Check memory pressure
    fn check_memory_pressure(&self) -> PressureLevel {
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        {
            let (total, used) = self.get_memory_stats();

            if total > 0 {
                let usage_ratio = used as f32 / total as f32;
                let threshold = self.limits.memory_pressure_threshold;

                if usage_ratio >= threshold + 0.1 {
                    // 10% over threshold = emergency
                    PressureLevel::Emergency
                } else if usage_ratio >= threshold {
                    // At threshold = critical
                    PressureLevel::Critical
                } else if usage_ratio >= threshold - 0.1 {
                    // Within 10% of threshold = warning
                    PressureLevel::Warning
                } else {
                    PressureLevel::Normal
                }
            } else {
                PressureLevel::Normal
            }
        }

        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            PressureLevel::Normal
        }
    }

    /// Check connection pressure
    fn check_connection_pressure(&self, active_connections: usize) -> PressureLevel {
        let max = self.limits.max_clients;
        let ratio = active_connections as f32 / max as f32;

        if ratio >= 0.95 {
            PressureLevel::Critical
        } else if ratio >= 0.85 {
            PressureLevel::Warning
        } else {
            PressureLevel::Normal
        }
    }

    /// Check session pressure
    fn check_session_pressure(&self, active_sessions: usize) -> PressureLevel {
        let max = self.limits.max_concurrent_sessions;
        let ratio = active_sessions as f32 / max as f32;

        if ratio >= 0.95 {
            PressureLevel::Critical
        } else if ratio >= 0.85 {
            PressureLevel::Warning
        } else {
            PressureLevel::Normal
        }
    }

    /// Check PTY pressure (high failure rate)
    async fn check_pty_pressure(&self, total_failures: u64) -> PressureLevel {
        // Track PTY failures in the last minute
        let current_failures = total_failures;
        let last_failures = self.last_pty_failure_count.load(Ordering::Relaxed);

        // If failures increased, track timestamp
        if current_failures > last_failures {
            let mut failures = self.pty_failures_last_minute.write().await;
            let now = Instant::now();

            // Add new failure timestamps
            for _ in 0..(current_failures - last_failures) {
                failures.push(now);
            }

            // Remove failures older than 1 minute
            failures.retain(|t| t.elapsed() < Duration::from_secs(60));

            // Update last count
            self.last_pty_failure_count.store(current_failures, Ordering::Relaxed);

            // Check failure rate
            let failures_per_minute = failures.len();

            if failures_per_minute > 100 {
                // More than 100 PTY failures/minute = critical
                return PressureLevel::Critical;
            } else if failures_per_minute > 50 {
                // More than 50 PTY failures/minute = warning
                return PressureLevel::Warning;
            }
        }

        PressureLevel::Normal
    }

    /// Activate emergency mode (manual override)
    pub fn activate_emergency_mode(&self) {
        self.emergency_mode.store(true, Ordering::Relaxed);
        tracing::error!("Emergency mode activated - server will reject all new operations");
    }

    /// Deactivate emergency mode
    pub fn deactivate_emergency_mode(&self) {
        self.emergency_mode.store(false, Ordering::Relaxed);
        tracing::info!("Emergency mode deactivated");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pressure_level_ordering() {
        assert!(PressureLevel::Normal < PressureLevel::Warning);
        assert!(PressureLevel::Warning < PressureLevel::Critical);
        assert!(PressureLevel::Critical < PressureLevel::Emergency);
    }

    #[test]
    fn test_pressure_level_should_reject_sessions() {
        assert!(!PressureLevel::Normal.should_reject_new_sessions());
        assert!(!PressureLevel::Warning.should_reject_new_sessions());
        assert!(PressureLevel::Critical.should_reject_new_sessions());
        assert!(PressureLevel::Emergency.should_reject_new_sessions());
    }

    #[test]
    fn test_pressure_level_should_apply_flow_control() {
        assert!(!PressureLevel::Normal.should_apply_flow_control());
        assert!(PressureLevel::Warning.should_apply_flow_control());
        assert!(PressureLevel::Critical.should_apply_flow_control());
        assert!(PressureLevel::Emergency.should_apply_flow_control());
    }

    #[test]
    fn test_pressure_status_normal() {
        let status = PressureStatus::normal();
        assert_eq!(status.level, PressureLevel::Normal);
        assert!(!status.should_reject_new_sessions());
        assert!(!status.should_apply_flow_control());
    }

    #[tokio::test]
    async fn test_pressure_monitor_creation() {
        let limits = Arc::new(ResourceLimits::default());
        let metrics = ServerMetrics::global();

        let monitor = PressureMonitor::new(limits, metrics);
        let status = monitor.check_pressure().await;

        // Should start in normal state
        assert_eq!(status.level, PressureLevel::Normal);
    }

    #[tokio::test]
    async fn test_emergency_mode() {
        let limits = Arc::new(ResourceLimits::default());
        let metrics = ServerMetrics::global();

        let monitor = PressureMonitor::new(limits, metrics);

        // Activate emergency mode
        monitor.activate_emergency_mode();

        let status = monitor.check_pressure().await;
        assert_eq!(status.level, PressureLevel::Emergency);
        assert!(status.should_reject_new_sessions());

        // Deactivate emergency mode
        monitor.deactivate_emergency_mode();

        // Clear cache to force fresh check
        {
            let mut cached = monitor.cached_status.write().await;
            *cached = None;
        }

        let status = monitor.check_pressure().await;
        assert_eq!(status.level, PressureLevel::Normal);
    }

    #[test]
    fn test_connection_pressure() {
        let limits = Arc::new(ResourceLimits::default());
        let metrics = ServerMetrics::global();

        let monitor = PressureMonitor::new(limits, metrics);

        // Test various connection levels
        assert_eq!(
            monitor.check_connection_pressure(100),
            PressureLevel::Normal
        );
        assert_eq!(
            monitor.check_connection_pressure(1700), // 85% of 2000
            PressureLevel::Warning
        );
        assert_eq!(
            monitor.check_connection_pressure(1900), // 95% of 2000
            PressureLevel::Critical
        );
    }

    #[test]
    fn test_session_pressure() {
        let limits = Arc::new(ResourceLimits::default());
        let metrics = ServerMetrics::global();

        let monitor = PressureMonitor::new(limits, metrics);

        // Test various session levels
        assert_eq!(
            monitor.check_session_pressure(100),
            PressureLevel::Normal
        );
        assert_eq!(
            monitor.check_session_pressure(850), // 85% of 1000
            PressureLevel::Warning
        );
        assert_eq!(
            monitor.check_session_pressure(950), // 95% of 1000
            PressureLevel::Critical
        );
    }
}
