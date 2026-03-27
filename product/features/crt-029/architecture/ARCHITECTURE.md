# Architecture: crt-029 — Background Graph Inference

## System Overview

The knowledge graph grows `Supports` and `Contradicts` edges today only via the post-store NLI
path (`run_post_store_nli`), which fires once per `context_store` call. This leaves entries
stored before NLI was enabled, and cross-category pairs that were never spatial neighbours at
write time, structurally isolated. col-029 made this isolation visible via
`isolated_entry_count`, `cross_category_edge_count`, and `inferred_edge_count` in
`context_status`. crt-029 acts on those metrics by adding a recurring background pass.

The feature adds a single new tick function, `run_graph_inference_tick`, which is the symmetric
counterpart to `maybe_run_bootstrap_promotion`: that function is one-shot and idempotency-gated;
this one is recurring and cap-throttled. Both share the W1-2 contract and the rayon pool.

## Component Breakdown

### Component 1 — `InferenceConfig` additions (`infra/config.rs`)

Four new fields appended to `InferenceConfig`:

```rust
pub supports_candidate_threshold: f32,   // HNSW pre-filter floor; default 0.5
pub supports_edge_threshold: f32,        // NLI entailment floor for Supports write; default 0.7
pub max_graph_inference_per_tick: usize, // pair cap per tick; default 100
pub graph_inference_k: usize,            // HNSW neighbour count for tick path; default 10
```

All four fields have `#[serde(default = "...")]` attributes following the existing per-field
default function pattern in `InferenceConfig`. The `Default` impl uses struct-literal form —
all four fields must be added there explicitly to keep the `Default` impl exhaustive (SR-07).

Validation additions to `InferenceConfig::validate()`:
- `supports_candidate_threshold >= supports_edge_threshold` → reject (strict `>=`; AC-02/SR-03)
- `supports_candidate_threshold` or `supports_edge_threshold` outside `(0.0, 1.0)` → reject
- `max_graph_inference_per_tick` outside `[1, 1000]` → reject
- `graph_inference_k` outside `[1, 100]` → reject

The existing `nli_contradiction_threshold < nli_auto_quarantine_threshold` guard is the
precedent pattern; the new guard follows the same two-field comparison error format.

### Component 2 — `Store::query_entries_without_edges()` (`unimatrix-store/src/read.rs`)

New public async method returning `Vec<u64>` (entry IDs with no non-bootstrap edge on either
endpoint):

```sql
SELECT id FROM entries
WHERE status = 0
  AND id NOT IN (
    SELECT source_id FROM graph_edges WHERE bootstrap_only = 0
    UNION
    SELECT target_id FROM graph_edges WHERE bootstrap_only = 0
  )
```

Uses `read_pool()` (read-only query, no write path involvement). Returns IDs only — not
`EntryRecord`. This follows the col-029 ADR-004 pattern: bounded SQL with no Cartesian product.

Note: `read.rs` is already well past 500 lines. The new method is an additive query; no split
of the existing file is in scope for crt-029 (that is a separate housekeeping task noted in
the file).

### Component 3 — `run_graph_inference_tick` (`services/nli_detection_tick.rs`)

A new file `nli_detection_tick.rs` under `crates/unimatrix-server/src/services/`. This is the
primary deliverable. The file split is mandatory: `nli_detection.rs` is already 1,373 lines
(SCOPE.md notes ~650; actual count is higher post-crt-023). The new function and all its
private helpers must live in the new file.

Module declaration: add `pub mod nli_detection_tick;` to
`crates/unimatrix-server/src/services/mod.rs`.

Public interface:

```rust
pub async fn run_graph_inference_tick(
    store: &Store,
    nli_handle: &NliServiceHandle,
    vector_index: &VectorIndex,
    rayon_pool: &RayonPool,
    config: &InferenceConfig,
)
```

This signature is modelled directly on `maybe_run_bootstrap_promotion`.

Private helpers extracted from the tick function body (SR-08 / entry #2800 — cap logic must
be a testable unit):

