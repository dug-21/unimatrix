# Component: store-queries
# File: `crates/unimatrix-store/src/query_log.rs`

---

## Purpose

Replace the old `query_phase_freq_table` (search-exposure signal from `query_log`) with
two new functions sourcing from the explicit-read signal in `observations`. Add a new
internal result type `PhaseOutcomeRow` for Query B and a `MILLIS_PER_DAY` constant.

This component owns all SQL and all SQLite deserialization for the rebuild path.
No weighting logic lives here — that is `phase-freq-table`'s responsibility.

---

## Deleted Function

```
// DELETE this entire function and its doc comment:
pub async fn query_phase_freq_table(&self, lookback_days: u32) -> Result<Vec<PhaseFreqRow>>
```

The `row_to_phase_freq_row` private deserializer is RETAINED — it is reused by
`query_phase_freq_observations`.

---

## New Constant

```
/// Milliseconds per day for ts_millis lookback arithmetic.
///
/// observations.ts_millis is millisecond-epoch (contrast with query_log.ts which
/// is second-epoch). MUST NOT be 86_400 — omitting the *1000 factor produces a
/// 1000x-wide lookback window with no error logged (ADR-006, R-05).
///
/// Used in query_phase_freq_observations to pre-compute cutoff_millis in Rust.
const MILLIS_PER_DAY: i64 = 86_400 * 1_000;
```

Placement: module-level constants block, near top of file.

---

## New Type: PhaseOutcomeRow

```
/// One row from Query B: a (phase, feature_cycle, outcome) triple from
/// cycle_events joined to sessions.
///
/// Internal to the store crate. NOT re-exported from the crate root.
/// Consumed only by PhaseFreqTable::rebuild() via query_phase_outcome_map().
///
/// Visibility: pub(crate) or struct-private to the module — never pub.
struct PhaseOutcomeRow {       // or pub(crate) PhaseOutcomeRow
    phase: String,
    feature_cycle: String,
    outcome: String,
}
```

Placement: alongside `PhaseFreqRow` in the `// -- Types --` section.

---

## New Function: query_phase_freq_observations (Query A)

### Signature

```rust
/// Aggregate (phase, category, entry_id, freq) from explicit-read observations.
///
/// Replaces query_phase_freq_table. Sources from observations (deliberate agent
/// reads via context_get / context_lookup) instead of query_log search exposures.
///
/// # SQL
///
/// Filters to PreToolUse hook events for context_get and context_lookup tools
/// (4-entry IN clause — bare and mcp__unimatrix__ prefix variants).
/// CAST(json_extract(o.input, '$.id') AS INTEGER) is MANDATORY in the JOIN
/// predicate — omitting it causes a silent zero-row return (col-031 R-05).
/// o.hook = 'PreToolUse' (NOT o.hook_event — the DB column is `hook`, ADR-007).
///
/// # Parameters
///
/// `lookback_days` is converted to cutoff_millis (i64) in Rust using MILLIS_PER_DAY.
/// Bound as ?1 (i64, not u32 — sqlx 0.8 INTEGER mapping, ADR-006).
///
/// # Returns
///
/// Empty Vec when no matching observations exist within the lookback window.
/// Caller (PhaseFreqTable::rebuild) treats empty as use_fallback=true.
/// Results pre-sorted by (phase, category, freq DESC) — caller uses this
/// ordering directly for rank-based normalization (col-031 ADR-001).
pub async fn query_phase_freq_observations(
    &self,
    lookback_days: u32,
) -> Result<Vec<PhaseFreqRow>, StoreError>
```

### Body

