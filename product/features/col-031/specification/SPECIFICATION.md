# SPECIFICATION: col-031 — Phase-Conditioned Frequency Table

## Objective

The Unimatrix fused scoring formula contains a `w_phase_explicit * phase_explicit_norm` term
that has been hardcoded to `0.0` since crt-026 (ADR-003: W3-1 reserved placeholder). This
feature activates that placeholder by implementing `PhaseFreqTable` — an in-memory,
background-rebuilt frequency table keyed by `(phase, category)` that provides rank-normalized
affinity scores for use in fused scoring and PPR personalization. The feature also fixes a gap
in the eval harness where `extract.rs` omitted `current_phase` from scenario extraction,
which would otherwise make the scoring regression gate vacuous.

---

## Functional Requirements

**FR-01** — A new module `crates/unimatrix-server/src/services/phase_freq_table.rs` must define
`PhaseFreqTable`, a struct holding `table: HashMap<(String, String), Vec<(u64, f32)>>` keyed by
`(phase, category)`, each Vec sorted descending by frequency score. The struct must also hold
`use_fallback: bool`, set `true` on cold-start.

**FR-02** — `PhaseFreqTable::new()` must return a cold-start instance: `use_fallback = true`,
`table` empty.

**FR-03** — `PhaseFreqTable::new_handle()` must return `Arc<RwLock<PhaseFreqTable>>` initialized
from `PhaseFreqTable::new()`. All lock acquisitions on the handle must use
`.unwrap_or_else(|e| e.into_inner())` for poison recovery, consistent with
`EffectivenessStateHandle` and `CategoryAllowlist` (codebase-wide convention).

**FR-04** — `PhaseFreqTable::rebuild(store: &Store, retention_cycles: u32) -> Result<Self, StoreError>`
must execute a SQL aggregation over `query_log` rows filtered to `WHERE phase IS NOT NULL` and
within the last `retention_cycles` completed feature cycles (plus any currently open cycle). The
result must be materialized into `table` with `use_fallback = false` on success.

