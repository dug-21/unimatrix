# Gate 3a Report: Component Design Review

**Feature**: crt-005 Coherence Gate
**Gate**: 3a (Component Design Review)
**Result**: PASS
**Date**: 2026-02-27

## Validation Summary

All 8 component pseudocode files and 8 component test plan files were reviewed against the three source documents (Architecture, Specification, Risk-Based Test Strategy). The Component Map in IMPLEMENTATION-BRIEF.md correctly reflects actual file paths on disk.

## Component-by-Component Review

### C1: Schema Migration (PASS)

**Pseudocode alignment**:
- V2EntryRecord struct matches Architecture C1 exactly (26 fields, confidence: f32)
- Migration follows established pattern from nxs-004 and crt-001 as specified
- Single write transaction with rollback on failure per Architecture
- CURRENT_SCHEMA_VERSION bumped to 3
- IEEE 754 lossless promotion (f32 as f64) per Architecture
- Empty database handling included

**Test plan coverage**:
- R-01 (migration failure): 6 integration test scenarios covering known values, boundary values, empty DB, idempotency, full chain v0->v3
- R-13 (V2EntryRecord mismatch): 3 unit test scenarios covering roundtrip, field order, serde(default)
- AC-25, AC-32 addressed

### C2: f64 Scoring Constants (PASS)

**Pseudocode alignment**:
- Exhaustive f32->f64 change inventory matches Architecture C2 table exactly
- All constants listed: W_BASE through W_COAC, SEARCH_SIMILARITY_WEIGHT, MAX_CO_ACCESS_BOOST, MAX_BRIEFING_CO_ACCESS_BOOST
- compute_confidence return type f64, removal of `as f32` specified
- rerank_score, co_access_affinity signature changes match Architecture
- Cast order in map_neighbours_to_results: `1.0_f64 - n.distance as f64` per ADR-001
- Test update strategy documented (~60-80 mechanical updates)

**Test plan coverage**:
- R-02 (residual f32): 8 test scenarios across confidence.rs, coaccess.rs, index.rs, write.rs
- R-04 (cast boundary): 3 scenarios for distance 0.0, 0.1, 1.0
- R-11 (regression): Full workspace test suite verification
- R-14 (weight sum): 3 scenarios for weight invariants
- R-17 (trait safety): Compile-time verification via Box<dyn VectorStore>
- AC-26 through AC-31 addressed

### C3: Vector Compaction (PASS)

**Pseudocode alignment**:
- compact method signature matches Architecture: `fn compact(&self, embeddings: Vec<(u64, Vec<f32>)>) -> Result<()>`
- Build-new-then-swap sequence per ADR-004
- VECTOR_MAP-first ordering per ADR-004 revised sequence (step 3 before step 4)
- Sequential data_ids starting from 0
- rewrite_vector_map as single write transaction
- VectorStore::compact trait method is object-safe (&self, concrete types)
- Empty embeddings edge case handled

**Test plan coverage**:
- R-03 (corruption): 6 integration test scenarios
- R-06 (VECTOR_MAP ordering): 4 scenarios including single transaction verification
- R-15 (search drift): 3 scenarios including similarity score epsilon comparison
- R-17 (trait safety): Compile-time test
- R-18 (empty KB): Empty embeddings test
- R-19 (concurrent): Harmless rebuild test
- AC-12, AC-13, AC-20 addressed

**Note**: Architecture C3 steps 5-6 describe VECTOR_MAP update after in-memory swap, but ADR-004 (the authoritative decision) revises this to VECTOR_MAP-first ordering. Pseudocode correctly follows ADR-004. This is an internal documentation inconsistency in the Architecture that does not affect correctness.

### C4: Coherence Module (PASS)

**Pseudocode alignment**:
- All 4 dimension score functions match Architecture C4 signatures exactly
- compute_lambda matches Architecture: re-normalization when embedding excluded
- CoherenceWeights struct with 4 fields per ADR-003
- DEFAULT_WEIGHTS values match ADR-003: 0.35/0.30/0.15/0.20
- All 5 named constants match Architecture C4
- generate_recommendations returns empty when lambda >= threshold per Specification FR-400
- oldest_stale_age helper function added for recommendation generation
- All functions are pure (no I/O) per Architecture and AC-15

**Test plan coverage**:
- R-05 (lambda re-normalization): 7 scenarios including custom zero-weight
- R-10 (boundary values): 16 scenarios covering all 4 dimension functions with boundary inputs
- R-14 (weight sum): 2 scenarios for DEFAULT_WEIGHTS
- R-16 (staleness detection): 5 scenarios including recently accessed, zero timestamps
- R-20 (recommendation accuracy): 7 scenarios including threshold boundary
- AC-01 through AC-08, AC-15, AC-16, AC-18 addressed

### C5: Confidence Refresh (PASS)

**Pseudocode alignment**:
- Batch cap at MAX_CONFIDENCE_REFRESH_BATCH (100) per Architecture C5
- Oldest-first sorting for refresh priority
- Individual failure handling (log + skip) per Architecture
- Gated on maintain_enabled per ADR-002
- Stale entry identification uses same logic as confidence_freshness_score

**Test plan coverage**:
- R-08 (batch overflow): 4 scenarios including 200 stale entries, under cap, second call
- R-16 (staleness detection): Covered via staleness-related tests
- AC-09, AC-10, AC-19 addressed
- FM-02 (partial failure) addressed

