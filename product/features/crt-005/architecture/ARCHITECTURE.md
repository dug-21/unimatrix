# Architecture: crt-005 Coherence Gate

## System Overview

crt-005 introduces a composite coherence metric (lambda) that unifies four independent health signals into a single [0.0, 1.0] score exposed via `context_status`. It also upgrades the entire scoring pipeline from f32 to f64, adds lazy confidence refresh and HNSW graph compaction as inline maintenance during `context_status`, and transitions `context_status` from a read-only diagnostic tool to a read-write maintenance tool with opt-out.

This feature touches every crate in the workspace:
- **unimatrix-store**: Schema migration v2->v3 (EntryRecord.confidence f32->f64), update_confidence signature change
- **unimatrix-vector**: SearchResult.similarity f32->f64, new `compact` method on VectorIndex
- **unimatrix-core**: VectorStore trait update (SearchResult type change), new VectorStore method
- **unimatrix-embed**: No changes (embeddings remain f32)
- **unimatrix-server**: New coherence module, f64 weight constants, StatusReport extension, confidence refresh, graph compaction trigger, maintenance parameter

crt-005 is the capstone of the Cortical (M4) phase and the direct prerequisite for col-002 (Retrospective Pipeline), which needs reliable knowledge quality signals.

## Delivery Tiers

Per SR-06 (scope boundary risk), the feature is split into two independently coherent delivery tiers.

### Tier 1: f64 Upgrade + Lambda Read-Only

Components: C1, C2, C3, C4, C6, C7 (partial)

Delivers the f64 scoring upgrade across all crates, schema migration v2->v3, and the coherence module with all four dimension score functions. Lambda is computed and reported in `context_status` but no maintenance writes occur. This tier is safe and low-risk: it is a type-level refactor plus pure computation.

### Tier 2: Confidence Refresh + Graph Compaction + Maintenance

Components: C5, C7 (complete), C8

Adds the write-side behavior: lazy confidence refresh during `context_status`, HNSW graph compaction trigger, and the `maintenance` opt-out parameter. This tier changes the behavioral contract of `context_status` from read-only to read-write.

## Component Breakdown

### C1: Schema Migration v2 -> v3 (store crate)

**Responsibility**: Migrate `EntryRecord.confidence` from f32 to f64.

**Approach**: Follow the established migration pattern (ADR-003 crt-001, ADR-005 nxs-004):

1. Define `V2EntryRecord` -- a 26-field struct matching the current schema with `confidence: f32`.
2. In `migrate_v2_to_v3`: scan all entries, deserialize with `V2EntryRecord`, construct new `EntryRecord` with `confidence: v2.confidence as f64` (lossless per IEEE 754), serialize and overwrite.
3. Bump `CURRENT_SCHEMA_VERSION` from 2 to 3.
4. Add `if current_version < 3 { migrate_v2_to_v3(&txn)?; }` to `migrate_if_needed`.
5. The entire migration runs in a single redb write transaction -- atomic by construction. If any entry fails, the transaction rolls back and the database remains at v2.

**Atomicity guarantee (SR-01)**: redb write transactions are all-or-nothing. The schema_version counter is updated in the same transaction as the entry rewrites. If the process crashes mid-migration, the uncommitted transaction is discarded and the next `Store::open` retries the migration from v2.

**Files modified**:
- `crates/unimatrix-store/src/schema.rs` -- `EntryRecord.confidence` type: `f32` -> `f64`
- `crates/unimatrix-store/src/migration.rs` -- `V2EntryRecord` struct, `migrate_v2_to_v3`, bump `CURRENT_SCHEMA_VERSION` to 3

### C2: f64 Scoring Constants (server crate)

**Responsibility**: Promote all scoring constants from f32 to f64.

**Exhaustive f32 -> f64 change inventory (per SR-02)**:

