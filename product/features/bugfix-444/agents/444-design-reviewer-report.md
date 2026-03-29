# Design Review Report: 444-design-reviewer

## Assessment: APPROVED WITH NOTES

Fixes 1, 3, and 4 are sound and can proceed. Fix 2 has one blocking issue (restore path) and one design choice that needs a concrete resolution. Specific notes below.

---

## Findings

### Fix 1 — Heal pass in `run_maintenance()`

**Non-blocking: embed-service availability and per-tick spike**

The heal pass calls `get_adapter()` followed by `embed_entries()` inside the background tick. This is functionally correct — the NLI tick already follows the same "get_adapter, then embed capped list" pattern. The 20-entry cap is appropriate. However, there is a subtle difference: `embed_entries` is called with a batch of up to 20 pairs in one call, not 20 individual `embed_entry` calls. This is the correct approach (same as compaction at line 1044). No issue here.

**Non-blocking: idempotency guard is implicit, not explicit**

The query `WHERE status = 0 AND embedding_dim = 0` is idempotent across ticks only because a successful heal updates `embedding_dim` in ENTRIES. If the `update_embedding_dim` write succeeds but the HNSW insert fails, the row will be skipped on the next tick even though the HNSW point is still missing. The proposal should specify: update `embedding_dim` in ENTRIES **after** a successful `insert_hnsw_only`, not before. The existing `insert_hnsw_only` is synchronous and infallible under valid input (validates dimension and float values, then inserts). Write order: embed → `insert_hnsw_only` (in-memory, sync) → `update_embedding_dim` (DB write). This makes the DB write the confirmation step.

**Non-blocking: VECTOR_MAP row existence assumption**

The heal pass assumes the VECTOR_MAP row already exists (investigator confirmed: it IS present, `data_id` was allocated at insert time). `store.get_vector_mapping(entry_id)` may return `None` only if the row was deleted by a prune pass that ran first in the same tick. Since prune runs before graph compaction (Fix 2 positioning) and heal is proposed to run after confidence refresh and before graph compaction, the ordering is: heal first, prune second. This ordering is wrong — if a quarantined entry also happens to have `embedding_dim = 0`, heal would attempt to re-embed it. The correct ordering is: **prune first, then heal**. This eliminates the risk of healing a quarantined entry. The investigator's proposed sequencing (heal after Step 2, before Step 3) must be amended: prune must run before heal.

**Non-blocking: embed-service unavailability**

If `get_adapter()` fails, the heal pass should log a `tracing::debug!` and return (same as compaction path). This is standard behavior in the codebase. The proposal implies this pattern by saying "same throttle shape as NLI" but should be stated explicitly.

---

### Fix 2 — Prune pass in `run_maintenance()`

**Blocking: restore path does not re-insert into HNSW**

`restore_with_audit` calls `change_status_with_audit`, which calls `store.update_entry_status_extended`. This writes only to ENTRIES — it does not call `insert_hnsw_only`, does not call `put_vector_mapping`, and does not call `compact`. If the prune pass deletes the VECTOR_MAP row and marks the IdMap stale, a subsequent restore leaves the entry with no HNSW point and no VECTOR_MAP row. The entry becomes permanently unsearchable until the next compaction cycle (which itself requires the stale ratio to exceed threshold).

This is a pre-existing gap that the prune pass would make operational: today, quarantined entries are never pruned, so restore can succeed silently. After Fix 2 ships, restore of a pruned entry will result in a searchable-by-status-filter but not searchable-by-vector entry.

**Required addition:** `restore_with_audit` must re-insert into HNSW on restore. The heal pass in Fix 1 provides the correct pattern. On restore:
1. If the entry has non-zero `embedding_dim` and no current VECTOR_MAP row (pruned), allocate a new `data_id`, write `put_vector_mapping` inside the status-update transaction (or immediately after), then call `insert_hnsw_only`.
2. If the entry has `embedding_dim = 0` (was never embedded), the next heal pass tick will pick it up. This is acceptable — the restore path need not re-embed.

This is a new code path, not just a note. It is a blocking issue because the prune pass is only safe to ship if restore is HNSW-aware.

**Non-blocking: prune alternative selection**

The "mark-stale in IdMap" approach (removing from `entry_to_data`/`data_to_entry` without touching the HNSW point) is architecturally correct. This is exactly the re-embedding stale-point pattern already in `VectorIndex::insert` (line 163-165 of index.rs). The stale HNSW point becomes invisible to search (skipped by `map_neighbours_to_results` because `data_to_entry.get(&data_id)` returns None) and is cleaned on the next `compact`. There is no need to trigger an immediate compaction for correctness — the stale-point mechanism is already battle-tested for this purpose.

