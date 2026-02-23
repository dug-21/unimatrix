# Alignment Report: nxs-003

> Reviewed: 2026-02-23
> Artifacts reviewed:
>   - product/features/nxs-003/architecture/ARCHITECTURE.md
>   - product/features/nxs-003/specification/SPECIFICATION.md
>   - product/features/nxs-003/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | WARN | API-based fallback excluded; vision text mentions it but SCOPE deliberately deferred |
| Milestone Fit | PASS | Squarely M1 Foundation; no premature M2+ capabilities |
| Scope Gaps | PASS | All 10 SCOPE goals and 19 ACs addressed in source documents |
| Scope Additions | PASS | No unauthorized additions beyond SCOPE |
| Architecture Consistency | WARN | Error enum variants differ between Architecture and Specification |
| Risk Completeness | PASS | 15 risks, 5 integration risks, 4 security risks, ~114 test scenarios |

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Simplification | API-based fallback | Vision says "ONNX runtime or API-based fallback." SCOPE resolved OQ-03: no API fallback, multiple local 384-d models instead. Human-approved. Rationale: local-first philosophy, 7 model options provide variety without network dependency. |
| Simplification | thiserror 2.0 | First crate in workspace to use thiserror (nxs-001/nxs-002 use manual Error impls). SCOPE explicitly specifies thiserror 2.0. Minor convention divergence. |

## Variances Requiring Approval

None. Both WARN items are human-approved simplifications documented in SCOPE.md. No VARIANCE or FAIL classifications.

## Detailed Findings

### Vision Alignment

**Product Vision M1 goal**: "Ship a working knowledge store that agents can read from and write to via MCP."

nxs-003 provides the embedding primitive that connects content authoring (nxs-001 `Store::insert`) to vector search (nxs-002 `VectorIndex::insert`). Without nxs-003, entries can be stored but never made semantically searchable. This directly serves the M1 goal.

**Vision roadmap entry for nxs-003**: "Local embedding generation via ONNX runtime or API-based fallback. Title+content concatenation strategy. Batch embedding on import."

The source documents deliver:
- Local embedding via ONNX Runtime (ort + tokenizers + hf-hub): Architecture Section 3 (OnnxProvider), Specification FR-05.
- Title+content concatenation: Architecture Section 6 (Text Module), Specification FR-10.
- Batch embedding: Architecture Section 3 (embed_batch flow), Specification FR-05.4.

**W1: API-based fallback excluded.** The vision mentions "or API-based fallback" but SCOPE.md explicitly lists "No API-based embedding providers" as a non-goal, resolving OQ-03 as "No API fallback. Support multiple local 384-d models instead." The specification echoes this in Section 9. This was a deliberate human-approved scope decision.

*Recommendation*: Accept. Update PRODUCT-VISION.md nxs-003 entry to reflect the resolved decision (e.g., "Local embedding generation via ONNX runtime with pre-configured model catalog").

**Strategic approach alignment**: The vision says "Start with Proposal A (Knowledge Oracle) -- a focused, testable knowledge store." nxs-003 fits: it's a focused embedding primitive, no workflow features, no learning features. The `EmbeddingProvider` trait enables future provider implementations without changing downstream consumers, supporting incremental evolution.

### Milestone Fit

nxs-003 is correctly scoped to M1 (Nexus/Foundation):

- **No MCP exposure** (M2 territory): Specification Section 9 explicitly excludes MCP tools.
- **No usage tracking** (M4 territory): No logging or helpfulness signals.
- **No agent-specific behavior** (M3 territory): The provider is agent-agnostic.
- **No GPU inference** (future iteration): CPU-only, matching M1's "functional storage + retrieval backend" goal.
- **No async runtime** (consistent with nxs-001/nxs-002): Synchronous API, matching the M1 pattern where async wrapping is deferred to vnc-001.

The feature builds exactly what M1 needs (embedding primitive) and defers everything else. No premature capabilities.

### Architecture Review

The architecture is well-structured with 11 modules, clear separation of concerns, and explicit integration surfaces.

