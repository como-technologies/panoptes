#!/bin/bash
# Build Container Images for Benchmarking
# Copyright 2026 Como Technologies, LTD
#
# Builds all daemon images and reports image sizes

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
RESULTS_DIR="${SCRIPT_DIR}/../results"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log() { echo -e "${GREEN}[BUILD]${NC} $*"; }
warn() { echo -e "${YELLOW}[WARN]${NC} $*"; }
error() { echo -e "${RED}[ERROR]${NC} $*" >&2; exit 1; }
info() { echo -e "${BLUE}[INFO]${NC} $*"; }

# Image definitions
declare -A IMAGES=(
    ["argusd-c"]="daemons/argusd/Dockerfile.c"
    ["argusd-rust"]="daemons/argusd/Dockerfile.rust"
    ["janusd-c"]="daemons/janusd/Dockerfile.c"
    ["janusd-rust"]="daemons/janusd/Dockerfile.rust"
)

# Parse arguments
BUILD_ALL=true
NO_CACHE=false
OUTPUT_JSON=false

usage() {
    cat << EOF
Usage: $(basename "$0") [OPTIONS] [IMAGE...]

Build container images for C vs Rust comparison benchmarks.

Options:
    --no-cache      Build without using Docker cache
    --json          Output image sizes as JSON
    -h, --help      Show this help message

Images:
    argusd-c        Argus daemon (C implementation)
    argusd-rust     Argus daemon (Rust implementation)
    janusd-c        Janus daemon (C implementation)
    janusd-rust     Janus daemon (Rust implementation)

If no images are specified, all images are built.

Examples:
    $(basename "$0")                    # Build all images
    $(basename "$0") argusd-c           # Build only C argus
    $(basename "$0") --no-cache         # Build all without cache
    $(basename "$0") --json             # Output JSON sizes
EOF
    exit 0
}

SELECTED_IMAGES=()

while [[ $# -gt 0 ]]; do
    case $1 in
        --no-cache)
            NO_CACHE=true
            shift
            ;;
        --json)
            OUTPUT_JSON=true
            shift
            ;;
        -h|--help)
            usage
            ;;
        *)
            if [[ -n "${IMAGES[$1]:-}" ]]; then
                SELECTED_IMAGES+=("$1")
                BUILD_ALL=false
            else
                error "Unknown image: $1"
            fi
            shift
            ;;
    esac
done

# If no specific images selected, build all
if $BUILD_ALL; then
    SELECTED_IMAGES=("argusd-c" "argusd-rust" "janusd-c" "janusd-rust")
fi

# Build an image and return build time
build_image() {
    local name=$1
    local dockerfile=${IMAGES[$name]}
    local tag="panoptes/${name%%-*}:${name##*-}"

    log "Building $name ($tag)..."

    local cache_arg=""
    if $NO_CACHE; then
        cache_arg="--no-cache"
    fi

    local start_time=$(date +%s%N)

    if ! docker build $cache_arg \
        -t "$tag" \
        -f "$PROJECT_ROOT/$dockerfile" \
        "$PROJECT_ROOT" 2>&1 | while read line; do
            if $NO_CACHE || [[ "$line" != *"Using cache"* ]]; then
                echo "  $line"
            fi
        done; then
        error "Failed to build $name"
    fi

    local end_time=$(date +%s%N)
    local build_time_ms=$(( (end_time - start_time) / 1000000 ))

    log "Built $name in ${build_time_ms}ms"
    echo "$build_time_ms"
}

# Get image size in bytes
get_image_size() {
    local tag=$1
    docker images --format "{{.Size}}" "$tag" | head -1
}

# Convert human-readable size to MB
size_to_mb() {
    local size=$1
    local num=$(echo "$size" | grep -oP '^[0-9.]+')
    local unit=$(echo "$size" | grep -oP '[A-Za-z]+$')

    case $unit in
        B)   echo "scale=2; $num / 1048576" | bc ;;
        KB)  echo "scale=2; $num / 1024" | bc ;;
        MB)  echo "$num" ;;
        GB)  echo "scale=2; $num * 1024" | bc ;;
        *)   echo "0" ;;
    esac
}

# Main
main() {
    log "Panoptes Container Build"
    log "========================"
    log "Building images: ${SELECTED_IMAGES[*]}"
    log ""

    mkdir -p "$RESULTS_DIR"

    declare -A BUILD_TIMES
    declare -A IMAGE_SIZES

    for name in "${SELECTED_IMAGES[@]}"; do
        BUILD_TIMES[$name]=$(build_image "$name")
    done

    log ""
    log "Collecting image sizes..."

    for name in "${SELECTED_IMAGES[@]}"; do
        local tag="panoptes/${name%%-*}:${name##*-}"
        local size=$(get_image_size "$tag")
        local size_mb=$(size_to_mb "$size")
        IMAGE_SIZES[$name]="$size_mb"
        info "$name: $size ($size_mb MB)"
    done

    # Output results
    log ""
    log "Build Summary"
    log "============="
    printf "%-15s %12s %12s\n" "Image" "Size (MB)" "Build (ms)"
    printf "%-15s %12s %12s\n" "---------------" "------------" "------------"

    for name in "${SELECTED_IMAGES[@]}"; do
        printf "%-15s %12s %12s\n" "$name" "${IMAGE_SIZES[$name]}" "${BUILD_TIMES[$name]}"
    done

    # Write JSON results
    local json_file="$RESULTS_DIR/image-sizes.json"
    cat > "$json_file" << EOF
{
  "timestamp": "$(date -Iseconds)",
  "images": {
EOF

    local first=true
    for name in "${SELECTED_IMAGES[@]}"; do
        if ! $first; then
            echo "," >> "$json_file"
        fi
        first=false
        local tag="panoptes/${name%%-*}:${name##*-}"
        cat >> "$json_file" << EOF
    "$name": {
      "tag": "$tag",
      "size_mb": ${IMAGE_SIZES[$name]},
      "build_time_ms": ${BUILD_TIMES[$name]}
    }
EOF
    done

    cat >> "$json_file" << EOF

  }
}
EOF

    log ""
    log "Results written to: $json_file"

    if $OUTPUT_JSON; then
        cat "$json_file"
    fi

    log ""
    log "All images built successfully!"
}

main
