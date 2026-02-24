# Panoptes Project Governance

## Overview

Panoptes is maintained by Como Technologies, LTD. We welcome contributions from the community and aim to make the governance process transparent.

## Roles

### Maintainers

Maintainers have full commit access and are responsible for:

- Reviewing and merging pull requests
- Triaging issues
- Making architectural decisions
- Managing releases

**Current Maintainers:**

| Name | GitHub | Focus Area |
|------|--------|------------|
| Como Technologies | [@como-technologies](https://github.com/como-technologies) | All |

### Contributors

Anyone who submits a pull request, files an issue, or participates in discussions is a contributor. Contributors are expected to follow the [Code of Conduct](CODE_OF_CONDUCT.md).

## Decision Making

### Day-to-Day Decisions

Maintainers make day-to-day decisions about code changes, issue triage, and minor feature additions through PR reviews.

### Architectural Decisions

Significant architectural changes are documented as Architecture Decision Records (ADRs) in `docs/adr/`. These are proposed via pull request and require maintainer approval.

### Feature Requests

Feature requests are discussed in GitHub Issues. Features that align with the project's [philosophy](README.md#philosophy) (kernel-level detection, Kubernetes-native, transparent, composable) are prioritized.

## Releases

Releases follow [Semantic Versioning](https://semver.org/):

- **Major** (X.0.0): Breaking API changes to CRDs
- **Minor** (0.X.0): New features, new compliance templates
- **Patch** (0.0.X): Bug fixes, documentation updates

Release cadence is as-needed rather than time-based.

## Becoming a Maintainer

Active contributors who demonstrate sustained, high-quality contributions may be invited to become maintainers. There is no formal process yet — reach out if you're interested.

## Contact

- **General questions**: [GitHub Discussions](https://github.com/como-technologies/panoptes/discussions)
- **Security issues**: security@comotech.io (see [vulnerability response](docs/security/vulnerability-response.md))
- **Maintainer contact**: security@comotech.io
