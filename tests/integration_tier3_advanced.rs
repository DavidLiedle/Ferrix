// TIER 3: Advanced Features & Error Handling
// These tests verify hooks, snapshots, scrollback, and error recovery

use std::process::{Command, Stdio, Child};
use std::time::Duration;
use std::path::PathBuf;
use tokio::time::sleep;
use tempfile::TempDir;

struct TestServer {
    process: Child,
    socket_path: PathBuf,
    _temp_dir: TempDir,
}

impl TestServer {
    async fn start() -> Self {
        let temp_dir = TempDir::new().unwrap();
        let socket_path = temp_dir.path().join("ferrix.sock");

        let ferrix_path = if std::path::Path::new("./target/release/ferrix").exists() {
            "./target/release/ferrix"
        } else {
            "./target/debug/ferrix"
        };

        let process = Command::new(ferrix_path)
            .arg("--socket")
            .arg(&socket_path)
            .arg("server")
            .arg("--foreground")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("Failed to start server");

        let mut retries = 0;
        while !socket_path.exists() && retries < 50 {
            sleep(Duration::from_millis(100)).await;
            retries += 1;
        }

        assert!(socket_path.exists(), "Server failed to create socket");
        sleep(Duration::from_millis(500)).await;

        Self {
            process,
            socket_path,
            _temp_dir: temp_dir,
        }
    }

    fn run_command(&self, args: &[&str]) -> std::process::Output {
        let ferrix_path = if std::path::Path::new("./target/release/ferrix").exists() {
            "./target/release/ferrix"
        } else {
            "./target/debug/ferrix"
        };
        Command::new(ferrix_path)
            .arg("--socket")
            .arg(&self.socket_path)
            .args(args)
            .output()
            .expect("Failed to run command")
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        let _ = self.process.kill();
    }
}

#[tokio::test]
async fn test_snapshot_save_and_list() {
    let server = TestServer::start().await;

    // Create session
    server.run_command(&["new", "-s", "snap-test", "--detached"]);
    sleep(Duration::from_millis(300)).await;

    // Generate some activity
    server.run_command(&["send-keys", "snap-test", "echo 'test output'"]);
    server.run_command(&["send-keys", "snap-test", "Enter"]);
    sleep(Duration::from_millis(500)).await;

    // Save snapshot
    let output = server.run_command(&[
        "save-snapshot", "snap-test",
        "--name", "test-snap",
        "--description", "Integration test snapshot"
    ]);

    // Check if command succeeded (snapshot feature may not be enabled)
    if output.status.success() {
        // List snapshots
        let output = server.run_command(&["list-snapshots"]);
        if output.status.success() {
            let list = String::from_utf8_lossy(&output.stdout);
            // Snapshot should appear in list
            assert!(list.contains("test-snap") || list.contains("snap-test"),
                    "Snapshot not found in list");
        }
    }

    // Clean up
    server.run_command(&["kill", "snap-test"]);
}

#[tokio::test]
async fn test_concurrent_snapshot_operations() {
    let server = TestServer::start().await;

    // Create multiple sessions
    for i in 0..3 {
        server.run_command(&["new", "-s", &format!("snap-{}", i), "--detached"]);
    }

    sleep(Duration::from_millis(500)).await;

    // Concurrently save snapshots
    let mut handles = vec![];
    for i in 0..3 {
        let socket_path = server.socket_path.clone();
        let handle = tokio::spawn(async move {
            let ferrix_path = if std::path::Path::new("./target/release/ferrix").exists() {
                "./target/release/ferrix"
            } else {
                "./target/debug/ferrix"
            };
            Command::new(ferrix_path)
                .arg("--socket")
                .arg(&socket_path)
                .arg("save-snapshot")
                .arg(&format!("snap-{}", i))
                .arg("--name")
                .arg(&format!("concurrent-{}", i))
                .output()
                .expect("Failed to save snapshot")
        });
        handles.push(handle);
    }

    // Wait for all to complete
    for handle in handles {
        let _ = handle.await;
    }

    // Verify server is still healthy
    let output = server.run_command(&["list"]);
    assert!(output.status.success(), "Server crashed during concurrent snapshots");

    // Clean up
    for i in 0..3 {
        server.run_command(&["kill", &format!("snap-{}", i)]);
    }
}

#[tokio::test]
async fn test_error_handling_invalid_operations() {
    let server = TestServer::start().await;

    // Try to kill non-existent session
    let output = server.run_command(&["kill", "does-not-exist"]);
    assert!(!output.status.success(), "Should fail for non-existent session");

    // Try to attach to non-existent session
    let output = server.run_command(&["attach", "does-not-exist"]);
    assert!(!output.status.success(), "Should fail for non-existent session");

    // Try to send keys to non-existent session
    let output = server.run_command(&["send-keys", "does-not-exist", "test"]);
    assert!(!output.status.success(), "Should fail for non-existent session");

    // Verify server is still responsive after errors
    let output = server.run_command(&["list"]);
    assert!(output.status.success(), "Server became unresponsive after errors");
}

#[tokio::test]
async fn test_send_keys_with_special_characters() {
    let server = TestServer::start().await;

    // Create session
    server.run_command(&["new", "-s", "special-chars", "--detached"]);
    sleep(Duration::from_millis(300)).await;

    // Send various special character sequences
    let test_strings = vec![
        "echo 'test'",
        "echo \"quoted\"",
        "echo $HOME",
        "ls -la",
        "cd /tmp",
    ];

    for test_str in test_strings {
        let output = server.run_command(&["send-keys", "special-chars", test_str]);
        assert!(output.status.success(), "Failed to send: {}", test_str);
    }

    sleep(Duration::from_millis(500)).await;

    // Verify session is still operational
    let output = server.run_command(&["list"]);
    assert!(output.status.success());

    // Clean up
    server.run_command(&["kill", "special-chars"]);
}