#### confidence.rs
| Constant/Function | Current Type | New Type | Notes |
|---|---|---|---|
| `W_BASE` | `f32 (0.18)` | `f64` | Weight constant |
| `W_USAGE` | `f32 (0.14)` | `f64` | Weight constant |
| `W_FRESH` | `f32 (0.18)` | `f64` | Weight constant |
| `W_HELP` | `f32 (0.14)` | `f64` | Weight constant |
| `W_CORR` | `f32 (0.14)` | `f64` | Weight constant |
| `W_TRUST` | `f32 (0.14)` | `f64` | Weight constant |
| `W_COAC` | `f32 (0.08)` | `f64` | Weight constant |
| `SEARCH_SIMILARITY_WEIGHT` | `f32 (0.85)` | `f64` | Re-ranking blend weight |
| `compute_confidence` return | `f32` | `f64` | Remove `as f32` on line 171 |
| `rerank_score` params/return | `f32, f32 -> f32` | `f64, f64 -> f64` | Full signature change |
| `co_access_affinity` params/return | `usize, f32 -> f32` | `usize, f64 -> f64` | Full signature change |

#### coaccess.rs
| Constant/Function | Current Type | New Type | Notes |
|---|---|---|---|
| `MAX_CO_ACCESS_BOOST` | `f32 (0.03)` | `f64` | Boost cap |
| `MAX_BRIEFING_CO_ACCESS_BOOST` | `f32 (0.01)` | `f64` | Briefing boost cap |
| `co_access_boost` (private) | `u32, f32 -> f32` | `u32, f64 -> f64` | Internal formula |
| `compute_search_boost` return value type | `HashMap<u64, f32>` | `HashMap<u64, f64>` | Boost map |
| `compute_briefing_boost` return value type | `HashMap<u64, f32>` | `HashMap<u64, f64>` | Boost map |
| `compute_boost_internal` | `..., f32 -> HashMap<u64, f32>` | `..., f64 -> HashMap<u64, f64>` | Internal |

#### contradiction.rs
| Constant/Function | Current Type | New Type | Notes |
|---|---|---|---|
| `SIMILARITY_THRESHOLD` | `f32` | `f32` | **No change** -- feeds into HNSW comparison which is f32 |
| `DEFAULT_CONFLICT_SENSITIVITY` | `f32` | `f32` | **No change** -- heuristic comparison, not scoring |
| `EMBEDDING_CONSISTENCY_THRESHOLD` | `f32` | `f32` | **No change** -- compared against f32 HNSW similarity |
| `NEGATION_WEIGHT`, `DIRECTIVE_WEIGHT`, `SENTIMENT_WEIGHT` | `f32` | `f32` | **No change** -- conflict heuristic, not scoring pipeline |
| `ContradictionPair.similarity` | `f32` | `f32` | **No change** -- comes from HNSW which is f32 |
| `ContradictionPair.conflict_score` | `f32` | `f32` | **No change** -- heuristic output |
| `EmbeddingInconsistency.expected_similarity` | `f32` | `f32` | **No change** -- from HNSW |
| `ContradictionConfig` fields | `f32` | `f32` | **No change** -- HNSW domain |

**Rationale for contradiction.rs staying f32**: The contradiction and embedding consistency modules compare against HNSW similarity scores, which are inherently f32 (hnsw_rs uses f32 internally via DistDot). Promoting these to f64 would add unnecessary casts with no precision benefit. The f64 upgrade applies to the **scoring pipeline** (confidence, re-ranking, co-access boost), not the **detection pipeline** (contradiction heuristic, embedding consistency).

#### schema.rs (store crate)
| Field | Current Type | New Type | Notes |
|---|---|---|---|
| `EntryRecord.confidence` | `f32` | `f64` | Requires schema migration |

#### index.rs (vector crate)
| Field | Current Type | New Type | Notes |
|---|---|---|---|
| `SearchResult.similarity` | `f32` | `f64` | In-memory only, no persistence |
| `map_neighbours_to_results` | `1.0 - n.distance` as f32 | `(1.0 - n.distance as f64)` | Cast at conversion boundary |

