# Implementation Brief: crt-005 Coherence Gate

## Source Documents

| Document | Path |
|----------|------|
| Scope | product/features/crt-005/SCOPE.md |
| Scope Risk Assessment | product/features/crt-005/SCOPE-RISK-ASSESSMENT.md |
| Architecture | product/features/crt-005/architecture/ARCHITECTURE.md |
| ADR-001 f64 Scoring Boundary | product/features/crt-005/architecture/ADR-001-f64-scoring-boundary.md |
| ADR-002 Maintenance Opt-Out | product/features/crt-005/architecture/ADR-002-maintenance-opt-out.md |
| ADR-003 Lambda Dimension Weights | product/features/crt-005/architecture/ADR-003-lambda-dimension-weights.md |
| ADR-004 Graph Compaction Atomicity | product/features/crt-005/architecture/ADR-004-graph-compaction-atomicity.md |
| Specification | product/features/crt-005/specification/SPECIFICATION.md |
| Risk Strategy | product/features/crt-005/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/crt-005/ALIGNMENT-REPORT.md |

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| schema-migration | pseudocode/schema-migration.md | test-plan/schema-migration.md |
| f64-scoring | pseudocode/f64-scoring.md | test-plan/f64-scoring.md |
| vector-compaction | pseudocode/vector-compaction.md | test-plan/vector-compaction.md |
| coherence-module | pseudocode/coherence-module.md | test-plan/coherence-module.md |
| confidence-refresh | pseudocode/confidence-refresh.md | test-plan/confidence-refresh.md |
| status-extension | pseudocode/status-extension.md | test-plan/status-extension.md |
| maintenance-parameter | pseudocode/maintenance-parameter.md | test-plan/maintenance-parameter.md |
| compaction-integration | pseudocode/compaction-integration.md | test-plan/compaction-integration.md |

## Goal

Unify four independent knowledge base health signals (confidence staleness, HNSW graph degradation, embedding inconsistency, contradiction density) into a composite coherence metric (lambda) exposed through `context_status`, with inline maintenance actions (confidence refresh, graph compaction) and actionable recommendations. Simultaneously upgrade the entire scoring pipeline from f32 to f64 to eliminate precision truncation artifacts and enable fine-grained score differentiation at scale. This feature is the capstone of the Cortical phase (M4) and the direct prerequisite for col-002 (Retrospective Pipeline), which needs reliable knowledge quality signals.

## Delivery Tiers

### Tier 1: f64 Upgrade + Lambda Read-Only (Low Risk)

Components: C1 (schema migration), C2 (f64 constants), C3 (VectorIndex compact method), C4 (coherence module), C6 (StatusReport extension), C7 (StatusParams extension -- partial: maintenance parameter defined but only used in Tier 2).

Delivers:
- Schema migration v2 to v3: `EntryRecord.confidence` f32 to f64
- All scoring constants promoted from f32 to f64 across all crates
- `compute_confidence` returns f64 (no `as f32` truncation)
- `SearchResult.similarity` promoted to f64
- `update_confidence` accepts f64
- `rerank_score` and `co_access_affinity` operate in f64
- New `coherence.rs` module with pure dimension score functions
- Lambda computation and dimension scores in StatusReport
- `VectorIndex::compact` method (available but not yet triggered)
- Response formatting with coherence section in all three formats

This tier is a type-level refactor plus pure computation. No writes beyond what already exists. Safe and independently coherent.

### Tier 2: Confidence Refresh + Graph Compaction + Maintenance (Higher Risk)

Components: C5 (confidence refresh), C7 (StatusParams maintenance -- complete), C8 (graph compaction integration).

Delivers:
- `maintain` parameter on `context_status` (opt-in for writes, default false)
- Lazy confidence refresh during `context_status(maintain: true)` (capped at 100 entries per call)
- HNSW graph compaction trigger when stale ratio exceeds 10% (only when `maintain: true`)
- Co-access cleanup gated behind maintain flag
- Maintenance recommendations when lambda drops below 0.8 (always generated, regardless of maintain flag)

