# ASS-061: Deployment Topology and Feature Gating

**Date**: 2026-05-27
**Status**: Scoped
**Depends on**: None (research spike)
**Feeds**: W2-1 (container packaging), W2-5 (GGUF), container UI decisions
**Breadth**: code+ecosystem
**Approach**: investigation
**Confidence required**: directional

---

## Question

How should Unimatrix gate cloud-only functionality, distribute ML models, and manage configuration discovery across the local (npm) and containerized deployment topologies — without fragmenting the codebase or violating the single-binary principle?

## Why This Matters

The container deployment will ship capabilities that don't belong in the local npm install — at minimum a different config discovery path, potentially a UI dashboard, and different model distribution strategies. Without a clear gating model, either the local binary accumulates unused cloud features or the container diverges into a separate codebase. Both outcomes are expensive. The single-binary principle ("all waves add capability to the same binary, not new services") needs an explicit ruling on where its boundary is.

## Bounded Questions

### Q1: Feature Gating Mechanism

How does cloud-only functionality get enabled/disabled? Options:

- **Cargo feature flags** — compile-time. Container build enables `features = ["dashboard", "infer"]`. npm binary builds without them. Clean separation but two build targets.
- **Runtime config** — single binary, config.toml enables/disables features. Simpler build matrix but ships unused code.
- **Separate crates composed at Docker layer** — dashboard is a standalone binary, composed via docker-compose alongside the Unimatrix binary. Explicit service boundary.

Evaluate each against: build complexity, binary size for npm users, testability, and the single-binary principle.

### Q2: The UI/Dashboard Question

A monitoring dashboard or knowledge browser is a likely cloud-only surface. Is it:

- A feature-flagged static asset bundle served by the existing HTTP server (single-binary preserved)
- A separate container service (e.g., Grafana, custom SPA) composed via docker-compose (single-binary violated but operationally clean)
- Out of scope for the OSS container entirely (enterprise-only)

Evaluate: what's the minimum viable cloud-only UI, and where does it live architecturally? Consider that the HTTP transport (W2-2) already provides the data API — the UI question is about the presentation layer only.

### Q3: Configuration Discovery Across Topologies

Local mode: `config.toml` discovered via project data directory (`~/.unimatrix/{hash}/config.toml`) or project root. Container mode: volume-mounted, environment-variable-driven, or baked into image.

- How does `config.toml` discovery unify across both? (#637 is already open: co-locate per-project config.toml with project data directory)
- What config values are topology-specific (listen address, TLS paths, volume paths) vs. universal (categories, confidence weights, inference config)?
- Should there be a `config.container.toml` overlay pattern or environment variable overrides?

### Q4: ML Model Distribution

ONNX models (~85MB cross-encoder) and potential GGUF models (1-8GB) need different strategies per topology:

- **Local (npm)**: Currently downloaded on first use. Acceptable?
- **Container — baked into image**: Large image, infrequent model updates don't require image rebuild? Or do they?
- **Container — separate volume**: Smaller image, model management complexity. Who downloads/updates?
- **Container — init container pattern**: Sidecar downloads models before main container starts.

Evaluate image size impact, update cadence, air-gapped deployment support, and the SHA-256 hash-pinning requirement (ADR-002 #4570).

### Q5: Single-Binary Principle Boundary

The vision states "all waves add capability to the same binary, not new services." With a container deployment that may include a UI, health monitoring, and model management:

- Where exactly is the boundary? Is docker-compose with multiple containers a violation?
- Proposed rule: the Unimatrix knowledge engine is always one binary. Auxiliary services (UI, model fetcher, backup agent) are separate containers composed alongside it. Is this the right framing?
- Does this framing hold for enterprise too, or does enterprise need a different composition model?

## Expected Output

- Decision framework for feature gating (recommended approach with trade-offs)
- Architecture diagram showing both topologies (local npm vs. container) and what's shared vs. divergent
- Config discovery specification that works for both topologies
- Model distribution recommendation per topology
- Explicit single-binary boundary rule with examples of what's inside vs. outside

## Constraints

### Hard (technically fixed, changing requires rewriting shipped code)
- ADR-002 (#4570): ORT supply chain verification via SHA-256 gate — model integrity is non-negotiable

### Hypothesis (design positions, subject to challenge by researcher)
- Single-binary principle (PRODUCT-VISION.md): "all waves add capability to the same binary, not new services"
- W2-5 (GGUF): Dedicated rayon pool, Cargo feature flag `features = ["infer"]` already planned
- Container pattern (#4579): Three-stage cargo-chef Dockerfile already designed

## Prior Art

- #637 / nxs-013: Co-locate per-project config.toml with project data directory (merged)
- W2-2: HTTP transport adds the API surface the UI would consume
