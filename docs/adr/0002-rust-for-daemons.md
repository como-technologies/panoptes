# ADR-0002: Rust for Node Daemons

## Status

Accepted

## Context

The Panoptes daemons (argusd, janusd) run as DaemonSets on every node, interfacing directly with Linux kernel APIs (inotify, fanotify) and processing high-throughput event streams. The language choice for these components affects memory safety, binary size, startup time, and operational overhead.

Options considered:
1. **Go** — Same language as operators, unified toolchain
2. **Rust** — Memory safety guarantees, minimal runtime, zero-cost abstractions
3. **C/C++** — Traditional choice for system-level Linux programming

## Decision

We chose **Rust** for the node daemons while keeping **Go** for the Kubernetes operators.

## Rationale

**Why Rust for daemons:**

- **Memory safety without GC**: Daemons handle raw kernel event structures (`inotify_event`, `fanotify_event_metadata`) via unsafe FFI. Rust's ownership model prevents use-after-free, buffer overflows, and data races at compile time — exactly the class of bugs that plague C-based security tools.
- **Minimal binary size**: Static musl builds produce ~5-8 MB binaries. FROM-scratch Docker images have no OS layer, no shell, no package manager — minimal attack surface for a security-critical component.
- **No GC pauses**: Daemons process real-time event streams. Go's garbage collector introduces latency spikes that can cause event drops under load. Rust has deterministic resource cleanup.
- **Zero-cost async**: tokio provides efficient async I/O for gRPC serving and kernel event processing without thread-per-connection overhead.
- **Ecosystem**: `nix` crate for type-safe Linux syscall wrappers, `tonic` for gRPC, `prometheus` for metrics — mature ecosystem for systems programming.

**Why Go for operators:**

- controller-runtime and kubebuilder are Go-native — no equivalent exists in Rust
- Operators are control-plane components with low throughput requirements
- Go's GC is acceptable for reconciliation loops (not real-time)
- Kubernetes client libraries are most mature in Go

**Why not C/C++:**

- No compile-time memory safety guarantees
- Manual memory management in event-processing hot paths is error-prone
- Build toolchain complexity (CMake, autotools) vs. Cargo
- Security tools written in C are themselves a security risk

## Consequences

- Two language toolchains in the project (Go + Rust), increasing contributor onboarding friction
- Rust compilation is slower than Go, especially with musl + static linking (15-30 min cold build for gRPC/protobuf)
- Shared code between daemons uses a Rust workspace (`panoptes-common` crate)
- gRPC proto definitions are shared between Go and Rust via the `proto/` directory
- Security practices are documented in `docs/security/rust-security-practices.md`