This tier adds write behavior to `context_status` behind an explicit opt-in. The default path remains read-only.

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| f64 upgrade scope: scoring pipeline only, embeddings stay f32 | Scoring pipeline (confidence, re-ranking, co-access boost) promoted to f64. Embeddings, HNSW internals, and contradiction detection remain f32. The boundary is at `map_neighbours_to_results` where f32 hnsw_rs distance is cast to f64. | ADR-001 | architecture/ADR-001-f64-scoring-boundary.md |
| context_status behavioral contract change | Add `maintain: Option<bool>` parameter (default false). Status is read-only by default. When `maintain: true`, confidence refresh, graph compaction, and co-access cleanup run. Maintenance requires explicit intent. | ADR-002 | architecture/ADR-002-maintenance-opt-out.md |
| Lambda dimension weights: unequal, with re-normalization | Weights: freshness 0.35, graph 0.30, contradiction 0.20, embedding 0.15. When embedding consistency is unavailable (check not performed), exclude from weighted average and re-normalize remaining weights. | ADR-003 | architecture/ADR-003-lambda-dimension-weights.md |
| Graph compaction atomicity: build-new-then-swap | Build a fresh HNSW index, write VECTOR_MAP first (single transaction), then swap in-memory graph. If VECTOR_MAP write fails, no in-memory change. Old index untouched until swap succeeds. | ADR-004 | architecture/ADR-004-graph-compaction-atomicity.md |
| Compaction embedding source: re-embed from content | hnsw_rs does not expose raw vectors. Caller (server crate) obtains embeddings via embed service and passes pre-computed `Vec<(u64, Vec<f32>)>` to `VectorIndex::compact`. VectorIndex stays independent of embed service. | ADR-004 | architecture/ADR-004-graph-compaction-atomicity.md |
| Dimension weight values: informed judgment, not empirical | 0.35 / 0.30 / 0.20 / 0.15 reflecting search-quality impact. Future tuning based on observed correlation is possible. | ADR-003 | architecture/ADR-003-lambda-dimension-weights.md |
| Confidence refresh batch cap: 100 entries per call | At current scale (<200 active entries), refreshes everything in one call. Named constant, easily adjustable. | ARCHITECTURE C4 | architecture/ARCHITECTURE.md |
| f64 test update strategy: Tier 1 first | Do the f64 upgrade as the first implementation step so all subsequent crt-005 work builds on the new types. ~60-80 tests need mechanical type updates. | ARCHITECTURE C2 | architecture/ARCHITECTURE.md |

## Files to Create/Modify

### New Files

| File | Purpose |
|------|---------|
| `crates/unimatrix-server/src/coherence.rs` | Coherence module: dimension score functions, `compute_lambda`, `CoherenceWeights`, `generate_recommendations`, all threshold/weight constants |

### Modified Files

| File | Change Summary |
|------|---------------|
| `crates/unimatrix-store/src/schema.rs` | `EntryRecord.confidence`: f32 to f64 |
| `crates/unimatrix-store/src/migration.rs` | Add `V2EntryRecord` struct, `migrate_v2_to_v3`, bump `CURRENT_SCHEMA_VERSION` to 3 |
| `crates/unimatrix-store/src/write.rs` | `Store::update_confidence` signature: f32 to f64 |
| `crates/unimatrix-vector/src/index.rs` | `SearchResult.similarity`: f32 to f64; `map_neighbours_to_results` cast; add `VectorIndex::compact` method |
| `crates/unimatrix-core/src/traits.rs` | Add `VectorStore::compact` trait method; `SearchResult` type propagation |
| `crates/unimatrix-server/src/confidence.rs` | All weight constants f32 to f64; `compute_confidence` return f64 (remove `as f32`); `rerank_score` signature f64; `co_access_affinity` signature f64 |
| `crates/unimatrix-server/src/coaccess.rs` | `MAX_CO_ACCESS_BOOST`, `MAX_BRIEFING_CO_ACCESS_BOOST` f32 to f64; `compute_search_boost`, `compute_briefing_boost` return `HashMap<u64, f64>`; `co_access_boost` internal f32 to f64 |
| `crates/unimatrix-server/src/response.rs` | `StatusReport` gains 10 new fields (coherence section); `format_status_report` extended for all three formats |
| `crates/unimatrix-server/src/tools.rs` | `StatusParams` gains `maintenance: Option<bool>`; `context_status` handler gains dimension score computation, confidence refresh, compaction trigger, lambda computation, recommendation generation |
| `crates/unimatrix-server/src/lib.rs` | Declare `coherence` module |

