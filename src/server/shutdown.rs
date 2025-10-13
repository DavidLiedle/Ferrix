//! Graceful Shutdown Coordinator
//!
//! Manages coordinated shutdown of all server components to ensure:
//! - No data loss (sessions saved, recordings flushed)
//! - No resource leaks (PTY handles closed, connections drained)
//! - Background tasks complete gracefully
//! - Forced shutdown after timeout
//!
//! ## Usage
//!
//! ```rust
//! let coordinator = ShutdownCoordinator::new(Duration::from_secs(30));
//! let shutdown_rx = coordinator.subscribe();
//!
//! // In background tasks:
//! loop {
//!     tokio::select! {
//!         _ = shutdown_rx.recv() => {
//!             // Graceful cleanup
//!             break;
//!         }
//!         // Normal work
//!     }
//! }
//!
//! // Initiate shutdown:
//! coordinator.shutdown().await;
//! ```

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use tokio::time::timeout;
use tracing::{info, warn, error};

/// Coordinates graceful shutdown across all server components
pub struct ShutdownCoordinator {
    /// Broadcast channel for shutdown signal
    shutdown_tx: broadcast::Sender<()>,

    /// Grace period before forced shutdown
    grace_period: Duration,

    /// Registered background tasks
    tasks: Arc<tokio::sync::RwLock<Vec<TaskHandle>>>,
}

/// Handle to a background task with metadata
struct TaskHandle {
    name: String,
    handle: JoinHandle<()>,
}

impl ShutdownCoordinator {
    /// Create new shutdown coordinator
    pub fn new(grace_period: Duration) -> Self {
        // Broadcast channel with capacity for 16 subscribers
        let (shutdown_tx, _) = broadcast::channel(16);

        Self {
            shutdown_tx,
            grace_period,
            tasks: Arc::new(tokio::sync::RwLock::new(Vec::new())),
        }
    }

    /// Create coordinator with default 30-second grace period
    pub fn with_default_timeout() -> Self {
        Self::new(Duration::from_secs(30))
    }

    /// Subscribe to shutdown notifications
    pub fn subscribe(&self) -> broadcast::Receiver<()> {
        self.shutdown_tx.subscribe()
    }

    /// Register a background task for shutdown tracking
    pub async fn register_task(&self, name: String, handle: JoinHandle<()>) {
        let mut tasks = self.tasks.write().await;
        info!("Registered background task: {}", name);
        tasks.push(TaskHandle { name, handle });
    }

    /// Initiate graceful shutdown
    pub async fn shutdown(self) -> ShutdownResult {
        info!("Initiating graceful shutdown (grace period: {:?})", self.grace_period);

        // Step 1: Broadcast shutdown signal to all subscribers
        let subscriber_count = self.shutdown_tx.receiver_count();
        info!("Broadcasting shutdown signal to {} subscribers", subscriber_count);

        if let Err(e) = self.shutdown_tx.send(()) {
            warn!("Failed to send shutdown signal: {}", e);
        }

        // Step 2: Wait for all registered tasks with timeout
        let mut tasks = self.tasks.write().await;
        let task_count = tasks.len();
        info!("Waiting for {} background tasks to complete", task_count);

        let mut completed = 0;
        let mut failed = 0;
        let mut timed_out = 0;

        for task in tasks.drain(..) {
            match timeout(self.grace_period, task.handle).await {
                Ok(Ok(())) => {
                    info!("Task '{}' completed gracefully", task.name);
                    completed += 1;
                }
                Ok(Err(e)) => {
                    error!("Task '{}' failed: {}", task.name, e);
                    failed += 1;
                }
                Err(_) => {
                    warn!("Task '{}' timed out after {:?}, forcing shutdown",
                          task.name, self.grace_period);
                    timed_out += 1;
                }
            }
        }

        info!("Shutdown complete: {} completed, {} failed, {} timed out",
              completed, failed, timed_out);

        ShutdownResult {
            completed,
            failed,
            timed_out,
        }
    }

    /// Check if shutdown has been initiated
    pub fn is_shutdown_initiated(&self) -> bool {
        self.shutdown_tx.receiver_count() > 0 && self.shutdown_tx.send(()).is_err()
    }
}

/// Result of shutdown operation
#[derive(Debug, Clone)]
pub struct ShutdownResult {
    pub completed: usize,
    pub failed: usize,
    pub timed_out: usize,
}