#### traits.rs (core crate)
| Function | Current | New | Notes |
|---|---|---|---|
| `VectorStore::search` return | `Vec<SearchResult>` | `Vec<SearchResult>` | Type change via SearchResult |
| `VectorStore::search_filtered` return | `Vec<SearchResult>` | `Vec<SearchResult>` | Type change via SearchResult |

#### write.rs (store crate)
| Function | Current | New | Notes |
|---|---|---|---|
| `Store::update_confidence` | `fn(..., f32)` | `fn(..., f64)` | Signature change |
| `Store::insert` initial confidence | `confidence: 0.0` | `confidence: 0.0` | Type inferred from field |

#### tools.rs (server crate)
| Call site | Change | Notes |
|---|---|---|
| `compute_confidence` calls | Return f64, no `as f32` | ~6 call sites |
| `rerank_score` calls | Pass f64 similarity + confidence | context_search, context_briefing |
| `co_access_affinity` calls | Pass f64 avg_partner_confidence | confidence computation path |
| `compute_search_boost` / `compute_briefing_boost` | Boost map values are f64 | Addition to rerank_score |
| `update_confidence` calls | Pass f64 | ~8 call sites across tools.rs and server.rs |

**Files modified**:
- `crates/unimatrix-server/src/confidence.rs` -- all constants and function signatures
- `crates/unimatrix-server/src/coaccess.rs` -- boost constants and function signatures
- `crates/unimatrix-vector/src/index.rs` -- SearchResult.similarity type, conversion in map_neighbours_to_results
- `crates/unimatrix-core/src/traits.rs` -- VectorStore trait return types (implicitly via SearchResult)
- `crates/unimatrix-store/src/write.rs` -- update_confidence signature
- `crates/unimatrix-server/src/tools.rs` -- all call sites

**Test blast radius (SR-09)**: Approximately 60-80 tests reference f32 confidence values, f32 similarity comparisons, or f32 weight constants. These fall into categories:
1. **Weight sum invariant tests** (confidence.rs) -- change `f32` literals to `f64`
2. **Confidence computation tests** (confidence.rs) -- change `as f64` comparisons, remove `as f64` on result
3. **Rerank score tests** (confidence.rs) -- change parameter types from `f32` to `f64`
4. **Co-access boost tests** (coaccess.rs) -- change constants and comparison types
5. **Schema tests** (schema.rs) -- change `confidence: 0.95` etc from f32 to f64
6. **Integration tests** (server.rs, tools.rs) -- update `update_confidence` call sites
7. **Vector index tests** (index.rs) -- `results[0].similarity` comparisons

All test changes are mechanical type promotions. No logic changes required.

### C3: VectorIndex Compaction (vector crate)

**Responsibility**: Add a `compact` method to VectorIndex that rebuilds the HNSW graph from active entries only, eliminating stale routing nodes.

**Design (per SR-03, build-new-then-swap)**:

```
fn compact(&self, embeddings: Vec<(u64, Vec<f32>)>) -> Result<()>
```

1. Accept a pre-built list of `(entry_id, embedding)` pairs. The caller (server crate) is responsible for obtaining embeddings via the embed service. This keeps VectorIndex independent of the embed service.
2. Create a fresh `Hnsw<'static, f32, DistDot>` with the same configuration parameters.
3. Insert all provided embeddings into the new HNSW graph, generating new data_ids starting from 0.
4. Build a new IdMap from the insertions.
5. **Swap atomically**: acquire write locks on both `self.hnsw` and `self.id_map`, replace both, reset `next_data_id`.
6. Update VECTOR_MAP entries in the store: for each `(entry_id, new_data_id)`, call `store.put_vector_mapping`. This runs in a single write transaction.
7. The old HNSW graph and IdMap are dropped when the replaced values go out of scope.

