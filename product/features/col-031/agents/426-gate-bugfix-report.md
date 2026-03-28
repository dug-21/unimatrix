# Gate Report: Bug Fix Validation — GH #426

> Gate: Bugfix Validation
> Agent ID: 426-gate-bugfix
> Date: 2026-03-28
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Root cause addressed | PASS | `FRESHNESS_HALF_LIFE_HOURS` constant changed 168.0 → 8760.0 in the one authoritative location |
| No stubs or placeholders | PASS | No `todo!()`, `unimplemented!()`, TODO, FIXME found |
| All tests pass | PASS | All test result lines show `ok. N passed; 0 failed` — build and test run confirmed |
| No new clippy warnings | PASS | Pre-existing warnings only; none introduced by this fix |
| No unsafe code | PASS | `#![forbid(unsafe_code)]` on the engine crate; no unsafe in any changed file |
| Fix is minimal | PASS | 4 files changed; diff is exactly the constant + doc + test recalibration work described |
| New test would have caught original bug | PASS | `freshness_score_30day_old_entry_under_default_params_exceeds_floor` fails with 168h, passes with 8760h |
| Integration smoke tests | PASS | 20/20 per tester report (not re-run here; full unit suite passes) |
| xfail markers with GH issues | PASS | No new xfail introduced; 1 pre-existing (GH#405) unchanged |
| Knowledge stewardship | PASS | Agent report has Queried and Stored entries |

## Detailed Findings

### Root Cause Addressed

**Status**: PASS

The diagnosed root cause was `FRESHNESS_HALF_LIFE_HOURS = 168.0` (1 week) in
`crates/unimatrix-engine/src/confidence.rs:37`. The fix changes this to `8760.0` (1 year) exactly as
prescribed. The constant feeds `ConfidenceParams::default()` which is the authoritative path for all
serving-path callers. No other half-life literals were promoted to be the new default (the
`collaborative` preset uses `ConfidenceParams::default()` via the SR-10 invariant, so it also
correctly picks up 8760.0 — matching the intent of the bug report).

The TOML config exposure was already present from dsn-001 (`freshness_half_life_hours` field in
`KnowledgeConfig`). The README example was updated from `168.0` to `8760.0` to reflect the new
default.

### No Stubs or Placeholders

**Status**: PASS

Grep found zero matches for `TODO`, `FIXME`, `todo!()`, or `unimplemented!()` in any changed file.

### All Tests Pass

**Status**: PASS

`cargo build --workspace` completed without errors. `cargo test --workspace` produced 30+ test
result lines, all `ok. N passed; 0 failed`. The specific new regression test
`freshness_score_30day_old_entry_under_default_params_exceeds_floor` was confirmed to execute and
pass via targeted `cargo test -p unimatrix-engine -- freshness`.

### No New Clippy Warnings

**Status**: PASS

Build output shows 13 pre-existing warnings in `unimatrix-server` (consistent with tester report
noting pre-existing failures). None are in the 4 changed files.

### No Unsafe Code

**Status**: PASS

`unimatrix-engine/src/lib.rs` has `#![forbid(unsafe_code)]`. No unsafe blocks exist in any
changed file.

### Fix Is Minimal

**Status**: PASS

`git diff HEAD~1 --name-only` returns exactly:
- `README.md`
- `crates/unimatrix-engine/src/confidence.rs`
- `crates/unimatrix-engine/src/test_scenarios.rs`
- `crates/unimatrix-server/src/services/confidence.rs`

All changes are directly tied to the bug:
1. Constant and doc comment update in `confidence.rs`
2. SR-10 invariant test updated to assert 8760.0 (correctly tracks the new default)
3. Four existing tests decoupled from default to use explicit 168.0 (testing formula, not default)
4. New regression test added
5. `stale_deprecated()` fixture recalibrated from 90-day → 2-year age to remain meaningfully stale
   under 8760h half-life (required to preserve `standard_ranking` expected ordering)
6. Two test sentinel values in `services/confidence.rs` changed 168.0 → 336.0 (remain
   distinct from the new default and all named presets)
7. README TOML example updated

No unrelated changes are present.

### New Test Would Have Caught Original Bug

**Status**: PASS

`freshness_score_30day_old_entry_under_default_params_exceeds_floor` directly asserts
`freshness_score(..., &ConfidenceParams::default()) > 0.5` for a 30-day-old entry.
- With 168h half-life: score ≈ 0.014 → would FAIL
- With 8760h half-life: score ≈ 0.920 → PASS

The test comment explicitly names GH #426 and explains the reversion scenario.

### xfail Markers

**Status**: PASS

No new xfail markers were introduced. The one pre-existing xfail (GH#405) is unchanged.

### Knowledge Stewardship

**Status**: PASS

`426-agent-1-fix-report.md` contains a `## Knowledge Stewardship` section with:
- `Queried:` entries (briefing surfaced entry #3696 and #2284)
- `Stored:` entry #3698 "Decouple freshness-decay tests from ConfidenceParams::default() when
  changing FRESHNESS_HALF_LIFE_HOURS" via `/uni-store-pattern`

## Knowledge Stewardship

- Queried: context_briefing for validation patterns before running checks
- Stored: nothing novel to store — this is a clean, well-scoped constant fix with a clear
  regression test; the pattern of decoupling tests from compiled defaults was already stored by the
  fix agent (#3698)
