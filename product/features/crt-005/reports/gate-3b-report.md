# Gate 3b Report: Code Review Validation

**Feature**: crt-005 Coherence Gate
**Gate**: 3b (Code matches pseudocode + architecture)
**Result**: PASS
**Date**: 2026-02-27

## Validation Summary

All 8 components validated against their pseudocode and architecture documents. Code matches design faithfully. 839 tests pass (28 new, up from 811 baseline). No TODOs, stubs, or placeholder functions found.

## Component Validation

### C1: Schema Migration v2->v3 -- PASS

| Check | Result |
|-------|--------|
| V2EntryRecord 26-field struct with confidence: f32 | PASS - Lines 263-290 of migration.rs |
| Field order matches bincode positional encoding | PASS - All fields in identical order |
| deserialize_v2_entry helper | PASS - Lines 292-298 |
| migrate_v2_to_v3 scan-and-rewrite | PASS - Lines 301-365 |
| confidence: v2.confidence as f64 (IEEE 754 lossless) | PASS - Line 337 |
| CURRENT_SCHEMA_VERSION = 3 | PASS - Line 9 |
| Migration dispatch: `if current_version == N` | PASS - Lines 28-34 (deviated from pseudocode's `< N` pattern to fix chain deserialization) |
| EntryRecord.confidence: f64 in schema.rs | PASS - Line 109 |

**Deviation**: Migration dispatch uses `== N` instead of `< N` per pseudocode. This is correct -- the `< N` pattern was identified as causing deserialization failures when intermediate migration structs try to read already-upgraded entries.

### C2: f64 Scoring Constants -- PASS

| Check | Result |
|-------|--------|
| W_BASE..W_COAC all f64 | PASS - Lines 16-28 of confidence.rs |
| SEARCH_SIMILARITY_WEIGHT: f64 | PASS - Line 43 |
| compute_confidence returns f64 | PASS - Line 156 |
| rerank_score(f64, f64) -> f64 | PASS - Line 177 |
| co_access_affinity(usize, f64) -> f64 | PASS - Line 191 |
| MAX_CO_ACCESS_BOOST: f64 | PASS - Line 18 of coaccess.rs |
| MAX_BRIEFING_CO_ACCESS_BOOST: f64 | PASS - Line 21 |
| co_access_boost returns f64 | PASS - Line 59 |
| compute_search_boost returns HashMap<u64, f64> | PASS - Line 77 |
| SearchResult.similarity: f64 | PASS - Line 21 of index.rs |
| Cast order: 1.0_f64 - n.distance as f64 | PASS - Line 276 (R-04 respected) |
| update_confidence(u64, f64) | PASS - Line 430 of write.rs |
| DUPLICATE_THRESHOLD: f64 | PASS - Line 44 of tools.rs |
| No `as f32` in scoring pipeline | PASS - grep verified |
| Contradiction.rs boundary casts | PASS - 5 `as f32` casts at HNSW boundary only |

### C3: Vector Compaction -- PASS

| Check | Result |
|-------|--------|
| VectorIndex::compact(&self, Vec<(u64, Vec<f32>)>) | PASS - Lines 355-399 of index.rs |
| Build-new-then-swap (ADR-004) | PASS - Steps 1-4 match pseudocode |
| validate_dimension + validate_embedding per entry | PASS - Lines 371-372 |
| Sequential data_ids from 0 | PASS - Line 370 |
| VECTOR_MAP-first ordering | PASS - Line 381 |
| Atomic in-memory swap (both locks held) | PASS - Lines 384-392 |
| Reset next_data_id | PASS - Lines 395-396 |
| Store::rewrite_vector_map single transaction | PASS - Lines 460-487 of write.rs |
| VectorStore::compact trait method | PASS - Line 56 of traits.rs |
| VectorAdapter::compact delegation | PASS - Lines 141-143 of adapters.rs |

### C4: Coherence Module -- PASS

| Check | Result |
|-------|--------|
| New file coherence.rs created | PASS |
| pub mod coherence in lib.rs | PASS |
| DEFAULT_WEIGHTS: 0.35/0.30/0.15/0.20 | PASS - Lines 31-36 |
| DEFAULT_STALENESS_THRESHOLD_SECS: 86400 | PASS - Line 11 |
| DEFAULT_STALE_RATIO_TRIGGER: 0.10 | PASS - Line 14 |
| DEFAULT_LAMBDA_THRESHOLD: 0.8 | PASS - Line 17 |
| MAX_CONFIDENCE_REFRESH_BATCH: 100 | PASS - Line 20 |
| confidence_freshness_score matches pseudocode | PASS - Lines 44-68 |
| graph_quality_score matches pseudocode | PASS - Lines 74-80 |
| embedding_consistency_score matches pseudocode | PASS - Lines 86-92 |
| contradiction_density_score matches pseudocode | PASS - Lines 98-104 |
| compute_lambda with re-normalization (ADR-003) | PASS - Lines 111-140 |
| oldest_stale_age helper | PASS - Lines 145-165 |
| generate_recommendations | PASS - Lines 170-212 |
| 28 unit tests | PASS |

### C5: Confidence Refresh -- PASS

| Check | Result |
|-------|--------|
| Gated by maintain_enabled | PASS - Line 1376 of tools.rs |
| Stale entry filter matches freshness logic | PASS - Lines 1381-1393 |
| Sort oldest first | PASS - Line 1396 |
| Truncate to MAX_CONFIDENCE_REFRESH_BATCH | PASS - Line 1399 |
| compute_confidence per stale entry | PASS - Line 1403 |
| store.update_confidence per entry | PASS - Line 1410 |
| Individual failures logged, not fatal | PASS - Lines 1412-1414 |
| refreshed_count tracks successes | PASS - Lines 1408, 1411 |
| When maintain=false: refreshed_count=0 | PASS - Default 0 in report |

### C6: StatusReport Extension -- PASS

| Check | Result |
|-------|--------|
| 10 new fields added to StatusReport | PASS - Lines 401-420 of response.rs |
| coherence: f64 | PASS |
| confidence_freshness_score: f64 | PASS |
| graph_quality_score: f64 | PASS |
| embedding_consistency_score: f64 | PASS |
| contradiction_density_score: f64 | PASS |
| stale_confidence_count: u64 | PASS |
| confidence_refreshed_count: u64 | PASS |
| graph_stale_ratio: f64 | PASS |
| graph_compacted: bool | PASS |
| maintenance_recommendations: Vec<String> | PASS |
| All StatusReport test constructions updated | PASS |
| Summary/Markdown/JSON format extensions | PASS |

### C7: Maintenance Parameter -- PASS

| Check | Result |
|-------|--------|
| maintain: Option<bool> in StatusParams | PASS - Line 197 of tools.rs |
| maintain_enabled = unwrap_or(false) | PASS - Line 1003 |
| Gates confidence refresh | PASS - Line 1376 |
| Gates co-access cleanup | PASS - Lines 1307 |
| Gates graph compaction | PASS - Line 1432 |
| Dimension scores computed regardless | PASS - Lines 1333-1373 |
| Lambda + recommendations computed regardless | PASS - Lines 1468-1489 |
| maintain: None in validation tests | PASS - 3 occurrences |
| JsonSchema derive on StatusParams | PASS - Struct derives JsonSchema |

### C8: Compaction Integration -- PASS

| Check | Result |
|-------|--------|
| Gated by maintain_enabled && stale_ratio > trigger | PASS - Line 1432 |
| Embed service availability check | PASS - Line 1433 |
| Re-embed via adapter.embed_entries | PASS - Lines 1435-1439 |
| Build (entry_id, Vec<f32>) pairs | PASS - Lines 1441-1444 |
| vector_index.compact(compact_input) | PASS - Line 1448 |
| graph_compacted = true on success | PASS - Line 1451 |
| Graceful failure at embed, compact stages | PASS - Lines 1453-1465 |
| When maintain=false: graph_compacted=false | PASS - Default false |

## Architecture Compliance

| ADR | Compliance |
|-----|-----------|
| ADR-001: f64 Scoring Boundary | PASS - All scoring pipeline f64, embeddings stay f32, cast at map_neighbours_to_results |
| ADR-002: Maintenance Opt-In | PASS - maintain defaults false, reads always run, writes gated |
| ADR-003: Lambda Dimension Weights | PASS - 0.35/0.30/0.20/0.15, re-normalization when embedding unavailable |
| ADR-004: Graph Compaction Atomicity | PASS - Build-new-then-swap, VECTOR_MAP-first, pre-computed embeddings |

## Risk Verification (R-02: Residual f32)

Grep verification confirms zero `as f32` casts in scoring pipeline files:
- confidence.rs: 0 occurrences
- coaccess.rs: 0 occurrences
- tools.rs: 0 occurrences
- coherence.rs: 0 occurrences
- response.rs: 0 occurrences

The only `as f32` casts in the server crate are in contradiction.rs (5 occurrences), which is correct per ADR-001 -- contradiction detection operates in the HNSW domain (f32).

## Test Results

| Crate | Tests | Status |
|-------|-------|--------|
| unimatrix-core | 21 | PASS |
| unimatrix-embed | 76 | PASS |
| unimatrix-server | 481 | PASS |
| unimatrix-store | 166 | PASS |
| unimatrix-vector | 95 | PASS |
| **Total** | **839** | **ALL PASS** |

New tests added: 28 (coherence module unit tests)
Baseline: 811 (pre crt-005)
Delta: +28

## Files Modified/Created

### New Files
- `crates/unimatrix-server/src/coherence.rs`

### Modified Files
- `crates/unimatrix-store/src/schema.rs`
- `crates/unimatrix-store/src/migration.rs`
- `crates/unimatrix-store/src/write.rs`
- `crates/unimatrix-vector/src/index.rs`
- `crates/unimatrix-core/src/traits.rs`
- `crates/unimatrix-core/src/adapters.rs`
- `crates/unimatrix-server/src/confidence.rs`
- `crates/unimatrix-server/src/coaccess.rs`
- `crates/unimatrix-server/src/contradiction.rs`
- `crates/unimatrix-server/src/response.rs`
- `crates/unimatrix-server/src/tools.rs`
- `crates/unimatrix-server/src/validation.rs`
- `crates/unimatrix-server/src/lib.rs`

## Deviations from Pseudocode

1. **Migration dispatch pattern** (C1): Changed from `if current_version < N` to `if current_version == N`. This is a necessary correction -- the pseudocode pattern causes deserialization failures when intermediate migration structs try to read entries already upgraded to the current schema format. The `== N` pattern ensures only the matching migration step runs.

2. **No other deviations** identified. All function signatures, constants, struct fields, algorithm steps, error handling patterns, and gating logic match their respective pseudocode documents.

## Conclusion

Gate 3b PASSES. All 8 components faithfully implement their pseudocode designs. Architecture decisions (ADR-001 through ADR-004) are correctly applied. The f32-to-f64 upgrade is complete with zero residual f32 in the scoring pipeline. The coherence gate computes lambda from four dimensions with proper re-normalization. Maintenance operations are correctly gated behind the opt-in maintain parameter. All 839 tests pass.
