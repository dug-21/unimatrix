# Agent Report: 426-agent-1-fix

**Bug**: GH #426 — freshness half-life 168h → 8760h, expose as TOML config
**Branch**: `bugfix/426-freshness-half-life`

## Files Modified

- `crates/unimatrix-engine/src/confidence.rs`
- `crates/unimatrix-engine/src/test_scenarios.rs`
- `crates/unimatrix-server/src/services/confidence.rs`
- `README.md`

## Changes Made

1. `FRESHNESS_HALF_LIFE_HOURS`: `168.0` → `8760.0`, doc comment updated "1 week" → "1 year (8760 hours)"
2. `ConfidenceParams` field doc comment: `(168.0)` → `(8760.0)`
3. SR-10 invariant test assertion: `168.0` → `8760.0`
4. Two `ConfidenceParams` literals in `unimatrix-server/src/services/confidence.rs`: `168.0` → `336.0` (sentinel distinct from new default and all presets)
5. `README.md` TOML example: `freshness_half_life_hours = 168.0` → `8760.0`
6. Decoupled four existing tests that asserted decay behavior using `ConfidenceParams::default()` — they now pass explicit `freshness_half_life_hours: 168.0` so they test the formula, not the default value:
   - `freshness_one_week_ago`
   - `freshness_very_old_entry`
   - `test_freshness_score_uses_params_half_life`
   - `test_freshness_score_configurable_half_life`
7. Updated `stale_deprecated()` scenario fixture from 90-day to 2-year age so it remains stale relative to the 8760h half-life, preserving the `standard_ranking` expected ordering.

## New Tests

- `freshness_score_30day_old_entry_under_default_params_exceeds_floor` — asserts `freshness_score` for a 30-day-old entry with `ConfidenceParams::default()` returns > 0.5. With 168h half-life this returned ≈ 0.014 (would fail); with 8760h it returns ≈ 0.920 (passes). This is the regression guard that would have caught the original bug.

## Test Results

- `cargo test -p unimatrix-engine`: **337 passed, 0 failed**
- `cargo test -p unimatrix-server`: **2313 passed, 0 failed**

## Issues / Blockers

None. Pre-existing clippy warnings in `unimatrix-observe`, `auth.rs`, and `event_queue.rs` are unrelated to this fix.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — entry #3696 (lesson-learned for this exact bug) and #2284 (dsn-001 ADR on operator-configurable half-life) were surfaced and confirmed the fix approach.
- Stored: entry #3698 "Decouple freshness-decay tests from ConfidenceParams::default() when changing FRESHNESS_HALF_LIFE_HOURS" via `/uni-store-pattern` — captures the four test categories that break on any constant change and the fix pattern for each.
