use std::process::{Command, Stdio};
use std::time::Duration;
use tokio::time::sleep;
use std::path::PathBuf;
use uuid::Uuid;

/// Helper to start a Ferrix server in the background
async fn start_test_server() -> (std::process::Child, PathBuf) {
    let socket_path = PathBuf::from(format!("/tmp/ferrix_e2e_{}.sock", Uuid::new_v4()));

    // Build the binary first
    Command::new("cargo")
        .args(["build", "--release", "--bin", "ferrix"])
        .output()
        .expect("Failed to build ferrix");

    // Start server
    let server = Command::new("target/release/ferrix")
        .args(["--socket", socket_path.to_str().unwrap(), "server", "--foreground"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("Failed to start server");

    // Wait for server to be ready
    sleep(Duration::from_millis(1000)).await;

    (server, socket_path)
}

#[tokio::test]
async fn test_e2e_session_workflow() {
    let (mut server, socket_path) = start_test_server().await;

    // Create a new session
    let output = Command::new("target/release/ferrix")
        .args([
            "--socket", socket_path.to_str().unwrap(),
            "new",
            "-s", "e2e_test",
            "--detached"
        ])
        .output()
        .expect("Failed to create session");

    assert!(output.status.success(), "Failed to create session: {:?}",
            String::from_utf8_lossy(&output.stderr));

    // List sessions
    let output = Command::new("target/release/ferrix")
        .args([
            "--socket", socket_path.to_str().unwrap(),
            "list"
        ])
        .output()
        .expect("Failed to list sessions");

    let list_output = String::from_utf8_lossy(&output.stdout);
    assert!(list_output.contains("e2e_test"));

    // Kill session
    let output = Command::new("target/release/ferrix")
        .args([
            "--socket", socket_path.to_str().unwrap(),
            "kill",
            "e2e_test"
        ])
        .output()
        .expect("Failed to kill session");

    assert!(output.status.success(), "Failed to kill session: {:?}",
            String::from_utf8_lossy(&output.stderr));

    // Cleanup
    server.kill().ok();
}

#[tokio::test]
async fn test_e2e_attach_detach() {
    let (mut server, socket_path) = start_test_server().await;

    // Create session
    Command::new("target/release/ferrix")
        .args([
            "--socket", socket_path.to_str().unwrap(),
            "new",
            "-s", "attach_test",
            "--detached"
        ])
        .output()
        .expect("Failed to create session");

    // Attach to session (would need PTY handling for full test)
    // This is a simplified version
    let mut attach = Command::new("target/release/ferrix")
        .args([
            "--socket", socket_path.to_str().unwrap(),
            "attach",
            "attach_test"
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("Failed to attach");

    // Wait a bit
    sleep(Duration::from_millis(500)).await;

    // Kill the attach process (simulating detach)
    attach.kill().ok();

    // Session should still exist
    let output = Command::new("target/release/ferrix")
        .args([
            "--socket", socket_path.to_str().unwrap(),
            "list"
        ])
        .output()
        .expect("Failed to list sessions");

    let list_output = String::from_utf8_lossy(&output.stdout);
    assert!(list_output.contains("attach_test"));

    // Cleanup
    server.kill().ok();
}

#[tokio::test]
async fn test_e2e_multiple_sessions() {
    let (mut server, socket_path) = start_test_server().await;

    // Create multiple sessions
    for i in 0..3 {
        let output = Command::new("target/release/ferrix")
            .args([
                "--socket", socket_path.to_str().unwrap(),
                "new",
                "-s", &format!("session_{}", i),
                "--detached"
            ])
            .output()
            .expect("Failed to create session");

        assert!(output.status.success());
    }

    // List should show all sessions
    let output = Command::new("target/release/ferrix")
        .args([
            "--socket", socket_path.to_str().unwrap(),
            "list"
        ])
        .output()
        .expect("Failed to list sessions");

    let list_output = String::from_utf8_lossy(&output.stdout);
    assert!(list_output.contains("session_0"));
    assert!(list_output.contains("session_1"));
    assert!(list_output.contains("session_2"));

    // Kill all sessions
    for i in 0..3 {
        Command::new("target/release/ferrix")
            .args([
                "--socket", socket_path.to_str().unwrap(),
                "kill",
                "-t",
                &format!("session_{}", i)
            ])
            .output()
            .ok();
    }

    // Cleanup
    server.kill().ok();
}

#[tokio::test]
#[ignore] // Recording CLI commands not yet fully implemented
async fn test_e2e_recording() {
    let (mut server, socket_path) = start_test_server().await;
    let recording_file = format!("/tmp/ferrix_rec_{}.rec", Uuid::new_v4());

    // Create session
    Command::new("target/release/ferrix")
        .args([
            "--socket", socket_path.to_str().unwrap(),
            "new",
            "-s", "rec_test",
            "--detached"
        ])
        .output()
        .expect("Failed to create session");

    // Start recording
    let output = Command::new("target/release/ferrix")
        .args([
            "--socket", socket_path.to_str().unwrap(),
            "start-recording",
            "rec_test",
            "--output", &recording_file
        ])
        .output()
        .expect("Failed to start recording");

    assert!(output.status.success());

    // Wait a bit for some activity
    sleep(Duration::from_secs(1)).await;

    // Stop recording
    let output = Command::new("target/release/ferrix")
        .args([
            "--socket", socket_path.to_str().unwrap(),
            "stop-recording",
            "rec_test"
        ])
        .output()
        .expect("Failed to stop recording");

    assert!(output.status.success());

    // Check that recording file was created
    assert!(std::path::Path::new(&recording_file).exists());

    // Cleanup
    std::fs::remove_file(&recording_file).ok();
    server.kill().ok();
}

#[tokio::test]
async fn test_e2e_snapshot() {
    let (mut server, socket_path) = start_test_server().await;

    // Create session
    Command::new("target/release/ferrix")
        .args([
            "--socket", socket_path.to_str().unwrap(),
            "new",
            "-s", "snap_test",
            "--detached"
        ])
        .output()
        .expect("Failed to create session");

    // Save snapshot
    let output = Command::new("target/release/ferrix")
        .args([
            "--socket", socket_path.to_str().unwrap(),
            "save-snapshot",
            "snap_test",
            "--name", "test_snapshot"
        ])
        .output()
        .expect("Failed to save snapshot");

    assert!(output.status.success());

    // List snapshots
    let output = Command::new("target/release/ferrix")
        .args([
            "--socket", socket_path.to_str().unwrap(),
            "list-snapshots"
        ])
        .output()
        .expect("Failed to list snapshots");

    let list_output = String::from_utf8_lossy(&output.stdout);
    assert!(list_output.contains("test_snapshot"));

    // Cleanup
    server.kill().ok();
}

/// Test that server handles client disconnection gracefully
#[tokio::test]
async fn test_e2e_client_crash_recovery() {
    let (mut server, socket_path) = start_test_server().await;

    // Create session
    Command::new("target/release/ferrix")
        .args([
            "--socket", socket_path.to_str().unwrap(),
            "new",
            "-s", "crash_test",
            "--detached"
        ])
        .output()
        .expect("Failed to create session");

    // Start an attach that we'll kill
    let mut attach = Command::new("target/release/ferrix")
        .args([
            "--socket", socket_path.to_str().unwrap(),
            "attach",
            "-t",
            "crash_test"
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("Failed to attach");

    sleep(Duration::from_millis(500)).await;

    // Kill client abruptly (simulate crash)
    attach.kill().expect("Failed to kill client");

    // Server should still be running
    sleep(Duration::from_millis(500)).await;

    // Should be able to list sessions
    let output = Command::new("target/release/ferrix")
        .args([
            "--socket", socket_path.to_str().unwrap(),
            "list"
        ])
        .output()
        .expect("Failed to list sessions");

    assert!(output.status.success());
    let list_output = String::from_utf8_lossy(&output.stdout);
    assert!(list_output.contains("crash_test"));

    // Should be able to reattach
    let reattach = Command::new("target/release/ferrix")
        .args([
            "--socket", socket_path.to_str().unwrap(),
            "attach",
            "-t",
            "crash_test"
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn();

    assert!(reattach.is_ok());
    reattach.unwrap().kill().ok();

    // Cleanup
    server.kill().ok();
}

/// Test performance with large output
#[tokio::test]
async fn test_e2e_large_output_performance() {
    use std::time::Instant;

    let (mut server, socket_path) = start_test_server().await;

    // Create session
    Command::new("target/release/ferrix")
        .args([
            "--socket", socket_path.to_str().unwrap(),
            "new",
            "-s", "perf_test",
            "--detached"
        ])
        .output()
        .expect("Failed to create session");

    // Measure time to handle large output
    let start = Instant::now();

    // Send command that generates large output
    let output = Command::new("target/release/ferrix")
        .args([
            "--socket", socket_path.to_str().unwrap(),
            "send-keys",
            "-t",
            "perf_test",
            "seq 1 100000"
        ])
        .output()
        .expect("Failed to send command");

    let duration = start.elapsed();

    // Should complete in reasonable time (< 5 seconds for 100k lines)
    assert!(duration < Duration::from_secs(5),
            "Large output took too long: {:?}", duration);

    // Cleanup
    server.kill().ok();
}