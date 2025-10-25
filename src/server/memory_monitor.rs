//! Memory Leak Detection and Monitoring
//!
//! Tracks memory usage over time to detect gradual leaks and memory growth.
//! Uses statistical analysis to distinguish between normal growth and leaks.
//!
//! ## Detection Strategy
//!
//! 1. **Periodic Sampling**: Collect memory snapshots every N seconds
//! 2. **Baseline Tracking**: Establish baseline after warmup period
//! 3. **Growth Detection**: Track memory growth rate over sliding window
//! 4. **Threshold Alerts**: Warn when growth exceeds acceptable bounds
//!
//! ## Usage
//!
//! ```rust
//! let monitor = MemoryMonitor::new();
//! monitor.start_monitoring(Duration::from_secs(60)).await;
//! ```

use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tokio::time::interval;
use tracing::{warn, info, debug};

/// Memory usage in bytes
#[derive(Debug, Clone, Copy)]
pub struct MemoryUsage {
    /// Resident Set Size (RSS) - actual physical memory used
    pub rss: u64,
    /// Virtual Memory Size
    pub vms: u64,
    /// Timestamp when measurement was taken
    pub timestamp: Instant,
}

/// Memory snapshot for trend analysis
#[derive(Debug, Clone)]
pub struct MemorySnapshot {
    pub usage: MemoryUsage,
    pub active_sessions: usize,
    pub active_connections: usize,
    pub active_panes: usize,
}

/// Configuration for memory leak detection
#[derive(Debug, Clone)]
pub struct MemoryMonitorConfig {
    /// How often to sample memory usage
    pub sample_interval: Duration,

    /// Number of samples to keep for analysis
    pub history_size: usize,

    /// Warmup period before baseline is established (samples)
    pub warmup_samples: usize,

    /// Maximum acceptable growth rate (bytes/sample)
    /// Default: 1MB per sample (60MB/min at 1 sample/sec)
    pub max_growth_rate: f64,

    /// Memory growth percentage threshold (compared to baseline)
    /// Default: 0.05 = 5% growth triggers warning
    pub growth_threshold: f64,

    /// Absolute memory limit in bytes (None = no limit)
    pub memory_limit: Option<u64>,
}

impl Default for MemoryMonitorConfig {
    fn default() -> Self {
        Self {
            sample_interval: Duration::from_secs(60),
            history_size: 120,           // 2 hours at 1 sample/min
            warmup_samples: 10,           // 10 minutes warmup
            max_growth_rate: 1024.0 * 1024.0, // 1MB per sample
            growth_threshold: 0.05,       // 5% growth
            memory_limit: None,
        }
    }
}

/// Memory leak detector and monitor
pub struct MemoryMonitor {
    config: MemoryMonitorConfig,
    snapshots: Arc<RwLock<Vec<MemorySnapshot>>>,
    baseline: Arc<RwLock<Option<u64>>>,
    leak_detected: Arc<RwLock<bool>>,
}

impl MemoryMonitor {
    pub fn new() -> Self {
        Self::with_config(MemoryMonitorConfig::default())
    }

    pub fn with_config(config: MemoryMonitorConfig) -> Self {
        Self {
            config,
            snapshots: Arc::new(RwLock::new(Vec::new())),
            baseline: Arc::new(RwLock::new(None)),
            leak_detected: Arc::new(RwLock::new(false)),
        }
    }

    /// Start monitoring memory usage in the background
    pub async fn start_monitoring(
        self: Arc<Self>,
        metrics: Arc<crate::server::metrics::ServerMetrics>,
    ) {
        let mut timer = interval(self.config.sample_interval);

        info!(
            "Memory monitor started (interval: {}s, history: {} samples)",
            self.config.sample_interval.as_secs(),
            self.config.history_size
        );

        loop {
            timer.tick().await;

            // Collect current memory usage
            let usage = match Self::get_memory_usage() {
                Ok(u) => u,
                Err(e) => {
                    warn!("Failed to get memory usage: {}", e);
                    continue;
                }
            };

            // Get current server metrics
            let snapshot_data = metrics.snapshot();
            let snapshot = MemorySnapshot {
                usage,
                active_sessions: snapshot_data.active_sessions,
                active_connections: snapshot_data.active_connections,
                active_panes: snapshot_data.active_panes,
            };

            // Store snapshot and analyze
            self.record_snapshot(snapshot).await;
            self.analyze_for_leaks().await;
        }
    }

