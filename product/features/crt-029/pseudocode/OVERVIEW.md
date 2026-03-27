# crt-029 Pseudocode Overview — Background Graph Inference

## Components Involved

| Component | File | Action |
|-----------|------|--------|
| inference-config | `crates/unimatrix-server/src/infra/config.rs` | Modify — four new fields + Default + validate |
| store-query-helpers | `crates/unimatrix-store/src/read.rs` | Modify — two new async query methods |
| nli-detection-tick | `crates/unimatrix-server/src/services/nli_detection_tick.rs` | Create — new module (primary deliverable) |
| background-call-site | `crates/unimatrix-server/src/background.rs` | Modify — two lines after bootstrap promotion |
| services/mod.rs | `crates/unimatrix-server/src/services/mod.rs` | Modify — one `pub mod` declaration |
| nli_detection.rs (visibility) | `crates/unimatrix-server/src/services/nli_detection.rs` | Modify — three `pub(crate)` promotions |

---

## Data Flow

```
background_tick_loop (background.rs)
  │
  └─ run_single_tick (background.rs)
        │
        ├─ maybe_run_bootstrap_promotion (nli_detection.rs)   [unchanged]
        │
        └─ [crt-029 addition — after bootstrap promotion]
             if inference_config.nli_enabled {
               run_graph_inference_tick(store, nli_handle, vector_index, rayon_pool, config)
             }
                  │
                  Phase 1 ─ NliServiceHandle::get_provider()
                           ─ return if Err
                  │
                  Phase 2 ─ Store::query_by_status(Active)          → Vec<EntryRecord>
                           ─ Store::query_entries_without_edges()   → Vec<u64>
                           ─ Store::query_existing_supports_pairs() → HashSet<(u64,u64)>
                  │
                  Phase 3 ─ select_source_candidates(all_active, edge_set, isolated_ids, cap)
                           → Vec<u64>   [bounded to max_graph_inference_per_tick]
                  │
                  Phase 4 ─ for each source_id in candidates:
                           ─   VectorIndex::get_embedding(id) → Option<Vec<f32>>
                           ─   VectorIndex::search(emb, k, ef) → Vec<SearchResult>
                           ─   collect (source_id, neighbor_id, similarity) triples
                           ─   deduplicate (min,max) pairs
                           ─   skip if pair in existing_supports_pairs
                  │
                  Phase 5 ─ sort pairs: cross-category → isolated endpoint → similarity desc
                           ─ truncate to max_graph_inference_per_tick
                  │
                  Phase 6 ─ Store::get_content_via_write_pool(id) for each pair endpoint
                           ─ skip pair if either content fetch fails (log debug)
                  │
                  Phase 7 ─ rayon_pool.spawn(sync closure: provider.score_batch(&pairs))
                           ─ await result        [W1-2; sync-only inside closure; no .await]
                  │
                  Phase 8 ─ write_inferred_edges_with_cap(store, pairs, scores,
                                supports_edge_threshold, max_graph_inference_per_tick)
                           ─ log edges_written at debug
                                │
                                └─ for each pair scoring above supports_edge_threshold:
                                   write_nli_edge(store, src, tgt, "Supports", weight, ts, meta)
                                   [pub(crate) from nli_detection.rs]
```

---

## Shared Types (new or modified)

### `InferenceConfig` — four new fields (inference-config.md)

```
supports_candidate_threshold: f32   // HNSW similarity floor before NLI; default 0.5
supports_edge_threshold: f32        // NLI entailment floor for Supports write; default 0.7
max_graph_inference_per_tick: usize // pair cap per tick; default 100
graph_inference_k: usize            // HNSW neighbour count for tick path; default 10
```

Invariant enforced by `validate()`: `supports_candidate_threshold < supports_edge_threshold`
(reject when `supports_candidate_threshold >= supports_edge_threshold`).

### `HashSet<(u64, u64)>` — normalized pair set

Pairs stored as `(min(a,b), max(a,b))`. Used in Phase 4 deduplication and as the pre-filter
from `query_existing_supports_pairs()`. Both usages normalise to the same canonical form.

### No new named structs

`ActiveEntryMeta` is **not** introduced as a named type. The tick passes `&[EntryRecord]`
slices directly into `select_source_candidates`. Implementation agent may introduce it if
preferred, but the pseudocode uses `&[EntryRecord]` to avoid requiring a new type.

---

## Sequencing Constraints

1. **nli_detection.rs `pub(crate)` promotions must be done first** (wave-1, before writing
   any `nli_detection_tick.rs` code that calls them). A compile error from missing visibility
   is the earliest possible failure signal — this is the desired catch.

2. **`pub mod nli_detection_tick;` in `services/mod.rs`** must be added before or alongside
   step 1. The module declaration causes the compiler to look for the file.

3. **Four new `InferenceConfig` fields** can be added in any wave. If added before the module
   exists, the `InferenceConfig` struct-literal coverage grep (`grep InferenceConfig {`) must
   be run at the end to catch the 52 existing occurrences (C-11 / AC-18).

4. **background-call-site** (two lines in `background.rs`) must come after the tick module
   compiles. The call site is the last step.

5. **Ordering within `run_single_tick`**: the new tick call MUST come after
   `maybe_run_bootstrap_promotion`. Reversed ordering means bootstrap-promoted edges may not
   be in the pre-filter HashSet that tick reads. No compile-time enforcement; this is a
   sequencing invariant verified by code review.

---

## Integration Surface (from ARCHITECTURE.md)

| Interface | Signature | Source |
|-----------|-----------|--------|
| `query_entries_without_edges` | `async fn(&self) -> Result<Vec<u64>>` | new, `read.rs` |
| `query_existing_supports_pairs` | `async fn(&self) -> Result<HashSet<(u64,u64)>>` | new, `read.rs` |
| `run_graph_inference_tick` | `pub async fn(store, nli_handle, vector_index, rayon_pool, config)` | new, `nli_detection_tick.rs` |
| `write_inferred_edges_with_cap` | `async fn(store, pairs, nli_scores, threshold, max_edges) -> usize` | new private, `nli_detection_tick.rs` |
| `select_source_candidates` | `fn(all_active, edge_set, isolated_ids, max_sources) -> Vec<u64>` | new private, `nli_detection_tick.rs` |
| `write_nli_edge` | `pub(crate) async fn(...)  -> bool` | promoted, `nli_detection.rs` |
| `format_nli_metadata` | `pub(crate) fn(scores: &NliScores) -> String` | promoted, `nli_detection.rs` |
| `current_timestamp_secs` | `pub(crate) fn() -> u64` | promoted, `nli_detection.rs` |
| `EDGE_SOURCE_NLI` | `&'static str = "nli"` | existing pub, `unimatrix_store::read` |

---

## Critical Constraints Summary

| ID | One-liner |
|----|-----------|
| C-01 / AC-08 | `score_batch` via `rayon_pool.spawn()` only — no `spawn_blocking`, no inline async NLI |
| C-13 / AC-10a | Tick writes NO `Contradicts` edges — `write_inferred_edges_with_cap` is Supports-only with no `contradiction_threshold` parameter |
| C-14 / R-09 | Rayon closure body MUST be sync-only: no `.await`, no `Handle::current()` |
| AC-06c / R-02 | Cap source candidates in Phase 3 (metadata only) BEFORE Phase 4 calls `get_embedding` |
| C-11 / R-07 | 52 `InferenceConfig {` struct literals must be updated before merge |
| C-08 | `nli_detection_tick.rs` must not exceed 800 lines |
