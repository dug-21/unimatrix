# crt-046 — Behavioral Signal Delivery: Implementation Brief

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/crt-046/SCOPE.md |
| Scope Risk Assessment | product/features/crt-046/SCOPE-RISK-ASSESSMENT.md |
| Architecture | product/features/crt-046/architecture/ARCHITECTURE.md |
| ADR-001 | product/features/crt-046/architecture/ADR-001-behavioral-signals-module.md |
| ADR-002 | product/features/crt-046/architecture/ADR-002-goal-clusters-write-strategy.md |
| ADR-003 | product/features/crt-046/architecture/ADR-003-goal-cluster-cosine-scan-recency-cap.md |
| ADR-004 | product/features/crt-046/architecture/ADR-004-briefing-null-short-circuit.md |
| ADR-005 | product/features/crt-046/architecture/ADR-005-briefing-score-based-cluster-interleaving.md |
| Specification | product/features/crt-046/specification/SPECIFICATION.md |
| Risk-Test Strategy | product/features/crt-046/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/crt-046/ALIGNMENT-REPORT.md |

---

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| behavioral_signals (services/behavioral_signals.rs) | pseudocode/behavioral-signals.md | test-plan/behavioral-signals.md |
| store-v22 (unimatrix-store schema + new methods) | pseudocode/store-v22.md | test-plan/store-v22.md |
| cycle-review-step-8b (context_cycle_review insertion point) | pseudocode/cycle-review-step-8b.md | test-plan/cycle-review-step-8b.md |
| briefing-blending (context_briefing goal-conditioned path) | pseudocode/briefing-blending.md | test-plan/briefing-blending.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

---

## Goal

crt-046 closes the learning loop between agent session telemetry and future retrieval quality. At `context_cycle_review` call time, co-access pairs are extracted from session observations and emitted as bidirectional behavioral `Informs` graph edges; a `goal_clusters` row is persisted alongside them. At `context_briefing` call time, the current session's goal embedding is compared to past cycles in `goal_clusters` via cosine similarity; matching cluster entries are scored, merged with semantic results into one ranked list, and the top-k=20 returned. Cold-start correctness is guaranteed: zero goal history, absent feature attribution, or a NULL goal embedding degrades silently to the existing pure-semantic path with no output change.