    /// Get current process memory usage
    fn get_memory_usage() -> Result<MemoryUsage, std::io::Error> {
        #[cfg(target_os = "linux")]
        {
            Self::get_memory_usage_linux()
        }

        #[cfg(target_os = "macos")]
        {
            Self::get_memory_usage_macos()
        }

        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            // Fallback for unsupported platforms
            Ok(MemoryUsage {
                rss: 0,
                vms: 0,
                timestamp: Instant::now(),
            })
        }
    }

    #[cfg(target_os = "linux")]
    fn get_memory_usage_linux() -> Result<MemoryUsage, std::io::Error> {
        let contents = std::fs::read_to_string("/proc/self/status")?;

        let mut rss = 0u64;
        let mut vms = 0u64;

        for line in contents.lines() {
            if line.starts_with("VmRSS:") {
                if let Some(value) = line.split_whitespace().nth(1) {
                    rss = value.parse::<u64>().unwrap_or(0) * 1024; // Convert KB to bytes
                }
            } else if line.starts_with("VmSize:") {
                if let Some(value) = line.split_whitespace().nth(1) {
                    vms = value.parse::<u64>().unwrap_or(0) * 1024;
                }
            }
        }

        Ok(MemoryUsage {
            rss,
            vms,
            timestamp: Instant::now(),
        })
    }

    #[cfg(target_os = "macos")]
    fn get_memory_usage_macos() -> Result<MemoryUsage, std::io::Error> {
        use std::process::Command;

        let output = Command::new("ps")
            .args(["-o", "rss,vsz", "-p", &std::process::id().to_string()])
            .output()?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let lines: Vec<&str> = stdout.lines().collect();

        if lines.len() < 2 {
            return Err(std::io::Error::other(
                "Failed to parse ps output",
            ));
        }

        let values: Vec<&str> = lines[1].split_whitespace().collect();
        if values.len() < 2 {
            return Err(std::io::Error::other(
                "Invalid ps output format",
            ));
        }

        let rss = values[0].parse::<u64>().unwrap_or(0) * 1024; // Convert KB to bytes
        let vms = values[1].parse::<u64>().unwrap_or(0) * 1024;

        Ok(MemoryUsage {
            rss,
            vms,
            timestamp: Instant::now(),
        })
    }

    /// Record a memory snapshot
    async fn record_snapshot(&self, snapshot: MemorySnapshot) {
        let mut snapshots = self.snapshots.write().await;

        // Extract values we need before moving snapshot
        let rss = snapshot.usage.rss;
        let active_sessions = snapshot.active_sessions;
        let active_connections = snapshot.active_connections;
        let active_panes = snapshot.active_panes;

        snapshots.push(snapshot);

        // Maintain history size limit
        if snapshots.len() > self.config.history_size {
            snapshots.remove(0);
        }

        // Establish baseline after warmup
        if snapshots.len() == self.config.warmup_samples {
            let mut baseline = self.baseline.write().await;
            *baseline = Some(rss);
            info!("Memory baseline established: {} bytes ({})",
                  rss, format_bytes(rss));
        }

        debug!("Memory snapshot: RSS={} ({}) sessions={} connections={} panes={}",
               rss,
               format_bytes(rss),
               active_sessions,
               active_connections,
               active_panes);
    }

    /// Analyze snapshots for potential memory leaks
    async fn analyze_for_leaks(&self) {
        let snapshots = self.snapshots.read().await;
        let baseline = self.baseline.read().await;

        // Need baseline and sufficient samples
        if baseline.is_none() || snapshots.len() < self.config.warmup_samples * 2 {
            return;
        }

        let baseline_rss = baseline.unwrap();
        let current_rss = snapshots.last().unwrap().usage.rss;

        // Check absolute memory limit
        if let Some(limit) = self.config.memory_limit {
            if current_rss > limit {
                warn!(
                    "MEMORY LIMIT EXCEEDED: current={} ({}) limit={} ({})",
                    current_rss,
                    format_bytes(current_rss),
                    limit,
                    format_bytes(limit)
                );
            }
        }

        // Calculate growth since baseline
        let growth = current_rss as f64 - baseline_rss as f64;
        let growth_ratio = growth / baseline_rss as f64;

        if growth_ratio > self.config.growth_threshold {
            warn!(
                "MEMORY GROWTH DETECTED: baseline={} ({}) current={} ({}) growth={:.1}%",
                baseline_rss,
                format_bytes(baseline_rss),
                current_rss,
                format_bytes(current_rss),
                growth_ratio * 100.0
            );
        }

        // Analyze growth rate over recent samples
        if snapshots.len() >= 30 {
            let recent: Vec<_> = snapshots.iter()
                .rev()
                .take(30)
                .collect();

            let first_rss = recent.last().unwrap().usage.rss as f64;
            let last_rss = recent.first().unwrap().usage.rss as f64;
            let samples = recent.len() as f64;
            let growth_rate = (last_rss - first_rss) / samples;

            if growth_rate > self.config.max_growth_rate {
                let mut leak_detected = self.leak_detected.write().await;
                if !*leak_detected {
                    warn!(
                        "POTENTIAL MEMORY LEAK: growth_rate={:.2} KB/sample (threshold={:.2} KB/sample) sessions={} connections={} panes={}",
                        growth_rate / 1024.0,
                        self.config.max_growth_rate / 1024.0,
                        recent[0].active_sessions,
                        recent[0].active_connections,
                        recent[0].active_panes,
                    );
                    *leak_detected = true;
                }
            }
        }
    }

    /// Get current memory statistics
    pub async fn get_stats(&self) -> MemoryStats {
        let snapshots = self.snapshots.read().await;
        let baseline = self.baseline.read().await;
        let leak_detected = self.leak_detected.read().await;

        if snapshots.is_empty() {
            return MemoryStats::default();
        }

        let current = snapshots.last().unwrap();
        let current_rss = current.usage.rss;

        MemoryStats {
            current_rss,
            baseline_rss: *baseline,
            sample_count: snapshots.len(),
            leak_detected: *leak_detected,
            growth_since_baseline: baseline.map(|b| current_rss as i64 - b as i64),
        }
    }
}

