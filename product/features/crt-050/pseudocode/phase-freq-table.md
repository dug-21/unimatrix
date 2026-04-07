# Component: phase-freq-table
# File: `crates/unimatrix-server/src/services/phase_freq_table.rs`

---

## Purpose

Replace the single `store.query_phase_freq_table()` call in `PhaseFreqTable::rebuild()`
with the two-query path (Query A + Query B). Add the Rust post-process weighting step.
Add `phase_category_weights()` public accessor for W3-1 GNN cold-start.
All other `PhaseFreqTable` internals (struct fields, `phase_affinity_score()`, `new()`,
`new_handle()`, `Default`, `PhaseFreqTableHandle`, poison recovery, existing tests) are
preserved exactly.

---

## Imports Required

Add to existing imports (verify not already present):

```
use unimatrix_store::PhaseFreqRow;
// PhaseOutcomeRow is NOT imported — it lives in unimatrix-store and is passed
// to apply_outcome_weights as Vec<unimatrix_store::PhaseOutcomeRow>.
// Because PhaseOutcomeRow is not re-exported, the server crate must call the store
// function and use the returned type. See "Visibility Note" below.
```

### Visibility Note for PhaseOutcomeRow

`PhaseOutcomeRow` is not pub-re-exported from `unimatrix-store`. To pass it from
`rebuild()` to `apply_outcome_weights()`, two options:

- **Option A (preferred):** Make `PhaseOutcomeRow` pub(crate) within the store crate and
  expose it as `pub use query_log::PhaseOutcomeRow` from the store crate's lib.rs with
  `#[doc(hidden)]`. Then import in phase_freq_table.rs.
- **Option B:** Move `apply_outcome_weights` into the store crate as a free function
  (implementation discretion).

The architecture says "internal to rebuild" — the implementer must choose Option A or B
and document the decision. Option A is preferred because it keeps weighting logic in the
server crate where `outcome_weight()` lives.

If Option A is chosen, `PhaseOutcomeRow` visibility in store becomes `pub` with a
`#[doc(hidden)]` marker to signal it is an implementation detail.

---

## Modified Function: rebuild()

### Signature (unchanged)

```rust
pub async fn rebuild(store: &Store, lookback_days: u32) -> Result<Self, StoreError>
```

### Body

