# Proto Makefile for panoptes
# Generates Go code from proto definitions using buf

.PHONY: all generate lint format clean deps

BUF := $(shell command -v buf 2>/dev/null)
PROTO_DIR := .
GEN_DIR := ../gen

all: generate

# Install buf if not present
deps:
ifndef BUF
	@echo "Installing buf..."
	@curl -sSL "https://github.com/bufbuild/buf/releases/latest/download/buf-$(shell uname -s)-$(shell uname -m)" -o /tmp/buf
	@chmod +x /tmp/buf
	@sudo mv /tmp/buf /usr/local/bin/buf || mv /tmp/buf ~/.local/bin/buf
endif

# Generate Go code from protos
generate: lint
	@echo "Generating Go code from protos..."
	@mkdir -p $(GEN_DIR)/go
	buf generate

# Lint proto files
lint:
	@echo "Linting proto files..."
	buf lint

# Format proto files
format:
	@echo "Formatting proto files..."
	buf format -w

# Clean generated files
clean:
	@echo "Cleaning generated files..."
	rm -rf $(GEN_DIR)

# Update buf dependencies
update:
	buf dep update
