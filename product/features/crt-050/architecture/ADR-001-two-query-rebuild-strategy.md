## ADR-001: Two-Query Rebuild Strategy with Rust Post-Process Outcome Weighting

### Context

`PhaseFreqTable::rebuild()` needs to (a) aggregate `(phase, category, entry_id,
freq)` counts from `observations` and (b) apply outcome-based weights derived
from `cycle_events`. The weighting join path spans three tables at different
granularities: `observations` (row-per-tool-call) → `sessions`
(feature_cycle FK) → `cycle_events` (per-phase outcomes). A single SQL query
combining all three would require a multi-table join with aggregate grouping at
different levels, making it difficult to test in isolation and difficult to
reason about the boundary between counting and weighting.

Two alternative approaches were considered:

**Option A (single SQL):** One query joining all four tables with conditional
weighting in SQL CASE expressions.
- Pro: single round-trip.
- Con: opaque weighting logic, difficult to unit-test in isolation, sensitive to
  NULL propagation across three join levels, requires reimplementing
  `infer_gate_result`-equivalent logic in SQL.

**Option B (two queries, Rust post-process):** Query A aggregates
`observations JOIN entries` → `Vec<PhaseFreqRow>`. Query B fetches
`cycle_events JOIN sessions` → `Vec<PhaseOutcomeRow>`. Rust post-process
builds the outcome weight map and applies it to the Query A rows.
- Pro: each query is independently testable with synthetic data; Rust weighting
  logic is testable without a database fixture; NULL degradation is explicit
  Rust code not SQL NULL propagation.
- Con: two DB round-trips per tick (acceptable — `PhaseFreqTable` rebuild is
  not on the search hot path).

### Decision

Use Option B (two queries with Rust post-process).

**Query A** (`query_phase_freq_observations`):

```sql
SELECT o.phase,
       e.category,
       CAST(json_extract(o.input, '$.id') AS INTEGER) AS entry_id,
       COUNT(*) AS freq
FROM observations o
  JOIN entries e ON CAST(json_extract(o.input, '$.id') AS INTEGER) = e.id
WHERE o.phase IS NOT NULL
  AND o.hook = 'PreToolUse'
  AND o.tool IN ('context_get', 'mcp__unimatrix__context_get',
                 'context_lookup', 'mcp__unimatrix__context_lookup')
  AND json_extract(o.input, '$.id') IS NOT NULL
  AND o.ts_millis > (strftime('%s', 'now') - ?1 * 86400) * 1000
GROUP BY o.phase, e.category, entry_id
ORDER BY o.phase, e.category, freq DESC
```

Returns `Vec<PhaseFreqRow>` — same type as today. `freq` is a raw read count
at this stage.

**Query B** (`query_phase_outcome_map`):

```sql
SELECT ce.phase, s.feature_cycle, ce.outcome
FROM cycle_events ce
  JOIN sessions s ON s.feature_cycle = ce.cycle_id
WHERE ce.event_type = 'cycle_phase_end'
  AND ce.phase IS NOT NULL
  AND ce.outcome IS NOT NULL
  AND s.feature_cycle IS NOT NULL
```

Returns `Vec<PhaseOutcomeRow { phase, feature_cycle, outcome }>`.

**Rust post-process (`apply_outcome_weights`):**

1. Build `HashMap<String, f32>` keyed by `phase` (not `(phase, feature_cycle)`).
   Aggregate across all `feature_cycle` entries for each phase using
   **mean weighting**: average `outcome_weight(outcome)` across all cycles that
   produced `cycle_phase_end` rows for that phase. This is more principled than
   best-weight (which would reward phases that ever passed regardless of failure
   rate).
2. For each `PhaseFreqRow`, multiply `freq` by the per-phase weight (default
   `1.0` when the phase has no `cycle_phase_end` rows — AC-05 contract).
3. The weighted `freq` is cast to `i64` (rounding) before storage in
   `PhaseFreqRow` to preserve the existing type. Downstream rank normalization
   uses only the ordering of `freq` values within a bucket, not absolute
   magnitude — the cast is invariant to this operation.

**Store placement:** Both new functions live on `SqlxStore` in
`unimatrix-store`. The weighting function lives in
`unimatrix-server/src/services/phase_freq_table.rs`. No cross-layer dependency
is created.

### Consequences

- Each query is independently testable with an in-memory SQLite fixture.
- The weighting logic (`apply_outcome_weights`) is testable with a synthetic
  `Vec<PhaseOutcomeRow>` without any DB fixture.
- Two DB round-trips per tick (negligible given tick cadence).
- Graceful degradation when Query B returns empty: weight map is empty, all
  rows default to weight `1.0`, `use_fallback` is not set (AC-05).
- Graceful degradation when Query A returns empty: `use_fallback = true`
  (existing cold-start behavior, unchanged).
- NULL `feature_cycle` on `sessions` rows (pre-col-022 sessions, SR-05)
  is handled by the `s.feature_cycle IS NOT NULL` predicate in Query B —
  those sessions produce no outcome rows, contributing no weight,
  defaulting to `1.0`.
