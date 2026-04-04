## ADR-004: Briefing Goal Embedding NULL Short-Circuit Before Any Cluster DB Query

### Context

At `context_briefing` time, the blending path requires two sequential DB reads:

1. `store.get_cycle_start_goal_embedding(feature)` — reads `cycle_events` to
   retrieve the current session's goal embedding.
2. `store.query_goal_clusters_by_embedding(embedding, ...)` — scans
   `goal_clusters` with in-process cosine computation.

SR-07 flags a risk: the NULL fast-path must fire before the cluster query, not
after. If the implementation calls `get_cycle_start_goal_embedding` and then
proceeds to `query_goal_clusters_by_embedding` regardless of the result, every
briefing for a goal-less session incurs an unnecessary `goal_clusters` scan.

Additionally, `session_state.feature` may be absent (no feature attribution on
this session). In that case step 1 cannot be called meaningfully (there is no
`cycle_id` to look up). This edge case must also be handled explicitly.

Three guard points were considered:

**Guard A** — after `get_cycle_start_goal_embedding`: if `Ok(None)`, skip
cluster query. This is the minimum required. Still calls
`get_cycle_start_goal_embedding` even when feature is absent.

**Guard B** — before everything: if `session_state.feature.is_none()`, skip
both DB calls entirely. Then if feature is Some, call
`get_cycle_start_goal_embedding`. If result is None, skip cluster query.

**Guard C** — inline in `IndexBriefingService::index()`: pass goal embedding
as a parameter; the service handles blending internally. This is cleaner
architecturally but requires adding `goal_embedding: Option<Vec<f32>>` to
`IndexBriefingParams`, which changes the service interface and creates an
expectation that the caller always resolves the embedding.

### Decision

Use Guard B: two-level short-circuit in the `context_briefing` handler before
calling `briefing.index()`:

```
if session_state.feature.is_none() → skip all blending, call briefing.index()
if store.get_cycle_start_goal_embedding(feature).await? is None → skip cluster query
if query_goal_clusters.is_empty() → skip blending
```

Each guard fires before the subsequent DB call. `IndexBriefingParams` is not
changed — the blending is handled in the handler before calling `index()`, not
inside the service. This keeps `IndexBriefingService` focused on semantic
retrieval and avoids adding a store dependency to that struct.

### Consequences

Easier:
- Sessions without feature attribution (common for standalone MCP calls) pay
  zero overhead.
- Sessions with feature attribution but no stored goal embedding pay only the
  cost of one indexed `cycle_events` point lookup (fast, indexed on
  `idx_cycle_events_cycle_id`).
- `IndexBriefingService` interface unchanged — no new constructor parameters
  required.

Harder:
- The blending logic is split across the handler and `behavioral_signals.rs`
  rather than being fully encapsulated in `IndexBriefingService`. This is an
  intentional scope decision — full encapsulation would require giving
  `IndexBriefingService` a store reference, which it does not currently hold.
