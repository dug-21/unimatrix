# Agent Report: crt-046-agent-1-architect

## Status: COMPLETE

## Artifacts Produced

- `/workspaces/unimatrix/product/features/crt-046/architecture/ARCHITECTURE.md`
- `/workspaces/unimatrix/product/features/crt-046/architecture/ADR-001-behavioral-signals-module.md`
- `/workspaces/unimatrix/product/features/crt-046/architecture/ADR-002-goal-clusters-write-strategy.md`
- `/workspaces/unimatrix/product/features/crt-046/architecture/ADR-003-goal-cluster-cosine-scan-recency-cap.md`
- `/workspaces/unimatrix/product/features/crt-046/architecture/ADR-004-briefing-null-short-circuit.md`

## ADR Unimatrix IDs

| ADR File | Title | Unimatrix ID |
|----------|-------|-------------|
| ADR-001-behavioral-signals-module.md | Behavioral Signal Logic in a Separate Module | #4110 |
| ADR-002-goal-clusters-write-strategy.md | goal_clusters Write Strategy — Direct write_pool + INSERT OR IGNORE | #4111 |
| ADR-003-goal-cluster-cosine-scan-recency-cap.md | Goal-Cluster Cosine Scan with Recency Cap of 100 Rows | #4112 |
| ADR-004-briefing-null-short-circuit.md | Briefing Goal Embedding NULL Short-Circuit Before Any Cluster DB Query | #4113 |

## Key Design Decisions

1. **New module `services/behavioral_signals.rs`** (ADR-001): all behavioral signal
   logic extracted here — keeps tools.rs under 500 lines, enables unit testing
   without full server stack.

2. **SR-04 resolved as INSERT OR IGNORE throughout** (ADR-002): additive-only
   invariant applies to goal_clusters; force=true re-runs are no-ops. Direct
   write_pool_server() (not analytics drain) for structural writes.

3. **Recency cap of 100 rows for cosine scan** (ADR-003): resolves SR-09 by
   bounding the in-process O(N×D) scan. `created_at DESC` index added to schema.
   Constants: COSINE_THRESHOLD=0.80, RECENCY_CAP=100, PAIR_CAP=200.

4. **Two-level NULL short-circuit at briefing time** (ADR-004): resolves SR-07.
   If `session_state.feature` absent — skip all blending. If goal embedding is
   NULL — skip cluster query. IndexBriefingService interface unchanged.

5. **Step 8b memoisation gate clarified**: the `force=false` cache-hit early
   return (step 2.5) skips step 8b. Full pipeline (cache miss OR force=true)
   always runs step 8b. INSERT OR IGNORE makes both paths idempotent.

6. **Parse-failure observability** (SR-01): `parse_failures` count from
   `collect_coaccess_entry_ids` is logged at `warn!` level per invocation.
   Not added to CycleReviewRecord (would require SUMMARY_SCHEMA_VERSION bump —
   out of scope).

7. **write_graph_edge return contract** (pattern #4041): delivery agent must
   increment `edges_enqueued` counter only on `Ok(true)`. `Ok(false)` = UNIQUE
   conflict, not an error.

## New Store Methods

Three new methods on SqlxStore (in new file `unimatrix-store/src/goal_clusters.rs`):
- `get_cycle_start_goal_embedding(cycle_id) -> Result<Option<Vec<f32>>>` — in db.rs
- `insert_goal_cluster(feature_cycle, goal_embedding, phase, entry_ids_json, outcome, created_at) -> Result<bool>`
- `query_goal_clusters_by_embedding(embedding, threshold, recency_limit) -> Result<Vec<GoalClusterRow>>`

## Schema Migration Cascade Sites (v21 → v22)

Full 7-touchpoint new-table checklist (pattern #3894) applies. Key gates:
- migration.rs: add `if current_version < 22` block
- db.rs: matching DDL + bump hardcoded integer to 22
- sqlite_parity.rs: add goal_clusters table + column count (7 columns) assertions
- server.rs: update both `assert_eq!(version, 21)` sites to 22
- Previous migration test: rename to `at_least_21` with `>= 21` predicate
- Gate check: `grep -r 'schema_version.*== 21' crates/` must return zero matches

## Open Questions for Other Agents

- OQ-1: `blend_cluster_entries` bulk fetch — confirm `store.get_entries_by_ids()`
  method name and signature (delivery agent).
- OQ-2: Sessions with `current_goal` Some but `feature` None — confirm cold-start
  is correct (no inline embedding). Spec agent to confirm.
- OQ-3: `AnalyticsWrite::GraphEdge` shedding policy — confirm `bootstrap_only=false`
  edges are NOT subject to shed (delivery agent to verify analytics drain docs).
