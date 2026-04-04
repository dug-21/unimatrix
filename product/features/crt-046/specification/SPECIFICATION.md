# SPECIFICATION: crt-046 — Behavioral Signal Delivery

## Objective

Unimatrix accumulates per-session telemetry but that data does not yet close the learning loop. This feature wires three components together: behavioral `Informs` edge emission at cycle-review time (derived from co-access pairs within the cycle), a `goal_clusters` table populated from the same event, and goal-conditioned blending in `context_briefing` that injects cluster-matching entry IDs into the retrieval result. Cold-start correctness is guaranteed: zero goal history degrades silently to the existing pure-semantic path.

---

## Functional Requirements

### Component 1 — Behavioral Edge Emission

**FR-01**: At `context_cycle_review` call time, the system shall collect all session IDs associated with the reviewed `feature_cycle` using the existing `store.load_sessions_for_feature` path.

**FR-02**: For each session, the system shall load all observations and filter to rows where `tool = "context_get"`. Each qualifying row's `input` field (JSON TEXT) shall be parsed to extract the integer `id` parameter. Rows where parsing fails (malformed JSON, missing `id` field, non-integer value) shall be skipped individually; the remaining parseable rows continue processing.

**FR-03**: A per-cycle parse-failure counter shall be tracked and returned in the `context_cycle_review` result. The counter captures the count of `context_get` observation rows that were skipped due to unparseable `input` fields during the current call. This makes silent drops observable without requiring server log inspection.

**FR-04**: Within each session, entry IDs extracted per FR-02 shall be ordered by `ts_millis` ascending. All ordered pairs (i, j) where i < j within the same session window form the co-access pair set for that session.

**FR-05**: Co-access pairs shall be deduplicated across sessions using the canonical form `(min(A, B), max(A, B))`. The total pair set is capped at 200 pairs per cycle. When the cap is reached, a warning shall be emitted to server logs; no error is returned to the caller.

**FR-06**: For each canonical pair (A, B) in the deduplicated set, the system shall enqueue **both** directed edges: `GraphEdge(A→B, relation_type="Informs", source="behavioral")` and `GraphEdge(B→A, relation_type="Informs", source="behavioral")` via `store.enqueue_analytics`. Weight assignment: `report.outcome == Some("success")` → `weight = 1.0`; all other outcomes (rework, None, unknown) → `weight = 0.5`.

**FR-07**: Edge enqueueing shall be fire-and-forget via `enqueue_analytics`. The drain task writes edges with `INSERT OR IGNORE` into `graph_edges(source_id, target_id, relation_type)`. When a behavioral edge conflicts with an existing `Informs` edge (e.g. one emitted by NLI), the insert is silently skipped; no error occurs and the existing edge is not modified.

**FR-08**: A cycle with zero qualifying `context_get` observations (after filtering and parsing) shall produce zero behavioral edges. No graph noise is introduced for empty cycles.

**FR-09**: Behavioral edge emission (step 8b) shall execute **after** `store.store_cycle_review()` (step 8a) and **before** the audit log fires (step 11). Step 8b runs on every `context_cycle_review` call — including `force=false` cache-hit returns — because `INSERT OR IGNORE` makes it idempotent.

### Component 2 — Goal-Cluster Population

**FR-10**: At the same step 8b, after enqueuing behavioral edges, the system shall call `store.get_cycle_start_goal_embedding(feature_cycle)` (new store method) to retrieve the goal embedding BLOB from the `cycle_events` start row for the reviewed cycle.

**FR-11**: If `get_cycle_start_goal_embedding` returns `NULL` (cycle had no goal, or goal text was empty at start time), `goal_clusters` population shall be skipped silently. No row is inserted. No error is returned.

**FR-12**: When a non-NULL goal embedding is present, the system shall collect the union of all entry IDs parsed from `context_get` observations across all sessions (the same set used for edge emission), serialize them as a JSON array of integers, and determine the cycle end phase from the latest `cycle_events` row with a non-NULL `phase` value (NULL if none exists).

**FR-13**: One `goal_clusters` row shall be inserted using `INSERT OR IGNORE ON CONFLICT(feature_cycle)`. The row contains: `feature_cycle` (TEXT, UNIQUE), `goal_embedding` (BLOB, bincode-encoded `Vec<f32>`), `phase` (TEXT or NULL), `entry_ids_json` (TEXT, JSON array), `outcome` (TEXT or NULL, from `report.outcome`), `created_at` (INTEGER, Unix millis). First write wins; re-runs for the same `feature_cycle` are no-ops.

