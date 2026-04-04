# crt-046 — Behavioral Signal Delivery: Architecture

## System Overview

crt-046 closes the learning loop between agent session telemetry and future
retrieval quality. Group 5 (crt-043) shipped the write infrastructure —
`cycle_events.goal_embedding`, `observations.phase`, and the encode/decode
helpers. Group 6 (this feature) reads that infrastructure at two points:

1. At `context_cycle_review` call time: mine co-access pairs from observations,
   emit bidirectional `Informs` graph edges, and persist a `goal_clusters` row
   so past-cycle context is queryable.
2. At `context_briefing` call time: retrieve the current goal embedding, find
   cosine-close past cycles from `goal_clusters`, and blend their accessed
   entry IDs into the semantic result set.

The result: briefings become goal-conditioned after the first completed cycle.
Cold-start (no prior cycles, no goal) falls through to pure semantic retrieval
unchanged.

## Component Breakdown

### Component 1 — `services/behavioral_signals.rs` (new)

Owns all behavioral signal logic. Extracted from `mcp/tools.rs` to keep that
file under the 500-line cap and to give the logic a coherent single-
responsibility home (ADR-001).

Responsibilities:
- `collect_coaccess_entry_ids(observations)` — filter observations to
  `tool = "context_get"`, parse entry ID from `input` JSON, return
  `(Vec<u64>, usize parse_failures)`.
- `build_coaccess_pairs(session_observations)` — sort IDs by `ts_millis`
  within each session, enumerate all (i, j) pairs where i < j, deduplicate by
  canonical `(min, max)`, enforce 200-pair cap.
- `outcome_to_weight(outcome: Option<&str>) -> f32` — `"success"` → 1.0,
  anything else → 0.5.