**Failure modes**:
- If HNSW construction panics (OOM): The old index is untouched because we build-new-then-swap. The method returns an error.
- If VECTOR_MAP update fails after swap: The in-memory index is correct but VECTOR_MAP is stale. On next server restart, persistence reload will reconstruct from VECTOR_MAP. To mitigate, VECTOR_MAP updates run in a single transaction -- if any fails, all are rolled back, and we also roll back the in-memory swap.

**Object safety**: `compact` takes `&self` (not `&mut self`) and uses interior mutability via `RwLock`. This is consistent with `insert` and maintains object safety for the `VectorStore` trait.

**VectorStore trait extension**: Add `compact(&self, embeddings: Vec<(u64, Vec<f32>)>) -> Result<(), CoreError>` to the `VectorStore` trait. This is object-safe (`&self`, no generics, concrete return type).

**Files modified**:
- `crates/unimatrix-vector/src/index.rs` -- `VectorIndex::compact` method
- `crates/unimatrix-core/src/traits.rs` -- `VectorStore::compact` trait method

### C4: Coherence Module (server crate)

**Responsibility**: Compute individual dimension scores and the composite lambda metric.

New module: `crates/unimatrix-server/src/coherence.rs`

**Dimension score functions** (all pure, no side effects):

```rust
/// Confidence freshness: ratio of non-stale entries to total active.
/// An entry is stale if max(updated_at, last_accessed_at) < now - staleness_threshold.
fn confidence_freshness_score(
    entries: &[EntryRecord],
    now: u64,
    staleness_threshold_secs: u64,
) -> (f64, u64)  // (score, stale_count)

/// Graph quality: 1.0 - (stale_count / point_count), clamped to [0.0, 1.0].
/// Returns 1.0 when point_count is 0.
fn graph_quality_score(stale_count: usize, point_count: usize) -> f64

/// Embedding consistency: 1.0 - (inconsistent_count / total_checked).
/// Returns 1.0 when total_checked is 0 (embedding check not performed).
fn embedding_consistency_score(inconsistent_count: usize, total_checked: usize) -> f64

/// Contradiction density: 1.0 - (total_quarantined / total_active), clamped to [0.0, 1.0].
/// Returns 1.0 when total_active is 0.
fn contradiction_density_score(total_quarantined: u64, total_active: u64) -> f64
```

**Composite lambda** (see ADR-003 for weight strategy):

```rust
/// Composite coherence score as weighted average of available dimensions.
fn compute_lambda(
    freshness: f64,
    graph_quality: f64,
    embedding_consistency: Option<f64>,  // None if check not performed
    contradiction_density: f64,
    weights: &CoherenceWeights,
) -> f64
```

Unavailable dimensions (embedding consistency when `check_embeddings` is false) are **excluded from the weighted average** rather than defaulted to 1.0. The remaining weights are re-normalized to sum to 1.0. See ADR-003 for the rationale.

**Maintenance recommendation generation**:

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

Returns an empty vec when lambda >= threshold. Otherwise returns specific, actionable recommendations.

**Named constants** (AC-16):

```rust
/// Default staleness threshold: 24 hours in seconds.
pub const DEFAULT_STALENESS_THRESHOLD_SECS: u64 = 24 * 3600;

/// Default stale ratio trigger for graph compaction: 10%.
pub const DEFAULT_STALE_RATIO_TRIGGER: f64 = 0.10;

/// Default lambda threshold for maintenance recommendations.
pub const DEFAULT_LAMBDA_THRESHOLD: f64 = 0.8;

/// Maximum entries to refresh per context_status call.
pub const MAX_CONFIDENCE_REFRESH_BATCH: usize = 100;

/// Default coherence weights.
pub const DEFAULT_WEIGHTS: CoherenceWeights = CoherenceWeights {
    confidence_freshness: 0.35,
    graph_quality: 0.30,
    embedding_consistency: 0.15,
    contradiction_density: 0.20,
};
```

