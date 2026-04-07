# Agent Report: crt-050-agent-6-status-diagnostics

Agent ID: crt-050-agent-6-status-diagnostics
Feature: crt-050 (#542)
Component: status-diagnostics

---

## Files Modified

- `crates/unimatrix-server/src/services/status.rs`
- `crates/unimatrix-server/src/background.rs`

---

## Changes Implemented

### status.rs

1. **`run_phase_freq_table_alignment_check`** — renamed parameter `query_log_lookback_days` → `phase_freq_lookback_days` throughout (parameter declaration, `lookback_cutoff_secs` calculation, `tracing::warn!` structured field, warning message text). Updated doc comment to note the function now governs the `observations` window (crt-050, not `query_log`). Call site at line 1407 already used `inference_config.phase_freq_lookback_days` (done by Wave 1 config agent).

2. **`run_observations_coverage_check`** (new) — advisory `tracing::warn!` emitted when `coverage_count < min_phase_session_pairs as u64`. Takes `(coverage_count: u64, min_phase_session_pairs: u32, phase_freq_lookback_days: u32)`. Warning message includes count, lookback window, and threshold. No state mutation.

3. **Coverage diagnostic wired into `run_maintenance`** — after the existing alignment check, calls `self.store.count_phase_session_pairs(inference_config.phase_freq_lookback_days).await`. Store errors produce a `tracing::warn!` and skip the advisory check (non-fatal). Result passed to `run_observations_coverage_check`.

4. **Existing test updated** — `test_gc_phase_freq_table_mismatch_warning_fires` assertion updated from `logs_contain("query_log_lookback_days")` to `logs_contain("phase_freq_lookback_days")` (AC-17).

5. **New test module `crt_050_observations_coverage_tests`** — 5 tests covering T-SD-04 through T-SD-06 and edge cases: warn below threshold, no warn above threshold, no warn at boundary (strict less-than), threshold=1/count=1, zero count.

### background.rs

- Added `let min_pairs = inference_config.min_phase_session_pairs;` and passed as third argument to `PhaseFreqTable::rebuild(&store_clone, lookback_days, min_pairs)` to match the Option A 3-arg signature added by the Wave 2 phase-freq-table agent.

---

## Wiring Decision

The pseudocode offered two options for obtaining `coverage_count` in `run_maintenance`:

- **Option A**: `PhaseFreqTable.last_coverage_count` field written by `rebuild()`, read via handle
- **Option B**: Direct store call in `run_maintenance`

Chose **Option B** (direct store call). Rationale: `run_maintenance` is async and already owns `self.store`. The scalar count query is cheap. Adding `last_coverage_count` to `PhaseFreqTable` purely for diagnostic observability would couple rebuild logic to its monitoring surface. Decision stored as pattern #4240.

---

## Test Results

```
cargo test -p unimatrix-server -- crt_036 crt_050_observations
  9 passed, 0 failed

cargo test -p unimatrix-server -- status
  129 passed, 0 failed

cargo test -p unimatrix-server
  2966 passed, 0 failed

cargo build --workspace
  Finished — zero errors
```

---

## Issues / Blockers

None. All changes are within component scope.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — retrieved entry #3917 (crt-036 phase_freq_lookback_days ADR), #4226 (crt-050 config field rename ADR), #1616 (background tick dedup flag pattern). Confirmed existing crt-036 function location and test structure.
- Stored: entry #4240 "Tick-time advisory diagnostics: call store directly in async run_maintenance, don't cache count on in-memory struct" via /uni-store-pattern — novel: the Option A vs Option B wiring choice for diagnostic count queries is not previously documented, and the decision rationale (async context eliminates need for cached state) is non-obvious from source.
