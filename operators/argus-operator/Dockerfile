# Build the manager binary
# NOTE: Build context must be the repository root (not the operator directory)
# Usage: docker build -f operators/argus-operator/Dockerfile -t argus-operator .
FROM golang:1.24 AS builder
ARG TARGETOS
ARG TARGETARCH

WORKDIR /workspace

# Copy the shared gen/go module first (required by replace directive in go.mod)
COPY gen/go gen/go

# Copy the operator's Go Modules manifests
COPY operators/argus-operator/go.mod operators/argus-operator/go.mod
COPY operators/argus-operator/go.sum operators/argus-operator/go.sum

# Download dependencies
WORKDIR /workspace/operators/argus-operator
RUN go mod download

# Copy the operator source
WORKDIR /workspace
COPY operators/argus-operator operators/argus-operator

# Build
# the GOARCH has no default value to allow the binary to be built according to the host where the command
# was called. For example, if we call make docker-build in a local env which has the Apple Silicon M1 SO
# the docker BUILDPLATFORM arg will be linux/arm64 when for Apple x86 it will be linux/amd64. Therefore,
# by leaving it empty we can ensure that the container and binary shipped on it will have the same platform.
WORKDIR /workspace/operators/argus-operator
RUN CGO_ENABLED=0 GOOS=${TARGETOS:-linux} GOARCH=${TARGETARCH} go build -a -o manager cmd/main.go

# Use distroless as minimal base image to package the manager binary
# Refer to https://github.com/GoogleContainerTools/distroless for more details
FROM gcr.io/distroless/static:nonroot
WORKDIR /
COPY --from=builder /workspace/operators/argus-operator/manager .
USER 65532:65532

ENTRYPOINT ["/manager"]
