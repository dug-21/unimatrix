# Specification: crt-005 Coherence Gate

## Objective

Unify four independent knowledge base health signals (confidence staleness, HNSW graph degradation, embedding inconsistency, contradiction density) into a composite coherence metric (lambda) exposed through `context_status`, with inline maintenance actions (confidence refresh, graph compaction) and actionable recommendations. Simultaneously upgrade the entire scoring pipeline from f32 to f64 to eliminate precision truncation artifacts and enable fine-grained score differentiation at scale.

## Functional Requirements

### FR-1xx: Coherence Metric (Lambda Computation, Dimension Scores)

**FR-100: Composite lambda computation.** Compute a single coherence score (lambda) in [0.0, 1.0] as a weighted average of four dimension scores. Lambda = sum(weight_i * dimension_i) for i in {confidence_freshness, graph_quality, embedding_consistency, contradiction_density}. Weights are named f64 constants summing to 1.0.

**FR-101: Confidence freshness dimension.** Compute as the ratio of entries with non-stale confidence to total active entries. An entry's confidence is stale when `max(updated_at, last_accessed_at)` is older than the staleness threshold (default 24 hours). Score = fresh_count / total_active. Returns 1.0 when total_active is 0.

**FR-102: Graph quality dimension.** Compute as `1.0 - (stale_count / point_count)` using `VectorIndex::stale_count()` and `VectorIndex::point_count()`. Clamp result to [0.0, 1.0]. Returns 1.0 when point_count is 0.

**FR-103: Embedding consistency dimension.** Compute as `1.0 - (inconsistent_count / total_checked)` when embedding consistency checks are performed (i.e., `check_embeddings: true` was passed to `context_status`). Defaults to 1.0 (healthy assumption) when checks are not performed.

**FR-104: Contradiction density dimension.** Compute as `1.0 - (total_quarantined / total_active)` using existing COUNTERS values. Clamp to [0.0, 1.0]. Returns 1.0 when total_active is 0.

**FR-105: Dimension score purity.** Each dimension score computation function is pure: deterministic given its inputs, no side effects, no I/O. Each function is independently unit-testable.

**FR-106: Named threshold constants.** All threshold constants -- staleness window, stale ratio trigger, lambda threshold, dimension weights -- are named f64 constants with descriptive names. No magic numbers in computation paths.

### FR-2xx: Confidence Refresh (Staleness Detection, Batch Refresh, Capping)

**FR-200: Staleness detection.** During `context_status`, scan active entries and identify those where `max(updated_at, last_accessed_at)` is older than the staleness threshold (default 24 hours as a named constant).

**FR-201: Confidence recomputation.** For each stale entry identified in FR-200, recompute confidence using the existing `compute_confidence(entry, now)` function and persist via `update_confidence(id, confidence)`.

**FR-202: Batch refresh capping.** Cap the number of entries refreshed per `context_status` call to a named constant (the architect determines the cap value). This bounds write latency on the diagnostic call path.

**FR-203: Refresh count reporting.** Report the number of entries refreshed in `StatusReport.confidence_refreshed_count: u64`.

**FR-204: Stale count reporting.** Report the total number of stale entries (before refresh) in `StatusReport.stale_confidence_count: u64`.

**FR-205: Maintenance opt-in.** When `StatusParams.maintain` is `true`, run confidence refresh writes and graph compaction. When `maintain` is absent or `false` (default), skip all maintenance writes — dimension scores are still computed (read-only). Default behavior is read-only diagnostics; maintenance requires explicit intent.

### FR-3xx: Graph Compaction (Stale Ratio Check, Rebuild, Atomicity)

**FR-300: Stale ratio threshold check.** During `context_status`, compute the stale node ratio as `VectorIndex::stale_count() / VectorIndex::point_count()`. If the ratio exceeds a configurable threshold (default 10%, named constant), trigger graph compaction.

