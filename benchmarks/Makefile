# Panoptes Benchmark Suite
# Copyright 2026 Como Technologies, LTD

.PHONY: all build bench compare report clean help \
       build-containers bench-containers compare-containers image-sizes

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
	@echo "Local Binary Targets:"
	@echo "  build      Build all components (daemons + event generator)"
	@echo "  bench      Run benchmarks against local binaries"
	@echo "  compare    Generate comparison between C and Rust implementations"
	@echo "  report     Generate HTML report from results"
	@echo ""
	@echo "Container Targets:"
	@echo "  build-containers      Build all Docker images"
	@echo "  bench-containers      Run benchmarks in containers"
	@echo "  compare-containers    Compare containerized implementations"
	@echo "  image-sizes           Show Docker image sizes"
	@echo "  containers-all        Full container workflow (build + bench + compare)"
	@echo ""
	@echo "Cleanup Targets:"
	@echo "  clean                 Clean build artifacts and results"
	@echo "  clean-containers      Remove Docker images and volumes"
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
	@echo "  make build-containers"
	@echo "  make bench-containers SCENARIO=high-event-rate DURATION=60"
	@echo "  make containers-all"

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

# ============================================
# Container-based benchmarks
# ============================================

# Build all container images (daemons + event generator)
build-containers: build-event-generator-container
	@echo "Building daemon container images..."
	./scripts/build-containers.sh

# Build specific container image
build-container-%:
	@echo "Building $* container..."
	./scripts/build-containers.sh $*

# Run container benchmarks
bench-containers:
	@echo "Running container benchmarks..."
	./scripts/run-container-bench.sh \
		--scenario $(SCENARIO) \
		--impl $(IMPL) \
		--duration $(DURATION)

# Quick container benchmark (shorter duration)
bench-containers-quick:
	$(MAKE) bench-containers DURATION=30

# Compare container implementations
compare-containers:
	@echo "Generating container comparison..."
	./scripts/compare-impls.sh $(OUTPUT_DIR) --container

# Report image sizes
image-sizes:
	@echo "Docker Image Sizes:"
	@echo "==================="
	@docker images --format "table {{.Repository}}:{{.Tag}}\t{{.Size}}" | grep panoptes || echo "No panoptes images found"
	@echo ""
	@if [ -f $(OUTPUT_DIR)/image-sizes.json ]; then \
		echo "Detailed sizes in: $(OUTPUT_DIR)/image-sizes.json"; \
		cat $(OUTPUT_DIR)/image-sizes.json; \
	fi

# Build event generator container (used for container benchmarks)
build-event-generator-container:
	@echo "Building event generator container..."
	docker build -t panoptes/event-generator:latest -f Dockerfile.event-generator .

# Full container workflow
containers-all: build-containers bench-containers compare-containers
	@echo "Container benchmark workflow complete!"

# ============================================
# Cleanup
# ============================================

# Clean
clean:
	rm -f $(EVENT_GEN)
	rm -rf $(OUTPUT_DIR)/*

clean-all: clean
	cd ../daemons/argusd/c && rm -rf build
	cd ../daemons/janusd/c && rm -rf build
	cd ../daemons/argusd/rust && cargo clean
	cd ../daemons/janusd/rust && cargo clean

clean-containers:
	@echo "Cleaning container images..."
	docker rmi panoptes/argusd:c panoptes/argusd:rust panoptes/janusd:c panoptes/janusd:rust panoptes/event-generator:latest 2>/dev/null || true
	docker volume rm panoptes_bench-data 2>/dev/null || true
