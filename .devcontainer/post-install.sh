#!/usr/bin/env bash
# post-install.sh — Install tools not covered by devcontainer features.
# All versions are pinned. Multi-arch via dpkg --print-architecture.
set -euo pipefail

ARCH=$(dpkg --print-architecture)   # amd64 | arm64
GOARCH=$(go env GOARCH)             # amd64 | arm64

# kind v0.27.0
curl -fsSL "https://kind.sigs.k8s.io/dl/v0.27.0/kind-linux-${GOARCH}" -o /tmp/kind
chmod +x /tmp/kind && sudo mv /tmp/kind /usr/local/bin/kind

# kubebuilder v4.5.2
curl -fsSL "https://github.com/kubernetes-sigs/kubebuilder/releases/download/v4.5.2/kubebuilder_linux_${GOARCH}" -o /tmp/kubebuilder
chmod +x /tmp/kubebuilder && sudo mv /tmp/kubebuilder /usr/local/bin/kubebuilder

# buf v1.50.0
curl -fsSL "https://github.com/bufbuild/buf/releases/download/v1.50.0/buf-Linux-$(uname -m)" -o /tmp/buf
chmod +x /tmp/buf && sudo mv /tmp/buf /usr/local/bin/buf

# oras v1.2.2
curl -fsSL "https://github.com/oras-project/oras/releases/download/v1.2.2/oras_1.2.2_linux_${GOARCH}.tar.gz" | tar xz -C /tmp oras
sudo mv /tmp/oras /usr/local/bin/oras

# golangci-lint v2.5.0
curl -fsSL "https://github.com/golangci/golangci-lint/releases/download/v2.5.0/golangci-lint-2.5.0-linux-${GOARCH}.tar.gz" \
  | tar xz --strip-components=1 -C /tmp "golangci-lint-2.5.0-linux-${GOARCH}/golangci-lint"
sudo mv /tmp/golangci-lint /usr/local/bin/golangci-lint

# kustomize (latest)
curl -fsSL "https://raw.githubusercontent.com/kubernetes-sigs/kustomize/master/hack/install_kustomize.sh" | bash
sudo mv kustomize /usr/local/bin/kustomize

# protoc (apt — needed by Rust build.rs for prost)
sudo apt-get update -qq && sudo apt-get install -y -qq protobuf-compiler

# cargo-deny v0.16.3
cargo install --locked cargo-deny@0.16.3

# Rust musl target for cross-compilation
rustup target add x86_64-unknown-linux-musl

# eBPF toolchain (skippable with SKIP_EBPF=1)
if [[ "${SKIP_EBPF:-0}" != "1" ]]; then
  echo "--- Installing eBPF toolchain (set SKIP_EBPF=1 to skip) ---"
  rustup toolchain install nightly --component rust-src
  sudo apt-get install -y -qq llvm clang
  cargo +nightly install bpf-linker
else
  echo "--- Skipping eBPF toolchain (SKIP_EBPF=1) ---"
fi

# Verification
echo ""
echo "=== Toolchain versions ==="
echo "go:             $(go version)"
echo "rustc:          $(rustc --version)"
echo "node:           $(node --version)"
echo "docker:         $(docker --version)"
echo "kubectl:        $(kubectl version --client --short 2>/dev/null || kubectl version --client)"
echo "helm:           $(helm version --short)"
echo "kind:           $(kind version)"
echo "kubebuilder:    $(kubebuilder version)"
echo "buf:            $(buf --version)"
echo "oras:           $(oras version)"
echo "golangci-lint:  $(golangci-lint version --short 2>/dev/null || golangci-lint --version)"
echo "kustomize:      $(kustomize version)"
echo "protoc:         $(protoc --version)"
echo "cargo-deny:     $(cargo deny --version)"
if [[ "${SKIP_EBPF:-0}" != "1" ]]; then
  echo "bpf-linker:     $(cargo +nightly bpf-linker --version 2>/dev/null || echo 'installed')"
fi
echo "=========================="