**FR-301: HNSW index rebuild.** Graph compaction creates a new HNSW index from active entries only. The rebuild re-embeds active entries (requires embed service) or retrieves existing embeddings, constructs a fresh index, and inserts all active entry vectors.

**FR-302: Build-new-then-swap atomicity.** Compaction constructs the new index fully before replacing the old one. The old index is retained until the new index is validated and ready. If rebuild fails at any point (OOM, embed service error, hnsw_rs error), the old index remains intact and operational.

**FR-303: VECTOR_MAP update.** After successful index swap, update VECTOR_MAP entries with the new data IDs assigned by the rebuilt index. This runs within a write transaction.

**FR-304: Post-compaction invariant.** After successful compaction, `VectorIndex::stale_count()` returns 0.

**FR-305: Compaction reporting.** Report whether compaction ran in `StatusReport.graph_compacted: bool`. Report the current stale ratio in `StatusReport.graph_stale_ratio: f64`.

**FR-306: Embed service gating.** If the embed service is unavailable (lazy loading not yet triggered, model file missing), compaction is skipped. The stale ratio is still reported. A maintenance recommendation is emitted: "HNSW compaction skipped: embed service unavailable".

**FR-307: Maintenance opt-in for compaction.** Compaction only runs when `maintain: true` (FR-205). When maintenance is not opted in (default), skip compaction. The stale ratio is still computed and reported.

### FR-4xx: Maintenance Recommendations (Threshold, Actionable Text)

**FR-400: Lambda threshold trigger.** When lambda < a configurable threshold (default 0.8, named constant), generate maintenance recommendations.

**FR-401: Actionable recommendation text.** Each recommendation includes specific counts and context. Examples:
- "42 entries have stale confidence (oldest: 7 days)"
- "HNSW graph has 15% stale nodes (23 of 153) -- compaction recommended"
- "3 embedding inconsistencies detected"
- "Quarantine ratio: 8% (12 of 150 entries quarantined)"

**FR-402: Recommendation vector.** Recommendations are returned in `StatusReport.maintenance_recommendations: Vec<String>`. The vector is empty when lambda >= threshold.

**FR-403: Per-dimension recommendations.** Each dimension whose individual score falls below 1.0 and contributes to lambda being below threshold generates its own recommendation with dimension-specific detail.

### FR-5xx: f64 Scoring Upgrade

**FR-500: EntryRecord.confidence type promotion.** Change `EntryRecord.confidence` from f32 to f64. This is the only field change on EntryRecord.

**FR-501: Schema migration v2 to v3.** Add `migrate_v2_to_v3` function following the established migration pattern. The migration reads each entry's f32 confidence bytes, casts to f64 (`f32 as f64`, which is exact for IEEE 754), and writes back the entry with the f64 confidence value. The migration runs within a single write transaction (all-or-nothing). On completion, `schema_version` counter is set to 3.

**FR-502: Migration chain compatibility.** The migration framework handles v0->v3, v1->v3, and v2->v3 paths. Existing `migrate_v0_to_v1` and `migrate_v1_to_v2` continue to run for older databases, followed by the new `migrate_v2_to_v3`.

**FR-503: SearchResult.similarity type promotion.** Change `SearchResult.similarity` from f32 to f64 in `unimatrix-vector`. The HNSW distance (f32 from DistDot) is cast to f64 when constructing SearchResult.

**FR-504: Scoring constant promotion.** All scoring constants in `confidence.rs` are promoted from f32 to f64: `W_BASE`, `W_USAGE`, `W_FRESH`, `W_HELP`, `W_CORR`, `W_TRUST`, `W_COAC`, `SEARCH_SIMILARITY_WEIGHT`, and any boost cap constants. Values remain numerically identical.

**FR-505: compute_confidence return type.** `compute_confidence(entry: &EntryRecord, now: u64)` returns f64. Remove the `as f32` truncation at the return boundary. Internal computation already uses f64 (ADR-002).

**FR-506: update_confidence signature.** `Store::update_confidence(entry_id: u64, confidence: f64)` accepts f64. The store serializes the f64 value into the EntryRecord.

