//! Crash Metadata Capture
//!
//! Captures detailed crash information including backtrace, system state,
//! and metrics at the time of crash.

use serde::{Serialize, Deserialize};
use std::sync::Arc;
use std::panic::PanicHookInfo;
use chrono::{DateTime, Utc};
use crate::server::metrics::{ServerMetrics, MetricsSnapshot};
use crate::error::Result;
use super::storage::CrashStorage;

/// Crash metadata captured at panic time
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrashMetadata {
    /// Unique crash ID
    pub id: uuid::Uuid,

    /// Timestamp of the crash
    pub timestamp: DateTime<Utc>,

    /// Panic message
    pub message: String,

    /// File and line number where panic occurred
    pub location: Option<CrashLocation>,

    /// Backtrace (if available)
    pub backtrace: Option<String>,

    /// System information
    pub system_info: SystemInfo,

    /// Server metrics at time of crash
    pub metrics: Option<MetricsSnapshot>,

    /// Ferrix version
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrashLocation {
    pub file: String,
    pub line: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInfo {
    pub hostname: String,
    pub os: String,
    pub architecture: String,
    pub memory_total_kb: u64,
    pub memory_available_kb: u64,
    pub cpu_count: usize,
}

impl SystemInfo {
    /// Capture current system information
    pub fn capture() -> Self {
        use sysinfo::System;

        let mut sys = System::new_all();
        sys.refresh_all();

        Self {
            hostname: hostname::get()
                .map(|h| h.to_string_lossy().to_string())
                .unwrap_or_else(|_| "unknown".to_string()),
            os: std::env::consts::OS.to_string(),
            architecture: std::env::consts::ARCH.to_string(),
            memory_total_kb: sys.total_memory(),
            memory_available_kb: sys.available_memory(),
            cpu_count: sys.cpus().len(),
        }
    }
}

/// Crash capture handler
pub struct CrashCapture {
    metrics: Option<Arc<ServerMetrics>>,
    storage: CrashStorage,
}

impl CrashCapture {
    /// Create a new crash capture handler
    pub fn new(metrics: Option<Arc<ServerMetrics>>) -> Result<Self> {
        Ok(Self {
            metrics,
            storage: CrashStorage::new()?,
        })
    }

    /// Capture crash metadata from a panic
    pub fn capture_panic(&self, panic_info: &PanicHookInfo) -> CrashMetadata {
        let message = if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
            s.to_string()
        } else if let Some(s) = panic_info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "Unknown panic".to_string()
        };

        let location = panic_info.location().map(|loc| CrashLocation {
            file: loc.file().to_string(),
            line: loc.line(),
        });

        // Capture backtrace
        let backtrace = std::backtrace::Backtrace::force_capture();
        let backtrace_str = format!("{:?}", backtrace);

        // Get metrics snapshot if available
        let metrics = self.metrics.as_ref().map(|m| m.snapshot());

        CrashMetadata {
            id: uuid::Uuid::new_v4(),
            timestamp: Utc::now(),
            message,
            location,
            backtrace: Some(backtrace_str),
            system_info: SystemInfo::capture(),
            metrics,
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    /// Handle a panic by capturing and storing crash data
    pub fn handle_panic(&self, panic_info: &PanicHookInfo) {
        let metadata = self.capture_panic(panic_info);

        // Store crash report
        if let Err(e) = self.storage.store_crash(&metadata) {
            eprintln!("Failed to store crash report: {}", e);
        } else {
            eprintln!("Crash report saved: {}", metadata.id);
            eprintln!("  Crash ID: {}", metadata.id);
            eprintln!("  Time: {}", metadata.timestamp);
            if let Some(loc) = &metadata.location {
                eprintln!("  Location: {}:{}", loc.file, loc.line);
            }
        }
    }
}

// Global panic hook state
static PANIC_HOOK_INSTALLED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Install the crash capture panic hook
///
/// This should be called once at application startup.
pub fn install_panic_hook(metrics: Option<Arc<ServerMetrics>>) -> Result<()> {
    // Check if already installed
    if PANIC_HOOK_INSTALLED.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return Ok(()); // Already installed
    }

    let capture = Arc::new(CrashCapture::new(metrics)?);

    // Install custom panic hook
    std::panic::set_hook(Box::new(move |panic_info| {
        // Print panic info to stderr
        eprintln!("\n========================================");
        eprintln!("FATAL ERROR: Ferrix has crashed");
        eprintln!("========================================\n");

        // Capture and store crash data
        capture.handle_panic(panic_info);

        eprintln!("\n========================================");
        eprintln!("Please report this crash at:");
        eprintln!("https://github.com/davidliedle/Ferrix/issues");
        eprintln!("========================================\n");
    }));

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_info_capture() {
        let info = SystemInfo::capture();
        assert!(!info.hostname.is_empty());
        assert!(!info.os.is_empty());
        assert!(info.cpu_count > 0);
    }

    #[test]
    fn test_crash_capture_creation() {
        let capture = CrashCapture::new(None);
        assert!(capture.is_ok());
    }

    #[test]
    fn test_panic_hook_install() {
        // Reset the flag for this test
        PANIC_HOOK_INSTALLED.store(false, std::sync::atomic::Ordering::SeqCst);

        let result = install_panic_hook(None);
        assert!(result.is_ok());

        // Second install should also succeed (idempotent)
        let result2 = install_panic_hook(None);
        assert!(result2.is_ok());
    }
}
