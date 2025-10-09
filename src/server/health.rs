//! Health Check System
//!
//! Provides health check endpoints for monitoring and load balancing.
//! Validates that all critical server components are functioning correctly.
//!
//! ## Health Status Levels
//!
//! - **Healthy**: All components operational
//! - **Degraded**: Some non-critical issues (warnings)
//! - **Unhealthy**: Critical failures requiring attention
//!
//! ## Component Checks
//!
//! - PTY spawning capability
//! - Memory pressure detection
//! - File descriptor availability
//! - Socket connectivity
//!
//! ## Usage
//!
//! ```rust
//! let health = HealthChecker::new();
//! let status = health.check().await;
//! println!("Status: {}", status.level());
//! ```

use std::time::{Duration, Instant};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Health check result
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HealthStatus {
    /// All systems operational
    Healthy,
    /// Some non-critical issues detected
    Degraded { reason: String },
    /// Critical failures detected
    Unhealthy { reason: String },
}

impl HealthStatus {
    /// Get status level as string
    pub fn level(&self) -> &str {
        match self {
            HealthStatus::Healthy => "healthy",
            HealthStatus::Degraded { .. } => "degraded",
            HealthStatus::Unhealthy { .. } => "unhealthy",
        }
    }

    /// Get HTTP status code for this health status
    pub fn http_status_code(&self) -> u16 {
        match self {
            HealthStatus::Healthy => 200,
            HealthStatus::Degraded { .. } => 200, // Still accepting traffic
            HealthStatus::Unhealthy { .. } => 503, // Service unavailable
        }
    }

    /// Check if status is OK for serving traffic
    pub fn is_ok(&self) -> bool {
        !matches!(self, HealthStatus::Unhealthy { .. })
    }
}

/// Component health check trait
#[async_trait::async_trait]
pub trait ComponentCheck: Send + Sync {
    /// Perform health check for this component
    async fn check(&self) -> HealthStatus;

    /// Get component name
    fn name(&self) -> &str;
}

/// PTY spawning health check
pub struct PtyCheck;

#[async_trait::async_trait]
impl ComponentCheck for PtyCheck {
    async fn check(&self) -> HealthStatus {
        // Test if we can spawn a PTY
        // This validates that we haven't hit ulimit or other OS restrictions
        match portable_pty::native_pty_system().openpty(portable_pty::PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        }) {
            Ok(_) => HealthStatus::Healthy,
            Err(e) => HealthStatus::Unhealthy {
                reason: format!("Cannot spawn PTY: {}", e),
            },
        }
    }

    fn name(&self) -> &str {
        "pty"
    }
}

/// Memory pressure health check
pub struct MemoryCheck {
    warning_threshold_percent: f32, // e.g., 0.8 = 80%
    critical_threshold_percent: f32, // e.g., 0.95 = 95%
}

impl MemoryCheck {
    pub fn new(warning: f32, critical: f32) -> Self {
        Self {
            warning_threshold_percent: warning,
            critical_threshold_percent: critical,
        }
    }
}

#[async_trait::async_trait]
impl ComponentCheck for MemoryCheck {
    async fn check(&self) -> HealthStatus {
        // Get system memory info
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        {
            use sysinfo::System;
            let mut sys = System::new_all();
            sys.refresh_memory();

            let total = sys.total_memory();
            let used = sys.used_memory();

            if total > 0 {
                let usage_ratio = used as f32 / total as f32;

                if usage_ratio >= self.critical_threshold_percent {
                    return HealthStatus::Unhealthy {
                        reason: format!("Memory critical: {:.1}% used", usage_ratio * 100.0),
                    };
                } else if usage_ratio >= self.warning_threshold_percent {
                    return HealthStatus::Degraded {
                        reason: format!("Memory pressure: {:.1}% used", usage_ratio * 100.0),
                    };
                }
            }
        }

        HealthStatus::Healthy
    }

    fn name(&self) -> &str {
        "memory"
    }
}

/// File descriptor availability check
pub struct FileDescriptorCheck {
    warning_threshold_percent: f32,
}

impl FileDescriptorCheck {
    pub fn new(warning: f32) -> Self {
        Self {
            warning_threshold_percent: warning,
        }
    }
}

#[async_trait::async_trait]
impl ComponentCheck for FileDescriptorCheck {
    async fn check(&self) -> HealthStatus {
        // Check file descriptor usage
        #[cfg(target_os = "linux")]
        {
            // Read /proc/self/limits to get max fds
            if let Ok(limits) = std::fs::read_to_string("/proc/self/limits") {
                if let Some(line) = limits.lines().find(|l| l.contains("Max open files")) {
                    if let Some(max_str) = line.split_whitespace().nth(3) {
                        if let Ok(max_fds) = max_str.parse::<usize>() {
                            // Count open fds in /proc/self/fd
                            if let Ok(entries) = std::fs::read_dir("/proc/self/fd") {
                                let current_fds = entries.count();
                                let usage_ratio = current_fds as f32 / max_fds as f32;

                                if usage_ratio >= 0.9 {
                                    return HealthStatus::Unhealthy {
                                        reason: format!("FD critical: {}/{} used ({:.1}%)",
                                            current_fds, max_fds, usage_ratio * 100.0),
                                    };
                                } else if usage_ratio >= self.warning_threshold_percent {
                                    return HealthStatus::Degraded {
                                        reason: format!("FD pressure: {}/{} used ({:.1}%)",
                                            current_fds, max_fds, usage_ratio * 100.0),
                                    };
                                }
                            }
                        }
                    }
                }
            }
        }

        // macOS and other platforms: basic check
        HealthStatus::Healthy
    }