**FR-507: rerank_score signature.** `rerank_score(similarity: f64, confidence: f64) -> f64`. Operates entirely in f64.

**FR-508: co_access_affinity signature.** `co_access_affinity(partner_count: usize, avg_partner_confidence: f64) -> f64`. Input and output promoted to f64.

**FR-509: Embeddings remain f32.** ONNX pipeline, VectorIndex HNSW (`Hnsw<'static, f32, DistDot>`), and all embedding storage remain f32. The f64 promotion applies only to the scoring pipeline. The f32-to-f64 conversion happens at the SearchResult construction boundary.

**FR-510: StatusReport score fields.** All new coherence-related score fields on StatusReport are f64. Existing StatusReport fields that are not scores (counts, distributions, booleans) remain unchanged.

### FR-6xx: Response Formatting (Summary, Markdown, JSON)

**FR-600: JSON format coherence section.** The JSON response includes all coherence fields as top-level keys with f64 values serialized at full precision.

**FR-601: Markdown format coherence section.** The markdown response includes a "Coherence" section with lambda score, individual dimension scores, and maintenance recommendations formatted as a readable list.

**FR-602: Summary format coherence section.** The summary response includes a coherence line (e.g., "Coherence: 0.85 (confidence_freshness: 0.92, graph_quality: 1.0, embedding_consistency: 1.0, contradiction_density: 0.95)") and any maintenance recommendations.

**FR-603: Maintenance action reporting.** All three formats report maintenance actions taken during the call (entries refreshed, compaction performed) in appropriate format-specific style.

## Non-Functional Requirements

### NFR-01: Inline Maintenance Latency

Confidence refresh (FR-201) capped per call must complete within reasonable wall-clock time. At current scale (<1000 entries), a full refresh scan completes in <100ms. The batch cap (FR-202) ensures bounded latency regardless of knowledge base size.

### NFR-02: Graph Compaction Duration

Graph compaction (FR-301) is O(n log n) where n = active entries. At <1000 entries, compaction completes in <1 second. At 10K entries, estimated 5-10 seconds. Compaction runs only when the stale ratio exceeds threshold, not on every `context_status` call.

### NFR-03: Schema Migration Safety

Migration v2->v3 (FR-501) executes within a single redb write transaction. If the process crashes mid-migration, the transaction is not committed and the database retains the v2 schema. On next `Store::open`, the migration re-runs from scratch.

### NFR-04: Backward Compatibility

- All new `StatusParams` fields are `Option<T>`. Existing callers without the new parameter continue to work.
- Existing entries are readable after migration. Schema v2 data (f32 confidence) that has not yet been migrated deserializes correctly into the v3 schema via `#[serde(default)]` or the migration itself.
- `StatusReport` new fields are additive. Existing consumers that ignore unknown JSON keys are unaffected.

### NFR-05: Safety Constraints

`#![forbid(unsafe_code)]` maintained across all crates. No new crate dependencies beyond the existing workspace. Edition 2024, MSRV 1.89.

### NFR-06: Object Safety

Any trait method additions or changes (e.g., `update_confidence` signature) must maintain object safety for trait objects used in the server layer.

### NFR-07: No Background Threads

All maintenance operations execute inline during `context_status`. No background threads, no timers, no new async spawn patterns. This preserves the server's single-threaded stdio architecture.

### NFR-08: Test Continuity

All 811+ existing tests pass after the f64 upgrade. Test expected values and comparison tolerances are updated as part of the f64 promotion. Test infrastructure is cumulative; new tests build on existing fixtures and helpers.

## Domain Models

### Coherence Dimensions

```
CoherenceDimension := ConfidenceFreshness | GraphQuality | EmbeddingConsistency | ContradictionDensity

Each dimension produces a score: f64 in [0.0, 1.0]
  1.0 = fully healthy (no degradation detected)
  0.0 = fully degraded
```

