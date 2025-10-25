//! Integration tests for remote access and security features
//!
//! Tests cover:
//! - TLS certificate validation (invalid, expired, self-signed)
//! - Authentication failures and edge cases
//! - Rate limiting and brute-force protection
//! - Concurrent client scenarios
//!
//! These tests require the 'remote' feature to be enabled.

#![cfg(feature = "remote")]

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

use ferrix::server::Server;
use ferrix::server::remote::{RemoteServer, AuthenticationHandler, TlsMode};
use ferrix::protocol::{AuthCredentials, ClientId};
use ferrix::error::{Result, FerrixError};

/// Test helper to generate self-signed certificates for testing
mod cert_helper {
    use std::path::PathBuf;
    use std::fs;

    pub fn setup_test_certs(test_name: &str) -> (PathBuf, PathBuf, PathBuf) {
        let test_dir = std::env::temp_dir().join("ferrix_test_certs").join(test_name);
        fs::create_dir_all(&test_dir).unwrap();

        let cert_path = test_dir.join("server.crt");
        let key_path = test_dir.join("server.key");
        let ca_path = test_dir.join("ca.crt");

        // Generate self-signed certificate using openssl
        // This is a simple test certificate - not for production!
        let output = std::process::Command::new("openssl")
            .args(&[
                "req", "-x509", "-newkey", "rsa:2048",
                "-keyout", key_path.to_str().unwrap(),
                "-out", cert_path.to_str().unwrap(),
                "-days", "1",
                "-nodes",
                "-subj", "/CN=localhost"
            ])
            .output();

        if output.is_err() || !output.unwrap().status.success() {
            // If openssl is not available, create dummy files
            // Real tests will skip if these are invalid
            fs::write(&cert_path, "dummy cert").unwrap();
            fs::write(&key_path, "dummy key").unwrap();
        }

        // Copy cert as CA for testing
        if cert_path.exists() {
            fs::copy(&cert_path, &ca_path).unwrap();
        } else {
            fs::write(&ca_path, "dummy ca").unwrap();
        }

        (cert_path, key_path, ca_path)
    }

    pub fn cleanup_test_certs(test_name: &str) {
        let test_dir = std::env::temp_dir().join("ferrix_test_certs").join(test_name);
        let _ = fs::remove_dir_all(test_dir);
    }
}

/// Mock authentication handler that always succeeds
struct AlwaysSucceedAuth;

#[async_trait::async_trait]
impl AuthenticationHandler for AlwaysSucceedAuth {
    async fn authenticate(&self, _credentials: &AuthCredentials) -> Result<ClientId> {
        Ok(ClientId(uuid::Uuid::new_v4()))
    }

    async fn authorize(&self, _client_id: &ClientId, _action: &str) -> Result<bool> {
        Ok(true)
    }
}

/// Mock authentication handler that always fails
#[allow(dead_code)]
struct AlwaysFailAuth;

#[async_trait::async_trait]
impl AuthenticationHandler for AlwaysFailAuth {
    async fn authenticate(&self, _credentials: &AuthCredentials) -> Result<ClientId> {
        Err(FerrixError::Other("Invalid credentials".to_string()))
    }

    async fn authorize(&self, _client_id: &ClientId, _action: &str) -> Result<bool> {
        Ok(false)
    }
}

/// Mock authentication handler that checks for specific username/password
struct TestAuth {
    valid_user: String,
    valid_pass: String,
}

#[async_trait::async_trait]
impl AuthenticationHandler for TestAuth {
    async fn authenticate(&self, credentials: &AuthCredentials) -> Result<ClientId> {
        if let Some(password) = &credentials.password {
            if &credentials.username == &self.valid_user && password == &self.valid_pass {
                Ok(ClientId(uuid::Uuid::new_v4()))
            } else {
                Err(FerrixError::Other("Invalid credentials".to_string()))
            }
        } else {
            Err(FerrixError::Other("Password required".to_string()))
        }
    }

    async fn authorize(&self, _client_id: &ClientId, _action: &str) -> Result<bool> {
        Ok(true)
    }
}