```
FUNCTION PhaseFreqTable::rebuild(store: &Store, lookback_days: u32) -> Result<Self, StoreError>:

  // Step 1: Query A — explicit-read aggregates from observations
  rows_a: Vec<PhaseFreqRow> = store.query_phase_freq_observations(lookback_days).await?

  // Step 2: Empty result → cold-start (unchanged behavior)
  IF rows_a.is_empty():
    RETURN Ok(PhaseFreqTable { table: HashMap::new(), use_fallback: true })

  // Step 3: Coverage gate — count distinct (phase, session_id) pairs in rows_a.
  //
  // NOTE: rows_a comes from the SQL aggregate (phase, category, entry_id, freq).
  // It does NOT carry session_id — that was aggregated away by GROUP BY.
  // The coverage count must be obtained via a SEPARATE SQL COUNT query.
  //
  // The coverage check queries the observations table directly (same window).
  // Implementation options:
  //   (a) A third store fn: store.count_phase_session_pairs(lookback_days) -> Result<u64>
  //   (b) Embed the count into Query A as a subquery (complicates SQL)
  // Option (a) is recommended — keeps SQL testable in isolation.
  //
  // See: architecture ARCHITECTURE.md "Observations Coverage Diagnostic" section.
  // The gate fires here in rebuild(); the status.rs diagnostic is separate (advisory only).
  //
  // If below threshold → use_fallback=true and tracing::warn!, return early.
  coverage_count: u64 = store.count_phase_session_pairs(lookback_days).await?
  // count_phase_session_pairs SQL:
  //   SELECT COUNT(DISTINCT phase || ':' || session_id)
  //   FROM observations
  //   WHERE phase IS NOT NULL
  //     AND hook = 'PreToolUse'
  //     AND tool IN ('context_get', 'mcp__unimatrix__context_get',
  //                  'context_lookup', 'mcp__unimatrix__context_lookup')
  //     AND ts_millis > ?1
  // Bound with same cutoff_millis as Query A.

  IF coverage_count < min_phase_session_pairs as u64:
    tracing::warn!(
      coverage_count = coverage_count,
      min_phase_session_pairs = min_phase_session_pairs,
      "PhaseFreqTable: distinct (phase, session_id) pairs ({}) below minimum \
       threshold ({}); falling back to neutral scoring",
      coverage_count, min_phase_session_pairs
    )
    RETURN Ok(PhaseFreqTable { table: HashMap::new(), use_fallback: true })

  // Step 4: Query B — outcome map from cycle_events + sessions
  // ERROR MUST PROPAGATE — do not catch and return empty (constraint C-7)
  rows_b: Vec<PhaseOutcomeRow> = store.query_phase_outcome_map().await?

  // Step 5: Apply outcome weights (Rust post-process)
  weighted_rows: Vec<PhaseFreqRow> = apply_outcome_weights(rows_a, rows_b)

  // Step 6: Group by (phase, category) — rows are pre-sorted by SQL ORDER BY
  grouped: HashMap<(String, String), Vec<PhaseFreqRow>> = HashMap::new()
  FOR row IN weighted_rows:
    grouped
      .entry((row.phase.clone(), row.category.clone()))
      .or_default()
      .push(row)

  // Step 7: Rank-normalize each bucket (UNCHANGED col-031 ADR-001 formula)
  table: HashMap<(String, String), Vec<(u64, f32)>> = HashMap::with_capacity(grouped.len())
  FOR (key, bucket_rows) IN grouped:
    n = bucket_rows.len()
    bucket: Vec<(u64, f32)> = bucket_rows
      .iter()
      .enumerate()
      .map(|(idx, row)| {
        rank = idx + 1                                  // 1-indexed
        score = 1.0_f32 - ((rank - 1) as f32 / n as f32)
        // CRITICAL: (rank-1)/N form:
        //   rank=1 (top): score=1.0
        //   rank=N (last): score=1/N (always > 0, never 0.0)
        //   N=1 (single entry): score=1.0
        (row.entry_id, score)
      })
      .collect()
    table.insert(key, bucket)

  // Step 8: Return populated table
  RETURN Ok(PhaseFreqTable { table, use_fallback: false })

END FUNCTION
```

### Parameter threading for min_phase_session_pairs

`rebuild()` currently has signature `(store: &Store, lookback_days: u32)`. The coverage
threshold `min_phase_session_pairs` must be passed in. Two options:

- **Option A (preferred):** Extend signature:
  `rebuild(store: &Store, lookback_days: u32, min_phase_session_pairs: u32) -> Result<Self, StoreError>`
- **Option B:** Add `min_phase_session_pairs` as a field on `PhaseFreqTable` (not preferred —
  it is a config value, not table state).

The caller in `background.rs` already reads `inference_config.phase_freq_lookback_days`
and would also read `inference_config.min_phase_session_pairs`. Option A requires
updating the background.rs call site alongside the signature change.

---

## New Private Function: outcome_weight()

```rust
/// Map a cycle outcome string to a weighting factor.
///
/// Priority order: rework checked before fail (ADR-003 constraint #7).
/// This mirrors the priority ordering in `infer_gate_result()` in `mcp/tools.rs`
/// (col-026 R-03). Any future change to the canonical outcome vocabulary must
/// update BOTH this function and `infer_gate_result()`.
///
/// Mapping:
///   case-insensitive contains "rework" → 0.5  (checked FIRST)
///   case-insensitive contains "fail"   → 0.5
///   case-insensitive contains "pass"   → 1.0
///   anything else (including "unknown", "") → 1.0 (graceful degradation, AC-05)
fn outcome_weight(outcome: &str) -> f32
```

### Body