**FR-14**: `goal_clusters` writes shall use `write_pool` / `write_pool_server()` directly — not the analytics drain. The table is structural (analogous to `cycle_events` and `cycle_review_index`), not observational telemetry.

**FR-15**: `force=true` re-runs of `context_cycle_review` shall produce idempotent `goal_clusters` behavior. The `INSERT OR IGNORE` on `UNIQUE(feature_cycle)` ensures the first-written row is retained. This is the resolved position from SR-04: INSERT OR IGNORE throughout; there is no INSERT OR REPLACE path.

### Component 3 — Goal-Conditioned Briefing Blending

**FR-16**: At `context_briefing` call time, when `session_state.feature` is populated and `session_state.current_goal` is non-empty, the system shall call `store.get_cycle_start_goal_embedding(session_state.feature)` to retrieve the current session's stored goal embedding.

**FR-17**: If `session_state.feature` is absent, or the returned goal embedding is NULL, the system shall skip all blending immediately — without issuing any further database query — and proceed to the existing pure-semantic retrieval path unchanged.

**FR-18**: When a non-NULL goal embedding is retrieved, the system shall query `goal_clusters` for past cycles whose `goal_embedding` has cosine similarity ≥ `config.goal_cluster_similarity_threshold` with the current goal embedding. The query shall be constrained to the **last 100 rows by `created_at` descending** (recency cap). Results shall be ordered by similarity descending; up to K=5 matching past cycles are used.

**FR-19**: The cosine similarity computation shall run in the async handler context directly (not in `spawn_blocking`). At embedding dimensionality D=384 or 768 and K≤5, this is O(D×K) — well under 1 ms. Embeddings are decoded from BLOB via `decode_goal_embedding` which returns `Vec<f32>`; arithmetic may use f32 or upcast to f64.

**FR-20**: Entry IDs collected from matching `goal_clusters` rows shall be filtered to `status = Active` only via a single `store.get_by_ids()` call. Inactive, deprecated, or quarantined entries are excluded.

**FR-21**: Blending strategy: run semantic search first (existing `SearchService` path, k=20). Fetch Active-filter cluster entries via `store.get_by_ids()`. Compute `cluster_score = (entry.confidence × w_conf_cluster) + (goal_cosine × w_goal_boost)` for each cluster entry, where `goal_cosine` is the cosine similarity already computed during cluster matching and `entry.confidence` is from the Active entry record. Merge semantic results and scored cluster entries into one candidate list. Sort by score descending, deduplicate by entry ID, return top-k=20. Cluster entries displace only the weakest semantic results when their cluster_score exceeds those results' scores.

**FR-22**: If the `goal_clusters` table is empty, or no row meets the cosine ≥ `config.goal_cluster_similarity_threshold` threshold, the blending step produces zero cluster entries; the result set is identical to the pre-crt-046 pure-semantic output.

**FR-23**: `InferenceConfig` shall expose two new fields: `w_goal_cluster_conf: f32` (default 0.35) and `w_goal_boost: f32` (default 0.25). Both weights are used in the cluster_score formula (FR-21). These are not constants in `behavioral_signals.rs` — they are configurable at runtime.

---

## Non-Functional Requirements

**NFR-01 (Idempotency)**: All writes in step 8b (`graph_edges` via analytics drain, `goal_clusters` via direct write) shall be idempotent. Repeated `context_cycle_review` calls for the same `feature_cycle` must produce no duplicate rows and no errors.

**NFR-02 (Cold-Start Correctness)**: When `goal_clusters` is empty, when the current session has no goal embedding, or when no past cycle meets the cosine threshold, `context_briefing` output must be bit-for-bit identical to the pre-crt-046 behavior. No degradation of existing briefing quality on cold start.

**NFR-03 (Briefing Latency)**: The added goal-cluster retrieval path for `context_briefing` (NULL fast-path + recency-capped DB query + in-process cosine scan) must add no more than 5 ms to the median briefing latency. The NULL fast-path (no feature, no goal) must add ≤ 0.1 ms (zero DB queries).

**NFR-04 (Pair Set Safety)**: The 200-pair-per-cycle cap (FR-05) prevents pathological O(N²) pair explosion. Implementation must enforce the cap before pair iteration, not after.