### Unchanged Files

| File | Reason |
|------|--------|
| `crates/unimatrix-embed/` (all files) | Embeddings remain f32; ONNX pipeline untouched |
| `crates/unimatrix-server/src/contradiction.rs` | Contradiction detection uses HNSW-domain f32 values; not part of scoring pipeline |

## Exhaustive f32 to f64 Change Inventory

This inventory addresses SR-02 (critical risk of missed f32 constants causing silent truncation).

### confidence.rs (server crate)

| Item | Current | New |
|------|---------|-----|
| `W_BASE` | `f32 (0.18)` | `f64` |
| `W_USAGE` | `f32 (0.14)` | `f64` |
| `W_FRESH` | `f32 (0.18)` | `f64` |
| `W_HELP` | `f32 (0.14)` | `f64` |
| `W_CORR` | `f32 (0.14)` | `f64` |
| `W_TRUST` | `f32 (0.14)` | `f64` |
| `W_COAC` | `f32 (0.08)` | `f64` |
| `SEARCH_SIMILARITY_WEIGHT` | `f32 (0.85)` | `f64` |
| `compute_confidence` return | `f32` | `f64` (remove `as f32` on return) |
| `rerank_score` params/return | `f32, f32 -> f32` | `f64, f64 -> f64` |
| `co_access_affinity` params/return | `usize, f32 -> f32` | `usize, f64 -> f64` |

### coaccess.rs (server crate)

| Item | Current | New |
|------|---------|-----|
| `MAX_CO_ACCESS_BOOST` | `f32 (0.03)` | `f64` |
| `MAX_BRIEFING_CO_ACCESS_BOOST` | `f32 (0.01)` | `f64` |
| `co_access_boost` (private) | `u32, f32 -> f32` | `u32, f64 -> f64` |
| `compute_search_boost` return | `HashMap<u64, f32>` | `HashMap<u64, f64>` |
| `compute_briefing_boost` return | `HashMap<u64, f32>` | `HashMap<u64, f64>` |
| `compute_boost_internal` | `..., f32 -> HashMap<u64, f32>` | `..., f64 -> HashMap<u64, f64>` |

### schema.rs (store crate)

| Item | Current | New |
|------|---------|-----|
| `EntryRecord.confidence` | `f32` | `f64` (requires schema migration) |

### index.rs (vector crate)

| Item | Current | New |
|------|---------|-----|
| `SearchResult.similarity` | `f32` | `f64` (in-memory only) |
| `map_neighbours_to_results` | `1.0 - n.distance` as f32 | `1.0_f64 - n.distance as f64` |

### write.rs (store crate)

| Item | Current | New |
|------|---------|-----|
| `Store::update_confidence` | `fn(..., f32)` | `fn(..., f64)` |

### tools.rs (server crate)

| Item | Change |
|------|--------|
| `compute_confidence` call sites (~6) | Return value is f64, no cast |
| `rerank_score` call sites | Pass f64 similarity + confidence |
| `co_access_affinity` call sites | Pass f64 avg_partner_confidence |
| `compute_search_boost` / `compute_briefing_boost` | Boost map values are f64 |
| `update_confidence` call sites (~8) | Pass f64 |

### NOT Changed (stays f32)