- `emit_behavioral_edges(store, pairs, weight)` — writes both directed
  edges for each pair directly via the module-private `write_graph_edge`
  helper, which executes `INSERT OR IGNORE INTO graph_edges` on
  `store.write_pool_server()` with `relation_type = "Informs"`,
  `source = "behavioral"`, `bootstrap_only = false`. Does NOT use
  `enqueue_analytics` — the analytics drain is fire-and-forget and cannot
  satisfy the `write_graph_edge` return contract (ADR-006, Unimatrix #4124).
  Returns `(edges_enqueued: usize, pairs_skipped_on_conflict: usize)`;
  `edges_enqueued` increments only on `Ok(true)` (pattern #4041).
- `populate_goal_cluster(store, feature_cycle, goal_embedding, entry_ids,
  phase, outcome)` — call `store.insert_goal_cluster(...)`. Returns
  `Ok(inserted: bool)` where `false` means INSERT OR IGNORE conflict (already
  exists for this feature_cycle).
- `blend_cluster_entries(semantic: Vec<IndexEntry>, cluster_entries_with_scores: Vec<(IndexEntry, f32)>, k: usize) -> Vec<IndexEntry>` — merge semantic results and pre-scored cluster entries into one candidate list, sort by score descending (semantic entries retain their existing score field; cluster entries use `cluster_score`), deduplicate by entry ID (first occurrence wins), return top-k. Does not perform store fetches — the caller supplies `cluster_entries_with_scores` already fetched and scored.

### Component 2 — `unimatrix-store` schema v22 + three new store methods

**Schema change**: new `goal_clusters` table (v21 → v22 migration).

```sql
CREATE TABLE IF NOT EXISTS goal_clusters (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    feature_cycle   TEXT    NOT NULL UNIQUE,
    goal_embedding  BLOB    NOT NULL,
    phase           TEXT,
    entry_ids_json  TEXT    NOT NULL,
    outcome         TEXT,
    created_at      INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_goal_clusters_created_at
    ON goal_clusters(created_at DESC);
```

The `created_at DESC` index supports the recency-capped query
(`ORDER BY created_at DESC LIMIT 100`) used at briefing time (SR-09 mitigation,
ADR-003).

**Three new store methods** on `SqlxStore`:

1. `get_cycle_start_goal_embedding(cycle_id: &str) -> Result<Option<Vec<f32>>>`
   — mirrors `get_cycle_start_goal`. Queries `cycle_events` WHERE
   `event_type = 'cycle_start'` AND `goal_embedding IS NOT NULL`, ORDER BY
   timestamp ASC LIMIT 1. Decodes BLOB via `decode_goal_embedding`. Returns
   `Ok(None)` on no row or NULL BLOB. Uses `read_pool()`.

2. `insert_goal_cluster(feature_cycle, goal_embedding, phase, entry_ids_json,
   outcome, created_at) -> Result<bool>` — `INSERT OR IGNORE` with UNIQUE
   constraint on `feature_cycle`. Returns `true` on new insert (rows_affected
   == 1), `false` on conflict. Uses `write_pool_server()` directly in async
   context (not via analytics drain — goal_clusters is structural; ADR-002).

3. `query_goal_clusters_by_embedding(embedding: &[f32], similarity_threshold:
   f32, limit: u64) -> Result<Vec<GoalClusterRow>>` — reads at most
   `limit` rows (recency cap, default 100) from `goal_clusters ORDER BY
   created_at DESC`, decodes each BLOB, computes cosine similarity in-process,
   filters to `>= similarity_threshold`, returns matching rows sorted by
   similarity descending. Uses `read_pool()`.

`GoalClusterRow` is a new struct:

```rust
pub struct GoalClusterRow {
    pub id: i64,
    pub feature_cycle: String,
    pub goal_embedding: Vec<f32>,
    pub phase: Option<String>,
    pub entry_ids_json: String,
    pub outcome: Option<String>,
    pub created_at: i64,
    pub similarity: f32,  // computed at query time, not stored
}
```

### Component 3 — `context_cycle_review` step 8b insertion point

**Location**: `mcp/tools.rs`, inside the full-pipeline branch, immediately
after step 8a (`store_cycle_review`) and before step 11 (audit).

**Memoisation gate behaviour**: the `force=false` cache-hit early return path
(step 2.5) returns before step 8b. The full pipeline path (cache miss OR
`force=true`) always runs step 8b. This is intentional: step 8b uses INSERT OR
IGNORE for both edges and goal_clusters, so re-runs are safe no-ops.

**Step 8b sequence**:

1. Load session IDs: `store.load_sessions_for_feature(feature_cycle)`.
2. Load observations: `store.load_observations_for_sessions(session_ids)`.
3. Call `behavioral_signals::collect_coaccess_entry_ids(observations)` →
   `(entry_ids_by_session, parse_failures)`.
4. Call `behavioral_signals::build_coaccess_pairs(entry_ids_by_session)` →
   `pairs` (Vec of `(u64, u64)`).
5. If `pairs.is_empty()` → log `debug!` and skip edge emission (AC-04).
6. Determine weight via `behavioral_signals::outcome_to_weight(report.outcome)`.
7. Call `behavioral_signals::emit_behavioral_edges(store, pairs, weight)` →
   `(edges_enqueued, skipped)`.
8. Load goal embedding: `store.get_cycle_start_goal_embedding(feature_cycle)`.
9. If `Some(embedding)` → collect union of all entry IDs from step 3, determine
   phase from cycle_events, call
   `behavioral_signals::populate_goal_cluster(...)`.
10. Log `parse_failures` at `warn!` level (SR-01 observability). Return `parse_failures` as `parse_failure_count: u32` in the `context_cycle_review` MCP response JSON as a top-level field, outside the serialized `CycleReviewRecord`. No `SUMMARY_SCHEMA_VERSION` bump required.

All errors in step 8b are logged and swallowed — step 8b never causes the
`context_cycle_review` response to fail.

### Component 4 — `context_briefing` goal-conditioned blending

**Location**: `mcp/tools.rs` `context_briefing` handler (or factored into
`services/index_briefing.rs`). Runs before `briefing.index()` is called but
after session state is resolved.

**Blending sequence** (only when `session_state.feature` is Some):

1. Call `store.get_cycle_start_goal_embedding(feature)` → `embedding_opt`.
2. If `embedding_opt.is_none()` → cold-start path: call `briefing.index()`
   unchanged (AC-08, AC-09).
3. If `Some(embedding)` → call
   `store.query_goal_clusters_by_embedding(embedding, config.goal_cluster_similarity_threshold, 100)` →
   `matching_clusters`.
4. If `matching_clusters.is_empty()` → cold-start path.
5. Collect `cluster_entry_ids`: union of `entry_ids_json` from each matching
   cluster, parsed as `Vec<u64>`. Each `GoalClusterRow` in `matching_clusters`
   carries its `similarity` field (cosine to current goal embedding — already
   computed during step 3).
6. Call `store.get_by_ids(cluster_entry_ids)` → Active `EntryRecord` objects.
   Compute per-cluster-entry score:
   `cluster_score = (entry_record.confidence × config.w_conf_cluster) + (row.similarity × config.w_goal_boost)`
   where `config.w_conf_cluster` (default 0.35) and `config.w_goal_boost`
   (default 0.25) are `InferenceConfig` fields.

   **NAMING COLLISION — critical**: `EntryRecord.confidence` (Wilson-score
   composite, from `store.get_by_ids()`) and `IndexEntry.confidence` (raw
   HNSW cosine similarity, from `briefing.index()`) are different values with
   the same field name in different structs. The formula uses
   `EntryRecord.confidence`. Never substitute `IndexEntry.confidence` — both
   compile; the wrong one silently uses cosine twice. See ADR-005.

   **Score scale — resolved**: `IndexBriefingService::index()` returns
   `IndexEntry.confidence = se.similarity` (raw cosine, [0,1]). `cluster_score`
   tops out at ~0.60 with defaults. Scales are compatible — no normalization
   required. Cluster entries displace semantic results with cosine < ~0.60
   (bottom half of result set). See ADR-005.

   Produce `Vec<(IndexEntry, f32)>` (entry, cluster_score).
7. Call `briefing.index(params, audit_ctx, caller_id)` → `semantic_results`
   (k=20 semantic candidates).
8. Call `behavioral_signals::blend_cluster_entries(semantic_results,
   cluster_entries_with_scores, effective_k)` → merge both lists, sort by
   score descending, deduplicate by entry ID, return top-k=20 final result.

This is Option A (score-based interleaving) — see ADR-005. R-13 ("zero
remaining slots — accepted") is resolved: the blending path is not inert
because cluster entries compete on the same scored list as semantic results
rather than filling leftover slots.

The NULL short-circuit (step 2) fires before any DB query for cluster data
(SR-07 mitigation, ADR-004). The DB call in step 1 (`get_cycle_start_goal_embedding`)
is unavoidable per Resolved Decision 5 (no SessionState cache), but its result
gates all subsequent cluster work. When `session_state.feature` is absent,
step 1 is skipped entirely.

## Component Interactions

```
context_cycle_review handler (mcp/tools.rs)
  │
  ├── step 8a: store.store_cycle_review(record)         [existing]
  │
  └── step 8b: behavioral_signals::run_step_8b(         [NEW]
        store, feature_cycle, observations, report.outcome
      )
        ├── collect_coaccess_entry_ids(observations)
        ├── build_coaccess_pairs(by_session)
        ├── emit_behavioral_edges(store, pairs, weight)
        │     └── write_graph_edge(store, ...) × 2N   [direct write_pool_server(); NOT analytics drain — ADR-006]
        └── populate_goal_cluster(store, feature_cycle, ...)
              └── store.insert_goal_cluster(...)

context_briefing handler (mcp/tools.rs)
  │
  ├── [existing] derive_briefing_query / session state
  │
  ├── [NEW] store.get_cycle_start_goal_embedding(feature)
  │         → None → skip cluster blending
  │         → Some(emb) → store.query_goal_clusters_by_embedding(emb, 0.80, 100)
  │
  ├── [existing] briefing.index(params)   ← semantic results
  │
  ├── [NEW] store.get_by_ids(cluster_entry_ids)    ← Active entry records
  │         → score each: confidence × w_conf_cluster + goal_cosine × w_goal_boost
  │
  └── [NEW] behavioral_signals::blend_cluster_entries(
              semantic_results, cluster_entries_with_scores, k=20
            )
              → merge + sort by score + dedup by ID + top-k
              → final Vec<IndexEntry>
```

## Technology Decisions

- **Separate module** `services/behavioral_signals.rs`: keeps `mcp/tools.rs`
  under 500 lines; single-responsibility; testable without a full handler stack.
  See ADR-001.
- **Direct `write_pool_server()` for behavioral graph edges**: `emit_behavioral_edges`
  writes `INSERT OR IGNORE INTO graph_edges` via a `write_graph_edge` helper that
  returns `Result<bool>` (`true` = new row, `false` = UNIQUE conflict). The
  analytics drain (`enqueue_analytics`) cannot satisfy the `write_graph_edge` return
  contract (pattern #4041) because it is fire-and-forget with no `rows_affected()`
  feedback, and it sheds events under queue pressure regardless of `bootstrap_only`.
  `edges_enqueued` increments on `Ok(true)` only. See ADR-006.
  **Note**: SPEC FR-06/FR-07 and the IMPLEMENTATION-BRIEF Resolved Decisions table
  originally specified `enqueue_analytics`; ADR-006 supersedes those entries.
- **Direct `write_pool_server()` for `goal_clusters`**: structural table (not
  observational telemetry). Pattern consistent with `cycle_events` and
  `cycle_review_index`. Drain is unsuitable — immediate-read visibility is not
  required, but the drain's 500ms DRAIN_FLUSH_INTERVAL adds uncertainty for
  query-side reads at briefing time. Direct write removes this dependency.
  See ADR-002.
- **In-process cosine scan with recency cap**: SQLite has no vector index;
  cosine must be computed in-process. O(100 × D) at D=384 is ~0.1ms — well
  under latency budget. Cap of 100 rows prevents unbounded growth from
  becoming a latency cliff. See ADR-003.
- **INSERT OR IGNORE for `goal_clusters`**: first-write-wins. Re-runs of
  `context_cycle_review` (including `force=true`) are no-ops for the cluster
  row. This resolves the SR-04 contradiction: the SCOPE Constraints section
  ("INSERT OR IGNORE throughout") takes precedence over the SCOPE body prose
  that mentioned INSERT OR REPLACE. The additive-only invariant applies
  uniformly. See ADR-002.
- **Parse-failure counter** (SR-01, FR-03): the count of unparseable observation
  rows is logged at `warn!` level per step 8b invocation AND returned as
  `parse_failure_count: u32` in the `context_cycle_review` MCP response JSON as
  a top-level field, outside the serialized `CycleReviewRecord`. No
  `SUMMARY_SCHEMA_VERSION` bump required — `CycleReviewRecord` is not extended.

## Integration Points

### Existing code consumed

| Component | Usage in crt-046 |
|-----------|-----------------|
| `store.load_sessions_for_feature` | step 8b session enumeration |
| `store.load_observations_for_sessions` | step 8b observation load |
| `store.write_pool_server()` | behavioral edge emission via `write_graph_edge` (direct write — NOT analytics drain; ADR-006) |
| `store.get_cycle_start_goal` | pattern for new `get_cycle_start_goal_embedding` |
| `encode_goal_embedding` / `decode_goal_embedding` | embedding BLOB encode/decode (crt-043 ADR-001) |
| `IndexBriefingService::index()` | semantic search before blending |
| `derive_briefing_query` | query derivation unchanged |
| `session_state.current_goal` / `.feature` | goal and feature extraction at briefing time |

### New interfaces introduced

| Interface | Location | Signature |
|-----------|----------|-----------|
| `get_cycle_start_goal_embedding` | `unimatrix-store/src/db.rs` | `async fn get_cycle_start_goal_embedding(&self, cycle_id: &str) -> Result<Option<Vec<f32>>>` |
| `insert_goal_cluster` | `unimatrix-store/src/goal_clusters.rs` (new file) | `async fn insert_goal_cluster(&self, feature_cycle: &str, goal_embedding: Vec<f32>, phase: Option<&str>, entry_ids_json: &str, outcome: Option<&str>, created_at: i64) -> Result<bool>` |
| `query_goal_clusters_by_embedding` | `unimatrix-store/src/goal_clusters.rs` | `async fn query_goal_clusters_by_embedding(&self, embedding: &[f32], threshold: f32, recency_limit: u64) -> Result<Vec<GoalClusterRow>>` |
| `GoalClusterRow` | `unimatrix-store/src/goal_clusters.rs` | struct with fields: id, feature_cycle, goal_embedding, phase, entry_ids_json, outcome, created_at, similarity |
| `behavioral_signals::collect_coaccess_entry_ids` | `unimatrix-server/src/services/behavioral_signals.rs` | `fn collect_coaccess_entry_ids(obs: &[ObservationRow]) -> (HashMap<String, Vec<(u64, i64)>>, usize)` |
| `behavioral_signals::build_coaccess_pairs` | same | `fn build_coaccess_pairs(by_session: HashMap<String, Vec<(u64, i64)>>) -> (Vec<(u64, u64)>, bool)` — returns `(pairs, cap_hit)` |
| `behavioral_signals::outcome_to_weight` | same | `fn outcome_to_weight(outcome: Option<&str>) -> f32` |
| `behavioral_signals::emit_behavioral_edges` | same | `async fn emit_behavioral_edges(store: &SqlxStore, pairs: &[(u64, u64)], weight: f32) -> (usize, usize)` |
| `behavioral_signals::populate_goal_cluster` | same | `async fn populate_goal_cluster(store: &SqlxStore, feature_cycle: &str, goal_embedding: Vec<f32>, entry_ids: &[u64], phase: Option<&str>, outcome: Option<&str>) -> Result<bool>` |
| `behavioral_signals::blend_cluster_entries` | same | `fn blend_cluster_entries(semantic: Vec<IndexEntry>, cluster_entries_with_scores: Vec<(IndexEntry, f32)>, k: usize) -> Vec<IndexEntry>` |

## Integration Surface

| Integration Point | Type/Signature | Source |
|-------------------|---------------|--------|
| `CURRENT_SCHEMA_VERSION` | `u64 = 22` | `unimatrix-store/src/migration.rs` |
| `goal_clusters` table DDL | SQL in migration.rs + db.rs (must be byte-identical) | new, crt-046 |
| `idx_goal_clusters_created_at` | `ON goal_clusters(created_at DESC)` | new, crt-046 |
| `graph_edges.source` for behavioral writes | `String = "behavioral"` | constant in `behavioral_signals.rs`; written via `write_graph_edge` helper (NOT `enqueue_analytics` — ADR-006) |
| `graph_edges.relation_type` for behavioral writes | `String = "Informs"` | constant in `behavioral_signals.rs` |
| `graph_edges.bootstrap_only` for behavioral writes | `bool = false` (integer 0) | constant in `behavioral_signals.rs` |
| `write_graph_edge` return contract | `Ok(true)` = new row; `Ok(false)` = UNIQUE conflict; `Err(_)` = SQL failure | pattern #4041, ADR-006 (Unimatrix #4124) |
| Cosine threshold | `InferenceConfig.goal_cluster_similarity_threshold: f32` (default 0.80) | `InferenceConfig` — not a constant; passed to `query_goal_clusters_by_embedding` |
| Cluster confidence weight | `InferenceConfig.w_conf_cluster: f32` (default 0.35) | `InferenceConfig` — multiplied by `entry.confidence` in cluster_score formula |
| Goal boost weight | `InferenceConfig.w_goal_boost: f32` (default 0.25) | `InferenceConfig` — multiplied by `goal_cosine` (row.similarity) in cluster_score formula |
| Recency cap | `u64 = 100` rows | constant in `behavioral_signals.rs` |
| Pair cap | `usize = 200` | constant in `behavioral_signals.rs` |
| Outcome weights | success → 1.0f32; other → 0.5f32 | `behavioral_signals::outcome_to_weight` |
| `decode_goal_embedding` | `fn(bytes: &[u8]) -> Result<Vec<f32>, DecodeError>` | `unimatrix-store/src/embedding.rs` |
| `encode_goal_embedding` | `fn(vec: Vec<f32>) -> Result<Vec<u8>, EncodeError>` | `unimatrix-store/src/embedding.rs` |
| `ObservationRow.input` | `Option<String>` containing raw MCP tool-call JSON | `unimatrix-store/src/observations.rs` |
| `context_get` input JSON `id` field | integer (not quoted) in JSON object | MCP tool convention |
| `session_state.feature` | `Option<String>` | `unimatrix-server/src/infra/session.rs` |

## Schema Migration v21 → v22

Migration block added to `migration.rs` under `if current_version < 22`:

```sql
CREATE TABLE IF NOT EXISTS goal_clusters (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    feature_cycle   TEXT    NOT NULL UNIQUE,
    goal_embedding  BLOB    NOT NULL,
    phase           TEXT,
    entry_ids_json  TEXT    NOT NULL,
    outcome         TEXT,
    created_at      INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_goal_clusters_created_at
    ON goal_clusters(created_at DESC);
UPDATE counters SET value = 22 WHERE name = 'schema_version';
```

The same DDL must appear byte-identically in `db.rs::create_tables_if_needed()`.

### Migration cascade checklist (from pattern #3894)

The delivery agent MUST address all of these before closing Gate 3a:

1. `migration.rs`: add `if current_version < 22` block.
2. `db.rs`: add matching DDL to `create_tables_if_needed()`.
3. `db.rs`: bump hardcoded schema_version INSERT integer to 22.
4. `db.rs`: rename `test_schema_version_initialized_to_21_on_fresh_db` → 22.
5. `sqlite_parity.rs`: add `test_create_tables_goal_clusters_exists` and
   `test_create_tables_goal_clusters_schema` (exact column count: 7).
6. `sqlite_parity.rs`: update `test_schema_version_is_N` to 22 and
   `test_schema_column_count` for any table with column changes.
7. `server.rs`: update both `assert_eq!(version, 21)` sites to 22.
8. Previous migration test: rename current `test_current_schema_version_is_21`
   to `test_current_schema_version_is_at_least_21` with `>= 21` predicate.
9. All migration test files: grep for column-count assertions referencing the
   old total — update if affected.

Gate check: `grep -r 'schema_version.*== 21' crates/` must return zero matches
before marking migration complete.

## write_graph_edge Return Contract

Implementation agents must lead their pseudocode with this table (pattern #4041):

| `write_graph_edge` return | Meaning | Counter action |
|---------------------------|---------|----------------|
| `Ok(true)` | New row inserted | Increment `edges_enqueued` |
| `Ok(false)` | UNIQUE conflict, silently ignored | Do not increment; not an error |
| `Err(_)` | SQL infrastructure failure | Log warn!, do not increment, continue |

## Error Handling

All errors in step 8b (behavioral edge emission, goal cluster population) are
non-fatal. Log at `warn!` level and continue. `context_cycle_review` must never
return an error solely due to step 8b failure. Same pattern as step 8a.

Goal embedding DB read failure at briefing time falls through to pure semantic
retrieval (cold-start path) — no error propagated to caller.

## Open Questions

- OQ-1: Should `blend_cluster_entries` use `store.get_entries_by_ids()` (bulk
  fetch) or a JOIN query? Recommendation is bulk fetch to reuse an existing
  store method. Delivery agent to verify the method name and signature.
- OQ-2: When `session_state.feature` is absent at briefing time but
  `session_state.current_goal` is Some, is inline embedding (calling
  embed_service) in scope? Current architecture says no — cold-start path
  activates. Spec agent should confirm this is the correct behaviour for
  sessions without feature attribution.
- OQ-3: RESOLVED by ADR-006 (Unimatrix #4124). Behavioral graph edge writes
  use `write_pool_server()` directly via the `write_graph_edge` helper.
  `enqueue_analytics(AnalyticsWrite::GraphEdge)` is NOT used for step 8b
  emission and is NOT consumed by this feature for that purpose. The structural
  incompatibility is definitive: `enqueue_analytics` is fire-and-forget
  (returns `()`), so it cannot supply the `rows_affected()` feedback required
  by the `write_graph_edge` return contract (pattern #4041). See
  §Technology Decisions and ADR-006 for full rationale.
