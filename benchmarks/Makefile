# Panoptes Benchmark Suite
# Copyright 2026 Como Technologies, LTD

.PHONY: all build bench compare report clean help

# Configuration
SCENARIO ?= all
IMPL ?= both
DURATION ?= 300
OUTPUT_DIR ?= results

# Daemon paths
ARGUSD_C := ../daemons/argusd/c/build/argusd
ARGUSD_RUST := ../daemons/argusd/rust/target/release/argusd
JANUSD_C := ../daemons/janusd/c/build/janusd
JANUSD_RUST := ../daemons/janusd/rust/target/release/janusd

# Event generator
EVENT_GEN := src/event-generator

all: build bench report

help:
	@echo "Panoptes Benchmark Suite"
	@echo ""
	@echo "Usage: make <target> [OPTION=value]"
	@echo ""
	@echo "Targets:"
	@echo "  build      Build all components (daemons + event generator)"
	@echo "  bench      Run benchmarks"
	@echo "  compare    Generate comparison between C and Rust implementations"
	@echo "  report     Generate HTML report from results"
	@echo "  clean      Clean build artifacts and results"
	@echo ""
	@echo "Options:"
	@echo "  SCENARIO   Scenario to run (high-event-rate, deep-recursion, large-file-count, mixed-workload, all)"
	@echo "  IMPL       Implementation to test (c, rust, both)"
	@echo "  DURATION   Test duration in seconds (default: 300)"
	@echo "  OUTPUT_DIR Results output directory (default: results)"
	@echo ""
	@echo "Examples:"
	@echo "  make bench SCENARIO=high-event-rate IMPL=c DURATION=600"
	@echo "  make compare"
	@echo "  make report"

# Build all components
build: build-daemons build-event-generator

build-daemons:
	@echo "Building C daemons..."
	cd ../daemons/argusd/c && cmake -B build -DCMAKE_BUILD_TYPE=Release && cmake --build build
	cd ../daemons/janusd/c && cmake -B build -DCMAKE_BUILD_TYPE=Release && cmake --build build
	@echo "Building Rust daemons..."
	cd ../daemons/argusd/rust && cargo build --release
	cd ../daemons/janusd/rust && cargo build --release

build-event-generator:
	@echo "Building event generator..."
	$(CC) -O2 -o $(EVENT_GEN) src/event-generator.c -lpthread

# Run benchmarks
bench:
	@mkdir -p $(OUTPUT_DIR)
	@echo "Running benchmarks..."
	@echo "  Scenario: $(SCENARIO)"
	@echo "  Implementation: $(IMPL)"
	@echo "  Duration: $(DURATION)s"
	./scripts/run-benchmarks.sh \
		--scenario $(SCENARIO) \
		--impl $(IMPL) \
		--duration $(DURATION) \
		--output $(OUTPUT_DIR)

# Benchmark scenarios
bench-high-event-rate:
	$(MAKE) bench SCENARIO=high-event-rate

bench-deep-recursion:
	$(MAKE) bench SCENARIO=deep-recursion

bench-large-file-count:
	$(MAKE) bench SCENARIO=large-file-count

bench-mixed-workload:
	$(MAKE) bench SCENARIO=mixed-workload

bench-sustained-load:
	$(MAKE) bench SCENARIO=sustained-load DURATION=86400

# Quick benchmarks (shorter duration for testing)
bench-quick:
	$(MAKE) bench DURATION=60

# Compare implementations
compare:
	@echo "Generating comparison report..."
	./scripts/compare-impls.sh $(OUTPUT_DIR)

# Generate report
report:
	@echo "Generating HTML report..."
	@if command -v python3 >/dev/null 2>&1; then \
		python3 scripts/generate-report.py --input $(OUTPUT_DIR) --format html; \
	else \
		echo "Python3 not found. Install it to generate reports."; \
	fi

# Clean
clean:
	rm -f $(EVENT_GEN)
	rm -rf $(OUTPUT_DIR)/*

clean-all: clean
	cd ../daemons/argusd/c && rm -rf build
	cd ../daemons/janusd/c && rm -rf build
	cd ../daemons/argusd/rust && cargo clean
	cd ../daemons/janusd/rust && cargo clean