---

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| Behavioral signal logic placement | New module `services/behavioral_signals.rs` (`pub(crate)`). Keeps `mcp/tools.rs` under 500-line cap; single-responsibility; unit-testable without full server stack. | ADR-001 | architecture/ADR-001-behavioral-signals-module.md |
| `goal_clusters` write path | Direct `write_pool_server()` in async handler context (not analytics drain). Structural table; drain's 500ms flush interval is inappropriate for structural writes. | ADR-002 | architecture/ADR-002-goal-clusters-write-strategy.md |
| INSERT semantics — SR-04 resolution | INSERT OR IGNORE throughout. No INSERT OR REPLACE path exists anywhere in this feature. First write wins; `force=true` re-runs are silent no-ops. SCOPE.md body prose mentioning INSERT OR REPLACE is overridden by Constraints section and ADR-002. | ADR-002 | architecture/ADR-002-goal-clusters-write-strategy.md |
| `graph_edges` write path | `enqueue_analytics(AnalyticsWrite::GraphEdge)` with `INSERT OR IGNORE`. Fire-and-forget drain; additive-only. Behavioral edges use `bootstrap_only = false` and are not subject to the shed policy. | SCOPE §Constraints | — |
| Cosine scan strategy | In-process scan over last 100 `goal_clusters` rows by `created_at DESC`. O(100×D) at D=384 is well under 1ms. Named constant `RECENCY_CAP = 100`. No HNSW index. | ADR-003 | architecture/ADR-003-goal-cluster-cosine-scan-recency-cap.md |
| Cosine threshold as config field | `InferenceConfig.goal_cluster_similarity_threshold: f32`, default 0.80. Not a hardcoded constant. Must be read from config and passed to `query_goal_clusters_by_embedding` at call time. | ADR-003, SPEC Constraint 13 | architecture/ADR-003-goal-cluster-cosine-scan-recency-cap.md |
| Briefing NULL short-circuit | Two-level guard in `context_briefing` handler: (1) skip all blending when `session_state.feature.is_none()` or `current_goal.is_empty()`; (2) skip cluster query when `get_cycle_start_goal_embedding` returns `None`. Both guards fire before the subsequent DB call. | ADR-004 | architecture/ADR-004-briefing-null-short-circuit.md |
| Blending strategy — FR-21 (Option A) | Score-based interleaving. Run semantic search at k=20. Fetch Active cluster entries via `store.get_by_ids()`. Compute `cluster_score = (entry.confidence × w_conf_cluster) + (goal_cosine × w_goal_boost)`. Merge semantic results and scored cluster entries into one candidate list. Sort by score descending, deduplicate by entry ID (first occurrence wins), return top-k=20. Cluster entries displace the weakest semantic results when their `cluster_score` exceeds those results' scores. Supersedes the prior "remaining slots" Option B — which was inert in any live deployment with ≥20 active entries. R-13 is resolved: zero-slot suppression no longer applies. | ADR-005 | architecture/ADR-005-briefing-score-based-cluster-interleaving.md |
| Blending weight fields | `InferenceConfig.w_goal_cluster_conf: f32` (default 0.35) and `InferenceConfig.w_goal_boost: f32` (default 0.25). Both are `InferenceConfig` fields, not constants in `behavioral_signals.rs`. Wired through to the cluster_score computation. Calibration deferred to post-ship eval. | ADR-005, SPEC FR-23 | architecture/ADR-005-briefing-score-based-cluster-interleaving.md |
| `context_search` exclusion from blending | `context_search` is NOT a blending call site. Blending lives in `IndexBriefingService::index()` only, invoked from the `context_briefing` handler. `SearchService::search()` must not be modified. | SCOPE §Non-Goals, SPEC Constraint 12 | — |
| Co-access pair source | `observations` table — parse `input` JSON field to extract `id` integer. Consistent with retrospective pipeline. `audit_log` not used. | SCOPE §Resolved Decisions #1 | — |
| Edge directionality | Bidirectional: emit `(A→B)` and `(B→A)` for each canonical pair. Required by crt-044 outgoing-only traversal convention. | SPEC FR-06 | — |
| Self-pair (A, A) exclusion | `filter(|(a, b)| a != b)` applied in `build_coaccess_pairs` before deduplication. (Human-approved DN-3.) | ALIGNMENT-REPORT §Human-Approved DN-3 | — |
| SR-04 INSERT OR REPLACE contradiction | Resolved as INSERT OR IGNORE throughout. The SCOPE Constraints section takes precedence. No INSERT OR REPLACE path. | ADR-002 | architecture/ADR-002-goal-clusters-write-strategy.md |

---

## Human-Approved Resolutions (Binding Implementation Rules)

These resolutions override architecture prose where there is a conflict. Implementers must treat them as non-negotiable directives.

### Resolution 1 — `parse_failure_count` surface (V-01)

`parse_failure_count: u32` must be added as a **top-level field in the `context_cycle_review` JSON response**, OUTSIDE the serialized `CycleReviewRecord`. The `CycleReviewRecord` struct is NOT extended. No `SUMMARY_SCHEMA_VERSION` bump is required. FR-03 and AC-13 are satisfied by this mechanism. The architecture prose stating "warn! log level only" is WRONG — do not follow it.

### Resolution 2 — Step 8b always runs (DN-1)

The architecture §Component 3 step-sequence prose stating that the `force=false` early-return "returns before step 8b" is **WRONG**. FR-09 is the authority. Step 8b runs on every `context_cycle_review` call — cache-hit (`force=false`) or cache-miss. The memoisation early-return must appear **after** the step 8b call site in code. AC-15 is the gate test. Any implementation that places the early-return before step 8b will fail Gate 3a.