| Dimension | Score = 1.0 | Score = 0.0 | Data Source |
|-----------|-------------|-------------|-------------|
| ConfidenceFreshness | All entries have confidence computed within staleness threshold | All entries are stale | ENTRIES scan: compare `max(updated_at, last_accessed_at)` to now |
| GraphQuality | No stale HNSW nodes | All nodes are stale | `VectorIndex::stale_count()` / `VectorIndex::point_count()` |
| EmbeddingConsistency | All entries pass self-similarity >= 0.99 | All entries inconsistent | `check_embedding_consistency()` results (opt-in) |
| ContradictionDensity | No quarantined entries | All entries quarantined | COUNTERS: `total_quarantined` / `total_active` |

### Lambda Formula

```
lambda = W_DIM_FRESH * confidence_freshness_score
       + W_DIM_GRAPH * graph_quality_score
       + W_DIM_EMBED * embedding_consistency_score
       + W_DIM_CONTRA * contradiction_density_score

where W_DIM_FRESH + W_DIM_GRAPH + W_DIM_EMBED + W_DIM_CONTRA = 1.0
All weights are named f64 constants.
```

### Scoring Pipeline Data Flow (f64 Boundaries)

```
ONNX model output (f32 embedding)
  |
  v
hnsw_rs distance (f32)  --->  1.0 - distance  --->  SearchResult.similarity (f64)
                                                          |
                                                          v
EntryRecord.confidence (f64, persisted)  ---------->  rerank_score(similarity: f64, confidence: f64) -> f64
  ^                                                       |
  |                                                       +---> co_access boost (f64) ---> final_score (f64)
  |
compute_confidence(entry, now) -> f64
  |
  +-- base_score(status) -> f64
  +-- usage_score(access_count) -> f64
  +-- freshness_score(last_accessed_at, created_at, now) -> f64
  +-- helpfulness_score(helpful, unhelpful) -> f64
  +-- correction_score(correction_count) -> f64
  +-- trust_score(source) -> f64

f32 boundary: ONLY at ONNX output and hnsw_rs internal operations.
f64 boundary: Everything from SearchResult construction onward.
```

### Staleness Model

```
staleness_age(entry, now) = now - max(entry.updated_at, entry.last_accessed_at)

is_stale(entry, now, threshold) = staleness_age(entry, now) > threshold

DEFAULT_STALENESS_THRESHOLD = 24 * 3600  (24 hours in seconds)
```

### Schema Version Progression

```
v0: Original 17-field EntryRecord (pre-nxs-004)
v1: 24-field EntryRecord (nxs-004: added last_accessed_at, access_count, supersedes, etc.)
v2: 26-field EntryRecord (crt-001: added helpful_count, unhelpful_count)
v3: confidence f32->f64 promotion (crt-005: no new fields, type change only)
```

## API Changes

### StatusParams Extension

```rust
pub struct StatusParams {
    pub topic: Option<String>,
    pub category: Option<String>,
    pub agent_id: Option<String>,
    pub format: Option<String>,
    pub check_embeddings: Option<bool>,
    // NEW (crt-005):
    /// Opt in to maintenance writes (confidence refresh, graph compaction).
    /// Default: false (read-only diagnostics). Set to true to run maintenance.
    pub maintain: Option<bool>,
}
```

The `maintain` parameter (per SR-07/ADR-002) controls whether `context_status` performs write operations. By default (`maintain` absent or `false`), the call is read-only — coherence scores are computed and recommendations are generated, but no writes occur. When `maintain` is `Some(true)`:
- Confidence refresh runs for stale entries (FR-201), capped per call (FR-202)
- Graph compaction triggers if stale ratio exceeds threshold (FR-300)
- `confidence_refreshed_count` and `graph_compacted` reflect actual actions taken

When `maintain` is absent or `Some(false)` (default):
- Dimension scores are computed (read-only)
- Lambda and recommendations are generated
- `confidence_refreshed_count` is 0
- `graph_compacted` is false
- Recommendations tell the caller what `maintain: true` would fix

