# Architecture: crt-040 — Cosine Supports Edge Detection

## System Overview

crt-040 restores `Supports` edge production to the knowledge graph by adding a
pure-cosine detection path (Path C) inside `run_graph_inference_tick`. The NLI
Supports path (`run_post_store_nli`) was deleted in crt-038; crt-039 subsequently
decoupled the structural Informs tick from the NLI gate so it runs unconditionally.
crt-040 extends that unconditional tick with a second structural path that detects
entailment-adjacent pairs via cosine similarity alone.

The resulting graph will contain `Supports` edges tagged `source = 'cosine_supports'`,
distinct from both `'nli'` (NLI-confirmed edges, Path B) and `'co_access'` (co-access
promotion tick). This distinction is the primary prerequisite for the PPR expander and
GNN training signal pipeline (Group 4 roadmap).

## Component Breakdown

### unimatrix-store (read.rs)

**Responsibility:** Named constant for the new edge source value.

New constant:
```rust
pub const EDGE_SOURCE_COSINE_SUPPORTS: &str = "cosine_supports";
```

Follows the pattern established by `EDGE_SOURCE_NLI` (col-029) and
`EDGE_SOURCE_CO_ACCESS` (crt-034). Must be re-exported from `unimatrix-store::lib.rs`.
No schema change — `graph_edges.source` (TEXT NOT NULL DEFAULT '') already accepts
arbitrary string values.

### unimatrix-server / nli_detection.rs

**Responsibility:** Generalized edge write helper.

New function:
```rust
pub(crate) async fn write_graph_edge(
    store: &Store,
    source_id: u64,
    target_id: u64,
    relation_type: &str,
    weight: f32,
    created_at: u64,
    source: &str,
    metadata: &str,
) -> bool
```

`write_nli_edge` is **NOT modified**. It retains its hardcoded `source = "nli"` literal and
its existing call signature. All Path A and Path B callers remain unmodified. `write_graph_edge`
is added as a sibling function alongside `write_nli_edge` — it accepts `source: &str` as a
parameter and is called only by Path C.

See ADR-001 for the write-helper decision.

### unimatrix-server / infra/config.rs (InferenceConfig)

**Responsibility:** New config field for Path C detection threshold.

Changes:
1. Add field `supports_cosine_threshold: f32` with serde default `0.65`.
2. Add backing function `default_supports_cosine_threshold() -> f32 { 0.65 }`.
3. Add explicit `supports_cosine_threshold: 0.65` to `impl Default for InferenceConfig`.
4. Add range validation in `InferenceConfig::validate()`: reject `<= 0.0` or `>= 1.0`.
5. Remove dead field `nli_post_store_k` (no consumer since crt-038 deleted `run_post_store_nli`).

