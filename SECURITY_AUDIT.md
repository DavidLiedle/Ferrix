# Security Audit - Ferrix v0.10.2+

**Date**: 2025-10-05
**Scope**: Remote/TLS features and authentication system
**Status**: Pre-v1.0 security review

## Executive Summary

This audit reviews the security posture of Ferrix's remote access and TLS features. Overall, the implementation demonstrates strong security fundamentals with bcrypt password hashing, proper TLS configuration, and authentication/authorization separation. However, several areas require attention before v1.0 release.

## Findings

### Critical Issues

**None identified** - No critical security vulnerabilities found.

### High Priority Issues

#### 1. TLS Client Authentication Not Enforced (src/server/remote.rs:131)
**Severity**: High
**Location**: `RemoteServer::with_tls()`

```rust
let config = ServerConfig::builder()
    .with_no_client_auth()  // ⚠️ No client certificate verification
    .with_single_cert(cert_chain, ...)
```

**Issue**: Server is configured with `with_no_client_auth()`, meaning TLS connections don't verify client certificates. This relies solely on password authentication, which is vulnerable to credential theft.

**Recommendation**:
- Implement mutual TLS (mTLS) with `with_client_cert_verifier()` for production deployments
- Provide configuration option to enable/disable client cert verification
- Document that password-only auth should use strong passwords and rate limiting

#### 2. No Rate Limiting on Authentication Attempts (src/server/remote.rs:186-203)
**Severity**: High
**Location**: `RemoteServer::handle_client()`

**Issue**: Authentication handler has no rate limiting or account lockout, making it vulnerable to brute force attacks.

**Recommendation**:
```rust
// Add to RemoteServer
struct RateLimiter {
    attempts: DashMap<SocketAddr, (u32, Instant)>,
    max_attempts: u32,
    lockout_duration: Duration,
}

// Before authentication:
if rate_limiter.is_locked(&peer_addr) {
    return Err(FerrixError::RateLimited);
}
```

#### 3. Authorization Check Uses Debug Formatting (src/server/remote.rs:233)
**Severity**: Medium-High
**Location**: `handle_client` authorization check

```rust
let action = format!("{:?}", client_msg);  // ⚠️ Unstable format
if !auth_handler.authorize(&client_id, &action).await.unwrap_or(false) {
```

**Issue**: Using `Debug` formatting for authorization checks is brittle and can break if enum representation changes. Also uses `.unwrap_or(false)` which silently allows access on authorization errors.

**Recommendation**:
```rust
// Define stable action identifiers
enum Action {
    CreateSession,
    KillSession,
    AttachSession,
    // ...
}

impl From<&ClientMessage> for Action {
    fn from(msg: &ClientMessage) -> Self {
        match msg {
            ClientMessage::CreateSession { .. } => Action::CreateSession,
            // ...
        }
    }
}

// In handler:
let action = Action::from(&client_msg);
match auth_handler.authorize(&client_id, &action).await {
    Ok(true) => { /* proceed */ }
    Ok(false) => return Err(FerrixError::Unauthorized),
    Err(e) => {
        error!("Authorization check failed: {}", e);
        return Err(FerrixError::AuthorizationError(e));
    }
}
```

### Medium Priority Issues

#### 4. Password Stored in Memory (src/auth/user_store.rs:195)
**Severity**: Medium
**Location**: `verify_password()`

**Issue**: Password is passed as `&str` and may remain in memory. While bcrypt is used for hashing, the plaintext password could be extracted from memory dumps.

**Recommendation**: Use a zeroing type for password handling:
```rust
use zeroize::Zeroize;

pub async fn verify_password(&self, username: &str, mut password: String) -> Result<ClientId> {
    let result = /* verification */;
    password.zeroize();
    result
}
```

#### 5. No Session Timeout for Remote Connections
**Severity**: Medium
**Location**: `RemoteServer::handle_client()`

**Issue**: Remote connections have no idle timeout, allowing indefinite sessions that could leak resources or maintain unauthorized access.

**Recommendation**: Add idle timeout tracking:
```rust
let mut last_activity = Instant::now();
const IDLE_TIMEOUT: Duration = Duration::from_secs(3600); // 1 hour

tokio::select! {
    Some(msg) = framed.next() => {
        last_activity = Instant::now();
        // ... handle message
    }
    _ = tokio::time::sleep_until(last_activity + IDLE_TIMEOUT) => {
        info!("Session timeout for client {}", client_id);
        break;
    }
}
```

#### 6. Certificate/Key File Permissions Not Checked
**Severity**: Medium
**Location**: `RemoteServer::with_tls()`