### StatusReport Extension

```rust
pub struct StatusReport {
    // ... existing fields unchanged ...

    // NEW (crt-005): Coherence section
    /// Composite coherence score [0.0, 1.0]. 1.0 = fully healthy.
    pub coherence: f64,
    /// Confidence freshness dimension [0.0, 1.0].
    pub confidence_freshness_score: f64,
    /// Graph quality dimension [0.0, 1.0].
    pub graph_quality_score: f64,
    /// Embedding consistency dimension [0.0, 1.0].
    pub embedding_consistency_score: f64,
    /// Contradiction density dimension [0.0, 1.0].
    pub contradiction_density_score: f64,
    /// Entries with stale confidence (before refresh).
    pub stale_confidence_count: u64,
    /// Entries whose confidence was refreshed during this call.
    pub confidence_refreshed_count: u64,
    /// Current HNSW stale node ratio [0.0, 1.0].
    pub graph_stale_ratio: f64,
    /// Whether graph compaction ran during this call.
    pub graph_compacted: bool,
    /// Actionable maintenance recommendations (empty when lambda >= threshold).
    pub maintenance_recommendations: Vec<String>,
}
```

## Data Model Changes

### EntryRecord.confidence: f32 -> f64

**Before (schema v2):**
```rust
pub struct EntryRecord {
    // ...
    #[serde(default)]
    pub confidence: f32,  // 4 bytes in bincode
    // ...
}
```

**After (schema v3):**
```rust
pub struct EntryRecord {
    // ...
    #[serde(default)]
    pub confidence: f64,  // 8 bytes in bincode
    // ...
}
```

Migration v2->v3 reads each entry, deserializes with an intermediate struct that has `confidence: f32`, casts `f32 as f64` (lossless for IEEE 754), and re-serializes with the v3 schema. This follows the established pattern from `migrate_v1_to_v2`.

**Critical note per SR-01:** The migration cannot read v2 bytes directly as f64 -- bincode encodes f32 as 4 bytes and f64 as 8 bytes. The migration must use an intermediate v2 struct with `confidence: f32` to deserialize, then construct the v3 struct with `confidence: value as f64`.

### SearchResult.similarity: f32 -> f64

**Before:**
```rust
pub struct SearchResult {
    pub entry_id: u64,
    pub similarity: f32,
}
```

**After:**
```rust
pub struct SearchResult {
    pub entry_id: u64,
    pub similarity: f64,
}
```

This is an in-memory-only change (SearchResult is not persisted). The hnsw_rs distance (f32) is cast to f64 at the point where SearchResult is constructed: `similarity: (1.0_f64 - distance as f64)`.

### Store::update_confidence Signature

**Before:** `pub fn update_confidence(&self, entry_id: u64, confidence: f32) -> Result<()>`

**After:** `pub fn update_confidence(&self, entry_id: u64, confidence: f64) -> Result<()>`

### Scoring Constants

**Before (f32):**
```rust
pub const W_BASE: f32 = 0.18;
pub const W_USAGE: f32 = 0.14;
pub const W_FRESH: f32 = 0.18;
pub const W_HELP: f32 = 0.14;
pub const W_CORR: f32 = 0.14;
pub const W_TRUST: f32 = 0.14;
pub const W_COAC: f32 = 0.08;
pub const SEARCH_SIMILARITY_WEIGHT: f32 = 0.85;
```

**After (f64):**
```rust
pub const W_BASE: f64 = 0.18;
pub const W_USAGE: f64 = 0.14;
pub const W_FRESH: f64 = 0.18;
pub const W_HELP: f64 = 0.14;
pub const W_CORR: f64 = 0.14;
pub const W_TRUST: f64 = 0.14;
pub const W_COAC: f64 = 0.08;
pub const SEARCH_SIMILARITY_WEIGHT: f64 = 0.85;
```

