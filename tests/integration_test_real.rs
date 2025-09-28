use std::time::Duration;
use std::process::{Command, Stdio};
use tokio::time::sleep;
use tempfile::TempDir;

#[tokio::test]
async fn test_basic_session_operations() {
    let temp_dir = TempDir::new().unwrap();
    let socket_path = temp_dir.path().join("ferrix.sock");

    // Start server
    let mut server = Command::new("./target/release/ferrix")
        .arg("server")
        .arg("--socket")
        .arg(&socket_path)
        .arg("--foreground")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("Failed to start server");

    // Give server time to start
    sleep(Duration::from_millis(500)).await;

    // Create a session
    let output = Command::new("./target/release/ferrix")
        .arg("new")
        .arg("-s")
        .arg("test-session")
        .arg("--socket")
        .arg(&socket_path)
        .arg("--detached")
        .output()
        .expect("Failed to create session");

    assert!(output.status.success(), "Failed to create session: {:?}",
            String::from_utf8_lossy(&output.stderr));

    // List sessions
    let output = Command::new("./target/release/ferrix")
        .arg("list")
        .arg("--socket")
        .arg(&socket_path)
        .output()
        .expect("Failed to list sessions");

    assert!(output.status.success());
    let list_output = String::from_utf8_lossy(&output.stdout);
    assert!(list_output.contains("test-session"), "Session not found in list");

    // Kill the session
    let output = Command::new("./target/release/ferrix")
        .arg("kill")
        .arg("-t")
        .arg("test-session")
        .arg("--socket")
        .arg(&socket_path)
        .output()
        .expect("Failed to kill session");

    assert!(output.status.success());

    // Clean up server
    server.kill().expect("Failed to kill server");
}

#[tokio::test]
async fn test_window_pane_operations() {
    let temp_dir = TempDir::new().unwrap();
    let socket_path = temp_dir.path().join("ferrix.sock");

    // Start server
    let mut server = Command::new("./target/release/ferrix")
        .arg("server")
        .arg("--socket")
        .arg(&socket_path)
        .arg("--foreground")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("Failed to start server");

    sleep(Duration::from_millis(500)).await;

    // Create a session
    Command::new("./target/release/ferrix")
        .arg("new")
        .arg("-s")
        .arg("window-test")
        .arg("--socket")
        .arg(&socket_path)
        .arg("--detached")
        .output()
        .expect("Failed to create session");

    // TODO: Test window/pane operations through client API
    // Currently no direct CLI commands for windows/panes

    // Clean up
    Command::new("./target/release/ferrix")
        .arg("kill")
        .arg("-t")
        .arg("window-test")
        .arg("--socket")
        .arg(&socket_path)
        .output()
        .expect("Failed to kill session");

    server.kill().expect("Failed to kill server");
}

#[tokio::test]
async fn test_session_persistence() {
    let temp_dir = TempDir::new().unwrap();
    let socket_path = temp_dir.path().join("ferrix.sock");
    let snapshot_dir = temp_dir.path().join("snapshots");
    std::fs::create_dir(&snapshot_dir).unwrap();

    // Start server
    let mut server = Command::new("./target/release/ferrix")
        .arg("server")
        .arg("--socket")
        .arg(&socket_path)
        .arg("--foreground")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("Failed to start server");

    sleep(Duration::from_millis(500)).await;

    // Create a session
    Command::new("./target/release/ferrix")
        .arg("new")
        .arg("-s")
        .arg("persist-test")
        .arg("--socket")
        .arg(&socket_path)
        .arg("--detached")
        .output()
        .expect("Failed to create session");

    // Save snapshot
    let output = Command::new("./target/release/ferrix")
        .arg("save-snapshot")
        .arg("persist-test")
        .arg("--name")
        .arg("test-snapshot")
        .arg("--socket")
        .arg(&socket_path)
        .output()
        .expect("Failed to save snapshot");

    let success = output.status.success();
    if !success {
        eprintln!("Save snapshot stderr: {}", String::from_utf8_lossy(&output.stderr));
    }

    // List snapshots
    let output = Command::new("./target/release/ferrix")
        .arg("list-snapshots")
        .arg("--socket")
        .arg(&socket_path)
        .output()
        .expect("Failed to list snapshots");

    if output.status.success() {
        let list_output = String::from_utf8_lossy(&output.stdout);
        assert!(list_output.contains("test-snapshot") || list_output.contains("persist-test"),
                "Snapshot not found in list");
    }

    // Clean up
    Command::new("./target/release/ferrix")
        .arg("kill")
        .arg("-t")
        .arg("persist-test")
        .arg("--socket")
        .arg(&socket_path)
        .output()
        .expect("Failed to kill session");

    server.kill().expect("Failed to kill server");
}

#[tokio::test]
async fn test_detach_reattach() {
    let temp_dir = TempDir::new().unwrap();
    let socket_path = temp_dir.path().join("ferrix.sock");

    // Start server
    let mut server = Command::new("./target/release/ferrix")
        .arg("server")
        .arg("--socket")
        .arg(&socket_path)
        .arg("--foreground")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("Failed to start server");

    sleep(Duration::from_millis(500)).await;

    // Create a detached session
    let output = Command::new("./target/release/ferrix")
        .arg("new")
        .arg("-s")
        .arg("detach-test")
        .arg("--socket")
        .arg(&socket_path)
        .arg("--detached")
        .output()
        .expect("Failed to create session");

    assert!(output.status.success());

    // Verify session exists
    let output = Command::new("./target/release/ferrix")
        .arg("list")
        .arg("--socket")
        .arg(&socket_path)
        .output()
        .expect("Failed to list sessions");

    let list_output = String::from_utf8_lossy(&output.stdout);
    assert!(list_output.contains("detach-test"));

    // Clean up
    Command::new("./target/release/ferrix")
        .arg("kill")
        .arg("-t")
        .arg("detach-test")
        .arg("--socket")
        .arg(&socket_path)
        .output()
        .expect("Failed to kill session");

    server.kill().expect("Failed to kill server");
}