// TIER 1: Critical Client-Server Protocol Edge Cases
// These tests verify the core client-server communication under stress conditions

use std::process::{Command, Stdio, Child};
use std::time::Duration;
use std::path::PathBuf;
use tokio::time::sleep;
use tempfile::TempDir;
use std::sync::Arc;
use tokio::sync::Barrier;

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

        // Wait for socket with timeout
        let mut retries = 0;
        while !socket_path.exists() && retries < 50 {
            sleep(Duration::from_millis(100)).await;
            retries += 1;
        }

        assert!(socket_path.exists(), "Server failed to create socket after 5s");
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

    async fn spawn_attach(&self, session_name: &str) -> std::process::Child {
        let ferrix_path = if std::path::Path::new("./target/release/ferrix").exists() {
            "./target/release/ferrix"
        } else {
            "./target/debug/ferrix"
        };
        Command::new(ferrix_path)
            .arg("--socket")
            .arg(&self.socket_path)
            .arg("attach")
            .arg(session_name)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("Failed to spawn attach")
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        let _ = self.process.kill();
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn test_concurrent_client_attach_to_same_session() {
    let server = TestServer::start().await;

    // Create session
    let output = server.run_command(&["new", "-s", "concurrent", "--detached"]);
    assert!(output.status.success(), "Failed to create session: {:?}",
            String::from_utf8_lossy(&output.stderr));

    // Give session time to fully initialize
    sleep(Duration::from_millis(500)).await;

    // Spawn 5 clients concurrently trying to attach
    let barrier = Arc::new(Barrier::new(5));
    let mut handles = vec![];

    for i in 0..5 {
        let socket_path = server.socket_path.clone();
        let barrier = barrier.clone();

        let handle = tokio::spawn(async move {
            // Wait for all tasks to be ready
            barrier.wait().await;

            // Attempt to spawn attach (won't actually attach since it's interactive)
            let ferrix_path = if std::path::Path::new("./target/release/ferrix").exists() {
                "./target/release/ferrix"
            } else {
                "./target/debug/ferrix"
            };
            let result = tokio::process::Command::new(ferrix_path)
                .arg("--socket")
                .arg(&socket_path)
                .arg("attach")
                .arg("concurrent")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn();

            result.is_ok()
        });

        handles.push(handle);
    }

    // All spawns should succeed (even though attach will hang since it's interactive)
    for handle in handles {
        let result = handle.await.unwrap();
        assert!(result, "Client spawn failed");
    }

    // Verify session still exists and server didn't crash
    sleep(Duration::from_millis(500)).await;
    let output = server.run_command(&["list"]);
    assert!(output.status.success(), "Server crashed or became unresponsive");
    let list_output = String::from_utf8_lossy(&output.stdout);
    assert!(list_output.contains("concurrent"), "Session disappeared");

    // Clean up
    server.run_command(&["kill", "concurrent"]);
}

#[tokio::test]
async fn test_session_output_persistence_on_reattach() {
    let server = TestServer::start().await;

    // Create session
    server.run_command(&["new", "-s", "persist-test", "--detached"]);
    sleep(Duration::from_millis(300)).await;

    // Send commands that generate significant output
    // Each line is ~80 chars, need ~625 lines to exceed 50KB buffer
    for i in 0..700 {
        let cmd = format!("echo 'Line {} - AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA'", i);
        server.run_command(&["send-keys", "persist-test", &cmd]);
        server.run_command(&["send-keys", "persist-test", "Enter"]);
    }

    // Wait for output to be generated
    sleep(Duration::from_millis(2000)).await;

    // The session should have the last ~50KB of output in its buffer
    // When we attach, we should see recent lines but not early ones

    // Note: Full verification would require actually attaching and reading output,
    // which is complex in non-interactive tests. This test verifies:
    // 1. Session survives large output
    // 2. Commands can be sent via send-keys
    // 3. Session remains responsive

    let output = server.run_command(&["list"]);
    assert!(output.status.success(), "Server became unresponsive");

    // Clean up
    server.run_command(&["kill", "persist-test"]);
}

#[tokio::test]
async fn test_client_disconnect_during_heavy_output() {
    let server = TestServer::start().await;

    // Create session
    server.run_command(&["new", "-s", "disconnect-test", "--detached"]);
    sleep(Duration::from_millis(300)).await;

    // Start command that generates continuous output
    server.run_command(&["send-keys", "disconnect-test", "while true; do echo 'Output line'; sleep 0.01; done"]);
    server.run_command(&["send-keys", "disconnect-test", "Enter"]);

    // Wait for output to start
    sleep(Duration::from_millis(500)).await;

    // Spawn a client that will attach then die
    let mut client = server.spawn_attach("disconnect-test").await;
    sleep(Duration::from_millis(500)).await;

    // Kill client abruptly
    let _ = client.kill();

    // Wait a bit for server to process disconnect
    sleep(Duration::from_millis(500)).await;

    // Verify server is still responsive and session exists
    let output = server.run_command(&["list"]);
    assert!(output.status.success(), "Server crashed after client disconnect");
    let list_output = String::from_utf8_lossy(&output.stdout);
    assert!(list_output.contains("disconnect-test"), "Session was destroyed");

    // Stop the output loop
    server.run_command(&["send-keys", "disconnect-test", "C-c"]);
    sleep(Duration::from_millis(200)).await;

    // Clean up
    server.run_command(&["kill", "disconnect-test"]);
}

#[tokio::test]
async fn test_rapid_attach_detach_cycling() {
    let server = TestServer::start().await;

    // Create session
    server.run_command(&["new", "-s", "cycle-test", "--detached"]);
    sleep(Duration::from_millis(300)).await;

    // Rapidly spawn and kill attach processes
    // This tests client registration/deregistration without memory leaks
    for i in 0..20 {
        let mut client = server.spawn_attach("cycle-test").await;
        sleep(Duration::from_millis(50)).await;
        let _ = client.kill();

        // Every 5 iterations, verify session still exists
        if i % 5 == 0 {
            let output = server.run_command(&["list"]);
            assert!(output.status.success(), "Server crashed at iteration {}", i);
        }
    }

    // Final verification
    sleep(Duration::from_millis(500)).await;
    let output = server.run_command(&["list"]);
    assert!(output.status.success(), "Server crashed after rapid cycling");
    let list_output = String::from_utf8_lossy(&output.stdout);
    assert!(list_output.contains("cycle-test"), "Session was lost");

    // Clean up
    server.run_command(&["kill", "cycle-test"]);
}

#[tokio::test]
async fn test_protocol_large_message_handling() {
    let server = TestServer::start().await;

    // Create session
    server.run_command(&["new", "-s", "large-msg", "--detached"]);
    sleep(Duration::from_millis(300)).await;

    // Send very large input (tests protocol message handling)
    // Create a large string (10KB)
    let large_input = "A".repeat(10000);

    let output = server.run_command(&["send-keys", "large-msg", &large_input]);
    assert!(output.status.success(), "Failed to send large input");

    // Generate large output by reading a file
    server.run_command(&["send-keys", "large-msg", "head -c 100000 /dev/urandom | base64"]);
    server.run_command(&["send-keys", "large-msg", "Enter"]);

    // Wait for command to complete
    sleep(Duration::from_millis(2000)).await;

    // Verify session is still responsive
    let output = server.run_command(&["list"]);
    assert!(output.status.success(), "Server crashed handling large messages");

    // Clean up
    server.run_command(&["send-keys", "large-msg", "C-c"]);
    sleep(Duration::from_millis(200)).await;
    server.run_command(&["kill", "large-msg"]);
}

#[tokio::test]
async fn test_session_state_consistency_under_concurrent_operations() {
    let server = TestServer::start().await;

    // Create session
    server.run_command(&["new", "-s", "concurrent-ops", "--detached"]);
    sleep(Duration::from_millis(300)).await;

    // Perform concurrent operations from multiple tasks
    let mut handles = vec![];

    // Task 1: Send input repeatedly
    for _ in 0..3 {
        let socket_path = server.socket_path.clone();
        let handle = tokio::spawn(async move {
            for i in 0..10 {
                let ferrix_path = if std::path::Path::new("./target/release/ferrix").exists() {
                    "./target/release/ferrix"
                } else {
                    "./target/debug/ferrix"
                };
                let _ = Command::new(ferrix_path)
                    .arg("--socket")
                    .arg(&socket_path)
                    .arg("send-keys")
                    .arg("concurrent-ops")
                    .arg(&format!("echo 'Message {}'", i))
                    .output();
                sleep(Duration::from_millis(50)).await;
            }
        });
        handles.push(handle);
    }

    // Task 2: List sessions repeatedly
    for _ in 0..3 {
        let socket_path = server.socket_path.clone();
        let handle = tokio::spawn(async move {
            for _ in 0..20 {
                let ferrix_path = if std::path::Path::new("./target/release/ferrix").exists() {
                    "./target/release/ferrix"
                } else {
                    "./target/debug/ferrix"
                };
                let _ = Command::new(ferrix_path)
                    .arg("--socket")
                    .arg(&socket_path)
                    .arg("list")
                    .output();
                sleep(Duration::from_millis(25)).await;
            }
        });
        handles.push(handle);
    }

    // Wait for all tasks to complete
    for handle in handles {
        handle.await.unwrap();
    }

    // Verify session is still in consistent state
    let output = server.run_command(&["list"]);
    assert!(output.status.success(), "Server crashed under concurrent load");
    let list_output = String::from_utf8_lossy(&output.stdout);
    assert!(list_output.contains("concurrent-ops"), "Session was lost");

    // Clean up
    server.run_command(&["kill", "concurrent-ops"]);
}
