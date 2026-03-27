# crt-029: Background Graph Inference — Supports and Prerequisite Edges

## Problem Statement

The knowledge graph currently grows its `Supports` and `Contradicts` edges only through the
post-store NLI path (`run_post_store_nli`), which fires once per `context_store` call and covers
only the K nearest neighbours of the newly-stored entry. Entries that were stored before NLI was
enabled, entries with no recent neighbour writes, and cross-category pairs that were never
near-neighbours at store time remain isolated — exactly the population that PPR-based retrieval
cannot reach.

col-029 (GH #413) introduced graph cohesion metrics that make this isolation visible
(`isolated_entry_count`, `cross_category_edge_count`, `inferred_edge_count`). These metrics are
now actionable. GH #412 targets the gap: a recurring background pass that fills the graph
systematically rather than opportunistically.

Affected parties: operators who observe high `isolated_entry_count` after NLI is enabled;
PPR-based retrieval quality for any deployment where most entries pre-date NLI activation.

## Goals

1. Add a recurring background pass (`run_graph_inference_tick`) called from `background_tick_loop`
   that systematically infers `Supports` edges across active entries using the existing
   NLI cross-encoder and HNSW index.
2. Use HNSW pre-filtering (similarity > `supports_candidate_threshold`, default 0.5) to collapse
   the O(N²) pair space to O(N×K) before any NLI call is made.
3. Process pairs in priority order: cross-category pairs first, then isolated entries (entries
   with no existing non-bootstrap edges), then high-similarity pairs.
4. Write `Supports` edges when `entailment > supports_edge_threshold` (default 0.7); share the
   single NLI call per pair with contradiction detection (combined pass).
5. Add three new fields to `InferenceConfig`: `supports_candidate_threshold` (default 0.5),
   `supports_edge_threshold` (default 0.7), `max_graph_inference_per_tick` (default 100).
6. Gate the new tick pass behind `inference_config.nli_enabled`; no-op when NLI is not ready.

## Non-Goals

- No new ML infrastructure. The existing `CrossEncoderProvider::score_batch` is the only
  inference surface. No new ONNX models, no new rayon pools.
- No changes to `run_post_store_nli`. That path continues to operate independently on each
  `context_store` call.
- No `Prerequisite` edge inference in crt-029. Asymmetric NLI entailment is a weak proxy for
  topic ordering without a domain-anchored category prerequisite signal; noisy edges would
  traverse PPR as positive signal and hurt ranking. Deferred to W3-1.
- No changes to `TypedRelationGraph` in-memory structure or `build_typed_relation_graph`.
- No changes to search ranking or confidence scoring. `Supports` edges affect `graph_penalty`
  only indirectly (they are already invisible to penalty logic via SR-01 / `edges_of_type`).
- No removal or replacement of `maybe_run_bootstrap_promotion`. That one-shot pass continues
  unchanged; it promotes bootstrap `Contradicts` edges and has its own idempotency marker.
- No per-tick deduplication beyond a pre-filter query that skips pairs already having a
  `Supports` edge. `INSERT OR IGNORE` remains the write-side guard but is not relied on as the
  primary deduplication mechanism — NLI budget is too valuable to waste on confirmed pairs.
- No schema migration. All required columns already exist in the current schema.
- No changes to the `contradiction::scan_contradictions` path in `infra/contradiction.rs`.
  That is a status-diagnostic path (reports to `ContradictionScanCacheHandle`); the new pass
  writes persistent graph edges via the NLI detection service.
- No alerting or automatic quarantine triggered by the new pass. Auto-quarantine for
  `Contradicts` edges remains the responsibility of the existing auto-quarantine cycle counter.
- `Prerequisite` edge promotion (removing `bootstrap_only = true`) is not in scope for crt-029.
  That is a W3-1 concern.

## Background Research

### What Already Exists

**Post-store Supports write path (`nli_detection.rs:write_edges_with_cap`)** — already writes
`Supports` edges in `run_post_store_nli`. The logic is: `scores.entailment > threshold → INSERT
OR IGNORE Supports`. The threshold is `nli_entailment_threshold` (default 0.6). The combined
cap `max_contradicts_per_tick` counts both Supports and Contradicts edges per call.

**HNSW pre-filter pattern** — `run_post_store_nli` already uses `vector_index.search(embedding,
k, EF_SEARCH=32)` to pre-filter candidates before NLI scoring. The exact same pattern is used
in `SearchService::search`. This is the O(N×K) pre-filter that crt-029 must extend to the tick
path.

**W1-2 contract** — all NLI inference (`CrossEncoderProvider::score_batch`) MUST run via
`rayon_pool.spawn()`, never inline in async context, never via `spawn_blocking`. This is
enforced in `nli_detection.rs` comments and must be preserved in the new tick function.

**`maybe_run_bootstrap_promotion`** — called on every tick after extraction. It is a one-shot
pass (idempotent via COUNTERS marker). The new graph inference pass is recurring, not one-shot.
It must not adopt the idempotency marker pattern; instead it runs on every N-th tick (or every
tick with a per-tick item cap).

**`CONTRADICTION_SCAN_INTERVAL_TICKS = 4`** — the existing contradiction scan already uses a
tick-modulo gate to avoid running O(N) ONNX on every tick. The same pattern applies here.

**`query_by_status(Status::Active)`** — `Store::query_by_status` already fetches all active
entries as `Vec<EntryRecord>`. This is how the contradiction scan fetches its candidate set.

**`query_graph_edges()`** — `Store::query_graph_edges` already fetches all non-bootstrap graph
edges. It is used by `TypedGraphState::rebuild`. The tick pass can use this to identify which
entries are isolated (have no existing edges as source or target).

**GRAPH_EDGES schema** — `UNIQUE(source_id, target_id, relation_type)` constraint in place.
`INSERT OR IGNORE` is the idempotent write pattern already used by `write_nli_edge`. No schema
changes needed.

**`RelationType::Prerequisite`** — already defined in `unimatrix-engine/src/graph.rs` with
comment "reserved for W3-1; no write path exists in crt-021". The string `"Prerequisite"` is
the canonical storage form.

**`bootstrap_only` flag** — already in GRAPH_EDGES schema. `bootstrap_only = 1` rows are
excluded from `TypedRelationGraph.inner` during rebuild (two-pass build in crt-021). Writing
`Prerequisite` edges with `bootstrap_only = true` is zero-cost from a schema perspective.

**`InferenceConfig`** — no `supports_candidate_threshold`, `supports_edge_threshold`, or
`max_graph_inference_per_tick` fields exist yet. All three need to be added with defaults and
validation ranges.

**`EDGE_SOURCE_NLI` constant** — col-029 ADR-001 (Unimatrix entry #3591) established that
`"nli"` is the canonical string for `source` on NLI-inferred edges. It may be a named constant
in `unimatrix-store`. The new pass must use this same value.

### Priority Ordering Rationale

The GH #412 spec calls for: cross-category pairs → isolated entries → high similarity. This
ordering maximises the observability impact per NLI call:
- Cross-category pairs fill the `cross_category_edge_count` metric fastest (the gap most visible
  in col-029 dashboards).
- Isolated entries directly reduce `isolated_entry_count`.
- High-similarity pairs are already partially covered by `run_post_store_nli`; they are lowest
  priority here.

The priority ordering requires fetching active entry embeddings and existing edges before
selecting pairs for the tick. Both are already available via `vector_index` and
`query_graph_edges()`.

### col-029 / col-030 Context

col-029 (GH #413) added `compute_graph_cohesion_metrics()` to the store and surfaced the six
metrics in `context_status`. These metrics are the visibility layer for crt-029's output.

col-030 (GH #419) added `suppress_contradicts` — always-on Contradicts suppression in
`SearchService::search`. An NLI false-positive `Contradicts` edge would suppress valid results.
The new tick pass must not lower the contradiction threshold below `nli_contradiction_threshold`
(currently 0.6 default, same pass used by post-store NLI which already uses this guard).

### Combined Pass Design

The GH #412 spec says "one NLI call per pair, both contradiction and Supports considered". This
maps directly to the existing `write_edges_with_cap` function — it already evaluates both
`scores.entailment` and `scores.contradiction` in a single loop. The new tick function can
call a variant of this helper (or the same helper) after the rayon batch returns, reusing all
existing threshold logic.

The difference from post-store NLI: the tick pass iterates over selected pairs drawn from the
full active entry set (not just one new entry's neighbours) and writes up to
`max_graph_inference_per_tick` edges per tick.

## Proposed Approach

### Layer 1: `InferenceConfig` additions (`infra/config.rs`)

Four new fields appended to `InferenceConfig`:
```rust
pub supports_candidate_threshold: f32,  // default 0.5, range (0.0, 1.0) exclusive
pub supports_edge_threshold: f32,       // default 0.7, range (0.0, 1.0) exclusive
pub max_graph_inference_per_tick: usize, // default 100, range [1, 1000]
pub graph_inference_k: usize,           // default same as nli_post_store_k (10), range [1, 100]
```

`graph_inference_k` is separate from `nli_post_store_k` — the post-store path is latency-sensitive
(hot path); the tick path is background with no latency budget. Sharing the knob creates invisible
coupling where tuning one breaks the other.

Validation: `supports_candidate_threshold` must be < `supports_edge_threshold` (similar to the
`nli_contradiction_threshold < nli_auto_quarantine_threshold` guard). All three added to
`InferenceConfig::validate()`.

### Layer 2: New tick function (`services/nli_detection.rs`)

Add `run_graph_inference_tick` as a new public async function in `nli_detection.rs`:

1. Check NLI readiness; return immediately if not ready (same pattern as bootstrap promotion).
2. Fetch all active entries via `query_by_status(Active)` (IDs + metadata only, no embeddings).
3. Fetch existing non-bootstrap edges via `query_graph_edges()` to build an isolated-entry set.
4. Pre-filter: query existing `Supports` edges from GRAPH_EDGES; collect covered `(source, target)`
   pairs into a `HashSet`. Pairs already in this set are skipped — avoids wasted NLI calls as
   the graph matures.
5. Build candidate sources using priority ordering: cross-category entry IDs first, then isolated
   entry IDs, then all remaining active entries ordered by recency or ID. Call
   `vector_index.get_embedding(id)` only for candidate sources as they are selected (not for all
   active entries). `get_embedding` is O(N) per call; bounding source selection bounds embedding
   lookups.
6. For each candidate source, query `graph_inference_k` HNSW neighbours at similarity >
   `supports_candidate_threshold`. Deduplicate (A,B) == (B,A) pairs; skip pairs in the
   pre-filter set.
7. Sort remaining pairs by priority: cross-category first, then isolated endpoints, then
   similarity desc. Truncate to `max_graph_inference_per_tick` pairs.
8. Dispatch all pairs as a single `rayon_pool.spawn()` call (W1-2 contract).
9. Write edges via the existing `write_edges_with_cap`-style logic, using
   `supports_edge_threshold` for entailment and `nli_contradiction_threshold` for Contradicts.
10. Log total edges written at `debug` level.

Note on `nli_detection.rs` file size: currently ~650 lines. Adding the new tick function and
helpers will approach the 500-line guidance in `rust-workspace.md`. Split to
`nli_detection_tick.rs` if the combined file exceeds 500 lines (judgment call at implementation).

### Layer 3: Background tick call site (`background.rs`)

After `maybe_run_bootstrap_promotion`, add a call to `run_graph_inference_tick` gated on
`inference_config.nli_enabled`. Runs on every tick — no interval gate. The
`max_graph_inference_per_tick` cap is the correct throttle; an additional interval parameter
is a second knob for the same concern and is not added upfront.

The function signature follows `maybe_run_bootstrap_promotion`: takes `&Store`, `&NliServiceHandle`,
`&VectorIndex`, `&RayonPool`, `&InferenceConfig`.

### Layer 4: Store query helper (`unimatrix-store/src/read.rs`)

Add `Store::query_entries_without_edges()` returning `Vec<u64>` (entry IDs with no non-bootstrap
edges as source or target). Used by `run_graph_inference_tick` step 3 to identify isolated entries
for priority ordering. SQL:
```sql
SELECT id FROM entries
WHERE status = 0
  AND id NOT IN (
    SELECT source_id FROM graph_edges WHERE bootstrap_only = 0
    UNION
    SELECT target_id FROM graph_edges WHERE bootstrap_only = 0
  )
```

This is a single bounded query (no Cartesian product, per col-029 ADR-004 pattern). Returns
only IDs, not full entry content.

### Rationale for Key Choices

- **Tick-modulo gate**: avoids running O(N×K+M·ONNX) on every 15-minute tick when N is large.
  Consistent with `CONTRADICTION_SCAN_INTERVAL_TICKS` precedent.
- **Single rayon dispatch per tick**: preserves W1-2 contract. Collects all pairs before
  dispatching, writes results back on the tokio thread. Exactly how bootstrap promotion works.
- **`write_edges_with_cap` reuse**: the combined-pass logic is already implemented correctly in
  this helper. The tick pass uses the same or a minimal variant, avoiding duplication.
- **`Prerequisite` with `bootstrap_only = true`**: matches the existing bootstrap flag semantics.
  The edge is recorded but excluded from penalty/traversal logic until promoted. Promotion path
  is W3-1 territory.
- **`max_graph_inference_per_tick` cap**: bounds the tick's NLI work. At default 100, worst-case
  is 100 pairs × ~0.5ms/pair = 50ms NLI time per tick, well within the existing `TICK_TIMEOUT`.
- **New store helper vs. in-memory filtering**: `query_entries_without_edges()` is a single SQL
  query using existing indexes on `graph_edges.source_id` and `graph_edges.target_id`. It avoids
  loading full entry content for the isolation check, which would be wasteful.

## Acceptance Criteria

- AC-01: `InferenceConfig` has four new fields: `supports_candidate_threshold: f32` (default
  0.5), `supports_edge_threshold: f32` (default 0.7), `max_graph_inference_per_tick: usize`
  (default 100), `graph_inference_k: usize` (default 10).
- AC-02: `InferenceConfig::validate()` rejects configs where
  `supports_candidate_threshold >= supports_edge_threshold`.
- AC-03: `InferenceConfig::validate()` rejects `supports_candidate_threshold` or
  `supports_edge_threshold` outside `(0.0, 1.0)` exclusive.
- AC-04: `InferenceConfig::validate()` rejects `max_graph_inference_per_tick` outside
  `[1, 1000]`.
- AC-04b: `InferenceConfig::validate()` rejects `graph_inference_k` outside `[1, 100]`.
- AC-05: `run_graph_inference_tick` is a no-op (returns immediately) when NLI is not ready
  (`nli_handle.get_provider()` returns `Err`).
- AC-06: `run_graph_inference_tick` queries only `status = Active` entries and uses HNSW
  similarity > `supports_candidate_threshold` to pre-filter candidate pairs. Uses
  `graph_inference_k` as the HNSW neighbour count (independent of `nli_post_store_k`).
- AC-06b: Pairs where a `Supports` edge already exists in GRAPH_EDGES are skipped before NLI
  scoring (pre-filter, not `INSERT OR IGNORE`). This bounds wasted NLI calls as the graph matures.
- AC-06c: `get_embedding` is called only for source candidates as they are selected — not for
  all active entries upfront.
- AC-07: Candidate pairs are processed in priority order: cross-category pairs first, isolated
  entries (no existing non-bootstrap edges) second, remaining pairs by similarity descending.
- AC-08: For each pair, a single `rayon_pool.spawn()` call is used for all NLI inference
  (W1-2 contract; no `spawn_blocking`, no inline async NLI).
- AC-09: A `Supports` edge `(A, B, "Supports")` is written when `score(A→B).entailment >
  supports_edge_threshold`. Uses `INSERT OR IGNORE` for idempotency.
- AC-10: A `Contradicts` edge `(A, B, "Contradicts")` is written when `score(A→B).contradiction
  > nli_contradiction_threshold` (existing threshold reused). Uses `INSERT OR IGNORE`.
- AC-11: Total edges written per tick is bounded by `max_graph_inference_per_tick`. The cap
  counts Supports + Contradicts + Prerequisite edges combined.
- AC-13: All written edges use `source = "nli"` (the `EDGE_SOURCE_NLI` constant from col-029).
- AC-14: `run_graph_inference_tick` is called from `background_tick_loop` after
  `maybe_run_bootstrap_promotion`, gated on `inference_config.nli_enabled`.
- AC-15: `Store::query_entries_without_edges()` returns the IDs of active entries with no
  non-bootstrap edge on either endpoint (source_id or target_id).
- AC-16: Unit tests cover: no-NLI no-op, cross-category priority ordering, isolated-entry
  priority, edge cap enforcement, pre-filter skips pairs with existing Supports edges, and
  idempotency (duplicate pair writes → INSERT OR IGNORE as write-side guard).
- AC-17: TOML deserialization of `InferenceConfig` with the four new fields works correctly,
  and default values match the specified defaults when fields are absent.

## Constraints

- **W1-2 contract (mandatory)**: all `CrossEncoderProvider::score_batch` calls inside
  `run_graph_inference_tick` MUST go via `rayon_pool.spawn()`. Never inline in async context.
  Never via `spawn_blocking`. This is a hard constraint from the existing NLI architecture.
- **SQLite via sqlx**: all new store queries use `sqlx::query` with `write_pool_server()`. No
  raw `rusqlite` connections.
- **`INSERT OR IGNORE` idempotency**: the `UNIQUE(source_id, target_id, relation_type)`
  constraint in GRAPH_EDGES is the deduplication mechanism. The tick is safe to re-run.
- **File size limit (500 lines)**: `nli_detection.rs` is currently ~650 lines. If adding the
  new function and helpers would push it significantly further, split into
  `nli_detection_tick.rs`. Do not merge if the combined file exceeds 800 lines.
- **No schema migration**: all required columns (`bootstrap_only`, `source`, `metadata`) exist
  in the current schema. No new columns.
- **No new crate dependencies**: uses only existing `unimatrix-core`, `unimatrix-embed`,
  `unimatrix-store`, `sqlx`, and `tracing`. No new external crates.
- **`supports_edge_threshold` >= `nli_entailment_threshold` note**: `supports_edge_threshold`
  (default 0.7) is a higher bar than `nli_entailment_threshold` (default 0.6 used in post-store
  NLI). This is intentional — the tick pass processes a much larger pair space, so a higher
  threshold reduces false positives. Both thresholds are independent config fields.
- **TICK_TIMEOUT**: `run_graph_inference_tick` must complete within the existing `TICK_TIMEOUT`
  constant. The `max_graph_inference_per_tick` cap (default 100 pairs) bounds the NLI work to
  well within this limit.
- **`get_embedding` is O(N)**: call only for selected source candidates, not for all active
  entries. Source candidate set is bounded by the `max_graph_inference_per_tick` cap.

## Design Decisions (Closed)

All open questions resolved by human review:

1. **Tick interval** — every tick. `max_graph_inference_per_tick` is the throttle; no interval
   gate added.

2. **Prerequisite scope** — deferred to W3-1. Asymmetric NLI entailment is a weak proxy for
   topic ordering without a domain-anchored category prerequisite signal; noisy edges would
   traverse PPR as positive signal. crt-029 ships Supports-only.

3. **K neighbours config** — separate `graph_inference_k` field in `InferenceConfig` (default
   10). Post-store path is latency-sensitive; tick path is background. Sharing the knob creates
   invisible coupling.

4. **Pair deduplication** — pre-filter before NLI. Query existing Supports edges into a
   `HashSet`; skip confirmed pairs. NLI budget is too valuable to spend re-confirming edges.
   `INSERT OR IGNORE` remains the write-side guard.

5. **Embedding access** — `vector_index.get_embedding(id)` exists at line 312, O(N) per call.
   Call only for selected source candidates as they are selected — not for all active entries
   upfront. Source candidate set is bounded by `max_graph_inference_per_tick`.

## Tracking

GH Issue: #412
