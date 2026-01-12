#!/bin/bash
# Compare C and Rust implementations
# Copyright 2026 Como Technologies, LTD

set -euo pipefail

RESULTS_DIR="${1:-results}"
CONTAINER_MODE=false

# Parse arguments
shift || true
while [[ $# -gt 0 ]]; do
    case $1 in
        --container)
            CONTAINER_MODE=true
            shift
            ;;
        *)
            shift
            ;;
    esac
done

if $CONTAINER_MODE; then
    echo "=========================================="
    echo "  C vs Rust Container Comparison"
    echo "=========================================="
else
    echo "=========================================="
    echo "  C vs Rust Implementation Comparison"
    echo "=========================================="
fi
echo ""

if [[ ! -d "$RESULTS_DIR" ]]; then
    echo "Error: Results directory not found: $RESULTS_DIR"
    exit 1
fi

# Calculate percentage difference
calc_diff() {
    local base=$1
    local compare=$2
    if (( $(echo "$base > 0" | bc -l 2>/dev/null || echo "0") )); then
        echo "scale=1; (($compare - $base) / $base) * 100" | bc 2>/dev/null || echo "N/A"
    else
        echo "N/A"
    fi
}

# Find matching result pairs
for daemon in argus janus; do
    echo "### ${daemon}d ###"
    echo ""

    for scenario in high-event-rate deep-recursion large-file-count mixed-workload; do
        # Determine file pattern based on mode
        if $CONTAINER_MODE; then
            c_file=$(ls -t "$RESULTS_DIR/${daemon}-c-${scenario}-container-"*.json 2>/dev/null | head -1 || true)
            rust_file=$(ls -t "$RESULTS_DIR/${daemon}-rust-${scenario}-container-"*.json 2>/dev/null | head -1 || true)
        else
            c_file=$(ls -t "$RESULTS_DIR/${daemon}-c-${scenario}-"*.json 2>/dev/null | grep -v container | head -1 || true)
            rust_file=$(ls -t "$RESULTS_DIR/${daemon}-rust-${scenario}-"*.json 2>/dev/null | grep -v container | head -1 || true)
        fi

        if [[ -f "$c_file" ]] && [[ -f "$rust_file" ]]; then
            echo "Scenario: $scenario"
            echo "---"

            # Extract metrics
            c_throughput=$(jq -r '.metrics.throughput.events_per_second // 0' "$c_file")
            rust_throughput=$(jq -r '.metrics.throughput.events_per_second // 0' "$rust_file")

            c_mem=$(jq -r '.metrics.resource_usage.memory_end_mb // 0' "$c_file")
            rust_mem=$(jq -r '.metrics.resource_usage.memory_end_mb // 0' "$rust_file")

            c_cpu=$(jq -r '.metrics.resource_usage.cpu_percent // 0' "$c_file")
            rust_cpu=$(jq -r '.metrics.resource_usage.cpu_percent // 0' "$rust_file")

            c_startup=$(jq -r '.metrics.startup_time_ms // .metrics.container.startup_time_ms // 0' "$c_file")
            rust_startup=$(jq -r '.metrics.startup_time_ms // .metrics.container.startup_time_ms // 0' "$rust_file")

            c_size=$(jq -r '.metrics.binary_size_mb // 0' "$c_file")
            rust_size=$(jq -r '.metrics.binary_size_mb // 0' "$rust_file")

            # Container-specific metrics
            if $CONTAINER_MODE; then
                c_image_size=$(jq -r '.metrics.container.image_size_mb // 0' "$c_file")
                rust_image_size=$(jq -r '.metrics.container.image_size_mb // 0' "$rust_file")
            fi

            # Calculate differences
            throughput_diff=$(calc_diff "$c_throughput" "$rust_throughput")
            mem_diff=$(calc_diff "$c_mem" "$rust_mem")
            startup_diff=$(calc_diff "$c_startup" "$rust_startup")
            size_diff=$(calc_diff "$c_size" "$rust_size")

            # Print comparison table
            printf "%-20s %12s %12s %12s\n" "Metric" "C" "Rust" "Diff %"
            printf "%-20s %12s %12s %12s\n" "--------------------" "------------" "------------" "------------"
            printf "%-20s %12.1f %12.1f %12s%%\n" "Throughput (evt/s)" "$c_throughput" "$rust_throughput" "$throughput_diff"
            printf "%-20s %12.1f %12.1f %12s%%\n" "Memory (MB)" "$c_mem" "$rust_mem" "$mem_diff"
            printf "%-20s %12.1f %12.1f %12s\n" "CPU (%)" "$c_cpu" "$rust_cpu" "N/A"
            printf "%-20s %12.0f %12.0f %12s%%\n" "Startup (ms)" "$c_startup" "$rust_startup" "$startup_diff"

            if $CONTAINER_MODE; then
                image_diff=$(calc_diff "$c_image_size" "$rust_image_size")
                printf "%-20s %12.1f %12.1f %12s%%\n" "Image Size (MB)" "$c_image_size" "$rust_image_size" "$image_diff"
            else
                printf "%-20s %12.0f %12.0f %12s%%\n" "Binary Size (MB)" "$c_size" "$rust_size" "$size_diff"
            fi
            echo ""
        fi
    done

    echo ""
done

# Summary section
echo "=========================================="
if $CONTAINER_MODE; then
    echo "  Container Comparison Summary"
    echo "=========================================="
    echo ""
    echo "Image Sizes (from Docker):"
    echo "--------------------------"
    docker images --format "table {{.Repository}}:{{.Tag}}\t{{.Size}}" 2>/dev/null | grep panoptes || echo "No panoptes images found"
else
    echo "  Comparison complete"
    echo "=========================================="
fi