| Item | Reason |
|------|--------|
| `contradiction.rs` constants (`SIMILARITY_THRESHOLD`, `DEFAULT_CONFLICT_SENSITIVITY`, `EMBEDDING_CONSISTENCY_THRESHOLD`, `NEGATION_WEIGHT`, `DIRECTIVE_WEIGHT`, `SENTIMENT_WEIGHT`) | HNSW domain, not scoring pipeline |
| `ContradictionPair.similarity`, `ContradictionPair.conflict_score` | Comes from HNSW (f32) |
| `EmbeddingInconsistency.expected_similarity` | From HNSW |
| `ContradictionConfig` fields | HNSW domain |
| All embedding types (`Vec<f32>`, `Hnsw<f32, DistDot>`) | ONNX model is the precision bottleneck |

## Schema Migration v2 to v3: Step-by-Step

Follow the established migration pattern from nxs-004 and crt-001.

1. Define `V2EntryRecord` -- a 26-field struct matching the current schema exactly, with `confidence: f32`. All other fields identical to current `EntryRecord`. Uses `#[serde(default)]` on `helpful_count`, `unhelpful_count`, `confidence`.

2. In `migrate_v2_to_v3` (within a single redb write transaction):
   - Open the ENTRIES table for read+write.
   - Iterate all entries.
   - For each `(key, value_bytes)`: deserialize as `V2EntryRecord` (bincode). Construct new `EntryRecord` with `confidence: v2.confidence as f64` (lossless per IEEE 754). Serialize the new `EntryRecord` (bincode). Overwrite the entry.
   - Set `schema_version` counter to 3.
   - Commit the write transaction.

3. Bump `CURRENT_SCHEMA_VERSION` from 2 to 3.

4. Add migration call to `migrate_if_needed`: `if current_version < 3 { migrate_v2_to_v3(&txn)?; }`

**Atomicity**: The entire migration runs in a single redb write transaction. If any entry fails deserialization or the process crashes, the transaction rolls back and the database remains at v2. Next `Store::open` retries the migration.

**Critical**: The migration must use `V2EntryRecord` with `confidence: f32` to deserialize, then construct the v3 struct. Cannot read v2 bytes directly as f64 because bincode encodes f32 as 4 bytes and f64 as 8 bytes.

## Graph Compaction Algorithm: Step-by-Step

`VectorIndex::compact(&self, embeddings: Vec<(u64, Vec<f32>)>) -> Result<()>`

1. **Build new graph**: Create a fresh `Hnsw<'static, f32, DistDot>` with the same configuration parameters (max_nb_connection, max_elements, max_layer, ef_construction).

2. **Insert embeddings**: Insert all provided `(entry_id, embedding)` pairs into the new HNSW graph. Assign sequential data_ids starting from 0.

3. **Build new IdMap**: Construct a fresh `IdMap` mapping the new data_ids to entry_ids.

4. **Write VECTOR_MAP first**: Write all new `(entry_id, data_id)` mappings to the store in a single write transaction. If this fails, return error -- no in-memory changes, old graph untouched.

5. **Atomic in-memory swap**: Acquire write locks on `self.hnsw` and `self.id_map` simultaneously. Replace both with the new versions. Reset `self.next_data_id` to the count of inserted embeddings.

6. **Cleanup**: The old HNSW graph and IdMap are dropped when the replaced values go out of scope.

**Failure modes**:
- Build fails (OOM): Return error. Old graph untouched.
- VECTOR_MAP write fails: Return error. No in-memory changes.
- Lock poisoned: Panic propagation (bug, not runtime condition).

**Caller responsibility**: The server crate (context_status handler) checks embed service availability, reads active entries, re-embeds via embed service, and passes the results to `compact`. VectorIndex never interacts with the embed service.

## Coherence Module Design

### New file: `crates/unimatrix-server/src/coherence.rs`

### Dimension Score Functions (pure, no side effects)

```rust
/// Confidence freshness: ratio of non-stale entries to total active.
/// Returns (score, stale_count).
fn confidence_freshness_score(
    entries: &[EntryRecord],
    now: u64,
    staleness_threshold_secs: u64,
) -> (f64, u64)

/// Graph quality: 1.0 - (stale_count / point_count), clamped to [0.0, 1.0].
fn graph_quality_score(stale_count: usize, point_count: usize) -> f64

/// Embedding consistency: 1.0 - (inconsistent_count / total_checked).
fn embedding_consistency_score(inconsistent_count: usize, total_checked: usize) -> f64

/// Contradiction density: 1.0 - (total_quarantined / total_active), clamped.
fn contradiction_density_score(total_quarantined: u64, total_active: u64) -> f64
```

