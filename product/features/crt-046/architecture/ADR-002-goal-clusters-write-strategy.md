## ADR-002: goal_clusters Write Strategy — Direct write_pool + INSERT OR IGNORE Throughout

### Context

Two write decisions must be made for the new `goal_clusters` table:

**Decision A — write path**

`goal_clusters` is a structural table that persists per-cycle goal context for
future briefing retrieval. It is analogous to `cycle_events` and
`cycle_review_index`. The question is whether writes should go through the
analytics drain (like `CoAccess` and `QueryLog`) or directly via
`write_pool_server()` (like `cycle_events` and `store_cycle_review`).

The analytics drain has a 500ms flush interval. Pattern #2125 documents that
the drain is unsuitable when callers may read back the written row in the same
request. Although step 8b does not read back the row immediately, the drain
adds unnecessary latency and introduces a dependency on the drain task's
liveness for a structural write. `cycle_review_index` uses `write_pool_server()`
directly; `goal_clusters` is the same class of data.

**Decision B — INSERT semantics**

The SCOPE Constraints section states "INSERT OR IGNORE throughout". The SCOPE
body prose mentioned INSERT OR REPLACE for `force=true` re-runs. This
contradiction was flagged as SR-04.

`INSERT OR REPLACE` on `goal_clusters` would overwrite the goal embedding and
entry_ids from a prior run, which could silently degrade a valid first-run
record if a `force=true` re-run of `context_cycle_review` happens after the
original session data has been partially cleaned. The additive-only invariant
that governs graph edges should apply here too: the goal cluster for a cycle
is immutable — it represents what was accessed under that goal in that cycle,
not a mutable summary.

### Decision

**Decision A**: `insert_goal_cluster` uses `write_pool_server()` directly in
the async handler context. It is NOT routed through the analytics drain. Pattern
is identical to `store_cycle_review` (cycle_review_index.rs).

**Decision B**: INSERT OR IGNORE throughout, including `force=true` re-runs.
`goal_clusters.feature_cycle UNIQUE` means the first write wins; re-runs for
the same feature_cycle are silent no-ops. This is safe because step 8b always
runs (Resolved Decision 6 from SCOPE.md) but edges are also idempotent, so the
combination is correct.

The SR-04 contradiction is resolved in favour of INSERT OR IGNORE. The
architectural invariant is: crt-046 writes are additive only. No path in this
feature overwrites existing records.

### Consequences

Easier:
- Consistent INSERT OR IGNORE semantics across all crt-046 writes (edges +
  goal_clusters).
- No risk of a force=true re-run corrupting a valid first-run goal cluster.
- Direct write_pool_server() is synchronous in the async handler — no drain
  dependency for a structural write.

Harder:
- A future feature that needs to update a goal cluster (e.g., to add
  retrospective outcome annotation) must use INSERT OR REPLACE explicitly
  and reason about the overwrite carefully. This is intentional scope
  enforcement.
- If a cycle's first `context_cycle_review` call fails mid-way (e.g., the
  goal_embedding was stored but entry_ids were incomplete), a subsequent
  re-run will be a no-op and the incomplete row will persist. This is
  acceptable given that step 8b errors are non-fatal and the additive-only
  guarantee takes priority.
