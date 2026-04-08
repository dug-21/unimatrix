# Agent Report: crt-050-agent-3-store-queries

Feature: crt-050 — Phase-Conditioned Category Affinity (Explicit Read Rebuild)
GH Issue: #542
Agent ID: crt-050-agent-3-store-queries

---

## Work Completed

### Files Modified

- `crates/unimatrix-store/src/query_log.rs`
- `crates/unimatrix-store/src/query_log_tests.rs`

### Changes in query_log.rs

1. Added `pub(crate) const MILLIS_PER_DAY: i64 = 86_400 * 1_000` — multiplication form per ADR-006. Exposed as `pub(crate)` so the external test file can assert the value (see Knowledge Stewardship note below).

2. Added `PhaseOutcomeRow { phase, feature_cycle, outcome }` as `pub struct` (not re-exported from `lib.rs` — callers must use the full module path `unimatrix_store::query_log::PhaseOutcomeRow`). Made `pub` (not `pub(crate)`) to suppress the `private_interfaces` warning when it appears as a return type of a `pub fn`.

3. Added `query_phase_freq_observations(lookback_days: u32) -> Result<Vec<PhaseFreqRow>>` — Query A per IMPLEMENTATION-BRIEF canonical SQL. Pre-computes `cutoff_millis` in Rust using `MILLIS_PER_DAY`. Uses 4-entry tool IN clause. Reuses `row_to_phase_freq_row` deserializer (column positions match).

4. Added `count_phase_session_pairs(lookback_days: u32) -> Result<i64>` — counts distinct `(phase || '|' || session_id)` pairs for the coverage gate in `status.rs`.

5. Added `query_phase_outcome_map() -> Result<Vec<PhaseOutcomeRow>>` — Query B per canonical SQL. Errors propagate (never silenced per constraint C-7).

6. Added `row_to_phase_outcome_row` private deserializer for Query B rows.

7. Deleted `query_phase_freq_table` and updated docstring of `row_to_phase_freq_row` to reference the new function name.

### Changes in query_log_tests.rs

Replaced all old `query_phase_freq_table` tests (deleted function) with 18 new tests covering the test plan scenarios:

- T-SQ-07: `MILLIS_PER_DAY` constant value assertion
- AC-SV-01/R-01: Write-path contract (no double-encoding, json_extract returns 42)
- AC-01/AC-13a: Query A returns rows from observations, not query_log
- AC-02/AC-13f: All four tool name variants included; context_search excluded
- AC-02/R-10: `hook = 'PreToolUse'` filter (validates ADR-007 column name)
- AC-03: CAST handles string-form IDs
- AC-13g/FR-03: Null-$.id observations excluded
- AC-07/R-05: ts_millis lookback boundary (inside/outside window)
- R-05 scenario 1: 30-day arithmetic uses milliseconds not seconds
- AC-15/R-08: Query B excludes NULL feature_cycle sessions
- Query B happy path: correct PhaseOutcomeRow fields returned
- Query B: non-cycle_phase_end and NULL-outcome events excluded
- count_phase_session_pairs: correct count, zero when empty, outside-window exclusion

---

## Test Results

- `cargo test -p unimatrix-store`: 273 passed, 0 failed
- 18 new query_log tests all pass
- `cargo build --workspace`: 0 errors, 0 new warnings
- `cargo clippy -p unimatrix-store -- -D warnings`: clean

---

## Issues / Blockers

None. The `phase_freq_table.rs` doc comment at line 103 still references `query_phase_freq_table` by name — that is owned by the phase-freq-table agent and is outside this component's scope.

---

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing — surfaced ADR-006 (ts_millis contract, entry #4228), observations query migration pattern (entry #4222), and dual-pool architecture (entry #2151). Applied the `pub(crate)` visibility fix proactively from the briefing.
- Stored: entry #4238 "Use pub(crate) const for constants needed in #[path] external test files" via /uni-store-pattern — the `MILLIS_PER_DAY` constant needed `pub(crate)` because `#[path]`-loaded test files cannot see private or `pub(super)` items from their parent module across file boundaries.
