# Rust Security Practices

This document describes the security practices and tools used in Panoptes' Rust codebase (argusd, janusd daemons).

## Why Rust for Security Daemons

Panoptes daemons interact directly with Linux kernel interfaces (inotify, fanotify) and handle sensitive file system operations. Rust was chosen for several compile-time security guarantees:

### Memory Safety

Rust's ownership system prevents entire classes of vulnerabilities at compile time:

| Vulnerability Class | Rust Prevention |
|---------------------|-----------------|
| Buffer overflow | Bounds checking on all array/slice access |
| Use-after-free | Ownership system prevents dangling references |
| Double-free | Single ownership ensures one deallocation |
| Null pointer dereference | No null pointers; `Option<T>` forces explicit handling |
| Data races | Ownership + borrow checker prevents shared mutable state |

### Type Safety

- No implicit type conversions that could cause truncation or overflow
- Exhaustive pattern matching ensures all enum variants are handled
- Strong typing for file descriptors, paths, and system calls

### Integer Safety

We enable overflow checks in release builds:

```toml
# Cargo.toml
[profile.release]
overflow-checks = true
```

This catches integer overflow/underflow that could lead to:
- Incorrect buffer sizes
- File offset calculation errors
- Resource exhaustion attacks

## Security Crates

### Secret Handling: `secrecy` + `zeroize`

Located in `daemons/common/Cargo.toml`:

```toml
secrecy = { version = "0.10", features = ["serde"] }
zeroize = { version = "1.8", features = ["derive"] }
```

**Purpose:**
- `Secret<T>` wrapper prevents accidental logging/display of sensitive data
- `Zeroize` trait ensures memory is zeroed on drop (prevents memory scraping)

**Use cases in Panoptes:**
- TLS certificates for gRPC communication
- File hashes during integrity verification
- Any authentication tokens or credentials

**Example usage:**
```rust
use secrecy::{Secret, ExposeSecret};
use zeroize::Zeroize;

// Secret wrapper prevents accidental logging
let api_key: Secret<String> = Secret::new("sensitive-key".to_string());

// Must explicitly expose to use
let key_value = api_key.expose_secret();

// Automatically zeroed when dropped
```

### Unsafe Code Policy

We enforce explicit unsafe blocks even within unsafe functions:

```toml
# Cargo.toml
[lints.rust]
unsafe_op_in_unsafe_fn = "deny"
```

This ensures every unsafe operation is:
1. Explicitly marked with `unsafe { }`
2. Documented with safety invariants
3. Easier to audit

**Guidelines for unsafe code:**
- Minimize unsafe usage; prefer safe abstractions
- Document why unsafe is required
- Document safety invariants that must hold
- Add `// SAFETY:` comments explaining guarantees

## CI Security Checks

### GitHub Actions Workflow

Located at `.github/workflows/security.yml`:

```yaml
name: Security
on:
  push:
    branches: [main]
    paths:
      - 'daemons/**'
      - '**/Cargo.toml'
      - '**/Cargo.lock'
      - 'deny.toml'
  pull_request:
    paths:
      - 'daemons/**'
      - '**/Cargo.toml'
      - '**/Cargo.lock'
      - 'deny.toml'
  schedule:
    - cron: '0 6 * * 1'  # Weekly Monday 6am UTC

jobs:
  audit:
    # Vulnerability scanning with cargo-audit
    # Checks against RustSec Advisory Database

  deny:
    # License, source, and dependency checks with cargo-deny

  geiger:
    # Counts unsafe code usage for visibility

  sbom:
    # Generates Software Bill of Materials
```

### cargo-audit

Scans dependencies against the [RustSec Advisory Database](https://rustsec.org/):

```bash
# Run locally
cargo install cargo-audit
cargo audit

# Check specific manifest
cargo audit -f daemons/argusd/Cargo.lock
```

**What it detects:**
- Known vulnerabilities in dependencies
- Unmaintained crates
- Yanked crate versions

### cargo-deny

Comprehensive dependency analysis configured in `deny.toml`:

```bash
# Run locally
cargo install cargo-deny
cargo deny check

# Check specific category
cargo deny check advisories
cargo deny check licenses
cargo deny check bans
cargo deny check sources
```

**Configuration sections:**

| Section | Purpose |
|---------|---------|
| `[advisories]` | Vulnerability policy; deny known vulnerabilities |
| `[licenses]` | Allowed license list; deny copyleft in proprietary code |
| `[bans]` | Prevent specific crates or duplicate versions |
| `[sources]` | Restrict to crates.io; prevent git dependencies |

### cargo-geiger

Counts unsafe code usage across the dependency tree:

```bash
# Run locally
cargo install cargo-geiger
cargo geiger

# Detailed report
cargo geiger --output-format=Json > geiger-report.json
```

**Purpose:**
- Visibility into total unsafe surface area
- Track unsafe usage over time
- Identify dependencies with high unsafe counts

## Local Security Audit

Run a complete local security audit:

```bash
#!/bin/bash
# scripts/security-audit.sh

set -e

echo "=== Checking for vulnerabilities ==="
cargo audit

echo "=== Checking licenses and dependencies ==="
cargo deny check

echo "=== Counting unsafe usage ==="
cargo geiger --update-readme

echo "=== Checking for outdated dependencies ==="
cargo outdated

echo "=== Security audit complete ==="
```

## Dependency Management

### Dependabot

GitHub Dependabot is configured to:
- Monitor Rust crates for security updates
- Create PRs for vulnerable dependency updates
- Weekly check for all dependency updates

### Vendoring (Optional)

For air-gapped or high-security environments:

```bash
# Vendor all dependencies
cargo vendor

# Add to .cargo/config.toml
[source.crates-io]
replace-with = "vendored-sources"

[source.vendored-sources]
directory = "vendor"
```

## ANSSI Guidelines Reference

Panoptes follows recommendations from the [ANSSI Secure Rust Guidelines](https://anssi-fr.github.io/rust-guide/):

| Guideline | Implementation |
|-----------|----------------|
| Avoid `unsafe` where possible | Minimal unsafe, explicit blocks only |
| Use safe abstractions | tokio, tonic for async/gRPC |
| Enable overflow checks | `overflow-checks = true` in release |
| Use `#[must_use]` | Applied to functions with important return values |
| Validate input early | Protobuf validation at gRPC boundaries |
| Prefer `&str` over `String` | Used where ownership not needed |

### Additional Resources

- [Rust Security Guidelines](https://anssi-fr.github.io/rust-guide/) - ANSSI
- [Secure Rust Guidelines](https://github.com/AdrienChampion/awesome-safety-critical-rust) - Community collection
- [RustSec Advisory Database](https://rustsec.org/) - Vulnerability tracking
- [The Rust Book - Unsafe Rust](https://doc.rust-lang.org/book/ch19-01-unsafe-rust.html)

## Reporting Security Issues

If you discover a security vulnerability in Panoptes' Rust code:

1. **DO NOT** file a public GitHub issue
2. Report via [GitHub Security Advisories](https://github.com/como-technologies/panoptes/security/advisories/new)
3. Or email: security@como-technologies.io

See [vulnerability-response.md](./vulnerability-response.md) for our response process.