**Issue**: No validation that certificate and key files have secure permissions (should be 0600 for keys).

**Recommendation**:
```rust
use std::os::unix::fs::PermissionsExt;

let metadata = std::fs::metadata(key_path)?;
let permissions = metadata.permissions();
if permissions.mode() & 0o177 != 0 {
    return Err(FerrixError::InsecureKeyPermissions(
        "Private key file must have permissions 0600".to_string()
    ));
}
```

### Low Priority Issues

#### 7. Default "all" Permissions Too Permissive (src/auth/user_store.rs:135)
**Severity**: Low
**Location**: `add_user()`

```rust
permissions: vec!["all".to_string()], // Default permissions
```

**Issue**: New users get "all" permissions by default. Least privilege principle suggests starting with minimal permissions.

**Recommendation**: Default to read-only or specific session access, require explicit privilege escalation.

#### 8. User Database File Not Encrypted at Rest
**Severity**: Low
**Location**: `UserStore::save_users()`

**Issue**: `~/.ferrix/users.json` contains password hashes in plaintext JSON. While hashes are used, the file itself is not encrypted.

**Recommendation**:
- Document that the user database file should have restrictive permissions (0600)
- Consider encrypting the database file with a master key
- Add integrity checking (HMAC) to detect tampering

#### 9. No Audit Logging for Security Events
**Severity**: Low
**Location**: Authentication and authorization handlers

**Issue**: Limited logging of security-relevant events (failed auth, authorization denials, user additions/removals).

**Recommendation**: Add structured audit logging:
```rust
audit_log::record(AuditEvent::AuthenticationFailed {
    username,
    source_ip: peer_addr,
    timestamp: Utc::now(),
});
```

## Positive Security Features

1. **Bcrypt Password Hashing**: Uses bcrypt with `DEFAULT_COST` (currently 12), providing strong protection against rainbow table and brute force attacks.

2. **Separation of Authentication and Authorization**: Clean separation with `AuthenticationHandler` trait allowing flexible implementation.

3. **TLS Support**: Proper TLS 1.3 configuration using rustls with modern cipher suites.

4. **Async Design**: Non-blocking architecture reduces DoS attack surface.

5. **Certificate Validation**: Client properly validates server certificates (with optional CA pinning).

6. **Error Handling**: Generally good error handling without information leakage.

## Compliance Considerations

### For Production Deployment:

1. **Data Protection**:
   - User credentials stored with bcrypt (✓)
   - Session data encrypted in transit when TLS enabled (✓)
   - Session data not encrypted at rest (⚠️)

2. **Access Control**:
   - Authentication required for remote access (✓)
   - Authorization checks on actions (✓)
   - Role-based access control implemented (✓)
   - Audit trail incomplete (⚠️)

3. **Network Security**:
   - TLS 1.3 support (✓)
   - Strong cipher suites (✓)
   - Mutual TLS optional (⚠️ should be default)

## Recommendations Priority Matrix

| Priority | Issue | Effort | Impact |
|----------|-------|--------|--------|
| P0 | Add rate limiting | Medium | High |
| P0 | Implement mTLS option | Medium | High |
| P1 | Fix authorization check stability | Low | Medium |
| P1 | Add session timeouts | Low | Medium |
| P2 | Check key file permissions | Low | Low |
| P2 | Add audit logging | Medium | Low |
| P3 | Encrypt user database | High | Low |
| P3 | Use zeroizing for passwords | Low | Low |

## Testing Recommendations

1. **Penetration Testing**:
   - Brute force authentication attempts
   - Session hijacking attempts
   - Certificate validation bypass attempts
   - Man-in-the-middle attack simulations

2. **Fuzzing**:
   - Protocol message fuzzing with cargo-fuzz
   - TLS handshake fuzzing
   - Authentication input fuzzing

3. **Static Analysis**:
   - Run cargo-audit for known vulnerabilities
   - Use cargo-clippy with security lints
   - Consider cargo-geiger for unsafe code audit

## Conclusion

The Ferrix remote access implementation has a solid security foundation. The primary concerns are:

1. Lack of rate limiting (critical for production)
2. Optional mTLS should be encouraged/required
3. Authorization mechanism needs stabilization

Recommended path to v1.0:
1. Implement rate limiting (P0)
2. Add mTLS configuration option with documentation (P0)
3. Fix authorization check (P1)
4. Add security documentation and deployment guide
5. Schedule professional security audit

**Overall Security Posture**: Good foundation, needs hardening for production use.

---

**Auditor Notes**: This is an internal security review. For production deployment, especially in multi-tenant or internet-facing scenarios, a professional third-party security audit is strongly recommended.
