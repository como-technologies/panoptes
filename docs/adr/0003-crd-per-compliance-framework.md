# ADR-0003: CRD Instances Per Compliance Framework

## Status

Accepted

## Context

Panoptes provides pre-built compliance templates for PCI-DSS, HIPAA, SOC2, CIS Kubernetes, NIST 800-53, GDPR, and a base security baseline. We needed to decide how to structure these templates:

1. **One CRD instance per framework** — Each compliance framework gets its own ArgusWatcher + JanusGuard pair with framework-specific paths, events, and tags
2. **Single mega-CRD** — One ArgusWatcher with all paths from all frameworks combined
3. **CRD-per-control** — Individual CRDs for each compliance control (e.g., one for PCI-DSS 10.5.5, another for PCI-DSS 11.5)

## Decision

We chose **one CRD instance pair (ArgusWatcher + JanusGuard) per compliance framework**.

## Rationale

**Why per-framework:**

- **Independent lifecycle**: Enable/disable PCI-DSS without affecting HIPAA monitoring. Organizations rarely need all frameworks simultaneously — they enable what their audit requires.
- **Clear ownership**: `kubectl get aw -l compliance=pci-dss` shows exactly what's monitoring PCI-DSS. Each CRD carries annotations mapping to specific control IDs (e.g., `pci-dss/requirements: "10.5.5, 11.5"`).
- **Auditor-friendly**: Compliance officers can see "PCI-DSS monitoring is active" as a single resource with a single status. They don't need to understand Kubernetes to verify coverage.
- **Tag isolation**: Each subject within a CRD carries framework-specific tags (`requirement: "11.5"`, `severity: critical`, `category: user-accounts`). Events are automatically tagged with the compliance context.
- **Helm integration**: `--set compliance.pciDss.enabled=true` is a clean, discoverable interface. Each framework maps to a single conditional template.

**Why not mega-CRD:**

- Impossible to disable one framework without affecting others
- Tag conflicts between frameworks (same path, different compliance meanings)
- Status becomes meaningless ("10/200 paths watching" vs. "PCI-DSS: all paths active")
- Blast radius: a misconfiguration affects all compliance monitoring

**Why not per-control:**

- Excessive CRD proliferation (PCI-DSS alone has 10+ relevant controls)
- Operational overhead: `kubectl get aw` returns 50+ resources
- Many controls monitor overlapping paths — deduplication becomes complex
- Auditors think in frameworks, not individual controls

## Consequences

- Each framework adds 2 CRD instances (ArgusWatcher + JanusGuard) to the cluster
- Organizations running multiple frameworks will have some path overlap between CRDs (e.g., both PCI-DSS and SOC2 monitor /etc/passwd) — this is acceptable as inotify deduplicates at the kernel level
- New framework templates are added as new Helm conditional templates in `charts/panoptes/templates/compliance/`
- Pod selector labels are framework-specific (e.g., `pci-dss/scope: in-scope`) allowing per-framework workload targeting
