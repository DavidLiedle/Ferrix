//! Common test assertions

use std::time::Duration;
use tokio::time::{sleep, timeout};

/// Assert that a condition becomes true within a timeout
///
/// # Arguments
/// * `duration` - Maximum time to wait
/// * `check_interval` - How often to check the condition
/// * `condition` - Async closure that returns bool
/// * `message` - Error message if condition never becomes true
pub async fn assert_eventually<F, Fut>(
    duration: Duration,
    check_interval: Duration,
    mut condition: F,
    message: &str,
) where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let result = timeout(duration, async {
        loop {
            if condition().await {
                return true;
            }
            sleep(check_interval).await;
        }
    })
    .await;

    assert!(
        result.is_ok(),
        "{} (waited {:?})",
        message,
        duration
    );
}

/// Assert that a condition remains true for a duration
///
/// # Arguments
/// * `duration` - How long to verify the condition
/// * `check_interval` - How often to check
/// * `condition` - Async closure that should stay true
/// * `message` - Error message if condition becomes false
pub async fn assert_stays_true<F, Fut>(
    duration: Duration,
    check_interval: Duration,
    mut condition: F,
    message: &str,
) where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let start = std::time::Instant::now();

    while start.elapsed() < duration {
        assert!(
            condition().await,
            "{} (failed after {:?})",
            message,
            start.elapsed()
        );
        sleep(check_interval).await;
    }
}

/// Assert that a session exists in the session list
pub fn assert_session_exists(sessions: &[String], session_name: &str) {
    assert!(
        sessions.iter().any(|s| s.contains(session_name)),
        "Session '{}' not found in: {:?}",
        session_name,
        sessions
    );
}

/// Assert that a session does not exist in the session list
pub fn assert_session_not_exists(sessions: &[String], session_name: &str) {
    assert!(
        !sessions.iter().any(|s| s.contains(session_name)),
        "Session '{}' should not exist but found in: {:?}",
        session_name,
        sessions
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[tokio::test]
    async fn test_assert_eventually_succeeds() {
        let counter = AtomicU32::new(0);
        assert_eventually(
            Duration::from_secs(1),
            Duration::from_millis(10),
            || async {
                let val = counter.fetch_add(1, Ordering::SeqCst);
                val >= 5
            },
            "Counter should reach 5"
        ).await;
    }

    #[tokio::test]
    async fn test_assert_stays_true_succeeds() {
        assert_stays_true(
            Duration::from_millis(100),
            Duration::from_millis(10),
            || async { true },
            "Should stay true"
        ).await;
    }

    #[test]
    fn test_assert_session_exists() {
        let sessions = vec![
            "session1 (1 windows)".to_string(),
            "session2 (2 windows)".to_string(),
        ];
        assert_session_exists(&sessions, "session1");
    }

    #[test]
    fn test_assert_session_not_exists() {
        let sessions = vec![
            "session1 (1 windows)".to_string(),
        ];
        assert_session_not_exists(&sessions, "session2");
    }
}
