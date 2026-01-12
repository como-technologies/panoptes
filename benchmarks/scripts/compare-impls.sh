#!/bin/bash
# Compare C and Rust implementations
# Copyright 2026 Como Technologies, LTD

set -euo pipefail

RESULTS_DIR="${1:-results}"

echo "=========================================="
echo "  C vs Rust Implementation Comparison"
echo "=========================================="
echo ""

if [[ ! -d "$RESULTS_DIR" ]]; then
    echo "Error: Results directory not found: $RESULTS_DIR"
    exit 1
fi

# Find matching result pairs
for daemon in argus janus; do
    echo "### $daemon ###"
    echo ""

    for scenario in high-event-rate deep-recursion large-file-count mixed-workload; do
        c_file=$(ls -t "$RESULTS_DIR/${daemon}-c-${scenario}-"*.json 2>/dev/null | head -1)
        rust_file=$(ls -t "$RESULTS_DIR/${daemon}-rust-${scenario}-"*.json 2>/dev/null | head -1)

        if [[ -f "$c_file" ]] && [[ -f "$rust_file" ]]; then
            echo "Scenario: $scenario"
            echo "---"

            # Extract metrics
            c_throughput=$(jq -r '.metrics.throughput.events_per_second' "$c_file")
            rust_throughput=$(jq -r '.metrics.throughput.events_per_second' "$rust_file")

            c_mem=$(jq -r '.metrics.resource_usage.memory_end_mb' "$c_file")
            rust_mem=$(jq -r '.metrics.resource_usage.memory_end_mb' "$rust_file")

            c_cpu=$(jq -r '.metrics.resource_usage.cpu_percent' "$c_file")
            rust_cpu=$(jq -r '.metrics.resource_usage.cpu_percent' "$rust_file")

            c_startup=$(jq -r '.metrics.startup_time_ms' "$c_file")
            rust_startup=$(jq -r '.metrics.startup_time_ms' "$rust_file")

            c_size=$(jq -r '.metrics.binary_size_mb' "$c_file")
            rust_size=$(jq -r '.metrics.binary_size_mb' "$rust_file")

            # Calculate differences
            if (( $(echo "$c_throughput > 0" | bc -l) )); then
                throughput_diff=$(echo "scale=1; (($rust_throughput - $c_throughput) / $c_throughput) * 100" | bc)
            else
                throughput_diff="N/A"
            fi

            if (( $(echo "$c_mem > 0" | bc -l) )); then
                mem_diff=$(echo "scale=1; (($rust_mem - $c_mem) / $c_mem) * 100" | bc)
            else
                mem_diff="N/A"
            fi

            if (( $(echo "$c_startup > 0" | bc -l) )); then
                startup_diff=$(echo "scale=1; (($rust_startup - $c_startup) / $c_startup) * 100" | bc)
            else
                startup_diff="N/A"
            fi

            # Print comparison table
            printf "%-20s %12s %12s %12s\n" "Metric" "C" "Rust" "Diff %"
            printf "%-20s %12s %12s %12s\n" "--------------------" "------------" "------------" "------------"
            printf "%-20s %12.1f %12.1f %12s\n" "Throughput (evt/s)" "$c_throughput" "$rust_throughput" "$throughput_diff%"
            printf "%-20s %12.1f %12.1f %12s\n" "Memory (MB)" "$c_mem" "$rust_mem" "$mem_diff%"
            printf "%-20s %12.1f %12.1f %12s\n" "CPU (%)" "$c_cpu" "$rust_cpu" "N/A"
            printf "%-20s %12.0f %12.0f %12s\n" "Startup (ms)" "$c_startup" "$rust_startup" "$startup_diff%"
            printf "%-20s %12.0f %12.0f %12s\n" "Binary Size (MB)" "$c_size" "$rust_size" "N/A"
            echo ""
        fi
    done

    echo ""
done

echo "=========================================="
echo "  Comparison complete"
echo "=========================================="