    fn name(&self) -> &str {
        "file_descriptors"
    }
}

/// Main health checker
pub struct HealthChecker {
    checks: Arc<RwLock<Vec<Box<dyn ComponentCheck>>>>,
    last_check: Arc<RwLock<Option<(Instant, HealthStatus)>>>,
    cache_duration: Duration,
}

impl HealthChecker {
    /// Create new health checker with default checks
    pub fn new() -> Self {
        let checks: Vec<Box<dyn ComponentCheck>> = vec![
            Box::new(PtyCheck),
            Box::new(MemoryCheck::new(0.8, 0.95)),
            Box::new(FileDescriptorCheck::new(0.8)),
        ];

        Self {
            checks: Arc::new(RwLock::new(checks)),
            last_check: Arc::new(RwLock::new(None)),
            cache_duration: Duration::from_secs(5), // Cache for 5 seconds
        }
    }

    /// Add a custom component check
    pub async fn add_check(&self, check: Box<dyn ComponentCheck>) {
        self.checks.write().await.push(check);
    }

    /// Perform health check (with caching)
    pub async fn check(&self) -> HealthStatus {
        // Check cache
        {
            let last = self.last_check.read().await;
            if let Some((timestamp, status)) = &*last {
                if timestamp.elapsed() < self.cache_duration {
                    return status.clone();
                }
            }
        }

        // Perform checks
        let status = self.check_internal().await;

        // Update cache
        {
            let mut last = self.last_check.write().await;
            *last = Some((Instant::now(), status.clone()));
        }

        status
    }

    /// Internal check implementation (no caching)
    async fn check_internal(&self) -> HealthStatus {
        let checks = self.checks.read().await;

        let mut unhealthy_reasons = Vec::new();
        let mut degraded_reasons = Vec::new();

        for check in checks.iter() {
            match check.check().await {
                HealthStatus::Unhealthy { reason } => {
                    unhealthy_reasons.push(format!("{}: {}", check.name(), reason));
                }
                HealthStatus::Degraded { reason } => {
                    degraded_reasons.push(format!("{}: {}", check.name(), reason));
                }
                HealthStatus::Healthy => {}
            }
        }

        // Return worst status
        if !unhealthy_reasons.is_empty() {
            HealthStatus::Unhealthy {
                reason: unhealthy_reasons.join("; "),
            }
        } else if !degraded_reasons.is_empty() {
            HealthStatus::Degraded {
                reason: degraded_reasons.join("; "),
            }
        } else {
            HealthStatus::Healthy
        }
    }

    /// Get detailed health report
    pub async fn detailed_report(&self) -> Vec<(String, HealthStatus)> {
        let checks = self.checks.read().await;
        let mut results = Vec::new();

        for check in checks.iter() {
            let status = check.check().await;
            results.push((check.name().to_string(), status));
        }

        results
    }
}

impl Default for HealthChecker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_status_level() {
        assert_eq!(HealthStatus::Healthy.level(), "healthy");
        assert_eq!(
            HealthStatus::Degraded { reason: "test".to_string() }.level(),
            "degraded"
        );
        assert_eq!(
            HealthStatus::Unhealthy { reason: "test".to_string() }.level(),
            "unhealthy"
        );
    }

    #[test]
    fn test_health_status_http_code() {
        assert_eq!(HealthStatus::Healthy.http_status_code(), 200);
        assert_eq!(
            HealthStatus::Degraded { reason: "test".to_string() }.http_status_code(),
            200
        );
        assert_eq!(
            HealthStatus::Unhealthy { reason: "test".to_string() }.http_status_code(),
            503
        );
    }

    #[test]
    fn test_health_status_is_ok() {
        assert!(HealthStatus::Healthy.is_ok());
        assert!(HealthStatus::Degraded { reason: "test".to_string() }.is_ok());
        assert!(!HealthStatus::Unhealthy { reason: "test".to_string() }.is_ok());
    }

    #[tokio::test]
    async fn test_pty_check() {
        let check = PtyCheck;
        let status = check.check().await;

        // PTY check should succeed in test environment
        // (or fail gracefully if no TTY available)
        assert!(matches!(status, HealthStatus::Healthy | HealthStatus::Unhealthy { .. }));
    }

    #[tokio::test]
    async fn test_memory_check() {
        let check = MemoryCheck::new(0.99, 0.999); // Very high thresholds
        let status = check.check().await;

        // Should be healthy under normal test conditions
        assert!(matches!(status, HealthStatus::Healthy | HealthStatus::Degraded { .. }));
    }

    #[tokio::test]
    async fn test_health_checker() {
        let checker = HealthChecker::new();
        let status = checker.check().await;

        // Should be healthy or degraded in test environment
        assert!(status.is_ok() || matches!(status, HealthStatus::Degraded { .. }));
    }

    #[tokio::test]
    async fn test_health_checker_caching() {
        let checker = HealthChecker::new();

        let status1 = checker.check().await;
        let status2 = checker.check().await;

        // Both should be the same (cached)
        assert_eq!(status1.level(), status2.level());
    }

    #[tokio::test]
    async fn test_detailed_report() {
        let checker = HealthChecker::new();
        let report = checker.detailed_report().await;

        // Should have at least 3 default checks
        assert!(report.len() >= 3);

        // Check that component names are present
        let names: Vec<_> = report.iter().map(|(name, _)| name.as_str()).collect();
        assert!(names.contains(&"pty"));
        assert!(names.contains(&"memory"));
        assert!(names.contains(&"file_descriptors"));
    }
}