### Resolution 3 — Empty `current_goal` activates cold-start (DN-2)

Treat `session_state.current_goal == ""` (empty string) identical to absent. When `current_goal` is empty, no `get_cycle_start_goal_embedding` call is made and blending is entirely skipped. Cold-start activates immediately. The write side (crt-043) already stores an embedding only for non-empty goals, so the cold-start path is safe from both sides.

### Resolution 4 — Self-pair (A, A) exclusion (DN-3)

Add `filter(|(a, b)| a != b)` in `build_coaccess_pairs` before the deduplication step. Self-loops (`Informs(A→A)`) have no traversal value and must not be emitted. The E-02 test in pair-building unit tests must assert self-pairs are excluded.

### Resolution 5 — R-13 resolved by Option A (ADR-005)

The prior "zero remaining slots — accepted" position is superseded. Option A score-based interleaving ensures the blending path is never inert: cluster entries compete on the same ranked candidate list as semantic results and displace the weakest semantic result when their `cluster_score` exceeds it. There is no silent suppression path to "confirm" or protect. Implementers must not add remaining-slot logic, slot-expansion logic, or any leftover-slot splice — the merged-sort approach handles all cases uniformly.

---

## write_graph_edge Return Contract (MUST lead pseudocode — pattern #4041)

Pseudocode for `emit_behavioral_edges` MUST begin with this table before any implementation prose:

| `write_graph_edge` return | Meaning | Counter action |
|---------------------------|---------|----------------|
| `Ok(true)` | New row inserted | Increment `edges_enqueued` |
| `Ok(false)` | UNIQUE conflict, silently ignored | Do not increment; not an error |
| `Err(_)` | SQL infrastructure failure | Log `warn!`, do not increment, continue |