**Strengths**:
- Standalone crate with no dependency on unimatrix-store or unimatrix-vector. Integration at the caller level keeps dependency graph clean (Architecture: "Cargo Workspace Integration" section).
- 4 ADRs document key decisions: Mutex concurrency (ADR-001), raw ort over fastembed (ADR-002), hf-hub downloads (ADR-003), custom cache directory (ADR-004).
- Concurrency model is clear: Mutex on Session, lock-free Tokenizer. The `Arc<OnnxProvider>` sharing pattern is documented with a code example.
- Full write and search path integration flows are documented, showing exactly how vnc-002 will use this crate.

**W2: Error enum inconsistency between Architecture and Specification.**

Architecture `error.rs` defines 7 variants:
```
OnnxRuntime, Tokenizer, Download, ModelNotFound { path: PathBuf },
Io, DimensionMismatch, EmptyInput(String)
```

Specification FR-12.1 defines 6 variants:
```
OnnxRuntime, Tokenizer, Download, ModelLoad(String),
DimensionMismatch, Io
```

Differences:
1. Architecture has `ModelNotFound { path: PathBuf }`, Specification has `ModelLoad(String)`. Different name, different payload type.
2. Architecture has `EmptyInput(String)`, Specification does not.

The `ModelNotFound` vs `ModelLoad` difference is structural: Architecture models it as a typed path, Specification as a generic string. The `EmptyInput` variant in Architecture has no corresponding specification requirement -- AC-12 says empty strings return valid embeddings (not errors), and `embed_batch(&[])` behavior is unspecified.

*Recommendation*: Resolve during implementation. The Specification's `ModelLoad(String)` is more flexible (covers both "file not found" and "file corrupt" cases). The `EmptyInput` variant should be dropped unless batch_size=0 handling needs it, which should be clarified.

### Specification Review

The specification is thorough at 784 lines, covering 13 functional requirements, 6 non-functional requirements, 19 acceptance criteria with verification methods, domain models, 6 user workflows, and constraints.

**Strengths**:
- Every SCOPE AC is restated with explicit verification methods (Section 4).
- FR numbering maps cleanly to SCOPE goals.
- Section 11 "Key Specification Decisions" documents 10 design rationale points, providing traceability.
- The "NOT in Scope" section (Section 9) mirrors SCOPE non-goals exactly.
- Open questions (Section 10) are flagged for architect/implementer resolution, not left ambiguous.

**No gaps detected.** All 10 SCOPE goals and 19 ACs are addressed:
- Goals 1-10 map to FR-01 through FR-13.
- ACs 01-19 are restated in Section 4 with verification methods.
- Non-goals are echoed in Section 9.
- Constraints are tabulated in Section 7.

### Risk Strategy Review

The RISK-TEST-STRATEGY.md is comprehensive:

| Category | Count | Detail |
|----------|-------|--------|
| Functional risks | 15 (R-01 through R-15) | Severity/likelihood/priority rated |
| Integration risks | 5 (IR-01 through IR-05) | Cross-crate boundary concerns |
| Edge case groups | 5 (EC-01 through EC-05) | Unicode, tokenizer, batch, float, config |
| Security risks | 4 (SR-01 through SR-04) | Model supply chain, input, cache, network |
| Failure modes | 6 (FM-01 through FM-06) | Recovery paths documented |
| Total test scenarios | ~114 | Prioritized execution order |

**Strengths**:
- R-01 (L2 normalization) and R-02 (mean pooling) correctly identified as Critical -- these are foundational to DistDot correctness in nxs-002.
- Test priority order (Section 7) starts with normalization, then pooling, then batch consistency -- the right order for building confidence in the pipeline.
- Two-tier test strategy (unit tests without model, integration tests with model) is pragmatic -- enables CI without large model downloads for fast feedback.
- Security risks appropriately scoped: model supply chain acknowledged, mitigations documented (catalog restriction, HTTPS), and checksum verification explicitly deferred with rationale.
- IR-03 (DistDot normalization dependency) correctly elevates R-01's importance from a quality issue to a correctness issue for the search pipeline.

**No gaps detected.** The risk strategy covers:
- All critical paths: normalization, pooling, batch consistency, model loading, thread safety.
- All edge cases identified in SCOPE: empty input (AC-12), concatenation (AC-06), batch boundaries (AC-04/AC-11).
- Cross-crate integration: dimension contract (IR-01), NaN rejection (IR-02), DistDot dependency (IR-03).
- Operational concerns: download failure (R-05/FM-01), cache corruption (FM-02), concurrent construction (FM-06).
