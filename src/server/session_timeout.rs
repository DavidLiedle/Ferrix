//! Session Timeout Tracker
//!
//! Tracks idle and absolute timeouts for remote sessions to prevent
//! abandoned connections from consuming resources indefinitely.

use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use std::collections::HashMap;
use crate::protocol::ClientId;

/// Session timeout configuration
#[derive(Debug, Clone)]
pub struct TimeoutConfig {
    /// Maximum idle time before disconnection (default: 1 hour)
    pub idle_timeout: Duration,

    /// Maximum absolute session duration (default: 24 hours)
    pub absolute_timeout: Duration,
}

impl Default for TimeoutConfig {
    fn default() -> Self {
        Self {
            idle_timeout: Duration::from_secs(3600),      // 1 hour
            absolute_timeout: Duration::from_secs(86400), // 24 hours
        }
    }
}

/// Session activity tracking
#[derive(Debug, Clone)]
struct SessionActivity {
    /// When the session was created
    created_at: Instant,

    /// Last activity timestamp
    last_activity: Instant,
}

/// Tracks session timeouts for idle and absolute limits
pub struct SessionTimeoutTracker {
    config: TimeoutConfig,
    sessions: Arc<RwLock<HashMap<ClientId, SessionActivity>>>,
}

impl SessionTimeoutTracker {
    /// Create a new timeout tracker with default configuration
    pub fn new() -> Self {
        Self::with_config(TimeoutConfig::default())
    }