```
FUNCTION query_phase_freq_observations(self, lookback_days: u32) -> Result<Vec<PhaseFreqRow>>:

  // Pre-compute lookback cutoff in Rust (ADR-006).
  // current time in milliseconds since UNIX epoch.
  now_millis: i64 = SystemTime::now()
                      .duration_since(UNIX_EPOCH)
                      .unwrap_or_default()
                      .as_millis() as i64

  cutoff_millis: i64 = now_millis - (lookback_days as i64) * MILLIS_PER_DAY

  sql = "
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
      AND o.ts_millis > ?1
    GROUP BY o.phase, e.category, entry_id
    ORDER BY o.phase, e.category, freq DESC
  "

  // Bind cutoff_millis as i64 (?1).
  rows = sqlx::query(sql)
           .bind(cutoff_millis)           // ?1 = i64
           .fetch_all(self.read_pool())
           .await
           .map_err(|e| StoreError::Database(e.into()))?

  // Deserialize using the existing row_to_phase_freq_row — column positions match:
  //   0: phase (String), 1: category (String), 2: entry_id (i64->u64), 3: freq (i64)
  rows.iter().map(row_to_phase_freq_row).collect()

END FUNCTION
```

### Column positions for row_to_phase_freq_row (unchanged deserializer)

```
SELECT clause column order:
  0: o.phase        → String
  1: e.category     → String
  2: entry_id       → i64 (CAST result), then cast to u64
  3: freq           → i64 (COUNT(*) always i64 in sqlx 0.8)
```

The existing `row_to_phase_freq_row` deserializer matches this order exactly — no change.

---

## New Function: query_phase_outcome_map (Query B)

### Signature

```rust
/// Fetch (phase, feature_cycle, outcome) triples for outcome-weight computation.
///
/// Returns all cycle_phase_end rows joined to sessions that have a non-NULL
/// feature_cycle. Pre-col-022 sessions (NULL feature_cycle) are excluded by
/// the WHERE clause — those sessions contribute no outcome weight (default 1.0).
///
/// # Returns
///
/// Empty Vec is valid (no cycle_phase_end history or all sessions pre-col-022).
/// Store error MUST propagate — do NOT return empty Vec on error (constraint C-7,
/// architecture constraint #12). The caller (PhaseFreqTable::rebuild) must
/// return Err and retain the previous table (retain-on-error semantics).
pub async fn query_phase_outcome_map(&self) -> Result<Vec<PhaseOutcomeRow>, StoreError>
```

### Body

```
FUNCTION query_phase_outcome_map(self) -> Result<Vec<PhaseOutcomeRow>>:

  sql = "
    SELECT ce.phase, s.feature_cycle, ce.outcome
    FROM cycle_events ce
      JOIN sessions s ON s.feature_cycle = ce.cycle_id
    WHERE ce.event_type = 'cycle_phase_end'
      AND ce.phase IS NOT NULL
      AND ce.outcome IS NOT NULL
      AND s.feature_cycle IS NOT NULL
  "

  rows = sqlx::query(sql)
           .fetch_all(self.read_pool())
           .await
           .map_err(|e| StoreError::Database(e.into()))?

  rows.iter().map(row_to_phase_outcome_row).collect()

END FUNCTION
```

### row_to_phase_outcome_row (new private deserializer)

```
FUNCTION row_to_phase_outcome_row(row: &SqliteRow) -> Result<PhaseOutcomeRow>:
  // Column order matches SELECT clause:
  //   0: ce.phase          → String
  //   1: s.feature_cycle   → String
  //   2: ce.outcome        → String

  Ok(PhaseOutcomeRow {
    phase:         row.try_get::<String, _>(0).map_err(|e| StoreError::Database(e.into()))?,
    feature_cycle: row.try_get::<String, _>(1).map_err(|e| StoreError::Database(e.into()))?,
    outcome:       row.try_get::<String, _>(2).map_err(|e| StoreError::Database(e.into()))?,
  })

END FUNCTION
```

---

## Module-Level Re-exports (crate root)

The `PhaseFreqRow` re-export from `lib.rs` (or wherever the crate root re-exports it)
is UNCHANGED. `PhaseOutcomeRow` is NOT added to the re-export list.