**NFR-05 (No spawn_blocking for sqlx)**: All new `sqlx` async queries (`get_cycle_start_goal_embedding`, `insert_goal_cluster`, `query_goal_clusters_by_embedding`) must execute in the async context. Calling sqlx from `spawn_blocking` is prohibited per ADR in entries #2266 and #2249.

**NFR-06 (File Size)**: `mcp/tools.rs` is already large. New step-8b logic must be extracted into a service module (`services/behavioral_signals.rs` or equivalent) to keep each file under 500 lines.

**NFR-07 (Schema Migration)**: Migration from v21 to v22 must be additive: `CREATE TABLE IF NOT EXISTS` for `goal_clusters` plus its index. No existing column modification. Migration must succeed on a live database that has previously run all v1–v21 migrations.

---

## Acceptance Criteria

**AC-01** [FR-06, FR-01] — Behavioral edge presence: after `context_cycle_review` for a cycle containing ≥ 2 `context_get` observations in the same session, `graph_edges` contains at least one `Informs` edge with `source = 'behavioral'` for the co-access pair.
- Verification: integration test — seed two `context_get` observations for the same session under a feature_cycle; call `context_cycle_review`; query `graph_edges WHERE source = 'behavioral'`; assert ≥ 1 row.

**AC-02** [FR-07, NFR-01] — Edge idempotency: a second `context_cycle_review` call for the same cycle does not produce duplicate `graph_edges` rows.
- Verification: call twice; assert `COUNT(*)` in `graph_edges WHERE source = 'behavioral' AND ...pair...` equals the count after the first call.

**AC-03** [FR-06] — Outcome weighting: a cycle outcome of `"success"` produces behavioral edges with `weight = 1.0`; any other outcome (rework, None) produces `weight = 0.5`.
- Verification: two integration tests, one with `outcome = "success"`, one with `outcome = None`; assert `weight` column values.

**AC-04** [FR-08] — Zero-observation cycle: a cycle with zero `context_get` observations produces zero behavioral edges with `source = 'behavioral'`.
- Verification: seed cycle with only non-get observations; call review; assert `COUNT(*) = 0` for behavioral edges.

**AC-05** [FR-13, FR-12] — Goal-cluster row written: after `context_cycle_review` for a cycle that supplied a goal at `context_cycle start`, `goal_clusters` contains exactly one row for `feature_cycle` with non-NULL `goal_embedding`, correctly serialized `entry_ids_json` (matching accessed IDs), and the expected `outcome` value.
- Verification: integration test — start a cycle with goal text; run context_get calls; call review; query `goal_clusters WHERE feature_cycle = ?`; assert row fields.

**AC-06** [FR-11] — No goal → no cluster row: a cycle that never called `context_cycle start` with a goal (NULL `goal_embedding` in `cycle_events`) produces no `goal_clusters` row.
- Verification: seed cycle with no goal start event; call review; assert `goal_clusters` has zero rows for that feature_cycle.

**AC-07** [FR-21, FR-18] — Blending delivers cluster entries: `context_briefing` for a session whose current goal has a stored embedding with a qualifying past-cycle match (cosine ≥ `config.goal_cluster_similarity_threshold`) returns at least one cluster-derived entry in the result set even when semantic search fills all k=20 slots, provided the cluster entry's cluster_score exceeds the weakest semantic result's score.
- Verification: seed a `goal_clusters` row containing a high-confidence entry whose `goal_embedding` is near the current goal; seed 20 semantic results with lower scores; call `context_briefing`; assert the cluster entry appears in the top-k=20 output and has displaced the lowest-scoring semantic result.

**AC-08** [FR-17, NFR-02] — NULL goal cold-start: `context_briefing` for a session whose goal embedding is NULL (no goal supplied or pre-v21 cycle) returns results identical to the pre-crt-046 pure semantic path.
- Verification: unit/integration test — assert no `goal_clusters` query is issued (spy or in-memory mock); assert result set matches baseline semantic output.

**AC-09** [FR-22, NFR-02] — Empty table cold-start: `context_briefing` with an empty `goal_clusters` table (fresh deployment) returns results identical to the pre-crt-046 pure semantic path.
- Verification: empty `goal_clusters`; call briefing with a goal; assert result set equals pure-semantic baseline.

**AC-10** [FR-20] — Inactive-entry exclusion: cluster-derived entry IDs that do not have `status = Active` are excluded from briefing results.
- Verification: seed a `goal_clusters` row whose `entry_ids_json` includes a deprecated or quarantined entry; assert that entry does not appear in briefing output.