The dual-site requirement (serde default fn AND impl Default literal) is non-negotiable.
See ADR-002 and the impl Default trap (pattern #4011, lesson #4014).

### unimatrix-server / nli_detection_tick.rs (run_graph_inference_tick)

**Responsibility:** Path C — Cosine Supports detection loop.

New module-level constant:
```rust
const MAX_COSINE_SUPPORTS_PER_TICK: usize = 50;
```

Path C runs between Path A completion and the Path B entry gate. It iterates the
already-computed `candidate_pairs` vec (Phase 4 output) and writes `Supports` edges
when:
- cosine >= `config.supports_cosine_threshold` (>= not >, symmetric pairs)
- `[source_category, target_category]` is in `config.informs_category_pairs`
- pair is NOT in `existing_supports_pairs` (pre-filter; INSERT OR IGNORE is backstop)
- budget cap `MAX_COSINE_SUPPORTS_PER_TICK` not exhausted

See ADR-003 for the Path C placement decision.
See ADR-004 for the budget constant decision.

## Component Interactions

```
run_graph_inference_tick()
  Phase 2: DB reads
    store.query_by_status()        → all_active
    store.query_existing_supports_pairs() → existing_supports_pairs (HashSet)
    store.query_existing_informs_pairs()  → existing_informs_pairs  (HashSet)
  Phase 3: source candidate selection (no embeddings)
  Phase 4: HNSW expansion → candidate_pairs: Vec<(u64, u64, f32)>
  Phase 4b: Informs HNSW scan → informs_metadata: Vec<InformsCandidate>
  Phase 5: Independent budget caps
  --- PATH A: Structural Informs write loop (unchanged) ---
    write_graph_edge(source="nli", relation="Informs") × informs_metadata
  --- PATH C: Cosine Supports write loop (NEW) ---
    for (src, tgt, cosine) in candidate_pairs:
      if cosine >= supports_cosine_threshold
        AND category_pair in informs_category_pairs
        AND (min,max) not in existing_supports_pairs:
          write_graph_edge(source="cosine_supports", relation="Supports")
  --- PATH B gate: get_provider() → Err = early return ---
    [Phase 6/7/8: NLI Supports — unchanged]
```

Path C is positioned after Path A writes complete and before the Path B entry gate.
This ordering guarantees:
- `existing_supports_pairs` (Phase 2) covers the tick-start state. Edges written
  in the same tick by Path C are NOT in `existing_supports_pairs` at write time —
  the pre-filter cannot catch intra-tick Path C duplicates. INSERT OR IGNORE is the
  authoritative dedup backstop. This is acceptable: Path C uses `(lo, hi)` canonical
  dedup from Phase 4 normalization, so no duplicate writes within Path C itself.
- Path B runs after Path C. When `nli_enabled=true`, Path B and Path C may both
  attempt `Supports` for the same pair. INSERT OR IGNORE on
  `UNIQUE(source_id, target_id, relation_type)` silently discards the second insert.
  The first writer wins. This is correct — one edge per typed pair, regardless of
  which path wrote it first. The `source` column is NOT part of the unique key (SR-04
  confirmed, see UNIQUE Constraint Verification below).

## Technology Decisions

- **No new HNSW scan** (constraint from SCOPE.md): Path C reuses `candidate_pairs`
  from Phase 4. This is the vector computed from `supports_candidate_threshold`
  (default 0.5 >= the cosine Supports threshold 0.65). All pairs in `candidate_pairs`
  have already passed the coarser threshold — Path C applies the finer 0.65 filter.
- **write_graph_edge sibling, not write_nli_edge parameterization** (ADR-001).
- **supports_cosine_threshold = 0.65 default** (ASS-035 empirical validation; see
  lesson #3713 for the calibration trap at 0.70+).
- **MAX_COSINE_SUPPORTS_PER_TICK = 50 constant** (ADR-004).
- **informs_category_pairs reuse** (SCOPE.md constraint; no separate allow-list).
- **weight = cosine** (SCOPE.md resolved decision §3): no PPR weight multiplier.
- **metadata = `{"cosine": f32}`** (SCOPE.md resolved decision §4).
- **No temporal ordering guard** (ASS-035 Group D: no benefit for Supports pairs;
  cosine Supports is semantically symmetric unlike directional Informs).
- **No same_feature_cycle guard** (ASS-035 Group D confirmed correctness at 0.65).

## UNIQUE Constraint Verification (SR-04)

Confirmed from `db.rs` (migration), `migration.rs` (v13 step), `analytics.rs` (test
helper), and `read.rs` (test helper) — all four DDL sites are consistent:

```sql
UNIQUE(source_id, target_id, relation_type)
```

The `source` column is NOT in the UNIQUE constraint. When both Path B (NLI) and
Path C (cosine) attempt a `Supports` edge for the same `(source_id, target_id)` pair
in the same tick, INSERT OR IGNORE correctly silences the second insert and returns
`false` from `write_graph_edge`. No duplicate rows, no PPR graph corruption. Delivery
must NOT treat the `false` return as an error — it is an expected no-op.

## Integration Points

### Existing interfaces consumed

| Component | Symbol | Location |
|-----------|--------|----------|
| Store | `query_existing_supports_pairs()` | `unimatrix-store/src/read.rs` |
| Store | `write_pool_server()` | `unimatrix-store/src/db.rs` |
| InferenceConfig | `supports_candidate_threshold: f32` | `infra/config.rs` |
| InferenceConfig | `informs_category_pairs: Vec<[String; 2]>` | `infra/config.rs` |
| InferenceConfig | `max_graph_inference_per_tick: usize` | `infra/config.rs` |
| nli_detection.rs | `write_nli_edge(...)` | `services/nli_detection.rs` |
| nli_detection.rs | `current_timestamp_secs()` | `services/nli_detection.rs` |
| nli_detection_tick.rs | `candidate_pairs: Vec<(u64, u64, f32)>` | Phase 4 local |
| nli_detection_tick.rs | `existing_supports_pairs: HashSet<(u64, u64)>` | Phase 2 local |
| nli_detection_tick.rs | `all_active: Vec<EntryRecord>` | Phase 2 local |
| nli_detection_tick.rs | `MAX_INFORMS_PER_TICK: usize = 25` | module constant |
| read.rs | `EDGE_SOURCE_NLI: &str` | `unimatrix-store/src/read.rs` |
| read.rs | `EDGE_SOURCE_CO_ACCESS: &str` | `unimatrix-store/src/read.rs` |

### New interfaces introduced

| Component | Symbol | Notes |
|-----------|--------|-------|
| read.rs | `EDGE_SOURCE_COSINE_SUPPORTS: &str = "cosine_supports"` | new constant |
| lib.rs | re-export `EDGE_SOURCE_COSINE_SUPPORTS` | follows existing pattern |
| nli_detection.rs | `write_graph_edge(store, src, tgt, rel, weight, ts, source, meta) -> bool` | new pub(crate) |
| config.rs | `InferenceConfig.supports_cosine_threshold: f32` | default 0.65 |
| config.rs | `default_supports_cosine_threshold() -> f32` | serde backing fn |
| nli_detection_tick.rs | `MAX_COSINE_SUPPORTS_PER_TICK: usize = 50` | module constant |

## Integration Surface

| Integration Point | Type/Signature | Source |
|-------------------|---------------|--------|
| `write_graph_edge` | `async fn(store: &Store, source_id: u64, target_id: u64, relation_type: &str, weight: f32, created_at: u64, source: &str, metadata: &str) -> bool` | `nli_detection.rs` (new) |
| `EDGE_SOURCE_COSINE_SUPPORTS` | `&str = "cosine_supports"` | `read.rs` (new) |
| `supports_cosine_threshold` | `f32`, default `0.65`, range `(0.0, 1.0)` exclusive | `infra/config.rs` (new field) |
| `MAX_COSINE_SUPPORTS_PER_TICK` | `usize = 50` | `nli_detection_tick.rs` (new constant) |
| `candidate_pairs` | `Vec<(u64, u64, f32)>` — `(source_id, target_id, cosine)`, canonical `(lo, hi)` | Phase 4 local in `run_graph_inference_tick` |
| `existing_supports_pairs` | `HashSet<(u64, u64)>` — canonical `(lo, hi)` pairs | Phase 2 local in `run_graph_inference_tick` |
| `informs_category_pairs` | `Vec<[String; 2]>` — `[source_category, target_category]` allow-list | `InferenceConfig` field |
| `UNIQUE(source_id, target_id, relation_type)` | DDL constraint — does NOT include `source` | `graph_edges` table DDL in `db.rs` / `migration.rs` |

## Error Handling Strategy (SR-07: Tick Infallibility)

`run_graph_inference_tick` has signature `async fn(...) -> ()`. It is infallible.
Path C must maintain this contract:

1. SQL errors from `write_graph_edge` return `false` (already logged inside the helper).
   The Path C loop treats `false` as a non-fatal write failure and continues.
2. Category lookup on `all_active` (to get `source_category` and `target_category` for
   a given `source_id`/`target_id`) may return `None` if an entry was deprecated between
   Phase 2 DB read and Path C execution. `if let Some(...) = ...` with `continue` on
   `None` — no `unwrap`, no `?`.
3. NaN/Inf cosine guard: cosine values from Phase 4 should be finite (HNSW contract),
   but Path C must apply the same `!weight.is_finite()` guard used by Path A before any
   weight computation.
4. No `?` propagation anywhere in Path C. All errors log at `warn!` and continue the loop.
5. No `unwrap()` calls in Path C. This is enforced by the project-wide no-unwrap rule
   for non-test code.

## impl Default Trap Mitigation (Pattern #4011)

`InferenceConfig` has both:
- `#[serde(default = "fn")]` per field — deserialization default
- `impl Default for InferenceConfig { ... }` with explicit literal values — code default

These two paths are maintained independently. Any field added with only one site updated
causes silent behavioral divergence (lesson #4014, gate-3b rework in crt-038).

For `supports_cosine_threshold`, delivery MUST update BOTH sites:
1. `#[serde(default = "default_supports_cosine_threshold")]` on the field declaration
2. `fn default_supports_cosine_threshold() -> f32 { 0.65 }` — new backing function
3. `supports_cosine_threshold: 0.65` in the `InferenceConfig { ... }` struct literal
   inside `impl Default`

A unit test MUST assert `InferenceConfig::default().supports_cosine_threshold == 0.65`
independently of the serde deserialization test. Grep the test module for all literal
references to `supports_cosine_threshold` before marking AC-09/AC-10 complete — the spec
may not enumerate all test sites (pattern #4011/#4013).

## GraphCohesionMetrics Observability (SR-02)

`inferred_edge_count` counts `source = 'nli'` only (hardcoded in `compute_graph_cohesion_metrics`).
Cosine Supports edges (`source = 'cosine_supports'`) will NOT appear in `inferred_edge_count`.

The eval gate for crt-040 uses `supports_edge_count` (counts all rows with
`relation_type = 'Supports'`), which is source-agnostic. This is the correct metric.
`inferred_edge_count` staleness is a naming issue — renaming is deferred (SCOPE.md
resolved decision §5). A follow-up issue should be filed to track this.

`supports_edge_count` is confirmed source-agnostic from the query in `read.rs`:
```sql
SUM(CASE WHEN relation_type = 'Supports' THEN 1 ELSE 0 END) AS supports_edge_count
```

## Constraints Checklist

- No new HNSW scan in Path C: path reuses `candidate_pairs` from Phase 4.
- `informs_category_pairs` reuse: no separate `supports_category_pairs` field.
- `write_nli_edge` not modified in behavior: existing callers unaffected.
- W1-2 contract: Path C has no `score_batch` call, no rayon usage, no `spawn_blocking`.
- R-09 Rayon/Tokio boundary: Path C is async Tokio only, same as Path A.
- No migration required: `graph_edges.source` column exists.
- `inferred_edge_count` backward compat: count remains `source = 'nli'` only.
- 500-line file limit: `nli_detection_tick.rs` is already large. If Path C extraction
  to a helper module is warranted, the delivery agent decides at implementation time.
  The constant and the write loop must be co-located with the tick function for clarity,
  but a private helper function `run_cosine_supports_path` can be extracted if the tick
  function body exceeds readable bounds.

## Open Questions

1. **SR-05 (follow-on):** `informs_category_pairs` is reused as the Supports category
   filter. If a future feature adds same-category Supports pairs, a separate
   `supports_category_pairs` config field would be needed. File as a follow-up issue;
   not in scope for crt-040.

2. **SR-03 (follow-on):** `MAX_COSINE_SUPPORTS_PER_TICK = 50` is not operator-tunable.
   A comment at the constant definition should note config-promotion as a future
   option if operators need tuning.

3. **inferred_edge_count rename (follow-on):** `inferred_edge_count` counts only
   `source='nli'` but now understates total inferred edges. A follow-up issue should
   track adding a `cosine_supports_edge_count` field to `GraphCohesionMetrics` or
   renaming `inferred_edge_count` to `nli_inferred_edge_count`.