**New constants (f64):**
```rust
// Coherence dimension weights (sum to 1.0)
pub const W_DIM_FRESH: f64 = ...;   // Architect determines values
pub const W_DIM_GRAPH: f64 = ...;
pub const W_DIM_EMBED: f64 = ...;
pub const W_DIM_CONTRA: f64 = ...;

// Thresholds
pub const DEFAULT_STALENESS_THRESHOLD_SECS: u64 = 86400;  // 24 hours
pub const DEFAULT_STALE_RATIO_TRIGGER: f64 = 0.10;         // 10%
pub const DEFAULT_LAMBDA_THRESHOLD: f64 = 0.80;            // Recommendation trigger
pub const DEFAULT_REFRESH_BATCH_CAP: u64 = ...;            // Architect determines
```

## Error Handling

### EH-01: Embed Service Unavailable During Compaction

If the embed service is not ready (lazy initialization not triggered) when graph compaction is needed:
- Compaction is skipped (FR-306)
- `graph_compacted` is false
- A maintenance recommendation is emitted: "HNSW compaction skipped: embed service unavailable"
- Lambda still computes with the graph quality dimension reflecting the current stale ratio
- `context_status` completes successfully (no error returned)

### EH-02: Mid-Compaction Failure

If compaction fails during HNSW rebuild (OOM, hnsw_rs panic caught, embedding failure):
- The old HNSW index remains intact and operational (FR-302: build-new-then-swap)
- `graph_compacted` is false
- A maintenance recommendation is emitted describing the failure
- `context_status` completes successfully with partial results
- Search continues to work with the pre-compaction index

### EH-03: Schema Migration Failure

If `migrate_v2_to_v3` fails (e.g., disk full during re-serialization):
- The write transaction is not committed (redb atomicity)
- The database retains schema v2
- `Store::open` returns an error
- On next `Store::open`, the migration re-attempts from scratch
- No partial migration state is possible (all-or-nothing transaction)

### EH-04: Confidence Refresh Write Failure

If `update_confidence` fails for an individual entry during batch refresh:
- The specific entry's confidence is not updated
- Other entries in the batch continue processing (best-effort)
- The failure is counted but does not abort `context_status`
- `confidence_refreshed_count` reflects only successful refreshes

### EH-05: Division by Zero Guards

All ratio computations guard against division by zero:
- `total_active == 0`: confidence freshness returns 1.0, contradiction density returns 1.0
- `point_count == 0`: graph quality returns 1.0
- `total_checked == 0`: embedding consistency returns 1.0 (already handled by opt-in default)

## User Workflows

### Workflow 1: Agent Checks Knowledge Base Health (Default — Read-Only)

```
Agent -> context_status()
  -> Computes all four dimension scores (read-only)
  -> No confidence refresh, no compaction (default: maintain absent)
  -> Returns StatusReport with:
     coherence: 0.72
     confidence_freshness_score: 0.78
     graph_quality_score: 0.82
     embedding_consistency_score: 1.0 (not checked)
     contradiction_density_score: 0.95
     stale_confidence_count: 42
     confidence_refreshed_count: 0
     graph_stale_ratio: 0.18
     graph_compacted: false
     maintenance_recommendations: [
       "42 entries have stale confidence (oldest: 7 days)",
       "HNSW graph has 18% stale nodes (27 of 150) -- run with maintain: true to compact"
     ]
```

### Workflow 2: Explicit Maintenance (Opt-In Writes)

```
Agent -> context_status(maintain: true)
  -> Computes all four dimension scores
  -> Refreshes stale confidence entries (up to batch cap)
  -> Checks stale ratio, triggers compaction if needed
  -> Returns StatusReport with:
     coherence: 0.87
     confidence_freshness_score: 0.92
     graph_quality_score: 0.85
     embedding_consistency_score: 1.0 (not checked)
     contradiction_density_score: 0.95
     stale_confidence_count: 42
     confidence_refreshed_count: 42
     graph_stale_ratio: 0.15
     graph_compacted: true
     maintenance_recommendations: []
```