The "trigger compact immediately" alternative is not preferable: compact requires all active embeddings plus the embed service to be available, adds O(n) embed calls at prune time rather than amortizing them, and duplicates compaction scheduling logic. Prefer the mark-stale path.

The `VectorIndex::remove_entry(entry_id)` method proposed needs only to:
1. Acquire write lock on `id_map`.
2. Remove `entry_to_data.get(&entry_id)` → get `old_data_id`.
3. Remove `data_to_entry.remove(&old_data_id)`.
4. Remove `entry_to_data.remove(&entry_id)`.

No HNSW write, no async. This is a pure IdMap mutation — identical to the stale-point path in `insert`. It does not need `async` and does not need to touch the store.

**Non-blocking: VECTOR_MAP deletion in prune pass**

The proposal also deletes the VECTOR_MAP row for each pruned entry. This is correct: the row must be deleted so that a future `compact()` (which calls `rewrite_vector_map`) starts from a clean slate. Without the delete, the old `data_id` persists in VECTOR_MAP, and after compact the `data_id` counter resets to `embeddings.len()` which will alias the old `data_id`. In practice, `rewrite_vector_map` does a full DELETE + re-insert (see `write_ext.rs:206`), so the old row is cleared by compaction anyway. Deleting at prune time is a correctness belt-and-suspenders: correct, not strictly required before compact, but good hygiene. Include it.

---

### Fix 3 — TypedRelationGraph rebuild filter

**Non-blocking: excluding Quarantined only is the correct minimum**

`find_terminal_active` traverses outgoing Supersedes edges. A chain like `Active → Deprecated → Active` is a valid Supersedes path (entry A is superseded by B, B is superseded by C, all reachable). Deprecated nodes that are intermediate hops must remain in the graph for chain traversal — `find_terminal_active` checks `status == Active && superseded_by.is_none()` at each node, so deprecated middle nodes are traversed but never returned as terminals. Removing them would break chain resolution for ADR correction chains.

Quarantined nodes are different: a quarantined entry represents poisoned or unreliable content. There is no valid Supersedes chain reason to traverse through a quarantined node — an admin would not set up a chain through quarantined content by design. Excluding Quarantined only is correct.

**Non-blocking: filter placement**

The filter must be applied to the `all_entries` slice passed to `build_typed_relation_graph`, not only to `all_entries` stored in `TypedGraphState`. Both places must be filtered: the graph builder adds a node for every entry it receives, and the `all_entries` stored in `TypedGraphState` is used by `graph_penalty` and `find_terminal_active` on the search hot path. A quarantined entry absent from `all_entries` but present as a graph node (or vice versa) would produce confusing penalty behavior. The consistent approach: filter once before passing to `build_typed_relation_graph`, store the filtered slice in `all_entries`.

**Non-blocking: `all_entries` snapshot in the search path**

The search path reads `all_entries` from `TypedGraphState` under a read lock, then clones and releases. After Fix 3, the snapshot will contain no quarantined entries. `graph_penalty` called with a quarantined `node_id` that is no longer in the graph returns `1.0` (no penalty, per the guard at graph.rs:386). This is conservative and correct — quarantined entries are already excluded from search results at a prior gate (Step 6D), so reaching `graph_penalty` for a quarantined entry should not happen.

---

### Fix 4 — `unembedded_active_count` metric

**Non-blocking: pure read, correct positioning**

This is a fast SQL COUNT query. No write risk. It belongs in `compute_report()` alongside the other SQL aggregates already executed there. Adding it to `embedding_consistency_score` is appropriate — the score becomes non-trivially actionable without requiring `check_embeddings=true`. No architectural concerns.

**Non-blocking: metric naming**

`unembedded_active_count` accurately describes the field. Incorporating it into `embedding_consistency_score` as `1.0 - (unembedded_active_count as f64 / active_count as f64)` is the natural formulation, guarded for division-by-zero when `active_count == 0`.

---

### Hot-Path Risk Assessment

The heal pass and prune pass run inside `run_maintenance()`, which is called by `background_tick_loop` — not by any MCP request handler. No hot-path risk. The tick already does O(n) DB reads (confidence refresh, co-access cleanup) and can do O(n) embed calls (compaction). Adding a capped 20-entry embed pass and a small JOIN query for quarantined VECTOR_MAP rows is within the established cost model.

