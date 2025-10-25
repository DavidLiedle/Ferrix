//! Test server management utilities

use std::process::{Command, Stdio, Child};
use std::time::Duration;
use std::path::PathBuf;
use tokio::time::sleep;

/// Test server instance with automatic cleanup
pub struct TestServer {
    process: Child,
    socket_path: PathBuf,
}

impl TestServer {
    /// Start a test server on the given socket path
    ///
    /// # Arguments
    /// * `socket_path` - Path where the Unix domain socket will be created
    /// * `foreground` - If true, run in foreground mode (default for tests)
    ///
    /// # Returns
    /// A running TestServer instance, or panics if startup fails
    pub async fn start(socket_path: PathBuf, foreground: bool) -> Self {
        let ferrix_path = Self::find_ferrix_binary();

        let mut cmd = Command::new(ferrix_path);
        cmd.arg("--socket")
            .arg(&socket_path)
            .arg("server");

        if foreground {
            cmd.arg("--foreground");
        }

        // Redirect output to avoid polluting test output
        cmd.stdout(Stdio::null())
            .stderr(Stdio::null());

        let process = cmd.spawn()
            .expect("Failed to start ferrix server");

        // Wait for socket to be created
        let mut retries = 0;
        while !socket_path.exists() && retries < 100 {
            sleep(Duration::from_millis(50)).await;
            retries += 1;
        }

        assert!(
            socket_path.exists(),
            "Server failed to create socket at {:?} after 5s",
            socket_path
        );

        // Give server a moment to fully initialize
        sleep(Duration::from_millis(200)).await;

        Self {
            process,
            socket_path,
        }
    }

    /// Start a test server with default settings (foreground mode)
    pub async fn start_default(socket_path: PathBuf) -> Self {
        Self::start(socket_path, true).await
    }

    /// Get the socket path for this server
    pub fn socket_path(&self) -> &PathBuf {
        &self.socket_path
    }

    /// Check if the server process is still running
    pub fn is_running(&mut self) -> bool {
        match self.process.try_wait() {
            Ok(None) => true,          // Still running
            Ok(Some(_)) => false,      // Exited
            Err(_) => false,           // Error checking status
        }
    }

    /// Wait for server to exit with timeout
    pub async fn wait_for_exit(&mut self, timeout: Duration) -> bool {
        let start = std::time::Instant::now();
        while start.elapsed() < timeout {
            if !self.is_running() {
                return true;
            }
            sleep(Duration::from_millis(100)).await;
        }
        false
    }

    /// Find the ferrix binary (release or debug)
    fn find_ferrix_binary() -> String {
        if std::path::Path::new("./target/release/ferrix").exists() {
            "./target/release/ferrix".to_string()
        } else if std::path::Path::new("./target/debug/ferrix").exists() {
            "./target/debug/ferrix".to_string()
        } else {
            panic!("Ferrix binary not found. Run 'cargo build' first.");
        }
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        // Try graceful shutdown first
        let _ = self.process.kill();
        let _ = self.process.wait();

        // Clean up socket file if it still exists
        if self.socket_path.exists() {
            let _ = std::fs::remove_file(&self.socket_path);
        }
    }
}
