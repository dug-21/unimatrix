# Bug Investigation Report: 444-investigator

## Bug Summary

Three index-correctness gaps share the root cause that the retrieval indices (VECTOR_MAP/HNSW and TypedRelationGraph) are never forced to match the active entry set. Active entries stored while the embed adapter was unavailable persist with `embedding_dim = 0` and a VECTOR_MAP row pointing to a data_id with no HNSW entry. Quarantined entries remain as nodes in HNSW and in TypedRelationGraph. No code path in the maintenance tick detects or repairs either condition.

## Root Cause Analysis

### Gap 1 — Unembedded active entries are never healed

Both `server::insert_with_audit()` (server.rs:454) and `StoreService::insert()` (store_ops.rs:140) call `put_vector_mapping(entry_id, data_id)` unconditionally, then gate HNSW insert on `if !embedding.is_empty()`. When the embed adapter is unavailable, the entry lands in ENTRIES with `embedding_dim = 0`, a VECTOR_MAP row referencing an allocated `data_id`, and no HNSW point. The issue description says "no VECTOR_MAP row" but this is imprecise — there IS a row, it is just orphaned (no corresponding HNSW point).

The maintenance tick (`run_maintenance()`, status.rs:880) iterates active entries only for confidence refresh and graph compaction — no query for `embedding_dim = 0`, no `VectorIndex::contains()` check, no re-embed loop. The NLI graph inference tick (`nli_detection_tick.rs:96-100`) explicitly builds an `embedded_ids` set and filters out unembedded entries as source candidates — skipping them, not healing them.

### Gap 2 — Quarantined entries are never pruned from VECTOR_MAP / HNSW

`store.update_status(id, Status::Quarantined)` updates the status column only. The VECTOR_MAP row is not deleted; the HNSW point is not removed. `compact()` (index.rs:381) only fires when `graph_stale_ratio > DEFAULT_STALE_RATIO_TRIGGER` (status.rs:1037) and rebuilds from active_entries only — quarantined VECTOR_MAP rows are cleared only as a side-effect of compaction, not on quarantine.

### Gap 3 — TypedRelationGraph includes quarantined (and deprecated) entries

`TypedGraphState::rebuild()` (typed_graph.rs:91) calls `store.query_all_entries()` — no status filter. All entries including Quarantined and Deprecated become nodes in the graph. For PPR (uses Supports/CoAccess/Prerequisite edges — graph_ppr.rs), quarantined nodes propagate mass to neighboring nodes, inflating their scores.

The search path does include a per-entry quarantine check for PPR-only expansion candidates (search.rs:946), but the quarantined node remains in the graph and can still route mass through its edges before that check fires.

### Gap 4 — `check_embeddings` metric is opt-in and does not cover the unembedded case directly

`check_embedding_consistency()` (contradiction.rs:265) operates on entries from `query_by_status(Active)`, which includes `embedding_dim = 0` entries. A fresh embed + HNSW search would detect them as inconsistent — but only if the embed adapter is available at call time, and only if `check_embeddings=true` is passed (defaults to `false`, tools.rs:904). The background tick never calls this check. There is no fast SQL count of `embedding_dim = 0` entries in the status report.

### Code Path Traces

**Gap 1**:
`context_store` → `StoreService::insert()` → `get_adapter()` fails → `embedding = vec![]` → `put_vector_mapping()` called → `insert_hnsw_only` skipped → entry stored with `embedding_dim = 0` and orphaned VECTOR_MAP row. Background tick → `maintenance_tick()` → `run_maintenance()` — no heal logic.

**Gap 2**:
`context_quarantine` → `store.update_status(id, Quarantined)` — VECTOR_MAP and HNSW untouched. `maintenance_tick()` → compaction only if stale ratio > threshold, active_entries only.

**Gap 3**:
Background tick → `TypedGraphState::rebuild()` (typed_graph.rs:91) → `store.query_all_entries()` (no filter) → `build_typed_relation_graph(all_entries, all_edges)` — quarantined entries are graph nodes with traversable edges.

**Gap 4**:
`context_status(check_embeddings=true)` → `check_embedding_consistency(active_entries)` — opt-in, not in background tick, no SQL count path.

## Affected Files and Functions

| File | Function | Role in Bug |
|------|----------|-------------|
| `crates/unimatrix-server/src/services/status.rs` | `run_maintenance()` | Missing heal pass and prune pass |
| `crates/unimatrix-server/src/services/typed_graph.rs` | `TypedGraphState::rebuild()` | Uses `query_all_entries()` — includes quarantined nodes |
| `crates/unimatrix-server/src/background.rs` | `maintenance_tick()` | No heal/prune steps wired in |
| `crates/unimatrix-server/src/services/store_ops.rs` | `StoreService::insert()` | Writes VECTOR_MAP unconditionally even when embedding is empty |
| `crates/unimatrix-server/src/server.rs` | `insert_with_audit()` | Same pattern as store_ops.rs |
| `crates/unimatrix-server/src/infra/contradiction.rs` | `check_embedding_consistency()` | Does not cover embedding_dim=0 as a dedicated metric path |
| `crates/unimatrix-vector/src/index.rs` | `compact()` | Correct; only issue is it is not triggered on quarantine |
| `crates/unimatrix-engine/src/graph.rs` | `build_typed_relation_graph()` | Correct; issue is in what it receives |