```
FUNCTION outcome_weight(outcome: &str) -> f32:

  lower = outcome.to_lowercase()

  // rework checked BEFORE fail — priority order (ADR-003 constraint #7)
  IF lower.contains("rework"):
    RETURN 0.5
  IF lower.contains("fail"):
    RETURN 0.5
  IF lower.contains("pass"):
    RETURN 1.0

  // All other strings (unknown, empty, unrecognized): graceful degradation = 1.0
  // AC-05 contract: missing outcome = unweighted = weight 1.0
  RETURN 1.0

END FUNCTION
```

---

## New Private Function: apply_outcome_weights()

```rust
/// Apply per-phase mean outcome weights to explicit-read frequency rows.
///
/// Builds a per-phase weight by averaging outcome_weight() across all
/// cycle_phase_end rows for each phase (per-phase MEAN, not per-cycle —
/// ADR-001 constraint #6, R-03). This preserves rank ordering invariant
/// within buckets: all rows for the same phase share the same multiplier.
///
/// When no outcome rows exist for a phase, the default weight 1.0 is used
/// (AC-05 contract).
///
/// The weighted freq is stored back as i64 (rounded via as i64 cast).
/// Rank normalization uses only ordering, not absolute magnitude — the cast
/// is invariant to the normalization formula (col-031 ADR-001).
fn apply_outcome_weights(
    rows: Vec<PhaseFreqRow>,
    outcome_rows: Vec<PhaseOutcomeRow>,
) -> Vec<PhaseFreqRow>
```

### Body

```
FUNCTION apply_outcome_weights(rows: Vec<PhaseFreqRow>, outcome_rows: Vec<PhaseOutcomeRow>)
    -> Vec<PhaseFreqRow>:

  // Step 1: Build HashMap<phase, Vec<f32>> — collect weights per phase
  raw_weights: HashMap<String, Vec<f32>> = HashMap::new()
  FOR outcome_row IN outcome_rows:
    w = outcome_weight(&outcome_row.outcome)
    raw_weights
      .entry(outcome_row.phase.clone())
      .or_default()
      .push(w)

  // Step 2: Compute per-phase MEAN weight from collected weights
  // (Mean, not best-weight — ADR-001 OQ-1, constraint #6)
  phase_weights: HashMap<String, f32> = HashMap::new()
  FOR (phase, weight_vec) IN raw_weights:
    mean = weight_vec.iter().sum::<f32>() / weight_vec.len() as f32
    phase_weights.insert(phase, mean)

  // Step 3: Apply per-phase mean weight to each row's freq
  result: Vec<PhaseFreqRow> = rows
    .into_iter()
    .map(|mut row| {
      weight = phase_weights.get(&row.phase).copied().unwrap_or(1.0_f32)
      row.freq = (row.freq as f32 * weight) as i64
      row
    })
    .collect()

  RETURN result

END FUNCTION
```

### Why per-phase mean (not per-cycle) — R-03 guard

If per-cycle weights were applied (different multipliers per row in the same bucket),
rows within the same `(phase, category)` bucket could receive different multipliers,
potentially scrambling relative ordering before rank normalization. Per-phase mean
ensures all rows in a bucket share the same multiplier, preserving the ordering
invariant of the rank-normalization formula.

---

## New Public Method: phase_category_weights()

```rust
/// Return a learned (phase, category) weight map for W3-1 GNN cold-start.
///
/// Weight = fraction of total explicit-read entries for the phase attributable
/// to the category. Formula: bucket.len() / total_entries_for_phase (breadth-based,
/// ADR-008). Sums to 1.0 per phase (up to f32 rounding).
///
/// This is categorical BREADTH (distinct entries accessed per category), not
/// categorical DEPTH (how often entries were accessed). W3-1 implementers: if
/// you need a weighted-sum projection, access self.table directly.
///
/// Returns empty map when use_fallback = true (no signal available).
/// NOT called on the search hot path — GNN initialization only (NFR-07).
pub fn phase_category_weights(&self) -> HashMap<(String, String), f32>
```