**Files created**: `crates/unimatrix-server/src/coherence.rs`
**Files modified**: `crates/unimatrix-server/src/lib.rs` (module declaration)

### C5: Confidence Refresh (server crate)

**Responsibility**: Recompute stale confidence values during `context_status`.

**Approach**: After the read transaction (step 5), before building the final report:

1. The ENTRIES scan from step 5d already reads all entries. Identify stale entries: `max(updated_at, last_accessed_at) < now - staleness_threshold`.
2. Sort stale entries by staleness (oldest first).
3. Take at most `MAX_CONFIDENCE_REFRESH_BATCH` (100) entries per call.
4. For each stale entry, call `compute_confidence(entry, now)` and `store.update_confidence(entry.id, new_confidence)`.
5. Each `update_confidence` is a separate write transaction (existing pattern). This is acceptable because confidence writes are ENTRIES-only (no index diffs) and at most 100 per call.
6. Report `confidence_refreshed_count` in StatusReport.

**Gating**: Refresh only runs when the `maintenance` parameter is true (default). When false, stale counts are computed and reported but no writes occur.

**Files modified**: `crates/unimatrix-server/src/tools.rs` (context_status handler)

### C6: StatusReport Extension (server crate)

**Responsibility**: Add coherence fields to StatusReport.

New fields on `StatusReport`:

```rust
/// Composite coherence score [0.0, 1.0].
pub coherence: f64,
/// Confidence freshness dimension [0.0, 1.0].
pub confidence_freshness_score: f64,
/// Graph quality dimension [0.0, 1.0].
pub graph_quality_score: f64,
/// Embedding consistency dimension [0.0, 1.0]; 1.0 if check not performed.
pub embedding_consistency_score: f64,
/// Contradiction density dimension [0.0, 1.0].
pub contradiction_density_score: f64,
/// Entries with stale confidence.
pub stale_confidence_count: u64,
/// Entries refreshed during this call.
pub confidence_refreshed_count: u64,
/// Current HNSW stale node ratio.
pub graph_stale_ratio: f64,
/// Whether HNSW compaction ran during this call.
pub graph_compacted: bool,
/// Actionable maintenance recommendations (empty when lambda >= threshold).
pub maintenance_recommendations: Vec<String>,
```

**Response formatting**: Extend `format_status_report` to include a coherence section in all three formats (summary, markdown, json). The coherence section appears after the existing sections.

**Files modified**: `crates/unimatrix-server/src/response.rs` (StatusReport struct, format_status_report)

### C7: StatusParams Extension (server crate)

**Responsibility**: Add `maintenance` parameter to StatusParams.

```rust
/// Opt-in/opt-out for maintenance writes (default: true).
/// When false, coherence scores are computed but no confidence refresh
/// or graph compaction occurs.
pub maintenance: Option<bool>,
```

**Files modified**: `crates/unimatrix-server/src/tools.rs` (StatusParams struct)

### C8: Graph Compaction Integration (server crate)

**Responsibility**: Trigger HNSW graph compaction during `context_status` when the stale ratio exceeds the threshold.

**Approach**: After co-access cleanup (step 5g), add step 5h:

1. Read stale ratio from `vector_index.stale_count()` / `vector_index.point_count()`.
2. If `stale_ratio > DEFAULT_STALE_RATIO_TRIGGER` AND `maintenance` is true:
   a. Check embed service availability. If unavailable, skip compaction and add recommendation "compaction skipped: embed service unavailable" (per SR-04).
   b. Read all active entries from store (already available from step 5d).
   c. Re-embed each active entry via `embed_service.embed_entries(...)`.
   d. Call `vector_index.compact(embeddings)`.
   e. Set `report.graph_compacted = true`.
3. If stale_ratio <= threshold or maintenance is false, set `report.graph_compacted = false`.

**Embed service dependency (SR-04)**: The embed service is lazily loaded. Compaction is gated on `embed_service.get_adapter().await` succeeding. If it fails, compaction is skipped with a recommendation in the report. No panic, no error -- graceful degradation.