impl Default for MemoryMonitor {
    fn default() -> Self {
        Self::new()
    }
}

/// Memory statistics snapshot
#[derive(Debug, Clone, Default)]
pub struct MemoryStats {
    pub current_rss: u64,
    pub baseline_rss: Option<u64>,
    pub sample_count: usize,
    pub leak_detected: bool,
    pub growth_since_baseline: Option<i64>,
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
    fn test_memory_monitor_creation() {
        let monitor = MemoryMonitor::new();
        assert_eq!(monitor.config.sample_interval, Duration::from_secs(60));
    }

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(500), "500 B");
        assert_eq!(format_bytes(2048), "2.00 KB");
        assert_eq!(format_bytes(2 * 1024 * 1024), "2.00 MB");
        assert_eq!(format_bytes(3 * 1024 * 1024 * 1024), "3.00 GB");
    }

    #[tokio::test]
    async fn test_memory_usage_retrieval() {
        // Should be able to get memory usage without error
        let result = MemoryMonitor::get_memory_usage();

        #[cfg(any(target_os = "linux", target_os = "macos"))]
        {
            assert!(result.is_ok());
            let usage = result.unwrap();
            assert!(usage.rss > 0, "RSS should be non-zero for running process");
        }
    }

    #[tokio::test]
    async fn test_snapshot_recording() {
        let monitor = MemoryMonitor::new();

        let snapshot = MemorySnapshot {
            usage: MemoryUsage {
                rss: 1024 * 1024 * 100, // 100MB
                vms: 1024 * 1024 * 200, // 200MB
                timestamp: Instant::now(),
            },
            active_sessions: 5,
            active_connections: 10,
            active_panes: 15,
        };

        monitor.record_snapshot(snapshot.clone()).await;

        let snapshots = monitor.snapshots.read().await;
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].active_sessions, 5);
    }

    #[tokio::test]
    async fn test_baseline_establishment() {
        let mut config = MemoryMonitorConfig::default();
        config.warmup_samples = 3;
        let monitor = MemoryMonitor::with_config(config);

        // Record snapshots until baseline is established
        for i in 0..3 {
            let snapshot = MemorySnapshot {
                usage: MemoryUsage {
                    rss: 1024 * 1024 * (100 + i), // Increasing memory
                    vms: 1024 * 1024 * 200,
                    timestamp: Instant::now(),
                },
                active_sessions: 1,
                active_connections: 1,
                active_panes: 1,
            };
            monitor.record_snapshot(snapshot).await;
        }

        let baseline = monitor.baseline.read().await;
        assert!(baseline.is_some(), "Baseline should be established after warmup");
    }

    #[tokio::test]
    async fn test_get_stats() {
        let monitor = MemoryMonitor::new();

        let snapshot = MemorySnapshot {
            usage: MemoryUsage {
                rss: 1024 * 1024 * 50,
                vms: 1024 * 1024 * 100,
                timestamp: Instant::now(),
            },
            active_sessions: 2,
            active_connections: 4,
            active_panes: 6,
        };

        monitor.record_snapshot(snapshot).await;

        let stats = monitor.get_stats().await;
        assert_eq!(stats.current_rss, 1024 * 1024 * 50);
        assert_eq!(stats.sample_count, 1);
    }
}