### Workflow 3: Healthy Knowledge Base

```
Agent -> context_status()
  -> All dimensions score 1.0 or near 1.0
  -> No entries stale, no stale HNSW nodes
  -> Returns StatusReport with:
     coherence: 1.0
     maintenance_recommendations: []
     confidence_refreshed_count: 0
     graph_compacted: false
```

### Workflow 4: Embed Service Unavailable

```
Agent -> context_status(check_embeddings: true)
  -> Embed service not yet initialized
  -> Embedding consistency check skipped (existing behavior)
  -> Graph compaction needed but skipped (embed service unavailable)
  -> Returns StatusReport with:
     embedding_consistency_score: 1.0 (default, not checked)
     graph_compacted: false
     maintenance_recommendations: ["HNSW compaction skipped: embed service unavailable"]
```

## Acceptance Criteria Traceability

| AC-ID | Description | Functional Requirements | Verification |
|-------|-------------|------------------------|--------------|
| AC-01 | StatusReport includes `coherence: f64` in [0.0, 1.0] | FR-100, FR-510 | unit test |
| AC-02 | StatusReport includes four dimension scores, each f64 in [0.0, 1.0] | FR-101, FR-102, FR-103, FR-104, FR-510 | unit test |
| AC-03 | Confidence freshness uses `max(updated_at, last_accessed_at)` with configurable threshold | FR-101, FR-200, FR-106 | unit test |
| AC-04 | Graph quality = `1.0 - (stale_count / point_count)`, clamped | FR-102 | unit test |
| AC-05 | Embedding consistency defaults to 1.0 when not checked | FR-103 | unit test |
| AC-06 | Contradiction density = `1.0 - (total_quarantined / total_active)`, clamped | FR-104 | unit test |
| AC-07 | Lambda is weighted average with configurable weights summing to 1.0 | FR-100, FR-106 | unit test |
| AC-08 | Lambda < threshold triggers non-empty maintenance_recommendations | FR-400, FR-401, FR-402 | unit test |
| AC-09 | Stale entries have confidence recomputed during context_status | FR-200, FR-201 | integration test |
| AC-10 | confidence_refreshed_count reflects entries refreshed | FR-203 | integration test |
| AC-11 | Compaction triggered when stale ratio > threshold | FR-300 | integration test |
| AC-12 | Compaction rebuilds index from active entries only | FR-301 | integration test |
| AC-13 | Post-compaction: stale_count() == 0, VECTOR_MAP updated | FR-303, FR-304 | integration test |
| AC-14 | graph_compacted bool reports compaction status | FR-305 | unit test |
| AC-15 | Dimension score functions are pure and independently testable | FR-105 | unit test |
| AC-16 | All thresholds are named constants | FR-106 | code review / grep |
| AC-17 | Coherence section in all three response formats | FR-600, FR-601, FR-602 | unit test |
| AC-18 | Recommendations are specific with counts and context | FR-401 | unit test |
| AC-19 | Batch cap on confidence refresh per call | FR-202 | unit test |
| AC-20 | Search results identical before/after compaction (modulo stale removal) | FR-301, FR-302 | integration test |
| AC-21 | Unit tests for all new code; integration tests for end-to-end flows | all FR-xxx | test |
| AC-22 | Existing tests pass, no regressions | NFR-08 | test suite |
| AC-23 | `#![forbid(unsafe_code)]`, no new dependencies | NFR-05 | grep |
| AC-24 | No background threads, no timers, all inline | NFR-07 | code review |
| AC-25 | EntryRecord.confidence is f64, migration v2->v3 lossless | FR-500, FR-501 | integration test |
| AC-26 | SearchResult.similarity is f64 | FR-503 | unit test |
| AC-27 | All scoring constants are f64 | FR-504 | grep |
| AC-28 | compute_confidence returns f64 | FR-505 | unit test |
| AC-29 | update_confidence accepts f64 | FR-506 | unit test |
| AC-30 | rerank_score operates in f64 | FR-507 | unit test |
| AC-31 | Embeddings remain f32 | FR-509 | grep |
| AC-32 | Existing entries readable after migration | FR-501, FR-502 | integration test |

