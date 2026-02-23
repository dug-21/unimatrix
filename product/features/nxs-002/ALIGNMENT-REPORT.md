# Alignment Report: nxs-002

> Reviewed: 2026-02-22
> Artifacts reviewed:
>   - product/features/nxs-002/architecture/ARCHITECTURE.md
>   - product/features/nxs-002/specification/SPECIFICATION.md
>   - product/features/nxs-002/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Implements nxs-002 as defined in M1 roadmap |
| Milestone Fit | PASS | Stays within M1 Foundation scope |
| Scope Gaps | WARN | W1: VECTOR_MAP iteration method needed but not in nxs-001 scope |
| Scope Additions | WARN | W2: NaN/infinity validation (EC-08) not in SCOPE.md but recommended |
| Architecture Consistency | PASS | Consistent with nxs-001 patterns |
| Risk Completeness | PASS | 12 risks + 4 integration risks + 10 edge cases |

---

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Gap | `Store::iter_vector_mappings()` | Architecture OQ-2 and Specification OQ-1 identify the need for a VECTOR_MAP iteration method in unimatrix-store. SCOPE.md does not mention this as a deliverable. This is a minor extension to nxs-001 required for nxs-002's load path. |
| Addition | NaN/infinity embedding validation | Risk Strategy EC-08 recommends validating embeddings for NaN/infinity values and adding `VectorError::InvalidEmbedding`. SCOPE.md AC-06 only covers dimension mismatch. This is a sensible safety addition. |
| Simplification | No batch insert API | Specification Section 9 explicitly excludes batch insert. SCOPE.md does not mention it. Caller loops over individual inserts. Acceptable -- parallel_insert can be added later without API change. |

---

## Variances Requiring Approval

### W1: VECTOR_MAP Iteration Method (WARN)

**What**: The `load` path needs to iterate all VECTOR_MAP entries to rebuild the bidirectional IdMap. unimatrix-store (nxs-001) does not currently expose this method. The architecture and specification both flag this as an open question and recommend adding `Store::iter_vector_mappings() -> Result<Vec<(u64, u64)>>`.

**Why it matters**: This is a minor scope addition to nxs-001 (adding one public method to an existing crate). It does not change nxs-001's architecture or data model -- it reads existing data from an existing table. However, it was not in nxs-001's original scope.

**Recommendation**: Accept. This is a natural extension required by the documented downstream integration contract in nxs-001's ARCHITECTURE.md (Section: "Downstream Integration (nxs-002 Vector Index)"). Add the method during nxs-002 implementation. It is a single read-only function with no side effects.

### W2: NaN/Infinity Embedding Validation (WARN)

**What**: The Risk Strategy (EC-08) recommends validating embedding vectors for NaN and infinity values, returning a `VectorError::InvalidEmbedding` error. SCOPE.md AC-06 only specifies dimension mismatch validation.

**Why it matters**: NaN values in embeddings would cause hnsw_rs to produce nonsensical distance computations silently. This is a reasonable safety check that prevents hard-to-debug search quality issues.

**Recommendation**: Accept. Add NaN/infinity validation alongside dimension validation in the insert path. This is a small addition that prevents silent corruption.

---

## Detailed Findings

### Vision Alignment

The product vision defines nxs-002 as: "hnsw_rs integration -- 384-dimension embeddings (all-MiniLM-L6-v2), DistDot, 16 max connections, ef_construction=200. VECTOR_MAP bridge table between entry IDs and hnsw data IDs."

**Architecture**: Implements exactly this. `Hnsw<f32, DistDot>` with dimension=384, max_nb_connection=16, ef_construction=200. VECTOR_MAP coordination via `Store::put_vector_mapping`. PASS.

**Specification**: Functional requirements (FR-01 through FR-07) cover all vision requirements. Search, filtered search, persistence, and lifecycle management are all specified. PASS.

**Risk Strategy**: Risks cover the key integration points between hnsw_rs and redb (R-01 IdMap desync, R-04 persistence). PASS.

The vision note about vnc-001 coordinating shutdown (`Store::compact()` + `VectorIndex::dump()`) is acknowledged in SCOPE.md OQ-2 resolution and Architecture's downstream integration section. PASS.

### Milestone Fit

nxs-002 is the second feature in M1 (Foundation). It depends on nxs-001 (implemented) and is consumed by nxs-003 (embedding pipeline) and vnc-002 (MCP tools).

The architecture correctly scopes to M1: synchronous API, no MCP exposure, no async runtime. No M2+ capabilities are introduced. PASS.

The architecture anticipates downstream consumers (vnc-001, vnc-002, nxs-003) with integration surface documentation, which is appropriate for design documents. These are informational references, not implementations. PASS.

### Architecture Review

**ADR-001 (hnsw_rs)**: Justified by ASS-001 spike research. Alternatives evaluated. Decision is consistent with product vision. PASS.

**ADR-002 (DistDot)**: Correctly identified as faster than DistCosine for normalized vectors. Requirement that embeddings be L2-normalized is documented. PASS.

**ADR-003 (RwLock)**: Addresses the `set_searching_mode(&mut self)` constraint. Lock ordering is documented (hnsw lock acquired before IdMap lock in insert; only one at a time in search). No nested lock risk identified. PASS.

**ADR-004 (Bidirectional IdMap)**: Memory overhead analysis is sound (~32 bytes/entry vs ~1.8 KB/entry for hnsw_rs at 384d). Re-embedding behavior is documented with correct analysis of stale points. PASS.

**Integration Surface**: All integration points with unimatrix-store are documented with exact method signatures. hnsw_rs API surface is mapped. PASS.

### Specification Review

**AC coverage**: All 18 acceptance criteria from SCOPE.md are present in the Specification's AC table with verification methods. PASS.

**Functional requirements**: FR-01 through FR-07 cover index creation, insertion, search, filtered search, persistence, and inspection. Each is testable. PASS.

**Non-functional requirements**: Performance targets (insert < 1ms, search < 10ms at 10K) are derived from hnsw_rs benchmarks. Memory estimates match spike research. PASS.

**Domain models**: Key entities defined. Ubiquitous language section establishes clear terminology (embedding, similarity, distance, data ID, allow-list, stale point, mode transition). PASS.

**User workflows**: Seven workflows cover the complete lifecycle from setup through crash recovery. PASS.

### Risk Strategy Review

**Risk completeness**: 12 risks covering data integrity (R-01, R-02, R-06), correctness (R-03, R-08), persistence (R-04, R-11), concurrency (R-05), validation (R-02, R-09), and API misuse (R-10). Plus 4 integration risks and 10 edge cases. PASS.

**Priority ordering**: R-02 (dimension mismatch) is correctly identified as "test FIRST" -- if dimension validation fails, all other tests are on corrupted data. This matches the nxs-001 pattern where R4 (schema evolution) was tested first. PASS.

**Coverage**: ~75 test scenarios across all risk categories. This is proportional to the feature complexity. PASS.

**Integration risks**: IR-01 (VECTOR_MAP write failure during insert) correctly identifies the failure mode and its benign consequences. IR-02 (VECTOR_MAP iteration) correctly flags the missing method. PASS.

**Edge cases**: EC-08 (NaN in embedding) recommends validation that goes beyond SCOPE.md AC-06. This is flagged as W2 above. Otherwise, edge cases are comprehensive. PASS.
