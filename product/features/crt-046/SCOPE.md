# crt-046 — Behavioral Signal Delivery

## Problem Statement

Unimatrix accumulates telemetry from every agent session, but that data does not yet close the learning loop. Entry co-access pairs recorded within a feature cycle capture genuine co-retrieval signal, yet they never produce graph edges — the Informs relation set is populated only by NLI inference and structural rules. Similarly, when an agent states a goal at cycle start the embedding is stored in `cycle_events.goal_embedding`, but nothing queries it to personalise the next briefing.

The result: every briefing treats each new agent as a first-time user with no history of what was helpful together, under what goal, in what phase. Group 6 of the ASS-040 roadmap closes this gap by wiring three pieces together: behavioural edge emission at cycle close, a goal-cluster table populated from that same event, and goal-conditioned blending in `context_briefing`.

Group 5 infrastructure (crt-043) is already live — `observations.phase`, `cycle_events.goal_embedding`, the encode/decode helpers, and the composite index are all shipped.

## Goals

1. At `context_cycle_review` call time, emit `Informs` graph edges for all `context_get` co-access pairs observed within the reviewed cycle, weighted by cycle outcome.
2. Persist a `goal_clusters` row at the same point: the goal embedding, phase, entry IDs accessed during the cycle, and outcome.
3. At `context_briefing` call time, query `goal_clusters` for past cycles whose goal embedding is cosine-close to the current session's goal, then blend the matching entry IDs into the semantic retrieval result set.
4. Guarantee cold-start correctness: zero goal history → pure semantic retrieval, no behaviour change.

## Non-Goals

- Removing or downweighting existing `Informs` edges when a cycle outcome is negative. Edges are additive only (roadmap spec is explicit).
- Phase-stratified briefing weighting (S6/S7 queries are enabled by the crt-043 composite index but belong to a later feature).
- Automatic edge emission on background tick; emission happens only inside the explicit `context_cycle_review` call.
- Modifying the co-access promotion tick or the `co_access` table itself.
- Extending the PPR expander (crt-045) to traverse new behavioral edges differently.
- Purging `goal_clusters` rows (retention policy is out of scope for this feature).
- Any UI, dashboard, or status surface for goal-cluster data.
- Changing `context_cycle_review` idempotency behaviour: a second call for the same cycle with `force=false` still returns the memoised result and still re-emits edges (additive, so safe).
- **`context_search` is not a call site for goal-conditioned blending.** `context_search` is a focused, agent-driven search mechanism with explicit parameters; silently altering its results with cluster-derived entries would break caller expectations. Blending is restricted to `context_briefing` and UDS injection only.

## Background Research

### Group 5 infrastructure confirmed live (schema v21)

- `cycle_events`: columns `id, cycle_id, seq, event_type, phase, outcome, next_phase, timestamp, goal, goal_embedding BLOB`. The `goal_embedding` column is populated by a fire-and-forget spawn in `handle_cycle_event` when `type=start` and goal text is non-empty. `encode_goal_embedding` / `decode_goal_embedding` are `pub` in `unimatrix-store::embedding`.
- `observations`: columns include `topic_signal TEXT` and `phase TEXT`. Composite index `idx_observations_topic_phase ON observations(topic_signal, phase)` is present.

### context_cycle_review call path

`context_cycle_review` in `mcp/tools.rs` (function `context_cycle_review`):

1. Loads attributed observations via three-path (UDS, MCP observation, session join).
2. Checks the memoisation cache in `cycle_review_index`.
3. Runs the full pipeline (hotspot detection, phase narrative, metrics).
4. At step 8a, stores a `CycleReviewRecord` via `store.store_cycle_review()`.
5. Step 11 fires the audit log.

The natural insertion point for behavioral edge emission and goal-cluster population is **between steps 8a and 11** — after the review record is stored, before the audit log fires. The cycle outcome is already resolved by step 8a (`report.outcome`).

### How context_get co-access pairs are tracked within a cycle

`context_get` does not write to `query_log` by itself. It records the retrieved entry ID via `session_registry.record_confirmed_entry(sid, id)` into `SessionState.confirmed_entries` (col-028), and fires a usage record. The `observations` table captures the PreToolUse hook observation with `tool = "context_get"`, including `session_id` and `ts_millis`.

