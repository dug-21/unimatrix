## ADR-005: Score-Based Cluster Interleaving for Goal-Conditioned Briefing Blending

### Context

FR-21 specifies how cluster-derived entries from `goal_clusters` are merged with
semantic retrieval results in `context_briefing`. Two strategies were considered:

**Option B — remaining slots splice** (prior design): run semantic search at
k=20, then fill any slots not occupied by semantic results with cluster entries.
Rationale: simple, zero re-scoring required.

Problem: `context_briefing` operates on a live deployment with hundreds of
active entries. `IndexBriefingService::index()` almost always fills all k=20
slots with semantic results. With a full result set there are zero remaining
slots, and the cluster blending path becomes a no-op. The feature would be
inert in every production deployment.

This was flagged as risk R-13 ("zero remaining slots") in the SCOPE.md
Constraints section. The prior resolution ("accepted") was wrong — inertness
in production is not acceptable for a goal-conditioning feature that exists
specifically to personalise briefings after the first completed cycle.

**Option A — score-based interleaving** (this decision): mirrors the PPR
expander pattern from crt-045. Expand the candidate pool first, score all
candidates uniformly, return the top-k from the merged pool. Cluster entries
must compete on the same ranked list as semantic results rather than relying on
leftover capacity.

The scoring formula for cluster entries is:

```
cluster_score = (entry.confidence × w_conf_cluster) + (goal_cosine × w_goal_boost)
```

- `entry.confidence`: real confidence value from the Active entry record
  returned by `store.get_by_ids()` — no synthetic fabrication.
- `goal_cosine`: cosine similarity between the cluster row's `goal_embedding`
  and the current session's goal embedding, already computed during the cluster
  matching step (`GoalClusterRow.similarity`).
- `w_conf_cluster` and `w_goal_boost`: configurable `InferenceConfig` fields
  (defaults 0.35 and 0.25 respectively). Calibration deferred to post-ship
  eval, consistent with all other scoring weights in the system.

Semantic results carry their existing score from `IndexBriefingService::index()`.
After merging both lists, entries are sorted by score descending, deduplicated
by entry ID (first occurrence wins), and truncated to top-k=20.

The `blend_cluster_entries` function signature becomes:

```rust
fn blend_cluster_entries(
    semantic: Vec<IndexEntry>,
    cluster_entries_with_scores: Vec<(IndexEntry, f32)>,
    k: usize,
) -> Vec<IndexEntry>
```

The store fetch (`get_by_ids`) and scoring are performed by the caller (in
the `context_briefing` handler) before passing to `blend_cluster_entries`.
This keeps `blend_cluster_entries` a pure function with no async dependencies,
making it directly unit-testable.

Supersedes the "Option B — remaining slots" approach described in the original
SCOPE.md Item 3 and the prior architecture draft.

### Decision

Use Option A: score-based interleaving. Fetch Active entry records for all
cluster-derived IDs via `store.get_by_ids()`, compute `cluster_score` per
entry, merge with semantic results, sort by score descending, dedup by ID,
return top-k=20.

Both `w_conf_cluster` and `w_goal_boost` are `InferenceConfig` fields, not
constants in `behavioral_signals.rs`, so they can be tuned without code
changes. Default values (0.35, 0.25) are chosen so that a highly-confident
cluster entry (confidence=0.9) with a strong goal match (similarity=0.9) would
score approximately 0.54, which is competitive with mid-tier semantic results.

### Consequences

Easier:
- The blending path is never inert regardless of how many active entries exist.
  A qualifying cluster entry will displace the weakest semantic result when its
  `cluster_score` exceeds that result's score.
- `blend_cluster_entries` is a pure synchronous function — straightforward to
  unit test with mocked inputs.
- Scoring weights are observable and tunable at runtime via `InferenceConfig`.

Harder:
- The caller must perform an additional `store.get_by_ids()` fetch in the
  `context_briefing` handler path whenever cluster matches exist. This is
  bounded by the number of distinct cluster entry IDs (itself bounded by
  `goal_clusters` recency cap × cycle entry count) and should remain fast in
  practice.

**Score scale — RESOLVED**: `IndexBriefingService::index()` maps
`se.similarity` (raw HNSW cosine) to `IndexEntry.confidence`, so semantic
results carry raw cosine scores in [0, 1]. `cluster_score` with defaults
(0.35 + 0.25) tops out at ~0.60. The scales are compatible for ranking: no
normalization is required. Cluster entries compete with and can displace
semantic results scoring below ~0.60 — approximately the bottom half of the
result set. The top half (cosine ≥ 0.60, genuinely semantically relevant)
is preserved. This is the correct conservative posture for a first ship;
tuning via `w_conf_cluster` / `w_goal_boost` is post-ship work.

**Naming collision — critical implementation trap**: `IndexEntry.confidence`
(raw HNSW cosine similarity, returned by `briefing.index()`) and
`EntryRecord.confidence` (Wilson-score composite, returned by
`store.get_by_ids()`) share the same field name in different structs.
The `cluster_score` formula requires `EntryRecord.confidence`. Never
substitute `IndexEntry.confidence` — both compile, neither panics, the
wrong one silently uses cosine twice and produces incorrect weights:

```rust
// WRONG — IndexEntry.confidence = raw cosine similarity
let cluster_score = (index_entry.confidence * w_conf_cluster) + (goal_cosine * w_goal_boost);

// CORRECT — EntryRecord.confidence = Wilson-score composite
let cluster_score = (entry_record.confidence * w_conf_cluster) + (goal_cosine * w_goal_boost);
```

The scoring step (step 6 of the Component 4 blending sequence) uses the
`EntryRecord` objects returned by `store.get_by_ids()`, not the
`IndexEntry` objects returned by `briefing.index()`. These are distinct
variables in the caller scope and must not be confused.
