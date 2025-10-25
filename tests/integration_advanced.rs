//! Advanced integration tests for complex workflows
//!
//! Tests multi-client scenarios, snapshots, copy mode, and keybindings

mod integration {
    pub mod helpers;
}

use integration::helpers::{TestFixture, TestServer, TestClient, assert_session_exists};
use std::time::Duration;
use tokio::time::sleep;

#[tokio::test]
async fn test_multi_client_attach_detach() {
    let fixture = TestFixture::new();
    let mut server = TestServer::start_default(fixture.socket_path().clone()).await;

    let client1 = TestClient::new(fixture.socket_path().clone());
    let client2 = TestClient::new(fixture.socket_path().clone());

    // Client 1 creates a session
    let output = client1.new_session("shared-session", true);
    assert!(output.status.success(), "Client 1 failed to create session");

    sleep(Duration::from_millis(300)).await;

    // Verify session exists
    let output = client1.list_sessions();
    let sessions = TestClient::parse_session_list(&output);
    assert_session_exists(&sessions, "shared-session");

    // Client 2 should be able to see the same session
    let output = client2.list_sessions();
    let sessions = TestClient::parse_session_list(&output);
    assert_session_exists(&sessions, "shared-session");
    assert_eq!(sessions.len(), 1, "Should see exactly one session");

    // Both clients can send keys to the same session
    let output = client1.send_keys("shared-session", "echo 'from client 1'");
    assert!(output.status.success(), "Client 1 failed to send keys");

    let output = client2.send_keys("shared-session", "echo 'from client 2'");
    assert!(output.status.success(), "Client 2 failed to send keys");

    sleep(Duration::from_millis(300)).await;

    // Clean up
    client1.kill_session("shared-session");

    assert!(server.is_running());
}

#[tokio::test]
async fn test_snapshot_save_restore_workflow() {
    let fixture = TestFixture::new();
    let mut server = TestServer::start_default(fixture.socket_path().clone()).await;

    let client = TestClient::new(fixture.socket_path().clone());

    // Create a session with some state
    client.new_session("snapshot-test", true);
    client.send_keys("snapshot-test", "echo 'initial state'");

    sleep(Duration::from_millis(500)).await;

    // Save a snapshot
    let output = client.save_snapshot("snapshot-test", "test-snapshot", "Test snapshot description");
    assert!(output.status.success(), "Failed to save snapshot: {:?}", String::from_utf8_lossy(&output.stderr));

    sleep(Duration::from_millis(300)).await;

    // List snapshots to verify it was saved
    let output = client.list_snapshots();
    assert!(output.status.success(), "Failed to list snapshots");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("test-snapshot"), "Snapshot not found in list");

    // Kill the original session
    client.kill_session("snapshot-test");

    sleep(Duration::from_millis(300)).await;

    // Verify session is gone
    let output = client.list_sessions();
    let sessions = TestClient::parse_session_list(&output);
    assert_eq!(sessions.len(), 0, "Session should be deleted");

    // Restore snapshot as new session (this would need load-snapshot implementation)
    // For now, we just verify the snapshot exists
    let output = client.list_snapshots();
    assert!(output.status.success());

    assert!(server.is_running());
}

#[tokio::test]
async fn test_copy_mode_workflow() {
    let fixture = TestFixture::new();
    let mut server = TestServer::start_default(fixture.socket_path().clone()).await;

    let client = TestClient::new(fixture.socket_path().clone());

    // Create session
    client.new_session("copy-test", true);

    sleep(Duration::from_millis(300)).await;

    // Send some text to create scrollback
    for i in 0..10 {
        client.send_keys("copy-test", &format!("echo 'Line {}'", i));
        sleep(Duration::from_millis(50)).await;
    }

    sleep(Duration::from_millis(500)).await;

    // Enter copy mode
    let output = client.enter_copy_mode();
    assert!(output.status.success(), "Failed to enter copy mode");

    sleep(Duration::from_millis(200)).await;

    // Exit copy mode
    let output = client.exit_copy_mode();
    assert!(output.status.success(), "Failed to exit copy mode");

    // Clean up
    client.kill_session("copy-test");

    assert!(server.is_running());
}

#[tokio::test]
async fn test_keybinding_management() {
    let fixture = TestFixture::new();
    let mut server = TestServer::start_default(fixture.socket_path().clone()).await;

    let client = TestClient::new(fixture.socket_path().clone());

    // List default keybindings
    let output = client.list_keys();
    assert!(output.status.success(), "Failed to list keys");

    // Bind a custom key
    let output = client.bind_key("ctrl-x", "split-pane");
    assert!(output.status.success(), "Failed to bind key");

    sleep(Duration::from_millis(200)).await;

    // Verify binding appears in list
    let output = client.list_keys();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("ctrl-x") || stdout.contains("custom"),
            "Custom keybinding not found in list");

    // Unbind the key
    let output = client.unbind_key("ctrl-x");
    assert!(output.status.success(), "Failed to unbind key");

    sleep(Duration::from_millis(200)).await;

    // Reset to defaults
    let output = client.reset_keys();
    assert!(output.status.success(), "Failed to reset keys");

    assert!(server.is_running());
}

#[tokio::test]
async fn test_window_management() {
    let fixture = TestFixture::new();
    let mut server = TestServer::start_default(fixture.socket_path().clone()).await;

    let client = TestClient::new(fixture.socket_path().clone());

    // Create a session
    client.new_session("window-test", true);

    sleep(Duration::from_millis(300)).await;

    // Create additional windows
    let output = client.new_window("window-1");
    assert!(output.status.success(), "Failed to create window-1");

    let output = client.new_window("window-2");
    assert!(output.status.success(), "Failed to create window-2");

    sleep(Duration::from_millis(300)).await;

    // List windows
    let output = client.list_windows();
    assert!(output.status.success(), "Failed to list windows");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("window-1"), "window-1 not found");
    assert!(stdout.contains("window-2"), "window-2 not found");

    // Rename a window
    let output = client.rename_window("window-renamed");
    assert!(output.status.success(), "Failed to rename window");

    sleep(Duration::from_millis(200)).await;

    // Verify rename
    let output = client.list_windows();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("window-renamed"), "Renamed window not found");

    // Clean up
    client.kill_session("window-test");

    assert!(server.is_running());
}
