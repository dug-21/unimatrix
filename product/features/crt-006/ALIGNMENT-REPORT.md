# Alignment Report: crt-006

> Reviewed: 2026-02-28
> Artifacts reviewed:
>   - product/features/crt-006/architecture/ARCHITECTURE.md
>   - product/features/crt-006/specification/SPECIFICATION.md
>   - product/features/crt-006/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Adaptive embedding is explicitly in the M4 roadmap (crt-006 entry). Architecture matches vision description. |
| Milestone Fit | PASS | Correctly placed in Milestone 4 (Cortical Phase). Dependencies satisfied: crt-004 (co-access data) and crt-005 (coherence gate) are complete. |
| Scope Gaps | PASS | All 39 acceptance criteria from SCOPE.md are present in the specification. All 10 goals are addressed in architecture. |
| Scope Additions | PASS | No additions beyond SCOPE.md. Architecture and specification stay within defined boundaries. |
| Architecture Consistency | PASS | Architecture follows established patterns: separate crate, trait-based integration, Arc-wrapped shared state, spawn_blocking for sync operations. |
| Risk Completeness | PASS | 13 risks identified with 41 test scenarios. All 9 scope risks (SR-01 through SR-09) traced in the Scope Risk Traceability table. Security risks assessed. |

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| (none) | No gaps or additions detected | All SCOPE.md goals map to architecture components; all AC-IDs appear in specification |

## Variances Requiring Approval

None. All source documents align with the product vision and approved scope.

## Detailed Findings

### Vision Alignment

The product vision (PRODUCT-VISION.md) describes crt-006 as:

> "4-stage adaptation pipeline on frozen ONNX embeddings: MicroLoRA (rank 2-8, ~3K params for rank=4 on 384d) -> Prototype Adjustment -> Episodic Augmentation -> adapted 384d vector."

The architecture implements exactly this pipeline with the following refinements from the approved SCOPE.md:
- Rank range expanded from 2-8 to 2-16 (SCOPE.md decision, based on scale analysis)
- Training buffer increased from 16 to configurable 512 with reservoir sampling (SCOPE.md decision, based on scale analysis)
- EWC++ added (SCOPE.md decision, for long-lived learning)

These refinements are consistent with the vision's intent ("Adaptive Embedding") and were explicitly approved in SCOPE.md. They do not represent scope additions.

The vision also states:
> "crt-005 embedding consistency dimension (0.99 self-similarity threshold) must compare against adapted embeddings, not raw re-embeds"

This is addressed in SCOPE.md Goal 8, architecture Component Interaction (coherence path), and specification FR-16.

### Milestone Fit

crt-006 is correctly sequenced in Milestone 4 (Cortical Phase):
- **Depends on**: crt-004 (co-access data as training signal) -- COMPLETE, crt-005 (coherence gate for consistency checks) -- COMPLETE
- **Blocks**: col-002 (retrospective pipeline benefits from better embeddings)
- **Does not build**: M5+ capabilities (no workflow orchestration, no process proposals)

### Architecture Review

The architecture follows established project patterns:
- **New crate pattern**: `unimatrix-adapt` follows the same pattern as `unimatrix-store`, `unimatrix-vector`, `unimatrix-embed`, `unimatrix-core`. Workspace member, edition 2024, MSRV 1.89.
- **Dependency graph**: Clean -- depends on `unimatrix-store` for shared types only. Does not create circular dependencies.
- **Trait consistency**: No changes to existing traits (`EmbeddingProvider`, `EmbedService`, `VectorStore`, `EntryStore`). The adaptation is orchestrated at the server level, not injected into trait abstractions.
- **Concurrency pattern**: RwLock for concurrent read access with atomic swap for writes, consistent with existing patterns (VectorIndex uses RwLock internally).
- **Persistence pattern**: Independent file alongside HNSW dump, consistent with existing persistence (redb, HNSW index are already independently persisted).

ADRs are concrete and well-reasoned:
- ADR-001 (ndarray): Justified by gradient computation needs. Alternatives documented.
- ADR-002 (bincode v2): Consistent with workspace convention. Forward compatibility via serde(default).
- ADR-003 (independent persistence): Addresses SR-06. Graceful degradation designed.
- ADR-004 (RwLock + atomic swap): Addresses SR-05. Standard concurrency pattern.

### Specification Review

All 39 acceptance criteria from SCOPE.md are present in the specification's AC table with verification methods. Functional requirements are testable. Non-functional requirements include measurable targets (NFR-01: < 10 microseconds, NFR-02: < 50ms, NFR-03: < 500KB). Domain models define all key entities. "NOT in scope" section is explicit and matches SCOPE.md non-goals.

### Risk Strategy Review

The risk strategy identifies 13 feature-level risks across gradient correctness, numerical stability, training regression, state persistence, concurrency, sampling bias, and build compatibility. All 9 scope-level risks (SR-01 through SR-09) are traced to architecture-level risks. Security section covers the attack surface. Edge cases include empty database, single entry, rank change, concurrent operations, and Unicode content.

The risk with highest practical concern is R-10 (embedding consistency false positives due to training between index and check time). This is correctly identified with a mitigation (weight snapshot during consistency check batch). The risk strategy should be reviewed if the actual false positive rate is higher than expected in practice.
