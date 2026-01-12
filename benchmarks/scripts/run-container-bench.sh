#!/bin/bash
# Run Container-Based Benchmarks
# Copyright 2026 Como Technologies, LTD
#
# Runs benchmarks against containerized C and Rust daemon implementations

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BENCH_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
PROJECT_ROOT="$(cd "$BENCH_DIR/.." && pwd)"
RESULTS_DIR="$BENCH_DIR/results"

# Default configuration
SCENARIO="high-event-rate"
IMPL="both"
DURATION=60
WARMUP=10
EVENT_RATE=1000
VERBOSE=false

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log() { echo -e "${GREEN}[BENCH]${NC} $*"; }
warn() { echo -e "${YELLOW}[WARN]${NC} $*"; }
error() { echo -e "${RED}[ERROR]${NC} $*" >&2; exit 1; }
info() { echo -e "${BLUE}[INFO]${NC} $*"; }

usage() {
    cat << EOF
Usage: $(basename "$0") [OPTIONS]

Run containerized benchmarks for C vs Rust daemon comparison.

Options:
    --scenario SCENARIO   Scenario to run (high-event-rate, deep-recursion,
                          large-file-count, mixed-workload, all)
    --impl IMPL           Implementation to test (c, rust, both)
    --duration SECONDS    Test duration (default: 60)
    --warmup SECONDS      Warmup period (default: 10)
    --event-rate RATE     Events per second (default: 1000)
    --verbose             Enable verbose output
    -h, --help            Show this help message

Examples:
    $(basename "$0") --scenario high-event-rate --impl both
    $(basename "$0") --impl c --duration 120
    $(basename "$0") --scenario all --verbose
EOF
    exit 0
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
VALID_SCENARIOS="high-event-rate deep-recursion large-file-count mixed-workload all"
if [[ ! " $VALID_SCENARIOS " =~ " $SCENARIO " ]]; then
    error "Invalid scenario: $SCENARIO. Valid: $VALID_SCENARIOS"
fi

# Validate implementation
if [[ ! "$IMPL" =~ ^(c|rust|both)$ ]]; then
    error "Invalid implementation: $IMPL. Valid: c, rust, both"
fi

mkdir -p "$RESULTS_DIR"

# Get system info
get_system_info() {
    cat << EOF
{
    "kernel": "$(uname -r)",
    "cpu": "$(grep 'model name' /proc/cpuinfo | head -1 | cut -d: -f2 | xargs)",
    "cpu_cores": $(nproc),
    "memory_gb": $(free -g | awk '/Mem:/ {print $2}'),
    "hostname": "$(hostname)",
    "docker_version": "$(docker --version | cut -d' ' -f3 | tr -d ',')"
}
EOF
}

# Create test directory structure in volume
setup_test_volume() {
    local scenario=$1
    local volume_name="panoptes_bench-data"

    log "Setting up test volume for: $scenario"

    # Ensure volume exists
    docker volume create "$volume_name" >/dev/null 2>&1 || true

    # Create test structure using a temporary container
    docker run --rm -v "$volume_name:/data" alpine sh -c "
        rm -rf /data/*
        case '$scenario' in
            high-event-rate)
                mkdir -p /data/bench
                for i in \$(seq 1 100); do
                    touch /data/bench/file\$i.txt
                done
                ;;
            deep-recursion)
                current='/data/bench'
                for i in \$(seq 1 50); do
                    current=\"\$current/dir\$i\"
                    mkdir -p \"\$current\"
                    touch \"\$current/file.txt\"
                done
                ;;
            large-file-count)
                mkdir -p /data/bench
                for i in \$(seq 1 5000); do
                    touch /data/bench/file\$i.txt
                done
                ;;
            mixed-workload)
                mkdir -p /data/bench/{config,data,logs,tmp}
                for i in \$(seq 1 25); do
                    touch /data/bench/config/config\$i.yaml
                    touch /data/bench/data/data\$i.json
                    touch /data/bench/logs/log\$i.log
                done
                ;;
        esac
    " 2>/dev/null

    log "Test volume ready"
}

# Cleanup test volume
cleanup_test_volume() {
    docker volume rm panoptes_bench-data >/dev/null 2>&1 || true
}

# Measure container startup time
measure_startup() {
    local container_name=$1
    local image=$2

    local start_ns=$(date +%s%N)

    # Start container
    docker run -d \
        --name "$container_name" \
        --privileged \
        --cap-add SYS_ADMIN \
        --cap-add SYS_PTRACE \
        --cap-add DAC_READ_SEARCH \
        -v panoptes_bench-data:/data:rw \
        "$image" >/dev/null 2>&1

    # Wait for container to be running
    local max_wait=30
    local waited=0
    while [[ "$(docker inspect -f '{{.State.Running}}' "$container_name" 2>/dev/null)" != "true" ]]; do
        sleep 0.1
        waited=$((waited + 1))
        if [[ $waited -gt $((max_wait * 10)) ]]; then
            error "Container $container_name failed to start"
        fi
    done

    local end_ns=$(date +%s%N)
    local startup_ms=$(( (end_ns - start_ns) / 1000000 ))

    echo "$startup_ms"
}

# Get container memory usage in MB
get_container_memory() {
    local container_name=$1
    docker stats --no-stream --format "{{.MemUsage}}" "$container_name" 2>/dev/null | \
        grep -oP '^[0-9.]+' | head -1 || echo "0"
}

# Get container CPU usage
get_container_cpu() {
    local container_name=$1
    docker stats --no-stream --format "{{.CPUPerc}}" "$container_name" 2>/dev/null | \
        tr -d '%' || echo "0"
}

# Get image size in MB
get_image_size_mb() {
    local image=$1
    local size=$(docker images --format "{{.Size}}" "$image" | head -1)
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

# Run benchmark for a specific daemon and implementation
run_container_bench() {
    local daemon=$1
    local impl=$2
    local scenario=$3

    local image="panoptes/${daemon}d:${impl}"
    local container_name="bench-${daemon}d-${impl}-$$"

    log "Running $daemon ($impl) - $scenario"

    # Check image exists
    if ! docker image inspect "$image" >/dev/null 2>&1; then
        warn "Image $image not found. Run 'make build-containers' first."
        return 1
    fi

    # Get image size
    local image_size_mb=$(get_image_size_mb "$image")
    info "Image size: ${image_size_mb} MB"

    # Measure startup time
    log "Starting container..."
    local startup_ms=$(measure_startup "$container_name" "$image")
    info "Startup time: ${startup_ms}ms"

    # Wait for daemon to be ready
    sleep 2

    # Run warmup
    if [[ $WARMUP -gt 0 ]]; then
        log "Warmup phase ($WARMUP seconds)..."
        docker run --rm -v panoptes_bench-data:/data \
            panoptes/event-generator:latest \
            --dir /data/bench --rate "$EVENT_RATE" --duration "$WARMUP" \
            >/dev/null 2>&1 || true
    fi

    # Collect pre-benchmark memory
    local mem_start=$(get_container_memory "$container_name")

    # Run benchmark
    log "Benchmark phase ($DURATION seconds)..."
    local events_generated=0
    events_generated=$(docker run --rm -v panoptes_bench-data:/data \
        panoptes/event-generator:latest \
        --dir /data/bench --rate "$EVENT_RATE" --duration "$DURATION" \
        2>/dev/null | tail -1 || echo "0")

    # Collect post-benchmark metrics
    local mem_end=$(get_container_memory "$container_name")
    local cpu_percent=$(get_container_cpu "$container_name")

    # Calculate throughput
    local throughput=$(echo "scale=2; $events_generated / $DURATION" | bc 2>/dev/null || echo "0")

    # Stop and remove container
    log "Stopping container..."
    docker stop "$container_name" >/dev/null 2>&1 || true
    docker rm "$container_name" >/dev/null 2>&1 || true

    # Write results
    local result_file="$RESULTS_DIR/${daemon}-${impl}-${scenario}-container-$(date +%Y%m%d%H%M%S).json"
    cat > "$result_file" << EOF
{
  "timestamp": "$(date -Iseconds)",
  "daemon": "$daemon",
  "implementation": "$impl",
  "scenario": "$scenario",
  "mode": "container",
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
    "container": {
      "image": "$image",
      "image_size_mb": $image_size_mb,
      "startup_time_ms": $startup_ms
    }
  },
  "system_info": $(get_system_info)
}
EOF

    log "Results written to: $result_file"
    log "  Throughput: $throughput events/sec"
    log "  Memory: ${mem_end}MB"
    log "  CPU: ${cpu_percent}%"
    log "  Image size: ${image_size_mb}MB"
    log "  Startup: ${startup_ms}ms"

    return 0
}

# Run benchmarks for a scenario
run_scenario() {
    local scenario=$1

    log "Starting scenario: $scenario"

    # Setup test volume
    setup_test_volume "$scenario"

    # Determine implementations to test
    local impls=""
    if [[ "$IMPL" == "both" ]]; then
        impls="c rust"
    else
        impls="$IMPL"
    fi

    # Run benchmarks
    for impl in $impls; do
        run_container_bench "argus" "$impl" "$scenario" || warn "Argus $impl benchmark failed"
        run_container_bench "janus" "$impl" "$scenario" || warn "Janus $impl benchmark failed"
    done

    log "Completed scenario: $scenario"
}

# Main
main() {
    log "Panoptes Container Benchmark Suite"
    log "==================================="
    log "Scenario: $SCENARIO"
    log "Implementation: $IMPL"
    log "Duration: ${DURATION}s"
    log "Warmup: ${WARMUP}s"
    log "Event rate: $EVENT_RATE/sec"
    log ""

    # Check Docker is available
    if ! command -v docker &>/dev/null; then
        error "Docker is not installed or not in PATH"
    fi

    # Check for required images
    log "Checking for required images..."
    local missing=false
    for img in panoptes/argusd:c panoptes/argusd:rust panoptes/janusd:c panoptes/janusd:rust panoptes/event-generator:latest; do
        if ! docker image inspect "$img" >/dev/null 2>&1; then
            warn "Missing image: $img"
            missing=true
        fi
    done

    if $missing; then
        error "Some images are missing. Run 'make build-containers' first."
    fi

    # Run scenarios
    if [[ "$SCENARIO" == "all" ]]; then
        for s in high-event-rate deep-recursion large-file-count mixed-workload; do
            run_scenario "$s"
        done
    else
        run_scenario "$SCENARIO"
    fi

    # Cleanup
    cleanup_test_volume

    log ""
    log "All container benchmarks complete!"
    log "Results saved to: $RESULTS_DIR/"
}

main
