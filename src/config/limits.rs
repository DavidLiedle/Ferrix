//! Resource Limits Configuration
//!
//! Defines configurable resource limits to prevent resource exhaustion
//! and ensure fair allocation across clients.
//!
//! ## Purpose
//!
//! Prevents scenarios like:
//! - Single client creating 10,000 sessions → server OOM
//! - Rapid pane creation filling memory with 50KB buffers each
//! - Unbounded scrollback growth consuming all RAM
//!
//! ## Configuration
//!
//! Limits can be set via config file or environment variables:
//! ```toml
//! [limits]
//! max_windows_per_session = 100
//! max_panes_per_window = 50
//! max_scrollback_lines = 10000
//! ```

use serde::{Deserialize, Serialize};

/// Server-wide resource limits
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResourceLimits {
    // ========================================
    // Per-Session Limits
    // ========================================
    /// Maximum windows per session
    /// Default: 100 (generous for normal use, prevents abuse)
    #[serde(default = "default_max_windows_per_session")]
    pub max_windows_per_session: usize,

    /// Maximum panes per window
    /// Default: 50 (prevents complex layouts from consuming too much memory)
    #[serde(default = "default_max_panes_per_window")]
    pub max_panes_per_window: usize,

    /// Maximum scrollback lines per pane
    /// Default: 10,000 (balance between usability and memory)
    #[serde(default = "default_max_scrollback_lines")]
    pub max_scrollback_lines: usize,

    /// Maximum raw output buffer size per pane (bytes)
    /// Default: 50KB (for session persistence/replay)
    #[serde(default = "default_max_raw_buffer_bytes")]
    pub max_raw_buffer_bytes: usize,

    // ========================================
    // Per-Server Limits
    // ========================================
    /// Maximum concurrent sessions server-wide
    /// Default: 1,000 (large enough for most deployments)
    #[serde(default = "default_max_concurrent_sessions")]
    pub max_concurrent_sessions: usize,

    /// Maximum concurrent client connections
    /// Default: 2,000 (multiple clients can attach to same session)
    #[serde(default = "default_max_clients")]
    pub max_clients: usize,

    // ========================================
    // Memory Management
    // ========================================
    /// Maximum server memory usage in MB (None = unlimited)
    /// Default: None (rely on system limits)
    #[serde(default)]
    pub max_memory_mb: Option<usize>,

    /// Memory pressure threshold (0.0-1.0)
    /// When exceeded, trigger graceful degradation
    /// Default: 0.85 (85% memory usage)
    #[serde(default = "default_memory_pressure_threshold")]
    pub memory_pressure_threshold: f32,

    // ========================================
    // Rate Limiting
    // ========================================
    /// Maximum session creations per minute per client
    /// Default: 10 (prevents rapid session creation spam)
    #[serde(default = "default_max_sessions_per_minute")]
    pub max_sessions_per_minute: usize,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_windows_per_session: default_max_windows_per_session(),
            max_panes_per_window: default_max_panes_per_window(),
            max_scrollback_lines: default_max_scrollback_lines(),
            max_raw_buffer_bytes: default_max_raw_buffer_bytes(),
            max_concurrent_sessions: default_max_concurrent_sessions(),
            max_clients: default_max_clients(),
            max_memory_mb: None,
            memory_pressure_threshold: default_memory_pressure_threshold(),
            max_sessions_per_minute: default_max_sessions_per_minute(),
        }
    }
}

impl ResourceLimits {
    /// Validate configuration values
    pub fn validate(&self) -> Result<(), String> {
        if self.max_windows_per_session == 0 {
            return Err("max_windows_per_session must be > 0".to_string());
        }
        if self.max_panes_per_window == 0 {
            return Err("max_panes_per_window must be > 0".to_string());
        }
        if self.max_scrollback_lines == 0 {
            return Err("max_scrollback_lines must be > 0".to_string());
        }
        if self.max_raw_buffer_bytes < 1024 {
            return Err("max_raw_buffer_bytes must be >= 1024".to_string());
        }
        if self.max_concurrent_sessions == 0 {
            return Err("max_concurrent_sessions must be > 0".to_string());
        }
        if self.max_clients == 0 {
            return Err("max_clients must be > 0".to_string());
        }
        if !(0.0..=1.0).contains(&self.memory_pressure_threshold) {
            return Err("memory_pressure_threshold must be between 0.0 and 1.0".to_string());
        }
        if self.max_sessions_per_minute == 0 {
            return Err("max_sessions_per_minute must be > 0".to_string());
        }

        // Warn about potentially dangerous configurations
        if self.max_windows_per_session > 500 {
            tracing::warn!(
                "max_windows_per_session is very high ({}). This may cause performance issues.",
                self.max_windows_per_session
            );
        }
        if self.max_panes_per_window > 100 {
            tracing::warn!(
                "max_panes_per_window is very high ({}). This may cause performance issues.",
                self.max_panes_per_window
            );
        }

        Ok(())
    }

    /// Check if we're at the session limit
    pub fn can_create_session(&self, current_sessions: usize) -> bool {
        current_sessions < self.max_concurrent_sessions
    }

