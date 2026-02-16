#!/usr/bin/env bash
# on-create.sh — One-time setup run when the devcontainer is first created.
set -euo pipefail

# Create the Docker bridge network for kind clusters.
# Matches the subnet used by the operator devcontainers.
docker network create -d=bridge --subnet=172.19.0.0/24 kind || true
