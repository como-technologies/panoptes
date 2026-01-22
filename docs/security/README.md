# Security Documentation

This directory contains security analysis and hardening documentation for Panoptes.

## Documents

| Document | Purpose | Audience |
|----------|---------|----------|
| [Privileged Container Justification](./privileged-container-justification.md) | Why daemons need elevated privileges and how risk is minimized | Security teams, Auditors |
| [Threat Model](./threat-model.md) | Compromise scenarios and blast radius analysis | Security teams, Incident Response |
| [Cryptographic Guarantees](./cryptographic-guarantees.md) | TLS, event signing, image verification | Security teams, Compliance |
| [Attack Surface Analysis](./attack-surface-analysis.md) | Identifies attack vectors, bypass techniques, and monitoring gaps | Security teams, Red teams |
| [Remediation Plan](./remediation-plan.md) | Actionable checklist for fixing identified gaps | Platform engineers, Security |
| [Advanced Hardening](./advanced-hardening.md) | Defense-in-depth measures beyond baseline | High-security deployments |
| [Vulnerability Response](./vulnerability-response.md) | Process for handling security vulnerabilities | Maintainers, Security teams |
| [Rust Security Practices](./rust-security-practices.md) | Compile-time security, crates, and CI checks | Developers, Security |

## Reading Order

1. **Privileged Container Justification** - Understand why elevated privileges are required
2. **Threat Model** - Understand what happens if components are compromised
3. **Cryptographic Guarantees** - Understand the cryptographic protections in place
4. **Attack Surface Analysis** - Understand what's detected and what's not
5. **Remediation Plan** - Fix critical gaps (complete before production)
6. **Advanced Hardening** - Additional measures for high-security environments

## Quick Reference

### Critical Gaps (Fix Immediately)

| Gap | Risk | Status |
|-----|------|--------|
| `/tmp`, `/dev/shm` unmonitored | Malware staging | See remediation 1.1 |
| Library paths unmonitored (except NIST) | Library injection | See remediation 1.2 |
| 6/7 templates audit-only | Attacks allowed | See remediation 2.1 |

### Kernel-Level Bypasses

| Technique | Mitigation |
|-----------|------------|
| inotify queue overflow | Increase `max_queued_events`, monitor for overflow |
| fanotify TOCTOU | Future: eBPF integration |
| Watch limit exhaustion | Increase `max_user_watches` |

### Template Security Ranking

| Template | Enforcement | Coverage | Recommendation |
|----------|-------------|----------|----------------|
| CIS-K8s | **Enabled** | K8s-focused | Best for K8s security |
| NIST 800-53 | Disabled | Most comprehensive | Enable enforcement |
| PCI-DSS | Disabled | Good | Enable enforcement |
| Others | Disabled | Adequate | Enable enforcement |

## Contributing

Security issues should be reported via the security policy, not public issues.