    /// Check if we can create a new window
    pub fn can_create_window(&self, current_windows: usize) -> bool {
        current_windows < self.max_windows_per_session
    }

    /// Check if we can create a new pane
    pub fn can_create_pane(&self, current_panes: usize) -> bool {
        current_panes < self.max_panes_per_window
    }

    /// Check if we can accept a new client connection
    pub fn can_accept_client(&self, current_clients: usize) -> bool {
        current_clients < self.max_clients
    }

    /// Estimate memory usage for a configuration
    pub fn estimate_max_memory_mb(&self) -> usize {
        // Very rough estimate
        let pane_memory = self.max_raw_buffer_bytes + (self.max_scrollback_lines * 100); // ~100 bytes per line
        let window_memory = pane_memory * self.max_panes_per_window;
        let session_memory = window_memory * self.max_windows_per_session + 10240; // +10KB overhead
        let total_bytes = session_memory * self.max_concurrent_sessions;

        total_bytes / (1024 * 1024) // Convert to MB
    }
}

// Default value functions (required for serde defaults)
fn default_max_windows_per_session() -> usize {
    100
}

fn default_max_panes_per_window() -> usize {
    50
}

fn default_max_scrollback_lines() -> usize {
    10_000
}

fn default_max_raw_buffer_bytes() -> usize {
    50_000 // 50KB
}

fn default_max_concurrent_sessions() -> usize {
    1_000
}

fn default_max_clients() -> usize {
    2_000
}

fn default_memory_pressure_threshold() -> f32 {
    0.85 // 85%
}

fn default_max_sessions_per_minute() -> usize {
    10
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_limits() {
        let limits = ResourceLimits::default();

        assert_eq!(limits.max_windows_per_session, 100);
        assert_eq!(limits.max_panes_per_window, 50);
        assert_eq!(limits.max_scrollback_lines, 10_000);
        assert_eq!(limits.max_raw_buffer_bytes, 50_000);
        assert_eq!(limits.max_concurrent_sessions, 1_000);
        assert_eq!(limits.max_clients, 2_000);
        assert_eq!(limits.memory_pressure_threshold, 0.85);
        assert_eq!(limits.max_sessions_per_minute, 10);
    }

    #[test]
    fn test_validate_success() {
        let limits = ResourceLimits::default();
        assert!(limits.validate().is_ok());
    }

    #[test]
    fn test_validate_zero_windows() {
        let mut limits = ResourceLimits::default();
        limits.max_windows_per_session = 0;
        assert!(limits.validate().is_err());
    }

    #[test]
    fn test_validate_zero_panes() {
        let mut limits = ResourceLimits::default();
        limits.max_panes_per_window = 0;
        assert!(limits.validate().is_err());
    }

    #[test]
    fn test_validate_invalid_threshold() {
        let mut limits = ResourceLimits::default();
        limits.memory_pressure_threshold = 1.5;
        assert!(limits.validate().is_err());
    }

    #[test]
    fn test_can_create_session() {
        let limits = ResourceLimits::default();

        assert!(limits.can_create_session(0));
        assert!(limits.can_create_session(999));
        assert!(!limits.can_create_session(1000));
        assert!(!limits.can_create_session(1001));
    }

    #[test]
    fn test_can_create_window() {
        let limits = ResourceLimits::default();

        assert!(limits.can_create_window(0));
        assert!(limits.can_create_window(99));
        assert!(!limits.can_create_window(100));
    }

    #[test]
    fn test_can_create_pane() {
        let limits = ResourceLimits::default();

        assert!(limits.can_create_pane(0));
        assert!(limits.can_create_pane(49));
        assert!(!limits.can_create_pane(50));
    }

    #[test]
    fn test_can_accept_client() {
        let limits = ResourceLimits::default();

        assert!(limits.can_accept_client(0));
        assert!(limits.can_accept_client(1999));
        assert!(!limits.can_accept_client(2000));
    }

    #[test]
    fn test_estimate_max_memory() {
        let limits = ResourceLimits::default();
        let estimated_mb = limits.estimate_max_memory_mb();

        // Should be a reasonable estimate (not zero)
        assert!(estimated_mb > 0);
        // With default limits: 100 windows × 50 panes × 1000 sessions = 5M panes
        // Each pane: ~1MB (50KB buffer + 10K lines × 100 bytes)
        // Total: ~5TB theoretical max (which is why we need limits!)
        // Just verify it calculated something
        assert!(estimated_mb > 1000); // At least 1GB
    }

    #[test]
    fn test_serde_serialization() {
        let limits = ResourceLimits::default();
        let toml_str = toml::to_string(&limits).unwrap();

        // Should serialize successfully
        assert!(toml_str.contains("max_windows_per_session"));
        assert!(toml_str.contains("100"));
    }

    #[test]
    fn test_serde_deserialization() {
        let toml_str = r#"
            max_windows_per_session = 50
            max_panes_per_window = 25
        "#;

        let limits: ResourceLimits = toml::from_str(toml_str).unwrap();

        assert_eq!(limits.max_windows_per_session, 50);
        assert_eq!(limits.max_panes_per_window, 25);
        // Other values should use defaults
        assert_eq!(limits.max_scrollback_lines, 10_000);
    }
}