**FR-05** — The SQL aggregation in FR-04 must expand `result_entry_ids` (a JSON integer array)
via SQLite's `json_each`, join with `entries` for `category`, group by `(phase, category,
entry_id)`, and count occurrences as the raw frequency signal. The exact `json_each` SQL form
must be verified against a real `query_log` row at implementation time (SR-01 constraint).

**FR-06** — The SQL aggregation method must reside in `unimatrix-store` as a dedicated query
method `Store::query_phase_freq_table(retention_cycles: u32)`, returning
`Vec<(String, String, u64, i64)>` — `(phase, category, entry_id, freq_count)`. It must not
cross crate boundaries inappropriately: only `unimatrix-store` owns direct SQL access.

**FR-07** — `PhaseFreqTable::phase_affinity_score(entry_id: u64, entry_category: &str, phase: &str) -> f32`
must be a public method implementing rank-based normalization: within each `(phase, category)`
bucket, `score = 1.0 - (rank / N)` where `rank` is 0-indexed (top entry: rank 0, score 1.0;
last entry: rank N-1, score 1/N). Entries absent from the bucket must return `1.0` (neutral —
no cold-start penalty). When `use_fallback = true`, all inputs must return `1.0`.

**FR-08** — `ServiceLayer::with_rate_config()` (or equivalent initialization point) must call
`PhaseFreqTable::new_handle()` and thread the resulting `PhaseFreqTableHandle` to both
`SearchService` and the background tick via `Arc::clone`, following the identical pattern used
for `TypedGraphStateHandle`.

**FR-09** — The background tick (`run_single_tick` in `background.rs`) must call
`PhaseFreqTable::rebuild(store, config.query_log_retention_cycles).await` once per tick cycle,
after the `TypedGraphState::rebuild` step (analytical state follows structural state by
convention). On success, the handle must be swapped under write lock: `*guard = new_state`. On
failure, the existing state must be retained and the error logged via `tracing::error!`.

**FR-10** — In `search.rs`, the hardcoded `phase_explicit_norm: 0.0` assignment must be
replaced with a computed value: when `params.current_phase` is `Some(phase)`, call
`freq_table.phase_affinity_score(entry.id, &entry.category, phase) as f64`; when
`params.current_phase` is `None`, use `0.0`. The `PhaseFreqTableHandle` must be read once
before the scoring loop (short read lock, clone what is needed, release before scoring work)
following the same pattern as `category_histogram`.

**FR-11** — `InferenceConfig.w_phase_explicit` default must be changed from `0.0` to `0.05`.
`InferenceConfig.query_log_retention_cycles: u32` must be added with default `20`. Both fields
must deserialize correctly from a TOML file that omits them (serde defaults).

**FR-12** — The existing test `test_inference_config_default_phase_weights` must be updated to
assert `w_phase_explicit == 0.05`. No other `validate()` sum-constraint logic requires change:
`w_phase_explicit` is additive outside the six-weight constraint per ADR-004 (crt-026, Unimatrix
#3175).

**FR-13** — `eval/scenarios/extract.rs` must be updated to include `current_phase` in scenario
extraction from `query_log`. The change is bounded: add the column selection and populate the
`current_phase` field in the emitted scenario; nothing else in `extract.rs` changes.

**FR-14** — `PhaseFreqTable::phase_affinity_score` must be unit-tested via existing test support
fixtures (not isolated scaffolding). Required cases:
  - Cold-start (`use_fallback = true`): any inputs return `1.0`.
  - Single-entry bucket: entry present returns `1.0` (rank 0 of N=1: `1.0 - 0/1 = 1.0`).
  - Multi-entry bucket, rank-based scores: top entry returns `1.0`, others follow formula.
  - Absent entry in a populated bucket: returns `1.0` (neutral).
  - Absent phase: returns `1.0` (neutral).

**FR-15** — An integration test must verify `PhaseFreqTable::rebuild` against a synthetic
`query_log` (seeded via test support fixtures): given 10 rows for `phase="delivery"`,
`category="decision"`, all pointing to `entry_id=42`, the rebuild must produce a table where
`phase_affinity_score(42, "decision", "delivery") > 0.0` and
`phase_affinity_score(99, "decision", "delivery") == 1.0`.

**FR-16** — `services/phase_freq_table.rs` must not exceed 500 lines. The SQL aggregation
belongs in `unimatrix-store`; the module contains only in-memory state and pure computation.

---

## Non-Functional Requirements

**NFR-01** — **Performance — hot path**: The `PhaseFreqTableHandle` read lock on the search hot
path must be held for the minimum duration: acquire, clone the needed slice or score, release,
then compute. No I/O or allocation beyond the score lookup may occur while the lock is held.

**NFR-02** — **Performance — tick rebuild**: At representative `query_log` sizes (up to 20 000
rows), `PhaseFreqTable::rebuild` must complete within the existing `TICK_TIMEOUT` budget
without requiring its own inner timeout. Tick wall-time must not materially increase after
col-031 ships (SR-07 concern). If profiling shows tick wall-time increases by more than 10%
at 20 000 rows, the rebuild SQL must be optimized before shipping.

**NFR-03** — **Cold-start invariant**: When `PhaseFreqTable` is in cold-start (empty table or
server just started), all `phase_explicit_norm` values in the scoring loop are `0.0`, and
`compute_fused_score` output is bit-for-bit identical to pre-col-031 behavior. This is
guaranteed by: `use_fallback = true` → `phase_affinity_score` returns `1.0` (neutral) →
`w_phase_explicit * 1.0 * 0.0` — wait. Correction: when `use_fallback = true`,
`phase_affinity_score` returns `1.0`, but `phase_explicit_norm` is used directly in the fused
score, not multiplied by affinity again. The cold-start invariant holds because: empty table →
`use_fallback = true` → `phase_affinity_score` returns `1.0` for all entries → all
`phase_explicit_norm` values are equal → `w_phase_explicit * (uniform constant)` adds a
constant offset to all candidates, preserving relative ranking. At `w_phase_explicit = 0.05`
and all `phase_explicit_norm = 1.0`, the absolute scores shift uniformly by `0.05` but
relative ordering is unchanged. When the table is populated, differentiated scores emerge.
The exact "bit-for-bit identical" guarantee requires: at empty table, all candidates receive
`phase_explicit_norm = 0.0`. This is satisfied because `params.current_phase = None` →
`phase_explicit_norm = 0.0` (FR-10), and pre-col-031 behavior required no `current_phase` set.
When `current_phase` is set and table is cold-start, scores shift uniformly — not bit-for-bit
identical but ranking-preserving. Implementation must document this distinction.

**NFR-04** — **Lock ordering — sole-writer contract**: The background tick is the sole writer
of `PhaseFreqTableHandle`. Search hot path is read-only. The lock acquisition order in the
background tick must never hold a `PhaseFreqTableHandle` write lock while acquiring any other
write lock (SR-06). The architect must document the full lock acquisition order for
`PhaseFreqTableHandle`, `TypedGraphStateHandle`, and `EffectivenessStateHandle` in the
architecture doc.

**NFR-05** — **Regression gate non-vacuous**: AC-12 regression gate is only meaningful when
AC-16 (eval harness `extract.rs` fix) is complete. AC-12 must not be declared passing if
`current_phase` is absent from eval scenarios (see SR-03). The two acceptance criteria form a
single non-separable deliverable.

**NFR-06** — **No new ML infrastructure**: The frequency table requires no model downloads,
training steps, or ML runtimes. It is purely a SQL aggregation + in-memory map.

**NFR-07** — **No schema migration**: `query_log.phase` (schema v17, col-028) is already
present. No new tables, columns, or migrations are introduced by col-031.

**NFR-08** — **Observability — silent degradation contract**: Phase vocabulary is runtime
strings. A phase name mismatch between `current_phase` and `query_log.phase` (case difference,
renamed phase) silently degrades to cold-start (neutral score) for that phase. This is
expected behavior; no error is raised. The architect must decide whether a `tracing::debug!`
log line is emitted when a `current_phase` value has no match in the table (SR-04). At
minimum, the silent-degradation contract must be documented in code comments.

**NFR-09** — **`w_phase_explicit` additive invariant**: The six-weight sum constraint
(`w_sim + w_nli + w_conf + w_coac + w_util + w_prov ≤ 1.0`) excludes `w_phase_explicit`. The
sum with new default becomes `0.95 + 0.02 + 0.05 = 1.02`. The `FusionWeights` sum-check
comment must be updated; `validate()` logic is unchanged (per-field range `[0.0, 1.0]` already
covers 0.05).

**NFR-10** — **No new crate dependencies**: The implementation uses only SQLite (via existing
`rusqlite`/`sqlx`), `tokio` (existing), and standard `std::collections::HashMap`. No new crates
may be added.

---

## Acceptance Criteria

**AC-01** — `PhaseFreqTable::new()` returns `use_fallback = true` and an empty `table`.
`phase_affinity_score(any_entry_id, any_category, any_phase)` returns `1.0` when
`use_fallback = true`.
*Verification*: unit test — call `PhaseFreqTable::new()`, assert `use_fallback == true`, assert
`phase_affinity_score` returns `1.0` for arbitrary inputs.

**AC-02** — `PhaseFreqTable::rebuild(store, retention_cycles)` queries `query_log` rows where
`phase IS NOT NULL` and feature_cycle is within the last `retention_cycles` completed cycles or
is an open cycle. The result is a `HashMap<(String, String), Vec<(u64, f32)>>` keyed by
`(phase, category)`, with each Vec sorted descending by frequency score.
*Verification*: integration test (FR-15) — synthetic `query_log` with known rows; assert correct
keying, correct entry ranking, `use_fallback == false` on returned struct.

**AC-03** — `PhaseFreqTable::new_handle()` returns `Arc<RwLock<PhaseFreqTable>>` initialized to
cold-start. All lock acquisitions use `.unwrap_or_else(|e| e.into_inner())`.
*Verification*: code review confirms poison recovery pattern on all lock sites in
`phase_freq_table.rs`.

**AC-04** — The background tick calls `PhaseFreqTable::rebuild` once per tick cycle. On success,
the handle is swapped under write lock. On failure, the existing state is retained and the error
is logged via `tracing::error!`.
*Verification*: code review confirms tick integration; existing tick-level integration tests pass
without modification; error path confirmed by injecting a store error in a test and asserting
the old state is retained.

**AC-05** — `ServiceLayer` creates `PhaseFreqTableHandle` and threads it to both `SearchService`
and the background tick via `Arc::clone`.
*Verification*: code review confirms handle initialization and threading in `ServiceLayer`;
`cargo test --workspace` passes.

**AC-06** — In the fused scoring loop, `FusedScoreInputs.phase_explicit_norm` is computed from
`PhaseFreqTable::phase_affinity_score` when `params.current_phase` is `Some`. When
`params.current_phase` is `None`, `phase_explicit_norm = 0.0`.
*Verification*: code review confirms replacement of hardcoded `phase_explicit_norm: 0.0` in
`search.rs`; unit test with `current_phase = None` asserts `phase_explicit_norm == 0.0` in
`FusedScoreInputs`; unit test with `current_phase = Some("delivery")` and a populated table
asserts `phase_explicit_norm != 0.0` for a known top-ranked entry.

**AC-07** — `PhaseFreqTable::phase_affinity_score` is a public method callable from the PPR
implementation (#398). Signature: `(entry_id: u64, entry_category: &str, phase: &str) -> f32`.
Returns `f32 ∈ [0.0, 1.0]` for all inputs. Returns `1.0` when cold-start, phase absent, or
entry absent in the `(phase, category)` bucket.
*Verification*: unit tests for cold-start, absent-phase, absent-entry cases (FR-14); confirm
return type and visibility via code review; confirm no panic on arbitrary inputs via property
test or exhaustive case coverage.

**AC-08** — Integration test: synthetic `query_log` with 10 rows for `phase="delivery"`,
`category="decision"`, all pointing to `entry_id=42`. After `PhaseFreqTable::rebuild`:
`phase_affinity_score(42, "decision", "delivery") > 0.0` and
`phase_affinity_score(99, "decision", "delivery") == 1.0` (neutral for absent entry).
*Verification*: integration test using existing test fixtures (not isolated scaffolding);
assertions as stated.

**AC-09** — `InferenceConfig.w_phase_explicit` default is `0.05`. Config TOML with no
`w_phase_explicit` key deserializes to `0.05`. Existing test
`test_inference_config_default_phase_weights` updated to assert `0.05`.
*Verification*: updated test passes; TOML round-trip test (omit field, deserialize, assert value).

**AC-10** — `InferenceConfig.query_log_retention_cycles` field exists with default `20`. Config
TOML with no `query_log_retention_cycles` key deserializes to `20`.
*Verification*: unit test — deserialize empty TOML section, assert `query_log_retention_cycles == 20`.

**AC-11** — Cold-start ranking-preservation invariant: when `params.current_phase` is `None`
(no phase in request), all `phase_explicit_norm` values in the scoring loop are `0.0`, and
`compute_fused_score` output is numerically identical to pre-col-031 behavior.
*Verification*: unit test — run scoring with `current_phase = None` and a populated
`PhaseFreqTable`; assert `phase_explicit_norm == 0.0` for all candidates; assert final scores
match pre-col-031 reference values (captured from col-030 baseline).

**AC-12** — Eval regression gate: after col-031 implementation, run the eval harness with
phase-aware scenarios (requires AC-16 complete). Results must satisfy: MRR ≥ 0.35 (floor),
CC@5 ≥ 0.2659 (col-030 baseline), ICD ≥ 0.5340 (col-030 baseline). The gate is non-vacuous
only when AC-16 is complete and eval scenarios carry `current_phase` values.
*Verification*: eval harness run; `render_distribution_gate` report shows all three metrics
meeting or exceeding thresholds; AC-16 must be confirmed complete before this gate is declared
passing.

**AC-13** — `phase_affinity_score` normalization: rank-based formula `score = 1.0 - (rank / N)`
where `rank` is 0-indexed within the `(phase, category)` bucket. Top entry (rank 0) → `1.0`.
Bottom entry (rank N-1) → `1/N` (approaches `0.0` for large N). Absent entry → `1.0`.
*Verification*: unit test with a 5-entry bucket: assert entry at rank 0 returns `1.0`, rank 1
returns `0.8`, rank 4 returns `0.2`; assert entry not in bucket returns `1.0`.

**AC-14** — Unit test: frequency table rebuild from a synthetic `query_log` (via test support
fixtures, not isolated scaffolding) produces: correct `(phase, category)` keying, correct
entry ranking (most-frequent entry at index 0), correct rank-based normalization.
*Verification*: test seeded with known frequency counts; assertions on `table` structure and
`phase_affinity_score` return values.

**AC-15** — `services/phase_freq_table.rs` does not exceed 500 lines. The store query method
is in `unimatrix-store` at the appropriate query layer.
*Verification*: `wc -l crates/unimatrix-server/src/services/phase_freq_table.rs` ≤ 500;
code review confirms SQL query in `unimatrix-store`, not in `phase_freq_table.rs`.

**AC-16** — `eval/scenarios/extract.rs` emits `current_phase` in extracted scenarios:
`query_log.phase` is selected and populated in the scenario output. AC-12 regression gate is
non-vacuous as a result — scenarios carry phase values matching what col-031 scoring will read.
*Verification*: code review confirms `extract.rs` selects `phase` from `query_log`; inspection
of generated scenario JSONL confirms `current_phase` field is present and non-null for
post-col-028 rows; AC-12 eval run shows phase-conditioned scoring is active (non-zero
`phase_explicit_norm` in scored candidates when phase is populated).

---

## Domain Models

### Entities

**PhaseFreqTable** — In-memory struct holding the phase-conditioned frequency signal. Keyed by
`(phase: String, category: String)`. Each bucket is a `Vec<(entry_id: u64, score: f32)>` sorted
descending by frequency rank. Built from `query_log` by the background tick. Empty on cold-start
(`use_fallback = true`). The sole source of `phase_explicit_norm` for fused scoring.

**PhaseFreqTableHandle** — `Arc<RwLock<PhaseFreqTable>>`. Shared between `SearchService` and the
background tick. Tick holds write lock only during the state swap; search path holds read lock
only during score lookup. Sole-writer contract: background tick is the only writer.

**phase_affinity_score** — The public API method on `PhaseFreqTable`. Accepts
`(entry_id, entry_category, phase)` and returns a rank-normalized score in `[0.0, 1.0]`.
This is the integration contract consumed by fused scoring (col-031) and PPR personalization
(#398). Returns `1.0` (neutral) when cold-start, phase absent, or entry absent in bucket —
ensuring no penalty for unknown phase/entry combinations.

**FusedScoreInputs.phase_explicit_norm** — The `f64` field in `FusedScoreInputs` (already
present since crt-026) that receives the output of `phase_affinity_score`. Previously hardcoded
to `0.0`; col-031 activates it with live signal from `PhaseFreqTable`.

**query_log** — The `query_log` SQLite table (schema v17, col-028). Columns relevant to
col-031: `result_entry_ids` (JSON integer array), `phase` (TEXT, nullable),
`feature_cycle` (TEXT). Index `idx_query_log_phase` on `phase`. Pre-col-028 rows have
`phase = NULL` and contribute no signal (filtered by `WHERE phase IS NOT NULL`).

**retention window** — The lookback bound for the frequency table SQL query, expressed as the
last `query_log_retention_cycles` completed feature cycles (plus any open cycle). Governed by
`InferenceConfig.query_log_retention_cycles` (default 20). Shared parameter with #409
retention framework — this feature exposes the config field; GC logic belongs to #409.

**InferenceConfig** — Configuration struct for the inference pipeline. col-031 changes:
`w_phase_explicit` default `0.0 → 0.05`; new field `query_log_retention_cycles: u32` default
`20`. The `w_phase_explicit` field is an additive term outside the six-weight constraint
(ADR-004, Unimatrix #3175).

### Ubiquitous Language

| Term | Meaning |
|------|---------|
| Phase | Runtime string value stored in `query_log.phase` and supplied as `current_phase` in search parameters. Examples: `"delivery"`, `"design"`, `"bugfix"`. Not a compile-time enum. |
| Frequency bucket | A `Vec<(entry_id, score)>` within `PhaseFreqTable` for a specific `(phase, category)` key. |
| Rank-based normalization | Scoring formula `1.0 - (rank / N)` applied within a bucket. Produces scores in `(0, 1]` regardless of raw frequency distribution. Chosen over min-max to handle power-law access patterns (SCOPE.md §Open Questions 1). |
| Cold-start | `use_fallback = true`; `PhaseFreqTable` has not yet been populated by the background tick, or `query_log` contains no phase-tagged rows. All `phase_affinity_score` calls return `1.0` (neutral). |
| Neutral score | The value `1.0` returned by `phase_affinity_score` for absent entries. Means "no signal — treat as equally affine." Ensures no ranking penalty for cold-start or unknown entries. |
| Phase vocabulary | The set of distinct phase strings present in `query_log.phase`. Runtime-determined; not validated at compile time. Mismatch (e.g., case difference) silently degrades to neutral score for the mismatched key. |
| Retention window | The set of feature cycles included in the frequency table rebuild: last K completed cycles plus open cycles, where K = `query_log_retention_cycles`. |
| Sole-writer contract | The guarantee that only the background tick writes to `PhaseFreqTableHandle`. Search hot path is read-only. Prevents lock ordering issues (SR-06). |
| w_phase_explicit | The fusion weight for the `phase_explicit_norm` term. Previously `0.0` (placeholder since crt-026); col-031 raises default to `0.05`. |
| Additive term | A fusion weight (`w_phase_explicit`, `w_phase_histogram`) that is added to the six-weight composite score and is excluded from the six-weight sum constraint (ADR-004). |

---

## User Workflows

### Workflow 1: Normal search with phase context (table populated)

1. Agent calls `context_search` or `context_briefing` with `current_phase = "delivery"`.
2. `SearchService::search` acquires a short read lock on `PhaseFreqTableHandle`; clones the
   affinity scores for the candidate set; releases lock.
3. In the fused scoring loop, for each candidate entry `e` with category `c`:
   `phase_explicit_norm = phase_affinity_score(e.id, c, "delivery")`.
4. `FusedScoreInputs` populated with non-zero `phase_explicit_norm` for entries with delivery
   history. Entries with strong delivery-phase history receive higher `phase_explicit_norm`.
5. Fused score: `... + w_phase_explicit * phase_explicit_norm + ...`
6. Agent receives a ranked result set biased toward entries that have been useful in the
   delivery phase in recent sessions.

### Workflow 2: Search without phase context (no phase, or phase absent)

1. Agent calls `context_search` with no `current_phase` set.
2. `SearchService::search`: `params.current_phase = None` → `phase_explicit_norm = 0.0` for
   all candidates (FR-10).
3. Fused score unchanged from pre-col-031 for this request.
4. Agent receives result set with no phase conditioning applied.

### Workflow 3: Cold-start (first tick not yet complete)

1. Agent calls `context_search` immediately after server start.
2. `PhaseFreqTable::new_handle()` returned `use_fallback = true`.
3. `phase_affinity_score(any_id, any_category, any_phase)` returns `1.0` for all candidates.
4. When `current_phase = Some("delivery")`, all candidates get `phase_explicit_norm = 1.0`.
   The `w_phase_explicit * 1.0` adds a constant offset to all scores — uniform, ranking-
   preserving. When `current_phase = None`, `phase_explicit_norm = 0.0` — identical to
   pre-col-031.
5. Background tick completes; `PhaseFreqTable::rebuild` swaps handle under write lock.
6. Subsequent searches receive differentiated phase-conditioned scores.

### Workflow 4: Background tick rebuilds frequency table

1. Background tick fires on schedule.
2. `TypedGraphState::rebuild` completes (structural state).
3. `PhaseFreqTable::rebuild(store, query_log_retention_cycles).await` called.
4. SQL aggregation: expand `result_entry_ids` via `json_each`, join `entries` for category,
   group by `(phase, category, entry_id)`, count rows, order descending.
5. New `PhaseFreqTable` materialized in memory; `use_fallback = false`.
6. Write lock acquired on `PhaseFreqTableHandle`; `*guard = new_table`; lock released.
7. Error path: if rebuild fails, `tracing::error!` emitted; old state retained; next tick retries.

### Workflow 5: Eval harness extraction (post-AC-16)

1. `eval/scenarios/extract.rs` runs against a populated `query_log`.
2. Each extracted scenario includes `current_phase` (from `query_log.phase`).
3. Eval harness replays scenarios with phase context against col-031 scoring.
4. `phase_explicit_norm` is non-zero for candidates with phase history.
5. AC-12 regression gate is non-vacuous: scoring changes are measurable.

---

## Constraints

**C-01 — col-028 prerequisite**: `query_log.phase` (schema v17) must be present. Confirmed
shipped (gate-3c PASS 2026-03-26). Pre-col-028 rows have `phase = NULL` and contribute no
signal; this is expected and safe.

**C-02 — SR-01: json_each form must be verified**: The `json_each` SQL form for expanding
`result_entry_ids` (JSON integer array) must be confirmed against an actual `query_log` row
before the store query method is finalized. Integer vs. string element typing in SQLite's
`json_each` is a known portability trap. The existing usage in `mcp/knowledge_reuse.rs` must
be checked to confirm the call form matches this use case. Do not assume correctness from
SCOPE.md prose.

**C-03 — SR-03: eval harness fix and scoring activation are non-separable**: AC-16 (`extract.rs`
fix) and the scoring activation (`w_phase_explicit = 0.05`) must ship as a single deliverable.
AC-12 is a vacuous gate if AC-16 is incomplete. The delivery wave must treat these as atomic;
neither is independently shippable.

**C-04 — SR-06: lock acquisition order**: The background tick must never hold
`PhaseFreqTableHandle` write lock while acquiring any other write lock
(`TypedGraphStateHandle`, `EffectivenessStateHandle`). The architect must document the full
lock acquisition sequence in the architecture doc before implementation.

**C-05 — Phase vocabulary is runtime strings**: Phase keys in the frequency table are string
values from `query_log.phase` at insertion time. No compile-time phase enum. A phase rename
makes the old key go cold (all neutral scores); the new key starts empty. This is accepted
behavior, documented in code comments (SR-04).

**C-06 — w_phase_explicit additive invariant**: `w_phase_explicit` is outside the six-weight
sum constraint. The `FusionWeights` sum-check comment must be updated to reflect
`0.95 + 0.02 + 0.05 = 1.02`. No change to `validate()` logic is needed.

**C-07 — 500-line file limit**: `services/phase_freq_table.rs` must stay under 500 lines. SQL
aggregation must be in `unimatrix-store`. No exceptions.

**C-08 — #398 PPR conditional**: AC-07 and AC-08 specify the `phase_affinity_score` API as the
PPR integration contract. PPR (#398) is a separate, not-yet-shipped feature. col-031 must not
block on #398; #398 must not block on col-031. If #398 ships before col-031, the wire-up AC
applicability must be confirmed at delivery start (SR-05).

**C-09 — No GC implementation**: `query_log_retention_cycles` in `InferenceConfig` governs
lookback window only. The GC logic belongs to #409. col-031 must not implement any `query_log`
deletion or compaction.

**C-10 — No Thompson Sampling, no GNN, no BM25**: these are deferred per SCOPE.md Non-Goals.
Any implementation touching these areas is out of scope.

**C-11 — Tick timeout**: The existing `TICK_TIMEOUT` applies to the full tick including the
frequency table rebuild. No separate inner timeout for `PhaseFreqTable::rebuild`.

---

## Dependencies

| Dependency | Type | Status | Notes |
|-----------|------|--------|-------|
| `query_log.phase` column (schema v17) | Schema (internal) | Shipped (col-028, 2026-03-26) | Source of all phase signal. Pre-col-028 rows contribute zero signal. |
| `TypedGraphStateHandle` pattern | Internal pattern | Active | Template for `PhaseFreqTableHandle` (services/typed_graph.rs). |
| `EffectivenessStateHandle` pattern | Internal pattern | Active | Reference for poison recovery and generation-counter optimization (deferred). |
| `CategoryAllowlist` poison recovery | Internal pattern | Active | `.unwrap_or_else(|e| e.into_inner())` convention. |
| `Store::query_phase_freq_table` | Internal (new) | To be created | New method in `unimatrix-store`. SQL aggregation over `query_log + entries`. |
| `json_each` (SQLite built-in) | SQLite built-in | Active | Used in `mcp/knowledge_reuse.rs`. Form must be verified for integer arrays (C-02). |
| `FusedScoreInputs.phase_explicit_norm` | Internal field | Active (pre-existing) | Field exists since crt-026; previously hardcoded to `0.0`. |
| `w_phase_explicit` in `InferenceConfig` | Config field | Active (pre-existing) | Default changes `0.0 → 0.05`. |
| `params.current_phase` in `ServiceSearchParams` | Internal field | Active (crt-025/crt-026) | Already present in search params; source of phase key for scoring. |
| `query_log_retention_cycles` | Config field (new) | To be added | New `InferenceConfig` field. Default 20. Shared with #409 retention framework. |
| PPR `phase_affinity_score` call site (#398) | External feature | Not yet shipped | Integration contract: `phase_affinity_score` is public API. Wire-up is conditional on #398 status. |
| Eval harness `extract.rs` | Internal (eval crate) | To be modified | Add `current_phase` to scenario extraction. Bounded change. |
| `tracing` crate | Crate (existing) | Active | For `tracing::error!` on tick rebuild failure. |
| `tokio` (async runtime) | Crate (existing) | Active | `rebuild` is `async`. |
| `rusqlite` / `sqlx` | Crate (existing) | Active | SQL aggregation in `unimatrix-store`. |

---

## NOT in Scope

- **No `query_log` schema changes**: schema v17 (col-028) is the prerequisite, not a deliverable.
  Zero new migrations.
- **No Thompson Sampling**: deferred until after PPR baseline ICD is measured (ROADMAP.md).
- **No gap detection**: Loop 3 (a distinct feature after the frequency table is operating).
- **No PPR implementation**: #398 is a separate feature. col-031 provides `phase_affinity_score`
  as the integration API; the PPR algorithm itself is not implemented here.
- **No W3-1 GNN**: frequency table is the non-parametric predecessor; GNN deferred until
  CC@k ≥ 0.7.
- **No BM25 hybrid retrieval**: separate work item.
- **No `query_log` GC implementation**: `query_log_retention_cycles` config field is lookback
  window only. GC belongs to #409.
- **No backfill of pre-col-028 rows**: `phase = NULL` rows contribute zero signal; this is
  accepted.
- **No new MCP tool**: `PhaseFreqTable` is internal state. No diagnostic endpoint.
- **No change to `w_phase_histogram`**: session histogram term (crt-026) is unaffected.
- **No `EffectivenessSnapshot` generation-counter optimization for `PhaseFreqTable`**: deferred
  until profiling shows HashMap clone cost is material.
- **No UI or dashboard**: internal state only.
- **No evaluation JSONL scenarios with phase-conditioned entries**: AC-12 uses existing
  scenarios with `current_phase` populated from `extract.rs`; crafting new phase-specific
  scenarios is deferred.

---

## Open Questions

**OQ-01** — **SR-01: json_each exact SQL form**: The precise `json_each.value` cast for integer
JSON arrays must be confirmed against a real `query_log` row at implementation time. The
architect's store query design must specify the exact form and confirm it matches
`knowledge_reuse.rs`. This is the single highest implementation-surprise risk (SR-01).

**OQ-02** — **SR-02: w_phase_explicit calibration empirical grounding**: SCOPE.md cites ASS-032
as background; 0.05 is calibrated by judgment, not a benchmarked spike. The architect should
retrieve ASS-032 from Unimatrix and confirm whether 0.05 has numerical grounding. If not,
document the accepted risk in the ADR (per crt-026 lesson, Unimatrix #3208).

**OQ-03** — **SR-04: Silent-degradation observability**: When `current_phase` is set but has
no match in the frequency table (e.g., a newly introduced phase not yet in `query_log`), should
a `tracing::debug!` line be emitted? The spec requires the silent-degradation contract to be
documented in code comments at minimum; the architect decides whether a log line is also
appropriate.

**OQ-04** — **NFR-03 cold-start distinction**: When `current_phase = Some(phase)` and the table
is cold-start (`use_fallback = true`), `phase_affinity_score` returns `1.0` for all candidates,
adding a uniform `w_phase_explicit * 1.0 = 0.05` offset to all scores. This is
ranking-preserving but not bit-for-bit identical to pre-col-031. The spec notes this
distinction; the architect must decide whether this is the intended behavior or whether cold-start
should return `0.0` instead of `1.0` (which would change the semantics of "neutral").

**OQ-05** — **SR-05: #398 PPR concurrency**: Track #398 status at delivery start. If #398 is
shipping concurrently, confirm wire-up AC applicability and check for merge conflicts at
`phase_affinity_score` call sites before the implementation wave begins.

---

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing — Entry #3163 (ADR-003 crt-026: w_phase_explicit=0.0
  placeholder, W3-1 reserved) directly relevant; confirms col-031 activates this placeholder.
  Entry #3175 (ADR-004 crt-026: w_phase_histogram additive term, sum 0.95→0.97) confirms
  additive invariant for w_phase_explicit. Entry #749 (test-infrastructure pattern) confirms
  test fixture extension over isolated scaffolding. Entry #3562 (nan-009 ADR-001: suppress null
  phase emission in JSONL) relevant to AC-16 eval harness behavior.