## Constraints

- **No new MCP tools.** Lambda is exposed via the existing `context_status` response.
- **No background threads.** The server has no scheduler. All maintenance runs inline during `context_status`.
- **No new crate dependencies.** All functionality uses existing workspace crates.
- **`#![forbid(unsafe_code)]`**, edition 2024, MSRV 1.89.
- **Object-safe traits.** Any trait extensions maintain object safety.
- **hnsw_rs does not support point deletion.** Compaction requires full HNSW index rebuild.
- **Schema migration v2->v3.** Single write transaction, all-or-nothing. Intermediate v2 struct required for deserialization (cannot read f32 bytes as f64).
- **No new redb tables.** All data sources for lambda already exist.
- **Confidence refresh is bounded.** Batch cap prevents unbounded write latency on the diagnostic path.
- **VectorStore trait is object-safe.** If compaction requires a new trait method, it must be object-safe.
- **Graph compaction requires embed service.** Graceful degradation when unavailable.
- **Test infrastructure is cumulative.** Build on existing 811+ test fixtures and helpers.

## Dependencies

- `unimatrix-store` -- EntryRecord schema change (f32->f64), migration v2->v3, `update_confidence` signature change
- `unimatrix-vector` -- SearchResult.similarity f64, potential compaction method on VectorIndex
- `unimatrix-core` -- trait re-exports propagating type changes
- `unimatrix-server` -- coherence module, confidence.rs constant/signature changes, StatusReport extension, StatusParams extension, response formatting
- `unimatrix-embed` -- embed service readiness check for compaction gating (existing infrastructure)
- `redb` -- write transactions for migration and confidence refresh (existing dependency)
- `bincode` -- serialization of f64 confidence (existing dependency)

## NOT in Scope

- **No new MCP tools.** No `context_coherence` or `context_maintenance` tool.
- **No background maintenance scheduler.** All maintenance is inline during `context_status`.
- **No automatic quarantine from coherence signals.** Low lambda triggers recommendations, not automatic actions.
- **No new fields on EntryRecord** beyond the `confidence` type promotion from f32 to f64.
- **No new redb tables.** All data needed for lambda computation already exists.
- **No confidence refresh during `context_search`.** Refresh is limited to `context_status` only.
- **No persistence of lambda history.** Coherence score is computed on demand. Historical trending is mtx-phase work.
- **No pi-based calibration of scoring weights.** The f64 upgrade enables it; actual calibration is future work.
- **No embedding precision changes.** Embeddings remain f32. Only the scoring pipeline is upgraded.
- **No adaptive precision lanes.** Multi-precision embedding storage is a future concern.
- **No graph compaction during `context_search`.** Compaction is expensive and only runs during `context_status`.

## Open Questions for Architect

1. **Dimension weight values.** SCOPE proposes equal weighting (0.25 each) as a default. However, embedding consistency requires opt-in and defaults to 1.0 when unchecked, which inflates lambda (SR-08). Should the architect consider excluding unavailable dimensions from the weighted average or use unequal weights that de-emphasize opt-in dimensions?

2. **Compaction embedding source.** Should compaction re-embed from stored content (more robust, uses current model, requires embed service) or read raw embeddings from hnsw_rs (faster, may not be API-supported)? The hnsw_rs API's ability to expose raw vectors for specific data points needs verification.

3. **Batch refresh cap value.** What is the appropriate cap for confidence entries refreshed per `context_status` call? Too low and staleness persists across many calls; too high and diagnostic latency increases. The architect should determine based on expected call frequency and entry count growth.

4. **f64 test update strategy.** With 811+ tests, many have hardcoded f32 values and f32 comparison assertions (SR-09). Should the f64 upgrade be done as the first implementation step (Tier 1 per SR-06) so all subsequent crt-005 work builds on the new types?