To recover co-access pairs within a cycle at review time:
- `store.load_sessions_for_feature(feature_cycle)` returns all session IDs (already used by the retrospective pipeline).
- `store.load_observations_for_sessions(session_ids)` returns all observations for those sessions.
- Filter to `tool = "context_get"` rows, extract the entry ID from `input` (the PreToolUse input field contains the tool call JSON, which includes the `id` parameter). This is the same approach the retrospective pipeline uses to count `knowledge_served`.

An alternative is the `audit_log` table which records `context_get` events with `target_ids`. The audit log is already queried by some retrospective paths. Either source is viable; the observation input is more consistent with the existing pipeline.

### AnalyticsWrite::GraphEdge — existing mechanism

Graph edges are written via `store.enqueue_analytics(AnalyticsWrite::GraphEdge { source_id, target_id, relation_type, weight, created_by, source, bootstrap_only })`. The drain task validates `weight.is_finite()` and inserts with `INSERT OR IGNORE` into `graph_edges(source_id, target_id, relation_type)` — the unique constraint guarantees additive-only semantics automatically (a second emit for the same pair at the same relation_type is a no-op).

The `source` field is a free-text attribution string. Existing values include `"nli"`, `"c_cosine"`, `"co_access"`. For behavioral edges: `source = "behavioral"` per the roadmap spec.

`signal_origin` does not exist as a column in `graph_edges`. The roadmap description `signal_origin='behavioral'` maps to the `source` column.

### graph_edges UNIQUE constraint

`UNIQUE(source_id, target_id, relation_type)` — one edge per ordered pair per relation type. Behavioral edges use `relation_type = "Informs"`. `INSERT OR IGNORE` means re-emitting the same pair does not error and does not update weight. This is safe but means outcome weighting is applied only on first emission. If the pair already has an `Informs` edge from NLI, the behavioral write is silently skipped (additive-only guarantee).

### context_briefing call path

`IndexBriefingService::index()` delegates to `SearchService` with `RetrievalMode::Strict`. The query is derived by `derive_briefing_query(task, session_state, topic)`. Goal is available at step 2 via `session_state.current_goal`. The goal embedding for the current session is **not** pre-computed in session state — it would need to be retrieved from `cycle_events` (looked up by `session_state.feature`) or the embed service called inline.

`index()` currently returns up to `k=20` entries. Goal-cluster blending would inject additional IDs from past-cycle matches and deduplicate before returning.

### Cold-start behaviour

`cycle_events.goal_embedding` is `NULL` for cycles where no goal was supplied, and `NULL` for all pre-v21 rows. The `goal_clusters` table (new) will be empty on fresh deployments and for cycles that ran without a goal. Any retrieval query over `goal_clusters` with no matching rows must degrade silently to the existing semantic path — no error, no truncated result set.

### SessionState confirmed_entries

`confirmed_entries: HashSet<u64>` in `SessionState` tracks entries explicitly fetched via `context_get` in the current in-memory session. This is lost when a session is evicted. At `context_cycle_review` time the session may no longer be active; the durable record is in `observations` or `audit_log`.

## Proposed Approach

### Item 1 — Behavioral Informs edge emission in context_cycle_review

At the end of `context_cycle_review` (step 8b, after `store_cycle_review` stores the memoised record):

1. Collect all session IDs for `feature_cycle`.
2. Load observations filtered to `tool = "context_get"`.
3. Parse the entry ID from each observation's `input` JSON (field `"id"`). Skip unparseable rows.
4. Build the ordered co-access pair set: for each session, sort entry IDs by `ts_millis`, then enumerate all pairs (i, j) where i < j and both are within the same session window. Deduplicate across sessions by canonical pair `(min, max)`.
5. Determine weight: `report.outcome == Some("success") → 1.0`, any rework indicator `→ 0.5`, no outcome / unknown `→ 0.5`.
6. For each pair (A, B), enqueue **both directions**: `GraphEdge(A→B)` and `GraphEdge(B→A)` with `relation_type: "Informs"`, `source: "behavioral"`, `bootstrap_only: false`, `weight`. The `INSERT OR IGNORE` unique constraint handles de-duplication against existing NLI edges automatically.

This is fire-and-forget from the handler's perspective — use `enqueue_analytics` which is synchronous in-process enqueue; the drain task writes asynchronously.

### Item 2 — Goal-cluster table and population

New table `goal_clusters`:

