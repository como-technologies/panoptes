# Panoptes Benchmarks

**Performance comparison between C and Rust daemon implementations**

## Overview

This benchmark suite measures the performance characteristics of the Argus and Janus
daemons across their C and Rust implementations. The benchmarks help inform decisions
about which implementation to use in different scenarios.

## Metrics

| Metric | Description | Unit |
|--------|-------------|------|
| **Throughput** | Events processed per second | events/sec |
| **Latency (P50)** | Median event processing time | ms |
| **Latency (P99)** | 99th percentile processing time | ms |
| **Memory** | Resident memory usage | MB |
| **CPU** | CPU utilization under load | % |
| **Startup Time** | Time to initialize and be ready | ms |
| **Binary Size** | Compiled binary size | MB |

## Test Scenarios

### 1. High Event Rate

Measures sustained throughput under heavy load.

- **Target**: 10,000+ events/second
- **Duration**: 5 minutes
- **File operations**: Create, modify, delete in rapid succession

### 2. Deep Recursion

Tests performance with deeply nested directory structures.

- **Directory depth**: 100+ levels
- **Files per level**: 10
- **Total watches**: 1,000+

### 3. Large File Count

Tests scaling with many watched files.

- **Total files**: 100,000+
- **Events**: Random access patterns

### 4. Mixed Workload

Simulates realistic production patterns.

- **Create/modify/delete ratio**: 1:5:1
- **Read/write ratio**: 10:1
- **Burst patterns**: Included

### 5. Sustained Load

Tests stability over extended periods.

- **Duration**: 24 hours
- **Event rate**: 1,000 events/second
- **Memory leak detection**: Enabled

## Directory Structure

```
benchmarks/
├── README.md                 # This file
├── Makefile                  # Build and run benchmarks
├── scripts/
│   ├── run-benchmarks.sh     # Main benchmark runner
│   ├── generate-report.py    # Results visualization
│   └── compare-impls.sh      # C vs Rust comparison
├── scenarios/
│   ├── high-event-rate/      # High throughput scenario
│   ├── deep-recursion/       # Deep directory trees
│   ├── large-file-count/     # Many files scenario
│   └── mixed-workload/       # Realistic patterns
├── results/                  # Benchmark results (gitignored)
└── src/
    └── event-generator.c     # Event generation tool
```

## Running Benchmarks

### Prerequisites

- Docker and Docker Compose
- Linux kernel 5.x+
- At least 8GB RAM
- Fast storage (SSD recommended)

### Quick Start

```bash
# Build all components
make build

# Run all benchmarks
make bench

# Run specific scenario
make bench SCENARIO=high-event-rate

# Run for specific implementation
make bench IMPL=rust

# Compare implementations
make compare
```

### Individual Benchmarks

```bash
# High event rate
./scripts/run-benchmarks.sh --scenario high-event-rate --duration 300

# Deep recursion
./scripts/run-benchmarks.sh --scenario deep-recursion --depth 100

# Large file count
./scripts/run-benchmarks.sh --scenario large-file-count --files 100000

# Mixed workload
./scripts/run-benchmarks.sh --scenario mixed-workload --duration 3600

# Sustained load (24h)
./scripts/run-benchmarks.sh --scenario sustained-load --duration 86400
```

### Configuration

Environment variables for customization:

| Variable | Default | Description |
|----------|---------|-------------|
| `BENCH_DURATION` | `300` | Test duration in seconds |
| `BENCH_EVENT_RATE` | `1000` | Target events per second |
| `BENCH_WARMUP` | `30` | Warmup period in seconds |
| `BENCH_IMPL` | `both` | Implementation to test (c, rust, both) |
| `BENCH_OUTPUT` | `results/` | Output directory |

## Generating Reports

```bash
# Generate HTML report
make report

# Generate comparison charts
python3 scripts/generate-report.py --format html

# Export to CSV
python3 scripts/generate-report.py --format csv
```

## Results Format

Results are stored as JSON in `results/`:

```json
{
  "timestamp": "2026-01-10T10:30:00Z",
  "scenario": "high-event-rate",
  "implementation": "c",
  "duration_seconds": 300,
  "metrics": {
    "throughput": {
      "events_per_second": 45000,
      "total_events": 13500000
    },
    "latency": {
      "p50_ms": 0.12,
      "p99_ms": 2.1,
      "max_ms": 15.3
    },
    "resource_usage": {
      "memory_mb": 45,
      "cpu_percent": 12
    }
  },
  "system_info": {
    "kernel": "6.6.0",
    "cpu": "AMD EPYC 7763",
    "memory_gb": 16
  }
}
```

## Sample Results

### Argus (inotify) - High Event Rate

| Metric | C Implementation | Rust Implementation | Difference |
|--------|------------------|---------------------|------------|
| Throughput (events/sec) | 50,000 | 48,000 | -4% |
| P50 Latency (ms) | 0.10 | 0.08 | -20% |
| P99 Latency (ms) | 2.1 | 1.8 | -14% |
| Memory (MB) | 45 | 38 | -16% |
| CPU (%) | 12 | 11 | -8% |
| Binary Size (MB) | 2.1 | 1.8 | -14% |
| Startup Time (ms) | 15 | 22 | +47% |

### Janus (fanotify) - High Event Rate

| Metric | C Implementation | Rust Implementation | Difference |
|--------|------------------|---------------------|------------|
| Throughput (events/sec) | 35,000 | 33,000 | -6% |
| P50 Latency (ms) | 0.15 | 0.12 | -20% |
| P99 Latency (ms) | 3.2 | 2.8 | -12% |
| Memory (MB) | 48 | 42 | -12% |
| CPU (%) | 15 | 14 | -7% |
| Binary Size (MB) | 2.3 | 2.0 | -13% |
| Startup Time (ms) | 18 | 25 | +39% |

*Note: These are example results. Actual performance varies by system.*

## Methodology

### Throughput Measurement

Events are generated at increasing rates until the daemon can no longer keep up.
The sustainable rate is recorded as throughput.

### Latency Measurement

Latency is measured from event generation to daemon acknowledgment. A high-precision
timer (nanosecond resolution) is used.

### Memory Measurement

Memory usage is sampled every 100ms using `/proc/{pid}/statm`. Peak and average
values are recorded.

### CPU Measurement

CPU utilization is measured using `/proc/{pid}/stat`. User and system time are
tracked separately.

### Statistical Validity

- Each scenario runs for at least 5 minutes (configurable)
- Results exclude warmup period
- Outliers beyond 3 standard deviations are flagged
- Multiple runs are averaged for final results

## Interpreting Results

### When to Choose C Implementation

- Maximum raw throughput required
- Faster startup time needed
- Existing C expertise on team
- Integration with other C components

### When to Choose Rust Implementation

- Memory safety is critical
- Lower latency requirements (P99)
- Smaller binary size preferred
- Modern tooling and ecosystem

## Contributing

To add new benchmark scenarios:

1. Create a new directory under `scenarios/`
2. Add a `config.yaml` describing the scenario
3. Implement the event pattern in `src/`
4. Update `run-benchmarks.sh` to include the scenario

## License

Copyright 2026 Como Technologies, LTD

Licensed under the Apache License, Version 2.0.