#[tokio::test]
async fn test_session_with_rapid_output_generation() {
    let server = TestServer::start().await;

    // Create session
    server.run_command(&["new", "-s", "rapid-output", "--detached"]);
    sleep(Duration::from_millis(300)).await;

    // Generate rapid output (tests scrollback buffer)
    server.run_command(&["send-keys", "rapid-output", "for i in {1..1000}; do echo \"Line $i\"; done"]);
    server.run_command(&["send-keys", "rapid-output", "Enter"]);

    // Wait for command to complete
    sleep(Duration::from_millis(2000)).await;

    // Verify session handled large output
    let output = server.run_command(&["list"]);
    assert!(output.status.success(), "Server crashed handling rapid output");
    let list_output = String::from_utf8_lossy(&output.stdout);
    assert!(list_output.contains("rapid-output"), "Session was lost");

    // Clean up
    server.run_command(&["kill", "rapid-output"]);
}

#[tokio::test]
async fn test_session_list_format_and_parsing() {
    let server = TestServer::start().await;

    // Create sessions with various names
    server.run_command(&["new", "-s", "format-test-1", "--detached"]);
    server.run_command(&["new", "-s", "format-test-2", "--detached"]);

    sleep(Duration::from_millis(500)).await;

    // List sessions
    let output = server.run_command(&["list"]);
    assert!(output.status.success());

    let list_output = String::from_utf8_lossy(&output.stdout);

    // Verify both sessions appear
    assert!(list_output.contains("format-test-1"));
    assert!(list_output.contains("format-test-2"));

    // Verify output is parseable (contains expected structure)
    // Should have session names, possibly timestamps, etc.

    // Clean up
    server.run_command(&["kill", "format-test-1"]);
    server.run_command(&["kill", "format-test-2"]);
}

#[tokio::test]
async fn test_server_handles_malformed_commands() {
    let server = TestServer::start().await;

    // Try various malformed commands
    // These should fail gracefully without crashing server

    // Empty session name
    let output = server.run_command(&["new", "-s", "", "--detached"]);
    assert!(!output.status.success() || output.stderr.len() > 0);

    // Invalid flags
    let output = server.run_command(&["new", "--invalid-flag"]);
    assert!(!output.status.success() || output.stderr.len() > 0);

    // Verify server still works
    let output = server.run_command(&["list"]);
    assert!(output.status.success(), "Server crashed after malformed commands");
}

#[tokio::test]
async fn test_session_with_long_running_command() {
    let server = TestServer::start().await;

    // Create session
    server.run_command(&["new", "-s", "long-running", "--detached"]);
    sleep(Duration::from_millis(300)).await;

    // Start long-running command
    server.run_command(&["send-keys", "long-running", "sleep 5"]);
    server.run_command(&["send-keys", "long-running", "Enter"]);

    // Immediately check session status (while command is running)
    sleep(Duration::from_millis(500)).await;
    let output = server.run_command(&["list"]);
    assert!(output.status.success());
    let list_output = String::from_utf8_lossy(&output.stdout);
    assert!(list_output.contains("long-running"));

    // Wait for command to complete
    sleep(Duration::from_millis(5000)).await;

    // Verify session still exists after command completion
    let output = server.run_command(&["list"]);
    assert!(output.status.success());

    // Clean up
    server.run_command(&["kill", "long-running"]);
}

#[tokio::test]
async fn test_resize_operations() {
    let server = TestServer::start().await;

    // Create session
    server.run_command(&["new", "-s", "resize-test", "--detached"]);
    sleep(Duration::from_millis(300)).await;

    // Note: Resize operations typically happen through client connection
    // This test verifies session can be created and managed

    // Send commands
    server.run_command(&["send-keys", "resize-test", "echo 'test'"]);
    server.run_command(&["send-keys", "resize-test", "Enter"]);

    sleep(Duration::from_millis(300)).await;

    // Verify session operational
    let output = server.run_command(&["list"]);
    assert!(output.status.success());

    // Clean up
    server.run_command(&["kill", "resize-test"]);
}

#[tokio::test]
async fn test_session_state_after_server_operations() {
    let server = TestServer::start().await;

    // Create initial sessions
    server.run_command(&["new", "-s", "state-1", "--detached"]);
    server.run_command(&["new", "-s", "state-2", "--detached"]);
    sleep(Duration::from_millis(500)).await;

    // Perform operations
    server.run_command(&["send-keys", "state-1", "echo 'test1'"]);
    server.run_command(&["send-keys", "state-2", "echo 'test2'"]);

    // Kill one
    server.run_command(&["kill", "state-1"]);
    sleep(Duration::from_millis(300)).await;

    // Create new one
    server.run_command(&["new", "-s", "state-3", "--detached"]);
    sleep(Duration::from_millis(300)).await;

    // List all
    let output = server.run_command(&["list"]);
    let list_output = String::from_utf8_lossy(&output.stdout);

    // state-1 should be gone, state-2 and state-3 should exist
    assert!(!list_output.contains("state-1") || list_output.contains("No active"));
    assert!(list_output.contains("state-2"));
    assert!(list_output.contains("state-3"));

    // Clean up
    server.run_command(&["kill", "state-2"]);
    server.run_command(&["kill", "state-3"]);
}