```rust
// Select up to max_sources candidate source IDs in priority order.
// Returns Vec<u64> bounded to max_sources.
fn select_source_candidates(
    all_active: &[ActiveEntryMeta],
    existing_edge_set: &HashSet<(u64, u64)>,
    isolated_ids: &HashSet<u64>,
    max_sources: usize,
) -> Vec<u64>

// Write NLI-scored Supports edges from a batch result. Returns edges_written count.
// Uses supports_edge_threshold (not nli_entailment_threshold).
// Supports-ONLY: this function MUST NOT write Contradicts edges (C-13 / AC-10a).
// The contradiction_threshold parameter is intentionally absent — the tick is a
// Supports-only pass; the dedicated contradiction path is the sole Contradicts writer.
async fn write_inferred_edges_with_cap(
    store: &Store,
    pairs: &[(u64, u64)],           // (source_id, target_id)
    nli_scores: &[NliScores],
    supports_threshold: f32,        // config.supports_edge_threshold
    max_edges: usize,               // config.max_graph_inference_per_tick
) -> usize
```

`write_inferred_edges_with_cap` is a named variant of `write_edges_with_cap` from
`nli_detection.rs`. It cannot reuse the existing function directly: the existing function
takes `(source_id, &[(neighbor_id, text)])` where one source maps to many targets, while the
tick path has a flat `Vec<(u64, u64)>` of mixed-source pairs. The threshold parameter
semantics also differ (supports vs entailment). Crucially, the tick variant has no
`contradiction_threshold` parameter and does not write `Contradicts` edges — this is an
intentional scope constraint (C-13), not an oversight. The cap logic and `write_nli_edge`
call pattern are shared by importing `write_nli_edge` from `nli_detection.rs` (making it
`pub(crate)`), or by duplicating the trivial INSERT helper.

### Component 4 — `background.rs` call site

`run_single_tick` gets one new call after `maybe_run_bootstrap_promotion`:

```rust
// crt-029: Background graph inference (recurring, cap-throttled via max_graph_inference_per_tick).
if inference_config.nli_enabled {
    run_graph_inference_tick(store, nli_handle, vector_index, ml_inference_pool, inference_config).await;
}
```

No timeout wrapper is required beyond the existing `TICK_TIMEOUT = 120s` budget. At default
100 pairs × ~0.5ms/pair = 50ms NLI time, the tick completes well within budget. The function
is infallible (errors logged at `warn`, no propagation) following the `maybe_run_bootstrap_promotion`
precedent.

No new parameters are added to `spawn_background_tick` or `background_tick_loop`. The
`vector_index` parameter is already threaded through to `run_single_tick` — the new call site
uses the existing reference.

## Component Interactions

```
background_tick_loop
  └── run_single_tick
        └── [after bootstrap promotion]
              └── run_graph_inference_tick (crt-029)
                    │
                    ├── Store::query_by_status(Active)         → Vec<EntryRecord> (metadata only)
                    ├── Store::query_entries_without_edges()   → Vec<u64> (isolated IDs)
                    ├── Store::query_existing_supports_pairs() → HashSet<(u64, u64)> (pre-filter)
                    │
                    ├── select_source_candidates()             → Vec<u64> (priority-ordered)
                    │
                    ├── [per source candidate]:
                    │     VectorIndex::get_embedding(id)       → Option<Vec<f32>>
                    │     VectorIndex::search(emb, k, ef)      → Vec<SearchResult>
                    │
                    ├── pair dedup + sort + truncate to max_graph_inference_per_tick
                    │
                    ├── fetch pair texts via Store::get() / get_content_via_write_pool()
                    │
                    ├── rayon_pool.spawn(score_batch)          → Vec<NliScores>  [W1-2]
                    │
                    └── write_inferred_edges_with_cap()        → edges_written
                          └── write_nli_edge() × N
```

## Technology Decisions

| ADR | Title | Unimatrix ID |
|-----|-------|-------------|
| ADR-001 | New Module `nli_detection_tick.rs` for Background Inference Tick | #3656 |
| ADR-002 | `write_inferred_edges_with_cap` as Named Variant, Not Reuse of `write_edges_with_cap` | #3657 |
| ADR-003 | Source-Candidate Bound Derived from `max_graph_inference_per_tick`, No Separate Config Field | #3658 |
| ADR-004 | Separate `query_existing_supports_pairs()` Store Helper for Pre-Filter | #3659 |

## Integration Points

### Existing interfaces consumed