### Lambda Computation

```rust
pub struct CoherenceWeights {
    pub confidence_freshness: f64,  // 0.35
    pub graph_quality: f64,         // 0.30
    pub embedding_consistency: f64, // 0.15
    pub contradiction_density: f64, // 0.20
}

/// Composite coherence score. Excludes unavailable dimensions and
/// re-normalizes remaining weights.
fn compute_lambda(
    freshness: f64,
    graph_quality: f64,
    embedding_consistency: Option<f64>,  // None if check not performed
    contradiction_density: f64,
    weights: &CoherenceWeights,
) -> f64
```

When `embedding_consistency` is `None`, the remaining three weights are re-normalized:
- freshness: 0.35 / 0.85 = 0.4118
- graph: 0.30 / 0.85 = 0.3529
- contradiction: 0.20 / 0.85 = 0.2353

### Maintenance Recommendations

```rust
fn generate_recommendations(
    lambda: f64,
    threshold: f64,
    stale_confidence_count: u64,
    oldest_stale_age_secs: u64,
    graph_stale_ratio: f64,
    embedding_inconsistent_count: usize,
    total_quarantined: u64,
) -> Vec<String>
```

Returns empty vec when `lambda >= threshold`. Otherwise returns per-dimension recommendations with specific counts and context.

### Named Constants

```rust
pub const DEFAULT_STALENESS_THRESHOLD_SECS: u64 = 24 * 3600;    // 24 hours
pub const DEFAULT_STALE_RATIO_TRIGGER: f64 = 0.10;               // 10%
pub const DEFAULT_LAMBDA_THRESHOLD: f64 = 0.8;
pub const MAX_CONFIDENCE_REFRESH_BATCH: usize = 100;

pub const DEFAULT_WEIGHTS: CoherenceWeights = CoherenceWeights {
    confidence_freshness: 0.35,
    graph_quality: 0.30,
    embedding_consistency: 0.15,
    contradiction_density: 0.20,
};
```

## StatusReport Extension

New fields added to `StatusReport`:

```rust
pub coherence: f64,                              // Composite lambda [0.0, 1.0]
pub confidence_freshness_score: f64,             // Dimension 1 [0.0, 1.0]
pub graph_quality_score: f64,                    // Dimension 2 [0.0, 1.0]
pub embedding_consistency_score: f64,            // Dimension 3 [0.0, 1.0]
pub contradiction_density_score: f64,            // Dimension 4 [0.0, 1.0]
pub stale_confidence_count: u64,                 // Entries with stale confidence
pub confidence_refreshed_count: u64,             // Entries refreshed this call
pub graph_stale_ratio: f64,                      // Current stale node ratio
pub graph_compacted: bool,                       // Whether compaction ran
pub maintenance_recommendations: Vec<String>,    // Actionable recommendations
```

### StatusParams Extension

```rust
pub maintain: Option<bool>,  // Default: false (read-only). Set true to run maintenance writes.
```

## Integration Points

### context_status Handler Flow (after crt-005)

```
(1)  Identity + Capability check [existing]
(2)  Validation [existing, extended for maintain param]
(3)  Read transaction: counters, distributions, correction metrics [existing]
       +-- Identify stale entries (confidence_freshness_score)
(4)  Contradiction scanning [existing]
(5)  Embedding consistency check if opted in [existing]
(6)  Compute dimension scores from available data [NEW: C4]
       +-- confidence_freshness_score (from entry scan)
       +-- graph_quality_score (from vector_index.stale_count/point_count)
       +-- embedding_consistency_score (from step 5 results, or None)
       +-- contradiction_density_score (from counters)
(7)  Confidence refresh for stale entries [NEW: C5, Tier 2, only when maintain=true]
       +-- compute_confidence per stale entry (capped at 100)
       +-- update_confidence per entry
(8)  Co-access stats and cleanup [existing, gated on maintain=true]
(9)  HNSW compaction if stale ratio > 10% [NEW: C8, Tier 2, only when maintain=true]
       +-- embed_service.embed_entries(active_entries)
       +-- vector_index.compact(embeddings)
(10) Compute composite lambda [NEW: C4]
(11) Generate maintenance recommendations [NEW: C4]
(12) Build StatusReport with coherence fields [C6]
(13) Format and return response
```

