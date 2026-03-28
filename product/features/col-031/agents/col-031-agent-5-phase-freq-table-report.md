# Agent Report: col-031-agent-5-phase-freq-table

## Task
Implement `crates/unimatrix-server/src/services/phase_freq_table.rs` — new module for `PhaseFreqTable`.

## Files Modified

- `crates/unimatrix-server/src/services/phase_freq_table.rs` — **CREATED** (388 lines, under 500 limit)
- `crates/unimatrix-server/src/services/mod.rs` — added `pub mod phase_freq_table;` declaration and `pub use phase_freq_table::{PhaseFreqTable, PhaseFreqTableHandle};` re-export

## Implementation Summary

### PhaseFreqTable struct
- `table: HashMap<(String, String), Vec<(u64, f32)>>` — (phase, category) buckets, sorted desc by score
- `use_fallback: bool` — cold-start flag
- `#[derive(Debug)]`, `Default` impl delegates to `new()`

### API implemented
- `new()` — cold-start state: empty table, use_fallback=true
- `new_handle()` — wraps `new()` in `Arc<RwLock<_>>`
- `rebuild(store, lookback_days)` — calls `store.query_phase_freq_table`, groups by (phase, category), applies `1.0 - ((rank-1) / N)` normalization
- `phase_affinity_score(entry_id, category, phase)` — three 1.0 return paths (use_fallback, absent phase, absent entry); doc comment names both callers (PPR and fused scoring) per AC-17

### Key constraints satisfied
- Rank formula: `1.0 - ((rank-1) as f32 / n as f32)` — single-entry bucket yields 1.0, not 0.0 (R-07)
- All RwLock acquisitions use `.unwrap_or_else(|e| e.into_inner())` — no bare `.unwrap()` on locks
- `use_fallback: bool` (not Option)
- Import path: `unimatrix_store::PhaseFreqRow` (confirmed re-exported from `unimatrix-store/src/lib.rs` line 35)

## Tests

14 unit tests in `#[cfg(test)] mod tests`:

| Test | Covers |
|------|--------|
| `test_phase_freq_table_new_returns_cold_start` | AC-01 |
| `test_phase_freq_table_default_matches_new` | AC-01 |
| `test_new_handle_wraps_cold_start_state` | AC-03 |
| `test_new_handle_write_then_read_reflects_change` | AC-03 |
| `test_new_handle_returns_independent_handles` | AC-03 |
| `test_arc_clone_shares_state` | AC-03 |
| `test_phase_freq_table_handle_poison_recovery` | Poison recovery |
| `test_phase_affinity_score_use_fallback_returns_one` | AC-07 / R-04 (PPR cold-start contract) |
| `test_phase_affinity_score_absent_phase_returns_one` | AC-07 |
| `test_phase_affinity_score_absent_entry_returns_one` | AC-07 |
| `test_phase_affinity_score_present_entry_returns_rank_score` | AC-07 |
| `test_phase_affinity_score_single_entry_bucket_returns_one` | AC-13 / R-07 |
| `test_rebuild_normalization_three_entry_bucket_exact_scores` | AC-14 |
| `test_rebuild_normalization_last_entry_in_five_bucket` | R-07 |
| `test_rebuild_normalization_two_entry_bucket` | AC-14 |
| `test_phase_affinity_score_unknown_phase_returns_one` | R-10 |

Tests cannot run in isolation because the workspace-level lib compilation fails due to cross-agent dependencies (`ServiceSearchParams.current_phase` missing in `replay.rs`, `tools.rs`, `listener.rs`, `index_briefing.rs` — all in other agents' scope). Zero errors originate from `phase_freq_table.rs` itself (confirmed via `cargo build 2>&1 | grep phase_freq_table`).

## Build Status

- `phase_freq_table.rs`: zero errors, zero warnings
- Workspace build: BLOCKED on cross-agent dependencies (`E0063: missing field current_phase` in 4 files owned by other agents)
- `unimatrix-store` crate: PASS (172 tests)

## Issues / Blockers

None in scope. The remaining build errors are all `E0063: missing field current_phase in ServiceSearchParams` — those are owned by other delivery agents (search.rs agent adds the field; replay.rs, tools.rs, listener.rs, index_briefing.rs agents add the field usage).

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — surfaced entries #1560 (Arc<RwLock<T>> background-tick pattern), #3677 (phase_affinity_score must return 1.0 for absent entries), #3682 (lock ordering ADR), #3687 (two cold-start contracts ADR), #3689 (required handle threading ADR). All applied.
- Stored: nothing novel to store — all patterns were already captured in Unimatrix entries #1560, #3677, #3682, #3687, #3689 during the design phase of col-031.