```sql
CREATE TABLE IF NOT EXISTS goal_clusters (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    feature_cycle   TEXT    NOT NULL UNIQUE,  -- one row per cycle; INSERT OR IGNORE on re-run
    goal_embedding  BLOB    NOT NULL,   -- bincode Vec<f32>, same encoding as cycle_events
    phase           TEXT,               -- phase at cycle end; NULL if not provided
    entry_ids_json  TEXT    NOT NULL,   -- JSON array of u64 entry IDs accessed during cycle
    outcome         TEXT,               -- "success" | "rework" | NULL
    created_at      INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_goal_clusters_feature_cycle ON goal_clusters(feature_cycle);
```

Schema version bump: v21 → v22.

Population: at the same step 8b, after emitting Informs edges:
- Read `goal_embedding` from `cycle_events` start row via `store.get_cycle_start_goal_embedding(feature_cycle)` (new store method — similar to existing `get_cycle_start_goal`).
- If `goal_embedding` is NULL (cycle had no goal), skip population silently.
- Collect entry IDs: union of all entry IDs parsed from `context_get` observations across all sessions.
- Determine phase from the latest `cycle_events` row with a non-NULL phase (or NULL if none).
- Insert one `goal_clusters` row using `INSERT OR IGNORE` (first write wins; re-runs for the same `feature_cycle` are no-ops). This matches the resolved OQ-6 decision — step 8b always runs but is idempotent.

### Item 3 — Goal-conditioned briefing blending

In `IndexBriefingService::index()` or in the `context_briefing` handler, before calling `self.search.search()`:

1. If `session_state.current_goal` is present, retrieve the goal embedding from `cycle_events` for `session_state.feature` (via new store query `get_cycle_start_goal_embedding`). This reuses the already-stored embedding rather than re-embedding.
2. If the embedding is NULL or absent (cold start), skip all blending — proceed to pure semantic search unchanged.
3. If the embedding is present, query `goal_clusters` for rows whose `goal_embedding` has cosine similarity ≥ threshold (0.80 proposed) with the current goal embedding, ordered by similarity descending, limited to the top-K closest past cycles (K=5 proposed).
4. Collect entry IDs from matching rows. Filter to Active status only (one `store.get_by_ids()` call).
5. Blend: prepend these entry IDs at the top of the result list (or inject as high-score synthetic `IndexEntry` objects with `confidence = similarity_to_goal`). Deduplicate with semantic results. Truncate to `effective_k`.

The cosine computation runs in the async handler context, not `spawn_blocking`, since it is O(D*K) where D=384 or 768 and K≤5 — well under 1ms.

**Cold-start guarantee**: any NULL goal embedding, empty `goal_clusters`, or similarity below threshold falls through to the existing semantic path with zero changes in output.

## Acceptance Criteria

- AC-01: At `context_cycle_review` for a cycle with two or more `context_get` observations in the same session, `graph_edges` contains an `Informs` edge with `source='behavioral'` for each co-access pair after the call returns.
- AC-02: A second `context_cycle_review` call for the same cycle does not produce duplicate `graph_edges` rows (INSERT OR IGNORE idempotency).
- AC-03: A cycle outcome of "success" produces behavioral edges with `weight = 1.0`; any other outcome produces `weight = 0.5`.
- AC-04: A cycle with zero `context_get` observations produces zero behavioral edges (no no-op graph noise).
- AC-05: After `context_cycle_review` for a cycle that supplied a goal at `context_cycle start`, a `goal_clusters` row exists with non-NULL `goal_embedding`, correct `entry_ids_json`, and correct `outcome`.
- AC-06: A cycle that never called `context_cycle start` with a goal (NULL `goal_embedding` in `cycle_events`) does not produce a `goal_clusters` row.
- AC-07: `context_briefing` with a session whose current goal has a stored embedding and matching past-cycle clusters returns at least one cluster-derived entry in the top-K result set when a qualifying match exists (cosine ≥ threshold).
- AC-08: `context_briefing` with a session whose `goal_embedding` is NULL (no goal or pre-v21 cycle) returns results identical to the pre-crt-046 pure semantic path (cold-start guarantee).
- AC-09: `context_briefing` with an empty `goal_clusters` table (fresh deployment or no prior cycles with goals) returns results identical to the pre-crt-046 pure semantic path.
- AC-10: Cluster-derived entries that do not have `status = Active` are excluded from briefing results.
- AC-11: The schema migration from v21 to v22 creates the `goal_clusters` table and index on a live database without error.
- AC-12: All new store methods (`get_cycle_start_goal_embedding`, `insert_goal_cluster`, `query_goal_clusters_by_embedding`) are covered by unit tests exercising happy path and NULL/empty inputs.

## Constraints

