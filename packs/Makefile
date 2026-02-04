# Panoptes Spectro Cloud Pack Build System
# Packages Helm charts, validates pack structure, and pushes to OCI registries.
#
# Usage:
#   make package         - Package all Helm charts and create ORAS-ready archives
#   make validate        - Validate pack structure against Palette requirements
#   make push REGISTRY=  - Push packs to an OCI registry
#   make clean           - Remove build artifacts

SHELL := /bin/bash
PACKS := panoptes panoptes-argus panoptes-janus
DIST_DIR := dist
VERSION := 2.0.0

.PHONY: all package validate push clean help

all: package validate

help: ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-20s\033[0m %s\n", $$1, $$2}'

$(DIST_DIR):
	mkdir -p $(DIST_DIR)

# ============================================================
# Package
# ============================================================

package: $(DIST_DIR) ## Package Helm charts as .tgz and create ORAS-ready archives
	@echo "==> Packaging Panoptes packs..."
	@for pack in $(PACKS); do \
		echo "--- Packaging $$pack ---"; \
		if [ -d "$$pack/charts" ]; then \
			for chart_dir in $$pack/charts/*/; do \
				chart_name=$$(basename $$chart_dir); \
				echo "  Packaging chart: $$chart_name"; \
				helm package "$$chart_dir" -d "$(DIST_DIR)/$$pack/charts/" 2>/dev/null || \
					echo "  Warning: helm package failed for $$chart_name (helm may not be installed)"; \
			done; \
		fi; \
		echo "  Creating ORAS archive..."; \
		tar czf "$(DIST_DIR)/$$pack-$(VERSION).tar.gz" \
			-C "$$pack" \
			pack.json values.yaml \
			$$([ -f "$$pack/presets.yaml" ] && echo "presets.yaml") \
			$$([ -f "$$pack/schema.yaml" ] && echo "schema.yaml") \
			$$([ -f "$$pack/logo.png" ] && echo "logo.png") \
			$$([ -d "$$pack/charts" ] && echo "charts") \
			2>/dev/null || echo "  Warning: archive creation had issues"; \
		echo "  Created: $(DIST_DIR)/$$pack-$(VERSION).tar.gz"; \
	done
	@echo "==> Packaging complete"

# ============================================================
# Validate
# ============================================================

validate: ## Validate pack structure against Palette requirements
	@echo "==> Validating Panoptes packs..."
	@errors=0; \
	for pack in $(PACKS); do \
		echo "--- Validating $$pack ---"; \
		if [ ! -f "$$pack/pack.json" ]; then \
			echo "  ERROR: Missing pack.json"; \
			errors=$$((errors + 1)); \
		else \
			echo "  OK: pack.json exists"; \
			if ! python3 -c "import json; json.load(open('$$pack/pack.json'))" 2>/dev/null; then \
				echo "  ERROR: pack.json is not valid JSON"; \
				errors=$$((errors + 1)); \
			fi; \
		fi; \
		if [ ! -f "$$pack/values.yaml" ]; then \
			echo "  ERROR: Missing values.yaml"; \
			errors=$$((errors + 1)); \
		else \
			echo "  OK: values.yaml exists"; \
		fi; \
		if [ ! -f "$$pack/presets.yaml" ]; then \
			echo "  WARN: Missing presets.yaml (presets won't appear in Palette UI)"; \
		else \
			echo "  OK: presets.yaml exists"; \
		fi; \
		if [ ! -f "$$pack/schema.yaml" ]; then \
			echo "  WARN: Missing schema.yaml (no value validation)"; \
		else \
			echo "  OK: schema.yaml exists"; \
		fi; \
		if [ ! -f "$$pack/logo.png" ]; then \
			echo "  WARN: Missing logo.png (pack will have no icon in Palette)"; \
		else \
			echo "  OK: logo.png exists"; \
		fi; \
		if [ ! -d "$$pack/charts" ]; then \
			echo "  ERROR: Missing charts/ directory"; \
			errors=$$((errors + 1)); \
		else \
			echo "  OK: charts/ directory exists"; \
		fi; \
	done; \
	echo ""; \
	if [ $$errors -gt 0 ]; then \
		echo "==> Validation FAILED with $$errors error(s)"; \
		exit 1; \
	else \
		echo "==> Validation PASSED"; \
	fi

# ============================================================
# Push
# ============================================================

ifndef REGISTRY
push:
	@echo "ERROR: REGISTRY is required. Usage: make push REGISTRY=myregistry.example.com/project"
	@exit 1
else
push: package ## Push packs to OCI registry (requires REGISTRY variable)
	@echo "==> Pushing packs to $(REGISTRY)..."
	@for pack in $(PACKS); do \
		echo "--- Pushing $$pack ---"; \
		oras push \
			"$(REGISTRY)/spectro-packs/archive/$$pack:$(VERSION)" \
			"$(DIST_DIR)/$$pack-$(VERSION).tar.gz" \
			|| echo "  ERROR: Failed to push $$pack"; \
	done
	@echo "==> Push complete. Sync your registry in Palette to see the new packs."
endif

# ============================================================
# Clean
# ============================================================

clean: ## Remove build artifacts
	rm -rf $(DIST_DIR)
	@echo "==> Clean complete"