**Files modified**: `crates/unimatrix-server/src/tools.rs` (context_status handler)

## Component Interactions

```
Agent (MCP caller)
  |
  v
context_status(maintenance: true, check_embeddings: false)
  |
  +-- (1) Identity + Capability check [existing]
  +-- (2) Validation [existing, extended for maintenance param]
  +-- (3) Read transaction: counters, distributions, correction metrics [existing]
  |       +-- Identify stale entries (C4: confidence_freshness_score)
  +-- (4) Contradiction scanning [existing]
  +-- (5) Embedding consistency check if opted in [existing]
  +-- (6) Compute dimension scores from available data [NEW: C4]
  |       +-- confidence_freshness_score (from step 3 entry scan)
  |       +-- graph_quality_score (from vector_index.stale_count/point_count)
  |       +-- embedding_consistency_score (from step 5 results, or None)
  |       +-- contradiction_density_score (from counters)
  +-- (7) Confidence refresh for stale entries [NEW: C5]
  |       +-- compute_confidence per stale entry
  |       +-- update_confidence per entry (capped at 100)
  +-- (8) Co-access stats and cleanup [existing, crt-004]
  +-- (9) HNSW compaction if stale ratio exceeds threshold [NEW: C8]
  |       +-- embed_service.embed_entries(active_entries)
  |       +-- vector_index.compact(embeddings)
  +-- (10) Compute composite lambda [NEW: C4]
  +-- (11) Generate maintenance recommendations [NEW: C4]
  +-- (12) Build StatusReport with coherence fields [C6]
  +-- (13) Format and return response
```

## Technology Decisions

### ADR-001: f64 Scoring Boundary

Why scoring is f64 while embeddings stay f32. See `architecture/ADR-001-f64-scoring-boundary.md`.

### ADR-002: Maintenance Opt-Out on context_status

Why `context_status` gains a `maintenance` parameter. See `architecture/ADR-002-maintenance-opt-out.md`.

### ADR-003: Lambda Dimension Weighting Strategy

How unavailable dimensions are handled and why weights are unequal. See `architecture/ADR-003-lambda-dimension-weights.md`.

### ADR-004: Graph Compaction Atomicity

Build-new-then-swap design for HNSW compaction. See `architecture/ADR-004-graph-compaction-atomicity.md`.

## Integration Points

### Store Crate (unimatrix-store)

- `EntryRecord.confidence` changes from `f32` to `f64` -- affects every code path that reads or writes entries
- `Store::update_confidence` signature changes from `fn(..., f32)` to `fn(..., f64)`
- Schema migration v2->v3 runs on `Store::open` for existing databases
- No new tables, no new exports beyond the type change

### Vector Crate (unimatrix-vector)

- `SearchResult.similarity` changes from `f32` to `f64`
- New `VectorIndex::compact` method
- `map_neighbours_to_results` converts `f32` hnsw_rs distance to `f64` similarity at the boundary

### Core Crate (unimatrix-core)

- `VectorStore` trait gains `compact` method
- `VectorStore::search` and `search_filtered` return types change implicitly (via SearchResult)
- Object safety preserved: `compact(&self, ...)` with concrete types

### Server Crate (unimatrix-server)

- New module: `coherence.rs`
- `confidence.rs`: All constants and function signatures change to f64
- `coaccess.rs`: All boost constants and function signatures change to f64
- `response.rs`: StatusReport gains 10 new fields, format_status_report extended
- `tools.rs`: StatusParams gains `maintenance` field, context_status handler gains steps 6-11

## Integration Surface