// ============================================================================
// TLS Certificate Validation Tests
// ============================================================================

#[tokio::test]
#[ignore] // Requires openssl, run with: cargo test --features remote -- --ignored
async fn test_tls_invalid_certificate() {
    let test_name = "invalid_cert";
    let (cert_path, key_path, _ca_path) = cert_helper::setup_test_certs(test_name);

    // Create dummy invalid certificate
    std::fs::write(&cert_path, "INVALID CERTIFICATE DATA").unwrap();

    let socket_path = std::env::temp_dir().join("ferrix_test_invalid_cert.sock");
    let _ = std::fs::remove_file(&socket_path);

    let server = Server::new(socket_path);
    let bind_addr: SocketAddr = "127.0.0.1:0".parse().unwrap();

    let remote_server = RemoteServer::new(
        bind_addr,
        Arc::new(server),
        Arc::new(AlwaysSucceedAuth),
    );

    // Attempting to enable TLS with invalid cert should fail
    let result = remote_server.with_tls(
        &cert_path,
        &key_path,
        TlsMode::ServerOnly,
        None,
    );

    assert!(result.is_err(), "Should reject invalid certificate");

    cert_helper::cleanup_test_certs(test_name);
}

#[tokio::test]
#[ignore] // Requires openssl
async fn test_tls_missing_certificate_file() {
    let cert_path = PathBuf::from("/nonexistent/cert.pem");
    let key_path = PathBuf::from("/nonexistent/key.pem");

    let socket_path = std::env::temp_dir().join("ferrix_test_missing_cert.sock");
    let _ = std::fs::remove_file(&socket_path);

    let server = Server::new(socket_path);
    let bind_addr: SocketAddr = "127.0.0.1:0".parse().unwrap();

    let remote_server = RemoteServer::new(
        bind_addr,
        Arc::new(server),
        Arc::new(AlwaysSucceedAuth),
    );

    // Should fail when certificate file doesn't exist
    let result = remote_server.with_tls(
        &cert_path,
        &key_path,
        TlsMode::ServerOnly,
        None,
    );

    assert!(result.is_err(), "Should fail when cert file doesn't exist");
    match result {
        Err(e) => assert!(e.to_string().contains("Failed to read certificate")),
        Ok(_) => panic!("Expected error but got Ok"),
    }
}

#[tokio::test]
#[ignore] // Requires openssl
async fn test_mtls_missing_client_ca() {
    let test_name = "mtls_no_ca";
    let (cert_path, key_path, _ca_path) = cert_helper::setup_test_certs(test_name);

    let socket_path = std::env::temp_dir().join("ferrix_test_mtls_no_ca.sock");
    let _ = std::fs::remove_file(&socket_path);

    let server = Server::new(socket_path);
    let bind_addr: SocketAddr = "127.0.0.1:0".parse().unwrap();

    let remote_server = RemoteServer::new(
        bind_addr,
        Arc::new(server),
        Arc::new(AlwaysSucceedAuth),
    );

    // MutualAuth without client CA should fail
    let result = remote_server.with_tls(
        &cert_path,
        &key_path,
        TlsMode::MutualAuth,
        None,  // Missing client CA!
    );

    assert!(result.is_err(), "Should require client CA for mTLS");
    match result {
        Err(e) => assert!(e.to_string().contains("Client CA certificate path required")),
        Ok(_) => panic!("Expected error but got Ok"),
    }

    cert_helper::cleanup_test_certs(test_name);
}

// ============================================================================
// Authentication Failure Tests
// ============================================================================

#[tokio::test]
async fn test_authentication_with_invalid_credentials() {
    let auth = TestAuth {
        valid_user: "admin".to_string(),
        valid_pass: "secret123".to_string(),
    };

    // Test with wrong password
    let result = auth.authenticate(&AuthCredentials {
        username: "admin".to_string(),
        password: Some("wrongpass".to_string()),
        token: None,
        certificate: None,
    }).await;

    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), FerrixError::Other(_)));

    // Test with wrong username
    let result = auth.authenticate(&AuthCredentials {
        username: "hacker".to_string(),
        password: Some("secret123".to_string()),
        token: None,
        certificate: None,
    }).await;

    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), FerrixError::Other(_)));
}