Emission counters key off `Ok(true)` only. Any counter increment on `Ok(false)` or `Err` is a bug (root cause of crt-040 Gate 3a rework, entry #4041).

---

---

## Files to Create / Modify

### New Files

| File | Summary |
|------|---------|
| `crates/unimatrix-server/src/services/behavioral_signals.rs` | All behavioral signal logic: `collect_coaccess_entry_ids`, `build_coaccess_pairs`, `outcome_to_weight`, `emit_behavioral_edges`, `populate_goal_cluster`, `blend_cluster_entries`. Constants: `RECENCY_CAP`, `PAIR_CAP`. |
| `crates/unimatrix-store/src/goal_clusters.rs` | `insert_goal_cluster`, `query_goal_clusters_by_embedding`, `GoalClusterRow` struct. |

### Modified Files

| File | Change Summary |
|------|---------------|
| `crates/unimatrix-store/src/migration.rs` | Add `if current_version < 22` block: `goal_clusters` DDL + `idx_goal_clusters_created_at` index + `UPDATE counters SET value = 22`. |
| `crates/unimatrix-store/src/db.rs` | (1) Add `goal_clusters` DDL (byte-identical to migration block) to `create_tables_if_needed()`. (2) Bump hardcoded schema_version INSERT to 22. (3) Rename `test_schema_version_initialized_to_21_on_fresh_db` → `_22`. (4) Add `get_cycle_start_goal_embedding` async method. |
| `crates/unimatrix-store/src/lib.rs` | Declare `pub mod goal_clusters;`. |
| `crates/unimatrix-server/src/services/mod.rs` | Declare `pub(crate) mod behavioral_signals;`. |
| `crates/unimatrix-server/src/mcp/tools.rs` | (1) Insert step 8b call in `context_cycle_review` after step 8a; memoisation early-return placed AFTER step 8b (Resolution 2). (2) Add `parse_failure_count` as top-level field in `context_cycle_review` JSON response (Resolution 1). (3) Add Guard B two-level short-circuit + Option A blending sequence in `context_briefing` handler. |
| `crates/unimatrix-server/src/infra/inference_config.rs` (or equivalent) | Add three new fields: `goal_cluster_similarity_threshold: f32` (default 0.80), `w_goal_cluster_conf: f32` (default 0.35), `w_goal_boost: f32` (default 0.25). |
| `crates/unimatrix-store/src/tests/sqlite_parity.rs` | Add `test_create_tables_goal_clusters_exists` and `test_create_tables_goal_clusters_schema` (7 columns). Update `test_schema_version_is_N` to 22. |
| `crates/unimatrix-store/src/tests/migration_tests.rs` (or equivalent) | Add v22 migration integration test (open v21 fixture, run migration, assert version == 22 and `goal_clusters` exists with 7 columns). Rename `test_current_schema_version_is_21` → `test_current_schema_version_is_at_least_21` with `>= 21` predicate. |
| `crates/unimatrix-server/src/server.rs` | Update both `assert_eq!(version, 21)` sites to 22. |

---

## Data Structures

### `goal_clusters` table (schema v22)

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

7 columns. The same DDL must appear byte-identically in `migration.rs` and `db.rs::create_tables_if_needed()`.

### `GoalClusterRow` (new struct in `unimatrix-store/src/goal_clusters.rs`)

```rust
pub struct GoalClusterRow {
    pub id: i64,
    pub feature_cycle: String,
    pub goal_embedding: Vec<f32>,
    pub phase: Option<String>,
    pub entry_ids_json: String,
    pub outcome: Option<String>,
    pub created_at: i64,
    pub similarity: f32,   // computed at query time, not stored
}
```

---

## Function Signatures

### `unimatrix-store/src/db.rs` — new method

```rust
pub async fn get_cycle_start_goal_embedding(
    &self,
    cycle_id: &str,
) -> Result<Option<Vec<f32>>>
```

Queries `cycle_events WHERE event_type = 'cycle_start' AND goal_embedding IS NOT NULL ORDER BY timestamp ASC LIMIT 1`. Decodes BLOB via `decode_goal_embedding`. Returns `Ok(None)` on no matching row or NULL BLOB. Uses `read_pool()`.

### `unimatrix-store/src/goal_clusters.rs` — new methods

```rust
pub async fn insert_goal_cluster(
    &self,
    feature_cycle: &str,
    goal_embedding: Vec<f32>,
    phase: Option<&str>,
    entry_ids_json: &str,
    outcome: Option<&str>,
    created_at: i64,
) -> Result<bool>
// INSERT OR IGNORE; returns true on new row, false on UNIQUE conflict.
// Uses write_pool_server() directly (NOT analytics drain).

pub async fn query_goal_clusters_by_embedding(
    &self,
    embedding: &[f32],
    threshold: f32,
    recency_limit: u64,
) -> Result<Vec<GoalClusterRow>>
// Fetches last `recency_limit` rows ORDER BY created_at DESC,
// decodes embeddings, computes cosine similarity in-process,
// filters to >= threshold, returns sorted by similarity descending.
// Uses read_pool().
```

### `unimatrix-server/src/services/behavioral_signals.rs` — all pub(crate)

```rust
fn collect_coaccess_entry_ids(
    obs: &[ObservationRow],
) -> (HashMap<String, Vec<(u64, i64)>>, usize)
// Returns (by_session_id → [(entry_id, ts_millis)], parse_failure_count).

fn build_coaccess_pairs(
    by_session: HashMap<String, Vec<(u64, i64)>>,
) -> (Vec<(u64, u64)>, bool)
// Returns (canonical_pairs_capped_at_200, cap_hit).
// Self-pairs (a == b) are excluded by filter(|(a, b)| a != b) before dedup.
// Cap is enforced at enumeration time (halt at 200), not by post-hoc truncation.

fn outcome_to_weight(outcome: Option<&str>) -> f32
// "success" → 1.0, all others → 0.5

async fn emit_behavioral_edges(
    store: &SqlxStore,
    pairs: &[(u64, u64)],
    weight: f32,
) -> (usize, usize)
// Returns (edges_enqueued, pairs_skipped_on_conflict).
// Increments edges_enqueued on Ok(true) ONLY (pattern #4041).
// Pseudocode MUST lead with the write_graph_edge return contract table.

async fn populate_goal_cluster(
    store: &SqlxStore,
    feature_cycle: &str,
    goal_embedding: Vec<f32>,
    entry_ids: &[u64],
    phase: Option<&str>,
    outcome: Option<&str>,
) -> Result<bool>
// Calls store.insert_goal_cluster(...). Returns Ok(true) on new row.

fn blend_cluster_entries(
    semantic: Vec<IndexEntry>,
    cluster_entries_with_scores: Vec<(IndexEntry, f32)>,
    k: usize,
) -> Vec<IndexEntry>
// Pure function — no store access. Caller supplies pre-fetched, pre-scored
// cluster entries. Merges both lists, sorts by score descending, deduplicates
// by entry ID (first occurrence wins), returns top-k.
// Semantic entries retain their existing score field.
// cluster_entries_with_scores carry their cluster_score as the f32.
```

---

## Step 8b Sequence (context_cycle_review)

Step 8b runs on EVERY `context_cycle_review` call — cache-hit or cache-miss. The memoisation early-return is positioned AFTER step 8b (Resolution 2 / FR-09).

```
[step 8a]  store.store_cycle_review(record)              // existing
[step 8b]  behavioral_signals::run_step_8b(               // NEW — always runs
             store, feature_cycle, report.outcome
           )
  1. store.load_sessions_for_feature(feature_cycle)
  2. store.load_observations_for_sessions(session_ids)
  3. collect_coaccess_entry_ids(observations)             → (by_session, parse_failures)
  4. build_coaccess_pairs(by_session)                     → (pairs, cap_hit)
  5. if cap_hit → warn!("pair cap reached …")
  6. outcome_to_weight(report.outcome)                    → weight
  7. if pairs.is_empty() → debug!; skip emission
  8. emit_behavioral_edges(store, pairs, weight)          → (enqueued, skipped)
  9. store.get_cycle_start_goal_embedding(feature_cycle)  → embedding_opt
 10. if Some(embedding) → populate_goal_cluster(store, …)
 11. return parse_failures as top-level parse_failure_count: u32 in response
[step 11]  audit log                                      // existing
[memoisation early-return]                                // AFTER step 8b
```

All step 8b errors are non-fatal: log at `warn!` and continue. `context_cycle_review` must return successfully even if all of step 8b fails.

---

## Briefing Blending Sequence (context_briefing — Option A, ADR-005)

```
// Level 1 guard — before any DB call (ADR-004, Resolution 3)
if session_state.feature.is_none() OR current_goal.is_empty()
    → cold-start: call briefing.index() unchanged; return

// Level 2 guard — goal embedding lookup
embedding_opt = store.get_cycle_start_goal_embedding(feature).await
if embedding_opt.is_none()
    → cold-start: call briefing.index() unchanged; return

// Cluster query (recency-capped, cosine-filtered)
matching_clusters = store.query_goal_clusters_by_embedding(
    embedding, config.goal_cluster_similarity_threshold, RECENCY_CAP=100
).await
if matching_clusters.is_empty()
    → cold-start: call briefing.index() unchanged; return

// Active entry fetch + score computation
cluster_entry_ids = union of entry_ids_json from matching_clusters (up to top-5)
active_entries    = store.get_by_ids(cluster_entry_ids).await  // Active filter
cluster_entries_with_scores = active_entries.map(|entry| {
    let row_similarity = matching_clusters[entry.cluster_row].similarity
    let cluster_score  = (entry.confidence * config.w_goal_cluster_conf)
                       + (row_similarity   * config.w_goal_boost)
    (entry, cluster_score)
})

// Semantic search (existing path — k=20)
semantic_results = briefing.index(params).await

// Score-based interleaving (Option A)
final = behavioral_signals::blend_cluster_entries(
    semantic_results,
    cluster_entries_with_scores,
    k = 20
)
// blend_cluster_entries:
//   merge both lists; semantic entries use their existing score field;
//   cluster entries use cluster_score;
//   sort descending by score;
//   deduplicate by entry ID (first occurrence wins);
//   return top-k=20.
```

// Score scale: RESOLVED. IndexEntry.confidence = raw cosine [0,1]. cluster_score max ≈ 0.60.
// Scales are compatible — no normalization before this sort.
//
// ⚠ NAMING COLLISION — critical: entry_record.confidence (Wilson-score, from store.get_by_ids())
// ≠ index_entry.confidence (raw cosine, from briefing.index()). The cluster_score formula
// uses EntryRecord.confidence. Both field names are "confidence". Both compile. The wrong
// one produces silent weight miscalculation. See ARCHITECTURE.md §Component 4 step 6, ADR-005.

---

## Schema Migration v21 → v22 Cascade Checklist (9 touchpoints, from pattern #3894)

All 9 must be addressed before Gate 3a. Gate check: `grep -r 'schema_version.*== 21' crates/` must return zero matches (AC-17).

1. `migration.rs` — add `if current_version < 22` block with `goal_clusters` DDL + `idx_goal_clusters_created_at` index + `UPDATE counters SET value = 22`.
2. `db.rs` — add matching `goal_clusters` DDL to `create_tables_if_needed()` (byte-identical to migration block).
3. `db.rs` — bump hardcoded schema_version INSERT integer to 22.
4. `db.rs` — rename `test_schema_version_initialized_to_21_on_fresh_db` → `_22`.
5. `sqlite_parity.rs` — add `test_create_tables_goal_clusters_exists` and `test_create_tables_goal_clusters_schema` (exact column count: 7).
6. `sqlite_parity.rs` — update `test_schema_version_is_N` to 22; update `test_schema_column_count` for any table with column changes.
7. `server.rs` — update both `assert_eq!(version, 21)` sites to 22.
8. Migration tests — rename current `test_current_schema_version_is_21` to `test_current_schema_version_is_at_least_21` with `>= 21` predicate.
9. All migration test files — grep for column-count assertions referencing the old total; update if affected.

---

## Constraints

1. **Schema version**: current is v21; crt-046 bumps to v22. Migration is additive only (`CREATE TABLE IF NOT EXISTS`; no column drops or alterations).
2. **INSERT OR IGNORE throughout**: both `graph_edges` (via analytics drain) and `goal_clusters` (direct write). No INSERT OR REPLACE anywhere in this feature.
3. **`write_pool_server()` for `goal_clusters`**: structural table, not observational telemetry. Use the same pattern as `cycle_review_index`.
4. **No `spawn_blocking` for sqlx**: all three new store methods (`get_cycle_start_goal_embedding`, `insert_goal_cluster`, `query_goal_clusters_by_embedding`) must be `async fn` called with `.await` from the async handler context. ADR entries #2266 and #2249.
5. **`graph_edges` UNIQUE constraint**: `UNIQUE(source_id, target_id, relation_type)`. A behavioral `Informs` write for a pair already owned by NLI is silently dropped — not an error, and `edges_enqueued` is not incremented (pattern #4041).
6. **Embed boundary is f32**: `decode_goal_embedding` returns `Vec<f32>`. Cosine similarity may use f32 or upcast to f64; either is correct.
7. **Pair cap 200**: enforced at enumeration time (halt when `pairs.len() == 200`), not by truncating after full generation. Server-log warning only; cap-hit is NOT surfaced in `CycleReviewRecord` or the response (SR-06 resolution).
8. **File size**: `mcp/tools.rs` must remain under 500 lines. All new signal logic lives in `behavioral_signals.rs`.
9. **`context_search` exclusion**: blending code must never be placed in or called from `SearchService::search()`.
10. **`InferenceConfig` new fields**: add `goal_cluster_similarity_threshold: f32` (default 0.80), `w_goal_cluster_conf: f32` (default 0.35), and `w_goal_boost: f32` (default 0.25). All three must be read from config at call time — none are constants in `behavioral_signals.rs`.
11. **Analytics drain shedding**: `bootstrap_only = false` behavioral edges are not subject to the shed policy. Verify `bootstrap_only=false` is not in the shed path in `analytics.rs`.
12. **Analytics drain flush before graph_edges assertions**: integration tests must force a drain flush before asserting `graph_edges` rows (I-02, entry #2148).
13. **`blend_cluster_entries` is a pure function**: it takes pre-fetched, pre-scored cluster entries and has no store access. The caller (in the `context_briefing` handler) is responsible for fetching Active entry records via `store.get_by_ids()` and computing `cluster_score` before passing to `blend_cluster_entries`.

---

## Dependencies

| Dependency | Type | Notes |
|------------|------|-------|
| `unimatrix-store::embedding::{encode_goal_embedding, decode_goal_embedding}` | Internal crate | Shipped in crt-043 (schema v21). Reused for BLOB encode/decode. |
| `cycle_events.goal_embedding` BLOB column | Schema — existing | Source of goal embeddings at both step 8b and briefing time. |
| `observations` table (`tool`, `input`, `ts_millis`, `session_id`) | Schema — existing | Source for co-access pair recovery. |
| `store.load_sessions_for_feature(feature_cycle)` | Store method — existing | Reused from retrospective pipeline. |
| `store.load_observations_for_sessions(session_ids)` | Store method — existing | Reused from retrospective pipeline. |
| `store.enqueue_analytics(AnalyticsWrite::GraphEdge)` | Store method — existing | Analytics drain; INSERT OR IGNORE. |
| `store.store_cycle_review()` | Store method — existing | Step 8a; step 8b runs after this. |
| `IndexBriefingService::index()` | Service — existing | Semantic search; called after blending guards in the briefing handler. |
| `SessionState.current_goal`, `SessionState.feature` | Domain model — existing | Goal text and feature ID at briefing time. |
| `store.get_by_ids(ids)` | Store method — existing | Active-status filter for cluster entry IDs in briefing blending path. |
| `write_pool_server()` | Infrastructure — existing | Direct structural table writes. |
| `AnalyticsWrite::GraphEdge` | Enum variant — existing | Fields: `source_id`, `target_id`, `relation_type`, `weight`, `created_by`, `source`, `bootstrap_only`. |
| `InferenceConfig` | Config struct — existing (modified) | Add `goal_cluster_similarity_threshold`, `w_goal_cluster_conf`, `w_goal_boost`. |

---

## NOT In Scope

- Removing or downweighting existing `Informs` edges for cycles with negative outcomes (additive-only).
- Phase-stratified briefing weighting (crt-043 composite index enables this; later feature).
- Automatic behavioral edge emission on background tick.
- Modifying the co-access promotion tick or `co_access` table.
- Extending the PPR expander (crt-045) for new behavioral edges.
- `goal_clusters` row purging or retention policy management.
- Any UI, dashboard, or status surface for goal-cluster data.
- `context_status` or `context_search` exposure of behavioral edge counts.
- Goal-conditioned blending in `context_search`.
- INSERT OR REPLACE semantics for `force=true` re-runs.
- Remaining-slot splice logic or k-expansion beyond k=20 (Option A score-based interleaving handles all cases; no leftover-slot logic).
- Using `audit_log` as the co-access source.
- Surfacing the pair-cap warning in `CycleReviewRecord`.
- `SUMMARY_SCHEMA_VERSION` bump (not required; `parse_failure_count` is a top-level response field, not inside `CycleReviewRecord`).

---

## Alignment Status

**Overall verdict from ALIGNMENT-REPORT.md: ALIGNED** — V-01 closed. All checks pass.

| Check | Status |
|-------|--------|
| Vision Alignment | PASS — directly implements W3-1 behavioral signal collection (Wave 1A / Group 6 roadmap) |
| Milestone Fit | PASS — correct Wave 1A post-WA-4 deliverable; no future milestone capabilities anticipated |
| Scope Gaps | PASS — all four SCOPE.md goals (Items 1–3 + cold-start guarantee) addressed in all source documents |
| Scope Additions | PASS — parse-failure counter surface conflict between architecture (`warn!` log only) and specification (FR-03 / AC-13 requires it in the response) was resolved: top-level `parse_failure_count: u32` field in the MCP response, outside `CycleReviewRecord`, no schema version bump. V-01 closed. |
| Architecture Consistency | PASS — SR-04 resolved; design amendments (context_search exclusion, configurable threshold, Option A blending) verified in both architecture and specification. |
| Risk Completeness | PASS — all 16 risks mapped to test scenarios; all scope risks from SCOPE-RISK-ASSESSMENT.md traced in risk register. R-13 resolved by ADR-005. |

**Critical architecture prose override**: ARCHITECTURE.md §Component 3 memoisation gate description is wrong. Do not follow it. FR-09 and Resolution 2 are authoritative: step 8b runs on every call.

**Design amendments carried forward** (all verified in architecture and specification):
- `context_search` is NOT a blending call site. Blending lives in `IndexBriefingService::index()` only.
- Cosine threshold is `InferenceConfig.goal_cluster_similarity_threshold` (default 0.80), not a hardcoded constant.
- Blending strategy is Option A (ADR-005): score-based interleaving. R-13 is resolved — no silent suppression, no remaining-slot logic.

---

## Non-Negotiable Gate Tests (from Risk-Test Strategy)

The following tests are gate blockers at Gate 3a and Gate 3c:

| Test | Risk | Description |
|------|------|-------------|
| AC-13 | R-04 | `parse_failure_count` in MCP response — seed malformed row; assert count ≥ 1 in returned payload |
| AC-15 | R-01 | `force=false` step 8b re-emission — call twice; assert `graph_edges` count identical after both calls |
| AC-11 | R-07 | Recency cap 101-row boundary — oldest row excluded even when highest cosine similarity |
| AC-17 | R-05 | `grep -r 'schema_version.*== 21' crates/` returns zero matches |
| R-02 contract test | R-02 | UNIQUE-conflict path — assert `edges_enqueued` not incremented when NLI already owns edge |
| Drain flush | R-03 / I-02 | Every integration test querying `graph_edges` must flush analytics drain before asserting |

---

## Open Questions for Delivery Agent

1. **RESOLVED — Score normalization**: `IndexBriefingService::index()` maps `se.similarity` (raw HNSW cosine) to `IndexEntry.confidence`. Semantic scores are in [0, 1]. `cluster_score` with defaults tops out at ~0.60. The merged sort is **scale-compatible — no normalization required**. Cluster entries compete with and can displace semantic results scoring below ~0.60 (approximately the bottom half of the result set). Top-half results (cosine ≥ 0.60, genuinely semantically relevant) are preserved. This is the correct conservative posture for first ship; post-ship tuning via `w_conf_cluster`/`w_goal_boost` is expected after eval.

2. **OQ-1**: Confirm that `store.get_by_ids()` returns full `EntryRecord` objects (or equivalent) that include the `confidence` field (Wilson-score composite). The `cluster_score` formula requires this. If `get_by_ids` returns a lighter projection without `confidence`, the scoring formula is broken at the call site.

3. **OQ-2** (ARCHITECTURE §Open Questions): When `session_state.feature` is absent but `current_goal` is Some, cold-start path activates (Resolution 3 also covers `current_goal = ""`). Confirm this is correct for feature-unattributed sessions.

4. **OQ-3** (ARCHITECTURE §Open Questions): Verify `bootstrap_only=false` behavioral edges are not subject to the analytics drain shed policy. Inspect the shedding condition in `analytics.rs` and confirm before pseudocode.

5. **I-04** (RISK-TEST-STRATEGY §Integration Risks): Confirm that an empty `current_goal` (Resolution 3) activates cold-start before the `get_cycle_start_goal_embedding` DB call, and that `feature.is_some()` with `current_goal = ""` does NOT issue the embedding lookup.
