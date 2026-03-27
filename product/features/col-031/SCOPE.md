# col-031: Phase-Conditioned Frequency Table

## Problem Statement

Phase is the highest-signal discrete feature for knowledge surfacing quality (ASS-032
RESEARCH-SYNTHESIS.md). The fused scoring formula has a placeholder term
`w_phase_explicit * phase_explicit_norm` that is hardcoded to `0.0` since crt-026
(ADR-003: "W3-1 reserved placeholder"). Neither the scoring formula nor the graph
traversal is phase-informed.

This creates a structural blindness: Unimatrix does not know that "during delivery
phase, entries of category lesson-learned are consistently useful" vs. "during scope
phase, decision entries dominate retrieval." Every query regardless of phase starts
from the same prior.

The frequency table is the non-parametric RA-DIT feedback loop (Loop 2,
RESEARCH-SYNTHESIS.md §Loop 2). It requires no training step, no model downloads, and
no new ML infrastructure — it is purely a SQL aggregation over `query_log` rows
(already populated with `phase` since col-028 / #397, schema v17). With col-028
shipped (gate-3c PASS 2026-03-26), the prerequisite data is present.

Affected agents: every agent in a named workflow phase querying Unimatrix, who receives
phase-agnostic results today.

## Goals

1. Implement `PhaseFreqTable` — an in-memory struct holding
   `HashMap<(phase: String, category: String), Vec<(entry_id: u64, score: f32)>>`
   sorted descending by rank score, rebuilt each background tick from `query_log`.
2. Implement `PhaseFreqTableHandle = Arc<RwLock<PhaseFreqTable>>` following the
   `TypedGraphStateHandle` / `EffectivenessStateHandle` pattern exactly.
3. Wire the handle into `ServiceLayer` and thread it to `SearchService` and the
   background tick via `Arc::clone`.
4. Activate `w_phase_explicit` in fused scoring: compute `phase_explicit_norm` from
   the frequency table at query time; raise the default from `0.0` to `0.05` in
   `InferenceConfig`. The scoring hot-path must check `use_fallback` before calling
   `phase_affinity_score` — when `use_fallback = true` or `current_phase = None`,
   `phase_explicit_norm = 0.0` (bit-for-bit identical to pre-col-031).
5. Expose `phase_affinity_score(entry_id: u64, entry_category: &str, phase: &str) -> f32`
   as a public method on `PhaseFreqTable` — the integration contract that #398 will call
   to weight its PPR personalization vector. col-031 does not implement PPR (#398 does
   not yet exist) and does not wire into PPR internals. It publishes the API only.
   `phase_affinity_score` returns `1.0` on cold-start (`use_fallback = true`) — neutral
   multiplier for PPR: `hnsw_score × 1.0 = hnsw_score`. This is distinct from Goal 4:
   fused scoring returns `0.0` via the `use_fallback` guard; PPR calls `phase_affinity_score`
   directly and receives `1.0`. Two callers, two cold-start behaviors, one method.
6. Add `query_log_lookback_days: u32` to `InferenceConfig` (default 30) to govern the
   time window used by the frequency table SQL query. The retention window is
   `WHERE ts > strftime('%s', 'now') - lookback_days * 86400` — a simple time filter
   on `query_log.ts`. No JOIN with `sessions` is required and no cycle-based schema is
   assumed. `#409` owns proper cycle-aligned GC; col-031 does not anticipate that schema.
7. Cold-start degrades gracefully: `use_fallback = true` → `phase_explicit_norm = 0.0`
   via the Goal 4 guard → scores bit-for-bit identical to pre-col-031.

## Non-Goals

- **No query_log schema changes** — `query_log.phase` (schema v17) is the prerequisite,
  already shipped. Zero new migrations.
- **No PPR implementation** — col-031 publishes `phase_affinity_score` as the integration
  contract. PPR itself is #398.
- **No cycle-based retention** — `query_log_lookback_days` is a time window, not a cycle
  count. Cycle-aligned GC and retention belong to #409.
- **No `query_log` GC** — `query_log_lookback_days` governs the rebuild SQL window only.
- **No Thompson Sampling** — deferred until after PPR baseline ICD is measured.
- **No gap detection** — Loop 3, a separate feature after the frequency table is operating.
- **No W3-1 GNN** — frequency table is the non-parametric predecessor; GNN deferred until
  CC@k ≥ 0.7.
- **No BM25 hybrid retrieval** — separate work item.
- **No backfill of phase=NULL rows** — pre-col-028 rows are filtered by `WHERE phase IS NOT NULL`.
- **No MCP tool or diagnostic endpoint** — `PhaseFreqTable` is internal state.
- **No change to `w_phase_histogram`** — session histogram term (crt-026) is unaffected.

## Background Research

### w_phase_explicit Placeholder (crt-026, ADR-003)

`search.rs:873`: `phase_explicit_norm: 0.0` — hardcoded regardless of phase.
`config.rs:441`: `w_phase_explicit: 0.0` — default zero, W3-1 placeholder.
`FusedScoreInputs.phase_explicit_norm` is a named, stable field. The field exists, the
weight exists, the formula includes it — only the signal source is missing.

ADR-003 (crt-026, Unimatrix #3163) deferred the signal to "W3-1". ASS-032 designates the
frequency table as the non-parametric predecessor that activates this placeholder.

### TypedGraphStateHandle Pattern

`services/typed_graph.rs` is the exact template:

- `TypedGraphState` struct wraps computed state.
- `TypedGraphStateHandle = Arc<RwLock<TypedGraphState>>` is the shared type.
- `TypedGraphState::new_handle()` creates the cold-start handle (`use_fallback = true`).
- `TypedGraphState::rebuild(store: &Store)` is called by the background tick; returns
  `Ok(new_state)` on success.
- The tick swaps via `*guard = new_state` under write lock.
- Hot path takes a short read lock, clones what it needs, releases before scoring.
- Poison recovery: `.unwrap_or_else(|e| e.into_inner())` on all lock acquisitions.

`EffectivenessState` adds a `generation: u64` counter for clone-avoidance. Defer this
optimization for `PhaseFreqTable` until profiling shows need.

### query_log Schema (col-028 / schema v17)

Actual `query_log` columns verified against `migration.rs` and `query_log.rs`:

```
query_id         INTEGER PRIMARY KEY AUTOINCREMENT
session_id       TEXT NOT NULL
query_text       TEXT NOT NULL
ts               INTEGER NOT NULL   -- Unix timestamp seconds (u64 in Rust, stored as i64)
result_count     INTEGER NOT NULL
result_entry_ids TEXT               -- JSON array of u64 entry IDs, e.g. [42, 7, 19]
similarity_scores TEXT
retrieval_mode   TEXT
source           TEXT NOT NULL
phase            TEXT               -- col-028: nullable, workflow phase at query time
```

Index: `idx_query_log_phase ON query_log (phase)` (col-028).
Index: `idx_query_log_ts ON query_log (ts)` (existing).

There is **no `feature_cycle` column** in `query_log`. Retention is time-based.

`result_entry_ids` is serialized by `serde_json::to_string(&[u64])` → unquoted JSON
integers, e.g. `[42,7,19]`. The `json_each` expansion form is
`CAST(json_each.value AS INTEGER)` (verified from `mcp/knowledge_reuse.rs` usage).

### SQL for the Frequency Table Rebuild

```sql
SELECT
    q.phase,
    e.category,
    CAST(je.value AS INTEGER)  AS entry_id,
    COUNT(*)                   AS freq
FROM query_log q
  CROSS JOIN json_each(q.result_entry_ids) AS je
  JOIN entries e ON CAST(je.value AS INTEGER) = e.id
WHERE q.phase IS NOT NULL
  AND q.result_entry_ids IS NOT NULL
  AND q.ts > strftime('%s', 'now') - ?1 * 86400
GROUP BY q.phase, e.category, CAST(je.value AS INTEGER)
ORDER BY q.phase, e.category, freq DESC
```

`?1` = `lookback_days as i64`. The `strftime('%s', 'now')` expression returns current
Unix time in seconds; `lookback_days * 86400` converts days to seconds.

No JOIN with `sessions` is required. No subquery over feature cycles.

### Normalization: Rank-Based

Access patterns are power-law distributed — a handful of entries dominate counts in any
given phase. Min-max (even with a floor) collapses most entries near-zero with one outlier
at 1.0, producing a degenerate PPR personalization vector. Rank-based spreads signal evenly
regardless of access count distribution, giving PageRank a richer gradient.

Formula: `score = 1.0 - (rank as f32 / N as f32)` where rank is 0-indexed within the
`(phase, category)` bucket, N = bucket size. Top entry (rank 0) → 1.0. Last entry → 1/N.
Single-entry bucket: N=1, rank=0 → 1.0. Absent entry → 1.0 (neutral).

### Cold-Start Semantics: Two Behaviors, One Method

`phase_affinity_score` returns `1.0` when `use_fallback = true`. This is the correct
neutral value for PPR (seed weight = hnsw_score × 1.0 = unmodified).

Fused scoring must NOT call `phase_affinity_score` when `use_fallback = true`. The
scoring hot-path checks `use_fallback` first and returns `phase_explicit_norm = 0.0`
directly. This preserves pre-col-031 score identity when no phase history exists.

```rust
// Before scoring loop — acquire lock once, extract snapshot, release:
let phase_snapshot = match &params.current_phase {
    None => None,  // lock never acquired
    Some(phase) => {
        let guard = freq_table.read().unwrap_or_else(|e| e.into_inner());
        if guard.use_fallback {
            None  // cold-start: treat as no phase — 0.0 contribution
        } else {
            Some(/* clone relevant bucket data */)
        }
    }
};

// In scoring loop:
let phase_explicit_norm: f64 = match &phase_snapshot {
    None => 0.0,
    Some(snapshot) => snapshot.affinity(entry.id, &entry.category) as f64,
};
```

### PPR Integration Contract (#398)

col-031 publishes `phase_affinity_score` as the API #398 will call:

```
personalization[v] = hnsw_score[v] * phase_affinity_score(v.id, v.category, current_phase)
```

Since #398 is not yet implemented, the function must exist as a public method and be
documented as the integration point. No PPR scaffolding goes into col-031.

### InferenceConfig Extension

`w_phase_explicit` is already in `InferenceConfig` and `FusionWeights`. Changes:
1. `default_w_phase_explicit()` → `0.05`.
2. FusionWeights sum-check comment updated: the six-weight sum (0.95) is unchanged;
   `w_phase_explicit` (0.05) is additive outside it. Total with defaults: `0.95 + 0.02 + 0.05 = 1.02`.
   Per ADR-004 (crt-026, Unimatrix #3175), additive phase terms do not enter the sum constraint.
3. `test_inference_config_default_phase_weights` updated to assert `0.05`.
4. New field: `query_log_lookback_days: u32`, default `30`.

### Eval Harness Gap

`eval/scenarios/extract.rs` does not select `query_log.phase`, so all eval scenarios have
`current_phase = None` → `phase_explicit_norm = 0.0` → AC-12 passes trivially (signal never
activated). This is a vacuous gate. Fix is bounded: add `current_phase` to scenario extraction
SQL; nothing else in `extract.rs` changes. This fix is in scope as AC-16. Without it, AC-12
cannot be a meaningful regression gate.

## Proposed Approach

### Module: `crates/unimatrix-server/src/services/phase_freq_table.rs`

New file (analogous to `services/typed_graph.rs`). Max 500 lines.

```rust
pub struct PhaseFreqTable {
    /// (phase, category) → Vec<(entry_id, rank_score)>, sorted descending.
    pub table: HashMap<(String, String), Vec<(u64, f32)>>,
    /// true on cold-start or when rebuild produced no rows.
    pub use_fallback: bool,
}

pub type PhaseFreqTableHandle = Arc<RwLock<PhaseFreqTable>>;

impl PhaseFreqTable {
    pub fn new() -> Self                         // use_fallback=true, empty table
    pub fn new_handle() -> PhaseFreqTableHandle
    pub async fn rebuild(store: &Store, lookback_days: u32) -> Result<Self, StoreError>
    /// Returns rank-based score ∈ [0.0, 1.0].
    /// Returns 1.0 when use_fallback=true, phase absent, or entry absent in bucket.
    pub fn phase_affinity_score(&self, entry_id: u64, entry_category: &str, phase: &str) -> f32
}
```

### Store Layer: `crates/unimatrix-store/src/query_log.rs`

New struct and method on `SqlxStore` (= `Store`):

```rust
pub struct PhaseFreqRow {
    pub phase: String,
    pub category: String,
    pub entry_id: u64,
    pub freq: i64,   // COUNT(*) — sqlx maps SQLite INTEGER to i64
}

impl SqlxStore {
    pub async fn query_phase_freq_table(&self, lookback_days: u32) -> Result<Vec<PhaseFreqRow>>
}
```

Uses `sqlx::query(...).bind(lookback_days as i64).fetch_all(self.read_pool()).await`.
Row deserialization via `row.try_get::<T, _>(index)` — same pattern as all existing
`query_log.rs` methods.

### ServiceLayer Integration

`PhaseFreqTable::new_handle()` called in `ServiceLayer::with_rate_config()`.
`Arc::clone`-d into `SearchService` and background tick. Same wiring as
`TypedGraphStateHandle`.

### Background Tick Integration

After `TypedGraphState::rebuild` in `run_single_tick`:
```rust
match PhaseFreqTable::rebuild(&store, config.query_log_lookback_days).await {
    Ok(new_table) => { *handle.write()... = new_table; }
    Err(e) => { tracing::error!(...); /* retain existing state */ }
}
```

### Scoring Wire-up (`search.rs`)

Before the scoring loop, acquire `PhaseFreqTableHandle` read lock once, check
`use_fallback`, extract the relevant bucket data, release the lock. Never hold the
lock across the scoring loop. When `use_fallback = true` or `current_phase = None`,
`phase_explicit_norm = 0.0` for all candidates.

### InferenceConfig Changes

- `w_phase_explicit` default: `0.0` → `0.05`
- New field: `query_log_lookback_days: u32`, default `30`
- `validate()`: unchanged (per-field range check already covers both fields)
- FusionWeights sum-check comment updated

## Acceptance Criteria

- **AC-01**: `PhaseFreqTable::new()` returns `use_fallback = true`, empty table.

- **AC-02**: `PhaseFreqTable::rebuild(store, lookback_days)` queries `query_log` rows
  where `phase IS NOT NULL`, `result_entry_ids IS NOT NULL`, and
  `ts > strftime('%s', 'now') - lookback_days * 86400`. Result is
  `HashMap<(String, String), Vec<(u64, f32)>>` keyed by `(phase, category)`, each Vec
  sorted descending by rank score.

- **AC-03**: `PhaseFreqTable::new_handle()` returns `Arc<RwLock<PhaseFreqTable>>` in
  cold-start state. All lock acquisitions use `.unwrap_or_else(|e| e.into_inner())`.

- **AC-04**: Background tick calls `PhaseFreqTable::rebuild` once per cycle. On success,
  handle swapped under write lock. On failure, existing state retained,
  `tracing::error!` emitted.

- **AC-05**: `ServiceLayer` creates `PhaseFreqTableHandle` and threads it to
  `SearchService` and background tick via `Arc::clone`.

- **AC-06**: In the fused scoring loop, `FusedScoreInputs.phase_explicit_norm` is:
  `0.0` when `current_phase = None` or `use_fallback = true`; otherwise computed from
  `phase_affinity_score`. The `use_fallback` check happens before `phase_affinity_score`
  is called — the lock is released before the scoring loop begins.

- **AC-07**: `phase_affinity_score(entry_id, entry_category, phase) -> f32` is public.
  Returns `f32 ∈ [0.0, 1.0]`. Returns `1.0` when `use_fallback = true`, phase absent,
  or entry absent in bucket.

- **AC-08**: Integration test using `TestDb`: seed `query_log` with 10 rows,
  `phase="delivery"`, `result_entry_ids=[42]`, `ts` within lookback window. After
  `rebuild`, assert `phase_affinity_score(42, "decision", "delivery") > 0.0` and
  `phase_affinity_score(99, "decision", "delivery") == 1.0`.

- **AC-09**: `InferenceConfig.w_phase_explicit` default is `0.05`. Config TOML with no
  `w_phase_explicit` key deserializes to `0.05`. Existing test
  `test_inference_config_default_phase_weights` updated to assert `0.05`.

- **AC-10**: `InferenceConfig.query_log_lookback_days` exists with default `30`. Config
  TOML with no `query_log_lookback_days` key deserializes to `30`.

- **AC-11**: Cold-start invariants — three separate unit tests:
  1. `current_phase = None`, populated table → `phase_explicit_norm = 0.0`, scores
     identical to pre-col-031.
  2. `current_phase = Some(phase)`, `use_fallback = true` → `phase_explicit_norm = 0.0`
     via `use_fallback` guard, scores identical to pre-col-031. The guard fires before
     `phase_affinity_score` is called.
  3. `phase_affinity_score` called directly on `use_fallback = true` table → returns
     `1.0` (PPR neutral multiplier contract).

- **AC-12**: Eval regression gate (requires AC-16 complete): MRR ≥ 0.35, CC@5 ≥ 0.2659,
  ICD ≥ 0.5340 (col-030 baselines). Gate is non-vacuous only when eval scenarios carry
  `current_phase` values. AC-12 must not be declared passing without AC-16.

- **AC-13**: Rank-based normalization: within each `(phase, category)` bucket,
  `score = 1.0 - (rank / N)` (0-indexed rank, N = bucket size). Top entry → 1.0,
  last entry → 1/N. Single-entry bucket → 1.0. Absent entry → 1.0.

- **AC-14**: Unit test: `PhaseFreqTable::rebuild` from synthetic `query_log` (via
  existing test fixtures) produces correct `(phase, category)` keying, correct
  descending rank order, and correct normalization values.

- **AC-15**: `services/phase_freq_table.rs` ≤ 500 lines. SQL aggregation in
  `unimatrix-store/src/query_log.rs`.

- **AC-16**: `eval/scenarios/extract.rs` selects `query_log.phase` and populates
  `current_phase` in emitted scenario output. Bounded change — nothing else in
  `extract.rs` is modified.

## Constraints

- **col-028 prerequisite**: `query_log.phase` (schema v17) confirmed shipped
  (gate-3c PASS 2026-03-26). Pre-col-028 rows have `phase = NULL`; filtered by
  `WHERE phase IS NOT NULL`.

- **sqlx 0.8**: The store layer uses `sqlx 0.8` with `features = ["sqlite", "runtime-tokio",
  "macros"]`. All SQL runs through `sqlx::query(...).bind(...).fetch_all(self.read_pool())`.
  SQLite INTEGER columns (including `COUNT(*)`) map to `i64` in sqlx — `PhaseFreqRow.freq`
  is `i64`.

- **No `feature_cycle` in `query_log`**: Retention is time-based (`ts` column).
  No JOIN with `sessions` table. No subquery over cycle identifiers.

- **Phase vocabulary is runtime strings**: no compile-time enum. Phase rename makes
  old key go cold; new key starts empty. Silent graceful degradation.

- **w_phase_explicit additive invariant**: outside the six-weight sum constraint.
  `validate()` unchanged. FusionWeights sum-check comment must be updated to reflect
  `0.95 + 0.02 + 0.05 = 1.02`.

- **Sole writer / lock ordering**: background tick is the sole writer of
  `PhaseFreqTableHandle`. Acquisition order: `EffectivenessStateHandle` →
  `TypedGraphStateHandle` → `PhaseFreqTableHandle`. Each lock acquired, data extracted,
  released before the next is acquired. Never hold any of these locks across the scoring
  loop.

- **500-line file limit**: `phase_freq_table.rs` ≤ 500 lines. SQL in `unimatrix-store`.

- **json_each form**: `CAST(json_each.value AS INTEGER)` — verified against
  `mcp/knowledge_reuse.rs`. No new SQLite extension required.

- **#398 not yet shipped**: `phase_affinity_score` is the published integration contract.
  col-031 must not implement PPR internals. Confirm #398 status at delivery start.

- **AC-12 / AC-16 non-separable**: AC-16 (eval harness fix) must be complete before
  AC-12 (regression gate) can be declared. Treating them as separate deliverables makes
  AC-12 vacuous.

## Decisions

| # | Decision | Rationale |
|---|----------|-----------|
| D-01 | Rank-based normalization: `1.0 - rank/N` | Power-law access distributions make min-max degenerate for PPR |
| D-02 | Time-based retention: `query_log_lookback_days = 30` | `query_log` has no `feature_cycle` column; #409 owns cycle-based GC |
| D-03 | `w_phase_explicit = 0.05` ships active | Cold-start guard ensures no effect until history exists; ships inert is a vacuous PPR gate |
| D-04 | Eval harness fix in scope (AC-16) | Without it AC-12 is a noise check, not a real gate |
| D-05 | Two cold-start values: `phase_affinity_score = 1.0`, fused scoring `= 0.0` | PPR needs neutral multiplier; fused scoring needs score identity with pre-col-031 |

## Tracking

GH Issue: #414
Draft PR: https://github.com/dug-21/unimatrix/pull/423