| Interface | Location | Notes |
|-----------|----------|-------|
| `Store::query_by_status(Status::Active)` | `unimatrix-store/src/read.rs:304` | Fetches all active `EntryRecord`; already used by contradiction scan |
| `Store::query_graph_edges()` | `unimatrix-store/src/read.rs:1323` | Full edge set used for `TypedGraphState::rebuild`; tick uses a lighter variant |
| `VectorIndex::search(query, k, ef)` | `unimatrix-core/src/async_wrappers.rs:29` | Async wrapper; calls `spawn_blocking` internally |
| `VectorIndex::get_embedding(id)` | `unimatrix-core/src/async_wrappers.rs:78` | Async wrapper over O(N) sync scan; call only for selected source candidates |
| `RayonPool::spawn(closure)` | `crates/unimatrix-server/src/infra/rayon_pool.rs` | Single dispatch per tick (W1-2) |
| `CrossEncoderProvider::score_batch(pairs)` | `unimatrix-embed` | Called inside rayon closure only |
| `write_nli_edge()` | `nli_detection.rs` | Currently private; promote to `pub(crate)` |
| `format_nli_metadata()` | `nli_detection.rs` | Currently private; promote to `pub(crate)` |
| `current_timestamp_secs()` | `nli_detection.rs` | Currently private; promote to `pub(crate)` |
| `EDGE_SOURCE_NLI` | `unimatrix-store/src/read.rs:1563` | Already `pub`; used in INSERT statements |
| `NliServiceHandle::get_provider()` | `infra/nli_handle.rs` | No-op guard; same pattern as bootstrap promotion |

### New interfaces introduced

| Interface | Location | Signature |
|-----------|----------|-----------|
| `Store::query_entries_without_edges()` | `unimatrix-store/src/read.rs` | `async fn query_entries_without_edges(&self) -> Result<Vec<u64>>` |
| `Store::query_existing_supports_pairs()` | `unimatrix-store/src/read.rs` | `async fn query_existing_supports_pairs(&self) -> Result<HashSet<(u64, u64)>>` |
| `run_graph_inference_tick` | `services/nli_detection_tick.rs` | `pub async fn run_graph_inference_tick(store: &Store, nli_handle: &NliServiceHandle, vector_index: &VectorIndex, rayon_pool: &RayonPool, config: &InferenceConfig)` |
| `select_source_candidates` | `services/nli_detection_tick.rs` | `fn select_source_candidates(all_active: &[ActiveEntryMeta], existing_edge_set: &HashSet<(u64, u64)>, isolated_ids: &HashSet<u64>, max_sources: usize) -> Vec<u64>` |
| `write_inferred_edges_with_cap` | `services/nli_detection_tick.rs` | `async fn write_inferred_edges_with_cap(store: &Store, pairs: &[(u64, u64)], nli_scores: &[NliScores], supports_threshold: f32, max_edges: usize) -> usize` — Supports-only; no `contradiction_threshold` (C-13) |

## Integration Surface

| Integration Point | Type / Signature | Source |
|-------------------|------------------|--------|
| `query_entries_without_edges` | `async fn(&self) -> Result<Vec<u64>>` — reads via `read_pool()` | new, `unimatrix-store/src/read.rs` |
| `query_existing_supports_pairs` | `async fn(&self) -> Result<HashSet<(u64, u64)>>` — reads via `read_pool()` | new, `unimatrix-store/src/read.rs` |
| `run_graph_inference_tick` | `async fn(store: &Store, nli_handle: &NliServiceHandle, vector_index: &VectorIndex, rayon_pool: &RayonPool, config: &InferenceConfig)` | new, `nli_detection_tick.rs` |
| `write_inferred_edges_with_cap` | `async fn(store, pairs, nli_scores, supports_threshold, max_edges) -> usize` — Supports-only; no `contradiction_threshold` parameter (C-13) | new, `nli_detection_tick.rs` (private but testable) |
| `write_nli_edge` | `async fn(store: &Store, source_id: u64, target_id: u64, relation_type: &str, weight: f32, created_at: u64, metadata: &str) -> bool` | existing, `nli_detection.rs`; promote to `pub(crate)` |
| `format_nli_metadata` | `fn(scores: &NliScores) -> String` | existing, `nli_detection.rs`; promote to `pub(crate)` |
| `current_timestamp_secs` | `fn() -> u64` | existing, `nli_detection.rs`; promote to `pub(crate)` |
| `EDGE_SOURCE_NLI` | `&'static str = "nli"` | existing, `unimatrix-store/src/read.rs:1563` |
| `InferenceConfig::supports_candidate_threshold` | `f32`, default `0.5` | new field, `infra/config.rs` |
| `InferenceConfig::supports_edge_threshold` | `f32`, default `0.7` | new field, `infra/config.rs` |
| `InferenceConfig::max_graph_inference_per_tick` | `usize`, default `100` | new field, `infra/config.rs` |
| `InferenceConfig::graph_inference_k` | `usize`, default `10` | new field, `infra/config.rs` |