- **Schema version**: v21 is current. crt-046 bumps to v22. Migration must use `CREATE TABLE IF NOT EXISTS` (idempotent) and `ALTER TABLE ... ADD COLUMN` patterns consistent with prior migrations.
- **graph_edges UNIQUE constraint**: `UNIQUE(source_id, target_id, relation_type)` means one `Informs` edge per ordered pair regardless of origin. A behavioral edge for a pair that NLI already covers is silently dropped. This is correct per roadmap spec ("additive only") but means behavioral weight is not applied when NLI already owns the edge.
- **INSERT OR IGNORE at analytics drain**: graph edge writes go through `enqueue_analytics` → drain. The drain uses `INSERT OR IGNORE`. Fire-and-forget from the handler.
- **Embed boundary is f32**: `decode_goal_embedding` returns `Vec<f32>`. Cosine similarity at briefing time must use f32 arithmetic (or upcast to f64 for the dot product — either is fine at this dimensionality).
- **No spawn_blocking for sqlx**: per ADR-001 (entries #2266, #2249), `sqlx` async queries must NOT be called from `spawn_blocking`. All new store queries must be called in async context (same pattern as `store_cycle_review`).
- **write_pool_server() for new table writes**: `goal_clusters` is a structural table (not observational telemetry), consistent with `cycle_events` and `cycle_review_index`. Use `write_pool` / `write_pool_server()` directly, not the analytics drain.
- **Max 500 lines per file**: existing `mcp/tools.rs` is already large. New logic for step 8b should be extracted into a service module (`services/behavioral_signals.rs` or similar) to stay under the limit.
- **context_cycle_review memoisation gate**: the memoisation check (step 2.5) returns early on a cache hit without running step 8b. For `force=false` cache hits, edges and goal_clusters are NOT re-emitted. This is acceptable — they were already emitted on the original call. For `force=true` calls, step 8b always runs.
- **Co-access pair extraction from observations**: the `input` field in `observations` is TEXT, containing the raw JSON tool call input. Parsing is best-effort; malformed rows are skipped. The `id` field is a JSON integer (not quoted) per MCP tool param conventions for `context_get`.
- **Pair set size cap**: to prevent pathological cases (cycle with 1000 context_get calls producing 499,500 pairs), cap the co-access pair set at a reasonable maximum (e.g. 200 pairs per cycle). Log a warning when cap is reached.

## Resolved Decisions

1. **Co-access pair source**: `observations` table — parse `input` JSON to extract entry ID. Consistent with retrospective pipeline; `audit_log` not used elsewhere in that path.

2. **UNIQUE constraint on Informs**: `INSERT OR IGNORE`. Roadmap spec is explicit: additive only. A behavioral write for a pair already owned by NLI is silently dropped.

3. **Cosine threshold for goal-cluster matching**: configurable via `InferenceConfig.goal_cluster_similarity_threshold: f32`, default 0.80. Not a hardcoded constant — must be read from config at call time.

4. **Goal-cluster blending strategy**: Option A — score-based interleaving. Run semantic search at k=20, fetch cluster entries (Active filter), compute `cluster_score = (confidence × w_conf_cluster) + (goal_cosine × w_goal_boost)`, merge into a single candidate list, sort by score descending, return top-k=20. Cluster entries compete on the same ranked list as semantic results; high-confidence, goal-similar entries naturally displace the weakest semantic results. Both weights are configurable in `InferenceConfig` (defaults: w_conf_cluster≈0.35, w_goal_boost≈0.25). Confidence is read from the Active entry record via the existing `get_by_ids()` call — no synthetic fabrication. Calibration deferred to post-ship eval, same as all other scoring weights. Supersedes the prior "Option B — remaining slots" decision which would have made the feature inert in any live deployment with ≥20 active entries.

5. **Goal embedding at briefing time**: Read from `cycle_events` DB per call. No `SessionState` cache — avoids embedding in hot session memory, keeps SessionState lean.

6. **Memoisation gate for step 8b**: Step 8b always runs, independent of the `force=false` memoisation hit. Edges use `INSERT OR IGNORE` (idempotent). `goal_clusters` insert uses `INSERT OR IGNORE ON CONFLICT(feature_cycle)` — first write wins, re-runs are no-ops.

7. **Informs edge direction**: Bidirectional — emit both `(A→B)` and `(B→A)`. Consistent with crt-044 ADR: outgoing-only traversal convention requires bidirectional write side.

## Tracking

https://github.com/dug-21/unimatrix/issues/511