### C6: Status Extension (PASS)

**Pseudocode alignment**:
- All 10 new StatusReport fields match Architecture C6 exactly
- Default values specified: scores=1.0, counts=0, ratio=0.0, compacted=false, recs=empty
- Format extensions for summary, markdown, and JSON per Specification FR-600/601/602
- Coherence section format matches Specification examples

**Test plan coverage**:
- R-12 (formatting): 11 test scenarios covering all three formats, recommendations, graph_compacted, f64 precision
- AC-01, AC-02, AC-14, AC-17 addressed
- Existing test construction site updates documented

### C7: Maintenance Parameter (PASS)

**Pseudocode alignment**:
- `maintain: Option<bool>` with default false per ADR-002
- Resolution logic: `unwrap_or(false)` per ADR-002 opt-in semantics
- Gates 3 write operations: confidence refresh (C5), co-access cleanup, graph compaction (C8)
- Reads (dimension scores, lambda, recommendations) always run per ADR-002
- MCP tool schema update specified

**Test plan coverage**:
- R-07 (opt-out completeness): 8 integration test scenarios covering all write operations
- AC-09 (maintenance=false default) addressed
- StatusParams unit tests for None/Some(true)/Some(false) included

**Note**: IMPLEMENTATION-BRIEF line 101 says `maintenance: Option<bool>` but line 328 and ADR-002 say `maintain: Option<bool>`. The pseudocode correctly uses `maintain`. This is a documentation typo, not a design inconsistency.

### C8: Compaction Integration (PASS)

**Pseudocode alignment**:
- Integration point after confidence refresh, before lambda computation per Architecture
- Stale ratio computation: stale_count / point_count
- Embed service availability check with graceful skip per SR-04, R-09
- Non-fatal error handling for all failure modes per Architecture error boundaries
- graph_compacted and graph_stale_ratio reported in all cases

**Test plan coverage**:
- R-09 (embed unavailable): 3 integration test scenarios
- R-18 (empty KB): Empty database test
- End-to-end integration scenarios IT-01, IT-04, IT-08 included
- AC-11, AC-14 addressed

## Risk Coverage Verification

All 20 risks from the Risk-Based Test Strategy have corresponding test scenarios in the component test plans:

| Risk | Priority | Covered By |
|------|----------|------------|
| R-01 | High | test-plan/schema-migration.md (6 scenarios) |
| R-02 | Critical | test-plan/f64-scoring.md (8 scenarios) |
| R-03 | High | test-plan/vector-compaction.md (6 scenarios) |
| R-04 | Med | test-plan/f64-scoring.md (3 scenarios) |
| R-05 | High | test-plan/coherence-module.md (7 scenarios) |
| R-06 | High | test-plan/vector-compaction.md (4 scenarios) |
| R-07 | Med | test-plan/maintenance-parameter.md (8 scenarios) |
| R-08 | Med | test-plan/confidence-refresh.md (4 scenarios) |
| R-09 | Med | test-plan/compaction-integration.md (3 scenarios) |
| R-10 | High | test-plan/coherence-module.md (16 scenarios) |
| R-11 | High | test-plan/f64-scoring.md (full suite regression) |
| R-12 | Med | test-plan/status-extension.md (11 scenarios) |
| R-13 | Critical | test-plan/schema-migration.md (3 scenarios) |
| R-14 | High | test-plan/f64-scoring.md + coherence-module.md (5 scenarios) |
| R-15 | Med | test-plan/vector-compaction.md (3 scenarios) |
| R-16 | Med | test-plan/coherence-module.md (5 scenarios) |
| R-17 | High | test-plan/f64-scoring.md + vector-compaction.md (compile-time) |
| R-18 | Med | test-plan/vector-compaction.md + compaction-integration.md (3 scenarios) |
| R-19 | Low | test-plan/vector-compaction.md (1 scenario) |
| R-20 | Low | test-plan/coherence-module.md (7 scenarios) |

## Interface Consistency

All component interfaces are consistent with the Architecture's Integration Surface table:
- Function signatures match across components
- Type propagation chain verified: EntryRecord.confidence (f64) -> compute_confidence (f64) -> update_confidence (f64) -> rerank_score (f64)
- SearchResult.similarity (f64) -> rerank_score (f64)
- VectorStore::compact trait method is object-safe

## Issues Found

### Minor Documentation Inconsistencies (Non-Blocking)

1. **Architecture C3 vs ADR-004 ordering**: Architecture C3 describes VECTOR_MAP update after swap (steps 5-6), but ADR-004 specifies VECTOR_MAP-first. Pseudocode correctly follows ADR-004. The Architecture's C3 narrative is slightly out of sync with the ADR.

2. **IMPLEMENTATION-BRIEF line 101**: Says `maintenance: Option<bool>` but the field name is `maintain`. Line 328 and ADR-002 correctly say `maintain`.

3. **Architecture C5 gating description**: Says "Gating: Refresh only runs when the maintenance parameter is true (default)." The word "default" is ambiguous here -- the default value of maintain is false (per ADR-002), but the sentence seems to say the default is to run maintenance. The pseudocode correctly implements ADR-002: maintain defaults to false.

None of these affect implementation correctness. The pseudocode follows the authoritative sources (ADRs, Specification) in all cases.

## Verdict

**PASS** -- All components align with Architecture, implement Specification requirements, and test plans address all 20 risks from the Risk-Based Test Strategy. Proceed to Stage 3b.
