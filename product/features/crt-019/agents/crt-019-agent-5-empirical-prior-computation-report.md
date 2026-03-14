# Agent Report: crt-019-agent-5-empirical-prior-computation

## Status: COMPLETE

## Files Modified

- `crates/unimatrix-server/src/services/status.rs`
- `Cargo.lock` (auto-updated)

## Changes Implemented

### New Constants

- `MINIMUM_VOTED_POPULATION: usize = 10` — authoritative ADR-002 threshold (pub const)
- `PRE_CRT019_SPREAD_BASELINE: f64 = 0.1471` — private baseline for small populations
- `COLD_START_ALPHA: f64 = 3.0`, `COLD_START_BETA: f64 = 3.0` — private cold-start defaults

### New Functions

**`compute_empirical_prior(voted_entries: &[(u32, u32)]) -> (f64, f64)`** (pub(crate))

Method-of-moments Beta distribution estimation:
- Input: (helpful_count, unhelpful_count) pairs for entries with >= 1 vote
- Returns cold-start (3.0, 3.0) when: fewer than 10 entries, zero variance, or p_bar*(1-p_bar)/variance <= 1.0
- Otherwise: concentration = p_bar*(1-p_bar)/variance - 1, alpha0/beta0 clamped to [0.5, 50.0]

**`compute_observed_spread(confidences: &[f64]) -> f64`** (pub(crate))

P95-P5 spread with three-tier behavior:
- Empty slice → 0.0 (EC-01)
- 1–9 entries → 0.1471 (pre-crt-019 baseline, per spawn prompt)
- 10+ entries → nearest-rank P95 - P5, .max(0.0) guarded

**`adaptive_confidence_weight_local(observed_spread: f64) -> f64`** (private)

Inline copy of `clamp(spread * 1.25, 0.15, 0.25)` pending `unimatrix_engine::confidence::adaptive_confidence_weight` from the engine agent.

### Step 2 Change (Confidence Refresh Loop)

Added `Instant`/`Duration` wall-clock guard inside the spawn_blocking task:
- Break early if 200ms elapsed, logging count at `debug!` level
- Uses `std::time::{Duration, Instant}` (added to imports)

### Step 2b (New — run_maintenance)

Added after the confidence refresh loop, before graph compaction:
- Single `spawn_blocking` task acquires one `lock_conn()` per tick
- Two SQL queries: voted entries (status='active' AND helpful+unhelpful>=1) and all confidence values
- Calls `compute_empirical_prior` and `compute_observed_spread`
- Logs all four values at `debug!` level
- TODO comment marks the write to `ConfidenceStateHandle` pending confidence-state agent wiring

## Deviations from Pseudocode

1. **`compute_observed_spread` for small populations**: Pseudocode says return 0.0 for empty input but compute for all other sizes. Spawn prompt says return 0.1471 for fewer than 10 entries. Implemented spawn prompt version: empty→0.0, 1-9→0.1471, 10+→computed. Test plan's EC-01 test (empty→0.0) passes under this implementation.

2. **`adaptive_confidence_weight_local` inlined**: The engine function doesn't exist yet (engine agent's scope). Inlined as a private helper with a comment pointing to the future delegation path.

3. **`variance <= 0.0` check explicit**: Added the `ratio <= 1.0` check per pseudocode note about method-of-moments requiring p_bar*(1-p_bar)/variance > 1.0 for valid Beta parameters, on top of the zero-variance check.

## Tests

21 new unit tests in `services::status::tests`, all passing:
- Cold-start: zero entries, 5 entries, 9 entries
- Threshold boundary: exactly 10 entries (two behaviors per variance)
- Balanced/mixed population: genuine variance, sensible clamped output
- Zero-variance degeneracy: all-helpful, all-unhelpful
- Clamp: near-identical high-rate population
- Spread edge cases: empty, single, 9 entries, uniform, full range, non-negative
- adaptive weight: floor, ceiling, initial spread, midrange
- Constant: MINIMUM_VOTED_POPULATION == 10

## Test Results

```
test result: ok. 1206 passed; 0 failed; 0 ignored
```

(1185 existing + 21 new)

## Blockers / Notes

None. The implementation is complete and self-contained. The ConfidenceStateHandle write is gated behind a TODO comment — the confidence-state agent (component 2) must wire `ConfidenceStateHandle` through `StatusService` before the atomic state update can be enabled.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for unimatrix-server services status spawn_blocking SQL confidence — no results returned (server query pattern, new work)
- Stored: nothing novel to store — the `spawn_blocking` + `lock_conn()` pattern for SQL queries in maintenance is already established codebase convention visible in existing `run_maintenance` steps. The `variance <= 0.0` + `ratio <= 1.0` double guard for method-of-moments is component-specific math logic, not a reusable server pattern.