### Cross-Crate Type Propagation

```
store::EntryRecord.confidence: f64
  --> server::compute_confidence() returns f64
  --> store::update_confidence(id, f64)
  --> server::rerank_score(similarity: f64, confidence: f64) -> f64

vector::SearchResult.similarity: f64
  --> cast from f32 at map_neighbours_to_results boundary
  --> server::rerank_score(similarity: f64, ...)

server::coaccess boost: HashMap<u64, f64>
  --> added to rerank_score output for final_score

core::VectorStore::compact(Vec<(u64, Vec<f32>)>)
  --> called from server context_status handler
  --> embeddings are f32 (from embed service)
```

## Critical Risks and Verification

### R-02: Residual f32 Constants (Critical, High Likelihood)

**Risk**: A literal `0.85_f32` or implicit f32 type inference left behind causes silent truncation.

**Verification**:
1. After implementation, grep all scoring-path `.rs` files for `as f32` -- the only legitimate `as f32` should be in contradiction.rs and embed pipeline.
2. Assert all weight constants are f64 type via compile-time or const assertions.
3. Verify `compute_confidence` produces precision beyond 7 decimal digits.
4. Verify JSON output has clean f64 values (no `0.8500000238418579` artifacts).

### R-13: V2EntryRecord Struct Mismatch (Critical, Med Likelihood)

**Risk**: V2EntryRecord field count or order mismatch causes deserialization failure for all entries during migration.

**Verification**:
1. V2EntryRecord must have exactly 26 fields matching current EntryRecord (with `confidence: f32`).
2. Field order must match bincode serialization order.
3. Round-trip test: serialize with current EntryRecord (v2), deserialize with V2EntryRecord -- all fields match.
4. Test migration with known f32 confidence values and verify lossless f64 promotion.
5. Test migration chain v0 to v1 to v2 to v3.

## Test Strategy Summary

### Test Categories

| Category | Scope | Estimated Count |
|----------|-------|----------------|
| Coherence dimension scores (pure functions) | Unit tests in coherence.rs | ~25 |
| Lambda computation + re-normalization | Unit tests in coherence.rs | ~10 |
| Recommendation generation | Unit tests in coherence.rs | ~8 |
| Schema migration v2 to v3 | Integration tests in migration.rs | ~7 |
| f64 scoring precision | Unit tests in confidence.rs, coaccess.rs | ~10 |
| VectorIndex compact | Integration tests in index.rs | ~8 |
| Maintenance opt-out | Integration tests in tools.rs/server.rs | ~7 |
| Confidence refresh + batch cap | Integration tests in tools.rs | ~5 |
| StatusReport formatting (3 formats) | Unit tests in response.rs | ~7 |
| Embed service unavailability | Integration tests | ~4 |
| f32 to f64 existing test updates | Mechanical updates across all crates | ~60-80 updates |
| End-to-end integration scenarios | Integration tests (IT-01 through IT-08) | ~8 |
| **New tests total** | | **~99** |
| **Existing test updates** | | **~60-80** |

### Risk Coverage

| Priority | Risk Count | Test Scenarios |
|----------|-----------|---------------|
| Critical | 2 (R-02, R-13) | 13 scenarios |
| High | 8 (R-01, R-03, R-05, R-06, R-10, R-11, R-14, R-17) | 42 scenarios |
| Medium | 8 (R-04, R-07, R-08, R-09, R-12, R-15, R-16, R-18) | 31 scenarios |
| Low | 2 (R-19, R-20) | 8 scenarios |
| **Total** | **20 risks** | **94 scenarios** |

### Regression Strategy