**AC-11** [FR-18, NFR-03] — Recency cap on cosine scan: the `goal_clusters` query at briefing time is constrained to the last 100 rows by `created_at` descending. Rows outside this window are not scanned.
- Verification: seed 101 rows in `goal_clusters` with the 101st (oldest) row having the highest cosine similarity to the test goal; assert that the oldest row's entry IDs do not appear in briefing output (the recency cap excludes it).

**AC-12** [NFR-07, entry #3894] — Schema migration: the v21→v22 migration creates `goal_clusters` and its index on a live database without error; all 7-touchpoint cascade sites (listed in entry #3894) are updated: `migration.rs`, `db.rs` DDL + version counter, `sqlite_parity.rs` table-exists + column-count assertions, `server.rs` version assertions (both sites), and the prior migration test renamed to `at_least_21`.
- Verification: migration integration test opens a v21 fixture DB, runs migration, asserts `read_schema_version(&store) == 22` and `goal_clusters` table exists with correct column count.

**AC-13** [FR-03, SR-01] — Parse-failure observability: when one or more `context_get` observation rows have unparseable `input` JSON during `context_cycle_review`, the returned result includes a non-zero `parse_failure_count` field (or equivalent named field on `CycleReviewRecord` or the review response). Silent drops are not invisible to callers.
- Verification: seed one malformed observation row (missing `id` field) alongside valid rows; call review; assert the returned result exposes a count ≥ 1 for parse failures; assert the valid rows still produce edges (partial recovery).

**AC-14** [FR-05] — Pair cap enforcement: when a cycle contains enough `context_get` observations to produce more than 200 canonical co-access pairs, the emitted edge count is ≤ 400 (200 pairs × 2 directions), and a warning is present in server logs.
- Verification: seed a session with 21 distinct context_get observations (produces 210 pairs); call review; assert edge count ≤ 400; assert warning log message contains "pair cap" or equivalent.

**AC-15** [FR-09, NFR-01] — `force=false` cache-hit re-emission: a `force=false` `context_cycle_review` call that returns a memoised result still runs step 8b. After the second call, `graph_edges` row count for behavioral edges is unchanged from after the first call (idempotent re-emission confirmed).
- Verification: call once (primes memoisation); call again with `force=false`; assert `graph_edges` count is the same both times.

**AC-16** [FR-17] — NULL fast-path zero queries: when `session_state.feature` is absent or the stored goal embedding is NULL, no `goal_clusters` query is issued.
- Verification: unit test using mock store; assert `query_goal_clusters_by_embedding` is not called when goal embedding is NULL.

**AC-17** [AC-12 extension, entry #3894] — Cascade grep clean: after all changes, the command `grep -r 'schema_version.*== 21' crates/` returns zero matches.
- Verification: enforced as part of the Gate 3a migration checklist.

---

## Domain Models

### goal_clusters table

A persistent record of one feature cycle's goal context and the entries accessed during that cycle. Enables cosine-similarity retrieval at briefing time.

```
goal_clusters
  id             INTEGER PK AUTOINCREMENT
  feature_cycle  TEXT    NOT NULL UNIQUE    -- e.g. "crt-046"; one row per cycle
  goal_embedding BLOB    NOT NULL           -- bincode Vec<f32>, same encoding as cycle_events
  phase          TEXT                       -- phase at cycle close; NULL if unavailable
  entry_ids_json TEXT    NOT NULL           -- JSON array of u64 IDs accessed via context_get
  outcome        TEXT                       -- "success" | "rework" | NULL
  created_at     INTEGER NOT NULL           -- Unix millis
```

Index: `idx_goal_clusters_feature_cycle ON goal_clusters(feature_cycle)` (fast unique lookup).
The recency-cap query uses `ORDER BY created_at DESC LIMIT 100` — no additional index required at current scale.

### Behavioral Edge

A directed `graph_edges` row with `relation_type = "Informs"` and `source = "behavioral"`. Emitted as a pair (A→B and B→A) for every co-access pair recovered from a reviewed cycle. Weight reflects cycle outcome (`1.0` for success, `0.5` otherwise). Writes go through the analytics drain with `INSERT OR IGNORE`.

Distinct from NLI-derived `Informs` edges (`source = "nli"`) and cosine-structural edges (`source = "c_cosine"`). When a behavioral edge conflicts with an existing `Informs` edge, the existing edge is retained unchanged (additive-only policy).

### Co-Access Pair

Two entry IDs (A, B) where both were retrieved via `context_get` within the same session window during a single feature cycle. Pairs are canonical: stored as `(min(A, B), max(A, B))` for deduplication. The ordered pair determines which bidirectional edge directions are emitted. Session window = all `context_get` observations sharing the same `session_id` within the cycle.

### Ubiquitous Language

| Term | Definition |
|------|------------|
| `feature_cycle` | Identifier string (e.g. `"crt-046"`) scoping a development cycle. Primary key in `goal_clusters`. |
| `goal_embedding` | A `Vec<f32>` BLOB representing the semantic embedding of a cycle's starting goal text. Encoded/decoded via `encode_goal_embedding` / `decode_goal_embedding` from `unimatrix-store::embedding`. |
| `behavioral edge` | An `Informs` graph edge emitted from observed co-access behavior, not from NLI inference. |
| `blending` | Score-based interleaving of cluster-derived entries with semantic results. Cluster entries are scored via `cluster_score = (entry.confidence × w_conf_cluster) + (goal_cosine × w_goal_boost)`, merged with semantic results, sorted descending, deduplicated, and truncated to top-k=20. A cluster entry displaces a semantic result only when its score exceeds that result's score. |
| `cluster_score` | A composite score for a cluster-derived candidate entry: `(entry.confidence × w_conf_cluster) + (goal_cosine × w_goal_boost)`. Used to rank cluster entries against semantic results in the merged candidate list. |
| `cold start` | Any briefing call where no goal embedding exists or `goal_clusters` has no qualifying match. Degrades silently to pure-semantic retrieval. |
| `recency cap` | The constraint that cosine scanning reads at most the last 100 `goal_clusters` rows by `created_at`. Prevents unbounded O(N) scan as the table grows. |
| `parse-failure count` | Per-cycle counter of `context_get` observation rows skipped due to unparseable `input` JSON. Surfaced in the review result, not only server logs. |

---

## User Workflows

### Workflow A — Cycle Review with Behavioral Edge Emission

1. Agent completes a development cycle with multiple `context_get` calls.
2. Orchestrator calls `context_cycle_review` with `feature_cycle`.
3. System loads sessions, collects `context_get` observations, extracts entry IDs.
4. Parse failures are counted; valid IDs form co-access pairs (capped at 200).
5. Bidirectional `Informs` edges enqueued via analytics drain (fire-and-forget).
6. `goal_clusters` row inserted if goal embedding is available.
7. `context_cycle_review` returns result including `parse_failure_count`.
8. Repeat calls for same cycle are no-ops (INSERT OR IGNORE throughout).

### Workflow B — Goal-Conditioned Briefing

1. Agent starts a new session with `context_cycle start` including goal text.
2. Unimatrix stores the goal embedding in `cycle_events`.
3. Agent calls `context_briefing` with task description.
4. System retrieves goal embedding for `session_state.feature` from `cycle_events`.
5. If NULL → pure semantic search, result returned unchanged.
6. If non-NULL → query `goal_clusters` (last 100 rows, cosine ≥ `config.goal_cluster_similarity_threshold`, top-5 matches).
7. Cluster-derived entry IDs resolved to Active-only records via `store.get_by_ids()`; `cluster_score` computed for each.
8. Semantic results and scored cluster candidates merged, sorted by score descending, deduplicated by entry ID, top-k=20 returned to agent.

### Workflow C — Cold-Start (No History)

1. Fresh deployment, or first cycle with a goal, or briefing with no goal.
2. `goal_clusters` table is empty or goal embedding is NULL.
3. `context_briefing` detects NULL fast-path (zero DB queries for cluster path).
4. Pure semantic search result returned — identical to pre-crt-046 behavior.

---

## Constraints

1. **Schema version**: `CURRENT_SCHEMA_VERSION` is 21. crt-046 bumps to 22. Migration uses `CREATE TABLE IF NOT EXISTS` (idempotent). All 7 cascade touchpoints from entry #3894 must be addressed before Gate 3a.

2. **`INSERT OR IGNORE` throughout**: `graph_edges` via analytics drain; `goal_clusters` via direct write. No `INSERT OR REPLACE` path exists. `force=true` re-runs are idempotent. This resolves SR-04 (the SCOPE.md body/Constraints contradiction): INSERT OR IGNORE is the single canonical pattern.

3. **`write_pool` for `goal_clusters`**: structural table — use `write_pool_server()`, not analytics drain.

4. **No `spawn_blocking` for sqlx**: enforced ADR (entries #2266, #2249). All three new store methods (`get_cycle_start_goal_embedding`, `insert_goal_cluster`, `query_goal_clusters_by_embedding`) must be called from async context.

5. **`graph_edges` UNIQUE constraint**: `UNIQUE(source_id, target_id, relation_type)`. A behavioral `Informs` write for a pair already owned by NLI is silently dropped. Behavioral weight (`1.0`) is never applied to NLI-covered pairs — this is accepted per roadmap spec (additive-only).

6. **Embed boundary is f32**: `decode_goal_embedding` returns `Vec<f32>`. Cosine similarity may use f32 arithmetic or upcast to f64; either is correct at this dimensionality.

7. **Pair cap at 200**: enforced before pair iteration, not after. Warning logged when cap is reached. Cap-hit behavior is server-log only; it is not surfaced in `CycleReviewRecord` (out of scope per SR-06 resolution: only `parse_failure_count` is added to the review result).

8. **File size**: new step-8b logic extracted into `services/behavioral_signals.rs` (or equivalent). `mcp/tools.rs` must remain under 500 lines.

9. **`context_cycle_review` memoisation gate**: step 8b always runs on every call (cache-hit or miss) because its writes are idempotent. The existing early-return on cache-hit must not bypass step 8b.

10. **Observations `input` format**: `context_get` `input` JSON contains `"id": N` where N is an unquoted integer (per MCP param conventions). Parsing is best-effort; malformed rows are skipped and counted.

11. **`session_state.feature` absent edge case**: if `session_state.feature` is absent at briefing time, `get_cycle_start_goal_embedding` is not called and the cold-start path activates immediately (FR-17).

12. **`context_search` is not a blending call site**: goal-conditioned cluster blending applies only to `context_briefing` (and UDS injection if applicable). `context_search` is an agent-driven focused search and must not be silently altered. The blending logic must be placed in `IndexBriefingService::index()` only — never in `SearchService::search()`.

13. **`goal_cluster_similarity_threshold` is a config field**: the cosine similarity threshold must be read from `InferenceConfig.goal_cluster_similarity_threshold: f32` (default 0.80). It is not a hardcoded constant in `behavioral_signals.rs`. The field must be added to `InferenceConfig` and passed through to `query_goal_clusters_by_embedding`.

14. **Naming collision — `EntryRecord.confidence` vs `IndexEntry.confidence`**: the `cluster_score` formula in FR-21 uses `entry.confidence` which refers to `EntryRecord.confidence` (Wilson-score composite, returned by `store.get_by_ids()`). `IndexEntry.confidence` is raw HNSW cosine similarity (returned by `briefing.index()`) — a completely different value with the same field name. Substituting `IndexEntry.confidence` in the formula compiles without error but produces incorrect weights. Implementers must use the `EntryRecord` objects from `store.get_by_ids()`, not the `IndexEntry` objects from the semantic search.

15. **Score scale compatibility — resolved**: `IndexBriefingService::index()` returns `IndexEntry.confidence = se.similarity` (raw HNSW cosine, range [0,1]). `cluster_score` with default weights tops out at ~0.60. The merged sort is scale-compatible — no normalization step is required. Cluster entries compete with and can displace semantic results scoring below ~0.60.

14. **`w_goal_cluster_conf` and `w_goal_boost` are `InferenceConfig` fields**: both blending weights used in the `cluster_score` formula (FR-21, FR-23) must be read from `InferenceConfig` at call time (`w_goal_cluster_conf: f32`, default 0.35; `w_goal_boost: f32`, default 0.25). They are not constants in `behavioral_signals.rs` or any other module. These fields must be wired through to wherever the cluster_score computation occurs.

---

## Dependencies

| Dependency | Type | Notes |
|------------|------|-------|
| `unimatrix-store::embedding::{encode_goal_embedding, decode_goal_embedding}` | Internal | Shipped in crt-043 (schema v21). Reused for goal_clusters BLOB encoding. |
| `cycle_events.goal_embedding` BLOB column | Schema | Shipped in crt-043. Source of goal embeddings for both emission and briefing. |
| `observations` table (`tool`, `input`, `ts_millis`, `session_id` columns) | Schema | Existing. Source for co-access pair recovery. |
| `store.load_sessions_for_feature(feature_cycle)` | Store method | Existing. Used by retrospective pipeline. Reused for edge emission. |
| `store.load_observations_for_sessions(session_ids)` | Store method | Existing. Returns observations for a set of session IDs. |
| `store.enqueue_analytics(AnalyticsWrite::GraphEdge { ... })` | Store method | Existing. Analytics drain writes with INSERT OR IGNORE. |
| `store.store_cycle_review()` | Store method | Existing. Step 8a. Step 8b runs after this. |
| `IndexBriefingService::index()` | Service | Existing. Injection point for blending (before or wrapping the search call). |
| `SessionState.current_goal`, `SessionState.feature` | Domain model | Existing fields. Provide goal text and feature_cycle ID at briefing time. |
| `store.get_by_ids(ids)` | Store method | Existing. Used to filter cluster entry IDs to Active status. |
| SQLite `write_pool_server()` | Infrastructure | Existing pattern for structural table writes. |
| **New**: `store.get_cycle_start_goal_embedding(feature_cycle)` | New store method | Reads goal_embedding BLOB from cycle_events start row. |
| **New**: `store.insert_goal_cluster(...)` | New store method | Inserts one goal_clusters row with INSERT OR IGNORE. |
| **New**: `store.query_goal_clusters_by_embedding(embedding, limit, recency_cap)` | New store method | Returns top-K matching goal_clusters rows within last 100 by created_at. |
| **New**: `services/behavioral_signals.rs` | New module | Contains step-8b orchestration logic to keep tools.rs under 500 lines. |

---

## NOT In Scope

- Removing or downweighting existing `Informs` edges for cycles with negative outcomes. Edges are additive only.
- Phase-stratified briefing weighting (crt-043 composite index enables this; belongs to a later feature).
- Automatic behavioral edge emission on background tick (emission happens only inside `context_cycle_review`).
- Modifying the co-access promotion tick or the `co_access` table.
- Extending the PPR expander (crt-045) to traverse new behavioral edges differently.
- `goal_clusters` row purging or retention policy management.
- Any UI, dashboard, or status surface for goal-cluster data.
- `context_status` or `context_search` exposure of behavioral edge counts.
- **Goal-conditioned blending applied to `context_search`**: `context_search` is a focused, agent-driven mechanism and must not be silently altered by this feature. Blending is restricted to `context_briefing` and UDS injection only.
- `INSERT OR REPLACE` semantics for `force=true` re-runs (resolved: INSERT OR IGNORE throughout).
- Using `audit_log` as the co-access source (observation input is the chosen path).
- Surfacing the pair-cap warning in `CycleReviewRecord` (server log only).

---

## Open Questions for Architect

1. **SR-03 — `write_graph_edge` return contract**: The drain task's `rows_affected()` return has three cases (new insert, UNIQUE conflict no-op, error). Entry #4041 records a Gate 3a rework caused by this confusion. The architect must specify the exact return-contract table in pseudocode before implementation so that any emission counters key off `true` (new row) only, not UNIQUE-conflict returns.

2. **SR-08 — RESOLVED by Option A**: The "zero remaining slots" risk (R-13) is eliminated by Option A score-based interleaving. Cluster entries compete on the same merged candidate list as semantic results; a high-scoring cluster entry displaces the weakest semantic result regardless of how many semantic slots are filled. No silent suppression occurs when a qualifying cluster entry outscores semantic candidates. R-13 is no longer an accepted risk — it is a resolved design decision (see SCOPE.md Resolved Decision #4).

3. **SR-02 — NLI edge weight asymmetry**: Behavioral weight (`1.0` for success) is never applied to pairs already covered by NLI `Informs` edges. This is accepted per roadmap spec, but the architect should confirm whether a future feature will need a mechanism to "promote" behavioral weight onto existing NLI edges. If so, a `weight_behavioral` shadow column could be reserved now at low cost.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned 20 entries; most relevant: #3894 (schema cascade checklist, 7-touchpoint new-table variant applied directly to AC-12/AC-17), #3409 (SubagentStart hook routing, background context on briefing call path), #3397/#3402 (col-025 ADRs for briefing query derivation and goal retrieval pattern), #395 (SignalDigest slot assignments, confirming behavioral source naming conventions). Entry #3312 (observation `input` field JSON format sensitivity) retrieved directly and applied to FR-02 and FR-03.
