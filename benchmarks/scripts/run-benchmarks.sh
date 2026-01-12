#!/bin/bash
# Panoptes Benchmark Runner
# Copyright 2026 Como Technologies, LTD

set -euo pipefail

# Default configuration
SCENARIO="all"
IMPL="both"
DURATION=300
WARMUP=30
OUTPUT_DIR="results"
EVENT_RATE=1000
VERBOSE=false

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

usage() {
    cat << EOF
Usage: $(basename "$0") [OPTIONS]

Options:
    --scenario SCENARIO     Scenario to run (high-event-rate, deep-recursion,
                           large-file-count, mixed-workload, sustained-load, all)
    --impl IMPL            Implementation to test (c, rust, both)
    --duration SECONDS     Test duration (default: 300)
    --warmup SECONDS       Warmup period (default: 30)
    --output DIR           Output directory (default: results)
    --event-rate RATE      Target events per second (default: 1000)
    --verbose              Enable verbose output
    -h, --help             Show this help message

Examples:
    $(basename "$0") --scenario high-event-rate --impl c --duration 600
    $(basename "$0") --scenario all --impl both
EOF
    exit 0
}

log() {
    echo -e "${GREEN}[BENCH]${NC} $*"
}

warn() {
    echo -e "${YELLOW}[WARN]${NC} $*"
}

error() {
    echo -e "${RED}[ERROR]${NC} $*" >&2
    exit 1
}

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --scenario)
            SCENARIO="$2"
            shift 2
            ;;
        --impl)
            IMPL="$2"
            shift 2
            ;;
        --duration)
            DURATION="$2"
            shift 2
            ;;
        --warmup)
            WARMUP="$2"
            shift 2
            ;;
        --output)
            OUTPUT_DIR="$2"
            shift 2
            ;;
        --event-rate)
            EVENT_RATE="$2"
            shift 2
            ;;
        --verbose)
            VERBOSE=true
            shift
            ;;
        -h|--help)
            usage
            ;;
        *)
            error "Unknown option: $1"
            ;;
    esac
done

# Validate scenario
VALID_SCENARIOS="high-event-rate deep-recursion large-file-count mixed-workload sustained-load all"
if [[ ! " $VALID_SCENARIOS " =~ " $SCENARIO " ]]; then
    error "Invalid scenario: $SCENARIO. Valid options: $VALID_SCENARIOS"
fi

# Validate implementation
VALID_IMPLS="c rust both"
if [[ ! " $VALID_IMPLS " =~ " $IMPL " ]]; then
    error "Invalid implementation: $IMPL. Valid options: $VALID_IMPLS"
fi

# Create output directory
mkdir -p "$OUTPUT_DIR"

# Get system info
get_system_info() {
    cat << EOF
{
  "kernel": "$(uname -r)",
  "cpu": "$(grep 'model name' /proc/cpuinfo | head -1 | cut -d: -f2 | xargs)",
  "cpu_cores": $(nproc),
  "memory_gb": $(free -g | awk '/Mem:/ {print $2}'),
  "hostname": "$(hostname)"
}
EOF
}

# Create test directory structure
setup_test_dir() {
    local scenario=$1
    local test_dir="/tmp/bench-$scenario-$$"

    log "Setting up test directory: $test_dir"

    case $scenario in
        high-event-rate)
            mkdir -p "$test_dir"
            # Create 100 files for high event rate testing
            for i in $(seq 1 100); do
                touch "$test_dir/file$i.txt"
            done
            ;;
        deep-recursion)
            local depth=100
            local current="$test_dir"
            for i in $(seq 1 $depth); do
                current="$current/dir$i"
                mkdir -p "$current"
                touch "$current/file.txt"
            done
            ;;
        large-file-count)
            mkdir -p "$test_dir"
            # Create 10000 files (adjust for actual benchmarks)
            for i in $(seq 1 10000); do
                touch "$test_dir/file$i.txt"
            done
            ;;
        mixed-workload)
            mkdir -p "$test_dir"/{config,data,logs,tmp}
            for i in $(seq 1 50); do
                touch "$test_dir/config/config$i.yaml"
                touch "$test_dir/data/data$i.json"
                touch "$test_dir/logs/log$i.log"
            done
            ;;
        sustained-load)
            mkdir -p "$test_dir"
            for i in $(seq 1 100); do
                touch "$test_dir/file$i.txt"
            done
            ;;
    esac

    echo "$test_dir"
}

# Cleanup test directory
cleanup_test_dir() {
    local test_dir=$1
    if [[ -d "$test_dir" ]]; then
        rm -rf "$test_dir"
    fi
}