1. **Before changes**: Run `cargo test --workspace` and record baseline pass count (811+).
2. **After Tier 1**: Fix all compile errors from type changes (~60-80 tests). Verify pass count >= baseline.
3. **After Tier 2**: Verify pass count >= Tier 1 count (new tests added, none removed).
4. **Final**: Verify no test is `#[ignore]`d or `#[cfg(skip)]`d. Every existing test must pass.

## ADR Decisions Summary

### ADR-001: f64 Scoring Boundary

The scoring pipeline (confidence, re-ranking, co-access boost) is promoted to f64 end-to-end. Embeddings and HNSW internals remain f32 because the ONNX model is the precision bottleneck. The contradiction detection module stays f32 because it compares against HNSW similarity scores (inherently f32). The f32-to-f64 boundary lives in `map_neighbours_to_results`. Key benefit: eliminates JSON precision artifacts and enables future pi-based calibration.

### ADR-002: Maintenance Opt-Out

`context_status` gains `maintain: Option<bool>` (default false). By default, status is read-only — coherence scores are computed and recommendations generated, but no writes occur. When `maintain: true`, confidence refresh, graph compaction, and co-access cleanup run. This preserves "safe to call repeatedly" semantics by default while requiring explicit intent for maintenance writes.

### ADR-003: Lambda Dimension Weights

Unequal weights reflecting search-quality impact: freshness 0.35, graph 0.30, contradiction 0.20, embedding 0.15. When embedding consistency is unavailable (check not performed), exclude from weighted average and re-normalize remaining weights. This avoids inflating lambda for callers who never enable embedding checks.

### ADR-004: Graph Compaction Atomicity

Build-new-then-swap with VECTOR_MAP-first ordering. A fresh HNSW index is built from pre-computed embeddings (provided by caller). VECTOR_MAP is written in a single transaction before the in-memory swap. If VECTOR_MAP fails, no in-memory changes. The old index remains functional for search during the entire build phase. VectorIndex receives pre-computed embeddings (stays independent of embed service).

## Data Structures

### EntryRecord (after migration)

```rust
pub struct EntryRecord {
    // ... 25 other fields unchanged ...
    #[serde(default)]
    pub confidence: f64,  // Was f32. 8 bytes in bincode.
}
```

### SearchResult (after upgrade)

```rust
pub struct SearchResult {
    pub entry_id: u64,
    pub similarity: f64,  // Was f32. Cast from f32 at HNSW boundary.
}
```

### CoherenceWeights (new)

```rust
pub struct CoherenceWeights {
    pub confidence_freshness: f64,
    pub graph_quality: f64,
    pub embedding_consistency: f64,
    pub contradiction_density: f64,
}
```

### Schema Version Progression

```
v0: Original 17-field EntryRecord (pre-nxs-004)
v1: 24-field EntryRecord (nxs-004)
v2: 26-field EntryRecord (crt-001: added helpful_count, unhelpful_count)
v3: confidence f32 to f64 (crt-005: no new fields, type change only)
```

## Function Signatures

### coherence.rs (new)

```rust
pub fn confidence_freshness_score(entries: &[EntryRecord], now: u64, staleness_threshold_secs: u64) -> (f64, u64)
pub fn graph_quality_score(stale_count: usize, point_count: usize) -> f64
pub fn embedding_consistency_score(inconsistent_count: usize, total_checked: usize) -> f64
pub fn contradiction_density_score(total_quarantined: u64, total_active: u64) -> f64
pub fn compute_lambda(freshness: f64, graph_quality: f64, embedding_consistency: Option<f64>, contradiction_density: f64, weights: &CoherenceWeights) -> f64
pub fn generate_recommendations(lambda: f64, threshold: f64, stale_confidence_count: u64, oldest_stale_age_secs: u64, graph_stale_ratio: f64, embedding_inconsistent_count: usize, total_quarantined: u64) -> Vec<String>
```

### confidence.rs (changed signatures)

```rust
pub fn compute_confidence(entry: &EntryRecord, now: u64) -> f64          // Was -> f32
pub fn rerank_score(similarity: f64, confidence: f64) -> f64             // Was f32 params/return
pub fn co_access_affinity(partner_count: usize, avg_partner_confidence: f64) -> f64  // Was f32
```

