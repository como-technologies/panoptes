# Contributing to Panoptes

Thank you for your interest in contributing to Panoptes! This document provides guidelines and instructions for contributing.

## Code of Conduct

This project adheres to the [Contributor Covenant Code of Conduct](CODE_OF_CONDUCT.md). By participating, you are expected to uphold this code.

## How to Contribute

### Reporting Issues

- Use [GitHub Issues](https://github.com/como-technologies/panoptes/issues) to report bugs or request features
- Search existing issues before creating a new one
- Use the provided issue templates for bug reports and feature requests

### Pull Requests

1. Fork the repository and create a feature branch from `main`
2. Follow the development setup below to build and test locally
3. Write clear commit messages describing the change
4. Ensure all tests pass and linting is clean
5. Submit a PR with a clear description of the changes

### What We're Looking For

- **Compliance templates**: New framework templates (FedRAMP, ISO 27001, etc.)
- **Platform guides**: Deployment guides for additional Kubernetes platforms
- **Bug fixes**: Especially around edge cases in container runtime detection
- **Documentation**: Improvements to guides, API docs, and examples
- **Integration examples**: Kyverno policies, AlertManager configs, SIEM integrations

## Development Setup

### Prerequisites

- Go 1.24+
- Rust 1.75+ (with musl target for static builds)
- Docker or Podman
- Kind (for local Kubernetes clusters)
- Helm 3.14+
- protoc (Protocol Buffers compiler)

### Building from Source

```bash
# Clone the repository
git clone https://github.com/como-technologies/panoptes.git
cd panoptes

# Build operators (Go)
cd operators/argus-operator && make build
cd operators/janus-operator && make build

# Build daemons (Rust)
cd daemons && cargo build --release

# Build UI
cd ui/panoptes-eye && npm ci && npm run build

# Or use the all-in-one local deployment
./hack/local-deploy.sh all
```

### Running Tests

```bash
# Operator unit tests
cd operators/argus-operator && make test
cd operators/janus-operator && make test

# Daemon tests
cd daemons && cargo test

# E2E tests (requires Kind cluster)
cd test/e2e && go test ./...
```

### Project Structure

```
panoptes/
├── operators/           # Kubernetes operators (Go, controller-runtime)
│   ├── argus-operator/  # File integrity monitoring operator
│   └── janus-operator/  # File access auditing operator
├── daemons/             # Node-level daemons (Rust, async tokio)
│   ├── argusd/          # inotify-based FIM daemon
│   ├── janusd/          # fanotify-based audit daemon
│   └── common/          # Shared library (panoptes-common)
├── ui/panoptes-eye/     # Web dashboard (Next.js, React)
├── charts/              # Helm charts (standalone + unified)
├── deploy/              # Kustomize manifests + compliance templates
├── proto/               # gRPC service definitions
├── examples/            # Copy-paste runnable demo scenarios
├── docs/                # Documentation
└── hack/                # Development and deployment scripts
```

### Coding Standards

**Go (Operators)**
- Follow [Effective Go](https://go.dev/doc/effective_go) and controller-runtime conventions
- Run `golangci-lint run` before submitting
- Use structured logging (`slog`)
- Add unit tests for controller reconciliation logic

**Rust (Daemons)**
- Follow standard Rust conventions (`cargo fmt`, `cargo clippy`)
- Use `cargo deny check` to verify dependency licenses
- See [Rust Security Practices](docs/security/rust-security-practices.md) for memory safety guidelines

**Helm Charts**
- Run `helm lint` and `helm template` to validate
- Follow [Helm best practices](https://helm.sh/docs/chart_best_practices/)

## License

By contributing, you agree that your contributions will be licensed under the [Apache License 2.0](LICENSE).
