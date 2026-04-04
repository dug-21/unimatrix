# Agent Report: crt-046-agent-1-pseudocode

## Task

Produce per-component pseudocode files for crt-046 Behavioral Signal Delivery.
Four components: store-v22, behavioral_signals, cycle-review-step-8b, briefing-blending.

## Output Files

- `/workspaces/unimatrix/product/features/crt-046/pseudocode/OVERVIEW.md`
- `/workspaces/unimatrix/product/features/crt-046/pseudocode/store-v22.md`
- `/workspaces/unimatrix/product/features/crt-046/pseudocode/behavioral-signals.md`
- `/workspaces/unimatrix/product/features/crt-046/pseudocode/cycle-review-step-8b.md`
- `/workspaces/unimatrix/product/features/crt-046/pseudocode/briefing-blending.md`

## Components Covered

1. **store-v22** — Schema v22 migration cascade (9 touchpoints), goal_clusters DDL,
   GoalClusterRow struct, get_cycle_start_goal_embedding, insert_goal_cluster,
   query_goal_clusters_by_embedding, InferenceConfig three new fields, cosine_similarity helper.

2. **behavioral_signals** — New module services/behavioral_signals.rs.
   write_graph_edge helper (direct write_pool_server, returns Result<bool>),
   collect_coaccess_entry_ids, build_coaccess_pairs (self-pair exclusion, enumeration-time cap),
   outcome_to_weight, emit_behavioral_edges (write_graph_edge contract table leads),
   populate_goal_cluster, blend_cluster_entries (pure function).

3. **cycle-review-step-8b** — Step 8b insertion in context_cycle_review. Memoisation
   early-return repositioned AFTER step 8b. parse_failure_count as top-level JSON
   response field. get_latest_cycle_phase helper. Non-fatal error handling throughout.

4. **briefing-blending** — Two-level guard (Level 1: feature/goal absent; Level 2: no
   stored embedding). Score-based interleaving (Option A, ADR-005). cluster_score formula
   using EntryRecord.confidence (Wilson-score). blend_cluster_entries call. All four
   cold-start paths documented and tested.

## Open Questions for Gate 3a

### OQ-1 (from IMPLEMENTATION-BRIEF): get_by_ids bulk method

The IMPLEMENTATION-BRIEF and ARCHITECTURE reference `store.get_by_ids(ids)` as an
existing store method for fetching Active EntryRecords in bulk. Searching the codebase
confirms NO such method exists. The briefing-blending pseudocode uses individual
`store.get(id)` calls in a loop as the fallback.

**Before implementation**: confirm whether `get_by_ids` should be added as a new store
method in Wave 1 (store-v22) or whether individual `store.get(id)` calls are acceptable.
Individual calls are correct but less efficient for large cluster entry sets.

If `get_by_ids` is to be added, it belongs in `crates/unimatrix-store/src/read.rs` as:
```rust
pub async fn get_by_ids(&self, ids: &[u64]) -> Result<Vec<EntryRecord>>
// Fetches only Active-status entries matching the given IDs.
// Uses read_pool(). WHERE id IN (...) with status = Active filter.
```
This should be added to the store-v22 cascade if confirmed.

### OQ-2 (from ARCHITECTURE): feature.is_some() but current_goal absent

Confirmed in pseudocode (Resolution 3): `feature.is_some()` with `current_goal = ""`
activates Level 1 cold-start before any DB call. Consistent with I-04 test requirement.

### OQ-3 (from IMPLEMENTATION-BRIEF): bootstrap_only=false shed policy

The `emit_behavioral_edges` pseudocode uses `write_graph_edge` directly (write_pool_server)
rather than `enqueue_analytics`. This sidesteps the shed policy entirely. If the
implementation agent routes edges through `enqueue_analytics` instead, they must verify
that `bootstrap_only=false` edges are not subject to the shed policy in analytics.rs.
Current analytics.rs code does not branch on `bootstrap_only` for shed decisions.

### Memoisation Gate Implementation Detail

The cycle-review-step-8b pseudocode describes the control flow restructuring needed to
make step 8b run on cache-hit. The exact Rust implementation requires careful borrow
handling since `report` may not be available on the cache-hit path. The implementation
agent must determine how to extract `outcome` from the cached `CycleReviewRecord`
(likely by deserializing `summary_json` or by storing outcome separately in the record).
The pseudocode describes the outcome extraction but leaves the exact deserialization
path to the implementer.

### IndexEntry Construction from EntryRecord

The briefing-blending pseudocode shows constructing `IndexEntry` from `EntryRecord`
fields. The implementation agent must verify the exact `IndexEntry` constructor fields
match the current struct definition (id, topic, category, confidence, snippet).
The `confidence` field set on the constructed IndexEntry uses `record.confidence` (f64
Wilson-score) which is passed through for display — this does NOT violate the naming
collision rule because it's used for display, not for the cluster_score formula.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned 15 entries; most relevant:
  #4110 (ADR-001 module boundary), #4115 (ADR-003 recency cap), #4123 (ADR-005 blending),
  #4113 (ADR-004 NULL short-circuit), #3397 (briefing query derivation pattern).
- Retrieved: #4041 (write_graph_edge return contract — applied directly to emit_behavioral_edges
  contract table), #4108 (behavioral co-access pair recovery pattern — applied to
  collect_coaccess_entry_ids and emit approach).
- Queried: `context_search` for behavioral signals co-access graph edge patterns (category=pattern)
  — returned #4108, #2429, #4056. #4108 applied. #2429 (TypedRelationGraph filter) noted as context
  only.

- Deviations from established patterns:
  - `emit_behavioral_edges` uses `write_graph_edge` (direct `write_pool_server()`) rather
    than `enqueue_analytics`. This is necessary to implement the `write_graph_edge` return
    contract (pattern #4041) which requires `rows_affected()` feedback for accurate counter
    tracking. The ARCHITECTURE says "enqueue_analytics" but pattern #4041 (which the brief
    explicitly requires) is only achievable with a direct write. The brief says "pseudocode
    MUST begin with the write_graph_edge return contract table" which implies a synchronous
    write helper. This deviation is intentional and correct — the implementation agent should
    be aware that if `enqueue_analytics` is used instead, counter tracking becomes impossible
    (fire-and-forget returns no rows_affected data).
  - `store.get_by_ids()` does not exist; replaced with individual `store.get(id)` loop
    (see OQ-1 above).