## Proposed Fix Approach

### Fix 1 — Heal pass in `run_maintenance()` (status.rs)

After Step 2 (confidence refresh), before Step 3 (graph compaction):

1. Query: `SELECT id, title, content FROM entries WHERE status = 0 AND embedding_dim = 0` (or check `VectorIndex::contains()`).
2. Cap at ~20 entries per tick (same throttle shape as NLI).
3. For each: call `get_adapter()`, embed, adapt, normalize, then `vector_index.insert_hnsw_only(entry_id, data_id, &embedding)` using the existing VECTOR_MAP `data_id` from `store.get_vector_mapping(entry_id)`. Update `embedding_dim` in ENTRIES.

The VECTOR_MAP row already exists — heal pass only needs to populate the HNSW point and update `embedding_dim`.

### Fix 2 — Prune pass in `run_maintenance()` (status.rs)

Before graph compaction:

1. Query: `SELECT vm.entry_id FROM vector_map vm INNER JOIN entries e ON e.id = vm.entry_id WHERE e.status = 3`.
2. For each: delete the VECTOR_MAP row, remove the entry from `VectorIndex`'s `IdMap` (mark stale). Since hnsw_rs has no deletion API, expose a `remove_entry(entry_id)` method on `VectorIndex` that removes from `entry_to_data`/`data_to_entry` maps (same mechanism as re-embed path at index.rs:164). The stale point is then cleaned up by the next `compact()` cycle.
3. Alternative: if quarantined entries are found, trigger a `compact()` immediately with only non-quarantined (active + deprecated) embeddings.

### Fix 3 — Filter TypedRelationGraph rebuild to exclude quarantined entries (`typed_graph.rs`)

In `TypedGraphState::rebuild()` at line 93, change:
```rust
let all_entries = store.query_all_entries().await?;
```
to filter out `Status::Quarantined` before passing to `build_typed_relation_graph()`. Deprecated entries should be retained for Supersedes chain traversal (graph_penalty uses Supersedes-only per SR-01). Minimum correct fix: exclude Quarantined only.

### Fix 4 — Add `unembedded_active_count` to status report (`status.rs::compute_report()`)

Add a fast SQL count: `SELECT COUNT(*) FROM entries WHERE status = 0 AND embedding_dim = 0`. Expose as `unembedded_active_count` in `StatusReport` and incorporate into `embedding_consistency_score`. This makes the metric self-reporting without requiring opt-in `check_embeddings`.

## Risk Assessment

- **Blast radius**: Heal pass adds HNSW points for entries already in VECTOR_MAP (bounded, no data loss). Prune pass removes HNSW-reachability of quarantined entries — need to verify context_restore path re-inserts into HNSW. Graph filter removes quarantined nodes from traversal (conservative, no existing behavior depends on quarantined node traversal).
- **Regression risk**: Prune pass — if quarantine is later reversed, the HNSW point must be re-added. The current context_restore path likely does not re-insert into HNSW; needs verification. Graph filter — excluding deprecated from TypedRelationGraph could affect `find_terminal_active` for Supersedes chains through deprecated middle nodes; filter should only target Quarantined unless deprecated exclusion is independently validated.
- **Confidence**: High for root cause of all four gaps. Medium for prune pass implementation detail (VectorIndex API surface for mark-as-stale). High for graph filter and heal pass.

## Missing Tests

1. **Heal pass**: Store entry with empty embedding → run `maintenance_tick()` with functioning embed adapter → assert `embedding_dim > 0`, `VectorIndex::contains(id) == true`, entry appears in semantic search.
2. **Prune pass**: Store entry with real embedding → quarantine it → run `maintenance_tick()` → assert `VectorIndex::contains(id) == false`, no VECTOR_MAP row for entry.
3. **Graph filter**: Rebuild `TypedGraphState` from store with one active + one quarantined entry connected by Supports edge → assert quarantined entry absent from `all_entries` → assert PPR from active entry does not propagate to quarantined entry_id.
4. **Metric**: Store entry with empty embedding → call `compute_report()` → assert `unembedded_active_count > 0` and `embedding_consistency_score < 1.0`.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned ADR entries for HNSW, graph compaction, crt-005, quarantine restore. No existing lesson covered the heal/prune invariant gap.
- Stored: entry #3761 "Maintenance tick must enforce VECTOR_MAP/HNSW/graph invariants — heal unembedded, prune quarantined" via `/uni-store-lesson`
