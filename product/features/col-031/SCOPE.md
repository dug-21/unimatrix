# col-031: Phase-Conditioned Frequency Table

## Problem Statement

Phase is the highest-signal discrete feature for knowledge surfacing quality (ASS-032
RESEARCH-SYNTHESIS.md). The fused scoring formula has a placeholder term
`w_phase_explicit * phase_explicit_norm` that is hardcoded to `0.0` since crt-026
(ADR-003: "W3-1 reserved placeholder"). The PPR personalization vector (#398) uses
flat HNSW scores with no phase weighting. Neither the scoring formula nor the graph
traversal is phase-informed.

This creates a structural blindness: Unimatrix does not know that "during delivery
phase, entries of category lesson-learned are consistently useful" vs. "during scope
phase, decision entries dominate retrieval." Every query regardless of phase starts
from the same prior.

The frequency table is the non-parametric RA-DIT feedback loop (Loop 2,
RESEARCH-SYNTHESIS.md §Loop 2). It requires no training step, no model downloads, and
no new ML infrastructure — it is purely a SQL aggregation over `query_log` rows
(already populated with `phase` since col-028 / #397), rebuilt each background tick.
With col-028 shipped (schema v17, `query_log.phase` populated for all four read-side
tools since 2026-03-26), the prerequisite data is present.

Affected agents: every agent in a named workflow phase querying Unimatrix, who receives
phase-agnostic results today.

## Goals

1. Implement `PhaseFreqTable` — an in-memory struct holding
   `HashMap<(phase: String, category: String), Vec<(entry_id, f32)>>` sorted by
   descending frequency, rebuilt each background tick from `query_log`.
2. Implement `PhaseFreqTableHandle = Arc<RwLock<PhaseFreqTable>>` following the
   `TypedGraphStateHandle` / `EffectivenessStateHandle` pattern.
3. Wire the handle into `ServiceLayer` and thread it to `SearchService` and the
   background tick.
4. Activate `w_phase_explicit` in fused scoring: compute `phase_explicit_norm` from
   the frequency table at query time; raise the default from `0.0` to `0.05` in
   `InferenceConfig`.
5. Wire the frequency table into PPR personalization vector (#398): when PPR is
   called, scale each seed entry's HNSW score by `phase_affinity_score(entry, phase)`
   derived from the frequency table before normalization.
6. Expose `query_log_retention_cycles` in `InferenceConfig` to govern the lookback
   window for the frequency table SQL query (aligns with #409 retention framework
   without blocking on it).
7. Cold-start degrades gracefully: empty table produces `phase_explicit_norm = 0.0`
   for all candidates — score bit-for-bit identical to pre-col-031.

## Non-Goals

- **No changes to col-028 / query_log schema** — `query_log.phase` column already
  exists (schema v17). No new migrations.
- **No Thompson Sampling** — deferred (ROADMAP.md: "after PPR baseline ICD measured").
- **No gap detection** — separate feature (#409 covers retention; gap detection is
  Loop 3, a distinct feature after the frequency table is operating).
- **No PPR implementation** — this feature does not implement PPR (#398). It provides
  `phase_affinity_score` which #398 will consume. If #398 is not yet shipped, the
  PPR wire-up AC is not applicable; the scoring wire-up (Goals 4) stands independently.
- **No W3-1 GNN** — the frequency table is the non-parametric predecessor. W3-1 is
  deferred until CC@k ≥ 0.7 (ROADMAP.md).
- **No BM25 hybrid retrieval** — separate work item.
- **No `query_log` GC implementation** — `query_log_retention_cycles` in config is
  used for the lookback window only; the GC logic itself belongs to #409.
- **No backfill of `query_log` rows with phase=NULL** — pre-col-028 rows contribute
  zero signal (filtered by `WHERE phase IS NOT NULL`). This is expected.
- **No UI or diagnostic endpoint** — the table is internal state; no new MCP tool.
- **No change to `w_phase_histogram`** — session histogram term (crt-026) is unaffected.

## Background Research

### w_phase_explicit Placeholder (crt-026, ADR-003)

`search.rs` line 873: `phase_explicit_norm: 0.0` — hardcoded regardless of phase.
`config.rs` line 441: `w_phase_explicit: 0.0` — default zero, held as W3-1 placeholder.
`FusedScoreInputs.phase_explicit_norm` is a named, stable field that W3-1 depends on
(NFR-06 comment). The field exists, the weight exists, the formula includes it — only
the signal source is missing. This feature provides that signal source.

ADR-003 (crt-026, Unimatrix #3163) deferred the explicit phase signal to "W3-1"; the
ASS-032 roadmap (#414) now designates the frequency table as the W3-1 predecessor that
activates this placeholder non-parametrically.

### TypedGraphStateHandle Pattern (the rebuild-each-tick model to replicate)

`services/typed_graph.rs` is the exact template for `PhaseFreqTable`:

- `TypedGraphState` struct wraps the computed state.
- `TypedGraphStateHandle = Arc<RwLock<TypedGraphState>>` is the shared type.
- `TypedGraphState::new_handle()` creates the cold-start handle.
- `TypedGraphState::rebuild(store: &Store)` is called by the background tick each
  cycle; returns `Ok(new_state)` on success.
- The tick swaps via `*guard = new_state` under write lock.
- Hot path (search) takes a short read lock, clones what it needs, releases before
  any scoring work.
- Poison recovery: `.unwrap_or_else(|e| e.into_inner())` on all lock acquisitions
  (consistent with `EffectivenessStateHandle` and `CategoryAllowlist`).
- Cold-start: empty state, graceful degradation. `EffectivenessState` is the reference
  for generation-counter optimization if HashMap clones become expensive.

`EffectivenessState` adds a `generation: u64` counter for clone-avoidance via
`EffectivenessSnapshot` (Unimatrix #1561). The frequency table may benefit from the
same optimization but it is out of scope for this feature (defer until profiling shows
need).

### query_log Schema (col-028 / schema v17)

`query_log` table columns (post-col-028): `query_id`, `session_id`, `query_text`,
`result_entry_ids` (JSON), `top_similarity`, `timestamp`, `source`, `feature_cycle`,
`phase` (TEXT, nullable).

Index: `idx_query_log_phase ON query_log (phase)`.

Relevant for the table build SQL:
```sql
SELECT phase, entry_id, COUNT(*) as freq
FROM query_log
  CROSS JOIN json_each(result_entry_ids)      -- entry_id from JSON array
  JOIN entries ON entries.id = json_each.value::INTEGER
WHERE phase IS NOT NULL
  AND feature_cycle IN (last K completed cycles OR open cycles)
GROUP BY phase, entries.category, entries.id
ORDER BY phase, entries.category, freq DESC
```

The `query_log_retention_cycles` value governs how many completed cycles are
included. This aligns with #409 (retention framework) which defines K=20 as the
suggested default.

Note: `result_entry_ids` is stored as a JSON array of integers. The `json_each`
expansion is the access pattern used elsewhere in the codebase (see
`mcp/knowledge_reuse.rs`).

### Background Tick Pattern

`background.rs` shows the rebuild sequence: maintenance → graph compaction →
`TypedGraphState::rebuild` → `EffectivenessState` update → NLI detection tick.
The `PhaseFreqTable` rebuild belongs in this sequence, after the `TypedGraphState`
rebuild (no dependency ordering required, but convention places analytical state
after structural state).

The tick is bounded by `TICK_TIMEOUT` (`tokio::time::timeout`). The frequency table
rebuild is a SQL aggregation, not a full graph traversal. At 20K query_log rows it
completes in < 5ms. A separate timeout is not necessary; the existing tick timeout
applies.

### PPR Personalization Vector (#398 integration point)

GH #398 specifies:
```
personalization[v] = hnsw_score[v] if v ∈ candidates, else 0.0
normalize personalization
```

The col-031 wire-up adds:
```
personalization[v] = hnsw_score[v] * phase_affinity_score(v, current_phase)
```

where `phase_affinity_score(entry_id, phase)` is derived from the frequency table:
if `(phase, entry.category)` is in the table and `entry_id` appears in the Vec,
return a normalized score in [0, 1]; otherwise return 1.0 (neutral, no penalty on
cold start). This matches the cold-start invariant.

Since #398 is not yet implemented, the PPR wire-up AC (AC-07, AC-08) is conditional:
the `phase_affinity_score` function must be implemented as a public API on
`PhaseFreqTable`, and the call site in #398 is left as a documented integration point.

### InferenceConfig Extension Pattern (crt-026 / Unimatrix #3206, #3207)

`w_phase_explicit` is already in `InferenceConfig` and `FusionWeights`. Raising the
default from 0.0 to 0.05 requires:
1. Changing `default_w_phase_explicit()` → 0.05.
2. Updating the `validate()` sum-check comment (the six-weight sum is unchanged;
   `w_phase_explicit` remains an additive term outside the constraint).
3. Updating the test `test_inference_config_default_phase_weights` to assert 0.05.
4. No `validate()` logic change — per-field range [0.0, 1.0] check already handles it.

ADR-004 (crt-026, Unimatrix #3175) established that additive phase terms do not enter
the six-weight sum constraint. The total with defaults becomes: 0.95 + 0.02 + 0.05 =
1.02 — still within per-field range but the sum comment in FusionWeights must be
updated.

### Retention Framework Alignment (#409)

GH #409 specifies `query_log_retention_cycles = 20` (K) as the single parameter
governing both GC and frequency table lookback. This feature adds the config field
`query_log_retention_cycles: u32` to `InferenceConfig` with default 20. The GC
implementation belongs to #409. The frequency table SQL uses this value as the
lookback bound — the constraint documented in Unimatrix #3414 ("K is the hard ceiling
for phase-conditioned frequency table lookback and GNN training reconstruction window")
is satisfied by sharing the same config parameter.

### Eval Harness Baseline

Current baseline (col-030, 2026-03-27):
- P@5: 0.2874, MRR: 0.4007, CC@5: 0.2659, ICD: 0.5340
- PPR gate: CC@5 ≥ 0.3659, ICD improvement, MRR ≥ 0.35

col-031 activates `w_phase_explicit = 0.05`. This is a distribution-changing
scoring modification. The distribution gate (#402) must be applied: CC@5 and ICD must
not regress from baseline; MRR floor ≥ 0.35. Because the frequency table starts empty
on fresh data, the eval harness replay uses phase-aware scenarios; results with
`query_log.phase = NULL` produce `phase_explicit_norm = 0.0` (no signal, safe).

## Proposed Approach

### Module: `services/phase_freq_table.rs`

New file (analogous to `services/typed_graph.rs`). Exports:

```rust
pub struct PhaseFreqTable {
    /// (phase, category) -> Vec<(entry_id, score_f32)>, sorted desc by score.
    /// Empty on cold-start; rebuilt each tick.
    pub table: HashMap<(String, String), Vec<(u64, f32)>>,
    pub use_fallback: bool,  // true = empty / cold-start
}

pub type PhaseFreqTableHandle = Arc<RwLock<PhaseFreqTable>>;

impl PhaseFreqTable {
    pub fn new() -> Self  // cold-start empty, use_fallback=true
    pub fn new_handle() -> PhaseFreqTableHandle
    pub async fn rebuild(store: &Store, retention_cycles: u32) -> Result<Self, StoreError>
    /// phase_affinity_score: normalized [0.0, 1.0] frequency score for an entry in a phase.
    /// Returns 1.0 (neutral) when table is empty, phase absent, or entry absent in phase.
    pub fn phase_affinity_score(&self, entry_id: u64, entry_category: &str, phase: &str) -> f32
}
```

The `rebuild` method executes the SQL aggregation. The `phase_affinity_score` method
is the integration API for #398 PPR and for `compute_phase_explicit_norm` in search.

### Store Layer: `query_log_phase_freq` query

New method on `Store`: `query_phase_freq_table(retention_cycles: u32)` → async result
of `Vec<(phase, category, entry_id, freq_count)>`. Implemented in `unimatrix-store`
as a SQL aggregation. The JSON expansion of `result_entry_ids` requires `json_each`.

### ServiceLayer integration

`PhaseFreqTable::new_handle()` called in `ServiceLayer::with_rate_config()`. The
handle is `Arc::clone`-d into `SearchService` and the background tick, following
the identical pattern used for `TypedGraphStateHandle`.

### Background tick integration

After `TypedGraphState::rebuild` in `run_single_tick`: call
`PhaseFreqTable::rebuild(store, config.query_log_retention_cycles).await`, swap
handle under write lock on success, log error and retain old state on failure.

### Scoring wire-up

In the fused scoring loop in `search.rs`, replace the hardcoded `phase_explicit_norm: 0.0`
with a computed value:

```rust
let phase_explicit_norm = if let Some(phase) = &params.current_phase {
    freq_table.phase_affinity_score(entry.id, &entry.category, phase) as f64
} else {
    0.0
};
```

`params.current_phase` is the snapshot taken at the start of `handle_search` (already
in `ServiceSearchParams` from crt-025/crt-026). The `PhaseFreqTableHandle` is read
once before the scoring loop, same pattern as `category_histogram`.

### InferenceConfig changes

- `w_phase_explicit` default: 0.0 → 0.05
- New field: `query_log_retention_cycles: u32`, default 20
- `validate()`: no sum-constraint changes needed; per-field range check already covers

## Acceptance Criteria

- AC-01: `PhaseFreqTable::new()` returns `use_fallback = true` and an empty `table`.
  `phase_affinity_score(any_entry_id, any_category, any_phase)` returns 1.0 (neutral)
  when `use_fallback = true`.

- AC-02: `PhaseFreqTable::rebuild(store, retention_cycles)` queries `query_log` rows
  where `phase IS NOT NULL` and the feature_cycle is within the last `retention_cycles`
  completed cycles (or is an open cycle). The result is a `HashMap<(String, String),
  Vec<(u64, f32)>>` keyed by `(phase, category)`, entries sorted descending by
  frequency score.

- AC-03: `PhaseFreqTable::new_handle()` returns `Arc<RwLock<PhaseFreqTable>>` with a
  cold-start state. Poison recovery uses `.unwrap_or_else(|e| e.into_inner())`.

- AC-04: The background tick calls `PhaseFreqTable::rebuild` once per tick cycle.
  On success, the handle is swapped under write lock. On failure, the existing state
  is retained and the error is logged via `tracing::error!`.

- AC-05: `ServiceLayer` creates the `PhaseFreqTableHandle` and threads it to both
  `SearchService` and the background tick via `Arc::clone`.

- AC-06: In the fused scoring loop, `FusedScoreInputs.phase_explicit_norm` is computed
  from `PhaseFreqTable::phase_affinity_score` when `params.current_phase` is `Some`.
  When `params.current_phase` is `None`, `phase_explicit_norm = 0.0`.

- AC-07: `PhaseFreqTable::phase_affinity_score` is a public method callable from the
  PPR implementation (#398). It accepts `(entry_id: u64, entry_category: &str,
  phase: &str)` and returns `f32 ∈ [0.0, 1.0]`. Returns 1.0 when the table is in
  cold-start, phase is absent, or entry_id is not in the top entries for
  `(phase, category)`.

- AC-08: Integration test: given a `query_log` with 10 rows for phase="delivery",
  category="decision", all pointing to entry ID 42, `PhaseFreqTable::rebuild` returns
  a table where `phase_affinity_score(42, "decision", "delivery") > 0.0` and
  `phase_affinity_score(99, "decision", "delivery") == 1.0` (neutral for absent entry).

- AC-09: `InferenceConfig.w_phase_explicit` default is `0.05` (raised from `0.0`).
  Config TOML with no `w_phase_explicit` key deserializes to `0.05`. Existing test
  `test_inference_config_default_phase_weights` updated to assert `0.05`.

- AC-10: `InferenceConfig.query_log_retention_cycles` field exists with default `20`.
  Config TOML with no `query_log_retention_cycles` deserializes to `20`.

- AC-11: Cold-start invariant: when `PhaseFreqTable` is empty (cold-start or empty
  `query_log`), all `phase_explicit_norm` values in the scoring loop are `0.0`, and
  `compute_fused_score` output is bit-for-bit identical to pre-col-031 behavior when
  `w_phase_explicit * 0.0 = 0.0`.

- AC-12: Eval regression gate: after col-031 is implemented, run the eval harness.
  MRR ≥ 0.35 (floor), CC@5 must not decrease from 0.2659 baseline, ICD must not
  decrease from 0.5340 baseline.

- AC-13: `phase_affinity_score` normalization: scores are normalized within each
  `(phase, category)` bucket so the maximum score in a bucket maps to 1.0 and
  minimum maps to 0.0. Entries absent from the bucket return 1.0 (neutral, not 0.0).

- AC-14: Unit test: frequency table rebuild from a synthetic `query_log` (via test
  support fixtures, not isolated scaffolding) produces correct `(phase, category)`
  keying, correct entry ranking, and correct normalization.

- AC-15: The `PhaseFreqTable` module is in `services/phase_freq_table.rs` and does
  not exceed 500 lines. The store query method is in `unimatrix-store` at the
  appropriate query layer.

## Constraints

- **col-028 must be shipped**: `query_log.phase` column (schema v17) must be present.
  Confirmed shipped (gate-3c PASS 2026-03-26, 3629 tests pass). Pre-col-028 rows
  have `phase = NULL` and are filtered by `WHERE phase IS NOT NULL`.

- **Phase vocabulary is runtime strings**: phase keys in the frequency table are string
  values from `query_log.phase` — whatever the session's `current_phase` was at call
  time. No compile-time phase enum. Renaming a phase = old phase key goes cold,
  new phase key starts empty. Domain-agnostic invariant (ASS-032).

- **w_phase_explicit additive invariant**: The six-weight sum constraint
  (`w_sim + w_nli + w_conf + w_coac + w_util + w_prov ≤ 1.0`) does not include
  `w_phase_explicit`. The total with new default becomes 0.95 + 0.02 + 0.05 = 1.02.
  FusionWeights sum-check and NLI-absent denominator exclude this field per ADR-004
  (crt-026, Unimatrix #3206). No change to `validate()` sum constraint required.

- **Sole writer contract**: The background tick is the sole writer of the
  `PhaseFreqTableHandle`. Search hot path is read-only. Lock ordering: never hold
  `PhaseFreqTableHandle` write lock while acquiring any other lock.

- **500-line file limit**: `services/phase_freq_table.rs` must stay under 500 lines.
  The SQL aggregation query moves to `unimatrix-store` as a dedicated method.

- **json_each availability**: The SQL aggregation must expand `result_entry_ids` (JSON
  array). SQLite's `json_each` is available; the existing codebase uses it (see
  `mcp/knowledge_reuse.rs`). No new SQLite extension needed.

- **#398 PPR not yet implemented**: AC-07 specifies the API surface for PPR
  integration, but PPR itself is a separate feature. col-031 must not block on #398,
  and #398 must not block on col-031. The `phase_affinity_score` method is the
  published integration contract.

- **Eval baseline**: The distribution gate (#402) is in place (ROADMAP.md ✅). Eval
  must be run post-implementation. Activate `w_phase_explicit = 0.05` only after
  confirming AC-12.

- **File path**: new module at
  `crates/unimatrix-server/src/services/phase_freq_table.rs`.

## Open Questions

1. **Normalization strategy for `phase_affinity_score`**: **DECIDED: rank-based.**
   Access patterns are power-law distributed — a handful of entries dominate counts in
   any given phase. Min-max (even with a 0.5 floor) collapses most entries to the floor
   with one or two outliers near 1.0, producing a degenerate PPR personalization vector
   (almost uniform with a spike). Rank-based spreads signal evenly across the bucket
   regardless of access count distribution, giving PageRank a richer gradient to
   traverse. Formula: `score = 1.0 - (rank / N)` where rank is 0-indexed (top entry
   rank=0 → score=1.0). Entries absent from the bucket return 1.0 (neutral).

2. **SQL query correctness for json_each expansion**: `result_entry_ids` is stored as
   a JSON array of integer entry IDs. The precise SQL form for `json_each` with SQLite
   needs confirmation against the actual stored format (integer vs. string elements in
   the JSON array). This should be verified against a real `query_log` row during
   implementation.

3. **Lookup join shape**: The rebuild query needs `entries.category` to key the
   `(phase, category)` bucket. This requires joining `query_log` (for phase, entry IDs
   via json_each) with `entries` (for category). Confirm this join is within the
   `unimatrix-store` query layer's access pattern and does not cross crate boundaries
   inappropriately.

4. **Tick placement within run_single_tick**: Should the `PhaseFreqTable` rebuild run
   before or after the `TypedGraphState` rebuild? There is no ordering dependency.
   Convention in the tick places structural state (graph) before analytical state
   (effectiveness, frequency). Confirm this placement is acceptable.

5. **Eval harness phase data availability**: **DECIDED: fix in col-031 scope.**
   `eval/scenarios/extract.rs` does not select `query_log.phase` (known gap, Unimatrix
   #3555). Without this fix, AC-12 passes trivially — not because the signal is
   neutral, but because it was never activated. That is a vacuous gate. The #398 PPR
   gate requires the frequency table active; if active is untestable, that prerequisite
   check is meaningless. The extract.rs change is bounded: add `current_phase` to
   scenario extraction, nothing else. AC-12 becomes a real regression gate rather than
   a noise check. Add AC-16 to acceptance criteria covering the eval harness fix.

6. **w_phase_explicit = 0.05 weight calibration**: **DECIDED: configurable default,
   ship active at 0.05.** Cold-start degradation already handles sparse-data risk:
   empty table → near-uniform weights → `0.05 × near-uniform ≈ 0` net effect on
   ranking. Real signal only emerges when query history is meaningful — which is
   precisely when the weight should be live. Shipping inert (0.0) would make the PPR
   gate's frequency-table prerequisite vacuous. Weight is larger than `w_phase_histogram`
   (0.02), which is correct: frequency table is a durable cross-session signal vs. the
   ephemeral session histogram. Configurable via `InferenceConfig` for operator tuning.

## Tracking

GH Issue: #414. Will be updated with GH Issue link after Session 1.
