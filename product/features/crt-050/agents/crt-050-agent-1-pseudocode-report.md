# Agent Report: crt-050-agent-1-pseudocode

## Summary

Produced per-component pseudocode for crt-050 (Phase-Conditioned Category Affinity —
Explicit Read Rebuild). All four components covered plus OVERVIEW.

## Artifacts

- `product/features/crt-050/pseudocode/OVERVIEW.md`
- `product/features/crt-050/pseudocode/store-queries.md`
- `product/features/crt-050/pseudocode/phase-freq-table.md`
- `product/features/crt-050/pseudocode/config.md`
- `product/features/crt-050/pseudocode/status-diagnostics.md`

## Components Covered

1. **store-queries** — delete `query_phase_freq_table`; add `query_phase_freq_observations`
   (Query A) + `query_phase_outcome_map` (Query B); add `PhaseOutcomeRow` struct;
   add `MILLIS_PER_DAY` constant; retain `row_to_phase_freq_row` deserializer.

2. **phase-freq-table** — modified `rebuild()` (two-query path + coverage gate);
   new `apply_outcome_weights()` private fn; new `outcome_weight()` private fn;
   new `phase_category_weights()` public method.

3. **config** — rename `query_log_lookback_days` → `phase_freq_lookback_days` with
   `#[serde(alias)]`; add `min_phase_session_pairs: u32` (default 5, range [1,1000]);
   5 update sites documented.

4. **status-diagnostics** — rename field refs in `run_phase_freq_table_alignment_check`;
   new `run_observations_coverage_check` fn; background.rs line 622 field rename.

## Open Questions Flagged

### OQ-1: PhaseOutcomeRow cross-crate visibility

`PhaseOutcomeRow` is declared in `unimatrix-store` (not re-exported per architecture),
but `apply_outcome_weights()` in `unimatrix-server` needs to receive it. Two options
documented in `phase-freq-table.md`:

- Option A: Make `PhaseOutcomeRow` `pub(crate)` in the store crate with `#[doc(hidden)]`
  re-export so server can import it.
- Option B: Move `apply_outcome_weights` into the store crate.

The implementer must choose and document. Option A is recommended.

### OQ-2: count_phase_session_pairs — missing store function

The coverage gate in `rebuild()` needs a distinct `(phase, session_id)` pair count
from within the lookback window. This requires either:
- A new `store.count_phase_session_pairs(lookback_days: u32) -> Result<u64>` function
  (not in the architecture's integration surface table).
- Embedding the count in Query A as a subquery.

The architecture describes the count as computed "via a SQL scalar subquery in
`status.rs` (or a new dedicated store fn)". A new store function is cleaner. The
implementer must add it; it is not blocking but should be added to the store-queries
component. SQL:
```sql
SELECT COUNT(DISTINCT phase || ':' || session_id)
FROM observations
WHERE phase IS NOT NULL
  AND hook = 'PreToolUse'
  AND tool IN ('context_get', 'mcp__unimatrix__context_get',
               'context_lookup', 'mcp__unimatrix__context_lookup')
  AND ts_millis > ?1
```

### OQ-3: min_phase_session_pairs parameter threading in rebuild()

Architecture says the gate lives in `rebuild()`. The current `rebuild` signature is
`(store: &Store, lookback_days: u32)`. To receive `min_phase_session_pairs`, the
signature must be extended. The background.rs call site must be updated accordingly.
Documented in `phase-freq-table.md` as "Option A (preferred)".

### OQ-4: run_maintenance signature for PhaseFreqTableHandle

The `run_observations_coverage_check` diagnostic in `status.rs` needs the
`last_coverage_count` value from `PhaseFreqTable`. Whether `PhaseFreqTableHandle`
is currently a parameter of `run_maintenance` is unclear from the grep output.
If it is not, the implementer must either add it or emit the coverage warning
solely from within `rebuild()`. The `rebuild()` path already covers AC-11's
warning requirement — the separate status.rs fn is advisory. Flagged in
`status-diagnostics.md` as an open implementation decision.

### OQ-5: freq field type discrepancy (i64 vs u32)

The `IMPLEMENTATION-BRIEF.md` Data Structures section shows `PhaseFreqRow.freq: u32`,
but the actual `query_log.rs` has `freq: i64` (SQLite `COUNT(*)` → i64 in sqlx 0.8,
R-13 comment in the existing doc). The pseudocode uses `i64` throughout, matching the
actual code. This is not a spec defect — it is an existing implementation decision
that the spec doc glossed over. Implementer: use `i64` as declared in the existing
struct.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_search` for pattern category — found entries
  #3677 (cold-start neutral 1.0 return), #3699 (use_fallback guard pattern),
  #3753 (lock snapshot pattern for search pipeline). All confirmed existing patterns
  are preserved in this pseudocode.
- Queried: `mcp__unimatrix__context_search` for crt-050 decisions — found #4225
  (ADR-003 outcome weighting), #4223 (ADR-001 two-query strategy), #4228 (ADR-006
  ts_millis contract). All incorporated.
- Queried: `mcp__unimatrix__context_briefing` — found additional supporting entries
  #3685, #3687, #3688, #3917 (rank normalization, affinity callers, crt-036 diagnostic).
- Deviations from established patterns: none. All pseudocode preserves existing
  patterns: rank-based normalization (col-031 ADR-001 #3685), cold-start 1.0 return
  (#3677), use_fallback guard (#3699), poison recovery via `unwrap_or_else`, lock
  snapshot acquisition (#3753).
