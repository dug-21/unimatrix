# SPECIFICATION: col-031 — Phase-Conditioned Frequency Table

GH Issue: #414
Draft PR: https://github.com/dug-21/unimatrix/pull/423
Spec Author: col-031-agent-2-spec
Date: 2026-03-27

---

## Objective

Activate the `w_phase_explicit` scoring placeholder (ADR-003, crt-026) by building a
non-parametric, in-memory frequency table that maps `(phase, category)` pairs to ranked
entry scores derived from `query_log` access history. This eliminates the structural
phase-blindness in fused scoring — where every query regardless of workflow phase starts
from the same prior — without requiring any ML training, model downloads, or schema
migrations. The frequency table is rebuilt each background tick from `query_log.phase`
(schema v17, col-028), exposing `phase_affinity_score` as the integration contract for
PPR personalization (#398).

---

## Functional Requirements

### FR-01: PhaseFreqTable struct

`PhaseFreqTable` must be a struct in
`crates/unimatrix-server/src/services/phase_freq_table.rs` containing:
- `table: HashMap<(String, String), Vec<(u64, f32)>>` — keyed by `(phase, category)`,
  each Vec sorted descending by rank score.
- `use_fallback: bool` — `true` on cold-start or when rebuild produced zero rows.

`PhaseFreqTable::new()` must return a cold-start instance: `use_fallback = true`,
empty table.

### FR-02: PhaseFreqTableHandle

`PhaseFreqTableHandle = Arc<RwLock<PhaseFreqTable>>` must be the shared type, following
the `TypedGraphStateHandle` / `EffectivenessStateHandle` pattern exactly.

`PhaseFreqTable::new_handle()` must return a `PhaseFreqTableHandle` holding the
cold-start state produced by `new()`.

All lock acquisitions on this handle — by all callers — must use
`.unwrap_or_else(|e| e.into_inner())` for poison recovery (no `unwrap()` calls).

### FR-03: Store query method

A new method `SqlxStore::query_phase_freq_table(lookback_days: u32) -> Result<Vec<PhaseFreqRow>>`
must be added to `crates/unimatrix-store/src/query_log.rs`.

The SQL must be:
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

`?1` is bound as `lookback_days as i64`. Row deserialization uses
`row.try_get::<T, _>(index)` consistent with all existing `query_log.rs` methods.

`PhaseFreqRow` must have fields: `phase: String`, `category: String`,
`entry_id: u64`, `freq: i64` (SQLite `COUNT(*)` maps to `i64` via sqlx 0.8).

### FR-04: PhaseFreqTable::rebuild

`PhaseFreqTable::rebuild(store: &Store, lookback_days: u32) -> Result<Self, StoreError>`
must:
1. Call `store.query_phase_freq_table(lookback_days)`.
2. Group rows by `(phase, category)`.
3. Apply rank-based normalization within each bucket (see FR-05).
4. Return a `PhaseFreqTable` with `use_fallback = false` when rows are present,
   `use_fallback = true` when the result set is empty.

### FR-05: Rank-based normalization

Within each `(phase, category)` bucket, the rank score is computed as:

```
score = 1.0 - ((rank - 1) as f32 / N as f32)
```

where:
- `rank` is 1-indexed position sorted by `freq DESC` (rank 1 = most frequent).
- `N` is the total number of entries in the bucket.
- Top entry (rank 1) → `1.0`.
- Last entry (rank N) → `(N-1) / N`.
- Single-entry bucket (N=1, rank=1) → `1.0`.
- The alternative formula `1 - rank/N` (0-indexed rank) must NOT be used; it produces
  `0.0` for N=1.

The resulting Vec must be sorted descending by score before storage.

### FR-06: phase_affinity_score

`PhaseFreqTable::phase_affinity_score(entry_id: u64, entry_category: &str, phase: &str) -> f32`
must be a public method returning `f32 ∈ [0.0, 1.0]`.

Return rules:
- `use_fallback = true` → returns `1.0` (neutral PPR multiplier).
- `phase` absent as a key in `table` → returns `1.0`.
- `(phase, entry_category)` bucket present but `entry_id` absent → returns `1.0`.
- Entry present → returns its stored rank score.

The method must carry a doc comment explicitly naming both callers and their
cold-start contracts (see CON-07 and AC-17).

### FR-07: Background tick integration

`run_single_tick` must call `PhaseFreqTable::rebuild` once per cycle, after
`TypedGraphState::rebuild`. On success the handle is swapped under write lock
(`*guard = new_table`). On failure the existing state is retained and
`tracing::error!` is emitted. No tick failure is propagated.

### FR-08: ServiceLayer wiring

`ServiceLayer::with_rate_config()` (or equivalent construction site) must call
`PhaseFreqTable::new_handle()` and thread the handle to `SearchService` and the
background tick via `Arc::clone`. `PhaseFreqTableHandle` is a required (non-optional)
constructor parameter for every consuming service — it must not be `Option<...>`.

### FR-09: Fused scoring integration

Before the scoring loop in `search.rs`, the implementation must:
1. Acquire the `PhaseFreqTableHandle` read lock once.
2. Check `use_fallback`.
3. If `current_phase = None` or `use_fallback = true`: set `phase_snapshot = None`.
   Release the lock. Do not call `phase_affinity_score`.
4. If `use_fallback = false` and `current_phase = Some(phase)`: clone the relevant
   bucket data into a local snapshot. Release the lock.
5. Never hold the lock across the scoring loop.

Within the scoring loop:
- `phase_snapshot = None` → `phase_explicit_norm = 0.0` (identical to pre-col-031).
- `phase_snapshot = Some(...)` → `phase_explicit_norm = snapshot.affinity(entry.id, &entry.category) as f64`.

The `use_fallback` guard must fire before `phase_affinity_score` is ever called.

### FR-10: InferenceConfig changes

Two changes to `crates/unimatrix-server/src/config.rs`:
1. `default_w_phase_explicit()` returns `0.05` (was `0.0`).
2. New field `query_log_lookback_days: u32` with default `30`.

The FusionWeights sum-check comment must be updated to state:
`// 0.95 + 0.02 + 0.05 = 1.02 — w_phase_explicit is additive outside the six-weight constraint (ADR-004, crt-026)`.

`InferenceConfig::validate()` is unchanged — additive terms outside the six-weight
constraint are not subject to the sum check.

### FR-11: Eval harness fix

`eval/scenarios/replay.rs` must be modified to forward `current_phase` from extracted
scenario data into `ServiceSearchParams`. The architect confirmed that `extract.rs` and
`output.rs` already handle phase; the gap is in `replay.rs` which does not propagate
`current_phase` to the search call.

No other change to `replay.rs` is in scope.

---

## Non-Functional Requirements

### NFR-01: File size constraint

`crates/unimatrix-server/src/services/phase_freq_table.rs` must not exceed 500 lines.
SQL aggregation logic lives exclusively in `unimatrix-store/src/query_log.rs`.

### NFR-02: Lock-hold discipline

No read or write lock on `PhaseFreqTableHandle` may be held across the scoring loop.
The lock acquire → snapshot extract → release sequence must complete before the scoring
iteration begins. This prevents scoring latency from being gated on lock contention.

### NFR-03: Lock acquisition order

In `run_single_tick`, lock acquisitions must follow this order:
`EffectivenessStateHandle` → `TypedGraphStateHandle` → `PhaseFreqTableHandle`.

Each lock must be acquired, used, and released before acquiring the next. This order
must be documented in a code comment at the tick's lock sequence site to prevent future
refactors from introducing deadlock.

### NFR-04: Cold-start score identity

When `use_fallback = true` or `current_phase = None`, all fused scores must be
bit-for-bit identical to pre-col-031 scores. No rounding or floating-point deviation
is permitted.

### NFR-05: AC-12 / AC-16 gate ordering (hard constraint)

AC-12 (eval regression gate) must not be declared PASS in any wave unless AC-16 (eval
harness replay.rs fix) is present in the same or preceding wave AND the eval scenario
output is verified to contain non-null `current_phase` values. A passing AC-12 against
scenarios where all `current_phase = None` is a vacuous gate and constitutes a gate
failure. Gate 3b must reject any AC-12 PASS submission that does not include evidence
of non-null `current_phase` in scenario output.

### NFR-06: No PPR implementation

col-031 publishes `phase_affinity_score` as the API contract only. No PPR scaffolding,
no personalization vector construction, no HNSW seed weighting code goes into col-031.
`#398` owns PPR. Any PPR internals appearing in col-031 are out-of-scope additions.

### NFR-07: No schema migrations

Zero new database migrations. `query_log.phase` (schema v17, col-028) is the sole
prerequisite. No `feature_cycle` column exists in `query_log`; no JOIN with `sessions`
is required.

### NFR-08: Poison recovery

All `RwLock` acquisitions on `PhaseFreqTableHandle` use `.unwrap_or_else(|e| e.into_inner())`.
No bare `.unwrap()` calls on lock acquisitions.

---

## Acceptance Criteria

### AC-01 — Cold-start construction
`PhaseFreqTable::new()` returns `use_fallback = true`, empty table.
**Verification**: Unit test asserting `use_fallback == true` and `table.is_empty()`.

### AC-02 — Rebuild SQL correctness
`PhaseFreqTable::rebuild(store, lookback_days)` queries `query_log` rows where
`phase IS NOT NULL`, `result_entry_ids IS NOT NULL`, and
`ts > strftime('%s', 'now') - lookback_days * 86400`. Result is
`HashMap<(String, String), Vec<(u64, f32)>>` keyed by `(phase, category)`, each Vec
sorted descending by rank score.
**Verification**: AC-08 and AC-14 integration tests; SQL inspected in code review.

### AC-03 — Handle type and poison recovery
`PhaseFreqTable::new_handle()` returns `Arc<RwLock<PhaseFreqTable>>` in cold-start
state. All lock acquisitions use `.unwrap_or_else(|e| e.into_inner())`.
**Verification**: Code review confirms no bare `.unwrap()` on lock acquisitions.

### AC-04 — Background tick rebuild
Background tick calls `PhaseFreqTable::rebuild` once per cycle. On success, handle
swapped under write lock. On failure, existing state retained, `tracing::error!`
emitted.
**Verification**: Unit test or integration test that injects a failing store and
confirms existing state preserved and error logged.

### AC-05 — ServiceLayer wiring
`ServiceLayer` creates `PhaseFreqTableHandle` and threads it to `SearchService` and
background tick via `Arc::clone`. Handle is a required non-optional constructor
parameter.
**Verification**: Code review confirms no `Option<PhaseFreqTableHandle>`. Grep for all
`SearchService::new` call sites in `background.rs` confirms all sites receive the handle.

### AC-06 — Fused scoring guard
In the fused scoring loop, `FusedScoreInputs.phase_explicit_norm` is: `0.0` when
`current_phase = None` or `use_fallback = true`; otherwise computed from
`phase_affinity_score`. The `use_fallback` check happens before `phase_affinity_score`
is called. The lock is released before the scoring loop begins.
**Verification**: AC-11 unit tests; code review confirms lock-release ordering.

### AC-07 — phase_affinity_score signature and return contract
`phase_affinity_score(entry_id: u64, entry_category: &str, phase: &str) -> f32` is
public. Returns `f32 ∈ [0.0, 1.0]`. Returns `1.0` when `use_fallback = true`, phase
absent, or entry absent in bucket.
**Verification**: Unit tests covering each `1.0` return path; type signature in code.

### AC-08 — Integration test: rebuild from seeded data
Integration test using `TestDb`: seed `query_log` with 10 rows, `phase="delivery"`,
`result_entry_ids=[42]`, `ts` within lookback window. After `rebuild`, assert
`phase_affinity_score(42, "decision", "delivery") > 0.0` and
`phase_affinity_score(99, "decision", "delivery") == 1.0`.
**Verification**: Test passes in CI.

### AC-09 — w_phase_explicit default
`InferenceConfig.w_phase_explicit` default is `0.05`. Config TOML with no
`w_phase_explicit` key deserializes to `0.05`. Existing test
`test_inference_config_default_phase_weights` updated to assert `0.05`.
**Verification**: Updated test passes; deserialization test with empty TOML asserts `0.05`.

### AC-10 — query_log_lookback_days field
`InferenceConfig.query_log_lookback_days` exists with type `u32` and default `30`.
Config TOML with no `query_log_lookback_days` key deserializes to `30`.
**Verification**: Unit test asserting default deserialization.

### AC-11 — Cold-start invariants (three unit tests)
Test 1: `current_phase = None`, populated table → `phase_explicit_norm = 0.0`, scores
bit-for-bit identical to pre-col-031.
Test 2: `current_phase = Some(phase)`, `use_fallback = true` → `phase_explicit_norm = 0.0`
via `use_fallback` guard. Guard fires before `phase_affinity_score` is called.
Test 3: `phase_affinity_score` called directly on `use_fallback = true` table → returns
`1.0` (PPR neutral multiplier contract).
**Verification**: Three named unit tests, all passing.

### AC-12 — Eval regression gate (non-vacuous; requires AC-16)
MRR ≥ 0.35, CC@5 ≥ 0.2659, ICD ≥ 0.5340 (col-030 baselines). Gate is non-vacuous
only when eval scenarios carry non-null `current_phase` values (confirmed by AC-16).
AC-12 must not be declared PASS without AC-16 complete and verified.
**Verification**: Eval harness run with AC-16 fix in place; scenario output inspected
for non-null `current_phase`; eval metrics reported against baselines.

### AC-13 — Rank-based normalization formula
Within each `(phase, category)` bucket, `score = 1.0 - ((rank - 1) / N)` (1-indexed
rank, N = bucket size). Top entry → 1.0. Last entry → (N-1)/N. Single-entry bucket
(N=1) → 1.0. Absent entry → 1.0.
**Verification**: AC-14 normalization assertion; formula inspected in code review.

### AC-14 — Unit test: rebuild normalization
Unit test using existing test fixtures: `PhaseFreqTable::rebuild` from synthetic
`query_log` produces correct `(phase, category)` keying, correct descending rank order,
and correct normalization values.
**Verification**: Test passes; asserts exact score values for a controlled input set.

### AC-15 — File size
`services/phase_freq_table.rs` ≤ 500 lines. SQL aggregation in
`unimatrix-store/src/query_log.rs`.
**Verification**: `wc -l crates/unimatrix-server/src/services/phase_freq_table.rs`
reports ≤ 500 at merge time.

### AC-16 — Eval harness replay.rs fix
`eval/scenarios/replay.rs` forwards `current_phase` from scenario data into
`ServiceSearchParams`. No other change to `replay.rs`.
**Verification**: Eval scenario output file contains non-null `current_phase` values
for rows that have `query_log.phase` set. Code diff limited to replay.rs propagation and
output struct assignment.

### AC-17 — phase_affinity_score doc comment (from SR-06)
`phase_affinity_score` carries a doc comment naming both callers and their respective
cold-start contracts:
- PPR (#398): calls directly, receives `1.0` as neutral multiplier when `use_fallback = true`.
- Fused scoring: must guard on `use_fallback` before calling; returns `0.0` via the
  guard path, not via this method.
**Verification**: Doc comment present and naming both callers inspected in code review.

---

## Domain Models

### PhaseFreqTable
In-memory struct. The primary artifact of this feature. Rebuilt each background tick
from `query_log`. Represents the non-parametric access frequency signal for the
`w_phase_explicit` scoring term.

Fields:
- `table: HashMap<(String, String), Vec<(u64, f32)>>` — keys are `(phase, category)`;
  values are entry ID + rank score pairs, sorted descending.
- `use_fallback: bool` — true on cold-start or when rebuild returns zero rows.

### PhaseFreqTableHandle
`Arc<RwLock<PhaseFreqTable>>` — the shared, thread-safe reference to `PhaseFreqTable`.
Background tick is sole writer. Read path (scoring) takes a short read lock, extracts
a snapshot, releases before scoring.

### PhaseFreqRow
Transient deserialization struct from the store layer. Lives only during rebuild.
Fields: `phase: String`, `category: String`, `entry_id: u64`, `freq: i64`.

### Phase (ubiquitous language)
A runtime string identifying the current workflow stage (e.g., "design", "delivery",
"scope"). No compile-time enum — vocabulary is defined by the workflow layer at runtime.
Phase rename silently strands historical data under the old key; the new key starts
cold and falls through to `use_fallback` behavior. This is expected degradation, not
a bug (SR-04).

### Category (ubiquitous language)
The knowledge category of an entry (e.g., "decision", "lesson-learned", "convention").
The frequency table groups entries by `(phase, category)` — the joint access pattern
captures which knowledge types agents use during which workflow phases.

### Rank Score
The normalized affinity value `f32 ∈ [0.0, 1.0]` assigned to an entry within its
`(phase, category)` bucket. Derived from rank position, not raw frequency count. Chosen
over min-max normalization because access patterns are power-law distributed — rank
spreading provides a richer gradient for PPR personalization vectors.

### Cold-Start
State when `use_fallback = true`. Occurs at server startup (before first tick) and
when the rebuild SQL returns zero rows. Two distinct cold-start behaviors apply:
- Fused scoring path: `phase_explicit_norm = 0.0` — preserves pre-col-031 score identity.
- PPR path: `phase_affinity_score = 1.0` — neutral multiplier for personalization vector.

### w_phase_explicit
A named weight in `InferenceConfig` / `FusionWeights`. Additive term outside the
six-weight sum constraint (ADR-004, crt-026). Default raised from `0.0` to `0.05` by
this feature. When `use_fallback = true` or `current_phase = None`, the term contributes
`0.0` regardless of the weight value.

---

## User Workflows

### Workflow 1: Normal query with active phase history
1. Agent calls `context_search` with `current_phase = "delivery"`.
2. `SearchService` acquires `PhaseFreqTableHandle` read lock.
3. `use_fallback = false` (history exists). Relevant bucket cloned. Lock released.
4. Scoring loop iterates candidates. For each entry: `phase_affinity_score` called on
   snapshot. `phase_explicit_norm` populated. `FusedScoreInputs` assembled.
5. Fused score includes `w_phase_explicit * phase_explicit_norm` contribution.
6. Results ranked; delivery-phase-relevant entries surface higher than phase-agnostic
   scoring would produce.

### Workflow 2: Cold-start query (server just started)
1. Agent calls `context_search` with `current_phase = "delivery"`.
2. `SearchService` acquires `PhaseFreqTableHandle` read lock.
3. `use_fallback = true`. Lock released. `phase_snapshot = None`.
4. Scoring loop: `phase_explicit_norm = 0.0` for all candidates.
5. Scores bit-for-bit identical to pre-col-031. No degradation visible to caller.

### Workflow 3: Query with no phase
1. Agent calls `context_search` with no `current_phase`.
2. `current_phase = None` → lock never acquired, `phase_snapshot = None`.
3. Scoring loop: `phase_explicit_norm = 0.0`. Scores identical to pre-col-031.

### Workflow 4: Background tick rebuild
1. Tick fires. `TypedGraphState::rebuild` completes.
2. `PhaseFreqTable::rebuild(store, lookback_days)` called.
3. SQL executes: returns rows from `query_log` within the time window, joined to `entries`.
4. Rows grouped by `(phase, category)`; rank-based normalization applied.
5. Write lock acquired. Handle swapped to new table. Lock released.
6. On error: existing state retained. `tracing::error!` emitted. Tick continues.

### Workflow 5: PPR integration (#398, future)
1. #398 PPR implementation calls `phase_affinity_score(entry_id, category, phase)`
   directly on the `PhaseFreqTable`.
2. Returns rank score ∈ [0.0, 1.0], or `1.0` on cold-start.
3. PPR uses this as a multiplier in the personalization vector:
   `personalization[v] = hnsw_score[v] * phase_affinity_score(v.id, v.category, phase)`.
4. col-031 is complete when `phase_affinity_score` exists and is tested. No PPR
   wiring goes into col-031.

---

## Constraints

### CON-01: col-028 prerequisite
`query_log.phase` (schema v17) must be present. col-028 gate-3c PASS confirmed
2026-03-26. Pre-col-028 rows have `phase = NULL` and are excluded by
`WHERE phase IS NOT NULL`.

### CON-02: sqlx 0.8 binding
`COUNT(*)` maps to `i64`. `PhaseFreqRow.freq` is `i64`. Bind `lookback_days as i64`.
Row deserialization via `row.try_get::<T, _>(index)`.

### CON-03: No feature_cycle column
`query_log` has no `feature_cycle` column. Retention is time-based via `ts` (INTEGER,
Unix epoch seconds). No JOIN with `sessions`. No cycle-based subquery. `#409` owns
cycle-aligned GC.

### CON-04: json_each form
`CAST(json_each.value AS INTEGER)` is the verified expansion form for `result_entry_ids`
(confirmed against `mcp/knowledge_reuse.rs`). No SQLite extension required.

### CON-05: w_phase_explicit additive invariant
`w_phase_explicit = 0.05` raises the total weight sum to `1.02`. This is outside the
six-weight sum constraint per ADR-004 (crt-026). `validate()` is not changed. The
sum-check comment must be updated to reflect the additive structure.

### CON-06: Sole writer / lock ordering
Background tick is the sole writer of `PhaseFreqTableHandle`. Lock acquisition order in
`run_single_tick`:
1. `EffectivenessStateHandle`
2. `TypedGraphStateHandle`
3. `PhaseFreqTableHandle`

Each lock acquired, data extracted, released before next. Never hold multiple locks
simultaneously. A code comment at the tick's lock sequence site must document this
order by name.

### CON-07: Two cold-start behaviors, one method
`phase_affinity_score` has two distinct cold-start contracts for two callers:
- Fused scoring: must NOT call `phase_affinity_score` when `use_fallback = true`.
  The `use_fallback` guard fires first and returns `phase_explicit_norm = 0.0`.
- PPR (#398): calls `phase_affinity_score` directly; receives `1.0`.

Both behaviors are correct and intentional. The method doc comment must make this
explicit (AC-17).

### CON-08: No PPR scaffolding
col-031 must not implement any PPR internals, personalization vector construction, or
HNSW seed weight code. `phase_affinity_score` is published as an API contract only.
Confirm `#398` is not yet shipped at delivery start.

### CON-09: Phase vocabulary is runtime strings
No compile-time enum for phase values. A phase rename causes old keys to go cold
silently. Cold-start fallback is the only recovery path. This is an explicit
operational characteristic, not a bug.

### CON-10: AC-12 non-vacuous gate (hard)
AC-12 cannot be validated without AC-16 complete and verified. The delivery wave
containing AC-12 must include AC-16. No exceptions. Treating these as separable
deliverables makes AC-12 a noise check.

---

## Dependencies

### Crate dependencies
- `crates/unimatrix-store` — `SqlxStore`, sqlx 0.8 with `sqlite`, `runtime-tokio`,
  `macros` features. `query_log.rs` is the extension point.
- `crates/unimatrix-server` — `services/typed_graph.rs` is the structural template.
  `services/effectiveness.rs` introduces the `generation` counter optimization (deferred
  for `PhaseFreqTable` until profiling shows need). `search.rs` is the scoring
  integration point. `background.rs` is the tick integration point.

### Existing components consumed
- `TypedGraphStateHandle` pattern (`services/typed_graph.rs`) — template for struct,
  handle type, `new_handle()`, `rebuild()`, tick swap, and poison recovery.
- `FusedScoreInputs.phase_explicit_norm` — existing named field in the scoring struct.
- `InferenceConfig.w_phase_explicit` — existing named field, default raised to `0.05`.
- `query_log.ts` (INTEGER) — time filter uses `strftime('%s','now') - ?1 * 86400`.
- `query_log.phase` (TEXT nullable, schema v17, col-028).
- `query_log.result_entry_ids` (TEXT, JSON array of unquoted u64 integers).
- `entries.category` (TEXT) — joined in rebuild SQL.

### External prerequisites
- col-028 (schema v17, `query_log.phase`) — COMPLETE (gate-3c PASS 2026-03-26).
- col-030 eval baselines (MRR ≥ 0.35, CC@5 ≥ 0.2659, ICD ≥ 0.5340) — required for
  AC-12 gate values.

### Future consumers
- #398 (PPR) — will call `phase_affinity_score` directly. col-031 does not implement
  PPR. Confirm #398 is not yet shipped at delivery start.
- #409 (cycle-aligned GC) — will own cycle-based retention. col-031's time-based
  lookback is the interim approach.

---

## NOT in Scope

- **PPR implementation** — `phase_affinity_score` is the API contract only. PPR internals
  belong to #398.
- **query_log schema changes** — zero new migrations. Schema v17 is the prerequisite.
- **feature_cycle column** — does not exist in `query_log`. No cycle-based retention.
- **query_log GC** — `query_log_lookback_days` governs the rebuild SQL window only, not
  data deletion. GC belongs to #409.
- **Thompson Sampling** — deferred until after PPR baseline ICD is measured.
- **Gap detection (Loop 3)** — separate feature after frequency table is operating.
- **W3-1 GNN** — frequency table is the non-parametric predecessor; GNN deferred.
- **BM25 hybrid retrieval** — separate work item.
- **Backfill of phase=NULL rows** — pre-col-028 rows are filtered out, not backfilled.
- **MCP tool or diagnostic endpoint** — `PhaseFreqTable` is internal state only.
- **w_phase_histogram changes** — session histogram term (crt-026) is unaffected.
- **generation counter optimization** — the `EffectivenessState` generation counter
  pattern is deferred for `PhaseFreqTable` until profiling shows need.
- **Sessions JOIN** — no JOIN with `sessions` table required.
- **cycle-aligned lookback** — belongs to #409; `lookback_days` is a time approximation.

---

## Open Questions

### OQ-01: TypedGraphStateHandle pattern drift
SCOPE.md documents the template from `services/typed_graph.rs`. The risk assessment
(SR-02 assumption) notes `EffectivenessState` adds a `generation: u64` counter.
**Question for architect**: Has `TypedGraphStateHandle` or the tick swap pattern
diverged from the SCOPE.md description in any way that `PhaseFreqTable` must match?
Specifically: does the current `run_single_tick` hold locks simultaneously rather than
sequentially? Confirm before writing background.rs integration.

### OQ-02: SearchService construction sites
SR-01 flags that `run_single_tick` may construct `SearchService` directly, bypassing
`ServiceLayer`. **Question for architect / implementer**: Grep `crates/unimatrix-server/`
for all `SearchService::new` call sites before writing wiring code. All sites must
receive `PhaseFreqTableHandle` as a required parameter. This check must be done before
delivery is declared complete.

### OQ-03: col-030 baseline validity with w_phase_explicit=0.05
SR-01 assumption: col-030 baselines (MRR ≥ 0.35, CC@5 ≥ 0.2659, ICD ≥ 0.5340) were
measured with `w_phase_explicit = 0.0`. If any baseline eval scenarios carry non-null
`current_phase`, raising the default to `0.05` may shift scores. **Question for
delivery**: Re-confirm baselines by running eval with col-030 baseline scenarios and
`w_phase_explicit = 0.05` before committing to AC-12 threshold values.

### OQ-04: lookback_days per-environment override
SR-05 notes the 30-day default is session-frequency-dependent, not cycle-representative.
**Question for delivery / ops**: Should `query_log_lookback_days` be exposed in
deployment config (e.g., `unimatrix.toml`) or is `InferenceConfig` deserialization
from TOML sufficient? SCOPE.md's constraint section implies TOML override is already
covered by `InferenceConfig`; confirm this is the intended mechanism.

---

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing — returned 15 entries; key relevant results:
  #3679 (col-031 ADR on rank-based normalization), #3683 (col-031 ADR on
  w_phase_explicit gate ordering), #3677 (phase_affinity_score absent-entry=1.0
  pattern), #3555 (eval harness extract.rs gap), #3576 (eval compute_phase_stats
  grouping pattern), #3565 (phase as soft vocabulary key ADR), #3519 (query_log.phase
  col-028 ADR). All key decisions consistent with SCOPE.md; no contradictions found.