# Run benchmark for a specific daemon and implementation
run_daemon_bench() {
    local daemon=$1      # argus or janus
    local impl=$2        # c or rust
    local scenario=$3
    local test_dir=$4

    log "Running $daemon ($impl) - $scenario"

    local daemon_path=""
    local port=""

    if [[ "$daemon" == "argus" ]]; then
        port=50051
        if [[ "$impl" == "c" ]]; then
            daemon_path="../daemons/argusd/c/build/argusd"
        else
            daemon_path="../daemons/argusd/rust/target/release/argusd"
        fi
    else
        port=50052
        if [[ "$impl" == "c" ]]; then
            daemon_path="../daemons/janusd/c/build/janusd"
        else
            daemon_path="../daemons/janusd/rust/target/release/janusd"
        fi
    fi

    if [[ ! -x "$daemon_path" ]]; then
        warn "Daemon not found: $daemon_path. Skipping."
        return 1
    fi

    local result_file="$OUTPUT_DIR/${daemon}-${impl}-${scenario}-$(date +%Y%m%d%H%M%S).json"

    # Start daemon in background
    log "Starting daemon..."
    local daemon_pid=""

    # Measure startup time
    local start_time=$(date +%s%N)
    $daemon_path &
    daemon_pid=$!
    sleep 2  # Wait for daemon to start
    local end_time=$(date +%s%N)
    local startup_ms=$(( (end_time - start_time) / 1000000 ))

    # Verify daemon is running
    if ! kill -0 $daemon_pid 2>/dev/null; then
        error "Daemon failed to start"
    fi

    log "Daemon started (PID: $daemon_pid, startup: ${startup_ms}ms)"

    # Run warmup
    log "Warmup phase ($WARMUP seconds)..."
    if [[ -x "src/event-generator" ]]; then
        src/event-generator --dir "$test_dir" --rate $EVENT_RATE --duration $WARMUP >/dev/null 2>&1 || true
    else
        sleep $WARMUP
    fi

    # Run benchmark
    log "Benchmark phase ($DURATION seconds)..."

    # Collect initial memory
    local mem_start=$(cat /proc/$daemon_pid/statm 2>/dev/null | awk '{print $2 * 4 / 1024}' || echo "0")

    # Run event generation
    local events_generated=0
    if [[ -x "src/event-generator" ]]; then
        events_generated=$(src/event-generator --dir "$test_dir" --rate $EVENT_RATE --duration $DURATION 2>/dev/null | tail -1 || echo "0")
    else
        # Fallback: generate events manually
        local end=$((SECONDS + DURATION))
        while [[ $SECONDS -lt $end ]]; do
            for f in "$test_dir"/*.txt 2>/dev/null; do
                echo "test" >> "$f" 2>/dev/null || true
                ((events_generated++)) || true
            done
            sleep 0.01
        done
    fi

    # Collect final memory
    local mem_end=$(cat /proc/$daemon_pid/statm 2>/dev/null | awk '{print $2 * 4 / 1024}' || echo "0")

    # Calculate CPU usage
    local cpu_stat=$(cat /proc/$daemon_pid/stat 2>/dev/null | awk '{print $14 + $15}' || echo "0")
    local cpu_percent=$(echo "scale=2; $cpu_stat / ($DURATION * 100)" | bc 2>/dev/null || echo "0")

    # Get binary size
    local binary_size=$(du -m "$daemon_path" 2>/dev/null | cut -f1 || echo "0")

    # Stop daemon
    log "Stopping daemon..."
    kill $daemon_pid 2>/dev/null || true
    wait $daemon_pid 2>/dev/null || true

    # Calculate throughput
    local throughput=$(echo "scale=2; $events_generated / $DURATION" | bc 2>/dev/null || echo "0")

    # Write results
    cat > "$result_file" << EOF
{
  "timestamp": "$(date -Iseconds)",
  "daemon": "$daemon",
  "implementation": "$impl",
  "scenario": "$scenario",
  "duration_seconds": $DURATION,
  "metrics": {
    "throughput": {
      "events_per_second": $throughput,
      "total_events": $events_generated
    },
    "latency": {
      "p50_ms": 0.0,
      "p99_ms": 0.0,
      "max_ms": 0.0
    },
    "resource_usage": {
      "memory_start_mb": $mem_start,
      "memory_end_mb": $mem_end,
      "cpu_percent": $cpu_percent
    },
    "binary_size_mb": $binary_size,
    "startup_time_ms": $startup_ms
  },
  "system_info": $(get_system_info)
}
EOF

    log "Results written to: $result_file"
    log "  Throughput: $throughput events/sec"
    log "  Memory: ${mem_end}MB"
    log "  CPU: ${cpu_percent}%"

    return 0
}

# Run benchmarks for a scenario
run_scenario() {
    local scenario=$1

    log "Starting scenario: $scenario"

    # Setup test directory
    local test_dir=$(setup_test_dir "$scenario")

    # Run for each implementation
    local impls=""
    if [[ "$IMPL" == "both" ]]; then
        impls="c rust"
    else
        impls="$IMPL"
    fi

    for impl in $impls; do
        run_daemon_bench "argus" "$impl" "$scenario" "$test_dir" || warn "Argus $impl benchmark failed"
        run_daemon_bench "janus" "$impl" "$scenario" "$test_dir" || warn "Janus $impl benchmark failed"
    done

    # Cleanup
    cleanup_test_dir "$test_dir"

    log "Completed scenario: $scenario"
}

# Main
main() {
    log "Panoptes Benchmark Suite"
    log "================================"
    log "Scenario: $SCENARIO"
    log "Implementation: $IMPL"
    log "Duration: ${DURATION}s"
    log "Warmup: ${WARMUP}s"
    log "Output: $OUTPUT_DIR"
    log ""

    # Run scenarios
    if [[ "$SCENARIO" == "all" ]]; then
        for s in high-event-rate deep-recursion large-file-count mixed-workload; do
            run_scenario "$s"
        done
    else
        run_scenario "$SCENARIO"
    fi

    log ""
    log "All benchmarks complete!"
    log "Results saved to: $OUTPUT_DIR/"
}

main