### coaccess.rs (changed signatures)

```rust
pub fn compute_search_boost(store: &dyn Store, entry_ids: &[u64], max_boost: f64) -> HashMap<u64, f64>
pub fn compute_briefing_boost(store: &dyn Store, entry_ids: &[u64], max_boost: f64) -> HashMap<u64, f64>
```

### index.rs (new method)

```rust
impl VectorIndex {
    pub fn compact(&self, embeddings: Vec<(u64, Vec<f32>)>) -> Result<()>
}
```

### traits.rs (new trait method)

```rust
pub trait VectorStore {
    // ... existing methods ...
    fn compact(&self, embeddings: Vec<(u64, Vec<f32>)>) -> Result<(), CoreError>;
}
```

### write.rs (changed signature)

```rust
impl Store {
    pub fn update_confidence(&self, entry_id: u64, confidence: f64) -> Result<()>  // Was f32
}
```

## Constraints

- `#![forbid(unsafe_code)]` -- all crates
- Edition 2024, MSRV 1.89
- No new crate dependencies beyond existing workspace
- Object-safe traits maintained (compact uses `&self` with concrete types)
- No background threads, no timers, no new async patterns
- hnsw_rs does not support point deletion (compaction requires full rebuild)
- Schema migration v2 to v3 uses intermediate V2EntryRecord for bincode deserialization
- No new redb tables
- Confidence refresh capped at 100 entries per context_status call
- Graph compaction requires embed service (graceful skip when unavailable)
- Test infrastructure is cumulative (build on existing 811+ test fixtures)
- Store crate stays domain-agnostic

## Dependencies

| Crate | Role |
|-------|------|
| `unimatrix-store` | EntryRecord schema change, migration v2 to v3, update_confidence signature |
| `unimatrix-vector` | SearchResult.similarity f64, VectorIndex::compact method |
| `unimatrix-core` | VectorStore trait: compact method, SearchResult type propagation |
| `unimatrix-server` | coherence module, f64 constants, StatusReport/StatusParams extension, context_status handler changes |
| `unimatrix-embed` | Embed service readiness check for compaction gating (existing infrastructure, no changes) |
| `redb` | Write transactions for migration and confidence refresh (existing dependency) |
| `bincode` | Serialization of f64 confidence (existing dependency) |

### Feature Dependencies

- **Depends on**: col-001 (Outcome Tracking) -- COMPLETE; crt-001 through crt-004 -- all COMPLETE
- **Blocks**: col-002 (Retrospective Pipeline) -- needs reliable knowledge quality signals from lambda

## NOT in Scope

- No new MCP tools (no `context_coherence` or `context_maintenance`)
- No background maintenance scheduler
- No automatic quarantine from coherence signals (low lambda triggers recommendations, not actions)
- No new fields on EntryRecord beyond confidence type promotion
- No new redb tables
- No confidence refresh during `context_search`
- No persistence of lambda history (mtx-phase work)
- No pi-based calibration of scoring weights (f64 enables it; actual calibration is future)
- No embedding precision changes (embeddings stay f32)
- No adaptive precision lanes
- No graph compaction during `context_search`

## Alignment Status

All checks PASS. No variances requiring approval.

| Check | Status |
|-------|--------|
| Vision Alignment | PASS |
| Milestone Fit | PASS |
| Scope Additions | PASS |
| Architecture Consistency | PASS |
| f64 Scoring Boundary | PASS |
| Lambda Serves col-002 | PASS |
| Self-Learning Engine Alignment | PASS |

**Warnings** (process, not design):
- SCOPE.md open questions (3 of 4) resolved by architecture but not marked resolved in SCOPE.md. These are documentation hygiene items.
- Embedding consistency defaults to 1.0 in the `StatusReport` field but is excluded from lambda computation when not checked (ADR-003). This is an intentional simplification with documented rationale.

See product/features/crt-005/ALIGNMENT-REPORT.md for full details.