Verify in `crates/unimatrix-store/src/lib.rs`:
- `pub use query_log::PhaseFreqRow;` — already present, no change.
- No `PhaseOutcomeRow` export — confirm it is absent.

---

## Error Handling

| Failure | Behavior |
|---------|----------|
| Query A sqlx error | `StoreError::Database(e.into())` propagated to caller |
| Query B sqlx error | `StoreError::Database(e.into())` propagated to caller — never silenced |
| Row deserialization failure | `StoreError::Database` for each column read failure |
| Empty Query A result | Returns `Ok(vec![])` — caller decides use_fallback |
| Empty Query B result | Returns `Ok(vec![])` — caller applies default weight 1.0 |

---

## Key Test Scenarios

These scenarios guide the tester agent. Each is a distinct test function.

**T-SQ-01: Query A — explicit read rows produce correct aggregation**
- Insert 3 observations for the same (phase, entry_id) using `context_get` tool,
  phase="delivery". Insert 1 observation with `context_search` (must be excluded).
- Call `query_phase_freq_observations(30)`.
- Assert: result contains one `PhaseFreqRow` with phase="delivery", freq=3.
  `context_search` row contributes zero.

**T-SQ-02: Query A — both bare and prefixed tool names included (AC-02, FR-02)**
- Insert one observation each for all four tool name variants.
- Assert: all four produce rows.
- Insert observation with `context_search`. Assert: produces no row.

**T-SQ-03: Query A — CAST handles string-form IDs (AC-03, FR-05)**
- Insert observation with `input = '{"id": "42"}'` (string-form ID).
- Assert: `query_phase_freq_observations` returns entry 42 in results.
  (Validates CAST converts string "42" correctly.)

**T-SQ-04: Query A — filter-based context_lookup excluded (FR-03)**
- Insert observation with `input = '{"query": "foo"}'` (no `$.id` field).
- Assert: `query_phase_freq_observations` returns empty vec for this session.

**T-SQ-05: Query A — PreToolUse-only, PostToolUse excluded (FR-04, ADR-007)**
- Insert paired Pre/Post observations for same entry. Hook column: 'PreToolUse'/'PostToolUse'.
- Assert: `query_phase_freq_observations` returns freq=1, not freq=2.

**T-SQ-06: Query A — ts_millis lookback boundary (AC-07, FR-06, R-05)**
- Insert observation at `now_millis - lookback_days * MILLIS_PER_DAY + 500` (inside).
- Insert observation at `now_millis - lookback_days * MILLIS_PER_DAY - 500` (outside).
- Assert: only the inside observation is returned.

**T-SQ-07: Query A — MILLIS_PER_DAY constant value assertion (R-05)**
- Assert: `MILLIS_PER_DAY == 86_400_000i64` as a compile-time-visible unit test.

**T-SQ-08: Query B — returns correct (phase, feature_cycle, outcome) triples**
- Insert `cycle_events` row with event_type='cycle_phase_end', phase="delivery", outcome="pass".
- Insert `sessions` row with feature_cycle matching the cycle_id.
- Call `query_phase_outcome_map()`.
- Assert: one `PhaseOutcomeRow` returned with correct fields.

**T-SQ-09: Query B — NULL feature_cycle sessions excluded (FR-10, SR-05)**
- Insert session with feature_cycle=NULL. Insert observations for that session.
- Call `query_phase_outcome_map()`.
- Assert: no row returned for that session (graceful degradation, not error).

**T-SQ-10: Query B — store error propagates (constraint C-7)**
- This is tested at integration level — verify the calling code propagates `Err`.

**T-SQ-11: Write-path storage contract (R-01, ADR-005)**
- Insert an observation through the hook-listener write path (`extract_observation_fields`)
  for a `context_get` with `{"id": 42}`.
- Call `json_extract(input, '$.id')` directly via a raw SQL query in the test.
- Assert: returns `42` (not NULL, not `"42"` as a double-encoded string).
  This validates ADR-005: no double-encoding on hook path.