    /// Create a new timeout tracker with custom configuration
    pub fn with_config(config: TimeoutConfig) -> Self {
        Self {
            config,
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a new session
    pub async fn register_session(&self, client_id: ClientId) {
        let now = Instant::now();
        let mut sessions = self.sessions.write().await;
        sessions.insert(
            client_id,
            SessionActivity {
                created_at: now,
                last_activity: now,
            },
        );
        tracing::debug!("Registered session timeout tracking for client {:?}", client_id);
    }

    /// Record activity for a session (resets idle timer)
    pub async fn record_activity(&self, client_id: &ClientId) {
        let mut sessions = self.sessions.write().await;
        if let Some(activity) = sessions.get_mut(client_id) {
            activity.last_activity = Instant::now();
        }
    }

    /// Remove session tracking
    pub async fn remove_session(&self, client_id: &ClientId) {
        let mut sessions = self.sessions.write().await;
        sessions.remove(client_id);
        tracing::debug!("Removed session timeout tracking for client {:?}", client_id);
    }

    /// Check if a session has timed out
    pub async fn is_timed_out(&self, client_id: &ClientId) -> Option<TimeoutReason> {
        let sessions = self.sessions.read().await;
        if let Some(activity) = sessions.get(client_id) {
            let now = Instant::now();
            let idle_duration = now.duration_since(activity.last_activity);
            let absolute_duration = now.duration_since(activity.created_at);

            // Check idle timeout first
            if idle_duration > self.config.idle_timeout {
                return Some(TimeoutReason::Idle {
                    idle_duration,
                    limit: self.config.idle_timeout,
                });
            }

            // Check absolute timeout
            if absolute_duration > self.config.absolute_timeout {
                return Some(TimeoutReason::Absolute {
                    session_duration: absolute_duration,
                    limit: self.config.absolute_timeout,
                });
            }

            None
        } else {
            None
        }
    }

    /// Get all timed out sessions
    pub async fn get_timed_out_sessions(&self) -> Vec<(ClientId, TimeoutReason)> {
        let sessions = self.sessions.read().await;
        let now = Instant::now();
        let mut timed_out = Vec::new();

        for (client_id, activity) in sessions.iter() {
            let idle_duration = now.duration_since(activity.last_activity);
            let absolute_duration = now.duration_since(activity.created_at);

            if idle_duration > self.config.idle_timeout {
                timed_out.push((
                    *client_id,
                    TimeoutReason::Idle {
                        idle_duration,
                        limit: self.config.idle_timeout,
                    },
                ));
            } else if absolute_duration > self.config.absolute_timeout {
                timed_out.push((
                    *client_id,
                    TimeoutReason::Absolute {
                        session_duration: absolute_duration,
                        limit: self.config.absolute_timeout,
                    },
                ));
            }
        }

        timed_out
    }

    /// Get session statistics
    pub async fn get_stats(&self) -> SessionTimeoutStats {
        let sessions = self.sessions.read().await;
        let now = Instant::now();

        let mut total_sessions = 0;
        let mut idle_warnings = 0;
        let mut absolute_warnings = 0;

        for activity in sessions.values() {
            total_sessions += 1;

            let idle_duration = now.duration_since(activity.last_activity);
            let absolute_duration = now.duration_since(activity.created_at);

            // Count sessions approaching timeout (80% of limit)
            if idle_duration > self.config.idle_timeout.mul_f32(0.8) {
                idle_warnings += 1;
            }
            if absolute_duration > self.config.absolute_timeout.mul_f32(0.8) {
                absolute_warnings += 1;
            }
        }

        SessionTimeoutStats {
            total_sessions,
            idle_warnings,
            absolute_warnings,
            config: self.config.clone(),
        }
    }
}

impl Default for SessionTimeoutTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Reason for session timeout
#[derive(Debug, Clone)]
pub enum TimeoutReason {
    /// Session exceeded idle timeout
    Idle {
        idle_duration: Duration,
        limit: Duration,
    },
    /// Session exceeded absolute timeout
    Absolute {
        session_duration: Duration,
        limit: Duration,
    },
}

impl TimeoutReason {
    /// Get a human-readable description
    pub fn description(&self) -> String {
        match self {
            TimeoutReason::Idle { idle_duration, limit } => {
                format!(
                    "Idle timeout: session idle for {:?} (limit: {:?})",
                    idle_duration, limit
                )
            }
            TimeoutReason::Absolute { session_duration, limit } => {
                format!(
                    "Absolute timeout: session duration {:?} (limit: {:?})",
                    session_duration, limit
                )
            }
        }
    }
}

/// Session timeout statistics
#[derive(Debug, Clone)]
pub struct SessionTimeoutStats {
    pub total_sessions: usize,
    pub idle_warnings: usize,
    pub absolute_warnings: usize,
    pub config: TimeoutConfig,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_timeout_tracker_creation() {
        let tracker = SessionTimeoutTracker::new();
        let stats = tracker.get_stats().await;
        assert_eq!(stats.total_sessions, 0);
    }

    #[tokio::test]
    async fn test_session_registration() {
        let tracker = SessionTimeoutTracker::new();
        let client_id = ClientId(uuid::Uuid::new_v4());

        tracker.register_session(client_id).await;

        let stats = tracker.get_stats().await;
        assert_eq!(stats.total_sessions, 1);
    }

    #[tokio::test]
    async fn test_session_removal() {
        let tracker = SessionTimeoutTracker::new();
        let client_id = ClientId(uuid::Uuid::new_v4());

        tracker.register_session(client_id).await;
        tracker.remove_session(&client_id).await;

        let stats = tracker.get_stats().await;
        assert_eq!(stats.total_sessions, 0);
    }

    #[tokio::test]
    async fn test_no_timeout_for_active_session() {
        let tracker = SessionTimeoutTracker::new();
        let client_id = ClientId(uuid::Uuid::new_v4());

        tracker.register_session(client_id).await;

        let timeout = tracker.is_timed_out(&client_id).await;
        assert!(timeout.is_none());
    }

    #[tokio::test]
    async fn test_idle_timeout_detection() {
        let config = TimeoutConfig {
            idle_timeout: Duration::from_millis(100),
            absolute_timeout: Duration::from_secs(3600),
        };
        let tracker = SessionTimeoutTracker::with_config(config);
        let client_id = ClientId(uuid::Uuid::new_v4());

        tracker.register_session(client_id).await;

        // Wait for idle timeout
        tokio::time::sleep(Duration::from_millis(150)).await;

        let timeout = tracker.is_timed_out(&client_id).await;
        assert!(timeout.is_some());
        assert!(matches!(timeout.unwrap(), TimeoutReason::Idle { .. }));
    }

    #[tokio::test]
    async fn test_activity_resets_idle_timer() {
        let config = TimeoutConfig {
            idle_timeout: Duration::from_millis(200),
            absolute_timeout: Duration::from_secs(3600),
        };
        let tracker = SessionTimeoutTracker::with_config(config);
        let client_id = ClientId(uuid::Uuid::new_v4());

        tracker.register_session(client_id).await;

        // Wait half the timeout
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Record activity
        tracker.record_activity(&client_id).await;

        // Wait another half timeout (shouldn't timeout yet)
        tokio::time::sleep(Duration::from_millis(100)).await;

        let timeout = tracker.is_timed_out(&client_id).await;
        assert!(timeout.is_none());
    }
}
