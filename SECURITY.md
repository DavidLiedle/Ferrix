# Security Policy

## Supported Versions

Ferrix follows semantic versioning. Security updates are provided for the following versions:

| Version | Supported          |
| ------- | ------------------ |
| 1.x.x   | :white_check_mark: (when released) |
| 0.11.x  | :white_check_mark: |
| 0.10.x  | :white_check_mark: (until v1.0 release) |
| < 0.10  | :x:                |

## Security Features

Ferrix includes the following security features:

### Authentication & Authorization
- **Bcrypt Password Hashing** - Passwords hashed with bcrypt (DEFAULT_COST)
- **Rate Limiting** - 5 failed authentication attempts trigger 15-minute lockout
- **Role-Based Access Control** - Permission system for multi-user environments
- **Session Locking** - Read-only mode for secure session viewing

### Network Security
- **TLS 1.3 Support** - Secure remote connections using rustls
- **Certificate Validation** - Proper TLS certificate verification
- **Configurable TLS** - Optional TLS for local-only deployments

### Operational Security
- **Dependency Auditing** - Regular security audits with `cargo audit`
- **Safe Rust** - 100% safe Rust code (no unsafe blocks)
- **Input Validation** - Sanitized user input throughout the codebase
- **Secure Defaults** - Security-first default configuration

## Security Audits

Ferrix has undergone comprehensive security audits. For detailed information:

- [**SECURITY_AUDIT.md**](SECURITY_AUDIT.md) - Code security analysis and hardening measures
- [**DEPENDENCY_AUDIT.md**](DEPENDENCY_AUDIT.md) - Dependency security status and mitigation strategies
- [**docs/DEPLOYMENT.md**](docs/DEPLOYMENT.md) - Production deployment security best practices

**Last Audit**: 2025-10-05 (v0.11.0)
**Status**: ✅ All critical vulnerabilities addressed

## Reporting a Vulnerability

**We take security vulnerabilities seriously.** If you discover a security issue, please report it responsibly.

### How to Report

**DO NOT** create a public GitHub issue for security vulnerabilities.

Instead, please email security reports to:

**Email**: [david@davidcanhelp.me](mailto:david@davidcanhelp.me)

**Subject**: [SECURITY] Ferrix Vulnerability Report

### What to Include

Please include the following information in your report:

1. **Description** - Clear description of the vulnerability
2. **Impact** - What an attacker could achieve
3. **Steps to Reproduce** - Detailed reproduction steps
4. **Affected Versions** - Which versions are affected
5. **Suggested Fix** - If you have ideas for mitigation (optional)
6. **Proof of Concept** - Code or commands demonstrating the issue (if applicable)

### Response Timeline

We aim to respond to security reports according to the following timeline:

- **Initial Response**: Within 48 hours
- **Severity Assessment**: Within 7 days
- **Fix Development**: Depends on severity
  - **Critical**: Emergency patch within 7 days
  - **High**: Patch within 30 days
  - **Medium**: Patch in next minor release
  - **Low**: Addressed in roadmap planning

### Disclosure Policy

We follow **coordinated disclosure**:

1. You report the vulnerability privately
2. We acknowledge receipt and assess severity
3. We develop and test a fix
4. We release a security patch
5. We publish a security advisory
6. You may publicly disclose after patch release (we appreciate 7 days notice)

### Bug Bounty

Ferrix is currently an open-source project without a formal bug bounty program. However:

- We deeply appreciate security researchers' efforts
- We will publicly acknowledge your contribution (if desired)
- We may offer swag/stickers as tokens of appreciation
- Critical findings will be prominently credited in release notes

## Known Security Considerations

### Current Limitations (v1.0.0)

The following security enhancements are planned for future releases:

**P1 - Planned for v1.1.0:**
- Certificate/key file permission validation
- Comprehensive audit logging system
- Session idle timeouts
- mTLS (mutual TLS) configuration option

**P2 - Planned for v2.0.0:**
- Encrypted user database at rest
- Zeroizing for password handling in memory
- Default least-privilege permissions
- Advanced authorization action identifiers

See [V1_RELEASE_CHECKLIST.md](V1_RELEASE_CHECKLIST.md) for detailed planning.

### Dependency Security

Ferrix dependencies are regularly audited. See [DEPENDENCY_AUDIT.md](DEPENDENCY_AUDIT.md) for:
- Current security advisories status
- Mitigation strategies for known issues
- Upgrade roadmap for vulnerable dependencies

**Note**: The `battery` feature (system battery monitoring) is **optional** and disabled by default due to a dependency vulnerability. Do not enable this feature in security-sensitive environments until v1.1.0.

## Security Best Practices

For production deployments, please follow these security best practices:

### Local Deployments
- Run ferrix server as non-root user
- Set proper file permissions (0700 for directories, 0600 for files)
- Use systemd user services (not system services) when possible
- Isolate ferrix data directory (`~/.ferrix/`)

### Remote Access
- **Always use TLS** for remote connections
- Use strong certificates (Let's Encrypt or properly signed certs)
- Configure firewall rules (allow only necessary IPs)
- Enable authentication with strong passwords
- Consider VPN instead of exposing ferrix directly to internet

### Configuration
- Review default configuration before deployment
- Disable features you don't need
- Use strong authentication passwords
- Set appropriate session timeouts
- Enable audit logging (when available in v1.1.0)

See [docs/DEPLOYMENT.md](docs/DEPLOYMENT.md) for comprehensive deployment security guidance.

## Security Hall of Fame

We appreciate security researchers who help make Ferrix more secure:

<!-- Future security contributors will be listed here -->

*No security reports yet - be the first!*

---

## Additional Resources

- [GitHub Security Advisories](https://github.com/davidliedle/Ferrix/security/advisories)
- [Changelog](CHANGELOG.md) - Security fixes documented in release notes
- [Contributing Guidelines](CONTRIBUTING.md) - Contribution process including security reviews

## Questions?

If you have questions about Ferrix security that are **not** vulnerability reports:
- Open a [GitHub Discussion](https://github.com/davidliedle/Ferrix/discussions)
- Check [docs/DEPLOYMENT.md](docs/DEPLOYMENT.md) for deployment security guidance
- Read [SECURITY_AUDIT.md](SECURITY_AUDIT.md) for detailed security analysis

---

**Last Updated**: 2025-10-05
**Version**: 0.11.0