### Body

```
FUNCTION phase_category_weights(&self) -> HashMap<(String, String), f32>:

  // Cold-start: no data, return empty map (AC-08)
  IF self.use_fallback:
    RETURN HashMap::new()

  // Step 1: Compute total distinct-entry count per phase
  // total_entries_for_phase[phase] = sum of bucket.len() across all categories for phase
  phase_totals: HashMap<String, usize> = HashMap::new()
  FOR ((phase, _category), bucket) IN &self.table:
    *phase_totals.entry(phase.clone()).or_insert(0) += bucket.len()

  // Step 2: For each (phase, category) bucket, weight = bucket.len() / phase_total
  result: HashMap<(String, String), f32> = HashMap::with_capacity(self.table.len())
  FOR ((phase, category), bucket) IN &self.table:
    total = *phase_totals.get(phase).unwrap_or(&1)  // unwrap_or(1) guards zero-divide
    weight = bucket.len() as f32 / total as f32
    result.insert((phase.clone(), category.clone()), weight)

  RETURN result

END FUNCTION
```

### Edge cases

- `use_fallback = true` → empty map returned (AC-08).
- Phase with a single category → weight = `1.0` (bucket.len() / bucket.len()).
- `phase_totals` zero-divide guard: `unwrap_or(1)` — in practice impossible (a phase
  key exists only if it has at least one bucket), but defensive coding required.
- f32 rounding: weights may not sum to exactly 1.0 — documented in ADR-008 as acceptable.

---

## Module-level Structure After Changes

```
// imports (add PhaseOutcomeRow import if Option A chosen for visibility)

// -- PhaseFreqTable struct (UNCHANGED) --
// -- PhaseFreqTableHandle type alias (UNCHANGED) --

// -- impl PhaseFreqTable --
//   new()                    UNCHANGED
//   new_handle()             UNCHANGED
//   rebuild()                MODIFIED (two-query path + coverage gate)
//   phase_affinity_score()   UNCHANGED
//   phase_category_weights() NEW

// -- Default impl (UNCHANGED) --

// -- Private free functions --
fn outcome_weight(outcome: &str) -> f32                          NEW
fn apply_outcome_weights(rows, outcome_rows) -> Vec<PhaseFreqRow> NEW

// -- tests module --
//   existing tests UNCHANGED
//   new test functions (see Key Test Scenarios below)
```

The existing file is ~412 lines. New additions (~100 lines of impl + tests) will approach
the 500-line limit. If the file exceeds 500 lines with tests, split tests into
`phase_freq_table_tests.rs` using the `#[path = ...]` pattern already used by
`query_log_tests.rs` in the store crate.

---

## Error Handling

| Failure | Behavior |
|---------|----------|
| Query A (store.query_phase_freq_observations) error | Return `Err(e)` — caller (background.rs) retains previous table |
| Query B (store.query_phase_outcome_map) error | Return `Err(e)` — do NOT treat as empty outcome map (C-7) |
| Coverage count query error | Return `Err(e)` — same retain-on-error semantics |
| Empty Query A | Return `Ok(PhaseFreqTable { use_fallback: true, ... })` |
| Coverage count below threshold | Return `Ok(PhaseFreqTable { use_fallback: true, ... })` + warn |
| Empty Query B (valid — no cycle history) | `apply_outcome_weights` gets empty outcome_rows → all weights default 1.0 |
| `phase_category_weights()` on cold-start | Return empty `HashMap` immediately |

---

## Key Test Scenarios

**T-PFT-01: Empty observations → use_fallback=true (AC-13a, AC-01 scenario a)**
- Rebuild with empty observations table.
- Assert: `use_fallback = true`.

**T-PFT-02: Pass-outcome rows weighted 1.0 (AC-13b)**
- Populate Query A rows for phase "delivery". Query B: outcome="pass".
- Assert: weighted freq equals raw freq (1.0 × raw = raw).

