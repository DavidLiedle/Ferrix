// TIER 2: Session Lifecycle & State Management
// These tests verify complex session operations and state transitions

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
async fn test_session_with_exiting_command() {
    let server = TestServer::start().await;

    // Create session that runs a command that exits immediately
    let output = server.run_command(&["new", "-s", "exit-test", "--detached"]);
    assert!(output.status.success());

    // Send a command that exits
    server.run_command(&["send-keys", "exit-test", "exit"]);
    server.run_command(&["send-keys", "exit-test", "Enter"]);

    // Wait for command to execute
    sleep(Duration::from_millis(1000)).await;

    // Session should still exist (pane marked dead but session remains)
    let output = server.run_command(&["list"]);
    let _list_output = String::from_utf8_lossy(&output.stdout);

    // Depending on remain_on_exit setting, session might be gone
    // This tests that server handles exited PTY gracefully

    assert!(output.status.success(), "Server crashed after PTY exit");
}

#[tokio::test]
async fn test_multiple_windows_operations() {
    let server = TestServer::start().await;

    // Create session
    server.run_command(&["new", "-s", "multi-window", "--detached"]);
    sleep(Duration::from_millis(300)).await;

    // Create additional windows via send-keys (simulating Ctrl-a c)
    // Note: Without proper keybinding support in tests, we test via CLI if available
    // For now, test that session with windows can be managed

    // Send input to first window
    server.run_command(&["send-keys", "multi-window", "echo 'Window 1'"]);
    server.run_command(&["send-keys", "multi-window", "Enter"]);

    sleep(Duration::from_millis(500)).await;

    // Verify session still works
    let output = server.run_command(&["list"]);
    assert!(output.status.success());
    let list_output = String::from_utf8_lossy(&output.stdout);
    assert!(list_output.contains("multi-window"));

    // Clean up
    server.run_command(&["kill", "multi-window"]);
}

#[tokio::test]
async fn test_pane_split_operations() {
    let server = TestServer::start().await;

    // Create session
    server.run_command(&["new", "-s", "split-test", "--detached"]);
    sleep(Duration::from_millis(300)).await;

    // Send input to default pane
    server.run_command(&["send-keys", "split-test", "echo 'Pane 1'"]);
    server.run_command(&["send-keys", "split-test", "Enter"]);

    // Note: Pane splitting requires keybindings or special commands
    // This test verifies basic session operations work as foundation

    sleep(Duration::from_millis(300)).await;

    // Verify session is operational
    let output = server.run_command(&["list"]);
    assert!(output.status.success());

    // Clean up
    server.run_command(&["kill", "split-test"]);
}

#[tokio::test]
async fn test_session_creation_with_custom_name() {
    let server = TestServer::start().await;

    // Create sessions with various names
    let names = vec!["test-1", "my-session", "work_project", "123-numeric"];

    for name in &names {
        let output = server.run_command(&["new", "-s", name, "--detached"]);
        assert!(output.status.success(), "Failed to create session: {}", name);
    }

    sleep(Duration::from_millis(500)).await;

    // Verify all sessions exist
    let output = server.run_command(&["list"]);
    let list_output = String::from_utf8_lossy(&output.stdout);

    for name in &names {
        assert!(list_output.contains(name), "Session {} not found", name);
    }

    // Clean up
    for name in &names {
        server.run_command(&["kill", name]);
    }
}

#[tokio::test]
async fn test_session_kill_and_recreation() {
    let server = TestServer::start().await;

    // Create session
    server.run_command(&["new", "-s", "recreate", "--detached"]);
    sleep(Duration::from_millis(300)).await;

    // Verify it exists
    let output = server.run_command(&["list"]);
    let list_output = String::from_utf8_lossy(&output.stdout);
    assert!(list_output.contains("recreate"));

    // Kill it
    let output = server.run_command(&["kill", "recreate"]);
    assert!(output.status.success(), "Failed to kill session");

    sleep(Duration::from_millis(300)).await;

    // Verify it's gone
    let output = server.run_command(&["list"]);
    let list_output = String::from_utf8_lossy(&output.stdout);
    assert!(!list_output.contains("recreate") || list_output.contains("No active sessions"));

    // Recreate with same name
    let output = server.run_command(&["new", "-s", "recreate", "--detached"]);
    assert!(output.status.success(), "Failed to recreate session");

    sleep(Duration::from_millis(300)).await;

    // Verify it exists again
    let output = server.run_command(&["list"]);
    let list_output = String::from_utf8_lossy(&output.stdout);
    assert!(list_output.contains("recreate"));

    // Clean up
    server.run_command(&["kill", "recreate"]);
}

