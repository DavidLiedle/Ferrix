//! Crash Analysis and Recovery System
//!
//! Provides automated crash capture, storage, and analysis for production debugging.
//!
//! Features:
//! - Automatic panic/crash detection
//! - Backtrace capture
//! - System state snapshot at crash time
//! - Pattern detection for recurring issues
//! - Integration with metrics for correlation

pub mod capture;
pub mod storage;
pub mod analysis;

pub use capture::{CrashCapture, CrashMetadata, install_panic_hook};
pub use storage::{CrashStorage, CrashReport};
pub use analysis::{CrashAnalyzer, CrashPattern};

use std::sync::Arc;
use crate::server::metrics::ServerMetrics;

/// Initialize the crash analysis system
///
/// This sets up the panic hook to automatically capture crashes
/// and integrates with the metrics system for correlation.
pub fn initialize(metrics: Option<Arc<ServerMetrics>>) -> crate::error::Result<()> {
    install_panic_hook(metrics)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crash_module_init() {
        // Basic module initialization test
        assert!(initialize(None).is_ok());
    }
}