## Algorithmic Design: `run_graph_inference_tick`

The tick proceeds in phases to bound NLI budget before committing to inference:

### Phase 1 — Guard (O(1))
Call `nli_handle.get_provider().await`. Return immediately on `Err`. This mirrors the
`maybe_run_bootstrap_promotion` no-op path.

### Phase 2 — Data fetch (async DB, tokio thread)
Three concurrent-safe sequential reads:
1. `store.query_by_status(Status::Active)` → `Vec<EntryRecord>` with `id`, `category`,
   `created_at` fields (no tag load needed here)
2. `store.query_entries_without_edges()` → `HashSet<u64>` (isolated IDs)
3. `store.query_existing_supports_pairs()` → `HashSet<(u64, u64)>` (skip set for pre-filter)

### Phase 3 — Source candidate selection (SR-02 mitigation; cap BEFORE embedding)
`select_source_candidates` applies priority ordering to produce a `Vec<u64>` of at most
`max_graph_inference_per_tick` source IDs. Priority:
1. Entries appearing in cross-category pairs (entries whose category differs from at least
   one other entry in the active set — computed from the active list, no DB join)
2. Isolated entries (in `isolated_ids` set)
3. Remaining active entries ordered by `created_at` descending (newest first)

Truncation to `max_graph_inference_per_tick` happens HERE — in Phase 3, on metadata only
(IDs and category strings). No `get_embedding` call occurs in Phase 3. This is the critical
ordering constraint (AC-06c / R-02): the source candidate list is CAPPED FIRST, then Phase 4
fetches embeddings only for the already-capped list. Fetching embeddings before capping would
allow O(N) scans on all N active entries.

No separate config field is needed (ADR-003).

### Phase 4 — HNSW expansion (sync lock-guarded, tokio thread; embeddings for capped list only)
For each source candidate (in order) — this list was already capped to
`max_graph_inference_per_tick` in Phase 3:
1. Call `vector_index.get_embedding(id).await` (async spawn_blocking wrapper). Skip if `None`.
2. Call `vector_index.search(emb, graph_inference_k, EF_SEARCH=32).await`.
3. Collect `(source_id, neighbour_id, similarity)` triples where `similarity > supports_candidate_threshold`
   and `neighbour_id != source_id`.
4. Deduplicate: normalize pair as `(min(a, b), max(a, b))` to collapse `(A,B)` and `(B,A)`.
5. Skip pairs already in `existing_supports_pairs` (pre-filter).

### Phase 5 — Priority sort and truncation
Sort collected pairs by: (1) cross-category first, (2) either endpoint isolated, (3)
similarity descending. Truncate to `max_graph_inference_per_tick` pairs.

### Phase 6 — Text fetch (async DB, tokio thread)
For each pair `(source_id, target_id)`: fetch `content` via `store.get_content_via_write_pool()`.
This matches the bootstrap promotion pattern (uses write-pool to see recently committed rows).
Skip pairs where either endpoint content cannot be fetched (log at `debug`).