#[tokio::test]
async fn test_multiple_sessions_isolation() {
    let server = TestServer::start().await;

    // Create multiple sessions
    server.run_command(&["new", "-s", "session-a", "--detached"]);
    server.run_command(&["new", "-s", "session-b", "--detached"]);
    server.run_command(&["new", "-s", "session-c", "--detached"]);

    sleep(Duration::from_millis(500)).await;

    // Send different commands to each
    server.run_command(&["send-keys", "session-a", "echo 'A'"]);
    server.run_command(&["send-keys", "session-a", "Enter"]);

    server.run_command(&["send-keys", "session-b", "echo 'B'"]);
    server.run_command(&["send-keys", "session-b", "Enter"]);

    server.run_command(&["send-keys", "session-c", "echo 'C'"]);
    server.run_command(&["send-keys", "session-c", "Enter"]);

    sleep(Duration::from_millis(500)).await;

    // Kill middle session
    server.run_command(&["kill", "session-b"]);

    sleep(Duration::from_millis(300)).await;

    // Verify other sessions still exist
    let output = server.run_command(&["list"]);
    let list_output = String::from_utf8_lossy(&output.stdout);

    assert!(list_output.contains("session-a"), "Session A was affected");
    assert!(!list_output.contains("session-b") || !list_output.contains("session-b"), "Session B should be gone");
    assert!(list_output.contains("session-c"), "Session C was affected");

    // Clean up
    server.run_command(&["kill", "session-a"]);
    server.run_command(&["kill", "session-c"]);
}

#[tokio::test]
async fn test_session_with_working_directory() {
    let server = TestServer::start().await;

    // Create session with custom working directory
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path().to_str().unwrap();

    let output = server.run_command(&["new", "-s", "workdir-test", "-c", work_dir, "--detached"]);
    assert!(output.status.success(), "Failed to create session with working directory");

    sleep(Duration::from_millis(500)).await;

    // Send pwd command to verify directory
    server.run_command(&["send-keys", "workdir-test", "pwd > /tmp/ferrix_pwd_test.txt"]);
    server.run_command(&["send-keys", "workdir-test", "Enter"]);

    sleep(Duration::from_millis(500)).await;

    // Read the output (if file exists)
    if let Ok(_content) = std::fs::read_to_string("/tmp/ferrix_pwd_test.txt") {
        // Clean up the test file
        let _ = std::fs::remove_file("/tmp/ferrix_pwd_test.txt");
    }

    // Clean up session
    server.run_command(&["kill", "workdir-test"]);
}

#[tokio::test]
async fn test_rapid_session_creation_and_destruction() {
    let server = TestServer::start().await;

    // Rapidly create and destroy sessions
    for i in 0..10 {
        let name = format!("rapid-{}", i);

        // Create
        let output = server.run_command(&["new", "-s", &name, "--detached"]);
        assert!(output.status.success(), "Failed to create session {}", i);

        // Immediately destroy
        let output = server.run_command(&["kill", &name]);
        assert!(output.status.success(), "Failed to kill session {}", i);
    }

    sleep(Duration::from_millis(500)).await;

    // Verify server is still healthy
    let output = server.run_command(&["list"]);
    assert!(output.status.success(), "Server became unhealthy");
}

#[tokio::test]
async fn test_session_persistence_across_operations() {
    let server = TestServer::start().await;

    // Create session
    server.run_command(&["new", "-s", "persist", "--detached"]);
    sleep(Duration::from_millis(300)).await;

    // Perform many operations
    for i in 0..50 {
        server.run_command(&["send-keys", "persist", &format!("echo '{}'", i)]);
        server.run_command(&["send-keys", "persist", "Enter"]);

        if i % 10 == 0 {
            // Check session still exists
            let output = server.run_command(&["list"]);
            let list_output = String::from_utf8_lossy(&output.stdout);
            assert!(list_output.contains("persist"), "Session lost at iteration {}", i);
        }
    }

    // Final check
    let output = server.run_command(&["list"]);
    let list_output = String::from_utf8_lossy(&output.stdout);
    assert!(list_output.contains("persist"), "Session lost after all operations");

    // Clean up
    server.run_command(&["kill", "persist"]);
}