| Integration Point | Type/Signature | Source |
|---|---|---|
| `EntryRecord.confidence` | `f64` (was `f32`) | `crates/unimatrix-store/src/schema.rs` |
| `Store::update_confidence` | `fn(&self, u64, f64) -> Result<()>` (was `f32`) | `crates/unimatrix-store/src/write.rs` |
| `CURRENT_SCHEMA_VERSION` | `3` (was `2`) | `crates/unimatrix-store/src/migration.rs` |
| `SearchResult.similarity` | `f64` (was `f32`) | `crates/unimatrix-vector/src/index.rs` |
| `VectorIndex::compact` | `fn(&self, Vec<(u64, Vec<f32>)>) -> Result<()>` | `crates/unimatrix-vector/src/index.rs` (new) |
| `VectorStore::compact` | `fn(&self, Vec<(u64, Vec<f32>)>) -> Result<(), CoreError>` | `crates/unimatrix-core/src/traits.rs` (new) |
| `W_BASE..W_COAC` | `f64` (was `f32`) | `crates/unimatrix-server/src/confidence.rs` |
| `SEARCH_SIMILARITY_WEIGHT` | `f64` (was `f32`) | `crates/unimatrix-server/src/confidence.rs` |
| `compute_confidence` | `fn(&EntryRecord, u64) -> f64` (was `-> f32`) | `crates/unimatrix-server/src/confidence.rs` |
| `rerank_score` | `fn(f64, f64) -> f64` (was `f32`) | `crates/unimatrix-server/src/confidence.rs` |
| `co_access_affinity` | `fn(usize, f64) -> f64` (was `f32`) | `crates/unimatrix-server/src/confidence.rs` |
| `MAX_CO_ACCESS_BOOST` | `f64` (was `f32`) | `crates/unimatrix-server/src/coaccess.rs` |
| `MAX_BRIEFING_CO_ACCESS_BOOST` | `f64` (was `f32`) | `crates/unimatrix-server/src/coaccess.rs` |
| `compute_search_boost` | `fn(...) -> HashMap<u64, f64>` (was `f32`) | `crates/unimatrix-server/src/coaccess.rs` |
| `compute_briefing_boost` | `fn(...) -> HashMap<u64, f64>` (was `f32`) | `crates/unimatrix-server/src/coaccess.rs` |
| `StatusParams.maintenance` | `Option<bool>` (new field) | `crates/unimatrix-server/src/tools.rs` |
| `StatusReport.coherence` | `f64` (new field) | `crates/unimatrix-server/src/response.rs` |
| `StatusReport.confidence_freshness_score` | `f64` (new field) | `crates/unimatrix-server/src/response.rs` |
| `StatusReport.graph_quality_score` | `f64` (new field) | `crates/unimatrix-server/src/response.rs` |
| `StatusReport.embedding_consistency_score` | `f64` (new field) | `crates/unimatrix-server/src/response.rs` |
| `StatusReport.contradiction_density_score` | `f64` (new field) | `crates/unimatrix-server/src/response.rs` |
| `StatusReport.stale_confidence_count` | `u64` (new field) | `crates/unimatrix-server/src/response.rs` |
| `StatusReport.confidence_refreshed_count` | `u64` (new field) | `crates/unimatrix-server/src/response.rs` |
| `StatusReport.graph_stale_ratio` | `f64` (new field) | `crates/unimatrix-server/src/response.rs` |
| `StatusReport.graph_compacted` | `bool` (new field) | `crates/unimatrix-server/src/response.rs` |
| `StatusReport.maintenance_recommendations` | `Vec<String>` (new field) | `crates/unimatrix-server/src/response.rs` |
| `confidence_freshness_score` | `fn(&[EntryRecord], u64, u64) -> (f64, u64)` | `crates/unimatrix-server/src/coherence.rs` (new) |
| `graph_quality_score` | `fn(usize, usize) -> f64` | `crates/unimatrix-server/src/coherence.rs` (new) |
| `embedding_consistency_score` | `fn(usize, usize) -> f64` | `crates/unimatrix-server/src/coherence.rs` (new) |
| `contradiction_density_score` | `fn(u64, u64) -> f64` | `crates/unimatrix-server/src/coherence.rs` (new) |
| `compute_lambda` | `fn(f64, f64, Option<f64>, f64, &CoherenceWeights) -> f64` | `crates/unimatrix-server/src/coherence.rs` (new) |
| `generate_recommendations` | `fn(f64, f64, u64, u64, f64, usize, u64) -> Vec<String>` | `crates/unimatrix-server/src/coherence.rs` (new) |
| `format_status_report` | Gains coherence section in all formats | `crates/unimatrix-server/src/response.rs` |

