## ADR-003: Goal-Cluster Cosine Scan with Recency Cap

### Context

At `context_briefing` time, crt-046 must find past cycles whose goal embedding
is cosine-close to the current session's goal. SQLite has no native vector
index, so similarity must be computed in-process after fetching candidate rows.

The `goal_clusters` table is unbounded by design (Non-Goals: "Purging
`goal_clusters` rows is out of scope"). Over time the table will accumulate one
row per cycle. With thousands of cycles, an O(N × D) scan where D = 384 (or
768 for future model upgrades) would become a latency cliff that arrives
silently — SR-09 from the risk assessment.

Three options were considered:

**Option A — Full table scan**: simple, correct, but unbounded. O(N × D)
where N grows indefinitely. Unacceptable for a latency-sensitive path like
`context_briefing`.

**Option B — Recency cap (last K rows by created_at)**: fetch at most K=100
rows ordered by `created_at DESC`, compute cosine in-process on those rows.
O(100 × 384) ≈ 38,400 multiplications — well under 1ms. Requires a
`created_at DESC` index. Accepts the tradeoff that older clusters are
progressively excluded. This is desirable: recent cycles are more likely to
reflect the current project context than year-old cycles.

**Option C — Separate vector index for goal_clusters**: reuse the HNSW index
from `unimatrix-vector`. Would require a separate HNSW instance, lifecycle
management, and persistence — significant scope increase for a secondary
retrieval path.

An ANN (approximate nearest neighbor) approach was also considered but rejected
as over-engineering for K=100 candidates.

### Decision

Use Option B: fetch at most 100 rows `ORDER BY created_at DESC`, decode
embeddings, compute cosine similarity in-process, filter to >= `config.goal_cluster_similarity_threshold`,
return sorted by similarity descending.

A `created_at DESC` index is added to `goal_clusters` at schema creation time:

```sql
CREATE INDEX IF NOT EXISTS idx_goal_clusters_created_at
    ON goal_clusters(created_at DESC);
```

The recency cap (`RECENCY_CAP: u64 = 100`) is a named constant in
`services/behavioral_signals.rs`. The cosine threshold is **not** a constant —
it is read from `InferenceConfig.goal_cluster_similarity_threshold: f32`
(default 0.80) and passed to `store.query_goal_clusters_by_embedding(...)` at
call time. This allows operators to tune sensitivity without a code change,
consistent with how other scoring parameters are managed in `InferenceConfig`.

The cosine computation runs in the async handler context without
`spawn_blocking` because O(100 × 384) is CPU-trivial. Future escalation path:
if `RECENCY_CAP` is increased beyond ~10,000 rows, move to `spawn_blocking`
or a background-cache approach.

### Consequences

Easier:
- `context_briefing` latency is O(100 × D) regardless of total table size.
- No additional HNSW index lifecycle to manage.
- Recency bias is semantically correct: recent project context is more relevant.
- Named constant makes the cap visible and auditable.

Harder:
- Cycles older than the 100 most recent will never influence briefings, even
  if they are semantically close. Projects with very long histories lose old
  context. This is an acceptable tradeoff given the retention-out-of-scope
  decision.
- If the cap must be raised in the future, the latency profile changes. A
  comment in the code must document the escalation threshold.
