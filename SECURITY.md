# Security Policy

## Supported Versions

| Version | Supported |
|---------|-----------|
| 0.x.x   | Yes       |

## Reporting a Vulnerability

**Please DO NOT file public GitHub issues for security vulnerabilities.**

### Preferred Method: GitHub Security Advisories

Report security vulnerabilities via GitHub Security Advisories:
https://github.com/como-technologies/panoptes/security/advisories/new

This allows us to discuss and fix the issue privately before public disclosure.

### Alternative: Email

If you cannot use GitHub Security Advisories, contact us at:
- Email: security@como-technologies.io

When reporting, please include:
- Description of the vulnerability
- Steps to reproduce
- Affected versions
- Potential impact assessment
- Any suggested fixes (optional)

## Response Timeline

We commit to the following response times based on severity:

| Severity | Initial Response | Fix Target |
|----------|-----------------|------------|
| Critical | 24 hours | 48-72 hours |
| High | 72 hours | 1 week |
| Medium | 1 week | 2 weeks |
| Low | 2 weeks | Next release |

### Severity Classification

- **Critical**: Remote code execution, container escape, authentication bypass, data exfiltration
- **High**: Privilege escalation, unauthorized data access, monitoring bypass
- **Medium**: Denial of service, information disclosure, configuration weaknesses
- **Low**: Minor issues, hardening recommendations, documentation gaps

## Security Practices

Panoptes follows security best practices:

### Rust Security
- Memory-safe implementation using Rust's ownership system
- `cargo-audit` for vulnerability scanning in CI
- `cargo-deny` for license and supply chain security
- Compile-time overflow checks enabled in release builds
- Minimal `unsafe` code, documented with safety invariants

### Container Security
- Minimal base images (distroless where possible)
- Non-root container execution
- Read-only root filesystem
- Capability dropping (only SYS_ADMIN, SYS_PTRACE, DAC_READ_SEARCH required)

### Kubernetes Security
- RBAC with least privilege
- Network policies for pod isolation
- Secrets management via Kubernetes Secrets API
- No hostPID or hostNetwork by default

## Security Updates

Security updates are announced via:
1. GitHub Security Advisories
2. Release notes
3. CHANGELOG.md

To receive security notifications, watch the repository for security advisories.

## Full Documentation

See [docs/security/vulnerability-response.md](docs/security/vulnerability-response.md) for our complete vulnerability response process, including:
- Detailed response procedures
- Version support policy
- Upgrade instructions for different deployment methods
- Post-incident review process