### Phase 7 — W1-2 dispatch (single rayon spawn) — R-09 NAMED CONSTRAINT
Collect all `(source_text, target_text)` pairs. Dispatch as one `rayon_pool.spawn()` closure
calling `provider.score_batch(&pairs)`. Await the result. This is the only point where rayon
is touched. A second spawn for remaining pairs is explicitly prohibited (entry #3653).

**R-09 rayon/tokio boundary (C-14)**: the closure body passed to `rayon_pool.spawn()` MUST
be a synchronous CPU-bound closure only. The following are PROHIBITED inside this closure:
- `tokio::runtime::Handle::current()`
- `.await` expressions
- Any function that internally awaits or accesses the Tokio runtime
Rayon worker threads have no Tokio runtime; any violation panics at runtime with "no current
Tokio runtime". This failure is compile-invisible and test-invisible in unit tests that don't
use the full runtime. Detection: `grep -n 'Handle::current\|\.await'` inside the rayon closure
body in `nli_detection_tick.rs`. Independent validator required (not the author of the closure).

### Phase 8 — Write (Supports only)
Call `write_inferred_edges_with_cap(store, &scored_pairs, &nli_scores, supports_edge_threshold,
max_graph_inference_per_tick)`. Note: no `contradiction_threshold` parameter — the tick writes
Supports edges only (C-13 / AC-10a). Log `edges_written` at `debug`.

## Risk Mitigations Applied

### SR-01 — Tick does not write Contradicts edges (scope change)
`write_inferred_edges_with_cap` is a Supports-only function. It has no `contradiction_threshold`
parameter and does not evaluate `scores.contradiction` for edge writing. The risk of false-
positive `Contradicts` edges from the tick is eliminated by design: there is no tick Contradicts
write path at all. The dedicated contradiction detection path (`run_post_store_nli`, the
contradiction scan) remains the sole writer. This is a stronger mitigation than the previous
approach of passing an explicit threshold parameter — the tick cannot write Contradicts edges
regardless of what scores the NLI model returns. AC-10a and AC-19† verify this constraint.

### SR-02 — `get_embedding` O(N) bound
Source candidates are capped to `max_graph_inference_per_tick` in `select_source_candidates`
before any `get_embedding` call. Phase 3 runs entirely on metadata (IDs, category strings)
— no embedding access. The embedding lookup in Phase 4 is therefore bounded to at most
`max_graph_inference_per_tick` calls.

### SR-03 — Threshold boundary condition
`InferenceConfig::validate()` rejects `supports_candidate_threshold >= supports_edge_threshold`
(strict `>=`). Equal values (e.g. both 0.7) are rejected.

### SR-04 — Pre-filter index coverage
`query_existing_supports_pairs()` selects only `source_id, target_id WHERE relation_type =
'Supports' AND bootstrap_only = 0`. The `UNIQUE(source_id, target_id, relation_type)` index
on GRAPH_EDGES covers this filter. This is lighter than `query_graph_edges()` (which returns
all edge types with all columns).

### SR-05 — Rayon pool contention
`run_graph_inference_tick` uses `rayon_pool.spawn()` with a single dispatch. If the pool is
saturated by concurrent post-store NLI calls, the `.await` on the spawn future will queue
behind them. The tick function has no timeout of its own; it defers to the outer `TICK_TIMEOUT`
on `run_single_tick`. At 100 pairs max this degrades gracefully.

### SR-06 — `compute_graph_cohesion_metrics` pool
`compute_graph_cohesion_metrics` (col-029) reads via `read_pool()` per entry #3619. The tick
writes via `write_pool_server()`. These are independent pools; no contention between the
observability query and tick writes.

### SR-07 — `InferenceConfig` struct literal trap
The `Default` impl for `InferenceConfig` is a struct literal. All four new fields must be
added there. Any test that constructs `InferenceConfig { ..., ..InferenceConfig::default() }`
is safe. Tests using a bare struct literal `InferenceConfig { field1, field2, ... }` (without
`..default()`) will fail to compile — this is the desired catch. The implementation spec must
include: "grep for `InferenceConfig {` and update all bare literal constructions to include
`..InferenceConfig::default()`" as a pre-merge step.

### SR-08 — Cap logic testability
`write_inferred_edges_with_cap` is a named, standalone `async fn` with no dependencies on
`InferenceConfig` fields — only the resolved threshold values are passed as scalars. This
makes it unit-testable without a live ONNX model (mock NliScores, mock Store in tests).

## Open Questions

None at architecture stage. All design decisions are closed in SCOPE.md. The following
implementation notes are passed to the delivery agent:

1. `write_nli_edge`, `format_nli_metadata`, and `current_timestamp_secs` in `nli_detection.rs`
   need to be promoted from private to `pub(crate)` so `nli_detection_tick.rs` can use them.
   Alternatively, re-export via `mod.rs`. Either approach is acceptable; the implementation
   agent should pick the simpler one.

2. `query_existing_supports_pairs()` is a new store method. If the implementation agent
   judges it cleaner to build the `HashSet<(u64, u64)>` from the existing
   `query_graph_edges()` result (filter in Rust), that is acceptable as long as the total
   in-memory size is bounded by the graph size (which it is). Document the choice in code.

3. The `ActiveEntryMeta` type used in `select_source_candidates` is an ad-hoc struct
   `{ id: u64, category: String }` derived from `EntryRecord`. It does not need to be a
   named type if the implementation agent prefers to pass `&[EntryRecord]` directly.
