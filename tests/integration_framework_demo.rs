//! Integration test demonstrating the new testing framework
//!
//! This test showcases the helper utilities and patterns for writing
//! comprehensive integration tests.

mod integration {
    pub mod helpers;
}

use integration::helpers::{TestFixture, TestServer, TestClient, assert_session_exists, assert_session_not_exists};
use std::time::Duration;
use tokio::time::sleep;

#[tokio::test]
async fn test_basic_session_lifecycle() {
    // Setup isolated test environment
    let fixture = TestFixture::new();
    let mut server = TestServer::start_default(fixture.socket_path().clone()).await;

    let client = TestClient::new(fixture.socket_path().clone());

    // Create a session
    let output = client.new_session("test-session", true);
    assert!(output.status.success(), "Failed to create session");

    // Verify session exists
    let output = client.list_sessions();
    assert!(output.status.success(), "Failed to list sessions");

    let sessions = TestClient::parse_session_list(&output);
    assert_session_exists(&sessions, "test-session");

    // Send some keys
    let output = client.send_keys("test-session", "echo 'Hello from Ferrix'");
    assert!(output.status.success(), "Failed to send keys");

    sleep(Duration::from_millis(500)).await;

    // Kill session
    let output = client.kill_session("test-session");
    assert!(output.status.success(), "Failed to kill session");

    // Verify session is gone
    let output = client.list_sessions();
    let sessions = TestClient::parse_session_list(&output);
    assert_session_not_exists(&sessions, "test-session");

    // Server should still be running
    assert!(server.is_running(), "Server should be running after session lifecycle");
}

#[tokio::test]
async fn test_multiple_concurrent_sessions() {
    let fixture = TestFixture::new();
    let mut server = TestServer::start_default(fixture.socket_path().clone()).await;

    let client = TestClient::new(fixture.socket_path().clone());

    // Create multiple sessions
    for i in 0..5 {
        let name = format!("session-{}", i);
        let output = client.new_session(&name, true);
        assert!(output.status.success(), "Failed to create {}", name);
    }

    // List and verify
    let output = client.list_sessions();
    let sessions = TestClient::parse_session_list(&output);
    assert_eq!(sessions.len(), 5, "Should have 5 sessions");

    // Clean up
    for i in 0..5 {
        let name = format!("session-{}", i);
        client.kill_session(&name);
    }

    assert!(server.is_running());
}

#[tokio::test]
async fn test_session_persistence() {
    let fixture = TestFixture::new();
    let mut server = TestServer::start_default(fixture.socket_path().clone()).await;

    let client = TestClient::new(fixture.socket_path().clone());

    // Create session with output
    client.new_session("persistent", true);
    client.send_keys("persistent", "echo 'test data'");

    sleep(Duration::from_millis(500)).await;

    // Session should persist
    let output = client.list_sessions();
    let sessions = TestClient::parse_session_list(&output);
    assert_session_exists(&sessions, "persistent");

    // Clean up
    client.kill_session("persistent");
    assert!(server.is_running());
}