## Data Flow

### f64 Scoring Pipeline (after upgrade)

```
EntryRecord.confidence: f64  <-- stored in ENTRIES (bincode, 8 bytes)
                |
                v
compute_confidence(&entry, now) -> f64  <-- pure function, no truncation
                |
                v
store.update_confidence(id, f64)  <-- write to ENTRIES

SearchResult.similarity: f64  <-- converted from f32 hnsw_rs distance
                |
                v
rerank_score(similarity: f64, confidence: f64) -> f64
    + co_access_boost: f64
    = final_score: f64  <-- used for result ordering
```

### Coherence Computation

```
context_status(maintenance: true)
  |
  +-- Read ENTRIES (step 5d) --> [EntryRecord...]
  |       |
  |       +-- confidence_freshness_score(entries, now, threshold) --> (freshness_dim, stale_count)
  |
  +-- vector_index.stale_count() / point_count() --> graph_quality_score --> graph_dim
  |
  +-- embedding_inconsistencies.len() / total_checked --> embedding_dim (or None)
  |
  +-- total_quarantined / total_active --> contradiction_dim
  |
  +-- compute_lambda(freshness, graph, embedding, contradiction, weights) --> lambda
  |
  +-- generate_recommendations(lambda, threshold, ...) --> Vec<String>
```

### Graph Compaction

```
context_status detects stale_ratio > 0.10
  |
  +-- embed_service.get_adapter() --> Ok(adapter)
  |       |
  |       +-- read all active entries from store
  |       +-- adapter.embed_entries(title+content pairs) --> Vec<Vec<f32>>
  |       +-- vector_index.compact(entry_id, embedding pairs)
  |               |
  |               +-- Build new Hnsw<f32, DistDot> with same config
  |               +-- Insert all embeddings into new graph
  |               +-- Build new IdMap
  |               +-- Acquire write locks on self.hnsw and self.id_map
  |               +-- Swap old -> new (atomically under locks)
  |               +-- Update VECTOR_MAP in single write transaction
  |               +-- Reset next_data_id
  |               +-- Drop old graph + old IdMap
  |
  +-- report.graph_compacted = true
```

## Error Boundaries

| Error | Origin | Handling |
|---|---|---|
| Schema migration v2->v3 fails (corrupt entry) | `migrate_v2_to_v3` | Transaction rolls back. `Store::open` returns error. Database remains at v2. |
| Confidence refresh fails for one entry | `update_confidence` | Log warning, skip entry, continue with remaining batch. Report partial refresh count. |
| Embed service unavailable for compaction | `embed_service.get_adapter()` | Skip compaction. Add recommendation to report. Set `graph_compacted = false`. |
| HNSW rebuild fails (OOM) | `VectorIndex::compact` | Old index is untouched (build-new-then-swap). Log error, skip compaction. |
| VECTOR_MAP update fails after swap | `compact` internal | Roll back in-memory swap. The method restores old HNSW and IdMap. |

## Open Questions

1. **Confidence refresh batch cap**: 100 entries per call is proposed. At current scale (<200 active entries), this refreshes everything in one call. At larger scale, multiple `context_status` calls would be needed to fully refresh. The batch cap is a named constant and easily adjustable.

2. **VECTOR_MAP transaction in compact**: The current design updates VECTOR_MAP entries in a single write transaction after the in-memory swap. An alternative is to batch the VECTOR_MAP writes in chunks if entry count is large. At current scale this is unnecessary.
