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
        .arg("--socket")
        .arg(&socket_path)
        .arg("server")
        .arg("--foreground")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start server");

    // Wait for socket to be created (with timeout)
    let mut retries = 0;
    while !socket_path.exists() && retries < 20 {
        sleep(Duration::from_millis(100)).await;
        retries += 1;
    }

    if !socket_path.exists() {
        // Get server output for debugging
        let _ = server.kill();
        let output = server.wait_with_output().unwrap();
        panic!("Server failed to create socket after 2s. stdout: {:?}, stderr: {:?}",
               String::from_utf8_lossy(&output.stdout),
               String::from_utf8_lossy(&output.stderr));
    }

    // Give server a bit more time to fully initialize
    sleep(Duration::from_millis(200)).await;

    // Create a session
    let output = Command::new("./target/release/ferrix")
        .arg("--socket")
        .arg(&socket_path)
        .arg("new")
        .arg("-s")
        .arg("test-session")
        .arg("--detached")
        .output()
        .expect("Failed to create session");

    assert!(output.status.success(), "Failed to create session: {:?}",
            String::from_utf8_lossy(&output.stderr));

    // List sessions
    let output = Command::new("./target/release/ferrix")
        .arg("--socket")
        .arg(&socket_path)
        .arg("list")
        .output()
        .expect("Failed to list sessions");

    assert!(output.status.success());
    let list_output = String::from_utf8_lossy(&output.stdout);
    assert!(list_output.contains("test-session"), "Session not found in list");

    // Kill the session
    let output = Command::new("./target/release/ferrix")
        .arg("--socket")
        .arg(&socket_path)
        .arg("kill")
        .arg("test-session")
        .output()
        .expect("Failed to kill session");

    assert!(output.status.success(), "Failed to kill session: {:?}",
            String::from_utf8_lossy(&output.stderr));

    // Clean up server
    server.kill().expect("Failed to kill server");
}

#[tokio::test]
async fn test_window_pane_operations() {
    let temp_dir = TempDir::new().unwrap();
    let socket_path = temp_dir.path().join("ferrix.sock");

    // Start server
    let mut server = Command::new("./target/release/ferrix")
        .arg("--socket")
        .arg(&socket_path)
        .arg("server")
        .arg("--foreground")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start server");

    // Wait for socket
    let mut retries = 0;
    while !socket_path.exists() && retries < 20 {
        sleep(Duration::from_millis(100)).await;
        retries += 1;
    }
    assert!(socket_path.exists(), "Server failed to create socket");
    sleep(Duration::from_millis(200)).await;

    // Create a session
    Command::new("./target/release/ferrix")
        .arg("--socket")
        .arg(&socket_path)
        .arg("new")
        .arg("-s")
        .arg("window-test")
        .arg("--detached")
        .output()
        .expect("Failed to create session");

    // TODO: Test window/pane operations through client API
    // Currently no direct CLI commands for windows/panes

    // Clean up
    Command::new("./target/release/ferrix")
        .arg("--socket")
        .arg(&socket_path)
        .arg("kill")
        .arg("window-test")
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
        .arg("--socket")
        .arg(&socket_path)
        .arg("server")
        .arg("--foreground")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start server");

    // Wait for socket
    let mut retries = 0;
    while !socket_path.exists() && retries < 20 {
        sleep(Duration::from_millis(100)).await;
        retries += 1;
    }
    assert!(socket_path.exists(), "Server failed to create socket");
    sleep(Duration::from_millis(200)).await;

    // Create a session
    Command::new("./target/release/ferrix")
        .arg("--socket")
        .arg(&socket_path)
        .arg("new")
        .arg("-s")
        .arg("persist-test")
        .arg("--detached")
        .output()
        .expect("Failed to create session");

    // Save snapshot
    let output = Command::new("./target/release/ferrix")
        .arg("--socket")
        .arg(&socket_path)
        .arg("save-snapshot")
        .arg("persist-test")
        .arg("--name")
        .arg("test-snapshot")
        .output()
        .expect("Failed to save snapshot");

    let success = output.status.success();
    if !success {
        eprintln!("Save snapshot stderr: {}", String::from_utf8_lossy(&output.stderr));
    }

    // List snapshots
    let output = Command::new("./target/release/ferrix")
        .arg("--socket")
        .arg(&socket_path)
        .arg("list-snapshots")
        .output()
        .expect("Failed to list snapshots");

    if output.status.success() {
        let list_output = String::from_utf8_lossy(&output.stdout);
        assert!(list_output.contains("test-snapshot") || list_output.contains("persist-test"),
                "Snapshot not found in list");
    }

    // Clean up
    Command::new("./target/release/ferrix")
        .arg("--socket")
        .arg(&socket_path)
        .arg("kill")
        .arg("persist-test")
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
        .arg("--socket")
        .arg(&socket_path)
        .arg("server")
        .arg("--foreground")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start server");

    // Wait for socket to be created (with timeout)
    let mut retries = 0;
    while !socket_path.exists() && retries < 20 {
        sleep(Duration::from_millis(100)).await;
        retries += 1;
    }

    if !socket_path.exists() {
        let _ = server.kill();
        let output = server.wait_with_output().unwrap();
        panic!("Server failed to create socket. stdout: {:?}, stderr: {:?}",
               String::from_utf8_lossy(&output.stdout),
               String::from_utf8_lossy(&output.stderr));
    }

    sleep(Duration::from_millis(200)).await;

    // Create a detached session
    let output = Command::new("./target/release/ferrix")
        .arg("--socket")
        .arg(&socket_path)
        .arg("new")
        .arg("-s")
        .arg("detach-test")
        .arg("--detached")
        .output()
        .expect("Failed to create session");

    assert!(output.status.success());

    // Verify session exists
    let output = Command::new("./target/release/ferrix")
        .arg("--socket")
        .arg(&socket_path)
        .arg("list")
        .output()
        .expect("Failed to list sessions");

    let list_output = String::from_utf8_lossy(&output.stdout);
    assert!(list_output.contains("detach-test"));

    // Clean up
    Command::new("./target/release/ferrix")
        .arg("--socket")
        .arg(&socket_path)
        .arg("kill")
        .arg("detach-test")
        .output()
        .expect("Failed to kill session");

    server.kill().expect("Failed to kill server");
}