The one concern is embed-service availability: if the embed service is slow or under load, the heal pass adds latency to an already-long tick. The 20-entry cap is the correct mitigation. No change needed.

---

### Security Surface

No new trust boundaries. The prune and heal passes operate on data from the store using the existing write pool. No external input is processed. `SYSTEM_AGENT_ID` is used for background tick audit events (established pattern). No new privilege paths.

The `remove_entry(entry_id)` method on `VectorIndex` is an in-memory mutation. It should be `pub(crate)` or `pub` — but given that `VectorIndex` already exposes `insert_hnsw_only` as `pub`, `remove_entry` at the same visibility is appropriate.

---

## Revised Fix Approach (Amendments)

### Amendment 1 — Ordering: prune before heal

Revised step order in `run_maintenance()`:
1. Co-access cleanup (unchanged — Step 1)
2. Confidence refresh (unchanged — Step 2)
3. **Prune pass** (new — quarantined VECTOR_MAP/HNSW cleanup)
4. **Heal pass** (new — unembedded active entry re-embed)
5. Graph compaction (unchanged — Step 3)
6. (Steps 4–6 unchanged)

Rationale: prune first ensures the heal pass does not attempt to embed quarantined entries with `embedding_dim = 0`.

### Amendment 2 — Heal pass write order

Write order within each heal iteration:
```
embed entry → insert_hnsw_only (sync) → update embedding_dim in ENTRIES
```
The DB write is the confirmation step. A crash after `insert_hnsw_only` and before the DB write is safe: next tick re-runs the query and the HNSW insert is idempotent via IdMap replacement (re-inserts with a new `data_id`, old point becomes stale, cleaned at next compact).

### Amendment 3 — Restore path must re-insert into HNSW (blocking)

`restore_with_audit` must be extended:
1. After the status update, check if `vector_index.contains(entry_id)` returns false.
2. If false and `entry.embedding_dim > 0`: allocate a new `data_id` via `vector_index.allocate_data_id()`, call `store.put_vector_mapping(entry_id, data_id)`, then call `vector_index.insert_hnsw_only(entry_id, data_id, &embedding)`.
   - Obtaining the embedding requires `embed_service.get_adapter()` + `adapter.embed_entry(title, content)`. If the embed service is unavailable at restore time, log a warning and skip HNSW re-insert — the heal pass will pick it up on the next tick (entry will have `embedding_dim > 0` so the heal pass needs to check IdMap presence, not just `embedding_dim == 0`).
3. If false and `entry.embedding_dim == 0`: no action; heal pass will embed it.

**Note:** This means the heal pass query also needs to cover restored entries that lost their HNSW point but have `embedding_dim > 0`. The heal query must be:
```sql
SELECT id, title, content FROM entries WHERE status = 0 AND embedding_dim > 0
```
followed by a Rust-side `!vector_index.contains(entry_id)` filter, OR a separate query for `embedding_dim = 0` plus a `contains` scan of all active entries. The simplest approach:

**Heal query A** (was-never-embedded): `WHERE status = 0 AND embedding_dim = 0`
**Heal query B** (was-pruned, not yet re-embedded): iterate `active_entries` and check `!vector_index.contains(id)` for entries with `embedding_dim > 0`

Or combine: for the heal pass, check both conditions. The combined cap of 20 entries applies across both.

### Amendment 4 — Prune: use mark-stale, not immediate compact

Expose `VectorIndex::remove_entry(entry_id: u64)` as a synchronous method performing IdMap-only mutation (no HNSW write). Prune deletes the VECTOR_MAP row from the store and calls `remove_entry` to make the HNSW stale point invisible. No `compact()` trigger. The stale point is cleaned at the next normal compaction.

---

## Knowledge Stewardship

**Queried:**
- `mcp__unimatrix__context_search` — "vector index maintenance hnsw compaction" → retrieved ADR-004 (Graph Compaction Atomicity, #180), ADR-001 (hnsw_rs library choice, #63), ADR-002 (Maintenance Opt-Out, #178)
- `mcp__unimatrix__context_search` — "graph compaction atomicity build-new-then-swap" → full content of ADR-004 (#180), ADR-002 corrected (#3559), VECTOR_MAP atomicity ADR (#91)
- `mcp__unimatrix__context_search` — "hnsw_rs deletion point removal no delete API" → no additional ADRs beyond ADR-004

**Stored:** Declined — no new architectural patterns discovered. The findings in this review are bug-fix-specific. The restore path gap will be documented by the implementer as part of the fix.
