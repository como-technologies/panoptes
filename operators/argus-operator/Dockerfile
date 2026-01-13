# Build the manager binary
# NOTE: Build context must be the repository root (not the operator directory)
# Usage: docker build -f operators/argus-operator/Dockerfile -t argus-operator .
FROM golang:1.24 AS builder
ARG TARGETOS
ARG TARGETARCH

# Install buf for proto generation
RUN curl -sSL "https://github.com/bufbuild/buf/releases/latest/download/buf-Linux-x86_64" -o /usr/local/bin/buf && chmod +x /usr/local/bin/buf

WORKDIR /workspace

# Copy proto definitions and generate Go code
COPY proto proto
RUN cd proto && buf generate && \
    cd ../gen/go && \
    go mod init github.com/como-technologies/panoptes/gen/go && \
    go mod tidy

# Copy shared common module manifests first (needed by operator go.mod replace directive)
COPY operators/common/go.mod operators/common/go.mod
COPY operators/common/go.sum operators/common/go.sum

# Copy the operator's Go Modules manifests
COPY operators/argus-operator/go.mod operators/argus-operator/go.mod
COPY operators/argus-operator/go.sum operators/argus-operator/go.sum

# Download dependencies
WORKDIR /workspace/operators/argus-operator
RUN go mod download

# Copy the shared common source
WORKDIR /workspace
COPY operators/common operators/common

# Copy the operator source
COPY operators/argus-operator operators/argus-operator

# Build
# the GOARCH has no default value to allow the binary to be built according to the host where the command
# was called. For example, if we call make docker-build in a local env which has the Apple Silicon M1 SO
# the docker BUILDPLATFORM arg will be linux/arm64 when for Apple x86 it will be linux/amd64. Therefore,
# by leaving it empty we can ensure that the container and binary shipped on it will have the same platform.
WORKDIR /workspace/operators/argus-operator
# Reconcile dependencies between generated proto and operator
RUN go mod tidy
RUN CGO_ENABLED=0 GOOS=${TARGETOS:-linux} GOARCH=${TARGETARCH} go build -a -o manager cmd/main.go

# Use distroless as minimal base image to package the manager binary
# Refer to https://github.com/GoogleContainerTools/distroless for more details
FROM gcr.io/distroless/static:nonroot
WORKDIR /
COPY --from=builder /workspace/operators/argus-operator/manager .
USER 65532:65532

ENTRYPOINT ["/manager"]