**T-PFT-03: Rework-outcome rows weighted 0.5 (AC-13c)**
- Query B: outcome="rework" for phase "delivery".
- Assert: weighted freq = raw_freq × 0.5 (rounded to i64).

**T-PFT-04: Fail-outcome rows weighted 0.5 (AC-13d)**
- Query B: outcome="FAILED" for phase "scope".
- Assert: weighted freq = raw_freq × 0.5.

**T-PFT-05: Missing outcome degrades to 1.0 (AC-13e, AC-05)**
- No Query B rows at all.
- Assert: use_fallback=false, all rows weighted 1.0, no error.

**T-PFT-06: Per-phase MEAN weighting (R-03)**
- Phase "delivery": cycle-A outcome "pass" (weight 1.0), cycle-B outcome "rework" (weight 0.5).
- Mean weight = 0.75.
- Assert: all "delivery" rows have freq multiplied by 0.75 (not 1.0 or 0.5 per-cycle).
- Assert: relative ordering of rows within the "delivery" bucket is preserved.

**T-PFT-07: Mixed-weight bucket ordering invariant (R-03)**
- Entry X: 10 reads in cycle-A (pass), 8 reads in cycle-B (rework).
- Entry Y: 6 reads in cycle-A (pass), 9 reads in cycle-B (rework).
- Raw totals: X=18, Y=15. Per-phase mean weight=0.75.
- Assert: after apply_outcome_weights, X ranks above Y (correct ordering preserved).

**T-PFT-08: coverage gate — below threshold → use_fallback (AC-14)**
- Insert N-1 distinct (phase, session_id) pairs. threshold=N.
- Assert: use_fallback=true, tracing::warn! emitted with count and threshold.

**T-PFT-09: coverage gate — at threshold → normal operation (AC-14)**
- Insert exactly N distinct (phase, session_id) pairs. threshold=N.
- Assert: use_fallback=false (assuming non-empty observations meeting other criteria).

**T-PFT-10: Query B error propagates (constraint C-7)**
- Stub store to error on query_phase_outcome_map call.
- Assert: rebuild() returns Err, previous table content retained.

**T-PFT-11: phase_category_weights() empty on cold-start (AC-08)**
- PhaseFreqTable { use_fallback: true, table: empty }.
- Assert: phase_category_weights() returns empty HashMap.

**T-PFT-12: phase_category_weights() correct distribution (AC-08)**
- Table: phase="delivery", cat-A bucket size 3, cat-B bucket size 1. Total=4.
- Assert: weights are A=0.75, B=0.25, sum=1.0.

**T-PFT-13: phase_category_weights() single-category phase = 1.0 (R-07 edge)**
- Phase with one category only.
- Assert: weight for that category = 1.0.

**T-PFT-14: outcome_weight() exhaustive vocabulary (R-02)**
- Test all known production strings: "PASS", "pass", "Pass", "REWORK", "rework",
  "Rework", "FAIL", "fail", "FAILED", "abandoned", "unknown", "".
- Expected: 1.0/1.0/1.0/0.5/0.5/0.5/0.5/0.5/0.5/1.0/1.0/1.0.

**T-PFT-15: outcome_weight() rework-before-fail priority (R-02)**
- outcome_weight("rework_fail") → 0.5 (rework branch fires, not double-penalized).
- outcome_weight("fail_rework") → 0.5 (rework check fires first due to contains).
  Wait: "fail_rework".contains("rework") → true, so 0.5 from rework branch.
  Both cases return 0.5 via rework branch. This is correct.

**T-PFT-16: NULL feature_cycle sessions — weight 1.0, no error (FR-10, AC-15)**
- Sessions with feature_cycle=NULL. Insert observations for those sessions.
- Call rebuild(). Assert: non-empty table, all rows weighted 1.0, use_fallback=false.

**T-PFT-17: All existing PhaseFreqTable contract tests pass (AC-06, FR-11)**
- No modification to existing test functions in the module.
- Assert: all existing tests continue passing without modification.