impl ShutdownResult {
    /// Check if shutdown was completely successful
    pub fn is_success(&self) -> bool {
        self.failed == 0 && self.timed_out == 0
    }

    /// Check if any tasks failed or timed out
    pub fn has_issues(&self) -> bool {
        !self.is_success()
    }
}

/// Helper for background tasks to handle shutdown gracefully
pub struct ShutdownHandle {
    rx: broadcast::Receiver<()>,
}

impl ShutdownHandle {
    /// Create from a receiver
    pub fn new(rx: broadcast::Receiver<()>) -> Self {
        Self { rx }
    }

    /// Check if shutdown has been signaled
    pub fn is_shutdown(&mut self) -> bool {
        matches!(self.rx.try_recv(), Ok(()) | Err(broadcast::error::TryRecvError::Closed))
    }

    /// Wait for shutdown signal
    pub async fn wait_for_shutdown(&mut self) {
        let _ = self.rx.recv().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_coordinator_creation() {
        let coordinator = ShutdownCoordinator::with_default_timeout();
        assert_eq!(coordinator.grace_period, Duration::from_secs(30));
    }

    #[tokio::test]
    async fn test_shutdown_signal_broadcast() {
        let coordinator = ShutdownCoordinator::with_default_timeout();
        let mut rx1 = coordinator.subscribe();
        let mut rx2 = coordinator.subscribe();

        // Spawn task that listens for shutdown
        let task1 = tokio::spawn(async move {
            rx1.recv().await.ok();
        });

        let task2 = tokio::spawn(async move {
            rx2.recv().await.ok();
        });

        // Initiate shutdown
        let result = coordinator.shutdown().await;

        // Wait for tasks
        task1.await.unwrap();
        task2.await.unwrap();

        assert_eq!(result.completed, 0); // No registered tasks
    }

    #[tokio::test]
    async fn test_graceful_task_completion() {
        let coordinator = ShutdownCoordinator::new(Duration::from_secs(5));
        let mut shutdown_rx = coordinator.subscribe();

        // Register a task that completes quickly
        let task = tokio::spawn(async move {
            shutdown_rx.recv().await.ok();
            // Simulate cleanup
            sleep(Duration::from_millis(100)).await;
        });

        coordinator.register_task("quick-task".to_string(), task).await;

        // Shutdown should complete successfully
        let result = coordinator.shutdown().await;
        assert_eq!(result.completed, 1);
        assert_eq!(result.failed, 0);
        assert_eq!(result.timed_out, 0);
        assert!(result.is_success());
    }

    #[tokio::test]
    async fn test_timeout_handling() {
        let coordinator = ShutdownCoordinator::new(Duration::from_millis(100));
        let mut shutdown_rx = coordinator.subscribe();

        // Register a task that takes too long
        let task = tokio::spawn(async move {
            shutdown_rx.recv().await.ok();
            // Simulate slow cleanup that exceeds grace period
            sleep(Duration::from_secs(10)).await;
        });

        coordinator.register_task("slow-task".to_string(), task).await;

        // Shutdown should timeout
        let result = coordinator.shutdown().await;
        assert_eq!(result.completed, 0);
        assert_eq!(result.timed_out, 1);
        assert!(!result.is_success());
    }

    #[tokio::test]
    async fn test_shutdown_handle() {
        let coordinator = ShutdownCoordinator::with_default_timeout();
        let rx = coordinator.subscribe();
        let mut handle = ShutdownHandle::new(rx);

        // Initially not shutdown
        assert!(!handle.is_shutdown());

        // Trigger shutdown
        coordinator.shutdown_tx.send(()).ok();

        // Should detect shutdown
        assert!(handle.is_shutdown());
    }

    #[tokio::test]
    async fn test_multiple_tasks() {
        let coordinator = ShutdownCoordinator::new(Duration::from_secs(2));

        // Register multiple tasks
        for i in 0..5 {
            let mut shutdown_rx = coordinator.subscribe();
            let task = tokio::spawn(async move {
                shutdown_rx.recv().await.ok();
                sleep(Duration::from_millis(50 * i)).await;
            });
            coordinator.register_task(format!("task-{}", i), task).await;
        }

        // All tasks should complete gracefully
        let result = coordinator.shutdown().await;
        assert_eq!(result.completed, 5);
        assert_eq!(result.failed, 0);
        assert_eq!(result.timed_out, 0);
    }
}
