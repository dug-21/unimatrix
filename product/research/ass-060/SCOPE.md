# ASS-060: Multi-Project Data Architecture for Containerized Deployment

**Date**: 2026-05-27
**Status**: Scoped
**Depends on**: None (research spike)
**Feeds**: W2-1 (container packaging), W2-2 (HTTP transport), enterprise multi-tenant design
**Breadth**: code-only
**Approach**: investigation
**Confidence**: directional

---

## Question

How should Unimatrix manage N independent project knowledge bases in a single containerized instance, such that the data model is the OSS subset of enterprise multi-tenant and supports local→cloud migration?

## Why This Matters

Wave 2 is delivering container packaging and HTTPS transport for a single-project model. A developer running Unimatrix in the cloud will have multiple projects (research repo, dev repo, etc.) hitting the same instance. The current architecture (ADR-004 path-hash isolation, single-project daemon, UDS transport) has no concept of project routing over HTTP. Getting the data model wrong here creates a migration wall between OSS and enterprise.

The per-project repo model is the natural fit — each project maintains a separate knowledge base with its own integrity chain. This spike designs that model as the OSS subset of what enterprise multi-tenant will provide with stronger isolation guarantees.

## Bounded Questions

### Q1: Project Identity in a Container

ADR-004 uses `SHA-256(project_root_path)` for project isolation — no equivalent concept in a container where there is no local project root. What replaces it?

Options to evaluate:
- Explicit project slug in `config.toml`
- Client-declared project header per request
- Path-prefix routing (`/v1/{project}/tools/...`)

Evaluate each against: determinism, collision safety, ergonomics, and enterprise upgrade path (JWT `unimatrix_project` claim from W2-3 vision spec).

### Q2: Volume Layout for N Projects

Current W2-1 spec: 3 named volumes for one project. With N projects:
- N separate volume sets (maximum isolation, complex compose)
- Subdirectories within one volume (simpler ops, shared backup blast radius)
- One DB with schema-level isolation (violates integrity chain per-project)

Evaluate trade-offs. Consider backup granularity — can I back up one project without the others?

### Q3: Request Routing

How does the HTTP server resolve which project store handles a request? Options:
- Header-based (`X-Unimatrix-Project`)
- Path-prefix (`/v1/{project-slug}/tools/...`)
- Per-project bearer token

Must compose with StaticTokenAuth (W2-3 delivered). Must be forward-compatible with enterprise JWT `unimatrix_project` claim.

### Q4: Local→Cloud Migration Path

A developer has `~/.unimatrix/{hash}/` locally. They spin up a cloud instance. What's the import path?

Addresses #631 (export/import missing GRAPH_EDGES + observations). Evaluate:
- Full DB file copy via volume mount
- CLI export/import with complete data fidelity
- API-driven sync

### Q5: Cross-Project Knowledge Sharing (Scoping Decision)

The vision doc describes an "owner store" for cross-project conventions. Is this in scope for OSS or enterprise-only? A developer with research + dev projects may want patterns from one visible in the other.

Evaluate:
- No sharing (simplest, cleanest isolation)
- Read-only fan-out at query time (vision doc's model)
- Explicit promotion (attributed, hash-chained)

Recommend where the OSS boundary sits.

### Q6: Enterprise Compatibility Contract

Define what must be true about the OSS data model so that enterprise multi-tenant (OAuth, RBAC, per-tenant isolation) is an additive layer, not a rewrite. What invariants does the OSS model establish that enterprise inherits?

## Expected Output

- Recommendation with clear decision rationale for each question
- Architecture diagram showing the container's internal project routing
- Proposed config surface for project registration
- Migration path spec (local→cloud)
- Explicit list of what enterprise adds vs. what OSS establishes

## Known Constraints and Prior Art

- ADR-004 (#80): Path-hash project isolation — `SHA-256(project_root)[..16]`
- W2-3 (#4556): StaticTokenAuth delivered — bearer token = full access, no per-project scoping
- Vision doc W2-3: `TenantRouter`, `unimatrix_project` JWT claim, owner+project two-tier store
- #631: Export/import incomplete (GRAPH_EDGES, observations missing)
- #637: Co-locate per-project config.toml with project data directory (open enhancement)
- Single-binary principle: no new services
- ADR-004 CI (#4572): Container CI jobs independent of binary/npm release jobs
