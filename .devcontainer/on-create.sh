#!/usr/bin/env bash
# on-create.sh — One-time setup run when the devcontainer is first created.
set -euo pipefail

# Clean stale DinD state.  The docker-in-docker feature uses a persistent
# volume for /var/lib/docker, so containers and networks from a previous
# devcontainer survive --remove-existing-container.  Wipe them so every
# rebuild starts clean.  Image layers are kept for build cache.
docker rm -f $(docker ps -aq) 2>/dev/null || true
docker network prune -f 2>/dev/null || true

# Fix: Host Docker uses nftables with a FORWARD policy of DROP, only
# accepting traffic from bridges it created.  The DinD Docker daemon
# creates additional bridges (e.g. the "kind" network) that the host's
# nftables rules don't know about, so their traffic gets dropped.
#
# DOCKER-USER is the official chain for user-defined forwarding rules
# and is evaluated before any other FORWARD sub-chain.  Adding an
# unconditional ACCEPT here lets DinD bridge traffic through the host
# firewall.  Try nftables first, fall back to iptables.
nft insert rule ip filter DOCKER-USER accept 2>/dev/null \
  || iptables -I DOCKER-USER -j ACCEPT 2>/dev/null \
  || true