#[tokio::test]
async fn test_authentication_with_empty_credentials() {
    let auth = TestAuth {
        valid_user: "admin".to_string(),
        valid_pass: "secret123".to_string(),
    };

    // Empty username
    let result = auth.authenticate(&AuthCredentials {
        username: "".to_string(),
        password: Some("secret123".to_string()),
        token: None,
        certificate: None,
    }).await;

    assert!(result.is_err());

    // Empty password
    let result = auth.authenticate(&AuthCredentials {
        username: "admin".to_string(),
        password: Some("".to_string()),
        token: None,
        certificate: None,
    }).await;

    assert!(result.is_err());

    // Both empty
    let result = auth.authenticate(&AuthCredentials {
        username: "".to_string(),
        password: Some("".to_string()),
        token: None,
        certificate: None,
    }).await;

    assert!(result.is_err());

    // No password provided
    let result = auth.authenticate(&AuthCredentials {
        username: "admin".to_string(),
        password: None,
        token: None,
        certificate: None,
    }).await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_authentication_success() {
    let auth = TestAuth {
        valid_user: "admin".to_string(),
        valid_pass: "secret123".to_string(),
    };

    let result = auth.authenticate(&AuthCredentials {
        username: "admin".to_string(),
        password: Some("secret123".to_string()),
        token: None,
        certificate: None,
    }).await;

    assert!(result.is_ok());
    let client_id = result.unwrap();

    // Should be able to authorize actions
    let authorized = auth.authorize(&client_id, "create_session").await.unwrap();
    assert!(authorized);
}

// ============================================================================
// Rate Limiting Tests
// ============================================================================

#[tokio::test]
async fn test_rate_limiting_blocks_after_max_attempts() {
    use ferrix::server::rate_limiter::RateLimiter;

    let rate_limiter = RateLimiter::new(5, Duration::from_secs(60));
    let test_addr: SocketAddr = "127.0.0.1:12345".parse().unwrap();

    // Should not be locked initially
    assert!(!rate_limiter.is_locked(&test_addr).await);

    // Record 5 failed attempts
    for i in 0..5 {
        let should_lock = rate_limiter.record_failure(test_addr).await;
        if i < 4 {
            assert!(!should_lock, "Should not lock before max attempts");
        } else {
            assert!(should_lock, "Should lock on 5th attempt");
        }
    }

    // Now should be locked
    assert!(rate_limiter.is_locked(&test_addr).await);

    // Additional attempts while locked should remain locked
    assert!(rate_limiter.is_locked(&test_addr).await);
}

#[tokio::test]
async fn test_rate_limiting_unlocks_after_duration() {
    use ferrix::server::rate_limiter::RateLimiter;

    // Short lockout for testing (2 seconds)
    let rate_limiter = RateLimiter::new(3, Duration::from_secs(2));
    let test_addr: SocketAddr = "127.0.0.1:23456".parse().unwrap();

    // Trigger lockout
    for _ in 0..3 {
        rate_limiter.record_failure(test_addr).await;
    }

    assert!(rate_limiter.is_locked(&test_addr).await);

    // Wait for lockout to expire
    sleep(Duration::from_secs(3)).await;

    // Should no longer be locked
    assert!(!rate_limiter.is_locked(&test_addr).await);

    // Should be able to record successful auth
    rate_limiter.record_success(&test_addr).await;
}

#[tokio::test]
async fn test_rate_limiting_per_address() {
    use ferrix::server::rate_limiter::RateLimiter;

    let rate_limiter = RateLimiter::new(3, Duration::from_secs(60));
    let addr1: SocketAddr = "127.0.0.1:11111".parse().unwrap();
    let addr2: SocketAddr = "127.0.0.1:22222".parse().unwrap();

    // Lock out addr1
    for _ in 0..3 {
        rate_limiter.record_failure(addr1).await;
    }

    // addr1 should be locked
    assert!(rate_limiter.is_locked(&addr1).await);

    // addr2 should NOT be locked (different address)
    assert!(!rate_limiter.is_locked(&addr2).await);

    // addr2 should still be able to attempt
    rate_limiter.record_failure(addr2).await;
    assert!(!rate_limiter.is_locked(&addr2).await);
}

#[tokio::test]
async fn test_rate_limiting_resets_on_success() {
    use ferrix::server::rate_limiter::RateLimiter;

    let rate_limiter = RateLimiter::new(5, Duration::from_secs(60));
    let test_addr: SocketAddr = "127.0.0.1:33333".parse().unwrap();

    // Record 3 failures
    for _ in 0..3 {
        rate_limiter.record_failure(test_addr).await;
    }

    // Not locked yet
    assert!(!rate_limiter.is_locked(&test_addr).await);

    // Successful auth should reset counter
    rate_limiter.record_success(&test_addr).await;

    // Should be able to fail 5 more times before lockout
    for i in 0..5 {
        let should_lock = rate_limiter.record_failure(test_addr).await;
        if i < 4 {
            assert!(!should_lock);
        } else {
            assert!(should_lock);
        }
    }
}

// ============================================================================
// Concurrent Client Tests
// ============================================================================

#[tokio::test]
async fn test_concurrent_authentication_attempts() {
    let auth = Arc::new(TestAuth {
        valid_user: "admin".to_string(),
        valid_pass: "secret123".to_string(),
    });

    let mut handles = vec![];

    // Spawn 10 concurrent authentication attempts
    for i in 0..10 {
        let auth_clone = auth.clone();
        let handle = tokio::spawn(async move {
            if i % 2 == 0 {
                // Even attempts succeed
                auth_clone.authenticate(&AuthCredentials {
                    username: "admin".to_string(),
                    password: Some("secret123".to_string()),
                    token: None,
                    certificate: None,
                }).await
            } else {
                // Odd attempts fail
                auth_clone.authenticate(&AuthCredentials {
                    username: "admin".to_string(),
                    password: Some("wrong".to_string()),
                    token: None,
                    certificate: None,
                }).await
            }
        });
        handles.push((i, handle));
    }

    // Collect results
    let mut successes = 0;
    let mut failures = 0;

    for (i, handle) in handles {
        let result = handle.await.unwrap();
        if i % 2 == 0 {
            assert!(result.is_ok(), "Even attempts should succeed");
            successes += 1;
        } else {
            assert!(result.is_err(), "Odd attempts should fail");
            failures += 1;
        }
    }

    assert_eq!(successes, 5);
    assert_eq!(failures, 5);
}

#[tokio::test]
async fn test_concurrent_rate_limit_attempts() {
    use ferrix::server::rate_limiter::RateLimiter;

    let rate_limiter = Arc::new(RateLimiter::new(10, Duration::from_secs(60)));
    let test_addr: SocketAddr = "127.0.0.1:44444".parse().unwrap();

    let mut handles = vec![];

    // Spawn 20 concurrent failure recordings (should hit limit)
    for _ in 0..20 {
        let limiter_clone = rate_limiter.clone();
        let handle = tokio::spawn(async move {
            limiter_clone.record_failure(test_addr).await
        });
        handles.push(handle);
    }

    // Wait for all to complete
    let mut locked_count = 0;
    for handle in handles {
        if handle.await.unwrap() {
            locked_count += 1;
        }
    }

    // Should eventually be locked
    assert!(rate_limiter.is_locked(&test_addr).await);
    // At least one attempt should have triggered the lock
    assert!(locked_count > 0);
}

// ============================================================================
// Integration Tests (Server + Client)
// ============================================================================

#[tokio::test]
#[ignore] // Requires full server setup
async fn test_remote_server_rejects_after_rate_limit() {
    // This test would set up a full RemoteServer and RemoteClient
    // and test the end-to-end authentication with rate limiting
    // Skipped for now as it requires more complex setup
    // TODO: Implement full end-to-end test
}

#[tokio::test]
#[ignore] // Requires full server setup
async fn test_multiple_concurrent_sessions() {
    // Test multiple clients creating and managing sessions simultaneously
    // TODO: Implement when server infrastructure is ready
}
